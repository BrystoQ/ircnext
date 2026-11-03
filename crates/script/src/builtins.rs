//! Built-in identifiers that need nothing from the client: text, tokens,
//! numbers and `$calc`.

/// Value of a built-in identifier, `None` if `name` is not one.
pub fn call(name: &str, args: &[String], has_parens: bool) -> Option<String> {
    let arg = |index: usize| args.get(index).map_or("", String::as_str);
    #[allow(clippy::cast_possible_truncation)] // mIRC truncates numbers
    let int = |index: usize| number(arg(index)) as i64;
    let value = match name {
        "null" => String::new(),
        "true" => "$true".into(),
        "false" => "$false".into(),
        "cr" => "\r".into(),
        "lf" => "\n".into(),
        "crlf" => "\r\n".into(),
        _ if !has_parens => return None,
        "len" => arg(0).chars().count().to_string(),
        "upper" => arg(0).to_uppercase(),
        "lower" => arg(0).to_lowercase(),
        "chr" => u32::try_from(int(0)).ok().and_then(char::from_u32).map(String::from).unwrap_or_default(),
        "asc" => arg(0).chars().next().map(|c| u32::from(c).to_string()).unwrap_or_default(),
        "left" => left(arg(0), int(1)),
        "right" => right(arg(0), int(1)),
        "mid" => mid(arg(0), int(1), args.get(2).map(|_| int(2))),
        "str" => arg(0).repeat(usize::try_from(int(1)).unwrap_or(0).min(10_000)),
        "replace" | "replacecs" => {
            let mut text = arg(0).to_owned();
            for pair in args[1.min(args.len())..].chunks(2) {
                if let [from, to] = pair {
                    text = replace(&text, from, to, name == "replacecs");
                }
            }
            text
        }
        "remove" | "removecs" => {
            let mut text = arg(0).to_owned();
            for from in &args[1.min(args.len())..] {
                text = replace(&text, from, "", name == "removecs");
            }
            text
        }
        "pos" | "poscs" => position(arg(0), arg(1), args.get(2).map(|_| int(2)), name == "poscs"),
        "count" | "countcs" => {
            let total: usize = args[1.min(args.len())..].iter().map(|sub| occurrences(arg(0), sub, name == "countcs").len()).sum();
            total.to_string()
        }
        "strip" => strip(arg(0)),
        "gettok" => tokens_range(&tokens(arg(0), int(2)), arg(1)).join(&separator(int(2))),
        "numtok" => tokens(arg(0), int(1)).len().to_string(),
        "addtok" => {
            let mut list = tokens(arg(0), int(2));
            if !list.iter().any(|token| token.eq_ignore_ascii_case(arg(1))) {
                list.push(arg(1).to_owned());
            }
            list.join(&separator(int(2)))
        }
        "deltok" => {
            let list = tokens(arg(0), int(2));
            let removed = token_indices(list.len(), arg(1));
            list.iter()
                .enumerate()
                .filter(|(index, _)| !removed.contains(index))
                .map(|(_, token)| token.as_str())
                .collect::<Vec<_>>()
                .join(&separator(int(2)))
        }
        "istok" | "istokcs" => {
            let found = tokens(arg(0), int(2)).iter().any(|token| {
                if name == "istokcs" { token == arg(1) } else { token.to_lowercase() == arg(1).to_lowercase() }
            });
            if found { "$true" } else { "$false" }.into()
        }
        "findtok" => {
            // $findtok(text, token, [N], C): position of the Nth match.
            let (wanted, separator_index) = if args.len() >= 4 { (int(2), 3) } else { (1, 2) };
            let list = tokens(arg(0), int(separator_index));
            let matches: Vec<usize> = list
                .iter()
                .enumerate()
                .filter(|(_, token)| token.to_lowercase() == arg(1).to_lowercase())
                .map(|(index, _)| index + 1)
                .collect();
            if wanted == 0 {
                matches.len().to_string()
            } else {
                usize::try_from(wanted - 1).ok().and_then(|n| matches.get(n)).map(ToString::to_string).unwrap_or_default()
            }
        }
        "puttok" | "instok" => {
            // $puttok(text, token, N, C) replaces, $instok inserts.
            let mut list = tokens(arg(0), int(3));
            let len = list.len();
            if let Some(index) = token_index(len + usize::from(name == "instok"), int(2)) {
                if name == "puttok" {
                    if let Some(slot) = list.get_mut(index) {
                        arg(1).clone_into(slot);
                    }
                } else {
                    list.insert(index.min(len), arg(1).to_owned());
                }
            }
            list.join(&separator(int(3)))
        }
        "calc" => calc(arg(0)).map(format_number).unwrap_or_default(),
        "round" => {
            let digits = i32::try_from(int(1).clamp(0, 12)).unwrap_or(0);
            let factor = 10f64.powi(digits);
            format_number((number(arg(0)) * factor).round() / factor)
        }
        "int" => format_number(number(arg(0)).trunc()),
        "abs" => format_number(number(arg(0)).abs()),
        "ceil" => format_number(number(arg(0)).ceil()),
        "floor" => format_number(number(arg(0)).floor()),
        "sqrt" => format_number(number(arg(0)).sqrt()),
        "ord" => {
            let n = int(0);
            let suffix = match (n % 10, n % 100) {
                (_, 11..=13) => "th",
                (1, _) => "st",
                (2, _) => "nd",
                (3, _) => "rd",
                _ => "th",
            };
            format!("{n}{suffix}")
        }
        _ => return None,
    };
    Some(value)
}

/// A number, `0` if the text is not one (as mIRC does).
pub fn number(text: &str) -> f64 {
    number_value(text).unwrap_or(0.0)
}

pub fn number_value(text: &str) -> Option<f64> {
    text.trim().parse::<f64>().ok().filter(|value| value.is_finite())
}

/// A number written without a useless fractional part.
pub fn format_number(value: f64) -> String {
    if !value.is_finite() {
        return "0".into();
    }
    if value.fract() == 0.0 && value.abs() < 1e15 {
        #[allow(clippy::cast_possible_truncation)]
        return (value as i64).to_string();
    }
    let text = format!("{value:.6}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// Wildcard match: `*` any sequence, `?` one character.
pub fn wildcard_match(pattern: &str, text: &str, _case_sensitive: bool) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, t));
            p += 1;
        } else if let Some((star_p, star_t)) = star {
            p = star_p + 1;
            t = star_t + 1;
            star = Some((star_p, star_t + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

fn left(text: &str, n: i64) -> String {
    let len = i64::try_from(text.chars().count()).unwrap_or(i64::MAX);
    let take = if n >= 0 { n.min(len) } else { (len + n).max(0) };
    text.chars().take(usize::try_from(take).unwrap_or(0)).collect()
}

fn right(text: &str, n: i64) -> String {
    let len = i64::try_from(text.chars().count()).unwrap_or(i64::MAX);
    let skip = if n >= 0 { (len - n).max(0) } else { (-n).min(len) };
    text.chars().skip(usize::try_from(skip).unwrap_or(0)).collect()
}

/// `$mid(text, start, [length])`: `start` from 1, negative from the end.
fn mid(text: &str, start: i64, length: Option<i64>) -> String {
    let chars: Vec<char> = text.chars().collect();
    let len = i64::try_from(chars.len()).unwrap_or(i64::MAX);
    let from = match start.cmp(&0) {
        std::cmp::Ordering::Greater => start - 1,
        std::cmp::Ordering::Less => (len + start).max(0),
        std::cmp::Ordering::Equal => 0,
    };
    let to = match length {
        Some(n) if n >= 0 => (from + n).min(len),
        Some(n) => (len + n).max(from),
        None => len,
    };
    if from >= len || to <= from {
        return String::new();
    }
    chars[usize::try_from(from).unwrap_or(0)..usize::try_from(to).unwrap_or(0)].iter().collect()
}

/// Byte offsets of the occurrences of `sub` in `text`.
fn occurrences(text: &str, sub: &str, case_sensitive: bool) -> Vec<usize> {
    if sub.is_empty() {
        return Vec::new();
    }
    let (haystack, needle) = if case_sensitive { (text.to_owned(), sub.to_owned()) } else { (text.to_lowercase(), sub.to_lowercase()) };
    haystack.match_indices(&needle).map(|(index, _)| index).collect()
}

fn replace(text: &str, from: &str, to: &str, case_sensitive: bool) -> String {
    if from.is_empty() {
        return text.to_owned();
    }
    if case_sensitive {
        return text.replace(from, to);
    }
    let lower = text.to_lowercase();
    if lower.len() != text.len() {
        // Lowercasing changed byte lengths: fall back to an exact replace.
        return text.replace(from, to);
    }
    let mut out = String::new();
    let mut last = 0;
    for (index, matched) in lower.match_indices(&from.to_lowercase()) {
        out.push_str(&text[last..index]);
        out.push_str(to);
        last = index + matched.len();
    }
    out.push_str(&text[last..]);
    out
}

/// `$pos(text, sub, [N])`: character position (from 1) of the Nth
/// occurrence, `0` if absent; `N = 0` gives the count.
fn position(text: &str, sub: &str, n: Option<i64>, case_sensitive: bool) -> String {
    let found = occurrences(text, sub, case_sensitive);
    match n {
        Some(0) => found.len().to_string(),
        n => {
            let wanted = usize::try_from(n.unwrap_or(1).max(1) - 1).unwrap_or(0);
            found
                .get(wanted)
                .map(|&byte| (text[..byte].chars().count() + 1).to_string())
                .unwrap_or_default()
        }
    }
}

/// Removes mIRC formatting codes.
fn strip(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x02' | '\x0f' | '\x16' | '\x1d' | '\x1f' | '\x1e' | '\x11' => {}
            '\x03' => {
                for _ in 0..2 {
                    if chars.peek().is_some_and(char::is_ascii_digit) {
                        chars.next();
                    }
                }
                if chars.peek() == Some(&',') {
                    let mut lookahead = chars.clone();
                    lookahead.next();
                    if lookahead.peek().is_some_and(char::is_ascii_digit) {
                        chars.next();
                        for _ in 0..2 {
                            if chars.peek().is_some_and(char::is_ascii_digit) {
                                chars.next();
                            }
                        }
                    }
                }
            }
            c => out.push(c),
        }
    }
    out
}

fn separator(code: i64) -> String {
    u32::try_from(code).ok().and_then(char::from_u32).map_or_else(|| " ".into(), String::from)
}

/// Tokens of `text` separated by the character of code `code` (empty
/// tokens are ignored, as in mIRC).
fn tokens(text: &str, code: i64) -> Vec<String> {
    let separator = separator(code);
    text.split(separator.as_str()).filter(|token| !token.is_empty()).map(str::to_owned).collect()
}

/// Index (from 0) of the token `n` (from 1, negative from the end).
fn token_index(len: usize, n: i64) -> Option<usize> {
    let len = i64::try_from(len).ok()?;
    let index = match n.cmp(&0) {
        std::cmp::Ordering::Greater => n - 1,
        std::cmp::Ordering::Less => len + n,
        std::cmp::Ordering::Equal => return None,
    };
    (0..len).contains(&index).then(|| usize::try_from(index).ok()).flatten()
}

/// Indices of `N`, `N-` or `N-M` (`0` alone: none).
fn token_indices(len: usize, spec: &str) -> Vec<usize> {
    let parse = |text: &str| text.trim().parse::<i64>().ok();
    match spec.split_once('-').filter(|(first, _)| !first.is_empty()) {
        Some((first, last)) => {
            let Some(from) = parse(first).and_then(|n| token_index(len, n)) else {
                return Vec::new();
            };
            let to = if last.trim().is_empty() { len.saturating_sub(1) } else { parse(last).and_then(|n| token_index(len, n)).unwrap_or(len.saturating_sub(1)) };
            (from..=to).collect()
        }
        None => parse(spec).and_then(|n| token_index(len, n)).into_iter().collect(),
    }
}

/// `$gettok`: the selected tokens, or the count for `0`.
fn tokens_range(list: &[String], spec: &str) -> Vec<String> {
    if spec.trim() == "0" {
        return vec![list.len().to_string()];
    }
    token_indices(list.len(), spec).into_iter().map(|index| list[index].clone()).collect()
}

/// `$calc`: `+ - * / % ^ //` and parentheses; division by zero gives 0.
pub fn calc(expression: &str) -> Option<f64> {
    let mut parser = Calc { chars: expression.chars().filter(|c| !c.is_whitespace()).collect(), pos: 0 };
    let value = parser.sum()?;
    (parser.pos == parser.chars.len()).then_some(value)
}

struct Calc {
    chars: Vec<char>,
    pos: usize,
}

impl Calc {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn sum(&mut self) -> Option<f64> {
        let mut value = self.product()?;
        while let Some(c @ ('+' | '-')) = self.peek() {
            self.pos += 1;
            let right = self.product()?;
            value = if c == '+' { value + right } else { value - right };
        }
        Some(value)
    }

    fn product(&mut self) -> Option<f64> {
        let mut value = self.power()?;
        loop {
            let floor = self.peek() == Some('/') && self.chars.get(self.pos + 1) == Some(&'/');
            match self.peek() {
                Some('*') => {
                    self.pos += 1;
                    value *= self.power()?;
                }
                Some('/') => {
                    self.pos += if floor { 2 } else { 1 };
                    let right = self.power()?;
                    value = if right == 0.0 { 0.0 } else if floor { (value / right).floor() } else { value / right };
                }
                Some('%') => {
                    self.pos += 1;
                    let right = self.power()?;
                    value = if right == 0.0 { 0.0 } else { value % right };
                }
                _ => return Some(value),
            }
        }
    }

    fn power(&mut self) -> Option<f64> {
        let base = self.unary()?;
        if self.peek() == Some('^') {
            self.pos += 1;
            return Some(base.powf(self.power()?));
        }
        Some(base)
    }

    fn unary(&mut self) -> Option<f64> {
        match self.peek()? {
            '-' => {
                self.pos += 1;
                Some(-self.unary()?)
            }
            '+' => {
                self.pos += 1;
                self.unary()
            }
            '(' => {
                self.pos += 1;
                let value = self.sum()?;
                (self.peek() == Some(')')).then(|| self.pos += 1)?;
                Some(value)
            }
            _ => {
                let start = self.pos;
                while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.') {
                    self.pos += 1;
                }
                self.chars[start..self.pos].iter().collect::<String>().parse().ok()
            }
        }
    }
}
