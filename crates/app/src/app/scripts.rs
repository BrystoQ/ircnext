//! mIRC scripts: loading the `.mrc` files, running aliases, and what the
//! interpreter needs from the client.

use std::path::PathBuf;

use chrono::Local;
use iced::Task;
use ircnext_script::{Event, Host, Interpreter};

use super::{App, Message};
use crate::buffer::{Kind, Line, LineKind};
use crate::server::{BufferId, Context, ScriptEvent, ServerId};
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
        self.scripts.globals = globals;
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
        for path in files {
            let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            match std::fs::read_to_string(&path) {
                Ok(source) => match self.scripts.load(&name, &source) {
                    Ok(()) => loaded.push(name),
                    Err(error) => errors.push(error.to_string()),
                },
                Err(error) => errors.push(format!("{name}: {error}")),
            }
        }
        (loaded, errors)
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

    /// Runs a script alias typed by the user (`/hello world`).
    pub(super) fn run_alias(&mut self, server: ServerId, buffer: BufferId, name: &str, args: &str) -> Task<Message> {
        let mut scripts = std::mem::take(&mut self.scripts);
        let mut host = ScriptHost { app: self, server, buffer, tasks: Vec::new() };
        let result = scripts.call(name, args, &mut host);
        let tasks = std::mem::take(&mut host.tasks);
        // Globals set by the script are kept; aliases are restored.
        self.scripts = scripts;
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
