//! Scripts run end to end against a recording host.

use crate::{Host, Interpreter};

#[derive(Default)]
struct Recorder {
    commands: Vec<String>,
    echoes: Vec<String>,
    files: std::collections::BTreeMap<String, String>,
}

impl Host for Recorder {
    fn command(&mut self, line: &str) {
        self.commands.push(line.to_owned());
    }

    fn echo(&mut self, text: &str) {
        self.echoes.push(text.to_owned());
    }

    fn identifier(&mut self, name: &str, _args: &[String], _property: Option<&str>) -> Option<String> {
        match name {
            "me" => Some("ircnext".into()),
            "chan" => Some("#rust".into()),
            _ => None,
        }
    }

    fn condition(&mut self, operator: &str, left: &str, right: &str) -> Option<bool> {
        (operator == "ison").then(|| left == "alice" && right == "#rust")
    }

    fn read_file(&mut self, path: &str) -> Option<String> {
        self.files.get(path).cloned()
    }

    fn write_file(&mut self, path: &str, content: &str) -> Result<(), String> {
        self.files.insert(path.to_owned(), content.to_owned());
        Ok(())
    }
}

fn run(script: &str, alias: &str, args: &str) -> Recorder {
    let mut interpreter = Interpreter::default();
    interpreter.load("test.mrc", script).unwrap();
    let mut host = Recorder::default();
    interpreter.call(alias, args, &mut host).unwrap();
    host
}

fn echo(script_body: &str, args: &str) -> Vec<String> {
    run(&format!("alias t {{\n{script_body}\n}}"), "t", args).echoes
}

#[test]
fn sends_commands_with_evaluated_arguments() {
    let host = run("alias greet { msg $chan Hello $1 $+ ! | notice $1 from $me }", "greet", "bob");
    assert_eq!(host.commands, ["msg #rust Hello bob!", "notice bob from ircnext"]);
}

#[test]
fn arguments_and_ranges() {
    assert_eq!(echo("echo -a $0 / $1 / $2- / $2-3 / $4", "a b c d e"), ["5 / a / b c d e / b c / d"]);
}

#[test]
fn variables_local_and_global() {
    let mut interpreter = Interpreter::default();
    interpreter
        .load(
            "vars.mrc",
            "alias t {\n var %local = 1, %other 2\n set %global hi\n inc %local 5\n inc %counter\n echo %local %other %global %counter\n }",
        )
        .unwrap();
    let mut host = Recorder::default();
    interpreter.call("t", "", &mut host).unwrap();
    assert_eq!(host.echoes, ["6 2 hi 1"]);
    assert_eq!(interpreter.globals.get("%global").map(String::as_str), Some("hi"));
    assert!(!interpreter.globals.contains_key("%local"), "local stays local");
    interpreter.load("vars.mrc", "alias u { unset %glob* | echo [ $+ %global $+ ] }").unwrap();
    interpreter.call("u", "", &mut host).unwrap();
    assert_eq!(host.echoes[1], "[]");
}

#[test]
fn control_flow() {
    let lines = echo(
        "var %i = 0\n\
         while (%i < 10) {\n\
           inc %i\n\
           if (2 // %i) continue\n\
           elseif (%i == 7) break\n\
           else echo odd %i\n\
         }",
        "",
    );
    assert_eq!(lines, ["odd 1", "odd 3", "odd 5"]);
}

#[test]
fn conditions() {
    let check = |condition: &str| echo(&format!("if ({condition}) echo yes | else echo no"), "alice Bob 12").remove(0) == "yes";
    assert!(check("$1 == ALICE"));
    assert!(!check("$1 === ALICE"));
    assert!(check("$3 isnum 1-20 && $2 !isupper"), "negated operator");
    assert!(check("($3 > 9) && (lic isin $1)"));
    assert!(check("!$4"));
    assert!(check("$1 !isin hello"));
    assert!(check("*ice iswm $1"));
    assert!(check("$1 ison $chan"));
    assert!(!check("$2 ison $chan"));
    assert!(check("(($1 == x) || ($2 == bob)) && $len($1) == 5"));
}

#[test]
fn text_identifiers() {
    let lines = echo(
        "echo $upper($1) $len($1) $left($1,2) $right($1,-2) $mid($1,2,3)\n\
         echo $replace($1,l,L) $remove($1,e) $pos($1,o) $count(banana,a) $str(ab,3)\n\
         echo $gettok(a.b.c.d,2,46) $gettok(a.b.c.d,-1,46) $gettok(a.b.c.d,2-,46) $numtok(a b c,32)\n\
         echo $addtok(a b,c,32) $addtok(a b,a,32) $deltok(a b c,2,32) $istok(a b c,B,32) $findtok(a b c,c,1,32)\n\
         echo $calc(2 + 3 * 4) $calc((2+3)*4) $calc(7 / 2) $calc(7 // 2) $calc(2^10) $calc(1/0) $round(3.14159,2)\n\
         echo $iif($1 == hello,yes,no) $chr(65) $asc(A) $ord(22) [ $+ $null $+ ]",
        "hello",
    );
    assert_eq!(
        lines,
        [
            "HELLO 5 he llo ell",
            "heLLo hllo 5 3 ababab",
            "b d b.c.d 3",
            "a b c a b a c $true 3",
            "14 20 3.5 3 1024 0 3.14",
            "yes A 65 22nd []",
        ]
    );
}

#[test]
fn aliases_call_each_other_and_return_values() {
    let script = "alias double return $calc($1 * 2)\n\
                  alias -l secret return hidden\n\
                  alias t { echo $double(21) $secret | shout loud }\n\
                  alias shout { echo $upper($1-) }";
    let host = run(script, "t", "");
    assert_eq!(host.echoes, ["42 hidden", "LOUD"]);
}

#[test]
fn local_aliases_are_private() {
    let mut interpreter = Interpreter::default();
    interpreter.load("a.mrc", "alias -l private echo no").unwrap();
    assert!(!interpreter.has_alias("private"));
}

#[test]
fn stops_runaway_scripts() {
    let mut interpreter = Interpreter::default();
    interpreter.load("loop.mrc", "alias spin { while (1) { noop } }\nalias deep deeper\nalias deeper deep").unwrap();
    let mut host = Recorder::default();
    assert!(interpreter.call("spin", "", &mut host).unwrap_err().to_string().contains("too many steps"));
    assert!(interpreter.call("deep", "", &mut host).unwrap_err().to_string().contains("nested"));
}

#[test]
fn halt_stops_everything() {
    let host = run("alias t { echo one | inner | echo three }\nalias inner { echo two | halt }", "t", "");
    assert_eq!(host.echoes, ["one", "two"], "halt also stops the caller");
}

#[test]
fn an_alias_can_wrap_the_command_of_the_same_name() {
    let host = run("alias msg { msg $1 [wrapped] $2- }", "msg", "#c hello");
    assert_eq!(host.commands, ["msg #c [wrapped] hello"]);
}

#[test]
fn the_documented_example_runs() {
    let mut interpreter = Interpreter::default();
    interpreter.load("example.mrc", include_str!("../../../docs/scripts/example.mrc")).unwrap();
    let mut host = Recorder::default();
    interpreter.call("hello", "bob", &mut host).unwrap();
    assert_eq!(host.commands, ["msg #rust Hello bob, welcome to #rust!"]);
    interpreter.call("hello", "", &mut host).unwrap();
    assert_eq!(host.echoes, ["Usage: /hello <nick>"]);
    interpreter.call("countdown", "", &mut host).unwrap();
    assert_eq!(host.echoes[1..], ["3...", "2...", "1...", "Go!"]);
    interpreter.call("dice", "20", &mut host).unwrap();
    let roll: u32 = host.commands[1].split_whitespace().nth(5).unwrap().parse().unwrap();
    assert!((1..=20).contains(&roll), "{}", host.commands[1]);
    assert!(host.commands[1].ends_with("(d20)"), "{}", host.commands[1]);
    interpreter.load("use.mrc", "alias t echo $initials(internet relay chat)").unwrap();
    interpreter.call("t", "", &mut host).unwrap();
    assert_eq!(host.echoes.last().map(String::as_str), Some("IRC"));
}

fn event(name: &str, channel: Option<&str>, text: &str, values: &[(&str, &str)]) -> crate::Event {
    crate::Event {
        name: name.into(),
        channel: channel.map(str::to_owned),
        text: text.into(),
        values: values.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(),
        ..crate::Event::default()
    }
}

#[test]
fn text_events_match_text_and_target() {
    let mut interpreter = Interpreter::default();
    interpreter
        .load(
            "bot.mrc",
            "on *:TEXT:!hello*:#:{ msg $chan Hello $nick $+ ! You said: $2- }\n\
             on *:TEXT:!dice &:#rust:msg $chan $nick rolls $2\n\
             on *:TEXT:*:?:echo private from $nick: $1-\n\
             on *:TEXT:*secret*:*:greet $nick\n\
             alias greet notice $1 hush, $nick",
        )
        .unwrap();
    let mut host = Recorder::default();
    let values = [("nick", "alice"), ("chan", "#rust")];
    interpreter.fire(&event("TEXT", Some("#rust"), "!hello there world", &values), &mut host);
    interpreter.fire(&event("TEXT", Some("#rust"), "!dice 6", &values), &mut host);
    interpreter.fire(&event("TEXT", Some("#other"), "!dice 6", &values), &mut host);
    interpreter.fire(&event("TEXT", Some("#rust"), "!dice 6 extra", &values), &mut host);
    interpreter.fire(&event("TEXT", None, "hi you", &[("nick", "bob")]), &mut host);
    interpreter.fire(&event("TEXT", None, "a SECRET thing", &[("nick", "eve")]), &mut host);
    assert_eq!(
        host.commands,
        ["msg #rust Hello alice! You said: there world", "msg #rust alice rolls 6", "notice eve hush, eve"]
    );
    assert_eq!(host.echoes, ["private from bob: hi you", "private from eve: a SECRET thing"]);
}

#[test]
fn levels_and_prefixes() {
    let mut interpreter = Interpreter::default();
    interpreter
        .load(
            "joins.mrc",
            "on *:JOIN:#:notice $nick welcome to $chan\n\
             on me:*:JOIN:#:echo I joined $chan\n\
             on @*:JOIN:#rust:mode $chan +v $nick\n\
             on *:KICK:#:echo $knick kicked by $nick for $1-\n\
             on *:CONNECT:echo connected to $network",
        )
        .unwrap();
    let mut host = Recorder::default();
    let mut join = event("JOIN", Some("#rust"), "", &[("nick", "carol"), ("chan", "#rust")]);
    interpreter.fire(&join, &mut host);
    join.operator = true;
    interpreter.fire(&join, &mut host);
    assert_eq!(host.commands, ["notice carol welcome to #rust", "notice carol welcome to #rust", "mode #rust +v carol"]);
    let own = crate::Event { own: true, ..event("JOIN", Some("#c"), "", &[("nick", "me"), ("chan", "#c")]) };
    interpreter.fire(&own, &mut host);
    assert_eq!(host.echoes, ["I joined #c"], "own joins only reach me: handlers");
    interpreter.fire(&event("KICK", Some("#c"), "spam", &[("nick", "op"), ("knick", "troll"), ("chan", "#c")]), &mut host);
    let connect = crate::Event { own: true, ..event("CONNECT", None, "", &[("network", "Libera")]) };
    interpreter.fire(&connect, &mut host);
    assert_eq!(host.echoes[1..], ["troll kicked by op for spam", "connected to Libera"]);
}

#[test]
fn input_can_be_stopped() {
    let mut interpreter = Interpreter::default();
    interpreter
        .load("input.mrc", "on *:INPUT:#:{ if ($1 == brb) { msg $chan be right back | halt } }\non *:INPUT:*:{ noop $nosuch( }")
        .unwrap();
    let mut host = Recorder::default();
    let mut input = crate::Event { own: true, ..event("INPUT", Some("#c"), "brb", &[("chan", "#c")]) };
    let (halted, errors) = interpreter.fire(&input, &mut host);
    assert!(halted);
    assert_eq!(host.commands, ["msg #c be right back"]);
    assert_eq!(errors.len(), 1, "the broken handler reports its error without stopping the other");
    input.text = "hello".into();
    assert!(!interpreter.fire(&input, &mut host).0);
}

#[test]
fn word_wildcards() {
    use crate::builtins::wildcard_match;
    assert!(wildcard_match("!seen &", "!seen bob", true));
    assert!(!wildcard_match("!seen &", "!seen", true));
    assert!(!wildcard_match("!seen &", "!seen bob alice", true));
    assert!(wildcard_match("& is *", "rust is great fun", true));
    assert!(wildcard_match("*bot*", "a robot here", true));
}

#[test]
fn the_documented_events_run() {
    let mut interpreter = Interpreter::default();
    interpreter.load("example.mrc", include_str!("../../../docs/scripts/example.mrc")).unwrap();
    let mut host = Recorder::default();
    interpreter.fire(&event("TEXT", Some("#rust"), "!seen bob", &[("nick", "alice"), ("chan", "#rust")]), &mut host);
    interpreter.fire(&event("JOIN", Some("#rust"), "", &[("nick", "carol"), ("chan", "#rust")]), &mut host);
    let input = crate::Event { own: true, ..event("INPUT", Some("#rust"), "brb", &[("chan", "#rust")]) };
    assert!(interpreter.fire(&input, &mut host).0);
    assert_eq!(
        host.commands,
        ["msg #rust alice: I do not keep logs of bob yet.", "notice carol Welcome to #rust, carol!", "msg #rust be right back"]
    );
}

#[test]
fn hash_tables() {
    let lines = echo(
        "hmake scores\n\
         hadd scores Alice 10\n\
         hinc scores alice 5\n\
         hinc scores bob\n\
         hdec scores carol 2\n\
         echo $hget(scores,ALICE) $hget(scores,bob) $hget(scores,carol) $hget(scores,0).item\n\
         echo $hget(scores,1).item = $hget(scores,1).data $hget(scores) [ $+ $hget(nope) $+ ]\n\
         hdel scores bob\n\
         hdel -w scores c*\n\
         echo $hget(scores,0).item\n\
         hsave scores scores.txt\n\
         hfree scores\n\
         echo [ $+ $hget(scores) $+ ]\n\
         hload restored scores.txt\n\
         echo $hget(restored,alice)",
        "",
    );
    assert_eq!(lines, ["15 1 -2 3", "Alice = 15 scores []", "1", "[]", "15"]);
}

#[test]
fn ini_and_text_files() {
    let mut interpreter = Interpreter::default();
    interpreter
        .load(
            "files.mrc",
            "alias t {\n\
               writeini settings.ini greet message Hello there\n\
               writeini settings.ini greet count 3\n\
               writeini settings.ini other x 1\n\
               remini settings.ini other\n\
               echo $readini(settings.ini, greet, message) / $readini(settings.ini, greet, count) / [ $+ $readini(settings.ini, other, x) $+ ]\n\
               write -c quotes.txt first quote\n\
               write quotes.txt second quote\n\
               echo $lines(quotes.txt) / $read(quotes.txt, 2) / $isfile(quotes.txt) $isfile(nope.txt)\n\
             }",
        )
        .unwrap();
    let mut host = Recorder::default();
    interpreter.call("t", "", &mut host).unwrap();
    assert_eq!(host.echoes, ["Hello there / 3 / []", "2 / second quote / $true $false"]);
    assert_eq!(host.files["settings.ini"], "[greet]\nmessage=Hello there\ncount=3\n");
}

#[test]
fn global_variables_round_trip_and_report_changes() {
    use crate::storage::{format_variables, parse_variables};
    let mut interpreter = Interpreter::default();
    interpreter.load("v.mrc", "alias t { set %Seen 3 | var %local 1 }\nalias u var %only local").unwrap();
    let mut host = Recorder::default();
    interpreter.call("u", "", &mut host).unwrap();
    assert!(!interpreter.take_globals_changed(), "locals do not count");
    interpreter.call("t", "", &mut host).unwrap();
    assert!(interpreter.take_globals_changed());
    assert!(!interpreter.take_globals_changed());
    let saved = format_variables(&interpreter.globals);
    assert_eq!(saved, "[variables]\nn0=%seen 3\n");
    assert_eq!(parse_variables(&saved), interpreter.globals);
}

#[test]
fn events_without_a_target() {
    let mut interpreter = Interpreter::default();
    interpreter
        .load(
            "people.mrc",
            "on *:NOTIFY:echo $nick is here ( $+ $address $+ )\n\
             on *:UNOTIFY:echo $nick left\n\
             on *:QUIT:echo $nick quit: $1-\n\
             on *:NICK:echo $nick is now $newnick",
        )
        .unwrap();
    let mut host = Recorder::default();
    interpreter.fire(&event("NOTIFY", None, "", &[("nick", "alice"), ("address", "a@h")]), &mut host);
    interpreter.fire(&event("UNOTIFY", None, "", &[("nick", "alice")]), &mut host);
    interpreter.fire(&event("QUIT", Some("#rust"), "bye all", &[("nick", "bob")]), &mut host);
    interpreter.fire(&event("NICK", Some("#rust"), "", &[("nick", "bob"), ("newnick", "bobby")]), &mut host);
    assert_eq!(host.echoes, ["alice is here (a@h)", "alice left", "bob quit: bye all", "bob is now bobby"]);
}
