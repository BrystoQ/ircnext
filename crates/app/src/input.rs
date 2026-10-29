//! Parsing of the input line: text to send or `/...` command.

use std::fmt;

use crate::i18n::tr;
use crate::tr;

/// What the user typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// Text to send as is in the current tab.
    Text(String),
    Command(Command),
}

/// Recognized commands, inspired by HexChat's.
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
    /// `/op`, `/deop`, `/voice`, `/devoice`: prefix mode to apply.
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
    /// `/ignore [mask]`: adds a mask, or lists the ignored users.
    Ignore(Option<String>),
    Unignore(String),
}

/// Parsing error, shown in the current tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Unknown(String),
    Usage(Spec),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(name) => {
                f.write_str(&tr!("Unknown command: /{name}. Type /help for the list.", name = name))
            }
            Self::Usage(spec) => f.write_str(&tr!("Usage: {usage}", usage = spec.usage)),
        }
    }
}

/// Description of a command for the help and the completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub names: &'static [&'static str],
    pub usage: &'static str,
    pub help: &'static str,
}

/// Known commands, translated into the current language.
pub fn commands() -> Vec<Spec> {
    vec![
        Spec { names: &["join", "j"], usage: tr("/join #channel [key]"), help: tr("Join a channel") },
        Spec { names: &["part", "leave"], usage: tr("/part [#channel] [reason]"), help: tr("Leave a channel") },
        Spec { names: &["msg"], usage: tr("/msg <target> <text>"), help: tr("Send a message") },
        Spec { names: &["query", "q"], usage: tr("/query <nick> [text]"), help: tr("Open a private conversation") },
        Spec { names: &["notice"], usage: tr("/notice <target> <text>"), help: tr("Send a notice") },
        Spec { names: &["ns", "nickserv"], usage: tr("/ns <command>"), help: tr("Command for NickServ (account)") },
        Spec { names: &["cs", "chanserv"], usage: tr("/cs <command>"), help: tr("Command for ChanServ (channels)") },
        Spec { names: &["ms", "memoserv"], usage: tr("/ms <command>"), help: tr("Command for MemoServ (memos)") },
        Spec { names: &["hs", "hostserv"], usage: tr("/hs <command>"), help: tr("Command for HostServ (vhost)") },
        Spec { names: &["me"], usage: tr("/me <action>"), help: tr("Describe an action") },
        Spec { names: &["nick"], usage: tr("/nick <nickname>"), help: tr("Change nickname") },
        Spec { names: &["topic"], usage: tr("/topic [topic]"), help: tr("Show or change the topic") },
        Spec { names: &["whois", "wi"], usage: tr("/whois <nick>"), help: tr("Information about a user") },
        Spec { names: &["away"], usage: tr("/away [message]"), help: tr("Mark yourself as away") },
        Spec { names: &["back"], usage: "/back", help: tr("Come back from away") },
        Spec { names: &["kick", "k"], usage: tr("/kick <nick> [reason]"), help: tr("Kick from the channel") },
        Spec { names: &["ban"], usage: tr("/ban <mask|nick>"), help: tr("Ban from the channel") },
        Spec { names: &["unban"], usage: tr("/unban <mask>"), help: tr("Lift a ban") },
        Spec { names: &["op"], usage: tr("/op <nick>..."), help: tr("Give operator status") },
        Spec { names: &["deop"], usage: tr("/deop <nick>..."), help: tr("Remove operator status") },
        Spec { names: &["voice"], usage: tr("/voice <nick>..."), help: tr("Give voice") },
        Spec { names: &["devoice"], usage: tr("/devoice <nick>..."), help: tr("Remove voice") },
        Spec { names: &["mode"], usage: tr("/mode <arguments>"), help: tr("Change modes") },
        Spec { names: &["invite"], usage: tr("/invite <nick> [#channel]"), help: tr("Invite to a channel") },
        Spec { names: &["ctcp"], usage: tr("/ctcp <target> <command> [arguments]"), help: tr("Send a CTCP request") },
        Spec { names: &["quote", "raw"], usage: tr("/quote <IRC line>"), help: tr("Send a raw line") },
        Spec { names: &["quit", "disconnect"], usage: tr("/quit [reason]"), help: tr("Disconnect from the server") },
        Spec { names: &["connect", "reconnect"], usage: "/connect", help: tr("(Re)connect to the server") },
        Spec { names: &["close"], usage: "/close", help: tr("Close the current tab") },
        Spec { names: &["clear"], usage: "/clear", help: tr("Clear the current tab") },
        Spec { names: &["ignore"], usage: tr("/ignore [nick or mask]"), help: tr("Ignore someone (no argument: the list)") },
        Spec { names: &["unignore"], usage: tr("/unignore <nick or mask>"), help: tr("Stop ignoring someone") },
        Spec { names: &["help"], usage: "/help", help: tr("Show this help") },
    ]
}

/// Parses a typed line. `//text` sends `/text` literally.
pub fn parse(line: &str) -> Result<Input, Error> {
    let Some(body) = line.strip_prefix('/') else {
        return Ok(Input::Text(line.to_owned()));
    };
    if body.starts_with('/') || body.is_empty() {
        return Ok(Input::Text(body.to_owned()));
    }

    let (name, args) = split(body);
    let name = name.to_ascii_lowercase();
    let spec = commands()
        .into_iter()
        .find(|spec| spec.names.contains(&name.as_str()))
        .ok_or_else(|| Error::Unknown(name.clone()))?;
    let usage = || Error::Usage(spec.clone());
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
            let service = crate::suggest::service(spec.names[0]).expect("declared service");
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
        other => unreachable!("declared command without handling: {other}"),
    };
    Ok(Input::Command(command))
}

/// Splits the first word from the rest (`None` if the rest is empty).
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

/// Command names (without `/`) starting with `prefix`, for the completion.
pub fn command_names(prefix: &str) -> Vec<String> {
    let prefix = prefix.to_ascii_lowercase();
    commands()
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
            other => panic!("command expected for {line:?}, got {other:?}"),
        }
    }

    #[test]
    fn plain_text_and_escapes() {
        assert_eq!(parse("hello"), Ok(Input::Text("hello".into())));
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
            command("/MSG bob  hello   you"),
            Command::Msg { target: "bob".into(), text: "hello   you".into() }
        );
        assert_eq!(command("/me dances"), Command::Me("dances".into()));
        assert_eq!(command("/topic"), Command::Topic(None));
        assert_eq!(command("/away"), Command::Away(None));
        assert_eq!(command("/quit see you"), Command::Quit(Some("see you".into())));
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
            command("/part done"),
            Command::Part { channel: None, reason: Some("done".into()) }
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
            panic!("usage error expected");
        };
        assert_eq!(spec.usage, "/msg <target> <text>");
        assert!(matches!(parse("/op"), Err(Error::Usage(_))));
    }

    #[test]
    fn completes_command_names() {
        assert_eq!(command_names("de"), ["/deop", "/devoice"]);
        assert_eq!(command_names("DIS"), ["/disconnect"]);
    }
}
