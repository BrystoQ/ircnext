# Contributing to ircnext

Thanks for your interest! Every contribution is welcome: bug reports, ideas, documentation, translations or code. Issues and pull requests can be written in English or French.

## Reporting a problem

Open an [issue](https://github.com/BrystoQ/ircnext/issues/new/choose) with:

- your system (macOS, Windows, Linux) and the ircnext version;
- the IRC network involved, if the problem depends on it;
- the steps to reproduce, what you expected and what happened.

For a security vulnerability, do **not** open a public issue: follow [SECURITY.md](SECURITY.md).

## Proposing a change

1. Fork the repository and create a branch from `main`.
2. Make your changes, with tests when the behavior changes.
3. Check that everything passes:

   ```sh
   cargo test --workspace
   cargo clippy --workspace --all-targets -- -D warnings
   ```

4. Open a pull request explaining **why** the change is needed.

## Conventions

- **Stable Rust**, minimum version 1.90, 2024 edition.
- **No `unsafe`**: it is forbidden in the whole workspace.
- **Clippy pedantic without warnings**; a local `#[allow]` must be justified in a comment.
- **Logic without I/O**: the protocol (`ircnext-proto`) and the state machines (`ircnext-client`) do not touch the network, which makes them testable on their own.
- **Language**: code, comments and commit messages are in English.
- **Interface**: for a visible change, attach a screenshot. `cargo test -p ircnext -- --ignored snapshots` renders headless screenshots in English and French into `target/snapshots/`.

## Translations

Interface texts are written in English and wrapped in `tr("…")`, or `tr!("… {name}", name = value)` when they contain values. Translations live in [`crates/app/locales/`](crates/app/locales), keyed by the English text:

```toml
"Connected as {nick}. IRCv3 capabilities: {caps}." = "Connecté en tant que {nick}. Capacités IRCv3 : {caps}."
```

When you add or change a text, add its French entry to `fr.toml`. `cargo test` fails if a text has no translation, if a translation is no longer used, or if the `{placeholders}` differ.

To add a language, create `locales/<code>.toml`, add a variant to `Language` in `crates/app/src/i18n.rs`, and open a pull request.

## Code layout

| Folder | Content |
|---|---|
| `crates/proto` | IRC messages, IRCv3 tags, mIRC formatting, ISUPPORT |
| `crates/client` | TCP/TLS connection, CAP/SASL/STS registration, network state |
| `crates/app` | iced interface, settings, shortcuts, notifications, translations |

By contributing, you agree that your contribution is distributed under the [MIT license](LICENSE).
