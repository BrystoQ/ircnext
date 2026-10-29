//! Color palette and widget styles.

use iced::widget::{button, container, pick_list, scrollable, text_input};
use iced::{Background, Border, Color, Shadow, Theme, Vector};
use ircnext_client::proto::format;

use crate::settings::{HexColor, ThemeMode};

/// Radius of rounded corners.
pub const RADIUS: f32 = 8.0;

/// Interface colors, derived from the light/dark mode and the accent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colors {
    pub dark: bool,
    pub background: Color,
    pub sidebar: Color,
    pub surface: Color,
    pub hover: Color,
    pub border: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    /// Text readable on an accent background.
    pub on_accent: Color,
    pub danger: Color,
    pub warning: Color,
    pub success: Color,
}

impl Colors {
    /// Colors of the `mode` theme; `system_dark` resolves the automatic mode.
    pub fn new(mode: ThemeMode, system_dark: bool, accent: HexColor) -> Self {
        let [r, g, b] = accent.0;
        let accent = Color::from_rgb8(r, g, b);
        let on_accent = if luminance(accent) > 0.55 { Color::BLACK } else { Color::WHITE };
        // Background, sidebar, surface, hover, border, text, secondary,
        // danger, warning, success.
        let (dark, [background, sidebar, surface, hover, border, text, muted, danger, warning, success]) = match mode {
            ThemeMode::System if !system_dark => LIGHT,
            ThemeMode::System | ThemeMode::Dark => GRAPHITE,
            ThemeMode::Midnight => MIDNIGHT,
            ThemeMode::Oled => OLED,
            ThemeMode::Nord => NORD,
            ThemeMode::Mocha => MOCHA,
            ThemeMode::Light => LIGHT,
            ThemeMode::Sepia => SEPIA,
        };
        Self {
            dark,
            background: hex(background),
            sidebar: hex(sidebar),
            surface: hex(surface),
            hover: hex(hover),
            border: hex(border),
            text: hex(text),
            muted: hex(muted),
            accent,
            on_accent,
            danger: hex(danger),
            warning: hex(warning),
            success: hex(success),
        }
    }

    /// Matching iced theme, for widgets without an explicit style.
    pub fn theme(&self) -> Theme {
        Theme::custom(
            "ircnext",
            iced::theme::Palette {
                background: self.background,
                text: self.text,
                primary: self.accent,
                success: self.success,
                warning: self.warning,
                danger: self.danger,
            },
        )
    }

    /// Stable color associated with a nickname.
    pub fn nick(&self, nick: &str) -> Color {
        // FNV-1a: fast, stable from one run to the next.
        let hash = nick
            .to_lowercase()
            .bytes()
            .fold(0x811c_9dc5_u32, |hash, byte| (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193));
        let hue = f32::from(u16::try_from(hash % 360).expect("< 360"));
        let lightness = if self.dark { 0.70 } else { 0.42 };
        hsl(hue, 0.62, lightness)
    }

    /// mIRC color adapted for display.
    pub fn irc(color: format::Color) -> Option<Color> {
        color.rgb().map(|(r, g, b)| Color::from_rgb8(r, g, b))
    }

    pub fn highlight_background(&self) -> Color {
        self.accent.scale_alpha(if self.dark { 0.14 } else { 0.10 })
    }
}

/// Base palettes, the accent being chosen separately.
#[allow(clippy::unreadable_literal)] // color codes in #rrggbb format
mod palettes {
    pub type Palette = (bool, [u32; 10]);

    pub const GRAPHITE: Palette = (
        true,
        [0x17181c, 0x111216, 0x1f2026, 0x272930, 0x2a2c33, 0xe6e7eb, 0x8b8e98, 0xf06a6a, 0xe8b35a, 0x4cc38a],
    );
    pub const MIDNIGHT: Palette = (
        true,
        [0x0f1420, 0x0a0e18, 0x161c2b, 0x1d2538, 0x222b3f, 0xe3e8f4, 0x7f8aa3, 0xf26d78, 0xe9b45c, 0x4fc79a],
    );
    pub const OLED: Palette = (
        true,
        [0x000000, 0x0a0a0a, 0x121212, 0x1c1c1c, 0x242424, 0xededed, 0x8a8a8a, 0xff6b6b, 0xf0b85a, 0x4fd18f],
    );
    pub const NORD: Palette = (
        true,
        [0x2e3440, 0x272c36, 0x3b4252, 0x434c5e, 0x3f4757, 0xeceff4, 0x939db0, 0xd8737c, 0xebcb8b, 0xa3be8c],
    );
    pub const MOCHA: Palette = (
        true,
        [0x1e1a17, 0x171310, 0x28221e, 0x322a25, 0x3a312b, 0xece2d8, 0x9b8d80, 0xe57373, 0xe0a95b, 0x8bbf7a],
    );
    pub const LIGHT: Palette = (
        false,
        [0xffffff, 0xf4f5f7, 0xf0f1f4, 0xe6e8ec, 0xe1e3e8, 0x1d1f24, 0x6b6f7a, 0xd64545, 0xb7791f, 0x2f9e6b],
    );
    pub const SEPIA: Palette = (
        false,
        [0xf6f0e4, 0xede5d4, 0xeae1cf, 0xe0d5bf, 0xd9cdb5, 0x3b3226, 0x6b5e4c, 0xb8433a, 0xa26b14, 0x4f7f3a],
    );
}

use palettes::{GRAPHITE, LIGHT, MIDNIGHT, MOCHA, NORD, OLED, SEPIA};

fn hex(value: u32) -> Color {
    let [_, r, g, b] = value.to_be_bytes();
    Color::from_rgb8(r, g, b)
}

fn luminance(color: Color) -> f32 {
    0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b
}

#[allow(clippy::many_single_char_names)] // usual notation of the HSL conversion
fn hsl(hue: f32, saturation: f32, lightness: f32) -> Color {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let segment = hue / 60.0;
    let x = chroma * (1.0 - (segment % 2.0 - 1.0).abs());
    let (r, g, b) = match segment {
        s if s < 1.0 => (chroma, x, 0.0),
        s if s < 2.0 => (x, chroma, 0.0),
        s if s < 3.0 => (0.0, chroma, x),
        s if s < 4.0 => (0.0, x, chroma),
        s if s < 5.0 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = lightness - chroma / 2.0;
    Color::from_rgb(r + m, g + m, b + m)
}

// --- Containers -------------------------------------------------------------

pub fn panel(background: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(background.into()),
        ..container::Style::default()
    }
}

pub fn bordered(c: Colors, background: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(background.into()),
        border: Border { color: c.border, width: 1.0, radius: 0.0.into() },
        ..container::Style::default()
    }
}

pub fn card(c: Colors) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(c.background.into()),
        text_color: Some(c.text),
        border: Border { color: c.border, width: 1.0, radius: (RADIUS * 1.5).into() },
        shadow: Shadow {
            color: Color::BLACK.scale_alpha(0.35),
            offset: Vector::new(0.0, 12.0),
            blur_radius: 32.0,
        },
        ..container::Style::default()
    }
}

pub fn backdrop(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::BLACK.scale_alpha(0.45).into()),
        ..container::Style::default()
    }
}

/// Card grouping settings.
pub fn group(c: Colors) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(c.surface.into()),
        border: Border { color: c.border, width: 1.0, radius: (RADIUS + 2.0).into() },
        ..container::Style::default()
    }
}

pub fn highlighted_line(c: Colors) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(c.highlight_background().into()),
        border: Border { color: c.accent, width: 0.0, radius: 4.0.into() },
        ..container::Style::default()
    }
}

/// Thumbnail of a theme in the settings.
pub fn preview(p: Colors) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(p.background.into()),
        border: Border { color: p.border, width: 1.0, radius: 6.0.into() },
        ..container::Style::default()
    }
}

/// Badge (unread counter).
pub fn badge(background: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(background.into()),
        border: Border { radius: 999.0.into(), ..Border::default() },
        ..container::Style::default()
    }
}

// --- Buttons ----------------------------------------------------------------

/// List item (sidebar tab, member...).
pub fn list_item(c: Colors, selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let background = match status {
            _ if selected => Some(c.accent.scale_alpha(0.18).into()),
            button::Status::Hovered | button::Status::Pressed => Some(c.hover.into()),
            _ => None,
        };
        button::Style {
            background,
            text_color: c.text,
            border: Border { radius: 6.0.into(), ..Border::default() },
            ..button::Style::default()
        }
    }
}

/// Discreet button (header icons).
pub fn ghost(c: Colors) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: matches!(status, button::Status::Hovered | button::Status::Pressed)
            .then(|| c.hover.into()),
        text_color: if matches!(status, button::Status::Disabled) { c.muted.scale_alpha(0.5) } else { c.muted },
        border: Border { radius: 6.0.into(), ..Border::default() },
        ..button::Style::default()
    }
}

pub fn primary(c: Colors) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let background = match status {
            button::Status::Hovered => lighten(c.accent, 0.08),
            button::Status::Pressed => lighten(c.accent, -0.08),
            button::Status::Disabled => c.accent.scale_alpha(0.4),
            button::Status::Active => c.accent,
        };
        button::Style {
            background: Some(background.into()),
            text_color: c.on_accent,
            border: Border { radius: RADIUS.into(), ..Border::default() },
            ..button::Style::default()
        }
    }
}

pub fn secondary(c: Colors) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: Some(
            if matches!(status, button::Status::Hovered | button::Status::Pressed) { c.hover } else { c.surface }
                .into(),
        ),
        text_color: c.text,
        border: Border { color: c.border, width: 1.0, radius: RADIUS.into() },
        ..button::Style::default()
    }
}

pub fn danger(c: Colors) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: matches!(status, button::Status::Hovered | button::Status::Pressed)
            .then(|| c.danger.scale_alpha(0.15).into()),
        text_color: c.danger,
        border: Border { color: c.danger.scale_alpha(0.5), width: 1.0, radius: RADIUS.into() },
        ..button::Style::default()
    }
}

/// Theme choice card.
pub fn theme_card(c: Colors, selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: matches!(status, button::Status::Hovered | button::Status::Pressed)
            .then(|| c.hover.into()),
        text_color: c.text,
        border: Border {
            color: if selected { c.accent } else { c.border },
            width: if selected { 2.0 } else { 1.0 },
            radius: RADIUS.into(),
        },
        ..button::Style::default()
    }
}

/// Clickable color swatch (accent choice).
pub fn swatch(color: Color, selected: bool, c: Colors) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: Some(color.into()),
        border: Border {
            color: if selected { c.text } else if matches!(status, button::Status::Hovered) { c.muted } else { Color::TRANSPARENT },
            width: 2.0,
            radius: 999.0.into(),
        },
        ..button::Style::default()
    }
}

/// Link without decoration (clickable nicknames).
pub fn link(color: Color) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: None,
        text_color: if matches!(status, button::Status::Hovered) { lighten(color, 0.12) } else { color },
        ..button::Style::default()
    }
}

fn lighten(color: Color, amount: f32) -> Color {
    Color {
        r: (color.r + amount).clamp(0.0, 1.0),
        g: (color.g + amount).clamp(0.0, 1.0),
        b: (color.b + amount).clamp(0.0, 1.0),
        a: color.a,
    }
}

// --- Fields and scrolling ---------------------------------------------------

pub fn input(c: Colors) -> impl Fn(&Theme, text_input::Status) -> text_input::Style {
    move |_, status| text_input::Style {
        background: Background::Color(c.surface),
        border: Border {
            color: match status {
                text_input::Status::Focused { .. } => c.accent,
                text_input::Status::Hovered => c.muted.scale_alpha(0.6),
                _ => c.border,
            },
            width: 1.0,
            radius: RADIUS.into(),
        },
        icon: c.muted,
        placeholder: c.muted,
        value: c.text,
        selection: c.accent.scale_alpha(0.35),
    }
}

/// Input field placed on a settings card (contrasting background).
pub fn field(c: Colors) -> impl Fn(&Theme, text_input::Status) -> text_input::Style {
    move |theme, status| text_input::Style {
        background: Background::Color(c.background),
        ..input(c)(theme, status)
    }
}

/// Pick list matching the input fields.
pub fn select(c: Colors) -> impl Fn(&Theme, pick_list::Status) -> pick_list::Style {
    move |_, status| pick_list::Style {
        text_color: c.text,
        placeholder_color: c.muted,
        handle_color: c.muted,
        background: Background::Color(c.background),
        border: Border {
            color: if matches!(status, pick_list::Status::Hovered | pick_list::Status::Opened { .. }) {
                c.muted.scale_alpha(0.6)
            } else {
                c.border
            },
            width: 1.0,
            radius: RADIUS.into(),
        },
    }
}

pub fn scrollbar(c: Colors) -> impl Fn(&Theme, scrollable::Status) -> scrollable::Style {
    move |theme, status| {
        let mut style = scrollable::default(theme, status);
        for rail in [&mut style.vertical_rail, &mut style.horizontal_rail] {
            rail.background = None;
            rail.border = Border::default();
            rail.scroller.background = c.muted.scale_alpha(0.35).into();
            rail.scroller.border = Border { radius: 999.0.into(), ..Border::default() };
        }
        style
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nick_colors_are_stable_and_case_insensitive() {
        let colors = Colors::new(ThemeMode::Dark, true, HexColor([0x5b, 0x8c, 0xff]));
        assert_eq!(colors.nick("Alice"), colors.nick("alice"));
        assert_ne!(colors.nick("alice"), colors.nick("bob"));
    }

    #[test]
    fn picks_readable_text_on_accent() {
        assert_eq!(Colors::new(ThemeMode::Dark, true, HexColor([0xff, 0xff, 0x00])).on_accent, Color::BLACK);
        assert_eq!(Colors::new(ThemeMode::Dark, true, HexColor([0x20, 0x30, 0x90])).on_accent, Color::WHITE);
    }

    #[test]
    fn every_theme_keeps_text_readable() {
        for mode in ThemeMode::ALL {
            let c = Colors::new(mode, true, HexColor([0x5b, 0x8c, 0xff]));
            // WCAG contrast, on the relative luminance (linearized channels).
            let relative = |color: Color| {
                let linear = |v: f32| if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
                luminance(Color { r: linear(color.r), g: linear(color.g), b: linear(color.b), a: 1.0 })
            };
            let contrast = |a: Color, b: Color| {
                let (l1, l2) = (relative(a), relative(b));
                (l1.max(l2) + 0.05) / (l1.min(l2) + 0.05)
            };
            assert!(contrast(c.text, c.background) > 7.0, "{mode}: text");
            assert!(contrast(c.muted, c.background) > 4.5, "{mode}: secondary text");
            assert_eq!(c.dark, luminance(c.background) < 0.5, "{mode}");
        }
    }

    #[test]
    fn hsl_primaries() {
        let red = hsl(0.0, 1.0, 0.5);
        assert!((red.r - 1.0).abs() < 1e-6 && red.g.abs() < 1e-6 && red.b.abs() < 1e-6);
    }
}
