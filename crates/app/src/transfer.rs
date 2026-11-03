//! DCC file transfers: their state in the application, and the background
//! tasks that carry them out.

use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use futures::SinkExt;
use futures::channel::mpsc;
use iced::Task;
use ircnext_client::dcc;
use ircnext_client::proto::dcc::{Dcc, Offer, sanitize_file_name};
use tokio::io::AsyncSeekExt;

use crate::server::ServerId;

/// Minimum interval between two progress reports of a transfer.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransferId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Incoming,
    Outgoing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Incoming offer waiting for the user's answer.
    Offered,
    /// Connecting, or waiting for the other side.
    Waiting,
    Active,
    Done,
    Failed(String),
    Cancelled,
    Declined,
}

impl State {
    pub fn is_finished(&self) -> bool {
        matches!(self, Self::Done | Self::Failed(_) | Self::Cancelled | Self::Declined)
    }
}

/// Report of a running transfer task.
#[derive(Debug, Clone)]
pub enum Event {
    /// Connected: the data starts flowing.
    Started,
    Progress(u64),
    Finished(Result<u64, String>),
}

#[derive(Debug)]
pub struct Transfer {
    pub id: TransferId,
    pub server: ServerId,
    pub nick: String,
    pub direction: Direction,
    /// Name shown, and sent for an outgoing file.
    pub name: String,
    /// Local file (destination or source).
    pub path: PathBuf,
    pub size: Option<u64>,
    /// Bytes transferred, from the start of the file.
    pub done: u64,
    pub state: State,
    /// Incoming: the offer received. Outgoing: the offer sent.
    pub offer: Offer,
    /// Outgoing: position asked by the receiver (`DCC RESUME`), read by the
    /// sending task once the receiver connects.
    pub resume_at: Arc<AtomicU64>,
    /// Incoming: position asked to the sender, waiting for `DCC ACCEPT`.
    pub resuming: Option<u64>,
    pub started: Option<Instant>,
    pub handle: Option<iced::task::Handle>,
}

impl Transfer {
    /// Average speed since the transfer started, in bytes per second.
    pub fn speed(&self, now: Instant) -> Option<u64> {
        let elapsed = now.duration_since(self.started?).as_secs_f64();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
        (elapsed > 0.5).then(|| (self.done as f64 / elapsed) as u64)
    }

    /// Whether this transfer answers `dcc` from `nick` (same file and port
    /// or token).
    pub fn matches(&self, nick: &str, file: &str, port: u16, token: Option<&str>) -> bool {
        self.nick.eq_ignore_ascii_case(nick)
            && match (token, self.offer.token.as_deref()) {
                (Some(token), Some(ours)) => token == ours,
                _ => port != 0 && port == self.offer.port && file == self.offer.file,
            }
    }
}

/// Destination for an incoming file in `dir`: the sanitized name, or a free
/// variant (`name (2).ext`) if a complete file already has that name.
/// Also returns the size of a partial file to resume, if any.
pub fn destination(dir: &Path, offered: &str, size: Option<u64>) -> (PathBuf, u64) {
    let name = sanitize_file_name(offered);
    let path = dir.join(&name);
    match (std::fs::metadata(&path), size) {
        (Err(_), _) => (path, 0),
        (Ok(meta), Some(size)) if meta.is_file() && meta.len() > 0 && meta.len() < size => (path, meta.len()),
        _ => {
            let (stem, extension) = match name.rsplit_once('.') {
                Some((stem, extension)) if !stem.is_empty() => (stem.to_owned(), format!(".{extension}")),
                _ => (name.clone(), String::new()),
            };
            let path = (2..u32::MAX)
                .map(|n| dir.join(format!("{stem} ({n}){extension}")))
                .find(|path| !path.exists())
                .unwrap_or_else(|| dir.join(format!("{stem} (new){extension}")));
            (path, 0)
        }
    }
}

/// Folder for received files: the configured one, or the system downloads.
pub fn download_dir(configured: &str) -> PathBuf {
    let configured = configured.trim();
    if !configured.is_empty() {
        return expand_home(configured);
    }
    directories::UserDirs::new()
        .and_then(|dirs| dirs.download_dir().map(Path::to_path_buf))
        .or_else(|| directories::BaseDirs::new().map(|dirs| dirs.home_dir().join("Downloads")))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// `~/…` resolved against the home folder.
pub fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), directories::BaseDirs::new()) {
        (Some(rest), Some(dirs)) => dirs.home_dir().join(rest),
        _ => PathBuf::from(path),
    }
}

/// Address announced to the other side: the configured one, or this
/// machine's address.
pub fn announced_ip(configured: &str) -> Option<IpAddr> {
    configured.trim().parse().ok().or_else(dcc::local_ip)
}

/// Receives the file of `offer`: by connecting to the sender, or, for a
/// passive offer, on `listener` (already announced to the sender).
pub fn receive(
    id: TransferId,
    address: Option<SocketAddr>,
    listener: Option<std::net::TcpListener>,
    path: PathBuf,
    offset: u64,
    size: Option<u64>,
) -> (Task<(TransferId, Event)>, iced::task::Handle) {
    run(id, move |mut events: mpsc::Sender<Event>| async move {
        let stream = match (address, listener) {
            (Some(address), _) => dcc::connect(address).await?,
            (None, Some(listener)) => dcc::accept(into_tokio(listener)?).await?,
            (None, None) => return Err(std::io::Error::other("no address")),
        };
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(offset > 0)
            .write(true)
            .truncate(offset == 0)
            .open(&path)
            .await?;
        let _ = events.send(Event::Started).await;
        let mut report = throttled(events.clone());
        dcc::receive(stream, &mut file, offset, size, &mut report).await
    })
}

/// Sends `path`: to the receiver connecting on `listener`, or by connecting
/// to a passive receiver at `address`.
pub fn send(
    id: TransferId,
    address: Option<SocketAddr>,
    listener: Option<std::net::TcpListener>,
    path: PathBuf,
    size: u64,
    resume_at: Arc<AtomicU64>,
) -> (Task<(TransferId, Event)>, iced::task::Handle) {
    run(id, move |mut events: mpsc::Sender<Event>| async move {
        let stream = match (address, listener) {
            (Some(address), _) => dcc::connect(address).await?,
            (None, Some(listener)) => dcc::accept(into_tokio(listener)?).await?,
            (None, None) => return Err(std::io::Error::other("no address")),
        };
        let offset = resume_at.load(Ordering::Relaxed).min(size);
        let mut file = tokio::fs::File::open(&path).await?;
        file.seek(std::io::SeekFrom::Start(offset)).await?;
        let _ = events.send(Event::Started).await;
        let mut report = throttled(events.clone());
        dcc::send(stream, &mut file, offset, size, &mut report).await
    })
}

/// Listener bound synchronously (to know its port at once), handed to the
/// runtime.
fn into_tokio(listener: std::net::TcpListener) -> std::io::Result<tokio::net::TcpListener> {
    listener.set_nonblocking(true)?;
    tokio::net::TcpListener::from_std(listener)
}

/// Runs a transfer in the background, reporting its events.
fn run<F>(
    id: TransferId,
    transfer: impl FnOnce(mpsc::Sender<Event>) -> F + Send + 'static,
) -> (Task<(TransferId, Event)>, iced::task::Handle)
where
    F: Future<Output = std::io::Result<u64>> + Send + 'static,
{
    let stream = iced::stream::channel(32, async move |mut events: mpsc::Sender<Event>| {
        let result = transfer(events.clone()).await.map_err(|error| error.to_string());
        let _ = events.send(Event::Finished(result)).await;
    });
    Task::run(stream, move |event| (id, event)).abortable()
}

/// Progress callback sending at most one report per `PROGRESS_INTERVAL`.
fn throttled(mut events: mpsc::Sender<Event>) -> impl FnMut(u64) {
    let mut last = Instant::now();
    move |done| {
        if last.elapsed() >= PROGRESS_INTERVAL {
            last = Instant::now();
            let _ = events.try_send(Event::Progress(done));
        }
    }
}

/// Our answer to a passive offer: the same offer with our address.
pub fn passive_answer(offer: &Offer, ip: IpAddr, port: u16) -> Dcc {
    Dcc::Send(Offer { ip, port, ..offer.clone() })
}

/// Human-readable size (`1.4 MB`).
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    #[allow(clippy::cast_precision_loss)]
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit + 1 < UNITS.len() {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 { format!("{bytes} B") } else { format!("{value:.1} {}", UNITS[unit]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_a_safe_free_destination() {
        let dir = std::env::temp_dir().join(format!("ircnext-transfers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        assert_eq!(destination(&dir, "../../secret.txt", Some(10)), (dir.join("secret.txt"), 0));
        // A partial file is resumed.
        std::fs::write(dir.join("secret.txt"), b"1234").unwrap();
        assert_eq!(destination(&dir, "secret.txt", Some(10)), (dir.join("secret.txt"), 4));
        // A complete file is kept: the new one gets another name.
        std::fs::write(dir.join("secret.txt"), b"0123456789").unwrap();
        assert_eq!(destination(&dir, "secret.txt", Some(10)), (dir.join("secret (2).txt"), 0));
        assert_eq!(destination(&dir, "secret.txt", None), (dir.join("secret (2).txt"), 0));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn formats_sizes() {
        assert_eq!(format_size(999), "999 B");
        assert_eq!(format_size(1_500), "1.5 KB");
        assert_eq!(format_size(2_345_678), "2.3 MB");
    }
}
