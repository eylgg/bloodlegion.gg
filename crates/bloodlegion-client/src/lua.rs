//! Reads a WoW SavedVariables file: top-level `Name = value` assignments, where a value is a
//! string, number, boolean, `nil`, or a table of `["key"] = value`, `[1] = value`, and bare
//! `value` entries, as the game writes them. Not a Lua parser: no expressions, no functions.

use std::collections::BTreeMap;

use anyhow::{Context, bail};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Nil,
    Bool(bool),
    Number(f64),
    String(String),
    /// Keyed by the string form of each key; bare entries count from 1, as in Lua.
    Table(BTreeMap<String, Value>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Table(entries) => entries.get(key),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_table(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Table(entries) => Some(entries),
            _ => None,
        }
    }
}

/// Every top-level assignment in the file, by name.
pub fn parse(source: &str) -> anyhow::Result<BTreeMap<String, Value>> {
    let mut parser = Parser {
        bytes: source.as_bytes(),
        at: 0,
    };
    let mut globals = BTreeMap::new();
    loop {
        parser.skip_space();
        if parser.at == parser.bytes.len() {
            return Ok(globals);
        }
        let name = parser.name()?;
        parser.skip_space();
        parser.expect(b'=')?;
        let value = parser.value()?;
        globals.insert(name, value);
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    /// Skips whitespace and `--` line comments.
    fn skip_space(&mut self) {
        loop {
            while self.peek().is_some_and(|b| b.is_ascii_whitespace()) {
                self.at += 1;
            }
            if self.bytes[self.at..].starts_with(b"--") {
                while self.peek().is_some_and(|b| b != b'\n') {
                    self.at += 1;
                }
            } else {
                return;
            }
        }
    }

    fn expect(&mut self, byte: u8) -> anyhow::Result<()> {
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            bail!("expected {:?} at byte {}", byte as char, self.at)
        }
    }

    fn name(&mut self) -> anyhow::Result<String> {
        let start = self.at;
        while self
            .peek()
            .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            self.at += 1;
        }
        if start == self.at {
            bail!("expected a name at byte {start}");
        }
        Ok(String::from_utf8_lossy(&self.bytes[start..self.at]).into_owned())
    }

    fn value(&mut self) -> anyhow::Result<Value> {
        self.skip_space();
        match self.peek() {
            Some(b'{') => self.table(),
            Some(b'"') => Ok(Value::String(self.string()?)),
            Some(b) if b == b'-' || b.is_ascii_digit() => Ok(Value::Number(self.number()?)),
            Some(_) => match self.name()?.as_str() {
                "true" => Ok(Value::Bool(true)),
                "false" => Ok(Value::Bool(false)),
                "nil" => Ok(Value::Nil),
                other => bail!("unexpected {other:?} at byte {}", self.at),
            },
            None => bail!("unexpected end of file"),
        }
    }

    fn number(&mut self) -> anyhow::Result<f64> {
        let start = self.at;
        while self
            .peek()
            .is_some_and(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'+' | b'.'))
        {
            self.at += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at])?;
        text.parse()
            .with_context(|| format!("not a number: {text:?} at byte {start}"))
    }

    /// A double-quoted string with Lua's escapes, as the game writes them.
    fn string(&mut self) -> anyhow::Result<String> {
        self.expect(b'"')?;
        let mut out = Vec::new();
        loop {
            let Some(byte) = self.peek() else {
                bail!("unterminated string");
            };
            self.at += 1;
            match byte {
                b'"' => return Ok(String::from_utf8_lossy(&out).into_owned()),
                b'\\' => {
                    let Some(escaped) = self.peek() else {
                        bail!("unterminated string");
                    };
                    self.at += 1;
                    match escaped {
                        b'n' | b'\n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'0'..=b'9' => {
                            // Up to three decimal digits: a byte.
                            let mut code = u32::from(escaped - b'0');
                            for _ in 0..2 {
                                match self.peek() {
                                    Some(d @ b'0'..=b'9') => {
                                        code = code * 10 + u32::from(d - b'0');
                                        self.at += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push(u8::try_from(code).context("escape out of range")?);
                        }
                        other => out.push(other),
                    }
                }
                other => out.push(other),
            }
        }
    }

    fn table(&mut self) -> anyhow::Result<Value> {
        self.expect(b'{')?;
        let mut entries = BTreeMap::new();
        let mut next_index = 1;
        loop {
            self.skip_space();
            match self.peek() {
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Value::Table(entries));
                }
                Some(b'[') => {
                    self.at += 1;
                    let key = match self.value()? {
                        Value::String(key) => key,
                        Value::Number(key) => key.to_string(),
                        other => bail!("unsupported table key {other:?}"),
                    };
                    self.skip_space();
                    self.expect(b']')?;
                    self.skip_space();
                    self.expect(b'=')?;
                    entries.insert(key, self.value()?);
                }
                Some(_) => {
                    entries.insert(next_index.to_string(), self.value()?);
                    next_index += 1;
                }
                None => bail!("unterminated table"),
            }
            self.skip_space();
            if matches!(self.peek(), Some(b',' | b';')) {
                self.at += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_the_game_writes() {
        let source = r#"
BloodLegionDB = {
["roster"] = {
["guild"] = "Blood Legion",
["members"] = {
["Garr Bonecrush"] = {
["level"] = 60,
["class"] = "Warrior",
["reached60"] = 1763608500,
},
["Zul \"Mojo\""] = {
["level"] = 37,
},
},
["scannedAt"] = 1763608600,
},
["flags"] = {
true, -- [1]
false, -- [2]
nil, -- [3]
-1.5, -- [4]
},
}
Other = "caf\195\169"
"#;
        let globals = parse(source).unwrap();
        let roster = globals["BloodLegionDB"].get("roster").unwrap();
        assert_eq!(roster.get("guild").unwrap().as_str(), Some("Blood Legion"));
        let members = roster.get("members").unwrap().as_table().unwrap();
        let garr = &members["Garr Bonecrush"];
        assert_eq!(garr.get("level").unwrap().as_f64(), Some(60.0));
        assert_eq!(garr.get("reached60").unwrap().as_f64(), Some(1763608500.0));
        assert!(members.contains_key("Zul \"Mojo\""));
        let flags = globals["BloodLegionDB"].get("flags").unwrap();
        assert_eq!(flags.get("1"), Some(&Value::Bool(true)));
        assert_eq!(flags.get("4").unwrap().as_f64(), Some(-1.5));
        assert_eq!(globals["Other"].as_str(), Some("café"));
    }

    #[test]
    fn says_where_it_breaks() {
        assert!(parse("A = {").is_err());
        assert!(parse("A = \"open").is_err());
        assert!(parse("A = function() end").is_err());
    }
}
