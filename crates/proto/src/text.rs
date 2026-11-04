//! Utilities on message text: CTCP, splitting, mentions and links.

use crate::casemap::CaseMapping;

const CTCP_DELIMITER: char = '\x01';

/// CTCP request or reply extracted from a `PRIVMSG`/`NOTICE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ctcp<'a> {
    pub command: &'a str,
    pub params: Option<&'a str>,
}

/// Parses a CTCP message (`\x01ACTION dances\x01`).
pub fn parse_ctcp(text: &str) -> Option<Ctcp<'_>> {
    let inner = text.strip_prefix(CTCP_DELIMITER)?;
    let inner = inner.strip_suffix(CTCP_DELIMITER).unwrap_or(inner);
    let (command, params) = match inner.split_once(' ') {
        Some((command, params)) => (command, Some(params)),
        None => (inner, None),
    };
    (!command.is_empty()).then_some(Ctcp { command, params })
}

/// Encodes a CTCP message.
pub fn ctcp(command: &str, params: Option<&str>) -> String {
    match params {
        Some(params) => format!("{CTCP_DELIMITER}{command} {params}{CTCP_DELIMITER}"),
        None => format!("{CTCP_DELIMITER}{command}{CTCP_DELIMITER}"),
    }
}

/// Splits a text into chunks of at most `max_bytes` bytes, without cutting a
/// UTF-8 character and preferring spaces as cut points.
pub fn split_message(text: &str, max_bytes: usize) -> Vec<&str> {
    assert!(max_bytes >= 4, "max_bytes must be able to hold a character");
    let mut chunks = Vec::new();
    let mut rest = text;
    while rest.len() > max_bytes {
        let mut cut = max_bytes;
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        // Cut at the last space unless it is too early in the chunk.
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

/// Whether `text` mentions `nick` as a whole word.
pub fn mentions(text: &str, nick: &str, casemapping: CaseMapping) -> bool {
    if nick.is_empty() {
        return false;
    }
    let is_word = |c: char| c.is_alphanumeric() || "_-[]\\`^{}|".contains(c);
    text.split(|c: char| !is_word(c))
        .any(|word| casemapping.eq(word, nick))
}

/// Byte range of a link detected in a text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    pub start: usize,
    pub end: usize,
}

/// Detects `http(s)://`, `irc(s)://` and `www.` links in a text.
pub fn find_links(text: &str) -> Vec<Link> {
    let mut links = Vec::new();
    let mut offset = 0;
    for word in text.split(' ') {
        let start = offset;
        offset += word.len() + 1;

        let lower = word.to_ascii_lowercase();
        let Some(begin) = ["https://", "http://", "ircs://", "irc://", "www."]
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

/// Safe address to hand to the browser, or `None`.
///
/// Only `http` and `https` are accepted (a `www.` link becomes `https://`):
/// other schemes can launch local applications (`file:`, `smb:`, custom URL
/// handlers). Characters a shell or a launcher could interpret are refused.
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

/// An `irc://` or `ircs://` address: a server, and a channel to join.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrcUrl {
    pub host: String,
    pub port: Option<u16>,
    pub tls: bool,
    /// Channel, with its prefix (`#rust`).
    pub channel: Option<String>,
    pub key: Option<String>,
}

/// Reads `irc[s]://host[:port][/channel[,needkey]][?key=…]`. A channel
/// without prefix gets `#`; `%23` and other escapes are decoded. Anything
/// unusual is refused.
pub fn parse_irc_url(link: &str) -> Option<IrcUrl> {
    let lower = link.to_ascii_lowercase();
    let (tls, rest) = if lower.starts_with("ircs://") {
        (true, &link[7..])
    } else if lower.starts_with("irc://") {
        (false, &link[6..])
    } else {
        return None;
    };
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, Some(port.parse::<u16>().ok().filter(|port| *port > 0)?)),
        None => (authority, None),
    };
    let valid_host = !host.is_empty() && host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'));
    if !valid_host {
        return None;
    }
    let (target, query) = path.split_once('?').unwrap_or((path, ""));
    let target = percent_decode(target)?;
    let mut parts = target.split(',');
    let name = parts.next().unwrap_or_default();
    let flags: Vec<&str> = parts.collect();
    let channel = if name.is_empty() || flags.contains(&"isnick") {
        None
    } else if name.chars().any(|c| c.is_whitespace() || c.is_control() || c == ':') {
        return None;
    } else if name.starts_with(['#', '&', '+', '!']) {
        Some(name.to_owned())
    } else {
        Some(format!("#{name}"))
    };
    let key = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("key="))
        .and_then(percent_decode)
        .filter(|key| !key.is_empty() && !key.contains(|c: char| c.is_whitespace() || c.is_control()));
    Some(IrcUrl { host: host.to_ascii_lowercase(), port, tls, channel, key })
}

/// `%xx` escapes decoded, as UTF-8.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

/// Removes trailing punctuation that is usually not part of the link,
/// keeping balanced parentheses (`https://en.wikipedia.org/wiki/X_(Y)`).
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
    fn reads_irc_urls() {
        let url = |host: &str, port, tls, channel: Option<&str>, key: Option<&str>| IrcUrl {
            host: host.into(),
            port,
            tls,
            channel: channel.map(str::to_owned),
            key: key.map(str::to_owned),
        };
        assert_eq!(parse_irc_url("ircs://irc.libera.chat/rust"), Some(url("irc.libera.chat", None, true, Some("#rust"), None)));
        assert_eq!(parse_irc_url("IRC://Irc.OFTC.net:6667/%23debian"), Some(url("irc.oftc.net", Some(6667), false, Some("#debian"), None)));
        assert_eq!(parse_irc_url("irc://h.example/##chat,needkey?key=s%20x"), Some(url("h.example", None, false, Some("##chat"), None)));
        assert_eq!(parse_irc_url("irc://h.example/club?key=secret"), Some(url("h.example", None, false, Some("#club"), Some("secret"))));
        assert_eq!(parse_irc_url("irc://h.example/bob,isnick"), Some(url("h.example", None, false, None, None)));
        assert_eq!(parse_irc_url("irc://h.example"), Some(url("h.example", None, false, None, None)));
        for bad in ["http://h.example", "irc://", "irc://h.example:0/x", "irc://h.example:99999", "irc://-e;rm/x", "irc://h/a%0Ab", "irc://h/a%zz", "irc://user@h/x"] {
            assert_eq!(parse_irc_url(bad), None, "{bad:?}");
        }
        let text = "join ircs://irc.libera.chat/ircnext.";
        let links = find_links(text);
        assert_eq!(&text[links[0].start..links[0].end], "ircs://irc.libera.chat/ircnext");
    }

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
            "https:///path",
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
        assert!(mentions("hey Alice, how are you?", "alice", cm));
        assert!(mentions("alice: ping", "Alice", cm));
        assert!(!mentions("malice", "alice", cm));
        assert!(mentions("ping [bot]", "{BOT}", cm));
    }

    #[test]
    fn finds_links() {
        let text = "see https://example.com/a?b=c, and (www.rust-lang.org).";
        let links: Vec<_> = find_links(text).iter().map(|l| &text[l.start..l.end]).collect();
        assert_eq!(links, ["https://example.com/a?b=c", "www.rust-lang.org"]);
    }

    #[test]
    fn keeps_balanced_parentheses() {
        let text = "https://en.wikipedia.org/wiki/Rust_(programming_language)";
        let links = find_links(text);
        assert_eq!(&text[links[0].start..links[0].end], text);
    }
}
