//! SOCKS5 client handshake (RFC 1928), with optional username/password
//! authentication (RFC 1929).
//!
//! The server name is sent to the proxy as is, so that the proxy resolves
//! it: no DNS request leaves the machine (important with Tor).

use std::io;
use std::net::IpAddr;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const VERSION: u8 = 5;
const NO_AUTH: u8 = 0x00;
const USER_PASS: u8 = 0x02;
const NO_ACCEPTABLE_METHOD: u8 = 0xff;
const CONNECT: u8 = 0x01;
const IPV4: u8 = 0x01;
const DOMAIN: u8 = 0x03;
const IPV6: u8 = 0x04;

/// Asks the proxy at the other end of `stream` to connect to `host:port`.
/// Once this returns, `stream` carries the connection to the target.
pub async fn connect<S>(
    stream: &mut S,
    host: &str,
    port: u16,
    credentials: Option<(&str, &str)>,
) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let methods: &[u8] = if credentials.is_some() { &[NO_AUTH, USER_PASS] } else { &[NO_AUTH] };
    let mut greeting = vec![VERSION, u8::try_from(methods.len()).expect("two methods at most")];
    greeting.extend_from_slice(methods);
    stream.write_all(&greeting).await?;

    let mut choice = [0; 2];
    stream.read_exact(&mut choice).await?;
    if choice[0] != VERSION {
        return Err(error("the proxy does not speak SOCKS5"));
    }
    match (choice[1], credentials) {
        (NO_AUTH, _) => {}
        (USER_PASS, Some((username, password))) => authenticate(stream, username, password).await?,
        (NO_ACCEPTABLE_METHOD, None) => return Err(error("the proxy requires a username and a password")),
        _ => return Err(error("the proxy refused the authentication methods")),
    }

    let mut request = vec![VERSION, CONNECT, 0];
    match host.trim_start_matches('[').trim_end_matches(']').parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            request.push(IPV4);
            request.extend_from_slice(&ip.octets());
        }
        Ok(IpAddr::V6(ip)) => {
            request.push(IPV6);
            request.extend_from_slice(&ip.octets());
        }
        Err(_) => {
            let length = u8::try_from(host.len()).map_err(|_| error("server name too long for SOCKS5"))?;
            request.push(DOMAIN);
            request.push(length);
            request.extend_from_slice(host.as_bytes());
        }
    }
    request.extend_from_slice(&port.to_be_bytes());
    stream.write_all(&request).await?;

    let mut reply = [0; 4];
    stream.read_exact(&mut reply).await?;
    if reply[0] != VERSION {
        return Err(error("invalid reply from the proxy"));
    }
    if reply[1] != 0 {
        return Err(error(&format!("the proxy could not connect: {}", reply_message(reply[1]))));
    }
    // Address the proxy bound for the connection, of no use here.
    let skip = match reply[3] {
        IPV4 => 4,
        IPV6 => 16,
        DOMAIN => usize::from(stream.read_u8().await?),
        _ => return Err(error("invalid reply from the proxy")),
    };
    let mut bound = vec![0; skip + 2];
    stream.read_exact(&mut bound).await?;
    Ok(())
}

async fn authenticate<S>(stream: &mut S, username: &str, password: &str) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let too_long = || error("proxy username or password longer than 255 bytes");
    let mut request = vec![1, u8::try_from(username.len()).map_err(|_| too_long())?];
    request.extend_from_slice(username.as_bytes());
    request.push(u8::try_from(password.len()).map_err(|_| too_long())?);
    request.extend_from_slice(password.as_bytes());
    stream.write_all(&request).await?;

    let mut status = [0; 2];
    stream.read_exact(&mut status).await?;
    if status[1] == 0 { Ok(()) } else { Err(error("the proxy rejected the username or the password")) }
}

fn reply_message(code: u8) -> &'static str {
    match code {
        1 => "general failure",
        2 => "connection not allowed by the proxy rules",
        3 => "network unreachable",
        4 => "host unreachable",
        5 => "connection refused",
        6 => "TTL expired",
        7 => "command not supported",
        8 => "address type not supported",
        _ => "unknown error",
    }
}

fn error(message: &str) -> io::Error {
    io::Error::other(format!("SOCKS5: {message}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fake proxy: checks the handshake, then answers `reply`.
    async fn proxy(
        mut stream: tokio::io::DuplexStream,
        expect_auth: Option<(&str, &str)>,
        expected_request: Vec<u8>,
        reply: u8,
    ) {
        let mut greeting = [0; 2];
        stream.read_exact(&mut greeting).await.unwrap();
        let mut methods = vec![0; usize::from(greeting[1])];
        stream.read_exact(&mut methods).await.unwrap();
        if let Some((username, password)) = expect_auth {
            assert!(methods.contains(&USER_PASS));
            stream.write_all(&[VERSION, USER_PASS]).await.unwrap();
            let mut header = [0; 2];
            stream.read_exact(&mut header).await.unwrap();
            let mut user = vec![0; usize::from(header[1])];
            stream.read_exact(&mut user).await.unwrap();
            let mut pass = vec![0; usize::from(stream.read_u8().await.unwrap())];
            stream.read_exact(&mut pass).await.unwrap();
            let ok = user == username.as_bytes() && pass == password.as_bytes();
            stream.write_all(&[1, u8::from(!ok)]).await.unwrap();
            if !ok {
                return;
            }
        } else {
            stream.write_all(&[VERSION, NO_AUTH]).await.unwrap();
        }
        let mut request = vec![0; expected_request.len()];
        stream.read_exact(&mut request).await.unwrap();
        assert_eq!(request, expected_request);
        stream.write_all(&[VERSION, reply, 0, IPV4, 10, 0, 0, 1, 0x1a, 0x0b]).await.unwrap();
        if reply == 0 {
            stream.write_all(b"hello through the proxy").await.unwrap();
        }
    }

    fn domain_request(host: &str, port: u16) -> Vec<u8> {
        let mut request = vec![VERSION, CONNECT, 0, DOMAIN, u8::try_from(host.len()).unwrap()];
        request.extend_from_slice(host.as_bytes());
        request.extend_from_slice(&port.to_be_bytes());
        request
    }

    #[tokio::test]
    async fn connects_through_the_proxy_by_name() {
        let (mut client, server) = tokio::io::duplex(256);
        let server = tokio::spawn(proxy(server, None, domain_request("irc.example.net", 6697), 0));
        connect(&mut client, "irc.example.net", 6697, None).await.unwrap();
        let mut greeting = [0; 23];
        client.read_exact(&mut greeting).await.unwrap();
        assert_eq!(&greeting, b"hello through the proxy");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn authenticates_with_username_and_password() {
        let (mut client, server) = tokio::io::duplex(256);
        let request = vec![VERSION, CONNECT, 0, IPV4, 127, 0, 0, 1, 0x1a, 0x0b];
        let server = tokio::spawn(proxy(server, Some(("me", "secret")), request, 0));
        connect(&mut client, "127.0.0.1", 6667, Some(("me", "secret"))).await.unwrap();
        server.await.unwrap();

        let (mut client, server) = tokio::io::duplex(256);
        let server = tokio::spawn(proxy(server, Some(("me", "secret")), Vec::new(), 0));
        let error = connect(&mut client, "127.0.0.1", 6667, Some(("me", "wrong"))).await.unwrap_err();
        assert!(error.to_string().contains("rejected"), "{error}");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn reports_proxy_failures() {
        let (mut client, server) = tokio::io::duplex(256);
        let server = tokio::spawn(proxy(server, None, domain_request("irc.example.net", 6697), 5));
        let error = connect(&mut client, "irc.example.net", 6697, None).await.unwrap_err();
        assert!(error.to_string().contains("connection refused"), "{error}");
        server.await.unwrap();

        // A proxy asking for credentials we do not have.
        let (mut client, mut server) = tokio::io::duplex(256);
        server.write_all(&[VERSION, NO_ACCEPTABLE_METHOD]).await.unwrap();
        let error = connect(&mut client, "irc.example.net", 6697, None).await.unwrap_err();
        assert!(error.to_string().contains("requires a username"), "{error}");
    }
}
