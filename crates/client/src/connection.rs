//! Transport : établissement de la connexion TCP/TLS et découpage en lignes.

use std::io;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::crypto::{self, CryptoProvider};
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use tokio_rustls::rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};

use ircnext_proto::Message;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Longueur maximale tolérée pour une ligne reçue (tags compris).
const MAX_LINE_BYTES: usize = 8191 + 512;

/// Flux d'octets bidirectionnel (TCP en clair ou TLS).
pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

/// Ouvre une connexion vers `host:port`, chiffrée si `tls` est vrai.
pub async fn open(
    host: &str,
    port: u16,
    tls: bool,
    accept_invalid_certs: bool,
) -> io::Result<Box<dyn Stream>> {
    let tcp = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "délai de connexion dépassé"))??;
    tcp.set_nodelay(true)?;

    if !tls {
        return Ok(Box::new(tcp));
    }

    let server_name = ServerName::try_from(host.to_owned())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let connector = TlsConnector::from(tls_config(accept_invalid_certs));
    let stream = tokio::time::timeout(CONNECT_TIMEOUT, connector.connect(server_name, tcp))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "délai de négociation TLS dépassé"))??;
    Ok(Box::new(stream))
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(crypto::ring::default_provider())
}

fn tls_config(accept_invalid_certs: bool) -> Arc<ClientConfig> {
    static VERIFIED: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    static UNVERIFIED: OnceLock<Arc<ClientConfig>> = OnceLock::new();

    let builder = || {
        ClientConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .expect("le fournisseur ring prend en charge les versions par défaut")
    };

    if accept_invalid_certs {
        UNVERIFIED
            .get_or_init(|| {
                let config = builder()
                    .dangerous()
                    .with_custom_certificate_verifier(Arc::new(AcceptAnyCertificate(provider())))
                    .with_no_client_auth();
                Arc::new(config)
            })
            .clone()
    } else {
        VERIFIED
            .get_or_init(|| {
                let roots = RootCertStore {
                    roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
                };
                Arc::new(builder().with_root_certificates(roots).with_no_client_auth())
            })
            .clone()
    }
}

/// Vérificateur qui accepte n'importe quel certificat tout en contrôlant les
/// signatures de la négociation. Utilisé uniquement sur demande explicite.
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

/// Lecteur de lignes IRC.
///
/// [`LineReader::next_line`] peut être annulé sans perte de données (utilisable
/// dans `tokio::select!`) : les octets déjà lus restent dans le tampon interne.
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

    /// Ligne suivante, sans `\r\n`, ou `None` si la connexion est fermée.
    pub async fn next_line(&mut self) -> io::Result<Option<String>> {
        loop {
            if let Some(end) = self.buffer.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = self.buffer.drain(..=end).collect();
                return Ok(Some(decode(&line)));
            }
            if self.buffer.len() > MAX_LINE_BYTES {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "ligne trop longue"));
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

/// Décode une ligne en UTF-8, avec repli sur Latin-1 pour les anciens clients.
fn decode(line: &[u8]) -> String {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    match std::str::from_utf8(line) {
        Ok(text) => text.to_owned(),
        Err(_) => line.iter().map(|&byte| char::from(byte)).collect(),
    }
}

/// Écrit un message suivi de `\r\n`.
pub async fn write_message<W: AsyncWrite + Unpin>(writer: &mut W, message: &Message) -> io::Result<()> {
    let line = format!("{message}\r\n");
    writer.write_all(line.as_bytes()).await?;
    writer.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;

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
