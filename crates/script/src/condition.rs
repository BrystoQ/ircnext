//! Conditions of `if`, `elseif`, `while` and `$iif`: comparisons joined by
//! `&&` and `||`, negated by `!`, grouped by parentheses.

use crate::builtins::{number_value, wildcard_match};
use crate::interp::Error;

/// What evaluating a condition needs.
pub trait Context {
    /// Evaluates the identifiers and variables of an operand.
    fn eval(&mut self, text: &str) -> Result<String, Error>;
    /// Operators that need the client (`ison`, `isop`…).
    fn operator(&mut self, operator: &str, left: &str, right: &str) -> Option<bool>;
}

/// Evaluates a condition (the text between the parentheses of an `if`).
pub fn evaluate(text: &str, context: &mut dyn Context) -> Result<bool, Error> {
    let mut parser = Parser { chars: text.chars().collect(), pos: 0 };
    let value = parser.or(context)?;
    parser.skip_spaces();
    if parser.pos < parser.chars.len() {
        return Err(Error::Runtime(format!("invalid condition: ({text})")));
    }
    Ok(value)
}

/// Truth of a single value: anything but `$null`, `0` and `$false`.
pub fn is_true(value: &str) -> bool {
    !(value.is_empty() || value == "0" || value.eq_ignore_ascii_case("$false"))
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn skip_spaces(&mut self) {
        while self.chars.get(self.pos).is_some_and(|c| c.is_whitespace()) {
            self.pos += 1;
        }
    }

    fn starts_with(&self, text: &str) -> bool {
        text.chars().enumerate().all(|(offset, c)| self.chars.get(self.pos + offset) == Some(&c))
    }

    fn or(&mut self, context: &mut dyn Context) -> Result<bool, Error> {
        let mut value = self.and(context)?;
        loop {
            self.skip_spaces();
            if !self.starts_with("||") {
                return Ok(value);
            }
            self.pos += 2;
            let right = self.and(context)?;
            value = value || right;
        }
    }

    fn and(&mut self, context: &mut dyn Context) -> Result<bool, Error> {
        let mut value = self.unary(context)?;
        loop {
            self.skip_spaces();
            if !self.starts_with("&&") {
                return Ok(value);
            }
            self.pos += 2;
            let right = self.unary(context)?;
            value = value && right;
        }
    }

    fn unary(&mut self, context: &mut dyn Context) -> Result<bool, Error> {
        self.skip_spaces();
        if self.starts_with("!(") {
            self.pos += 1;
            return Ok(!self.unary(context)?);
        }
        if self.chars.get(self.pos) == Some(&'(') {
            self.pos += 1;
            let value = self.or(context)?;
            self.skip_spaces();
            if self.chars.get(self.pos) != Some(&')') {
                return Err(Error::Runtime("unclosed parenthesis in a condition".into()));
            }
            self.pos += 1;
            return Ok(value);
        }
        let raw = self.comparison_text();
        compare(&raw, context)
    }

    /// Text of a comparison, up to `&&`, `||` or an unmatched `)`.
    fn comparison_text(&mut self) -> String {
        let mut depth = 0;
        let mut text = String::new();
        while let Some(&c) = self.chars.get(self.pos) {
            if depth == 0 && (self.starts_with("&&") || self.starts_with("||")) {
                break;
            }
            match c {
                '(' => depth += 1,
                ')' if depth == 0 => break,
                ')' => depth -= 1,
                _ => {}
            }
            text.push(c);
            self.pos += 1;
        }
        text.trim().to_owned()
    }
}

/// Operators that compare two values.
const OPERATORS: &[&str] = &[
    "==", "===", "!=", "!==", "<", ">", "<=", ">=", "//", "\\\\", "&", "isin", "isincs", "iswm", "iswmcs", "isnum",
    "isletter", "isalnum", "isalpha", "islower", "isupper", "ison", "isop", "ishop", "isvoice", "isreg", "ischan",
    "isban", "isquiet",
];

fn compare(raw: &str, context: &mut dyn Context) -> Result<bool, Error> {
    let tokens = split_tokens(raw);
    let found = tokens.iter().enumerate().skip(1).find_map(|(index, token)| {
        let lower = token.to_ascii_lowercase();
        let (negated, operator) = match lower.strip_prefix('!') {
            Some(rest) if OPERATORS.contains(&rest) && rest != "=" && !rest.starts_with('=') => (true, rest.to_owned()),
            _ => (false, lower),
        };
        OPERATORS.contains(&operator.as_str()).then_some((index, negated, operator))
    });
    let Some((index, negated, operator)) = found else {
        // A single value, possibly negated: `!%x`.
        let (negated, value) = match raw.strip_prefix('!') {
            Some(rest) => (true, rest),
            None => (false, raw),
        };
        return Ok(is_true(&context.eval(value)?) != negated);
    };
    let left = context.eval(&tokens[..index].join(" "))?;
    let right = context.eval(&tokens[index + 1..].join(" "))?;
    let result = match operator.as_str() {
        "==" => left.to_lowercase() == right.to_lowercase(),
        "===" => left == right,
        "!=" => left.to_lowercase() != right.to_lowercase(),
        "!==" => left != right,
        "<" | ">" | "<=" | ">=" => {
            let ordering = match (number_value(&left), number_value(&right)) {
                (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal),
                _ => left.to_lowercase().cmp(&right.to_lowercase()),
            };
            match operator.as_str() {
                "<" => ordering.is_lt(),
                ">" => ordering.is_gt(),
                "<=" => ordering.is_le(),
                _ => ordering.is_ge(),
            }
        }
        // `a // b`: b is a multiple of a.
        "//" | "\\\\" => {
            let multiple = match (number_value(&left), number_value(&right)) {
                (Some(a), Some(b)) if a != 0.0 => (b % a).abs() < f64::EPSILON,
                _ => false,
            };
            multiple == (operator == "//")
        }
        "&" => match (number_value(&left), number_value(&right)) {
            #[allow(clippy::cast_possible_truncation)]
            (Some(a), Some(b)) => (a as i64) & (b as i64) != 0,
            _ => false,
        },
        "isin" => right.to_lowercase().contains(&left.to_lowercase()),
        "isincs" => right.contains(&left),
        "iswm" => wildcard_match(&left.to_lowercase(), &right.to_lowercase(), false),
        "iswmcs" => wildcard_match(&left, &right, false),
        "isnum" => match number_value(&left) {
            Some(value) if right.is_empty() => value.is_finite(),
            Some(value) => match right.split_once('-') {
                Some((low, high)) => {
                    number_value(low).is_none_or(|low| value >= low) && number_value(high).is_none_or(|high| value <= high)
                }
                None => number_value(&right) == Some(value),
            },
            None => false,
        },
        "isletter" => left.chars().count() == 1 && left.chars().all(|c| if right.is_empty() { c.is_alphabetic() } else { right.contains(c) }),
        "isalnum" => !left.is_empty() && left.chars().all(char::is_alphanumeric),
        "isalpha" => !left.is_empty() && left.chars().all(char::is_alphabetic),
        "islower" => left.chars().any(char::is_lowercase) && !left.chars().any(char::is_uppercase),
        "isupper" => left.chars().any(char::is_uppercase) && !left.chars().any(char::is_lowercase),
        other => context.operator(other, &left, &right).unwrap_or(false),
    };
    Ok(result != negated)
}

/// Words of a comparison, keeping identifiers with spaces in their
/// parentheses together.
fn split_tokens(raw: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    for c in raw.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            c if c.is_whitespace() && depth == 0 => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}
