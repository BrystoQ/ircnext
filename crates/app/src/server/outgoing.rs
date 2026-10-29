//! Sending text and running typed commands.

use std::time::Instant;

use chrono::Local;
use ircnext_client::proto::ircv3::{CompletionEdit, Typing};
use ircnext_client::proto::text::{ctcp, split_message};
use ircnext_client::proto::{Command as Verb, Message};

use super::{BufferId, Context, Effects, RemoteCompletion, Server};
use crate::buffer::{Kind, Line, LineKind};
use crate::i18n::tr;
use crate::input::{self, Command};
use crate::tr;

/// Maximum size of the text of a `PRIVMSG`, in bytes, to stay under the
/// 512-byte limit once the server adds the source.
const MAX_TEXT_BYTES: usize = 400;

impl Server {
    /// Sends text in a tab (each pasted line becomes a message).
    pub fn send_text(&mut self, id: BufferId, text: &str, ctx: &Context<'_>) -> Result<(), String> {
        let target = self.text_target(id)?;
        for line in text.lines().filter(|line| !line.is_empty()) {
            for chunk in split_message(line, MAX_TEXT_BYTES) {
                self.send(Message::privmsg(&target, chunk))?;
                self.echo(id, LineKind::Message {
                    nick: self.network.nick().to_owned(),
                    prefix: self.own_prefix(&target),
                    text: chunk.to_owned(),
                }, ctx);
            }
        }
        Ok(())
    }

    /// Reports the typing state in a tab (IRCv3 `+typing`).
    pub fn send_typing(&self, id: BufferId, state: Typing) {
        if !self.network.has_cap("message-tags") {
            return;
        }
        if let Ok(target) = self.text_target(id) {
            let _ = self.send(state.message(&target));
        }
    }

    /// Asks bots to complete `text` (IRCv3 `autocomplete` draft).
    pub fn request_completion(&mut self, id: BufferId, text: &str) -> bool {
        if !self.network.has_cap("message-tags") {
            return false;
        }
        let Ok(target) = self.text_target(id) else {
            return false;
        };
        if self.send(CompletionEdit::request(&target, text)).is_err() {
            return false;
        }
        self.remote_completion = Some(RemoteCompletion {
            buffer: id,
            request: text.to_owned(),
            sent: Instant::now(),
            replies: Vec::new(),
        });
        true
    }

    /// Runs a command in the context of the tab `id`.
    pub fn execute(&mut self, id: BufferId, command: Command, ctx: &Context<'_>) -> Result<Effects, String> {
        let mut effects = Effects::default();
        let current = self.buffer(id).map(|b| (b.kind, b.name.clone()));
        let current_channel = current
            .as_ref()
            .filter(|(kind, _)| *kind == Kind::Channel)
            .map(|(_, name)| name.clone());
        let require_channel = || current_channel.clone().ok_or_else(|| tr("This command is used in a channel.").to_owned());

        match command {
            Command::Join { channels, key } => {
                self.send(Message::join(channels, key.as_deref()))?;
            }
            Command::Part { channel, reason } => {
                let channel = channel.map_or_else(require_channel, Ok)?;
                let reason = reason.or_else(|| default_part_message(ctx));
                self.send(Message::part(channel, reason.as_deref()))?;
            }
            Command::Msg { target, text } => {
                self.send(Message::privmsg(&target, &text))?;
                if let Some(target_id) = self.find(&target) {
                    let nick = self.network.nick().to_owned();
                    self.echo(target_id, LineKind::Message { nick, prefix: None, text }, ctx);
                } else {
                    let line = LineKind::Info(format!("→ {target}: {text}"));
                    self.echo(id, line, ctx);
                }
            }
            Command::Query { nick, text } => {
                let query = self.ensure(&nick);
                effects.focus = Some(query);
                if self.network.isupport().monitor.is_some() {
                    let _ = self.send(Message::new(Verb::Monitor, ["+", nick.as_str()]));
                }
                if self.network.has_cap("draft/chathistory") {
                    self.request_history(&nick);
                }
                if let Some(text) = text {
                    self.send_text(query, &text, ctx)?;
                }
            }
            Command::Notice { target, text } => {
                self.send(Message::notice(&target, &text))?;
                let line = LineKind::Info(format!("→ notice {target}: {text}"));
                self.echo(id, line, ctx);
            }
            Command::Me(text) => {
                let target = self.text_target(id)?;
                self.send(Message::privmsg(&target, ctcp("ACTION", Some(&text))))?;
                let nick = self.network.nick().to_owned();
                self.echo(id, LineKind::Action { nick, text }, ctx);
            }
            Command::Nick(nick) => {
                if self.is_online() {
                    self.send(Message::nick(nick))?;
                } else {
                    self.entry.connection.nick = nick;
                    effects.config_changed = true;
                    self.info(LineKind::Info(tr("Nickname saved for the next connection.").into()), ctx);
                }
            }
            Command::Topic(topic) => {
                let channel = require_channel()?;
                let mut params = vec![channel];
                params.extend(topic);
                self.send(Message::new(Verb::Topic, params))?;
            }
            Command::Whois(nick) => self.send(Message::new(Verb::Whois, [nick]))?,
            Command::Away(reason) => {
                let reason = reason.unwrap_or_else(|| tr("Away").to_owned());
                self.send(Message::new(Verb::Away, [reason]))?;
            }
            Command::Back => self.send(Message::new(Verb::Away, Vec::<String>::new()))?,
            Command::Kick { nick, reason } => {
                let mut params = vec![require_channel()?, nick];
                params.extend(reason);
                self.send(Message::new(Verb::Kick, params))?;
            }
            Command::Ban(target) => {
                let mask = if target.contains(['!', '@', '*']) { target } else { format!("{target}!*@*") };
                self.send(Message::new(Verb::Mode, [require_channel()?, "+b".into(), mask]))?;
            }
            Command::Unban(mask) => {
                self.send(Message::new(Verb::Mode, [require_channel()?, "-b".into(), mask]))?;
            }
            Command::Prefix { adding, mode, nicks } => {
                let channel = require_channel()?;
                let sign = if adding { '+' } else { '-' };
                let modes: String = std::iter::once(sign)
                    .chain(std::iter::repeat_n(mode, nicks.len()))
                    .collect();
                let mut params = vec![channel, modes];
                params.extend(nicks);
                self.send(Message::new(Verb::Mode, params))?;
            }
            Command::Mode(args) => {
                let mut params: Vec<String> = args.split_whitespace().map(str::to_owned).collect();
                // `/mode +m` applies to the current channel.
                if params.first().is_some_and(|p| p.starts_with(['+', '-']))
                    && let Some(channel) = current_channel.clone()
                {
                    params.insert(0, channel);
                }
                self.send(Message::new(Verb::Mode, params))?;
            }
            Command::Invite { nick, channel } => {
                let channel = channel.map_or_else(require_channel, Ok)?;
                self.send(Message::new(Verb::Invite, [nick, channel]))?;
            }
            Command::Ctcp { target, command, params } => {
                self.send(Message::privmsg(&target, ctcp(&command, params.as_deref())))?;
                self.echo(id, LineKind::Info(tr!("CTCP {command} request sent to {target}", command = command, target = target)), ctx);
            }
            Command::Raw(line) => {
                let message: Message = line.parse().map_err(|error| tr!("Invalid line: {error}", error = error))?;
                self.send(message)?;
            }
            Command::Quit(reason) => {
                let reason = reason.unwrap_or_else(|| ctx.settings.quit_message.clone());
                self.enabled = false;
                if let Some(handle) = &self.handle {
                    let _ = handle.quit(Some(reason));
                }
            }
            Command::Connect => {
                self.enabled = true;
                self.generation += 1;
            }
            // Application settings: handled before getting here.
            Command::Ignore(_) | Command::Unignore(_) => {}
            Command::Close => {
                let Some((kind, name)) = current else {
                    return Ok(effects);
                };
                match kind {
                    Kind::Server => return Err(tr("The server tab cannot be closed.").into()),
                    Kind::Channel => {
                        if self.buffer(id).is_some_and(|b| b.joined) {
                            let _ = self.send(Message::part(&name, default_part_message(ctx).as_deref()));
                        }
                    }
                    Kind::Query => {
                        if self.network.isupport().monitor.is_some() {
                            let _ = self.send(Message::new(Verb::Monitor, ["-", name.as_str()]));
                        }
                    }
                }
                self.remove_buffer(id);
                effects.closed = true;
            }
            Command::Clear => {
                if let Some(buffer) = self.buffer_mut(id) {
                    buffer.clear();
                }
            }
            Command::Help => {
                for spec in input::commands() {
                    let text = format!("{:<38} {}", spec.usage, spec.help);
                    self.push(id, Line::new(Local::now(), LineKind::Info(text)), ctx);
                }
            }
        }
        Ok(effects)
    }

    /// Message target of a tab (channel or nickname).
    fn text_target(&self, id: BufferId) -> Result<String, String> {
        match self.buffer(id) {
            Some(buffer) if buffer.kind != Kind::Server => Ok(buffer.name.clone()),
            _ => Err(tr("Cannot send text here: use /msg or /quote.").to_owned()),
        }
    }

    fn own_prefix(&self, channel: &str) -> Option<char> {
        self.network
            .channel(channel)?
            .member(&self.network.fold(self.network.nick()))?
            .highest_prefix()
    }

    /// Shows a sent message locally, unless the server will echo it itself
    /// (`echo-message`).
    fn echo(&mut self, id: BufferId, kind: LineKind, ctx: &Context<'_>) {
        let echoed_by_server = self.network.has_cap("echo-message")
            && matches!(kind, LineKind::Message { .. } | LineKind::Action { .. });
        if echoed_by_server {
            return;
        }
        let mut line = Line::new(Local::now(), kind);
        line.own = true;
        self.push(id, line, ctx);
    }
}

/// Configured part reason, if not empty.
fn default_part_message(ctx: &Context<'_>) -> Option<String> {
    Some(ctx.settings.part_message.trim().to_owned()).filter(|message| !message.is_empty())
}

#[cfg(test)]
pub(super) fn default_part_message_for_tests(ctx: &Context<'_>) -> Option<String> {
    default_part_message(ctx)
}
