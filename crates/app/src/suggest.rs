//! Suggestions affichées pendant la saisie : commandes, commandes des
//! services (NickServ, ChanServ...), salons, pseudonymes (`@`) et commandes
//! de bots (`!`), apprises dans le salon ou proposées par le bot lui-même
//! (brouillon IRCv3 `autocomplete`).

use crate::input::COMMANDS;

/// Nombre maximal de suggestions affichées.
pub const MAX: usize = 8;

/// Suggestion proposée sous la saisie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// Ligne de saisie complète une fois la suggestion acceptée.
    pub text: String,
    pub label: String,
    pub detail: String,
    /// Origine (« commande », « NickServ », nom du bot...).
    pub source: String,
}

/// Données disponibles pour suggérer.
#[derive(Debug, Default)]
pub struct Context<'a> {
    /// Nom de l'onglet si c'est une conversation privée.
    pub query: Option<&'a str>,
    pub channels: Vec<&'a str>,
    /// Pseudonymes, les interlocuteurs récents d'abord.
    pub nicks: Vec<&'a str>,
    /// Commandes de bots vues dans l'onglet, les plus récentes d'abord.
    pub bot_commands: Vec<&'a str>,
    /// Complétions proposées par des bots pour la saisie actuelle : (bot, texte).
    pub remote: Vec<(&'a str, &'a str)>,
}

/// Commande d'un service.
#[derive(Debug)]
pub struct ServiceCommand {
    pub name: &'static str,
    pub usage: &'static str,
    pub help: &'static str,
}

/// Service de réseau et ses commandes usuelles (communes à Atheme et Anope).
#[derive(Debug)]
pub struct Service {
    pub name: &'static str,
    /// Commande raccourcie (`ns` pour `/ns`).
    pub alias: &'static str,
    pub commands: &'static [ServiceCommand],
}

macro_rules! commands {
    ($($name:literal, $usage:literal, $help:literal;)*) => {
        &[$(ServiceCommand { name: $name, usage: $usage, help: $help }),*]
    };
}

pub const SERVICES: &[Service] = &[
    Service {
        name: "NickServ",
        alias: "ns",
        commands: commands![
            "IDENTIFY", "IDENTIFY [compte] <mot de passe>", "S'identifier";
            "REGISTER", "REGISTER <mot de passe> <e-mail>", "Enregistrer son pseudonyme";
            "GROUP", "GROUP", "Ajouter le pseudonyme actuel à son compte";
            "UNGROUP", "UNGROUP [pseudo]", "Retirer un pseudonyme de son compte";
            "REGAIN", "REGAIN <pseudo>", "Récupérer son pseudonyme";
            "GHOST", "GHOST <pseudo>", "Déconnecter une session fantôme";
            "RELEASE", "RELEASE <pseudo>", "Libérer un pseudonyme retenu";
            "INFO", "INFO <pseudo>", "Informations sur un compte";
            "SET", "SET <option> <valeur>", "Modifier les réglages du compte";
            "CERT", "CERT ADD|DEL|LIST [empreinte]", "Gérer les certificats TLS";
            "VERIFY", "VERIFY REGISTER <compte> <code>", "Valider l'inscription";
            "LOGOUT", "LOGOUT", "Se déconnecter du compte";
            "DROP", "DROP <compte> <mot de passe>", "Supprimer le compte";
            "HELP", "HELP [commande]", "Aide du service";
        ],
    },
    Service {
        name: "ChanServ",
        alias: "cs",
        commands: commands![
            "REGISTER", "REGISTER <#salon>", "Enregistrer un salon";
            "INFO", "INFO <#salon>", "Informations sur un salon";
            "OP", "OP <#salon> [pseudo]", "Donner le statut d'opérateur";
            "DEOP", "DEOP <#salon> [pseudo]", "Retirer le statut d'opérateur";
            "VOICE", "VOICE <#salon> [pseudo]", "Donner la parole";
            "DEVOICE", "DEVOICE <#salon> [pseudo]", "Retirer la parole";
            "FLAGS", "FLAGS <#salon> [pseudo] [droits]", "Gérer les droits d'accès";
            "ACCESS", "ACCESS <#salon> ADD|DEL|LIST", "Gérer la liste d'accès";
            "INVITE", "INVITE <#salon>", "Se faire inviter";
            "UNBAN", "UNBAN <#salon>", "Lever ses bannissements";
            "TOPIC", "TOPIC <#salon> <sujet>", "Changer le sujet";
            "KICK", "KICK <#salon> <pseudo> [raison]", "Expulser quelqu'un";
            "AKICK", "AKICK <#salon> ADD|DEL|LIST", "Expulsions automatiques";
            "SET", "SET <#salon> <option> <valeur>", "Modifier les réglages du salon";
            "DROP", "DROP <#salon>", "Supprimer l'enregistrement";
            "HELP", "HELP [commande]", "Aide du service";
        ],
    },
    Service {
        name: "MemoServ",
        alias: "ms",
        commands: commands![
            "SEND", "SEND <pseudo> <texte>", "Envoyer un mémo";
            "LIST", "LIST", "Lister ses mémos";
            "READ", "READ <numéro|NEW>", "Lire un mémo";
            "DEL", "DEL <numéro|ALL>", "Supprimer des mémos";
            "HELP", "HELP [commande]", "Aide du service";
        ],
    },
    Service {
        name: "HostServ",
        alias: "hs",
        commands: commands![
            "REQUEST", "REQUEST <vhost>", "Demander un vhost";
            "ON", "ON", "Activer son vhost";
            "OFF", "OFF", "Désactiver son vhost";
            "HELP", "HELP [commande]", "Aide du service";
        ],
    },
];

/// Service désigné par son nom ou par son alias de commande.
pub fn service(name: &str) -> Option<&'static Service> {
    SERVICES
        .iter()
        .find(|service| service.name.eq_ignore_ascii_case(name) || service.alias.eq_ignore_ascii_case(name))
}

/// Suggestions pour la saisie `input` (curseur en fin de ligne).
pub fn suggest(input: &str, ctx: &Context<'_>) -> Vec<Suggestion> {
    if !ctx.remote.is_empty() {
        return ctx
            .remote
            .iter()
            .map(|&(bot, text)| Suggestion {
                text: text.to_owned(),
                label: text.rsplit(' ').next().unwrap_or(text).to_owned(),
                detail: String::new(),
                source: bot.to_owned(),
            })
            .take(MAX)
            .collect();
    }

    let word_start = input.rfind(' ').map_or(0, |index| index + 1);
    let (head, word) = input.split_at(word_start);
    let mut suggestions = if let Some(service) = service_context(head, ctx) {
        service_commands(service, head, word)
    } else if head.is_empty() && word.starts_with('/') && !word.starts_with("//") {
        commands(&word[1..])
    } else if word.starts_with(['#', '&']) {
        matching(&ctx.channels, word)
            .into_iter()
            .map(|channel| suggestion(format!("{head}{channel} "), channel, "salon"))
            .collect()
    } else if head.is_empty() && word.starts_with('!') {
        matching(&ctx.bot_commands, word)
            .into_iter()
            .map(|command| suggestion(format!("{command} "), command, "bot"))
            .collect()
    } else if let Some(nick) = word.strip_prefix('@').filter(|nick| !nick.is_empty()) {
        let suffix = if head.is_empty() { ": " } else { " " };
        matching(&ctx.nicks, nick)
            .into_iter()
            .map(|found| suggestion(format!("{head}{found}{suffix}"), found, "pseudo"))
            .collect()
    } else {
        Vec::new()
    };
    suggestions.truncate(MAX);
    // Rien à proposer si la saisie est déjà exactement la seule suggestion.
    if let [only] = suggestions.as_slice()
        && only.text.trim_end() == input.trim_end()
    {
        suggestions.clear();
    }
    suggestions
}

/// Service ciblé quand on tape sa sous-commande : `/ns `, `/msg NickServ `
/// ou conversation privée avec le service.
fn service_context(head: &str, ctx: &Context<'_>) -> Option<&'static Service> {
    let tokens: Vec<&str> = head.split_whitespace().collect();
    match tokens.as_slice() {
        [] if head.is_empty() => ctx.query.and_then(service),
        [command] => command.strip_prefix('/').and_then(service),
        [command, target] if ["/msg", "/query", "/q"].contains(&command.to_ascii_lowercase().as_str()) => {
            service(target)
        }
        _ => None,
    }
}

fn service_commands(service: &Service, head: &str, word: &str) -> Vec<Suggestion> {
    service
        .commands
        .iter()
        .filter(|command| starts_with_ignore_case(command.name, word))
        .map(|command| Suggestion {
            text: format!("{head}{} ", command.name),
            label: command.usage.to_owned(),
            detail: command.help.to_owned(),
            source: service.name.to_owned(),
        })
        .collect()
}

fn commands(prefix: &str) -> Vec<Suggestion> {
    COMMANDS
        .iter()
        .filter_map(|spec| {
            let name = spec.names.iter().find(|name| starts_with_ignore_case(name, prefix))?;
            Some(Suggestion {
                text: format!("/{name} "),
                label: spec.usage.to_owned(),
                detail: spec.help.to_owned(),
                source: "commande".to_owned(),
            })
        })
        .collect()
}

/// Candidats commençant par `prefix`, sans doublons (casse ignorée).
fn matching<'a>(candidates: &[&'a str], prefix: &str) -> Vec<&'a str> {
    let mut found: Vec<&str> = Vec::new();
    for &candidate in candidates {
        if starts_with_ignore_case(candidate, prefix) && !found.iter().any(|known| known.eq_ignore_ascii_case(candidate)) {
            found.push(candidate);
        }
    }
    found
}

fn suggestion(text: String, label: &str, source: &str) -> Suggestion {
    Suggestion {
        text,
        label: label.to_owned(),
        detail: String::new(),
        source: source.to_owned(),
    }
}

fn starts_with_ignore_case(value: &str, prefix: &str) -> bool {
    value.to_lowercase().starts_with(&prefix.to_lowercase())
}

/// Extrait une commande de bot (`!meteo`) d'un mot, ponctuation finale ôtée.
pub fn bot_command(word: &str) -> Option<&str> {
    let word = word.trim_end_matches([',', '.', ':', ';', ')', ']', '\'', '"']);
    let name = word.strip_prefix('!')?;
    let valid = (1..=32).contains(&name.len())
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        && name.bytes().any(|b| b.is_ascii_alphabetic());
    valid.then_some(word)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: [&str; 0] = [];

    fn texts(input: &str, ctx: &Context<'_>) -> Vec<String> {
        suggest(input, ctx).into_iter().map(|s| s.text).collect()
    }

    #[test]
    fn suggests_client_commands_with_help() {
        let found = suggest("/jo", &Context::default());
        assert_eq!(found[0].text, "/join ");
        assert_eq!(found[0].label, "/join #salon [clé]");
        assert_eq!(texts("/", &Context::default()).len(), MAX);
        assert_eq!(texts("//jo", &Context::default()), NONE);
    }

    #[test]
    fn suggests_service_commands_in_every_form() {
        let ctx = Context::default();
        assert_eq!(texts("/ns ide", &ctx), ["/ns IDENTIFY "]);
        assert_eq!(texts("/msg chanserv unb", &ctx), ["/msg chanserv UNBAN "]);
        assert_eq!(suggest("/ms ", &ctx).len(), 5);
        let query = Context { query: Some("NickServ"), ..Context::default() };
        assert_eq!(texts("reg", &query), ["REGISTER ", "REGAIN "]);
        assert_eq!(texts("/ns IDENTIFY mot", &ctx), NONE);
    }

    #[test]
    fn suggests_channels_nicks_and_bot_commands() {
        let ctx = Context {
            channels: vec!["#rust", "#ruby", "#go"],
            nicks: vec!["alice", "Alan", "alice"],
            bot_commands: vec!["!meteo", "!help"],
            ..Context::default()
        };
        assert_eq!(texts("/join #ru", &ctx), ["/join #rust ", "/join #ruby "]);
        assert_eq!(texts("@al", &ctx), ["alice: ", "Alan: "]);
        assert_eq!(texts("merci @ali", &ctx), ["merci alice "]);
        assert_eq!(texts("!", &ctx), ["!meteo ", "!help "]);
        assert!(texts("bonjour !he", &ctx).is_empty(), "uniquement en début de ligne");
        assert!(texts("al", &ctx).is_empty(), "pas de liste sans @");
    }

    #[test]
    fn remote_completions_take_precedence() {
        let ctx = Context { remote: vec![("limnoria", "!weather Paris")], ..Context::default() };
        let found = suggest("!weather Par", &ctx);
        assert_eq!(found[0].text, "!weather Paris");
        assert_eq!(found[0].label, "Paris");
        assert_eq!(found[0].source, "limnoria");
    }

    #[test]
    fn hides_an_already_complete_entry() {
        assert_eq!(texts("/join", &Context::default()), NONE);
    }

    #[test]
    fn extracts_bot_commands() {
        assert_eq!(bot_command("!meteo"), Some("!meteo"));
        assert_eq!(bot_command("!help,"), Some("!help"));
        assert_eq!(bot_command("!"), None);
        assert_eq!(bot_command("!!!"), None);
        assert_eq!(bot_command("!42"), None);
        assert_eq!(bot_command("bonjour"), None);
    }
}
