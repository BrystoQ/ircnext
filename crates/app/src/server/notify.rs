//! Notify list (mIRC's `/notify`): people whose connections and departures
//! are reported, followed with `MONITOR`.

use chrono::Local;
use ircnext_client::proto::{Command as Verb, Message};

use super::{Alert, Context, Server};
use crate::buffer::{Line, LineKind};
use crate::i18n::tr;
use crate::tr;

/// Nicknames of a list typed by hand (`alice, bob`).
pub fn parse_list(text: &str) -> Vec<String> {
    let mut nicks: Vec<String> = Vec::new();
    for nick in text.split([',', ' ']).map(str::trim) {
        let valid = !nick.is_empty() && !nick.starts_with(['#', '&', ':', '-']) && !nick.contains(['!', '@', '*', '?']);
        if valid && !nicks.iter().any(|known| known.eq_ignore_ascii_case(nick)) {
            nicks.push(nick.to_owned());
        }
    }
    nicks
}

impl Server {
    /// Whether `nick` is on the notify list.
    pub fn is_notified(&self, nick: &str) -> bool {
        let key = self.network.fold(nick);
        self.entry.notify.iter().any(|known| self.network.fold(known) == key)
    }

    /// New notify list (network form), followed at once if connected.
    pub fn replace_notify(&mut self, notify: Vec<String>) {
        if self.network.isupport().monitor.is_some() && self.is_online() {
            let added: Vec<&str> = notify.iter().filter(|nick| !self.is_notified(nick)).map(String::as_str).collect();
            let removed: Vec<&str> = self
                .entry
                .notify
                .iter()
                .filter(|old| !notify.iter().any(|nick| self.network.fold(nick) == self.network.fold(old)))
                .filter(|old| self.find(old).is_none())
                .map(String::as_str)
                .collect();
            if !added.is_empty() {
                let _ = self.send(Message::new(Verb::Monitor, ["+".to_owned(), added.join(",")]));
            }
            if !removed.is_empty() {
                let _ = self.send(Message::new(Verb::Monitor, ["-".to_owned(), removed.join(",")]));
            }
        }
        self.entry.notify = notify;
    }

    /// `RPL_MONONLINE` / `RPL_MONOFFLINE`: arrivals and departures of the
    /// notify list, in the server tab.
    pub(super) fn on_monitor(&mut self, online: bool, message: &Message, ctx: &Context<'_>) {
        let targets = message.param(1).unwrap_or_default().to_owned();
        let id = self.server_buffer();
        for target in targets.split(',').filter(|target| !target.is_empty()) {
            let (nick, address) = target.split_once('!').map_or((target, None), |(nick, address)| (nick, Some(address)));
            if !self.is_notified(nick) {
                continue;
            }
            let text = match (online, address) {
                (true, Some(address)) => tr!("{nick} ({address}) is on IRC", nick = nick, address = address),
                (true, None) => tr!("{nick} is on IRC", nick = nick),
                (false, _) => tr!("{nick} has left IRC", nick = nick),
            };
            self.push(id, Line::new(Local::now(), LineKind::Info(text.clone())), ctx);
            if online {
                self.alerts.push(Alert { buffer: id, title: self.entry.name.clone(), body: text, private: true });
            }
            let event = if online { "NOTIFY" } else { "UNOTIFY" };
            let address = address.unwrap_or_default();
            self.script_event(id, event, None, "", false, &[("nick", nick), ("address", address)]);
        }
    }

    /// `/notify`: adds `nick` (or removes it), or shows the list. Returns
    /// whether the list changed.
    pub(super) fn notify_command(&mut self, nick: Option<&str>, remove: bool, ctx: &Context<'_>) -> bool {
        let monitor = self.network.isupport().monitor.is_some();
        let Some(nick) = nick else {
            let text = if self.entry.notify.is_empty() {
                tr("The notify list is empty · /notify <nick> to add someone").to_owned()
            } else {
                let states: Vec<String> = self
                    .entry
                    .notify
                    .iter()
                    .map(|nick| match self.network.is_online(nick) {
                        Some(true) => tr!("{nick} (online)", nick = nick),
                        Some(false) => tr!("{nick} (offline)", nick = nick),
                        None => nick.clone(),
                    })
                    .collect();
                tr!("Notify list: {nicks}", nicks = states.join(", "))
            };
            self.info(LineKind::Info(text), ctx);
            return false;
        };
        if remove {
            let key = self.network.fold(nick);
            let before = self.entry.notify.len();
            let network = &self.network;
            self.entry.notify.retain(|known| network.fold(known) != key);
            let removed = self.entry.notify.len() != before;
            let query_open = self.find(nick).is_some();
            if removed && monitor && !query_open {
                let _ = self.send(Message::new(Verb::Monitor, ["-", nick]));
            }
            let text = if removed {
                tr!("{nick} removed from the notify list", nick = nick)
            } else {
                tr!("{nick} is not on the notify list", nick = nick)
            };
            self.info(LineKind::Info(text), ctx);
            removed
        } else if self.is_notified(nick) || parse_list(nick).is_empty() {
            let text = if self.is_notified(nick) {
                tr!("{nick} is already on the notify list", nick = nick)
            } else {
                tr!("Not a nickname: {nick}", nick = nick)
            };
            self.info(LineKind::Error(text), ctx);
            false
        } else {
            self.entry.notify.push(nick.to_owned());
            if monitor {
                let _ = self.send(Message::new(Verb::Monitor, ["+", nick]));
            }
            let text = if monitor || !self.is_online() {
                tr!("{nick} added to the notify list", nick = nick)
            } else {
                tr!("{nick} added to the notify list, but this server cannot report connections (no MONITOR)", nick = nick)
            };
            self.info(LineKind::Info(text), ctx);
            true
        }
    }
}
