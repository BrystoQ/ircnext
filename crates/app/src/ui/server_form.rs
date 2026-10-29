//! Formulaire d'ajout ou de modification d'un réseau.

use iced::widget::{button, column, container, row, scrollable, space, text};
use iced::{Alignment, Element, Length};
use ircnext_client::{Config, Sasl, Secret};

use crate::app::Message;
use crate::config::{PRESETS, Preset, ServerEntry};
use crate::server::ServerId;
use crate::settings::Settings;
use crate::theme::{self, Colors};
use crate::ui::form;

#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)] // reflète les cases à cocher du formulaire
pub struct ServerForm {
    /// Réseau modifié, ou `None` pour un ajout.
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
    sasl: bool,
    sasl_username: String,
    sasl_password: String,
    autojoin: String,
    auto_connect: bool,
    auto_reconnect: bool,
    alt_nicks: String,
    on_connect: Vec<String>,
    error: Option<String>,
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
    Sasl(bool),
    SaslUsername(String),
    SaslPassword(String),
    Autojoin(String),
    AutoConnect(bool),
    AutoReconnect(bool),
    AltNicks(String),
    AddCommand,
    Command(usize, String),
    RemoveCommand(usize),
}

impl ServerForm {
    /// Nouveau réseau, prérempli avec Libera.Chat et l'identité par défaut.
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
            sasl: false,
            sasl_username: String::new(),
            sasl_password: String::new(),
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
            sasl: connection.sasl.is_some(),
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
                // On bascule le port usuel s'il n'a pas été personnalisé.
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
            Msg::Sasl(value) => {
                if value && self.sasl_username.is_empty() {
                    self.sasl_username.clone_from(&self.nick);
                }
                self.sasl = value;
            }
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

    /// Valide le formulaire et construit l'entrée de configuration.
    pub fn validate(&self) -> Result<ServerEntry, String> {
        let name = self.name.trim();
        let host = self.host.trim();
        let nick = self.nick.trim();
        if name.is_empty() {
            return Err("Le nom du réseau est obligatoire.".into());
        }
        if host.is_empty() || host.contains(|c: char| c.is_whitespace() || c.is_control() || c == '/') {
            return Err("L'adresse du serveur est invalide.".into());
        }
        let port: u16 = self
            .port
            .parse()
            .ok()
            .filter(|&port| port > 0)
            .ok_or("Le port doit être compris entre 1 et 65535.")?;
        if !is_valid_nick(nick) {
            return Err("Le pseudonyme est invalide.".into());
        }
        let alt_nicks: Vec<String> = self
            .alt_nicks
            .split([',', ' '])
            .map(str::trim)
            .filter(|alt| !alt.is_empty())
            .map(str::to_owned)
            .collect();
        if let Some(bad) = alt_nicks.iter().find(|alt| !is_valid_nick(alt)) {
            return Err(format!("Pseudonyme de secours invalide : {bad}"));
        }
        let on_connect: Vec<String> = self
            .on_connect
            .iter()
            .map(|command| command.trim())
            .filter(|command| !command.is_empty())
            .map(str::to_owned)
            .collect();
        let sasl = if self.sasl {
            if self.sasl_username.trim().is_empty() || self.sasl_password.is_empty() {
                return Err("Le compte et le mot de passe SASL sont obligatoires.".into());
            }
            if !self.tls && !ircnext_client::config::is_loopback(host) {
                return Err("SASL exige TLS : sans chiffrement, le mot de passe circulerait en clair.".into());
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
                "Réseau prédéfini",
                form::select(PRESETS, self.preset, |p| Message::Form(Msg::Preset(p)), c),
            )
            .view(c),
            form::setting("Nom", input("Libera.Chat", &self.name, Msg::Name)).view(c),
            form::setting("Adresse", input("irc.exemple.net", &self.host, Msg::Host)).view(c),
            form::setting("Port", input("6697", &self.port, Msg::Port)).view(c),
            switch("Connexion chiffrée (TLS)", Some("Recommandé, obligatoire pour SASL"), self.tls, Msg::Tls),
        ];
        if self.tls {
            server.push(switch(
                "Accepter les certificats invalides",
                Some("Déconseillé : expose aux interceptions"),
                self.accept_invalid_certs,
                Msg::AcceptInvalidCerts,
            ));
        }

        let identity = [
            form::setting("Pseudonyme", input("pseudo", &self.nick, Msg::Nick)).view(c),
            form::setting("Pseudonymes de secours", input("pseudo_, pseudo2", &self.alt_nicks, Msg::AltNicks))
                .description("Essayés dans l'ordre si le premier est pris")
                .view(c),
            form::setting("Nom d'utilisateur", input("pseudo", &self.username, Msg::Username)).view(c),
            form::setting("Nom réel", input("Nom affiché dans /whois", &self.realname, Msg::Realname)).view(c),
            form::setting("Mot de passe du serveur", input("", &self.password, Msg::Password).secure(true))
                .description("Facultatif (PASS), pour un bouncer par exemple")
                .view(c),
        ];

        let mut authentication = vec![switch(
            "Se connecter avec SASL",
            Some("Identification au compte NickServ dès la connexion"),
            self.sasl,
            Msg::Sasl,
        )];
        if self.sasl {
            authentication.push(form::setting("Compte", input("compte", &self.sasl_username, Msg::SaslUsername)).view(c));
            authentication.push(
                form::setting("Mot de passe", input("", &self.sasl_password, Msg::SaslPassword).secure(true)).view(c),
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
            button(text("＋ Ajouter une commande").size(13))
                .padding([6, 10])
                .style(theme::secondary(c))
                .on_press(Message::Form(Msg::AddCommand)),
        );

        let connection = [
            form::setting("Salons à rejoindre", input("#rust, #linux", &self.autojoin, Msg::Autojoin))
                .description("Séparés par des virgules")
                .view(c),
            switch("Connexion au lancement", None, self.auto_connect, Msg::AutoConnect),
            switch("Reconnexion automatique", Some("Après une coupure du réseau"), self.auto_reconnect, Msg::AutoReconnect),
            form::block(
                "Commandes après la connexion",
                Some("Exécutées dans l'ordre, comme si vous les tapiez"),
                commands,
                c,
            ),
        ];

        let title = if self.editing.is_some() { "Modifier le réseau" } else { "Ajouter un réseau" };
        let body = scrollable(
            container(form::page(
                title,
                [
                    form::group("Serveur", server, c),
                    form::group("Identité", identity, c),
                    form::group("Authentification", authentication, c),
                    form::group("Connexion", connection, c),
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
                button(text("Supprimer").size(14))
                    .padding([8, 16])
                    .style(theme::danger(c))
                    .on_press(Message::RemoveServer(id)),
            );
        }
        actions = actions
            .push(space().width(Length::Fill))
            .push(
                button(text("Annuler").size(14))
                    .padding([8, 16])
                    .style(theme::secondary(c))
                    .on_press(Message::CloseModal),
            )
            .push(
                button(text(if self.editing.is_some() { "Enregistrer" } else { "Ajouter et se connecter" }).size(14))
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

/// Pseudonyme acceptable par les serveurs IRC.
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
        form.update(Msg::Sasl(true));
        form.update(Msg::SaslUsername("moi".into()));
        form.update(Msg::SaslPassword("secret".into()));
        form.update(Msg::Tls(false));
        assert!(form.validate().unwrap_err().contains("TLS"));
        form.update(Msg::Host("127.0.0.1".into()));
        assert!(form.validate().is_ok(), "bouncer local");
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
        form.update(Msg::Sasl(true));
        assert!(form.validate().is_err(), "mot de passe SASL manquant");
    }

    #[test]
    fn edit_round_trips_entry() {
        let mut form = ServerForm::new(&Settings::default());
        form.update(Msg::Sasl(true));
        form.update(Msg::SaslPassword("pw".into()));
        let entry = form.validate().unwrap();
        let edited = ServerForm::edit(ServerId(3), &entry);
        assert_eq!(edited.validate().unwrap(), entry);
    }
}
