//! Raccourcis clavier configurables.
//!
//! Une combinaison s'écrit `Mod+Shift+K` dans la configuration. `Mod` désigne
//! ⌘ sur macOS et Ctrl sous Windows et Linux, si bien qu'un même réglage
//! convient aux deux. `Ctrl` désigne la touche Contrôle elle-même (distincte
//! de ⌘ sur macOS, confondue avec `Mod` ailleurs).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{Key, Modifiers};
use serde::{Deserialize, Serialize};

/// Système courant : détermine le sens de `Mod` et les raccourcis par défaut.
pub const MAC: bool = cfg!(target_os = "macos");

/// Groupe d'actions, pour l'affichage dans les paramètres.
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
            Self::Navigation => "Navigation",
            Self::Display => "Affichage",
            Self::Formatting => "Mise en forme (codes mIRC)",
            Self::Application => "Application",
        }
    }
}

macro_rules! actions {
    ($($variant:ident => $group:ident, $label:literal, mac: [$($mac:literal),*], other: [$($other:literal),*];)*) => {
        /// Action déclenchable par un raccourci.
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

            /// Raccourcis par défaut, selon le système.
            pub fn defaults(self, mac: bool) -> &'static [&'static str] {
                match self {
                    $(Self::$variant => if mac { &[$($mac),*] } else { &[$($other),*] },)*
                }
            }
        }
    };
}

// Conventions reprises de HexChat, irssi, des navigateurs et des messageries
// (Slack, Discord) : Ctrl+Tab / Ctrl+Page pour les onglets, Alt+chiffre
// (⌘+chiffre sur macOS), ⌘/Ctrl+W pour fermer, codes mIRC sur B/I/U/K...
actions! {
    NextBuffer => Navigation, "Onglet suivant",
        mac: ["Ctrl+Tab", "Mod+Alt+Right", "Alt+Down"], other: ["Ctrl+Tab", "Ctrl+PageDown", "Alt+Down"];
    PreviousBuffer => Navigation, "Onglet précédent",
        mac: ["Ctrl+Shift+Tab", "Mod+Alt+Left", "Alt+Up"], other: ["Ctrl+Shift+Tab", "Ctrl+PageUp", "Alt+Up"];
    NextUnread => Navigation, "Onglet non lu suivant",
        mac: ["Alt+Shift+Down"], other: ["Alt+Shift+Down", "Alt+A"];
    PreviousUnread => Navigation, "Onglet non lu précédent",
        mac: ["Alt+Shift+Up"], other: ["Alt+Shift+Up"];
    GoTo1 => Navigation, "Aller à l'onglet 1", mac: ["Mod+1"], other: ["Alt+1"];
    GoTo2 => Navigation, "Aller à l'onglet 2", mac: ["Mod+2"], other: ["Alt+2"];
    GoTo3 => Navigation, "Aller à l'onglet 3", mac: ["Mod+3"], other: ["Alt+3"];
    GoTo4 => Navigation, "Aller à l'onglet 4", mac: ["Mod+4"], other: ["Alt+4"];
    GoTo5 => Navigation, "Aller à l'onglet 5", mac: ["Mod+5"], other: ["Alt+5"];
    GoTo6 => Navigation, "Aller à l'onglet 6", mac: ["Mod+6"], other: ["Alt+6"];
    GoTo7 => Navigation, "Aller à l'onglet 7", mac: ["Mod+7"], other: ["Alt+7"];
    GoTo8 => Navigation, "Aller à l'onglet 8", mac: ["Mod+8"], other: ["Alt+8"];
    GoTo9 => Navigation, "Aller au dernier onglet", mac: ["Mod+9"], other: ["Alt+9"];
    QuickSwitcher => Navigation, "Recherche rapide d'onglet", mac: ["Mod+T", "Mod+J"], other: ["Mod+T", "Mod+J"];
    Search => Navigation, "Rechercher dans l'onglet", mac: ["Mod+F"], other: ["Mod+F"];
    CloseBuffer => Navigation, "Fermer l'onglet", mac: ["Mod+W"], other: ["Mod+W", "Mod+F4"];
    MarkAllRead => Navigation, "Tout marquer comme lu", mac: ["Shift+Escape"], other: ["Shift+Escape"];

    ScrollUp => Display, "Remonter d'une page", mac: ["PageUp", "Shift+PageUp"], other: ["PageUp", "Shift+PageUp"];
    ScrollDown => Display, "Descendre d'une page", mac: ["PageDown", "Shift+PageDown"], other: ["PageDown", "Shift+PageDown"];
    ScrollTop => Display, "Début de l'historique", mac: ["Mod+Home", "Mod+Up"], other: ["Mod+Home"];
    ScrollBottom => Display, "Derniers messages", mac: ["Mod+End", "Mod+Down"], other: ["Mod+End"];
    ToggleNicklist => Display, "Afficher/masquer les membres", mac: ["Mod+Shift+U"], other: ["Mod+Shift+U"];
    ToggleSidebar => Display, "Afficher/masquer la barre latérale", mac: ["Mod+Shift+S"], other: ["Mod+Shift+S"];
    ClearBuffer => Display, "Effacer l'onglet", mac: ["Mod+L"], other: ["Mod+L"];
    ZoomIn => Display, "Agrandir le texte", mac: ["Mod+Equal", "Mod+Plus"], other: ["Mod+Equal", "Mod+Plus"];
    ZoomOut => Display, "Réduire le texte", mac: ["Mod+Minus"], other: ["Mod+Minus"];
    ZoomReset => Display, "Taille de texte par défaut", mac: ["Mod+0"], other: ["Mod+0"];

    Bold => Formatting, "Gras", mac: ["Mod+B"], other: ["Mod+B"];
    Italic => Formatting, "Italique", mac: ["Mod+I"], other: ["Mod+I"];
    Underline => Formatting, "Souligné", mac: ["Mod+U"], other: ["Mod+U"];
    Strikethrough => Formatting, "Barré", mac: ["Mod+Shift+X"], other: ["Mod+Shift+X"];
    Color => Formatting, "Couleur", mac: ["Mod+K"], other: ["Mod+K"];
    Reverse => Formatting, "Inversion des couleurs", mac: ["Mod+R"], other: ["Mod+R"];
    ResetFormat => Formatting, "Annuler la mise en forme", mac: ["Mod+O"], other: ["Mod+O"];

    AddNetwork => Application, "Ajouter un réseau", mac: ["Mod+N"], other: ["Mod+N"];
    OpenSettings => Application, "Paramètres", mac: ["Mod+Comma"], other: ["Mod+Comma", "Mod+P"];
    Quit => Application, "Quitter", mac: ["Mod+Q"], other: ["Mod+Q"];
}

impl Action {
    /// Numéro d'onglet visé par `GoToN` (le 9 vise le dernier).
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

/// Touches à rôle fixe, rappelées dans les paramètres.
pub const FIXED_KEYS: &[(&str, &str)] = &[
    ("Tab / Maj+Tab", "Compléter (pseudos, commandes, salons)"),
    ("↑ / ↓", "Historique de saisie, ou choix d'une suggestion"),
    ("Entrée", "Envoyer, ou valider la suggestion"),
    ("Échap", "Fermer les suggestions, la recherche ou la fenêtre ouverte"),
];

/// Combinaison de touches.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(clippy::struct_excessive_bools)] // un indicateur par touche de modification
pub struct Combo {
    /// ⌘ sur macOS, Ctrl ailleurs.
    pub command: bool,
    /// Contrôle sur macOS (toujours faux ailleurs, où il se confond avec `command`).
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// Nom canonique de la touche (`K`, `1`, `PageDown`, `Comma`...).
    pub key: String,
}

/// Touches nommées reconnues, avec leur nom canonique.
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

/// Ponctuation, nommée pour éviter toute ambiguïté avec le séparateur `+`.
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
    /// Analyse une combinaison écrite `Mod+Shift+K` (casse indifférente).
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

    /// Combinaison correspondant à un appui de touche (`None` pour une
    /// touche de modification seule ou une touche non reconnue).
    pub fn from_key(key: &Key, physical: Physical, modifiers: Modifiers, mac: bool) -> Option<Self> {
        // Chiffres : touche physique, pour que ⌘1 fonctionne aussi en AZERTY.
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

    /// Forme enregistrée dans la configuration (`Mod+Shift+K`).
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

    /// Forme lisible selon le système : `⌘⇧K` sur macOS, `Ctrl+Maj+K` ailleurs.
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
            for (on, name) in [(self.command, "Ctrl"), (self.alt, "Alt"), (self.shift, "Maj")] {
                if on {
                    let _ = write!(text, "{name}+");
                }
            }
            text + &key
        }
    }

    /// Au moins une touche de modification autre que Maj.
    pub fn has_modifier(&self) -> bool {
        self.command || self.ctrl || self.alt
    }

    /// Touche seule, sans aucune modification.
    pub fn is_bare(&self, key: &str) -> bool {
        !self.has_modifier() && !self.shift && self.key == key
    }
}

/// Nom canonique d'une touche, tel qu'écrit dans la configuration.
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
        "Enter" => "Entrée",
        "Escape" if mac => "⎋",
        "Escape" => "Échap",
        "Backspace" => "⌫",
        "Delete" if mac => "⌦",
        "Delete" => "Suppr",
        "PageUp" if mac => "⇞",
        "PageUp" => "Page↑",
        "PageDown" if mac => "⇟",
        "PageDown" => "Page↓",
        "Home" if mac => "↖",
        "Home" => "Début",
        "End" if mac => "↘",
        "End" => "Fin",
        "Space" => "Espace",
        "Insert" => "Inser",
        other => {
            return PUNCTUATION
                .iter()
                .find(|(_, name)| *name == other)
                .map_or_else(|| other.to_owned(), |(c, _)| c.to_string());
        }
    };
    symbol.to_owned()
}

/// Raccourcis personnalisés (les actions absentes gardent leurs défauts).
pub type Overrides = BTreeMap<Action, Vec<String>>;

/// Raccourcis effectifs d'une action.
pub fn bindings(overrides: &Overrides, action: Action, mac: bool) -> Vec<Combo> {
    match overrides.get(&action) {
        Some(custom) => custom.iter().filter_map(|text| Combo::parse(text, mac)).collect(),
        None => action.defaults(mac).iter().filter_map(|text| Combo::parse(text, mac)).collect(),
    }
}

/// Action associée à une combinaison.
pub fn action_for(overrides: &Overrides, combo: &Combo, mac: bool) -> Option<Action> {
    Action::ALL
        .iter()
        .copied()
        .find(|&action| bindings(overrides, action, mac).contains(combo))
}

/// Autres actions utilisant déjà `combo`.
pub fn conflicts(overrides: &Overrides, combo: &Combo, except: Action, mac: bool) -> Vec<Action> {
    Action::ALL
        .iter()
        .copied()
        .filter(|&action| action != except && bindings(overrides, action, mac).contains(combo))
        .collect()
}

/// Indique si une combinaison est réservée à une touche à rôle fixe.
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
                    let combo = Combo::parse(text, mac).unwrap_or_else(|| panic!("{action:?} : {text}"));
                    assert!(!is_reserved(&combo), "{action:?} : {text} est réservé");
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
                    assert_eq!(conflicts(&none, &combo, action, mac), [], "{} (mac : {mac})", combo.to_config());
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
        assert_eq!(combo.display(false), "Ctrl+Maj+K");
        assert_eq!(Combo::parse("Mod+,", false).unwrap().to_config(), "Mod+Comma");
        assert_eq!(Combo::parse("Mod+Comma", true).unwrap().display(true), "⌘,");
        assert_eq!(Combo::parse("Ctrl+PageDown", false).unwrap().display(false), "Ctrl+Page↓");
        assert_eq!(Combo::parse("Hyper+K", false), None);
        assert_eq!(Combo::parse("Mod+Inconnue", false), None);
    }

    #[test]
    fn reads_key_events_portably() {
        let cmd = Modifiers::LOGO;
        let ctrl = Modifiers::CTRL;
        let w = Key::Character("w".into());
        let unidentified = Physical::Unidentified(iced::keyboard::key::NativeCode::Unidentified);
        assert_eq!(Combo::from_key(&w, unidentified, cmd, true), Combo::parse("Mod+W", true));
        assert_eq!(Combo::from_key(&w, unidentified, ctrl, false), Combo::parse("Mod+W", false));
        // AZERTY : la touche « 1 » produit « & », mais reste la touche physique Digit1.
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
