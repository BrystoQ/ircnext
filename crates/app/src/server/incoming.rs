//! Routing of received messages to the tabs.

use std::time::Instant;

use chrono::Local;
use ircnext_client::proto::ircv3::{CompletionEdit, ReplyKind, StandardReply};
use ircnext_client::proto::numeric::{self, ERR_NOMOTD, RPL_ENDOFMOTD, RPL_TOPIC};
use ircnext_client::proto::text::{self, Ctcp};
use ircnext_client::fish::Dh1080;
use ircnext_client::proto::dcc::Dcc;
use ircnext_client::proto::{Command, Message, format};
use ircnext_client::whox_request;

use super::{Alert, BufferId, Context, Effects, REMOTE_COMPLETION_TIMEOUT, Server, message_time, parse_time};
use crate::buffer::{Kind, Line, LineKind};
use crate::i18n::tr;
use crate::suggest;
use crate::ignore;
use crate::tr;

/// Maximum length of a message preview in a notification.
const ALERT_PREVIEW: usize = 200;

/// Numeric replies handled elsewhere or of no interest to the user.
const SILENT_NUMERICS: &[u16] = &[
    numeric::RPL_ISUPPORT,
    numeric::RPL_TOPICWHOTIME,
    numeric::RPL_NAMREPLY,
    numeric::RPL_ENDOFNAMES,
    numeric::RPL_WHOSPCRPL,
    numeric::RPL_ENDOFWHO,
    numeric::RPL_NOTOPIC,
    numeric::RPL_CREATIONTIME,
    numeric::RPL_MONONLINE,
    numeric::RPL_MONOFFLINE,
    numeric::RPL_MONLIST,
    numeric::RPL_ENDOFMONLIST,
    numeric::RPL_SASLSUCCESS,
    numeric::RPL_SASLMECHS,
];

impl Server {
    /// Applies a message to the state and shows it in the relevant tabs.
    pub(super) fn receive(&mut self, message: &Message, ctx: &Context<'_>) -> Effects {
        // `draft/multiline`: lines are kept until the batch closes, then
        // handled as one message.
        if let Some(combined) = self.network.finish_multiline(message) {
            return self.receive(&combined, ctx);
        }
        if self.network.collect_multiline(message) {
            return Effects::default();
        }
        let mut effects = Effects::default();
        let source = message.source_nick().unwrap_or_default().to_owned();
        // History batches (`chathistory`) must not change the current state.
        let history = self
            .network
            .batch_of(message)
            .is_some_and(|batch| batch.kind.ends_with("chathistory"));
        // Shared channels, computed before the state forgets the user.
        let shared_channels = match message.command {
            Command::Quit | Command::Nick => self.network.channels_with(&source),
            _ => Vec::new(),
        };
        self.replaying = history;
        // Ignored users: messages, notices, CTCP, invitations and typing
        // indicators vanish without a trace.
        if let Some(kind) = self.ignore_kind(message)
            && !self.network.is_me(&source)
            && message
                .source
                .as_ref()
                .is_some_and(|s| ignore::is_ignored(&ctx.settings.ignored, s, kind, chrono::Utc::now().timestamp()))
        {
            return effects;
        }
        if !history {
            self.network.apply(message);
        }
        if self.on_list_reply(message) || self.is_quiet_reply(message) {
            return effects;
        }
        let time = message_time(message);
        let line = |kind| Line::new(time, kind);
        if !history {
            self.message_script_events(message, &source, &shared_channels);
        }

        match &message.command {
            Command::Privmsg | Command::Notice => {
                effects.config_changed |= self.on_text(message, &source, history, ctx);
            }
            Command::Tagmsg => self.on_tagmsg(message, &source),
            Command::Join => {
                let Some(channel) = message.param(0) else {
                    return effects;
                };
                let id = self.ensure(channel);
                let mask = message.source.as_ref().and_then(ircnext_client::proto::Source::mask);
                self.push(id, line(LineKind::Join { nick: source.clone(), mask }), ctx);
                if self.network.is_me(&source) && !history {
                    effects = self.on_self_join(id, channel);
                } else if !history && let Some(who) = &message.source {
                    self.apply_auto_modes(channel, who);
                }
            }
            Command::Part => {
                let Some(channel) = message.param(0) else {
                    return effects;
                };
                let id = self.ensure(channel);
                let reason = message.param(1).map(str::to_owned);
                self.push(id, line(LineKind::Part { nick: source.clone(), reason }), ctx);
                if self.network.is_me(&source) && !history {
                    if let Some(buffer) = self.buffer_mut(id) {
                        buffer.joined = false;
                    }
                    let mapping = self.network.isupport().casemapping;
                    let autojoin = &mut self.entry.connection.autojoin;
                    let before = autojoin.len();
                    autojoin.retain(|c| !mapping.eq(c, channel));
                    effects.config_changed = autojoin.len() != before;
                }
            }
            Command::Kick => {
                let (Some(channel), Some(nick)) = (message.param(0), message.param(1)) else {
                    return effects;
                };
                let id = self.ensure(channel);
                let kind = LineKind::Kick {
                    nick: nick.to_owned(),
                    by: source.clone(),
                    reason: message.param(2).map(str::to_owned),
                };
                self.push(id, line(kind), ctx);
                if self.network.is_me(nick)
                    && !history
                    && let Some(buffer) = self.buffer_mut(id)
                {
                    buffer.joined = false;
                }
            }
            Command::Quit => {
                let reason = message.param(0).map(str::to_owned);
                let mut targets: Vec<BufferId> =
                    shared_channels.iter().filter_map(|c| self.find(c)).collect();
                targets.extend(self.find(&source));
                for id in targets {
                    let kind = LineKind::Quit { nick: source.clone(), reason: reason.clone() };
                    self.push(id, line(kind), ctx);
                }
            }
            Command::Nick => {
                let Some(new) = message.param(0) else {
                    return effects;
                };
                self.on_nick(&source, new, &shared_channels, time, ctx);
            }
            Command::Topic => {
                if let Some(channel) = message.param(0) {
                    let id = self.ensure(channel);
                    let topic = message.param(1).filter(|t| !t.is_empty()).map(|t| self.display_topic(channel, t));
                    self.push(id, line(LineKind::Topic { nick: Some(source), topic }), ctx);
                }
            }
            Command::Numeric(RPL_TOPIC) => {
                if let (Some(channel), Some(topic)) = (message.param(1), message.param(2)) {
                    let id = self.ensure(channel);
                    let kind = LineKind::Topic { nick: None, topic: Some(self.display_topic(channel, topic)) };
                    self.push(id, line(kind), ctx);
                }
            }
            Command::Mode => {
                let target = message.param(0).unwrap_or_default();
                let modes = message.params.get(1..).unwrap_or_default().join(" ");
                let id = if self.network.is_channel(target) {
                    self.ensure(target)
                } else {
                    self.server_buffer()
                };
                self.push(id, line(LineKind::Mode { nick: source, modes }), ctx);
            }
            Command::Invite => {
                if let Some(channel) = message.param(1) {
                    let text = tr!("{nick} invites you to {channel}.", nick = source, channel = channel);
                    let id = ctx.active.filter(|id| self.buffer(*id).is_some());
                    let id = id.unwrap_or_else(|| self.server_buffer());
                    self.push(id, line(LineKind::Info(text)), ctx);
                }
            }
            Command::Redact => {
                if let (Some(target), Some(msgid)) = (message.param(0), message.param(1)) {
                    let buffer = if self.network.is_channel(target) || !self.network.is_me(target) {
                        self.find(target)
                    } else {
                        self.find(&source)
                    };
                    if let Some(buffer) = buffer.and_then(|id| self.buffer_mut(id)) {
                        buffer.redact(msgid);
                    }
                }
            }
            Command::Markread => {
                let timestamp = message
                    .param(1)
                    .and_then(|value| value.strip_prefix("timestamp="))
                    .and_then(parse_time);
                if let (Some(target), Some(timestamp)) = (message.param(0), timestamp)
                    && let Some(buffer) = self.find(target).and_then(|id| self.buffer_mut(id))
                {
                    buffer.mark_read_until(timestamp);
                }
            }
            Command::Rename => {
                if let (Some(old), Some(new)) = (message.param(0), message.param(1))
                    && let Some(id) = self.find(old)
                {
                    if let Some(buffer) = self.buffer_mut(id) {
                        new.clone_into(&mut buffer.name);
                    }
                    self.resort();
                    let text = tr!("Channel {old} was renamed to {new}.", old = old, new = new);
                    self.push(id, line(LineKind::Info(text)), ctx);
                }
            }
            Command::Fail | Command::Warn | Command::Note => {
                if let Some(reply) = StandardReply::parse(message) {
                    let text = format!("[{} {}] {}", reply.command, reply.code, reply.description);
                    let kind = match reply.kind {
                        ReplyKind::Fail => LineKind::Error(text),
                        ReplyKind::Warn | ReplyKind::Note => LineKind::Info(text),
                    };
                    let id = self.server_buffer();
                    self.push(id, line(kind), ctx);
                }
            }
            Command::Error => {
                let text = message.param(0).unwrap_or(tr("server error")).to_owned();
                self.info(LineKind::Error(text), ctx);
            }
            Command::Numeric(code @ (numeric::RPL_MONONLINE | numeric::RPL_MONOFFLINE)) => {
                self.on_monitor(*code == numeric::RPL_MONONLINE, message, ctx);
            }
            Command::Numeric(RPL_ENDOFMOTD | ERR_NOMOTD) => {
                self.on_motd_end();
                self.on_numeric(message, *ctx);
            }
            // Away reply: shown again only when the away message changes.
            Command::Numeric(numeric::RPL_AWAY) if !ctx.settings.repeat_away_replies => {
                if let (Some(nick), Some(reason)) = (message.param(1), message.param(2)) {
                    let key = self.network.fold(nick);
                    if self.away_replies.get(&key).map(String::as_str) != Some(reason) {
                        self.away_replies.insert(key, reason.to_owned());
                        self.on_numeric(message, *ctx);
                    }
                }
            }
            Command::Numeric(code) if !SILENT_NUMERICS.contains(code) => {
                self.on_numeric(message, *ctx);
            }
            _ => {}
        }
        effects
    }

    /// Returns whether the configuration changed (FiSH key exchanged).
    fn on_text(&mut self, message: &Message, source: &str, history: bool, ctx: &Context<'_>) -> bool {
        let (Some(target), Some(body)) = (message.param(0), message.param(1)) else {
            return false;
        };
        let is_notice = message.command == Command::Notice;
        let own = self.network.is_me(source);
        if is_notice && !own && !history
            && let Some(dh) = Dh1080::parse(body)
        {
            return self.on_dh1080(source, dh, ctx);
        }
        let ctcp = text::parse_ctcp(body);

        if let Some(ctcp) = ctcp.filter(|ctcp| ctcp.command != "ACTION") {
            if !history {
                self.on_ctcp(ctcp, source, is_notice, own, ctx);
            }
            return false;
        }

        let stripped = self.network.isupport().strip_statusmsg(target);
        let from_server = message.source.as_ref().is_none_or(|s| s.user.is_none() && s.host.is_none());
        let id = if self.network.is_channel(stripped) {
            self.ensure(stripped)
        } else if own {
            self.ensure(target)
        } else if from_server || target == "*" {
            self.server_buffer()
        } else if is_notice {
            // Private notices (services...): existing conversation or current tab.
            self.find(source)
                .or(ctx.active.filter(|id| self.buffer(*id).is_some()))
                .unwrap_or_else(|| self.server_buffer())
        } else {
            self.ensure(source)
        };

        let prefix = self
            .network
            .channel(stripped)
            .and_then(|channel| channel.member(&self.network.fold(source)))
            .and_then(ircnext_client::Member::highest_prefix);
        // FiSH: the key is the one of the conversation (channel, or the
        // other person in private).
        let conversation = if self.network.is_channel(stripped) {
            stripped
        } else if own {
            target
        } else {
            source
        };
        let nick = source.to_owned();
        let (kind, encrypted) = if let Some(action) = ctcp {
            let (text, encrypted) = self.unseal(conversation, action.params.unwrap_or_default());
            (LineKind::Action { nick, text }, encrypted)
        } else {
            let (text, encrypted) = self.unseal(conversation, body);
            if is_notice {
                (LineKind::Notice { nick, text }, encrypted)
            } else {
                (LineKind::Message { nick, prefix, text }, encrypted)
            }
        };
        let body = match &kind {
            LineKind::Action { text, .. } | LineKind::Notice { text, .. } | LineKind::Message { text, .. } => text.clone(),
            _ => String::new(),
        };

        let mut line = Line::new(message_time(message), kind);
        line.own = own;
        line.bot = message.is_from_bot();
        line.msgid = message.msgid().map(str::to_owned);
        line.encrypted = encrypted;
        line.highlight = !own && !from_server && self.mentions_me(&body, ctx);
        let private = !is_notice && self.buffer(id).is_some_and(|b| b.kind == Kind::Query);
        if !history && !own && !is_notice && (line.highlight || private) {
            let title = if private { source.to_owned() } else { tr!("{nick} in {channel}", nick = source, channel = stripped) };
            let text = match &line.kind {
                LineKind::Action { text, .. } => format!("{source} {text}"),
                LineKind::Message { text, .. } => text.clone(),
                _ => String::new(),
            };
            let body: String = format::strip(&text).chars().take(ALERT_PREVIEW).collect();
            self.alerts.push(Alert { buffer: id, title, body, private });
        }
        self.push(id, line, ctx);
        if !own
            && let Some(buffer) = self.buffer_mut(id)
        {
            buffer.note_speaker(source);
        }
        // Scripts: messages of the others (own lines go through `on INPUT`).
        if !history && !own && !from_server {
            let name = match (is_notice, ctcp.is_some()) {
                (true, _) => "NOTICE",
                (false, true) => "ACTION",
                (false, false) => "TEXT",
            };
            let channel = self.network.is_channel(stripped).then_some(stripped);
            let address = message.source.as_ref().and_then(ircnext_client::proto::Source::mask).unwrap_or_default();
            self.script_event(id, name, channel, &body, false, &[("nick", source), ("address", &address)]);
        }
        false
    }

    fn mentions_me(&self, body: &str, ctx: &Context<'_>) -> bool {
        let plain = format::strip(body);
        let mapping = self.network.isupport().casemapping;
        text::mentions(&plain, self.network.nick(), mapping)
            || ctx
                .settings
                .highlight_words
                .iter()
                .any(|word| text::mentions(&plain, word, mapping))
    }

    fn on_ctcp(&mut self, ctcp: Ctcp<'_>, source: &str, is_reply: bool, own: bool, ctx: &Context<'_>) {
        let params = ctcp.params.unwrap_or_default();
        // File transfers are handled by the application, whatever the CTCP
        // reply settings and limits.
        if ctcp.command == "DCC" && !is_reply && !own {
            if let Some(dcc) = Dcc::parse(params) {
                self.dcc.push((source.to_owned(), dcc));
            }
            return;
        }
        if is_reply {
            let text = tr!("CTCP {command} reply from {nick}: {reply}", command = ctcp.command, nick = source, reply = params);
            self.info(LineKind::Info(text), ctx);
            return;
        }
        if own || !ctx.settings.ctcp_replies || !self.allow_ctcp_reply(Instant::now()) {
            return;
        }
        let version = ctx.settings.ctcp_version.trim();
        let reply = match ctcp.command {
            "VERSION" if !version.is_empty() => Some(version.to_owned()),
            "VERSION" => Some(format!("ircnext {}", env!("CARGO_PKG_VERSION"))),
            "PING" => Some(params.chars().take(64).collect()),
            // In UTC: the local time would reveal the time zone.
            "TIME" => Some(chrono::Utc::now().to_rfc2822()),
            "CLIENTINFO" => Some("ACTION CLIENTINFO PING TIME VERSION".to_owned()),
            _ => None,
        };
        if let Some(reply) = reply {
            let _ = self.send(Message::notice(source, text::ctcp(ctcp.command, Some(&reply))));
        }
        let text = tr!("CTCP {command} request from {nick}", command = ctcp.command, nick = source);
        self.info(LineKind::Info(text), ctx);
    }

    fn on_tagmsg(&mut self, message: &Message, source: &str) {
        if let Some(edits) = message.completion_edits() {
            self.on_completion_reply(edits, source);
            return;
        }
        let (Some(target), Some(state)) = (message.param(0), message.typing()) else {
            return;
        };
        if self.network.is_me(source) {
            return;
        }
        let name = if self.network.is_channel(target) { target } else { source };
        if let Some(buffer) = self.find(name).and_then(|id| self.buffer_mut(id)) {
            buffer.set_typing(source, state, Instant::now());
        }
    }

    /// Reply from a bot to a completion request still pending.
    fn on_completion_reply(&mut self, edits: Vec<CompletionEdit>, source: &str) {
        let Some(pending) = &mut self.remote_completion else {
            return;
        };
        if pending.sent.elapsed() >= REMOTE_COMPLETION_TIMEOUT {
            return;
        }
        for edit in edits {
            if pending.replies.len() >= suggest::MAX {
                break;
            }
            let text = edit.apply(&pending.request);
            if !pending.replies.iter().any(|(_, known)| *known == text) {
                pending.replies.push((source.to_owned(), text));
            }
        }
    }

    fn on_self_join(&mut self, id: BufferId, channel: &str) -> Effects {
        let mut effects = Effects::default();
        if let Some(buffer) = self.buffer_mut(id) {
            buffer.joined = true;
        }
        if self.network.has_cap("draft/chathistory") && self.prefs.history_lines > 0 {
            self.request_history(channel);
        }
        if self.network.isupport().whox {
            let _ = self.send(whox_request(channel));
        }
        let mapping = self.network.isupport().casemapping;
        let autojoin = &mut self.entry.connection.autojoin;
        if !autojoin.iter().any(|c| mapping.eq(c, channel)) {
            autojoin.push(channel.to_owned());
            effects.config_changed = true;
        }
        effects
    }

    /// Requests the latest messages of a channel or a conversation.
    pub(super) fn request_history(&self, target: &str) {
        let wanted = usize::from(self.prefs.history_lines);
        if wanted == 0 {
            return;
        }
        let limit = match self.network.isupport().chathistory {
            Some(max) if max > 0 => max.min(wanted),
            _ => wanted,
        };
        let _ = self.send(Message::new(
            Command::Chathistory,
            ["LATEST".to_owned(), target.to_owned(), "*".to_owned(), limit.to_string()],
        ));
    }

    fn on_nick(
        &mut self,
        old: &str,
        new: &str,
        shared_channels: &[String],
        time: chrono::DateTime<Local>,
        ctx: &Context<'_>,
    ) {
        let kind = LineKind::Nick { old: old.to_owned(), new: new.to_owned() };
        let mut targets: Vec<BufferId> =
            shared_channels.iter().filter_map(|c| self.find(c)).collect();
        if let Some(query) = self.find(old) {
            if let Some(buffer) = self.buffer_mut(query) {
                new.clone_into(&mut buffer.name);
            }
            self.resort();
            targets.push(query);
        }
        if self.network.is_me(new) {
            targets.push(self.server_buffer());
        }
        for id in targets {
            self.push(id, Line::new(time, kind.clone()), ctx);
            if let Some(buffer) = self.buffer_mut(id) {
                buffer.rename_speaker(old, new);
            }
        }
    }

    /// Kind of `message` for the ignore list (a letter of
    /// [`ignore::KINDS`]), or `None` if it cannot be ignored.
    fn ignore_kind(&self, message: &Message) -> Option<char> {
        let text = message.param(1).unwrap_or_default();
        let to_channel = message.param(0).is_some_and(|target| self.network.is_channel(target));
        let ctcp = text.strip_prefix('\x01').filter(|ctcp| !ctcp.starts_with("ACTION"));
        Some(match message.command {
            Command::Invite => 'i',
            Command::Notice if ctcp.is_some() => 't',
            Command::Notice => 'n',
            Command::Privmsg if ctcp.is_some_and(|ctcp| ctcp.starts_with("DCC ")) => 'd',
            Command::Privmsg if ctcp.is_some() => 't',
            Command::Privmsg | Command::Tagmsg if to_channel => 'c',
            Command::Privmsg | Command::Tagmsg => 'p',
            _ => return None,
        })
    }

    /// End of the MOTD: registration is complete and `ISUPPORT` known.
    fn on_motd_end(&self) {
        if self.network.isupport().monitor.is_none() {
            return;
        }
        let mut queries: Vec<&str> = self
            .buffers()
            .filter(|(_, b)| b.kind == Kind::Query)
            .map(|(_, b)| b.name.as_str())
            .collect();
        queries.extend(self.entry.notify.iter().map(String::as_str));
        if !queries.is_empty() {
            let _ = self.send(Message::new(Command::Monitor, ["+".to_owned(), queries.join(",")]));
        }
    }

    fn on_numeric(&mut self, message: &Message, ctx: Context<'_>) {
        let Command::Numeric(code) = message.command else {
            return;
        };
        // Parameters: <our nick> [context...] :<text>
        let params = message.params.get(1..).unwrap_or_default();
        let text = params.join(" ");
        let kind = if numeric::is_error(code) {
            LineKind::Error(text)
        } else {
            LineKind::Info(text)
        };
        let id = params
            .first()
            .filter(|_| params.len() > 1)
            .and_then(|context| self.find(context))
            .unwrap_or_else(|| self.server_buffer());
        self.push(id, Line::new(message_time(message), kind), &ctx);
    }
}
