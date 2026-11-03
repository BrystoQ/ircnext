//! FiSH encryption, compatible with FiSH 10, `FiSHLiM` and mircryption.
//!
//! - ECB: `+OK <data>`, Blowfish ECB on zero-padded blocks, each block
//!   written as 12 characters of FiSH's own base64 (right half first).
//! - CBC: `+OK *<base64>`, a random 8-byte IV followed by Blowfish CBC.
//! - DH1080 key exchange: `DH1080_INIT <public key> [CBC]` answered by
//!   `DH1080_FINISH <public key> [CBC]` in notices; the key is the SHA-256
//!   of the shared secret.
//!
//! Keys are strings: `cbc:<key>` selects CBC, anything else (or an `ecb:`
//! prefix) ECB, as in FiSH 10.

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use blowfish::Blowfish;
use blowfish::cipher::{Block, BlockCipherDecrypt, BlockCipherEncrypt, KeyInit};
use num_bigint::BigUint;
use sha2::{Digest, Sha256};

/// FiSH's base64 alphabet for ECB.
const ALPHABET: &[u8; 64] = b"./0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
/// Longest Blowfish key, in bytes.
const MAX_KEY: usize = 56;

/// DH1080 prime (1080-bit safe prime) and generator.
const DH1080_PRIME: &str = "FBE1022E23D213E8ACFA9AE8B9DFADA3EA6B7AC7A7B7E95AB5EB2DF858921FEADE95E6AC7BE7DE6ADBAB8A783E7AF7A7FA6A2B7BEB1E72EAE2B72F9FA2BFB2A2EFBEFAC868BADB3E828FA8BADFADA3E4CC1BE7E8AFE85E9698A783EB68FA07A77AB6AD7BEB618ACF9CA2897EB28A6189EFA07AB99A8A7FA9AE299EFA7BA66DEAFEFBEFBF0B7D8B";
const DH1080_GENERATOR: u32 = 2;

/// Encryption mode of a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Ecb,
    Cbc,
}

/// A FiSH key, parsed from its `cbc:`/`ecb:` form.
#[derive(Clone, PartialEq, Eq)]
pub struct Key {
    pub mode: Mode,
    secret: Vec<u8>,
}

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Key({:?}, ***)", self.mode)
    }
}

impl Key {
    /// Parses `cbc:secret`, `ecb:secret` or `secret` (ECB). `None` if the
    /// secret is shorter than 4 bytes; longer than 56 bytes, it is cut.
    pub fn parse(text: &str) -> Option<Self> {
        let (mode, secret) = if let Some(secret) = text.strip_prefix("cbc:") {
            (Mode::Cbc, secret)
        } else {
            (Mode::Ecb, text.strip_prefix("ecb:").unwrap_or(text))
        };
        let mut secret = secret.as_bytes().to_vec();
        secret.truncate(MAX_KEY);
        (secret.len() >= 4).then_some(Self { mode, secret })
    }

    /// Text form, as stored (`cbc:…` or `ecb:…`).
    pub fn to_text(&self) -> String {
        let prefix = if self.mode == Mode::Cbc { "cbc:" } else { "ecb:" };
        format!("{prefix}{}", String::from_utf8_lossy(&self.secret))
    }

    fn cipher(&self) -> Blowfish {
        Blowfish::new_from_slice(&self.secret).expect("key length checked")
    }

    /// Encrypts `text` into a `+OK …` message.
    pub fn encrypt(&self, text: &str) -> String {
        let cipher = self.cipher();
        let mut data = text.as_bytes().to_vec();
        data.resize(data.len().div_ceil(8).max(1) * 8, 0);
        match self.mode {
            Mode::Ecb => {
                let mut out = String::from("+OK ");
                for chunk in data.as_chunks_mut::<8>().0 {
                    cipher.encrypt_block(block(chunk));
                    encode_block(chunk, &mut out);
                }
                out
            }
            Mode::Cbc => {
                let mut iv = [0u8; 8];
                getrandom::fill(&mut iv).expect("system random generator");
                let mut previous = iv;
                for chunk in data.as_chunks_mut::<8>().0 {
                    for (byte, prev) in chunk.iter_mut().zip(previous) {
                        *byte ^= prev;
                    }
                    cipher.encrypt_block(block(chunk));
                    previous.copy_from_slice(chunk);
                }
                let mut payload = iv.to_vec();
                payload.extend_from_slice(&data);
                format!("+OK *{}", STANDARD.encode(payload))
            }
        }
    }

    /// Decrypts a `+OK …` (or `mcps …`) message; `None` if it is not one,
    /// or does not decrypt with this key.
    pub fn decrypt(&self, message: &str) -> Option<String> {
        let data = message.strip_prefix("+OK ").or_else(|| message.strip_prefix("mcps "))?.trim();
        let cipher = self.cipher();
        // A CBC message starts with `*`, whatever the key says.
        let mut plain = if let Some(encoded) = data.strip_prefix('*') {
            let mut bytes = STANDARD.decode(encoded).ok()?;
            if bytes.len() < 16 || !bytes.len().is_multiple_of(8) {
                return None;
            }
            let mut previous: [u8; 8] = bytes[..8].try_into().ok()?;
            let mut plain = Vec::with_capacity(bytes.len() - 8);
            for chunk in bytes[8..].as_chunks_mut::<8>().0 {
                let ciphertext = *chunk;
                cipher.decrypt_block(block(chunk));
                plain.extend(chunk.iter().zip(previous).map(|(byte, prev)| byte ^ prev));
                previous = ciphertext;
            }
            plain
        } else {
            if !data.len().is_multiple_of(12) || data.is_empty() {
                return None;
            }
            let mut plain = Vec::with_capacity(data.len() / 12 * 8);
            for group in data.as_bytes().as_chunks::<12>().0 {
                let mut block = decode_block(group)?;
                cipher.decrypt_block(self::block(&mut block));
                plain.extend_from_slice(&block);
            }
            plain
        };
        while plain.last() == Some(&0) {
            plain.pop();
        }
        // A wrong key gives bytes that are not text.
        let text = String::from_utf8(plain).ok()?;
        (!text.chars().any(|c| c.is_control() && !matches!(c, '\x01'..='\x1f'))).then_some(text)
    }
}

/// An 8-byte chunk as a cipher block.
fn block(chunk: &mut [u8]) -> &mut Block<Blowfish> {
    chunk.try_into().expect("8-byte chunk")
}

/// Writes an encrypted block as 12 characters: right half, then left half,
/// 6 bits at a time from the least significant ones.
fn encode_block(block: &[u8], out: &mut String) {
    let left = u32::from_be_bytes(block[..4].try_into().expect("8-byte block"));
    let right = u32::from_be_bytes(block[4..].try_into().expect("8-byte block"));
    for mut half in [right, left] {
        for _ in 0..6 {
            out.push(char::from(ALPHABET[(half & 0x3f) as usize]));
            half >>= 6;
        }
    }
}

fn decode_block(group: &[u8]) -> Option<[u8; 8]> {
    let mut halves = [0u32; 2];
    for (half, chars) in halves.iter_mut().zip(group.as_chunks::<6>().0) {
        for (index, &c) in chars.iter().enumerate() {
            let value = ALPHABET.iter().position(|&a| a == c)?;
            *half |= u32::try_from(value).ok()? << (index * 6);
        }
    }
    let [right, left] = halves;
    let mut block = [0u8; 8];
    block[..4].copy_from_slice(&left.to_be_bytes());
    block[4..].copy_from_slice(&right.to_be_bytes());
    Some(block)
}

/// A DH1080 key pair, kept while waiting for the other side.
#[derive(Clone)]
pub struct KeyExchange {
    private: BigUint,
    public: BigUint,
}

impl std::fmt::Debug for KeyExchange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KeyExchange(***)")
    }
}

fn prime() -> BigUint {
    BigUint::parse_bytes(DH1080_PRIME.as_bytes(), 16).expect("valid prime")
}

impl KeyExchange {
    pub fn new() -> Self {
        let prime = prime();
        // The generator does not stay in the prime-order subgroup the other
        // side checks: draw again until the public key does (half the time).
        loop {
            let mut bytes = [0u8; 135];
            getrandom::fill(&mut bytes).expect("system random generator");
            let private = BigUint::from_bytes_be(&bytes) % (&prime - 2u32) + 2u32;
            let public = BigUint::from(DH1080_GENERATOR).modpow(&private, &prime);
            if is_valid_public(&public, &prime) {
                return Self { private, public };
            }
        }
    }

    /// Our public key, as sent in `DH1080_INIT` / `DH1080_FINISH`.
    pub fn public_key(&self) -> String {
        dh_encode(&self.public.to_bytes_be())
    }

    /// The FiSH key shared with the owner of `their_public_key`, in `mode`.
    /// `None` if that public key is invalid (outside the group or in a
    /// small subgroup).
    pub fn shared_key(&self, their_public_key: &str, mode: Mode) -> Option<Key> {
        let prime = prime();
        let theirs = BigUint::from_bytes_be(&dh_decode(their_public_key)?);
        if !is_valid_public(&theirs, &prime) {
            return None;
        }
        let secret = theirs.modpow(&self.private, &prime);
        let digest = Sha256::digest(secret.to_bytes_be());
        Some(Key { mode, secret: dh_encode(&digest).into_bytes() })
    }
}

impl Default for KeyExchange {
    fn default() -> Self {
        Self::new()
    }
}

/// Public key inside the group and in its prime-order subgroup (RFC 2631,
/// 2.1.5), which rules out small-subgroup attacks.
fn is_valid_public(key: &BigUint, prime: &BigUint) -> bool {
    let one = BigUint::from(1u32);
    let order = (prime - 1u32) >> 1;
    *key > one && *key < prime - 1u32 && key.modpow(&order, prime) == one
}

/// DH1080's base64: no padding, plus an `A` when the length is a multiple
/// of 3 bytes.
fn dh_encode(bytes: &[u8]) -> String {
    let mut text = STANDARD_NO_PAD.encode(bytes);
    if bytes.len().is_multiple_of(3) {
        text.push('A');
    }
    text
}

fn dh_decode(text: &str) -> Option<Vec<u8>> {
    let mut text = text.trim().trim_end_matches('=');
    if text.len() % 4 == 1 {
        text = &text[..text.len() - 1];
    }
    STANDARD_NO_PAD.decode(text).ok()
}

/// A DH1080 notice: `DH1080_INIT` (start) or `DH1080_FINISH` (answer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dh1080 {
    Init { public_key: String, mode: Mode },
    Finish { public_key: String, mode: Mode },
}

impl Dh1080 {
    pub fn parse(text: &str) -> Option<Self> {
        let mut words = text.split_whitespace();
        let kind = words.next()?;
        let public_key = words.next()?.to_owned();
        let mode = if words.next().is_some_and(|word| word.eq_ignore_ascii_case("CBC")) { Mode::Cbc } else { Mode::Ecb };
        match kind {
            "DH1080_INIT" | "DH1080_INIT_cbc" => Some(Self::Init { public_key, mode }),
            "DH1080_FINISH" | "DH1080_FINISH_cbc" => Some(Self::Finish { public_key, mode }),
            _ => None,
        }
    }

    /// Notice text.
    pub fn to_text(&self) -> String {
        let (kind, public_key, mode) = match self {
            Self::Init { public_key, mode } => ("DH1080_INIT", public_key, mode),
            Self::Finish { public_key, mode } => ("DH1080_FINISH", public_key, mode),
        };
        let suffix = if *mode == Mode::Cbc { " CBC" } else { "" };
        format!("{kind} {public_key}{suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_keys() {
        assert_eq!(Key::parse("cbc:secret").unwrap().mode, Mode::Cbc);
        assert_eq!(Key::parse("ecb:secret").unwrap().mode, Mode::Ecb);
        assert_eq!(Key::parse("secret").unwrap().to_text(), "ecb:secret");
        assert!(Key::parse("abc").is_none(), "too short");
        assert!(!format!("{:?}", Key::parse("secret").unwrap()).contains("secret"));
    }

    /// Reference computed independently: `printf 'hello\0\0\0' | openssl
    /// enc -bf-ecb -K 30313233343536373839616263646566 -nopad` (a 16-byte
    /// key, since OpenSSL pads shorter ones), then FiSH's block encoding.
    #[test]
    fn ecb_matches_the_reference() {
        let key = Key::parse("0123456789abcdef").unwrap();
        assert_eq!(key.encrypt("hello"), REFERENCE_ECB);
        assert_eq!(key.decrypt(REFERENCE_ECB).as_deref(), Some("hello"));
    }

    #[test]
    fn round_trips_in_both_modes() {
        for mode in ["ecb:", "cbc:"] {
            let key = Key::parse(&format!("{mode}a fairly long secret key")).unwrap();
            for text in ["hi", "exactly8", "héllo wörld, a longer message with unicode ✓", "\x01ACTION waves\x01"] {
                let encrypted = key.encrypt(text);
                assert!(encrypted.starts_with("+OK "));
                assert_eq!(key.decrypt(&encrypted).as_deref(), Some(text), "{mode} {text}");
            }
        }
        let cbc = Key::parse("cbc:secret").unwrap();
        assert_ne!(cbc.encrypt("same"), cbc.encrypt("same"), "random IV");
    }

    #[test]
    fn rejects_foreign_messages_and_wrong_keys() {
        let key = Key::parse("right key").unwrap();
        let other = Key::parse("wrong key").unwrap();
        assert_eq!(key.decrypt("plain text"), None);
        assert_eq!(key.decrypt("+OK not!valid"), None);
        assert_eq!(other.decrypt(&key.encrypt("this is a secret message")), None);
    }

    #[test]
    fn key_exchange_agrees_on_a_key() {
        let alice = KeyExchange::new();
        let bob = KeyExchange::new();
        let init = Dh1080::Init { public_key: alice.public_key(), mode: Mode::Cbc };
        let Some(Dh1080::Init { public_key, mode }) = Dh1080::parse(&init.to_text()) else {
            panic!("init expected");
        };
        let bob_key = bob.shared_key(&public_key, mode).unwrap();
        let alice_key = alice.shared_key(&bob.public_key(), Mode::Cbc).unwrap();
        assert_eq!(bob_key, alice_key);
        assert_eq!(bob_key.mode, Mode::Cbc);
        assert_eq!(alice_key.decrypt(&bob_key.encrypt("hello")).as_deref(), Some("hello"));
        // The key is the base64 of a SHA-256: 43 characters.
        assert_eq!(alice_key.secret.len(), 43);
    }

    #[test]
    fn rejects_weak_public_keys() {
        let ours = KeyExchange::new();
        for weak in [BigUint::from(1u32), prime() - 1u32, BigUint::from(0u32)] {
            assert!(ours.shared_key(&dh_encode(&weak.to_bytes_be()), Mode::Ecb).is_none());
        }
        assert!(ours.shared_key("not base64 at all!", Mode::Ecb).is_none());
    }

    #[test]
    fn dh_base64_quirk() {
        assert_eq!(dh_encode(&[1, 2, 3]), "AQIDA");
        assert_eq!(dh_decode("AQIDA").unwrap(), [1, 2, 3]);
        assert_eq!(dh_encode(&[1, 2]), "AQI");
        assert_eq!(dh_decode("AQI").unwrap(), [1, 2]);
    }

    const REFERENCE_ECB: &str = "+OK KGCmS/1XyKv/";
}
