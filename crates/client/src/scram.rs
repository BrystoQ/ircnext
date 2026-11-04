//! SASL SCRAM-SHA-256 (RFC 5802, RFC 7677): the password never leaves the
//! client, and the server proves it knows it too.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use sha2::{Digest, Sha256};

/// Mechanism name.
pub const NAME: &str = "SCRAM-SHA-256";
/// Highest iteration count accepted (each costs a hash: a server could
/// otherwise freeze the client).
const MAX_ITERATIONS: u32 = 1_000_000;

/// A SCRAM exchange in progress.
#[derive(Debug)]
pub struct Scram {
    password: String,
    client_nonce: String,
    client_first_bare: String,
    /// Expected server signature, once the proof is sent.
    server_signature: Option<[u8; 32]>,
}

/// Why an exchange stopped.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScramError {
    #[error("malformed server message")]
    Malformed,
    #[error("the server nonce does not extend ours")]
    Nonce,
    #[error("iteration count out of range")]
    Iterations,
    #[error("the server could not prove it knows the password")]
    ServerSignature,
    #[error("server error: {0}")]
    Server(String),
}

impl Scram {
    pub fn new(username: &str, password: &str) -> Self {
        let mut bytes = [0u8; 18];
        // A failing system random source leaves a zero nonce: still unique
        // enough with the timestamp, and the exchange stays safe (the
        // server adds its own nonce).
        if getrandom::fill(&mut bytes).is_err() {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
            bytes[..16].copy_from_slice(&now.as_nanos().to_le_bytes());
        }
        Self::with_nonce(username, password, &BASE64.encode(bytes))
    }

    fn with_nonce(username: &str, password: &str, nonce: &str) -> Self {
        let username = username.replace('=', "=3D").replace(',', "=2C");
        Self {
            password: password.to_owned(),
            client_nonce: nonce.to_owned(),
            client_first_bare: format!("n={username},r={nonce}"),
            server_signature: None,
        }
    }

    /// First message (no channel binding).
    pub fn client_first(&self) -> String {
        format!("n,,{}", self.client_first_bare)
    }

    /// Answer to the server's first message: the proof.
    pub fn client_final(&mut self, server_first: &str) -> Result<String, ScramError> {
        let attributes = attributes(server_first)?;
        let nonce = attribute(&attributes, 'r')?;
        if !nonce.starts_with(&self.client_nonce) || nonce.len() == self.client_nonce.len() {
            return Err(ScramError::Nonce);
        }
        let salt = BASE64.decode(attribute(&attributes, 's')?).map_err(|_| ScramError::Malformed)?;
        let iterations: u32 = attribute(&attributes, 'i')?.parse().map_err(|_| ScramError::Malformed)?;
        if !(1..=MAX_ITERATIONS).contains(&iterations) {
            return Err(ScramError::Iterations);
        }
        let salted = pbkdf2(self.password.as_bytes(), &salt, iterations);
        let client_key = hmac(&salted, b"Client Key");
        let stored_key: [u8; 32] = Sha256::digest(client_key).into();
        let without_proof = format!("c=biws,r={nonce}");
        let auth_message = format!("{},{server_first},{without_proof}", self.client_first_bare);
        let signature = hmac(&stored_key, auth_message.as_bytes());
        let proof: Vec<u8> = client_key.iter().zip(signature).map(|(key, sig)| key ^ sig).collect();
        let server_key = hmac(&salted, b"Server Key");
        self.server_signature = Some(hmac(&server_key, auth_message.as_bytes()));
        Ok(format!("{without_proof},p={}", BASE64.encode(proof)))
    }

    /// Checks the server's last message.
    pub fn verify(&self, server_final: &str) -> Result<(), ScramError> {
        let attributes = attributes(server_final)?;
        if let Ok(error) = attribute(&attributes, 'e') {
            return Err(ScramError::Server(error.to_owned()));
        }
        let received = BASE64.decode(attribute(&attributes, 'v')?).map_err(|_| ScramError::Malformed)?;
        match self.server_signature {
            Some(expected) if received == expected => Ok(()),
            _ => Err(ScramError::ServerSignature),
        }
    }
}

fn attributes(message: &str) -> Result<Vec<(char, &str)>, ScramError> {
    message
        .split(',')
        .map(|part| {
            let mut chars = part.chars();
            match (chars.next(), chars.next()) {
                (Some(name), Some('=')) => Ok((name, &part[2..])),
                _ => Err(ScramError::Malformed),
            }
        })
        .collect()
}

fn attribute<'a>(attributes: &[(char, &'a str)], name: char) -> Result<&'a str, ScramError> {
    attributes.iter().find(|(key, _)| *key == name).map(|(_, value)| *value).ok_or(ScramError::Malformed)
}

/// HMAC-SHA-256 (RFC 2104).
pub(crate) fn hmac(key: &[u8], data: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut block = [0u8; BLOCK];
    if key.len() > BLOCK {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let pad = |byte: u8| block.map(|b| b ^ byte);
    let inner = Sha256::new().chain_update(pad(0x36)).chain_update(data).finalize();
    Sha256::new().chain_update(pad(0x5c)).chain_update(inner).finalize().into()
}

/// PBKDF2-HMAC-SHA-256 with a 32-byte output (one block): SCRAM's `Hi`.
pub(crate) fn pbkdf2(password: &[u8], salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut first = salt.to_vec();
    first.extend_from_slice(&1u32.to_be_bytes());
    let mut u = hmac(password, &first);
    let mut result = u;
    for _ in 1..iterations {
        u = hmac(password, &u);
        for (out, byte) in result.iter_mut().zip(u) {
            *out ^= byte;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc_7677_exchange() {
        let mut scram = Scram::with_nonce("user", "pencil", "rOprNGfwEbeRWgbNEkqO");
        assert_eq!(scram.client_first(), "n,,n=user,r=rOprNGfwEbeRWgbNEkqO");
        let server_first = "r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096";
        assert_eq!(
            scram.client_final(server_first).unwrap(),
            "c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,p=dHzbZapWIk4jUhN+Ute9ytag9zjfMHgsqmmiz7AndVQ="
        );
        assert_eq!(scram.verify("v=6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4="), Ok(()));
        assert_eq!(scram.verify("v=AAAATRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4="), Err(ScramError::ServerSignature));
        assert_eq!(scram.verify("e=invalid-proof"), Err(ScramError::Server("invalid-proof".into())));
    }

    #[test]
    fn refuses_bad_server_messages() {
        let mut scram = Scram::with_nonce("a,b=c", "pw", "abc");
        assert_eq!(scram.client_first(), "n,,n=a=2Cb=3Dc,r=abc", "user name escaped");
        assert_eq!(scram.client_final("r=xyz123,s=AAAA,i=4096"), Err(ScramError::Nonce));
        assert_eq!(scram.client_final("r=abc,s=AAAA,i=4096"), Err(ScramError::Nonce), "the server adds nothing");
        assert_eq!(scram.client_final("r=abcdef,s=AAAA,i=99999999"), Err(ScramError::Iterations));
        assert_eq!(scram.client_final("r=abcdef,s=AAAA,i=0"), Err(ScramError::Iterations));
        assert_eq!(scram.client_final("r=abcdef,i=4096"), Err(ScramError::Malformed));
        assert_eq!(scram.client_final("garbage"), Err(ScramError::Malformed));
        assert_eq!(scram.verify("v=6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4="), Err(ScramError::ServerSignature), "no proof sent yet");
        assert_ne!(Scram::new("u", "p").client_nonce, Scram::new("u", "p").client_nonce);
    }

    #[test]
    fn hmac_matches_rfc_4231() {
        // Test case 2.
        let mac = hmac(b"Jefe", b"what do ya want for nothing?");
        let expected = "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843";
        let expected: Vec<u8> = (0..expected.len()).step_by(2).map(|i| u8::from_str_radix(&expected[i..i + 2], 16).unwrap()).collect();
        assert_eq!(mac.to_vec(), expected);
    }
}
