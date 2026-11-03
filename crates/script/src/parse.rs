//! Parsing of mIRC script files (`.mrc`): `alias` and `on` blocks, made of
//! commands, `if`/`elseif`/`else` and `while`.
//!
//! Commands are separated by line breaks or `|`; blocks are delimited by
//! `{` and `}`. Lines starting with `;` and `/* … */` blocks are comments.

use std::collections::BTreeMap;

/// A statement of a script body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    /// A command line, evaluated when run (`msg $chan hello`).
    Command(String),
    If {
        /// `if` then `elseif` conditions (raw text) with their bodies.
        branches: Vec<(String, Vec<Stmt>)>,
        otherwise: Option<Vec<Stmt>>,
    },
    While {
        condition: String,
        body: Vec<Stmt>,
    },
}

/// `alias name { … }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias {
    pub name: String,
    /// `alias -l`: callable only from its own script.
    pub local: bool,
    pub body: Vec<Stmt>,
}

/// `on level:EVENT:…:{ … }`: the raw header, interpreted by the events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handler {
    pub header: String,
    pub body: Vec<Stmt>,
    /// Line of the header, for error messages.
    pub line: usize,
}

/// A parsed script file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Script {
    /// Aliases by lowercase name.
    pub aliases: BTreeMap<String, Alias>,
    pub handlers: Vec<Handler>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("line {line}: {message}")]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

/// Parses a script file.
pub fn parse(source: &str) -> Result<Script, ParseError> {
    let text = strip_comments(source);
    let mut parser = Parser { chars: text.chars().collect(), pos: 0, line: 1 };
    let mut script = Script::default();
    loop {
        parser.skip_blank();
        if parser.at_end() {
            return Ok(script);
        }
        let line = parser.line;
        let word = parser.word();
        match word.to_ascii_lowercase().as_str() {
            "alias" => {
                parser.skip_spaces();
                let mut local = false;
                let mut name = parser.word();
                if name.eq_ignore_ascii_case("-l") {
                    local = true;
                    parser.skip_spaces();
                    name = parser.word();
                }
                if name.is_empty() {
                    return Err(parser.error("alias without a name"));
                }
                let body = parser.body()?;
                let name = name.trim_start_matches('/').to_owned();
                script.aliases.insert(name.to_ascii_lowercase(), Alias { name, local, body });
            }
            "on" => {
                parser.skip_spaces();
                // Header up to the colon before the body (`…:{` or `…:command`).
                let header = parser.event_header()?;
                let body = parser.body()?;
                script.handlers.push(Handler { header, body, line });
            }
            "" => return Err(parser.error("unexpected character")),
            other => return Err(ParseError { line, message: format!("unknown block “{other}” (expected alias or on)") }),
        }
    }
}

/// Removes `;` comment lines and `/* … */` blocks, keeping line breaks so
/// that line numbers stay right.
fn strip_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut in_block = false;
    for line in source.lines() {
        let trimmed = line.trim_start();
        if in_block {
            if trimmed.trim_end().ends_with("*/") {
                in_block = false;
            }
        } else if trimmed.starts_with("/*") {
            in_block = !trimmed.trim_end().ends_with("*/") || trimmed.trim_end() == "/*";
        } else if !trimmed.starts_with(';') {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// Fields of an `on` header: `level:EVENT[:match][:target]`.
pub fn header_fields(event: &str) -> usize {
    match event {
        "TEXT" | "ACTION" | "NOTICE" | "CTCP" | "CTCPREPLY" => 4,
        "CONNECT" | "DISCONNECT" | "START" | "EXIT" | "LOAD" | "UNLOAD" => 2,
        _ => 3,
    }
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    line: usize,
}

impl Parser {
    fn at_end(&self) -> bool {
        self.pos >= self.chars.len()
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
        }
        Some(c)
    }

    fn error(&self, message: &str) -> ParseError {
        ParseError { line: self.line, message: message.to_owned() }
    }

    /// Spaces and tabs.
    fn skip_spaces(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\r')) {
            self.bump();
        }
    }

    /// Spaces, line breaks and `|` separators.
    fn skip_blank(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\r' | '\n' | '|')) {
            self.bump();
        }
    }

    /// A word up to a blank or a brace.
    fn word(&mut self) -> String {
        let mut word = String::new();
        while let Some(c) = self.peek().filter(|c| !c.is_whitespace() && *c != '{' && *c != '}') {
            word.push(c);
            self.bump();
        }
        word
    }

    fn event_header(&mut self) -> Result<String, ParseError> {
        // The header ends at the last `:` before a `{` or a line break.
        let start = self.pos;
        let mut end_line = self.pos;
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.pos += 1;
            end_line = self.pos;
        }
        let line: String = self.chars[start..end_line].iter().collect();
        let colon = if let Some(index) = line.rfind(":{") {
            index
        } else {
            // Single-line handler (`on *:TEXT:hi:#:msg $chan hello`): the
            // number of header fields depends on the event.
            let event = line.split(':').nth(1).unwrap_or_default().to_ascii_uppercase();
            let fields = header_fields(&event);
            let parts: Vec<&str> = line.splitn(fields + 1, ':').collect();
            if parts.len() <= fields {
                return Err(self.error("incomplete on header"));
            }
            parts[..fields].iter().map(|part| part.len() + 1).sum::<usize>() - 1
        };
        let header = line[..colon].trim().to_owned();
        self.pos = start + line[..=colon].chars().count();
        Ok(header)
    }

    /// Body after an alias name or a condition: a `{ … }` block, or a single
    /// command up to the end of the line.
    fn body(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.skip_spaces();
        if self.peek() == Some('{') {
            self.bump();
            return self.statements(true);
        }
        Ok(vec![self.statement()?])
    }

    /// Statements up to the closing `}` (`in_block`) or the end of the text.
    fn statements(&mut self, in_block: bool) -> Result<Vec<Stmt>, ParseError> {
        let mut statements = Vec::new();
        loop {
            self.skip_blank();
            match self.peek() {
                None if in_block => return Err(self.error("missing }")),
                None => return Ok(statements),
                Some('}') => {
                    self.bump();
                    return Ok(statements);
                }
                _ => statements.push(self.statement()?),
            }
        }
    }

    fn statement(&mut self) -> Result<Stmt, ParseError> {
        let save = (self.pos, self.line);
        let keyword = self.word().to_ascii_lowercase();
        match keyword.as_str() {
            "if" => {
                let mut branches = vec![(self.condition()?, self.body()?)];
                let mut otherwise = None;
                loop {
                    let before = (self.pos, self.line);
                    self.skip_blank();
                    match self.word().to_ascii_lowercase().as_str() {
                        "elseif" => branches.push((self.condition()?, self.body()?)),
                        "else" => {
                            self.skip_spaces();
                            // `else if` is accepted as `elseif`.
                            let after_else = (self.pos, self.line);
                            if self.word().eq_ignore_ascii_case("if") {
                                branches.push((self.condition()?, self.body()?));
                                continue;
                            }
                            (self.pos, self.line) = after_else;
                            otherwise = Some(self.body()?);
                            break;
                        }
                        _ => {
                            (self.pos, self.line) = before;
                            break;
                        }
                    }
                }
                Ok(Stmt::If { branches, otherwise })
            }
            "while" => Ok(Stmt::While { condition: self.condition()?, body: self.body()? }),
            _ => {
                (self.pos, self.line) = save;
                Ok(Stmt::Command(self.command()))
            }
        }
    }

    /// `( … )` with balanced parentheses; returns the inside.
    fn condition(&mut self) -> Result<String, ParseError> {
        self.skip_spaces();
        if self.peek() != Some('(') {
            return Err(self.error("expected ( after if, elseif or while"));
        }
        self.bump();
        let mut depth = 1;
        let mut text = String::new();
        while let Some(c) = self.bump() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(text.trim().to_owned());
                    }
                }
                '\n' => return Err(self.error("unclosed parenthesis in a condition")),
                _ => {}
            }
            text.push(c);
        }
        Err(self.error("unclosed parenthesis in a condition"))
    }

    /// A command up to a line break, a `|`, or a `}` closing the block
    /// (a `}` preceded by a space).
    fn command(&mut self) -> String {
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if c == '\n' || c == '|' || (c == '}' && (text.is_empty() || text.ends_with(' '))) {
                break;
            }
            text.push(c);
            self.bump();
        }
        text.trim().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(text: &str) -> Stmt {
        Stmt::Command(text.into())
    }

    #[test]
    fn parses_aliases_in_every_form() {
        let script = parse(
            "; a comment\n\
             alias hello { echo -a Hello $1 | msg $chan hi }\n\
             alias -l helper return 42\n\
             /* a block\n of comments */\n\
             alias multi {\n  var %x = 1\n  echo %x\n}\n",
        )
        .unwrap();
        assert_eq!(script.aliases["hello"].body, [command("echo -a Hello $1"), command("msg $chan hi")]);
        assert!(script.aliases["helper"].local);
        assert_eq!(script.aliases["helper"].body, [command("return 42")]);
        assert_eq!(script.aliases["multi"].body, [command("var %x = 1"), command("echo %x")]);
    }

    #[test]
    fn parses_control_flow() {
        let script = parse(
            "alias test {\n\
               if ($1 == a) { echo one }\n\
               elseif (($1 == b) && ($2 isnum)) echo two\n\
               else if ($1 == c) { echo three }\n\
               else { echo other }\n\
               while (%i < 3) { inc %i | if (%i == 2) continue }\n\
             }",
        )
        .unwrap();
        let body = &script.aliases["test"].body;
        let Stmt::If { branches, otherwise } = &body[0] else { panic!("if expected") };
        assert_eq!(branches.len(), 3);
        assert_eq!(branches[1].0, "($1 == b) && ($2 isnum)");
        assert_eq!(branches[1].1, [command("echo two")]);
        assert_eq!(otherwise.as_deref(), Some(&[command("echo other")][..]));
        let Stmt::While { condition, body } = &body[1] else { panic!("while expected") };
        assert_eq!(condition, "%i < 3");
        assert_eq!(body.len(), 2);
    }

    #[test]
    fn keeps_event_headers() {
        let script = parse("on *:TEXT:!hello*:#:{ msg $chan Hi $nick }\non 1:JOIN:#rust:notice $nick welcome").unwrap();
        assert_eq!(script.handlers[0].header, "*:TEXT:!hello*:#");
        assert_eq!(script.handlers[0].body, [command("msg $chan Hi $nick")]);
        assert_eq!(script.handlers[1].header, "1:JOIN:#rust");
        assert_eq!(script.handlers[1].body, [command("notice $nick welcome")]);
        assert_eq!(script.handlers[1].line, 2);
    }

    #[test]
    fn reports_errors_with_their_line() {
        assert_eq!(parse("alias a {\n echo x\n").unwrap_err().message, "missing }");
        assert_eq!(parse("\n\nwhatever").unwrap_err().line, 3);
        assert!(parse("alias a { if $1 == 1 { echo } }").is_err());
    }
}
