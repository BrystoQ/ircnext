//! User preferences, saved in `config.toml`.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::i18n::{Language, tr};
use crate::keymap::Overrides;

/// Interface theme.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    /// Follows the system theme (Graphite or Light).
    #[default]
    System,
    /// Neutral gray.
    Dark,
    /// Deep navy blue.
    Midnight,
    /// Pure black, ideal on OLED screens.
    Oled,
    /// Cool blue-gray.
    Nord,
    /// Warm brown.
    Mocha,
    Light,
    /// Cream paper, easy on the eyes.
    Sepia,
}

impl ThemeMode {
    pub const ALL: [Self; 8] = [
        Self::System,
        Self::Dark,
        Self::Midnight,
        Self::Oled,
        Self::Nord,
        Self::Mocha,
        Self::Light,
        Self::Sepia,
    ];
}

impl fmt::Display for ThemeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::System => tr("Automatic"),
            Self::Dark => "Graphite",
            Self::Midnight => tr("Midnight"),
            Self::Oled => tr("Pure black"),
            Self::Nord => tr("Nordic"),
            Self::Mocha => tr("Mocha"),
            Self::Light => tr("Light"),
            Self::Sepia => tr("Sepia"),
        })
    }
}

/// Alignment of nicknames in their column.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NickAlignment {
    /// Right, next to the message (HexChat).
    #[default]
    Right,
    Left,
}

impl NickAlignment {
    pub const ALL: [Self; 2] = [Self::Right, Self::Left];
}

impl fmt::Display for NickAlignment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Right => tr("Right"),
            Self::Left => tr("Left"),
        })
    }
}

/// RGB color serialized as `#rrggbb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct HexColor(pub [u8; 3]);

impl HexColor {
    pub fn parse(value: &str) -> Option<Self> {
        let hex = value.trim().strip_prefix('#').unwrap_or(value.trim());
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let value = u32::from_str_radix(hex, 16).ok()?;
        let [_, r, g, b] = value.to_be_bytes();
        Some(Self([r, g, b]))
    }
}

impl TryFrom<String> for HexColor {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value).ok_or_else(|| format!("invalid color: {value:?}"))
    }
}

impl From<HexColor> for String {
    fn from(color: HexColor) -> Self {
        color.to_string()
    }
}

impl fmt::Display for HexColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [r, g, b] = self.0;
        write!(f, "#{r:02x}{g:02x}{b:02x}")
    }
}

/// Accent colors offered in the settings.
pub const ACCENT_PRESETS: [(&str, HexColor); 6] = [
    ("Blue", HexColor([0x5b, 0x8c, 0xff])),
    ("Purple", HexColor([0x9b, 0x6b, 0xff])),
    ("Green", HexColor([0x3c, 0xc4, 0x8a])),
    ("Orange", HexColor([0xf5, 0x93, 0x3d])),
    ("Pink", HexColor([0xf0, 0x5f, 0xa8])),
    ("Turquoise", HexColor([0x2b, 0xb8, 0xc4])),
];

/// All preferences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Settings {
    // Appearance
    /// Interface language.
    pub language: Language,
    pub theme: ThemeMode,
    pub accent: HexColor,
    pub font_size: u16,
    /// Monospace font for messages.
    pub monospace: bool,
    /// Reduced spacing between lines.
    pub compact: bool,
    pub show_sidebar: bool,
    /// Width of the nickname column, in pixels.
    pub nick_column_width: u16,
    pub nick_alignment: NickAlignment,
    /// Status prefix before nicknames (`@`, `+`).
    pub show_nick_prefix: bool,
    /// mIRC colors in messages (otherwise plain text).
    pub show_colors: bool,
    pub show_bot_badge: bool,

    // Chat
    pub show_timestamps: bool,
    /// `strftime` format of timestamps.
    pub timestamp_format: String,
    pub show_joins_parts: bool,
    pub nick_colors: bool,
    pub show_nicklist: bool,
    /// Lines kept per tab.
    pub scrollback: usize,
    /// Words triggering a highlight, besides the nickname.
    pub highlight_words: Vec<String>,
    /// Ignored users: nicknames or `nick!ident@host` masks (`*` and `?`
    /// wildcards).
    pub ignored: Vec<String>,
    /// History lines requested when joining a channel (0: none).
    pub history_lines: u16,
    /// Conversation logs saved to disk.
    pub logging: bool,

    // Notifications
    pub notify_mentions: bool,
    pub notify_queries: bool,
    /// Notify even when the window is in the foreground.
    pub notify_when_focused: bool,
    /// Message content in the notification (otherwise only the sender).
    pub notify_preview: bool,
    pub notify_sound: bool,
    /// Dock icon bounce / taskbar flash.
    pub request_attention: bool,
    /// Number of tabs with an unread mention or private message, in the title.
    pub title_unread_count: bool,

    // Privacy
    /// Automatic replies to CTCP requests (VERSION, PING, TIME).
    pub ctcp_replies: bool,
    /// Reply to CTCP VERSION (empty: ircnext name and version).
    pub ctcp_version: String,
    /// Tells the server which messages were read (`draft/read-marker`).
    pub send_read_markers: bool,
    /// On Tab, ask compatible bots to complete a command (IRCv3
    /// `autocomplete` draft).
    pub bot_completion: bool,
    /// Passwords stored in the system keychain rather than in the file.
    pub use_keychain: bool,

    // Typing indicator (IRCv3 `+typing`)
    pub send_typing: bool,
    pub show_typing: bool,

    // Default identity of new networks
    pub nick: String,
    pub realname: String,
    pub quit_message: String,
    /// Reason given when leaving a channel (empty: none).
    pub part_message: String,

    /// Custom shortcuts; missing actions keep their defaults.
    pub keybindings: Overrides,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::default(),
            theme: ThemeMode::default(),
            accent: ACCENT_PRESETS[0].1,
            font_size: 14,
            monospace: false,
            compact: false,
            show_sidebar: true,
            nick_column_width: 132,
            nick_alignment: NickAlignment::Right,
            show_nick_prefix: true,
            show_colors: true,
            show_bot_badge: true,
            show_timestamps: true,
            timestamp_format: "%H:%M".to_owned(),
            show_joins_parts: true,
            nick_colors: true,
            show_nicklist: true,
            scrollback: 2000,
            highlight_words: Vec::new(),
            ignored: Vec::new(),
            history_lines: 100,
            logging: false,
            notify_mentions: true,
            notify_queries: true,
            notify_when_focused: false,
            notify_preview: true,
            notify_sound: true,
            request_attention: true,
            title_unread_count: true,
            ctcp_replies: true,
            ctcp_version: String::new(),
            send_read_markers: true,
            bot_completion: true,
            use_keychain: true,
            send_typing: true,
            show_typing: true,
            nick: "ircnext".to_owned(),
            realname: "ircnext".to_owned(),
            quit_message: "ircnext".to_owned(),
            part_message: String::new(),
            keybindings: Overrides::new(),
        }
    }
}

impl Settings {
    pub const FONT_SIZES: std::ops::RangeInclusive<u16> = 11..=22;
    pub const SCROLLBACK_RANGE: std::ops::RangeInclusive<usize> = 200..=20_000;
    pub const NICK_COLUMN_WIDTHS: std::ops::RangeInclusive<u16> = 60..=260;
    pub const HISTORY_LINES: std::ops::RangeInclusive<u16> = 0..=1000;

    /// Fixes out-of-range values (hand-edited file).
    pub fn sanitize(&mut self) {
        self.font_size = self
            .font_size
            .clamp(*Self::FONT_SIZES.start(), *Self::FONT_SIZES.end());
        self.scrollback = self
            .scrollback
            .clamp(*Self::SCROLLBACK_RANGE.start(), *Self::SCROLLBACK_RANGE.end());
        if self.timestamp_format.trim().is_empty() || !is_valid_time_format(&self.timestamp_format) {
            self.timestamp_format = Self::default().timestamp_format;
        }
        self.highlight_words.retain(|word| !word.trim().is_empty());
        self.ignored.retain(|mask| !mask.trim().is_empty());
        self.nick_column_width = self
            .nick_column_width
            .clamp(*Self::NICK_COLUMN_WIDTHS.start(), *Self::NICK_COLUMN_WIDTHS.end());
        self.history_lines = self.history_lines.min(*Self::HISTORY_LINES.end());
    }
}

/// Checks that a `strftime` format is usable (chrono panics on display otherwise).
pub fn is_valid_time_format(format: &str) -> bool {
    use chrono::format::{Item, StrftimeItems};
    StrftimeItems::new(format).all(|item| !matches!(item, Item::Error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_colors() {
        assert_eq!(HexColor::parse("#5b8cff"), Some(HexColor([0x5b, 0x8c, 0xff])));
        assert_eq!(HexColor::parse(" 5B8CFF "), Some(HexColor([0x5b, 0x8c, 0xff])));
        assert_eq!(HexColor::parse("#5b8cf"), None);
        assert_eq!(HexColor::parse("#zzzzzz"), None);
        assert_eq!(HexColor([1, 2, 255]).to_string(), "#0102ff");
    }

    #[test]
    fn validates_time_formats() {
        assert!(is_valid_time_format("%H:%M:%S"));
        assert!(is_valid_time_format("[%d/%m %H:%M]"));
        assert!(!is_valid_time_format("%Q"));
    }

    #[test]
    fn sanitizes_out_of_range_values() {
        let mut settings = Settings {
            font_size: 99,
            scrollback: 1,
            timestamp_format: "%Q".into(),
            highlight_words: vec!["  ".into(), "rust".into()],
            ..Settings::default()
        };
        settings.sanitize();
        assert_eq!(settings.font_size, 22);
        assert_eq!(settings.scrollback, 200);
        assert_eq!(settings.timestamp_format, "%H:%M");
        assert_eq!(settings.highlight_words, ["rust"]);
    }
}
