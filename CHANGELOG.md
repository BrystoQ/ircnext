# Changelog

This project follows [semantic versioning](https://semver.org). The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- Client certificate identification: SASL `EXTERNAL` and CertFP. The network form can create a self-signed certificate, stored in `certs/` next to the configuration.
- English interface with a French translation; new *Appearance → Language* setting (system language by default).

### Security

- Passwords (server and SASL) stored in the system keychain: Keychain on macOS, Credential Manager on Windows, Secret Service on Linux. Passwords already in the file are moved there on the next save; setting in *Privacy*.

### Changed

- Two networks can no longer have the same name.
- Code, comments and documentation are in English; the README is also available in French.

## [0.1.0] - 2026-10-29

### Added

- Multi-network IRC client with a native interface (iced), inspired by HexChat.
- TLS by default, SASL PLAIN, STS, automatic reconnection, alternative nicknames.
- More than 25 IRCv3 extensions: history (`chathistory`), read markers, typing indicator, message deletion, `MONITOR`, `WHOX`…
- 7 themes, accent color, text size, adjustable nickname column.
- Suggestions as you type: commands, channels, nicknames, services (NickServ, ChanServ…) and bot commands.
- 37 rebindable keyboard shortcuts, adapted to macOS and Windows.
- Quick tab switcher and search in the history.
- System notifications, Dock bounce, counter in the title.
- Logs on disk, ignored users (`/ignore`), commands run after connecting.
- Privacy settings: typing indicator, read markers, CTCP replies.

### Security

- SASL refused without TLS, STS only remembered with a verified certificate.
- Private configuration and log files (`0600`), atomic writes.
- Filtered links (`http`/`https`) and bidirectional characters removed from display.
- Limited send rate and CTCP replies, bounded memory against a hostile server.
- NickServ passwords masked on display, in logs and in the input history.

[Unreleased]: https://github.com/BrystoQ/ircnext/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/BrystoQ/ircnext/releases/tag/v0.1.0
