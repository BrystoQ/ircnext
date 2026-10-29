//! Paramètres d'une connexion à un serveur IRC.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Tout ce qui est nécessaire pour se connecter et s'enregistrer sur un serveur.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub tls: bool,
    /// Accepte les certificats invalides ou auto-signés (déconseillé).
    pub accept_invalid_certs: bool,
    pub nick: String,
    /// Pseudonymes essayés, dans l'ordre, si `nick` est déjà pris.
    pub alt_nicks: Vec<String>,
    pub username: String,
    pub realname: String,
    /// Mot de passe serveur (`PASS`).
    pub password: Option<Secret>,
    pub sasl: Option<Sasl>,
    /// Salons rejoints automatiquement après l'enregistrement.
    pub autojoin: Vec<String>,
    /// Reconnexion automatique après une coupure.
    pub auto_reconnect: bool,
}

impl Config {
    /// Les identifiants (SASL) peuvent être envoyés : connexion chiffrée, ou
    /// serveur local (bouncer sur la même machine).
    pub fn can_send_credentials(&self) -> bool {
        self.tls || is_loopback(&self.host)
    }

    /// Connexion TLS dont le certificat est vérifié.
    pub fn is_verified_tls(&self) -> bool {
        self.tls && !self.accept_invalid_certs
    }
}

/// `localhost` ou adresse de bouclage (`127.0.0.0/8`, `::1`).
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
            nick: "ircnext".to_owned(),
            alt_nicks: Vec::new(),
            username: "ircnext".to_owned(),
            realname: "ircnext".to_owned(),
            password: None,
            sasl: None,
            autojoin: Vec::new(),
            auto_reconnect: true,
        }
    }
}

/// Identifiants SASL `PLAIN`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Sasl {
    pub username: String,
    pub password: Secret,
}

/// Chaîne sensible, masquée dans les sorties de débogage.
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
