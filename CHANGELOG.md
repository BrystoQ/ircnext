# Changelog

**English** · [Français](CHANGELOG.fr.md)

This project follows [semantic versioning](https://semver.org). The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- Installable and portable packages: a `setup.exe` installer for Windows, a `.dmg` for macOS and a `.deb` for Debian and Ubuntu, next to portable archives. A `portable.txt` file next to the executable keeps the configuration, logs and scripts in a `data` folder beside it.
- Update check: once a day at startup (or with *Check now* in Settings › Privacy), ircnext asks GitHub for the latest release. A bar offers to install it: the installer is downloaded, checked against the release's SHA-256 checksums, then opened. Portable copies open the release page instead.
- Confirmation before pasting several lines: from 3 lines (adjustable in Settings › Chat, 0 to turn it off), a dialog shows what will be sent, one message per line, and offers to send it as one line or to cancel. Shorter pastes keep their lines, separated by spaces, instead of being glued together.
- Links panel, like mIRC's URL catcher: `/links` or ⌘/Ctrl+Shift+L lists the web links posted in the open tabs, newest first, with who posted them and where. Filter them, open, copy, or jump to the conversation.
- “New messages” line: coming back to a tab, a red line shows where the messages written while you were away start.
- Notify list, as in mIRC: `/notify alice` (or the network form) reports in the server tab when someone connects or leaves, with a notification on arrival. `/notify` shows who is online, `/notify -r alice` removes. Based on IRCv3 `MONITOR`. Scripts get the `on NOTIFY` and `on UNOTIFY` events.
- Channel list: `/list` opens a window with the network's channels, most populated first, their topic and user count. Search by name or topic, and join in one click.
- Channel settings (mIRC's channel central): the ⚙ button of a channel, or `/channel`, opens its topic, its modes (no outside messages, protected topic, invite only, moderated, secret, key, user limit) and its ban, exception and invitation lists. Operators change them in place; others see them read only.
- Auto-op and auto-voice lists, as in mIRC: `/aop *!*@trusted.host` or `/avoice nick #channel` give `+o` or `+v` to matching people when they join a channel you manage. `/aop` lists, `/aop -r` removes. Kept per network.
- Log viewer: `/logs`, or *Browse* next to the logs folder in Settings › Chat, lists the saved conversations by network and shows them with a search, opening on the current one.

### Fixed

- Scripts: `on QUIT` and `on NICK` handlers take no target field, as in mIRC (`on *:QUIT:commands`).

## [0.3.0] - 2026-11-04

### Added

- Script manager in *Settings → Scripts*: each script file with its state (loaded with its aliases and events, disabled, or its error), a switch to turn it on or off, and buttons to reload, open the folder or edit a file.
- Script timers (`/timer`, `/timers`), hash tables (`hadd`, `$hget`…), ini files (`$readini`, `/writeini`) and text files (`$read`, `/write`), kept inside the `scripts` folder; global variables kept between sessions; `/echo` command.
- Script events: `on TEXT`, `ACTION`, `NOTICE`, `JOIN`, `PART`, `KICK`, `QUIT`, `NICK`, `TOPIC`, `INVITE`, `OP`, `DEOP`, `VOICE`, `DEVOICE`, `CONNECT`, `DISCONNECT`, `START` and `INPUT` (which can stop typed text), with mIRC match texts, targets and `me:`/`!`/`@` prefixes.
- mIRC scripts (mSL), first part: `.mrc` files of the `scripts` folder are loaded at startup and with `/reload`; aliases are typed as commands or used as identifiers, with variables, `if`/`elseif`/`else`, `while`, mIRC conditions and about fifty identifiers. Runaway scripts are stopped.
- FiSH encryption, compatible with FiSH 10: `/setkey [target] <key>` (`cbc:` prefix for CBC), `/delkey`, and `/keyx <nick>` for a DH1080 key exchange. Messages, actions, notices and topics are encrypted and decrypted; encrypted messages carry a *FiSH* badge and the header says when a conversation is encrypted. Keys are kept in the system keychain.
- Application logo: window, macOS app, Windows executable and Linux package (icon and `.desktop` entry).
- Options taken from mIRC: *Do not disturb* (☾ in the sidebar, `Mod+Shift+D`) silences notifications and window alerts; optional shaded lines; an eye button shows the passwords of the network form; middle click closes a tab; ⧉ copies a whole tab to the clipboard; an away message is shown once until it changes (setting to repeat it).
- DCC file transfers: `/dcc send <nick> <file>` or drop a file on a private conversation; incoming offers wait for your answer in the new *Transfers* panel (⇅), with progress, speed, cancel and resume of interrupted downloads. Passive mode for users behind a router, downloads folder and announced address in *Chat* settings.

## [0.2.0] - 2026-11-03

### Added

- Client certificate identification: SASL `EXTERNAL` and CertFP. The network form can create a self-signed certificate, stored in `certs/` next to the configuration.
- English interface with a French translation; new *Appearance → Language* setting (system language by default).
- `draft/multiline`: pasted text of several lines is sent as one message when the server supports it, and multi-line messages are shown with their line breaks.
- SOCKS5 proxy per network, with optional username/password; the server name is resolved by the proxy (no DNS leak with Tor). The proxy password goes to the system keychain.

### Security

- Passwords (server and SASL) stored in the system keychain: Keychain on macOS, Credential Manager on Windows, Secret Service on Linux. Passwords already in the file are moved there on the next save; setting in *Privacy*.

### Changed

- Patch notes in English and French, published with each release.
- Clearer sidebar: each network is a block set apart by a line, its tabs are grouped under “Channels” and “Private messages” headings with their count, and a vertical guide links them to their network.
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

[Unreleased]: https://github.com/BrystoQ/ircnext/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/BrystoQ/ircnext/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/BrystoQ/ircnext/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/BrystoQ/ircnext/releases/tag/v0.1.0
