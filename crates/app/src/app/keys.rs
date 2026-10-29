//! Clavier : touches à rôle fixe et actions des raccourcis configurables.

use std::time::Duration;

use iced::Task;
use iced::widget::operation;
use iced::widget::scrollable::{AbsoluteOffset, RelativeOffset};

use super::{App, INPUT_ID, Message, Modal, Shortcut};
use crate::input::Command;
use crate::keymap::{self, Action, Combo, MAC};
use crate::server::{BufferId, ServerId};
use crate::settings::Settings;
use crate::ui::chat::{MESSAGES_ID, SEARCH_ID};
use crate::ui::settings_form;
use crate::ui::switcher::{SWITCHER_ID, Switcher};

/// Défilement d'une page (en pixels logiques).
const PAGE: f32 = 400.0;
/// Délai laissé aux `QUIT` pour partir avant de fermer l'application.
const QUIT_DELAY: Duration = Duration::from_millis(1500);

impl App {
    /// Appui de touche. `captured` indique qu'un widget l'a déjà traité
    /// (le champ de saisie pour les flèches, par exemple).
    pub(super) fn on_key(&mut self, combo: Combo, captured: bool) -> Task<Message> {
        // Enregistrement d'un nouveau raccourci dans les paramètres.
        if let Some(Modal::Settings(form)) = &mut self.modal
            && form.is_capturing()
        {
            let msg = if combo.is_bare("Escape") {
                settings_form::Msg::CancelCapture
            } else {
                settings_form::Msg::Captured(combo)
            };
            form.update(msg);
            return Task::none();
        }

        if !combo.has_modifier() {
            let fixed = match combo.key.as_str() {
                "Tab" => Some(Shortcut::Complete { backwards: combo.shift }),
                "Escape" if !combo.shift => Some(Shortcut::Escape),
                "Up" if !combo.shift && !captured => Some(Shortcut::HistoryPrevious),
                "Down" if !combo.shift && !captured => Some(Shortcut::HistoryNext),
                _ => None,
            };
            if let Some(shortcut) = fixed {
                return self.on_shortcut(shortcut);
            }
        }

        let Some(action) = keymap::action_for(&self.config.settings.keybindings, &combo, MAC) else {
            return Task::none();
        };
        // Une touche seule déjà utilisée par un champ reste au champ.
        if captured && !combo.has_modifier() {
            return Task::none();
        }
        // Une fenêtre ouverte ne laisse passer que les actions globales.
        if self.modal.is_some() && !matches!(action, Action::Quit | Action::OpenSettings | Action::QuickSwitcher) {
            return Task::none();
        }
        self.perform(action)
    }

    pub(super) fn perform(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::NextBuffer => self.cycle_buffer(1, false),
            Action::PreviousBuffer => self.cycle_buffer(-1, false),
            Action::NextUnread => self.cycle_buffer(1, true),
            Action::PreviousUnread => self.cycle_buffer(-1, true),
            Action::GoTo1
            | Action::GoTo2
            | Action::GoTo3
            | Action::GoTo4
            | Action::GoTo5
            | Action::GoTo6
            | Action::GoTo7
            | Action::GoTo8
            | Action::GoTo9 => {
                let buffers = self.ordered_buffers();
                let index = action.goto_index().unwrap_or(1);
                // Le dernier raccourci vise toujours le dernier onglet, comme dans les navigateurs.
                let target = if action == Action::GoTo9 { buffers.last() } else { buffers.get(index - 1) };
                match target.copied() {
                    Some((server, buffer)) => self.select(server, buffer),
                    None => Task::none(),
                }
            }
            Action::QuickSwitcher => {
                self.modal = Some(Modal::Switcher(Switcher::default()));
                operation::focus(SWITCHER_ID)
            }
            Action::Search => {
                if self.active.is_none() {
                    return Task::none();
                }
                self.search.get_or_insert_with(String::new);
                operation::focus(SEARCH_ID)
            }
            Action::CloseBuffer => match self.active {
                Some((server, buffer)) => self.update(Message::CloseBuffer(server, buffer)),
                None => Task::none(),
            },
            Action::MarkAllRead => {
                for server in &mut self.servers {
                    let ids: Vec<BufferId> = server.buffers().map(|(id, _)| id).collect();
                    for id in ids {
                        server.mark_read(id);
                    }
                }
                Task::none()
            }
            Action::ScrollUp => operation::scroll_by(MESSAGES_ID, AbsoluteOffset { x: 0.0, y: PAGE }),
            Action::ScrollDown => operation::scroll_by(MESSAGES_ID, AbsoluteOffset { x: 0.0, y: -PAGE }),
            // Défilement ancré en bas : 0 est le bas, 1 le haut.
            Action::ScrollTop => operation::snap_to(MESSAGES_ID, RelativeOffset { x: 0.0, y: 1.0 }),
            Action::ScrollBottom => operation::snap_to(MESSAGES_ID, RelativeOffset { x: 0.0, y: 0.0 }),
            Action::ToggleNicklist => self.update(Message::ToggleNicklist),
            Action::ToggleSidebar => {
                self.config.settings.show_sidebar = !self.config.settings.show_sidebar;
                self.save();
                Task::none()
            }
            Action::ClearBuffer => {
                if let Some((server, buffer)) = self.active {
                    let effects = self.execute(server, Some(buffer), Command::Clear);
                    return self.apply_effects(server, effects);
                }
                Task::none()
            }
            Action::ZoomIn => self.zoom(1),
            Action::ZoomOut => self.zoom(-1),
            Action::ZoomReset => self.zoom(0),
            Action::Bold => self.insert_format('\x02'),
            Action::Italic => self.insert_format('\x1d'),
            Action::Underline => self.insert_format('\x1f'),
            Action::Strikethrough => self.insert_format('\x1e'),
            Action::Color => self.insert_format('\x03'),
            Action::Reverse => self.insert_format('\x16'),
            Action::ResetFormat => self.insert_format('\x0f'),
            Action::AddNetwork => self.update(Message::OpenServerForm(None)),
            Action::OpenSettings => self.update(Message::OpenSettings),
            Action::Quit => self.quit_application(),
        }
    }

    /// Onglets dans l'ordre de la barre latérale.
    pub(super) fn ordered_buffers(&self) -> Vec<(ServerId, BufferId)> {
        self.servers
            .iter()
            .flat_map(|server| server.buffers().map(move |(id, _)| (server.id, id)))
            .collect()
    }

    /// Passe à l'onglet suivant (`step` = 1) ou précédent (-1), éventuellement
    /// en ne s'arrêtant que sur les onglets non lus.
    fn cycle_buffer(&mut self, step: isize, unread_only: bool) -> Task<Message> {
        let all = self.ordered_buffers();
        if all.is_empty() {
            return Task::none();
        }
        let len = all.len().cast_signed();
        let start = self
            .active
            .and_then(|active| all.iter().position(|&b| b == active))
            // Sans onglet actif, on part d'avant le premier (ou du premier en arrière).
            .map_or(if step > 0 { -1 } else { 0 }, usize::cast_signed);
        let is_unread = |&(server, buffer): &(ServerId, BufferId)| {
            self.server(server)
                .and_then(|s| s.buffer(buffer))
                .is_some_and(|b| b.unread > 0 || b.highlighted)
        };
        let target = (1..=len)
            .map(|offset| all[(start + step * offset).rem_euclid(len).cast_unsigned()])
            .find(|candidate| !unread_only || is_unread(candidate));
        match target {
            Some((server, buffer)) => self.select(server, buffer),
            None => Task::none(),
        }
    }

    /// Agrandit (`1`), réduit (`-1`) ou rétablit (`0`) la taille du texte.
    fn zoom(&mut self, direction: i8) -> Task<Message> {
        let settings = &mut self.config.settings;
        let range = Settings::FONT_SIZES;
        settings.font_size = match direction {
            0 => Settings::default().font_size,
            d if d > 0 => (settings.font_size + 1).min(*range.end()),
            _ => settings.font_size.saturating_sub(1).max(*range.start()),
        };
        self.save();
        Task::none()
    }

    /// Ajoute un code de mise en forme mIRC à la saisie.
    fn insert_format(&mut self, code: char) -> Task<Message> {
        if self.modal.is_some() || self.active.is_none() {
            return Task::none();
        }
        self.input.push(code);
        Task::batch([operation::focus(INPUT_ID), operation::move_cursor_to_end(INPUT_ID)])
    }

    /// Envoie `QUIT` sur chaque réseau connecté, puis ferme l'application.
    fn quit_application(&mut self) -> Task<Message> {
        let reason = Some(self.config.settings.quit_message.clone()).filter(|r| !r.is_empty());
        let connected: Vec<ServerId> = self.servers.iter().filter(|s| s.is_online()).map(|s| s.id).collect();
        for id in &connected {
            let _ = self.execute(*id, None, Command::Quit(reason.clone()));
        }
        if connected.is_empty() {
            return iced::exit();
        }
        Task::perform(tokio::time::sleep(QUIT_DELAY), |()| Message::Exit)
    }
}
