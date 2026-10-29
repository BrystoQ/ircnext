//! Configuration file: preferences, networks and STS policies.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use ircnext_client::Config as Connection;
use serde::{Deserialize, Serialize};

use crate::i18n::tr;
use crate::settings::Settings;
use crate::tr;

/// Content of `config.toml`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub settings: Settings,
    pub servers: Vec<ServerEntry>,
    /// Remembered STS policies, by host name.
    pub sts: BTreeMap<String, StsEntry>,
}

/// Saved network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerEntry {
    pub name: String,
    #[serde(default = "enabled")]
    pub auto_connect: bool,
    /// Commands run after connecting (`/msg`, `/mode`...), like HexChat's
    /// “connect commands” list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_connect: Vec<String>,
    #[serde(flatten)]
    pub connection: Connection,
}

fn enabled() -> bool {
    true
}

/// STS policy: TLS required on `port` until `expires_at` (UNIX seconds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StsEntry {
    pub port: u16,
    pub expires_at: i64,
}

/// Loading or saving error.
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Parse(toml::de::Error),
    Serialize(toml::ser::Error),
    NoConfigDir,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => f.write_str(&tr!("file access error: {error}", error = error)),
            Self::Parse(error) => f.write_str(&tr!("unreadable configuration: {error}", error = error)),
            Self::Serialize(error) => f.write_str(&tr!("configuration cannot be written: {error}", error = error)),
            Self::NoConfigDir => f.write_str(tr("configuration folder not found")),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl AppConfig {
    /// Location of the file: `$IRCNEXT_CONFIG` if set, otherwise the
    /// system's configuration folder
    /// (`~/Library/Application Support/ircnext/` on macOS).
    pub fn path() -> Result<PathBuf, Error> {
        if let Some(path) = std::env::var_os("IRCNEXT_CONFIG") {
            return Ok(PathBuf::from(path));
        }
        directories::ProjectDirs::from("", "", "ircnext")
            .map(|dirs| dirs.config_dir().join("config.toml"))
            .ok_or(Error::NoConfigDir)
    }

    /// Loads the configuration; a missing file gives the default configuration.
    ///
    /// An unreadable file is renamed to `config.toml.bak` so that it is not
    /// overwritten, and the error is returned with a blank configuration.
    pub fn load() -> (Self, Option<Error>) {
        let path = match Self::path() {
            Ok(path) => path,
            Err(error) => return (Self::default(), Some(error)),
        };
        match Self::load_from(&path) {
            Ok(config) => (config, None),
            Err(error @ Error::Parse(_)) => {
                let _ = std::fs::rename(&path, path.with_extension("toml.bak"));
                (Self::default(), Some(error))
            }
            Err(error) => (Self::default(), Some(error)),
        }
    }

    fn load_from(path: &Path) -> Result<Self, Error> {
        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error.into()),
        };
        let mut config: Self = toml::from_str(&content).map_err(Error::Parse)?;
        config.settings.sanitize();
        Ok(config)
    }

    /// Saves atomically (temporary file then rename), with restricted
    /// permissions since the file may contain passwords.
    pub fn save_to(&self, path: &Path) -> Result<(), Error> {
        use std::io::Write;

        let content = toml::to_string_pretty(self).map_err(Error::Serialize)?;
        if let Some(dir) = path.parent() {
            create_private_dir(dir)?;
        }
        let temporary = path.with_extension("toml.tmp");
        // An old temporary file (or a symbolic link put in its place) is
        // removed: the file is then created exclusively, with restricted
        // permissions from the start.
        match std::fs::remove_file(&temporary) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&temporary)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)?;
        Ok(())
    }

    /// Effective connection settings, after applying a valid STS policy.
    pub fn effective_connection(&self, connection: &Connection, now: i64) -> Connection {
        let mut connection = connection.clone();
        if let Some(policy) = self.sts.get(&connection.host.to_ascii_lowercase())
            && policy.expires_at > now
        {
            connection.tls = true;
            connection.port = policy.port;
        }
        connection
    }

    /// Remembers (or removes, if the duration is zero) an STS policy.
    pub fn record_sts(&mut self, host: &str, port: u16, duration_secs: u64, now: i64) {
        let host = host.to_ascii_lowercase();
        if duration_secs == 0 {
            self.sts.remove(&host);
        } else {
            let duration = i64::try_from(duration_secs).unwrap_or(i64::MAX);
            let expires_at = now.saturating_add(duration);
            self.sts.insert(host, StsEntry { port, expires_at });
        }
    }
}

/// Creates the configuration folder, accessible to the user only.
fn create_private_dir(dir: &Path) -> io::Result<()> {
    if dir.as_os_str().is_empty() || dir.is_dir() {
        return Ok(());
    }
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)
}

/// Network offered in the list, like HexChat's network list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset {
    pub name: &'static str,
    pub host: &'static str,
    pub port: u16,
    pub tls: bool,
}

impl std::fmt::Display for Preset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

pub const PRESETS: &[Preset] = &[
    Preset { name: "Libera.Chat", host: "irc.libera.chat", port: 6697, tls: true },
    Preset { name: "OFTC", host: "irc.oftc.net", port: 6697, tls: true },
    Preset { name: "Rizon", host: "irc.rizon.net", port: 6697, tls: true },
    Preset { name: "EFnet", host: "irc.efnet.org", port: 6697, tls: true },
    Preset { name: "hackint", host: "irc.hackint.org", port: 6697, tls: true },
    Preset { name: "IRCnet", host: "open.ircnet.net", port: 6667, tls: false },
    Preset { name: "QuakeNet", host: "irc.quakenet.org", port: 6667, tls: false },
    Preset { name: "Undernet", host: "irc.undernet.org", port: 6667, tls: false },
    Preset { name: "Ergo (IRCv3 testnet)", host: "testnet.ergo.chat", port: 6697, tls: true },
];

#[cfg(test)]
mod tests {
    use super::*;
    use ircnext_client::{Sasl, Secret};

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ircnext-test-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("config.toml")
    }

    #[test]
    fn round_trips_through_toml() {
        let path = temp_path("roundtrip");
        let mut config = AppConfig::default();
        config.settings.highlight_words = vec!["rust".into()];
        config.servers.push(ServerEntry {
            name: "Libera".into(),
            auto_connect: true,
            on_connect: vec!["/mode me +i".into()],
            connection: Connection {
                host: "irc.libera.chat".into(),
                sasl: Some(Sasl {
                    username: "me".into(),
                    password: Secret::new("pw"),
                }),
                autojoin: vec!["#rust".into()],
                ..Connection::default()
            },
        });
        config.record_sts("IRC.Libera.Chat", 6697, 60, 1000);

        config.save_to(&path).unwrap();
        assert_eq!(AppConfig::load_from(&path).unwrap(), config);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "file readable by its owner only");
        }
        // An abandoned temporary file does not block saving.
        std::fs::write(path.with_extension("toml.tmp"), "reste").unwrap();
        config.save_to(&path).unwrap();
    }

    #[test]
    fn missing_file_gives_defaults() {
        let path = temp_path("missing").with_file_name("absent.toml");
        assert_eq!(AppConfig::load_from(&path).unwrap(), AppConfig::default());
    }

    #[test]
    fn partial_file_uses_defaults_for_the_rest() {
        let path = temp_path("partial");
        std::fs::write(&path, "[settings]\nfont_size = 16\n").unwrap();
        let config = AppConfig::load_from(&path).unwrap();
        assert_eq!(config.settings.font_size, 16);
        assert!(config.settings.show_timestamps);
    }

    #[test]
    fn applies_only_valid_sts_policies() {
        let mut config = AppConfig::default();
        let plain = Connection {
            host: "irc.example.net".into(),
            port: 6667,
            tls: false,
            ..Connection::default()
        };
        config.record_sts("irc.example.net", 6697, 100, 1000);
        let upgraded = config.effective_connection(&plain, 1050);
        assert!(upgraded.tls);
        assert_eq!(upgraded.port, 6697);
        assert_eq!(config.effective_connection(&plain, 2000), plain);

        config.record_sts("irc.example.net", 6697, 0, 1000);
        assert!(config.sts.is_empty());
    }
}
