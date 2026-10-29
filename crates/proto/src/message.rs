//! IRC message: parsing of a received line and serialization of a line to send.
//!
//! Reference grammar: <https://modern.ircdocs.horse/#messages>

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use crate::tags::Tags;

/// A complete IRC message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub tags: Tags,
    pub source: Option<Source>,
    pub command: Command,
    pub params: Vec<String>,
}

/// Parsing error of an IRC line.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseError {
    #[error("empty line")]
    Empty,
    #[error("empty source after `:`")]
    EmptySource,
    #[error("missing command")]
    MissingCommand,
    #[error("invalid command: {0:?}")]
    InvalidCommand(String),
}

impl Message {
    /// Builds a message without tags or source, ready to be sent.
    pub fn new<P>(command: Command, params: impl IntoIterator<Item = P>) -> Self
    where
        P: Into<String>,
    {
        Self {
            tags: Tags::default(),
            source: None,
            command,
            params: params.into_iter().map(Into::into).collect(),
        }
    }

    /// Parameter at the given index.
    pub fn param(&self, index: usize) -> Option<&str> {
        self.params.get(index).map(String::as_str)
    }

    /// Nickname (or server name) of the message source.
    pub fn source_nick(&self) -> Option<&str> {
        self.source.as_ref().map(|source| source.name.as_str())
    }

    /// Timestamp given by the server (`server-time` capability), in ISO 8601 format.
    pub fn server_time(&self) -> Option<&str> {
        self.tags.get("time")
    }

    pub fn privmsg(target: impl Into<String>, text: impl Into<String>) -> Self {
        Self::new(Command::Privmsg, [target.into(), text.into()])
    }

    pub fn notice(target: impl Into<String>, text: impl Into<String>) -> Self {
        Self::new(Command::Notice, [target.into(), text.into()])
    }

    pub fn join(channel: impl Into<String>, key: Option<&str>) -> Self {
        let mut params = vec![channel.into()];
        params.extend(key.map(str::to_owned));
        Self::new(Command::Join, params)
    }

    pub fn part(channel: impl Into<String>, reason: Option<&str>) -> Self {
        let mut params = vec![channel.into()];
        params.extend(reason.map(str::to_owned));
        Self::new(Command::Part, params)
    }

    pub fn nick(nick: impl Into<String>) -> Self {
        Self::new(Command::Nick, [nick.into()])
    }

    pub fn quit(reason: Option<&str>) -> Self {
        Self::new(Command::Quit, reason.map(str::to_owned))
    }

    pub fn ping(token: impl Into<String>) -> Self {
        Self::new(Command::Ping, [token.into()])
    }

    pub fn pong(token: impl Into<String>) -> Self {
        Self::new(Command::Pong, [token.into()])
    }
}

impl FromStr for Message {
    type Err = ParseError;

    fn from_str(line: &str) -> Result<Self, Self::Err> {
        let mut rest = line.trim_end_matches(['\r', '\n']).trim_start_matches(' ');
        if rest.is_empty() {
            return Err(ParseError::Empty);
        }

        let tags = match rest.strip_prefix('@') {
            Some(after) => {
                let (raw, remainder) = split_word(after);
                rest = remainder;
                Tags::parse(raw)
            }
            None => Tags::default(),
        };

        rest = rest.trim_start_matches(' ');
        let source = match rest.strip_prefix(':') {
            Some(after) => {
                let (raw, remainder) = split_word(after);
                if raw.is_empty() {
                    return Err(ParseError::EmptySource);
                }
                rest = remainder;
                Some(Source::parse(raw))
            }
            None => None,
        };

        let (raw_command, mut rest) = split_word(rest.trim_start_matches(' '));
        if raw_command.is_empty() {
            return Err(ParseError::MissingCommand);
        }
        let command = Command::parse(raw_command)?;

        let mut params = Vec::new();
        loop {
            rest = rest.trim_start_matches(' ');
            if rest.is_empty() {
                break;
            }
            if let Some(trailing) = rest.strip_prefix(':') {
                params.push(trailing.to_owned());
                break;
            }
            let (param, remainder) = split_word(rest);
            params.push(param.to_owned());
            rest = remainder;
        }

        Ok(Self {
            tags,
            source,
            command,
            params,
        })
    }
}

impl fmt::Display for Message {
    /// Serializes the message to the wire format, without the final `\r\n`.
    ///
    /// The `\r`, `\n` and `\0` characters are removed from the parameters:
    /// they would otherwise allow injecting extra commands.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.tags.is_empty() {
            write!(f, "@{} ", self.tags)?;
        }
        if let Some(source) = &self.source {
            write!(f, ":{source} ")?;
        }
        write!(f, "{}", self.command)?;

        let last = self.params.len().saturating_sub(1);
        for (index, param) in self.params.iter().enumerate() {
            let needs_colon =
                index == last && (param.is_empty() || param.starts_with(':') || param.contains(' '));
            f.write_str(if needs_colon { " :" } else { " " })?;
            for c in param.chars().filter(|c| !matches!(c, '\r' | '\n' | '\0')) {
                fmt::Write::write_char(f, c)?;
            }
        }
        Ok(())
    }
}

fn split_word(s: &str) -> (&str, &str) {
    s.split_once(' ').unwrap_or((s, ""))
}

/// Source of a message: `nick!user@host` or server name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Source {
    /// Nickname, or server name.
    pub name: String,
    pub user: Option<String>,
    pub host: Option<String>,
}

impl Source {
    pub fn parse(raw: &str) -> Self {
        let (rest, host) = match raw.split_once('@') {
            Some((rest, host)) => (rest, Some(host.to_owned())),
            None => (raw, None),
        };
        let (name, user) = match rest.split_once('!') {
            Some((name, user)) => (name, Some(user.to_owned())),
            None => (rest, None),
        };
        Self {
            name: name.to_owned(),
            user,
            host,
        }
    }

    /// `user@host` part, if known.
    pub fn mask(&self) -> Option<String> {
        match (&self.user, &self.host) {
            (Some(user), Some(host)) => Some(format!("{user}@{host}")),
            (None, Some(host)) => Some(host.clone()),
            _ => None,
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)?;
        if let Some(user) = &self.user {
            write!(f, "!{user}")?;
        }
        if let Some(host) = &self.host {
            write!(f, "@{host}")?;
        }
        Ok(())
    }
}

macro_rules! commands {
    ($($variant:ident => $name:literal),* $(,)?) => {
        /// Verb of an IRC message.
        ///
        /// Common commands have their own variant; the others are kept as is
        /// (uppercase) in [`Command::Other`].
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub enum Command {
            $($variant,)*
            /// Numeric reply (`001`, `353`, ...).
            Numeric(u16),
            Other(String),
        }

        impl Command {
            /// Parses a verb, case-insensitively.
            pub fn parse(raw: &str) -> Result<Self, ParseError> {
                if raw.len() == 3 && raw.bytes().all(|b| b.is_ascii_digit()) {
                    return raw
                        .parse()
                        .map(Self::Numeric)
                        .map_err(|_| ParseError::InvalidCommand(raw.to_owned()));
                }
                if !raw.bytes().all(|b| b.is_ascii_alphabetic()) {
                    return Err(ParseError::InvalidCommand(raw.to_owned()));
                }
                $(if raw.eq_ignore_ascii_case($name) {
                    return Ok(Self::$variant);
                })*
                Ok(Self::Other(raw.to_ascii_uppercase()))
            }
        }

        impl fmt::Display for Command {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    $(Self::$variant => f.write_str($name),)*
                    Self::Numeric(code) => write!(f, "{code:03}"),
                    Self::Other(name) => f.write_str(name),
                }
            }
        }
    };
}

commands! {
    Account => "ACCOUNT",
    Authenticate => "AUTHENTICATE",
    Away => "AWAY",
    Batch => "BATCH",
    Cap => "CAP",
    Chathistory => "CHATHISTORY",
    Chghost => "CHGHOST",
    Error => "ERROR",
    Fail => "FAIL",
    Invite => "INVITE",
    Join => "JOIN",
    Kick => "KICK",
    Markread => "MARKREAD",
    Mode => "MODE",
    Monitor => "MONITOR",
    Names => "NAMES",
    Nick => "NICK",
    Note => "NOTE",
    Notice => "NOTICE",
    Part => "PART",
    Pass => "PASS",
    Ping => "PING",
    Pong => "PONG",
    Privmsg => "PRIVMSG",
    Quit => "QUIT",
    Redact => "REDACT",
    Rename => "RENAME",
    Setname => "SETNAME",
    Tagmsg => "TAGMSG",
    Topic => "TOPIC",
    User => "USER",
    Warn => "WARN",
    Who => "WHO",
    Whois => "WHOIS",
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Message {
        line.parse().expect("ligne valide")
    }

    #[test]
    fn parses_full_message() {
        let msg = parse("@time=2026-10-26T10:00:00Z :nick!user@host PRIVMSG #rust :hello world\r\n");
        assert_eq!(msg.server_time(), Some("2026-10-26T10:00:00Z"));
        assert_eq!(
            msg.source,
            Some(Source {
                name: "nick".into(),
                user: Some("user".into()),
                host: Some("host".into()),
            })
        );
        assert_eq!(msg.command, Command::Privmsg);
        assert_eq!(msg.params, ["#rust", "hello world"]);
    }

    #[test]
    fn parses_without_source_or_trailing() {
        let msg = parse("PING irc.example.net");
        assert_eq!(msg.source, None);
        assert_eq!(msg.command, Command::Ping);
        assert_eq!(msg.params, ["irc.example.net"]);
    }

    #[test]
    fn parses_numeric_and_server_source() {
        let msg = parse(":irc.example.net 001 me :Welcome to IRC");
        assert_eq!(msg.source_nick(), Some("irc.example.net"));
        assert_eq!(msg.command, Command::Numeric(1));
        assert_eq!(msg.params, ["me", "Welcome to IRC"]);
    }

    #[test]
    fn keeps_empty_trailing_and_colons_inside() {
        let msg = parse(":a TOPIC #c :");
        assert_eq!(msg.params, ["#c", ""]);
        let msg = parse(":a PRIVMSG #c ::-) ok");
        assert_eq!(msg.params, ["#c", ":-) ok"]);
    }

    #[test]
    fn tolerates_repeated_spaces() {
        let msg = parse(":a  MODE   #c  +o   nick");
        assert_eq!(msg.params, ["#c", "+o", "nick"]);
    }

    #[test]
    fn command_is_case_insensitive() {
        assert_eq!(parse("privmsg a b").command, Command::Privmsg);
        assert_eq!(parse("foo").command, Command::Other("FOO".into()));
    }

    #[test]
    fn rejects_invalid_lines() {
        assert_eq!("".parse::<Message>(), Err(ParseError::Empty));
        assert_eq!("\r\n".parse::<Message>(), Err(ParseError::Empty));
        assert_eq!(": PING".parse::<Message>(), Err(ParseError::EmptySource));
        assert_eq!(":src".parse::<Message>(), Err(ParseError::MissingCommand));
        assert!(matches!(
            "PR1VMSG x".parse::<Message>(),
            Err(ParseError::InvalidCommand(_))
        ));
    }

    #[test]
    fn serializes_with_trailing_only_when_needed() {
        assert_eq!(Message::privmsg("#c", "hi").to_string(), "PRIVMSG #c hi");
        assert_eq!(Message::privmsg("#c", "hi there").to_string(), "PRIVMSG #c :hi there");
        assert_eq!(Message::privmsg("#c", ":)").to_string(), "PRIVMSG #c ::)");
        assert_eq!(Message::new(Command::Topic, ["#c", ""]).to_string(), "TOPIC #c :");
        assert_eq!(Message::new(Command::Numeric(5), ["x"]).to_string(), "005 x");
    }

    #[test]
    fn strips_line_breaks_to_prevent_injection() {
        let msg = Message::privmsg("#c", "hello\r\nQUIT :pwned");
        assert_eq!(msg.to_string(), "PRIVMSG #c :helloQUIT :pwned");
    }

    #[test]
    fn round_trips_through_display() {
        let line = "@msgid=abc :n!u@h PRIVMSG #chan :some text";
        assert_eq!(parse(line).to_string(), line);
    }
}
