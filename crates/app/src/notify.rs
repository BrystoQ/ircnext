//! Notifications du système (Centre de notifications sur macOS, centre de
//! notifications de Windows, D-Bus sous Linux).

/// Son par défaut de chaque système.
const SOUND: &str = if cfg!(target_os = "macos") {
    "Ping"
} else if cfg!(target_os = "windows") {
    "Default"
} else {
    "message-new-instant"
};

/// Affiche une notification sans bloquer l'interface.
pub fn show(title: &str, body: &str, sound: bool) {
    let (title, body) = (title.to_owned(), body.to_owned());
    // L'envoi peut bloquer (D-Bus, macOS) : il se fait hors du fil de l'interface.
    std::thread::spawn(move || {
        let mut notification = notify_rust::Notification::new();
        notification.appname("ircnext").summary(&title).body(&body);
        if sound {
            notification.sound_name(SOUND);
        }
        // Une notification refusée par le système n'a pas à interrompre l'application.
        let _ = notification.show();
    });
}
