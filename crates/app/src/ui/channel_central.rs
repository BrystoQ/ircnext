//! Channel central window (mIRC's “channel central”): topic, modes, and the
//! ban, exception and invitation lists of a channel.

use chrono::{Local, TimeZone};
use iced::widget::{button, column, container, row, scrollable, space, text, text_input};
use iced::{Alignment, Element, Length};

use crate::app::Message;
use crate::i18n::tr;
use crate::server::{Server, ServerId};
use crate::theme::{self, Colors};
use crate::tr;
use crate::ui::{BOLD, form};

/// Simple modes offered as switches, if the network has them.
const SWITCHES: [char; 5] = ['n', 't', 'i', 'm', 's'];

#[derive(Debug, Clone)]
pub struct ChannelCentral {
    pub server: ServerId,
    pub channel: String,
    /// Mask list shown (`b`, `e`, `I`), or `None` for the topic and modes.
    pub tab: Option<char>,
    /// Edited values; `None` shows the current one.
    pub topic: Option<String>,
    pub key: Option<String>,
    pub limit: Option<String>,
    pub new_mask: String,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Tab(Option<char>),
    Topic(String),
    SaveTopic,
    Mode(char, bool),
    Key(String),
    SaveKey,
    Limit(String),
    SaveLimit,
    NewMask(String),
    AddMask,
    RemoveMask(String),
}

fn mode_label(mode: char) -> (&'static str, &'static str) {
    match mode {
        'n' => (tr("No outside messages"), tr("Only members can write to the channel")),
        't' => (tr("Protected topic"), tr("Only operators change the topic")),
        'i' => (tr("Invite only"), tr("Joining requires an invitation")),
        'm' => (tr("Moderated"), tr("Only operators and voiced users speak")),
        _ => (tr("Secret"), tr("Hidden from /list and /whois")),
    }
}

fn list_label(mode: char) -> &'static str {
    match mode {
        'b' => tr("Bans"),
        'e' => tr("Exceptions"),
        _ => tr("Invitations"),
    }
}

impl ChannelCentral {
    pub fn new(server: ServerId, channel: String) -> Self {
        Self { server, channel, tab: None, topic: None, key: None, limit: None, new_mask: String::new() }
    }

    pub fn view<'a>(&'a self, server: Option<&'a Server>, c: Colors) -> Element<'a, Message> {
        let msg = |msg: Msg| Message::ChannelCentral(msg);
        let channel = server.and_then(|server| server.network.channel(&self.channel));
        let manage = server.is_some_and(|server| server.can_manage(&self.channel));
        let current = |mode: char| channel.and_then(|channel| channel.modes.get(&mode));
        let topic_locked = current('t').is_some() && !manage;

        let topic_value = self.topic.as_deref().or_else(|| channel.and_then(|channel| channel.topic.as_deref())).unwrap_or_default();
        let topic_input = text_input(tr("No topic"), topic_value).padding([8, 12]).size(14).style(theme::input(c));
        let topic_input = if topic_locked { topic_input } else { topic_input.on_input(move |value| msg(Msg::Topic(value))).on_submit(msg(Msg::SaveTopic)) };
        let topic = row![
            topic_input,
            button(text(tr("Set")).size(13))
                .padding([7, 12])
                .style(theme::secondary(c))
                .on_press_maybe((self.topic.is_some() && !topic_locked).then(|| msg(Msg::SaveTopic))),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let simple = server.map(|server| server.network.isupport().chanmodes[3].clone()).unwrap_or_default();
        let switches: Vec<Element<'_, Message>> = SWITCHES
            .into_iter()
            .filter(|mode| simple.contains(*mode))
            .map(|mode| {
                let (label, description) = mode_label(mode);
                let on = current(mode).is_some();
                let control: Element<'_, Message> = if manage {
                    form::switch(on, move |value| msg(Msg::Mode(mode, value)))
                } else {
                    text(if on { tr("On") } else { tr("Off") }).size(13).color(c.muted).into()
                };
                form::setting(format!("{label} (+{mode})"), control).description(description).view(c)
            })
            .collect();

        let field = |placeholder: &'a str, edited: Option<&'a str>, mode: char, on_input: fn(String) -> Msg, save: Msg| -> Element<'a, Message> {
            let value = edited.or_else(|| current(mode).and_then(Option::as_deref)).unwrap_or_default();
            let input = text_input(placeholder, value).width(200).padding([6, 10]).size(13).style(theme::input(c));
            if manage {
                row![input.on_input(move |value| msg(on_input(value))).on_submit(msg(save.clone())), button(text(tr("Apply")).size(12)).padding([6, 10]).style(theme::secondary(c)).on_press_maybe(edited.is_some().then(|| msg(save.clone())))]
                    .spacing(6)
                    .align_y(Alignment::Center)
                    .into()
            } else {
                input.into()
            }
        };
        let key = form::setting(tr("Key (+k)"), field(tr("None"), self.key.as_deref(), 'k', Msg::Key, Msg::SaveKey))
            .description(tr("Password to join · empty to remove"))
            .view(c);
        let limit = form::setting(tr("User limit (+l)"), field(tr("None"), self.limit.as_deref(), 'l', Msg::Limit, Msg::SaveLimit))
            .description(tr("Empty to remove"))
            .view(c);

        let mut settings: Vec<Element<'_, Message>> = switches;
        settings.push(key);
        settings.push(limit);
        let modes = form::group(tr("Modes"), settings, c);

        let lists = server.map(Server::mask_modes).unwrap_or_default();
        let general = button(text(tr("General")).size(13))
            .padding([6, 12])
            .style(theme::list_item(c, self.tab.is_none()))
            .on_press(msg(Msg::Tab(None)));
        let tabs = lists.iter().fold(row![general].spacing(4), |tabs, &mode| {
            let count = channel.and_then(|channel| channel.mask_lists.get(&mode)).map(|list| list.entries.len());
            let label = count.map_or_else(|| list_label(mode).to_owned(), |count| format!("{} ({count})", list_label(mode)));
            tabs.push(button(text(label).size(13)).padding([6, 12]).style(theme::list_item(c, Some(mode) == self.tab)).on_press(msg(Msg::Tab(Some(mode)))))
        });
        let list = self.tab.and_then(|tab| channel.and_then(|channel| channel.mask_lists.get(&tab)));
        let entries: Element<'_, Message> = match list {
            None => text(tr("Reserved to operators on most networks.")).size(13).color(c.muted).into(),
            Some(list) if list.loading => text(tr("Loading…")).size(13).color(c.muted).into(),
            Some(list) if list.entries.is_empty() => text(tr("Empty list.")).size(13).color(c.muted).into(),
            Some(list) => column(list.entries.iter().map(|entry| {
                let mut details = entry.set_by.as_deref().map(|by| by.split('!').next().unwrap_or(by)).map_or_else(String::new, |by| tr!("by {nick}", nick = by));
                if let Some(at) = entry.set_at.and_then(|at| Local.timestamp_opt(at, 0).single()) {
                    details = format!("{details} · {}", at.format("%Y-%m-%d %H:%M"));
                }
                let remove = button(text(tr("Remove")).size(12)).padding([4, 10]).style(theme::secondary(c));
                container(
                    row![
                        column![text(&entry.mask).size(13).color(c.text), text(details.trim_start_matches(" · ").to_owned()).size(11).color(c.muted)]
                            .spacing(2)
                            .width(Length::Fill),
                        if manage { remove.on_press(msg(Msg::RemoveMask(entry.mask.clone()))) } else { remove },
                    ]
                    .align_y(Alignment::Center),
                )
                .padding([6, 10])
                .style(theme::panel(c.surface))
                .into()
            }))
            .spacing(4)
            .into(),
        };
        let add: Element<'_, Message> = if manage {
            row![
                text_input("nick!user@host", &self.new_mask)
                    .on_input(move |value| msg(Msg::NewMask(value)))
                    .on_submit(msg(Msg::AddMask))
                    .padding([6, 10])
                    .size(13)
                    .style(theme::input(c)),
                button(text(tr("Add")).size(12))
                    .padding([6, 12])
                    .style(theme::secondary(c))
                    .on_press_maybe((!self.new_mask.trim().is_empty()).then(|| msg(Msg::AddMask))),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .into()
        } else {
            space().into()
        };

        let notice = if manage { tr("Changes are sent to the server at once.") } else { tr("You are not an operator of this channel: read only.") };
        let content: Element<'_, Message> = if self.tab.is_some() {
            column![entries, add].spacing(8).into()
        } else {
            column![form::group(tr("Topic"), [container(topic).padding([10, 14]).into()], c), modes].spacing(18).into()
        };

        container(
            column![
                text(tr!("{channel} settings", channel = self.channel)).size(20).font(BOLD).color(c.text),
                tabs,
                scrollable(container(content).padding(iced::Padding { right: 14.0, ..iced::Padding::ZERO }))
                    .style(theme::scrollbar(c))
                    .height(Length::Fill),
                row![
                    text(notice).size(12).color(c.muted),
                    space().width(Length::Fill),
                    button(text(tr("Close")).size(14)).padding([8, 16]).style(theme::primary(c)).on_press(Message::CloseModal),
                ]
                .align_y(Alignment::Center),
            ]
            .spacing(14),
        )
        .padding(24)
        .width(760)
        .height(Length::Fixed(640.0))
        .style(theme::card(c))
        .into()
    }
}
