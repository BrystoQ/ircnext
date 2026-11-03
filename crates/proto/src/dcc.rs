//! DCC file transfers: the CTCP messages that negotiate them.
//!
//! - `DCC SEND <file> <ip> <port> <size> [token]`: file offer. A zero port
//!   with a token is a *passive* offer: the receiver listens and answers
//!   with the same message carrying its own address.
//! - `DCC RESUME <file> <port> <position> [token]` and `DCC ACCEPT ...`:
//!   resuming an interrupted transfer.
//!
//! Reference: <https://modern.ircdocs.horse/dcc>

use std::fmt;
use std::net::{IpAddr, Ipv4Addr};

/// A `DCC` request carried in a CTCP message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dcc {
    Send(Offer),
    /// The receiver asks to resume `file` at `position`.
    Resume(Resume),
    /// The sender agrees to resume at `position`.
    Accept(Resume),
}

/// File offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    /// File name as announced (not sanitized).
    pub file: String,
    pub ip: IpAddr,
    /// Zero for a passive offer.
    pub port: u16,
    pub size: Option<u64>,
    /// Identifies a passive offer and its answer.
    pub token: Option<String>,
}

impl Offer {
    /// Passive offer: the other side has to listen.
    pub fn is_passive(&self) -> bool {
        self.port == 0 && self.token.is_some()
    }
}

/// Resume request or acceptance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resume {
    pub file: String,
    pub port: u16,
    pub position: u64,
    pub token: Option<String>,
}

impl Dcc {
    /// Parses the parameters of a `DCC` CTCP (everything after `DCC `).
    pub fn parse(params: &str) -> Option<Self> {
        let (kind, rest) = params.trim().split_once(' ')?;
        let (file, rest) = split_file_name(rest.trim_start())?;
        let args: Vec<&str> = rest.split_whitespace().collect();
        match kind.to_ascii_uppercase().as_str() {
            "SEND" => {
                let [ip, port, rest @ ..] = args.as_slice() else {
                    return None;
                };
                Some(Self::Send(Offer {
                    file,
                    ip: parse_ip(ip)?,
                    port: port.parse().ok()?,
                    size: rest.first().and_then(|size| size.parse().ok()),
                    token: rest.get(1).map(|token| (*token).to_owned()),
                }))
            }
            kind @ ("RESUME" | "ACCEPT") => {
                let [port, position, rest @ ..] = args.as_slice() else {
                    return None;
                };
                let resume = Resume {
                    file,
                    port: port.parse().ok()?,
                    position: position.parse().ok()?,
                    token: rest.first().map(|token| (*token).to_owned()),
                };
                Some(if kind == "RESUME" { Self::Resume(resume) } else { Self::Accept(resume) })
            }
            _ => None,
        }
    }
}

/// CTCP parameters (after `DCC `): `SEND "my file.txt" 3232235777 5000 1234`.
impl fmt::Display for Dcc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Send(offer) => {
                write!(f, "SEND {} {} {}", quote(&offer.file), format_ip(offer.ip), offer.port)?;
                if let Some(size) = offer.size {
                    write!(f, " {size}")?;
                }
                if let Some(token) = &offer.token {
                    write!(f, " {token}")?;
                }
                Ok(())
            }
            Self::Resume(resume) | Self::Accept(resume) => {
                let kind = if matches!(self, Self::Resume(_)) { "RESUME" } else { "ACCEPT" };
                write!(f, "{kind} {} {} {}", quote(&resume.file), resume.port, resume.position)?;
                if let Some(token) = &resume.token {
                    write!(f, " {token}")?;
                }
                Ok(())
            }
        }
    }
}

/// File name, quoted or not, then the rest of the parameters.
fn split_file_name(text: &str) -> Option<(String, &str)> {
    if let Some(quoted) = text.strip_prefix('"') {
        let end = quoted.find('"')?;
        Some((quoted[..end].to_owned(), &quoted[end + 1..]))
    } else {
        let (file, rest) = text.split_once(' ').unwrap_or((text, ""));
        Some((file.to_owned(), rest))
    }
}

fn quote(file: &str) -> String {
    if file.contains(' ') { format!("\"{}\"", file.replace('"', "")) } else { file.to_owned() }
}

/// IPv4 as a decimal integer (historical form), IPv6 as text.
fn parse_ip(text: &str) -> Option<IpAddr> {
    match text.parse::<u32>() {
        Ok(number) => Some(IpAddr::V4(Ipv4Addr::from(number))),
        Err(_) => text.parse().ok(),
    }
}

fn format_ip(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(ip) => u32::from(ip).to_string(),
        IpAddr::V6(ip) => ip.to_string(),
    }
}

/// Safe local name for a received file: the last path component only,
/// without control or reserved characters, never hidden nor empty.
pub fn sanitize_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = base
        .chars()
        .map(|c| if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').trim_end_matches(['.', ' ']);
    let mut cleaned: String = cleaned.chars().take(200).collect();
    // Reserved device names on Windows.
    let stem = cleaned.split('.').next().unwrap_or_default().to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) && stem.as_bytes()[3].is_ascii_digit())
    {
        cleaned.insert(0, '_');
    }
    if cleaned.is_empty() { "file".to_owned() } else { cleaned }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_offers() {
        let Some(Dcc::Send(offer)) = Dcc::parse("SEND photo.jpg 3232235777 5000 1234") else {
            panic!("offer expected");
        };
        assert_eq!(offer.file, "photo.jpg");
        assert_eq!(offer.ip, "192.168.1.1".parse::<IpAddr>().unwrap());
        assert_eq!((offer.port, offer.size, offer.token), (5000, Some(1234), None));

        let Some(Dcc::Send(passive)) = Dcc::parse("SEND \"my notes.txt\" 2130706433 0 42 77") else {
            panic!("offer expected");
        };
        assert_eq!(passive.file, "my notes.txt");
        assert!(passive.is_passive());

        let Some(Dcc::Send(v6)) = Dcc::parse("SEND a.bin ::1 6000 10") else {
            panic!("offer expected");
        };
        assert_eq!(v6.ip, "::1".parse::<IpAddr>().unwrap());

        assert_eq!(Dcc::parse("SEND only-a-name"), None);
        assert_eq!(Dcc::parse("CHAT chat 1 2"), None);
    }

    #[test]
    fn round_trips() {
        for text in [
            "SEND \"my notes.txt\" 3232235777 5000 1234",
            "SEND a.bin 2130706433 0 42 77",
            "RESUME \"my notes.txt\" 5000 1024",
            "ACCEPT a.bin 0 512 77",
        ] {
            assert_eq!(Dcc::parse(text).unwrap().to_string(), text);
        }
    }

    #[test]
    fn sanitizes_file_names() {
        assert_eq!(sanitize_file_name("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_file_name("C:\\Windows\\evil.exe"), "evil.exe");
        assert_eq!(sanitize_file_name(".bashrc"), "bashrc");
        assert_eq!(sanitize_file_name("a\u{0}b?c.txt"), "a_b_c.txt");
        assert_eq!(sanitize_file_name("CON.txt"), "_CON.txt");
        assert_eq!(sanitize_file_name("COM1"), "_COM1");
        assert_eq!(sanitize_file_name(".."), "file");
        assert_eq!(sanitize_file_name("photo.jpg"), "photo.jpg");
    }
}
