//! Suggestions shown while typing: commands, service commands (NickServ,
//! ChanServ...), channels, nicknames (`@`) and bot commands (`!`), learned in
//! the channel or offered by the bot itself (IRCv3 `autocomplete` draft).

use crate::i18n::tr;
use crate::input;

/// Maximum number of suggestions shown.
pub const MAX: usize = 8;

/// Suggestion offered below the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// Full input line once the suggestion is accepted.
    pub text: String,
    pub label: String,
    pub detail: String,
    /// Origin (“command”, “NickServ”, bot name...).
    pub source: String,
}

/// Data available for suggesting.
#[derive(Debug, Default)]
pub struct Context<'a> {
    /// Name of the tab if it is a private conversation.
    pub query: Option<&'a str>,
    pub channels: Vec<&'a str>,
    /// Nicknames, recent speakers first.
    pub nicks: Vec<&'a str>,
    /// Bot commands seen in the tab, most recent first.
    pub bot_commands: Vec<&'a str>,
    /// Completions offered by bots for the current input: (bot, text).
    pub remote: Vec<(&'a str, &'a str)>,
}

/// Command of a service (usage and help in English, translated on display).
#[derive(Debug)]
pub struct ServiceCommand {
    pub name: &'static str,
    pub usage: &'static str,
    pub help: &'static str,
}

/// Network service and its usual commands (shared by Atheme and Anope).
#[derive(Debug)]
pub struct Service {
    pub name: &'static str,
    /// Short command (`ns` for `/ns`).
    pub alias: &'static str,
    pub commands: &'static [ServiceCommand],
}

// `tr(...)` marks the texts for the translation check; they are translated
// on display.
macro_rules! commands {
    ($($name:literal, tr($usage:literal), tr($help:literal);)*) => {
        &[$(ServiceCommand { name: $name, usage: $usage, help: $help }),*]
    };
}

pub const SERVICES: &[Service] = &[
    Service {
        name: "NickServ",
        alias: "ns",
        commands: commands![
            "IDENTIFY", tr("IDENTIFY [account] <password>"), tr("Identify yourself");
            "REGISTER", tr("REGISTER <password> <email>"), tr("Register your nickname");
            "GROUP", tr("GROUP"), tr("Add the current nickname to your account");
            "UNGROUP", tr("UNGROUP [nick]"), tr("Remove a nickname from your account");
            "REGAIN", tr("REGAIN <nick>"), tr("Get your nickname back");
            "GHOST", tr("GHOST <nick>"), tr("Disconnect a ghost session");
            "RELEASE", tr("RELEASE <nick>"), tr("Release a held nickname");
            "INFO", tr("INFO <nick>"), tr("Information about an account");
            "SET", tr("SET <option> <value>"), tr("Change the account settings");
            "CERT", tr("CERT ADD|DEL|LIST [fingerprint]"), tr("Manage TLS certificates");
            "VERIFY", tr("VERIFY REGISTER <account> <code>"), tr("Confirm the registration");
            "LOGOUT", tr("LOGOUT"), tr("Log out of the account");
            "DROP", tr("DROP <account> <password>"), tr("Delete the account");
            "HELP", tr("HELP [command]"), tr("Service help");
        ],
    },
    Service {
        name: "ChanServ",
        alias: "cs",
        commands: commands![
            "REGISTER", tr("REGISTER <#channel>"), tr("Register a channel");
            "INFO", tr("INFO <#channel>"), tr("Information about a channel");
            "OP", tr("OP <#channel> [nick]"), tr("Give operator status");
            "DEOP", tr("DEOP <#channel> [nick]"), tr("Remove operator status");
            "VOICE", tr("VOICE <#channel> [nick]"), tr("Give voice");
            "DEVOICE", tr("DEVOICE <#channel> [nick]"), tr("Remove voice");
            "FLAGS", tr("FLAGS <#channel> [nick] [flags]"), tr("Manage access rights");
            "ACCESS", tr("ACCESS <#channel> ADD|DEL|LIST"), tr("Manage the access list");
            "INVITE", tr("INVITE <#channel>"), tr("Get invited");
            "UNBAN", tr("UNBAN <#channel>"), tr("Lift your bans");
            "TOPIC", tr("TOPIC <#channel> <topic>"), tr("Change the topic");
            "KICK", tr("KICK <#channel> <nick> [reason]"), tr("Kick someone");
            "AKICK", tr("AKICK <#channel> ADD|DEL|LIST"), tr("Automatic kicks");
            "SET", tr("SET <#channel> <option> <value>"), tr("Change the channel settings");
            "DROP", tr("DROP <#channel>"), tr("Delete the registration");
            "HELP", tr("HELP [command]"), tr("Service help");
        ],
    },
    Service {
        name: "MemoServ",
        alias: "ms",
        commands: commands![
            "SEND", tr("SEND <nick> <text>"), tr("Send a memo");
            "LIST", tr("LIST"), tr("List your memos");
            "READ", tr("READ <number|NEW>"), tr("Read a memo");
            "DEL", tr("DEL <number|ALL>"), tr("Delete memos");
            "HELP", tr("HELP [command]"), tr("Service help");
        ],
    },
    Service {
        name: "HostServ",
        alias: "hs",
        commands: commands![
            "REQUEST", tr("REQUEST <vhost>"), tr("Request a vhost");
            "ON", tr("ON"), tr("Turn your vhost on");
            "OFF", tr("OFF"), tr("Turn your vhost off");
            "HELP", tr("HELP [command]"), tr("Service help");
        ],
    },
];

/// Service named by its name or by its command alias.
pub fn service(name: &str) -> Option<&'static Service> {
    SERVICES
        .iter()
        .find(|service| service.name.eq_ignore_ascii_case(name) || service.alias.eq_ignore_ascii_case(name))
}

/// Suggestions for the input `input` (cursor at the end of the line).
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
        client_commands(&word[1..])
    } else if word.starts_with(['#', '&']) {
        matching(&ctx.channels, word)
            .into_iter()
            .map(|channel| suggestion(format!("{head}{channel} "), channel, tr("channel")))
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
            .map(|found| suggestion(format!("{head}{found}{suffix}"), found, tr("nick")))
            .collect()
    } else {
        Vec::new()
    };
    suggestions.truncate(MAX);
    // Nothing to offer if the input already is exactly the only suggestion.
    if let [only] = suggestions.as_slice()
        && only.text.trim_end() == input.trim_end()
    {
        suggestions.clear();
    }
    suggestions
}

/// Service targeted while typing its subcommand: `/ns `, `/msg NickServ ` or
/// private conversation with the service.
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
            label: tr(command.usage).to_owned(),
            detail: tr(command.help).to_owned(),
            source: service.name.to_owned(),
        })
        .collect()
}

fn client_commands(prefix: &str) -> Vec<Suggestion> {
    input::commands()
        .iter()
        .filter_map(|spec| {
            let name = spec.names.iter().find(|name| starts_with_ignore_case(name, prefix))?;
            Some(Suggestion {
                text: format!("/{name} "),
                label: spec.usage.to_owned(),
                detail: spec.help.to_owned(),
                source: tr("command").to_owned(),
            })
        })
        .collect()
}

/// Candidates starting with `prefix`, without duplicates (case-insensitive).
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

/// Extracts a bot command (`!weather`) from a word, trailing punctuation removed.
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
        assert_eq!(found[0].label, "/join #channel [key]");
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
        assert_eq!(texts("/ns IDENTIFY word", &ctx), NONE);
    }

    #[test]
    fn suggests_channels_nicks_and_bot_commands() {
        let ctx = Context {
            channels: vec!["#rust", "#ruby", "#go"],
            nicks: vec!["alice", "Alan", "alice"],
            bot_commands: vec!["!weather", "!help"],
            ..Context::default()
        };
        assert_eq!(texts("/join #ru", &ctx), ["/join #rust ", "/join #ruby "]);
        assert_eq!(texts("@al", &ctx), ["alice: ", "Alan: "]);
        assert_eq!(texts("thanks @ali", &ctx), ["thanks alice "]);
        assert_eq!(texts("!", &ctx), ["!weather ", "!help "]);
        assert!(texts("hello !he", &ctx).is_empty(), "only at the start of the line");
        assert!(texts("al", &ctx).is_empty(), "no list without @");
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
        assert_eq!(bot_command("!weather"), Some("!weather"));
        assert_eq!(bot_command("!help,"), Some("!help"));
        assert_eq!(bot_command("!"), None);
        assert_eq!(bot_command("!!!"), None);
        assert_eq!(bot_command("!42"), None);
        assert_eq!(bot_command("hello"), None);
    }
}
