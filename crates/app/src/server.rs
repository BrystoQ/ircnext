//! Réseau en cours d'utilisation : connexion, état IRC et onglets associés.

mod incoming;
mod outgoing;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local, SecondsFormat, Utc};
use ircnext_client::proto::{Command, Message};
use ircnext_client::{Event, Handle, Network};

use crate::buffer::{Buffer, Kind, Line, LineKind};
use crate::secrets;
use crate::config::ServerEntry;
use crate::settings::Settings;

/// Identifiant stable d'un réseau pendant la session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ServerId(pub u64);

/// Identifiant stable d'un onglet (insensible aux renommages).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BufferId(u64);

/// État de la connexion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Disconnected(Option<String>),
    Connecting(usize),
    Registering,
    Online,
    Waiting { error: Option<String>, retry_in: Duration },
}

/// Contexte fourni par l'application pour traiter un événement.
#[derive(Debug, Clone, Copy)]
pub struct Context<'a> {
    pub settings: &'a Settings,
    /// Onglet affiché, s'il appartient à ce réseau.
    pub active: Option<BufferId>,
}

/// Conséquences d'un traitement que l'application doit répercuter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Effects {
    /// La configuration du réseau a changé et doit être enregistrée.
    pub config_changed: bool,
    /// Politique STS reçue : (port TLS, durée en secondes).
    pub sts: Option<(u16, u64)>,
    /// Onglet à afficher.
    pub focus: Option<BufferId>,
    /// L'onglet courant a été fermé.
    pub closed: bool,
}

/// Préférences de l'application utiles au réseau en dehors du traitement
/// d'un événement : demandes d'historique, accusés de lecture, journaux.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prefs {
    /// Lignes demandées en rejoignant un salon (0 : aucune).
    pub history_lines: u16,
    pub read_markers: bool,
    /// Dossier des journaux, si la journalisation est active.
    pub log_dir: Option<PathBuf>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self { history_lines: 100, read_markers: true, log_dir: None }
    }
}

/// Message à signaler à l'utilisateur (notification).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub buffer: BufferId,
    /// Expéditeur, et salon le cas échéant.
    pub title: String,
    pub body: String,
    /// Message privé (sinon mention dans un salon).
    pub private: bool,
}

/// Au plus `CTCP_REPLY_LIMIT` réponses CTCP par `CTCP_REPLY_WINDOW` : au-delà,
/// les requêtes sont ignorées (sinon un flood de requêtes nous ferait
/// expulser du serveur pour excès de messages).
const CTCP_REPLY_LIMIT: usize = 3;
const CTCP_REPLY_WINDOW: Duration = Duration::from_secs(10);

/// Délai au-delà duquel les réponses d'un bot à une demande de complétion
/// sont ignorées.
pub const REMOTE_COMPLETION_TIMEOUT: Duration = Duration::from_secs(15);

/// Demande de complétion envoyée aux bots (brouillon IRCv3 `autocomplete`).
#[derive(Debug, Clone)]
pub struct RemoteCompletion {
    pub buffer: BufferId,
    /// Saisie envoyée ; les réponses ne valent que tant qu'elle est inchangée.
    pub request: String,
    pub sent: Instant,
    /// Saisies complétées proposées, avec le bot qui les propose.
    pub replies: Vec<(String, String)>,
}

impl RemoteCompletion {
    /// Propositions valables pour la saisie `input` dans l'onglet `buffer`.
    pub fn replies_for(&self, buffer: BufferId, input: &str) -> &[(String, String)] {
        let current = self.buffer == buffer && self.request == input && self.sent.elapsed() < REMOTE_COMPLETION_TIMEOUT;
        if current { &self.replies } else { &[] }
    }
}

#[derive(Debug)]
pub struct Server {
    pub id: ServerId,
    pub entry: ServerEntry,
    pub status: Status,
    pub network: Network,
    /// L'utilisateur souhaite être connecté (pilote l'abonnement).
    pub enabled: bool,
    /// Incrémenté pour forcer une nouvelle connexion.
    pub generation: u64,
    pub collapsed: bool,
    handle: Option<Handle>,
    buffers: Vec<(BufferId, Buffer)>,
    next_buffer: u64,
    pub remote_completion: Option<RemoteCompletion>,
    /// Instants des dernières réponses CTCP (limitation anti-flood).
    ctcp_replies: VecDeque<Instant>,
    pub prefs: Prefs,
    /// Messages à signaler, relevés par l'application.
    alerts: Vec<Alert>,
    /// Le message en cours de traitement vient d'un historique rejoué.
    replaying: bool,
}

impl Server {
    pub fn new(id: ServerId, entry: ServerEntry) -> Self {
        let network = Network::new(&entry.connection.nick);
        let enabled = entry.auto_connect;
        let mut server = Self {
            id,
            status: Status::Disconnected(None),
            network,
            enabled,
            generation: 0,
            collapsed: false,
            handle: None,
            buffers: Vec::new(),
            next_buffer: 0,
            remote_completion: None,
            ctcp_replies: VecDeque::new(),
            prefs: Prefs::default(),
            alerts: Vec::new(),
            replaying: false,
            entry,
        };
        server.insert_buffer(Buffer::new(Kind::Server, server.entry.name.clone()));
        server
    }

    pub fn name(&self) -> &str {
        &self.entry.name
    }

    pub fn is_online(&self) -> bool {
        self.status == Status::Online
    }

    pub fn server_buffer(&self) -> BufferId {
        self.buffers[0].0
    }

    pub fn buffers(&self) -> impl Iterator<Item = (BufferId, &Buffer)> {
        self.buffers.iter().map(|(id, buffer)| (*id, buffer))
    }

    pub fn buffer(&self, id: BufferId) -> Option<&Buffer> {
        self.buffers.iter().find(|(b, _)| *b == id).map(|(_, buffer)| buffer)
    }

    pub fn buffer_mut(&mut self, id: BufferId) -> Option<&mut Buffer> {
        self.buffers
            .iter_mut()
            .find(|(b, _)| *b == id)
            .map(|(_, buffer)| buffer)
    }

    /// Onglet de salon ou de conversation portant ce nom (selon le casemapping).
    pub fn find(&self, name: &str) -> Option<BufferId> {
        let mapping = self.network.isupport().casemapping;
        self.buffers
            .iter()
            .find(|(_, buffer)| buffer.kind != Kind::Server && mapping.eq(&buffer.name, name))
            .map(|(id, _)| *id)
    }

    /// Onglet du salon ou de la conversation `name`, créé au besoin.
    pub fn ensure(&mut self, name: &str) -> BufferId {
        if let Some(id) = self.find(name) {
            return id;
        }
        let kind = if self.network.is_channel(name) { Kind::Channel } else { Kind::Query };
        self.insert_buffer(Buffer::new(kind, name))
    }

    /// Insère en conservant l'ordre : serveur, salons, puis conversations.
    fn insert_buffer(&mut self, buffer: Buffer) -> BufferId {
        let id = BufferId(self.next_buffer);
        self.next_buffer += 1;
        let rank = |b: &Buffer| (b.kind as u8, b.name.to_lowercase());
        let key = rank(&buffer);
        let position = self.buffers.partition_point(|(_, existing)| rank(existing) <= key);
        self.buffers.insert(position, (id, buffer));
        id
    }

    pub fn remove_buffer(&mut self, id: BufferId) {
        if id != self.server_buffer() {
            self.buffers.retain(|(b, _)| *b != id);
        }
    }

    fn resort(&mut self) {
        self.buffers
            .sort_by_cached_key(|(_, b)| (b.kind as u8, b.name.to_lowercase()));
    }

    /// Envoie un message si la connexion est active.
    pub fn send(&self, message: Message) -> Result<(), String> {
        self.handle
            .as_ref()
            .ok_or_else(|| "Non connecté.".to_owned())?
            .send(message)
            .map_err(|error| error.to_string())
    }

    /// Ajoute une ligne et met à jour les compteurs de non-lus.
    pub fn push(&mut self, id: BufferId, mut line: Line, ctx: &Context<'_>) {
        let limit = ctx.settings.scrollback;
        let is_active = ctx.active == Some(id);
        // Champs empruntés séparément : l'onglet est modifié pendant qu'on lit
        // les préférences.
        let log_dir = self.prefs.log_dir.as_deref().filter(|_| !self.replaying);
        let network = self.entry.name.as_str();
        let Some((_, buffer)) = self.buffers.iter_mut().find(|(buffer, _)| *buffer == id) else {
            return;
        };
        if let LineKind::Message { text, .. } = &mut line.kind {
            if line.own
                && buffer.kind == Kind::Query
                && secrets::is_nickserv(&buffer.name)
                && let Some(masked) = secrets::mask(text)
            {
                *text = masked;
            }
            buffer.learn_bot_commands(text, line.bot);
        }
        // Journal écrit après le masquage des mots de passe, et seulement pour
        // une ligne réellement ajoutée (pas un doublon).
        let logged = log_dir.map(|_| line.clone());
        let counts = line.kind.is_chat() && !line.own;
        let highlight = line.highlight || (counts && buffer.kind == Kind::Query);
        let added = buffer.push(line, limit);
        if added && counts && !is_active {
            buffer.unread += 1;
            buffer.highlighted |= highlight;
        }
        if added
            && let (Some(dir), Some(line)) = (log_dir, logged)
        {
            // Une erreur d'écriture ne doit pas interrompre la discussion.
            let _ = crate::log::append(dir, network, &buffer.name, &line);
        }
    }

    /// Messages à signaler depuis le dernier appel.
    pub fn take_alerts(&mut self) -> Vec<Alert> {
        std::mem::take(&mut self.alerts)
    }

    /// Réserve une réponse CTCP si la limite le permet.
    fn allow_ctcp_reply(&mut self, now: Instant) -> bool {
        while self
            .ctcp_replies
            .front()
            .is_some_and(|&sent| now.duration_since(sent) >= CTCP_REPLY_WINDOW)
        {
            self.ctcp_replies.pop_front();
        }
        if self.ctcp_replies.len() >= CTCP_REPLY_LIMIT {
            return false;
        }
        self.ctcp_replies.push_back(now);
        true
    }

    /// Ajoute une ligne d'information à l'onglet du serveur.
    pub fn info(&mut self, kind: LineKind, ctx: &Context<'_>) {
        let id = self.server_buffer();
        self.push(id, Line::new(Local::now(), kind), ctx);
    }

    /// Marque un onglet comme lu et le signale au serveur (`draft/read-marker`).
    pub fn mark_read(&mut self, id: BufferId) {
        let Some(buffer) = self.buffer_mut(id) else {
            return;
        };
        let had_unread = buffer.unread > 0;
        buffer.mark_read();
        let (kind, name, last) = (buffer.kind, buffer.name.clone(), buffer.last_time());
        if had_unread
            && kind != Kind::Server
            && self.prefs.read_markers
            && self.network.has_cap("draft/read-marker")
            && let Some(last) = last
        {
            let timestamp = format!("timestamp={}", format_time(last));
            let _ = self.send(Message::new(Command::Markread, [name, timestamp]));
        }
    }

    /// Traite un événement de la connexion.
    pub fn on_event(&mut self, event: Event, ctx: &Context<'_>) -> Effects {
        let mut effects = Effects::default();
        match event {
            Event::Connecting { attempt } => {
                self.status = Status::Connecting(attempt);
                let connection = &self.entry.connection;
                let text = format!(
                    "Connexion à {}:{}{}…",
                    connection.host,
                    connection.port,
                    if connection.tls { " (TLS)" } else { "" }
                );
                self.info(LineKind::Info(text), ctx);
            }
            Event::Connected(handle) => {
                self.handle = Some(handle);
                self.status = Status::Registering;
            }
            Event::Registered(registered) => {
                self.network.reset(&registered.nick, &registered.caps);
                self.status = Status::Online;
                if let Some(policy) = registered.sts
                    && let (Some(port), Some(duration)) = (policy.port, policy.duration)
                {
                    effects.sts = Some((port, duration.as_secs()));
                }
                if registered.sasl_refused {
                    let text = "SASL non utilisé : la connexion n'est pas chiffrée. Activez TLS pour vous identifier sans exposer votre mot de passe.";
                    self.info(LineKind::Error(text.to_owned()), ctx);
                }
                let caps = if registered.caps.is_empty() {
                    "aucune".to_owned()
                } else {
                    registered.caps.join(", ")
                };
                let text = format!(
                    "Connecté en tant que {}. Capacités IRCv3 : {caps}.",
                    registered.nick
                );
                self.info(LineKind::Info(text), ctx);
            }
            Event::UpgradedToTls { port } => {
                self.entry.connection.tls = true;
                self.entry.connection.port = port;
                effects.config_changed = true;
                let text = format!("Le serveur impose TLS (STS) : reconnexion sur le port {port}.");
                self.info(LineKind::Info(text), ctx);
            }
            Event::Message(message) => {
                effects = self.receive(&message, ctx);
                self.replaying = false;
            }
            Event::Disconnected { error, retry_in } => {
                self.handle = None;
                let text = match (&error, retry_in) {
                    (Some(error), Some(delay)) => {
                        format!("Déconnecté : {error}. Nouvelle tentative dans {} s.", delay.as_secs())
                    }
                    (None, Some(delay)) => {
                        format!("Déconnecté. Nouvelle tentative dans {} s.", delay.as_secs())
                    }
                    (Some(error), None) => format!("Déconnecté : {error}."),
                    (None, None) => "Déconnecté.".to_owned(),
                };
                let ids: Vec<BufferId> = self.buffers().map(|(id, _)| id).collect();
                for id in ids {
                    self.push(id, Line::new(Local::now(), LineKind::Error(text.clone())), ctx);
                    if let Some(buffer) = self.buffer_mut(id)
                        && buffer.kind == Kind::Channel
                    {
                        buffer.joined = false;
                    }
                }
                self.status = if let Some(retry_in) = retry_in {
                    Status::Waiting { error, retry_in }
                } else {
                    self.enabled = false;
                    Status::Disconnected(error)
                };
            }
        }
        effects
    }
}

/// Horodatage au format IRCv3 (`2026-10-26T10:00:00.000Z`).
pub fn format_time(time: DateTime<Local>) -> String {
    time.with_timezone(&Utc).to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Horodatage d'un message : `server-time` s'il est présent, sinon maintenant.
pub fn message_time(message: &Message) -> DateTime<Local> {
    message
        .server_time()
        .and_then(parse_time)
        .unwrap_or_else(Local::now)
}

pub fn parse_time(value: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.with_timezone(&Local))
}

#[cfg(test)]
mod tests;
