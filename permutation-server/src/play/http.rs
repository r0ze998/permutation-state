//! The minimal HTTP/1.1 the play server needs: parse one request, answer it,
//! close. Routes build a `Response` value; only `write` touches the socket,
//! so no route ever writes to a client while it holds the game lock.
//!
//! Every response carries the same security headers (`SECURITY_HEADERS`):
//! no framing (a framed page could be clicked into acting), no referrer, no
//! content sniffing. There is no `script-src` policy: it can break the
//! scripts wallets inject into the page.

use serde_json::{json, Value};
use std::borrow::Cow;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, TcpStream};
use std::path::Path;
use std::time::Duration;

/// A client gets this long to send its request and to take the answer.
pub const IO_TIMEOUT: Duration = Duration::from_secs(10);
/// Request bodies larger than this are cut (orders and governance are small).
const MAX_BODY: usize = 1 << 20;
const MAX_HEADERS: usize = 64;

/// Sent with every response.
pub const SECURITY_HEADERS: [(&str, &str); 4] = [
    ("X-Frame-Options", "DENY"),
    ("Content-Security-Policy", "frame-ancestors 'none'"),
    ("Referrer-Policy", "no-referrer"),
    ("X-Content-Type-Options", "nosniff"),
];

pub struct Request {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    /// The query string as sent (without `?`).
    pub raw_query: String,
    /// Lower-cased names.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// The socket's peer (set by the server; `None` when parsed from bytes).
    pub peer: Option<IpAddr>,
}

impl Request {
    pub fn q(&self, k: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(a, _)| a == k)
            .map(|(_, b)| b.as_str())
    }

    pub fn qi<T: std::str::FromStr>(&self, k: &str) -> Option<T> {
        self.q(k)?.parse().ok()
    }

    pub fn header(&self, k: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(a, _)| a == k)
            .map(|(_, b)| b.as_str())
    }

    /// The body as JSON; `{}` if it is not JSON.
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(json!({}))
    }

    /// Parse one request from `input` (a socket, or bytes in tests).
    pub fn read(input: impl Read) -> Option<Request> {
        let mut reader = BufReader::new(input);
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        let mut parts = line.split_whitespace();
        let method = parts.next()?.to_string();
        let target = parts.next()?.to_string();
        let mut len = 0usize;
        let mut headers = Vec::new();
        loop {
            let mut h = String::new();
            if reader.read_line(&mut h).ok()? == 0 || h == "\r\n" || h == "\n" {
                break;
            }
            if let Some((k, v)) = h.split_once(':') {
                let (k, v) = (k.trim().to_ascii_lowercase(), v.trim().to_string());
                if k == "content-length" {
                    len = v.parse().unwrap_or(0);
                }
                if headers.len() < MAX_HEADERS {
                    headers.push((k, v));
                }
            }
        }
        let mut body = vec![0u8; len.min(MAX_BODY)];
        reader.read_exact(&mut body).ok()?;
        let (path, qs) = target.split_once('?').unwrap_or((&target, ""));
        let query = qs
            .split('&')
            .filter(|p| !p.is_empty())
            .map(|p| {
                let (a, b) = p.split_once('=').unwrap_or((p, ""));
                (a.to_string(), b.to_string())
            })
            .collect();
        Some(Request {
            method,
            path: path.to_string(),
            query,
            raw_query: qs.to_string(),
            headers,
            body,
            peer: None,
        })
    }
}

pub struct Response {
    /// Code and reason, e.g. `200 OK`.
    pub status: Cow<'static, str>,
    pub ctype: Cow<'static, str>,
    pub body: Vec<u8>,
    /// Headers besides the ones every response has (`head`).
    pub headers: Vec<(&'static str, String)>,
}

const JSON: &str = "application/json; charset=utf-8";

impl Response {
    pub fn new(
        status: impl Into<Cow<'static, str>>,
        ctype: impl Into<Cow<'static, str>>,
        body: Vec<u8>,
    ) -> Response {
        Response {
            status: status.into(),
            ctype: ctype.into(),
            body,
            headers: Vec::new(),
        }
    }

    /// `200 OK` with a JSON body.
    pub fn ok(v: &Value) -> Response {
        Response::json("200 OK", v)
    }

    pub fn json(status: &'static str, v: &Value) -> Response {
        Response::new(status, JSON, v.to_string().into_bytes())
    }

    /// `{"ok": false, "error": …}` with `status`.
    pub fn error(status: &'static str, error: impl Into<String>) -> Response {
        Response::json(status, &json!({"ok": false, "error": error.into()}))
    }

    pub fn text(status: &'static str, text: &str) -> Response {
        Response::new(status, "text/plain", text.as_bytes().to_vec())
    }

    /// A file of the web client under `web` (`/` is `index.html`).
    pub fn static_file(web: &Path, path: &str) -> Response {
        let rel = if path == "/" {
            "index.html"
        } else {
            path.trim_start_matches('/')
        };
        let rel = if rel.ends_with('/') {
            format!("{rel}index.html")
        } else {
            rel.to_string()
        };
        if rel.contains("..") {
            return Response::text("400 Bad Request", "bad path");
        }
        let file = web.join(&rel);
        match std::fs::read(&file) {
            Ok(body) => {
                let ctype = match file.extension().and_then(|e| e.to_str()) {
                    Some("html") => "text/html; charset=utf-8",
                    Some("css") => "text/css; charset=utf-8",
                    Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
                    Some("svg") => "image/svg+xml",
                    Some("png") => "image/png",
                    Some("json") => JSON,
                    Some("txt") | Some("md") => "text/plain; charset=utf-8",
                    _ => "application/octet-stream",
                };
                Response::new("200 OK", ctype, body)
            }
            Err(_) => Response::text("404 Not Found", "not found"),
        }
    }

    /// Status line and headers, up to and including the blank line.
    pub fn head(&self) -> String {
        let mut head = format!(
            "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: content-type, x-member-token\r\nConnection: close\r\n",
            self.status,
            self.ctype,
            self.body.len()
        );
        for (k, v) in SECURITY_HEADERS
            .iter()
            .map(|(k, v)| (*k, *v))
            .chain(self.headers.iter().map(|(k, v)| (*k, v.as_str())))
        {
            head.push_str(&format!("{k}: {v}\r\n"));
        }
        head.push_str("\r\n");
        head
    }

    pub fn write(&self, stream: &mut TcpStream) {
        let _ = stream.write_all(self.head().as_bytes());
        let _ = stream.write_all(&self.body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_method_path_query_headers_and_body() {
        let raw = b"POST /api/orders?member=3&civ=1&flag HTTP/1.1\r\nX-Member-Token: abc\r\nContent-Length: 11\r\n\r\n{\"a\": true}";
        let r = Request::read(&raw[..]).unwrap();
        assert_eq!(
            (r.method.as_str(), r.path.as_str()),
            ("POST", "/api/orders")
        );
        assert_eq!(r.qi::<u32>("member"), Some(3));
        assert_eq!(r.q("flag"), Some(""));
        assert_eq!(r.raw_query, "member=3&civ=1&flag");
        assert_eq!(r.header("x-member-token"), Some("abc"));
        assert_eq!(r.json()["a"], true);
    }

    #[test]
    fn a_short_body_is_no_request_and_a_bad_body_is_empty_json() {
        assert!(Request::read(&b"POST / HTTP/1.1\r\nContent-Length: 50\r\n\r\n{}"[..]).is_none());
        let r = Request::read(&b"POST / HTTP/1.1\r\nContent-Length: 3\r\n\r\nnot"[..]).unwrap();
        assert_eq!(r.json(), json!({}));
        assert!(Request::read(&b""[..]).is_none());
    }

    #[test]
    fn static_files_stay_inside_the_web_root() {
        let web = Path::new(env!("CARGO_MANIFEST_DIR")).join("web");
        assert_eq!(
            Response::static_file(&web, "/../Cargo.toml").status,
            "400 Bad Request"
        );
        assert_eq!(
            Response::static_file(&web, "/no-such-file").status,
            "404 Not Found"
        );
        let index = Response::static_file(&web, "/");
        assert_eq!(index.status, "200 OK");
        assert!(index.ctype.starts_with("text/html"));
    }

    #[test]
    fn every_response_forbids_framing_sniffing_and_referrers() {
        for r in [
            Response::ok(&json!({"ok": true})),
            Response::text("404 Not Found", "not found"),
            Response::error("503 Service Unavailable", "registering"),
        ] {
            let head = r.head();
            assert!(head.starts_with(&format!("HTTP/1.1 {}\r\n", r.status)));
            for (k, v) in SECURITY_HEADERS {
                assert!(head.contains(&format!("\r\n{k}: {v}\r\n")), "{k}: {head}");
            }
            assert!(!head.contains("x-seat-token"), "{head}");
            assert!(head.ends_with("\r\n\r\n"));
        }
    }
}
