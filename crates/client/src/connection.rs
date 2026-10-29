//! Transport: TCP/TLS connection setup and splitting into lines.

use std::io;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::crypto::{self, CryptoProvider};
use tokio_rustls::rustls::client::WantsClientCert;
use tokio_rustls::rustls::pki_types::pem::PemObject;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use tokio_rustls::rustls::{
    ClientConfig, ConfigBuilder, DigitallySignedStruct, RootCertStore, SignatureScheme,
};

use ircnext_proto::Message;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Maximum length allowed for a received line (tags included).
const MAX_LINE_BYTES: usize = 8191 + 512;

/// Bidirectional byte stream (plain TCP or TLS).
pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

/// Opens a connection to `host:port`, encrypted if `tls` is true,
/// presenting the client certificate `client_cert` if given.
pub async fn open(
    host: &str,
    port: u16,
    tls: bool,
    accept_invalid_certs: bool,
    client_cert: Option<&Path>,
) -> io::Result<Box<dyn Stream>> {
    let tcp = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "connection timed out"))??;
    tcp.set_nodelay(true)?;

    if !tls {
        return Ok(Box::new(tcp));
    }

    let server_name = ServerName::try_from(host.to_owned())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let config = match client_cert {
        Some(path) => {
            let (chain, key) = load_identity(path)?;
            let config = builder(accept_invalid_certs)
                .with_client_auth_cert(chain, key)
                .map_err(|error| invalid_certificate(path, error))?;
            Arc::new(config)
        }
        None => tls_config(accept_invalid_certs),
    };
    let connector = TlsConnector::from(config);
    let stream = tokio::time::timeout(CONNECT_TIMEOUT, connector.connect(server_name, tcp))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "TLS handshake timed out"))??;
    Ok(Box::new(stream))
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(crypto::ring::default_provider())
}

/// TLS configuration without a client certificate, shared by connections.
fn tls_config(accept_invalid_certs: bool) -> Arc<ClientConfig> {
    static VERIFIED: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    static UNVERIFIED: OnceLock<Arc<ClientConfig>> = OnceLock::new();

    let cell = if accept_invalid_certs { &UNVERIFIED } else { &VERIFIED };
    cell.get_or_init(|| Arc::new(builder(accept_invalid_certs).with_no_client_auth()))
        .clone()
}

/// TLS configuration up to the choice of the client certificate.
fn builder(accept_invalid_certs: bool) -> ConfigBuilder<ClientConfig, WantsClientCert> {
    let builder = ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .expect("the ring provider supports the default versions");
    if accept_invalid_certs {
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAnyCertificate(provider())))
    } else {
        builder.with_root_certificates(RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        })
    }
}

/// Reads a PEM file holding the certificate chain and the private key.
pub fn load_identity(
    path: &Path,
) -> io::Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    let pem = std::fs::read(path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("unreadable client certificate {}: {error}", path.display()),
        )
    })?;
    let chain = CertificateDer::pem_slice_iter(&pem)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| invalid_certificate(path, error))?;
    if chain.is_empty() {
        return Err(invalid_certificate(path, "no certificate"));
    }
    let key = PrivateKeyDer::from_pem_slice(&pem)
        .map_err(|_| invalid_certificate(path, "no private key"))?;
    Ok((chain, key))
}

fn invalid_certificate(path: &Path, error: impl std::fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("invalid client certificate {}: {error}", path.display()),
    )
}

/// Verifier accepting any certificate while still checking the handshake
/// signatures. Only used when explicitly requested.
#[derive(Debug)]
struct AcceptAnyCertificate(Arc<CryptoProvider>);

impl ServerCertVerifier for AcceptAnyCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, tokio_rustls::rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, tokio_rustls::rustls::Error> {
        crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, tokio_rustls::rustls::Error> {
        crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// IRC line reader.
///
/// [`LineReader::next_line`] can be cancelled without losing data (usable in
/// `tokio::select!`): the bytes already read stay in the internal buffer.
#[derive(Debug)]
pub struct LineReader<R> {
    inner: R,
    buffer: Vec<u8>,
}

impl<R: AsyncRead + Unpin> LineReader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            buffer: Vec::with_capacity(1024),
        }
    }

    /// Next line, without `\r\n`, or `None` if the connection is closed.
    pub async fn next_line(&mut self) -> io::Result<Option<String>> {
        loop {
            if let Some(end) = self.buffer.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = self.buffer.drain(..=end).collect();
                return Ok(Some(decode(&line)));
            }
            if self.buffer.len() > MAX_LINE_BYTES {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "line too long"));
            }
            let mut chunk = [0; 4096];
            let read = self.inner.read(&mut chunk).await?;
            if read == 0 {
                return Ok(None);
            }
            self.buffer.extend_from_slice(&chunk[..read]);
        }
    }
}

/// Decodes a line as UTF-8, falling back to Latin-1 for old clients.
fn decode(line: &[u8]) -> String {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    match std::str::from_utf8(line) {
        Ok(text) => text.to_owned(),
        Err(_) => line.iter().map(|&byte| char::from(byte)).collect(),
    }
}

/// Writes a message followed by `\r\n`.
pub async fn write_message<W: AsyncWrite + Unpin>(writer: &mut W, message: &Message) -> io::Result<()> {
    let line = format!("{message}\r\n");
    writer.write_all(line.as_bytes()).await?;
    writer.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_file(name: &str, content: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("ircnext-{}-{name}.pem", std::process::id()));
        std::fs::write(&path, content).unwrap();
        path
    }

    /// Real handshake with an Ergo server; run by hand with
    /// `cargo test -p ircnext-client -- --ignored external_on_ergo`.
    #[tokio::test]
    #[ignore = "connects to testnet.ergo.chat"]
    async fn external_on_ergo() {
        use tokio::io::{AsyncBufReadExt, BufReader};

        let key = rcgen::KeyPair::generate().unwrap();
        let cert = rcgen::CertificateParams::new(["test".to_owned()])
            .unwrap()
            .self_signed(&key)
            .unwrap();
        let path = identity_file("ergo", &format!("{}{}", cert.pem(), key.serialize_pem()));
        let stream = open("testnet.ergo.chat", 6697, true, false, Some(&path)).await.unwrap();
        let (reader, mut writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader).lines();
        let nick = format!("test{}", std::process::id() % 100_000);
        writer
            .write_all(format!("CAP LS 302\r\nNICK {nick}\r\nUSER test 0 * :test\r\n").as_bytes())
            .await
            .unwrap();
        let mut outcome = None;
        let mut caps = String::new();
        while let Ok(Ok(Some(line))) =
            tokio::time::timeout(Duration::from_secs(20), lines.next_line()).await
        {
            let reply = if line.contains(" CAP ") && line.contains(" LS ") {
                caps.push_str(&line);
                // `CAP * LS * :...`: the list continues on the next line.
                if line.contains(" LS * ") {
                    continue;
                }
                assert!(caps.contains("EXTERNAL"), "EXTERNAL advertised: {caps}");
                "CAP REQ sasl"
            } else if line.contains(" ACK ") {
                "AUTHENTICATE EXTERNAL"
            } else if line == "AUTHENTICATE +" {
                "AUTHENTICATE +"
            } else if [" 900 ", " 903 ", " 904 "].iter().any(|code| line.contains(code)) {
                outcome = Some(line);
                break;
            } else {
                continue;
            };
            writer.write_all(format!("{reply}\r\n").as_bytes()).await.unwrap();
        }
        writer.write_all(b"QUIT\r\n").await.unwrap();
        // Certificate unknown to the server: 904 expected, but the exchange happened.
        let outcome = outcome.expect("SASL reply from the server");
        assert!(outcome.contains(" 904 ") || outcome.contains(" 903 "), "{outcome}");
    }

    #[test]
    fn loads_certificate_and_key_from_one_pem_file() {
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = rcgen::CertificateParams::new(["me".to_owned()])
            .unwrap()
            .self_signed(&key)
            .unwrap();
        let path = identity_file("ok", &format!("{}{}", cert.pem(), key.serialize_pem()));
        let (chain, _) = load_identity(&path).unwrap();
        assert_eq!(chain.len(), 1);
        builder(false)
            .with_client_auth_cert(chain, load_identity(&path).unwrap().1)
            .expect("rustls accepts the generated key");

        let without_key = identity_file("no-key", &cert.pem());
        let error = load_identity(&without_key).unwrap_err();
        assert!(error.to_string().contains("no private key"), "{error}");
        assert!(load_identity(Path::new("/nowhere/at-all.pem")).is_err());
    }

    #[tokio::test]
    async fn splits_lines_across_chunks() {
        let data: &[u8] = b"PING :a\r\nPRIV";
        let (client, mut server) = tokio::io::duplex(64);
        let mut reader = LineReader::new(client);
        server.write_all(data).await.unwrap();
        assert_eq!(reader.next_line().await.unwrap().as_deref(), Some("PING :a"));
        server.write_all(b"MSG #c :x\n").await.unwrap();
        assert_eq!(reader.next_line().await.unwrap().as_deref(), Some("PRIVMSG #c :x"));
        drop(server);
        assert_eq!(reader.next_line().await.unwrap(), None);
    }

    #[tokio::test]
    async fn rejects_oversized_lines() {
        let (client, mut server) = tokio::io::duplex(64 * 1024);
        let mut reader = LineReader::new(client);
        server.write_all(&vec![b'a'; MAX_LINE_BYTES + 10]).await.unwrap();
        let error = reader.next_line().await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn decodes_latin1_fallback() {
        assert_eq!(decode(b"caf\xe9\r\n"), "café");
        assert_eq!(decode("café\n".as_bytes()), "café");
    }
}
