//! State of an IRC network rebuilt from the received messages: channels,
//! members, topics, server parameters. No I/O: messages are applied.

use std::collections::{BTreeMap, HashMap, HashSet};

use ircnext_proto::isupport::{ModeKind, parse_channel_modes};
use ircnext_proto::numeric::{
    RPL_BANLIST, RPL_CHANNELMODEIS, RPL_ENDOFBANLIST, RPL_ENDOFEXCEPTLIST, RPL_ENDOFINVITELIST,
    RPL_EXCEPTLIST, RPL_INVITELIST, RPL_ENDOFNAMES, RPL_ISUPPORT, RPL_MONOFFLINE, RPL_MONONLINE, RPL_NAMREPLY, RPL_NOTOPIC,
    RPL_NOWAWAY, RPL_TOPIC, RPL_TOPICWHOTIME, RPL_UNAWAY, RPL_WELCOME, RPL_WHOSPCRPL,
};
use ircnext_proto::ircv3::Multiline;
use ircnext_proto::{Command, ISupport, Message, Tags};

/// Token identifying our WHOX requests in the 354 replies.
const WHOX_TOKEN: &str = "42";

/// WHOX request fetching, for each member of `channel`: channel, nick,
/// flags (away) and account.
pub fn whox_request(channel: &str) -> Message {
    Message::new(Command::Who, [channel.to_owned(), format!("%tcnfa,{WHOX_TOKEN}")])
}

/// Maximum number of batches open at the same time.
const MAX_OPEN_BATCHES: usize = 64;
/// Maximum number of lines kept for a `draft/multiline` batch.
const MAX_MULTILINE_LINES: usize = 256;

/// Channel member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub nick: String,
    /// Prefix symbols held, from highest to lowest (`@+`).
    pub prefixes: Vec<char>,
    pub away: bool,
    /// Services account, if known (`extended-join`, `account-notify`).
    pub account: Option<String>,
}

impl Member {
    fn new(nick: impl Into<String>) -> Self {
        Self {
            nick: nick.into(),
            prefixes: Vec::new(),
            away: false,
            account: None,
        }
    }

    /// Highest prefix, for display.
    pub fn highest_prefix(&self) -> Option<char> {
        self.prefixes.first().copied()
    }
}

/// Joined channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub name: String,
    pub topic: Option<String>,
    pub topic_set_by: Option<String>,
    members: HashMap<String, Member>,
    /// A `NAMES` reply is being received.
    names_pending: bool,
    /// Modes other than lists and member prefixes (`n`, `t`, `k` with its
    /// key, `l` with its limit), as far as known.
    pub modes: BTreeMap<char, Option<String>>,
    /// Mask lists (`b`, `e`, `I`), once asked with [`Network::request_mask_list`].
    pub mask_lists: BTreeMap<char, MaskList>,
}

/// Entries of a mask list (bans, exceptions, invitations).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MaskList {
    pub entries: Vec<MaskEntry>,
    /// Replies still arriving.
    pub loading: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaskEntry {
    pub mask: String,
    pub set_by: Option<String>,
    /// Unix time.
    pub set_at: Option<i64>,
}

impl Channel {
    fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            topic: None,
            topic_set_by: None,
            members: HashMap::new(),
            names_pending: false,
            modes: BTreeMap::new(),
            mask_lists: BTreeMap::new(),
        }
    }

    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    pub fn member(&self, folded_nick: &str) -> Option<&Member> {
        self.members.get(folded_nick)
    }

    /// Members sorted by prefix rank then by nickname.
    pub fn sorted_members(&self, isupport: &ISupport) -> Vec<&Member> {
        let mut members: Vec<&Member> = self.members.values().collect();
        members.sort_by_cached_key(|member| {
            let rank = member
                .highest_prefix()
                .and_then(|symbol| isupport.prefix_rank(symbol))
                .unwrap_or(usize::MAX);
            (rank, isupport.fold(&member.nick))
        });
        members
    }
}

/// IRCv3 batch (`BATCH`) in progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    /// Batch type (`chathistory`, `netsplit`...).
    pub kind: String,
    pub params: Vec<String>,
    /// Tags of the `BATCH +` line (`time`, `msgid`, outer `batch`...).
    pub tags: Tags,
    /// Lines of a `draft/multiline` batch, until it closes.
    pub lines: Vec<Message>,
}

/// Full state of a connection.
#[derive(Debug, Clone, Default)]
pub struct Network {
    nick: String,
    isupport: ISupport,
    caps: HashSet<String>,
    channels: BTreeMap<String, Channel>,
    batches: HashMap<String, Batch>,
    /// Nicknames followed by `MONITOR` and currently online.
    online: HashSet<String>,
    away: bool,
    multiline: Option<Multiline>,
}

impl Network {
    pub fn new(nick: impl Into<String>) -> Self {
        Self {
            nick: nick.into(),
            ..Self::default()
        }
    }

    /// Resets the state after registering on a new connection.
    pub fn reset(&mut self, nick: &str, caps: &[String]) {
        *self = Self {
            nick: nick.to_owned(),
            caps: caps.iter().cloned().collect(),
            ..Self::default()
        };
    }

    pub fn nick(&self) -> &str {
        &self.nick
    }

    pub fn isupport(&self) -> &ISupport {
        &self.isupport
    }

    pub fn has_cap(&self, cap: &str) -> bool {
        self.caps.contains(cap)
    }

    /// Limits of `draft/multiline`, if the server accepts it.
    pub fn multiline(&self) -> Option<Multiline> {
        self.multiline
    }

    pub fn set_multiline(&mut self, multiline: Option<Multiline>) {
        self.multiline = multiline;
    }

    /// Keeps a line of an open `draft/multiline` batch for later; returns
    /// `true` if the line was taken (it must not be shown on its own).
    pub fn collect_multiline(&mut self, message: &Message) -> bool {
        if !matches!(message.command, Command::Privmsg | Command::Notice) {
            return false;
        }
        let Some(batch) = message.batch().and_then(|id| self.batches.get_mut(id)) else {
            return false;
        };
        if batch.kind != Multiline::CAP {
            return false;
        }
        if batch.lines.len() < MAX_MULTILINE_LINES {
            batch.lines.push(message.clone());
        }
        true
    }

    /// For the `BATCH -` line closing a `draft/multiline` batch: the whole
    /// message, with the tags of the batch and the lines joined.
    pub fn finish_multiline(&mut self, message: &Message) -> Option<Message> {
        if message.command != Command::Batch {
            return None;
        }
        let id = message.param(0)?.strip_prefix('-')?;
        if self.batches.get(id)?.kind != Multiline::CAP {
            return None;
        }
        let batch = self.batches.remove(id)?;
        let first = batch.lines.first()?;
        let mut combined = first.clone();
        combined.tags = batch.tags.clone();
        combined.params = vec![
            batch.params.first().cloned().unwrap_or_else(|| first.param(0).unwrap_or_default().to_owned()),
            Multiline::join(&batch.lines),
        ];
        Some(combined)
    }

    pub fn is_away(&self) -> bool {
        self.away
    }

    pub fn fold(&self, name: &str) -> String {
        self.isupport.fold(name)
    }

    pub fn is_me(&self, nick: &str) -> bool {
        self.isupport.casemapping.eq(nick, &self.nick)
    }

    pub fn is_channel(&self, target: &str) -> bool {
        self.isupport.is_channel(target)
    }

    /// Prepares the mask list `mode` of `channel` for the replies to
    /// `MODE <channel> <mode>`, which the caller sends.
    pub fn request_mask_list(&mut self, channel: &str, mode: char) {
        if let Some(channel) = self.channel_mut(channel) {
            channel.mask_lists.insert(mode, MaskList { entries: Vec::new(), loading: true });
        }
    }

    pub fn channel(&self, name: &str) -> Option<&Channel> {
        self.channels.get(&self.fold(name))
    }

    pub fn channels(&self) -> impl Iterator<Item = &Channel> {
        self.channels.values()
    }

    /// Channels where `nick` is present.
    pub fn channels_with(&self, nick: &str) -> Vec<String> {
        let key = self.fold(nick);
        self.channels
            .values()
            .filter(|channel| channel.members.contains_key(&key))
            .map(|channel| channel.name.clone())
            .collect()
    }

    /// Batch the message belongs to, if any.
    pub fn batch_of(&self, message: &Message) -> Option<&Batch> {
        message.batch().and_then(|id| self.batches.get(id))
    }

    /// Whether `nick` is online according to `MONITOR` (`None` if unknown).
    pub fn is_online(&self, nick: &str) -> Option<bool> {
        self.isupport
            .monitor
            .map(|_| self.online.contains(&self.fold(nick)))
    }

    /// Updates the state from a received message.
    pub fn apply(&mut self, message: &Message) {
        let source = message.source_nick().unwrap_or_default();
        match &message.command {
            Command::Numeric(RPL_WELCOME) => {
                if let Some(nick) = message.param(0) {
                    nick.clone_into(&mut self.nick);
                }
            }
            Command::Numeric(RPL_ISUPPORT) => self.apply_isupport(message),
            Command::Nick => {
                if let Some(new) = message.param(0) {
                    self.rename_member(source, new);
                }
            }
            Command::Join => self.on_join(message, source),
            Command::Part => {
                if let Some(channel) = message.param(0) {
                    self.remove_member(channel, source);
                }
            }
            Command::Kick => {
                if let (Some(channel), Some(target)) = (message.param(0), message.param(1)) {
                    self.remove_member(channel, target);
                }
            }
            Command::Quit => {
                let key = self.fold(source);
                for channel in self.channels.values_mut() {
                    channel.members.remove(&key);
                }
            }
            Command::Numeric(RPL_NAMREPLY) => self.on_names(message),
            Command::Numeric(RPL_CHANNELMODEIS) => {
                if let Some(name) = message.param(1) {
                    let isupport = self.isupport.clone();
                    let changes = parse_channel_modes(&isupport, message.param(2).unwrap_or_default(), message.params.get(3..).unwrap_or_default());
                    if let Some(channel) = self.channel_mut(name) {
                        channel.modes.clear();
                        for change in changes {
                            channel.modes.insert(change.mode, change.arg);
                        }
                    }
                }
            }
            Command::Numeric(code @ (RPL_BANLIST | RPL_EXCEPTLIST | RPL_INVITELIST)) => {
                let mode = match *code {
                    RPL_BANLIST => 'b',
                    RPL_EXCEPTLIST => 'e',
                    _ => 'I',
                };
                // `RPL_INVITELIST` and `RPL_EXCEPTLIST` may carry the mode
                // letter on some servers: the mask is the first parameter
                // after the channel either way.
                if let (Some(name), Some(mask)) = (message.param(1), message.param(2))
                    && let Some(list) = self.channel_mut(name).and_then(|channel| channel.mask_lists.get_mut(&mode))
                    && list.loading
                {
                    list.entries.push(MaskEntry {
                        mask: mask.to_owned(),
                        set_by: message.param(3).map(str::to_owned),
                        set_at: message.param(4).and_then(|time| time.parse().ok()),
                    });
                }
            }
            Command::Numeric(code @ (RPL_ENDOFBANLIST | RPL_ENDOFEXCEPTLIST | RPL_ENDOFINVITELIST)) => {
                let mode = match *code {
                    RPL_ENDOFBANLIST => 'b',
                    RPL_ENDOFEXCEPTLIST => 'e',
                    _ => 'I',
                };
                if let Some(list) = message.param(1).and_then(|name| self.channel_mut(name)).and_then(|channel| channel.mask_lists.get_mut(&mode)) {
                    list.loading = false;
                }
            }
            Command::Numeric(RPL_ENDOFNAMES) => {
                if let Some(channel) = message.param(1).and_then(|c| self.channel_mut(c)) {
                    channel.names_pending = false;
                }
            }
            Command::Numeric(RPL_TOPIC) => {
                if let (Some(name), Some(topic)) = (message.param(1), message.param(2)) {
                    self.set_topic(name, Some(topic), None);
                }
            }
            Command::Numeric(RPL_NOTOPIC) => {
                if let Some(name) = message.param(1) {
                    self.set_topic(name, None, None);
                }
            }
            Command::Numeric(RPL_TOPICWHOTIME) => {
                if let (Some(name), Some(setter)) = (message.param(1), message.param(2))
                    && let Some(channel) = self.channel_mut(name)
                {
                    channel.topic_set_by = Some(setter.to_owned());
                }
            }
            Command::Topic => {
                if let Some(name) = message.param(0) {
                    let topic = message.param(1).filter(|topic| !topic.is_empty());
                    self.set_topic(name, topic, Some(source));
                }
            }
            Command::Mode => self.on_mode(message),
            Command::Away => {
                let away = message.param(0).is_some();
                self.update_members(source, |member| member.away = away);
            }
            Command::Account => {
                let account = message.param(0).filter(|&a| a != "*").map(str::to_owned);
                self.update_members(source, |member| member.account.clone_from(&account));
            }
            Command::Numeric(RPL_UNAWAY) => self.away = false,
            Command::Numeric(RPL_NOWAWAY) => self.away = true,
            Command::Numeric(code @ (RPL_MONONLINE | RPL_MONOFFLINE)) => {
                let targets = message.param(1).unwrap_or_default();
                for target in targets.split(',').filter(|t| !t.is_empty()) {
                    let nick = target.split('!').next().unwrap_or(target);
                    let key = self.fold(nick);
                    if *code == RPL_MONONLINE {
                        self.online.insert(key);
                    } else {
                        self.online.remove(&key);
                    }
                }
            }
            Command::Numeric(RPL_WHOSPCRPL) if message.param(1) == Some(WHOX_TOKEN) => {
                self.on_whox(message);
            }
            Command::Batch => self.on_batch(message),
            Command::Rename => {
                if let (Some(old), Some(new)) = (message.param(0), message.param(1)) {
                    let old_key = self.fold(old);
                    if let Some(mut channel) = self.channels.remove(&old_key) {
                        new.clone_into(&mut channel.name);
                        self.channels.insert(self.fold(new), channel);
                    }
                }
            }
            _ => {}
        }
    }

    fn apply_isupport(&mut self, message: &Message) {
        // Parameters: <nick> <token>... :are supported by this server
        let tokens = message.params.get(1..message.params.len().saturating_sub(1));
        let previous = self.isupport.casemapping;
        self.isupport
            .apply(tokens.unwrap_or_default().iter().map(String::as_str));
        if self.isupport.casemapping != previous {
            self.rekey();
        }
    }

    /// Recomputes the keys after a `CASEMAPPING` change.
    fn rekey(&mut self) {
        let mapping = self.isupport.casemapping;
        let channels = std::mem::take(&mut self.channels);
        for (_, mut channel) in channels {
            channel.members = channel
                .members
                .into_values()
                .map(|member| (mapping.fold(&member.nick), member))
                .collect();
            self.channels.insert(mapping.fold(&channel.name), channel);
        }
    }

    fn on_join(&mut self, message: &Message, source: &str) {
        let Some(name) = message.param(0) else {
            return;
        };
        if self.is_me(source) {
            self.channels
                .insert(self.fold(name), Channel::new(name));
        }
        let key = self.fold(source);
        // `extended-join`: JOIN #channel <account|*> :<real name>
        let account = message
            .param(1)
            .filter(|&account| account != "*" && message.params.len() >= 3)
            .map(str::to_owned);
        if let Some(channel) = self.channel_mut(name) {
            let mut member = Member::new(source);
            member.account = account;
            channel.members.insert(key, member);
        }
    }

    fn on_names(&mut self, message: &Message) {
        // Parameters: <nick> <symbol> <channel> :<members>
        let (Some(name), Some(names)) = (message.param(2), message.param(3)) else {
            return;
        };
        let isupport = self.isupport.clone();
        let Some(channel) = self.channel_mut(name) else {
            return;
        };
        if !channel.names_pending {
            channel.members.clear();
            channel.names_pending = true;
        }
        for entry in names.split_whitespace() {
            let nick_start = entry
                .find(|c| !isupport.is_prefix_symbol(c))
                .unwrap_or(entry.len());
            let (prefixes, rest) = entry.split_at(nick_start);
            // `userhost-in-names`: nick!user@host
            let nick = rest.split('!').next().unwrap_or(rest);
            if nick.is_empty() {
                continue;
            }
            let mut member = Member::new(nick);
            member.prefixes = prefixes.chars().collect();
            member
                .prefixes
                .sort_by_key(|&symbol| isupport.prefix_rank(symbol));
            channel.members.insert(isupport.fold(nick), member);
        }
    }

    fn on_mode(&mut self, message: &Message) {
        let (Some(target), Some(modes)) = (message.param(0), message.param(1)) else {
            return;
        };
        if !self.is_channel(target) {
            return;
        }
        let isupport = self.isupport.clone();
        let changes = parse_channel_modes(&isupport, modes, &message.params[2..]);
        let Some(channel) = self.channel_mut(target) else {
            return;
        };
        for change in changes {
            match isupport.mode_kind(change.mode) {
                ModeKind::Prefix => {}
                ModeKind::List => {
                    if let Some(list) = channel.mask_lists.get_mut(&change.mode)
                        && let Some(mask) = change.arg
                    {
                        list.entries.retain(|entry| entry.mask != mask);
                        if change.adding {
                            let set_by = message.source_nick().map(str::to_owned);
                            list.entries.push(MaskEntry { mask, set_by, set_at: None });
                        }
                    }
                    continue;
                }
                _ => {
                    if change.adding {
                        channel.modes.insert(change.mode, change.arg);
                    } else {
                        channel.modes.remove(&change.mode);
                    }
                    continue;
                }
            }
            let (Some(symbol), Some(nick)) = (isupport.prefix_symbol(change.mode), change.arg)
            else {
                continue;
            };
            let Some(member) = channel.members.get_mut(&isupport.fold(&nick)) else {
                continue;
            };
            member.prefixes.retain(|&s| s != symbol);
            if change.adding {
                member.prefixes.push(symbol);
                member.prefixes.sort_by_key(|&s| isupport.prefix_rank(s));
            }
        }
    }

    fn on_whox(&mut self, message: &Message) {
        // Parameters: <me> <token> <channel> <nick> <flags> <account|0>
        let (Some(channel), Some(nick), Some(flags)) =
            (message.param(2), message.param(3), message.param(4))
        else {
            return;
        };
        let account = message.param(5).filter(|&a| a != "0").map(str::to_owned);
        let key = self.fold(nick);
        if let Some(member) = self
            .channel_mut(channel)
            .and_then(|channel| channel.members.get_mut(&key))
        {
            member.away = flags.starts_with('G');
            member.account = account;
        }
    }

    fn on_batch(&mut self, message: &Message) {
        let Some(reference) = message.param(0) else {
            return;
        };
        if let Some(id) = reference.strip_prefix('+') {
            // A server opening batches without ever closing them must not
            // make memory grow forever.
            if self.batches.len() >= MAX_OPEN_BATCHES && !self.batches.contains_key(id) {
                return;
            }
            let batch = Batch {
                kind: message.param(1).unwrap_or_default().to_owned(),
                params: message.params.iter().skip(2).cloned().collect(),
                tags: message.tags.clone(),
                lines: Vec::new(),
            };
            self.batches.insert(id.to_owned(), batch);
        } else if let Some(id) = reference.strip_prefix('-') {
            self.batches.remove(id);
        }
    }

    fn channel_mut(&mut self, name: &str) -> Option<&mut Channel> {
        let key = self.fold(name);
        self.channels.get_mut(&key)
    }

    fn set_topic(&mut self, name: &str, topic: Option<&str>, setter: Option<&str>) {
        if let Some(channel) = self.channel_mut(name) {
            channel.topic = topic.map(str::to_owned);
            channel.topic_set_by = setter.map(str::to_owned);
        }
    }

    fn remove_member(&mut self, channel: &str, nick: &str) {
        let channel_key = self.fold(channel);
        if self.is_me(nick) {
            self.channels.remove(&channel_key);
        } else if let Some(channel) = self.channels.get_mut(&channel_key) {
            channel.members.remove(&self.isupport.fold(nick));
        }
    }

    fn rename_member(&mut self, old: &str, new: &str) {
        if self.is_me(old) {
            new.clone_into(&mut self.nick);
        }
        let old_key = self.fold(old);
        let new_key = self.fold(new);
        for channel in self.channels.values_mut() {
            if let Some(mut member) = channel.members.remove(&old_key) {
                new.clone_into(&mut member.nick);
                channel.members.insert(new_key.clone(), member);
            }
        }
    }

    fn update_members(&mut self, nick: &str, mut update: impl FnMut(&mut Member)) {
        let key = self.fold(nick);
        for channel in self.channels.values_mut() {
            if let Some(member) = channel.members.get_mut(&key) {
                update(member);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn network() -> Network {
        let mut network = Network::new("me");
        for line in [
            ":s 001 me :Welcome",
            ":s 005 me CHANTYPES=# PREFIX=(ov)@+ CASEMAPPING=rfc1459 MONITOR=10 :are supported",
            ":me!u@h JOIN #rust",
            ":s 353 me = #rust :@Alice +bob me carol!c@host",
            ":s 366 me #rust :End of /NAMES list.",
        ] {
            network.apply(&line.parse().unwrap());
        }
        network
    }

    fn apply(network: &mut Network, line: &str) {
        network.apply(&line.parse().unwrap());
    }

    fn nicks(network: &Network, channel: &str) -> Vec<String> {
        let channel = network.channel(channel).unwrap();
        channel
            .sorted_members(network.isupport())
            .iter()
            .map(|m| format!("{}{}", m.prefixes.iter().collect::<String>(), m.nick))
            .collect()
    }

    #[test]
    fn tracks_channel_modes_and_mask_lists() {
        let mut network = network();
        apply(&mut network, ":s 324 me #rust +ntk secret");
        let modes = |network: &Network| network.channel("#rust").unwrap().modes.clone();
        assert_eq!(modes(&network), BTreeMap::from([('k', Some("secret".into())), ('n', None), ('t', None)]));
        apply(&mut network, ":Alice!a@h MODE #rust +ml-k+o 20 secret bob");
        assert_eq!(modes(&network), BTreeMap::from([('l', Some("20".into())), ('m', None), ('n', None), ('t', None)]));
        assert_eq!(nicks(&network, "#rust")[..2], ["@Alice", "@+bob"], "prefixes still handled");

        // Bans: only kept once asked for.
        apply(&mut network, ":s 367 me #rust *!*@spam.example Alice 1700000000");
        assert!(network.channel("#rust").unwrap().mask_lists.is_empty());
        network.request_mask_list("#rust", 'b');
        apply(&mut network, ":s 367 me #rust *!*@spam.example Alice!a@h 1700000000");
        apply(&mut network, ":s 367 me #rust troll!*@*");
        let bans = |network: &Network| network.channel("#rust").unwrap().mask_lists[&'b'].clone();
        assert!(bans(&network).loading);
        apply(&mut network, ":s 368 me #rust :End of channel ban list");
        let list = bans(&network);
        assert!(!list.loading);
        assert_eq!(list.entries[0], MaskEntry { mask: "*!*@spam.example".into(), set_by: Some("Alice!a@h".into()), set_at: Some(1_700_000_000) });
        assert_eq!(list.entries[1].set_by, None);

        apply(&mut network, ":Alice!a@h MODE #rust -b+b troll!*@* *!*@new.example");
        let masks: Vec<String> = bans(&network).entries.into_iter().map(|entry| entry.mask).collect();
        assert_eq!(masks, ["*!*@spam.example", "*!*@new.example"]);
        assert_eq!(bans(&network).entries[1].set_by.as_deref(), Some("Alice"));
    }

    #[test]
    fn tracks_names_sorted_by_rank() {
        let network = network();
        assert_eq!(nicks(&network, "#RUST"), ["@Alice", "+bob", "carol", "me"]);
    }

    #[test]
    fn tracks_joins_parts_kicks_and_quits() {
        let mut network = network();
        apply(&mut network, ":dave!d@h JOIN #rust");
        apply(&mut network, ":bob!b@h PART #rust :bye");
        apply(&mut network, ":Alice!a@h KICK #rust carol :spam");
        apply(&mut network, ":dave!d@h QUIT :gone");
        assert_eq!(nicks(&network, "#rust"), ["@Alice", "me"]);
        assert_eq!(network.channels_with("alice"), ["#rust"]);
    }

    #[test]
    fn leaving_removes_channel() {
        let mut network = network();
        apply(&mut network, ":me!u@h PART #rust");
        assert!(network.channel("#rust").is_none());
    }

    #[test]
    fn renames_members_and_self() {
        let mut network = network();
        apply(&mut network, ":bob!b@h NICK robert");
        apply(&mut network, ":me!u@h NICK myself");
        assert_eq!(network.nick(), "myself");
        assert_eq!(nicks(&network, "#rust"), ["@Alice", "+robert", "carol", "myself"]);
    }

    #[test]
    fn applies_prefix_modes() {
        let mut network = network();
        apply(&mut network, ":Alice!a@h MODE #rust +o-v+v carol bob carol");
        assert_eq!(nicks(&network, "#rust"), ["@Alice", "@+carol", "bob", "me"]);
    }

    #[test]
    fn tracks_topic() {
        let mut network = network();
        apply(&mut network, ":s 332 me #rust :Welcome");
        assert_eq!(network.channel("#rust").unwrap().topic.as_deref(), Some("Welcome"));
        apply(&mut network, ":Alice!a@h TOPIC #rust :");
        let channel = network.channel("#rust").unwrap();
        assert_eq!(channel.topic, None);
        assert_eq!(channel.topic_set_by.as_deref(), Some("Alice"));
    }

    #[test]
    fn tracks_away_and_account() {
        let mut network = network();
        apply(&mut network, ":bob!b@h AWAY :lunch");
        apply(&mut network, ":bob!b@h ACCOUNT bobacct");
        let bob = network.channel("#rust").unwrap().member("bob").unwrap();
        assert!(bob.away);
        assert_eq!(bob.account.as_deref(), Some("bobacct"));
        apply(&mut network, ":bob!b@h AWAY");
        assert!(!network.channel("#rust").unwrap().member("bob").unwrap().away);
    }

    #[test]
    fn extended_join_records_account() {
        let mut network = network();
        apply(&mut network, ":dave!d@h JOIN #rust daveacct :Dave Real");
        let dave = network.channel("#rust").unwrap().member("dave").unwrap();
        assert_eq!(dave.account.as_deref(), Some("daveacct"));
    }

    #[test]
    fn bounds_open_batches() {
        let mut network = Network::new("me");
        for i in 0..200 {
            network.apply(&format!(":s BATCH +b{i} chathistory #c").parse().unwrap());
        }
        assert_eq!(network.batches.len(), MAX_OPEN_BATCHES);
    }

    #[test]
    fn joins_multiline_batches() {
        let mut network = Network::new("me");
        let lines = [
            "@time=2026-11-03T10:00:00.000Z;msgid=ml1 :bob!b@h BATCH +ml draft/multiline #rust",
            "@batch=ml :bob!b@h PRIVMSG #rust :first line",
            "@batch=ml;draft/multiline-concat :bob!b@h PRIVMSG #rust : continued",
            "@batch=ml :bob!b@h PRIVMSG #rust :second line",
        ];
        for line in lines {
            let message: Message = line.parse().unwrap();
            if !network.collect_multiline(&message) {
                network.apply(&message);
            }
        }
        let close: Message = "BATCH -ml".parse().unwrap();
        let combined = network.finish_multiline(&close).unwrap();
        assert_eq!(combined.param(0), Some("#rust"));
        assert_eq!(combined.param(1), Some("first line continued\nsecond line"));
        assert_eq!(combined.source_nick(), Some("bob"));
        assert_eq!(combined.msgid(), Some("ml1"));
        assert!(network.batches.is_empty());

        let other: Message = "BATCH -unknown".parse().unwrap();
        assert!(network.finish_multiline(&other).is_none());
    }

    #[test]
    fn tracks_batches() {
        let mut network = network();
        apply(&mut network, ":s BATCH +abc chathistory #rust");
        let message: Message = "@batch=abc :bob!b@h PRIVMSG #rust :old".parse().unwrap();
        assert_eq!(network.batch_of(&message).unwrap().kind, "chathistory");
        apply(&mut network, ":s BATCH -abc");
        assert!(network.batch_of(&message).is_none());
    }

    #[test]
    fn tracks_monitor_status() {
        let mut network = network();
        assert_eq!(network.is_online("friend"), Some(false));
        apply(&mut network, ":s 730 me :Friend!f@h,other");
        assert_eq!(network.is_online("friend"), Some(true));
        apply(&mut network, ":s 731 me :friend");
        assert_eq!(network.is_online("FRIEND"), Some(false));
    }

    #[test]
    fn applies_whox_replies() {
        let mut network = network();
        assert_eq!(whox_request("#rust").to_string(), "WHO #rust %tcnfa,42");
        apply(&mut network, ":s 354 me 42 #rust bob G+ bobacct");
        apply(&mut network, ":s 354 me 42 #rust carol H 0");
        let channel = network.channel("#rust").unwrap();
        let bob = channel.member("bob").unwrap();
        assert!(bob.away);
        assert_eq!(bob.account.as_deref(), Some("bobacct"));
        assert_eq!(channel.member("carol").unwrap().account, None);
    }

    #[test]
    fn renames_channels() {
        let mut network = network();
        apply(&mut network, ":s RENAME #rust #rustlang :moved");
        assert!(network.channel("#rust").is_none());
        assert_eq!(network.channel("#rustlang").unwrap().name, "#rustlang");
    }

    #[test]
    fn names_refresh_replaces_members() {
        let mut network = network();
        apply(&mut network, ":s 353 me = #rust :@me");
        apply(&mut network, ":s 366 me #rust :End");
        assert_eq!(nicks(&network, "#rust"), ["@me"]);
    }
}
