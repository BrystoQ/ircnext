//! Analyse de la ligne de saisie : texte à envoyer ou commande `/...`.

use std::fmt;

/// Ce que l'utilisateur a tapé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// Texte à envoyer tel quel dans l'onglet courant.
    Text(String),
    Command(Command),
}

/// Commandes reconnues, inspirées de celles de HexChat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Join { channels: String, key: Option<String> },
    Part { channel: Option<String>, reason: Option<String> },
    Msg { target: String, text: String },
    Query { nick: String, text: Option<String> },
    Notice { target: String, text: String },
    Me(String),
    Nick(String),
    Topic(Option<String>),
    Whois(String),
    Away(Option<String>),
    Back,
    Kick { nick: String, reason: Option<String> },
    Ban(String),
    Unban(String),
    /// `/op`, `/deop`, `/voice`, `/devoice` : mode de préfixe à appliquer.
    Prefix { adding: bool, mode: char, nicks: Vec<String> },
    Mode(String),
    Invite { nick: String, channel: Option<String> },
    Ctcp { target: String, command: String, params: Option<String> },
    Raw(String),
    Quit(Option<String>),
    Connect,
    Close,
    Clear,
    Help,
    /// `/ignore [masque]` : ajoute un masque, ou liste les ignorés.
    Ignore(Option<String>),
    Unignore(String),
}

/// Erreur d'analyse, affichée dans l'onglet courant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Unknown(String),
    Usage(&'static Spec),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(name) => {
                write!(f, "Commande inconnue : /{name}. Tapez /help pour la liste.")
            }
            Self::Usage(spec) => write!(f, "Utilisation : {}", spec.usage),
        }
    }
}

/// Description d'une commande pour l'aide et la complétion.
#[derive(Debug, PartialEq, Eq)]
pub struct Spec {
    pub names: &'static [&'static str],
    pub usage: &'static str,
    pub help: &'static str,
}

pub const COMMANDS: &[Spec] = &[
    Spec { names: &["join", "j"], usage: "/join #salon [clé]", help: "Rejoindre un salon" },
    Spec { names: &["part", "leave"], usage: "/part [#salon] [raison]", help: "Quitter un salon" },
    Spec { names: &["msg"], usage: "/msg <cible> <texte>", help: "Envoyer un message" },
    Spec { names: &["query", "q"], usage: "/query <pseudo> [texte]", help: "Ouvrir une conversation privée" },
    Spec { names: &["notice"], usage: "/notice <cible> <texte>", help: "Envoyer une notice" },
    Spec { names: &["ns", "nickserv"], usage: "/ns <commande>", help: "Commande pour NickServ (compte)" },
    Spec { names: &["cs", "chanserv"], usage: "/cs <commande>", help: "Commande pour ChanServ (salons)" },
    Spec { names: &["ms", "memoserv"], usage: "/ms <commande>", help: "Commande pour MemoServ (mémos)" },
    Spec { names: &["hs", "hostserv"], usage: "/hs <commande>", help: "Commande pour HostServ (vhost)" },
    Spec { names: &["me"], usage: "/me <action>", help: "Décrire une action" },
    Spec { names: &["nick"], usage: "/nick <pseudo>", help: "Changer de pseudonyme" },
    Spec { names: &["topic"], usage: "/topic [sujet]", help: "Afficher ou changer le sujet" },
    Spec { names: &["whois", "wi"], usage: "/whois <pseudo>", help: "Informations sur un utilisateur" },
    Spec { names: &["away"], usage: "/away [message]", help: "Se déclarer absent" },
    Spec { names: &["back"], usage: "/back", help: "Revenir de l'absence" },
    Spec { names: &["kick", "k"], usage: "/kick <pseudo> [raison]", help: "Expulser du salon" },
    Spec { names: &["ban"], usage: "/ban <masque|pseudo>", help: "Bannir du salon" },
    Spec { names: &["unban"], usage: "/unban <masque>", help: "Lever un bannissement" },
    Spec { names: &["op"], usage: "/op <pseudo>...", help: "Donner le statut d'opérateur" },
    Spec { names: &["deop"], usage: "/deop <pseudo>...", help: "Retirer le statut d'opérateur" },
    Spec { names: &["voice"], usage: "/voice <pseudo>...", help: "Donner la parole" },
    Spec { names: &["devoice"], usage: "/devoice <pseudo>...", help: "Retirer la parole" },
    Spec { names: &["mode"], usage: "/mode <arguments>", help: "Changer des modes" },
    Spec { names: &["invite"], usage: "/invite <pseudo> [#salon]", help: "Inviter sur un salon" },
    Spec { names: &["ctcp"], usage: "/ctcp <cible> <commande> [arguments]", help: "Envoyer une requête CTCP" },
    Spec { names: &["quote", "raw"], usage: "/quote <ligne IRC>", help: "Envoyer une ligne brute" },
    Spec { names: &["quit", "disconnect"], usage: "/quit [raison]", help: "Se déconnecter du serveur" },
    Spec { names: &["connect", "reconnect"], usage: "/connect", help: "Se (re)connecter au serveur" },
    Spec { names: &["close"], usage: "/close", help: "Fermer l'onglet courant" },
    Spec { names: &["clear"], usage: "/clear", help: "Effacer l'onglet courant" },
    Spec { names: &["ignore"], usage: "/ignore [pseudo ou masque]", help: "Ignorer quelqu'un (sans argument : la liste)" },
    Spec { names: &["unignore"], usage: "/unignore <pseudo ou masque>", help: "Ne plus ignorer quelqu'un" },
    Spec { names: &["help"], usage: "/help", help: "Afficher cette aide" },
];

/// Analyse une ligne saisie. `//texte` envoie `/texte` littéralement.
pub fn parse(line: &str) -> Result<Input, Error> {
    let Some(body) = line.strip_prefix('/') else {
        return Ok(Input::Text(line.to_owned()));
    };
    if body.starts_with('/') || body.is_empty() {
        return Ok(Input::Text(body.to_owned()));
    }

    let (name, args) = split(body);
    let name = name.to_ascii_lowercase();
    let spec = COMMANDS
        .iter()
        .find(|spec| spec.names.contains(&name.as_str()))
        .ok_or_else(|| Error::Unknown(name.clone()))?;
    let usage = || Error::Usage(spec);
    let required = |value: Option<&str>| value.map(str::to_owned).ok_or_else(usage);

    let command = match spec.names[0] {
        "join" => {
            let (channels, key) = split(args.ok_or_else(usage)?);
            Command::Join { channels: channels.to_owned(), key: owned(key) }
        }
        "part" => match args {
            Some(args) if args.starts_with(['#', '&']) => {
                let (channel, reason) = split(args);
                Command::Part { channel: Some(channel.to_owned()), reason: owned(reason) }
            }
            reason => Command::Part { channel: None, reason: owned(reason) },
        },
        "msg" => {
            let (target, text) = split(args.ok_or_else(usage)?);
            Command::Msg { target: target.to_owned(), text: required(text)? }
        }
        "query" => {
            let (nick, text) = split(args.ok_or_else(usage)?);
            Command::Query { nick: nick.to_owned(), text: owned(text) }
        }
        "notice" => {
            let (target, text) = split(args.ok_or_else(usage)?);
            Command::Notice { target: target.to_owned(), text: required(text)? }
        }
        "ns" | "cs" | "ms" | "hs" => {
            let service = crate::suggest::service(spec.names[0]).expect("service déclaré");
            Command::Msg { target: service.name.to_owned(), text: required(args)? }
        }
        "me" => Command::Me(required(args)?),
        "nick" => Command::Nick(required(args.map(first_word))?),
        "topic" => Command::Topic(owned(args)),
        "whois" => Command::Whois(required(args.map(first_word))?),
        "away" => Command::Away(owned(args)),
        "back" => Command::Back,
        "kick" => {
            let (nick, reason) = split(args.ok_or_else(usage)?);
            Command::Kick { nick: nick.to_owned(), reason: owned(reason) }
        }
        "ban" => Command::Ban(required(args.map(first_word))?),
        "unban" => Command::Unban(required(args.map(first_word))?),
        "op" | "deop" | "voice" | "devoice" => {
            let nicks: Vec<String> = args
                .unwrap_or_default()
                .split_whitespace()
                .map(str::to_owned)
                .collect();
            if nicks.is_empty() {
                return Err(usage());
            }
            let name = spec.names[0];
            Command::Prefix {
                adding: !name.starts_with("de"),
                mode: if name.ends_with("op") { 'o' } else { 'v' },
                nicks,
            }
        }
        "mode" => Command::Mode(required(args)?),
        "invite" => {
            let (nick, channel) = split(args.ok_or_else(usage)?);
            Command::Invite { nick: nick.to_owned(), channel: owned(channel.map(first_word)) }
        }
        "ctcp" => {
            let (target, rest) = split(args.ok_or_else(usage)?);
            let (command, params) = split(rest.ok_or_else(usage)?);
            Command::Ctcp {
                target: target.to_owned(),
                command: command.to_ascii_uppercase(),
                params: owned(params),
            }
        }
        "quote" => Command::Raw(required(args)?),
        "quit" => Command::Quit(owned(args)),
        "connect" => Command::Connect,
        "close" => Command::Close,
        "clear" => Command::Clear,
        "ignore" => Command::Ignore(owned(args.map(first_word))),
        "unignore" => Command::Unignore(required(args.map(first_word))?),
        "help" => Command::Help,
        other => unreachable!("commande déclarée sans traitement : {other}"),
    };
    Ok(Input::Command(command))
}

/// Sépare le premier mot du reste (`None` si le reste est vide).
fn split(text: &str) -> (&str, Option<&str>) {
    let text = text.trim_start();
    match text.split_once(' ') {
        Some((head, rest)) => {
            let rest = rest.trim_start();
            (head, (!rest.is_empty()).then_some(rest))
        }
        None => (text, None),
    }
}

fn first_word(text: &str) -> &str {
    split(text).0
}

fn owned(value: Option<&str>) -> Option<String> {
    value.map(str::to_owned).filter(|value| !value.trim().is_empty())
}

/// Noms de commandes (sans `/`) commençant par `prefix`, pour la complétion.
pub fn command_names(prefix: &str) -> Vec<String> {
    let prefix = prefix.to_ascii_lowercase();
    COMMANDS
        .iter()
        .flat_map(|spec| spec.names.iter())
        .filter(|name| name.starts_with(&prefix))
        .map(|name| format!("/{name}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(line: &str) -> Command {
        match parse(line) {
            Ok(Input::Command(command)) => command,
            other => panic!("commande attendue pour {line:?}, obtenu {other:?}"),
        }
    }

    #[test]
    fn plain_text_and_escapes() {
        assert_eq!(parse("salut"), Ok(Input::Text("salut".into())));
        assert_eq!(parse("//etc/hosts"), Ok(Input::Text("/etc/hosts".into())));
        assert_eq!(parse("/"), Ok(Input::Text(String::new())));
    }

    #[test]
    fn parses_commands_and_aliases() {
        assert_eq!(
            command("/j #rust secret"),
            Command::Join { channels: "#rust".into(), key: Some("secret".into()) }
        );
        assert_eq!(
            command("/MSG bob  salut   toi"),
            Command::Msg { target: "bob".into(), text: "salut   toi".into() }
        );
        assert_eq!(command("/me danse"), Command::Me("danse".into()));
        assert_eq!(command("/topic"), Command::Topic(None));
        assert_eq!(command("/away"), Command::Away(None));
        assert_eq!(command("/quit à plus"), Command::Quit(Some("à plus".into())));
    }

    #[test]
    fn service_shortcuts_message_the_service() {
        assert_eq!(
            command("/ns IDENTIFY secret"),
            Command::Msg { target: "NickServ".into(), text: "IDENTIFY secret".into() }
        );
        assert_eq!(
            command("/ChanServ op #rust"),
            Command::Msg { target: "ChanServ".into(), text: "op #rust".into() }
        );
        assert!(matches!(parse("/hs"), Err(Error::Usage(_))));
    }

    #[test]
    fn part_distinguishes_channel_and_reason() {
        assert_eq!(
            command("/part #rust bye"),
            Command::Part { channel: Some("#rust".into()), reason: Some("bye".into()) }
        );
        assert_eq!(
            command("/part fini"),
            Command::Part { channel: None, reason: Some("fini".into()) }
        );
    }

    #[test]
    fn prefix_commands() {
        assert_eq!(
            command("/deop alice bob"),
            Command::Prefix { adding: false, mode: 'o', nicks: vec!["alice".into(), "bob".into()] }
        );
        assert_eq!(
            command("/voice carol"),
            Command::Prefix { adding: true, mode: 'v', nicks: vec!["carol".into()] }
        );
    }

    #[test]
    fn ctcp_uppercases_command() {
        assert_eq!(
            command("/ctcp bob version"),
            Command::Ctcp { target: "bob".into(), command: "VERSION".into(), params: None }
        );
    }

    #[test]
    fn reports_errors() {
        assert_eq!(parse("/frobnicate"), Err(Error::Unknown("frobnicate".into())));
        let Err(Error::Usage(spec)) = parse("/msg bob") else {
            panic!("erreur d'utilisation attendue");
        };
        assert_eq!(spec.usage, "/msg <cible> <texte>");
        assert!(matches!(parse("/op"), Err(Error::Usage(_))));
    }

    #[test]
    fn completes_command_names() {
        assert_eq!(command_names("de"), ["/deop", "/devoice"]);
        assert_eq!(command_names("DIS"), ["/disconnect"]);
    }
}
