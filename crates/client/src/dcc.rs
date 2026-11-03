//! DCC file transfers over TCP.
//!
//! The receiver acknowledges the bytes received with 32-bit big-endian
//! counters, as most clients expect; the sender reads them on the side so
//! that neither end blocks.

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Delay to reach the other side, or for it to connect to us.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(120);
/// Silence allowed during a transfer.
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
/// Delay given to the receiver to acknowledge the last bytes.
const FINAL_ACK_TIMEOUT: Duration = Duration::from_secs(30);
const CHUNK: usize = 64 * 1024;

/// Connects to the sender (or to a passive receiver).
pub async fn connect(address: SocketAddr) -> io::Result<TcpStream> {
    let stream = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(address))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "DCC: connection timed out"))??;
    stream.set_nodelay(true)?;
    Ok(stream)
}

/// Listens on a free port of all interfaces.
pub async fn listen() -> io::Result<(TcpListener, u16)> {
    let listener = TcpListener::bind((IpAddr::from([0, 0, 0, 0]), 0)).await?;
    let port = listener.local_addr()?.port();
    Ok((listener, port))
}

/// Waits for the other side to connect.
pub async fn accept(listener: TcpListener) -> io::Result<TcpStream> {
    let (stream, _) = tokio::time::timeout(CONNECT_TIMEOUT, listener.accept())
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "DCC: nobody connected"))??;
    stream.set_nodelay(true)?;
    Ok(stream)
}

/// Address of this machine on the network used to reach the Internet (no
/// packet is sent). Behind a router, it is the local address.
pub fn local_ip() -> Option<IpAddr> {
    let socket = std::net::UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    socket.connect(("192.0.2.1", 9)).ok()?;
    Some(socket.local_addr().ok()?.ip())
}

/// Receives into `file` (positioned at `offset`) until `size` bytes in all,
/// or until the sender closes when the size is unknown. `progress` gets the
/// total bytes received. Returns that total.
pub async fn receive(
    mut stream: TcpStream,
    file: &mut File,
    offset: u64,
    size: Option<u64>,
    mut progress: impl FnMut(u64),
) -> io::Result<u64> {
    let mut total = offset;
    let mut buffer = vec![0; CHUNK];
    loop {
        if size.is_some_and(|size| total >= size) {
            break;
        }
        let wanted = size.map_or(CHUNK, |size| usize::try_from(size - total).unwrap_or(CHUNK).min(CHUNK));
        let read = match tokio::time::timeout(IDLE_TIMEOUT, stream.read(&mut buffer[..wanted])).await {
            Err(_) => return Err(io::Error::new(io::ErrorKind::TimedOut, "DCC: the sender stopped sending")),
            // The sender went away: what was received is kept, the size
            // check below tells whether the file is complete.
            Ok(Err(error)) if is_disconnection(&error) => break,
            Ok(result) => result?,
        };
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).await?;
        total += read as u64;
        // Acknowledgment: low 32 bits of the total, big-endian. A sender
        // that already left is noticed by the next read.
        #[allow(clippy::cast_possible_truncation)] // truncation is the protocol
        let _ = stream.write_all(&(total as u32).to_be_bytes()).await;
        progress(total);
    }
    file.flush().await?;
    if size.is_some_and(|size| total < size) {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "DCC: transfer interrupted"));
    }
    Ok(total)
}

fn is_disconnection(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted | io::ErrorKind::BrokenPipe
    )
}

/// Sends `file` (positioned at `offset`) to the receiver; `size` is the full
/// size of the file. `progress` gets the total bytes sent. Returns once the
/// receiver has acknowledged everything, or closed the connection.
pub async fn send(stream: TcpStream, file: &mut File, offset: u64, size: u64, mut progress: impl FnMut(u64)) -> io::Result<u64> {
    let (mut reader, mut writer) = stream.into_split();
    let acknowledged = Arc::new(AtomicU64::new(0));
    let closed = Arc::new(AtomicBool::new(false));
    let acks = {
        let (acknowledged, closed) = (Arc::clone(&acknowledged), Arc::clone(&closed));
        tokio::spawn(async move {
            let mut ack = [0; 4];
            while reader.read_exact(&mut ack).await.is_ok() {
                acknowledged.store(u64::from(u32::from_be_bytes(ack)), Ordering::Relaxed);
            }
            closed.store(true, Ordering::Relaxed);
        })
    };

    let mut total = offset;
    let mut buffer = vec![0; CHUNK];
    while total < size {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        tokio::time::timeout(IDLE_TIMEOUT, writer.write_all(&buffer[..read]))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "DCC: the receiver stopped reading"))??;
        total += read as u64;
        progress(total);
    }
    writer.flush().await?;

    // Wait for the last acknowledgment, or for the receiver to close.
    #[allow(clippy::cast_possible_truncation)] // acknowledgments are 32-bit
    let expected = u64::from(total as u32);
    let done = async {
        while !closed.load(Ordering::Relaxed) && acknowledged.load(Ordering::Relaxed) != expected {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    let _ = tokio::time::timeout(FINAL_ACK_TIMEOUT, done).await;
    acks.abort();
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncSeekExt;

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ircnext-dcc-{}-{name}", std::process::id()))
    }

    async fn transfer(content: &[u8], offset: u64) -> (u64, Vec<u8>, Vec<u64>) {
        let source = temp(&format!("source-{offset}"));
        let target = temp(&format!("target-{offset}"));
        std::fs::write(&source, content).unwrap();
        std::fs::write(&target, &content[..usize::try_from(offset).unwrap()]).unwrap();

        let (listener, port) = listen().await.unwrap();
        let size = content.len() as u64;
        let sender = tokio::spawn(async move {
            let stream = accept(listener).await.unwrap();
            let mut file = File::open(&source).await.unwrap();
            file.seek(io::SeekFrom::Start(offset)).await.unwrap();
            let sent = send(stream, &mut file, offset, size, |_| {}).await.unwrap();
            std::fs::remove_file(source).unwrap();
            sent
        });

        let stream = connect(SocketAddr::from(([127, 0, 0, 1], port))).await.unwrap();
        let mut file = tokio::fs::OpenOptions::new().append(true).open(&target).await.unwrap();
        let mut steps = Vec::new();
        let received = receive(stream, &mut file, offset, Some(size), |total| steps.push(total)).await.unwrap();
        assert_eq!(sender.await.unwrap(), size);
        let written = std::fs::read(&target).unwrap();
        std::fs::remove_file(target).unwrap();
        (received, written, steps)
    }

    #[tokio::test]
    async fn transfers_a_file() {
        let content: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let (received, written, steps) = transfer(&content, 0).await;
        assert_eq!(received, content.len() as u64);
        assert_eq!(written, content);
        assert!(steps.len() > 1 && steps.windows(2).all(|w| w[0] < w[1]), "progress goes up");
    }

    #[tokio::test]
    async fn resumes_at_an_offset() {
        let content: Vec<u8> = (0..100_000u32).map(|i| (i % 13) as u8).collect();
        let (received, written, steps) = transfer(&content, 40_000).await;
        assert_eq!(received, content.len() as u64);
        assert_eq!(written, content);
        assert!(steps[0] > 40_000);
    }

    #[tokio::test]
    async fn reports_an_interrupted_transfer() {
        let (listener, port) = listen().await.unwrap();
        let sender = tokio::spawn(async move {
            let mut stream = accept(listener).await.unwrap();
            stream.write_all(b"only part").await.unwrap();
        });
        let target = temp("interrupted");
        let mut file = File::create(&target).await.unwrap();
        let stream = connect(SocketAddr::from(([127, 0, 0, 1], port))).await.unwrap();
        let error = receive(stream, &mut file, 0, Some(1000), |_| {}).await.unwrap_err();
        assert!(error.to_string().contains("interrupted"), "{error}");
        sender.await.unwrap();
        std::fs::remove_file(target).unwrap();
    }

    #[test]
    fn finds_a_local_address() {
        // Without any network, there may be no address: only check the type.
        if let Some(ip) = local_ip() {
            assert!(!ip.is_unspecified());
        }
    }
}
