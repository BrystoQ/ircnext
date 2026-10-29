//! Zone de discussion : en-tête, messages, indicateur de saisie et champ de saisie.

use std::time::Instant;

use iced::widget::{button, center, column, container, row, scrollable, space, stack, text, text_input, tooltip};
use iced::{Alignment, Element, Length, alignment};
use ircnext_client::proto::format;

use crate::app::{INPUT_ID, Message};
use crate::buffer::{Buffer, Kind};
use crate::server::{BufferId, Server, Status};
use crate::keymap::{self, Action};
use crate::settings::Settings;
use crate::suggest::Suggestion;
use crate::theme::{self, Colors};
use crate::ui::{BOLD, line};

/// Nombre maximal de lignes rendues (les plus récentes).
const RENDERED_LINES: usize = 500;
/// Identifiant de la zone des messages (défilement au clavier).
pub const MESSAGES_ID: &str = "messages";
/// Identifiant du champ de recherche.
pub const SEARCH_ID: &str = "search";

pub struct Props<'a> {
    pub server: &'a Server,
    pub buffer_id: BufferId,
    pub buffer: &'a Buffer,
    pub settings: &'a Settings,
    pub colors: Colors,
    pub input: &'a str,
    /// Recherche en cours dans l'onglet.
    pub search: Option<&'a str>,
    pub suggestions: &'a [Suggestion],
    pub selected_suggestion: usize,
    pub banner: Option<&'a str>,
    pub now: Instant,
}

pub fn view<'a>(props: &Props<'a>) -> Element<'a, Message> {
    let Props { server, buffer, settings, colors: c, .. } = *props;

    let needle = props.search.map(|query| query.trim().to_lowercase()).filter(|q| !q.is_empty());
    let mut lines: Vec<_> = buffer
        .lines()
        .rev()
        .filter(|line| match &needle {
            // Une recherche porte sur tout l'historique, arrivées et départs compris.
            Some(needle) => line.matches(needle),
            None => settings.show_joins_parts || !line.kind.is_presence(),
        })
        .take(RENDERED_LINES)
        .collect();
    lines.reverse();

    let messages: Element<'_, Message> = if lines.is_empty() {
        let hint = if needle.is_some() { "Aucun message ne correspond." } else { empty_hint(buffer) };
        center(text(hint).size(14).color(c.muted)).into()
    } else {
        let column = lines.into_iter().fold(column![].padding([12, 0]), |column, entry| {
            column.push(line::view(entry, server.id, settings, c))
        });
        scrollable(column)
            .id(MESSAGES_ID)
            .anchor_bottom()
            .style(theme::scrollbar(c))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    };

    let mut content = column![header(props)];
    if let Some(query) = props.search {
        content = content.push(search_bar(query, lines_count(needle.as_deref(), buffer), c));
    }
    if let Some(banner) = props.banner {
        content = content.push(banner_view(banner, c));
    }
    let messages: Element<'_, Message> = match suggestions(props) {
        Some(popup) => stack![
            messages,
            container(popup)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_y(alignment::Vertical::Bottom)
                .padding([4, 16]),
        ]
        .into(),
        None => messages,
    };
    content = content
        .push(messages)
        .push(typing_indicator(props))
        .push(input(props));

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(theme::panel(c.background))
        .into()
}

fn header<'a>(props: &Props<'a>) -> Element<'a, Message> {
    let &Props { server, buffer, buffer_id, colors: c, settings, .. } = props;

    let subtitle = match buffer.kind {
        Kind::Channel => {
            let channel = server.network.channel(&buffer.name);
            match channel.and_then(|channel| channel.topic.as_deref()) {
                Some(topic) => format::strip(topic),
                None if !buffer.joined => "Vous n'êtes pas sur ce salon.".to_owned(),
                None => "Aucun sujet".to_owned(),
            }
        }
        Kind::Query => match server.network.is_online(&buffer.name) {
            Some(true) => "En ligne".to_owned(),
            Some(false) => "Hors ligne".to_owned(),
            None => "Conversation privée".to_owned(),
        },
        Kind::Server => {
            let connection = &server.entry.connection;
            let status = match &server.status {
                Status::Online => "connecté",
                Status::Registering | Status::Connecting(_) => "connexion…",
                Status::Waiting { .. } => "reconnexion…",
                Status::Disconnected(_) => "déconnecté",
            };
            format!(
                "{}:{}{} — {status}",
                connection.host,
                connection.port,
                if connection.tls { " · TLS" } else { "" }
            )
        }
    };

    let title = column![
        text(buffer.name.as_str()).size(16).font(BOLD).color(c.text),
        text(subtitle).size(12).color(c.muted).wrapping(text::Wrapping::None),
    ]
    .spacing(2)
    .width(Length::Fill)
    .clip(true);

    // Infobulle : libellé suivi du premier raccourci configuré.
    let hint = |label: &str, action: Action| match keymap::bindings(&settings.keybindings, action, keymap::MAC).first() {
        Some(combo) => format!("{label} ({})", combo.display(keymap::MAC)),
        None => label.to_owned(),
    };
    let mut actions = row![].spacing(4).align_y(Alignment::Center);
    actions = actions.push(header_button("⌕", hint("Rechercher", Action::Search), Message::Action(Action::Search), c));
    if buffer.kind == Kind::Channel {
        let label = if settings.show_nicklist { "Masquer les membres" } else { "Afficher les membres" };
        actions = actions.push(header_button("☰", hint(label, Action::ToggleNicklist), Message::ToggleNicklist, c));
    }
    if buffer.kind != Kind::Server {
        let close = Message::CloseBuffer(server.id, buffer_id);
        actions = actions.push(header_button("✕", hint("Fermer l'onglet", Action::CloseBuffer), close, c));
    }

    container(row![title, actions].spacing(12).align_y(Alignment::Center))
        .padding([12, 16])
        .width(Length::Fill)
        .style(theme::bordered(c, c.background))
        .into()
}

fn header_button(icon: &str, label: String, message: Message, c: Colors) -> Element<'_, Message> {
    tooltip(
        button(text(icon).size(15)).padding([4, 9]).style(theme::ghost(c)).on_press(message),
        container(text(label).size(12)).padding(6).style(theme::card(c)),
        tooltip::Position::Bottom,
    )
    .into()
}

fn banner_view(message: &str, c: Colors) -> Element<'_, Message> {
    container(
        row![
            text(message).size(13).color(c.text).width(Length::Fill),
            button(text("✕").size(12)).padding([2, 8]).style(theme::ghost(c)).on_press(Message::DismissBanner),
        ]
        .align_y(Alignment::Center),
    )
    .padding([8, 16])
    .width(Length::Fill)
    .style(theme::panel(c.warning.scale_alpha(0.18)))
    .into()
}

fn typing_indicator<'a>(props: &Props<'a>) -> Element<'a, Message> {
    let c = props.colors;
    if !props.settings.show_typing {
        return space().height(4).into();
    }
    let nicks: Vec<&str> = props.buffer.typing(props.now).collect();
    let label = match nicks.as_slice() {
        [] => return space().height(20).into(),
        [one] => format!("{one} est en train d'écrire…"),
        [first, second] => format!("{first} et {second} écrivent…"),
        _ => "Plusieurs personnes écrivent…".to_owned(),
    };
    container(text(label).size(12).color(c.muted))
        .padding([2, 18])
        .height(20)
        .into()
}

fn lines_count(needle: Option<&str>, buffer: &Buffer) -> Option<usize> {
    needle.map(|needle| buffer.lines().filter(|line| line.matches(needle)).count())
}

fn search_bar(query: &str, found: Option<usize>, c: Colors) -> Element<'_, Message> {
    let count = match found {
        None => String::new(),
        Some(0) => "aucun résultat".to_owned(),
        Some(1) => "1 résultat".to_owned(),
        Some(n) => format!("{n} résultats"),
    };
    container(
        row![
            text("⌕").size(16).color(c.muted),
            text_input("Rechercher dans l'onglet…", query)
                .id(SEARCH_ID)
                .on_input(Message::SearchChanged)
                .padding([6, 10])
                .size(14)
                .style(theme::input(c)),
            text(count).size(12).color(c.muted),
            button(text("✕").size(12))
                .padding([4, 8])
                .style(theme::ghost(c))
                .on_press(Message::Shortcut(crate::app::Shortcut::Escape)),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding([8, 16])
    .width(Length::Fill)
    .style(theme::bordered(c, c.sidebar))
    .into()
}

/// Liste des suggestions, au-dessus du champ de saisie.
fn suggestions<'a>(props: &Props<'a>) -> Option<Element<'a, Message>> {
    let c = props.colors;
    if props.suggestions.is_empty() {
        return None;
    }
    let rows = props
        .suggestions
        .iter()
        .enumerate()
        .fold(column![].spacing(2), |rows, (index, suggestion)| {
            let entry = row![
                text(suggestion.label.as_str()).size(13).font(BOLD).color(c.text),
                text(suggestion.detail.as_str())
                    .size(12)
                    .color(c.muted)
                    .width(Length::Fill)
                    .wrapping(text::Wrapping::None),
                text(suggestion.source.as_str()).size(11).color(c.muted),
            ]
            .spacing(12)
            .align_y(Alignment::Center);
            rows.push(
                button(entry)
                    .width(Length::Fill)
                    .padding([6, 10])
                    .style(theme::list_item(c, index == props.selected_suggestion))
                    .on_press(Message::AcceptSuggestion(index)),
            )
        });
    let hint = text("↑↓ pour choisir · Tab ou Entrée pour valider · Échap pour fermer")
        .size(11)
        .color(c.muted);
    Some(
        container(column![rows, container(hint).padding([2, 10])].spacing(4))
            .padding(6)
            .width(Length::Fill)
            .style(theme::card(c))
            .into(),
    )
}

fn input<'a>(props: &Props<'a>) -> Element<'a, Message> {
    let &Props { server, buffer, colors: c, settings, .. } = props;
    let placeholder = if !server.is_online() && server.status != Status::Registering {
        "Non connecté — tapez /connect pour vous connecter".to_owned()
    } else {
        match buffer.kind {
            Kind::Channel => format!("Message dans {}", buffer.name),
            Kind::Query => format!("Message à {}", buffer.name),
            Kind::Server => "Commande… (/help pour la liste)".to_owned(),
        }
    };

    container(
        text_input(&placeholder, props.input)
            .id(INPUT_ID)
            .on_input(Message::InputChanged)
            .on_submit(Message::Submit)
            .padding([11, 14])
            .size(f32::from(settings.font_size))
            .style(theme::input(c)),
    )
    .padding(iced::Padding { top: 4.0, right: 16.0, bottom: 16.0, left: 16.0 })
    .into()
}

fn empty_hint(buffer: &Buffer) -> &'static str {
    match buffer.kind {
        Kind::Server => "Les messages du serveur apparaîtront ici.",
        Kind::Channel => "Aucun message pour l'instant.",
        Kind::Query => "Début de la conversation.",
    }
}

/// Écran affiché lorsqu'aucun réseau n'est configuré.
pub fn welcome(c: Colors, banner: Option<&str>) -> Element<'_, Message> {
    let mut content = column![
        text("Bienvenue dans ircnext").size(26).font(BOLD).color(c.text),
        text("Un client IRC moderne, compatible IRCv3.").size(15).color(c.muted),
        space().height(12),
        button(text("Ajouter un réseau").size(15))
            .padding([10, 18])
            .style(theme::primary(c))
            .on_press(Message::OpenServerForm(None)),
    ]
    .spacing(8)
    .align_x(Alignment::Center);
    if let Some(banner) = banner {
        content = content.push(text(banner).size(13).color(c.warning));
    }
    container(center(content))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(theme::panel(c.background))
        .into()
}
