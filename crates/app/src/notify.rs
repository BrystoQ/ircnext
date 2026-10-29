//! System notifications (Notification Center on macOS, Windows
//! notifications, D-Bus on Linux).

/// Default sound of each system.
const SOUND: &str = if cfg!(target_os = "macos") {
    "Ping"
} else if cfg!(target_os = "windows") {
    "Default"
} else {
    "message-new-instant"
};

/// Shows a notification without blocking the interface.
pub fn show(title: &str, body: &str, sound: bool) {
    let (title, body) = (title.to_owned(), body.to_owned());
    // Sending can block (D-Bus, macOS): it happens off the interface thread.
    std::thread::spawn(move || {
        let mut notification = notify_rust::Notification::new();
        notification.appname("ircnext").summary(&title).body(&body);
        if sound {
            notification.sound_name(SOUND);
        }
        // A notification refused by the system must not interrupt the application.
        let _ = notification.show();
    });
}
