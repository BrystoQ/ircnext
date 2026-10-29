//! Interface translation.
//!
//! User-facing text is written in English in the code and goes through
//! [`tr`] (or [`tr!`](crate::tr) for text with placeholders). Translations
//! live in `locales/<language>.toml`, keyed by the English text, so a
//! missing entry simply falls back to English.

use std::collections::HashMap;
use std::fmt::{self, Display};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

/// Language chosen in the settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// Follow the operating system.
    #[default]
    System,
    English,
    French,
}

impl Language {
    pub const ALL: [Self; 3] = [Self::System, Self::English, Self::French];

    /// Language actually displayed.
    fn resolve(self) -> Locale {
        match self {
            Self::English => Locale::English,
            Self::French => Locale::French,
            Self::System => {
                let system = sys_locale::get_locale().unwrap_or_default();
                if system.to_ascii_lowercase().starts_with("fr") { Locale::French } else { Locale::English }
            }
        }
    }
}

impl Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Language names are written in their own language.
        f.write_str(match self {
            Self::System => tr("System language"),
            Self::English => "English",
            Self::French => "Français",
        })
    }
}

/// Displayed language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Locale {
    English = 0,
    French = 1,
}

static CURRENT: AtomicU8 = AtomicU8::new(Locale::English as u8);

/// Changes the interface language.
pub fn set_language(language: Language) {
    CURRENT.store(language.resolve() as u8, Ordering::Relaxed);
}

fn current() -> Locale {
    if CURRENT.load(Ordering::Relaxed) == Locale::French as u8 { Locale::French } else { Locale::English }
}

static FRENCH: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    toml::from_str(include_str!("../locales/fr.toml")).expect("locales/fr.toml is valid TOML")
});

/// Translates `english` into the current language.
pub fn tr(english: &'static str) -> &'static str {
    translate(current(), english)
}

fn translate(locale: Locale, english: &'static str) -> &'static str {
    match locale {
        Locale::English => english,
        Locale::French => FRENCH.get(english).map_or(english, String::as_str),
    }
}

/// Replaces each `{name}` of `template` with its value.
pub fn fill(template: &str, values: &[(&str, &dyn Display)]) -> String {
    let mut text = template.to_owned();
    for (name, value) in values {
        text = text.replace(&format!("{{{name}}}"), &value.to_string());
    }
    text
}

/// Translated text with named placeholders: the English template, then
/// `name = value` pairs filling each `{name}`.
#[macro_export]
macro_rules! tr {
    ($english:literal $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::i18n::fill(
            $crate::i18n::tr($english),
            &[$((stringify!($name), &$value as &dyn ::std::fmt::Display)),*],
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_english() {
        assert_eq!(translate(Locale::French, "Settings"), "Réglages");
        assert_eq!(translate(Locale::French, "no translation for this"), "no translation for this");
        assert_eq!(translate(Locale::English, "Settings"), "Settings");
    }

    #[test]
    fn fills_placeholders() {
        assert_eq!(fill("{a} and {b}, {a}", &[("a", &1), ("b", &"two")]), "1 and two, 1");
    }

    /// Every text passed to `tr` or `tr!` in the sources has a French
    /// translation keeping the same placeholders, and every translation
    /// is still used.
    #[test]
    fn french_translation_is_complete() {
        let mut used = std::collections::BTreeSet::new();
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![src];
        while let Some(path) = stack.pop() {
            if path.is_dir() {
                stack.extend(std::fs::read_dir(&path).unwrap().map(|entry| entry.unwrap().path()));
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let source = std::fs::read_to_string(&path).unwrap();
                used.extend(literals(&source));
            }
        }
        let missing: Vec<_> = used.iter().filter(|text| !FRENCH.contains_key(*text)).collect();
        assert!(missing.is_empty(), "missing from locales/fr.toml: {missing:#?}");
        let unused: Vec<_> = FRENCH.keys().filter(|key| !used.contains(*key)).collect();
        assert!(unused.is_empty(), "unused in locales/fr.toml: {unused:#?}");
        for (english, french) in FRENCH.iter() {
            assert_eq!(placeholders(english), placeholders(french), "placeholders of {english:?}");
        }
    }

    /// String literals passed to `tr(` or `tr!(` (escapes `\"` and `\\` only).
    fn literals(source: &str) -> Vec<String> {
        let mut found = Vec::new();
        // Written with `concat!` so that this scan does not find itself.
        for start in [concat!("t", "r("), concat!("t", "r!(")] {
            for (index, _) in source.match_indices(start) {
                // `str(` or `attr(` are other functions.
                if source[..index].ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let Some(rest) = source[index + start.len()..].trim_start().strip_prefix('"') else {
                    continue;
                };
                let mut text = String::new();
                let mut chars = rest.chars();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('n') => text.push('\n'),
                            Some(other) => text.push(other),
                            None => break,
                        },
                        c => text.push(c),
                    }
                }
                found.push(text);
            }
        }
        found
    }

    fn placeholders(text: &str) -> std::collections::BTreeSet<&str> {
        text.match_indices('{')
            .filter_map(|(start, _)| {
                let end = text[start..].find('}')?;
                Some(&text[start..=start + end])
            })
            .collect()
    }
}
