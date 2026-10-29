//! Tab completion: nicknames, channels and commands.
//!
//! As in HexChat, a nickname completed at the start of the line is followed
//! by `: `, and pressing Tab again moves to the next candidate.

/// Completion session in progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// Text before the completed word.
    head: String,
    matches: Vec<String>,
    index: usize,
    /// Suffix added after the candidate (`": "` at the start of the line).
    suffix: &'static str,
}

impl Completion {
    /// Starts a completion on the last word of `input`.
    ///
    /// `candidates` is walked in the given order (ideally recent speakers
    /// first); the comparison is case-insensitive.
    pub fn start<'a>(input: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<Self> {
        let word_start = input.rfind(' ').map_or(0, |index| index + 1);
        let (head, word) = input.split_at(word_start);
        if word.is_empty() {
            return None;
        }
        let needle = word.to_lowercase();
        let mut matches: Vec<String> = Vec::new();
        for candidate in candidates {
            if candidate.to_lowercase().starts_with(&needle)
                && !matches.iter().any(|m| m.eq_ignore_ascii_case(candidate))
            {
                matches.push(candidate.to_owned());
            }
        }
        if matches.is_empty() {
            return None;
        }
        let is_nick = !word.starts_with(['/', '#', '&']);
        let suffix = if head.is_empty() && is_nick { ": " } else { " " };
        Some(Self {
            head: head.to_owned(),
            matches,
            index: 0,
            suffix,
        })
    }

    /// Full text with the current candidate.
    pub fn text(&self) -> String {
        format!("{}{}{}", self.head, self.matches[self.index], self.suffix)
    }

    /// Moves to the next (or previous) candidate and returns the new text.
    pub fn cycle(&mut self, backwards: bool) -> String {
        let len = self.matches.len();
        self.index = if backwards {
            (self.index + len - 1) % len
        } else {
            (self.index + 1) % len
        };
        self.text()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completes_nick_at_line_start_with_colon() {
        let completion = Completion::start("al", ["bob", "Alice", "alan"]).unwrap();
        assert_eq!(completion.text(), "Alice: ");
    }

    #[test]
    fn completes_inside_sentence_with_space() {
        let completion = Completion::start("thanks bo", ["bob"]).unwrap();
        assert_eq!(completion.text(), "thanks bob ");
    }

    #[test]
    fn cycles_through_matches() {
        let mut completion = Completion::start("a", ["alice", "alan", "bob"]).unwrap();
        assert_eq!(completion.cycle(false), "alan: ");
        assert_eq!(completion.cycle(false), "alice: ");
        assert_eq!(completion.cycle(true), "alan: ");
    }

    #[test]
    fn commands_and_channels_get_a_space() {
        let completion = Completion::start("/jo", ["/join"]).unwrap();
        assert_eq!(completion.text(), "/join ");
        let completion = Completion::start("/join #ru", ["#rust"]).unwrap();
        assert_eq!(completion.text(), "/join #rust ");
    }

    #[test]
    fn no_match_or_empty_word() {
        assert_eq!(Completion::start("zz", ["alice"]), None);
        assert_eq!(Completion::start("hello ", ["alice"]), None);
    }

    #[test]
    fn deduplicates_candidates() {
        let mut completion = Completion::start("a", ["alice", "Alice"]).unwrap();
        assert_eq!(completion.cycle(false), "alice: ");
    }
}
