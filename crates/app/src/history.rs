//! Recent messages kept between launches: the last lines of each tab,
//! saved when quitting and shown again when the tab comes back.

use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::buffer::{Buffer, Kind, Line, LineKind};

/// Lines kept per tab.
pub const LINES: usize = 200;

/// Saved tabs of a network: lowercase name → lines, oldest first.
pub type Saved = HashMap<String, Vec<Line>>;

fn path(dir: &Path, network: &str) -> PathBuf {
    dir.join(format!("{}.json", crate::log::file_name(network)))
}

/// Lines of `buffer` worth keeping: conversation and topics, never
/// encrypted ones.
fn kept(buffer: &Buffer) -> Vec<Line> {
    let lines: Vec<&Line> = buffer
        .lines()
        .filter(|line| !line.encrypted && (line.kind.is_chat() || matches!(line.kind, LineKind::Topic { .. })))
        .collect();
    lines[lines.len().saturating_sub(LINES)..].iter().map(|line| (*line).clone()).collect()
}

/// Writes the tabs of a network (only readable by the user).
pub fn save<'a>(dir: &Path, network: &str, buffers: impl Iterator<Item = &'a Buffer>) -> io::Result<()> {
    let saved: Saved = buffers
        .filter(|buffer| buffer.kind != Kind::Server)
        .map(|buffer| (buffer.name.to_lowercase(), kept(buffer)))
        .filter(|(_, lines)| !lines.is_empty())
        .collect();
    let path = path(dir, network);
    if saved.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    }
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)?;
    let mut options = std::fs::OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let json = serde_json::to_vec(&saved).map_err(io::Error::other)?;
    options.open(path)?.write_all(&json)
}

/// Saved tabs of a network; nothing if missing or unreadable.
pub fn load(dir: &Path, network: &str) -> Saved {
    std::fs::read(path(dir, network)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

/// Forgets everything saved.
pub fn clear(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    #[test]
    fn keeps_the_last_conversation_lines() {
        let dir = std::env::temp_dir().join(format!("ircnext-history-{}", std::process::id()));
        clear(&dir);
        let mut channel = Buffer::new(Kind::Channel, "#Rust");
        for n in 0..(LINES + 5) {
            let line = Line::new(Local::now(), LineKind::Message { nick: "alice".into(), prefix: None, text: format!("message {n}") });
            channel.push(line, 1000);
        }
        let mut secret = Line::new(Local::now(), LineKind::Message { nick: "bob".into(), prefix: None, text: "encrypted".into() });
        secret.encrypted = true;
        channel.push(secret, 1000);
        channel.push(Line::new(Local::now(), LineKind::Join { nick: "carol".into(), mask: None }), 1000);
        let server = Buffer::new(Kind::Server, "Libera.Chat");
        let empty = Buffer::new(Kind::Query, "zoe");

        save(&dir, "Libera.Chat", [&server, &channel, &empty].into_iter()).unwrap();
        let saved = load(&dir, "Libera.Chat");
        assert_eq!(saved.keys().collect::<Vec<_>>(), ["#rust"]);
        let lines = &saved["#rust"];
        assert_eq!(lines.len(), LINES);
        assert!(matches!(&lines[LINES - 1].kind, LineKind::Message { text, .. } if text == "message 204"), "encrypted and joins left out");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(path(&dir, "Libera.Chat")).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        save(&dir, "Libera.Chat", [&server].into_iter()).unwrap();
        assert!(load(&dir, "Libera.Chat").is_empty(), "nothing left: file removed");
        assert!(load(&dir, "../../etc").is_empty());
        clear(&dir);
    }
}
