//! Log viewer (`/logs`): the conversations saved on disk, read and searched
//! without leaving the application.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use iced::widget::{button, column, container, row, scrollable, space, text, text_input};
use iced::{Alignment, Element, Length};

use crate::app::Message;
use crate::i18n::tr;
use crate::theme::{self, Colors};
use crate::tr;
use crate::ui::BOLD;

/// Id of the search field.
pub const LOG_SEARCH_ID: &str = "log-search";
/// Size read at most from the end of a log.
const MAX_READ: u64 = 8 * 1024 * 1024;
/// Lines shown at most (the most recent ones).
const MAX_LINES: usize = 2000;

/// A log file: one conversation of one network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogFile {
    pub network: String,
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct LogViewer {
    pub files: Vec<LogFile>,
    pub selected: Option<usize>,
    /// Lines of the selected file.
    pub lines: Vec<String>,
    /// Only the end of a long file was read.
    pub truncated: bool,
    pub query: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Select(usize),
    Query(String),
    /// Opens the selected file with the system.
    OpenFile,
}

/// Log files under `dir` (`<network>/<conversation>.log`), by network then
/// name.
pub fn scan(dir: &Path) -> Vec<LogFile> {
    let mut files = Vec::new();
    let Ok(networks) = std::fs::read_dir(dir) else {
        return files;
    };
    for network in networks.flatten().filter(|entry| entry.path().is_dir()) {
        let Ok(logs) = std::fs::read_dir(network.path()) else {
            continue;
        };
        for log in logs.flatten() {
            let path = log.path();
            if path.extension().is_some_and(|extension| extension == "log") {
                files.push(LogFile {
                    network: network.file_name().to_string_lossy().into_owned(),
                    name: path.file_stem().unwrap_or_default().to_string_lossy().into_owned(),
                    path,
                });
            }
        }
    }
    files.sort_by(|a, b| (a.network.to_lowercase(), &a.name).cmp(&(b.network.to_lowercase(), &b.name)));
    files
}

/// Lines of `path`, or of its last `limit` bytes; `true` if cut.
fn read_lines(path: &Path, limit: u64) -> std::io::Result<(Vec<String>, bool)> {
    let mut file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    let truncated = size > limit;
    if truncated {
        file.seek(SeekFrom::Start(size - limit))?;
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    // The first line read from the middle of the file is incomplete.
    if truncated && !lines.is_empty() {
        lines.remove(0);
    }
    Ok((lines, truncated))
}

impl LogViewer {
    /// The logs of `dir`, with the file of `current` (network, conversation)
    /// selected if it exists.
    pub fn open(dir: &Path, current: Option<&Path>) -> Self {
        let mut viewer = Self { files: scan(dir), ..Self::default() };
        if let Some(index) = current.and_then(|current| viewer.files.iter().position(|file| file.path == current)) {
            viewer.select(index);
        }
        viewer
    }

    pub fn select(&mut self, index: usize) {
        let Some(file) = self.files.get(index) else {
            return;
        };
        self.selected = Some(index);
        match read_lines(&file.path, MAX_READ) {
            Ok((lines, truncated)) => {
                self.lines = lines;
                self.truncated = truncated;
                self.error = None;
            }
            Err(error) => {
                self.lines.clear();
                self.error = Some(tr!("Cannot read {path}: {error}", path = file.path.display(), error = error));
            }
        }
    }

    pub fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Select(index) => self.select(index),
            Msg::Query(query) => self.query = query,
            Msg::OpenFile => {
                if let Some(file) = self.selected.and_then(|index| self.files.get(index))
                    && let Err(error) = open::that_detached(&file.path)
                {
                    self.error = Some(tr!("Cannot open {path}: {error}", path = file.path.display(), error = error));
                }
            }
        }
    }

    /// Lines of the selected file matching the search.
    pub fn matching(&self) -> Vec<&str> {
        let query = self.query.trim().to_lowercase();
        self.lines
            .iter()
            .filter(|line| query.is_empty() || line.to_lowercase().contains(&query))
            .map(String::as_str)
            .collect()
    }

    pub fn view(&self, c: Colors) -> Element<'_, Message> {
        let msg = |msg: Msg| Message::LogViewer(msg);
        let mut network = "";
        let mut list = column![].spacing(2);
        for (index, file) in self.files.iter().enumerate() {
            if file.network != network {
                network = &file.network;
                list = list.push(container(text(&file.network).size(11).color(c.muted)).padding([8, 8]));
            }
            list = list.push(
                button(text(&file.name).size(13))
                    .width(Length::Fill)
                    .padding([5, 10])
                    .style(theme::list_item(c, self.selected == Some(index)))
                    .on_press(msg(Msg::Select(index))),
            );
        }
        let files: Element<'_, Message> = if self.files.is_empty() {
            text(tr("No log yet. Turn on “Logs on disk” in Settings › Chat.")).size(13).color(c.muted).into()
        } else {
            scrollable(list).style(theme::scrollbar(c)).height(Length::Fill).into()
        };

        let search = text_input(tr("Search in this log"), &self.query)
            .id(LOG_SEARCH_ID)
            .on_input(move |query| msg(Msg::Query(query)))
            .padding([8, 12])
            .size(14)
            .style(theme::input(c));
        let matching = self.matching();
        let shown = &matching[matching.len().saturating_sub(MAX_LINES)..];
        let content: Element<'_, Message> = if let Some(error) = &self.error {
            text(error).size(13).color(c.danger).into()
        } else if self.selected.is_none() {
            text(tr("Choose a conversation.")).size(13).color(c.muted).into()
        } else if shown.is_empty() {
            text(if self.query.trim().is_empty() { tr("Empty log.") } else { tr("No line matches.") }).size(13).color(c.muted).into()
        } else {
            let lines = shown.iter().fold(column![].spacing(2), |lines, line| {
                lines.push(text(*line).size(13).color(c.text).wrapping(text::Wrapping::WordOrGlyph))
            });
            scrollable(container(lines).padding(iced::Padding { right: 12.0, ..iced::Padding::ZERO }))
                .anchor_bottom()
                .style(theme::scrollbar(c))
                .height(Length::Fill)
                .into()
        };
        let mut status = if self.selected.is_none() {
            String::new()
        } else if self.query.trim().is_empty() {
            tr!("{count} lines", count = self.lines.len())
        } else {
            tr!("{matching} of {count} lines", matching = matching.len(), count = self.lines.len())
        };
        if matching.len() > MAX_LINES {
            status = tr!("{status} · the last {shown} shown", status = status, shown = MAX_LINES);
        }
        if self.truncated {
            status = tr!("{status} · end of a long file", status = status);
        }

        let actions = row![
            text(status).size(12).color(c.muted),
            space().width(Length::Fill),
            button(text(tr("Open the file")).size(13))
                .padding([7, 12])
                .style(theme::secondary(c))
                .on_press_maybe(self.selected.map(|_| msg(Msg::OpenFile))),
            button(text(tr("Open the folder")).size(13)).padding([7, 12]).style(theme::secondary(c)).on_press(Message::OpenLogs),
            button(text(tr("Close")).size(14)).padding([8, 16]).style(theme::primary(c)).on_press(Message::CloseModal),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let body = row![
            container(files).width(200).height(Length::Fill),
            column![search, content].spacing(10).width(Length::Fill),
        ]
        .spacing(16);
        container(column![text(tr("Logs")).size(20).font(BOLD).color(c.text), body, actions].spacing(14))
            .padding(24)
            .width(980)
            .height(Length::Fixed(660.0))
            .style(theme::card(c))
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::{Line, LineKind};
    use chrono::Local;

    #[test]
    fn lists_reads_and_searches_logs() {
        let dir = std::env::temp_dir().join(format!("ircnext-log-viewer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let message = |nick: &str, text: &str| Line::new(Local::now(), LineKind::Message { nick: nick.into(), prefix: None, text: text.into() });
        crate::log::append(&dir, "Libera.Chat", "#Rust", &message("alice", "hello world")).unwrap();
        crate::log::append(&dir, "Libera.Chat", "#Rust", &message("bob", "Hello again")).unwrap();
        crate::log::append(&dir, "Libera.Chat", "zoe", &message("zoe", "hi")).unwrap();
        crate::log::append(&dir, "OFTC", "#debian", &message("sam", "apt")).unwrap();
        std::fs::write(dir.join("OFTC").join("notes.txt"), "not a log").unwrap();

        let current = crate::log::path(&dir, "Libera.Chat", "#Rust");
        let mut viewer = LogViewer::open(&dir, Some(&current));
        let names: Vec<(&str, &str)> = viewer.files.iter().map(|file| (file.network.as_str(), file.name.as_str())).collect();
        assert_eq!(names, [("Libera.Chat", "#rust"), ("Libera.Chat", "zoe"), ("OFTC", "#debian")]);
        assert_eq!(viewer.selected, Some(0), "current conversation selected");
        assert_eq!(viewer.lines.len(), 2);
        viewer.update(Msg::Query("HELLO".into()));
        assert_eq!(viewer.matching().len(), 2, "case-insensitive");
        viewer.update(Msg::Query("again".into()));
        assert!(viewer.matching()[0].ends_with("<bob> Hello again"));

        // A long file: only its end, from a whole line.
        let (lines, truncated) = read_lines(&current, 50).unwrap();
        assert!(truncated);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].ends_with("Hello again"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
