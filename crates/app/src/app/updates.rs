//! New versions: the check at startup (once a day) or from the settings,
//! the notice, and the installation.

use std::path::PathBuf;

use chrono::Local;
use iced::widget::{button, container, row, text};
use iced::{Alignment, Element, Length, Task};

use super::{App, Message, Modal};
use crate::i18n::tr;
use crate::theme::{self, Colors};
use crate::tr;
use crate::update::{self, Release};

/// Time between two automatic checks.
const CHECK_INTERVAL: i64 = 24 * 60 * 60;

/// A newer version, shown above the conversation.
#[derive(Debug, Clone)]
pub struct UpdateNotice {
    pub release: Release,
    pub state: UpdateState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateState {
    Available,
    Downloading,
    Failed(String),
}

/// How this copy is updated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Install {
    /// Download the installer and open it.
    Installer,
    /// Portable copy, or one not installed by our package: the release
    /// page, to download it by hand.
    ReleasePage,
}

fn install_kind(release: &Release) -> Install {
    if crate::config::portable_dir().is_some() || release.installer().is_none() {
        return Install::ReleasePage;
    }
    // The `.deb` only replaces the copy it installed.
    if cfg!(target_os = "linux") {
        let packaged = std::env::current_exe().is_ok_and(|exe| exe.starts_with("/usr/bin"));
        if !packaged {
            return Install::ReleasePage;
        }
    }
    Install::Installer
}

impl App {
    /// Startup check, if enabled and not done in the last day.
    pub(super) fn startup_update_check(&mut self) -> Task<Message> {
        let now = Local::now().timestamp();
        if !self.config.settings.check_updates || now - self.config.last_update_check < CHECK_INTERVAL {
            return Task::none();
        }
        self.check_updates(false)
    }

    /// Asks GitHub for the latest release; `manual`: from the settings.
    pub(super) fn check_updates(&mut self, manual: bool) -> Task<Message> {
        if self.checking_update {
            return Task::none();
        }
        self.checking_update = true;
        self.set_update_status(tr("Checking…").to_owned());
        Task::perform(
            async { tokio::task::spawn_blocking(update::check).await.unwrap_or_else(|error| Err(error.to_string())) },
            move |result| Message::UpdateChecked { manual, result },
        )
    }

    pub(super) fn update_checked(&mut self, manual: bool, result: Result<Option<Release>, String>) {
        self.checking_update = false;
        match result {
            Ok(release) => {
                self.config.last_update_check = Local::now().timestamp();
                self.save();
                let status = match &release {
                    Some(release) => tr!("Version {version} is available", version = release.version()),
                    None => tr!("ircnext {version} is up to date", version = env!("CARGO_PKG_VERSION")),
                };
                self.set_update_status(status);
                if let Some(release) = release {
                    self.update = Some(UpdateNotice { release, state: UpdateState::Available });
                }
            }
            // Offline at startup: tried again next time, silently.
            Err(error) => {
                let status = tr!("Cannot check for updates: {error}", error = error);
                if manual {
                    self.set_update_status(status);
                } else {
                    self.set_update_status(String::new());
                }
            }
        }
    }

    /// Installs the version of the notice.
    pub(super) fn install_update(&mut self) -> Task<Message> {
        let Some(notice) = &mut self.update else {
            return Task::none();
        };
        if notice.state == UpdateState::Downloading {
            return Task::none();
        }
        let release = notice.release.clone();
        if install_kind(&release) == Install::ReleasePage {
            return Task::done(Message::OpenUrl(release.html_url));
        }
        notice.state = UpdateState::Downloading;
        let dir = crate::transfer::download_dir(&self.config.settings.download_dir);
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || update::download(&release, &dir))
                    .await
                    .unwrap_or_else(|error| Err(error.to_string()))
            },
            Message::UpdateDownloaded,
        )
    }

    pub(super) fn update_downloaded(&mut self, result: Result<PathBuf, String>) -> Task<Message> {
        let Some(notice) = &mut self.update else {
            return Task::none();
        };
        let opened = result.and_then(|path| open::that_detached(&path).map_err(|error| error.to_string()));
        match opened {
            Ok(()) => {
                notice.state = UpdateState::Available;
                // The Windows installer replaces the running program: it
                // asks to close it, and starts the new version.
                if cfg!(target_os = "windows") {
                    return Task::done(Message::Exit);
                }
                self.update = None;
            }
            Err(error) => notice.state = UpdateState::Failed(error),
        }
        Task::none()
    }

    /// Status line of the settings' update group.
    fn set_update_status(&mut self, status: String) {
        self.update_status = status;
        if let Some(Modal::Settings(form)) = &mut self.modal {
            form.update_status.clone_from(&self.update_status);
        }
    }
}

/// Bar above the conversation: the new version and what to do with it.
pub fn view(notice: &UpdateNotice, c: Colors) -> Element<'_, Message> {
    let release = &notice.release;
    let (label, color) = match &notice.state {
        UpdateState::Available => (tr!("ircnext {version} is available", version = release.version()), c.text),
        UpdateState::Downloading => (tr!("Downloading ircnext {version}…", version = release.version()), c.text),
        UpdateState::Failed(error) => (tr!("Update failed: {error}", error = error), c.danger),
    };
    let install_label = match install_kind(release) {
        Install::Installer => tr("Install"),
        Install::ReleasePage => tr("Download"),
    };
    let install = button(text(install_label).size(13)).padding([4, 12]).style(theme::primary(c));
    let install = if notice.state == UpdateState::Downloading { install } else { install.on_press(Message::InstallUpdate) };
    container(
        row![
            text(label).size(13).color(color).width(Length::Fill),
            button(text(tr("What's new")).size(13))
                .padding([4, 12])
                .style(theme::secondary(c))
                .on_press(Message::OpenUrl(release.html_url.clone())),
            install,
            button(text("✕").size(12)).padding([2, 8]).style(theme::ghost(c)).on_press(Message::DismissUpdate),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([8, 16])
    .width(Length::Fill)
    .style(theme::panel(c.accent.scale_alpha(0.16)))
    .into()
}
