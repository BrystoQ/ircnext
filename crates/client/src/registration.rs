//! Enregistrement de la connexion : négociation CAP, authentification SASL
//! et choix du pseudonyme, sous forme de machine à états sans I/O.
//!
//! Références :
//! - <https://ircv3.net/specs/extensions/capability-negotiation>
//! - <https://ircv3.net/specs/extensions/sasl-3.1>
//! - <https://ircv3.net/specs/extensions/sts>

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ircnext_proto::numeric::{
    ERR_ERRONEUSNICKNAME, ERR_NICKCOLLISION, ERR_NICKNAMEINUSE, ERR_SASLABORTED, ERR_SASLALREADY,
    ERR_SASLFAIL, ERR_SASLTOOLONG, ERR_UNAVAILRESOURCE, RPL_SASLSUCCESS, RPL_WELCOME,
};
use ircnext_proto::ircv3::StsPolicy;
use ircnext_proto::{Command, Message};

use crate::config::{Config, Sasl};

/// Capacités IRCv3 demandées lorsque le serveur les propose.
const WANTED_CAPS: &[&str] = &[
    "account-notify",
    "account-tag",
    "away-notify",
    "batch",
    "cap-notify",
    "chghost",
    "draft/channel-rename",
    "draft/chathistory",
    "draft/event-playback",
    "draft/message-redaction",
    "draft/read-marker",
    "echo-message",
    "extended-join",
    "extended-monitor",
    "invite-notify",
    "labeled-response",
    "message-tags",
    "multi-prefix",
    "server-time",
    "setname",
    "standard-replies",
    "userhost-in-names",
];

/// Taille maximale d'un fragment `AUTHENTICATE`.
const SASL_CHUNK: usize = 400;

/// Nombre maximal de capacités retenues (protection contre un serveur hostile).
const MAX_CAPS: usize = 256;

/// Nombre de variantes `pseudo_` essayées avant d'ajouter un suffixe numérique.
const UNDERSCORE_ATTEMPTS: usize = 3;

/// Résultat d'un enregistrement réussi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registered {
    /// Pseudonyme effectivement attribué.
    pub nick: String,
    /// Capacités acceptées par le serveur.
    pub caps: Vec<String>,
    /// Politique STS annoncée sur une connexion TLS vérifiée, à mémoriser ;
    /// son port est celui de la connexion en cours.
    pub sts: Option<StsPolicy>,
    /// SASL configuré mais non utilisé : la connexion n'est pas chiffrée.
    pub sasl_refused: bool,
}

/// Avancement après le traitement d'un message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// Messages à envoyer (éventuellement aucun) ; l'enregistrement continue.
    Continue(Vec<Message>),
    Done(Registered),
    /// Le serveur exige TLS (STS) : il faut se reconnecter sur ce port.
    UpgradeToTls { port: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// En attente de la liste complète des capacités (`CAP LS`).
    Listing,
    /// `CAP REQ` envoyé, en attente de `ACK`/`NAK`.
    Requesting,
    Authenticating,
    /// `CAP END` envoyé (ou serveur sans CAP) : attente de `001`.
    Waiting,
}

/// Mécanisme SASL utilisé.
#[derive(Debug)]
enum Mechanism {
    /// Compte et mot de passe.
    Plain(Sasl),
    /// Certificat client présenté pendant la négociation TLS.
    External,
}

impl Mechanism {
    fn name(&self) -> &'static str {
        match self {
            Self::Plain(_) => "PLAIN",
            Self::External => "EXTERNAL",
        }
    }
}

/// Machine à états de l'enregistrement.
#[derive(Debug)]
pub struct Registration {
    state: State,
    tls: bool,
    /// TLS avec certificat vérifié : seule condition pour mémoriser STS.
    verified: bool,
    port: u16,
    sasl_refused: bool,
    base_nick: String,
    alt_nicks: Vec<String>,
    nick: String,
    nick_attempts: usize,
    sasl: Option<Mechanism>,
    available: Vec<(String, Option<String>)>,
    acked: Vec<String>,
}

impl Registration {
    /// Crée la machine à états et renvoie les premiers messages à envoyer.
    pub fn start(config: &Config) -> (Self, Vec<Message>) {
        let registration = Self {
            state: State::Listing,
            tls: config.tls,
            verified: config.is_verified_tls(),
            port: config.port,
            sasl_refused: !config.uses_external()
                && config.sasl.is_some()
                && !config.can_send_credentials(),
            base_nick: config.nick.clone(),
            alt_nicks: config.alt_nicks.clone(),
            nick: config.nick.clone(),
            nick_attempts: 0,
            sasl: if config.uses_external() {
                Some(Mechanism::External)
            } else {
                // Jamais de mot de passe en clair sur le réseau.
                config.sasl.clone().filter(|_| config.can_send_credentials()).map(Mechanism::Plain)
            },
            available: Vec::new(),
            acked: Vec::new(),
        };

        let mut messages = vec![Message::new(Command::Cap, ["LS", "302"])];
        if let Some(password) = &config.password {
            messages.push(Message::new(Command::Pass, [password.expose()]));
        }
        messages.push(Message::nick(&config.nick));
        messages.push(Message::new(
            Command::User,
            [config.username.as_str(), "0", "*", config.realname.as_str()],
        ));
        (registration, messages)
    }

    /// Pseudonyme actuellement demandé au serveur.
    pub fn nick(&self) -> &str {
        &self.nick
    }

    pub fn handle(&mut self, message: &Message) -> Progress {
        let replies = match &message.command {
            Command::Cap => return self.on_cap(message),
            Command::Authenticate if message.param(0) == Some("+") => self.sasl_payload(),
            Command::Numeric(RPL_WELCOME) => {
                return Progress::Done(Registered {
                    nick: message.param(0).unwrap_or(&self.nick).to_owned(),
                    caps: std::mem::take(&mut self.acked),
                    sts: self.sts_policy().filter(|_| self.verified).map(|policy| StsPolicy {
                        port: Some(self.port),
                        ..policy
                    }),
                    sasl_refused: self.sasl_refused,
                });
            }
            Command::Numeric(
                RPL_SASLSUCCESS | ERR_SASLFAIL | ERR_SASLTOOLONG | ERR_SASLABORTED
                | ERR_SASLALREADY,
            ) if self.state == State::Authenticating => self.end(),
            Command::Numeric(
                ERR_NICKNAMEINUSE | ERR_ERRONEUSNICKNAME | ERR_NICKCOLLISION
                | ERR_UNAVAILRESOURCE,
            ) => vec![self.next_nick()],
            _ => Vec::new(),
        };
        Progress::Continue(replies)
    }

    fn on_cap(&mut self, message: &Message) -> Progress {
        let subcommand = message.param(1).unwrap_or_default().to_ascii_uppercase();
        match subcommand.as_str() {
            "LS" if self.state == State::Listing => {
                // `CAP * LS * :...` annonce une suite sur plusieurs lignes.
                let (more, list) = match (message.param(2), message.param(3)) {
                    (Some("*"), Some(list)) => (true, list),
                    (Some(list), _) => (false, list),
                    (None, _) => (false, ""),
                };
                let room = MAX_CAPS.saturating_sub(self.available.len());
                self.available.extend(list.split_whitespace().take(room).map(|cap| {
                    match cap.split_once('=') {
                        Some((name, value)) => (name.to_owned(), Some(value.to_owned())),
                        None => (cap.to_owned(), None),
                    }
                }));
                if more {
                    return Progress::Continue(Vec::new());
                }
                if let Some(port) = self.sts_policy().and_then(|policy| policy.port)
                    && !self.tls
                {
                    return Progress::UpgradeToTls { port };
                }
                Progress::Continue(self.request())
            }
            "ACK" if self.state == State::Requesting => {
                let list = message.params.last().map_or("", String::as_str);
                self.acked.extend(
                    list.split_whitespace()
                        .filter(|cap| !cap.starts_with('-'))
                        .map(str::to_owned),
                );
                if let Some(mechanism) = &self.sasl
                    && self.acked.iter().any(|cap| cap == "sasl")
                {
                    let name = mechanism.name();
                    self.state = State::Authenticating;
                    Progress::Continue(vec![Message::new(Command::Authenticate, [name])])
                } else {
                    Progress::Continue(self.end())
                }
            }
            "NAK" if self.state == State::Requesting => Progress::Continue(self.end()),
            _ => Progress::Continue(Vec::new()),
        }
    }

    fn sts_policy(&self) -> Option<StsPolicy> {
        self.available
            .iter()
            .find(|(name, _)| name == "sts")
            .map(|(_, value)| StsPolicy::parse(value.as_deref().unwrap_or_default()))
    }

    fn request(&mut self) -> Vec<Message> {
        let mechanism = self.sasl.as_ref().map(Mechanism::name);
        let requested: Vec<&str> = self
            .available
            .iter()
            .filter(|(name, value)| match name.as_str() {
                "sasl" => mechanism.is_some_and(|mechanism| {
                    value
                        .as_deref()
                        .is_none_or(|mechs| mechs.split(',').any(|m| m == mechanism))
                }),
                name => WANTED_CAPS.contains(&name),
            })
            .map(|(name, _)| name.as_str())
            .collect();

        if requested.is_empty() {
            return self.end();
        }
        self.state = State::Requesting;
        vec![Message::new(Command::Cap, ["REQ".to_owned(), requested.join(" ")])]
    }

    fn sasl_payload(&self) -> Vec<Message> {
        let sasl = match self.sasl.as_ref().filter(|_| self.state == State::Authenticating) {
            Some(Mechanism::Plain(sasl)) => sasl,
            // L'identité vient du certificat : réponse vide.
            Some(Mechanism::External) => {
                return vec![Message::new(Command::Authenticate, ["+"])];
            }
            None => return Vec::new(),
        };
        let credentials = format!(
            "{user}\0{user}\0{password}",
            user = sasl.username,
            password = sasl.password.expose()
        );
        let encoded = BASE64.encode(credentials);

        let mut messages: Vec<Message> = encoded
            .as_bytes()
            .chunks(SASL_CHUNK)
            .map(|chunk| {
                let chunk = std::str::from_utf8(chunk).expect("le base64 est de l'ASCII");
                Message::new(Command::Authenticate, [chunk])
            })
            .collect();
        // Un dernier fragment plein doit être suivi de `+` pour signaler la fin.
        if encoded.len() % SASL_CHUNK == 0 {
            messages.push(Message::new(Command::Authenticate, ["+"]));
        }
        messages
    }

    fn end(&mut self) -> Vec<Message> {
        self.state = State::Waiting;
        vec![Message::new(Command::Cap, ["END"])]
    }

    /// Pseudonymes de secours configurés, puis `pseudo_`, `pseudo__`... et
    /// enfin un suffixe numérique.
    fn next_nick(&mut self) -> Message {
        self.nick_attempts += 1;
        let generated = self.nick_attempts.saturating_sub(self.alt_nicks.len());
        self.nick = if let Some(alternative) = self.alt_nicks.get(self.nick_attempts - 1) {
            alternative.clone()
        } else if generated <= UNDERSCORE_ATTEMPTS {
            format!("{}{}", self.base_nick, "_".repeat(generated))
        } else {
            format!("{}{}", self.base_nick, generated)
        };
        Message::nick(&self.nick)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Secret;

    fn config(sasl: bool) -> Config {
        Config {
            host: "irc.example.net".into(),
            nick: "me".into(),
            sasl: sasl.then(|| Sasl {
                username: "account".into(),
                password: Secret::new("secret"),
            }),
            ..Config::default()
        }
    }

    fn msg(line: &str) -> Message {
        line.parse().unwrap()
    }

    fn lines(progress: Progress) -> Vec<String> {
        match progress {
            Progress::Continue(messages) => messages.iter().map(ToString::to_string).collect(),
            other => panic!("progression inattendue : {other:?}"),
        }
    }

    #[test]
    fn starts_with_cap_ls_nick_and_user() {
        let (_, messages) = Registration::start(&config(false));
        let lines: Vec<_> = messages.iter().map(ToString::to_string).collect();
        assert_eq!(lines, ["CAP LS 302", "NICK me", "USER ircnext 0 * ircnext"]);
    }

    #[test]
    fn requests_supported_caps_across_multiline_ls() {
        let (mut reg, _) = Registration::start(&config(false));
        assert_eq!(lines(reg.handle(&msg(":s CAP * LS * :multi-prefix sasl=PLAIN"))), Vec::<String>::new());
        assert_eq!(
            lines(reg.handle(&msg(":s CAP * LS :server-time unknown-cap"))),
            ["CAP REQ :multi-prefix server-time"]
        );
        assert_eq!(
            lines(reg.handle(&msg(":s CAP * ACK :multi-prefix server-time"))),
            ["CAP END"]
        );
        let Progress::Done(registered) = reg.handle(&msg(":s 001 me :Welcome")) else {
            panic!("devrait être enregistré");
        };
        assert_eq!(registered.caps, ["multi-prefix", "server-time"]);
    }

    #[test]
    fn ends_negotiation_when_nothing_to_request() {
        let (mut reg, _) = Registration::start(&config(false));
        assert_eq!(lines(reg.handle(&msg(":s CAP * LS :foo bar"))), ["CAP END"]);
    }

    fn external(tls: bool) -> Config {
        Config {
            tls,
            client_cert: Some("moi.pem".into()),
            sasl_external: true,
            ..config(true)
        }
    }

    #[test]
    fn authenticates_with_sasl_external() {
        let (mut reg, _) = Registration::start(&external(true));
        assert_eq!(
            lines(reg.handle(&msg(":s CAP * LS :sasl=PLAIN,EXTERNAL"))),
            ["CAP REQ sasl"]
        );
        assert_eq!(
            lines(reg.handle(&msg(":s CAP * ACK sasl"))),
            ["AUTHENTICATE EXTERNAL"]
        );
        assert_eq!(lines(reg.handle(&msg("AUTHENTICATE +"))), ["AUTHENTICATE +"]);
        assert_eq!(
            lines(reg.handle(&msg(":s 903 me :SASL authentication successful"))),
            ["CAP END"]
        );
    }

    #[test]
    fn external_needs_a_matching_mechanism_and_tls() {
        let (mut reg, _) = Registration::start(&external(true));
        assert_eq!(lines(reg.handle(&msg(":s CAP * LS :sasl=PLAIN"))), ["CAP END"]);

        // Sans TLS, pas de certificat : retour au mot de passe, refusé en clair.
        let (mut reg, _) = Registration::start(&external(false));
        assert_eq!(lines(reg.handle(&msg(":s CAP * LS :sasl=EXTERNAL,PLAIN"))), ["CAP END"]);
    }

    #[test]
    fn authenticates_with_sasl_plain() {
        let (mut reg, _) = Registration::start(&config(true));
        assert_eq!(
            lines(reg.handle(&msg(":s CAP * LS :sasl=EXTERNAL,PLAIN"))),
            ["CAP REQ sasl"]
        );
        assert_eq!(
            lines(reg.handle(&msg(":s CAP * ACK sasl"))),
            ["AUTHENTICATE PLAIN"]
        );
        let expected = BASE64.encode("account\0account\0secret");
        assert_eq!(
            lines(reg.handle(&msg("AUTHENTICATE +"))),
            [format!("AUTHENTICATE {expected}")]
        );
        assert_eq!(lines(reg.handle(&msg(":s 903 me :SASL successful"))), ["CAP END"]);
    }

    #[test]
    fn skips_sasl_without_plain_mechanism() {
        let (mut reg, _) = Registration::start(&config(true));
        assert_eq!(lines(reg.handle(&msg(":s CAP * LS :sasl=EXTERNAL"))), ["CAP END"]);
    }

    #[test]
    fn continues_registration_after_sasl_failure() {
        let (mut reg, _) = Registration::start(&config(true));
        reg.handle(&msg(":s CAP * LS :sasl"));
        reg.handle(&msg(":s CAP * ACK :sasl"));
        assert_eq!(lines(reg.handle(&msg(":s 904 me :SASL failed"))), ["CAP END"]);
    }

    #[test]
    fn retries_alternative_nicks() {
        let (mut reg, _) = Registration::start(&config(false));
        let in_use = msg(":s 433 * me :Nickname is already in use");
        assert_eq!(lines(reg.handle(&in_use)), ["NICK me_"]);
        assert_eq!(lines(reg.handle(&in_use)), ["NICK me__"]);
        assert_eq!(lines(reg.handle(&in_use)), ["NICK me___"]);
        assert_eq!(lines(reg.handle(&in_use)), ["NICK me4"]);
        assert_eq!(reg.nick(), "me4");
    }

    #[test]
    fn upgrades_plaintext_connection_on_sts() {
        let config = Config { tls: false, ..config(false) };
        let (mut reg, _) = Registration::start(&config);
        let progress = reg.handle(&msg(":s CAP * LS :sts=port=6697,duration=60 server-time"));
        assert_eq!(progress, Progress::UpgradeToTls { port: 6697 });
    }

    #[test]
    fn reports_sts_policy_on_secure_connection() {
        let (mut reg, _) = Registration::start(&config(false));
        reg.handle(&msg(":s CAP * LS :sts=duration=60"));
        let Progress::Done(registered) = reg.handle(&msg(":s 001 me :Welcome")) else {
            panic!("devrait être enregistré");
        };
        let policy = registered.sts.expect("politique STS");
        assert_eq!(policy.duration, Some(std::time::Duration::from_secs(60)));
        assert_eq!(policy.port, Some(6697), "port de la connexion en cours");
    }

    #[test]
    fn ignores_sts_policy_when_certificate_is_not_verified() {
        let config = Config { accept_invalid_certs: true, ..config(false) };
        let (mut reg, _) = Registration::start(&config);
        reg.handle(&msg(":s CAP * LS :sts=duration=60"));
        let Progress::Done(registered) = reg.handle(&msg(":s 001 me :Welcome")) else {
            panic!("devrait être enregistré");
        };
        assert_eq!(registered.sts, None);
    }

    #[test]
    fn never_sends_sasl_credentials_in_clear() {
        let plain = Config { tls: false, port: 6667, ..config(true) };
        let (mut reg, _) = Registration::start(&plain);
        assert_eq!(lines(reg.handle(&msg(":s CAP * LS :sasl=PLAIN server-time"))), ["CAP REQ server-time"]);
        reg.handle(&msg(":s CAP * ACK :server-time"));
        let Progress::Done(registered) = reg.handle(&msg(":s 001 me :Welcome")) else {
            panic!("devrait être enregistré");
        };
        assert!(registered.sasl_refused);

        let local = Config { tls: false, host: "127.0.0.1".into(), ..config(true) };
        let (mut reg, _) = Registration::start(&local);
        assert_eq!(lines(reg.handle(&msg(":s CAP * LS :sasl"))), ["CAP REQ sasl"]);
    }

    #[test]
    fn tries_alternative_nicks_first() {
        let config = Config { alt_nicks: vec!["moi".into(), "toi".into()], ..config(false) };
        let (mut reg, _) = Registration::start(&config);
        let in_use = msg(":s 433 * me :Nickname is already in use");
        assert_eq!(lines(reg.handle(&in_use)), ["NICK moi"]);
        assert_eq!(lines(reg.handle(&in_use)), ["NICK toi"]);
        assert_eq!(lines(reg.handle(&in_use)), ["NICK me_"]);
    }

    #[test]
    fn bounds_the_capability_list() {
        let (mut reg, _) = Registration::start(&config(false));
        let many: Vec<String> = (0..1000).map(|i| format!("cap{i}")).collect();
        reg.handle(&msg(&format!(":s CAP * LS * :{}", many.join(" "))));
        reg.handle(&msg(&format!(":s CAP * LS * :{}", many.join(" "))));
        assert_eq!(reg.available.len(), MAX_CAPS);
    }

    #[test]
    fn sasl_payload_of_exact_chunk_size_ends_with_plus() {
        let (mut reg, _) = Registration::start(&config(false));
        // 300 octets bruts → exactement 400 caractères base64, soit un fragment plein.
        reg.sasl = Some(Mechanism::Plain(Sasl {
            username: "u".repeat(149),
            password: Secret::new(String::new()),
        }));
        reg.state = State::Authenticating;
        let messages = reg.sasl_payload();
        let total: usize = messages.iter().map(|m| m.params[0].len()).sum();
        assert_eq!(BASE64.encode(format!("{0}\0{0}\0", "u".repeat(149))).len(), 400);
        assert_eq!(messages.last().unwrap().params[0], "+");
        assert_eq!(total, 401);
    }
}
