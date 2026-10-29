//! Ignore list: nicknames or `nick!ident@host` masks with wildcards.

use ircnext_client::proto::Source;

/// Whether `source` matches one of the masks.
///
/// A mask without `!` or `@` is a nickname (`bob` is the same as `bob!*@*`).
/// The comparison is case-insensitive.
pub fn is_ignored(masks: &[String], source: &Source) -> bool {
    if masks.is_empty() {
        return false;
    }
    let full = format!(
        "{}!{}@{}",
        source.name,
        source.user.as_deref().unwrap_or("*"),
        source.host.as_deref().unwrap_or("*")
    )
    .to_lowercase();
    masks.iter().any(|mask| glob(&normalize(mask), &full))
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

    #[test]
    fn matches_nicks_and_masks() {
        let masks = vec!["Spammer".to_owned(), "*!*@*.evil.example".to_owned(), "bot?!ident@*".to_owned()];
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
