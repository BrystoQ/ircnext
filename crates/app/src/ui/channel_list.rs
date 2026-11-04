//! Channel list window (`/list`): the network's channels, searchable.

use iced::widget::{button, column, container, row, scrollable, space, text, text_input};
use iced::{Alignment, Element, Length};

use crate::app::Message;
use crate::i18n::tr;
use crate::server::{Server, ServerId};
use crate::theme::{self, Colors};
use crate::tr;
use crate::ui::BOLD;

/// Id of the filter field.
pub const CHANNEL_LIST_ID: &str = "channel-list";
/// Rows shown at most (large networks list tens of thousands).
const MAX_ROWS: usize = 200;

#[derive(Debug, Clone)]
pub struct ChannelListView {
    pub server: ServerId,
    pub query: String,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Query(String),
    Join(String),
    Refresh,
}

impl ChannelListView {
    pub fn new(server: ServerId) -> Self {
        Self { server, query: String::new() }
    }

    pub fn view<'a>(&'a self, server: Option<&'a Server>, c: Colors) -> Element<'a, Message> {
        let list = server.and_then(|server| server.channel_list.as_ref());
        let query = self.query.trim().to_lowercase();
        let matching: Vec<_> = list
            .map(|list| list.entries.iter().filter(|entry| query.is_empty() || entry.matches(&query)).collect())
            .unwrap_or_default();

        let filter = text_input(tr("Search a channel or a topic"), &self.query)
            .id(CHANNEL_LIST_ID)
            .on_input(|query| Message::ChannelList(Msg::Query(query)))
            .padding([8, 12])
            .size(14)
            .style(theme::input(c));

        let rows: Vec<Element<'_, Message>> = matching
            .iter()
            .take(MAX_ROWS)
            .map(|entry| {
                let topic: Element<'_, Message> = if entry.topic.is_empty() {
                    space().into()
                } else {
                    text(&entry.topic).size(12).color(c.muted).wrapping(text::Wrapping::WordOrGlyph).into()
                };
                container(
                    row![
                        column![text(&entry.name).size(14).color(c.text), topic].spacing(2).width(Length::Fill),
                        text(tr!("{count} users", count = entry.users)).size(12).color(c.muted),
                        button(text(tr("Join")).size(12))
                            .padding([5, 10])
                            .style(theme::secondary(c))
                            .on_press(Message::ChannelList(Msg::Join(entry.name.clone()))),
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                )
                .padding([8, 10])
                .style(theme::panel(c.surface))
                .into()
            })
            .collect();

        let status = match list {
            None => tr("Not connected.").to_owned(),
            Some(list) if !list.complete => tr!("Loading… {count} channels", count = list.entries.len()),
            Some(_) if matching.len() > MAX_ROWS => tr!(
                "{shown} of {count} channels shown · refine the search",
                shown = MAX_ROWS,
                count = matching.len()
            ),
            Some(list) if query.is_empty() => tr!("{count} channels", count = list.entries.len()),
            Some(list) => tr!("{matching} of {count} channels", matching = matching.len(), count = list.entries.len()),
        };
        let body: Element<'_, Message> = if rows.is_empty() {
            let empty = if list.is_some_and(|list| list.complete) { tr("No channel matches.") } else { "" };
            container(text(empty).size(13).color(c.muted)).padding(16).height(Length::Fill).into()
        } else {
            scrollable(container(column(rows).spacing(4)).padding(iced::Padding { right: 12.0, ..iced::Padding::ZERO }))
                .style(theme::scrollbar(c))
                .height(Length::Fill)
                .into()
        };
        let actions = row![
            text(status).size(12).color(c.muted),
            space().width(Length::Fill),
            button(text(tr("Refresh")).size(13))
                .padding([7, 12])
                .style(theme::secondary(c))
                .on_press_maybe(list.is_none_or(|list| list.complete).then_some(Message::ChannelList(Msg::Refresh))),
            button(text(tr("Close")).size(14)).padding([8, 16]).style(theme::primary(c)).on_press(Message::CloseModal),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let title = server.map_or_else(|| tr("Channels").to_owned(), |server| tr!("Channels of {network}", network = server.entry.name));
        container(column![text(title).size(20).font(BOLD).color(c.text), filter, body, actions].spacing(14))
            .padding(24)
            .width(760)
            .height(Length::Fixed(600.0))
            .style(theme::card(c))
            .into()
    }
}
