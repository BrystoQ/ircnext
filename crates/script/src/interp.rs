//! Execution of script bodies and evaluation of their text.

use std::collections::BTreeMap;

use crate::builtins;
use crate::condition;
use crate::parse::{Script, Stmt, parse};
use crate::ParseError;

/// Deepest nesting of alias calls.
const MAX_DEPTH: usize = 64;
/// Statements run by one call before it is stopped (infinite loops).
const MAX_STEPS: u64 = 500_000;

/// What scripts can ask from the client.
pub trait Host {
    /// Runs a client command (`msg #c hi`, `join #c`…), without `/`.
    fn command(&mut self, line: &str);
    /// Shows a line in the active tab (`echo`).
    fn echo(&mut self, text: &str);
    /// Value of a client identifier (`$me`, `$chan`, `$time`…), `None` if
    /// the client does not know it.
    fn identifier(&mut self, name: &str, args: &[String], property: Option<&str>) -> Option<String>;
    /// Result of a condition operator that needs the client (`ison`,
    /// `isop`, `ischan`…), `None` if unknown.
    fn condition(&mut self, operator: &str, left: &str, right: &str) -> Option<bool>;
    /// Content of a script file (`$read`, `hload`, ini files), `None` if
    /// it does not exist or is not allowed.
    fn read_file(&mut self, _path: &str) -> Option<String> {
        None
    }
    /// Writes a script file.
    fn write_file(&mut self, path: &str, _content: &str) -> Result<(), String> {
        Err(format!("cannot write {path}"))
    }
}

/// Error stopping a script.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("{script}: {error}")]
    Parse { script: String, error: ParseError },
    #[error("{0}")]
    Runtime(String),
}

/// A loaded script file.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub name: String,
    pub script: Script,
}

/// How a statement ends.
#[derive(Debug)]
enum Flow {
    Next,
    Return(Option<String>),
    Halt,
    Break,
    Continue,
}

/// Arguments and local variables of a running alias.
#[derive(Debug, Default)]
pub struct Frame {
    pub args: Vec<String>,
    pub locals: BTreeMap<String, String>,
    /// Script the code comes from, for `alias -l`.
    pub script: Option<usize>,
    /// Alias being run (lowercase): calling a command of the same name from
    /// it runs the client's command, as in mIRC.
    pub alias: Option<String>,
}

/// The script engine: loaded scripts and global variables.
#[derive(Debug, Default)]
pub struct Interpreter {
    pub scripts: Vec<Loaded>,
    /// Global variables (`set`), by lowercase name with its `%`.
    pub globals: BTreeMap<String, String>,
    depth: usize,
    steps: u64,
    random: u64,
    /// Identifiers of the event being handled (`$nick`, `$chan`…), also
    /// seen by the aliases it calls.
    pub(crate) event: BTreeMap<String, String>,
    /// Hash tables, by lowercase name.
    pub tables: BTreeMap<String, crate::storage::Table>,
    /// Global variables changed since the last [`take_globals_changed`](Self::take_globals_changed).
    globals_changed: bool,
}

impl Interpreter {
    /// Loads (or reloads) a script under `name`.
    pub fn load(&mut self, name: &str, source: &str) -> Result<(), Error> {
        let script = parse(source).map_err(|error| Error::Parse { script: name.to_owned(), error })?;
        match self.scripts.iter_mut().find(|loaded| loaded.name == name) {
            Some(loaded) => loaded.script = script,
            None => self.scripts.push(Loaded { name: name.to_owned(), script }),
        }
        Ok(())
    }

    pub fn unload(&mut self, name: &str) {
        self.scripts.retain(|loaded| loaded.name != name);
    }

    /// Whether global variables changed since the last call (to save them).
    pub fn take_globals_changed(&mut self) -> bool {
        std::mem::take(&mut self.globals_changed)
    }

    /// Alias visible from outside its script (or from `from`).
    fn find_alias(&self, name: &str, from: Option<usize>) -> Option<(usize, &[Stmt])> {
        let name = name.to_ascii_lowercase();
        self.scripts.iter().enumerate().find_map(|(index, loaded)| {
            let alias = loaded.script.aliases.get(&name)?;
            (!alias.local || from == Some(index)).then_some((index, alias.body.as_slice()))
        })
    }

    /// Whether a command typed by the user is a script alias.
    pub fn has_alias(&self, name: &str) -> bool {
        self.find_alias(name, None).is_some()
    }

    /// Runs the alias `name` with `args` (space-separated), as typed by the
    /// user. Returns its `return` value.
    pub fn call(&mut self, name: &str, args: &str, host: &mut dyn Host) -> Result<Option<String>, Error> {
        self.steps = 0;
        let args = args.split_whitespace().map(str::to_owned).collect();
        self.call_alias(name, args, None, host).map(|(value, _)| value)
    }

    fn call_alias(
        &mut self,
        name: &str,
        args: Vec<String>,
        from: Option<usize>,
        host: &mut dyn Host,
    ) -> Result<(Option<String>, bool), Error> {
        let Some((script, body)) = self.find_alias(name, from).map(|(index, body)| (index, body.to_vec())) else {
            return Err(Error::Runtime(format!("unknown alias {name}")));
        };
        if self.depth >= MAX_DEPTH {
            return Err(Error::Runtime(format!("{name}: too many nested calls")));
        }
        self.depth += 1;
        let mut frame = Frame { args, locals: BTreeMap::new(), script: Some(script), alias: Some(name.to_ascii_lowercase()) };
        let result = self.run(&body, &mut frame, host);
        self.depth -= 1;
        // The flag tells the caller that `halt` stopped the script.
        match result? {
            Flow::Return(value) => Ok((value, false)),
            Flow::Halt => Ok((None, true)),
            _ => Ok((None, false)),
        }
    }

    /// Runs a body in a given frame (events use it with their own frame).
    /// Returns whether it ended with `halt`.
    pub fn run_body(&mut self, body: &[Stmt], frame: &mut Frame, host: &mut dyn Host) -> Result<bool, Error> {
        self.steps = 0;
        self.run(body, frame, host).map(|flow| matches!(flow, Flow::Halt))
    }

    fn run(&mut self, body: &[Stmt], frame: &mut Frame, host: &mut dyn Host) -> Result<Flow, Error> {
        for statement in body {
            self.steps += 1;
            if self.steps > MAX_STEPS {
                return Err(Error::Runtime("script stopped: too many steps (infinite loop?)".into()));
            }
            let flow = match statement {
                Stmt::Command(line) => self.command(line, frame, host)?,
                Stmt::If { branches, otherwise } => {
                    let mut chosen = otherwise.as_deref();
                    for (condition, branch) in branches {
                        if self.test(condition, frame, host)? {
                            chosen = Some(branch);
                            break;
                        }
                    }
                    match chosen {
                        Some(branch) => self.run(branch, frame, host)?,
                        None => Flow::Next,
                    }
                }
                Stmt::While { condition, body } => {
                    let mut flow = Flow::Next;
                    while self.test(condition, frame, host)? {
                        self.steps += 1;
                        if self.steps > MAX_STEPS {
                            return Err(Error::Runtime("script stopped: too many steps (infinite loop?)".into()));
                        }
                        match self.run(body, frame, host)? {
                            Flow::Break => break,
                            Flow::Next | Flow::Continue => {}
                            other => {
                                flow = other;
                                break;
                            }
                        }
                    }
                    flow
                }
            };
            if !matches!(flow, Flow::Next) {
                return Ok(flow);
            }
        }
        Ok(Flow::Next)
    }

    fn test(&mut self, condition: &str, frame: &mut Frame, host: &mut dyn Host) -> Result<bool, Error> {
        let mut context = Evaluation { interpreter: self, frame, host };
        condition::evaluate(condition, &mut context)
    }

    fn command(&mut self, line: &str, frame: &mut Frame, host: &mut dyn Host) -> Result<Flow, Error> {
        // `/cmd` and `.cmd` (silent) are the same command here.
        let line = line.trim_start_matches(['/', '.']);
        let (name, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim();
        match name.to_ascii_lowercase().as_str() {
            "var" => {
                let global = rest.starts_with("-g ");
                let rest = rest.strip_prefix("-g ").unwrap_or(rest);
                for declaration in split_top_level(rest, ',') {
                    let declaration = declaration.trim();
                    let (variable, value) = match declaration.split_once(' ') {
                        Some((variable, value)) => (variable, value.trim().strip_prefix("= ").or_else(|| value.trim().strip_prefix('=')).unwrap_or(value.trim())),
                        None => (declaration, ""),
                    };
                    let value = self.eval(value.trim(), frame, host)?;
                    let key = variable.to_ascii_lowercase();
                    if global {
                        self.globals.insert(key, value);
                        self.globals_changed = true;
                    } else {
                        frame.locals.insert(key, value);
                    }
                }
            }
            "set" => {
                let rest = skip_switches(rest);
                let (variable, value) = rest.split_once(' ').unwrap_or((rest, ""));
                let value = self.eval(value.trim(), frame, host)?;
                self.globals.insert(variable.to_ascii_lowercase(), value);
                self.globals_changed = true;
            }
            "unset" => {
                for variable in skip_switches(rest).split_whitespace() {
                    let pattern = variable.to_ascii_lowercase();
                    self.globals.retain(|name, _| !builtins::wildcard_match(&pattern, name, false));
                }
                self.globals_changed = true;
            }
            "inc" | "dec" => {
                let rest = skip_switches(rest);
                let (variable, amount) = rest.split_once(' ').unwrap_or((rest, "1"));
                let amount = builtins::number(&self.eval(amount.trim(), frame, host)?);
                let key = variable.to_ascii_lowercase();
                let sign = if name.eq_ignore_ascii_case("dec") { -1.0 } else { 1.0 };
                let local = frame.locals.contains_key(&key);
                self.globals_changed |= !local;
                let target = if local { &mut frame.locals } else { &mut self.globals };
                let current = target.get(&key).map_or(0.0, |value| builtins::number(value));
                target.insert(key, builtins::format_number(current + sign * amount));
            }
            "return" => {
                let value = self.eval(rest, frame, host)?;
                return Ok(Flow::Return((!rest.is_empty()).then_some(value)));
            }
            "halt" => return Ok(Flow::Halt),
            "break" => return Ok(Flow::Break),
            "continue" => return Ok(Flow::Continue),
            "echo" => {
                let text = self.eval(skip_echo_switches(rest), frame, host)?;
                host.echo(&text);
            }
            "noop" => {
                self.eval(rest, frame, host)?;
            }
            storage @ ("hmake" | "hfree" | "hadd" | "hinc" | "hdec" | "hdel" | "hsave" | "hload" | "writeini" | "remini" | "write") => {
                let args = self.eval(rest, frame, host)?;
                if let Some(result) = self.storage_command(storage, &args, host) {
                    result?;
                }
            }
            _ => {
                let args = self.eval(rest, frame, host)?;
                let is_self = frame.alias.as_deref().is_some_and(|alias| alias.eq_ignore_ascii_case(name));
                if !is_self && self.find_alias(name, frame.script).is_some() {
                    let args = args.split_whitespace().map(str::to_owned).collect();
                    if self.call_alias(name, args, frame.script, host)?.1 {
                        return Ok(Flow::Halt);
                    }
                } else if args.is_empty() {
                    host.command(name);
                } else {
                    host.command(&format!("{name} {args}"));
                }
            }
        }
        Ok(Flow::Next)
    }

    /// Evaluates the identifiers and variables of `text`.
    pub fn eval(&mut self, text: &str, frame: &mut Frame, host: &mut dyn Host) -> Result<String, Error> {
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::new();
        let mut index = 0;
        let mut word_start = true;
        let mut glue = false;
        while index < chars.len() {
            let c = chars[index];
            if c == ' ' {
                if !glue {
                    out.push(' ');
                }
                index += 1;
                word_start = true;
                continue;
            }
            glue = false;
            if word_start && (c == '$' || c == '%') {
                let (value, next, is_glue) = self.token(&chars, index, frame, host)?;
                if is_glue {
                    // `$+`: glues the words around it.
                    while out.ends_with(' ') {
                        out.pop();
                    }
                    glue = true;
                } else {
                    out.push_str(&value);
                }
                index = next;
                word_start = false;
                continue;
            }
            out.push(c);
            index += 1;
            word_start = false;
        }
        Ok(out)
    }

    /// Evaluates the `$identifier` or `%variable` at `start`. Returns its
    /// value, the index after it, and whether it is `$+`.
    fn token(&mut self, chars: &[char], start: usize, frame: &mut Frame, host: &mut dyn Host) -> Result<(String, usize, bool), Error> {
        let mut index = start + 1;
        if chars[start] == '%' {
            while index < chars.len() && !matches!(chars[index], ' ' | ',' | '(' | ')') {
                index += 1;
            }
            if index == start + 1 {
                return Ok(("%".into(), index, false));
            }
            let name: String = chars[start..index].iter().collect::<String>().to_ascii_lowercase();
            let value = frame.locals.get(&name).or_else(|| self.globals.get(&name)).cloned().unwrap_or_default();
            return Ok((value, index, false));
        }
        // `$+` alone.
        if chars.get(index) == Some(&'+') && chars.get(index + 1).is_none_or(|c| *c == ' ') {
            return Ok((String::new(), index + 1, true));
        }
        // `$1`, `$2-`, `$2-4`, `$0`.
        if chars.get(index).is_some_and(char::is_ascii_digit) {
            let mut end = index;
            while chars.get(end).is_some_and(char::is_ascii_digit) {
                end += 1;
            }
            let first: usize = chars[index..end].iter().collect::<String>().parse().unwrap_or(0);
            let mut last = Some(first);
            if chars.get(end) == Some(&'-') {
                end += 1;
                let digits = end;
                while chars.get(end).is_some_and(char::is_ascii_digit) {
                    end += 1;
                }
                last = (end > digits).then(|| chars[digits..end].iter().collect::<String>().parse().unwrap_or(0));
            }
            let value = if first == 0 && last == Some(0) {
                frame.args.len().to_string()
            } else {
                let from = first.max(1) - 1;
                let to = last.unwrap_or(frame.args.len()).min(frame.args.len());
                frame.args.get(from..to).map(|args| args.join(" ")).unwrap_or_default()
            };
            return Ok((value, end, false));
        }
        let mut end = index;
        while chars.get(end).is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_') {
            end += 1;
        }
        if end == index {
            return Ok(("$".into(), index, false));
        }
        let name: String = chars[index..end].iter().collect();
        let mut args = Vec::new();
        let mut has_parens = false;
        if chars.get(end) == Some(&'(') {
            has_parens = true;
            let mut depth = 0;
            let mut close = end;
            for (offset, c) in chars[end..].iter().enumerate() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            close = end + offset;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if depth != 0 {
                return Err(Error::Runtime(format!("${name}: unclosed parenthesis")));
            }
            let inside: String = chars[end + 1..close].iter().collect();
            for arg in split_top_level(&inside, ',') {
                args.push(self.eval(arg.trim(), frame, host)?);
            }
            end = close + 1;
        }
        let mut property = None;
        if chars.get(end) == Some(&'.') && chars.get(end + 1).is_some_and(char::is_ascii_alphabetic) {
            let from = end + 1;
            end = from;
            while chars.get(end).is_some_and(char::is_ascii_alphanumeric) {
                end += 1;
            }
            property = Some(chars[from..end].iter().collect::<String>());
        }
        let value = self.identifier(&name, &args, has_parens, property.as_deref(), frame, host)?;
        Ok((value, end, false))
    }

    fn identifier(
        &mut self,
        name: &str,
        args: &[String],
        has_parens: bool,
        property: Option<&str>,
        frame: &mut Frame,
        host: &mut dyn Host,
    ) -> Result<String, Error> {
        let lower = name.to_ascii_lowercase();
        if lower == "iif" {
            let condition = args.first().cloned().unwrap_or_default();
            let chosen = if self.test(&condition, frame, host)? { args.get(1) } else { args.get(2) };
            return Ok(chosen.cloned().unwrap_or_default());
        }
        if lower == "rand" {
            return Ok(self.rand(args));
        }
        if let Some(value) = builtins::call(&lower, args, has_parens) {
            return Ok(value);
        }
        if let Some(value) = self.event.get(&lower) {
            return Ok(value.clone());
        }
        if let Some(value) = self.storage_identifier(&lower, args, property, host) {
            return Ok(value);
        }
        if self.find_alias(&lower, frame.script).is_some() {
            return Ok(self.call_alias(&lower, args.to_vec(), frame.script, host)?.0.unwrap_or_default());
        }
        Ok(host.identifier(&lower, args, property).unwrap_or_default())
    }

    /// A random number below `bound`.
    pub(crate) fn roll(&mut self, bound: u64) -> u64 {
        use std::hash::BuildHasher;
        self.random = self.random.wrapping_add(1);
        std::hash::RandomState::new().hash_one(self.random) % bound.max(1)
    }

    /// `$rand(a,b)`: a number, or a character between two characters.
    fn rand(&mut self, args: &[String]) -> String {
        let roll = self.roll(u64::MAX);
        let (low, high) = (args.first().map_or("0", String::as_str), args.get(1).map_or("0", String::as_str));
        if let (Ok(a), Ok(b)) = (low.parse::<i64>(), high.parse::<i64>()) {
            let (a, b) = (a.min(b), a.max(b));
            let span = u64::try_from(b - a).unwrap_or(0) + 1;
            (a + i64::try_from(roll % span).unwrap_or(0)).to_string()
        } else {
            let (a, b) = (low.chars().next().unwrap_or('a'), high.chars().next().unwrap_or('z'));
            let (a, b) = (u32::from(a.min(b)), u32::from(a.max(b)));
            char::from_u32(a + u32::try_from(roll % u64::from(b - a + 1)).unwrap_or(0)).map(String::from).unwrap_or_default()
        }
    }
}

/// Splits on `separator` outside parentheses.
pub fn split_top_level(text: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (index, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            c if c == separator && depth == 0 => {
                parts.push(&text[start..index]);
                start = index + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

/// Skips leading `-switches` of a command.
fn skip_switches(text: &str) -> &str {
    let mut rest = text.trim_start();
    while rest.starts_with('-') {
        rest = rest.split_once(' ').map_or("", |(_, rest)| rest.trim_start());
    }
    rest
}

/// `echo [-switches] [color] text`.
fn skip_echo_switches(text: &str) -> &str {
    let rest = skip_switches(text);
    match rest.split_once(' ') {
        Some((color, after)) if color.parse::<u8>().is_ok() && text.trim_start().starts_with('-') => after,
        _ => rest,
    }
}

/// What a condition needs: evaluating its operands, and the client for
/// some operators.
struct Evaluation<'a> {
    interpreter: &'a mut Interpreter,
    frame: &'a mut Frame,
    host: &'a mut dyn Host,
}

impl condition::Context for Evaluation<'_> {
    fn eval(&mut self, text: &str) -> Result<String, Error> {
        self.interpreter.eval(text, self.frame, self.host)
    }

    fn operator(&mut self, operator: &str, left: &str, right: &str) -> Option<bool> {
        self.host.condition(operator, left, right)
    }
}
