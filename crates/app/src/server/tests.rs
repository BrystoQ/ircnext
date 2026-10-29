use ircnext_client::proto::Message;
use ircnext_client::{Config, Event, Registered};

use super::*;
use crate::input::Command as Cmd;

fn server() -> Server {
    let entry = ServerEntry {
        name: "Test".into(),
        auto_connect: false,
        on_connect: Vec::new(),
        connection: Config {
            host: "irc.example.net".into(),
            nick: "me".into(),
            ..Config::default()
        },
    };
    let mut server = Server::new(ServerId(1), entry);
    let settings = Settings::default();
    let ctx = Context { settings: &settings, active: None };
    server.on_event(
        Event::Registered(Registered { nick: "me".into(), caps: vec![], sts: None, sasl_refused: false }),
        &ctx,
    );
    for line in [
        ":s 005 me CHANTYPES=# PREFIX=(ov)@+ :are supported",
        ":me!u@h JOIN #rust",
        ":s 353 me = #rust :@me alice bob",
        ":s 366 me #rust :End",
    ] {
        receive(&mut server, line);
    }
    server
}

fn receive(server: &mut Server, line: &str) -> Effects {
    receive_with(server, line, &Settings::default(), None)
}

fn receive_with(server: &mut Server, line: &str, settings: &Settings, active: Option<BufferId>) -> Effects {
    let message: Message = line.parse().unwrap();
    server.on_event(Event::Message(message), &Context { settings, active })
}

fn lines(server: &Server, name: &str) -> Vec<LineKind> {
    let id = if name == server.name() {
        server.server_buffer()
    } else {
        server.find(name).unwrap_or_else(|| panic!("tab {name} missing"))
    };
    server.buffer(id).unwrap().lines().map(|l| l.kind.clone()).collect()
}

fn last(server: &Server, name: &str) -> Line {
    let id = server.find(name).unwrap();
    server.buffer(id).unwrap().lines().last().unwrap().clone()
}

#[test]
fn self_join_creates_channel_and_records_autojoin() {
    let server = server();
    let id = server.find("#RUST").unwrap();
    assert!(server.buffer(id).unwrap().joined);
    assert_eq!(server.entry.connection.autojoin, ["#rust"]);
}

#[test]
fn channel_messages_count_as_unread_with_highlight() {
    let mut server = server();
    receive(&mut server, ":alice!a@h PRIVMSG #rust :hi everyone");
    receive(&mut server, ":bob!b@h PRIVMSG #rust :hey me, how are you?");
    let buffer = server.buffer(server.find("#rust").unwrap()).unwrap();
    assert_eq!(buffer.unread, 2);
    assert!(buffer.highlighted);
    assert!(last(&server, "#rust").highlight);
    assert_eq!(buffer.recent_speakers().collect::<Vec<_>>(), ["bob", "alice"]);
}

#[test]
fn active_buffer_does_not_accumulate_unread() {
    let mut server = server();
    let id = server.find("#rust").unwrap();
    receive_with(&mut server, ":alice!a@h PRIVMSG #rust :x", &Settings::default(), Some(id));
    assert_eq!(server.buffer(id).unwrap().unread, 0);
}

#[test]
fn custom_highlight_words() {
    let mut server = server();
    let settings = Settings { highlight_words: vec!["rustacean".into()], ..Settings::default() };
    receive_with(&mut server, ":alice!a@h PRIVMSG #rust :hi Rustacean!", &settings, None);
    assert!(last(&server, "#rust").highlight);
}

#[test]
fn private_messages_open_a_query() {
    let mut server = server();
    receive(&mut server, ":alice!a@h PRIVMSG me :hey");
    let buffer = server.buffer(server.find("Alice").unwrap()).unwrap();
    assert_eq!(buffer.kind, Kind::Query);
    assert!(buffer.highlighted);
}

#[test]
fn echoed_own_messages_go_to_the_target_buffer() {
    let mut server = server();
    receive(&mut server, ":me!u@h PRIVMSG alice :my message");
    let line = last(&server, "alice");
    assert!(line.own);
    assert_eq!(server.buffer(server.find("alice").unwrap()).unwrap().unread, 0);
}

#[test]
fn actions_and_prefixes() {
    let mut server = server();
    receive(&mut server, ":me!u@h PRIVMSG #rust :\x01ACTION dances\x01");
    assert_eq!(
        last(&server, "#rust").kind,
        LineKind::Action { nick: "me".into(), text: "dances".into() }
    );
    receive(&mut server, ":me!u@h PRIVMSG #rust :op talk");
    assert!(matches!(
        last(&server, "#rust").kind,
        LineKind::Message { prefix: Some('@'), .. }
    ));
}

#[test]
fn quit_and_nick_fan_out_to_shared_buffers() {
    let mut server = server();
    receive(&mut server, ":alice!a@h PRIVMSG me :hello");
    receive(&mut server, ":alice!a@h NICK alicia");
    assert!(server.find("alice").is_none());
    let nick = LineKind::Nick { old: "alice".into(), new: "alicia".into() };
    assert_eq!(last(&server, "#rust").kind, nick);
    assert_eq!(last(&server, "alicia").kind, nick);

    receive(&mut server, ":alicia!a@h QUIT :bye");
    let quit = LineKind::Quit { nick: "alicia".into(), reason: Some("bye".into()) };
    assert_eq!(last(&server, "#rust").kind, quit);
    assert_eq!(last(&server, "alicia").kind, quit);
}

#[test]
fn self_part_marks_channel_and_updates_autojoin() {
    let mut server = server();
    let effects = receive(&mut server, ":me!u@h PART #rust :bye");
    assert!(effects.config_changed);
    assert_eq!(server.entry.connection.autojoin, Vec::<String>::new());
    assert!(!server.buffer(server.find("#rust").unwrap()).unwrap().joined);
}

#[test]
fn history_batch_does_not_change_state() {
    let mut server = server();
    receive(&mut server, ":s BATCH +h1 chathistory #rust");
    receive(&mut server, "@batch=h1;time=2020-01-01T00:00:00.000Z :carol!c@h JOIN #rust");
    receive(&mut server, "@batch=h1;time=2020-01-01T00:00:01.000Z;msgid=m1 :carol!c@h PRIVMSG #rust :old");
    receive(&mut server, ":s BATCH -h1");
    let channel = server.network.channel("#rust").unwrap();
    assert!(channel.member("carol").is_none());
    // History is inserted before more recent lines.
    let kinds = lines(&server, "#rust");
    assert_eq!(kinds[0], LineKind::Join { nick: "carol".into(), mask: Some("c@h".into()) });

    // A duplicate (same msgid) is ignored.
    receive(&mut server, "@time=2020-01-01T00:00:01.000Z;msgid=m1 :carol!c@h PRIVMSG #rust :old");
    assert_eq!(lines(&server, "#rust").len(), kinds.len());
}

#[test]
fn redaction_replaces_message() {
    let mut server = server();
    receive(&mut server, "@msgid=x1 :bob!b@h PRIVMSG #rust :oops");
    receive(&mut server, ":bob!b@h REDACT #rust x1");
    assert_eq!(last(&server, "#rust").kind, LineKind::Redacted { nick: "bob".into() });
}

#[test]
fn typing_notifications() {
    let mut server = server();
    receive(&mut server, "@+typing=active :bob!b@h TAGMSG #rust");
    let buffer = server.buffer(server.find("#rust").unwrap()).unwrap();
    assert_eq!(buffer.typing(std::time::Instant::now()).collect::<Vec<_>>(), ["bob"]);
}

#[test]
fn server_notices_and_numerics_go_to_server_buffer() {
    let mut server = server();
    receive(&mut server, ":irc.example.net NOTICE * :*** Looking up your hostname");
    receive(&mut server, ":s 401 me nobody :No such nick");
    let kinds = lines(&server, "Test");
    assert!(kinds.contains(&LineKind::Notice {
        nick: "irc.example.net".into(),
        text: "*** Looking up your hostname".into()
    }));
    assert!(kinds.contains(&LineKind::Error("nobody No such nick".into())));
}

#[test]
fn standard_replies_are_shown() {
    let mut server = server();
    receive(&mut server, "FAIL JOIN CHANNEL_FULL #x :Channel is full");
    assert!(lines(&server, "Test").contains(&LineKind::Error("[JOIN CHANNEL_FULL] Channel is full".into())));
}

#[test]
fn sending_text_requires_a_connection() {
    let mut server = server();
    let settings = Settings::default();
    let ctx = Context { settings: &settings, active: None };
    let id = server.find("#rust").unwrap();
    assert_eq!(server.send_text(id, "hello", &ctx), Err("Not connected.".into()));
    let server_buffer = server.server_buffer();
    assert!(server.send_text(server_buffer, "hello", &ctx).is_err());
}

#[test]
fn close_and_clear_commands() {
    let mut server = server();
    let settings = Settings::default();
    let ctx = Context { settings: &settings, active: None };
    receive(&mut server, ":alice!a@h PRIVMSG me :hello");
    let query = server.find("alice").unwrap();
    server.execute(query, Cmd::Clear, &ctx).unwrap();
    assert_eq!(server.buffer(query).unwrap().lines().count(), 0);
    let effects = server.execute(query, Cmd::Close, &ctx).unwrap();
    assert!(effects.closed);
    assert!(server.find("alice").is_none());
    let server_buffer = server.server_buffer();
    assert!(server.execute(server_buffer, Cmd::Close, &ctx).is_err());
}

#[test]
fn buffers_are_sorted_server_channels_queries() {
    let mut server = server();
    receive(&mut server, ":zed!z@h PRIVMSG me :hi");
    receive(&mut server, ":me!u@h JOIN #abc");
    receive(&mut server, ":amy!a@h PRIVMSG me :hi");
    let names: Vec<_> = server.buffers().map(|(_, b)| b.name.clone()).collect();
    assert_eq!(names, ["Test", "#abc", "#rust", "amy", "zed"]);
}

#[test]
fn disconnection_marks_channels_and_stops_when_final() {
    let mut server = server();
    server.enabled = true;
    let settings = Settings::default();
    let ctx = Context { settings: &settings, active: None };
    server.on_event(Event::Disconnected { error: None, retry_in: None }, &ctx);
    assert!(!server.enabled);
    assert_eq!(server.status, Status::Disconnected(None));
    assert!(!server.buffer(server.find("#rust").unwrap()).unwrap().joined);
}

#[test]
fn limits_ctcp_replies() {
    let mut server = server();
    let start = Instant::now();
    assert!((0..CTCP_REPLY_LIMIT).all(|_| server.allow_ctcp_reply(start)));
    assert!(!server.allow_ctcp_reply(start));
    assert!(server.allow_ctcp_reply(start + CTCP_REPLY_WINDOW));
}

#[test]
fn masks_passwords_sent_to_nickserv() {
    let mut server = server();
    // Server echo (echo-message) of an IDENTIFY sent to NickServ.
    receive(&mut server, ":me!u@h PRIVMSG NickServ :IDENTIFY hunter2");
    let LineKind::Message { text, .. } = last(&server, "NickServ").kind else {
        panic!("message expected");
    };
    assert_eq!(text, "IDENTIFY ••••••••");
    // Ordinary messages and those from others are left alone.
    receive(&mut server, ":alice!a@h PRIVMSG me :IDENTIFY this");
    assert!(matches!(last(&server, "alice").kind, LineKind::Message { ref text, .. } if text == "IDENTIFY this"));
}

#[test]
fn drops_messages_from_ignored_users() {
    let mut server = server();
    let settings = Settings { ignored: vec!["*!*@spam.example".into()], ..Settings::default() };
    receive_with(&mut server, ":troll!t@spam.example PRIVMSG #rust :ad", &settings, None);
    receive_with(&mut server, ":troll!t@spam.example PRIVMSG me :ad", &settings, None);
    receive_with(&mut server, ":alice!a@ok.example PRIVMSG #rust :hello", &settings, None);
    let texts: Vec<_> = lines(&server, "#rust")
        .into_iter()
        .filter_map(|kind| match kind {
            LineKind::Message { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["hello"]);
    assert!(server.find("troll").is_none(), "no tab opened for an ignored user");
}

#[test]
fn raises_alerts_for_mentions_and_private_messages_only() {
    let mut server = server();
    receive(&mut server, ":alice!a@h PRIVMSG #rust :hello all");
    assert_eq!(server.take_alerts(), []);
    receive(&mut server, ":alice!a@h PRIVMSG #rust :me: are you there?");
    receive(&mut server, ":bob!b@h PRIVMSG me :\x01ACTION waves\x01");
    let alerts = server.take_alerts();
    assert_eq!(alerts.len(), 2);
    assert_eq!(alerts[0].title, "alice in #rust");
    assert!(!alerts[0].private);
    assert_eq!(alerts[1].body, "bob waves");
    assert!(alerts[1].private);
}

#[test]
fn history_replay_raises_no_alert() {
    let mut server = server();
    receive(&mut server, ":s BATCH +h chathistory #rust");
    receive(&mut server, "@batch=h :alice!a@h PRIVMSG #rust :me: old message");
    receive(&mut server, ":s BATCH -h");
    assert_eq!(server.take_alerts(), []);
}

#[test]
fn part_uses_the_configured_message() {
    let settings = Settings { part_message: "see you".into(), ..Settings::default() };
    let ctx = Context { settings: &settings, active: None };
    assert_eq!(super::outgoing::default_part_message_for_tests(&ctx).as_deref(), Some("see you"));
}
