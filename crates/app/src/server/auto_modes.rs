//! Auto-op and auto-voice lists (mIRC's `/aop` and `/avoice`): people given
//! `+o` or `+v` when they join a channel we manage.

use ircnext_client::proto::{Command as Verb, Message, Source};

use super::{Context, Server};
use crate::buffer::LineKind;
use crate::config::AutoMode;
use crate::i18n::tr;
use crate::tr;

impl Server {
    /// Someone joined `channel`: the modes the lists give them.
    pub(super) fn apply_auto_modes(&self, channel: &str, source: &Source) {
        if self.entry.auto_modes.is_empty() || self.network.is_me(&source.name) || !self.can_manage(channel) {
            return;
        }
        let mapping = self.network.isupport().casemapping;
        let mut modes = String::new();
        for entry in &self.entry.auto_modes {
            let for_channel = entry.channels.is_empty() || entry.channels.iter().any(|known| mapping.eq(known, channel));
            if for_channel && !modes.contains(entry.mode) && crate::ignore::matches(&entry.mask, source) {
                modes.push(entry.mode);
            }
        }
        // `+o` makes `+v` useless.
        if modes.contains('o') {
            modes = "o".into();
        }
        if !modes.is_empty() {
            let nicks = vec![source.name.as_str(); modes.len()];
            let mut params = vec![channel.to_owned(), format!("+{modes}")];
            params.extend(nicks.into_iter().map(str::to_owned));
            let _ = self.send(Message::new(Verb::Mode, params));
        }
    }

    /// `/aop` and `/avoice`: shows the list, adds or removes a mask.
    /// Returns whether the list changed.
    pub(super) fn auto_mode_command(&mut self, mode: char, mask: Option<&str>, channels: &[String], remove: bool, ctx: &Context<'_>) -> bool {
        let name = if mode == 'o' { tr("Auto-op") } else { tr("Auto-voice") };
        let Some(mask) = mask else {
            let entries: Vec<String> = self
                .entry
                .auto_modes
                .iter()
                .filter(|entry| entry.mode == mode)
                .map(|entry| if entry.channels.is_empty() { entry.mask.clone() } else { format!("{} ({})", entry.mask, entry.channels.join(", ")) })
                .collect();
            let text = if entries.is_empty() {
                tr!("{list}: empty", list = name)
            } else {
                tr!("{list}: {entries}", list = name, entries = entries.join(" · "))
            };
            self.info(LineKind::Info(text), ctx);
            return false;
        };
        let mask = crate::ignore::normalize(mask);
        let before = self.entry.auto_modes.len();
        self.entry.auto_modes.retain(|entry| !(entry.mode == mode && entry.mask == mask));
        let text = if remove {
            if self.entry.auto_modes.len() == before {
                tr!("{mask} is not on the list", mask = mask)
            } else {
                tr!("{mask} removed from the {list} list", mask = mask, list = name)
            }
        } else {
            self.entry.auto_modes.push(AutoMode { mask: mask.clone(), mode, channels: channels.to_vec() });
            if channels.is_empty() {
                tr!("{mask} added to the {list} list, on every channel you manage", mask = mask, list = name)
            } else {
                tr!("{mask} added to the {list} list for {channels}", mask = mask, list = name, channels = channels.join(", "))
            }
        };
        self.info(LineKind::Info(text), ctx);
        self.entry.auto_modes.len() != before || !remove
    }
}
