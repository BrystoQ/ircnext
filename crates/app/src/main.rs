//! ircnext : client IRC moderne, compatible IRCv3.

// Pas de fenêtre de console derrière l'application sous Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod buffer;
mod certificate;
mod completion;
mod config;
mod ignore;
mod input;
mod keychain;
mod keymap;
mod log;
mod notify;
mod secrets;
mod server;
mod settings;
mod suggest;
mod theme;
mod ui;

use app::App;

fn main() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .window_size((1200.0, 780.0))
        .centered()
        .antialiasing(true)
        .run()
}
