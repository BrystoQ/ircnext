//! Tabs (server, channel, private conversation) and their lines.

use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use ircnext_client::proto::format;
use ircnext_client::proto::ircv3::Typing;

use crate::suggest;

/// How long a “typing” indicator stays shown without a refresh.
const TYPING_TIMEOUT: Duration = Duration::from_secs(6);
/// Number of recent speakers remembered for the completion.
const RECENT_SPEAKERS: usize = 50;
/// Maximum number of typing indicators tracked per tab.
const MAX_TYPING: usize = 32;
/// Number of bot commands remembered per tab.
const BOT_COMMANDS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Server,
    Channel,
    Query,
}

/// Content of a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineKind {
    Message { nick: String, prefix: Option<char>, text: String },
    Action { nick: String, text: String },
    Notice { nick: String, text: String },
    Join { nick: String, mask: Option<String> },
    Part { nick: String, reason: Option<String> },
    Quit { nick: String, reason: Option<String> },
    Kick { nick: String, by: String, reason: Option<String> },
    Nick { old: String, new: String },
    Topic { nick: Option<String>, topic: Option<String> },
    Mode { nick: String, modes: String },
    /// Deleted message (`draft/message-redaction`).
    Redacted { nick: String },
    Info(String),
    Error(String),
}

impl LineKind {
    /// Joins, parts and nick changes, which can be hidden in the settings.
    pub fn is_presence(&self) -> bool {
        matches!(
            self,
            Self::Join { .. } | Self::Part { .. } | Self::Quit { .. } | Self::Nick { .. }
        )
    }

    /// Line written by a user (counted as unread).
    pub fn is_chat(&self) -> bool {
        matches!(self, Self::Message { .. } | Self::Action { .. } | Self::Notice { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Line {
    pub time: DateTime<Local>,
    pub kind: LineKind,
    pub msgid: Option<String>,
    /// Sent by us.
    pub own: bool,
    /// Mentions our nickname or a highlight word.
    pub highlight: bool,
    /// Sender flagged as a bot.
    pub bot: bool,
}

impl Line {
    /// Whether the line contains `needle` (already lowercase), nicknames
    /// included, ignoring formatting.
    pub fn matches(&self, needle: &str) -> bool {
        let fields: Vec<&str> = match &self.kind {
            LineKind::Message { nick, text, .. } | LineKind::Action { nick, text } | LineKind::Notice { nick, text } => {
                vec![nick, text]
            }
            LineKind::Join { nick, mask } => vec![nick, mask.as_deref().unwrap_or_default()],
            LineKind::Part { nick, reason } | LineKind::Quit { nick, reason } => {
                vec![nick, reason.as_deref().unwrap_or_default()]
            }
            LineKind::Kick { nick, by, reason } => vec![nick, by, reason.as_deref().unwrap_or_default()],
            LineKind::Nick { old, new } => vec![old, new],
            LineKind::Topic { nick, topic } => {
                vec![nick.as_deref().unwrap_or_default(), topic.as_deref().unwrap_or_default()]
            }
            LineKind::Mode { nick, modes } => vec![nick, modes],
            LineKind::Redacted { nick } => vec![nick],
            LineKind::Info(text) | LineKind::Error(text) => vec![text],
        };
        fields.iter().any(|field| format::strip(field).to_lowercase().contains(needle))
    }

    pub fn new(time: DateTime<Local>, kind: LineKind) -> Self {
        Self {
            time,
            kind,
            msgid: None,
            own: false,
            highlight: false,
            bot: false,
        }
    }
}

/// Tab.
#[derive(Debug, Clone)]
pub struct Buffer {
    pub kind: Kind,
    /// Displayed name (`#rust`, `alice`, network name).
    pub name: String,
    lines: VecDeque<Line>,
    msgids: HashSet<String>,
    pub unread: usize,
    pub highlighted: bool,
    /// Channel currently joined (always true for other tabs).
    pub joined: bool,
    typing: Vec<(String, Instant)>,
    speakers: VecDeque<String>,
    /// Bot commands (`!weather`) seen in the tab, most recent first.
    bot_commands: VecDeque<String>,
}

impl Buffer {
    pub fn new(kind: Kind, name: impl Into<String>) -> Self {
        Self {
            kind,
            name: name.into(),
            lines: VecDeque::new(),
            msgids: HashSet::new(),
            unread: 0,
            highlighted: false,
            joined: kind != Kind::Channel,
            typing: Vec::new(),
            speakers: VecDeque::new(),
            bot_commands: VecDeque::new(),
        }
    }

    pub fn lines(&self) -> impl DoubleEndedIterator<Item = &Line> + ExactSizeIterator {
        self.lines.iter()
    }

    pub fn last_time(&self) -> Option<DateTime<Local>> {
        self.lines.back().map(|line| line.time)
    }

    /// Adds a line at its chronological place (history can arrive after more
    /// recent lines). Returns `false` for a duplicate (`msgid`).
    pub fn push(&mut self, line: Line, limit: usize) -> bool {
        if let Some(id) = &line.msgid
            && !self.msgids.insert(id.clone())
        {
            return false;
        }
        let position = self.lines.partition_point(|existing| existing.time <= line.time);
        self.lines.insert(position, line);
        while self.lines.len() > limit {
            if let Some(evicted) = self.lines.pop_front()
                && let Some(id) = evicted.msgid
            {
                self.msgids.remove(&id);
            }
        }
        true
    }

    /// Replaces the content of a deleted message.
    pub fn redact(&mut self, msgid: &str) -> bool {
        let Some(line) = self.lines.iter_mut().find(|l| l.msgid.as_deref() == Some(msgid)) else {
            return false;
        };
        let nick = match &line.kind {
            LineKind::Message { nick, .. }
            | LineKind::Action { nick, .. }
            | LineKind::Notice { nick, .. } => nick.clone(),
            _ => return false,
        };
        line.kind = LineKind::Redacted { nick };
        true
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.msgids.clear();
    }

    pub fn mark_read(&mut self) {
        self.unread = 0;
        self.highlighted = false;
    }

    /// Marks as read if `until` covers the last line (`draft/read-marker`).
    pub fn mark_read_until(&mut self, until: DateTime<Local>) {
        if self.last_time().is_none_or(|last| last <= until) {
            self.mark_read();
        }
    }

    /// Remembers a speaker for the completion (most recent first).
    pub fn note_speaker(&mut self, nick: &str) {
        self.speakers.retain(|speaker| speaker != nick);
        self.speakers.push_front(nick.to_owned());
        self.speakers.truncate(RECENT_SPEAKERS);
        self.set_typing(nick, Typing::Done, Instant::now());
    }

    pub fn recent_speakers(&self) -> impl Iterator<Item = &str> {
        self.speakers.iter().map(String::as_str)
    }

    /// Remembers the bot commands of a message: the one opening an ordinary
    /// message, or all those a bot quotes (help, list...).
    pub fn learn_bot_commands(&mut self, text: &str, from_bot: bool) {
        let plain = format::strip(text);
        let mut words = plain.split_whitespace();
        let found: Vec<&str> = if from_bot {
            words.filter_map(suggest::bot_command).collect()
        } else {
            words.next().and_then(suggest::bot_command).into_iter().collect()
        };
        for command in found.into_iter().rev() {
            self.bot_commands.retain(|known| !known.eq_ignore_ascii_case(command));
            self.bot_commands.push_front(command.to_owned());
        }
        self.bot_commands.truncate(BOT_COMMANDS);
    }

    pub fn bot_commands(&self) -> impl Iterator<Item = &str> {
        self.bot_commands.iter().map(String::as_str)
    }

    pub fn rename_speaker(&mut self, old: &str, new: &str) {
        for speaker in &mut self.speakers {
            if speaker == old {
                new.clone_into(speaker);
            }
        }
        self.typing.retain(|(nick, _)| nick != old);
    }

    pub fn set_typing(&mut self, nick: &str, state: Typing, now: Instant) {
        self.typing.retain(|(existing, _)| existing != nick);
        if state == Typing::Active && self.typing.len() < MAX_TYPING {
            self.typing.push((nick.to_owned(), now));
        }
    }

    /// Nicknames currently typing, after old indicators expire.
    pub fn typing(&self, now: Instant) -> impl Iterator<Item = &str> {
        self.typing
            .iter()
            .filter(move |(_, since)| now.duration_since(*since) < TYPING_TIMEOUT)
            .map(|(nick, _)| nick.as_str())
    }

    /// Removes expired indicators; returns `true` if some remain.
    pub fn expire_typing(&mut self, now: Instant) -> bool {
        self.typing
            .retain(|(_, since)| now.duration_since(*since) < TYPING_TIMEOUT);
        !self.typing.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(seconds: i64) -> DateTime<Local> {
        Local.timestamp_opt(1_700_000_000 + seconds, 0).unwrap()
    }

    fn info(seconds: i64, text: &str) -> Line {
        Line::new(at(seconds), LineKind::Info(text.into()))
    }

    fn texts(buffer: &Buffer) -> Vec<String> {
        buffer
            .lines()
            .map(|line| match &line.kind {
                LineKind::Info(text) => text.clone(),
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn inserts_in_chronological_order() {
        let mut buffer = Buffer::new(Kind::Channel, "#c");
        buffer.push(info(10, "b"), 100);
        buffer.push(info(30, "d"), 100);
        buffer.push(info(5, "a"), 100);
        buffer.push(info(20, "c"), 100);
        assert_eq!(texts(&buffer), ["a", "b", "c", "d"]);
    }

    #[test]
    fn deduplicates_by_msgid_and_respects_limit() {
        let mut buffer = Buffer::new(Kind::Channel, "#c");
        let mut line = info(1, "x");
        line.msgid = Some("id1".into());
        assert!(buffer.push(line.clone(), 2));
        assert!(!buffer.push(line, 2));
        buffer.push(info(2, "y"), 2);
        buffer.push(info(3, "z"), 2);
        assert_eq!(texts(&buffer), ["y", "z"]);
        // The evicted id may come back (history reloaded).
        let mut again = info(4, "x");
        again.msgid = Some("id1".into());
        assert!(buffer.push(again, 2));
    }

    #[test]
    fn redacts_messages() {
        let mut buffer = Buffer::new(Kind::Channel, "#c");
        let mut line = Line::new(
            at(1),
            LineKind::Message { nick: "bob".into(), prefix: None, text: "oups".into() },
        );
        line.msgid = Some("m1".into());
        buffer.push(line, 10);
        assert!(buffer.redact("m1"));
        assert_eq!(
            buffer.lines().next().unwrap().kind,
            LineKind::Redacted { nick: "bob".into() }
        );
        assert!(!buffer.redact("absent"));
    }

    #[test]
    fn searches_line_content_without_formatting() {
        let line = Line::new(
            Local::now(),
            LineKind::Message { nick: "Alice".into(), prefix: None, text: "\x02Rust\x02 is nice".into() },
        );
        assert!(line.matches("alice"));
        assert!(line.matches("rust is"));
        assert!(!line.matches("go"));
    }

    #[test]
    fn learns_bot_commands() {
        let mut buffer = Buffer::new(Kind::Channel, "#c");
        buffer.learn_bot_commands("!weather Paris", false);
        buffer.learn_bot_commands("say hello !not_a_command", false);
        buffer.learn_bot_commands("Commands: \x02!help\x02, !quote, !weather.", true);
        assert_eq!(buffer.bot_commands().collect::<Vec<_>>(), ["!help", "!quote", "!weather"]);
    }

    #[test]
    fn tracks_recent_speakers() {
        let mut buffer = Buffer::new(Kind::Channel, "#c");
        buffer.note_speaker("alice");
        buffer.note_speaker("bob");
        buffer.note_speaker("alice");
        assert_eq!(buffer.recent_speakers().collect::<Vec<_>>(), ["alice", "bob"]);
        buffer.rename_speaker("bob", "robert");
        assert_eq!(buffer.recent_speakers().collect::<Vec<_>>(), ["alice", "robert"]);
    }

    #[test]
    fn typing_indicators_expire() {
        let mut buffer = Buffer::new(Kind::Channel, "#c");
        let start = Instant::now();
        buffer.set_typing("bob", Typing::Active, start);
        assert_eq!(buffer.typing(start).collect::<Vec<_>>(), ["bob"]);
        assert!(!buffer.expire_typing(start + TYPING_TIMEOUT));
        buffer.set_typing("bob", Typing::Active, start);
        buffer.set_typing("bob", Typing::Done, start);
        assert_eq!(buffer.typing(start).count(), 0);
    }

    #[test]
    fn read_marker_clears_unread_only_when_up_to_date() {
        let mut buffer = Buffer::new(Kind::Channel, "#c");
        buffer.push(info(10, "x"), 10);
        buffer.unread = 3;
        buffer.mark_read_until(at(5));
        assert_eq!(buffer.unread, 3);
        buffer.mark_read_until(at(10));
        assert_eq!(buffer.unread, 0);
    }
}
