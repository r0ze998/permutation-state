//! A small HTTP/1.1 client over tokio for loopback services (localnet
//! JSON-RPC, drand-replay, the keeper API). Plain `http://` only: the
//! contract's expected dependency set has no TLS client, and every M1
//! service is on 127.0.0.1 (a TLS client for live drand or a public RPC is a
//! dependency request, see W1-F notes). One request per connection
//! (`Connection: close`); `Content-Length` and chunked bodies are read.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::ports::PortError;

pub const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// `http://host:port/path` → (host:port, path).
pub fn split_url(url: &str) -> Result<(String, String), PortError> {
    let rest = url.strip_prefix("http://").ok_or(PortError::Unsupported(
        "only http:// URLs (loopback services)",
    ))?;
    let (hp, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let hp = if hp.contains(':') {
        hp.to_string()
    } else {
        format!("{hp}:80")
    };
    Ok((hp, path.to_string()))
}

pub async fn get(url: &str) -> Result<Response, PortError> {
    request("GET", url, None).await
}

pub async fn post_json(url: &str, body: &serde_json::Value) -> Result<Response, PortError> {
    request(
        "POST",
        url,
        Some(serde_json::to_vec(body).map_err(|e| PortError::Decode(e.to_string()))?),
    )
    .await
}

pub async fn request(
    method: &str,
    url: &str,
    body: Option<Vec<u8>>,
) -> Result<Response, PortError> {
    tokio::time::timeout(TIMEOUT, request_inner(method, url, body, &[]))
        .await
        .map_err(|_| PortError::Io(format!("timeout: {method} {url}")))?
}

/// `GET` with extra request headers (names and values must be plain
/// printable ASCII without CR/LF; others are dropped).
pub async fn get_with_headers(url: &str, headers: &[(&str, &str)]) -> Result<Response, PortError> {
    tokio::time::timeout(TIMEOUT, request_inner("GET", url, None, headers))
        .await
        .map_err(|_| PortError::Io(format!("timeout: GET {url}")))?
}

async fn request_inner(
    method: &str,
    url: &str,
    body: Option<Vec<u8>>,
    headers: &[(&str, &str)],
) -> Result<Response, PortError> {
    let (hp, path) = split_url(url)?;
    let mut s = TcpStream::connect(&hp)
        .await
        .map_err(|e| PortError::Io(format!("{hp}: {e}")))?;
    let body = body.unwrap_or_default();
    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: {hp}\r\nConnection: close\r\nAccept: application/json\r\n");
    for (k, v) in headers {
        let name_ok = !k.is_empty() && k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
        if name_ok && v.bytes().all(|b| (0x20..0x7f).contains(&b)) {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
    }
    if method != "GET" {
        req.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes())
        .await
        .map_err(|e| PortError::Io(e.to_string()))?;
    if !body.is_empty() {
        s.write_all(&body)
            .await
            .map_err(|e| PortError::Io(e.to_string()))?;
    }
    let mut buf = Vec::with_capacity(4096);
    s.read_to_end(&mut buf)
        .await
        .map_err(|e| PortError::Io(e.to_string()))?;
    parse_response(&buf)
}

pub fn parse_response(buf: &[u8]) -> Result<Response, PortError> {
    let end = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| PortError::Decode("no header end".into()))?;
    let head =
        std::str::from_utf8(&buf[..end]).map_err(|_| PortError::Decode("header utf8".into()))?;
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    let status: u16 = status_line
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| PortError::Decode(format!("status line {status_line:?}")))?;
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| {
            l.split_once(':')
                .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        })
        .collect();
    let raw = &buf[end + 4..];
    let r = Response {
        status,
        headers,
        body: vec![],
    };
    let body = if r
        .header("transfer-encoding")
        .is_some_and(|v| v.eq_ignore_ascii_case("chunked"))
    {
        dechunk(raw)?
    } else if let Some(n) = r
        .header("content-length")
        .and_then(|v| v.parse::<usize>().ok())
    {
        raw.get(..n)
            .ok_or_else(|| PortError::Decode("short body".into()))?
            .to_vec()
    } else {
        raw.to_vec()
    };
    Ok(Response { body, ..r })
}

fn dechunk(mut raw: &[u8]) -> Result<Vec<u8>, PortError> {
    let mut out = vec![];
    loop {
        let e = raw
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or_else(|| PortError::Decode("chunk size".into()))?;
        let size_s =
            std::str::from_utf8(&raw[..e]).map_err(|_| PortError::Decode("chunk utf8".into()))?;
        let n = usize::from_str_radix(size_s.split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| PortError::Decode("chunk hex".into()))?;
        raw = &raw[e + 2..];
        if n == 0 {
            return Ok(out);
        }
        out.extend_from_slice(
            raw.get(..n)
                .ok_or_else(|| PortError::Decode("short chunk".into()))?,
        );
        raw = raw
            .get(n + 2..)
            .ok_or_else(|| PortError::Decode("chunk end".into()))?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_chunked() {
        let r = parse_response(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nhiXX").unwrap();
        assert_eq!((r.status, r.body.as_slice()), (200, &b"hi"[..]));
        let c = parse_response(b"HTTP/1.1 425 Too Early\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n").unwrap();
        assert_eq!((c.status, c.body.as_slice()), (425, &b"abcde"[..]));
        assert_eq!(
            split_url("http://127.0.0.1:41010").unwrap(),
            ("127.0.0.1:41010".into(), "/".into())
        );
        assert!(split_url("https://api.drand.sh").is_err());
    }
}
