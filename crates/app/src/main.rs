//! ircnext : client IRC moderne, compatible IRCv3.

mod app;
mod buffer;
mod completion;
mod config;
mod ignore;
mod input;
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
