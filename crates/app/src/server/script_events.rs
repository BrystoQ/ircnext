//! Events for the scripts (`on TEXT`, `on JOIN`…), collected while the
//! network handles messages, then run by the application.

use std::collections::BTreeMap;

use ircnext_client::proto::isupport::parse_channel_modes;
use ircnext_client::proto::{Command, Message};
use ircnext_script::Event;

use super::{BufferId, Server};

/// An event for the scripts, with the tab it concerns.
#[derive(Debug, Clone)]
pub struct ScriptEvent {
    pub buffer: BufferId,
    pub event: Event,
}

impl Server {
    /// Events collected since the last call.
    pub fn take_script_events(&mut self) -> Vec<ScriptEvent> {
        std::mem::take(&mut self.script_events)
    }

    /// Whether we are an operator of `channel`.
    fn is_operator(&self, channel: &str) -> bool {
        let symbol = self.network.isupport().prefix_symbol('o');
        self.network
            .channel(channel)
            .and_then(|channel| channel.member(&self.network.fold(self.network.nick())))
            .is_some_and(|member| symbol.is_some_and(|symbol| member.prefixes.contains(&symbol)))
    }

    /// Records an event in `buffer`; `values` are its identifiers.
    pub(super) fn script_event(
        &mut self,
        buffer: BufferId,
        name: &str,
        channel: Option<&str>,
        text: &str,
        own: bool,
        values: &[(&str, &str)],
    ) {
        let mut identifiers: BTreeMap<String, String> =
            values.iter().map(|(key, value)| ((*key).to_owned(), (*value).to_owned())).collect();
        if let Some(channel) = channel {
            identifiers.entry("chan".into()).or_insert_with(|| channel.to_owned());
            identifiers.entry("target".into()).or_insert_with(|| channel.to_owned());
        }
        let event = Event {
            name: name.to_owned(),
            channel: channel.map(str::to_owned),
            text: text.to_owned(),
            own,
            operator: channel.is_some_and(|channel| self.is_operator(channel)),
            values: identifiers,
        };
        self.script_events.push(ScriptEvent { buffer, event });
    }

    /// Events of a message other than text (`JOIN`, `PART`, `KICK`, `QUIT`,
    /// `NICK`, `TOPIC`, `INVITE`, modes), after the state was updated.
    /// `shared` are the channels a quitting or renamed user was on.
    pub(super) fn message_script_events(&mut self, message: &Message, source: &str, shared: &[String]) {
        let address = message
            .source
            .as_ref()
            .and_then(ircnext_client::proto::Source::mask)
            .unwrap_or_default();
        let own = self.network.is_me(source);
        let base = [("nick", source), ("address", address.as_str())];
        match &message.command {
            Command::Join | Command::Part => {
                let Some(channel) = message.param(0) else { return };
                let name = if message.command == Command::Join { "JOIN" } else { "PART" };
                let id = self.ensure(channel);
                self.script_event(id, name, Some(channel), message.param(1).unwrap_or_default(), own, &base);
            }
            Command::Kick => {
                let (Some(channel), Some(kicked)) = (message.param(0), message.param(1)) else { return };
                let id = self.ensure(channel);
                let values = [base[0], base[1], ("knick", kicked)];
                self.script_event(id, "KICK", Some(channel), message.param(2).unwrap_or_default(), own, &values);
            }
            Command::Topic => {
                let Some(channel) = message.param(0) else { return };
                let topic = self.display_topic(channel, message.param(1).unwrap_or_default());
                let id = self.ensure(channel);
                self.script_event(id, "TOPIC", Some(channel), &topic, own, &base);
            }
            Command::Invite => {
                let Some(channel) = message.param(1) else { return };
                let id = self.server_buffer();
                self.script_event(id, "INVITE", Some(channel), "", own, &base);
            }
            Command::Quit | Command::Nick => {
                let (name, text) = if message.command == Command::Quit {
                    ("QUIT", message.param(0).unwrap_or_default())
                } else {
                    ("NICK", "")
                };
                let new = message.param(0).unwrap_or_default();
                let values = [base[0], base[1], ("newnick", new)];
                let mut buffers: Vec<(BufferId, Option<String>)> =
                    shared.iter().filter_map(|channel| self.find(channel).map(|id| (id, Some(channel.clone())))).collect();
                if buffers.is_empty() {
                    buffers.push((self.server_buffer(), None));
                }
                for (id, channel) in buffers {
                    self.script_event(id, name, channel.as_deref(), text, own, &values);
                }
            }
            Command::Mode => {
                let Some(channel) = message.param(0).filter(|target| self.network.is_channel(target)) else { return };
                let id = self.ensure(channel);
                let modes = message.param(1).unwrap_or_default();
                let args = message.params.get(2..).unwrap_or_default();
                for change in parse_channel_modes(self.network.isupport(), modes, args) {
                    let (event, key) = match (change.mode, change.adding) {
                        ('o', true) => ("OP", "opnick"),
                        ('o', false) => ("DEOP", "opnick"),
                        ('v', true) => ("VOICE", "vnick"),
                        ('v', false) => ("DEVOICE", "vnick"),
                        _ => continue,
                    };
                    let target = change.arg.unwrap_or_default();
                    let values = [base[0], base[1], (key, target.as_str())];
                    self.script_event(id, event, Some(channel), "", own, &values);
                }
            }
            _ => {}
        }
    }
}
