//! FiSH encryption on a network: keys by conversation, `/setkey`,
//! `/delkey`, `/keyx` and the DH1080 key exchange.

use ircnext_client::Secret;
use ircnext_client::fish::{Dh1080, Key, KeyExchange, Mode};
use ircnext_client::proto::Message;

use super::{Context, Server};
use crate::buffer::LineKind;
use crate::i18n::tr;
use crate::tr;

/// Longest plain text encrypted into one message: once encrypted (padding,
/// IV, base64) it stays under the size of a `PRIVMSG`.
pub(super) const MAX_PLAIN_BYTES: usize = 256;

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Cbc => "CBC",
        Mode::Ecb => "ECB",
    }
}

impl Server {
    /// FiSH key of a conversation (channel or nickname), if any.
    pub fn fish_key(&self, target: &str) -> Option<Key> {
        self.entry.fish_keys.get(&target.to_lowercase()).and_then(|key| Key::parse(key.expose()))
    }

    /// Text to send to `target`: encrypted if a key is set.
    pub(super) fn seal(&self, target: &str, text: &str) -> String {
        self.fish_key(target).map_or_else(|| text.to_owned(), |key| key.encrypt(text))
    }

    /// Received text of the conversation `target`: decrypted if it is a
    /// FiSH message and the key fits. The flag tells whether it was.
    pub(super) fn unseal(&self, target: &str, text: &str) -> (String, bool) {
        let decrypted = (text.starts_with("+OK ") || text.starts_with("mcps "))
            .then(|| self.fish_key(target))
            .flatten()
            .and_then(|key| key.decrypt(text));
        match decrypted {
            Some(plain) => (plain, true),
            None => (text.to_owned(), false),
        }
    }

    /// Topic as shown: decrypted with the channel key if needed.
    pub fn display_topic(&self, channel: &str, topic: &str) -> String {
        self.unseal(channel, topic).0
    }

    /// `/setkey [target] <key>`.
    pub(super) fn set_key(&mut self, target: &str, key: &str, ctx: &Context<'_>) -> Result<(), String> {
        let key = Key::parse(key).ok_or_else(|| tr("The key is too short (4 characters minimum).").to_owned())?;
        self.entry.fish_keys.insert(target.to_lowercase(), Secret::new(key.to_text()));
        self.notice_in(target, tr!("FiSH key set for {target} ({mode}): messages are now encrypted.", target = target, mode = mode_name(key.mode)), ctx);
        Ok(())
    }

    /// `/delkey [target]`.
    pub(super) fn delete_key(&mut self, target: &str, ctx: &Context<'_>) -> Result<(), String> {
        if self.entry.fish_keys.remove(&target.to_lowercase()).is_none() {
            return Err(tr!("No FiSH key for {target}.", target = target));
        }
        self.notice_in(target, tr!("FiSH key removed for {target}: messages are sent in clear.", target = target), ctx);
        Ok(())
    }

    /// `/keyx [nick]`: starts a DH1080 key exchange (CBC).
    pub(super) fn start_key_exchange(&mut self, nick: &str, ctx: &Context<'_>) -> Result<(), String> {
        if self.network.is_channel(nick) {
            return Err(tr("A key exchange is done with a person, not a channel. Use /setkey for a channel.").to_owned());
        }
        let exchange = KeyExchange::new();
        let init = Dh1080::Init { public_key: exchange.public_key(), mode: Mode::Cbc };
        self.send(Message::notice(nick, init.to_text()))?;
        self.key_exchanges.insert(nick.to_lowercase(), exchange);
        self.notice_in(nick, tr!("Key exchange started with {nick}…", nick = nick), ctx);
        Ok(())
    }

    /// A `DH1080_INIT` or `DH1080_FINISH` notice from `nick`. Returns
    /// whether a key was stored (the configuration changed).
    pub(super) fn on_dh1080(&mut self, nick: &str, dh: Dh1080, ctx: &Context<'_>) -> bool {
        let (key, answer) = match dh {
            Dh1080::Init { public_key, mode } => {
                let exchange = KeyExchange::new();
                let answer = Dh1080::Finish { public_key: exchange.public_key(), mode };
                (exchange.shared_key(&public_key, mode), Some(answer))
            }
            Dh1080::Finish { public_key, mode } => {
                let Some(exchange) = self.key_exchanges.remove(&nick.to_lowercase()) else {
                    return false;
                };
                (exchange.shared_key(&public_key, mode), None)
            }
        };
        let Some(key) = key else {
            self.notice_in(nick, tr!("Invalid key exchange from {nick}: ignored.", nick = nick), ctx);
            return false;
        };
        if let Some(answer) = answer {
            let _ = self.send(Message::notice(nick, answer.to_text()));
        }
        self.entry.fish_keys.insert(nick.to_lowercase(), Secret::new(key.to_text()));
        self.notice_in(nick, tr!("Key exchanged with {nick} ({mode}): messages are now encrypted.", nick = nick, mode = mode_name(key.mode)), ctx);
        true
    }

    /// Information line in the conversation `target` (opened if needed).
    fn notice_in(&mut self, target: &str, text: String, ctx: &Context<'_>) {
        let id = self.ensure(target);
        self.push(id, crate::buffer::Line::new(chrono::Local::now(), LineKind::Info(text)), ctx);
    }
}
