//! Paramètres annoncés par le serveur via `RPL_ISUPPORT` (005).
//!
//! Référence : <https://modern.ircdocs.horse/#rplisupport-parameters>

use crate::casemap::CaseMapping;

/// Catégorie d'un mode de salon, qui détermine s'il prend un argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeKind {
    /// Liste (`b`, `e`, `I`) : argument à l'ajout comme au retrait.
    List,
    /// Toujours un argument (`k`).
    AlwaysArg,
    /// Argument uniquement à l'ajout (`l`).
    ArgOnSet,
    /// Jamais d'argument (`m`, `n`, `t`...).
    NoArg,
    /// Préfixe de membre (`o`, `v`...) : argument = pseudonyme.
    Prefix,
}

/// État `ISUPPORT` d'une connexion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ISupport {
    pub network: Option<String>,
    pub casemapping: CaseMapping,
    /// Préfixes de salon (`#`, `&`...).
    pub chantypes: String,
    /// Paires (mode, symbole) triées du plus élevé au plus bas : `[('o', '@'), ('v', '+')]`.
    pub prefix: Vec<(char, char)>,
    /// Groupes A, B, C, D de `CHANMODES`.
    pub chanmodes: [String; 4],
    /// Symboles autorisés devant une cible de message (`@#salon`).
    pub statusmsg: String,
    /// Nombre maximal de messages par requête `CHATHISTORY` (`Some(0)` = illimité).
    pub chathistory: Option<usize>,
    /// Taille maximale de la liste `MONITOR` (`Some(usize::MAX)` = illimitée).
    pub monitor: Option<usize>,
    /// Mode utilisateur marquant un bot (`BOT=B`).
    pub bot_mode: Option<char>,
    /// Le serveur n'accepte que de l'UTF-8.
    pub utf8_only: bool,
    /// Le serveur prend en charge `WHO` étendu (WHOX).
    pub whox: bool,
}

impl Default for ISupport {
    fn default() -> Self {
        Self {
            network: None,
            casemapping: CaseMapping::default(),
            chantypes: "#&".to_owned(),
            prefix: vec![('o', '@'), ('v', '+')],
            chanmodes: ["beI", "k", "l", "imnpst"].map(str::to_owned),
            statusmsg: String::new(),
            chathistory: None,
            monitor: None,
            bot_mode: None,
            utf8_only: false,
            whox: false,
        }
    }
}

impl ISupport {
    /// Applique les jetons d'un message 005 (sans le pseudonyme initial ni le
    /// texte final « are supported by this server »).
    pub fn apply<'a>(&mut self, tokens: impl IntoIterator<Item = &'a str>) {
        let defaults = Self::default();
        for token in tokens {
            if let Some(key) = token.strip_prefix('-') {
                self.reset(key, &defaults);
                continue;
            }
            let (key, value) = token.split_once('=').unwrap_or((token, ""));
            match key {
                "NETWORK" if !value.is_empty() => self.network = Some(value.to_owned()),
                "CASEMAPPING" => {
                    if let Some(mapping) = CaseMapping::parse(value) {
                        self.casemapping = mapping;
                    }
                }
                "CHANTYPES" => value.clone_into(&mut self.chantypes),
                "PREFIX" => {
                    if let Some(prefix) = parse_prefix(value) {
                        self.prefix = prefix;
                    }
                }
                "CHANMODES" => {
                    let mut groups = value.split(',').map(str::to_owned);
                    for slot in &mut self.chanmodes {
                        *slot = groups.next().unwrap_or_default();
                    }
                }
                "STATUSMSG" => value.clone_into(&mut self.statusmsg),
                "CHATHISTORY" => self.chathistory = Some(value.parse().unwrap_or(0)),
                "MONITOR" => self.monitor = Some(value.parse().unwrap_or(usize::MAX)),
                "BOT" => self.bot_mode = value.chars().next(),
                "UTF8ONLY" => self.utf8_only = true,
                "WHOX" => self.whox = true,
                _ => {}
            }
        }
    }

    fn reset(&mut self, key: &str, defaults: &Self) {
        match key {
            "NETWORK" => self.network = None,
            "CASEMAPPING" => self.casemapping = defaults.casemapping,
            "CHANTYPES" => self.chantypes.clone_from(&defaults.chantypes),
            "PREFIX" => self.prefix.clone_from(&defaults.prefix),
            "CHANMODES" => self.chanmodes.clone_from(&defaults.chanmodes),
            "STATUSMSG" => self.statusmsg.clear(),
            "CHATHISTORY" => self.chathistory = None,
            "MONITOR" => self.monitor = None,
            "BOT" => self.bot_mode = None,
            "UTF8ONLY" => self.utf8_only = false,
            "WHOX" => self.whox = false,
            _ => {}
        }
    }

    /// Indique si `target` désigne un salon.
    pub fn is_channel(&self, target: &str) -> bool {
        target.chars().next().is_some_and(|c| self.chantypes.contains(c))
    }

    /// Retire un éventuel préfixe `STATUSMSG` (`@#salon` → `#salon`).
    pub fn strip_statusmsg<'a>(&self, target: &'a str) -> &'a str {
        target.trim_start_matches(|c| self.statusmsg.contains(c))
    }

    /// Rang d'un symbole de préfixe (0 = le plus élevé).
    pub fn prefix_rank(&self, symbol: char) -> Option<usize> {
        self.prefix.iter().position(|&(_, s)| s == symbol)
    }

    /// Symbole associé à un mode de préfixe (`o` → `@`).
    pub fn prefix_symbol(&self, mode: char) -> Option<char> {
        self.prefix.iter().find(|&&(m, _)| m == mode).map(|&(_, s)| s)
    }

    pub fn is_prefix_symbol(&self, c: char) -> bool {
        self.prefix_rank(c).is_some()
    }

    pub fn mode_kind(&self, mode: char) -> ModeKind {
        if self.prefix_symbol(mode).is_some() {
            return ModeKind::Prefix;
        }
        let [a, b, c, _] = &self.chanmodes;
        if a.contains(mode) {
            ModeKind::List
        } else if b.contains(mode) {
            ModeKind::AlwaysArg
        } else if c.contains(mode) {
            ModeKind::ArgOnSet
        } else {
            ModeKind::NoArg
        }
    }

    pub fn fold(&self, name: &str) -> String {
        self.casemapping.fold(name)
    }
}

fn parse_prefix(value: &str) -> Option<Vec<(char, char)>> {
    if value.is_empty() {
        return Some(Vec::new());
    }
    let (modes, symbols) = value.strip_prefix('(')?.split_once(')')?;
    if modes.chars().count() != symbols.chars().count() {
        return None;
    }
    Some(modes.chars().zip(symbols.chars()).collect())
}

/// Un changement de mode de salon, une fois les arguments associés.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeChange {
    pub adding: bool,
    pub mode: char,
    pub arg: Option<String>,
}

/// Associe chaque mode d'une chaîne `+ov-b` à son argument éventuel.
pub fn parse_channel_modes(isupport: &ISupport, modes: &str, args: &[String]) -> Vec<ModeChange> {
    let mut args = args.iter();
    let mut adding = true;
    let mut changes = Vec::new();
    for mode in modes.chars() {
        match mode {
            '+' => adding = true,
            '-' => adding = false,
            mode => {
                let takes_arg = match isupport.mode_kind(mode) {
                    ModeKind::List | ModeKind::AlwaysArg | ModeKind::Prefix => true,
                    ModeKind::ArgOnSet => adding,
                    ModeKind::NoArg => false,
                };
                let arg = if takes_arg { args.next().cloned() } else { None };
                changes.push(ModeChange { adding, mode, arg });
            }
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn libera() -> ISupport {
        let mut isupport = ISupport::default();
        isupport.apply([
            "CHANTYPES=#",
            "PREFIX=(qaohv)~&@%+",
            "CHANMODES=eIbq,k,flj,CFLMPQScgimnprstuz",
            "CASEMAPPING=ascii",
            "NETWORK=Libera.Chat",
            "STATUSMSG=@+",
            "MONITOR=100",
            "WHOX",
            "BOT=B",
            "UTF8ONLY",
            "CHATHISTORY=1000",
        ]);
        isupport
    }

    #[test]
    fn applies_tokens() {
        let isupport = libera();
        assert_eq!(isupport.network.as_deref(), Some("Libera.Chat"));
        assert_eq!(isupport.casemapping, CaseMapping::Ascii);
        assert!(isupport.is_channel("#rust"));
        assert!(!isupport.is_channel("&local"));
        assert_eq!(isupport.prefix_rank('@'), Some(2));
        assert_eq!(isupport.prefix_symbol('h'), Some('%'));
        assert_eq!(isupport.strip_statusmsg("@#rust"), "#rust");
        assert_eq!(isupport.monitor, Some(100));
        assert_eq!(isupport.chathistory, Some(1000));
        assert_eq!(isupport.bot_mode, Some('B'));
        assert!(isupport.whox && isupport.utf8_only);
    }

    #[test]
    fn negated_token_restores_default() {
        let mut isupport = libera();
        isupport.apply(["-NETWORK", "-PREFIX"]);
        assert_eq!(isupport.network, None);
        assert_eq!(isupport.prefix, ISupport::default().prefix);
    }

    #[test]
    fn ignores_malformed_prefix() {
        let mut isupport = ISupport::default();
        isupport.apply(["PREFIX=(ov)@"]);
        assert_eq!(isupport.prefix, ISupport::default().prefix);
    }

    #[test]
    fn pairs_modes_with_arguments() {
        let isupport = libera();
        let args: Vec<String> = ["alice", "*!*@spam", "10", "bob"].map(String::from).into();
        let changes = parse_channel_modes(&isupport, "+obl-v+n", &args);
        let summary: Vec<_> = changes
            .iter()
            .map(|c| (c.adding, c.mode, c.arg.as_deref()))
            .collect();
        assert_eq!(
            summary,
            [
                (true, 'o', Some("alice")),
                (true, 'b', Some("*!*@spam")),
                (true, 'l', Some("10")),
                (false, 'v', Some("bob")),
                (true, 'n', None),
            ]
        );
    }
}
