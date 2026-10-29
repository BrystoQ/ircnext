//! Member list of a channel.

use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Font, Length, font};
use ircnext_client::{Channel, Member};

use crate::app::Message;
use crate::tr;
use crate::server::Server;
use crate::settings::Settings;
use crate::theme::{self, Colors};
use crate::ui::BOLD;

const WIDTH: f32 = 196.0;

pub fn view<'a>(server: &'a Server, channel: &'a Channel, settings: &Settings, c: Colors) -> Element<'a, Message> {
    let members = channel.sorted_members(server.network.isupport());
    let count = members.len();
    let nick_colors = settings.nick_colors;

    let list = members.into_iter().fold(column![].spacing(1), |list, member| {
        list.push(member_item(server, member, nick_colors, c))
    });

    let header = text(if count == 1 { tr!("{count} user", count = count) } else { tr!("{count} users", count = count) })
    .size(12)
    .font(BOLD)
    .color(c.muted);

    container(
        column![
            container(header).padding([18, 14]),
            scrollable(container(list).padding([0, 8]))
                .style(theme::scrollbar(c))
                .height(Length::Fill),
        ],
    )
    .width(WIDTH)
    .height(Length::Fill)
    .style(theme::bordered(c, c.sidebar))
    .into()
}

fn member_item<'a>(server: &'a Server, member: &'a Member, nick_colors: bool, c: Colors) -> Element<'a, Message> {
    let prefix = member.highest_prefix().map(String::from).unwrap_or_default();
    let prefix_color = match member.highest_prefix() {
        Some('~' | '&') => c.danger,
        Some('@') => c.success,
        Some('%') => c.warning,
        Some(_) => c.accent,
        None => c.muted,
    };
    let base = if nick_colors { c.nick(&member.nick) } else { c.text };
    let (color, style) = if member.away {
        (base.scale_alpha(0.45), font::Style::Italic)
    } else {
        (base, font::Style::Normal)
    };

    let label = row![
        text(prefix).size(13).font(BOLD).color(prefix_color).width(12),
        text(&member.nick).size(13).color(color).font(Font { style, ..Font::DEFAULT }),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    button(label)
        .width(Length::Fill)
        .padding([3, 6])
        .style(theme::list_item(c, false))
        .on_press(Message::OpenQuery(server.id, member.nick.clone()))
        .into()
}
