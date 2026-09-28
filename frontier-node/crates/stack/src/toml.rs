//! The TOML subset the stack configs use: `[section]` headers, `key = value`
//! with strings, integers, floats and booleans, `#` comments. A key inside
//! a section is read as `section.key`. No dependency is added for it (the
//! keeper's `keeper.toml` parser is the same idea without sections).

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

impl Value {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

fn strip_comment(s: &str) -> &str {
    let mut in_str = false;
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_str = !in_str,
            '#' if !in_str => return &s[..i],
            _ => {}
        }
    }
    s
}

fn key_ok(k: &str) -> bool {
    !k.is_empty()
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Parses the subset; every key once.
pub fn parse(text: &str) -> Result<BTreeMap<String, Value>, String> {
    let mut out = BTreeMap::new();
    let mut section = String::new();
    for (n, raw) in text.lines().enumerate() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(s) = line.strip_prefix('[') {
            let s = s
                .strip_suffix(']')
                .ok_or(format!("line {}: bad section header", n + 1))?
                .trim();
            if !key_ok(s) {
                return Err(format!("line {}: bad section `{s}`", n + 1));
            }
            section = s.to_string();
            continue;
        }
        let (k, v) = line
            .split_once('=')
            .ok_or(format!("line {}: expected `key = value`", n + 1))?;
        let k = k.trim();
        if !key_ok(k) {
            return Err(format!("line {}: bad key `{k}`", n + 1));
        }
        let full = if section.is_empty() {
            k.to_string()
        } else {
            format!("{section}.{k}")
        };
        let v = value(v.trim()).map_err(|e| format!("line {}: {e}", n + 1))?;
        if out.insert(full.clone(), v).is_some() {
            return Err(format!("line {}: `{full}` given twice", n + 1));
        }
    }
    Ok(out)
}

fn value(v: &str) -> Result<Value, String> {
    if let Some(s) = v.strip_prefix('"') {
        let s = s.strip_suffix('"').ok_or("unclosed string")?;
        if s.contains('"') || s.contains('\\') {
            return Err("escapes are not supported".into());
        }
        return Ok(Value::Str(s.into()));
    }
    match v {
        "true" => return Ok(Value::Bool(true)),
        "false" => return Ok(Value::Bool(false)),
        _ => {}
    }
    let clean = v.replace('_', "");
    if let Ok(i) = clean.parse::<i64>() {
        return Ok(Value::Int(i));
    }
    if let Ok(f) = clean.parse::<f64>() {
        if f.is_finite() {
            return Ok(Value::Float(f));
        }
    }
    Err(format!("cannot parse `{v}`"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_comments_and_types() {
        let m = parse(
            "a = 1 # one\nb = \"x # not a comment\"\n\n[ports]\nherald = 40\nscale = 2.5\nok = true\n",
        )
        .unwrap();
        assert_eq!(m["a"], Value::Int(1));
        assert_eq!(m["b"], Value::Str("x # not a comment".into()));
        assert_eq!(m["ports.herald"], Value::Int(40));
        assert_eq!(m["ports.scale"], Value::Float(2.5));
        assert_eq!(m["ports.ok"], Value::Bool(true));
    }

    #[test]
    fn refuses_duplicates_and_junk() {
        assert!(parse("a = 1\na = 2").is_err());
        assert!(parse("[x]\na = 1\n[x]\na = 2").is_err());
        assert!(parse("a 1").is_err());
        assert!(parse("a = [1]").is_err());
        assert!(parse("a = \"x").is_err());
        assert!(parse("[bad section]").is_err());
    }
}
