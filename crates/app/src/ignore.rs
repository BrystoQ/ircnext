//! Ignore list: nicknames or `nick!ident@host` masks with wildcards, for
//! some kinds of messages or all, for a while or for good (mIRC's
//! `/ignore -pcntid -uN`).

use ircnext_client::proto::Source;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Kinds of messages an ignore can cover, with mIRC's letters: private,
/// channel, notice, CTCP, invitation, DCC.
pub const KINDS: &str = "pcntid";

/// An ignored mask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoreEntry {
    pub mask: String,
    /// Letters of [`KINDS`] covered; empty: everything.
    pub kinds: String,
    /// End of a temporary ignore (Unix time).
    pub until: Option<i64>,
}

impl IgnoreEntry {
    /// Everything, for good.
    pub fn new(mask: impl Into<String>) -> Self {
        Self { mask: mask.into(), kinds: String::new(), until: None }
    }

    pub fn is_active(&self, now: i64) -> bool {
        self.until.is_none_or(|until| until > now)
    }

    pub fn covers(&self, kind: char) -> bool {
        self.kinds.is_empty() || self.kinds.contains(kind)
    }
}

impl From<&str> for IgnoreEntry {
    fn from(mask: &str) -> Self {
        Self::new(mask)
    }
}

impl PartialEq<&str> for IgnoreEntry {
    fn eq(&self, mask: &&str) -> bool {
        self.mask == *mask
    }
}

impl PartialEq<String> for IgnoreEntry {
    fn eq(&self, mask: &String) -> bool {
        self.mask == *mask
    }
}

/// A plain mask stays a plain string in `config.toml`.
impl Serialize for IgnoreEntry {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Full<'a> {
            mask: &'a str,
            #[serde(skip_serializing_if = "str::is_empty")]
            kinds: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            until: Option<i64>,
        }
        if self.kinds.is_empty() && self.until.is_none() {
            serializer.serialize_str(&self.mask)
        } else {
            Full { mask: &self.mask, kinds: &self.kinds, until: self.until }.serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for IgnoreEntry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Mask(String),
            Full {
                mask: String,
                #[serde(default)]
                kinds: String,
                #[serde(default)]
                until: Option<i64>,
            },
        }
        Ok(match Repr::deserialize(deserializer)? {
            Repr::Mask(mask) => Self::new(mask),
            Repr::Full { mask, kinds, until } => {
                let kinds = KINDS.chars().filter(|kind| kinds.contains(*kind)).collect();
                Self { mask, kinds, until }
            }
        })
    }
}

/// The entries of `masks` (a list typed in the settings), keeping the kinds
/// and duration of those already in `previous`.
pub fn from_masks(masks: Vec<String>, previous: &[IgnoreEntry]) -> Vec<IgnoreEntry> {
    masks
        .into_iter()
        .map(|mask| match previous.iter().find(|entry| normalize(&entry.mask) == normalize(&mask)) {
            Some(entry) => IgnoreEntry { mask, ..entry.clone() },
            None => IgnoreEntry::new(mask),
        })
        .collect()
}

/// Whether a message of `kind` (a letter of [`KINDS`]) from `source` is
/// ignored at `now`.
pub fn is_ignored(entries: &[IgnoreEntry], source: &Source, kind: char, now: i64) -> bool {
    entries.iter().any(|entry| entry.is_active(now) && entry.covers(kind) && matches(&entry.mask, source))
}

/// Whether `source` matches `mask`.
///
/// A mask without `!` or `@` is a nickname (`bob` is the same as `bob!*@*`).
/// The comparison is case-insensitive.
pub fn matches(mask: &str, source: &Source) -> bool {
    let full = format!(
        "{}!{}@{}",
        source.name,
        source.user.as_deref().unwrap_or("*"),
        source.host.as_deref().unwrap_or("*")
    )
    .to_lowercase();
    glob(&normalize(mask), &full)
}

/// Full, lowercase form of a mask.
pub fn normalize(mask: &str) -> String {
    let mask = mask.trim().to_lowercase();
    match (mask.contains('!'), mask.contains('@')) {
        (false, false) => format!("{mask}!*@*"),
        (false, true) => format!("*!{mask}"),
        (true, false) => format!("{mask}@*"),
        (true, true) => mask,
    }
}

/// Matching with `*` (any sequence) and `?` (one character) wildcards.
fn glob(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0, 0);
    // Last star met and the matching position in the text.
    let mut backtrack: Option<(usize, usize)> = None;
    while t < text.len() {
        match pattern.get(p) {
            Some('*') => {
                backtrack = Some((p, t));
                p += 1;
            }
            Some(&c) if c == '?' || c == text[t] => {
                p += 1;
                t += 1;
            }
            _ => match backtrack {
                Some((star, matched)) => {
                    p = star + 1;
                    t = matched + 1;
                    backtrack = Some((star, matched + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(text: &str) -> Source {
        Source::parse(text)
    }

    #[derive(Serialize, Deserialize)]
    struct List {
        ignored: Vec<IgnoreEntry>,
    }

    #[test]
    fn kinds_and_durations() {
        let entries = vec![
            IgnoreEntry { mask: "noisy".into(), kinds: "cn".into(), until: None },
            IgnoreEntry { mask: "brief".into(), kinds: String::new(), until: Some(100) },
        ];
        assert!(is_ignored(&entries, &source("noisy!u@h"), 'c', 0));
        assert!(!is_ignored(&entries, &source("noisy!u@h"), 'p', 0), "private messages still come");
        assert!(is_ignored(&entries, &source("brief!u@h"), 'd', 99));
        assert!(!is_ignored(&entries, &source("brief!u@h"), 'd', 100), "expired");

        // A plain mask is stored as a string, the others as tables.
        let text = toml::to_string(&List { ignored: vec![IgnoreEntry::new("troll"), entries[0].clone(), entries[1].clone()] }).unwrap();
        assert!(text.contains(r#""troll""#), "{text}");
        let read: List = toml::from_str(&text).unwrap();
        assert_eq!(read.ignored, [IgnoreEntry::new("troll"), entries[0].clone(), entries[1].clone()]);
        let old: List = toml::from_str(r#"ignored = ["a", { mask = "b", kinds = "xn" }]"#).unwrap();
        assert_eq!(read.ignored[0], "troll");
        assert_eq!(old.ignored[1].kinds, "n", "unknown kinds dropped");

        let kept = from_masks(vec!["NOISY".into(), "new".into()], &entries);
        assert_eq!(kept[0].kinds, "cn", "edited in the settings: kinds kept");
        assert_eq!(kept[1], IgnoreEntry::new("new"));
    }

    #[test]
    fn matches_nicks_and_masks() {
        let masks: Vec<IgnoreEntry> = ["Spammer", "*!*@*.evil.example", "bot?!ident@*"].map(IgnoreEntry::new).to_vec();
        let is_ignored = |masks: &[IgnoreEntry], source: &Source| is_ignored(masks, source, 'p', 0);
        assert!(is_ignored(&masks, &source("spammer!u@h")));
        assert!(is_ignored(&masks, &source("anyone!x@host.evil.example")));
        assert!(is_ignored(&masks, &source("bot1!ident@somewhere")));
        assert!(!is_ignored(&masks, &source("bot12!ident@somewhere")));
        assert!(!is_ignored(&masks, &source("alice!a@good.example")));
        assert!(!is_ignored(&[], &source("alice!a@h")));
    }

    #[test]
    fn globs_backtrack() {
        assert!(glob("*a*b", "xaxxab"));
        assert!(glob("a*", "a"));
        assert!(!glob("a*b", "ac"));
        assert!(glob("***", ""));
    }

    #[test]
    fn normalizes_partial_masks() {
        assert_eq!(normalize(" Bob "), "bob!*@*");
        assert_eq!(normalize("*@host"), "*!*@host");
        assert_eq!(normalize("bob!ident"), "bob!ident@*");
    }
}
