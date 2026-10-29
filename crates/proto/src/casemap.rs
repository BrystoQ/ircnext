//! Comparison of nicknames and channels according to the server's `CASEMAPPING`.

/// Case folding rule announced by the server.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum CaseMapping {
    /// Only ASCII letters are folded.
    Ascii,
    /// ASCII plus `[]\~` ↔ `{}|^` (historical default).
    #[default]
    Rfc1459,
    /// Like `rfc1459`, without `~` ↔ `^`.
    Rfc1459Strict,
}

impl CaseMapping {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "ascii" => Some(Self::Ascii),
            "rfc1459" => Some(Self::Rfc1459),
            "rfc1459-strict" | "strict-rfc1459" => Some(Self::Rfc1459Strict),
            _ => None,
        }
    }

    pub fn fold_char(self, c: char) -> char {
        match (self, c) {
            (_, 'A'..='Z') => c.to_ascii_lowercase(),
            (Self::Rfc1459 | Self::Rfc1459Strict, '[') => '{',
            (Self::Rfc1459 | Self::Rfc1459Strict, ']') => '}',
            (Self::Rfc1459 | Self::Rfc1459Strict, '\\') => '|',
            (Self::Rfc1459, '~') => '^',
            _ => c,
        }
    }

    /// Canonical form of a name, usable as a map key.
    pub fn fold(self, s: &str) -> String {
        s.chars().map(|c| self.fold_char(c)).collect()
    }

    pub fn eq(self, a: &str, b: &str) -> bool {
        a.chars().count() == b.chars().count()
            && a.chars().zip(b.chars()).all(|(x, y)| self.fold_char(x) == self.fold_char(y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_according_to_mapping() {
        assert_eq!(CaseMapping::Rfc1459.fold("Nick[A]~\\"), "nick{a}^|");
        assert_eq!(CaseMapping::Rfc1459Strict.fold("Nick[A]~"), "nick{a}~");
        assert_eq!(CaseMapping::Ascii.fold("Nick[A]"), "nick[a]");
    }

    #[test]
    fn compares_names() {
        assert!(CaseMapping::Rfc1459.eq("Foo[1]", "fOO{1}"));
        assert!(!CaseMapping::Ascii.eq("Foo[1]", "foo{1}"));
        assert!(!CaseMapping::Ascii.eq("foo", "fooo"));
    }

    #[test]
    fn parses_names() {
        assert_eq!(CaseMapping::parse("ASCII"), Some(CaseMapping::Ascii));
        assert_eq!(CaseMapping::parse("rfc7613"), None);
    }
}
