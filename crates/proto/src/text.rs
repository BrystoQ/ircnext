//! Utilitaires sur le texte des messages : CTCP, découpage, mentions et liens.

use crate::casemap::CaseMapping;

const CTCP_DELIMITER: char = '\x01';

/// Requête ou réponse CTCP extraite d'un `PRIVMSG`/`NOTICE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ctcp<'a> {
    pub command: &'a str,
    pub params: Option<&'a str>,
}

/// Analyse un message CTCP (`\x01ACTION danse\x01`).
pub fn parse_ctcp(text: &str) -> Option<Ctcp<'_>> {
    let inner = text.strip_prefix(CTCP_DELIMITER)?;
    let inner = inner.strip_suffix(CTCP_DELIMITER).unwrap_or(inner);
    let (command, params) = match inner.split_once(' ') {
        Some((command, params)) => (command, Some(params)),
        None => (inner, None),
    };
    (!command.is_empty()).then_some(Ctcp { command, params })
}

/// Encode un message CTCP.
pub fn ctcp(command: &str, params: Option<&str>) -> String {
    match params {
        Some(params) => format!("{CTCP_DELIMITER}{command} {params}{CTCP_DELIMITER}"),
        None => format!("{CTCP_DELIMITER}{command}{CTCP_DELIMITER}"),
    }
}

/// Découpe un texte en morceaux d'au plus `max_bytes` octets, sans couper un
/// caractère UTF-8 et en privilégiant les espaces comme points de coupure.
pub fn split_message(text: &str, max_bytes: usize) -> Vec<&str> {
    assert!(max_bytes >= 4, "max_bytes doit pouvoir contenir un caractère");
    let mut chunks = Vec::new();
    let mut rest = text;
    while rest.len() > max_bytes {
        let mut cut = max_bytes;
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        // On coupe sur le dernier espace s'il n'est pas trop tôt dans le morceau.
        if let Some(space) = rest[..cut].rfind(' ').filter(|&i| i > cut / 2) {
            cut = space;
        }
        chunks.push(&rest[..cut]);
        rest = rest[cut..].strip_prefix(' ').unwrap_or(&rest[cut..]);
    }
    if !rest.is_empty() || chunks.is_empty() {
        chunks.push(rest);
    }
    chunks
}

/// Indique si `text` mentionne `nick` comme mot entier.
pub fn mentions(text: &str, nick: &str, casemapping: CaseMapping) -> bool {
    if nick.is_empty() {
        return false;
    }
    let is_word = |c: char| c.is_alphanumeric() || "_-[]\\`^{}|".contains(c);
    text.split(|c: char| !is_word(c))
        .any(|word| casemapping.eq(word, nick))
}

/// Plage d'octets d'un lien détecté dans un texte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    pub start: usize,
    pub end: usize,
}

/// Détecte les liens `http(s)://` et `www.` dans un texte.
pub fn find_links(text: &str) -> Vec<Link> {
    let mut links = Vec::new();
    let mut offset = 0;
    for word in text.split(' ') {
        let start = offset;
        offset += word.len() + 1;

        let lower = word.to_ascii_lowercase();
        let Some(begin) = ["https://", "http://", "www."]
            .iter()
            .filter_map(|scheme| lower.find(scheme))
            .min()
        else {
            continue;
        };
        let candidate = &word[begin..];
        let trimmed = trim_link_end(candidate);
        if trimmed.len() > "www.".len() {
            links.push(Link {
                start: start + begin,
                end: start + begin + trimmed.len(),
            });
        }
    }
    links
}

/// Adresse sûre à confier au navigateur, ou `None`.
///
/// Seuls `http` et `https` sont acceptés (un lien `www.` devient `https://`) :
/// les autres schémas peuvent lancer des applications locales (`file:`,
/// `smb:`, gestionnaires d'URL personnalisés). Les caractères qu'un shell ou
/// un lanceur pourrait interpréter sont refusés.
pub fn safe_url(link: &str) -> Option<String> {
    let lower = link.to_ascii_lowercase();
    let url = if lower.starts_with("https://") || lower.starts_with("http://") {
        link.to_owned()
    } else if lower.starts_with("www.") {
        format!("https://{link}")
    } else {
        return None;
    };
    let rest = &url[url.find("://")? + 3..];
    let host_present = rest.chars().next().is_some_and(|c| c != '/' && c != '?' && c != '#');
    let forbidden = |c: char| {
        c.is_whitespace()
            || c.is_control()
            || matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
            || "\"'<>`\\^{}|".contains(c)
    };
    (host_present && !url.contains(forbidden) && url.len() <= 2048).then_some(url)
}

/// Retire la ponctuation finale qui ne fait généralement pas partie du lien,
/// en conservant les parenthèses équilibrées (`https://fr.wikipedia.org/wiki/X_(Y)`).
fn trim_link_end(link: &str) -> &str {
    let mut end = link.len();
    loop {
        let Some(last) = link[..end].chars().next_back() else {
            return "";
        };
        let strip = match last {
            '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' | '>' | ']' | '}' => true,
            ')' => link[..end].matches('(').count() < link[..end].matches(')').count(),
            _ => false,
        };
        if !strip {
            return &link[..end];
        }
        end -= last.len_utf8();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_safe_web_urls() {
        assert_eq!(safe_url("https://docs.rs/tokio").as_deref(), Some("https://docs.rs/tokio"));
        assert_eq!(safe_url("HTTP://Example.org/a?b=c#d").as_deref(), Some("HTTP://Example.org/a?b=c#d"));
        assert_eq!(safe_url("www.rust-lang.org").as_deref(), Some("https://www.rust-lang.org"));
        for bad in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "smb://share/x",
            "-a Calculator",
            "https://",
            "https:///chemin",
            "https://a.org/\"&calc",
            "https://a.org/x|y",
            "https://a.org/\u{202E}x",
        ] {
            assert_eq!(safe_url(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn parses_ctcp() {
        assert_eq!(
            parse_ctcp("\x01ACTION waves\x01"),
            Some(Ctcp { command: "ACTION", params: Some("waves") })
        );
        assert_eq!(parse_ctcp("\x01VERSION"), Some(Ctcp { command: "VERSION", params: None }));
        assert_eq!(parse_ctcp("hello"), None);
        assert_eq!(parse_ctcp("\x01\x01"), None);
    }

    #[test]
    fn encodes_ctcp() {
        assert_eq!(ctcp("ACTION", Some("waves")), "\x01ACTION waves\x01");
        assert_eq!(ctcp("VERSION", None), "\x01VERSION\x01");
    }

    #[test]
    fn splits_on_spaces_and_char_boundaries() {
        assert_eq!(split_message("hello", 10), ["hello"]);
        assert_eq!(split_message("", 10), [""]);
        assert_eq!(split_message("aaaa bbbb cccc", 10), ["aaaa bbbb", "cccc"]);
        assert_eq!(split_message("ééééé", 5), ["éé", "éé", "é"]);
        let long = "x".repeat(25);
        assert_eq!(split_message(&long, 10).len(), 3);
    }

    #[test]
    fn detects_mentions() {
        let cm = CaseMapping::Rfc1459;
        assert!(mentions("hey Alice, ça va ?", "alice", cm));
        assert!(mentions("alice: ping", "Alice", cm));
        assert!(!mentions("malice", "alice", cm));
        assert!(mentions("ping [bot]", "{BOT}", cm));
    }

    #[test]
    fn finds_links() {
        let text = "voir https://example.com/a?b=c, et (www.rust-lang.org).";
        let links: Vec<_> = find_links(text).iter().map(|l| &text[l.start..l.end]).collect();
        assert_eq!(links, ["https://example.com/a?b=c", "www.rust-lang.org"]);
    }

    #[test]
    fn keeps_balanced_parentheses() {
        let text = "https://fr.wikipedia.org/wiki/Rust_(langage)";
        let links = find_links(text);
        assert_eq!(&text[links[0].start..links[0].end], text);
    }
}
