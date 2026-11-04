//! Channel list of the network (`/list`), filled by `RPL_LIST` replies.

use ircnext_client::proto::numeric::{RPL_LIST, RPL_LISTEND, RPL_LISTSTART};
use ircnext_client::proto::{Command, Message, format};

use super::Server;

/// A listed channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListEntry {
    pub name: String,
    pub users: u32,
    /// Topic without formatting.
    pub topic: String,
    /// Name and topic in lowercase, for the filter.
    search: String,
}

impl ListEntry {
    pub fn matches(&self, query: &str) -> bool {
        self.search.contains(query)
    }
}

/// Replies to the last `/list`.
#[derive(Debug, Clone, Default)]
pub struct ChannelList {
    /// Most populated first, once complete.
    pub entries: Vec<ListEntry>,
    pub complete: bool,
}

impl Server {
    /// Asks the server for its channels (`filter`: `LIST` parameters).
    pub fn request_list(&mut self, filter: Option<&str>) -> Result<(), String> {
        let params: Vec<&str> = filter.into_iter().flat_map(str::split_whitespace).collect();
        self.send(Message::new(Command::List, params))?;
        self.channel_list = Some(ChannelList::default());
        Ok(())
    }

    /// `LIST` replies, while a list was asked from the window. Returns
    /// whether the message was one.
    pub(super) fn on_list_reply(&mut self, message: &Message) -> bool {
        let Some(list) = &mut self.channel_list else {
            return false;
        };
        match message.command {
            Command::Numeric(RPL_LISTSTART) => {}
            Command::Numeric(RPL_LIST) => {
                let name = message.param(1).unwrap_or_default().to_owned();
                let users = message.param(2).and_then(|n| n.parse().ok()).unwrap_or(0);
                let topic = format::strip(message.param(3).unwrap_or_default());
                // `[+nt] topic`: modes some servers put before the topic.
                let topic = match topic.strip_prefix('[').and_then(|rest| rest.split_once("] ")) {
                    Some((modes, rest)) if modes.starts_with('+') => rest.to_owned(),
                    _ => topic,
                };
                let search = format!("{name} {topic}").to_lowercase();
                list.entries.push(ListEntry { name, users, topic, search });
            }
            Command::Numeric(RPL_LISTEND) => {
                list.entries.sort_by(|a, b| b.users.cmp(&a.users).then_with(|| a.name.cmp(&b.name)));
                list.complete = true;
            }
            _ => return false,
        }
        true
    }
}
