use ircnext_client::proto::Message;
use ircnext_client::{Config, Event, Registered};

use super::*;
use crate::input::Command as Cmd;

fn server() -> Server {
    let entry = ServerEntry {
        name: "Test".into(),
        auto_connect: false,
        on_connect: Vec::new(),
        fish_keys: std::collections::BTreeMap::new(),
            notify: Vec::new(),
            auto_modes: Vec::new(),
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
        Event::Registered(Registered { nick: "me".into(), caps: vec![], sts: None, sasl_refused: false, multiline: None }),
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

#[test]
fn multiline_batches_show_as_one_message() {
    let mut server = server();
    let before = lines(&server, "#rust").len();
    receive(&mut server, "@msgid=ml1 :bob!b@h BATCH +ml draft/multiline #rust");
    receive(&mut server, "@batch=ml :bob!b@h PRIVMSG #rust :fn main() {");
    receive(&mut server, "@batch=ml :bob!b@h PRIVMSG #rust :    println!(\"hi\");");
    receive(&mut server, "@batch=ml :bob!b@h PRIVMSG #rust :}");
    assert_eq!(lines(&server, "#rust").len(), before, "nothing shown before the batch closes");
    receive(&mut server, ":bob!b@h BATCH -ml");
    let line = last(&server, "#rust");
    assert!(
        matches!(&line.kind, LineKind::Message { nick, text, .. } if nick == "bob" && text == "fn main() {\n    println!(\"hi\");\n}"),
        "{:?}",
        line.kind
    );
    assert_eq!(line.msgid.as_deref(), Some("ml1"));
}

#[test]
fn away_replies_are_shown_once_per_message() {
    let mut server = server();
    let count = |server: &Server| lines(server, "Test").iter().filter(|kind| matches!(kind, LineKind::Info(text) if text.contains("lunch"))).count();
    receive(&mut server, ":s 301 me bob :at lunch");
    receive(&mut server, ":s 301 me bob :at lunch");
    assert_eq!(count(&server), 1);
    receive(&mut server, ":s 301 me bob :back at lunch soon");
    assert_eq!(count(&server), 2, "a new message is shown");

    let always = Settings { repeat_away_replies: true, ..Settings::default() };
    receive_with(&mut server, ":s 301 me bob :at lunch", &always, None);
    receive_with(&mut server, ":s 301 me bob :at lunch", &always, None);
    assert_eq!(count(&server), 4);
}

#[test]
fn fish_decrypts_conversations_with_a_key() {
    use ircnext_client::fish::Key;
    let mut server = server();
    let settings = Settings::default();
    let ctx = Context { settings: &settings, active: None };
    let rust = server.find("#rust").unwrap();
    let effects = server.execute(rust, Cmd::SetKey { target: None, key: "cbc:channel secret".into() }, &ctx).unwrap();
    assert!(effects.config_changed, "key saved");
    assert!(server.execute(rust, Cmd::SetKey { target: None, key: "abc".into() }, &ctx).is_err(), "too short");

    let key = Key::parse("cbc:channel secret").unwrap();
    receive(&mut server, &format!(":alice!a@h PRIVMSG #rust :{}", key.encrypt("secret plans")));
    let line = last(&server, "#rust");
    assert!(line.encrypted);
    assert!(matches!(&line.kind, LineKind::Message { text, .. } if text == "secret plans"));

    receive(&mut server, &format!(":alice!a@h PRIVMSG #rust :\x01ACTION {}\x01", key.encrypt("whispers")));
    assert!(matches!(&last(&server, "#rust").kind, LineKind::Action { text, .. } if text == "whispers"));

    // Encrypted with another key: shown as received. Long enough that the
    // wrong key cannot give text by chance (random IV).
    let other = Key::parse("cbc:another key").unwrap().encrypt("not for you, not for anyone holding the wrong key");
    receive(&mut server, &format!(":alice!a@h PRIVMSG #rust :{other}"));
    let line = last(&server, "#rust");
    assert!(!line.encrypted);
    assert!(matches!(&line.kind, LineKind::Message { text, .. } if *text == other));

    // Topics too.
    receive(&mut server, &format!(":s 332 me #rust :{}", key.encrypt("hidden topic")));
    assert!(matches!(&last(&server, "#rust").kind, LineKind::Topic { topic: Some(topic), .. } if topic == "hidden topic"));

    // What we send is encrypted with the same key.
    assert_eq!(key.decrypt(&server.seal("#rust", "hello")).as_deref(), Some("hello"));

    server.execute(rust, Cmd::DelKey(None), &ctx).unwrap();
    assert!(server.fish_key("#rust").is_none());
    assert_eq!(server.seal("#rust", "hello"), "hello");
}

#[test]
fn fish_key_exchange_stores_a_key() {
    use ircnext_client::fish::{KeyExchange, Mode};
    let mut server = server();
    let peer = KeyExchange::new();
    let effects = receive(&mut server, &format!(":bob!b@h NOTICE me :DH1080_INIT {} CBC", peer.public_key()));
    assert!(effects.config_changed, "key saved");
    assert_eq!(server.fish_key("bob").unwrap().mode, Mode::Cbc);
    let bob = server.find("bob").expect("conversation opened");
    assert!(server.buffer(bob).unwrap().lines().any(|line| matches!(&line.kind, LineKind::Info(text) if text.contains("bob"))));

    // A FINISH nobody asked for is ignored, an invalid key too.
    receive(&mut server, ":eve!e@h NOTICE me :DH1080_FINISH AQIDA");
    assert!(server.fish_key("eve").is_none());
    receive(&mut server, ":eve!e@h NOTICE me :DH1080_INIT AQ CBC");
    assert!(server.fish_key("eve").is_none());
}

#[test]
fn notify_list_reports_arrivals_and_departures() {
    use super::notify::parse_list;
    assert_eq!(parse_list("alice, Bob bob  #chan *!*@host carol"), ["alice", "Bob", "carol"]);

    let mut server = server();
    let settings = Settings::default();
    let ctx = Context { settings: &settings, active: None };
    let status = server.server_buffer();
    let info = |server: &Server| match server.buffer(status).unwrap().lines().last().unwrap().kind.clone() {
        LineKind::Info(text) | LineKind::Error(text) => text,
        other => panic!("{other:?}"),
    };
    let effects = server.execute(status, Cmd::Notify { nick: Some("alice".into()), remove: false }, &ctx).unwrap();
    assert!(effects.config_changed);
    assert_eq!(server.entry.notify, ["alice"]);
    let effects = server.execute(status, Cmd::Notify { nick: Some("ALICE".into()), remove: false }, &ctx).unwrap();
    assert!(!effects.config_changed, "already there, whatever the case");

    receive(&mut server, ":s 005 me MONITOR=100 :are supported");
    server.take_script_events();
    receive(&mut server, ":s 730 me :alice!a@example.org,zed!z@h");
    assert_eq!(info(&server), "alice (a@example.org) is on IRC", "only the notify list is reported");
    assert_eq!(server.take_alerts().len(), 1);
    let events = server.take_script_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event.name, "NOTIFY");
    assert_eq!(events[0].event.values["nick"], "alice");

    server.execute(status, Cmd::Notify { nick: None, remove: false }, &ctx).unwrap();
    assert_eq!(info(&server), "Notify list: alice (online)");

    receive(&mut server, ":s 731 me :alice");
    assert_eq!(info(&server), "alice has left IRC");
    assert!(server.take_alerts().is_empty(), "departures are not notified");
    assert_eq!(server.take_script_events()[0].event.name, "UNOTIFY");

    let effects = server.execute(status, Cmd::Notify { nick: Some("Alice".into()), remove: true }, &ctx).unwrap();
    assert!(effects.config_changed);
    assert_eq!(server.entry.notify, Vec::<String>::new());
    receive(&mut server, ":s 730 me :alice!a@example.org");
    assert_eq!(info(&server), "Alice removed from the notify list", "no longer reported");
}

#[test]
fn channel_list_collects_list_replies() {
    let mut server = server();
    let settings = Settings::default();
    let ctx = Context { settings: &settings, active: None };
    let status = server.server_buffer();
    assert_eq!(server.execute(status, Cmd::List(None), &ctx).unwrap_err(), "Not connected.");
    let shown = lines(&server, "Test").len();

    server.channel_list = Some(super::channel_list::ChannelList::default());
    for line in [
        ":s 321 me Channel :Users Name",
        ":s 322 me #small 3 :\x02tiny\x02 corner",
        ":s 322 me #rust 1200 :[+nt] The Rust language",
        ":s 322 me #empty 1 :",
    ] {
        receive(&mut server, line);
    }
    assert!(!server.channel_list.as_ref().unwrap().complete);
    receive(&mut server, ":s 323 me :End of /LIST");
    let list = server.channel_list.as_ref().unwrap();
    assert!(list.complete);
    let names: Vec<&str> = list.entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, ["#rust", "#small", "#empty"], "most populated first");
    assert_eq!(list.entries[0].topic, "The Rust language", "modes removed");
    assert_eq!(list.entries[1].topic, "tiny corner", "formatting removed");
    assert!(list.entries[0].matches("rust language"));
    assert_eq!(lines(&server, "Test").len(), shown, "not printed in the server tab");
}

#[test]
fn channel_central_replies_stay_out_of_the_server_tab() {
    let mut server = server();
    let settings = Settings::default();
    let ctx = Context { settings: &settings, active: None };
    let rust = server.find("#rust").unwrap();
    assert!(server.can_manage("#rust"), "operator");
    assert_eq!(server.mask_modes(), ['b', 'e', 'I']);
    assert_eq!(server.execute(rust, Cmd::Channel(Some("#nope".into())), &ctx).unwrap_err(), "You are not on #nope.");
    assert_eq!(server.execute(rust, Cmd::Channel(None), &ctx).unwrap_err(), "Not connected.");

    // What `request_channel_details` sets up before sending.
    server.network.request_mask_list("#rust", 'b');
    server.quiet_replies.extend([("#rust".to_owned(), ircnext_client::proto::numeric::RPL_CHANNELMODEIS), ("#rust".to_owned(), ircnext_client::proto::numeric::RPL_ENDOFBANLIST)]);
    let count = |server: &Server| lines(server, "Test").len() + lines(server, "#rust").len();
    let shown = count(&server);
    for line in [":s 324 me #RUST +nt", ":s 367 me #rust *!*@spam.example alice 1700000000", ":s 368 me #rust :End of ban list"] {
        receive(&mut server, line);
    }
    assert_eq!(count(&server), shown, "answers for the window only");
    let channel = server.network.channel("#rust").unwrap();
    assert_eq!(channel.modes.keys().collect::<String>(), "nt");
    assert_eq!(channel.mask_lists[&'b'].entries.len(), 1);

    // Asked by hand afterwards: shown as usual.
    receive(&mut server, ":s 324 me #rust +nt");
    assert_eq!(count(&server), shown + 1);

    receive(&mut server, ":alice!a@h MODE #rust -o me");
    assert!(!server.can_manage("#rust"), "no longer operator");
}

#[test]
fn auto_op_and_voice_on_join() {
    let mut server = server();
    let (handle, mut outbox) = ircnext_client::Handle::detached();
    server.handle = Some(handle);
    let settings = Settings::default();
    let ctx = Context { settings: &settings, active: None };
    let rust = server.find("#rust").unwrap();
    let run = |server: &mut Server, command| server.execute(rust, command, &ctx).unwrap();

    let effects = run(&mut server, Cmd::AutoMode { mode: 'o', mask: Some("*!*@trusted.example".into()), channels: vec![], remove: false });
    assert!(effects.config_changed);
    run(&mut server, Cmd::AutoMode { mode: 'v', mask: Some("dave".into()), channels: vec!["#rust".into()], remove: false });
    run(&mut server, Cmd::AutoMode { mode: 'v', mask: Some("*!*@trusted.example".into()), channels: vec![], remove: false });
    assert_eq!(server.entry.auto_modes[1].mask, "dave!*@*", "nick turned into a mask");

    receive(&mut server, ":friend!f@TRUSTED.example JOIN #rust");
    receive(&mut server, ":dave!d@elsewhere JOIN #rust");
    receive(&mut server, ":eve!e@elsewhere JOIN #rust");
    receive(&mut server, ":dave!d@elsewhere JOIN #other");
    assert_eq!(outbox.sent(), ["MODE #rust +o friend", "MODE #rust +v dave"], "+o wins over +v; #other is not ours");

    run(&mut server, Cmd::AutoMode { mode: 'v', mask: None, channels: vec![], remove: false });
    let status = server.server_buffer();
    let last = server.buffer(status).unwrap().lines().last().unwrap().kind.clone();
    assert_eq!(last, LineKind::Info("Auto-voice: dave!*@* (#rust) · *!*@trusted.example".into()));

    let effects = run(&mut server, Cmd::AutoMode { mode: 'v', mask: Some("dave".into()), channels: vec![], remove: true });
    assert!(effects.config_changed);
    receive(&mut server, ":alice!a@h MODE #rust -o me");
    receive(&mut server, ":friend!f@trusted.example PART #rust");
    receive(&mut server, ":friend!f@trusted.example JOIN #rust");
    assert!(outbox.sent().is_empty(), "no longer operator");
}

#[test]
fn unverified_certificate_is_pinned_then_enforced() {
    use ircnext_client::connection::PeerCertificate;
    let mut server = server();
    server.enabled = true;
    let settings = Settings::default();
    let ctx = Context { settings: &settings, active: None };
    let certificate = |fingerprint: &str, verified| Event::Certificate(PeerCertificate { fingerprint: fingerprint.into(), verified });

    assert!(!server.on_event(certificate("aa", true), &ctx).config_changed, "verified: nothing to pin");
    server.entry.connection.accept_invalid_certs = true;
    assert!(server.on_event(certificate("abcd", false), &ctx).config_changed);
    assert_eq!(server.entry.connection.pinned_cert.as_deref(), Some("abcd"));
    assert!(matches!(lines(&server, "Test").last(), Some(LineKind::Info(text)) if text.contains("ab:cd")));

    assert!(!server.on_event(certificate("ABCD", false), &ctx).config_changed, "same certificate");
    assert!(server.enabled);
    server.on_event(certificate("ffff", false), &ctx);
    assert!(!server.enabled, "changed: disconnected");
    assert_eq!(server.entry.connection.pinned_cert.as_deref(), Some("abcd"), "the pin is kept");
    assert!(matches!(lines(&server, "Test").last(), Some(LineKind::Error(text)) if text.contains("ff:ff")));
}
