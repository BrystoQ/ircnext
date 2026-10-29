//! Quick tab switcher, as in Slack or Discord.

use iced::widget::{button, column, container, row, text, text_input};
use iced::{Alignment, Element, Length};

use crate::app::Message;
use crate::i18n::tr;
use crate::buffer::Kind;
use crate::server::{BufferId, Server, ServerId};
use crate::theme::{self, Colors};
use crate::ui::BOLD;

/// Id of the search field.
pub const SWITCHER_ID: &str = "switcher";
/// Maximum number of results shown.
const MAX_RESULTS: usize = 10;

#[derive(Debug, Clone, Default)]
pub struct Switcher {
    pub query: String,
    pub selected: usize,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Query(String),
    Submit,
}

impl Switcher {
    /// Tabs matching the search, most relevant first.
    ///
    /// Without a search: unread tabs first (mentions on top), except the
    /// current tab. With a search: start of the name, then content, then
    /// letters in order (`rst` finds `#rust`).
    pub fn results(&self, servers: &[Server], active: Option<(ServerId, BufferId)>) -> Vec<(ServerId, BufferId)> {
        let query = self.query.trim().to_lowercase();
        let mut scored: Vec<(u8, usize, (ServerId, BufferId))> = Vec::new();
        let mut order = 0;
        for server in servers {
            for (id, buffer) in server.buffers() {
                order += 1;
                let key = (server.id, id);
                let name = buffer.name.to_lowercase();
                let trimmed = name.trim_start_matches(['#', '&']);
                let score = if query.is_empty() {
                    if Some(key) == active {
                        continue;
                    }
                    match (buffer.highlighted, buffer.unread > 0) {
                        (true, _) => 0,
                        (false, true) => 1,
                        _ => 2,
                    }
                } else if name.starts_with(&query) || trimmed.starts_with(&query) {
                    0
                } else if name.contains(&query) {
                    1
                } else if is_subsequence(&query, &name) {
                    2
                } else {
                    continue;
                };
                scored.push((score, order, key));
            }
        }
        scored.sort_unstable_by_key(|&(score, order, _)| (score, order));
        scored.into_iter().take(MAX_RESULTS).map(|(_, _, key)| key).collect()
    }

    pub fn view<'a>(
        &'a self,
        servers: &'a [Server],
        active: Option<(ServerId, BufferId)>,
        c: Colors,
    ) -> Element<'a, Message> {
        let results = self.results(servers, active);
        let mut list = column![].spacing(2);
        for (index, &(server_id, buffer_id)) in results.iter().enumerate() {
            let Some(server) = servers.iter().find(|s| s.id == server_id) else {
                continue;
            };
            let Some(buffer) = server.buffer(buffer_id) else {
                continue;
            };
            let icon = match buffer.kind {
                Kind::Server => "◆",
                Kind::Channel => "#",
                Kind::Query => "@",
            };
            let mut entry = row![
                text(icon).size(13).color(c.muted).width(16),
                text(buffer.name.as_str()).size(14).font(BOLD).color(c.text),
                text(server.name()).size(12).color(c.muted).width(Length::Fill),
            ]
            .spacing(10)
            .align_y(Alignment::Center);
            if buffer.unread > 0 {
                let background = if buffer.highlighted { c.accent } else { c.muted.scale_alpha(0.4) };
                entry = entry.push(
                    container(text(buffer.unread.min(999).to_string()).size(11).color(c.on_accent))
                        .padding([1, 7])
                        .style(theme::badge(background)),
                );
            }
            list = list.push(
                button(entry)
                    .width(Length::Fill)
                    .padding([8, 12])
                    .style(theme::list_item(c, index == self.selected))
                    .on_press(Message::Select(server_id, buffer_id)),
            );
        }
        if results.is_empty() {
            list = list.push(container(text(tr("No tab matches.")).size(13).color(c.muted)).padding(12));
        }

        let input = text_input(tr("Go to a channel, a conversation or a network…"), &self.query)
            .id(SWITCHER_ID)
            .on_input(|query| Message::Switcher(Msg::Query(query)))
            .on_submit(Message::Switcher(Msg::Submit))
            .padding([12, 14])
            .size(16)
            .style(theme::input(c));
        let hint = text(tr("↑↓ to choose · Enter to open · Esc to close")).size(11).color(c.muted);

        container(column![input, list, hint].spacing(10))
            .padding(16)
            .width(560)
            .style(theme::card(c))
            .into()
    }
}

/// Letters of `needle` found in `haystack`, in order.
fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut letters = haystack.chars();
    needle.chars().all(|wanted| letters.any(|c| c == wanted))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_letters_in_order() {
        assert!(is_subsequence("rst", "#rust"));
        assert!(!is_subsequence("tsr", "#rust"));
        assert!(is_subsequence("", "#rust"));
    }
}
