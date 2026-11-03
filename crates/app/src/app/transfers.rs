//! DCC file transfers: offers received, user decisions and task events.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use chrono::Local;
use iced::Task;
use ircnext_client::proto::dcc::{Dcc, Offer, Resume};

use super::{App, Message};
use crate::buffer::{Line, LineKind};
use crate::i18n::tr;
use crate::server::{Alert, Context, ServerId};
use crate::tr;
use crate::transfer::{self, Direction, Event, State, Transfer, TransferId, format_size};
use crate::ui::transfers::Msg;

impl App {
    /// `DCC` requests received from a network.
    pub(super) fn on_dcc(&mut self, server: ServerId, requests: Vec<(String, Dcc)>) -> Task<Message> {
        let mut tasks = Vec::new();
        for (nick, dcc) in requests {
            match dcc {
                // Answer to one of our passive offers: connect and send.
                Dcc::Send(offer) if offer.port != 0 && offer.token.is_some() => {
                    let token = offer.token.as_deref();
                    let pending = self.transfers.iter_mut().find(|t| {
                        t.direction == Direction::Outgoing
                            && t.state == State::Waiting
                            && t.handle.is_none()
                            && t.matches(&nick, &offer.file, 0, token)
                    });
                    if let Some(transfer) = pending {
                        let address = SocketAddr::new(offer.ip, offer.port);
                        let size = transfer.size.unwrap_or_default();
                        let (task, handle) = transfer::send(
                            transfer.id,
                            Some(address),
                            None,
                            transfer.path.clone(),
                            size,
                            Arc::clone(&transfer.resume_at),
                        );
                        transfer.handle = Some(handle);
                        tasks.push(task.map(|(id, event)| Message::Transfer(id, event)));
                    } else {
                        self.offer_received(server, nick, offer);
                    }
                }
                Dcc::Send(offer) => self.offer_received(server, nick, offer),
                Dcc::Resume(resume) => self.resume_requested(server, &nick, &resume),
                Dcc::Accept(resume) => tasks.push(self.resume_accepted(&nick, &resume)),
            }
        }
        Task::batch(tasks)
    }

    fn offer_received(&mut self, server: ServerId, nick: String, offer: Offer) {
        let id = self.new_transfer_id();
        let name = ircnext_client::proto::dcc::sanitize_file_name(&offer.file);
        let size = offer.size.map_or_else(|| tr("unknown size").to_owned(), format_size);
        let text = tr!(
            "{nick} offers to send you {file} ({size}) from {address}. Open the transfers (⇅) to accept or decline.",
            nick = nick,
            file = name,
            size = size,
            address = offer.ip
        );
        self.transfer_info(server, &nick, text.clone(), true);
        self.transfers.push(Transfer {
            id,
            server,
            nick,
            direction: Direction::Incoming,
            name,
            path: PathBuf::new(),
            size: offer.size,
            done: 0,
            state: State::Offered,
            offer,
            resume_at: Arc::new(AtomicU64::new(0)),
            resuming: None,
            started: None,
            handle: None,
        });
    }

    /// The receiver of one of our offers asks to resume at a position.
    fn resume_requested(&mut self, server: ServerId, nick: &str, resume: &Resume) {
        let Some(transfer) = self.transfers.iter().find(|t| {
            t.direction == Direction::Outgoing
                && t.state == State::Waiting
                && t.matches(nick, &resume.file, resume.port, resume.token.as_deref())
        }) else {
            return;
        };
        let position = resume.position.min(transfer.size.unwrap_or_default());
        transfer.resume_at.store(position, Ordering::Relaxed);
        let accept = Dcc::Accept(Resume { position, ..resume.clone() });
        if let Some(server) = self.server(server) {
            let _ = server.send_dcc(nick, &accept);
        }
    }

    /// The sender agreed to resume: start receiving at that position.
    fn resume_accepted(&mut self, nick: &str, resume: &Resume) -> Task<Message> {
        let Some(transfer) = self.transfers.iter_mut().find(|t| {
            t.direction == Direction::Incoming
                && t.resuming.is_some()
                && t.matches(nick, &resume.file, resume.port, resume.token.as_deref())
        }) else {
            return Task::none();
        };
        let offset = transfer.resuming.take().unwrap_or_default().min(resume.position);
        transfer.done = offset;
        let address = SocketAddr::new(transfer.offer.ip, transfer.offer.port);
        let (task, handle) = transfer::receive(transfer.id, Some(address), None, transfer.path.clone(), offset, transfer.size);
        transfer.handle = Some(handle);
        task.map(|(id, event)| Message::Transfer(id, event))
    }

    pub(super) fn on_transfers(&mut self, msg: &Msg) -> Task<Message> {
        match msg {
            &Msg::Accept(id) => return self.accept_offer(id),
            &Msg::Decline(id) => {
                if let Some(transfer) = self.transfer_mut(id) {
                    transfer.state = State::Declined;
                }
            }
            &Msg::Cancel(id) => {
                if let Some(transfer) = self.transfer_mut(id) {
                    if let Some(handle) = transfer.handle.take() {
                        handle.abort();
                    }
                    transfer.state = State::Cancelled;
                }
            }
            &Msg::Reveal(id) => {
                let target = self.transfer_mut(id).map(|transfer| transfer.path.clone());
                if let Some(path) = target {
                    self.open_path(path.parent().unwrap_or(&path));
                }
            }
            Msg::OpenDownloads => {
                let dir = transfer::download_dir(&self.config.settings.download_dir);
                let _ = std::fs::create_dir_all(&dir);
                self.open_path(&dir);
            }
            Msg::ClearFinished => self.transfers.retain(|transfer| !transfer.state.is_finished()),
        }
        Task::none()
    }

    fn accept_offer(&mut self, id: TransferId) -> Task<Message> {
        let dir = transfer::download_dir(&self.config.settings.download_dir);
        if let Err(error) = std::fs::create_dir_all(&dir) {
            self.banner = Some(tr!("Cannot create the downloads folder: {error}", error = error));
            return Task::none();
        }
        let announced = transfer::announced_ip(&self.config.settings.dcc_address);
        let Some(index) = self.transfers.iter().position(|t| t.id == id && t.state == State::Offered) else {
            return Task::none();
        };
        let server_id = self.transfers[index].server;
        let transfer = &mut self.transfers[index];
        let (path, partial) = transfer::destination(&dir, &transfer.offer.file, transfer.size);
        transfer.path = path;
        transfer.state = State::Waiting;

        // A partial file of the same name: ask the sender to resume it.
        if partial > 0 && !transfer.offer.is_passive() {
            transfer.resuming = Some(partial);
            let resume = Dcc::Resume(Resume {
                file: transfer.offer.file.clone(),
                port: transfer.offer.port,
                position: partial,
                token: transfer.offer.token.clone(),
            });
            let nick = transfer.nick.clone();
            if let Some(server) = self.server(server_id) {
                let _ = server.send_dcc(&nick, &resume);
            }
            return Task::none();
        }

        let (address, listener) = if transfer.offer.is_passive() {
            // Passive offer: we listen and tell the sender where.
            let (Some(ip), Ok(listener)) = (announced, std::net::TcpListener::bind(("0.0.0.0", 0))) else {
                transfer.state = State::Failed(tr("no address to receive the file").to_owned());
                return Task::none();
            };
            let port = listener.local_addr().map(|address| address.port()).unwrap_or_default();
            let answer = transfer::passive_answer(&transfer.offer, ip, port);
            let nick = transfer.nick.clone();
            if let Some(server) = self.server(server_id) {
                let _ = server.send_dcc(&nick, &answer);
            }
            (None, Some(listener))
        } else {
            (Some(SocketAddr::new(transfer.offer.ip, transfer.offer.port)), None)
        };
        let transfer = &mut self.transfers[index];
        let (task, handle) = transfer::receive(id, address, listener, transfer.path.clone(), 0, transfer.size);
        transfer.handle = Some(handle);
        task.map(|(id, event)| Message::Transfer(id, event))
    }

    /// `/dcc send`, or a file dropped on a private conversation.
    pub(super) fn offer_file(&mut self, server_id: ServerId, nick: &str, path: &Path) -> Task<Message> {
        let metadata = match std::fs::metadata(path) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => return self.transfer_error(server_id, nick, tr!("{path} is not a file.", path = path.display())),
            Err(error) => return self.transfer_error(server_id, nick, tr!("Cannot read {path}: {error}", path = path.display(), error = error)),
        };
        let Some(ip) = transfer::announced_ip(&self.config.settings.dcc_address) else {
            return self.transfer_error(server_id, nick, tr("No network address to offer the file from.").to_owned());
        };
        let name = path.file_name().map_or_else(|| "file".to_owned(), |name| name.to_string_lossy().into_owned());
        let size = metadata.len();
        let id = self.new_transfer_id();
        let passive = self.config.settings.dcc_passive;

        let listener = if passive {
            None
        } else {
            match std::net::TcpListener::bind(("0.0.0.0", 0)) {
                Ok(listener) => Some(listener),
                Err(error) => return self.transfer_error(server_id, nick, tr!("Cannot listen for the transfer: {error}", error = error)),
            }
        };
        let port = listener.as_ref().and_then(|l| l.local_addr().ok()).map_or(0, |address| address.port());
        let offer = Offer {
            file: name.clone(),
            ip,
            port,
            size: Some(size),
            token: passive.then(|| id.0.to_string()),
        };
        let sent = self.server(server_id).map(|server| server.send_dcc(nick, &Dcc::Send(offer.clone())));
        if let Some(Err(error)) = sent {
            return self.transfer_error(server_id, nick, error);
        }

        let resume_at = Arc::new(AtomicU64::new(0));
        let mut task = Task::none();
        let mut handle = None;
        if let Some(listener) = listener {
            let (sending, abort) = transfer::send(id, None, Some(listener), path.to_path_buf(), size, Arc::clone(&resume_at));
            task = sending.map(|(id, event)| Message::Transfer(id, event));
            handle = Some(abort);
        }
        self.transfer_info(
            server_id,
            nick,
            tr!("Offering {file} ({size}) to {nick}…", file = name, size = format_size(size), nick = nick),
            false,
        );
        self.transfers.push(Transfer {
            id,
            server: server_id,
            nick: nick.to_owned(),
            direction: Direction::Outgoing,
            name,
            path: path.to_path_buf(),
            size: Some(size),
            done: 0,
            state: State::Waiting,
            offer,
            resume_at,
            resuming: None,
            started: None,
            handle,
        });
        task
    }

    pub(super) fn on_transfer_event(&mut self, id: TransferId, event: Event) {
        self.now = Instant::now();
        let Some(transfer) = self.transfer_mut(id) else {
            return;
        };
        if transfer.state.is_finished() {
            return;
        }
        match event {
            Event::Started => {
                transfer.state = State::Active;
                transfer.started = Some(Instant::now());
            }
            Event::Progress(done) => transfer.done = done,
            Event::Finished(result) => {
                transfer.handle = None;
                let (server, nick, name) = (transfer.server, transfer.nick.clone(), transfer.name.clone());
                let (text, error) = match (result, transfer.direction) {
                    (Ok(done), direction) => {
                        transfer.done = done;
                        transfer.state = State::Done;
                        let text = match direction {
                            Direction::Incoming => tr!("{file} received from {nick}.", file = name, nick = nick),
                            Direction::Outgoing => tr!("{file} sent to {nick}.", file = name, nick = nick),
                        };
                        (text, false)
                    }
                    (Err(error), _) => {
                        let text = tr!("Transfer of {file} failed: {error}", file = name, error = error);
                        transfer.state = State::Failed(error);
                        (text, true)
                    }
                };
                self.transfer_line(server, &nick, if error { LineKind::Error(text) } else { LineKind::Info(text) });
            }
        }
    }

    fn transfer_error(&mut self, server: ServerId, nick: &str, text: String) -> Task<Message> {
        self.transfer_line(server, nick, LineKind::Error(text));
        Task::none()
    }

    fn transfer_info(&mut self, server: ServerId, nick: &str, text: String, alert: bool) {
        if alert && let Some(buffer) = self.server_mut(server).map(|s| s.ensure(nick)) {
            let alerts = vec![Alert { buffer, title: nick.to_owned(), body: text.clone(), private: true }];
            let _ = self.alert(server, alerts);
        }
        self.transfer_line(server, nick, LineKind::Info(text));
    }

    /// Line in the conversation with `nick`.
    fn transfer_line(&mut self, server: ServerId, nick: &str, kind: LineKind) {
        let active = self.active.filter(|(id, _)| *id == server).map(|(_, b)| b);
        let settings = &self.config.settings;
        if let Some(server) = self.servers.iter_mut().find(|s| s.id == server) {
            let buffer = server.ensure(nick);
            server.push(buffer, Line::new(Local::now(), kind), &Context { settings, active });
        }
    }

    fn transfer_mut(&mut self, id: TransferId) -> Option<&mut Transfer> {
        self.transfers.iter_mut().find(|transfer| transfer.id == id)
    }

    fn new_transfer_id(&mut self) -> TransferId {
        self.next_transfer += 1;
        TransferId(self.next_transfer)
    }

    fn open_path(&mut self, path: &Path) {
        if let Err(error) = open::that_detached(path) {
            self.banner = Some(tr!("Cannot open {path}: {error}", path = path.display(), error = error));
        }
    }

    /// Offers waiting for an answer, for the sidebar badge.
    pub(super) fn pending_offers(&self) -> usize {
        self.transfers.iter().filter(|transfer| transfer.state == State::Offered).count()
    }
}
