//! Links panel (mIRC's URL catcher): the web links seen in the open tabs,
//! newest first.

use chrono::{DateTime, Local};
use iced::widget::{button, column, container, row, scrollable, space, text, text_input};
use iced::{Alignment, Element, Length};
use ircnext_client::proto::{format, text as irc_text};

use crate::app::Message;
use crate::buffer::LineKind;
use crate::i18n::tr;
use crate::server::{BufferId, Server, ServerId};
use crate::theme::{self, Colors};
use crate::tr;
use crate::ui::BOLD;

/// Id of the filter field.
pub const LINKS_ID: &str = "links";

/// A link and where it was seen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caught {
    pub url: String,
    /// Who posted it (empty for a topic without author).
    pub nick: String,
    pub server: ServerId,
    pub buffer: BufferId,
    /// `#channel · Network`.
    pub place: String,
    pub time: DateTime<Local>,
}

#[derive(Debug, Clone, Default)]
pub struct Links {
    pub query: String,
    pub links: Vec<Caught>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Query(String),
    Copy(String),
    /// Shows the conversation where the link was posted.
    GoTo(ServerId, BufferId),
}

impl Links {
    /// Links of the open tabs, at the time the panel opens.
    pub fn new(servers: &[Server]) -> Self {
        Self { query: String::new(), links: collect(servers) }
    }

    /// Links matching the filter (address, nick or place).
    pub fn filtered(&self) -> impl Iterator<Item = &Caught> {
        let query = self.query.trim().to_lowercase();
        self.links.iter().filter(move |link| {
            query.is_empty()
                || [&link.url, &link.nick, &link.place].iter().any(|field| field.to_lowercase().contains(&query))
        })
    }

    pub fn view(&self, c: Colors) -> Element<'_, Message> {
        let filter = text_input(tr("Filter by address, nick or channel"), &self.query)
            .id(LINKS_ID)
            .on_input(|query| Message::Links(Msg::Query(query)))
            .padding([8, 12])
            .size(14)
            .style(theme::input(c));
        let rows: Vec<Element<'_, Message>> = self.filtered().map(|link| item(link, c)).collect();
        let list: Element<'_, Message> = if rows.is_empty() {
            let empty = if self.links.is_empty() {
                tr("No link in the open tabs yet.")
            } else {
                tr("No link matches the filter.")
            };
            container(text(empty).size(13).color(c.muted)).padding(16).into()
        } else {
            scrollable(container(column(rows).spacing(4)).padding(iced::Padding { right: 12.0, ..iced::Padding::ZERO }))
                .style(theme::scrollbar(c))
                .height(Length::Fill)
                .into()
        };
        let count = self.links.len();
        let actions = row![
            text(tr!("{count} links", count = count)).size(12).color(c.muted),
            space().width(Length::Fill),
            button(text(tr("Close")).size(14)).padding([8, 16]).style(theme::primary(c)).on_press(Message::CloseModal),
        ]
        .align_y(Alignment::Center);

        container(column![text(tr("Links")).size(20).font(BOLD).color(c.text), filter, list, actions].spacing(14))
            .padding(24)
            .width(760)
            .height(Length::Fixed(560.0))
            .style(theme::card(c))
            .into()
    }
}

fn item(link: &Caught, c: Colors) -> Element<'_, Message> {
    let from = if link.nick.is_empty() {
        format!("{} · {}", link.time.format("%H:%M"), link.place)
    } else {
        format!("{} · {} · {}", link.time.format("%H:%M"), link.nick, link.place)
    };
    let details = column![
        button(text(&link.url).size(14).color(c.accent).wrapping(text::Wrapping::Glyph))
            .padding(0)
            .style(theme::ghost(c))
            .on_press(Message::OpenUrl(link.url.clone())),
        text(from).size(12).color(c.muted),
    ]
    .spacing(2)
    .width(Length::Fill);
    container(
        row![
            details,
            button(text(tr("Copy")).size(12))
                .padding([5, 10])
                .style(theme::secondary(c))
                .on_press(Message::Links(Msg::Copy(link.url.clone()))),
            button(text(tr("Show")).size(12))
                .padding([5, 10])
                .style(theme::secondary(c))
                .on_press(Message::Links(Msg::GoTo(link.server, link.buffer))),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([8, 10])
    .style(theme::panel(c.surface))
    .into()
}

/// Links in messages and topics of every tab, newest first; a link posted
/// again only appears once, at its last mention.
pub fn collect(servers: &[Server]) -> Vec<Caught> {
    let mut links: Vec<Caught> = Vec::new();
    for server in servers {
        for (id, buffer) in server.buffers() {
            let place = format!("{} · {}", buffer.name, server.entry.name);
            for line in buffer.lines() {
                let (nick, content) = match &line.kind {
                    LineKind::Message { nick, text, .. } | LineKind::Action { nick, text } | LineKind::Notice { nick, text } => {
                        (nick.as_str(), text.as_str())
                    }
                    LineKind::Topic { nick, topic: Some(topic) } => (nick.as_deref().unwrap_or_default(), topic.as_str()),
                    _ => continue,
                };
                let clean = format::strip(content);
                for found in irc_text::find_links(&clean) {
                    let url = &clean[found.start..found.end];
                    if irc_text::safe_url(url).is_none() {
                        continue;
                    }
                    links.push(Caught {
                        url: url.to_owned(),
                        nick: nick.to_owned(),
                        server: server.id,
                        buffer: id,
                        place: place.clone(),
                        time: line.time,
                    });
                }
            }
        }
    }
    links.sort_by_key(|link| std::cmp::Reverse(link.time));
    let mut seen = std::collections::HashSet::new();
    links.retain(|link| seen.insert(link.url.clone()));
    links
}
