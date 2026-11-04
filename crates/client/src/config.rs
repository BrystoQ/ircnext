//! Settings of a connection to an IRC server.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Everything needed to connect and register on a server.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)] // independent connection options
pub struct Config {
    pub host: String,
    pub port: u16,
    pub tls: bool,
    /// Accepts invalid or self-signed certificates (not recommended).
    pub accept_invalid_certs: bool,
    /// SHA-256 of the server certificate trusted on first use, when
    /// `accept_invalid_certs` is set: another certificate is refused.
    pub pinned_cert: Option<String>,
    pub nick: String,
    /// Nicknames tried, in order, if `nick` is already taken.
    pub alt_nicks: Vec<String>,
    pub username: String,
    pub realname: String,
    /// Server password (`PASS`).
    pub password: Option<Secret>,
    pub sasl: Option<Sasl>,
    /// Client certificate presented during the TLS handshake (`CertFP`): PEM
    /// file holding the certificate then its private key.
    pub client_cert: Option<PathBuf>,
    /// SASL `EXTERNAL` identification with the client certificate, instead
    /// of the password.
    pub sasl_external: bool,
    /// SOCKS5 proxy the connection goes through.
    pub proxy: Option<Proxy>,
    /// Channels joined automatically after registration.
    pub autojoin: Vec<String>,
    /// Automatic reconnection after an outage.
    pub auto_reconnect: bool,
}

impl Config {
    /// The credentials (SASL) can be sent: encrypted connection, or local
    /// server (bouncer on the same machine).
    pub fn can_send_credentials(&self) -> bool {
        self.tls || is_loopback(&self.host)
    }

    /// SASL `EXTERNAL` identification possible: requested, with a client
    /// certificate presented on a TLS connection.
    pub fn uses_external(&self) -> bool {
        self.sasl_external && self.tls && self.client_cert.is_some()
    }

    /// TLS connection whose certificate is verified.
    pub fn is_verified_tls(&self) -> bool {
        self.tls && !self.accept_invalid_certs
    }
}

/// `localhost` or loopback address (`127.0.0.0/8`, `::1`).
pub fn is_loopback(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 6697,
            tls: true,
            accept_invalid_certs: false,
            pinned_cert: None,
            nick: "ircnext".to_owned(),
            alt_nicks: Vec::new(),
            username: "ircnext".to_owned(),
            realname: "ircnext".to_owned(),
            password: None,
            sasl: None,
            client_cert: None,
            sasl_external: false,
            proxy: None,
            autojoin: Vec::new(),
            auto_reconnect: true,
        }
    }
}

/// SOCKS5 proxy (Tor, SSH tunnel...).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Proxy {
    pub host: String,
    pub port: u16,
    /// Username/password authentication, if the proxy requires it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<Secret>,
}

/// SASL `PLAIN` credentials.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Sasl {
    pub username: String,
    pub password: Secret,
}

/// Sensitive string, masked in debug output.
#[derive(Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_not_printed() {
        let sasl = Sasl {
            username: "me".into(),
            password: Secret::new("hunter2"),
        };
        assert!(!format!("{sasl:?}").contains("hunter2"));
    }
}
