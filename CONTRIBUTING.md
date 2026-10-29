# Contribuer à ircnext

Merci de votre intérêt ! Toute contribution est la bienvenue : signalement de bug, idée, documentation ou code.

## Signaler un problème

Ouvrez un [ticket](https://github.com/BrystoQ/ircnext/issues/new/choose) en précisant :

- votre système (macOS, Windows, Linux) et la version d'ircnext ;
- le réseau IRC concerné, si le problème en dépend ;
- les étapes pour reproduire, ce que vous attendiez et ce qui s'est passé.

Pour une faille de sécurité, n'ouvrez **pas** de ticket public : suivez [SECURITY.md](SECURITY.md).

## Proposer une modification

1. Forkez le dépôt et créez une branche à partir de `main`.
2. Faites vos modifications, avec des tests si le comportement change.
3. Vérifiez que tout passe :

   ```sh
   cargo test --workspace
   cargo clippy --workspace --all-targets -- -D warnings
   ```

4. Ouvrez une *pull request* qui explique le **pourquoi** de la modification.

## Conventions

- **Rust stable**, version minimale 1.90, édition 2024.
- **Aucun `unsafe`** : il est interdit dans tout l'espace de travail.
- **Clippy pedantic sans avertissement** ; un `#[allow]` local doit être justifié en commentaire.
- **Logique sans I/O** : le protocole (`ircnext-proto`) et les machines à états (`ircnext-client`) ne touchent pas au réseau, ce qui les rend testables isolément.
- **Langue** : l'interface, les commentaires et la documentation sont en français, avec les accents.
- **Interface** : pour une modification visible, joignez une capture. Le test `cargo test -p ircnext -- --ignored snapshots` génère des captures sans fenêtre dans `target/snapshots/`.

## Organisation du code

| Dossier | Contenu |
|---|---|
| `crates/proto` | Messages IRC, tags IRCv3, mise en forme mIRC, ISUPPORT |
| `crates/client` | Connexion TCP/TLS, enregistrement CAP/SASL/STS, état du réseau |
| `crates/app` | Interface iced, réglages, raccourcis, notifications |

En contribuant, vous acceptez que votre contribution soit distribuée sous [licence MIT](LICENSE).
