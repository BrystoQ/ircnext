//! Scripts run end to end against a recording host.

use crate::{Host, Interpreter};

#[derive(Default)]
struct Recorder {
    commands: Vec<String>,
    echoes: Vec<String>,
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
