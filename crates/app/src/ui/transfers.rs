//! Transfers panel: incoming offers, running and finished DCC transfers.

use std::time::Instant;

use iced::widget::{button, column, container, progress_bar, row, scrollable, space, text};
use iced::{Alignment, Element, Length};

use crate::app::Message;
use crate::i18n::tr;
use crate::theme::{self, Colors};
use crate::tr;
use crate::transfer::{Direction, State, Transfer, TransferId, format_size};
use crate::ui::BOLD;

#[derive(Debug, Clone, Copy)]
pub enum Msg {
    Accept(TransferId),
    Decline(TransferId),
    Cancel(TransferId),
    /// Shows the file (or the downloads folder) in the file manager.
    Reveal(TransferId),
    OpenDownloads,
    ClearFinished,
}

pub fn view<'a>(transfers: &'a [Transfer], now: Instant, c: Colors) -> Element<'a, Message> {
    let list: Element<'a, Message> = if transfers.is_empty() {
        container(
            text(tr("No transfers yet. Use /dcc send <nick> <file>, or drop a file on a private conversation."))
                .size(13)
                .color(c.muted),
        )
        .padding(16)
        .into()
    } else {
        let rows = transfers
            .iter()
            .rev()
            .fold(column![].spacing(8), |rows, transfer| rows.push(item(transfer, now, c)));
        scrollable(container(rows).padding(iced::Padding { right: 12.0, ..iced::Padding::ZERO }))
            .style(theme::scrollbar(c))
            .height(Length::Fill)
            .into()
    };

    let has_finished = transfers.iter().any(|transfer| transfer.state.is_finished());
    let actions = row![
        button(text(tr("Open the downloads folder")).size(13))
            .padding([7, 12])
            .style(theme::secondary(c))
            .on_press(Message::Transfers(Msg::OpenDownloads)),
        button(text(tr("Clear finished")).size(13))
            .padding([7, 12])
            .style(theme::secondary(c))
            .on_press_maybe(has_finished.then_some(Message::Transfers(Msg::ClearFinished))),
        space().width(Length::Fill),
        button(text(tr("Close")).size(14)).padding([8, 16]).style(theme::primary(c)).on_press(Message::CloseModal),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    container(column![text(tr("Transfers")).size(20).font(BOLD).color(c.text), list, actions].spacing(16))
        .padding(24)
        .width(720)
        .height(Length::Fixed(520.0))
        .style(theme::card(c))
        .into()
}

fn item(transfer: &Transfer, now: Instant, c: Colors) -> Element<'_, Message> {
    let id = transfer.id;
    let (arrow, who) = match transfer.direction {
        Direction::Incoming => ("↓", tr!("from {nick}", nick = transfer.nick)),
        Direction::Outgoing => ("↑", tr!("to {nick}", nick = transfer.nick)),
    };
    let size = transfer.size.map_or_else(|| tr("unknown size").to_owned(), format_size);
    let (status, status_color) = match &transfer.state {
        State::Offered => (tr("Waiting for your answer").to_owned(), c.accent),
        State::Waiting if transfer.resuming.is_some() => (tr("Asking to resume…").to_owned(), c.muted),
        State::Waiting => (tr("Waiting for the other side…").to_owned(), c.muted),
        State::Active => {
            let speed = transfer.speed(now).map(|speed| format!(" · {}/s", format_size(speed))).unwrap_or_default();
            (format!("{} / {size}{speed}", format_size(transfer.done)), c.muted)
        }
        State::Done => (tr("Done").to_owned(), c.success),
        State::Failed(error) => (tr!("Failed: {error}", error = error), c.danger),
        State::Cancelled => (tr("Cancelled").to_owned(), c.muted),
        State::Declined => (tr("Declined").to_owned(), c.muted),
    };

    let header = row![
        text(arrow).size(16).color(c.accent),
        column![
            text(transfer.name.as_str()).size(14).font(BOLD).color(c.text),
            text(format!("{who} · {size}")).size(12).color(c.muted),
        ]
        .spacing(2)
        .width(Length::Fill),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let button_of = |label: &'static str, primary: bool, msg: Msg| {
        let button = button(text(label).size(13)).padding([6, 12]).on_press(Message::Transfers(msg));
        if primary { button.style(theme::primary(c)) } else { button.style(theme::secondary(c)) }
    };
    let mut buttons = row![].spacing(6);
    match transfer.state {
        State::Offered => {
            buttons = buttons
                .push(button_of(tr("Accept"), true, Msg::Accept(id)))
                .push(button_of(tr("Decline"), false, Msg::Decline(id)));
        }
        State::Waiting | State::Active => buttons = buttons.push(button_of(tr("Cancel"), false, Msg::Cancel(id))),
        State::Done if transfer.direction == Direction::Incoming => {
            buttons = buttons.push(button_of(tr("Show"), false, Msg::Reveal(id)));
        }
        _ => {}
    }

    let mut content = column![row![header, buttons].spacing(12).align_y(Alignment::Center)].spacing(8);
    if matches!(transfer.state, State::Active | State::Done) {
        #[allow(clippy::cast_precision_loss)]
        let fraction = match transfer.size {
            Some(size) if size > 0 => transfer.done as f32 / size as f32,
            _ => 0.0,
        };
        content = content.push(progress_bar(0.0..=1.0, fraction).girth(4).style(theme::progress(c)));
    }
    content = content.push(text(status).size(12).color(status_color));

    container(content).padding([12, 14]).width(Length::Fill).style(theme::group(c)).into()
}
