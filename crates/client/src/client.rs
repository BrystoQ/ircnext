//! Connection loop: registration, keepalive, automatic reconnection.
//!
//! [`connect`] returns a stream of [`Event`]s; the connection lives as long
//! as this stream is consumed. Commands are sent through the [`Handle`]
//! received in [`Event::Connected`].

use std::collections::VecDeque;
use std::io;
use std::time::Duration;

use futures::channel::mpsc;
use futures::{SinkExt, StreamExt, future, stream};
use tokio::sync::mpsc as tokio_mpsc;
use tokio::time::Instant;
use thiserror::Error;

use ircnext_proto::{Command, Message};

use crate::config::Config;
use crate::connection::{self, LineReader, write_message};
use crate::registration::{Progress, Registered, Registration};
use crate::throttle::Throttle;

/// Silence allowed before sending a `PING` (then before giving up).
const PING_INTERVAL: Duration = Duration::from_secs(90);
/// Time given to the server to close the connection after a `QUIT`.
const QUIT_GRACE: Duration = Duration::from_secs(3);
const EVENT_BUFFER: usize = 256;
const BACKOFF_STEPS: [u64; 6] = [2, 4, 8, 16, 32, 60];
/// Sending: allowed burst, then one message per interval.
const SEND_BURST: u32 = 5;
const SEND_INTERVAL: Duration = Duration::from_secs(2);
/// Messages waiting to be sent beyond which new ones are refused.
const MAX_QUEUED: usize = 1000;
/// Maximum length of the parameters of a grouped `JOIN`.
const JOIN_BATCH_BYTES: usize = 400;

/// Event produced by a connection.
#[derive(Debug, Clone)]
pub enum Event {
    /// Connection attempt (1 for the first one).
    Connecting { attempt: usize },
    /// Transport established; the handle lets you send messages.
    Connected(Handle),
    Registered(Registered),
    /// The server requires TLS (STS): the configuration must be updated.
    UpgradedToTls { port: u16 },
    Message(Message),
    /// Connection lost. `retry_in` is `None` if no reconnection is planned.
    Disconnected {
        error: Option<String>,
        retry_in: Option<Duration>,
    },
}

#[derive(Debug)]
enum Action {
    Send(Message),
    Quit(Option<String>),
}

/// Error returned when the connection is no longer active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("the connection is closed")]
pub struct Closed;

/// Sending handle to an active connection.
#[derive(Debug, Clone)]
pub struct Handle(tokio_mpsc::UnboundedSender<Action>);

impl Handle {
    pub fn send(&self, message: Message) -> Result<(), Closed> {
        self.0.send(Action::Send(message)).map_err(|_| Closed)
    }

    /// Quits the server; the event stream ends afterwards.
    pub fn quit(&self, reason: Option<String>) -> Result<(), Closed> {
        self.0.send(Action::Quit(reason)).map_err(|_| Closed)
    }
}

/// Starts a connection and returns its event stream.
pub fn connect(config: Config) -> impl futures::Stream<Item = Event> + Send + 'static {
    let (sender, receiver) = mpsc::channel(EVENT_BUFFER);
    let runner = stream::once(run(config, sender)).filter_map(|()| future::ready(None));
    stream::select(receiver, runner)
}

/// End of a session.
enum End {
    /// Quit at the user's request.
    Quit,
    /// Nobody listens to the events anymore.
    Abandoned,
    UpgradeToTls(u16),
    Closed(Option<String>),
}

async fn run(mut config: Config, mut events: mpsc::Sender<Event>) {
    let mut attempt = 0;
    loop {
        attempt += 1;
        if events.send(Event::Connecting { attempt }).await.is_err() {
            return;
        }

        let mut registered = false;
        let end = session(&config, &mut events, &mut registered)
            .await
            .unwrap_or_else(|error| End::Closed(Some(error.to_string())));
        if registered {
            attempt = 1;
        }

        let (error, retry_in) = match end {
            End::Abandoned => return,
            End::Quit => (None, None),
            End::UpgradeToTls(port) => {
                config.tls = true;
                config.port = port;
                attempt = 0;
                if events.send(Event::UpgradedToTls { port }).await.is_err() {
                    return;
                }
                continue;
            }
            End::Closed(error) => (error, config.auto_reconnect.then(|| backoff(attempt))),
        };

        if events.send(Event::Disconnected { error, retry_in }).await.is_err() {
            return;
        }
        match retry_in {
            Some(delay) => tokio::time::sleep(delay).await,
            None => return,
        }
    }
}

fn backoff(attempt: usize) -> Duration {
    let index = attempt.saturating_sub(1).min(BACKOFF_STEPS.len() - 1);
    Duration::from_secs(BACKOFF_STEPS[index])
}

async fn session(
    config: &Config,
    events: &mut mpsc::Sender<Event>,
    registered: &mut bool,
) -> io::Result<End> {
    let stream = connection::open(
        &config.host,
        config.port,
        config.tls,
        config.accept_invalid_certs,
        config.client_cert.as_deref().filter(|_| config.tls),
    )
    .await?;
    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = LineReader::new(reader);

    let (actions_tx, mut actions) = tokio_mpsc::unbounded_channel();
    if events.send(Event::Connected(Handle(actions_tx))).await.is_err() {
        return Ok(End::Abandoned);
    }

    let (mut registration, greeting) = Registration::start(config);
    for message in &greeting {
        write_message(&mut writer, message).await?;
    }

    let mut last_activity = Instant::now();
    let mut awaiting_pong = false;
    // User messages, sent at the pace allowed by `throttle`. Protocol
    // replies (PONG, registration) go first.
    let mut queue: VecDeque<Message> = VecDeque::new();
    let mut throttle = Throttle::new(SEND_BURST, SEND_INTERVAL, Instant::now());
    // `QUIT` requested: sent once the queue is empty, so that no message is lost.
    let mut quit: Option<Option<String>> = None;

    loop {
        while !queue.is_empty() && throttle.try_acquire(Instant::now()) {
            let message = queue.pop_front().expect("non-empty queue");
            write_message(&mut writer, &message).await?;
        }
        if queue.is_empty()
            && let Some(reason) = quit.take()
        {
            write_message(&mut writer, &Message::quit(reason.as_deref())).await?;
            // Let the server close cleanly, without waiting forever.
            let drain = async { while let Ok(Some(_)) = reader.next_line().await {} };
            let _ = tokio::time::timeout(QUIT_GRACE, drain).await;
            return Ok(End::Quit);
        }
        tokio::select! {
            line = reader.next_line() => {
                let Some(line) = line? else {
                    return Ok(End::Closed(None));
                };
                last_activity = Instant::now();
                awaiting_pong = false;

                let Ok(message) = line.parse::<Message>() else {
                    continue;
                };
                if message.command == Command::Ping {
                    let token = message.params.last().cloned().unwrap_or_default();
                    write_message(&mut writer, &Message::pong(token)).await?;
                }

                if !*registered {
                    match registration.handle(&message) {
                        Progress::Continue(replies) => {
                            for reply in &replies {
                                write_message(&mut writer, reply).await?;
                            }
                        }
                        Progress::Done(info) => {
                            *registered = true;
                            if events.send(Event::Registered(info)).await.is_err() {
                                return Ok(End::Abandoned);
                            }
                            queue.extend(join_batches(&config.autojoin));
                        }
                        Progress::UpgradeToTls { port } => return Ok(End::UpgradeToTls(port)),
                    }
                }

                if events.send(Event::Message(message)).await.is_err() {
                    return Ok(End::Abandoned);
                }
            }
            action = actions.recv() => match action {
                Some(Action::Send(message)) => enqueue(&mut queue, message),
                Some(Action::Quit(reason)) => quit = Some(reason),
                None => return Ok(End::Abandoned),
            },
            () = tokio::time::sleep_until(throttle.ready_at()), if !queue.is_empty() => {}
            () = tokio::time::sleep_until(last_activity + PING_INTERVAL) => {
                if awaiting_pong {
                    return Ok(End::Closed(Some("no reply from the server (ping)".to_owned())));
                }
                write_message(&mut writer, &Message::ping("ircnext")).await?;
                awaiting_pong = true;
                last_activity = Instant::now();
            }
        }
    }
}

/// Queues a message. A typing indicator is dropped if the queue is not
/// empty (it would be outdated once sent).
fn enqueue(queue: &mut VecDeque<Message>, message: Message) {
    let ephemeral = message.command == Command::Tagmsg && message.typing().is_some();
    if (ephemeral && !queue.is_empty()) || queue.len() >= MAX_QUEUED {
        return;
    }
    queue.push_back(message);
}

/// Groups channels into `JOIN #a,#b,...` of reasonable length.
fn join_batches(channels: &[String]) -> Vec<Message> {
    let mut batches: Vec<String> = Vec::new();
    for channel in channels.iter().map(|c| c.trim()).filter(|c| !c.is_empty() && !c.contains([' ', ','])) {
        match batches.last_mut() {
            Some(batch) if batch.len() + 1 + channel.len() <= JOIN_BATCH_BYTES => {
                batch.push(',');
                batch.push_str(channel);
            }
            _ => batches.push(channel.to_owned()),
        }
    }
    batches.into_iter().map(|batch| Message::join(batch, None)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    #[test]
    fn groups_autojoin_channels() {
        let channels: Vec<String> = ["#a", "#b", "bad channel", "#c"].map(String::from).to_vec();
        let lines: Vec<String> = join_batches(&channels).iter().map(ToString::to_string).collect();
        assert_eq!(lines, ["JOIN #a,#b,#c"]);
        let many: Vec<String> = (0..100).map(|i| format!("#channel{i:03}")).collect();
        let batches = join_batches(&many);
        assert!(batches.len() > 1);
        assert!(batches.iter().all(|m| m.params[0].len() <= JOIN_BATCH_BYTES));
    }

    #[test]
    fn drops_stale_typing_notifications() {
        use ircnext_proto::ircv3::Typing;
        let mut queue = VecDeque::new();
        enqueue(&mut queue, Typing::Active.message("#a"));
        enqueue(&mut queue, Message::privmsg("#a", "x"));
        enqueue(&mut queue, Typing::Done.message("#a"));
        assert_eq!(queue.len(), 2);
    }

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(backoff(1), Duration::from_secs(2));
        assert_eq!(backoff(3), Duration::from_secs(8));
        assert_eq!(backoff(50), Duration::from_secs(60));
    }

    /// Fake server: checks registration, PING/PONG, autojoin and QUIT.
    #[tokio::test]
    async fn registers_and_quits_against_local_server() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let (read, mut write) = socket.into_split();
            let mut lines = BufReader::new(read).lines();
            let mut received = Vec::new();
            let mut next = async || {
                let line = lines.next_line().await.unwrap().unwrap();
                received.push(line.clone());
                line
            };

            assert_eq!(next().await, "CAP LS 302");
            assert_eq!(next().await, "NICK tester");
            assert!(next().await.starts_with("USER "));
            write.write_all(b":srv CAP * LS :server-time\r\n").await.unwrap();
            assert_eq!(next().await, "CAP REQ server-time");
            write.write_all(b":srv CAP * ACK :server-time\r\n").await.unwrap();
            assert_eq!(next().await, "CAP END");
            write.write_all(b":srv 001 tester :Welcome\r\nPING :tok\r\n").await.unwrap();
            assert_eq!(next().await, "JOIN #test");
            assert_eq!(next().await, "PONG tok");
            // More messages than the allowed burst: all leave before the QUIT.
            for i in 0..SEND_BURST {
                assert_eq!(next().await, format!("PRIVMSG #test {i}"));
            }
            assert_eq!(next().await, "QUIT");
        });

        let config = Config {
            host: "127.0.0.1".into(),
            port,
            tls: false,
            nick: "tester".into(),
            autojoin: vec!["#test".into()],
            ..Config::default()
        };
        let mut events = Box::pin(connect(config));
        let mut handle = None;
        let mut registered = None;

        while let Some(event) = events.next().await {
            match event {
                Event::Connected(h) => handle = Some(h),
                Event::Registered(info) => registered = Some(info),
                Event::Message(message) if message.command == Command::Ping => {
                    let handle = handle.as_ref().unwrap();
                    for i in 0..SEND_BURST {
                        handle.send(Message::privmsg("#test", i.to_string())).unwrap();
                    }
                    handle.quit(None).unwrap();
                }
                Event::Disconnected { retry_in, .. } => {
                    assert_eq!(retry_in, None);
                    break;
                }
                _ => {}
            }
        }

        let registered = registered.expect("registered");
        assert_eq!(registered.nick, "tester");
        assert_eq!(registered.caps, ["server-time"]);
        server.await.unwrap();
    }
}
