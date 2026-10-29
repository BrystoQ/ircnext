//! Masquage des mots de passe envoyés à NickServ : ils ne doivent apparaître
//! ni dans l'historique affiché, ni dans l'historique de saisie.

/// Remplace un mot de passe à l'affichage.
const MASK: &str = "••••••••";

/// Texte à afficher pour un message envoyé à NickServ, si un mot de passe
/// y figure.
pub fn mask(text: &str) -> Option<String> {
    let mut words: Vec<&str> = text.split_whitespace().collect();
    let command = words.first()?.to_ascii_uppercase();
    let secret = match (command.as_str(), words.len()) {
        // IDENTIFY [compte] <mot de passe>
        ("IDENTIFY" | "ID" | "LOGIN", n) if n >= 2 => n - 1,
        // REGISTER <mot de passe> [e-mail]
        ("REGISTER", n) if n >= 2 => 1,
        // GHOST <pseudo> <mot de passe>, DROP <compte> <mot de passe>...
        ("GHOST" | "REGAIN" | "RELEASE" | "RECOVER" | "DROP", n) if n >= 3 => 2,
        // SET PASSWORD <mot de passe>
        ("SET", n) if n >= 3 && words[1].eq_ignore_ascii_case("PASSWORD") => 2,
        _ => return None,
    };
    words[secret] = MASK;
    Some(words.join(" "))
}

/// Indique si une saisie envoie un mot de passe à NickServ, directement
/// (`/ns`, `/msg NickServ`) ou depuis la conversation avec le service.
pub fn input_has_secret(input: &str, query: Option<&str>) -> bool {
    let body = match input.strip_prefix('/') {
        Some(command) => {
            let mut words = command.splitn(2, ' ');
            let name = words.next().unwrap_or_default().to_ascii_lowercase();
            let rest = words.next().unwrap_or_default();
            match name.as_str() {
                "ns" | "nickserv" => rest,
                "msg" | "query" | "q" => match rest.split_once(' ') {
                    Some((target, text)) if is_nickserv(target) => text,
                    _ => return false,
                },
                _ => return false,
            }
        }
        None if query.is_some_and(is_nickserv) => input,
        None => return false,
    };
    mask(body).is_some()
}

/// Cible désignant NickServ (y compris `NickServ@services.example`).
pub fn is_nickserv(target: &str) -> bool {
    target.split('@').next().is_some_and(|name| name.eq_ignore_ascii_case("NickServ"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_passwords_in_service_commands() {
        assert_eq!(mask("IDENTIFY hunter2").as_deref(), Some("IDENTIFY ••••••••"));
        assert_eq!(mask("identify compte hunter2").as_deref(), Some("identify compte ••••••••"));
        assert_eq!(mask("REGISTER hunter2 moi@example.org").as_deref(), Some("REGISTER •••••••• moi@example.org"));
        assert_eq!(mask("GHOST moi hunter2").as_deref(), Some("GHOST moi ••••••••"));
        assert_eq!(mask("SET PASSWORD hunter2").as_deref(), Some("SET PASSWORD ••••••••"));
        assert_eq!(mask("GHOST moi"), None);
        assert_eq!(mask("INFO moi"), None);
        assert_eq!(mask(""), None);
    }

    #[test]
    fn detects_secrets_in_every_input_form() {
        assert!(input_has_secret("/ns IDENTIFY hunter2", None));
        assert!(input_has_secret("/msg NickServ identify hunter2", None));
        assert!(input_has_secret("/msg nickserv@services.libera.chat IDENTIFY x", None));
        assert!(input_has_secret("IDENTIFY hunter2", Some("NickServ")));
        assert!(!input_has_secret("IDENTIFY hunter2", Some("alice")));
        assert!(!input_has_secret("/msg alice IDENTIFY hunter2", None));
        assert!(!input_has_secret("/ns INFO moi", None));
    }
}
