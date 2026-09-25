//! The minimal HTTP/1.1 the play server needs: parse one request, answer it,
//! close. Routes build a `Response` value; only `write` touches the socket,
//! so no route ever writes to a client while it holds the game lock.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

/// A client gets this long to send its request and to take the answer.
pub const IO_TIMEOUT: Duration = Duration::from_secs(10);
/// Request bodies larger than this are cut (orders and governance are small).
const MAX_BODY: usize = 1 << 20;
const MAX_HEADERS: usize = 64;

pub struct Request {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    /// Lower-cased names.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
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
            headers,
            body,
        })
    }
}

pub struct Response {
    pub status: &'static str,
    pub ctype: &'static str,
    pub body: Vec<u8>,
}

const JSON: &str = "application/json; charset=utf-8";

impl Response {
    /// `200 OK` with a JSON body.
    pub fn ok(v: &Value) -> Response {
        Response::json("200 OK", v)
    }

    pub fn json(status: &'static str, v: &Value) -> Response {
        Response {
            status,
            ctype: JSON,
            body: v.to_string().into_bytes(),
        }
    }

    /// `{"ok": false, "error": …}` with `status`.
    pub fn error(status: &'static str, error: impl Into<String>) -> Response {
        Response::json(status, &json!({"ok": false, "error": error.into()}))
    }

    pub fn text(status: &'static str, text: &str) -> Response {
        Response {
            status,
            ctype: "text/plain",
            body: text.as_bytes().to_vec(),
        }
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
                Response {
                    status: "200 OK",
                    ctype,
                    body,
                }
            }
            Err(_) => Response::text("404 Not Found", "not found"),
        }
    }

    pub fn write(&self, stream: &mut TcpStream) {
        let head = format!(
            "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: content-type, x-member-token, x-seat-token\r\nConnection: close\r\n\r\n",
            self.status,
            self.ctype,
            self.body.len()
        );
        let _ = stream.write_all(head.as_bytes());
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
}
