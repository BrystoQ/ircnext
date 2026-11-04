//! Update check: the latest release on GitHub, and the download of its
//! installer, checked against the release's `SHA256SUMS.txt` before it is
//! opened.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Releases of the project.
const LATEST: &str = "https://api.github.com/repos/BrystoQ/ircnext/releases/latest";
/// Longest installer accepted.
const MAX_DOWNLOAD: u64 = 300 * 1024 * 1024;

/// A published version.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Release {
    pub tag_name: String,
    /// Release page, with the patch notes.
    pub html_url: String,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
}

impl Release {
    /// Version without the `v`.
    pub fn version(&self) -> &str {
        self.tag_name.trim_start_matches('v')
    }

    fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|asset| asset.name == name)
    }

    /// The installer for this system, if the release has one.
    pub fn installer(&self) -> Option<&Asset> {
        let suffix = installer_suffix()?;
        self.assets.iter().find(|asset| asset.name.ends_with(suffix))
    }
}

/// End of the installer's file name for this system.
fn installer_suffix() -> Option<&'static str> {
    if cfg!(target_os = "windows") {
        Some("-windows-x64-setup.exe")
    } else if cfg!(target_os = "macos") {
        Some("-macos.dmg")
    } else if cfg!(target_os = "linux") {
        Some("_amd64.deb")
    } else {
        None
    }
}

/// Whether `candidate` (`1.2.3`) is newer than `current`.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    let parse = |version: &str| -> Vec<u64> {
        version
            .split(['-', '+'])
            .next()
            .unwrap_or_default()
            .split('.')
            .map(|part| part.parse().unwrap_or(0))
            .collect()
    };
    let (candidate, current) = (parse(candidate), parse(current));
    let len = candidate.len().max(current.len());
    let pad = |mut parts: Vec<u64>| {
        parts.resize(len, 0);
        parts
    };
    pad(candidate) > pad(current)
}

/// Expected SHA-256 of `name` in a `SHA256SUMS.txt` (`<hash>  <name>`).
pub fn expected_hash(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.split_once(char::is_whitespace)?;
        (file.trim().trim_start_matches('*') == name).then(|| hash.to_ascii_lowercase())
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().fold(String::with_capacity(64), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .user_agent(concat!("ircnext/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// The latest release, if it is newer than this version.
pub fn check() -> Result<Option<Release>, String> {
    let release: Release = agent()
        .get(LATEST)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|error| error.to_string())?
        .body_mut()
        .read_json()
        .map_err(|error| error.to_string())?;
    Ok(is_newer(release.version(), env!("CARGO_PKG_VERSION")).then_some(release))
}

/// Downloads the installer of `release` into `dir`, after checking it
/// against the release's checksums. Returns the file.
pub fn download(release: &Release, dir: &Path) -> Result<PathBuf, String> {
    let installer = release.installer().ok_or("no installer for this system in the release")?;
    let sums = release.asset("SHA256SUMS.txt").ok_or("the release has no checksums")?;
    let agent = agent();
    let sums = agent
        .get(&sums.browser_download_url)
        .call()
        .map_err(|error| error.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())?;
    let expected = expected_hash(&sums, &installer.name).ok_or("the installer is missing from the checksums")?;
    let bytes = agent
        .get(&installer.browser_download_url)
        .call()
        .map_err(|error| error.to_string())?
        .body_mut()
        .with_config()
        .limit(MAX_DOWNLOAD)
        .read_to_vec()
        .map_err(|error| error.to_string())?;
    if sha256_hex(&bytes) != expected {
        return Err("the downloaded installer does not match its checksum: not opened".into());
    }
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let (path, _) = crate::transfer::destination(dir, &installer.name, None);
    std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("0.4.0", "0.3.0"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(is_newer("0.3.10", "0.3.9"));
        assert!(!is_newer("0.3.0", "0.3.0"));
        assert!(!is_newer("0.2.9", "0.3.0"));
        assert!(!is_newer("0.3.0-beta", "0.3.0"));
    }

    #[test]
    fn finds_checksums() {
        let sums = "abc123  ircnext-0.4.0-macos.dmg\nDEF456 *ircnext-0.4.0-windows-x64-setup.exe\n";
        assert_eq!(expected_hash(sums, "ircnext-0.4.0-macos.dmg").as_deref(), Some("abc123"));
        assert_eq!(expected_hash(sums, "ircnext-0.4.0-windows-x64-setup.exe").as_deref(), Some("def456"));
        assert_eq!(expected_hash(sums, "other"), None);
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn picks_the_installer_of_this_system() {
        let asset = |name: &str| Asset { name: name.into(), browser_download_url: format!("https://example.org/{name}") };
        let release = Release {
            tag_name: "v0.4.0".into(),
            html_url: "https://github.com/BrystoQ/ircnext/releases/tag/v0.4.0".into(),
            assets: vec![
                asset("ircnext-0.4.0-macos.dmg"),
                asset("ircnext-0.4.0-macos.zip"),
                asset("ircnext-0.4.0-windows-x64-setup.exe"),
                asset("ircnext-0.4.0-windows-x64-portable.zip"),
                asset("ircnext_0.4.0_amd64.deb"),
                asset("SHA256SUMS.txt"),
            ],
        };
        assert_eq!(release.version(), "0.4.0");
        let expected = if cfg!(target_os = "macos") {
            "ircnext-0.4.0-macos.dmg"
        } else if cfg!(target_os = "windows") {
            "ircnext-0.4.0-windows-x64-setup.exe"
        } else {
            "ircnext_0.4.0_amd64.deb"
        };
        assert_eq!(release.installer().unwrap().name, expected);
    }

    /// Real request to GitHub; run by hand with
    /// `cargo test -p ircnext -- --ignored latest_release`.
    #[test]
    #[ignore = "needs the network"]
    fn latest_release() {
        let release: Release = agent()
            .get(LATEST)
            .call()
            .unwrap()
            .body_mut()
            .read_json()
            .unwrap();
        assert!(release.tag_name.starts_with('v'), "{release:?}");
        assert!(release.asset("SHA256SUMS.txt").is_some());
    }
}
