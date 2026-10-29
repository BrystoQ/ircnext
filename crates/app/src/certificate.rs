//! Création de certificats clients auto-signés, pour s'identifier par
//! certificat (SASL `EXTERNAL`, `CertFP`) sans passer par `openssl`.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::config::AppConfig;

/// Crée un certificat pour le réseau `network` dans le dossier `certs` de la
/// configuration, et renvoie son chemin.
pub fn create(network: &str, nick: &str) -> io::Result<PathBuf> {
    let config = AppConfig::path().map_err(|error| io::Error::other(error.to_string()))?;
    let dir = config.parent().unwrap_or(Path::new(".")).join("certs");
    write(&dir, network, nick)
}

/// Écrit le certificat et sa clé dans un nouveau fichier PEM de `dir`, lisible
/// par le seul utilisateur. Un fichier existant n'est jamais remplacé.
fn write(dir: &Path, network: &str, nick: &str) -> io::Result<PathBuf> {
    let key = rcgen::KeyPair::generate().map_err(io::Error::other)?;
    let name = if nick.trim().is_empty() { "ircnext" } else { nick.trim() };
    let cert = rcgen::CertificateParams::new([name.to_owned()])
        .and_then(|params| params.self_signed(&key))
        .map_err(io::Error::other)?;
    let pem = format!("{}{}", cert.pem(), key.serialize_pem());

    create_private_dir(dir)?;
    let stem = file_stem(network);
    for attempt in 1.. {
        let file = if attempt == 1 { format!("{stem}.pem") } else { format!("{stem}-{attempt}.pem") };
        let path = dir.join(file);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        match options.open(&path) {
            Ok(mut file) => {
                file.write_all(pem.as_bytes())?;
                file.sync_all()?;
                return Ok(path);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    unreachable!("la boucle ne se termine que par un retour")
}

/// Nom de fichier tiré du nom du réseau (lettres, chiffres, `-`, `_` et `.`).
fn file_stem(network: &str) -> String {
    let stem: String = network
        .trim()
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' })
        .collect();
    let stem = stem.trim_start_matches('.');
    if stem.is_empty() { "certificat".to_owned() } else { stem.to_owned() }
}

fn create_private_dir(dir: &Path) -> io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_usable_certificates_without_overwriting() {
        let dir = std::env::temp_dir().join(format!("ircnext-certs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let first = write(&dir, "Libera.Chat", "moi").unwrap();
        let second = write(&dir, "Libera.Chat", "moi").unwrap();
        assert_eq!(first.file_name().unwrap(), "Libera.Chat.pem");
        assert_eq!(second.file_name().unwrap(), "Libera.Chat-2.pem");
        let (chain, _) = ircnext_client::connection::load_identity(&first).unwrap();
        assert_eq!(chain.len(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&first).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "clé privée lisible par son seul propriétaire");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn file_names_stay_inside_the_folder() {
        assert_eq!(file_stem("../../etc/passwd"), "_.._etc_passwd");
        assert_eq!(file_stem("Mon réseau"), "Mon_réseau");
        assert_eq!(file_stem("  "), "certificat");
    }
}
