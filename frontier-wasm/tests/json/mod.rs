//! A small JSON reader for the vector files (no serde in this crate's
//! dependency set). Numbers are kept as their source text.

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, k: &str) -> &Json {
        match self {
            Json::Obj(v) => v
                .iter()
                .find(|(n, _)| n == k)
                .map(|(_, j)| j)
                .unwrap_or_else(|| panic!("no key {k}")),
            _ => panic!("not an object (looking for {k})"),
        }
    }
    pub fn str(&self) -> &str {
        match self {
            Json::Str(s) => s,
            other => panic!("not a string: {other:?}"),
        }
    }
    pub fn i64(&self) -> i64 {
        match self {
            Json::Num(s) => s.parse().unwrap_or_else(|_| panic!("not an i64: {s}")),
            other => panic!("not a number: {other:?}"),
        }
    }
    pub fn arr(&self) -> &[Json] {
        match self {
            Json::Arr(v) => v,
            other => panic!("not an array: {other:?}"),
        }
    }
}

pub fn parse(s: &str) -> Result<Json, String> {
    let b = s.as_bytes();
    let mut i = 0;
    let v = value(b, &mut i)?;
    ws(b, &mut i);
    if i != b.len() {
        return Err(format!("trailing data at {i}"));
    }
    Ok(v)
}

fn ws(b: &[u8], i: &mut usize) {
    while *i < b.len() && b[*i].is_ascii_whitespace() {
        *i += 1;
    }
}

fn value(b: &[u8], i: &mut usize) -> Result<Json, String> {
    ws(b, i);
    match b.get(*i) {
        Some(b'{') => {
            *i += 1;
            let mut out = Vec::new();
            ws(b, i);
            if b.get(*i) == Some(&b'}') {
                *i += 1;
                return Ok(Json::Obj(out));
            }
            loop {
                ws(b, i);
                let k = match value(b, i)? {
                    Json::Str(s) => s,
                    _ => return Err(format!("key at {i}")),
                };
                ws(b, i);
                if b.get(*i) != Some(&b':') {
                    return Err(format!("':' at {i}"));
                }
                *i += 1;
                out.push((k, value(b, i)?));
                ws(b, i);
                match b.get(*i) {
                    Some(b',') => *i += 1,
                    Some(b'}') => {
                        *i += 1;
                        return Ok(Json::Obj(out));
                    }
                    _ => return Err(format!("',' or '}}' at {i}")),
                }
            }
        }
        Some(b'[') => {
            *i += 1;
            let mut out = Vec::new();
            ws(b, i);
            if b.get(*i) == Some(&b']') {
                *i += 1;
                return Ok(Json::Arr(out));
            }
            loop {
                out.push(value(b, i)?);
                ws(b, i);
                match b.get(*i) {
                    Some(b',') => *i += 1,
                    Some(b']') => {
                        *i += 1;
                        return Ok(Json::Arr(out));
                    }
                    _ => return Err(format!("',' or ']' at {i}")),
                }
            }
        }
        Some(b'"') => {
            *i += 1;
            let mut s = String::new();
            while let Some(&c) = b.get(*i) {
                *i += 1;
                match c {
                    b'"' => return Ok(Json::Str(s)),
                    b'\\' => {
                        let e = *b.get(*i).ok_or("escape")?;
                        *i += 1;
                        match e {
                            b'n' => s.push('\n'),
                            b't' => s.push('\t'),
                            b'r' => s.push('\r'),
                            b'u' => {
                                let h = std::str::from_utf8(&b[*i..*i + 4])
                                    .map_err(|e| e.to_string())?;
                                let c = u32::from_str_radix(h, 16).map_err(|e| e.to_string())?;
                                s.push(char::from_u32(c).unwrap_or('?'));
                                *i += 4;
                            }
                            other => s.push(other as char),
                        }
                    }
                    _ => {
                        // Multi-byte UTF-8: copy the whole sequence.
                        let start = *i - 1;
                        let mut end = *i;
                        while end < b.len() && (b[end] & 0xC0) == 0x80 {
                            end += 1;
                        }
                        s.push_str(std::str::from_utf8(&b[start..end]).map_err(|e| e.to_string())?);
                        *i = end;
                    }
                }
            }
            Err("unterminated string".into())
        }
        Some(b't') if b[*i..].starts_with(b"true") => {
            *i += 4;
            Ok(Json::Bool(true))
        }
        Some(b'f') if b[*i..].starts_with(b"false") => {
            *i += 5;
            Ok(Json::Bool(false))
        }
        Some(b'n') if b[*i..].starts_with(b"null") => {
            *i += 4;
            Ok(Json::Null)
        }
        Some(c) if *c == b'-' || c.is_ascii_digit() => {
            let start = *i;
            while *i < b.len()
                && (b[*i] == b'-'
                    || b[*i] == b'+'
                    || b[*i] == b'.'
                    || b[*i] == b'e'
                    || b[*i] == b'E'
                    || b[*i].is_ascii_digit())
            {
                *i += 1;
            }
            Ok(Json::Num(
                String::from_utf8_lossy(&b[start..*i]).into_owned(),
            ))
        }
        _ => Err(format!("unexpected at {i}")),
    }
}
