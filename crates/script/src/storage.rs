//! Data kept by scripts: hash tables, ini files and text files. Files go
//! through the [`Host`], which decides where they live.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::builtins::{format_number, number};
use crate::interp::{Error, Host, Interpreter};

/// A hash table: items by lowercase name, keeping the name as given.
pub type Table = BTreeMap<String, (String, String)>;

impl Interpreter {
    /// Storage commands; `None` if `name` is not one.
    pub(crate) fn storage_command(&mut self, name: &str, args: &str, host: &mut dyn Host) -> Option<Result<(), Error>> {
        let (switches, args) = split_switches(args);
        let mut words = args.splitn(3, ' ');
        let mut word = || words.next().unwrap_or_default().to_owned();
        let result = match name {
            "hmake" => {
                self.tables.entry(word().to_lowercase()).or_default();
                Ok(())
            }
            "hfree" => {
                let pattern = word().to_lowercase();
                self.tables.retain(|table, _| !crate::builtins::wildcard_match(&pattern, table, false));
                Ok(())
            }
            "hadd" | "hinc" | "hdec" => {
                let (table, item, data) = (word(), word(), word());
                // `hadd` without the table: created on the fly (`-m`).
                let table = self.tables.entry(table.to_lowercase()).or_default();
                let key = item.to_lowercase();
                let value = if name == "hadd" {
                    data
                } else {
                    let step = if data.is_empty() { 1.0 } else { number(&data) };
                    let current = table.get(&key).map_or(0.0, |(_, value)| number(value));
                    format_number(if name == "hinc" { current + step } else { current - step })
                };
                // An existing item keeps the name it was added with.
                let item = table.get(&key).map_or(item, |(name, _)| name.clone());
                table.insert(key, (item, value));
                Ok(())
            }
            "hdel" => {
                let (table, item) = (word(), word());
                if let Some(table) = self.tables.get_mut(&table.to_lowercase()) {
                    let pattern = item.to_lowercase();
                    if switches.contains('w') {
                        table.retain(|key, _| !crate::builtins::wildcard_match(&pattern, key, false));
                    } else {
                        table.remove(&pattern);
                    }
                }
                Ok(())
            }
            "hsave" => {
                let (table, file) = (word(), word());
                let content = self
                    .tables
                    .get(&table.to_lowercase())
                    .map(|table| {
                        table.values().fold(String::new(), |mut text, (item, data)| {
                            let _ = writeln!(text, "{item}\n{data}");
                            text
                        })
                    })
                    .unwrap_or_default();
                host.write_file(&file, &content).map_err(Error::Runtime)
            }
            "hload" => {
                let (table, file) = (word(), word());
                match host.read_file(&file) {
                    Some(content) => {
                        let table = self.tables.entry(table.to_lowercase()).or_default();
                        let mut lines = content.lines();
                        while let (Some(item), Some(data)) = (lines.next(), lines.next()) {
                            table.insert(item.to_lowercase(), (item.to_owned(), data.to_owned()));
                        }
                        Ok(())
                    }
                    None => Err(Error::Runtime(format!("hload: cannot read {file}"))),
                }
            }
            "writeini" => {
                let (file, section, rest) = (word(), word(), word());
                let (item, value) = rest.split_once(' ').unwrap_or((&rest, ""));
                let mut ini = parse_ini(&host.read_file(&file).unwrap_or_default());
                ini_set(&mut ini, &section, item, value);
                host.write_file(&file, &format_ini(&ini)).map_err(Error::Runtime)
            }
            "remini" => {
                let (file, section, item) = (word(), word(), word());
                let mut ini = parse_ini(&host.read_file(&file).unwrap_or_default());
                if item.is_empty() {
                    ini.retain(|(name, _)| !name.eq_ignore_ascii_case(&section));
                } else if let Some((_, values)) = ini.iter_mut().find(|(name, _)| name.eq_ignore_ascii_case(&section)) {
                    values.retain(|(name, _)| !name.eq_ignore_ascii_case(&item));
                }
                host.write_file(&file, &format_ini(&ini)).map_err(Error::Runtime)
            }
            "write" => {
                let file = word();
                let text = [word(), word()].join(" ").trim().to_owned();
                if switches.contains('c') {
                    host.write_file(&file, if text.is_empty() { "" } else { &text }).map_err(Error::Runtime)
                } else {
                    let mut content = host.read_file(&file).unwrap_or_default();
                    if !content.is_empty() && !content.ends_with('\n') {
                        content.push('\n');
                    }
                    content.push_str(&text);
                    content.push('\n');
                    host.write_file(&file, &content).map_err(Error::Runtime)
                }
            }
            _ => return None,
        };
        Some(result)
    }

    /// Storage identifiers; `None` if `name` is not one.
    pub(crate) fn storage_identifier(
        &mut self,
        name: &str,
        args: &[String],
        property: Option<&str>,
        host: &mut dyn Host,
    ) -> Option<String> {
        let arg = |index: usize| args.get(index).map_or("", String::as_str);
        let value = match name {
            "hget" => {
                let table = self.tables.get(&arg(0).to_lowercase());
                match (table, args.len(), property) {
                    // `$hget(name)`: the table's name if it exists.
                    (table, 1, _) => table.map(|_| arg(0).to_owned()).unwrap_or_default(),
                    (Some(table), _, Some(property)) if property.eq_ignore_ascii_case("item") || property.eq_ignore_ascii_case("data") => {
                        let n = number(arg(1));
                        if n == 0.0 {
                            table.len().to_string()
                        } else {
                            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                            let index = n as usize;
                            table
                                .values()
                                .nth(index.saturating_sub(1))
                                .map(|(item, data)| if property.eq_ignore_ascii_case("item") { item.clone() } else { data.clone() })
                                .unwrap_or_default()
                        }
                    }
                    (Some(table), _, _) => table.get(&arg(1).to_lowercase()).map(|(_, data)| data.clone()).unwrap_or_default(),
                    (None, _, _) => String::new(),
                }
            }
            "readini" => {
                let ini = parse_ini(&host.read_file(arg(0)).unwrap_or_default());
                ini.iter()
                    .find(|(section, _)| section.eq_ignore_ascii_case(arg(1)))
                    .and_then(|(_, values)| values.iter().find(|(item, _)| item.eq_ignore_ascii_case(arg(2))))
                    .map(|(_, value)| value.clone())
                    .unwrap_or_default()
            }
            "read" => {
                let content = host.read_file(arg(0)).unwrap_or_default();
                let lines: Vec<&str> = content.lines().collect();
                if lines.is_empty() {
                    return Some(String::new());
                }
                let index = match args.get(1).map(|n| number(n)) {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    Some(n) if n >= 1.0 => (n as usize) - 1,
                    _ => {
                        let roll = self.roll(u64::try_from(lines.len()).unwrap_or(1));
                        usize::try_from(roll).unwrap_or(0)
                    }
                };
                lines.get(index).map(|line| (*line).to_owned()).unwrap_or_default()
            }
            "lines" => host.read_file(arg(0)).map_or_else(|| "0".into(), |content| content.lines().count().to_string()),
            "isfile" => if host.read_file(arg(0)).is_some() { "$true" } else { "$false" }.into(),
            _ => return None,
        };
        Some(value)
    }
}

/// `-switches` before the arguments.
fn split_switches(text: &str) -> (String, &str) {
    let text = text.trim_start();
    match text.strip_prefix('-') {
        Some(rest) if !rest.starts_with(' ') => {
            let (switches, rest) = rest.split_once(' ').unwrap_or((rest, ""));
            (switches.to_owned(), rest.trim_start())
        }
        _ => (String::new(), text),
    }
}

/// Sections in order, each with its items in order.
type Ini = Vec<(String, Vec<(String, String)>)>;

/// Sets `item` of `section`, adding them if needed.
fn ini_set(ini: &mut Ini, section: &str, item: &str, value: &str) {
    let index = if let Some(index) = ini.iter().position(|(name, _)| name.eq_ignore_ascii_case(section)) {
        index
    } else {
        ini.push((section.to_owned(), Vec::new()));
        ini.len() - 1
    };
    let values = &mut ini[index].1;
    match values.iter_mut().find(|(name, _)| name.eq_ignore_ascii_case(item)) {
        Some((_, slot)) => value.clone_into(slot),
        None => values.push((item.to_owned(), value.to_owned())),
    }
}

fn parse_ini(text: &str) -> Ini {
    let mut ini = Ini::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(section) = line.strip_prefix('[').and_then(|line| line.strip_suffix(']')) {
            ini.push((section.to_owned(), Vec::new()));
        } else if let (Some((item, value)), Some(last)) = (line.split_once('='), ini.last_mut()) {
            last.1.push((item.to_owned(), value.to_owned()));
        }
    }
    ini
}

fn format_ini(ini: &Ini) -> String {
    let mut text = String::new();
    for (section, values) in ini {
        if values.is_empty() {
            continue;
        }
        let _ = writeln!(text, "[{section}]");
        for (item, value) in values {
            let _ = writeln!(text, "{item}={value}");
        }
    }
    text
}

/// Global variables written as `vars.ini` lines (`n0=%name value`).
pub fn format_variables(globals: &BTreeMap<String, String>) -> String {
    let mut text = String::from("[variables]\n");
    for (index, (name, value)) in globals.iter().enumerate() {
        let _ = writeln!(text, "n{index}={name} {value}");
    }
    text
}

/// Global variables read from `vars.ini`.
pub fn parse_variables(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| line.split_once('=').map(|(_, rest)| rest))
        .filter(|rest| rest.starts_with('%'))
        .map(|rest| {
            let (name, value) = rest.split_once(' ').unwrap_or((rest, ""));
            (name.to_lowercase(), value.to_owned())
        })
        .collect()
}
