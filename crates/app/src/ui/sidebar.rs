//! Sidebar: networks and tabs, as a tree like in HexChat.

use iced::widget::{button, column, container, row, scrollable, space, text, tooltip};
use iced::{Alignment, Element, Length};

use crate::app::Message;
use crate::i18n::tr;
use crate::tr;
use crate::buffer::{Buffer, Kind};
use crate::server::{BufferId, Server, ServerId, Status};
use crate::theme::{self, Colors};
use crate::ui::BOLD;

const WIDTH: f32 = 236.0;

pub fn view(servers: &[Server], active: Option<(ServerId, BufferId)>, c: Colors) -> Element<'_, Message> {
    let header = row![
        text("ircnext").size(17).font(BOLD).color(c.text),
        space().width(Length::Fill),
        icon_button("+", tr("Add a network"), Message::OpenServerForm(None), c),
        icon_button("⚙", tr("Settings"), Message::OpenSettings, c),
    ]
    .spacing(2)
    .align_y(Alignment::Center)
    .padding([14, 12]);

    let tree = servers.iter().fold(column![].spacing(10), |tree, server| {
        tree.push(server_section(server, active, c))
    });

    let list: Element<'_, Message> = if servers.is_empty() {
        container(text(tr("No network.\nClick + to add one.")).size(13).color(c.muted))
            .padding(16)
            .into()
    } else {
        scrollable(container(tree).padding([4, 8]))
            .style(theme::scrollbar(c))
            .height(Length::Fill)
            .into()
    };

    container(column![header, list])
        .width(WIDTH)
        .height(Length::Fill)
        .style(theme::bordered(c, c.sidebar))
        .into()
}

fn server_section(server: &Server, active: Option<(ServerId, BufferId)>, c: Colors) -> Element<'_, Message> {
    let id = server.id;
    let server_buffer = server.server_buffer();
    let selected = active == Some((id, server_buffer));

    let (status_color, status_label) = match &server.status {
        Status::Online => (c.success, tr("Connected").to_owned()),
        Status::Registering => (c.warning, tr("Registering…").to_owned()),
        Status::Connecting(_) => (c.warning, tr("Connecting…").to_owned()),
        Status::Waiting { retry_in, .. } => (c.warning, tr!("Reconnecting in {seconds} s", seconds = retry_in.as_secs())),
        Status::Disconnected(Some(_)) => (c.danger, tr("Disconnected (error)").to_owned()),
        Status::Disconnected(None) => (c.muted, tr("Disconnected").to_owned()),
    };

    let chevron = button(text(if server.collapsed { "▸" } else { "▾" }).size(12).color(c.muted))
        .padding([2, 4])
        .style(theme::ghost(c))
        .on_press(Message::ToggleCollapsed(id));

    let unread_collapsed: usize = if server.collapsed {
        server.buffers().map(|(_, b)| b.unread).sum()
    } else {
        0
    };

    let title = button(
        row![
            tooltip(
                text("●").size(10).color(status_color),
                container(text(status_label).size(12)).padding(6).style(theme::card(c)),
                tooltip::Position::Bottom,
            ),
            text(server.name()).size(13).font(BOLD).color(c.text),
            space().width(Length::Fill),
        ]
        .push((unread_collapsed > 0).then(|| unread_badge(unread_collapsed, false, c)))
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([6, 8])
    .style(theme::list_item(c, selected))
    .on_press(Message::Select(id, server_buffer));

    let power = if server.enabled {
        icon_button("⏻", tr("Disconnect"), Message::Disconnect(id), c)
    } else {
        icon_button("⏻", tr("Connect"), Message::Connect(id), c)
    };
    let header = row![
        chevron,
        title,
        power,
        icon_button("✎", tr("Edit the network"), Message::OpenServerForm(Some(id)), c),
    ]
    .spacing(2)
    .align_y(Alignment::Center);

    if server.collapsed {
        return header.into();
    }

    let items = server
        .buffers()
        .filter(|(buffer_id, _)| *buffer_id != server_buffer)
        .fold(column![].spacing(1), |items, (buffer_id, buffer)| {
            items.push(buffer_item(server, buffer_id, buffer, active == Some((id, buffer_id)), c))
        });

    column![header, container(items).padding(iced::Padding { left: 22.0, ..iced::Padding::ZERO })]
        .spacing(2)
        .into()
}

fn buffer_item<'a>(
    server: &'a Server,
    id: BufferId,
    buffer: &'a Buffer,
    selected: bool,
    c: Colors,
) -> Element<'a, Message> {
    let (sigil, sigil_color) = match buffer.kind {
        Kind::Channel => ("#", c.muted),
        Kind::Query => match server.network.is_online(&buffer.name) {
            Some(true) => ("●", c.success),
            Some(false) => ("○", c.muted),
            None => ("@", c.muted),
        },
        Kind::Server => ("", c.muted),
    };
    let name = buffer.name.trim_start_matches(['#', '&']);
    let name_color = if buffer.unread > 0 || selected {
        c.text
    } else if buffer.joined {
        c.text.scale_alpha(0.75)
    } else {
        c.muted.scale_alpha(0.7)
    };

    let content = row![
        text(sigil).size(13).color(sigil_color).width(12),
        text(name).size(13).color(name_color).font(if buffer.unread > 0 { BOLD } else { iced::Font::DEFAULT }),
        space().width(Length::Fill),
    ]
    .push((buffer.unread > 0).then(|| unread_badge(buffer.unread, buffer.highlighted, c)))
    .spacing(6)
    .align_y(Alignment::Center);

    button(content)
        .width(Length::Fill)
        .padding([5, 8])
        .style(theme::list_item(c, selected))
        .on_press(Message::Select(server.id, id))
        .into()
}

fn unread_badge(count: usize, highlighted: bool, c: Colors) -> Element<'static, Message> {
    let label = if count > 99 { "99+".to_owned() } else { count.to_string() };
    let (background, foreground) = if highlighted { (c.accent, c.on_accent) } else { (c.hover, c.muted) };
    container(text(label).size(11).font(BOLD).color(foreground))
        .padding([1, 7])
        .style(theme::badge(background))
        .into()
}

fn icon_button<'a>(icon: &'a str, label: &'a str, message: Message, c: Colors) -> Element<'a, Message> {
    tooltip(
        button(text(icon).size(14)).padding([3, 7]).style(theme::ghost(c)).on_press(message),
        container(text(label).size(12)).padding(6).style(theme::card(c)),
        tooltip::Position::Bottom,
    )
    .into()
}
