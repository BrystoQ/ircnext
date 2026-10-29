//! État global de l'application, messages et logique de mise à jour (architecture Elm).

use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::Local;
use futures::{Stream, StreamExt};
use iced::keyboard;
use iced::widget::{center, container, opaque, row, stack};
use iced::{Element, Length, Subscription, Task, Theme, event, system, time, window};
use ircnext_client::proto::ircv3::Typing;
use ircnext_client::{Config, Event};

use crate::buffer::{Kind, Line, LineKind};
use crate::completion::Completion;
use crate::secrets;
use crate::suggest::{self, Suggestion};
use crate::config::{AppConfig, ServerEntry};
use crate::ignore;
use crate::input::{self, Command, Input};
use crate::keychain;
use crate::notify;
use crate::keymap::{self, Action, Combo};
use crate::server::{Alert, BufferId, Context, Effects, Prefs, Server, ServerId, Status};
use crate::settings::Settings;
use crate::theme::{self, Colors};
use crate::ui::server_form::{self, ServerForm};
use crate::ui::settings_form::{self, SettingsForm};
use crate::ui::switcher::{self, Switcher};
use crate::ui::{chat, nicklist, sidebar};

mod keys;

/// Identifiant du champ de saisie (pour lui donner le focus).
pub const INPUT_ID: &str = "input";
/// Intervalle minimal entre deux notifications de saisie « active ».
const TYPING_THROTTLE: Duration = Duration::from_secs(3);
/// Taille maximale de l'historique de saisie.
const INPUT_HISTORY: usize = 100;

#[derive(Debug, Clone)]
pub enum Message {
    Client(ServerId, Event),
    Select(ServerId, BufferId),
    InputChanged(String),
    AcceptSuggestion(usize),
    Submit,
    /// Appui de touche ; `captured` : déjà traité par un widget.
    Key { combo: Combo, captured: bool },
    Shortcut(Shortcut),
    Action(Action),
    Switcher(switcher::Msg),
    SearchChanged(String),
    /// La fenêtre gagne ou perd le focus.
    WindowFocus { window: window::Id, focused: bool },
    OpenLogs,
    Exit,
    OpenUrl(String),
    OpenQuery(ServerId, String),
    ToggleCollapsed(ServerId),
    ToggleNicklist,
    Connect(ServerId),
    Disconnect(ServerId),
    CloseBuffer(ServerId, BufferId),
    OpenSettings,
    Settings(settings_form::Msg),
    SaveSettings,
    OpenServerForm(Option<ServerId>),
    Form(server_form::Msg),
    SaveServerForm,
    RemoveServer(ServerId),
    CloseModal,
    DismissBanner,
    SystemTheme(iced::theme::Mode),
    Tick(Instant),
}

/// Touches à rôle fixe (non configurables).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    Complete { backwards: bool },
    HistoryPrevious,
    HistoryNext,
    Escape,
}

#[derive(Debug)]
enum Modal {
    Settings(SettingsForm),
    Server(ServerForm),
    Switcher(Switcher),
}

/// État de la fenêtre principale.
#[derive(Debug, Clone, Copy)]
struct WindowState {
    id: Option<window::Id>,
    focused: bool,
    /// Le système est en mode sombre.
    dark: bool,
}

/// Dernière notification de saisie envoyée.
#[derive(Debug, Clone, Copy)]
struct TypingSent {
    server: ServerId,
    buffer: BufferId,
    at: Instant,
}

#[derive(Debug)]
pub struct App {
    config: AppConfig,
    /// Fichier de configuration ; `None` désactive l'enregistrement (tests).
    config_path: Option<PathBuf>,
    servers: Vec<Server>,
    next_server: u64,
    active: Option<(ServerId, BufferId)>,
    input: String,
    completion: Option<Completion>,
    /// Suggestions affichées au-dessus de la saisie.
    suggestions: Vec<Suggestion>,
    selected_suggestion: usize,
    /// Suggestions fermées avec Échap, jusqu'à la prochaine frappe.
    suggestions_dismissed: bool,
    history: Vec<String>,
    history_cursor: Option<usize>,
    /// Saisie mise de côté pendant la navigation dans l'historique.
    draft: String,
    typing_sent: Option<TypingSent>,
    modal: Option<Modal>,
    /// Recherche dans l'onglet courant (barre affichée si `Some`).
    search: Option<String>,
    window: WindowState,
    /// Notifications du système activées (jamais dans les tests).
    system_notifications: bool,
    /// Mots de passe déjà rangés dans le trousseau du système.
    keychain: keychain::Cache,
    banner: Option<String>,
    now: Instant,
}

impl App {
    pub fn boot() -> (Self, Task<Message>) {
        let (mut config, error) = AppConfig::load();
        let (keychain, missing) = keychain::restore(&mut config, &keychain::System);
        let banner = error.map(|error| format!("Configuration : {error}")).or_else(|| {
            (!missing.is_empty())
                .then(|| format!("Mot de passe SASL introuvable dans le trousseau : {}", missing.join(", ")))
        });
        let mut app = Self::new(config, AppConfig::path().ok(), banner);
        app.keychain = keychain;
        app.system_notifications = true;
        app.sync_prefs();
        let tasks = Task::batch([
            system::theme().map(Message::SystemTheme),
            iced::widget::operation::focus(INPUT_ID),
        ]);
        (app, tasks)
    }

    fn new(config: AppConfig, config_path: Option<PathBuf>, banner: Option<String>) -> Self {
        let mut app = Self {
            config,
            config_path,
            servers: Vec::new(),
            next_server: 0,
            active: None,
            input: String::new(),
            completion: None,
            suggestions: Vec::new(),
            selected_suggestion: 0,
            suggestions_dismissed: false,
            history: Vec::new(),
            history_cursor: None,
            draft: String::new(),
            typing_sent: None,
            modal: None,
            search: None,
            window: WindowState { id: None, focused: true, dark: true },
            system_notifications: false,
            keychain: keychain::Cache::new(),
            banner,
            now: Instant::now(),
        };
        for entry in app.config.servers.clone() {
            app.add_server(entry);
        }
        app.active = app.servers.first().map(|s| (s.id, s.server_buffer()));
        if app.servers.is_empty() {
            app.modal = Some(Modal::Server(ServerForm::new(&app.config.settings)));
        }
        app
    }

    pub fn title(&self) -> String {
        let title = match self.active_buffer() {
            Some((server, buffer)) if buffer.name != server.name() => {
                format!("{} — {} · ircnext", buffer.name, server.name())
            }
            Some((server, _)) => format!("{} · ircnext", server.name()),
            None => "ircnext".to_owned(),
        };
        // Onglets attendant une réponse : mention ou message privé non lu.
        let mentions = self
            .servers
            .iter()
            .flat_map(Server::buffers)
            .filter(|(_, buffer)| buffer.highlighted)
            .count();
        if self.config.settings.title_unread_count && mentions > 0 {
            format!("({mentions}) {title}")
        } else {
            title
        }
    }

    /// Préférences transmises aux réseaux.
    fn sync_prefs(&mut self) {
        let settings = &self.config.settings;
        let prefs = Prefs {
            history_lines: settings.history_lines,
            read_markers: settings.send_read_markers,
            log_dir: self.log_dir().filter(|_| settings.logging),
        };
        for server in &mut self.servers {
            server.prefs.clone_from(&prefs);
        }
    }

    /// Dossier des journaux, à côté du fichier de configuration.
    fn log_dir(&self) -> Option<PathBuf> {
        Some(self.config_path.as_ref()?.parent()?.join("logs"))
    }

    pub fn theme(&self) -> Theme {
        self.colors().theme()
    }

    /// Paramètres effectifs (ceux du panneau ouvert, pour l'aperçu en direct).
    fn settings(&self) -> &Settings {
        match &self.modal {
            Some(Modal::Settings(form)) => &form.draft,
            _ => &self.config.settings,
        }
    }

    fn colors(&self) -> Colors {
        let settings = self.settings();
        Colors::new(settings.theme, self.window.dark, settings.accent)
    }

    fn server(&self, id: ServerId) -> Option<&Server> {
        self.servers.iter().find(|s| s.id == id)
    }

    fn server_mut(&mut self, id: ServerId) -> Option<&mut Server> {
        self.servers.iter_mut().find(|s| s.id == id)
    }

    fn active_buffer(&self) -> Option<(&Server, &crate::buffer::Buffer)> {
        let (server_id, buffer_id) = self.active?;
        let server = self.server(server_id)?;
        Some((server, server.buffer(buffer_id)?))
    }

    fn add_server(&mut self, entry: ServerEntry) -> ServerId {
        let id = ServerId(self.next_server);
        self.next_server += 1;
        self.servers.push(Server::new(id, entry));
        self.sync_prefs();
        id
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        self.refresh_suggestions();
        task
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Client(id, event) => return self.on_client_event(id, event),
            Message::Select(id, buffer) => {
                if matches!(self.modal, Some(Modal::Switcher(_))) {
                    self.modal = None;
                }
                return self.select(id, buffer);
            }
            Message::InputChanged(value) => {
                self.input = value;
                self.completion = None;
                self.suggestions_dismissed = false;
                self.history_cursor = None;
                self.update_typing();
            }
            Message::AcceptSuggestion(index) => return self.accept_suggestion(index),
            Message::Submit if !self.suggestions.is_empty() => {
                return self.accept_suggestion(self.selected_suggestion);
            }
            Message::Submit => return self.submit(),
            Message::Key { combo, captured } => return self.on_key(combo, captured),
            Message::Shortcut(shortcut) => return self.on_shortcut(shortcut),
            Message::Action(action) => return self.perform(action),
            Message::Switcher(msg) => return self.on_switcher(msg),
            Message::SearchChanged(query) => self.search = Some(query),
            Message::WindowFocus { window, focused } => {
                self.window.id = Some(window);
                self.window.focused = focused;
            }
            Message::OpenLogs => {
                if let Some(dir) = self.log_dir() {
                    let _ = std::fs::create_dir_all(&dir);
                    if let Err(error) = open::that_detached(&dir) {
                        self.banner = Some(format!("Impossible d'ouvrir le dossier des journaux : {error}"));
                    }
                }
            }
            Message::Exit => return iced::exit(),
            Message::OpenUrl(link) => match ircnext_client::proto::text::safe_url(&link) {
                Some(url) => {
                    if let Err(error) = open::that_detached(&url) {
                        self.banner = Some(format!("Impossible d'ouvrir le lien : {error}"));
                    }
                }
                None => self.banner = Some(format!("Lien refusé (adresse non web ou suspecte) : {link}")),
            },
            Message::OpenQuery(id, nick) => {
                let effects = self.execute(id, None, Command::Query { nick, text: None });
                return self.apply_effects(id, effects);
            }
            Message::ToggleCollapsed(id) => {
                if let Some(server) = self.server_mut(id) {
                    server.collapsed = !server.collapsed;
                }
            }
            Message::ToggleNicklist => {
                self.config.settings.show_nicklist = !self.config.settings.show_nicklist;
                self.save();
            }
            Message::Connect(id) => {
                let effects = self.execute(id, None, Command::Connect);
                return self.apply_effects(id, effects);
            }
            Message::Disconnect(id) => {
                let effects = self.execute(id, None, Command::Quit(None));
                if let Some(server) = self.server_mut(id)
                    && matches!(server.status, Status::Waiting { .. } | Status::Connecting(_))
                {
                    server.status = Status::Disconnected(None);
                }
                return self.apply_effects(id, effects);
            }
            Message::CloseBuffer(id, buffer) => {
                let effects = self.execute(id, Some(buffer), Command::Close);
                return self.apply_effects(id, effects);
            }
            Message::OpenSettings => {
                self.modal = Some(Modal::Settings(SettingsForm::new(&self.config.settings)));
            }
            Message::Settings(msg) => {
                if let Some(Modal::Settings(form)) = &mut self.modal {
                    form.update(msg);
                }
            }
            Message::SaveSettings => {
                if let Some(Modal::Settings(form)) = self.modal.take() {
                    self.config.settings = form.finish();
                    self.sync_prefs();
                    self.save();
                }
                return iced::widget::operation::focus(INPUT_ID);
            }
            Message::OpenServerForm(id) => {
                let form = match id.and_then(|id| self.server(id)) {
                    Some(server) => ServerForm::edit(server.id, &server.entry),
                    None => ServerForm::new(&self.config.settings),
                };
                self.modal = Some(Modal::Server(form));
            }
            Message::Form(msg) => {
                if let Some(Modal::Server(form)) = &mut self.modal {
                    form.update(msg);
                }
            }
            Message::SaveServerForm => return self.save_server_form(),
            Message::RemoveServer(id) => {
                if let Some(server) = self.server_mut(id) {
                    server.enabled = false;
                }
                let _ = self.execute(id, None, Command::Quit(None));
                self.servers.retain(|s| s.id != id);
                if self.active.is_some_and(|(server, _)| server == id) {
                    self.active = self.servers.first().map(|s| (s.id, s.server_buffer()));
                }
                self.modal = None;
                self.persist_servers();
            }
            Message::CloseModal => {
                self.modal = None;
                return iced::widget::operation::focus(INPUT_ID);
            }
            Message::DismissBanner => self.banner = None,
            Message::SystemTheme(mode) => self.window.dark = mode != iced::theme::Mode::Light,
            Message::Tick(now) => {
                self.now = now;
                for server in &mut self.servers {
                    let ids: Vec<BufferId> = server.buffers().map(|(id, _)| id).collect();
                    for id in ids {
                        if let Some(buffer) = server.buffer_mut(id) {
                            buffer.expire_typing(now);
                        }
                    }
                }
            }
        }
        Task::none()
    }

    fn on_client_event(&mut self, id: ServerId, event: Event) -> Task<Message> {
        let active = self.active.filter(|(server, _)| *server == id).map(|(_, b)| b);
        let Some(server) = self.servers.iter_mut().find(|s| s.id == id) else {
            return Task::none();
        };
        let registered = matches!(event, Event::Registered(_));
        let ctx = Context { settings: &self.config.settings, active };
        let effects = server.on_event(event, &ctx);
        let alerts = server.take_alerts();
        self.now = Instant::now();
        let mut tasks = vec![self.apply_effects(id, effects), self.alert(id, alerts)];
        if registered {
            tasks.push(self.run_connect_commands(id));
        }
        Task::batch(tasks)
    }

    /// Notifications et alertes de fenêtre pour les messages signalés.
    fn alert(&mut self, id: ServerId, alerts: Vec<Alert>) -> Task<Message> {
        let settings = &self.config.settings;
        let mut attention = false;
        for alert in alerts {
            let wanted = if alert.private { settings.notify_queries } else { settings.notify_mentions };
            // Onglet affiché dans une fenêtre au premier plan : rien à signaler.
            let visible = self.window.focused && self.active == Some((id, alert.buffer));
            if !wanted || visible || (self.window.focused && !settings.notify_when_focused) {
                continue;
            }
            if self.system_notifications {
                let body = if settings.notify_preview { alert.body.as_str() } else { "Nouveau message" };
                notify::show(&alert.title, body, settings.notify_sound);
            }
            attention |= !self.window.focused;
        }
        match self.window.id {
            Some(window) if attention && settings.request_attention => {
                window::request_user_attention(window, Some(window::UserAttention::Informational))
            }
            _ => Task::none(),
        }
    }

    /// Exécute les commandes de connexion du réseau, dans l'onglet serveur.
    fn run_connect_commands(&mut self, id: ServerId) -> Task<Message> {
        let Some(server) = self.server(id) else {
            return Task::none();
        };
        let buffer = server.server_buffer();
        let commands = server.entry.on_connect.clone();
        Task::batch(commands.iter().map(|command| self.run_line(id, buffer, command)).collect::<Vec<_>>())
    }

    /// Exécute une commande sur un réseau, dans l'onglet donné (ou l'onglet serveur).
    fn execute(&mut self, id: ServerId, buffer: Option<BufferId>, command: Command) -> Effects {
        let active = self.active.filter(|(server, _)| *server == id).map(|(_, b)| b);
        let Some(server) = self.servers.iter_mut().find(|s| s.id == id) else {
            return Effects::default();
        };
        let buffer = buffer.unwrap_or_else(|| server.server_buffer());
        let ctx = Context { settings: &self.config.settings, active };
        match server.execute(buffer, command, &ctx) {
            Ok(effects) => effects,
            Err(error) => {
                server.push(buffer, Line::new(Local::now(), LineKind::Error(error)), &ctx);
                Effects::default()
            }
        }
    }

    fn apply_effects(&mut self, id: ServerId, effects: Effects) -> Task<Message> {
        if let Some((port, duration)) = effects.sts
            && let Some(host) = self.server(id).map(|s| s.entry.connection.host.clone())
        {
            self.config.record_sts(&host, port, duration, Local::now().timestamp());
            self.save();
        }
        if effects.config_changed {
            self.persist_servers();
        }
        if effects.closed {
            let fallback = self.server(id).map(Server::server_buffer);
            if let Some(buffer) = fallback {
                return self.select(id, buffer);
            }
        }
        match effects.focus {
            Some(buffer) => self.select(id, buffer),
            None => Task::none(),
        }
    }

    fn select(&mut self, id: ServerId, buffer: BufferId) -> Task<Message> {
        if self.active != Some((id, buffer)) {
            self.stop_typing();
            self.completion = None;
            self.search = None;
        }
        self.active = Some((id, buffer));
        if let Some(server) = self.server_mut(id) {
            server.mark_read(buffer);
        }
        iced::widget::operation::focus(INPUT_ID)
    }

    fn submit(&mut self) -> Task<Message> {
        let text = std::mem::take(&mut self.input);
        self.completion = None;
        self.history_cursor = None;
        if text.trim().is_empty() {
            return Task::none();
        }
        let query = self
            .active_buffer()
            .filter(|(_, buffer)| buffer.kind == Kind::Query)
            .map(|(_, buffer)| buffer.name.as_str());
        // Les mots de passe envoyés à NickServ ne restent pas dans l'historique.
        if self.history.last() != Some(&text) && !secrets::input_has_secret(&text, query) {
            self.history.push(text.clone());
            if self.history.len() > INPUT_HISTORY {
                self.history.remove(0);
            }
        }
        self.stop_typing();

        let Some((id, buffer)) = self.active else {
            return Task::none();
        };
        self.run_line(id, buffer, &text)
    }

    /// Exécute une ligne saisie (texte ou commande) dans un onglet.
    fn run_line(&mut self, id: ServerId, buffer: BufferId, text: &str) -> Task<Message> {
        match input::parse(text) {
            Ok(Input::Command(Command::Ignore(mask))) => return self.ignore(id, buffer, mask),
            Ok(Input::Command(Command::Unignore(mask))) => return self.unignore(id, buffer, &mask),
            _ => {}
        }
        let active = self.active.filter(|(server, _)| *server == id).map(|(_, b)| b);
        let Some(server) = self.servers.iter_mut().find(|s| s.id == id) else {
            return Task::none();
        };
        let ctx = Context { settings: &self.config.settings, active };
        let result = match input::parse(text) {
            Ok(Input::Text(text)) => server.send_text(buffer, &text, &ctx).map(|()| Effects::default()),
            Ok(Input::Command(command)) => server.execute(buffer, command, &ctx),
            Err(error) => Err(error.to_string()),
        };
        match result {
            Ok(effects) => self.apply_effects(id, effects),
            Err(error) => {
                server.push(buffer, Line::new(Local::now(), LineKind::Error(error)), &ctx);
                Task::none()
            }
        }
    }

    /// `/ignore` : ajoute un masque, ou affiche la liste.
    fn ignore(&mut self, id: ServerId, buffer: BufferId, mask: Option<String>) -> Task<Message> {
        let text = match mask {
            None if self.config.settings.ignored.is_empty() => "Personne n'est ignoré.".to_owned(),
            None => format!("Ignorés : {}", self.config.settings.ignored.join(", ")),
            Some(mask) => {
                let normalized = ignore::normalize(&mask);
                let ignored = &mut self.config.settings.ignored;
                if ignored.iter().any(|known| ignore::normalize(known) == normalized) {
                    format!("{mask} est déjà ignoré.")
                } else {
                    ignored.push(mask.clone());
                    self.save();
                    format!("{mask} est désormais ignoré ({normalized}).")
                }
            }
        };
        self.info(id, buffer, text);
        Task::none()
    }

    fn unignore(&mut self, id: ServerId, buffer: BufferId, mask: &str) -> Task<Message> {
        let normalized = ignore::normalize(mask);
        let ignored = &mut self.config.settings.ignored;
        let before = ignored.len();
        ignored.retain(|known| ignore::normalize(known) != normalized);
        let text = if ignored.len() < before {
            self.save();
            format!("{mask} n'est plus ignoré.")
        } else {
            format!("{mask} n'était pas ignoré.")
        };
        self.info(id, buffer, text);
        Task::none()
    }

    /// Ligne d'information dans un onglet.
    fn info(&mut self, id: ServerId, buffer: BufferId, text: String) {
        let active = self.active.filter(|(server, _)| *server == id).map(|(_, b)| b);
        if let Some(server) = self.servers.iter_mut().find(|s| s.id == id) {
            let ctx = Context { settings: &self.config.settings, active };
            server.push(buffer, Line::new(Local::now(), LineKind::Info(text)), &ctx);
        }
    }

    fn update_typing(&mut self) {
        let composing = !self.input.is_empty() && !self.input.starts_with('/');
        if !composing || !self.config.settings.send_typing {
            self.stop_typing();
            return;
        }
        let Some((id, buffer)) = self.active else {
            return;
        };
        let now = Instant::now();
        let recent = self.typing_sent.is_some_and(|sent| {
            sent.server == id && sent.buffer == buffer && now.duration_since(sent.at) < TYPING_THROTTLE
        });
        if recent {
            return;
        }
        if let Some(server) = self.server(id) {
            server.send_typing(buffer, Typing::Active);
        }
        self.typing_sent = Some(TypingSent { server: id, buffer, at: now });
    }

    fn stop_typing(&mut self) {
        if let Some(sent) = self.typing_sent.take()
            && let Some(server) = self.server(sent.server)
        {
            server.send_typing(sent.buffer, Typing::Done);
        }
    }

    fn on_switcher(&mut self, msg: switcher::Msg) -> Task<Message> {
        let Some(Modal::Switcher(switcher)) = &mut self.modal else {
            return Task::none();
        };
        match msg {
            switcher::Msg::Query(query) => {
                switcher.query = query;
                switcher.selected = 0;
                Task::none()
            }
            switcher::Msg::Submit => {
                let results = switcher.results(&self.servers, self.active);
                match results.get(switcher.selected).copied() {
                    Some((server, buffer)) => self.update(Message::Select(server, buffer)),
                    None => Task::none(),
                }
            }
        }
    }

    fn on_shortcut(&mut self, shortcut: Shortcut) -> Task<Message> {
        if let Some(Modal::Switcher(switcher)) = &mut self.modal
            && matches!(shortcut, Shortcut::HistoryPrevious | Shortcut::HistoryNext)
        {
            let count = switcher.results(&self.servers, self.active).len().max(1);
            switcher.selected = if shortcut == Shortcut::HistoryPrevious {
                (switcher.selected + count - 1) % count
            } else {
                (switcher.selected + 1) % count
            };
            return Task::none();
        }
        if self.modal.is_some() {
            return match shortcut {
                Shortcut::Escape => self.update(Message::CloseModal),
                Shortcut::Complete { backwards: false } => iced::widget::operation::focus_next(),
                Shortcut::Complete { backwards: true } => iced::widget::operation::focus_previous(),
                _ => Task::none(),
            };
        }
        match shortcut {
            Shortcut::Complete { .. } if !self.suggestions.is_empty() => {
                return self.accept_suggestion(self.selected_suggestion);
            }
            Shortcut::Complete { backwards } => {
                self.complete(backwards);
                self.history_cursor = None;
            }
            Shortcut::HistoryPrevious | Shortcut::HistoryNext if !self.suggestions.is_empty() => {
                let len = self.suggestions.len();
                self.selected_suggestion = if shortcut == Shortcut::HistoryPrevious {
                    (self.selected_suggestion + len - 1) % len
                } else {
                    (self.selected_suggestion + 1) % len
                };
                return Task::none();
            }
            Shortcut::HistoryPrevious => {
                if self.history.is_empty() {
                    return Task::none();
                }
                let index = match self.history_cursor {
                    None => {
                        self.draft = std::mem::take(&mut self.input);
                        self.history.len() - 1
                    }
                    Some(index) => index.saturating_sub(1),
                };
                self.history_cursor = Some(index);
                self.input.clone_from(&self.history[index]);
            }
            Shortcut::HistoryNext => match self.history_cursor {
                Some(index) if index + 1 < self.history.len() => {
                    self.history_cursor = Some(index + 1);
                    self.input.clone_from(&self.history[index + 1]);
                }
                Some(_) => {
                    self.history_cursor = None;
                    self.input = std::mem::take(&mut self.draft);
                }
                None => return Task::none(),
            },
            Shortcut::Escape if self.search.is_some() => {
                self.search = None;
                return iced::widget::operation::focus(INPUT_ID);
            }
            Shortcut::Escape => {
                self.suggestions_dismissed = true;
                return Task::none();
            }
        }
        iced::widget::operation::move_cursor_to_end(INPUT_ID)
    }

    /// Complétion : commandes, salons, puis interlocuteurs récents et membres.
    fn complete(&mut self, backwards: bool) {
        if let Some(completion) = &mut self.completion
            && completion.text() == self.input
        {
            self.input = completion.cycle(backwards);
            return;
        }
        let Some((server, buffer)) = self.active_buffer() else {
            return;
        };
        let mut candidates: Vec<String> = input::command_names("");
        candidates.extend(server.network.channels().map(|channel| channel.name.clone()));
        candidates.extend(buffer.recent_speakers().map(str::to_owned));
        if let Some(channel) = server.network.channel(&buffer.name) {
            candidates.extend(
                channel
                    .sorted_members(server.network.isupport())
                    .into_iter()
                    .map(|member| member.nick.clone()),
            );
        } else {
            candidates.push(buffer.name.clone());
        }
        candidates.push(server.network.nick().to_owned());

        self.completion = Completion::start(&self.input, candidates.iter().map(String::as_str));
        if let Some(completion) = &self.completion {
            self.input = completion.text();
        } else {
            self.request_bot_completion();
        }
    }

    /// Sans complétion locale, demande aux bots de compléter une commande
    /// (`!...` dans un salon, ou toute saisie en conversation privée).
    fn request_bot_completion(&mut self) {
        let Some((server_id, buffer_id)) = self.active else {
            return;
        };
        let input = self.input.clone();
        let enabled = self.config.settings.bot_completion;
        let Some(server) = self.server_mut(server_id) else {
            return;
        };
        // Seules les commandes de bots (`!...`) sont envoyées : la saisie est
        // visible du destinataire, et jamais adressée aux services.
        let wanted = match server.buffer(buffer_id) {
            Some(buffer) if buffer.kind == Kind::Channel => true,
            Some(buffer) if buffer.kind == Kind::Query => suggest::service(&buffer.name).is_none(),
            _ => false,
        } && input.starts_with('!');
        if enabled && wanted && !input.trim().is_empty() {
            server.request_completion(buffer_id, &input);
        }
    }

    /// Recalcule les suggestions affichées pour la saisie courante.
    fn refresh_suggestions(&mut self) {
        let found = match self.active_buffer() {
            Some(_) if self.suggestions_dismissed || self.history_cursor.is_some() || self.modal.is_some() => Vec::new(),
            Some((server, buffer)) => {
                let buffer_id = self.active.expect("onglet actif").1;
                let mut nicks: Vec<&str> = buffer.recent_speakers().collect();
                match server.network.channel(&buffer.name) {
                    Some(channel) => nicks.extend(
                        channel
                            .sorted_members(server.network.isupport())
                            .into_iter()
                            .map(|member| member.nick.as_str()),
                    ),
                    None if buffer.kind == Kind::Query => nicks.push(&buffer.name),
                    None => {}
                }
                let remote = server
                    .remote_completion
                    .as_ref()
                    .map(|pending| pending.replies_for(buffer_id, &self.input))
                    .unwrap_or_default();
                let mut channels: Vec<&str> = server.network.channels().map(|channel| channel.name.as_str()).collect();
                channels.sort_unstable_by_key(|name| name.to_lowercase());
                let ctx = suggest::Context {
                    query: (buffer.kind == Kind::Query).then_some(buffer.name.as_str()),
                    channels,
                    nicks,
                    bot_commands: buffer.bot_commands().collect(),
                    remote: remote.iter().map(|(bot, text)| (bot.as_str(), text.as_str())).collect(),
                };
                suggest::suggest(&self.input, &ctx)
            }
            None => Vec::new(),
        };
        if found != self.suggestions {
            self.suggestions = found;
            self.selected_suggestion = 0;
        }
    }

    fn accept_suggestion(&mut self, index: usize) -> Task<Message> {
        let Some(suggestion) = self.suggestions.get(index) else {
            return Task::none();
        };
        self.input.clone_from(&suggestion.text);
        self.completion = None;
        self.suggestions_dismissed = false;
        self.update_typing();
        Task::batch([
            iced::widget::operation::focus(INPUT_ID),
            iced::widget::operation::move_cursor_to_end(INPUT_ID),
        ])
    }

    fn save_server_form(&mut self) -> Task<Message> {
        let Some(Modal::Server(form)) = &mut self.modal else {
            return Task::none();
        };
        let entry = match form.validate() {
            Ok(entry) => entry,
            Err(error) => {
                form.set_error(error);
                return Task::none();
            }
        };
        let editing = form.editing;
        // Le nom identifie le réseau, notamment dans le trousseau du système.
        let taken = self
            .servers
            .iter()
            .any(|server| Some(server.id) != editing && server.entry.name.eq_ignore_ascii_case(&entry.name));
        if taken {
            form.set_error("Un réseau porte déjà ce nom.".into());
            return Task::none();
        }
        self.modal = None;

        let task = if let Some(server) = editing.and_then(|id| self.server_mut(id)) {
            let reconnect = server.entry.connection != entry.connection;
            server.entry = entry;
            if reconnect && server.enabled {
                server.generation += 1;
            }
            iced::widget::operation::focus(INPUT_ID)
        } else {
            // « Ajouter et se connecter » : connexion immédiate quel que soit `auto_connect`.
            let id = self.add_server(entry);
            let server = self.server_mut(id).expect("réseau ajouté");
            server.enabled = true;
            let buffer = server.server_buffer();
            self.select(id, buffer)
        };
        self.persist_servers();
        task
    }

    fn persist_servers(&mut self) {
        self.config.servers = self.servers.iter().map(|s| s.entry.clone()).collect();
        self.save();
    }

    fn save(&mut self) {
        let Some(path) = &self.config_path else {
            return;
        };
        let mut on_disk = self.config.clone();
        let use_keychain = self.config.settings.use_keychain;
        let keychain_error = if use_keychain {
            keychain::stash(&mut on_disk, &keychain::System, &mut self.keychain)
        } else {
            None
        };
        if let Err(error) = on_disk.save_to(path) {
            self.banner = Some(format!("Impossible d'enregistrer la configuration : {error}"));
            return;
        }
        // Trousseau désactivé : les mots de passe sont maintenant dans le
        // fichier, les entrées du trousseau peuvent disparaître.
        let keychain_error = keychain_error.or_else(|| (!use_keychain).then(|| keychain::clear(&keychain::System, &mut self.keychain)).flatten());
        if let Some(error) = keychain_error {
            self.banner = Some(format!("Trousseau du système indisponible, mots de passe gardés dans le fichier : {error}"));
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let now = Local::now().timestamp();
        let connections = self.servers.iter().filter(|s| s.enabled).map(|server| {
            let key = ConnectionKey {
                id: server.id,
                generation: server.generation,
                config: self.config.effective_connection(&server.entry.connection, now),
            };
            Subscription::run_with(key, connection_stream)
        });

        let typing = self
            .servers
            .iter()
            .flat_map(Server::buffers)
            .any(|(_, buffer)| buffer.typing(self.now).next().is_some());
        let tick = if typing {
            time::every(Duration::from_secs(1)).map(Message::Tick)
        } else {
            Subscription::none()
        };

        Subscription::batch(connections.chain([
            event::listen_with(runtime_event),
            system::theme_changes().map(Message::SystemTheme),
            tick,
        ]))
    }

    pub fn view(&self) -> Element<'_, Message> {
        let colors = self.colors();
        let settings = self.settings();

        let sidebar = settings
            .show_sidebar
            .then(|| sidebar::view(&self.servers, self.active, colors));
        let main: Element<'_, Message> = match self.active_buffer() {
            Some((server, buffer)) => {
                let id = self.active.expect("onglet actif").1;
                let chat = chat::view(&chat::Props {
                    server,
                    buffer_id: id,
                    buffer,
                    settings,
                    colors,
                    input: &self.input,
                    search: self.search.as_deref(),
                    suggestions: &self.suggestions,
                    selected_suggestion: self.selected_suggestion,
                    banner: self.banner.as_deref(),
                    now: self.now,
                });
                let nicklist = settings
                    .show_nicklist
                    .then(|| server.network.channel(&buffer.name))
                    .flatten()
                    .map(|channel| nicklist::view(server, channel, settings, colors));
                row![chat].push(nicklist).into()
            }
            None => chat::welcome(colors, self.banner.as_deref()),
        };

        let base = container(row![].push(sidebar).push(main))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme::panel(colors.background));

        let Some(modal) = &self.modal else {
            return base.into();
        };
        let dialog = match modal {
            Modal::Settings(form) => form.view(colors, self.window.dark),
            Modal::Server(form) => form.view(colors),
            Modal::Switcher(switcher) => switcher.view(&self.servers, self.active, colors),
        };
        stack![
            base,
            opaque(
                container(center(dialog))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(theme::backdrop)
            )
        ]
        .into()
    }
}

/// Clé d'abonnement d'une connexion : seuls l'identifiant et la génération
/// comptent, pour qu'une modification de configuration ne coupe pas la
/// connexion sans demande explicite.
#[derive(Debug, Clone)]
struct ConnectionKey {
    id: ServerId,
    generation: u64,
    config: Config,
}

impl Hash for ConnectionKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.generation.hash(state);
    }
}

fn connection_stream(key: &ConnectionKey) -> impl Stream<Item = Message> + use<> {
    let id = key.id;
    ircnext_client::connect(key.config.clone()).map(move |event| Message::Client(id, event))
}

fn runtime_event(event: iced::Event, status: event::Status, window: window::Id) -> Option<Message> {
    match event {
        iced::Event::Keyboard(keyboard::Event::KeyPressed { key, physical_key, modifiers, .. }) => {
            let combo = Combo::from_key(&key, physical_key, modifiers, keymap::MAC)?;
            Some(Message::Key { combo, captured: status == event::Status::Captured })
        }
        iced::Event::Window(window::Event::Focused) => Some(Message::WindowFocus { window, focused: true }),
        iced::Event::Window(window::Event::Unfocused) => Some(Message::WindowFocus { window, focused: false }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::ThemeMode;
    use ircnext_client::proto::Message as IrcMessage;
    use ircnext_client::Registered;

    /// Application de démonstration alimentée par de faux événements réseau.
    fn demo() -> App {
        let mut config = AppConfig::default();
        config.servers.push(ServerEntry {
            name: "Libera.Chat".into(),
            auto_connect: false,
            on_connect: Vec::new(),
            connection: Config { host: "irc.libera.chat".into(), nick: "me".into(), ..Config::default() },
        });
        let mut app = App::new(config, None, None);
        let id = app.servers[0].id;
        let registered = Registered { nick: "me".into(), caps: vec!["echo-message".into()], sts: None, sasl_refused: false };
        let _ = app.update(Message::Client(id, Event::Registered(registered)));
        for line in [
            ":s 005 me CHANTYPES=# PREFIX=(ov)@+ NETWORK=Libera.Chat MONITOR=100 :are supported",
            ":me!u@h JOIN #rust",
            ":s 332 me #rust :Le langage Rust · https://www.rust-lang.org · soyez bienveillants",
            ":s 353 me = #rust :@alice +bob carol dave erin @me",
            ":s 366 me #rust :End",
            ":me!u@h JOIN #ircnext",
            ":alice!a@h PRIVMSG #rust :Quelqu'un a testé la nouvelle version de \x02tokio\x02 ?",
            ":bob!b@h PRIVMSG #rust :oui, la doc est ici : https://docs.rs/tokio",
            ":carol!c@h PRIVMSG #rust :\x0304rouge\x03, \x0309vert\x03 et \x1ditalique\x1d : les codes mIRC marchent",
            ":dave!d@h PRIVMSG #rust :\x01ACTION prépare un café\x01",
            ":erin!e@h JOIN #rust",
            ":alice!a@h PRIVMSG #rust :me: tu en penses quoi d'ircnext ?",
            ":me!u@h PRIVMSG #rust :franchement, ça a l'air propre",
            "@bot :helper!h@h PRIVMSG #rust :Rappel : réunion à 18 h",
            ":bob!b@h NICK bobby",
            ":zoe!z@h PRIVMSG me :salut, tu as deux minutes ?",
            ":carol!c@h PRIVMSG #ircnext :hello",
            "@+typing=active :carol!c@h TAGMSG #rust",
        ] {
            let message: IrcMessage = line.parse().unwrap();
            let _ = app.update(Message::Client(id, Event::Message(message)));
        }
        let buffer = app.servers[0].find("#rust").unwrap();
        let _ = app.update(Message::Select(id, buffer));
        app.input = "Message en cours de saisie".into();
        app
    }

    #[test]
    fn demo_state_is_consistent() {
        let app = demo();
        let server = &app.servers[0];
        let rust = server.buffer(server.find("#rust").unwrap()).unwrap();
        assert_eq!(rust.unread, 0, "onglet actif");
        let query = server.buffer(server.find("zoe").unwrap()).unwrap();
        assert!(query.highlighted);
        assert_eq!(app.title(), "(1) #rust — Libera.Chat · ircnext", "message privé de zoe non lu");
    }

    fn inject(app: &mut App, line: &str) {
        let id = app.servers[0].id;
        let message: IrcMessage = line.parse().unwrap();
        let _ = app.update(Message::Client(id, Event::Message(message)));
    }

    fn labels(app: &App) -> Vec<&str> {
        app.suggestions.iter().map(|s| s.label.as_str()).collect()
    }

    #[test]
    fn suggestions_follow_typing_and_keyboard() {
        let mut app = demo();
        let _ = app.update(Message::InputChanged("/jo".into()));
        assert_eq!(labels(&app), ["/join #salon [clé]"]);
        let _ = app.update(Message::Submit);
        assert_eq!(app.input, "/join ", "Entrée valide la suggestion");

        let _ = app.update(Message::InputChanged("/join #".into()));
        assert_eq!(labels(&app), ["#ircnext", "#rust"]);
        let _ = app.update(Message::Shortcut(Shortcut::HistoryNext));
        let _ = app.update(Message::Shortcut(Shortcut::Complete { backwards: false }));
        assert_eq!(app.input, "/join #rust ", "↓ puis Tab");

        let _ = app.update(Message::InputChanged("/ns ".into()));
        assert!(labels(&app).contains(&"IDENTIFY [compte] <mot de passe>"));
        let _ = app.update(Message::Shortcut(Shortcut::Escape));
        assert_eq!(labels(&app), Vec::<&str>::new());
        let _ = app.update(Message::InputChanged("/ns ID".into()));
        assert_eq!(app.suggestions.len(), 1, "réouverture à la frappe suivante");
    }

    #[test]
    fn suggests_learned_and_remote_bot_commands() {
        let mut app = demo();
        inject(&mut app, "@bot :helper!h@h PRIVMSG #rust :Commandes : !meteo, !quote");
        inject(&mut app, ":carol!c@h PRIVMSG #rust :!quote 42");
        let _ = app.update(Message::InputChanged("!".into()));
        assert_eq!(labels(&app), ["!quote", "!meteo"]);

        // Réponse d'un bot (brouillon IRCv3) à une demande envoyée pour « !meteo Par ».
        let (_, buffer) = app.active.unwrap();
        app.servers[0].remote_completion = Some(crate::server::RemoteCompletion {
            buffer,
            request: "!meteo Par".into(),
            sent: Instant::now(),
            replies: Vec::new(),
        });
        let _ = app.update(Message::InputChanged("!meteo Par".into()));
        inject(&mut app, "@+draft/autocomplete-response=is\t\u{8}\u{8}\u{8}Pau :helper!h@h TAGMSG #rust");
        assert_eq!(labels(&app), ["Paris", "Pau"]);
        assert_eq!(app.suggestions[1].source, "helper");
        let _ = app.update(Message::Submit);
        assert_eq!(app.input, "!meteo Paris");
        assert_eq!(labels(&app), Vec::<&str>::new(), "réponse périmée une fois la saisie modifiée");
    }

    #[test]
    fn ignore_commands_update_settings() {
        let mut app = demo();
        app.input = "/ignore Troll".into();
        let _ = app.update(Message::Submit);
        assert_eq!(app.config.settings.ignored, ["Troll"]);
        app.input = "/ignore troll".into();
        let _ = app.update(Message::Submit);
        assert_eq!(app.config.settings.ignored.len(), 1, "pas de doublon (casse ignorée)");
        app.input = "/unignore troll".into();
        let _ = app.update(Message::Submit);
        assert_eq!(app.config.settings.ignored, Vec::<String>::new());
    }

    #[test]
    fn runs_connect_commands_after_registration() {
        let mut app = demo();
        app.servers[0].entry.on_connect = vec!["/ignore bavard".into()];
        let id = app.servers[0].id;
        let registered = Registered { nick: "me".into(), caps: vec![], sts: None, sasl_refused: false };
        let _ = app.update(Message::Client(id, Event::Registered(registered)));
        assert_eq!(app.config.settings.ignored, ["bavard"]);
    }

    #[test]
    fn network_names_are_unique() {
        let mut app = demo();
        let _ = app.update(Message::OpenServerForm(None));
        for msg in [server_form::Msg::Name("libera.chat".into()), server_form::Msg::Host("irc.example.net".into())] {
            let _ = app.update(Message::Form(msg));
        }
        let _ = app.update(Message::SaveServerForm);
        assert_eq!(app.servers.len(), 1, "nom déjà pris");
        assert!(matches!(app.modal, Some(Modal::Server(_))), "le formulaire reste ouvert");

        // Modifier un réseau sans changer son nom reste possible.
        let id = app.servers[0].id;
        let _ = app.update(Message::OpenServerForm(Some(id)));
        let _ = app.update(Message::Form(server_form::Msg::Host("irc.eu.libera.chat".into())));
        let _ = app.update(Message::SaveServerForm);
        assert!(app.modal.is_none());
        assert_eq!(app.servers[0].entry.connection.host, "irc.eu.libera.chat");
    }

    #[test]
    fn title_counts_unread_mentions() {
        let mut app = demo();
        inject(&mut app, ":alice!a@h PRIVMSG #ircnext :me: regarde ça");
        assert!(app.title().starts_with("(2) "), "zoe et #ircnext : {}", app.title());
        app.config.settings.title_unread_count = false;
        assert!(!app.title().starts_with('('));
    }

    fn press(app: &mut App, combo: &str) {
        let combo = Combo::parse(combo, keymap::MAC).expect("combinaison valide");
        let _ = app.update(Message::Key { combo, captured: false });
    }

    fn active_name(app: &App) -> String {
        app.active_buffer().map(|(_, buffer)| buffer.name.clone()).unwrap_or_default()
    }

    #[test]
    fn navigation_shortcuts() {
        let mut app = demo();
        // Depuis #rust, seul « zoe » est non lu après lui : Alt+Maj+↓ y mène.
        press(&mut app, "Alt+Shift+Down");
        assert_eq!(active_name(&app), "zoe");

        let goto = if keymap::MAC { "Mod+1" } else { "Alt+1" };
        press(&mut app, goto);
        assert_eq!(active_name(&app), "Libera.Chat");
        press(&mut app, "Ctrl+Tab");
        assert_eq!(active_name(&app), app.servers[0].buffers().nth(1).unwrap().1.name);
        let last = if keymap::MAC { "Mod+9" } else { "Alt+9" };
        press(&mut app, last);
        assert_eq!(active_name(&app), app.servers[0].buffers().last().unwrap().1.name);

        press(&mut app, "Shift+Escape");
        assert!(app.servers[0].buffers().all(|(_, buffer)| buffer.unread == 0));
    }

    #[test]
    fn editing_and_display_shortcuts() {
        let mut app = demo();
        app.input = "salut".into();
        press(&mut app, "Mod+B");
        assert_eq!(app.input, "salut\x02");

        let size = app.config.settings.font_size;
        press(&mut app, "Mod+Equal");
        assert_eq!(app.config.settings.font_size, size + 1);
        press(&mut app, "Mod+0");
        assert_eq!(app.config.settings.font_size, Settings::default().font_size);

        press(&mut app, "Mod+Shift+S");
        assert!(!app.config.settings.show_sidebar);
    }

    #[test]
    fn quick_switcher_and_search() {
        let mut app = demo();
        press(&mut app, "Mod+T");
        assert!(matches!(app.modal, Some(Modal::Switcher(_))));
        let _ = app.update(Message::Switcher(switcher::Msg::Query("irc".into())));
        let _ = app.update(Message::Switcher(switcher::Msg::Submit));
        assert!(app.modal.is_none());
        assert_eq!(active_name(&app), "#ircnext");

        press(&mut app, "Mod+F");
        assert_eq!(app.search.as_deref(), Some(""));
        let _ = app.update(Message::Key { combo: Combo::parse("Escape", keymap::MAC).unwrap(), captured: false });
        assert_eq!(app.search, None);
    }

    #[test]
    fn custom_shortcuts_replace_defaults() {
        let mut app = demo();
        app.config.settings.keybindings.insert(Action::QuickSwitcher, vec!["F2".into()]);
        press(&mut app, "Mod+T");
        assert!(app.modal.is_none());
        press(&mut app, "F2");
        assert!(matches!(app.modal, Some(Modal::Switcher(_))));
    }

    /// Génère des captures de l'interface dans `target/snapshots/`.
    /// `cargo test -p ircnext -- --ignored snapshots`
    #[test]
    #[ignore = "génère des fichiers PNG"]
    fn snapshots() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/snapshots");
        let _ = std::fs::remove_dir_all(&dir);
        let mut app = demo();
        for mode in ThemeMode::ALL.into_iter().skip(1) {
            app.config.settings.theme = mode;
            render(&app, &dir.join(format!("{mode:?}").to_lowercase()));
        }
        app.config.settings.theme = ThemeMode::Dark;
        let _ = app.update(Message::OpenSettings);
        for section in settings_form::Section::ALL {
            let _ = app.update(Message::Settings(settings_form::Msg::Section(section)));
            render(&app, &dir.join(format!("settings-{section:?}").to_lowercase()));
        }
        let _ = app.update(Message::CloseModal);
        let _ = app.update(Message::InputChanged("/".into()));
        render(&app, &dir.join("suggestions"));
        let _ = app.update(Message::OpenServerForm(None));
        render(&app, &dir.join("server-form"));
        let _ = app.update(Message::CloseModal);
        let _ = app.update(Message::OpenSettings);
        let _ = app.update(Message::Settings(settings_form::Msg::Section(settings_form::Section::Shortcuts)));
        render(&app, &dir.join("shortcuts"));
        let _ = app.update(Message::CloseModal);
        let _ = app.update(Message::Action(Action::QuickSwitcher));
        render(&app, &dir.join("switcher"));
        let _ = app.update(Message::CloseModal);
        let _ = app.update(Message::Action(Action::Search));
        let _ = app.update(Message::SearchChanged("rust".into()));
        render(&app, &dir.join("search"));
    }

    fn render(app: &App, path: &std::path::Path) {
        let mut simulator =
            iced_test::Simulator::with_size(iced::Settings::default(), (1200.0, 780.0), app.view());
        let snapshot = simulator.snapshot(&app.theme()).expect("rendu");
        assert!(snapshot.matches_image(path).expect("écriture PNG"));
    }
}
