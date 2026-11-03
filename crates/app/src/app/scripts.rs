//! mIRC scripts: loading the `.mrc` files, running aliases, and what the
//! interpreter needs from the client.

use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::Local;
use iced::Task;
use ircnext_script::{Event, Host, Interpreter};

use super::{App, Message};
use crate::buffer::{Kind, Line, LineKind};
use crate::server::{BufferId, Context, ScriptEvent, ServerId};
use crate::ui::settings_form::{ScriptState, ScriptStatus};
use crate::i18n::tr;
use crate::tr;

impl App {
    /// Folder of the script files, next to the configuration.
    pub(super) fn scripts_dir(&self) -> Option<PathBuf> {
        Some(self.config_path.as_ref()?.parent()?.join("scripts"))
    }

    /// Loads every `.mrc` file of the scripts folder, keeping the global
    /// variables. Returns the names loaded and the errors.
    pub(super) fn load_scripts(&mut self) -> (Vec<String>, Vec<String>) {
        let Some(dir) = self.scripts_dir() else {
            return (Vec::new(), Vec::new());
        };
        let globals = std::mem::take(&mut self.scripts.globals);
        self.scripts = Interpreter::default();
        // Global variables live in `vars.ini`, written as soon as they change.
        self.scripts.globals = std::fs::read_to_string(dir.join("vars.ini"))
            .map_or(globals, |text| ircnext_script::storage::parse_variables(&text));
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("mrc")))
                    .collect()
            })
            .unwrap_or_default();
        files.sort();
        let (mut loaded, mut errors) = (Vec::new(), Vec::new());
        self.script_status.clear();
        for path in files {
            let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            let disabled = self.config.settings.disabled_scripts.iter().any(|script| script.eq_ignore_ascii_case(&name));
            let state = if disabled {
                ScriptState::Disabled
            } else {
                match std::fs::read_to_string(&path).map_err(|error| format!("{name}: {error}")) {
                    Ok(source) => match self.scripts.load(&name, &source) {
                        Ok(()) => {
                            let script = &self.scripts.scripts.last().expect("just loaded").script;
                            let state = ScriptState::Loaded { aliases: script.aliases.len(), events: script.handlers.len() };
                            loaded.push(name.clone());
                            state
                        }
                        Err(error) => {
                            errors.push(error.to_string());
                            ScriptState::Error(error.to_string())
                        }
                    },
                    Err(error) => {
                        errors.push(error.clone());
                        ScriptState::Error(error)
                    }
                }
            };
            self.script_status.push(ScriptStatus { name, state });
        }
        (loaded, errors)
    }

    /// Turns a script on or off (script manager), then reloads the scripts.
    pub(super) fn toggle_script(&mut self, name: &str, enabled: bool) {
        for list in std::iter::once(&mut self.config.settings.disabled_scripts).chain(match &mut self.modal {
            Some(super::Modal::Settings(form)) => Some(&mut form.draft.disabled_scripts),
            _ => None,
        }) {
            list.retain(|script| !script.eq_ignore_ascii_case(name));
            if !enabled {
                list.push(name.to_owned());
            }
        }
        self.save();
        self.load_scripts();
    }

    /// Opens the scripts folder, or one of its files, with the system.
    pub(super) fn open_script_path(&mut self, file: Option<&str>) {
        let Some(dir) = self.scripts_dir() else {
            return;
        };
        let _ = std::fs::create_dir_all(&dir);
        let path = file.and_then(|file| script_file(&dir, file)).unwrap_or(dir);
        if let Err(error) = open::that_detached(&path) {
            self.banner = Some(tr!("Cannot open {path}: {error}", path = path.display(), error = error));
        }
    }

    /// `/reload`: loads the scripts again and says what happened.
    pub(super) fn reload_scripts(&mut self, server: ServerId, buffer: BufferId) -> Task<Message> {
        let (loaded, errors) = self.load_scripts();
        let folder = self.scripts_dir().map(|dir| dir.display().to_string()).unwrap_or_default();
        let text = if loaded.is_empty() {
            tr!("No script loaded. Put .mrc files in {folder}", folder = folder)
        } else {
            tr!("Scripts loaded: {scripts}", scripts = loaded.join(", "))
        };
        self.script_line(server, buffer, LineKind::Info(text));
        for error in errors {
            self.script_line(server, buffer, LineKind::Error(error));
        }
        Task::none()
    }

    /// Saves the global variables if a script changed them.
    fn save_script_variables(&mut self) {
        if !self.scripts.take_globals_changed() {
            return;
        }
        let Some(dir) = self.scripts_dir() else {
            return;
        };
        let text = ircnext_script::storage::format_variables(&self.scripts.globals);
        if let Err(error) = std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(dir.join("vars.ini"), text)) {
            self.banner = Some(tr!("Cannot save the script variables: {error}", error = error));
        }
    }

    /// `/timer[name] [-m] <repetitions> <interval> <command>`, `/timer<name> off`,
    /// `/timers [off]`. Returns `None` if `line` is not a timer command.
    pub(super) fn timer_command(&mut self, server: ServerId, buffer: BufferId, line: &str) -> Option<Task<Message>> {
        let command = line.strip_prefix('/')?;
        let (word, args) = command.split_once(' ').unwrap_or((command, ""));
        let lower = word.to_ascii_lowercase();
        let name = lower.strip_prefix("timer")?;
        let args = args.trim();
        if name == "s" {
            if args.eq_ignore_ascii_case("off") {
                self.timers.clear();
            } else if self.timers.is_empty() {
                self.script_line(server, buffer, LineKind::Info(tr("No active timer.").to_owned()));
            } else {
                let lines: Vec<String> = self
                    .timers
                    .iter()
                    .map(|timer| {
                        let repetitions = timer.remaining.map_or_else(|| "∞".to_owned(), |n| n.to_string());
                        tr!(
                            "Timer {name}: {repetitions} × every {interval} — {command}",
                            name = timer.name,
                            repetitions = repetitions,
                            interval = format!("{:.1} s", timer.interval.as_secs_f64()),
                            command = timer.command
                        )
                    })
                    .collect();
                for line in lines {
                    self.script_line(server, buffer, LineKind::Info(line));
                }
            }
            return Some(Task::none());
        }
        if args.eq_ignore_ascii_case("off") {
            self.timers.retain(|timer| !timer.name.eq_ignore_ascii_case(name));
            return Some(Task::none());
        }
        let (milliseconds, args) = match args.strip_prefix("-m ") {
            Some(rest) => (true, rest.trim_start()),
            None => (false, args.strip_prefix("-o ").unwrap_or(args)),
        };
        let mut words = args.splitn(3, ' ');
        let parsed = (words.next().and_then(|n| n.parse::<u32>().ok()), words.next().and_then(|n| n.parse::<f64>().ok()), words.next());
        let (Some(repetitions), Some(interval), Some(action)) = parsed else {
            let usage = tr("Usage: /timer[name] [-m] <repetitions> <interval> <command>").to_owned();
            self.script_line(server, buffer, LineKind::Error(usage));
            return Some(Task::none());
        };
        let interval = if milliseconds { interval / 1000.0 } else { interval };
        let interval = Duration::from_secs_f64(interval.clamp(0.1, 86_400.0 * 7.0));
        let name = if name.is_empty() {
            self.next_timer += 1;
            self.next_timer.to_string()
        } else {
            name.to_owned()
        };
        self.timers.retain(|timer| !timer.name.eq_ignore_ascii_case(&name));
        self.timers.push(ScriptTimer {
            name,
            server,
            buffer,
            remaining: (repetitions > 0).then_some(repetitions),
            interval,
            next: Instant::now() + interval,
            command: action.to_owned(),
        });
        Some(Task::none())
    }

    /// Runs the timers that are due.
    pub(super) fn run_timers(&mut self, now: Instant) -> Task<Message> {
        let due: Vec<(ServerId, BufferId, String)> = self
            .timers
            .iter_mut()
            .filter(|timer| timer.next <= now)
            .map(|timer| {
                timer.next = now + timer.interval;
                timer.remaining = timer.remaining.map(|n| n.saturating_sub(1));
                (timer.server, timer.buffer, timer.command.clone())
            })
            .collect();
        self.timers.retain(|timer| timer.remaining != Some(0));
        let tasks: Vec<Task<Message>> = due
            .into_iter()
            .map(|(server, buffer, command)| {
                let line = if command.starts_with('/') { command } else { format!("/{command}") };
                self.run_line(server, buffer, &line)
            })
            .collect();
        Task::batch(tasks)
    }

    /// Runs a script alias typed by the user (`/hello world`).
    pub(super) fn run_alias(&mut self, server: ServerId, buffer: BufferId, name: &str, args: &str) -> Task<Message> {
        let mut scripts = std::mem::take(&mut self.scripts);
        let mut host = ScriptHost { app: self, server, buffer, tasks: Vec::new() };
        let result = scripts.call(name, args, &mut host);
        let tasks = std::mem::take(&mut host.tasks);
        // Globals set by the script are kept; aliases are restored.
        self.scripts = scripts;
        self.save_script_variables();
        if let Err(error) = result {
            self.script_line(server, buffer, LineKind::Error(tr!("Script: {error}", error = error)));
        }
        Task::batch(tasks)
    }

    /// Runs the scripts' handlers of the events collected by a network.
    pub(super) fn fire_script_events(&mut self, server: ServerId, events: Vec<ScriptEvent>) -> Task<Message> {
        if self.scripts.scripts.is_empty() {
            return Task::none();
        }
        let tasks: Vec<Task<Message>> =
            events.into_iter().map(|ScriptEvent { buffer, event }| self.fire_script_event(server, buffer, &event).1).collect();
        Task::batch(tasks)
    }

    /// Runs the handlers of one event in `buffer`. Returns whether one of
    /// them used `halt`.
    pub(super) fn fire_script_event(&mut self, server: ServerId, buffer: BufferId, event: &Event) -> (bool, Task<Message>) {
        if self.scripts.scripts.is_empty() {
            return (false, Task::none());
        }
        let mut scripts = std::mem::take(&mut self.scripts);
        let mut host = ScriptHost { app: self, server, buffer, tasks: Vec::new() };
        let (halted, errors) = scripts.fire(event, &mut host);
        let tasks = std::mem::take(&mut host.tasks);
        self.scripts = scripts;
        self.save_script_variables();
        for error in errors {
            self.script_line(server, buffer, LineKind::Error(tr!("Script: {error}", error = error)));
        }
        (halted, Task::batch(tasks))
    }

    /// `on INPUT`: what the user typed in a tab, before it is sent.
    /// Returns whether a handler stopped it.
    pub(super) fn input_event(&mut self, server: ServerId, buffer: BufferId, text: &str) -> (bool, Task<Message>) {
        let Some((kind, name)) = self.server(server).and_then(|s| s.buffer(buffer)).map(|b| (b.kind, b.name.clone())) else {
            return (false, Task::none());
        };
        let channel = (kind == Kind::Channel).then_some(name.clone());
        let event = Event {
            name: "INPUT".into(),
            channel,
            text: text.to_owned(),
            own: true,
            values: [("target".to_owned(), name)].into(),
            ..Event::default()
        };
        self.fire_script_event(server, buffer, &event)
    }

    fn script_line(&mut self, server: ServerId, buffer: BufferId, kind: LineKind) {
        let active = self.active.filter(|(id, _)| *id == server).map(|(_, b)| b);
        let settings = &self.config.settings;
        if let Some(server) = self.servers.iter_mut().find(|s| s.id == server) {
            server.push(buffer, Line::new(Local::now(), kind), &Context { settings, active });
        }
    }
}

/// A `/timer`: a command run every `interval`, in the tab it was set from.
#[derive(Debug, Clone)]
pub(super) struct ScriptTimer {
    name: String,
    server: ServerId,
    buffer: BufferId,
    /// Runs left, `None` for endless.
    remaining: Option<u32>,
    interval: Duration,
    next: Instant,
    command: String,
}

/// A path inside the scripts folder: relative, without `..`.
fn script_file(dir: &Path, path: &str) -> Option<PathBuf> {
    let path = Path::new(path.trim());
    let safe = !path.as_os_str().is_empty() && path.components().all(|component| matches!(component, Component::Normal(_)));
    safe.then(|| dir.join(path))
}

/// The client as seen by a running script: the network and tab it runs in.
struct ScriptHost<'a> {
    app: &'a mut App,
    server: ServerId,
    buffer: BufferId,
    tasks: Vec<Task<Message>>,
}

impl ScriptHost<'_> {
    fn buffer_name(&self) -> Option<(Kind, String)> {
        let server = self.app.server(self.server)?;
        let buffer = server.buffer(self.buffer)?;
        Some((buffer.kind, buffer.name.clone()))
    }
}

impl Host for ScriptHost<'_> {
    fn command(&mut self, line: &str) {
        let task = self.app.run_builtin(self.server, self.buffer, &format!("/{line}"));
        self.tasks.push(task);
    }

    fn echo(&mut self, text: &str) {
        self.app.script_line(self.server, self.buffer, LineKind::Info(text.to_owned()));
    }

    fn identifier(&mut self, name: &str, args: &[String], _property: Option<&str>) -> Option<String> {
        let now = Local::now();
        let server = self.app.server(self.server)?;
        let tab = self.buffer_name();
        let value = match name {
            "me" => server.network.nick().to_owned(),
            "chan" => tab.filter(|(kind, _)| *kind == Kind::Channel).map(|(_, name)| name).unwrap_or_default(),
            "active" | "target" => tab.map(|(_, name)| name).unwrap_or_default(),
            "query" => tab.filter(|(kind, _)| *kind == Kind::Query).map(|(_, name)| name).unwrap_or_default(),
            "network" => server.network.isupport().network.clone().unwrap_or_else(|| server.name().to_owned()),
            "server" => server.entry.connection.host.clone(),
            "port" => server.entry.connection.port.to_string(),
            "status" => if server.is_online() { "connected" } else { "disconnected" }.to_owned(),
            "time" => match args.first() {
                Some(format) => now.format(&mirc_time_format(format)).to_string(),
                None => now.format("%H:%M:%S").to_string(),
            },
            "date" => now.format("%d/%m/%Y").to_string(),
            "fulldate" => now.format("%a %b %d %H:%M:%S %Y").to_string(),
            "day" => now.format("%A").to_string(),
            "ctime" => now.timestamp().to_string(),
            "timestamp" => now.format("[%H:%M]").to_string(),
            "version" => env!("CARGO_PKG_VERSION").to_owned(),
            "away" => if server.network.is_away() { "$true" } else { "$false" }.to_owned(),
            _ => return None,
        };
        Some(value)
    }

    fn read_file(&mut self, path: &str) -> Option<String> {
        let path = script_file(&self.app.scripts_dir()?, path)?;
        std::fs::read_to_string(path).ok()
    }

    fn write_file(&mut self, path: &str, content: &str) -> Result<(), String> {
        let dir = self.app.scripts_dir().ok_or_else(|| tr("no scripts folder").to_owned())?;
        let target = script_file(&dir, path).ok_or_else(|| tr!("{path}: only files of the scripts folder can be written", path = path))?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(target, content).map_err(|error| error.to_string())
    }

    fn condition(&mut self, operator: &str, left: &str, right: &str) -> Option<bool> {
        let server = self.app.server(self.server)?;
        let network = &server.network;
        let member = |channel: &str| network.channel(channel).and_then(|channel| channel.member(&network.fold(left)).cloned());
        let has_prefix = |mode: char| {
            let symbol = network.isupport().prefix_symbol(mode);
            member(right).is_some_and(|member| symbol.is_some_and(|symbol| member.prefixes.contains(&symbol)))
        };
        Some(match operator {
            "ison" => member(right).is_some(),
            "isop" => has_prefix('o'),
            "ishop" => has_prefix('h'),
            "isvoice" => has_prefix('v'),
            "isreg" => member(right).is_some_and(|member| member.prefixes.is_empty()),
            "ischan" => network.channel(left).is_some(),
            _ => return None,
        })
    }
}

/// mIRC time format (`HH:nn:ss`, `dd/mm/yyyy`…) to `strftime`.
fn mirc_time_format(format: &str) -> String {
    let mut out = format.to_owned();
    for (mirc, strftime) in [
        ("yyyy", "%Y"),
        ("yy", "%y"),
        ("dddd", "%A"),
        ("ddd", "%a"),
        ("dd", "%d"),
        ("mmmm", "%B"),
        ("mmm", "%b"),
        ("mm", "%m"),
        ("HH", "%H"),
        ("hh", "%I"),
        ("nn", "%M"),
        ("ss", "%S"),
        ("tt", "%p"),
    ] {
        out = out.replace(mirc, strftime);
    }
    out
}
