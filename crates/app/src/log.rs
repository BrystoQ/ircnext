//! Conversation logs: one text file per tab, grouped by network
//! (`<folder>/<network>/<tab>.log`).

use std::fs::{DirBuilder, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use ircnext_client::proto::format;

use crate::buffer::{Line, LineKind};
use crate::tr;

/// Maximum length of a file name, extension included.
const MAX_NAME: usize = 100;

/// Appends a line to the log of the tab `buffer` of the network `network`.
pub fn append(dir: &Path, network: &str, buffer: &str, line: &Line) -> io::Result<()> {
    let path = path(dir, network, buffer);
    if let Some(parent) = path.parent() {
        let mut builder = DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder.create(parent)?;
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path)?;
    let entry = format!("[{}] {}\n", line.time.format("%Y-%m-%d %H:%M:%S"), describe(line));
    file.write_all(entry.as_bytes())
}

/// Location of a tab's log.
pub fn path(dir: &Path, network: &str, buffer: &str) -> PathBuf {
    dir.join(file_name(network)).join(format!("{}.log", file_name(&buffer.to_lowercase())))
}

/// Safe file name: no separator, no `..`, no control character; channel
/// names come from the network and cannot be trusted.
fn file_name(name: &str) -> String {
    let mut safe: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || "#&+-_.@".contains(c) { c } else { '_' })
        .take(MAX_NAME - ".log".len())
        .collect();
    if safe.starts_with('.') || safe.is_empty() {
        safe.insert(0, '_');
    }
    safe
}

/// Readable line, without formatting codes.
fn describe(line: &Line) -> String {
    let reason = |reason: &Option<String>| reason.as_deref().map(|r| format!(" ({r})")).unwrap_or_default();
    let text = match &line.kind {
        LineKind::Message { nick, text, .. } => format!("<{nick}> {text}"),
        LineKind::Action { nick, text } => format!("* {nick} {text}"),
        LineKind::Notice { nick, text } => format!("-{nick}- {text}"),
        LineKind::Join { nick, mask } => tr!("--> {nick}{mask} joined", nick = nick, mask = reason(mask)),
        LineKind::Part { nick, reason: r } => tr!("<-- {nick} left{reason}", nick = nick, reason = reason(r)),
        LineKind::Quit { nick, reason: r } => tr!("<-- {nick} quit{reason}", nick = nick, reason = reason(r)),
        LineKind::Kick { nick, by, reason: r } => {
            tr!("<-- {nick} was kicked by {by}{reason}", nick = nick, by = by, reason = reason(r))
        }
        LineKind::Nick { old, new } => tr!("--- {old} is now known as {new}", old = old, new = new),
        LineKind::Topic { nick, topic } => tr!(
            "--- Topic{by}: {topic}",
            by = nick.as_deref().map(|n| format!(" ({n})")).unwrap_or_default(),
            topic = topic.as_deref().unwrap_or("")
        ),
        LineKind::Mode { nick, modes } => tr!("--- {nick} sets mode {modes}", nick = nick, modes = modes),
        LineKind::Redacted { nick } => tr!("--- message from {nick} deleted", nick = nick),
        LineKind::Info(text) => format!("--- {text}"),
        LineKind::Error(text) => format!("!!! {text}"),
    };
    // One log line per message: line breaks neutralized.
    format::strip(&text).replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    #[test]
    fn file_names_stay_inside_the_log_directory() {
        let dir = Path::new("/logs");
        assert_eq!(path(dir, "Libera.Chat", "#Rust"), Path::new("/logs/Libera.Chat/#rust.log"));
        for hostile in ["../../etc/passwd", "/absolute", "..", "a\\b", "nul\0"] {
            let path = path(dir, hostile, hostile);
            assert!(path.starts_with(dir), "{hostile:?} → {path:?}");
            assert_eq!(path.components().count(), 4, "{hostile:?} → {path:?}");
        }
    }

    #[test]
    fn writes_private_append_only_logs() {
        let dir = std::env::temp_dir().join(format!("ircnext-logs-{}", std::process::id()));
        let line = Line::new(
            Local::now(),
            LineKind::Message { nick: "alice".into(), prefix: None, text: "\x02hello\x02".into() },
        );
        append(&dir, "Network", "#channel", &line).unwrap();
        append(&dir, "Network", "#channel", &line).unwrap();
        let file = path(&dir, "Network", "#channel");
        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content.lines().count(), 2);
        assert!(content.lines().all(|l| l.ends_with("<alice> hello")));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
