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

4. Add a line to the patch notes, under *Unreleased*: [`CHANGELOG.md`](CHANGELOG.md) in English and [`CHANGELOG.fr.md`](CHANGELOG.fr.md) in French. Write it for users: what changes for them, not how it is coded.
5. Open a pull request explaining **why** the change is needed.

## Releasing a version

1. In both changelogs, rename the *Unreleased* section to `[X.Y.Z] - YYYY-MM-DD` and add a new empty *Unreleased* section.
2. Set `version = "X.Y.Z"` in `Cargo.toml` and run `cargo check` to update `Cargo.lock`.
3. Merge, then push the tag: `git tag vX.Y.Z && git push origin vX.Y.Z`.

The release workflow builds the executables and publishes that version's patch notes, in English and French, as the release notes. It fails if the notes are missing.

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

## Logo

The logo is drawn in [`assets/icon/ircnext.svg`](assets/icon/ircnext.svg). After changing it, regenerate the icons (PNG sizes, Windows `.ico`, window icon) and the macOS `.icns`:

```sh
cargo run --manifest-path tools/icons/Cargo.toml
iconutil -c icns assets/icon/ircnext.iconset -o assets/icon/ircnext.icns   # macOS only
```

## Code layout

| Folder | Content |
|---|---|
| `crates/proto` | IRC messages, IRCv3 tags, mIRC formatting, ISUPPORT |
| `crates/client` | TCP/TLS connection, CAP/SASL/STS registration, network state |
| `crates/app` | iced interface, settings, shortcuts, notifications, translations |

By contributing, you agree that your contribution is distributed under the [MIT license](LICENSE).
