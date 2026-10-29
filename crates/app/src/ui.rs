//! Interface views and shared elements.

pub mod chat;
pub mod form;
pub mod line;
pub mod nicklist;
pub mod server_form;
pub mod settings_form;
pub mod sidebar;
pub mod switcher;

use iced::{Font, font};

pub const BOLD: Font = Font {
    weight: font::Weight::Semibold,
    ..Font::DEFAULT
};
