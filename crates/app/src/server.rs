//! Network in use: connection, IRC state and its tabs.

mod incoming;
mod outgoing;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local, SecondsFormat, Utc};
use ircnext_client::proto::{Command, Message};
use ircnext_client::{Event, Handle, Network};

use crate::buffer::{Buffer, Kind, Line, LineKind};
use crate::i18n::tr;
use crate::secrets;
use crate::tr;
use crate::config::ServerEntry;
use crate::settings::Settings;

/// Stable id of a network during the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ServerId(pub u64);

/// Stable id of a tab (unaffected by renames).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BufferId(u64);

/// Connection state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Disconnected(Option<String>),
    Connecting(usize),
    Registering,
    Online,
    Waiting { error: Option<String>, retry_in: Duration },
}

/// Context given by the application to handle an event.
#[derive(Debug, Clone, Copy)]
pub struct Context<'a> {
    pub settings: &'a Settings,
    /// Displayed tab, if it belongs to this network.
    pub active: Option<BufferId>,
}

/// Consequences of a handling that the application must carry out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Effects {
    /// The network configuration changed and must be saved.
    pub config_changed: bool,
    /// STS policy received: (TLS port, duration in seconds).
    pub sts: Option<(u16, u64)>,
    /// Tab to display.
    pub focus: Option<BufferId>,
    /// The current tab was closed.
    pub closed: bool,
}

/// Application preferences the network needs outside of an event:
/// history requests, read markers, logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prefs {
    /// Lines requested when joining a channel (0: none).
    pub history_lines: u16,
    pub read_markers: bool,
    /// Logs folder, if logging is enabled.
    pub log_dir: Option<PathBuf>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self { history_lines: 100, read_markers: true, log_dir: None }
    }
}

/// Message to report to the user (notification).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub buffer: BufferId,
    /// Sender, and channel if any.
    pub title: String,
    pub body: String,
    /// Private message (otherwise a mention in a channel).
    pub private: bool,
}

/// At most `CTCP_REPLY_LIMIT` CTCP replies per `CTCP_REPLY_WINDOW`: beyond
/// that, requests are ignored (otherwise a request flood would get us
/// kicked off the server for excess flood).
const CTCP_REPLY_LIMIT: usize = 3;
const CTCP_REPLY_WINDOW: Duration = Duration::from_secs(10);

/// Delay after which bot replies to a completion request are ignored.
pub const REMOTE_COMPLETION_TIMEOUT: Duration = Duration::from_secs(15);

/// Completion request sent to bots (IRCv3 `autocomplete` draft).
#[derive(Debug, Clone)]
pub struct RemoteCompletion {
    pub buffer: BufferId,
    /// Input sent; the replies only hold while it is unchanged.
    pub request: String,
    pub sent: Instant,
    /// Completed inputs offered, with the bot offering them.
    pub replies: Vec<(String, String)>,
}

impl RemoteCompletion {
    /// Offers valid for the input `input` in the tab `buffer`.
    pub fn replies_for(&self, buffer: BufferId, input: &str) -> &[(String, String)] {
        let current = self.buffer == buffer && self.request == input && self.sent.elapsed() < REMOTE_COMPLETION_TIMEOUT;
        if current { &self.replies } else { &[] }
    }
}

#[derive(Debug)]
pub struct Server {
    pub id: ServerId,
    pub entry: ServerEntry,
    pub status: Status,
    pub network: Network,
    /// The user wants to be connected (drives the subscription).
    pub enabled: bool,
    /// Incremented to force a new connection.
    pub generation: u64,
    pub collapsed: bool,
    handle: Option<Handle>,
    buffers: Vec<(BufferId, Buffer)>,
    next_buffer: u64,
    pub remote_completion: Option<RemoteCompletion>,
    /// Times of the last CTCP replies (flood limiting).
    ctcp_replies: VecDeque<Instant>,
    pub prefs: Prefs,
    /// Messages to report, collected by the application.
    alerts: Vec<Alert>,
    /// The message being handled comes from a replayed history.
    replaying: bool,
}

impl Server {
    pub fn new(id: ServerId, entry: ServerEntry) -> Self {
        let network = Network::new(&entry.connection.nick);
        let enabled = entry.auto_connect;
        let mut server = Self {
            id,
            status: Status::Disconnected(None),
            network,
            enabled,
            generation: 0,
            collapsed: false,
            handle: None,
            buffers: Vec::new(),
            next_buffer: 0,
            remote_completion: None,
            ctcp_replies: VecDeque::new(),
            prefs: Prefs::default(),
            alerts: Vec::new(),
            replaying: false,
            entry,
        };
        server.insert_buffer(Buffer::new(Kind::Server, server.entry.name.clone()));
        server
    }

    pub fn name(&self) -> &str {
        &self.entry.name
    }

    pub fn is_online(&self) -> bool {
        self.status == Status::Online
    }

    pub fn server_buffer(&self) -> BufferId {
        self.buffers[0].0
    }

    pub fn buffers(&self) -> impl Iterator<Item = (BufferId, &Buffer)> {
        self.buffers.iter().map(|(id, buffer)| (*id, buffer))
    }

    pub fn buffer(&self, id: BufferId) -> Option<&Buffer> {
        self.buffers.iter().find(|(b, _)| *b == id).map(|(_, buffer)| buffer)
    }

    pub fn buffer_mut(&mut self, id: BufferId) -> Option<&mut Buffer> {
        self.buffers
            .iter_mut()
            .find(|(b, _)| *b == id)
            .map(|(_, buffer)| buffer)
    }

    /// Channel or conversation tab with this name (according to the casemapping).
    pub fn find(&self, name: &str) -> Option<BufferId> {
        let mapping = self.network.isupport().casemapping;
        self.buffers
            .iter()
            .find(|(_, buffer)| buffer.kind != Kind::Server && mapping.eq(&buffer.name, name))
            .map(|(id, _)| *id)
    }

    /// Tab of the channel or conversation `name`, created if needed.
    pub fn ensure(&mut self, name: &str) -> BufferId {
        if let Some(id) = self.find(name) {
            return id;
        }
        let kind = if self.network.is_channel(name) { Kind::Channel } else { Kind::Query };
        self.insert_buffer(Buffer::new(kind, name))
    }

    /// Inserts keeping the order: server, channels, then conversations.
    fn insert_buffer(&mut self, buffer: Buffer) -> BufferId {
        let id = BufferId(self.next_buffer);
        self.next_buffer += 1;
        let rank = |b: &Buffer| (b.kind as u8, b.name.to_lowercase());
        let key = rank(&buffer);
        let position = self.buffers.partition_point(|(_, existing)| rank(existing) <= key);
        self.buffers.insert(position, (id, buffer));
        id
    }

    pub fn remove_buffer(&mut self, id: BufferId) {
        if id != self.server_buffer() {
            self.buffers.retain(|(b, _)| *b != id);
        }
    }

    fn resort(&mut self) {
        self.buffers
            .sort_by_cached_key(|(_, b)| (b.kind as u8, b.name.to_lowercase()));
    }

    /// Sends a message if the connection is active.
    pub fn send(&self, message: Message) -> Result<(), String> {
        self.handle
            .as_ref()
            .ok_or_else(|| tr("Not connected.").to_owned())?
            .send(message)
            .map_err(|error| error.to_string())
    }

    /// Adds a line and updates the unread counters.
    pub fn push(&mut self, id: BufferId, mut line: Line, ctx: &Context<'_>) {
        let limit = ctx.settings.scrollback;
        let is_active = ctx.active == Some(id);
        // Fields borrowed separately: the tab is modified while the
        // preferences are read.
        let log_dir = self.prefs.log_dir.as_deref().filter(|_| !self.replaying);
        let network = self.entry.name.as_str();
        let Some((_, buffer)) = self.buffers.iter_mut().find(|(buffer, _)| *buffer == id) else {
            return;
        };
        if let LineKind::Message { text, .. } = &mut line.kind {
            if line.own
                && buffer.kind == Kind::Query
                && secrets::is_nickserv(&buffer.name)
                && let Some(masked) = secrets::mask(text)
            {
                *text = masked;
            }
            buffer.learn_bot_commands(text, line.bot);
        }
        // Log written after passwords are masked, and only for a line
        // actually added (not a duplicate).
        let logged = log_dir.map(|_| line.clone());
        let counts = line.kind.is_chat() && !line.own;
        let highlight = line.highlight || (counts && buffer.kind == Kind::Query);
        let added = buffer.push(line, limit);
        if added && counts && !is_active {
            buffer.unread += 1;
            buffer.highlighted |= highlight;
        }
        if added
            && let (Some(dir), Some(line)) = (log_dir, logged)
        {
            // A write error must not interrupt the conversation.
            let _ = crate::log::append(dir, network, &buffer.name, &line);
        }
    }

    /// Messages to report since the last call.
    pub fn take_alerts(&mut self) -> Vec<Alert> {
        std::mem::take(&mut self.alerts)
    }

    /// Reserves a CTCP reply if the limit allows it.
    fn allow_ctcp_reply(&mut self, now: Instant) -> bool {
        while self
            .ctcp_replies
            .front()
            .is_some_and(|&sent| now.duration_since(sent) >= CTCP_REPLY_WINDOW)
        {
            self.ctcp_replies.pop_front();
        }
        if self.ctcp_replies.len() >= CTCP_REPLY_LIMIT {
            return false;
        }
        self.ctcp_replies.push_back(now);
        true
    }

    /// Adds an information line to the server tab.
    pub fn info(&mut self, kind: LineKind, ctx: &Context<'_>) {
        let id = self.server_buffer();
        self.push(id, Line::new(Local::now(), kind), ctx);
    }

    /// Marks a tab as read and tells the server (`draft/read-marker`).
    pub fn mark_read(&mut self, id: BufferId) {
        let Some(buffer) = self.buffer_mut(id) else {
            return;
        };
        let had_unread = buffer.unread > 0;
        buffer.mark_read();
        let (kind, name, last) = (buffer.kind, buffer.name.clone(), buffer.last_time());
        if had_unread
            && kind != Kind::Server
            && self.prefs.read_markers
            && self.network.has_cap("draft/read-marker")
            && let Some(last) = last
        {
            let timestamp = format!("timestamp={}", format_time(last));
            let _ = self.send(Message::new(Command::Markread, [name, timestamp]));
        }
    }

    /// Handles an event of the connection.
    pub fn on_event(&mut self, event: Event, ctx: &Context<'_>) -> Effects {
        let mut effects = Effects::default();
        match event {
            Event::Connecting { attempt } => {
                self.status = Status::Connecting(attempt);
                let connection = &self.entry.connection;
                let text = tr!(
                    "Connecting to {host}:{port}{tls}…",
                    host = connection.host,
                    port = connection.port,
                    tls = if connection.tls { " (TLS)" } else { "" }
                );
                self.info(LineKind::Info(text), ctx);
            }
            Event::Connected(handle) => {
                self.handle = Some(handle);
                self.status = Status::Registering;
            }
            Event::Registered(registered) => {
                self.network.reset(&registered.nick, &registered.caps);
                self.status = Status::Online;
                if let Some(policy) = registered.sts
                    && let (Some(port), Some(duration)) = (policy.port, policy.duration)
                {
                    effects.sts = Some((port, duration.as_secs()));
                }
                if registered.sasl_refused {
                    let text = tr("SASL not used: the connection is not encrypted. Enable TLS to identify without exposing your password.");
                    self.info(LineKind::Error(text.to_owned()), ctx);
                }
                let caps = if registered.caps.is_empty() {
                    tr("none").to_owned()
                } else {
                    registered.caps.join(", ")
                };
                let text = tr!("Connected as {nick}. IRCv3 capabilities: {caps}.", nick = registered.nick, caps = caps);
                self.info(LineKind::Info(text), ctx);
            }
            Event::UpgradedToTls { port } => {
                self.entry.connection.tls = true;
                self.entry.connection.port = port;
                effects.config_changed = true;
                let text = tr!("The server requires TLS (STS): reconnecting on port {port}.", port = port);
                self.info(LineKind::Info(text), ctx);
            }
            Event::Message(message) => {
                effects = self.receive(&message, ctx);
                self.replaying = false;
            }
            Event::Disconnected { error, retry_in } => {
                self.handle = None;
                let text = match (&error, retry_in) {
                    (Some(error), Some(delay)) => {
                        tr!("Disconnected: {error}. Retrying in {seconds} s.", error = error, seconds = delay.as_secs())
                    }
                    (None, Some(delay)) => {
                        tr!("Disconnected. Retrying in {seconds} s.", seconds = delay.as_secs())
                    }
                    (Some(error), None) => tr!("Disconnected: {error}.", error = error),
                    (None, None) => tr("Disconnected.").to_owned(),
                };
                let ids: Vec<BufferId> = self.buffers().map(|(id, _)| id).collect();
                for id in ids {
                    self.push(id, Line::new(Local::now(), LineKind::Error(text.clone())), ctx);
                    if let Some(buffer) = self.buffer_mut(id)
                        && buffer.kind == Kind::Channel
                    {
                        buffer.joined = false;
                    }
                }
                self.status = if let Some(retry_in) = retry_in {
                    Status::Waiting { error, retry_in }
                } else {
                    self.enabled = false;
                    Status::Disconnected(error)
                };
            }
        }
        effects
    }
}

/// Timestamp in IRCv3 format (`2026-10-26T10:00:00.000Z`).
pub fn format_time(time: DateTime<Local>) -> String {
    time.with_timezone(&Utc).to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Timestamp of a message: `server-time` if present, otherwise now.
pub fn message_time(message: &Message) -> DateTime<Local> {
    message
        .server_time()
        .and_then(parse_time)
        .unwrap_or_else(Local::now)
}

pub fn parse_time(value: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.with_timezone(&Local))
}

#[cfg(test)]
mod tests;
