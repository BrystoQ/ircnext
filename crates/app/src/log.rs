//! Journaux des conversations : un fichier texte par onglet, rangé par réseau
//! (`<dossier>/<réseau>/<onglet>.log`).

use std::fs::{DirBuilder, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use ircnext_client::proto::format;

use crate::buffer::{Line, LineKind};

/// Longueur maximale d'un nom de fichier, extension comprise.
const MAX_NAME: usize = 100;

/// Ajoute une ligne au journal de l'onglet `buffer` du réseau `network`.
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

/// Emplacement du journal d'un onglet.
pub fn path(dir: &Path, network: &str, buffer: &str) -> PathBuf {
    dir.join(file_name(network)).join(format!("{}.log", file_name(&buffer.to_lowercase())))
}

/// Nom de fichier sûr : ni séparateur, ni `..`, ni caractère de contrôle ;
/// les noms de salons viennent du réseau et ne sont pas fiables.
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

/// Ligne lisible, sans codes de mise en forme.
fn describe(line: &Line) -> String {
    let reason = |reason: &Option<String>| reason.as_deref().map(|r| format!(" ({r})")).unwrap_or_default();
    let text = match &line.kind {
        LineKind::Message { nick, text, .. } => format!("<{nick}> {text}"),
        LineKind::Action { nick, text } => format!("* {nick} {text}"),
        LineKind::Notice { nick, text } => format!("-{nick}- {text}"),
        LineKind::Join { nick, mask } => format!("--> {nick}{} a rejoint", reason(mask)),
        LineKind::Part { nick, reason: r } => format!("<-- {nick} est parti{}", reason(r)),
        LineKind::Quit { nick, reason: r } => format!("<-- {nick} s'est déconnecté{}", reason(r)),
        LineKind::Kick { nick, by, reason: r } => format!("<-- {nick} expulsé par {by}{}", reason(r)),
        LineKind::Nick { old, new } => format!("--- {old} s'appelle désormais {new}"),
        LineKind::Topic { nick, topic } => {
            format!("--- Sujet{} : {}", nick.as_deref().map(|n| format!(" ({n})")).unwrap_or_default(), topic.as_deref().unwrap_or(""))
        }
        LineKind::Mode { nick, modes } => format!("--- {nick} définit le mode {modes}"),
        LineKind::Redacted { nick } => format!("--- message de {nick} supprimé"),
        LineKind::Info(text) => format!("--- {text}"),
        LineKind::Error(text) => format!("!!! {text}"),
    };
    // Une ligne de journal par message : retours à la ligne neutralisés.
    format::strip(&text).replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    #[test]
    fn file_names_stay_inside_the_log_directory() {
        let dir = Path::new("/journaux");
        assert_eq!(path(dir, "Libera.Chat", "#Rust"), Path::new("/journaux/Libera.Chat/#rust.log"));
        for hostile in ["../../etc/passwd", "/absolu", "..", "a\\b", "nul\0"] {
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
            LineKind::Message { nick: "alice".into(), prefix: None, text: "\x02salut\x02".into() },
        );
        append(&dir, "Réseau", "#salon", &line).unwrap();
        append(&dir, "Réseau", "#salon", &line).unwrap();
        let file = path(&dir, "Réseau", "#salon");
        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content.lines().count(), 2);
        assert!(content.lines().all(|l| l.ends_with("<alice> salut")));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
