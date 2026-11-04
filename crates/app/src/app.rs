//! Global application state, messages and update logic (Elm architecture).

use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::Local;
use futures::{Stream, StreamExt};
use iced::keyboard;
use iced::widget::{center, column, container, opaque, row, stack};
use iced::{Element, Length, Subscription, Task, Theme, event, system, time, window};
use ircnext_client::proto::ircv3::Typing;
use ircnext_client::{Config, Event};

use crate::buffer::{Kind, Line, LineKind};
use crate::completion::Completion;
use crate::secrets;
use crate::suggest::{self, Suggestion};
use crate::config::{AppConfig, ServerEntry};
use crate::i18n::{self, tr};
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
use crate::tr;

mod keys;
mod scripts;
mod transfers;
mod updates;

/// Id of the input field (to focus it).
pub const INPUT_ID: &str = "input";
/// Minimum interval between two “active” typing notifications.
const TYPING_THROTTLE: Duration = Duration::from_secs(3);
/// Maximum size of the input history.
const INPUT_HISTORY: usize = 100;

#[derive(Debug, Clone)]
pub enum Message {
    Client(ServerId, Event),
    Select(ServerId, BufferId),
    InputChanged(String),
    AcceptSuggestion(usize),
    Submit,
    /// Key press; `captured`: already handled by a widget.
    Key { combo: Combo, captured: bool },
    Shortcut(Shortcut),
    Action(Action),
    Switcher(switcher::Msg),
    SearchChanged(String),
    /// The window gains or loses focus.
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
    OpenTransfers,
    ToggleDoNotDisturb,
    /// Copies the active tab's text to the clipboard.
    CopyBuffer,
    Transfers(crate::ui::transfers::Msg),
    /// Event of a running file transfer.
    Transfer(crate::transfer::TransferId, crate::transfer::Event),
    /// A file was dropped on the window.
    FileDropped(PathBuf),
    /// Script timers to check.
    TimerTick(Instant),
    /// Script manager: loads the scripts again.
    ReloadScripts,
    /// Script manager: turns a script on or off.
    ToggleScript(String, bool),
    /// Opens the scripts folder, or one script file.
    OpenScripts(Option<String>),
    /// Settings: looks for a new version now.
    CheckUpdates,
    UpdateChecked { manual: bool, result: Result<Option<crate::update::Release>, String> },
    /// Downloads and opens the new version's installer.
    InstallUpdate,
    UpdateDownloaded(Result<PathBuf, String>),
    DismissUpdate,
    /// Sends the pasted text waiting for confirmation, line by line.
    PasteSend,
    /// Sends it as one line.
    PasteJoined,
    Links(crate::ui::links::Msg),
    ChannelList(crate::ui::channel_list::Msg),
    ChannelCentral(crate::ui::channel_central::Msg),
    /// Opens the settings window of a channel.
    OpenChannelCentral(ServerId, String),
    /// Clipboard text, read after a paste in the input.
    Pasted(Option<String>),
}

/// Keys with a fixed role (not configurable).
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
    Transfers,
    Paste(crate::ui::paste::Paste),
    Links(crate::ui::links::Links),
    ChannelList(crate::ui::channel_list::ChannelListView),
    ChannelCentral(crate::ui::channel_central::ChannelCentral),
}

/// State of the main window.
#[derive(Debug, Clone, Copy)]
struct WindowState {
    id: Option<window::Id>,
    focused: bool,
    /// The system is in dark mode.
    dark: bool,
}

/// Last typing notification sent.
#[derive(Debug, Clone, Copy)]
struct TypingSent {
    server: ServerId,
    buffer: BufferId,
    at: Instant,
}

#[derive(Debug)]
pub struct App {
    config: AppConfig,
    /// Configuration file; `None` disables saving (tests).
    config_path: Option<PathBuf>,
    servers: Vec<Server>,
    next_server: u64,
    active: Option<(ServerId, BufferId)>,
    input: String,
    completion: Option<Completion>,
    /// Suggestions shown above the input.
    suggestions: Vec<Suggestion>,
    selected_suggestion: usize,
    /// Suggestions closed with Esc, until the next keystroke.
    suggestions_dismissed: bool,
    history: Vec<String>,
    history_cursor: Option<usize>,
    /// Input set aside while browsing the history.
    draft: String,
    typing_sent: Option<TypingSent>,
    modal: Option<Modal>,
    /// Search in the current tab (bar shown if `Some`).
    search: Option<String>,
    window: WindowState,
    /// System notifications enabled (never in tests).
    system_notifications: bool,
    /// Passwords already stored in the system keychain.
    keychain: keychain::Cache,
    /// mIRC scripts (`scripts/*.mrc`).
    scripts: ircnext_script::Interpreter,
    /// State of each script file, for the script manager.
    script_status: Vec<crate::ui::settings_form::ScriptStatus>,
    /// Script timers (`/timer`).
    timers: Vec<scripts::ScriptTimer>,
    next_timer: u32,
    /// DCC file transfers of the session.
    transfers: Vec<crate::transfer::Transfer>,
    next_transfer: u64,
    banner: Option<String>,
    /// Newer version found on GitHub.
    update: Option<updates::UpdateNotice>,
    checking_update: bool,
    /// Result of the last check, for the settings.
    update_status: String,
    now: Instant,
}

impl App {
    pub fn boot() -> (Self, Task<Message>) {
        let (mut config, error) = AppConfig::load();
        i18n::set_language(config.settings.language);
        let (keychain, missing) = keychain::restore(&mut config, &keychain::System);
        let banner = error.map(|error| tr!("Configuration: {error}", error = error)).or_else(|| {
            (!missing.is_empty())
                .then(|| tr!("SASL password not found in the keychain: {networks}", networks = missing.join(", ")))
        });
        let mut app = Self::new(config, AppConfig::path().ok(), banner);
        app.keychain = keychain;
        let (_, script_errors) = app.load_scripts();
        if !script_errors.is_empty() && app.banner.is_none() {
            app.banner = Some(tr!("Script errors: {errors}", errors = script_errors.join(" · ")));
        }
        let start = app.active.map(|(server, buffer)| {
            let event = ircnext_script::Event { name: "START".into(), own: true, ..ircnext_script::Event::default() };
            app.fire_script_event(server, buffer, &event).1
        });
        app.system_notifications = true;
        app.sync_prefs();
        let tasks = Task::batch([
            system::theme().map(Message::SystemTheme),
            iced::widget::operation::focus(INPUT_ID),
            start.unwrap_or_else(Task::none),
            app.startup_update_check(),
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
            scripts: ircnext_script::Interpreter::default(),
            script_status: Vec::new(),
            timers: Vec::new(),
            next_timer: 0,
            transfers: Vec::new(),
            next_transfer: 0,
            banner,
            update: None,
            checking_update: false,
            update_status: String::new(),
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
        // Tabs waiting for an answer: unread mention or private message.
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

    /// Preferences passed on to the networks.
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

    /// Logs folder, next to the configuration file.
    fn log_dir(&self) -> Option<PathBuf> {
        Some(self.config_path.as_ref()?.parent()?.join("logs"))
    }

    pub fn theme(&self) -> Theme {
        self.colors().theme()
    }

    /// Effective settings (those of the open panel, for the live preview).
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
                        self.banner = Some(tr!("Cannot open the logs folder: {error}", error = error));
                    }
                }
            }
            Message::Exit => return iced::exit(),
            Message::OpenUrl(link) => match ircnext_client::proto::text::safe_url(&link) {
                Some(url) => {
                    if let Err(error) = open::that_detached(&url) {
                        self.banner = Some(tr!("Cannot open the link: {error}", error = error));
                    }
                }
                None => self.banner = Some(tr!("Link refused (not a web address, or suspicious): {link}", link = link)),
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
                let mut form = SettingsForm::new(&self.config.settings);
                form.update_status.clone_from(&self.update_status);
                self.modal = Some(Modal::Settings(form));
            }
            Message::Settings(msg) => {
                if let Some(Modal::Settings(form)) = &mut self.modal {
                    form.update(msg);
                }
            }
            Message::SaveSettings => {
                if let Some(Modal::Settings(form)) = self.modal.take() {
                    self.config.settings = form.finish();
                    i18n::set_language(self.config.settings.language);
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
            Message::CheckUpdates => return self.check_updates(true),
            Message::UpdateChecked { manual, result } => self.update_checked(manual, result),
            Message::InstallUpdate => return self.install_update(),
            Message::UpdateDownloaded(result) => return self.update_downloaded(result),
            Message::DismissUpdate => self.update = None,
            Message::Pasted(text) => self.on_paste(text),
            Message::Links(msg) => return self.on_links(msg),
            Message::ChannelList(msg) => return self.on_channel_list(msg),
            Message::ChannelCentral(msg) => self.on_channel_central(msg),
            Message::OpenChannelCentral(id, channel) => {
                let effects = self.execute(id, None, Command::Channel(Some(channel)));
                return self.apply_effects(id, effects);
            }
            Message::PasteSend | Message::PasteJoined => {
                let Some(Modal::Paste(paste)) = self.modal.take() else {
                    return Task::none();
                };
                self.input.clone_from(&paste.input);
                let text = if matches!(message, Message::PasteJoined) { paste.joined() } else { paste.text };
                return Task::batch([self.send_input(paste.server, paste.buffer, &text), iced::widget::operation::focus(INPUT_ID)]);
            }
            Message::OpenTransfers => self.modal = Some(Modal::Transfers),
            Message::TimerTick(now) => return self.run_timers(now),
            Message::ReloadScripts => {
                self.load_scripts();
            }
            Message::ToggleScript(name, enabled) => self.toggle_script(&name, enabled),
            Message::OpenScripts(file) => self.open_script_path(file.as_deref()),
            Message::ToggleDoNotDisturb => {
                self.config.settings.do_not_disturb = !self.config.settings.do_not_disturb;
                self.save();
            }
            Message::CopyBuffer => {
                if let Some((_, buffer)) = self.active_buffer() {
                    let text: Vec<String> = buffer.lines().map(crate::log::entry).collect();
                    return iced::clipboard::write(text.join("\n"));
                }
            }
            Message::Transfers(msg) => return self.on_transfers(&msg),
            Message::Transfer(id, event) => self.on_transfer_event(id, event),
            Message::FileDropped(path) => {
                // Dropped on a private conversation: offer the file to that person.
                let target = self
                    .active_buffer()
                    .filter(|(server, buffer)| buffer.kind == Kind::Query && server.is_online())
                    .map(|(server, buffer)| (server.id, buffer.name.clone()));
                match target {
                    Some((server, nick)) => return self.offer_file(server, &nick, &path),
                    None => self.banner = Some(tr("Drop a file on a private conversation to send it.").to_owned()),
                }
            }
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
        let dcc = server.take_dcc();
        let script_events = server.take_script_events();
        self.now = Instant::now();
        let mut tasks = vec![
            self.apply_effects(id, effects),
            self.alert(id, alerts),
            self.on_dcc(id, dcc),
            self.fire_script_events(id, script_events),
        ];
        if registered {
            tasks.push(self.run_connect_commands(id));
        }
        Task::batch(tasks)
    }

    /// Notifications and window alerts for flagged messages.
    fn alert(&mut self, id: ServerId, alerts: Vec<Alert>) -> Task<Message> {
        let settings = &self.config.settings;
        if settings.do_not_disturb {
            return Task::none();
        }
        let mut attention = false;
        for alert in alerts {
            let wanted = if alert.private { settings.notify_queries } else { settings.notify_mentions };
            // Tab shown in a foreground window: nothing to report.
            let visible = self.window.focused && self.active == Some((id, alert.buffer));
            if !wanted || visible || (self.window.focused && !settings.notify_when_focused) {
                continue;
            }
            if self.system_notifications {
                let body = if settings.notify_preview { alert.body.as_str() } else { tr("New message") };
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

    /// Runs the network's connect commands, in the server tab.
    fn run_connect_commands(&mut self, id: ServerId) -> Task<Message> {
        let Some(server) = self.server(id) else {
            return Task::none();
        };
        let buffer = server.server_buffer();
        let commands = server.entry.on_connect.clone();
        Task::batch(commands.iter().map(|command| self.run_line(id, buffer, command)).collect::<Vec<_>>())
    }

    /// Runs a command on a network, in the given tab (or the server tab).
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
        if let Some(channel) = effects.channel_central.and_then(|buffer| self.server(id)?.buffer(buffer)).map(|buffer| buffer.name.clone()) {
            self.modal = Some(Modal::ChannelCentral(crate::ui::channel_central::ChannelCentral::new(id, channel)));
            return Task::none();
        }
        if effects.channel_list {
            self.modal = Some(Modal::ChannelList(crate::ui::channel_list::ChannelListView::new(id)));
            return iced::widget::operation::focus(crate::ui::channel_list::CHANNEL_LIST_ID);
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
            if let Some((server, previous)) = self.active
                && let Some(previous) = self.server_mut(server).and_then(|server| server.buffer_mut(previous))
            {
                previous.set_marker();
            }
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
        // Passwords sent to NickServ are not kept in the history.
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
        self.send_input(id, buffer, &text)
    }

    /// Text pasted in the input. The input field drops line breaks, so the
    /// clipboard is read again: several lines are confirmed before being
    /// sent one message each, fewer are kept on one line.
    fn on_paste(&mut self, clipboard: Option<String>) {
        let Some(pasted) = clipboard.filter(|text| text.contains('\n')) else {
            return;
        };
        let flat: String = pasted.chars().filter(|c| !c.is_control()).collect();
        // Pasted somewhere else (the search field).
        let Some(at) = self.input.rfind(&flat).filter(|_| !flat.is_empty()) else {
            return;
        };
        let mut input = self.input.clone();
        input.replace_range(at..at + flat.len(), "");
        let threshold = usize::from(self.config.settings.paste_confirm_lines);
        let target = self.active.zip(self.active_buffer().map(|(_, buffer)| buffer.name.clone()));
        match target {
            Some(((server, buffer), target)) if threshold > 0 && crate::ui::paste::line_count(&pasted) >= threshold => {
                self.input.clone_from(&input);
                self.modal = Some(Modal::Paste(crate::ui::paste::Paste { server, buffer, target, text: pasted, input }));
            }
            _ => {
                let joined = pasted.lines().map(str::trim).filter(|line| !line.is_empty()).collect::<Vec<_>>().join(" ");
                input.insert_str(at, &joined);
                self.input = input;
            }
        }
    }

    /// Opens the links panel.
    pub(super) fn open_links(&mut self) -> Task<Message> {
        self.modal = Some(Modal::Links(crate::ui::links::Links::new(&self.servers)));
        iced::widget::operation::focus(crate::ui::links::LINKS_ID)
    }

    fn on_links(&mut self, msg: crate::ui::links::Msg) -> Task<Message> {
        use crate::ui::links::Msg;
        match msg {
            Msg::Query(query) => {
                if let Some(Modal::Links(links)) = &mut self.modal {
                    links.query = query;
                }
                Task::none()
            }
            Msg::Copy(url) => iced::clipboard::write(url),
            Msg::GoTo(server, buffer) => {
                self.modal = None;
                self.update(Message::Select(server, buffer))
            }
        }
    }

    fn on_channel_central(&mut self, msg: crate::ui::channel_central::Msg) {
        use crate::ui::channel_central::Msg;
        let Some(Modal::ChannelCentral(central)) = &mut self.modal else {
            return;
        };
        let Some(server) = self.servers.iter().find(|server| server.id == central.server) else {
            return;
        };
        let channel = central.channel.clone();
        let result = match msg {
            Msg::Tab(mode) => {
                central.tab = mode;
                Ok(())
            }
            Msg::Topic(value) => {
                central.topic = Some(value);
                Ok(())
            }
            Msg::Key(value) => {
                central.key = Some(value);
                Ok(())
            }
            Msg::Limit(value) => {
                central.limit = Some(value.chars().filter(char::is_ascii_digit).collect());
                Ok(())
            }
            Msg::NewMask(value) => {
                central.new_mask = value;
                Ok(())
            }
            Msg::SaveTopic => match central.topic.take() {
                Some(topic) => server.set_topic(&channel, topic.trim()),
                None => Ok(()),
            },
            Msg::Mode(mode, on) => server.change_mode(&channel, &format!("{}{mode}", if on { '+' } else { '-' }), &[]),
            Msg::SaveKey => match central.key.take().map(|key| key.trim().to_owned()) {
                Some(key) if key.is_empty() => {
                    // Servers want the current key (or `*`) to remove it.
                    let current = server.network.channel(&channel).and_then(|c| c.modes.get(&'k').cloned().flatten());
                    server.change_mode(&channel, "-k", &[current.as_deref().unwrap_or("*")])
                }
                Some(key) => server.change_mode(&channel, "+k", &[key.split_whitespace().next().unwrap_or_default()]),
                None => Ok(()),
            },
            Msg::SaveLimit => match central.limit.take() {
                Some(limit) if limit.is_empty() || limit == "0" => server.change_mode(&channel, "-l", &[]),
                Some(limit) => server.change_mode(&channel, "+l", &[&limit]),
                None => Ok(()),
            },
            Msg::AddMask => {
                let mask = std::mem::take(&mut central.new_mask);
                let mask = mask.trim();
                if mask.is_empty() || mask.contains(' ') {
                    Ok(())
                } else {
                    server.change_mode(&channel, &format!("+{}", central.tab.unwrap_or('b')), &[mask])
                }
            }
            Msg::RemoveMask(mask) => server.change_mode(&channel, &format!("-{}", central.tab.unwrap_or('b')), &[&mask]),
        };
        if let Err(error) = result {
            self.banner = Some(error);
        }
    }

    fn on_channel_list(&mut self, msg: crate::ui::channel_list::Msg) -> Task<Message> {
        use crate::ui::channel_list::Msg;
        let Some(Modal::ChannelList(list)) = &mut self.modal else {
            return Task::none();
        };
        let id = list.server;
        match msg {
            Msg::Query(query) => {
                list.query = query;
                Task::none()
            }
            Msg::Refresh => {
                if let Some(Err(error)) = self.server_mut(id).map(|server| server.request_list(None)) {
                    self.banner = Some(error);
                }
                Task::none()
            }
            Msg::Join(channel) => {
                self.modal = None;
                let effects = self.execute(id, None, Command::Join { channels: channel, key: None });
                self.apply_effects(id, effects)
            }
        }
    }

    /// Sends typed text, after the scripts' `on INPUT` handlers.
    fn send_input(&mut self, id: ServerId, buffer: BufferId, text: &str) -> Task<Message> {
        // `on INPUT` handlers see the text first, and may stop it.
        let (halted, task) = self.input_event(id, buffer, text);
        if halted {
            return task;
        }
        Task::batch([task, self.run_line(id, buffer, text)])
    }

    /// Runs a typed line (text or command) in a tab.
    fn run_line(&mut self, id: ServerId, buffer: BufferId, text: &str) -> Task<Message> {
        // A script alias takes precedence over the client's commands.
        if let Some(command) = text.strip_prefix('/').filter(|command| !command.starts_with('/')) {
            let (name, args) = command.split_once(' ').unwrap_or((command, ""));
            if self.scripts.has_alias(name) {
                return self.run_alias(id, buffer, name, args);
            }
        }
        self.run_builtin(id, buffer, text)
    }

    /// Runs a typed line with the client's own commands only.
    fn run_builtin(&mut self, id: ServerId, buffer: BufferId, text: &str) -> Task<Message> {
        if let Some(task) = self.timer_command(id, buffer, text) {
            return task;
        }
        match input::parse(text) {
            Ok(Input::Command(Command::Reload)) => return self.reload_scripts(id, buffer),
            Ok(Input::Command(Command::Ignore(mask))) => return self.ignore(id, buffer, mask),
            Ok(Input::Command(Command::Unignore(mask))) => return self.unignore(id, buffer, &mask),
            Ok(Input::Command(Command::Transfers)) => {
                self.modal = Some(Modal::Transfers);
                return Task::none();
            }
            Ok(Input::Command(Command::Links)) => return self.open_links(),
            Ok(Input::Command(Command::DccSend { nick, path })) => {
                return self.offer_file(id, &nick, &crate::transfer::expand_home(&path));
            }
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

    /// `/ignore`: adds a mask, or shows the list.
    fn ignore(&mut self, id: ServerId, buffer: BufferId, mask: Option<String>) -> Task<Message> {
        let text = match mask {
            None if self.config.settings.ignored.is_empty() => tr("Nobody is ignored.").to_owned(),
            None => tr!("Ignored: {masks}", masks = self.config.settings.ignored.join(", ")),
            Some(mask) => {
                let normalized = ignore::normalize(&mask);
                let ignored = &mut self.config.settings.ignored;
                if ignored.iter().any(|known| ignore::normalize(known) == normalized) {
                    tr!("{mask} is already ignored.", mask = mask)
                } else {
                    ignored.push(mask.clone());
                    self.save();
                    tr!("{mask} is now ignored ({normalized}).", mask = mask, normalized = normalized)
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
            tr!("{mask} is no longer ignored.", mask = mask)
        } else {
            tr!("{mask} was not ignored.", mask = mask)
        };
        self.info(id, buffer, text);
        Task::none()
    }

    /// Information line in a tab.
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

    /// Completion: commands, channels, then recent speakers and members.
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

    /// Without a local completion, asks bots to complete a command (`!...`
    /// in a channel or a private conversation).
    fn request_bot_completion(&mut self) {
        let Some((server_id, buffer_id)) = self.active else {
            return;
        };
        let input = self.input.clone();
        let enabled = self.config.settings.bot_completion;
        let Some(server) = self.server_mut(server_id) else {
            return;
        };
        // Only bot commands (`!...`) are sent: the input is visible to the
        // recipient, and never sent to services.
        let wanted = match server.buffer(buffer_id) {
            Some(buffer) if buffer.kind == Kind::Channel => true,
            Some(buffer) if buffer.kind == Kind::Query => suggest::service(&buffer.name).is_none(),
            _ => false,
        } && input.starts_with('!');
        if enabled && wanted && !input.trim().is_empty() {
            server.request_completion(buffer_id, &input);
        }
    }

    /// Recomputes the suggestions shown for the current input.
    fn refresh_suggestions(&mut self) {
        let found = match self.active_buffer() {
            Some(_) if self.suggestions_dismissed || self.history_cursor.is_some() || self.modal.is_some() => Vec::new(),
            Some((server, buffer)) => {
                let buffer_id = self.active.expect("active tab").1;
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
        // The name identifies the network, in the system keychain in particular.
        let taken = self
            .servers
            .iter()
            .any(|server| Some(server.id) != editing && server.entry.name.eq_ignore_ascii_case(&entry.name));
        if taken {
            form.set_error(tr("A network already has this name.").into());
            return Task::none();
        }
        self.modal = None;

        let task = if let Some(server) = editing.and_then(|id| self.server_mut(id)) {
            let reconnect = server.entry.connection != entry.connection;
            let fish_keys = std::mem::take(&mut server.entry.fish_keys);
            let old_notify = std::mem::take(&mut server.entry.notify);
            let mut entry = entry;
            let notify = std::mem::take(&mut entry.notify);
            server.entry = ServerEntry { fish_keys, notify: old_notify, ..entry };
            server.replace_notify(notify);
            if reconnect && server.enabled {
                server.generation += 1;
            }
            iced::widget::operation::focus(INPUT_ID)
        } else {
            // “Add and connect”: immediate connection whatever `auto_connect` says.
            let id = self.add_server(entry);
            let server = self.server_mut(id).expect("network added");
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
        // A portable copy keeps its passwords with it, not in this
        // machine's keychain.
        let use_keychain = self.config.settings.use_keychain && crate::config::portable_dir().is_none();
        let keychain_error = if use_keychain {
            keychain::stash(&mut on_disk, &keychain::System, &mut self.keychain)
        } else {
            None
        };
        if let Err(error) = on_disk.save_to(path) {
            self.banner = Some(tr!("Cannot save the configuration: {error}", error = error));
            return;
        }
        // Keychain disabled: the passwords are now in the file, so the
        // keychain entries can go.
        let keychain_error = keychain_error.or_else(|| (!use_keychain).then(|| keychain::clear(&keychain::System, &mut self.keychain)).flatten());
        if let Some(error) = keychain_error {
            self.banner = Some(tr!("System keychain unavailable, passwords kept in the file: {error}", error = error));
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

        let timers = if self.timers.is_empty() {
            Subscription::none()
        } else {
            time::every(Duration::from_millis(100)).map(Message::TimerTick)
        };

        Subscription::batch(connections.chain([
            event::listen_with(runtime_event),
            system::theme_changes().map(Message::SystemTheme),
            tick,
            timers,
        ]))
    }

    pub fn view(&self) -> Element<'_, Message> {
        let colors = self.colors();
        let settings = self.settings();

        let sidebar = settings
            .show_sidebar
            .then(|| sidebar::view(&self.servers, self.active, self.pending_offers(), settings.do_not_disturb, colors));
        let main: Element<'_, Message> = match self.active_buffer() {
            Some((server, buffer)) => {
                let id = self.active.expect("active tab").1;
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

        let notice = self.update.as_ref().map(|notice| updates::view(notice, colors));
        let main = column![].push(notice).push(main);
        let base = container(row![].push(sidebar).push(main))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme::panel(colors.background));

        let Some(modal) = &self.modal else {
            return base.into();
        };
        let dialog = match modal {
            Modal::Settings(form) => form.view(colors, self.window.dark, &self.script_status, self.scripts_dir()),
            Modal::Server(form) => form.view(colors),
            Modal::Switcher(switcher) => switcher.view(&self.servers, self.active, colors),
            Modal::Transfers => crate::ui::transfers::view(&self.transfers, self.now, colors),
            Modal::Paste(paste) => crate::ui::paste::view(paste, colors),
            Modal::Links(links) => links.view(colors),
            Modal::ChannelList(list) => list.view(self.server(list.server), colors),
            Modal::ChannelCentral(central) => central.view(self.server(central.server), colors),
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

/// Subscription key of a connection: only the id and the generation count,
/// so that a configuration change does not drop the connection unless
/// explicitly asked.
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
        iced::Event::Window(window::Event::FileDropped(path)) => Some(Message::FileDropped(path)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::ThemeMode;
    use ircnext_client::proto::Message as IrcMessage;
    use ircnext_client::Registered;

    /// Demo application fed with fake network events.
    fn demo() -> App {
        let mut config = AppConfig::default();
        // Tests do not depend on the machine's language.
        config.settings.language = i18n::Language::English;
        i18n::set_language(config.settings.language);
        config.servers.push(ServerEntry {
            name: "Libera.Chat".into(),
            auto_connect: false,
            on_connect: Vec::new(),
            fish_keys: std::collections::BTreeMap::new(),
            notify: Vec::new(),
            connection: Config { host: "irc.libera.chat".into(), nick: "me".into(), ..Config::default() },
        });
        let mut app = App::new(config, None, None);
        let id = app.servers[0].id;
        let registered = Registered { nick: "me".into(), caps: vec!["echo-message".into()], sts: None, sasl_refused: false, multiline: None };
        let _ = app.update(Message::Client(id, Event::Registered(registered)));
        for line in [
            ":s 005 me CHANTYPES=# PREFIX=(ov)@+ NETWORK=Libera.Chat MONITOR=100 :are supported",
            ":me!u@h JOIN #rust",
            ":s 332 me #rust :The Rust language · https://www.rust-lang.org · be kind",
            ":s 353 me = #rust :@alice +bob carol dave erin @me",
            ":s 366 me #rust :End",
            ":me!u@h JOIN #ircnext",
            ":alice!a@h PRIVMSG #rust :Has anyone tried the new \x02tokio\x02 release?",
            ":bob!b@h PRIVMSG #rust :yes, the docs are here: https://docs.rs/tokio",
            ":carol!c@h PRIVMSG #rust :\x0304red\x03, \x0309green\x03 and \x1ditalic\x1d: mIRC codes work",
            ":dave!d@h PRIVMSG #rust :\x01ACTION makes some coffee\x01",
            ":erin!e@h JOIN #rust",
            ":alice!a@h PRIVMSG #rust :me: what do you think of ircnext?",
            ":me!u@h PRIVMSG #rust :honestly, it looks clean",
            "@bot :helper!h@h PRIVMSG #rust :Reminder: meeting at 6 pm",
            ":bob!b@h BATCH +ml draft/multiline #rust",
            "@batch=ml :bob!b@h PRIVMSG #rust :multiline works too:",
            "@batch=ml :bob!b@h PRIVMSG #rust :let client = Client::new();",
            "@batch=ml :bob!b@h PRIVMSG #rust :client.connect().await?;",
            ":bob!b@h BATCH -ml",
            ":bob!b@h NICK bobby",
            ":zoe!z@h PRIVMSG me :hi, got two minutes?",
            ":carol!c@h PRIVMSG #ircnext :hello",
            "@+typing=active :carol!c@h TAGMSG #rust",
        ] {
            let message: IrcMessage = line.parse().unwrap();
            let _ = app.update(Message::Client(id, Event::Message(message)));
        }
        let buffer = app.servers[0].find("#rust").unwrap();
        let _ = app.update(Message::Select(id, buffer));
        app.input = "Message being typed".into();
        app
    }

    #[test]
    fn demo_state_is_consistent() {
        let app = demo();
        let server = &app.servers[0];
        let rust = server.buffer(server.find("#rust").unwrap()).unwrap();
        assert_eq!(rust.unread, 0, "active tab");
        let query = server.buffer(server.find("zoe").unwrap()).unwrap();
        assert!(query.highlighted);
        assert_eq!(app.title(), "(1) #rust — Libera.Chat · ircnext", "unread private message from zoe");
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
        assert_eq!(labels(&app), ["/join #channel [key]"]);
        let _ = app.update(Message::Submit);
        assert_eq!(app.input, "/join ", "Enter accepts the suggestion");

        let _ = app.update(Message::InputChanged("/join #".into()));
        assert_eq!(labels(&app), ["#ircnext", "#rust"]);
        let _ = app.update(Message::Shortcut(Shortcut::HistoryNext));
        let _ = app.update(Message::Shortcut(Shortcut::Complete { backwards: false }));
        assert_eq!(app.input, "/join #rust ", "↓ then Tab");

        let _ = app.update(Message::InputChanged("/ns ".into()));
        assert!(labels(&app).contains(&"IDENTIFY [account] <password>"));
        let _ = app.update(Message::Shortcut(Shortcut::Escape));
        assert_eq!(labels(&app), Vec::<&str>::new());
        let _ = app.update(Message::InputChanged("/ns ID".into()));
        assert_eq!(app.suggestions.len(), 1, "reopened on the next keystroke");
    }

    #[test]
    fn suggests_learned_and_remote_bot_commands() {
        let mut app = demo();
        inject(&mut app, "@bot :helper!h@h PRIVMSG #rust :Commands: !weather, !quote");
        inject(&mut app, ":carol!c@h PRIVMSG #rust :!quote 42");
        let _ = app.update(Message::InputChanged("!".into()));
        assert_eq!(labels(&app), ["!quote", "!weather"]);

        // Reply from a bot (IRCv3 draft) to a request sent for “!weather Par”.
        let (_, buffer) = app.active.unwrap();
        app.servers[0].remote_completion = Some(crate::server::RemoteCompletion {
            buffer,
            request: "!weather Par".into(),
            sent: Instant::now(),
            replies: Vec::new(),
        });
        let _ = app.update(Message::InputChanged("!weather Par".into()));
        inject(&mut app, "@+draft/autocomplete-response=is\t\u{8}\u{8}\u{8}Pau :helper!h@h TAGMSG #rust");
        assert_eq!(labels(&app), ["Paris", "Pau"]);
        assert_eq!(app.suggestions[1].source, "helper");
        let _ = app.update(Message::Submit);
        assert_eq!(app.input, "!weather Paris");
        assert_eq!(labels(&app), Vec::<&str>::new(), "reply outdated once the input changes");
    }

    #[test]
    fn ignore_commands_update_settings() {
        let mut app = demo();
        app.input = "/ignore Troll".into();
        let _ = app.update(Message::Submit);
        assert_eq!(app.config.settings.ignored, ["Troll"]);
        app.input = "/ignore troll".into();
        let _ = app.update(Message::Submit);
        assert_eq!(app.config.settings.ignored.len(), 1, "no duplicate (case-insensitive)");
        app.input = "/unignore troll".into();
        let _ = app.update(Message::Submit);
        assert_eq!(app.config.settings.ignored, Vec::<String>::new());
    }

    #[test]
    fn runs_connect_commands_after_registration() {
        let mut app = demo();
        app.servers[0].entry.on_connect = vec!["/ignore chatty".into()];
        let id = app.servers[0].id;
        let registered = Registered { nick: "me".into(), caps: vec![], sts: None, sasl_refused: false, multiline: None };
        let _ = app.update(Message::Client(id, Event::Registered(registered)));
        assert_eq!(app.config.settings.ignored, ["chatty"]);
    }

    #[test]
    fn network_names_are_unique() {
        let mut app = demo();
        let _ = app.update(Message::OpenServerForm(None));
        for msg in [server_form::Msg::Name("libera.chat".into()), server_form::Msg::Host("irc.example.net".into())] {
            let _ = app.update(Message::Form(msg));
        }
        let _ = app.update(Message::SaveServerForm);
        assert_eq!(app.servers.len(), 1, "name already taken");
        assert!(matches!(app.modal, Some(Modal::Server(_))), "the form stays open");

        // Editing a network without renaming it is still possible.
        let id = app.servers[0].id;
        let _ = app.update(Message::OpenServerForm(Some(id)));
        let _ = app.update(Message::Form(server_form::Msg::Host("irc.eu.libera.chat".into())));
        let _ = app.update(Message::SaveServerForm);
        assert!(app.modal.is_none());
        assert_eq!(app.servers[0].entry.connection.host, "irc.eu.libera.chat");
    }

    #[test]
    fn dcc_offers_wait_for_the_user() {
        let mut app = demo();
        inject(&mut app, ":bob!b@h PRIVMSG me :\x01DCC SEND \"../../.ssh/id_rsa\" 2130706433 5000 1234\x01");
        assert_eq!(app.transfers.len(), 1);
        let offer = &app.transfers[0];
        assert_eq!(offer.state, crate::transfer::State::Offered, "never accepted automatically");
        assert_eq!(offer.name, "id_rsa", "path removed from the name");
        assert_eq!(app.pending_offers(), 1);
        let query = app.servers[0].find("bob").expect("conversation with the sender");
        assert!(app.servers[0].buffer(query).unwrap().lines().any(|line| matches!(&line.kind, LineKind::Info(text) if text.contains("id_rsa"))));

        let id = offer.id;
        let _ = app.update(Message::Transfers(crate::ui::transfers::Msg::Decline(id)));
        assert_eq!(app.transfers[0].state, crate::transfer::State::Declined);
        assert_eq!(app.pending_offers(), 0);
    }

    #[test]
    fn partial_downloads_are_resumed() {
        let dir = std::env::temp_dir().join(format!("ircnext-resume-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("big.iso"), vec![0; 400]).unwrap();
        let mut app = demo();
        app.config.settings.download_dir = dir.display().to_string();
        inject(&mut app, ":bob!b@h PRIVMSG me :\x01DCC SEND big.iso 2130706433 5000 1000\x01");
        let id = app.transfers[0].id;
        let _ = app.update(Message::Transfers(crate::ui::transfers::Msg::Accept(id)));
        assert_eq!(app.transfers[0].resuming, Some(400), "RESUME asked at the partial size");
        assert_eq!(app.transfers[0].path, dir.join("big.iso"));

        // A different file or port does not match.
        inject(&mut app, ":bob!b@h PRIVMSG me :\x01DCC ACCEPT big.iso 6000 400\x01");
        assert_eq!(app.transfers[0].resuming, Some(400));
        inject(&mut app, ":bob!b@h PRIVMSG me :\x01DCC ACCEPT big.iso 5000 400\x01");
        assert_eq!(app.transfers[0].resuming, None);
        assert_eq!(app.transfers[0].done, 400);
        assert!(app.transfers[0].handle.is_some(), "download started");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn script_aliases_run_client_commands() {
        let mut app = demo();
        app.scripts
            .load(
                "test.mrc",
                "alias quiet { ignore $1 | echo $1 is ignored in $chan on $network as $me }\n\
                 alias broken { echo $calc(1 + | noop $nosuch( }",
            )
            .unwrap();
        app.input = "/quiet troll".into();
        let _ = app.update(Message::Submit);
        assert_eq!(app.config.settings.ignored, ["troll"], "client command run by the script");
        let (server, buffer) = app.active.unwrap();
        let last = |app: &App| app.server(server).unwrap().buffer(buffer).unwrap().lines().last().unwrap().kind.clone();
        assert_eq!(last(&app), LineKind::Info("troll is ignored in #rust on Libera.Chat as me".into()));

        app.input = "/broken".into();
        let _ = app.update(Message::Submit);
        assert!(matches!(last(&app), LineKind::Error(text) if text.starts_with("Script:")), "errors are shown");

        // Built-in commands still work when no alias has their name.
        app.input = "/unignore troll".into();
        let _ = app.update(Message::Submit);
        assert_eq!(app.config.settings.ignored, Vec::<String>::new());
    }

    #[test]
    fn pasted_lines_are_confirmed() {
        let mut app = demo();
        // Shows what would be sent, instead of sending it.
        app.scripts.load("input.mrc", "on *:INPUT:#:{ echo sent: $1- | halt }").unwrap();
        let (server, buffer) = app.active.unwrap();
        let last = |app: &App| match app.server(server).unwrap().buffer(buffer).unwrap().lines().last().unwrap().kind.clone() {
            LineKind::Info(text) => text,
            other => panic!("{other:?}"),
        };
        // What the input field does: the text without its line breaks.
        let paste = |app: &mut App, typed: &str, pasted: &str| {
            let flat: String = pasted.chars().filter(|c| !c.is_control()).collect();
            let _ = app.update(Message::InputChanged(format!("{typed}{flat}")));
            let _ = app.update(Message::Pasted(Some(pasted.into())));
        };

        paste(&mut app, "see: ", "one\ntwo");
        assert!(app.modal.is_none(), "under the threshold");
        assert_eq!(app.input, "see: one two", "kept on one line");

        let pasted = "first line\n\nsecond line\nthird line";
        paste(&mut app, "draft ", pasted);
        assert!(matches!(&app.modal, Some(Modal::Paste(paste)) if paste.target == "#rust" && paste.lines().count() == 3));
        let _ = app.update(Message::Shortcut(Shortcut::Escape));
        assert!(app.modal.is_none());
        assert_eq!(app.input, "draft ", "cancelled: the paste is undone");

        paste(&mut app, "draft ", pasted);
        let _ = app.update(Message::PasteSend);
        assert!(app.modal.is_none());
        assert!(last(&app).starts_with("sent: first line") && last(&app).contains("third line"));
        assert_eq!(app.input, "draft ", "what was typed stays");

        app.input.clear();
        paste(&mut app, "", pasted);
        let _ = app.update(Message::PasteJoined);
        assert_eq!(last(&app), "sent: first line second line third line");

        // Pasted elsewhere (search field): the input is left alone.
        app.input = "hello".into();
        let _ = app.update(Message::Pasted(Some(pasted.into())));
        assert!(app.modal.is_none());
        assert_eq!(app.input, "hello");

        app.config.settings.paste_confirm_lines = 0;
        app.input.clear();
        paste(&mut app, "", pasted);
        assert!(app.modal.is_none(), "confirmation turned off");
        assert_eq!(app.input, "first line second line third line");
    }

    #[test]
    fn links_panel_lists_posted_links() {
        let mut app = demo();
        inject(&mut app, ":carol!c@h PRIVMSG #rust :see https://docs.rs/tokio again, and \x02www.example.org\x02.");
        inject(&mut app, ":dave!d@h PRIVMSG #rust :javascript:alert(1) is not a link");
        app.input = "/links".into();
        let _ = app.update(Message::Submit);
        let Some(Modal::Links(links)) = &app.modal else { panic!("panel open") };
        let urls: Vec<&str> = links.links.iter().map(|link| link.url.as_str()).collect();
        assert_eq!(&urls[..2], ["https://docs.rs/tokio", "www.example.org"], "newest first, formatting removed");
        assert_eq!(urls.iter().filter(|url| **url == "https://docs.rs/tokio").count(), 1, "posted twice, listed once");
        assert!(urls.contains(&"https://www.rust-lang.org"), "topics too");
        assert_eq!(links.links[0].nick, "carol");
        assert!(links.links[0].place.starts_with("#rust"));

        let _ = app.update(Message::Links(crate::ui::links::Msg::Query("EXAMPLE".into())));
        let Some(Modal::Links(links)) = &app.modal else { panic!() };
        assert_eq!(links.filtered().map(|link| link.url.as_str()).collect::<Vec<_>>(), ["www.example.org"]);

        let target = (links.links[0].server, links.links[0].buffer);
        let zoe = app.servers[0].find("zoe").unwrap();
        let _ = app.update(Message::Select(app.servers[0].id, zoe));
        let _ = app.update(Message::Action(Action::Links));
        let _ = app.update(Message::Links(crate::ui::links::Msg::GoTo(target.0, target.1)));
        assert!(app.modal.is_none());
        assert_eq!(app.active, Some(target), "back to the conversation");
    }

    #[test]
    fn script_events_react_to_the_network_and_input() {
        let mut app = demo();
        app.scripts
            .load(
                "events.mrc",
                "on *:TEXT:!mute &:#:{ ignore $2 | echo $nick muted $2 in $chan }\n\
                 on *:JOIN:#rust:echo welcome $nick from $address\n\
                 on *:INPUT:#:{ if ($1 == secret) { echo kept for myself | halt } }",
            )
            .unwrap();
        let (server, buffer) = app.active.unwrap();
        let last = |app: &App| app.server(server).unwrap().buffer(buffer).unwrap().lines().last().unwrap().kind.clone();

        inject(&mut app, ":alice!a@h PRIVMSG #rust :!mute spammer");
        assert_eq!(app.config.settings.ignored, ["spammer"]);
        assert_eq!(last(&app), LineKind::Info("alice muted spammer in #rust".into()));

        inject(&mut app, ":frank!f@example.org JOIN #rust");
        assert_eq!(last(&app), LineKind::Info("welcome frank from f@example.org".into()));

        app.input = "secret".into();
        let _ = app.update(Message::Submit);
        assert_eq!(last(&app), LineKind::Info("kept for myself".into()), "nothing sent");
        app.input = "public".into();
        let _ = app.update(Message::Submit);
        assert!(matches!(last(&app), LineKind::Error(_)), "sent (the demo is not connected)");
    }

    #[test]
    fn timers_repeat_then_stop() {
        let mut app = demo();
        let (server, buffer) = app.active.unwrap();
        let lines = |app: &App| -> Vec<LineKind> { app.server(server).unwrap().buffer(buffer).unwrap().lines().map(|line| line.kind.clone()).collect() };
        app.input = "/timertick 2 1 echo -a tock".into();
        let _ = app.update(Message::Submit);
        assert_eq!(app.timers.len(), 1);
        let start = Instant::now();
        let _ = app.update(Message::TimerTick(start));
        assert!(!lines(&app).contains(&LineKind::Info("tock".into())), "not due yet");
        let _ = app.update(Message::TimerTick(start + Duration::from_millis(1100)));
        let _ = app.update(Message::TimerTick(start + Duration::from_millis(2200)));
        assert_eq!(lines(&app).iter().filter(|kind| **kind == LineKind::Info("tock".into())).count(), 2);
        assert!(app.timers.is_empty(), "two repetitions, then gone");

        app.input = "/timer 0 60 echo forever".into();
        let _ = app.update(Message::Submit);
        app.input = "/timers off".into();
        let _ = app.update(Message::Submit);
        assert!(app.timers.is_empty());
    }

    #[test]
    fn script_files_and_variables_stay_in_the_scripts_folder() {
        let dir = std::env::temp_dir().join(format!("ircnext-script-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(
            dir.join("scripts/files.mrc"),
            "alias keep { write data/notes.txt $1- | set %count $calc(%count + 1) }\nalias escape write ../outside.txt nope",
        )
        .unwrap();
        let mut app = App::new(AppConfig::default(), Some(dir.join("config.toml")), None);
        app.load_scripts();
        let id = app.add_server(ServerEntry {
            name: "Test".into(),
            auto_connect: false,
            on_connect: Vec::new(),
            fish_keys: std::collections::BTreeMap::new(),
            notify: Vec::new(),
            connection: Config::default(),
        });
        let tab = app.server(id).unwrap().server_buffer();
        let _ = app.run_alias(id, tab, "keep", "hello world");
        assert_eq!(std::fs::read_to_string(dir.join("scripts/data/notes.txt")).unwrap(), "hello world\n");
        let _ = app.run_alias(id, tab, "escape", "");
        assert!(!dir.join("outside.txt").exists(), "nothing written outside the scripts folder");

        // Variables are saved, and found again by a new session.
        let mut next = App::new(AppConfig::default(), Some(dir.join("config.toml")), None);
        next.load_scripts();
        assert_eq!(next.scripts.globals.get("%count").map(String::as_str), Some("1"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn script_manager_turns_scripts_on_and_off() {
        use crate::ui::settings_form::ScriptState;
        let dir = std::env::temp_dir().join(format!("ircnext-script-manager-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join("scripts/bot.mrc"), "alias hi echo hi\non *:JOIN:#:echo hello").unwrap();
        std::fs::write(dir.join("scripts/broken.mrc"), "alias {").unwrap();
        let mut app = App::new(AppConfig::default(), Some(dir.join("config.toml")), None);
        app.load_scripts();
        assert_eq!(app.script_status[0].state, ScriptState::Loaded { aliases: 1, events: 1 });
        assert!(matches!(app.script_status[1].state, ScriptState::Error(_)));

        let _ = app.update(Message::ToggleScript("bot.mrc".into(), false));
        assert_eq!(app.script_status[0].state, ScriptState::Disabled);
        assert!(!app.scripts.has_alias("hi"));
        assert_eq!(app.config.settings.disabled_scripts, ["bot.mrc"]);
        let saved = std::fs::read_to_string(dir.join("config.toml")).unwrap();
        assert!(saved.contains("bot.mrc"), "choice saved");

        let _ = app.update(Message::ToggleScript("bot.mrc".into(), true));
        assert!(app.scripts.has_alias("hi"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn loads_scripts_from_the_scripts_folder() {
        let dir = std::env::temp_dir().join(format!("ircnext-scripts-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join("scripts/good.mrc"), "alias hi echo hi").unwrap();
        std::fs::write(dir.join("scripts/bad.mrc"), "alias broken {").unwrap();
        std::fs::write(dir.join("scripts/notes.txt"), "not a script").unwrap();
        let mut app = App::new(AppConfig::default(), Some(dir.join("config.toml")), None);
        app.scripts.globals.insert("%kept".into(), "1".into());
        let (loaded, errors) = app.load_scripts();
        assert_eq!(loaded, ["good.mrc"]);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].starts_with("bad.mrc"), "{errors:?}");
        assert!(app.scripts.has_alias("hi"));
        assert_eq!(app.scripts.globals.get("%kept").map(String::as_str), Some("1"), "globals kept on reload");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn title_counts_unread_mentions() {
        let mut app = demo();
        inject(&mut app, ":alice!a@h PRIVMSG #ircnext :me: look at this");
        assert!(app.title().starts_with("(2) "), "zoe and #ircnext: {}", app.title());
        app.config.settings.title_unread_count = false;
        assert!(!app.title().starts_with('('));
    }

    fn press(app: &mut App, combo: &str) {
        let combo = Combo::parse(combo, keymap::MAC).expect("valid combination");
        let _ = app.update(Message::Key { combo, captured: false });
    }

    fn active_name(app: &App) -> String {
        app.active_buffer().map(|(_, buffer)| buffer.name.clone()).unwrap_or_default()
    }

    #[test]
    fn navigation_shortcuts() {
        let mut app = demo();
        // From #rust, only “zoe” is unread after it: Alt+Shift+↓ leads there.
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
        app.input = "hello".into();
        press(&mut app, "Mod+B");
        assert_eq!(app.input, "hello\x02");

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
    fn do_not_disturb_silences_alerts() {
        let mut app = demo();
        press(&mut app, "Mod+Shift+D");
        assert!(app.config.settings.do_not_disturb);
        app.window.focused = false;
        app.window.id = Some(window::Id::unique());
        let alert = Alert { buffer: app.servers[0].server_buffer(), title: "zoe".into(), body: "hi".into(), private: true };
        let id = app.servers[0].id;
        assert!(app.alert(id, vec![alert.clone()]).units() == 0, "no window alert");
        press(&mut app, "Mod+Shift+D");
        assert!(app.alert(id, vec![alert]).units() > 0, "window alert again");
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

    /// Renders screenshots of the interface into `target/snapshots/<language>/`.
    /// `cargo test -p ircnext -- --ignored snapshots`
    #[test]
    #[ignore = "writes PNG files"]
    fn snapshots() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/snapshots");
        let _ = std::fs::remove_dir_all(&root);
        for (language, folder) in [(i18n::Language::English, "en"), (i18n::Language::French, "fr")] {
            snapshots_in(language, &root.join(folder));
        }
    }

    fn snapshots_in(language: i18n::Language, dir: &std::path::Path) {
        let mut app = demo();
        // A second network, to show how networks are set apart.
        let oftc = app.add_server(ServerEntry {
            name: "OFTC".into(),
            auto_connect: false,
            on_connect: Vec::new(),
            fish_keys: std::collections::BTreeMap::new(),
            notify: Vec::new(),
            connection: Config { host: "irc.oftc.net".into(), nick: "me".into(), ..Config::default() },
        });
        let registered = Registered { nick: "me".into(), caps: vec![], sts: None, sasl_refused: false, multiline: None };
        let _ = app.update(Message::Client(oftc, Event::Registered(registered)));
        for line in [":me!u@h JOIN #debian", ":me!u@h JOIN #tor", ":sam!s@h PRIVMSG me :ping"] {
            let message: IrcMessage = line.parse().unwrap();
            let _ = app.update(Message::Client(oftc, Event::Message(message)));
        }
        // An encrypted channel (FiSH).
        app.servers[0].entry.fish_keys.insert("#rust".into(), ircnext_client::Secret::new("cbc:demo key"));
        let key = ircnext_client::fish::Key::parse("cbc:demo key").unwrap();
        inject(&mut app, &format!(":erin!e@h PRIVMSG #rust :{}", key.encrypt("this one is end-to-end encrypted")));
        let rust = app.servers[0].find("#rust").unwrap();
        let _ = app.update(Message::Select(app.servers[0].id, rust));
        // Back from another tab: the new message is after the marker.
        let zoe = app.servers[0].find("zoe").unwrap();
        let _ = app.update(Message::Select(app.servers[0].id, zoe));
        inject(&mut app, ":frank!f@h PRIVMSG #rust :anyone around? I just got back");
        let _ = app.update(Message::Select(app.servers[0].id, rust));
        app.config.settings.language = language;
        i18n::set_language(language);
        for mode in ThemeMode::ALL.into_iter().skip(1) {
            app.config.settings.theme = mode;
            render(&app, &dir.join(format!("{mode:?}").to_lowercase()));
        }
        app.config.settings.theme = ThemeMode::Dark;
        app.script_status = vec![
            crate::ui::settings_form::ScriptStatus {
                name: "bot.mrc".into(),
                state: crate::ui::settings_form::ScriptState::Loaded { aliases: 4, events: 6 },
            },
            crate::ui::settings_form::ScriptStatus { name: "away.mrc".into(), state: crate::ui::settings_form::ScriptState::Disabled },
            crate::ui::settings_form::ScriptStatus {
                name: "quiz.mrc".into(),
                state: crate::ui::settings_form::ScriptState::Error("quiz.mrc: line 12: missing }".into()),
            },
        ];
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
        let _ = app.update(Message::Shortcut(Shortcut::Escape));
        inject(&mut app, ":carol!c@h PRIVMSG me :\x01DCC SEND \"holiday photos.zip\" 2130706433 5000 48213000\x01");
        inject(&mut app, ":dave!d@h PRIVMSG me :\x01DCC SEND notes.pdf 2130706433 5001 812000\x01");
        let last = app.transfers.len() - 1;
        app.transfers[last].state = crate::transfer::State::Active;
        app.transfers[last].done = 530_000;
        let _ = app.update(Message::OpenTransfers);
        render(&app, &dir.join("transfers"));
        let _ = app.update(Message::CloseModal);
        let release = crate::update::Release {
            tag_name: "v0.5.0".into(),
            html_url: "https://github.com/BrystoQ/ircnext/releases/tag/v0.5.0".into(),
            assets: Vec::new(),
        };
        let _ = app.update(Message::UpdateChecked { manual: true, result: Ok(Some(release)) });
        render(&app, &dir.join("update"));
        let _ = app.update(Message::DismissUpdate);
        let code = "fn main() {\n    let client = Client::new();\n    client.run();\n}";
        let _ = app.update(Message::InputChanged(code.chars().filter(|c| !c.is_control()).collect()));
        let _ = app.update(Message::Pasted(Some(code.into())));
        render(&app, &dir.join("paste"));
        let _ = app.update(Message::CloseModal);
        let _ = app.update(Message::Action(Action::Links));
        render(&app, &dir.join("links"));
        let _ = app.update(Message::CloseModal);
        let libera = app.servers[0].id;
        app.servers[0].channel_list = Some(crate::server::channel_list::ChannelList::default());
        for line in [
            ":s 322 me #libera 2100 :Libera.Chat support · https://libera.chat",
            ":s 322 me #rust 1200 :[+nt] The Rust language · https://www.rust-lang.org · be kind",
            ":s 322 me #linux 980 :Linux help and discussion",
            ":s 322 me #python 870 :Python programming · paste code to a pastebin",
            ":s 322 me #ircnext 42 :A modern IRC client in Rust",
            ":s 323 me :End of /LIST",
        ] {
            inject(&mut app, line);
        }
        app.modal = Some(Modal::ChannelList(crate::ui::channel_list::ChannelListView::new(libera)));
        render(&app, &dir.join("channel-list"));
        app.servers[0].network.request_mask_list("#rust", 'b');
        for line in [
            ":s 324 me #rust +ntk hunter2",
            ":s 367 me #rust *!*@spam.example.net alice!a@h 1762200000",
            ":s 367 me #rust troll!*@* alice!a@h 1762300000",
            ":s 368 me #rust :End of channel ban list",
        ] {
            inject(&mut app, line);
        }
        app.modal = Some(Modal::ChannelCentral(crate::ui::channel_central::ChannelCentral::new(libera, "#rust".into())));
        render(&app, &dir.join("channel-central"));
        let _ = app.update(Message::ChannelCentral(crate::ui::channel_central::Msg::Tab(Some('b'))));
        render(&app, &dir.join("channel-bans"));
    }

    fn render(app: &App, path: &std::path::Path) {
        let mut simulator =
            iced_test::Simulator::with_size(iced::Settings::default(), (1200.0, 780.0), app.view());
        let snapshot = simulator.snapshot(&app.theme()).expect("rendering");
        assert!(snapshot.matches_image(path).expect("PNG writing"));
    }
}
