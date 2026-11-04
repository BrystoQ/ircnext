//! Confirmation before sending a text of several lines.

use iced::widget::{button, column, container, row, scrollable, space, text};
use iced::{Alignment, Element, Length};

use crate::app::Message;
use crate::i18n::tr;
use crate::server::{BufferId, ServerId};
use crate::theme::{self, Colors};
use crate::tr;
use crate::ui::BOLD;

/// Lines shown in the preview.
const PREVIEW_LINES: usize = 12;

/// A text waiting for confirmation.
#[derive(Debug, Clone)]
pub struct Paste {
    pub server: ServerId,
    pub buffer: BufferId,
    /// Conversation it goes to.
    pub target: String,
    pub text: String,
    /// The input without the pasted text: kept on cancel and after sending.
    pub input: String,
}

impl Paste {
    /// Lines that will be sent.
    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.text.lines().filter(|line| !line.trim().is_empty())
    }

    /// The lines joined by spaces, as one message.
    pub fn joined(&self) -> String {
        self.lines().map(str::trim).collect::<Vec<_>>().join(" ")
    }
}

/// Number of non-empty lines of `text`.
pub fn line_count(text: &str) -> usize {
    text.lines().filter(|line| !line.trim().is_empty()).count()
}

pub fn view(paste: &Paste, c: Colors) -> Element<'_, Message> {
    let count = paste.lines().count();
    let mut preview = paste.lines().take(PREVIEW_LINES).fold(column![].spacing(2), |preview, line| {
        preview.push(text(line).size(13).color(c.text))
    });
    if count > PREVIEW_LINES {
        preview = preview.push(text(tr!("… and {count} more lines", count = count - PREVIEW_LINES)).size(13).color(c.muted));
    }
    let preview = container(scrollable(preview).style(theme::scrollbar(c)).height(Length::Shrink))
        .padding(12)
        .width(Length::Fill)
        .max_height(260)
        .style(theme::panel(c.surface));

    let actions = row![
        button(text(tr("Send as one line")).size(13))
            .padding([7, 12])
            .style(theme::secondary(c))
            .on_press(Message::PasteJoined),
        space().width(Length::Fill),
        button(text(tr("Cancel")).size(14)).padding([8, 16]).style(theme::secondary(c)).on_press(Message::CloseModal),
        button(text(tr!("Send {count} lines", count = count)).size(14))
            .padding([8, 16])
            .style(theme::primary(c))
            .on_press(Message::PasteSend),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    container(
        column![
            text(tr("Send several lines?")).size(20).font(BOLD).color(c.text),
            text(tr!("{count} messages will be sent to {target}, one per line.", count = count, target = paste.target))
                .size(13)
                .color(c.muted),
            preview,
            actions,
        ]
        .spacing(16),
    )
    .padding(24)
    .width(620)
    .style(theme::card(c))
    .into()
}
