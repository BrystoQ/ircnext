//! Settings panel. Changes are previewed live and applied on save.

use iced::widget::{button, column, container, row, scrollable, space, text, text_input};
use iced::{Alignment, Color, Element, Length};

use crate::app::Message;
use crate::i18n::{Language, tr};
use crate::settings::{ACCENT_PRESETS, HexColor, NickAlignment, Settings, ThemeMode, is_valid_time_format};
use crate::theme::{self, Colors};
use crate::keymap::{self, Action, Combo, Group, MAC};
use crate::ui::{BOLD, form};
use crate::tr;

/// Scrollback sizes offered.
const SCROLLBACK_CHOICES: [usize; 5] = [500, 1000, 2000, 5000, 10_000];
/// Width of the ＋ and ↺ shortcut buttons.
const TRAILING_BUTTON: f32 = 30.0;
/// History lines offered when joining a channel.
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
            Self::Appearance => tr("Appearance"),
            Self::Chat => tr("Chat"),
            Self::Notifications => tr("Notifications"),
            Self::Privacy => tr("Privacy"),
            Self::Identity => tr("Identity"),
            Self::Shortcuts => tr("Shortcuts"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SettingsForm {
    /// Settings being edited (used for the preview).
    pub draft: Settings,
    section: Section,
    accent: String,
    timestamp_format: String,
    highlight_words: String,
    ignored: String,
    /// Action waiting for its new key combination.
    capturing: Option<Action>,
    /// Notice after a shortcut change (resolved conflict...).
    shortcut_notice: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Section(Section),
    Language(Language),
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

    /// A key combination is expected.
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
                self.shortcut_notice = Some(tr("Default shortcuts restored.").to_owned());
            }
            Msg::Section(section) => self.section = section,
            Msg::Language(language) => draft.language = language,
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

    /// Validated settings, ready to be saved.
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
            text(tr!("File: {path}", path = home_relative(crate::config::AppConfig::path().ok())))
                .size(11)
                .color(c.muted)
                .width(Length::Fill)
                .wrapping(text::Wrapping::None),
            button(text(tr("Cancel")).size(14))
                .padding([8, 16])
                .style(theme::secondary(c))
                .on_press(Message::CloseModal),
            button(text(tr("Save")).size(14))
                .padding([8, 16])
                .style(theme::primary(c))
                .on_press(Message::SaveSettings),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let body = row![
            column![text(tr("Settings")).size(20).font(BOLD).color(c.text), navigation]
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
        let accent_error = HexColor::parse(&self.accent).is_none().then_some(tr("Expected format: #rrggbb"));

        form::page(
            tr("Appearance"),
            [
                form::group(
                    tr("Language"),
                    [form::setting(
                        tr("Interface language"),
                        form::select(Language::ALL, Some(draft.language), |l| Message::Settings(Msg::Language(l)), c),
                    )
                    .description(tr("Applied when saving"))
                    .view(c)],
                    c,
                ),
                form::group(
                    tr("Colors"),
                    [
                        form::block(tr("Theme"), None, theme_grid(draft, system_dark, c), c),
                        form::setting(tr("Accent color"), accent).error(accent_error).view(c),
                    ],
                    c,
                ),
                form::group(
                    tr("Text and layout"),
                    [
                        form::setting(
                            tr("Text size"),
                            form::range(Settings::FONT_SIZES, draft.font_size, 1, "px", |s| {
                                Message::Settings(Msg::FontSize(s))
                            }, c),
                        )
                        .view(c),
                        toggle(tr("Monospace font"), Some(tr("For messages")), draft.monospace, Msg::Monospace, c),
                        toggle(tr("Compact layout"), Some(tr("Less space between lines")), draft.compact, Msg::Compact, c),
                        toggle(tr("Network sidebar"), None, draft.show_sidebar, Msg::ShowSidebar, c),
                    ],
                    c,
                ),
                form::group(
                    tr("Nicknames"),
                    [
                        form::setting(
                            tr("Column width"),
                            form::range(Settings::NICK_COLUMN_WIDTHS, draft.nick_column_width, 4, "px", |w| {
                                Message::Settings(Msg::NickColumnWidth(w))
                            }, c),
                        )
                        .view(c),
                        form::setting(
                            tr("Alignment"),
                            form::select(NickAlignment::ALL, Some(draft.nick_alignment), |a| {
                                Message::Settings(Msg::NickAlignment(a))
                            }, c),
                        )
                        .view(c),
                        toggle(tr("Nickname colors"), None, draft.nick_colors, Msg::NickColors, c),
                        toggle(tr("Status before the nickname"), Some(tr("@ operator, + voice")), draft.show_nick_prefix, Msg::ShowNickPrefix, c),
                        toggle(tr("“BOT” badge"), Some(tr("For identified bots (IRCv3)")), draft.show_bot_badge, Msg::ShowBotBadge, c),
                        toggle(tr("mIRC colors"), Some(tr("Colors in message text")), draft.show_colors, Msg::ShowColors, c),
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
            .then_some(tr("Invalid format (e.g. %H:%M:%S)"));
        let preview = chrono::Local::now().format(&draft.timestamp_format).to_string();

        form::page(
            tr("Chat"),
            [
                form::group(
                    tr("Messages"),
                    [
                        toggle(tr("Timestamps"), None, draft.show_timestamps, Msg::ShowTimestamps, c),
                        form::setting(
                            tr("Timestamp format"),
                            form::input("%H:%M", &self.timestamp_format, |v| Message::Settings(Msg::TimestampFormat(v)), c),
                        )
                        .description(tr!("strftime syntax · preview: {preview}", preview = preview))
                        .error(format_error)
                        .view(c),
                        toggle(
                            tr("Joins and parts"),
                            Some(tr("And nickname changes")),
                            draft.show_joins_parts,
                            Msg::ShowJoinsParts,
                            c,
                        ),
                        toggle(tr("User list"), None, draft.show_nicklist, Msg::ShowNicklist, c),
                        form::setting(
                            tr("Highlight words"),
                            form::input("rust, release", &self.highlight_words, |v| Message::Settings(Msg::HighlightWords(v)), c),
                        )
                        .description(tr("Highlighted like your nickname"))
                        .view(c),
                    ],
                    c,
                ),
                form::group(
                    tr("History"),
                    [
                        form::setting(
                            tr("Lines kept per tab"),
                            form::select(SCROLLBACK_CHOICES, Some(draft.scrollback), |n| Message::Settings(Msg::Scrollback(n)), c),
                        )
                        .view(c),
                        form::setting(
                            tr("History loaded on join"),
                            form::select(HISTORY_CHOICES, Some(draft.history_lines), |n| Message::Settings(Msg::HistoryLines(n)), c),
                        )
                        .description(tr("On IRCv3 servers (chathistory) · 0: none"))
                        .view(c),
                        toggle(tr("Logs on disk"), Some(tr("One text file per conversation")), draft.logging, Msg::Logging, c),
                        form::setting(
                            tr("Logs folder"),
                            button(text(tr("Open")).size(13)).padding([6, 14]).style(theme::secondary(c)).on_press(Message::OpenLogs),
                        )
                        .description(home_relative(logs_dir()))
                        .view(c),
                    ],
                    c,
                ),
                form::group(
                    tr("Filters and completion"),
                    [
                        form::setting(
                            tr("Ignored users"),
                            form::input(tr("spammer, *!*@*.example.net"), &self.ignored, |v| Message::Settings(Msg::Ignored(v)), c),
                        )
                        .description(tr("Nicknames or nick!ident@host masks · /ignore, /unignore"))
                        .view(c),
                        toggle(
                            tr("Completion by bots"),
                            Some(tr("Tab on a “!” command sends the input to the bot (IRCv3 draft)")),
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
        let attention = if MAC { tr("Bounce the Dock icon") } else { tr("Flash the taskbar button") };
        form::page(
            tr("Notifications"),
            [
                form::group(
                    tr("System notifications"),
                    [
                        toggle(tr("Mentions"), Some(tr("Your nickname or a highlight word")), draft.notify_mentions, Msg::NotifyMentions, c),
                        toggle(tr("Private messages"), None, draft.notify_queries, Msg::NotifyQueries, c),
                        toggle(
                            tr("Window in the foreground"),
                            Some(tr("Notify even when ircnext is active")),
                            draft.notify_when_focused,
                            Msg::NotifyWhenFocused,
                            c,
                        ),
                        toggle(tr("Message preview"), Some(tr("Otherwise only the sender is shown")), draft.notify_preview, Msg::NotifyPreview, c),
                        toggle(tr("Sound"), None, draft.notify_sound, Msg::NotifySound, c),
                    ],
                    c,
                ),
                form::group(
                    tr("Window"),
                    [
                        toggle(attention, Some(tr("When the window is in the background")), draft.request_attention, Msg::RequestAttention, c),
                        toggle(
                            tr("Counter in the title"),
                            Some(tr("Conversations with an unread mention or private message")),
                            draft.title_unread_count,
                            Msg::TitleUnreadCount,
                            c,
                        ),
                        toggle(tr("Others' typing indicator"), Some(tr("“alice is typing…”")), draft.show_typing, Msg::ShowTyping, c),
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
            tr("Privacy"),
            [
                form::group(
                    tr("What others see"),
                    [
                        toggle(tr("Show when I'm typing"), Some(tr("Typing indicator (IRCv3)")), draft.send_typing, Msg::SendTyping, c),
                        toggle(
                            tr("Sync read messages"),
                            Some(tr("Across your devices, through the server (draft/read-marker)")),
                            draft.send_read_markers,
                            Msg::SendReadMarkers,
                            c,
                        ),
                    ],
                    c,
                ),
                form::group(
                    tr("Passwords"),
                    [toggle(
                        keychain_label(),
                        Some(tr("Otherwise they are written to the configuration file, readable by you only")),
                        draft.use_keychain,
                        Msg::UseKeychain,
                        c,
                    )],
                    c,
                ),
                form::group(
                    tr("CTCP requests"),
                    [
                        toggle(tr("Automatic replies"), Some(tr("VERSION, PING and TIME")), draft.ctcp_replies, Msg::CtcpReplies, c),
                        form::setting(
                            tr("Reply to VERSION"),
                            form::input("ircnext", &draft.ctcp_version, |v| Message::Settings(Msg::CtcpVersion(v)), c),
                        )
                        .description(tr("Empty: ircnext and its version"))
                        .view(c),
                    ],
                    c,
                ),
                form::note(tr("TIME answers in UTC so as not to reveal your time zone. Replies are limited to 3 every 10 seconds."), c),
            ],
            c,
        )
    }

    fn identity(&self, c: Colors) -> Element<'_, Message> {
        let draft = &self.draft;
        form::page(
            tr("Identity"),
            [
                form::group(
                    tr("New networks"),
                    [
                        form::setting(tr("Nickname"), form::input(tr("nickname"), &draft.nick, |v| Message::Settings(Msg::Nick(v)), c))
                            .view(c),
                        form::setting(tr("Real name"), form::input(tr("Real name"), &draft.realname, |v| Message::Settings(Msg::Realname(v)), c))
                            .description(tr("Shown in /whois"))
                            .view(c),
                    ],
                    c,
                ),
                form::group(
                    tr("Leave messages"),
                    [
                        form::setting(
                            tr("When leaving a network"),
                            form::input(tr("See you soon"), &draft.quit_message, |v| Message::Settings(Msg::QuitMessage(v)), c),
                        )
                        .view(c),
                        form::setting(
                            tr("When leaving a channel"),
                            form::input(tr("(none)"), &draft.part_message, |v| Message::Settings(Msg::PartMessage(v)), c),
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
            tr!(
                "Click ＋ then press the key combination you want. In the configuration file, “Mod” stands for {modifier}, so your settings work on both macOS and Windows.",
                modifier = modifier
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
            tr("Fixed keys"),
            keymap::fixed_keys()
                .into_iter()
                .map(|(keys, role)| form::setting(role, text(keys).size(13).font(BOLD).color(c.text)).view(c)),
            c,
        ));
        groups.push(form::group(
            tr("Reset"),
            [form::setting(
                tr("Default shortcuts"),
                button(text(tr("Restore all")).size(13))
                    .padding([6, 14])
                    .style(theme::danger(c))
                    .on_press(Message::Settings(Msg::ResetAllBindings)),
            )
            .description(tr("Undoes all your customizations"))
            .view(c)],
            c,
        ));
        form::page(tr("Shortcuts"), groups, c)
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
                button(text(tr("Press a key combination… (Esc)")).size(12).color(c.accent))
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
        // Space kept even without customization, so the right edge stays aligned.
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

/// Name of each system's keychain.
fn keychain_label() -> &'static str {
    if cfg!(target_os = "macos") {
        tr("Store in the macOS Keychain")
    } else if cfg!(target_os = "windows") {
        tr("Store in the Windows Credential Manager")
    } else {
        tr("Store in the system keyring")
    }
}

/// Theme cards, four per row, with a preview of their colors.
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

/// Row with a switch.
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

/// Effective shortcuts of an action, in their saved form.
fn effective(overrides: &keymap::Overrides, action: Action) -> Vec<String> {
    keymap::bindings(overrides, action, MAC).iter().map(Combo::to_config).collect()
}

/// Binds `combo` to `action`, removing it from the other actions that used
/// it. Returns a message to display, if any.
fn assign(overrides: &mut keymap::Overrides, action: Action, combo: &Combo) -> Option<String> {
    if keymap::is_reserved(combo) {
        return Some(tr!("{combo} is reserved (see “Fixed keys”).", combo = combo.display(MAC)));
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
    (!names.is_empty()).then(|| tr!("{combo} was removed from: {actions}.", combo = combo.display(MAC), actions = names.join(", ")))
}

/// Items of a comma-separated list.
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

/// Path shown with `~` for the home folder.
fn home_relative(path: Option<std::path::PathBuf>) -> String {
    let Some(path) = path else {
        return tr("not found").to_owned();
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
        assert!(form.shortcut_notice.as_deref().unwrap().contains("Close tab"));

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
