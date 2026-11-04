//! Channel central (as in mIRC): topic, modes and mask lists of a channel,
//! asked when its window opens.

use ircnext_client::proto::numeric::{
    RPL_BANLIST, RPL_CHANNELMODEIS, RPL_ENDOFBANLIST, RPL_ENDOFEXCEPTLIST, RPL_ENDOFINVITELIST, RPL_EXCEPTLIST,
    RPL_INVITELIST,
};
use ircnext_client::proto::{Command, Message};

use super::Server;

/// Mask lists shown, with the replies that fill and end them.
pub const MASK_LISTS: [(char, u16, u16); 3] = [
    ('b', RPL_BANLIST, RPL_ENDOFBANLIST),
    ('e', RPL_EXCEPTLIST, RPL_ENDOFEXCEPTLIST),
    ('I', RPL_INVITELIST, RPL_ENDOFINVITELIST),
];

impl Server {
    /// Whether we may change the channel's settings (operator or half-op).
    pub fn can_manage(&self, channel: &str) -> bool {
        let isupport = self.network.isupport();
        let symbols: Vec<char> = ['q', 'a', 'o', 'h'].into_iter().filter_map(|mode| isupport.prefix_symbol(mode)).collect();
        self.network
            .channel(channel)
            .and_then(|channel| channel.member(&self.network.fold(self.network.nick())))
            .is_some_and(|member| member.prefixes.iter().any(|symbol| symbols.contains(symbol)))
    }

    /// Mask lists the network has (`CHANMODES`).
    pub fn mask_modes(&self) -> Vec<char> {
        let lists = &self.network.isupport().chanmodes[0];
        MASK_LISTS.iter().map(|(mode, ..)| *mode).filter(|mode| lists.contains(*mode)).collect()
    }

    /// Asks the modes and mask lists of `channel`; the replies fill the
    /// window instead of the server tab. Exceptions and invitations are
    /// usually reserved to operators: asked only then.
    pub fn request_channel_details(&mut self, channel: &str) -> Result<(), String> {
        self.send(Message::new(Command::Mode, [channel]))?;
        let key = self.network.fold(channel);
        self.quiet_replies.push((key.clone(), RPL_CHANNELMODEIS));
        let manage = self.can_manage(channel);
        for mode in self.mask_modes() {
            if mode != 'b' && !manage {
                continue;
            }
            let end = MASK_LISTS.iter().find(|(list, ..)| *list == mode).map_or(0, |(_, _, end)| *end);
            self.network.request_mask_list(channel, mode);
            self.send(Message::new(Command::Mode, [channel.to_owned(), mode.to_string()]))?;
            self.quiet_replies.push((key.clone(), end));
        }
        Ok(())
    }

    /// Sends a mode change of `channel` (`+m`, `-k *`, `+b mask`).
    pub fn change_mode(&self, channel: &str, modes: &str, args: &[&str]) -> Result<(), String> {
        let mut params = vec![channel.to_owned(), modes.to_owned()];
        params.extend(args.iter().map(|arg| (*arg).to_owned()));
        self.send(Message::new(Command::Mode, params))
    }

    pub fn set_topic(&self, channel: &str, topic: &str) -> Result<(), String> {
        self.send(Message::new(Command::Topic, [channel, topic]))
    }

    /// Whether `message` answers a request of the window, and stays out of
    /// the server tab.
    pub(super) fn is_quiet_reply(&mut self, message: &Message) -> bool {
        let Command::Numeric(code) = message.command else {
            return false;
        };
        let Some(channel) = message.param(1) else {
            return false;
        };
        let key = self.network.fold(channel);
        // List entries: quiet while their list is awaited.
        if let Some((_, _, end)) = MASK_LISTS.iter().find(|(_, item, _)| *item == code) {
            return self.quiet_replies.iter().any(|(channel, code)| *channel == key && code == end);
        }
        match self.quiet_replies.iter().position(|(channel, awaited)| *channel == key && *awaited == code) {
            Some(index) => {
                self.quiet_replies.remove(index);
                true
            }
            None => false,
        }
    }
}
