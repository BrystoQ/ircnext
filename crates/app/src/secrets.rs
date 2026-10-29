//! Masking of passwords sent to NickServ: they must appear neither in the
//! displayed history nor in the input history.

/// Replaces a password on display.
const MASK: &str = "••••••••";

/// Text to display for a message sent to NickServ, if it contains a
/// password.
pub fn mask(text: &str) -> Option<String> {
    let mut words: Vec<&str> = text.split_whitespace().collect();
    let command = words.first()?.to_ascii_uppercase();
    let secret = match (command.as_str(), words.len()) {
        // IDENTIFY [account] <password>
        ("IDENTIFY" | "ID" | "LOGIN", n) if n >= 2 => n - 1,
        // REGISTER <password> [email]
        ("REGISTER", n) if n >= 2 => 1,
        // GHOST <nick> <password>, DROP <account> <password>...
        ("GHOST" | "REGAIN" | "RELEASE" | "RECOVER" | "DROP", n) if n >= 3 => 2,
        // SET PASSWORD <password>
        ("SET", n) if n >= 3 && words[1].eq_ignore_ascii_case("PASSWORD") => 2,
        _ => return None,
    };
    words[secret] = MASK;
    Some(words.join(" "))
}

/// Whether an input sends a password to NickServ, directly (`/ns`,
/// `/msg NickServ`) or from the conversation with the service.
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

/// Target naming NickServ (including `NickServ@services.example`).
pub fn is_nickserv(target: &str) -> bool {
    target.split('@').next().is_some_and(|name| name.eq_ignore_ascii_case("NickServ"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_passwords_in_service_commands() {
        assert_eq!(mask("IDENTIFY hunter2").as_deref(), Some("IDENTIFY ••••••••"));
        assert_eq!(mask("identify account hunter2").as_deref(), Some("identify account ••••••••"));
        assert_eq!(mask("REGISTER hunter2 me@example.org").as_deref(), Some("REGISTER •••••••• me@example.org"));
        assert_eq!(mask("GHOST me hunter2").as_deref(), Some("GHOST me ••••••••"));
        assert_eq!(mask("SET PASSWORD hunter2").as_deref(), Some("SET PASSWORD ••••••••"));
        assert_eq!(mask("GHOST me"), None);
        assert_eq!(mask("INFO me"), None);
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
        assert!(!input_has_secret("/ns INFO me", None));
    }
}
