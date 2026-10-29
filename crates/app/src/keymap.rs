//! Configurable keyboard shortcuts.
//!
//! A combination is written `Mod+Shift+K` in the configuration. `Mod` means
//! ⌘ on macOS and Ctrl on Windows and Linux, so the same setting works on
//! both. `Ctrl` means the Control key itself (distinct from ⌘ on macOS, the
//! same as `Mod` elsewhere).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{Key, Modifiers};
use serde::{Deserialize, Serialize};

use crate::i18n::tr;

/// Current system: decides what `Mod` means and the default shortcuts.
pub const MAC: bool = cfg!(target_os = "macos");

/// Group of actions, for display in the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Navigation,
    Display,
    Formatting,
    Application,
}

impl Group {
    pub const ALL: [Self; 4] = [Self::Navigation, Self::Display, Self::Formatting, Self::Application];

    pub fn label(self) -> &'static str {
        match self {
            Self::Navigation => tr("Navigation"),
            Self::Display => tr("Display"),
            Self::Formatting => tr("Formatting (mIRC codes)"),
            Self::Application => tr("Application"),
        }
    }
}

macro_rules! actions {
    ($($variant:ident => $group:ident, $label:expr, mac: [$($mac:literal),*], other: [$($other:literal),*];)*) => {
        /// Action a shortcut can trigger.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum Action {
            $($variant,)*
        }

        impl Action {
            pub const ALL: &[Self] = &[$(Self::$variant,)*];

            pub fn label(self) -> &'static str {
                match self {
                    $(Self::$variant => $label,)*
                }
            }

            pub fn group(self) -> Group {
                match self {
                    $(Self::$variant => Group::$group,)*
                }
            }

            /// Default shortcuts, depending on the system.
            pub fn defaults(self, mac: bool) -> &'static [&'static str] {
                match self {
                    $(Self::$variant => if mac { &[$($mac),*] } else { &[$($other),*] },)*
                }
            }
        }
    };
}

// Conventions taken from HexChat, irssi, web browsers and chat apps (Slack,
// Discord): Ctrl+Tab / Ctrl+Page for tabs, Alt+digit (⌘+digit on macOS),
// ⌘/Ctrl+W to close, mIRC codes on B/I/U/K...
actions! {
    NextBuffer => Navigation, tr("Next tab"),
        mac: ["Ctrl+Tab", "Mod+Alt+Right", "Alt+Down"], other: ["Ctrl+Tab", "Ctrl+PageDown", "Alt+Down"];
    PreviousBuffer => Navigation, tr("Previous tab"),
        mac: ["Ctrl+Shift+Tab", "Mod+Alt+Left", "Alt+Up"], other: ["Ctrl+Shift+Tab", "Ctrl+PageUp", "Alt+Up"];
    NextUnread => Navigation, tr("Next unread tab"),
        mac: ["Alt+Shift+Down"], other: ["Alt+Shift+Down", "Alt+A"];
    PreviousUnread => Navigation, tr("Previous unread tab"),
        mac: ["Alt+Shift+Up"], other: ["Alt+Shift+Up"];
    GoTo1 => Navigation, tr("Go to tab 1"), mac: ["Mod+1"], other: ["Alt+1"];
    GoTo2 => Navigation, tr("Go to tab 2"), mac: ["Mod+2"], other: ["Alt+2"];
    GoTo3 => Navigation, tr("Go to tab 3"), mac: ["Mod+3"], other: ["Alt+3"];
    GoTo4 => Navigation, tr("Go to tab 4"), mac: ["Mod+4"], other: ["Alt+4"];
    GoTo5 => Navigation, tr("Go to tab 5"), mac: ["Mod+5"], other: ["Alt+5"];
    GoTo6 => Navigation, tr("Go to tab 6"), mac: ["Mod+6"], other: ["Alt+6"];
    GoTo7 => Navigation, tr("Go to tab 7"), mac: ["Mod+7"], other: ["Alt+7"];
    GoTo8 => Navigation, tr("Go to tab 8"), mac: ["Mod+8"], other: ["Alt+8"];
    GoTo9 => Navigation, tr("Go to the last tab"), mac: ["Mod+9"], other: ["Alt+9"];
    QuickSwitcher => Navigation, tr("Quick tab switcher"), mac: ["Mod+T", "Mod+J"], other: ["Mod+T", "Mod+J"];
    Search => Navigation, tr("Search in the tab"), mac: ["Mod+F"], other: ["Mod+F"];
    CloseBuffer => Navigation, tr("Close tab"), mac: ["Mod+W"], other: ["Mod+W", "Mod+F4"];
    MarkAllRead => Navigation, tr("Mark all as read"), mac: ["Shift+Escape"], other: ["Shift+Escape"];

    ScrollUp => Display, tr("Page up"), mac: ["PageUp", "Shift+PageUp"], other: ["PageUp", "Shift+PageUp"];
    ScrollDown => Display, tr("Page down"), mac: ["PageDown", "Shift+PageDown"], other: ["PageDown", "Shift+PageDown"];
    ScrollTop => Display, tr("Start of history"), mac: ["Mod+Home", "Mod+Up"], other: ["Mod+Home"];
    ScrollBottom => Display, tr("Latest messages"), mac: ["Mod+End", "Mod+Down"], other: ["Mod+End"];
    ToggleNicklist => Display, tr("Show/hide members"), mac: ["Mod+Shift+U"], other: ["Mod+Shift+U"];
    ToggleSidebar => Display, tr("Show/hide sidebar"), mac: ["Mod+Shift+S"], other: ["Mod+Shift+S"];
    ClearBuffer => Display, tr("Clear tab"), mac: ["Mod+L"], other: ["Mod+L"];
    ZoomIn => Display, tr("Larger text"), mac: ["Mod+Equal", "Mod+Plus"], other: ["Mod+Equal", "Mod+Plus"];
    ZoomOut => Display, tr("Smaller text"), mac: ["Mod+Minus"], other: ["Mod+Minus"];
    ZoomReset => Display, tr("Default text size"), mac: ["Mod+0"], other: ["Mod+0"];

    Bold => Formatting, tr("Bold"), mac: ["Mod+B"], other: ["Mod+B"];
    Italic => Formatting, tr("Italic"), mac: ["Mod+I"], other: ["Mod+I"];
    Underline => Formatting, tr("Underline"), mac: ["Mod+U"], other: ["Mod+U"];
    Strikethrough => Formatting, tr("Strikethrough"), mac: ["Mod+Shift+X"], other: ["Mod+Shift+X"];
    Color => Formatting, tr("Color"), mac: ["Mod+K"], other: ["Mod+K"];
    Reverse => Formatting, tr("Reverse colors"), mac: ["Mod+R"], other: ["Mod+R"];
    ResetFormat => Formatting, tr("Reset formatting"), mac: ["Mod+O"], other: ["Mod+O"];

    AddNetwork => Application, tr("Add a network"), mac: ["Mod+N"], other: ["Mod+N"];
    OpenSettings => Application, tr("Settings"), mac: ["Mod+Comma"], other: ["Mod+Comma", "Mod+P"];
    Quit => Application, tr("Quit"), mac: ["Mod+Q"], other: ["Mod+Q"];
}

impl Action {
    /// Tab number targeted by `GoToN` (9 targets the last one).
    pub fn goto_index(self) -> Option<usize> {
        let index = match self {
            Self::GoTo1 => 1,
            Self::GoTo2 => 2,
            Self::GoTo3 => 3,
            Self::GoTo4 => 4,
            Self::GoTo5 => 5,
            Self::GoTo6 => 6,
            Self::GoTo7 => 7,
            Self::GoTo8 => 8,
            Self::GoTo9 => 9,
            _ => return None,
        };
        Some(index)
    }
}

/// Keys with a fixed role, listed in the settings.
pub fn fixed_keys() -> [(&'static str, &'static str); 4] {
    [
        (tr("Tab / Shift+Tab"), tr("Complete (nicknames, commands, channels)")),
        ("↑ / ↓", tr("Input history, or choose a suggestion")),
        (tr("Enter"), tr("Send, or accept the suggestion")),
        (tr("Esc"), tr("Close the suggestions, the search or the open window")),
    ]
}

/// Key combination.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(clippy::struct_excessive_bools)] // one flag per modifier key
pub struct Combo {
    /// ⌘ on macOS, Ctrl elsewhere.
    pub command: bool,
    /// Control on macOS (always false elsewhere, where it is `command`).
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// Canonical key name (`K`, `1`, `PageDown`, `Comma`...).
    pub key: String,
}

/// Recognized named keys, with their canonical name.
const NAMED_KEYS: &[(Named, &str)] = &[
    (Named::Tab, "Tab"),
    (Named::Enter, "Enter"),
    (Named::Escape, "Escape"),
    (Named::Space, "Space"),
    (Named::Backspace, "Backspace"),
    (Named::Delete, "Delete"),
    (Named::Insert, "Insert"),
    (Named::Home, "Home"),
    (Named::End, "End"),
    (Named::PageUp, "PageUp"),
    (Named::PageDown, "PageDown"),
    (Named::ArrowUp, "Up"),
    (Named::ArrowDown, "Down"),
    (Named::ArrowLeft, "Left"),
    (Named::ArrowRight, "Right"),
    (Named::F1, "F1"),
    (Named::F2, "F2"),
    (Named::F3, "F3"),
    (Named::F4, "F4"),
    (Named::F5, "F5"),
    (Named::F6, "F6"),
    (Named::F7, "F7"),
    (Named::F8, "F8"),
    (Named::F9, "F9"),
    (Named::F10, "F10"),
    (Named::F11, "F11"),
    (Named::F12, "F12"),
];

/// Punctuation, named to avoid any ambiguity with the `+` separator.
const PUNCTUATION: &[(char, &str)] = &[
    (',', "Comma"),
    ('.', "Period"),
    ('/', "Slash"),
    (';', "Semicolon"),
    ('\'', "Quote"),
    ('[', "BracketLeft"),
    (']', "BracketRight"),
    ('\\', "Backslash"),
    ('-', "Minus"),
    ('=', "Equal"),
    ('+', "Plus"),
    ('`', "Backquote"),
];

const DIGITS: [(Code, char); 10] = [
    (Code::Digit0, '0'),
    (Code::Digit1, '1'),
    (Code::Digit2, '2'),
    (Code::Digit3, '3'),
    (Code::Digit4, '4'),
    (Code::Digit5, '5'),
    (Code::Digit6, '6'),
    (Code::Digit7, '7'),
    (Code::Digit8, '8'),
    (Code::Digit9, '9'),
];

impl Combo {
    /// Parses a combination written `Mod+Shift+K` (case-insensitive).
    pub fn parse(text: &str, mac: bool) -> Option<Self> {
        let mut parts: Vec<&str> = text.split('+').map(str::trim).collect();
        let key = canonical_key(parts.pop()?)?;
        let mut combo = Self { command: false, ctrl: false, alt: false, shift: false, key };
        for modifier in parts {
            match modifier.to_ascii_lowercase().as_str() {
                "ctrl" | "control" | "⌃" if mac => combo.ctrl = true,
                "mod" | "cmd" | "command" | "⌘" | "ctrl" | "control" | "⌃" => combo.command = true,
                "alt" | "option" | "opt" | "⌥" => combo.alt = true,
                "shift" | "maj" | "⇧" => combo.shift = true,
                _ => return None,
            }
        }
        Some(combo)
    }

    /// Combination matching a key press (`None` for a lone modifier key or
    /// an unrecognized key).
    pub fn from_key(key: &Key, physical: Physical, modifiers: Modifiers, mac: bool) -> Option<Self> {
        // Digits: physical key, so that ⌘1 also works on AZERTY keyboards.
        let digit = match physical {
            Physical::Code(code) => DIGITS.iter().find(|(c, _)| *c == code).map(|(_, d)| d.to_string()),
            Physical::Unidentified(_) => None,
        };
        let name = match (digit, key.as_ref()) {
            (Some(digit), _) => digit,
            (None, Key::Named(named)) => NAMED_KEYS.iter().find(|(n, _)| *n == named)?.1.to_owned(),
            (None, Key::Character(text)) => canonical_key(text)?,
            (None, Key::Unidentified) => return None,
        };
        Some(Self {
            command: if mac { modifiers.logo() } else { modifiers.control() },
            ctrl: mac && modifiers.control(),
            alt: modifiers.alt(),
            shift: modifiers.shift(),
            key: name,
        })
    }

    /// Form saved in the configuration (`Mod+Shift+K`).
    pub fn to_config(&self) -> String {
        let mut text = String::new();
        for (on, name) in [(self.ctrl, "Ctrl"), (self.command, "Mod"), (self.alt, "Alt"), (self.shift, "Shift")] {
            if on {
                text.push_str(name);
                text.push('+');
            }
        }
        text.push_str(&self.key);
        text
    }

    /// Readable form for the system: `⌘⇧K` on macOS, `Ctrl+Shift+K` elsewhere.
    pub fn display(&self, mac: bool) -> String {
        let key = display_key(&self.key, mac);
        if mac {
            let mut text = String::new();
            for (on, symbol) in [(self.ctrl, '⌃'), (self.alt, '⌥'), (self.shift, '⇧'), (self.command, '⌘')] {
                if on {
                    text.push(symbol);
                }
            }
            text + &key
        } else {
            let mut text = String::new();
            for (on, name) in [(self.command, "Ctrl"), (self.alt, "Alt"), (self.shift, tr("Shift"))] {
                if on {
                    let _ = write!(text, "{name}+");
                }
            }
            text + &key
        }
    }

    /// At least one modifier key other than Shift.
    pub fn has_modifier(&self) -> bool {
        self.command || self.ctrl || self.alt
    }

    /// Bare key, without any modifier.
    pub fn is_bare(&self, key: &str) -> bool {
        !self.has_modifier() && !self.shift && self.key == key
    }
}

/// Canonical name of a key, as written in the configuration.
fn canonical_key(name: &str) -> Option<String> {
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_alphanumeric() {
            return Some(c.to_uppercase().collect());
        }
        return PUNCTUATION.iter().find(|(p, _)| *p == c).map(|(_, n)| (*n).to_owned());
    }
    let known = NAMED_KEYS
        .iter()
        .map(|(_, n)| *n)
        .chain(PUNCTUATION.iter().map(|(_, n)| *n))
        .find(|known| known.eq_ignore_ascii_case(name))?;
    Some(known.to_owned())
}

fn display_key(key: &str, mac: bool) -> String {
    let symbol = match key {
        "Up" => "↑",
        "Down" => "↓",
        "Left" => "←",
        "Right" => "→",
        "Tab" => "⇥",
        "Enter" if mac => "↩",
        "Enter" => tr("Enter"),
        "Escape" if mac => "⎋",
        "Escape" => tr("Esc"),
        "Backspace" => "⌫",
        "Delete" if mac => "⌦",
        "Delete" => tr("Del"),
        "PageUp" if mac => "⇞",
        "PageUp" => "Page↑",
        "PageDown" if mac => "⇟",
        "PageDown" => "Page↓",
        "Home" if mac => "↖",
        "Home" => tr("Home"),
        "End" if mac => "↘",
        "End" => tr("End"),
        "Space" => tr("Space"),
        "Insert" => tr("Ins"),
        other => {
            return PUNCTUATION
                .iter()
                .find(|(_, name)| *name == other)
                .map_or_else(|| other.to_owned(), |(c, _)| c.to_string());
        }
    };
    symbol.to_owned()
}

/// Custom shortcuts (missing actions keep their defaults).
pub type Overrides = BTreeMap<Action, Vec<String>>;

/// Effective shortcuts of an action.
pub fn bindings(overrides: &Overrides, action: Action, mac: bool) -> Vec<Combo> {
    match overrides.get(&action) {
        Some(custom) => custom.iter().filter_map(|text| Combo::parse(text, mac)).collect(),
        None => action.defaults(mac).iter().filter_map(|text| Combo::parse(text, mac)).collect(),
    }
}

/// Action bound to a combination.
pub fn action_for(overrides: &Overrides, combo: &Combo, mac: bool) -> Option<Action> {
    Action::ALL
        .iter()
        .copied()
        .find(|&action| bindings(overrides, action, mac).contains(combo))
}

/// Other actions already using `combo`.
pub fn conflicts(overrides: &Overrides, combo: &Combo, except: Action, mac: bool) -> Vec<Action> {
    Action::ALL
        .iter()
        .copied()
        .filter(|&action| action != except && bindings(overrides, action, mac).contains(combo))
        .collect()
}

/// Whether a combination is reserved for a fixed-role key.
pub fn is_reserved(combo: &Combo) -> bool {
    let bare_or_shift = !combo.has_modifier();
    bare_or_shift && matches!(combo.key.as_str(), "Tab" | "Enter" | "Up" | "Down")
        || combo.is_bare("Escape")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_binding_parses_on_both_systems() {
        for &action in Action::ALL {
            for mac in [true, false] {
                for text in action.defaults(mac) {
                    let combo = Combo::parse(text, mac).unwrap_or_else(|| panic!("{action:?}: {text}"));
                    assert!(!is_reserved(&combo), "{action:?}: {text} is reserved");
                }
            }
        }
    }

    #[test]
    fn default_bindings_do_not_conflict() {
        let none = Overrides::new();
        for mac in [true, false] {
            for &action in Action::ALL {
                for combo in bindings(&none, action, mac) {
                    assert_eq!(conflicts(&none, &combo, action, mac), [], "{} (mac: {mac})", combo.to_config());
                }
            }
        }
    }

    #[test]
    fn mod_means_command_on_mac_and_ctrl_elsewhere() {
        let mac = Combo::parse("Mod+W", true).unwrap();
        assert!(mac.command && !mac.ctrl);
        let ctrl_on_mac = Combo::parse("Ctrl+Tab", true).unwrap();
        assert!(ctrl_on_mac.ctrl && !ctrl_on_mac.command);
        assert_eq!(Combo::parse("Ctrl+W", false), Combo::parse("Mod+W", false));
    }

    #[test]
    fn round_trips_and_displays() {
        let combo = Combo::parse("shift+mod+k", true).unwrap();
        assert_eq!(combo.to_config(), "Mod+Shift+K");
        assert_eq!(combo.display(true), "⇧⌘K");
        assert_eq!(combo.display(false), "Ctrl+Shift+K");
        assert_eq!(Combo::parse("Mod+,", false).unwrap().to_config(), "Mod+Comma");
        assert_eq!(Combo::parse("Mod+Comma", true).unwrap().display(true), "⌘,");
        assert_eq!(Combo::parse("Ctrl+PageDown", false).unwrap().display(false), "Ctrl+Page↓");
        assert_eq!(Combo::parse("Hyper+K", false), None);
        assert_eq!(Combo::parse("Mod+Unknown", false), None);
    }

    #[test]
    fn reads_key_events_portably() {
        let cmd = Modifiers::LOGO;
        let ctrl = Modifiers::CTRL;
        let w = Key::Character("w".into());
        let unidentified = Physical::Unidentified(iced::keyboard::key::NativeCode::Unidentified);
        assert_eq!(Combo::from_key(&w, unidentified, cmd, true), Combo::parse("Mod+W", true));
        assert_eq!(Combo::from_key(&w, unidentified, ctrl, false), Combo::parse("Mod+W", false));
        // AZERTY: the “1” key produces “&”, but stays the physical Digit1 key.
        let ampersand = Key::Character("&".into());
        assert_eq!(Combo::from_key(&ampersand, Physical::Code(Code::Digit1), cmd, true), Combo::parse("Mod+1", true));
        let shift = Key::Named(Named::Shift);
        assert_eq!(Combo::from_key(&shift, unidentified, Modifiers::SHIFT, true), None);
    }

    #[test]
    fn overrides_replace_defaults() {
        let mut overrides = Overrides::new();
        overrides.insert(Action::CloseBuffer, vec!["Mod+Shift+W".into()]);
        let closing = Combo::parse("Mod+Shift+W", false).unwrap();
        assert_eq!(action_for(&overrides, &closing, false), Some(Action::CloseBuffer));
        assert_eq!(action_for(&overrides, &Combo::parse("Mod+W", false).unwrap(), false), None);
        assert_eq!(
            action_for(&overrides, &Combo::parse("Alt+1", false).unwrap(), false),
            Some(Action::GoTo1)
        );
    }
}
