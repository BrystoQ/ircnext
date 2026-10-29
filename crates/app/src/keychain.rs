//! Mots de passe dans le trousseau du système (Trousseau d'accès sur macOS,
//! Gestionnaire d'identifiants sur Windows, Secret Service sous Linux).
//!
//! En mémoire, la configuration garde les mots de passe. À l'écriture, ils
//! partent dans le trousseau et le fichier ne contient plus qu'un champ vide ;
//! au chargement, chaque champ vide est complété depuis le trousseau. Si le
//! trousseau est indisponible, le mot de passe reste dans le fichier.

use std::collections::BTreeMap;

use ircnext_client::Secret;

use crate::config::AppConfig;

/// Nom du service sous lequel les entrées sont rangées.
const SERVICE: &str = "ircnext";

/// Accès à un trousseau.
pub trait Store {
    /// Mot de passe d'un compte, `None` s'il n'existe pas.
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    fn set(&self, account: &str, secret: &str) -> Result<(), String>;
    fn delete(&self, account: &str) -> Result<(), String>;
}

/// Trousseau du système.
pub struct System;

impl System {
    fn entry(account: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(SERVICE, account).map_err(|error| error.to_string())
    }
}

impl Store for System {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        match Self::entry(account)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), String> {
        Self::entry(account)?.set_password(secret).map_err(|error| error.to_string())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

/// Mots de passe déjà présents dans le trousseau, par compte : seuls ceux
/// qui changent y sont réécrits (sur macOS, chaque écriture peut demander
/// une autorisation).
pub type Cache = BTreeMap<String, String>;

fn server_account(name: &str) -> String {
    format!("{name} — mot de passe du serveur")
}

fn sasl_account(name: &str) -> String {
    format!("{name} — SASL")
}

/// Range les mots de passe de `config` dans le trousseau et les vide, pour
/// obtenir la version à écrire sur le disque. Les comptes des réseaux
/// supprimés ou renommés sont effacés du trousseau.
///
/// Renvoie la première erreur rencontrée ; le mot de passe concerné reste
/// alors dans le fichier.
pub fn stash(config: &mut AppConfig, store: &dyn Store, cache: &mut Cache) -> Option<String> {
    let mut error = None;
    let mut wanted = Cache::new();
    for server in &mut config.servers {
        let connection = &mut server.connection;
        let secrets = [
            (server_account(&server.name), connection.password.as_mut()),
            (sasl_account(&server.name), connection.sasl.as_mut().map(|sasl| &mut sasl.password)),
        ];
        for (account, secret) in secrets {
            let Some(secret) = secret.filter(|secret| !secret.expose().is_empty()) else {
                continue;
            };
            let value = secret.expose().to_owned();
            let stored = if cache.get(&account) == Some(&value) {
                Ok(())
            } else {
                store.set(&account, &value)
            };
            match stored {
                Ok(()) => {
                    *secret = Secret::default();
                    wanted.insert(account, value);
                }
                Err(message) => {
                    error.get_or_insert(message);
                }
            }
        }
    }
    forget(store, cache, &wanted, &mut error);
    *cache = wanted;
    error
}

/// Efface du trousseau tous les comptes connus (trousseau désactivé).
pub fn clear(store: &dyn Store, cache: &mut Cache) -> Option<String> {
    let mut error = None;
    forget(store, cache, &Cache::new(), &mut error);
    cache.clear();
    error
}

fn forget(store: &dyn Store, cache: &Cache, keep: &Cache, error: &mut Option<String>) {
    for account in cache.keys().filter(|account| !keep.contains_key(*account)) {
        if let Err(message) = store.delete(account) {
            error.get_or_insert(message);
        }
    }
}

/// Complète depuis le trousseau les mots de passe vides de `config`.
///
/// Renvoie les comptes trouvés (le cache) et la liste des réseaux dont un
/// mot de passe SASL manque.
pub fn restore(config: &mut AppConfig, store: &dyn Store) -> (Cache, Vec<String>) {
    let mut cache = Cache::new();
    let mut missing = Vec::new();
    for server in &mut config.servers {
        let connection = &mut server.connection;
        let secrets = [
            (server_account(&server.name), connection.password.as_mut(), false),
            (sasl_account(&server.name), connection.sasl.as_mut().map(|sasl| &mut sasl.password), true),
        ];
        for (account, secret, required) in secrets {
            let Some(secret) = secret.filter(|secret| secret.expose().is_empty()) else {
                continue;
            };
            match store.get(&account) {
                Ok(Some(value)) => {
                    *secret = Secret::new(value.clone());
                    cache.insert(account, value);
                }
                Ok(None) | Err(_) if required => missing.push(server.name.clone()),
                Ok(None) | Err(_) => {}
            }
        }
        // Un mot de passe serveur introuvable n'est pas envoyé vide.
        if connection.password.as_ref().is_some_and(|password| password.expose().is_empty()) {
            connection.password = None;
        }
    }
    missing.dedup();
    (cache, missing)
}

/// Trousseau en mémoire, pour les tests.
#[cfg(test)]
#[derive(Default)]
pub struct Memory {
    pub entries: std::cell::RefCell<BTreeMap<String, String>>,
    pub writes: std::cell::Cell<usize>,
    pub broken: bool,
}

#[cfg(test)]
impl Store for Memory {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        Ok(self.entries.borrow().get(account).cloned())
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), String> {
        if self.broken {
            return Err("trousseau verrouillé".into());
        }
        self.writes.set(self.writes.get() + 1);
        self.entries.borrow_mut().insert(account.to_owned(), secret.to_owned());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        self.entries.borrow_mut().remove(account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServerEntry;
    use ircnext_client::{Config as Connection, Sasl};

    fn config(name: &str, server_password: &str, sasl_password: &str) -> AppConfig {
        let mut config = AppConfig::default();
        config.servers.push(ServerEntry {
            name: name.into(),
            auto_connect: true,
            on_connect: Vec::new(),
            connection: Connection {
                host: "irc.example.net".into(),
                password: Some(Secret::new(server_password)),
                sasl: Some(Sasl {
                    username: "moi".into(),
                    password: Secret::new(sasl_password),
                }),
                ..Connection::default()
            },
        });
        config
    }

    #[test]
    fn passwords_leave_the_file_and_come_back() {
        let store = Memory::default();
        let mut cache = Cache::new();
        let original = config("Libera", "pass", "sasl");

        let mut on_disk = original.clone();
        assert_eq!(stash(&mut on_disk, &store, &mut cache), None);
        let toml = toml::to_string(&on_disk).unwrap();
        assert!(!toml.contains("pass\"") && !toml.contains("\"sasl\""), "aucun mot de passe dans le fichier :\n{toml}");
        assert_eq!(store.entries.borrow().len(), 2);

        let mut loaded: AppConfig = toml::from_str(&toml).unwrap();
        let (restored_cache, missing) = restore(&mut loaded, &store);
        assert_eq!(loaded, original);
        assert_eq!(restored_cache, cache);
        assert_eq!(missing, Vec::<String>::new());
    }

    #[test]
    fn unchanged_passwords_are_not_rewritten() {
        let store = Memory::default();
        let mut cache = Cache::new();
        stash(&mut config("Libera", "pass", "sasl"), &store, &mut cache);
        stash(&mut config("Libera", "pass", "sasl"), &store, &mut cache);
        assert_eq!(store.writes.get(), 2);
        stash(&mut config("Libera", "pass", "nouveau"), &store, &mut cache);
        assert_eq!(store.writes.get(), 3);
    }

    #[test]
    fn renamed_networks_leave_no_stale_entry() {
        let store = Memory::default();
        let mut cache = Cache::new();
        stash(&mut config("Libera", "pass", "sasl"), &store, &mut cache);
        stash(&mut config("Libera.Chat", "pass", "sasl"), &store, &mut cache);
        let accounts: Vec<String> = store.entries.borrow().keys().cloned().collect();
        assert!(accounts.iter().all(|account| account.starts_with("Libera.Chat ")), "{accounts:?}");

        assert_eq!(clear(&store, &mut cache), None);
        assert!(store.entries.borrow().is_empty());
    }

    #[test]
    fn unavailable_keychain_keeps_passwords_in_the_file() {
        let store = Memory { broken: true, ..Memory::default() };
        let mut cache = Cache::new();
        let mut on_disk = config("Libera", "pass", "sasl");
        assert!(stash(&mut on_disk, &store, &mut cache).is_some());
        assert_eq!(on_disk, config("Libera", "pass", "sasl"));
    }

    #[test]
    fn reports_missing_sasl_passwords() {
        let mut loaded = config("Libera", "", "");
        let (cache, missing) = restore(&mut loaded, &Memory::default());
        assert!(cache.is_empty());
        assert_eq!(missing, ["Libera"]);
    }
}

/// Aller-retour dans le vrai trousseau ; à lancer à la main avec
/// `cargo test -p ircnext -- --ignored system_keychain`.
#[cfg(test)]
#[test]
#[ignore = "accède au trousseau de la machine"]
fn system_keychain_round_trip() {
    let account = "test — aller-retour";
    System.set(account, "secret").unwrap();
    assert_eq!(System.get(account).unwrap().as_deref(), Some("secret"));
    System.delete(account).unwrap();
    assert_eq!(System.get(account).unwrap(), None);
    System.delete(account).unwrap();
}
