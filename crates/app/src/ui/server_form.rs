//! Form to add or edit a network.

use iced::widget::{button, column, container, row, scrollable, space, text};
use std::path::PathBuf;

use iced::{Alignment, Element, Length};
use ircnext_client::{Config, Sasl, Secret};

use crate::app::Message;
use crate::config::{PRESETS, Preset, ServerEntry};
use crate::server::ServerId;
use crate::settings::Settings;
use crate::theme::{self, Colors};
use crate::certificate;
use crate::i18n::tr;
use crate::tr;
use crate::ui::form;

#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)] // mirrors the form's checkboxes
pub struct ServerForm {
    /// Network being edited, or `None` when adding.
    pub editing: Option<ServerId>,
    preset: Option<Preset>,
    name: String,
    host: String,
    port: String,
    tls: bool,
    accept_invalid_certs: bool,
    nick: String,
    username: String,
    realname: String,
    password: String,
    auth: Auth,
    sasl_username: String,
    sasl_password: String,
    /// Path of the client certificate (PEM file).
    client_cert: String,
    autojoin: String,
    auto_connect: bool,
    auto_reconnect: bool,
    alt_nicks: String,
    on_connect: Vec<String>,
    error: Option<String>,
}

/// Identification to the network account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Auth {
    None,
    /// SASL `PLAIN`: account and password.
    Password,
    /// SASL `EXTERNAL`: client certificate.
    Certificate,
}

impl Auth {
    const ALL: [Self; 3] = [Self::None, Self::Password, Self::Certificate];
}

impl std::fmt::Display for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::None => tr("None"),
            Self::Password => tr("Account and password"),
            Self::Certificate => tr("Client certificate"),
        })
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Preset(Preset),
    Name(String),
    Host(String),
    Port(String),
    Tls(bool),
    AcceptInvalidCerts(bool),
    Nick(String),
    Username(String),
    Realname(String),
    Password(String),
    Auth(Auth),
    SaslUsername(String),
    SaslPassword(String),
    ClientCert(String),
    /// Creates a self-signed certificate and selects it.
    CreateCertificate,
    Autojoin(String),
    AutoConnect(bool),
    AutoReconnect(bool),
    AltNicks(String),
    AddCommand,
    Command(usize, String),
    RemoveCommand(usize),
}

impl ServerForm {
    /// New network, prefilled with Libera.Chat and the default identity.
    pub fn new(settings: &Settings) -> Self {
        let mut form = Self {
            editing: None,
            preset: None,
            name: String::new(),
            host: String::new(),
            port: String::new(),
            tls: true,
            accept_invalid_certs: false,
            nick: settings.nick.clone(),
            username: settings.nick.clone(),
            realname: settings.realname.clone(),
            password: String::new(),
            auth: Auth::None,
            sasl_username: String::new(),
            sasl_password: String::new(),
            client_cert: String::new(),
            autojoin: String::new(),
            auto_connect: true,
            auto_reconnect: true,
            alt_nicks: String::new(),
            on_connect: Vec::new(),
            error: None,
        };
        form.apply_preset(PRESETS[0]);
        form
    }

    pub fn edit(id: ServerId, entry: &ServerEntry) -> Self {
        let connection = &entry.connection;
        Self {
            editing: Some(id),
            preset: PRESETS.iter().copied().find(|p| p.host == connection.host),
            name: entry.name.clone(),
            host: connection.host.clone(),
            port: connection.port.to_string(),
            tls: connection.tls,
            accept_invalid_certs: connection.accept_invalid_certs,
            nick: connection.nick.clone(),
            username: connection.username.clone(),
            realname: connection.realname.clone(),
            password: connection
                .password
                .as_ref()
                .map(|p| p.expose().to_owned())
                .unwrap_or_default(),
            auth: if connection.sasl_external {
                Auth::Certificate
            } else if connection.sasl.is_some() {
                Auth::Password
            } else {
                Auth::None
            },
            sasl_username: connection
                .sasl
                .as_ref()
                .map(|s| s.username.clone())
                .unwrap_or_default(),
            sasl_password: connection
                .sasl
                .as_ref()
                .map(|s| s.password.expose().to_owned())
                .unwrap_or_default(),
            client_cert: connection
                .client_cert
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
            autojoin: connection.autojoin.join(", "),
            auto_connect: entry.auto_connect,
            auto_reconnect: connection.auto_reconnect,
            alt_nicks: connection.alt_nicks.join(", "),
            on_connect: entry.on_connect.clone(),
            error: None,
        }
    }

    fn apply_preset(&mut self, preset: Preset) {
        self.preset = Some(preset);
        preset.name.clone_into(&mut self.name);
        preset.host.clone_into(&mut self.host);
        self.port = preset.port.to_string();
        self.tls = preset.tls;
    }

    pub fn update(&mut self, msg: Msg) {
        self.error = None;
        match msg {
            Msg::Preset(preset) => self.apply_preset(preset),
            Msg::Name(value) => self.name = value,
            Msg::Host(value) => {
                self.host = value;
                self.preset = None;
            }
            Msg::Port(value) => {
                if value.chars().all(|c| c.is_ascii_digit()) && value.len() <= 5 {
                    self.port = value;
                }
            }
            Msg::Tls(tls) => {
                // Switch to the usual port unless it was customized.
                match (tls, self.port.as_str()) {
                    (true, "6667") => self.port = "6697".into(),
                    (false, "6697") => self.port = "6667".into(),
                    _ => {}
                }
                self.tls = tls;
            }
            Msg::AcceptInvalidCerts(value) => self.accept_invalid_certs = value,
            Msg::Nick(value) => {
                if self.username.is_empty() || self.username == self.nick {
                    self.username.clone_from(&value);
                }
                self.nick = value;
            }
            Msg::Username(value) => self.username = value,
            Msg::Realname(value) => self.realname = value,
            Msg::Password(value) => self.password = value,
            Msg::Auth(value) => {
                if value == Auth::Password && self.sasl_username.is_empty() {
                    self.sasl_username.clone_from(&self.nick);
                }
                self.auth = value;
            }
            Msg::ClientCert(value) => self.client_cert = value,
            Msg::CreateCertificate => match certificate::create(&self.name, &self.nick) {
                Ok(path) => {
                    self.client_cert = path.display().to_string();
                    self.error = None;
                }
                Err(error) => self.error = Some(tr!("Cannot create the certificate: {error}", error = error)),
            },
            Msg::SaslUsername(value) => self.sasl_username = value,
            Msg::SaslPassword(value) => self.sasl_password = value,
            Msg::Autojoin(value) => self.autojoin = value,
            Msg::AutoConnect(value) => self.auto_connect = value,
            Msg::AutoReconnect(value) => self.auto_reconnect = value,
            Msg::AltNicks(value) => self.alt_nicks = value,
            Msg::AddCommand => self.on_connect.push(String::new()),
            Msg::Command(index, value) => {
                if let Some(command) = self.on_connect.get_mut(index) {
                    *command = value;
                }
            }
            Msg::RemoveCommand(index) => {
                if index < self.on_connect.len() {
                    self.on_connect.remove(index);
                }
            }
        }
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
    }

    /// Validates the form and builds the configuration entry.
    pub fn validate(&self) -> Result<ServerEntry, String> {
        let name = self.name.trim();
        let host = self.host.trim();
        let nick = self.nick.trim();
        if name.is_empty() {
            return Err(tr("The network name is required.").into());
        }
        if host.is_empty() || host.contains(|c: char| c.is_whitespace() || c.is_control() || c == '/') {
            return Err(tr("The server address is invalid.").into());
        }
        let port: u16 = self
            .port
            .parse()
            .ok()
            .filter(|&port| port > 0)
            .ok_or(tr("The port must be between 1 and 65535."))?;
        if !is_valid_nick(nick) {
            return Err(tr("The nickname is invalid.").into());
        }
        let alt_nicks: Vec<String> = self
            .alt_nicks
            .split([',', ' '])
            .map(str::trim)
            .filter(|alt| !alt.is_empty())
            .map(str::to_owned)
            .collect();
        if let Some(bad) = alt_nicks.iter().find(|alt| !is_valid_nick(alt)) {
            return Err(tr!("Invalid alternative nickname: {nick}", nick = bad));
        }
        let on_connect: Vec<String> = self
            .on_connect
            .iter()
            .map(|command| command.trim())
            .filter(|command| !command.is_empty())
            .map(str::to_owned)
            .collect();
        let client_cert = Some(self.client_cert.trim()).filter(|path| !path.is_empty()).map(PathBuf::from);
        if self.auth == Auth::Certificate {
            if !self.tls {
                return Err(tr("Certificate identification requires TLS.").into());
            }
            if client_cert.is_none() {
                return Err(tr("Choose or create a client certificate.").into());
            }
        }
        if let Some(path) = client_cert.as_deref().filter(|_| self.tls) {
            ircnext_client::connection::load_identity(path).map_err(|error| tr!("Certificate: {error}", error = error))?;
        }
        let sasl = if self.auth == Auth::Password {
            if self.sasl_username.trim().is_empty() || self.sasl_password.is_empty() {
                return Err(tr("The SASL account and password are required.").into());
            }
            if !self.tls && !ircnext_client::config::is_loopback(host) {
                return Err(tr("SASL requires TLS: without encryption, the password would travel in clear text.").into());
            }
            Some(Sasl {
                username: self.sasl_username.trim().to_owned(),
                password: Secret::new(self.sasl_password.clone()),
            })
        } else {
            None
        };
        let non_empty = |value: &str, fallback: &str| {
            let value = value.trim();
            if value.is_empty() { fallback.to_owned() } else { value.to_owned() }
        };
        let autojoin = self
            .autojoin
            .split([',', ' '])
            .map(str::trim)
            .filter(|channel| !channel.is_empty() && !channel.contains(char::is_control))
            .map(|channel| {
                if channel.starts_with(['#', '&']) { channel.to_owned() } else { format!("#{channel}") }
            })
            .collect();

        Ok(ServerEntry {
            name: name.to_owned(),
            auto_connect: self.auto_connect,
            on_connect,
            connection: Config {
                host: host.to_owned(),
                port,
                tls: self.tls,
                accept_invalid_certs: self.tls && self.accept_invalid_certs,
                nick: nick.to_owned(),
                alt_nicks,
                username: non_empty(&self.username, nick),
                realname: non_empty(&self.realname, nick),
                password: (!self.password.is_empty()).then(|| Secret::new(self.password.clone())),
                sasl,
                client_cert,
                sasl_external: self.auth == Auth::Certificate,
                autojoin,
                auto_reconnect: self.auto_reconnect,
            },
        })
    }

    pub fn view(&self, c: Colors) -> Element<'_, Message> {
        let input = |placeholder: &str, value: &str, on_input: fn(String) -> Msg| {
            form::input(placeholder, value, move |v| Message::Form(on_input(v)), c).on_submit(Message::SaveServerForm)
        };
        let switch = |label: &'static str, description: Option<&'static str>, value: bool, on_toggle: fn(bool) -> Msg| {
            let setting = form::setting(label, form::switch(value, move |v| Message::Form(on_toggle(v))));
            match description {
                Some(description) => setting.description(description),
                None => setting,
            }
            .view(c)
        };

        let mut server = vec![
            form::setting(
                tr("Preset network"),
                form::select(PRESETS, self.preset, |p| Message::Form(Msg::Preset(p)), c),
            )
            .view(c),
            form::setting(tr("Name"), input("Libera.Chat", &self.name, Msg::Name)).view(c),
            form::setting(tr("Address"), input(tr("irc.example.net"), &self.host, Msg::Host)).view(c),
            form::setting("Port", input("6697", &self.port, Msg::Port)).view(c),
            switch(tr("Encrypted connection (TLS)"), Some(tr("Recommended, required for SASL")), self.tls, Msg::Tls),
        ];
        if self.tls {
            server.push(switch(
                tr("Accept invalid certificates"),
                Some(tr("Not recommended: exposes you to interception")),
                self.accept_invalid_certs,
                Msg::AcceptInvalidCerts,
            ));
        }

        let identity = [
            form::setting(tr("Nickname"), input(tr("nickname"), &self.nick, Msg::Nick)).view(c),
            form::setting(tr("Alternative nicknames"), input(tr("nick_, nick2"), &self.alt_nicks, Msg::AltNicks))
                .description(tr("Tried in order if the first one is taken"))
                .view(c),
            form::setting(tr("Username"), input(tr("nickname"), &self.username, Msg::Username)).view(c),
            form::setting(tr("Real name"), input(tr("Name shown in /whois"), &self.realname, Msg::Realname)).view(c),
            form::setting(tr("Server password"), input("", &self.password, Msg::Password).secure(true))
                .description(tr("Optional (PASS), for a bouncer for example"))
                .view(c),
        ];

        let method = form::setting(tr("Identification"), form::select(Auth::ALL, Some(self.auth), |a| Message::Form(Msg::Auth(a)), c));
        let method = match self.auth {
            Auth::None => method.description(tr("No identification to the network account")),
            Auth::Password => method.description(tr("SASL PLAIN, as soon as you connect")),
            Auth::Certificate => method.description(tr("SASL EXTERNAL. First link the certificate to your account: /msg NickServ CERT ADD")),
        };
        let mut authentication = vec![method.view(c)];
        if self.auth == Auth::Password {
            authentication.push(form::setting(tr("Account"), input(tr("account"), &self.sasl_username, Msg::SaslUsername)).view(c));
            authentication.push(
                form::setting(tr("Password"), input("", &self.sasl_password, Msg::SaslPassword).secure(true)).view(c),
            );
        }
        if self.tls {
            let control = row![
                input(tr("~/certificate.pem"), &self.client_cert, Msg::ClientCert).width(Length::Fill),
                button(text(tr("Create")).size(13))
                    .padding([7, 10])
                    .style(theme::secondary(c))
                    .on_press(Message::Form(Msg::CreateCertificate)),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .width(form::CONTROL_WIDTH);
            authentication.push(
                form::setting(tr("Client certificate"), control)
                    .description(tr("PEM file with its key. Also recognized by NickServ (CertFP)"))
                    .view(c),
            );
        }

        let mut commands = column![].spacing(8);
        for (index, command) in self.on_connect.iter().enumerate() {
            commands = commands.push(
                row![
                    form::input("/msg NickServ …", command, move |v| Message::Form(Msg::Command(index, v)), c)
                        .width(Length::Fill),
                    button(text("✕").size(12).center())
                        .width(30)
                        .padding([6, 0])
                        .style(theme::ghost(c))
                        .on_press(Message::Form(Msg::RemoveCommand(index))),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            );
        }
        commands = commands.push(
            button(text(tr("＋ Add a command")).size(13))
                .padding([6, 10])
                .style(theme::secondary(c))
                .on_press(Message::Form(Msg::AddCommand)),
        );

        let connection = [
            form::setting(tr("Channels to join"), input("#rust, #linux", &self.autojoin, Msg::Autojoin))
                .description(tr("Separated by commas"))
                .view(c),
            switch(tr("Connect at startup"), None, self.auto_connect, Msg::AutoConnect),
            switch(tr("Automatic reconnection"), Some(tr("After a network outage")), self.auto_reconnect, Msg::AutoReconnect),
            form::block(
                tr("Commands after connecting"),
                Some(tr("Run in order, as if you typed them")),
                commands,
                c,
            ),
        ];

        let title = if self.editing.is_some() { tr("Edit the network") } else { tr("Add a network") };
        let body = scrollable(
            container(form::page(
                title,
                [
                    form::group(tr("Server"), server, c),
                    form::group(tr("Identity"), identity, c),
                    form::group(tr("Authentication"), authentication, c),
                    form::group(tr("Connection"), connection, c),
                ],
                c,
            ))
            .padding(iced::Padding { right: 16.0, bottom: 8.0, ..iced::Padding::ZERO }),
        )
        .style(theme::scrollbar(c))
        .height(Length::Fill);

        let error: Element<'_, Message> = match &self.error {
            Some(error) => text(error.as_str()).size(13).color(c.danger).into(),
            None => space().into(),
        };

        let mut actions = row![].spacing(8).align_y(Alignment::Center);
        if let Some(id) = self.editing {
            actions = actions.push(
                button(text(tr("Delete")).size(14))
                    .padding([8, 16])
                    .style(theme::danger(c))
                    .on_press(Message::RemoveServer(id)),
            );
        }
        actions = actions
            .push(space().width(Length::Fill))
            .push(
                button(text(tr("Cancel")).size(14))
                    .padding([8, 16])
                    .style(theme::secondary(c))
                    .on_press(Message::CloseModal),
            )
            .push(
                button(text(if self.editing.is_some() { tr("Save") } else { tr("Add and connect") }).size(14))
                    .padding([8, 16])
                    .style(theme::primary(c))
                    .on_press(Message::SaveServerForm),
            );

        container(column![body, error, actions].spacing(14))
            .padding(24)
            .width(680)
            .height(Length::Fixed(640.0))
            .style(theme::card(c))
            .into()
    }
}

/// Nickname acceptable to IRC servers.
fn is_valid_nick(nick: &str) -> bool {
    !nick.is_empty()
        && !nick.contains(|c: char| c.is_whitespace() || c.is_control() || ",*?!@".contains(c))
        && !nick.starts_with(['#', '&', ':', '$'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_form_is_valid_libera() {
        let form = ServerForm::new(&Settings::default());
        let entry = form.validate().unwrap();
        assert_eq!(entry.name, "Libera.Chat");
        assert_eq!(entry.connection.port, 6697);
        assert!(entry.connection.tls);
    }

    #[test]
    fn normalizes_autojoin_channels() {
        let mut form = ServerForm::new(&Settings::default());
        form.update(Msg::Autojoin("rust, #linux  &local".into()));
        let entry = form.validate().unwrap();
        assert_eq!(entry.connection.autojoin, ["#rust", "#linux", "&local"]);
    }

    #[test]
    fn refuses_sasl_without_tls_except_locally() {
        let mut form = ServerForm::new(&Settings::default());
        form.update(Msg::Auth(Auth::Password));
        form.update(Msg::SaslUsername("me".into()));
        form.update(Msg::SaslPassword("secret".into()));
        form.update(Msg::Tls(false));
        assert!(form.validate().unwrap_err().contains("TLS"));
        form.update(Msg::Host("127.0.0.1".into()));
        assert!(form.validate().is_ok(), "local bouncer");
    }

    #[test]
    fn toggling_tls_switches_default_port() {
        let mut form = ServerForm::new(&Settings::default());
        form.update(Msg::Tls(false));
        assert_eq!(form.validate().unwrap().connection.port, 6667);
    }

    #[test]
    fn rejects_invalid_input() {
        let mut form = ServerForm::new(&Settings::default());
        form.update(Msg::Nick("bad nick".into()));
        assert!(form.validate().is_err());

        let mut form = ServerForm::new(&Settings::default());
        form.update(Msg::Port("0".into()));
        assert!(form.validate().is_err());

        let mut form = ServerForm::new(&Settings::default());
        form.update(Msg::Auth(Auth::Password));
        assert!(form.validate().is_err(), "SASL password missing");
    }

    #[test]
    fn certificate_authentication_needs_a_valid_file_and_tls() {
        let mut form = ServerForm::new(&Settings::default());
        form.update(Msg::Auth(Auth::Certificate));
        assert!(form.validate().unwrap_err().contains("certificate"));
        form.update(Msg::ClientCert("/nowhere/at-all.pem".into()));
        assert!(form.validate().unwrap_err().starts_with("Certificate:"));

        let path = std::env::temp_dir().join(format!("ircnext-form-{}.pem", std::process::id()));
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = rcgen::CertificateParams::new(["me".to_owned()]).unwrap().self_signed(&key).unwrap();
        std::fs::write(&path, format!("{}{}", cert.pem(), key.serialize_pem())).unwrap();
        form.update(Msg::ClientCert(path.display().to_string()));
        let entry = form.validate().unwrap();
        assert!(entry.connection.uses_external());
        assert_eq!(entry.connection.sasl, None);
        assert_eq!(ServerForm::edit(ServerId(1), &entry).validate().unwrap(), entry);

        form.update(Msg::Tls(false));
        assert!(form.validate().unwrap_err().contains("TLS"));
    }

    #[test]
    fn edit_round_trips_entry() {
        let mut form = ServerForm::new(&Settings::default());
        form.update(Msg::Auth(Auth::Password));
        form.update(Msg::SaslPassword("pw".into()));
        let entry = form.validate().unwrap();
        let edited = ServerForm::edit(ServerId(3), &entry);
        assert_eq!(edited.validate().unwrap(), entry);
    }
}
