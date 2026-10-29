//! Panneau des paramètres. Les modifications sont prévisualisées en direct et
//! appliquées à l'enregistrement.

use iced::widget::{button, column, container, row, scrollable, space, text, text_input};
use iced::{Alignment, Color, Element, Length};

use crate::app::Message;
use crate::settings::{ACCENT_PRESETS, HexColor, NickAlignment, Settings, ThemeMode, is_valid_time_format};
use crate::theme::{self, Colors};
use crate::keymap::{self, Action, Combo, FIXED_KEYS, Group, MAC};
use crate::ui::{BOLD, form};

/// Tailles d'historique proposées.
const SCROLLBACK_CHOICES: [usize; 5] = [500, 1000, 2000, 5000, 10_000];
/// Largeur des boutons ＋ et ↺ des raccourcis.
const TRAILING_BUTTON: f32 = 30.0;
/// Lignes d'historique proposées à l'arrivée sur un salon.
const HISTORY_CHOICES: [u16; 6] = [0, 50, 100, 200, 500, 1000];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Appearance,
    Chat,
    Notifications,
    Privacy,
    Identity,
    Shortcuts,
}

impl Section {
    pub const ALL: [Self; 6] = [
        Self::Appearance,
        Self::Chat,
        Self::Notifications,
        Self::Privacy,
        Self::Identity,
        Self::Shortcuts,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Apparence",
            Self::Chat => "Discussion",
            Self::Notifications => "Notifications",
            Self::Privacy => "Confidentialité",
            Self::Identity => "Identité",
            Self::Shortcuts => "Raccourcis",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SettingsForm {
    /// Paramètres en cours d'édition (utilisés pour l'aperçu).
    pub draft: Settings,
    section: Section,
    accent: String,
    timestamp_format: String,
    highlight_words: String,
    ignored: String,
    /// Action dont on attend la nouvelle combinaison.
    capturing: Option<Action>,
    /// Information après une modification de raccourci (conflit résolu...).
    shortcut_notice: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Section(Section),
    Theme(ThemeMode),
    Accent(HexColor),
    AccentText(String),
    FontSize(u16),
    Monospace(bool),
    Compact(bool),
    ShowTimestamps(bool),
    TimestampFormat(String),
    ShowJoinsParts(bool),
    NickColors(bool),
    ShowNicklist(bool),
    Scrollback(usize),
    HighlightWords(String),
    BotCompletion(bool),
    SendTyping(bool),
    ShowTyping(bool),
    Nick(String),
    Realname(String),
    QuitMessage(String),
    ShowSidebar(bool),
    NickColumnWidth(u16),
    NickAlignment(NickAlignment),
    ShowNickPrefix(bool),
    ShowColors(bool),
    ShowBotBadge(bool),
    HistoryLines(u16),
    Ignored(String),
    Logging(bool),
    NotifyMentions(bool),
    NotifyQueries(bool),
    NotifyWhenFocused(bool),
    NotifyPreview(bool),
    NotifySound(bool),
    RequestAttention(bool),
    TitleUnreadCount(bool),
    CtcpReplies(bool),
    CtcpVersion(String),
    SendReadMarkers(bool),
    UseKeychain(bool),
    PartMessage(String),
    CaptureBinding(Action),
    Captured(Combo),
    CancelCapture,
    RemoveBinding(Action, usize),
    ResetBinding(Action),
    ResetAllBindings,
}

impl SettingsForm {
    pub fn new(settings: &Settings) -> Self {
        Self {
            draft: settings.clone(),
            section: Section::Appearance,
            accent: settings.accent.to_string(),
            timestamp_format: settings.timestamp_format.clone(),
            highlight_words: settings.highlight_words.join(", "),
            ignored: settings.ignored.join(", "),
            capturing: None,
            shortcut_notice: None,
        }
    }

    /// Une combinaison de touches est attendue.
    pub fn is_capturing(&self) -> bool {
        self.capturing.is_some()
    }

    pub fn update(&mut self, msg: Msg) {
        let draft = &mut self.draft;
        match msg {
            Msg::ShowSidebar(value) => draft.show_sidebar = value,
            Msg::NickColumnWidth(value) => draft.nick_column_width = value,
            Msg::NickAlignment(value) => draft.nick_alignment = value,
            Msg::ShowNickPrefix(value) => draft.show_nick_prefix = value,
            Msg::ShowColors(value) => draft.show_colors = value,
            Msg::ShowBotBadge(value) => draft.show_bot_badge = value,
            Msg::HistoryLines(value) => draft.history_lines = value,
            Msg::Ignored(value) => {
                draft.ignored = split_list(&value);
                self.ignored = value;
            }
            Msg::Logging(value) => draft.logging = value,
            Msg::NotifyMentions(value) => draft.notify_mentions = value,
            Msg::NotifyQueries(value) => draft.notify_queries = value,
            Msg::NotifyWhenFocused(value) => draft.notify_when_focused = value,
            Msg::NotifyPreview(value) => draft.notify_preview = value,
            Msg::NotifySound(value) => draft.notify_sound = value,
            Msg::RequestAttention(value) => draft.request_attention = value,
            Msg::TitleUnreadCount(value) => draft.title_unread_count = value,
            Msg::CtcpReplies(value) => draft.ctcp_replies = value,
            Msg::CtcpVersion(value) => draft.ctcp_version = value,
            Msg::SendReadMarkers(value) => draft.send_read_markers = value,
            Msg::UseKeychain(value) => draft.use_keychain = value,
            Msg::PartMessage(value) => draft.part_message = value,
            Msg::CaptureBinding(action) => {
                self.capturing = Some(action);
                self.shortcut_notice = None;
            }
            Msg::CancelCapture => self.capturing = None,
            Msg::Captured(combo) => {
                if let Some(action) = self.capturing.take() {
                    self.shortcut_notice = assign(&mut draft.keybindings, action, &combo);
                }
            }
            Msg::RemoveBinding(action, index) => {
                let mut list = effective(&draft.keybindings, action);
                if index < list.len() {
                    list.remove(index);
                    draft.keybindings.insert(action, list);
                }
            }
            Msg::ResetBinding(action) => {
                draft.keybindings.remove(&action);
            }
            Msg::ResetAllBindings => {
                draft.keybindings.clear();
                self.shortcut_notice = Some("Raccourcis par défaut rétablis.".to_owned());
            }
            Msg::Section(section) => self.section = section,
            Msg::Theme(mode) => draft.theme = mode,
            Msg::Accent(color) => {
                draft.accent = color;
                self.accent = color.to_string();
            }
            Msg::AccentText(value) => {
                if let Some(color) = HexColor::parse(&value) {
                    draft.accent = color;
                }
                self.accent = value;
            }
            Msg::FontSize(size) => draft.font_size = size,
            Msg::Monospace(value) => draft.monospace = value,
            Msg::Compact(value) => draft.compact = value,
            Msg::ShowTimestamps(value) => draft.show_timestamps = value,
            Msg::TimestampFormat(value) => {
                if !value.trim().is_empty() && is_valid_time_format(&value) {
                    draft.timestamp_format.clone_from(&value);
                }
                self.timestamp_format = value;
            }
            Msg::ShowJoinsParts(value) => draft.show_joins_parts = value,
            Msg::NickColors(value) => draft.nick_colors = value,
            Msg::ShowNicklist(value) => draft.show_nicklist = value,
            Msg::Scrollback(value) => draft.scrollback = value,
            Msg::HighlightWords(value) => {
                draft.highlight_words = split_list(&value);
                self.highlight_words = value;
            }
            Msg::BotCompletion(value) => draft.bot_completion = value,
            Msg::SendTyping(value) => draft.send_typing = value,
            Msg::ShowTyping(value) => draft.show_typing = value,
            Msg::Nick(value) => draft.nick = value,
            Msg::Realname(value) => draft.realname = value,
            Msg::QuitMessage(value) => draft.quit_message = value,
        }
    }

    /// Paramètres validés, prêts à être enregistrés.
    pub fn finish(&self) -> Settings {
        let mut settings = self.draft.clone();
        settings.sanitize();
        settings
    }

    pub fn view(&self, c: Colors, system_dark: bool) -> Element<'_, Message> {
        let navigation = Section::ALL.iter().fold(column![].spacing(2), |nav, &section| {
            nav.push(
                button(text(section.label()).size(14))
                    .width(Length::Fill)
                    .padding([8, 12])
                    .style(theme::list_item(c, section == self.section))
                    .on_press(Message::Settings(Msg::Section(section))),
            )
        });

        let content = match self.section {
            Section::Appearance => self.appearance(c, system_dark),
            Section::Chat => self.chat(c),
            Section::Notifications => self.notifications(c),
            Section::Privacy => self.privacy(c),
            Section::Identity => self.identity(c),
            Section::Shortcuts => self.shortcuts(c),
        };

        let actions = row![
            text(format!("Fichier : {}", home_relative(crate::config::AppConfig::path().ok())))
                .size(11)
                .color(c.muted)
                .width(Length::Fill)
                .wrapping(text::Wrapping::None),
            button(text("Annuler").size(14))
                .padding([8, 16])
                .style(theme::secondary(c))
                .on_press(Message::CloseModal),
            button(text("Enregistrer").size(14))
                .padding([8, 16])
                .style(theme::primary(c))
                .on_press(Message::SaveSettings),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let body = row![
            column![text("Paramètres").size(20).font(BOLD).color(c.text), navigation]
                .spacing(18)
                .width(170),
            scrollable(container(content).padding(iced::Padding { right: 16.0, bottom: 8.0, ..iced::Padding::ZERO }))
                .style(theme::scrollbar(c))
                .width(Length::Fill)
                .height(Length::Fill),
        ]
        .spacing(24);

        container(column![body, actions].spacing(18))
            .padding(24)
            .width(860)
            .height(Length::Fixed(640.0))
            .style(theme::card(c))
            .into()
    }

    fn appearance(&self, c: Colors, system_dark: bool) -> Element<'_, Message> {
        let draft = &self.draft;
        let swatches = ACCENT_PRESETS.iter().fold(row![].spacing(8), |swatches, &(_, color)| {
            let [r, g, b] = color.0;
            swatches.push(
                button(space().width(20).height(20))
                    .padding(0)
                    .style(theme::swatch(Color::from_rgb8(r, g, b), draft.accent == color, c))
                    .on_press(Message::Settings(Msg::Accent(color))),
            )
        });
        let accent = row![
            swatches,
            text_input("#5b8cff", &self.accent)
                .on_input(|v| Message::Settings(Msg::AccentText(v)))
                .padding([7, 10])
                .size(14)
                .width(96)
                .style(theme::field(c)),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let accent_error = HexColor::parse(&self.accent).is_none().then_some("Format attendu : #rrggbb");

        form::page(
            "Apparence",
            [
                form::group(
                    "Couleurs",
                    [
                        form::block("Thème", None, theme_grid(draft, system_dark, c), c),
                        form::setting("Couleur d'accent", accent).error(accent_error).view(c),
                    ],
                    c,
                ),
                form::group(
                    "Texte et mise en page",
                    [
                        form::setting(
                            "Taille du texte",
                            form::range(Settings::FONT_SIZES, draft.font_size, 1, "px", |s| {
                                Message::Settings(Msg::FontSize(s))
                            }, c),
                        )
                        .view(c),
                        toggle("Police à chasse fixe", Some("Pour les messages"), draft.monospace, Msg::Monospace, c),
                        toggle("Affichage compact", Some("Réduit l'espace entre les lignes"), draft.compact, Msg::Compact, c),
                        toggle("Barre latérale des réseaux", None, draft.show_sidebar, Msg::ShowSidebar, c),
                    ],
                    c,
                ),
                form::group(
                    "Pseudonymes",
                    [
                        form::setting(
                            "Largeur de la colonne",
                            form::range(Settings::NICK_COLUMN_WIDTHS, draft.nick_column_width, 4, "px", |w| {
                                Message::Settings(Msg::NickColumnWidth(w))
                            }, c),
                        )
                        .view(c),
                        form::setting(
                            "Alignement",
                            form::select(NickAlignment::ALL, Some(draft.nick_alignment), |a| {
                                Message::Settings(Msg::NickAlignment(a))
                            }, c),
                        )
                        .view(c),
                        toggle("Couleurs de pseudonymes", None, draft.nick_colors, Msg::NickColors, c),
                        toggle("Statut devant le pseudonyme", Some("@ opérateur, + parole"), draft.show_nick_prefix, Msg::ShowNickPrefix, c),
                        toggle("Badge « BOT »", Some("Pour les bots identifiés (IRCv3)"), draft.show_bot_badge, Msg::ShowBotBadge, c),
                        toggle("Couleurs mIRC", Some("Couleurs dans le texte des messages"), draft.show_colors, Msg::ShowColors, c),
                    ],
                    c,
                ),
            ],
            c,
        )
    }

    fn chat(&self, c: Colors) -> Element<'_, Message> {
        let draft = &self.draft;
        let format_error = (!is_valid_time_format(&self.timestamp_format) || self.timestamp_format.trim().is_empty())
            .then_some("Format invalide (ex. %H:%M:%S)");
        let preview = chrono::Local::now().format(&draft.timestamp_format).to_string();

        form::page(
            "Discussion",
            [
                form::group(
                    "Messages",
                    [
                        toggle("Horodatage", None, draft.show_timestamps, Msg::ShowTimestamps, c),
                        form::setting(
                            "Format de l'horodatage",
                            form::input("%H:%M", &self.timestamp_format, |v| Message::Settings(Msg::TimestampFormat(v)), c),
                        )
                        .description(format!("Syntaxe strftime · aperçu : {preview}"))
                        .error(format_error)
                        .view(c),
                        toggle(
                            "Arrivées et départs",
                            Some("Ainsi que les changements de pseudonyme"),
                            draft.show_joins_parts,
                            Msg::ShowJoinsParts,
                            c,
                        ),
                        toggle("Liste des utilisateurs", None, draft.show_nicklist, Msg::ShowNicklist, c),
                        form::setting(
                            "Mots surveillés",
                            form::input("rust, release", &self.highlight_words, |v| Message::Settings(Msg::HighlightWords(v)), c),
                        )
                        .description("Mis en évidence comme votre pseudonyme")
                        .view(c),
                    ],
                    c,
                ),
                form::group(
                    "Historique",
                    [
                        form::setting(
                            "Lignes conservées par onglet",
                            form::select(SCROLLBACK_CHOICES, Some(draft.scrollback), |n| Message::Settings(Msg::Scrollback(n)), c),
                        )
                        .view(c),
                        form::setting(
                            "Historique chargé à l'arrivée",
                            form::select(HISTORY_CHOICES, Some(draft.history_lines), |n| Message::Settings(Msg::HistoryLines(n)), c),
                        )
                        .description("Sur les serveurs IRCv3 (chathistory) · 0 : aucun")
                        .view(c),
                        toggle("Journaux sur le disque", Some("Un fichier texte par conversation"), draft.logging, Msg::Logging, c),
                        form::setting(
                            "Dossier des journaux",
                            button(text("Ouvrir").size(13)).padding([6, 14]).style(theme::secondary(c)).on_press(Message::OpenLogs),
                        )
                        .description(home_relative(logs_dir()))
                        .view(c),
                    ],
                    c,
                ),
                form::group(
                    "Filtres et complétion",
                    [
                        form::setting(
                            "Utilisateurs ignorés",
                            form::input("spammeur, *!*@*.exemple.net", &self.ignored, |v| Message::Settings(Msg::Ignored(v)), c),
                        )
                        .description("Pseudonymes ou masques pseudo!ident@hôte · /ignore, /unignore")
                        .view(c),
                        toggle(
                            "Complétion par les bots",
                            Some("Tab sur une commande « ! » : la saisie est envoyée au bot (brouillon IRCv3)"),
                            draft.bot_completion,
                            Msg::BotCompletion,
                            c,
                        ),
                    ],
                    c,
                ),
            ],
            c,
        )
    }

    fn notifications(&self, c: Colors) -> Element<'_, Message> {
        let draft = &self.draft;
        let attention = if MAC { "Rebond de l'icône dans le Dock" } else { "Clignotement dans la barre des tâches" };
        form::page(
            "Notifications",
            [
                form::group(
                    "Notifications du système",
                    [
                        toggle("Mentions", Some("Votre pseudonyme ou un mot surveillé"), draft.notify_mentions, Msg::NotifyMentions, c),
                        toggle("Messages privés", None, draft.notify_queries, Msg::NotifyQueries, c),
                        toggle(
                            "Fenêtre au premier plan",
                            Some("Notifier même quand ircnext est actif"),
                            draft.notify_when_focused,
                            Msg::NotifyWhenFocused,
                            c,
                        ),
                        toggle("Aperçu du message", Some("Sinon, seul l'expéditeur est indiqué"), draft.notify_preview, Msg::NotifyPreview, c),
                        toggle("Son", None, draft.notify_sound, Msg::NotifySound, c),
                    ],
                    c,
                ),
                form::group(
                    "Fenêtre",
                    [
                        toggle(attention, Some("Quand la fenêtre est en arrière-plan"), draft.request_attention, Msg::RequestAttention, c),
                        toggle(
                            "Compteur dans le titre",
                            Some("Conversations avec une mention ou un message privé non lu"),
                            draft.title_unread_count,
                            Msg::TitleUnreadCount,
                            c,
                        ),
                        toggle("Indicateur de saisie des autres", Some("« alice est en train d'écrire… »"), draft.show_typing, Msg::ShowTyping, c),
                    ],
                    c,
                ),
            ],
            c,
        )
    }

    fn privacy(&self, c: Colors) -> Element<'_, Message> {
        let draft = &self.draft;
        form::page(
            "Confidentialité",
            [
                form::group(
                    "Ce que voient les autres",
                    [
                        toggle("Signaler quand j'écris", Some("Indicateur de saisie (IRCv3)"), draft.send_typing, Msg::SendTyping, c),
                        toggle(
                            "Synchroniser les messages lus",
                            Some("Entre vos appareils, via le serveur (draft/read-marker)"),
                            draft.send_read_markers,
                            Msg::SendReadMarkers,
                            c,
                        ),
                    ],
                    c,
                ),
                form::group(
                    "Mots de passe",
                    [toggle(
                        KEYCHAIN_LABEL,
                        Some("Sinon, ils sont écrits dans le fichier de configuration, lisible par vous seul"),
                        draft.use_keychain,
                        Msg::UseKeychain,
                        c,
                    )],
                    c,
                ),
                form::group(
                    "Requêtes CTCP",
                    [
                        toggle("Réponses automatiques", Some("VERSION, PING et TIME"), draft.ctcp_replies, Msg::CtcpReplies, c),
                        form::setting(
                            "Réponse à VERSION",
                            form::input("ircnext", &draft.ctcp_version, |v| Message::Settings(Msg::CtcpVersion(v)), c),
                        )
                        .description("Vide : ircnext et sa version")
                        .view(c),
                    ],
                    c,
                ),
                form::note("TIME répond en UTC pour ne pas révéler votre fuseau horaire. Les réponses sont limitées à 3 toutes les 10 secondes.", c),
            ],
            c,
        )
    }

    fn identity(&self, c: Colors) -> Element<'_, Message> {
        let draft = &self.draft;
        form::page(
            "Identité",
            [
                form::group(
                    "Nouveaux réseaux",
                    [
                        form::setting("Pseudonyme", form::input("pseudo", &draft.nick, |v| Message::Settings(Msg::Nick(v)), c))
                            .view(c),
                        form::setting("Nom réel", form::input("Nom réel", &draft.realname, |v| Message::Settings(Msg::Realname(v)), c))
                            .description("Affiché dans /whois")
                            .view(c),
                    ],
                    c,
                ),
                form::group(
                    "Messages de départ",
                    [
                        form::setting(
                            "En quittant un réseau",
                            form::input("À bientôt", &draft.quit_message, |v| Message::Settings(Msg::QuitMessage(v)), c),
                        )
                        .view(c),
                        form::setting(
                            "En quittant un salon",
                            form::input("(aucun)", &draft.part_message, |v| Message::Settings(Msg::PartMessage(v)), c),
                        )
                        .view(c),
                    ],
                    c,
                ),
            ],
            c,
        )
    }

    fn shortcuts(&self, c: Colors) -> Element<'_, Message> {
        let overrides = &self.draft.keybindings;
        let modifier = if MAC { "⌘" } else { "Ctrl" };
        let mut groups: Vec<Element<'_, Message>> = vec![form::note(
            format!(
                "Cliquez sur ＋ puis appuyez sur la combinaison voulue. Dans le fichier de configuration, « Mod » désigne {modifier} : vos réglages restent valables sur macOS comme sur Windows."
            ),
            c,
        )];
        if let Some(notice) = &self.shortcut_notice {
            groups.push(container(text(notice.as_str()).size(12).color(c.warning)).padding([0, 4]).into());
        }
        for group in Group::ALL {
            let rows = Action::ALL
                .iter()
                .filter(|action| action.group() == group)
                .map(|&action| form::setting(action.label(), self.binding_chips(action, overrides, c)).view(c));
            groups.push(form::group(group.label(), rows, c));
        }
        groups.push(form::group(
            "Touches fixes",
            FIXED_KEYS
                .iter()
                .map(|&(keys, role)| form::setting(role, text(keys).size(13).font(BOLD).color(c.text)).view(c)),
            c,
        ));
        groups.push(form::group(
            "Réinitialisation",
            [form::setting(
                "Raccourcis par défaut",
                button(text("Tout rétablir").size(13))
                    .padding([6, 14])
                    .style(theme::danger(c))
                    .on_press(Message::Settings(Msg::ResetAllBindings)),
            )
            .description("Annule toutes vos personnalisations")
            .view(c)],
            c,
        ));
        form::page("Raccourcis", groups, c)
    }

    fn binding_chips(&self, action: Action, overrides: &keymap::Overrides, c: Colors) -> Element<'_, Message> {
        let mut chips = row![].spacing(6).align_y(Alignment::Center);
        for (index, combo) in keymap::bindings(overrides, action, MAC).iter().enumerate() {
            chips = chips.push(
                button(text(format!("{}  ✕", combo.display(MAC))).size(12))
                    .padding([4, 8])
                    .style(theme::secondary(c))
                    .on_press(Message::Settings(Msg::RemoveBinding(action, index))),
            );
        }
        if self.capturing == Some(action) {
            chips = chips.push(
                button(text("Appuyez sur une combinaison… (Échap)").size(12).color(c.accent))
                    .padding([4, 8])
                    .style(theme::ghost(c))
                    .on_press(Message::Settings(Msg::CancelCapture)),
            );
        } else {
            chips = chips.push(
                button(text("＋").size(12).center())
                    .width(TRAILING_BUTTON)
                    .padding([4, 0])
                    .style(theme::ghost(c))
                    .on_press(Message::Settings(Msg::CaptureBinding(action))),
            );
        }
        // Place réservée même sans personnalisation : le bord droit reste aligné.
        let reset: Element<'_, Message> = if overrides.contains_key(&action) {
            button(text("↺").size(12).center())
                .width(TRAILING_BUTTON)
                .padding([4, 0])
                .style(theme::ghost(c))
                .on_press(Message::Settings(Msg::ResetBinding(action)))
                .into()
        } else {
            space().width(TRAILING_BUTTON).into()
        };
        chips.push(reset).into()
    }
}

/// Vignettes des thèmes, quatre par ligne, avec un aperçu de leurs couleurs.
/// Nom du trousseau propre à chaque système.
const KEYCHAIN_LABEL: &str = if cfg!(target_os = "macos") {
    "Ranger dans le Trousseau d'accès"
} else if cfg!(target_os = "windows") {
    "Ranger dans le Gestionnaire d'identifiants"
} else {
    "Ranger dans le trousseau du système"
};

fn theme_grid(draft: &Settings, system_dark: bool, c: Colors) -> Element<'static, Message> {
    let card = |mode: ThemeMode| {
        let p = Colors::new(mode, system_dark, draft.accent);
        let bar = |color: Color, width: f32| {
            container(space().width(width).height(5)).style(theme::badge(color))
        };
        let preview = container(row![
            container(space().width(22).height(Length::Fill)).style(theme::panel(p.sidebar)),
            column![bar(p.text, 52.0), bar(p.muted, 36.0), bar(p.accent, 44.0), bar(p.muted, 28.0)]
                .spacing(6)
                .padding(8),
        ])
        .height(58)
        .width(Length::Fill)
        .clip(true)
        .style(theme::preview(p));
        button(column![preview, text(mode.to_string()).size(13)].spacing(6))
            .width(Length::Fill)
            .padding(6)
            .style(theme::theme_card(c, draft.theme == mode))
            .on_press(Message::Settings(Msg::Theme(mode)))
    };
    ThemeMode::ALL
        .chunks(4)
        .fold(column![].spacing(10), |grid, modes| {
            grid.push(modes.iter().fold(row![].spacing(10), |line, &mode| line.push(card(mode))))
        })
        .into()
}

/// Ligne avec interrupteur.
fn toggle<'a>(
    label: &'a str,
    description: Option<&'a str>,
    value: bool,
    on_toggle: fn(bool) -> Msg,
    c: Colors,
) -> Element<'a, Message> {
    let setting = form::setting(label, form::switch(value, move |v| Message::Settings(on_toggle(v))));
    match description {
        Some(description) => setting.description(description),
        None => setting,
    }
    .view(c)
}

/// Raccourcis effectifs d'une action, sous leur forme enregistrée.
fn effective(overrides: &keymap::Overrides, action: Action) -> Vec<String> {
    keymap::bindings(overrides, action, MAC).iter().map(Combo::to_config).collect()
}

/// Associe `combo` à `action`, en le retirant des autres actions qui
/// l'utilisaient. Renvoie un message à afficher, le cas échéant.
fn assign(overrides: &mut keymap::Overrides, action: Action, combo: &Combo) -> Option<String> {
    if keymap::is_reserved(combo) {
        return Some(format!("{} est réservé (voir « Touches fixes »).", combo.display(MAC)));
    }
    let taken_by = keymap::conflicts(overrides, combo, action, MAC);
    for &other in &taken_by {
        let remaining: Vec<String> = keymap::bindings(overrides, other, MAC)
            .iter()
            .filter(|existing| *existing != combo)
            .map(Combo::to_config)
            .collect();
        overrides.insert(other, remaining);
    }
    let mut list = effective(overrides, action);
    let text = combo.to_config();
    if !list.contains(&text) {
        list.push(text);
    }
    overrides.insert(action, list);
    let names: Vec<&str> = taken_by.iter().map(|other| other.label()).collect();
    (!names.is_empty()).then(|| format!("{} a été retiré de : {}.", combo.display(MAC), names.join(", ")))
}

/// Éléments d'une liste saisie séparés par des virgules.
fn split_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

fn logs_dir() -> Option<std::path::PathBuf> {
    Some(crate::config::AppConfig::path().ok()?.parent()?.join("logs"))
}

/// Chemin affiché avec `~` pour le dossier personnel.
fn home_relative(path: Option<std::path::PathBuf>) -> String {
    let Some(path) = path else {
        return "introuvable".to_owned();
    };
    match directories::BaseDirs::new().and_then(|dirs| path.strip_prefix(dirs.home_dir()).ok().map(|p| p.display().to_string())) {
        Some(relative) => format!("~/{relative}"),
        None => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_values_do_not_reach_the_draft() {
        let mut form = SettingsForm::new(&Settings::default());
        form.update(Msg::AccentText("#12".into()));
        form.update(Msg::TimestampFormat("%Q".into()));
        assert_eq!(form.draft.accent, Settings::default().accent);
        assert_eq!(form.draft.timestamp_format, "%H:%M");

        form.update(Msg::AccentText("#112233".into()));
        assert_eq!(form.draft.accent, HexColor([0x11, 0x22, 0x33]));
    }

    #[test]
    fn captured_shortcut_moves_from_its_previous_action() {
        let mut form = SettingsForm::new(&Settings::default());
        form.update(Msg::CaptureBinding(Action::Search));
        assert!(form.is_capturing());
        let close = Combo::parse("Mod+W", MAC).unwrap();
        form.update(Msg::Captured(close.clone()));
        assert!(!form.is_capturing());
        let overrides = &form.draft.keybindings;
        assert_eq!(keymap::action_for(overrides, &close, MAC), Some(Action::Search));
        assert!(!keymap::bindings(overrides, Action::CloseBuffer, MAC).contains(&close));
        assert!(form.shortcut_notice.as_deref().unwrap().contains("Fermer l'onglet"));

        form.update(Msg::ResetAllBindings);
        assert!(form.draft.keybindings.is_empty());
    }

    #[test]
    fn reserved_keys_cannot_be_bound() {
        let mut form = SettingsForm::new(&Settings::default());
        form.update(Msg::CaptureBinding(Action::Search));
        form.update(Msg::Captured(Combo::parse("Tab", MAC).unwrap()));
        assert!(form.draft.keybindings.is_empty());
        assert!(form.shortcut_notice.is_some());
    }

    #[test]
    fn removing_a_binding_keeps_the_others() {
        let mut form = SettingsForm::new(&Settings::default());
        let before = keymap::bindings(&form.draft.keybindings, Action::NextBuffer, MAC);
        form.update(Msg::RemoveBinding(Action::NextBuffer, 0));
        let after = keymap::bindings(&form.draft.keybindings, Action::NextBuffer, MAC);
        assert_eq!(after, before[1..]);
        form.update(Msg::ResetBinding(Action::NextBuffer));
        assert_eq!(keymap::bindings(&form.draft.keybindings, Action::NextBuffer, MAC), before);
    }

    #[test]
    fn parses_highlight_words() {
        let mut form = SettingsForm::new(&Settings::default());
        form.update(Msg::HighlightWords(" rust, ,release ".into()));
        assert_eq!(form.finish().highlight_words, ["rust", "release"]);
    }
}
