//! Byte encodings the server and the verifier share: hex and base64.

/// Lower-case hex.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Hex to bytes; pairs that are not hex are skipped.
pub fn from_hex(h: &str) -> Vec<u8> {
    (0..h.len() / 2)
        .filter_map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).ok())
        .collect()
}

/// Standard base64 (with padding) decoder.
pub fn base64(s: &str) -> Result<Vec<u8>, String> {
    let val = |c: u8| -> Result<u32, String> {
        Ok(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            _ => return Err(format!("bad base64 byte {c}")),
        })
    };
    let bytes: Vec<u8> = s
        .bytes()
        .filter(|c| *c != b'=' && !c.is_ascii_whitespace())
        .collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            n |= val(*c)? << (18 - 6 * i);
        }
        let take = chunk.len().saturating_sub(1);
        for i in 0..take {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    Ok(out)
}

/// Standard base64 (with padding) encoder.
pub fn base64_encode(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                s.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_decodes_padding_variants() {
        assert_eq!(base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64("aGk=").unwrap(), b"hi");
        assert_eq!(base64("YWJj").unwrap(), b"abc");
        assert_eq!(base64("").unwrap(), b"");
        assert!(base64("a*==").is_err());
    }

    #[test]
    fn encodings_round_trip() {
        for len in 0..40 {
            let bytes: Vec<u8> = (0..len).map(|i| (i * 37 + 11) as u8).collect();
            assert_eq!(base64(&base64_encode(&bytes)).unwrap(), bytes);
            assert_eq!(from_hex(&hex(&bytes)), bytes);
        }
        assert_eq!(hex(&[0, 255, 16]), "00ff10");
    }
}
