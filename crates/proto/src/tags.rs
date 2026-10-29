//! IRCv3 message tags (`@key=value;key2 ...`).
//!
//! Specification: <https://ircv3.net/specs/extensions/message-tags>

use std::fmt;

/// Ordered set of tags attached to a message.
///
/// A tag without a value and a tag with an empty value are equivalent
/// according to the specification: both are represented by an empty string.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tags(Vec<(String, String)>);

impl Tags {
    /// Parses the tags part of a line, without the leading `@`.
    pub fn parse(raw: &str) -> Self {
        let tags = raw
            .split(';')
            .filter(|tag| !tag.is_empty())
            .map(|tag| match tag.split_once('=') {
                Some((key, value)) => (key.to_owned(), unescape(value)),
                None => (tag.to_owned(), String::new()),
            })
            .collect();
        Self(tags)
    }

    /// Value of the `key` tag, if present.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, value)| value.as_str())
    }

    /// Adds or replaces a tag.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let value = value.into();
        match self.0.iter_mut().find(|(k, _)| *k == key) {
            Some(entry) => entry.1 = value,
            None => self.0.push((key, value)),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

impl fmt::Display for Tags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, (key, value)) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(";")?;
            }
            f.write_str(key)?;
            if !value.is_empty() {
                f.write_str("=")?;
                escape(value, f)?;
            }
        }
        Ok(())
    }
}

fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        // A trailing `\` is ignored; an unknown escape gives the character itself.
        match chars.next() {
            Some(':') => out.push(';'),
            Some('s') => out.push(' '),
            Some('r') => out.push('\r'),
            Some('n') => out.push('\n'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

fn escape(value: &str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    for c in value.chars() {
        match c {
            ';' => f.write_str("\\:")?,
            ' ' => f.write_str("\\s")?,
            '\\' => f.write_str("\\\\")?,
            '\r' => f.write_str("\\r")?,
            '\n' => f.write_str("\\n")?,
            c => fmt::Write::write_char(f, c)?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_values_and_flags() {
        let tags = Tags::parse("time=2026-01-01T00:00:00.000Z;+draft/reply=abc;flag");
        assert_eq!(tags.get("time"), Some("2026-01-01T00:00:00.000Z"));
        assert_eq!(tags.get("+draft/reply"), Some("abc"));
        assert_eq!(tags.get("flag"), Some(""));
        assert_eq!(tags.get("absent"), None);
    }

    #[test]
    fn unescapes_values() {
        let tags = Tags::parse(r"a=x\:y\sz\\w\r\n;b=trail\;c=\q");
        assert_eq!(tags.get("a"), Some("x;y z\\w\r\n"));
        assert_eq!(tags.get("b"), Some("trail"));
        assert_eq!(tags.get("c"), Some("q"));
    }

    #[test]
    fn round_trips() {
        let mut tags = Tags::default();
        tags.insert("msgid", "a b;c\\d");
        tags.insert("flag", "");
        let encoded = tags.to_string();
        assert_eq!(encoded, r"msgid=a\sb\:c\\d;flag");
        assert_eq!(Tags::parse(&encoded), tags);
    }
}
