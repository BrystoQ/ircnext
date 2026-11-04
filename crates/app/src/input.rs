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
    /// `/ignore [-pcntid] [-uN] [mask]`: adds a mask, for some kinds of
    /// messages (empty: all) and `seconds` if given, or lists the ignored
    /// users.
    Ignore { mask: Option<String>, kinds: String, seconds: Option<u64> },
    Unignore(String),
    /// `/notify [-r] [nick]`: adds to or removes from the notify list, or
    /// shows it.
    Notify { nick: Option<String>, remove: bool },
    /// `/dcc send <nick> <file>`: offers a file.
    DccSend { nick: String, path: String },
    /// `/dcc`: shows the transfers.
    Transfers,
    /// `/setkey [target] <key>`: FiSH key of a conversation.
    SetKey { target: Option<String>, key: String },
    /// `/delkey [target]`.
    DelKey(Option<String>),
    /// `/keyx [nick]`: DH1080 key exchange.
    KeyExchange(Option<String>),
    /// `/reload`: loads the scripts again.
    Reload,
    /// `/links`: the links seen in the open tabs.
    Links,
    /// `/logs`: the log viewer.
    Logs,
    /// `/aop` and `/avoice [-r] [mask] [#channel,...]`: `mode` is `o` or `v`.
    AutoMode { mode: char, mask: Option<String>, channels: Vec<String>, remove: bool },
    /// `/list [filter]`: the network's channels.
    List(Option<String>),
    /// `/channel [#channel]`: topic, modes and bans of a channel.
    Channel(Option<String>),
    /// `/echo <text>`: a line in the current tab, for the user only.
    Echo(String),
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
        Spec { names: &["ignore"], usage: tr("/ignore [-pcntid] [-uN] [nick or mask]"), help: tr("Ignore someone: private, channels, notices, CTCP, invitations, DCC; -uN for N seconds") },
        Spec { names: &["unignore"], usage: tr("/unignore <nick or mask>"), help: tr("Stop ignoring someone") },
        Spec { names: &["notify"], usage: tr("/notify [-r] [nick]"), help: tr("Be told when someone connects (no argument: the list)") },
        Spec { names: &["dcc"], usage: tr("/dcc [send <nick> <file>]"), help: tr("Send a file, or show the transfers") },
        Spec { names: &["setkey"], usage: tr("/setkey [#channel|nick] <key>"), help: tr("Encrypt a conversation with FiSH (cbc: prefix for CBC)") },
        Spec { names: &["delkey"], usage: tr("/delkey [#channel|nick]"), help: tr("Stop encrypting a conversation") },
        Spec { names: &["keyx"], usage: tr("/keyx [nick]"), help: tr("Exchange a FiSH key with someone (DH1080)") },
        Spec { names: &["channel"], usage: tr("/channel [#channel]"), help: tr("Topic, modes and bans of a channel") },
        Spec { names: &["aop"], usage: tr("/aop [-r] [mask] [#channels]"), help: tr("Give operator status to someone when they join") },
        Spec { names: &["avoice"], usage: tr("/avoice [-r] [mask] [#channels]"), help: tr("Give voice to someone when they join") },
        Spec { names: &["list"], usage: tr("/list [filter]"), help: tr("Browse the network's channels") },
        Spec { names: &["logs", "log"], usage: "/logs", help: tr("Read and search the saved conversations") },
        Spec { names: &["links", "url"], usage: "/links", help: tr("Show the links posted in the open tabs") },
        Spec { names: &["reload"], usage: "/reload", help: tr("Reload the scripts (scripts folder)") },
        Spec { names: &["echo"], usage: tr("/echo <text>"), help: tr("Show a line in the current tab, for you only") },
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
        "ignore" => {
            let (mut kinds, mut seconds, mut mask) = (String::new(), None, None);
            for word in args.unwrap_or_default().split_whitespace() {
                match word.strip_prefix('-') {
                    Some(flags) => {
                        let mut chars = flags.chars();
                        while let Some(flag) = chars.next() {
                            if flag == 'u' {
                                let digits: String = chars.clone().take_while(char::is_ascii_digit).collect();
                                chars.nth(digits.len().saturating_sub(1));
                                seconds = Some(digits.parse().map_err(|_| usage())?);
                            } else if crate::ignore::KINDS.contains(flag) {
                                if !kinds.contains(flag) {
                                    kinds.push(flag);
                                }
                            } else {
                                return Err(usage());
                            }
                        }
                    }
                    None if mask.is_none() => mask = Some(word.to_owned()),
                    None => return Err(usage()),
                }
            }
            if mask.is_none() && (seconds.is_some() || !kinds.is_empty()) {
                return Err(usage());
            }
            Command::Ignore { mask, kinds, seconds }
        }
        "unignore" => Command::Unignore(required(args.map(first_word))?),
        "notify" => match args.map(str::trim).unwrap_or_default().split_whitespace().collect::<Vec<_>>().as_slice() {
            [] => Command::Notify { nick: None, remove: false },
            ["-r", nick, ..] => Command::Notify { nick: Some((*nick).to_owned()), remove: true },
            [nick, ..] => Command::Notify { nick: Some((*nick).to_owned()), remove: false },
        },
        "help" => Command::Help,
        "setkey" => match split(args.ok_or_else(usage)?) {
            (key, None) => Command::SetKey { target: None, key: key.to_owned() },
            (target, Some(key)) => Command::SetKey { target: Some(target.to_owned()), key: key.trim().to_owned() },
        },
        "delkey" => Command::DelKey(owned(args.map(first_word))),
        "keyx" => Command::KeyExchange(owned(args.map(first_word))),
        "reload" => Command::Reload,
        "links" | "url" => Command::Links,
        "logs" | "log" => Command::Logs,
        "list" => Command::List(owned(args)),
        "aop" | "avoice" => {
            let mode = if name == "aop" { 'o' } else { 'v' };
            let mut words: Vec<&str> = args.unwrap_or_default().split_whitespace().collect();
            let remove = words.first() == Some(&"-r");
            if remove {
                words.remove(0);
            }
            let channels = words.get(1).map(|list| list.split(',').filter(|c| !c.is_empty()).map(str::to_owned).collect()).unwrap_or_default();
            Command::AutoMode { mode, mask: words.first().map(|mask| (*mask).to_owned()), channels, remove }
        }
        "channel" => Command::Channel(owned(args.map(first_word))),
        "echo" => {
            // Switches such as `-a` are accepted and ignored.
            let mut text = required(args)?;
            while text.starts_with('-') {
                text = text.split_once(' ').map(|(_, rest)| rest.trim_start().to_owned()).unwrap_or_default();
            }
            Command::Echo(text)
        }
        "dcc" => match args.map(split) {
            None | Some(("list" | "LIST", None)) => Command::Transfers,
            Some((sub, Some(rest))) if sub.eq_ignore_ascii_case("send") => {
                let (nick, path) = split(rest);
                let path = path.ok_or_else(usage)?.trim().trim_matches('"').to_owned();
                Command::DccSend { nick: nick.to_owned(), path }
            }
            _ => return Err(usage()),
        },
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
    fn parses_ignore_kinds_and_durations() {
        let ignore = |mask: Option<&str>, kinds: &str, seconds| Input::Command(Command::Ignore { mask: mask.map(str::to_owned), kinds: kinds.into(), seconds });
        assert_eq!(parse("/ignore"), Ok(ignore(None, "", None)));
        assert_eq!(parse("/ignore troll"), Ok(ignore(Some("troll"), "", None)));
        assert_eq!(parse("/ignore -pn -u60 troll"), Ok(ignore(Some("troll"), "pn", Some(60))));
        assert_eq!(parse("/ignore -cu600t *!*@spam"), Ok(ignore(Some("*!*@spam"), "ct", Some(600))));
        for bad in ["/ignore -x troll", "/ignore -u troll", "/ignore -p", "/ignore a b"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn parses_fish_commands() {
        assert_eq!(command("/setkey cbc:secret"), Command::SetKey { target: None, key: "cbc:secret".into() });
        assert_eq!(command("/setkey #c secret"), Command::SetKey { target: Some("#c".into()), key: "secret".into() });
        assert_eq!(command("/delkey"), Command::DelKey(None));
        assert_eq!(command("/keyx bob"), Command::KeyExchange(Some("bob".into())));
        assert!(matches!(parse("/setkey"), Err(Error::Usage(_))));
    }

    #[test]
    fn parses_dcc_commands() {
        assert_eq!(command("/dcc"), Command::Transfers);
        assert_eq!(
            command("/dcc send bob \"~/My Documents/notes.txt\""),
            Command::DccSend { nick: "bob".into(), path: "~/My Documents/notes.txt".into() }
        );
        assert!(matches!(parse("/dcc send bob"), Err(Error::Usage(_))));
        assert!(matches!(parse("/dcc chat bob"), Err(Error::Usage(_))));
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
        assert_eq!(command_names("de"), ["/deop", "/devoice", "/delkey"]);
        assert_eq!(command_names("DIS"), ["/disconnect"]);
    }
}
