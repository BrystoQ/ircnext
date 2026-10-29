//! Shared building blocks of the settings forms, for a uniform look: groups
//! as cards, and in each card rows with “label on the left, control aligned
//! on the right”, separated by a thin line.

use std::borrow::Cow;
use std::fmt::Display;

use iced::widget::{column, container, pick_list, row, slider, space, text, text_input, toggler};
use iced::{Alignment, Element, Length};

use crate::app::Message;
use crate::theme::{self, Colors};
use crate::ui::BOLD;

/// Common width of the controls (fields, lists, sliders).
pub const CONTROL_WIDTH: f32 = 240.0;
/// Text size of labels and controls.
const LABEL_SIZE: f32 = 14.0;
const DESCRIPTION_SIZE: f32 = 12.0;

/// Page of a section: title then groups.
pub fn page<'a>(title: &'a str, groups: impl IntoIterator<Item = Element<'a, Message>>, c: Colors) -> Element<'a, Message> {
    groups
        .into_iter()
        .fold(column![text(title).size(18).font(BOLD).color(c.text)].spacing(22), iced::widget::Column::push)
        .into()
}

/// Group of settings: discreet heading, then a card holding the rows.
pub fn group<'a>(title: &'a str, rows: impl IntoIterator<Item = Element<'a, Message>>, c: Colors) -> Element<'a, Message> {
    let mut card = column![];
    for (index, entry) in rows.into_iter().enumerate() {
        if index > 0 {
            card = card.push(divider(c));
        }
        card = card.push(entry);
    }
    column![
        container(text(title).size(12).font(BOLD).color(c.muted)).padding([0, 4]),
        container(card).width(Length::Fill).style(theme::group(c)),
    ]
    .spacing(8)
    .into()
}

/// Accompanying text below a group.
pub fn note<'a>(content: impl Into<Cow<'a, str>>, c: Colors) -> Element<'a, Message> {
    container(text(content.into()).size(DESCRIPTION_SIZE).color(c.muted))
        .padding([0, 4])
        .into()
}

/// Settings row.
pub struct Setting<'a> {
    label: Cow<'a, str>,
    description: Option<Cow<'a, str>>,
    error: Option<Cow<'a, str>>,
    control: Element<'a, Message>,
}

/// “Label on the left, control on the right” row.
pub fn setting<'a>(label: impl Into<Cow<'a, str>>, control: impl Into<Element<'a, Message>>) -> Setting<'a> {
    Setting { label: label.into(), description: None, error: None, control: control.into() }
}

impl<'a> Setting<'a> {
    pub fn description(mut self, description: impl Into<Cow<'a, str>>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Error message shown below the label, if any.
    pub fn error(mut self, error: Option<impl Into<Cow<'a, str>>>) -> Self {
        self.error = error.map(Into::into);
        self
    }

    pub fn view(self, c: Colors) -> Element<'a, Message> {
        let mut label = column![text(self.label).size(LABEL_SIZE).color(c.text)].spacing(3);
        if let Some(description) = self.description {
            label = label.push(text(description).size(DESCRIPTION_SIZE).color(c.muted));
        }
        if let Some(error) = self.error {
            label = label.push(text(error).size(DESCRIPTION_SIZE).color(c.danger));
        }
        row![label.width(Length::Fill), self.control]
            .spacing(16)
            .padding([10, 14])
            .align_y(Alignment::Center)
            .into()
    }
}

/// Setting taking the full width, below its label (grids, lists).
pub fn block<'a>(
    label: impl Into<Cow<'a, str>>,
    description: Option<&'a str>,
    content: impl Into<Element<'a, Message>>,
    c: Colors,
) -> Element<'a, Message> {
    let mut header = column![text(label.into()).size(LABEL_SIZE).color(c.text)].spacing(3);
    if let Some(description) = description {
        header = header.push(text(description).size(DESCRIPTION_SIZE).color(c.muted));
    }
    column![header, content.into()].spacing(12).padding([12, 14]).into()
}

/// Switch.
pub fn switch<'a>(value: bool, on_toggle: impl Fn(bool) -> Message + 'a) -> Element<'a, Message> {
    toggler(value).on_toggle(on_toggle).size(20).into()
}

/// Text field with the common width.
pub fn input<'a>(
    placeholder: &str,
    value: &str,
    on_input: impl Fn(String) -> Message + 'a,
    c: Colors,
) -> text_input::TextInput<'a, Message> {
    text_input(placeholder, value)
        .on_input(on_input)
        .padding([7, 10])
        .size(LABEL_SIZE)
        .width(CONTROL_WIDTH)
        .style(theme::field(c))
}

/// Pick list with the common width.
pub fn select<'a, T>(
    options: impl std::borrow::Borrow<[T]> + 'a,
    selected: Option<T>,
    on_select: impl Fn(T) -> Message + 'a,
    c: Colors,
) -> Element<'a, Message>
where
    T: Display + PartialEq + Clone + 'a,
{
    pick_list(options, selected, on_select)
        .padding([7, 10])
        .text_size(LABEL_SIZE)
        .width(CONTROL_WIDTH)
        .style(theme::select(c))
        .into()
}

/// Slider with the common width, followed by its value.
pub fn range<'a>(
    bounds: std::ops::RangeInclusive<u16>,
    value: u16,
    step: u16,
    unit: &str,
    on_change: impl Fn(u16) -> Message + 'a,
    c: Colors,
) -> Element<'a, Message> {
    row![
        slider(bounds, value, on_change).step(step).width(Length::Fill),
        text(format!("{value} {unit}")).size(13).color(c.muted).width(64).align_x(Alignment::End),
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .width(CONTROL_WIDTH)
    .into()
}

fn divider<'a>(c: Colors) -> Element<'a, Message> {
    // Thin line inset from the card edges.
    container(container(space().height(1)).width(Length::Fill).style(theme::panel(c.border)))
        .padding([0, 14])
        .into()
}
