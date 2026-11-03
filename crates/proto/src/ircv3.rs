//! IRCv3 extensions outside the base grammar: standard replies, STS policy,
//! client tags.
//!
//! References:
//! - <https://ircv3.net/specs/extensions/standard-replies>
//! - <https://ircv3.net/specs/extensions/sts>
//! - <https://ircv3.net/specs/client-tags/typing>
//! - Bot completion draft (not ratified):
//!   <https://github.com/ircv3/ircv3-specifications/pull/415>

use std::time::Duration;

use crate::message::{Command, Message};

/// Severity of a standard reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyKind {
    Fail,
    Warn,
    Note,
}

/// Standard reply (`FAIL`, `WARN` or `NOTE`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StandardReply<'a> {
    pub kind: ReplyKind,
    /// Command concerned, or `*`.
    pub command: &'a str,
    /// Machine-readable code (`ACCOUNT_REQUIRED`...).
    pub code: &'a str,
    pub context: &'a [String],
    pub description: &'a str,
}

impl<'a> StandardReply<'a> {
    pub fn parse(message: &'a Message) -> Option<Self> {
        let kind = match message.command {
            Command::Fail => ReplyKind::Fail,
            Command::Warn => ReplyKind::Warn,
            Command::Note => ReplyKind::Note,
            _ => return None,
        };
        let [command, code, context @ .., description] = message.params.as_slice() else {
            return None;
        };
        Some(Self {
            kind,
            command,
            code,
            context,
            description,
        })
    }
}

/// STS policy announced in the value of the `sts` capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StsPolicy {
    /// TLS port to use (only present on a plain-text connection).
    pub port: Option<u16>,
    /// Validity duration (only present on a secure connection).
    pub duration: Option<Duration>,
    pub preload: bool,
}

impl StsPolicy {
    /// Parses `port=6697,duration=2592000,preload`.
    pub fn parse(value: &str) -> Self {
        let mut policy = Self {
            port: None,
            duration: None,
            preload: false,
        };
        for key_value in value.split(',') {
            let (key, value) = key_value.split_once('=').unwrap_or((key_value, ""));
            match key {
                "port" => policy.port = value.parse().ok(),
                "duration" => policy.duration = value.parse().ok().map(Duration::from_secs),
                "preload" => policy.preload = true,
                _ => {}
            }
        }
        policy
    }
}

/// Typing state sent by the `+typing` client tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Typing {
    Active,
    Paused,
    Done,
}

impl Typing {
    pub const TAG: &'static str = "+typing";

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "paused" => Some(Self::Paused),
            "done" => Some(Self::Done),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Done => "done",
        }
    }

    /// `TAGMSG` message reporting the typing state to `target`.
    pub fn message(self, target: &str) -> Message {
        let mut message = Message::new(Command::Tagmsg, [target]);
        message.tags.insert(Self::TAG, self.as_str());
        message
    }
}

/// Completion offered by a bot (IRCv3 `autocomplete` draft).
///
/// The client sends the text being typed in a `TAGMSG`; the bot replies with
/// a list of edits: erase `erase` characters at the end of the text, then
/// append `append`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionEdit {
    pub erase: usize,
    pub append: String,
}

impl CompletionEdit {
    pub const REQUEST_TAG: &str = "+draft/autocomplete-request";
    pub const RESPONSE_TAG: &str = "+draft/autocomplete-response";

    /// `TAGMSG` message asking `target` to complete `text`.
    pub fn request(target: &str, text: &str) -> Message {
        let mut message = Message::new(Command::Tagmsg, [target]);
        message.tags.insert(Self::REQUEST_TAG, text);
        message
    }

    /// Tab-separated replies, each preceded by as many backspaces (0x08) as
    /// characters to erase.
    pub fn parse_response(value: &str) -> Vec<Self> {
        value
            .split('\t')
            .filter(|response| !response.is_empty())
            .map(|response| {
                let append = response.trim_start_matches('\u{8}');
                Self {
                    erase: response.len() - append.len(),
                    append: append.to_owned(),
                }
            })
            .collect()
    }

    /// Applies the edit to `text`.
    pub fn apply(&self, text: &str) -> String {
        let keep = match self.erase {
            0 => text.len(),
            n => text.char_indices().rev().nth(n - 1).map_or(0, |(index, _)| index),
        };
        format!("{}{}", &text[..keep], self.append)
    }
}

impl Message {
    /// Completions offered by a bot, if the message has any.
    pub fn completion_edits(&self) -> Option<Vec<CompletionEdit>> {
        self.tags
            .get(CompletionEdit::RESPONSE_TAG)
            .map(CompletionEdit::parse_response)
    }

    /// Unique id of the message (`msgid`).
    pub fn msgid(&self) -> Option<&str> {
        self.tags.get("msgid").filter(|id| !id.is_empty())
    }

    /// Reference of the batch (`batch`) the message belongs to.
    pub fn batch(&self) -> Option<&str> {
        self.tags.get("batch").filter(|id| !id.is_empty())
    }

    /// Account of the sender (`account-tag`).
    pub fn account(&self) -> Option<&str> {
        self.tags.get("account").filter(|account| !account.is_empty())
    }

    /// Sender identified as a bot (tag defined by `ISUPPORT BOT`).
    pub fn is_from_bot(&self) -> bool {
        self.tags.get("bot").is_some()
    }

    /// Typing state (`+typing`), if present.
    pub fn typing(&self) -> Option<Typing> {
        self.tags.get(Typing::TAG).and_then(Typing::parse)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_replies() {
        let message: Message = "FAIL CHATHISTORY INVALID_TARGET #chan :Messages could not be retrieved"
            .parse()
            .unwrap();
        let reply = StandardReply::parse(&message).unwrap();
        assert_eq!(reply.kind, ReplyKind::Fail);
        assert_eq!(reply.command, "CHATHISTORY");
        assert_eq!(reply.code, "INVALID_TARGET");
        assert_eq!(reply.context, ["#chan"]);
        assert_eq!(reply.description, "Messages could not be retrieved");

        let short: Message = "NOTE * :missing parts".parse().unwrap();
        assert_eq!(StandardReply::parse(&short), None);
    }

    #[test]
    fn parses_sts_policy() {
        let policy = StsPolicy::parse("port=6697,duration=300,preload");
        assert_eq!(policy.port, Some(6697));
        assert_eq!(policy.duration, Some(Duration::from_secs(300)));
        assert!(policy.preload);
        assert_eq!(StsPolicy::parse("duration=x").duration, None);
    }

    #[test]
    fn builds_typing_tagmsg() {
        assert_eq!(Typing::Active.message("#c").to_string(), "@+typing=active TAGMSG #c");
        let message: Message = "@+typing=paused;msgid=x :n TAGMSG #c".parse().unwrap();
        assert_eq!(message.typing(), Some(Typing::Paused));
        assert_eq!(message.msgid(), Some("x"));
    }

    #[test]
    fn builds_completion_requests() {
        let message = CompletionEdit::request("#chan", "Bot: disconnect freen");
        assert_eq!(
            message.to_string(),
            "@+draft/autocomplete-request=Bot:\\sdisconnect\\sfreen TAGMSG #chan"
        );
    }

    #[test]
    fn parses_and_applies_completion_responses() {
        let message: Message = "@+draft/reply=1;+draft/autocomplete-response=ode\t\u{8}\u{8}\u{8}enode :b TAGMSG #c"
            .parse()
            .unwrap();
        let edits = message.completion_edits().unwrap();
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0].apply("disconnect freen"), "disconnect freenode");
        assert_eq!(edits[1], CompletionEdit { erase: 3, append: "enode".into() });
        assert_eq!(edits[1].apply("frenod"), "freenode");
        assert_eq!(CompletionEdit { erase: 9, append: "x".into() }.apply("été"), "x");
    }
}

/// Limits of the `draft/multiline` capability: a message of several lines
/// sent as one batch.
///
/// Reference: <https://ircv3.net/specs/extensions/multiline>
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Multiline {
    /// Maximum total size of the message text, in bytes.
    pub max_bytes: usize,
    /// Maximum number of lines in a batch, if limited.
    pub max_lines: Option<usize>,
}

impl Multiline {
    pub const CAP: &str = "draft/multiline";
    /// Tag of a line glued to the previous one, without a line break.
    pub const CONCAT: &str = "draft/multiline-concat";

    /// Parses the capability value, `max-bytes=4096,max-lines=24`.
    pub fn parse(value: &str) -> Option<Self> {
        let mut max_bytes = None;
        let mut max_lines = None;
        for item in value.split(',') {
            match item.split_once('=') {
                Some(("max-bytes", value)) => max_bytes = value.parse().ok(),
                Some(("max-lines", value)) => max_lines = value.parse().ok(),
                _ => {}
            }
        }
        Some(Self { max_bytes: max_bytes.filter(|&bytes| bytes > 0)?, max_lines })
    }

    /// Messages sending `text` to `target` as a `reference` batch, each line
    /// cut into pieces of at most `max_line_bytes` bytes. `None` if the text
    /// has a single line or does not fit the limits: it is then sent as
    /// separate messages.
    pub fn batch(&self, reference: &str, target: &str, text: &str, max_line_bytes: usize) -> Option<Vec<Message>> {
        let lines: Vec<&str> = text.trim_matches('\n').split('\n').map(|line| line.trim_end_matches('\r')).collect();
        let size = lines.iter().map(|line| line.len() + 1).sum::<usize>() - 1;
        if lines.len() < 2 || size > self.max_bytes {
            return None;
        }
        let mut messages = vec![Message::new(
            Command::Batch,
            [format!("+{reference}"), Self::CAP.to_owned(), target.to_owned()],
        )];
        for line in lines {
            for (index, piece) in pieces(line, max_line_bytes).into_iter().enumerate() {
                let mut message = Message::privmsg(target, piece);
                message.tags.insert("batch", reference);
                if index > 0 {
                    message.tags.insert(Self::CONCAT, "");
                }
                messages.push(message);
            }
        }
        if self.max_lines.is_some_and(|max| messages.len() - 1 > max) {
            return None;
        }
        messages.push(Message::new(Command::Batch, [format!("-{reference}")]));
        Some(messages)
    }

    /// Text of a received batch: lines joined with line breaks, except those
    /// tagged `draft/multiline-concat`, glued to the previous one.
    pub fn join<'a>(lines: impl IntoIterator<Item = &'a Message>) -> String {
        let mut text = String::new();
        for (index, line) in lines.into_iter().enumerate() {
            if index > 0 && line.tags.get(Self::CONCAT).is_none() {
                text.push('\n');
            }
            text.push_str(line.param(1).unwrap_or_default());
        }
        text
    }
}

/// Cuts a line into pieces of at most `max_bytes` bytes on character
/// boundaries, preferably after a space; gluing the pieces back gives the
/// line exactly.
fn pieces(line: &str, max_bytes: usize) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut rest = line;
    while rest.len() > max_bytes {
        let mut cut = max_bytes;
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        if let Some(space) = rest[..cut].rfind(' ').filter(|&i| i > cut / 2) {
            cut = space + 1;
        }
        pieces.push(&rest[..cut]);
        rest = &rest[cut..];
    }
    pieces.push(rest);
    pieces
}

#[cfg(test)]
mod multiline_tests {
    use super::*;

    const LIMITS: Multiline = Multiline { max_bytes: 4096, max_lines: Some(24) };

    fn wire(messages: &[Message]) -> Vec<String> {
        messages.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn parses_the_capability_value() {
        assert_eq!(Multiline::parse("max-bytes=4096,max-lines=24"), Some(LIMITS));
        assert_eq!(Multiline::parse("max-bytes=40000"), Some(Multiline { max_bytes: 40000, max_lines: None }));
        assert_eq!(Multiline::parse("max-lines=24"), None);
        assert_eq!(Multiline::parse(""), None);
    }

    #[test]
    fn sends_lines_as_one_batch() {
        let messages = LIMITS.batch("m1", "#rust", "first\nsecond\r\n\nfourth", 400).unwrap();
        assert_eq!(
            wire(&messages),
            [
                "BATCH +m1 draft/multiline #rust",
                "@batch=m1 PRIVMSG #rust first",
                "@batch=m1 PRIVMSG #rust second",
                "@batch=m1 PRIVMSG #rust :",
                "@batch=m1 PRIVMSG #rust fourth",
                "BATCH -m1",
            ]
        );
        assert_eq!(Multiline::join(&messages[1..5]), "first\nsecond\n\nfourth");
    }

    #[test]
    fn long_lines_are_glued_back_exactly() {
        let line = "word ".repeat(30);
        let text = format!("{line}\nend");
        let messages = LIMITS.batch("m2", "bob", &text, 40).unwrap();
        assert!(messages.iter().any(|m| m.tags.get(Multiline::CONCAT).is_some()));
        assert!(messages[1..messages.len() - 1].iter().all(|m| m.param(1).unwrap().len() <= 40));
        assert_eq!(Multiline::join(&messages[1..messages.len() - 1]), text);
    }

    #[test]
    fn falls_back_outside_the_limits() {
        assert_eq!(LIMITS.batch("m3", "#c", "one line", 400), None);
        let small = Multiline { max_bytes: 10, max_lines: None };
        assert_eq!(small.batch("m4", "#c", "hello\nworld!", 400), None);
        let few = Multiline { max_bytes: 4096, max_lines: Some(2) };
        assert_eq!(few.batch("m5", "#c", "a\nb\nc", 400), None);
    }
}
