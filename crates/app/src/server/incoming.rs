//! Routage des messages reçus vers les onglets.

use std::time::Instant;

use chrono::Local;
use ircnext_client::proto::ircv3::{CompletionEdit, ReplyKind, StandardReply};
use ircnext_client::proto::numeric::{self, ERR_NOMOTD, RPL_ENDOFMOTD, RPL_TOPIC};
use ircnext_client::proto::text::{self, Ctcp};
use ircnext_client::proto::{Command, Message, format};
use ircnext_client::whox_request;

use super::{Alert, BufferId, Context, Effects, REMOTE_COMPLETION_TIMEOUT, Server, message_time, parse_time};
use crate::buffer::{Kind, Line, LineKind};
use crate::suggest;
use crate::ignore;

/// Longueur maximale de l'aperçu d'un message dans une notification.
const ALERT_PREVIEW: usize = 200;

/// Réponses numériques traitées ailleurs ou sans intérêt pour l'utilisateur.
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
    /// Applique un message à l'état et l'affiche dans les onglets concernés.
    pub(super) fn receive(&mut self, message: &Message, ctx: &Context<'_>) -> Effects {
        let mut effects = Effects::default();
        let source = message.source_nick().unwrap_or_default().to_owned();
        // Les lots d'historique (`chathistory`) ne doivent pas modifier l'état courant.
        let history = self
            .network
            .batch_of(message)
            .is_some_and(|batch| batch.kind.ends_with("chathistory"));
        // Salons communs, à calculer avant que l'état n'oublie l'utilisateur.
        let shared_channels = match message.command {
            Command::Quit | Command::Nick => self.network.channels_with(&source),
            _ => Vec::new(),
        };
        self.replaying = history;
        // Utilisateurs ignorés : messages, notices, CTCP, invitations et
        // indicateurs de saisie disparaissent sans trace.
        let ignorable = matches!(
            message.command,
            Command::Privmsg | Command::Notice | Command::Tagmsg | Command::Invite
        );
        if ignorable
            && !self.network.is_me(&source)
            && message.source.as_ref().is_some_and(|s| ignore::is_ignored(&ctx.settings.ignored, s))
        {
            return effects;
        }
        if !history {
            self.network.apply(message);
        }
        let time = message_time(message);
        let line = |kind| Line::new(time, kind);

        match &message.command {
            Command::Privmsg | Command::Notice => self.on_text(message, &source, history, ctx),
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
                    let topic = message.param(1).filter(|t| !t.is_empty()).map(str::to_owned);
                    self.push(id, line(LineKind::Topic { nick: Some(source), topic }), ctx);
                }
            }
            Command::Numeric(RPL_TOPIC) => {
                if let (Some(channel), Some(topic)) = (message.param(1), message.param(2)) {
                    let id = self.ensure(channel);
                    let kind = LineKind::Topic { nick: None, topic: Some(topic.to_owned()) };
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
                    let text = format!("{source} vous invite sur {channel}.");
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
                    let text = format!("Le salon {old} a été renommé en {new}.");
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
                let text = message.param(0).unwrap_or("erreur du serveur").to_owned();
                self.info(LineKind::Error(text), ctx);
            }
            Command::Numeric(RPL_ENDOFMOTD | ERR_NOMOTD) => {
                self.on_motd_end();
                self.on_numeric(message, *ctx);
            }
            Command::Numeric(code) if !SILENT_NUMERICS.contains(code) => {
                self.on_numeric(message, *ctx);
            }
            _ => {}
        }
        effects
    }

    fn on_text(&mut self, message: &Message, source: &str, history: bool, ctx: &Context<'_>) {
        let (Some(target), Some(body)) = (message.param(0), message.param(1)) else {
            return;
        };
        let is_notice = message.command == Command::Notice;
        let own = self.network.is_me(source);
        let ctcp = text::parse_ctcp(body);

        if let Some(ctcp) = ctcp.filter(|ctcp| ctcp.command != "ACTION") {
            if !history {
                self.on_ctcp(ctcp, source, is_notice, own, ctx);
            }
            return;
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
            // Notices privées (services...) : conversation existante ou onglet courant.
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
        let nick = source.to_owned();
        let kind = match ctcp {
            Some(action) => LineKind::Action {
                nick,
                text: action.params.unwrap_or_default().to_owned(),
            },
            None if is_notice => LineKind::Notice { nick, text: body.to_owned() },
            None => LineKind::Message { nick, prefix, text: body.to_owned() },
        };

        let mut line = Line::new(message_time(message), kind);
        line.own = own;
        line.bot = message.is_from_bot();
        line.msgid = message.msgid().map(str::to_owned);
        line.highlight = !own && !from_server && self.mentions_me(body, ctx);
        let private = !is_notice && self.buffer(id).is_some_and(|b| b.kind == Kind::Query);
        if !history && !own && !is_notice && (line.highlight || private) {
            let title = if private { source.to_owned() } else { format!("{source} dans {stripped}") };
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
        if is_reply {
            let text = format!("Réponse CTCP {} de {source} : {params}", ctcp.command);
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
            // En UTC : l'heure locale révélerait le fuseau horaire.
            "TIME" => Some(chrono::Utc::now().to_rfc2822()),
            "CLIENTINFO" => Some("ACTION CLIENTINFO PING TIME VERSION".to_owned()),
            _ => None,
        };
        if let Some(reply) = reply {
            let _ = self.send(Message::notice(source, text::ctcp(ctcp.command, Some(&reply))));
        }
        let text = format!("Requête CTCP {} de {source}", ctcp.command);
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

    /// Réponse d'un bot à une demande de complétion encore en cours.
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

    /// Demande les derniers messages d'un salon ou d'une conversation.
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

    /// Fin du MOTD : l'enregistrement est complet et `ISUPPORT` connu.
    fn on_motd_end(&self) {
        if self.network.isupport().monitor.is_none() {
            return;
        }
        let queries: Vec<&str> = self
            .buffers()
            .filter(|(_, b)| b.kind == Kind::Query)
            .map(|(_, b)| b.name.as_str())
            .collect();
        if !queries.is_empty() {
            let _ = self.send(Message::new(Command::Monitor, ["+".to_owned(), queries.join(",")]));
        }
    }

    fn on_numeric(&mut self, message: &Message, ctx: Context<'_>) {
        let Command::Numeric(code) = message.command else {
            return;
        };
        // Paramètres : <notre pseudo> [contexte...] :<texte>
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
