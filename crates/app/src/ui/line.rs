//! Rendering of a chat line.

use std::borrow::Cow;

use iced::widget::text::Span;
use iced::widget::{button, container, rich_text, row, span, text};
use iced::{Alignment, Color, Element, Font, Length, alignment, font};
use ircnext_client::proto::{format, text as irc_text};

use crate::app::Message;
use crate::buffer::{Line, LineKind};
use crate::i18n::tr;
use crate::tr;
use crate::server::ServerId;
use crate::settings::{NickAlignment, Settings};
use crate::theme::{self, Colors};
use crate::ui::BOLD;

pub fn view<'a>(line: &'a Line, server: ServerId, settings: &Settings, c: Colors) -> Element<'a, Message> {
    let size = f32::from(settings.font_size);
    let base_font = if settings.monospace { Font::MONOSPACE } else { Font::DEFAULT };
    let nick_color = |nick: &str| {
        if settings.nick_colors { c.nick(nick) } else { c.text }
    };

    let (gutter, body): (Element<'a, Message>, Element<'a, Message>) = match &line.kind {
        LineKind::Message { nick, prefix, text } => {
            let label = match prefix {
                Some(prefix) if settings.show_nick_prefix => format!("{prefix}{nick}"),
                _ => nick.clone(),
            };
            let color = if line.own { c.text } else { nick_color(nick) };
            (
                nick_button(label, nick, server, color, size),
                message_body(text, c.text, base_font, size, c, settings.show_colors),
            )
        }
        LineKind::Action { nick, text } => {
            let mut spans = vec![
                span(nick.as_str()).color(nick_color(nick)).font(Font { style: font::Style::Italic, ..BOLD }),
                span(" "),
            ];
            spans.extend(formatted_spans(text, c.text, Font { style: font::Style::Italic, ..base_font }, c, settings.show_colors));
            (glyph("•", c.muted, size), rich(spans, size))
        }
        LineKind::Notice { nick, text } => (
            nick_button(format!("-{nick}-"), nick, server, nick_color(nick), size),
            message_body(text, c.text, base_font, size, c, settings.show_colors),
        ),
        LineKind::Redacted { nick } => (
            nick_button(nick.clone(), nick, server, nick_color(nick).scale_alpha(0.6), size),
            event_text(tr("message deleted"), c.muted, size, true),
        ),
        LineKind::Join { nick, mask } => {
            let mask = mask.as_ref().map(|m| format!(" ({m})")).unwrap_or_default();
            (glyph("→", c.success, size), event(&tr!("{nick}{mask} joined the channel", nick = nick, mask = mask), c, size))
        }
        LineKind::Part { nick, reason } => (
            glyph("←", c.muted, size),
            event(&tr!("{nick} left the channel{reason}", nick = nick, reason = reason_suffix(reason.as_deref())), c, size),
        ),
        LineKind::Quit { nick, reason } => (
            glyph("←", c.danger.scale_alpha(0.8), size),
            event(&tr!("{nick} quit{reason}", nick = nick, reason = reason_suffix(reason.as_deref())), c, size),
        ),
        LineKind::Kick { nick, by, reason } => (
            glyph("✕", c.danger, size),
            event(
                &tr!("{nick} was kicked by {by}{reason}", nick = nick, by = by, reason = reason_suffix(reason.as_deref())),
                c,
                size,
            ),
        ),
        LineKind::Nick { old, new } => (
            glyph("↔", c.muted, size),
            event(&tr!("{old} is now known as {new}", old = old, new = new), c, size),
        ),
        LineKind::Topic { nick, topic } => {
            let text = match (nick, topic) {
                (Some(nick), Some(topic)) => tr!("{nick} changed the topic: {topic}", nick = nick, topic = topic),
                (Some(nick), None) => tr!("{nick} removed the topic", nick = nick),
                (None, Some(topic)) => tr!("Topic: {topic}", topic = topic),
                (None, None) => tr("No topic").to_owned(),
            };
            (glyph("✎", c.accent, size), event(&text, c, size))
        }
        LineKind::Mode { nick, modes } => (
            glyph("⚑", c.muted, size),
            event(&tr!("{nick} set mode {modes}", nick = nick, modes = modes), c, size),
        ),
        LineKind::Info(text) => (glyph("•", c.muted, size), event(text, c, size)),
        LineKind::Error(text) => (
            glyph("!", c.danger, size),
            rich(formatted_spans(text, c.danger, base_font, c, true).into_iter().map(Span::to_static).collect(), size),
        ),
    };

    let mut content = row![].spacing(10).align_y(Alignment::Start);
    if settings.show_timestamps {
        let time = line.time.format(&settings.timestamp_format).to_string();
        content = content.push(
            text(time)
                .size(size - 2.0)
                .color(c.muted.scale_alpha(0.8))
                .font(Font::MONOSPACE)
                .line_height(text::LineHeight::Absolute((size * 1.4).into())),
        );
    }
    let align = match settings.nick_alignment {
        NickAlignment::Right => alignment::Horizontal::Right,
        NickAlignment::Left => alignment::Horizontal::Left,
    };
    content = content.push(
        container(gutter)
            .width(f32::from(settings.nick_column_width))
            .align_x(align)
            .clip(true),
    );
    if line.bot && settings.show_bot_badge {
        content = content.push(
            container(text("BOT").size(size - 4.0).font(BOLD).color(c.on_accent))
                .padding([1, 5])
                .style(theme::badge(c.accent.scale_alpha(0.8))),
        );
    }
    content = content.push(container(body).width(Length::Fill));

    let vertical = if settings.compact { 1 } else { 3 };
    let row = container(content).padding([vertical, 16]).width(Length::Fill);
    if line.highlight {
        row.style(theme::highlighted_line(c)).into()
    } else {
        row.into()
    }
}

fn reason_suffix(reason: Option<&str>) -> String {
    match reason.map(str::trim).filter(|r| !r.is_empty()) {
        Some(reason) => format!(" ({})", format::strip(reason)),
        None => String::new(),
    }
}

fn nick_button<'a>(label: String, nick: &str, server: ServerId, color: Color, size: f32) -> Element<'a, Message> {
    button(
        text(label)
            .size(size)
            .font(BOLD)
            .color(color)
            .wrapping(text::Wrapping::None),
    )
    .padding(0)
    .style(theme::link(color))
    .on_press(Message::OpenQuery(server, nick.to_owned()))
    .into()
}

fn glyph(symbol: &str, color: Color, size: f32) -> Element<'_, Message> {
    text(symbol).size(size).color(color).into()
}

fn event(content: &str, c: Colors, size: f32) -> Element<'static, Message> {
    let spans = linkified(content, c.muted, Font::DEFAULT, c);
    rich(spans, size - 1.0)
}

fn event_text(content: &str, color: Color, size: f32, italic: bool) -> Element<'_, Message> {
    let style = if italic { font::Style::Italic } else { font::Style::Normal };
    text(content)
        .size(size - 1.0)
        .color(color)
        .font(Font { style, ..Font::DEFAULT })
        .into()
}

fn message_body(content: &str, color: Color, font: Font, size: f32, c: Colors, colors: bool) -> Element<'_, Message> {
    rich(formatted_spans(content, color, font, c, colors), size)
}

fn rich(spans: Vec<Span<'_, String>>, size: f32) -> Element<'_, Message> {
    rich_text(spans)
        .on_link_click(Message::OpenUrl)
        .size(size)
        .line_height(text::LineHeight::Relative(1.4))
        .into()
}

/// Turns a text with mIRC codes into styled spans, links included.
fn formatted_spans<'a>(content: &'a str, base: Color, base_font: Font, c: Colors, colors: bool) -> Vec<Span<'a, String>> {
    let mut spans = Vec::new();
    for fragment in format::parse(content) {
        let style = fragment.style;
        // mIRC colors disabled: plain text, only bold, italic... remain.
        let mut foreground = style.foreground.and_then(Colors::irc).filter(|_| colors).unwrap_or(base);
        let mut background = style.background.and_then(Colors::irc).filter(|_| colors);
        if style.reverse && colors {
            let swapped = background.unwrap_or(c.background);
            background = Some(foreground);
            foreground = swapped;
        }
        let font = Font {
            weight: if style.bold { font::Weight::Bold } else { base_font.weight },
            style: if style.italic { font::Style::Italic } else { base_font.style },
            ..if style.monospace { Font::MONOSPACE } else { base_font }
        };
        // Bidirectional controls and invisible characters removed: the
        // displayed text of a link then exactly matches its target.
        let clean = format::sanitize(fragment.text);
        let piece = |range: std::ops::Range<usize>| -> Cow<'a, str> {
            match &clean {
                Cow::Borrowed(text) => Cow::Borrowed(&text[range]),
                Cow::Owned(text) => Cow::Owned(text[range].to_owned()),
            }
        };
        let styled = |piece: Cow<'a, str>| {
            span(piece)
                .color(foreground)
                .font(font)
                .underline(style.underline)
                .strikethrough(style.strikethrough)
                .background_maybe(background)
        };

        let mut cursor = 0;
        for link in irc_text::find_links(&clean) {
            if link.start > cursor {
                spans.push(styled(piece(cursor..link.start)));
            }
            let url = clean[link.start..link.end].to_owned();
            spans.push(styled(piece(link.start..link.end)).color(c.accent).underline(true).link(url));
            cursor = link.end;
        }
        if cursor < clean.len() {
            spans.push(styled(piece(cursor..clean.len())));
        }
    }
    spans
}

/// Spans of an owned text (event lines), with detected links.
fn linkified(content: &str, color: Color, font: Font, c: Colors) -> Vec<Span<'static, String>> {
    formatted_spans(content, color, font, c, true)
        .into_iter()
        .map(Span::to_static)
        .collect()
}
