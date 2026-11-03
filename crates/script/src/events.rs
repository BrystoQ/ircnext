//! `on` events: matching a client event against the handlers' headers
//! (`on level:EVENT:matchtext:target:{ … }`) and running them.

use std::collections::BTreeMap;

use crate::builtins::wildcard_match;
use crate::interp::{Error, Frame, Host, Interpreter};
use crate::parse::header_fields;

/// An event of the client, for the scripts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Event {
    /// `TEXT`, `JOIN`, `CONNECT`…
    pub name: String,
    /// Channel concerned, if any.
    pub channel: Option<String>,
    /// Text matched by `matchtext` (message, reason…), also `$1-`.
    pub text: String,
    /// The user caused it (own message, own join…).
    pub own: bool,
    /// We are an operator of the channel (for `@` handlers).
    pub operator: bool,
    /// Identifiers of the event: `nick`, `address`, `chan`, `target`,
    /// `knick`, `newnick`, `opnick`, `vnick`…
    pub values: BTreeMap<String, String>,
}

/// Parsed handler header.
struct Header<'a> {
    own_only: bool,
    others_only: bool,
    operator_only: bool,
    event: String,
    matchtext: Option<&'a str>,
    target: Option<&'a str>,
}

fn header(text: &str) -> Option<Header<'_>> {
    // `me:` comes before the level: `on me:*:JOIN:#:…`.
    let (own_only, text) = match text.get(..3) {
        Some(prefix) if prefix.eq_ignore_ascii_case("me:") => (true, &text[3..]),
        _ => (false, text),
    };
    let mut parts = text.splitn(4, ':');
    let level = parts.next()?;
    let event = parts.next()?.to_ascii_uppercase();
    let fields = header_fields(&event);
    let (matchtext, target) = match fields {
        4 => (parts.next(), parts.next()),
        3 => (None, parts.next()),
        _ => (None, None),
    };
    let level = level.to_ascii_lowercase();
    Some(Header {
        own_only,
        others_only: level.starts_with('!'),
        operator_only: level.contains('@'),
        event,
        matchtext,
        target,
    })
}

/// Whether `target` of a header (`#`, `?`, `*`, `#a,#b`…) covers the
/// event's channel (or private conversation when there is none).
fn target_matches(target: &str, channel: Option<&str>) -> bool {
    target.split(',').any(|target| match (target, channel) {
        ("*", _) | ("#", Some(_)) | ("?", None) => true,
        (pattern, Some(channel)) if pattern.starts_with(['#', '&']) => {
            wildcard_match(&pattern.to_lowercase(), &channel.to_lowercase(), false)
        }
        _ => false,
    })
}

/// Whether a handler answers an event.
fn applies(header: &Header<'_>, event: &Event) -> bool {
    // Events that only the user causes reach every handler; the others
    // reach `me:` handlers for the user's own actions, and the rest
    // for everybody else's.
    let user_event = matches!(event.name.as_str(), "INPUT" | "CONNECT" | "DISCONNECT" | "START");
    let by_whom = if user_event { true } else if header.own_only { event.own } else { !event.own };
    header.event == event.name
        && by_whom
        && (!header.others_only || !event.own)
        && (!header.operator_only || event.operator)
        && header.matchtext.is_none_or(|pattern| wildcard_match(&pattern.to_lowercase(), &event.text.to_lowercase(), true))
        && header.target.is_none_or(|target| target_matches(target, event.channel.as_deref()))
}

impl Interpreter {
    /// Runs the handlers of `event`, in load order. Returns whether one of
    /// them used `halt` (for `INPUT`: the text must not be sent), and the
    /// errors met (one handler's error does not stop the others).
    pub fn fire(&mut self, event: &Event, host: &mut dyn Host) -> (bool, Vec<Error>) {
        let mut handlers = Vec::new();
        for (index, loaded) in self.scripts.iter().enumerate() {
            for handler in &loaded.script.handlers {
                let Some(header) = header(&handler.header) else {
                    continue;
                };
                if applies(&header, event) {
                    handlers.push((index, handler.body.clone(), loaded.name.clone(), handler.line));
                }
            }
        }
        let mut halted = false;
        let mut errors = Vec::new();
        for (script, body, name, line) in handlers {
            let mut frame = Frame {
                args: event.text.split_whitespace().map(str::to_owned).collect(),
                script: Some(script),
                ..Frame::default()
            };
            self.event = event.values.clone();
            match self.run_body(&body, &mut frame, host) {
                Ok(stopped) => halted |= stopped,
                Err(error) => errors.push(Error::Runtime(format!("{name}, on {} (line {line}): {error}", event.name))),
            }
        }
        self.event.clear();
        (halted, errors)
    }
}
