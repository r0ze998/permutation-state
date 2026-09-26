//! `/gw/*`: the gateway's public listener behind this server's origin
//! (`--gateway-proxy`), so one HTTPS hostname serves the page and the
//! gateway (no CORS, no mixed content).
//!
//! It is its own client, not `ChainLink`: it never sends the operator token,
//! and of the browser's headers it passes on only the body's content type
//! and the x402 payment (`X-PAYMENT`, the browser's own signed transaction,
//! which `/x402/join` reads from that header). It adds the browser's address
//! as `X-Forwarded-For` (one address, `client_ip`, never the browser's own
//! header), so the gateway's limits per client are per browser, not one for
//! this whole server (the gateway trusts the header from a loopback peer by
//! default; not with `--no-trust-proxy`). Only the gateway's status,
//! content type, body and x402 receipt (`X-PAYMENT-RESPONSE`) come back.

use std::io::{Read, Write};
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use super::http::{Request, Response};
use crate::chainlink::{dechunk, parse_http_url};

/// Paths under this prefix are the gateway's.
pub const PREFIX: &str = "/gw";
/// Largest request body passed on (every gateway request is small).
pub const MAX_BODY: usize = 64 * 1024;
/// Largest `X-PAYMENT` header (a base64 transaction is about 2 KiB).
const MAX_PAYMENT: usize = 16 * 1024;
/// Largest answer taken from the gateway.
const MAX_ANSWER: u64 = 16 << 20;
/// A relayed transaction is answered once it is confirmed.
const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(90);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Whether `path` is one this proxy answers.
pub fn is_proxied(path: &str) -> bool {
    path == PREFIX
        || path
            .strip_prefix(PREFIX)
            .is_some_and(|r| r.starts_with('/'))
}

pub struct GatewayProxy {
    host: String,
    port: u16,
}

/// A header value safe to write on one line.
fn plain(v: &str) -> bool {
    v.bytes().all(|b| b == b'\t' || (0x20..0x7f).contains(&b))
}

fn refuse(status: &'static str, error: &str) -> Response {
    Response::error(status, error)
}

/// The browser's address: the socket's peer or, when that is this machine
/// (a reverse proxy or tunnel in front of this server), the last address
/// that proxy put in `X-Forwarded-For` (earlier ones are the client's say).
pub fn client_ip(req: &Request) -> Option<IpAddr> {
    let peer = req.peer?;
    let forwarded = req
        .header("x-forwarded-for")
        .and_then(|v| v.rsplit(',').next())
        .and_then(|a| a.trim().parse().ok());
    Some(match forwarded {
        Some(ip) if peer.is_loopback() => ip,
        _ => peer,
    })
}

impl GatewayProxy {
    /// `url`: the gateway's public listener, e.g. `http://127.0.0.1:4194`.
    pub fn new(url: &str) -> Result<GatewayProxy, String> {
        let (host, port) = parse_http_url(url).map_err(|e| format!("--gateway-proxy: {e}"))?;
        Ok(GatewayProxy { host, port })
    }

    pub fn url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }

    /// Answer a `/gw/<path>?<query>` request with the gateway's
    /// `<path>?<query>` answer.
    pub fn forward(&self, req: &Request) -> Response {
        if !matches!(req.method.as_str(), "GET" | "POST" | "OPTIONS") {
            return refuse("405 Method Not Allowed", "GET, POST or OPTIONS");
        }
        let path = match req.path.strip_prefix(PREFIX) {
            Some("") => "/",
            Some(p) if p.starts_with('/') => p,
            _ => return refuse("404 Not Found", "not a gateway path"),
        };
        let safe_path = path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/-_.~%".contains(&b))
            && !path.split('/').any(|seg| seg == "..");
        if !safe_path || !plain(&req.raw_query) || req.raw_query.contains(' ') {
            return refuse("400 Bad Request", "bad gateway path");
        }
        let declared: usize = req
            .header("content-length")
            .and_then(|l| l.parse().ok())
            .unwrap_or(0);
        if declared > MAX_BODY || req.body.len() > MAX_BODY {
            return refuse("413 Payload Too Large", "request body too large");
        }
        let body: &[u8] = if req.method == "POST" { &req.body } else { &[] };
        let mut head = format!(
            "{} {path}{}{} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\nContent-Length: {}\r\n",
            req.method,
            if req.raw_query.is_empty() { "" } else { "?" },
            req.raw_query,
            self.host,
            self.port,
            body.len()
        );
        // The only headers of the browser's that go on: nothing that could
        // carry a credential of this server or of a member.
        for (name, max) in [("content-type", 256), ("x-payment", MAX_PAYMENT)] {
            if let Some(v) = req.header(name) {
                if v.len() > max || !plain(v) {
                    return refuse("400 Bad Request", "bad header");
                }
                head.push_str(&format!("{name}: {v}\r\n"));
            }
        }
        if let Some(ip) = client_ip(req) {
            head.push_str(&format!("X-Forwarded-For: {ip}\r\n"));
        }
        head.push_str("\r\n");
        match self.exchange(head.as_bytes(), body) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("/gw{path}: gateway unreachable ({e})");
                refuse("502 Bad Gateway", "gateway unreachable")
            }
        }
    }

    fn exchange(&self, head: &[u8], body: &[u8]) -> Result<Response, String> {
        let addr = (self.host.as_str(), self.port)
            .to_socket_addrs()
            .map_err(|e| e.to_string())?
            .next()
            .ok_or("no address")?;
        let mut s =
            TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).map_err(|e| e.to_string())?;
        s.set_read_timeout(Some(UPSTREAM_TIMEOUT)).ok();
        s.set_write_timeout(Some(UPSTREAM_TIMEOUT)).ok();
        s.write_all(head).map_err(|e| e.to_string())?;
        s.write_all(body).map_err(|e| e.to_string())?;
        let mut raw = Vec::new();
        s.take(MAX_ANSWER)
            .read_to_end(&mut raw)
            .map_err(|e| e.to_string())?;
        parse_answer(&raw)
    }
}

/// Status, content type, body and x402 receipt of a gateway's answer.
fn parse_answer(raw: &[u8]) -> Result<Response, String> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("bad HTTP answer")?;
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let mut lines = head.lines();
    let status_line = lines.next().unwrap_or("");
    let mut parts = status_line.splitn(3, ' ');
    let _version = parts.next();
    let code: u16 = parts
        .next()
        .and_then(|c| c.parse().ok())
        .filter(|c| (100..600).contains(c))
        .ok_or("bad status")?;
    let reason = parts.next().unwrap_or("").trim();
    let reason = if reason.is_empty() || !plain(reason) {
        "Gateway"
    } else {
        reason
    };
    let mut ctype = String::from("application/octet-stream");
    let mut chunked = false;
    let mut receipt = None;
    for l in lines {
        let Some((k, v)) = l.split_once(':') else {
            continue;
        };
        let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
        match k.as_str() {
            "content-type" if plain(v) => ctype = v.to_string(),
            "transfer-encoding" => chunked = v.to_ascii_lowercase().contains("chunked"),
            "x-payment-response" if plain(v) => receipt = Some(v.to_string()),
            _ => {}
        }
    }
    let body = if chunked {
        dechunk(&raw[split + 4..])
    } else {
        raw[split + 4..].to_vec()
    };
    let mut r = Response::new(format!("{code} {reason}"), ctype, body);
    if let Some(v) = receipt {
        r.headers.push(("X-PAYMENT-RESPONSE", v));
    }
    Ok(r)
}

/// Tests: a stand-in for the gateway.
#[cfg(test)]
pub mod fake {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::time::Duration;

    /// A gateway on a random port that answers `answer` once and hands
    /// back the request it got.
    pub fn fake_gateway(answer: &'static str) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).ok();
            let mut got = Vec::new();
            let mut buf = [0u8; 4096];
            // Read the head, then the body its Content-Length announces.
            loop {
                let n = s.read(&mut buf).unwrap_or(0);
                got.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&got).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let len: usize = text[..end]
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if got.len() >= end + 4 + len {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            s.write_all(answer.as_bytes()).unwrap();
            tx.send(String::from_utf8_lossy(&got).to_string()).unwrap();
        });
        (url, rx)
    }
}

#[cfg(test)]
mod tests {
    use super::fake::fake_gateway;
    use super::*;

    fn request(raw: &str) -> Request {
        Request::read(raw.as_bytes()).unwrap()
    }

    #[test]
    fn forwards_path_query_body_and_payment_but_no_credentials() {
        let (url, got) = fake_gateway(
            "HTTP/1.1 402 Payment Required\r\nContent-Type: application/json\r\nX-PAYMENT-RESPONSE: cmVjZWlwdA==\r\nSet-Cookie: s=1\r\nTransfer-Encoding: chunked\r\n\r\n7\r\n{\"x402\"\r\n4\r\n: 1}\r\n0\r\n\r\n",
        );
        let proxy = GatewayProxy::new(&url).unwrap();
        let body = r#"{"civ":1}"#;
        let r = proxy.forward(&request(&format!(
            "POST /gw/x402/join?civ=1&x=a%20b HTTP/1.1\r\nHost: play.example\r\nAuthorization: Bearer operator-secret\r\nX-Member-Token: member-secret\r\nX-Seat-Token: seat-secret\r\nCookie: c=1\r\nX-Forwarded-For: 10.0.0.1\r\nContent-Type: application/json\r\nX-PAYMENT: eyJ0eCI6MX0=\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )));
        let sent = got.recv().unwrap();
        let lower = sent.to_ascii_lowercase();
        assert!(
            sent.starts_with("POST /x402/join?civ=1&x=a%20b HTTP/1.1\r\n"),
            "{sent}"
        );
        for secret in [
            "authorization",
            "bearer",
            "operator-secret",
            "x-member-token",
            "member-secret",
            "seat",
            "cookie",
            "x-forwarded-for",
            "play.example",
        ] {
            assert!(!lower.contains(secret), "{secret} leaked: {sent}");
        }
        assert!(
            lower.contains("content-type: application/json\r\n"),
            "{sent}"
        );
        assert!(sent.contains("x-payment: eyJ0eCI6MX0=\r\n"), "{sent}");
        assert!(sent.ends_with(&format!("\r\n\r\n{body}")), "{sent}");
        assert_eq!(r.status, "402 Payment Required");
        assert_eq!(r.ctype, "application/json");
        assert_eq!(r.body, br#"{"x402": 1}"#);
        assert_eq!(
            r.headers,
            vec![("X-PAYMENT-RESPONSE", "cmVjZWlwdA==".to_string())]
        );
        assert!(!r.head().to_ascii_lowercase().contains("set-cookie"));
    }

    /// The gateway limits per client: it learns the browser's address, and
    /// only a proxy on this machine may say what it is.
    #[test]
    fn the_browsers_address_goes_along() {
        let (url, got) = fake_gateway("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
        let mut req = request(
            "POST /gw/faucet HTTP/1.1\r\nX-Forwarded-For: 6.6.6.6\r\nContent-Length: 0\r\n\r\n",
        );
        req.peer = Some("203.0.113.7".parse().unwrap());
        GatewayProxy::new(&url).unwrap().forward(&req);
        let sent = got.recv().unwrap();
        assert!(
            sent.contains("X-Forwarded-For: 203.0.113.7\r\n") && !sent.contains("6.6.6.6"),
            "{sent}"
        );
        let behind = |xff: &str| {
            let mut r = request(&format!(
                "GET /gw/tick HTTP/1.1\r\nX-Forwarded-For: {xff}\r\n\r\n"
            ));
            r.peer = Some("127.0.0.1".parse().unwrap());
            client_ip(&r).map(|ip| ip.to_string())
        };
        assert_eq!(
            behind("6.6.6.6, 198.51.100.4").as_deref(),
            Some("198.51.100.4")
        );
        assert_eq!(behind("junk").as_deref(), Some("127.0.0.1"));
        assert_eq!(client_ip(&request("GET / HTTP/1.1\r\n\r\n")), None);
    }

    #[test]
    fn a_get_goes_out_without_a_body() {
        let (url, got) = fake_gateway(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}",
        );
        let r = GatewayProxy::new(&url).unwrap().forward(&request(
            "GET /gw/season HTTP/1.1\r\nAuthorization: Bearer t\r\n\r\n",
        ));
        let sent = got.recv().unwrap();
        assert!(sent.starts_with("GET /season HTTP/1.1\r\n"), "{sent}");
        assert!(
            sent.contains("Content-Length: 0\r\n")
                && !sent.to_ascii_lowercase().contains("authorization")
        );
        assert_eq!(
            (r.status.as_ref(), r.body.as_slice()),
            ("200 OK", &b"{}"[..])
        );
    }

    #[test]
    fn refuses_odd_requests_without_calling_the_gateway() {
        // Nothing listens here: any request that went out would be a 502.
        let proxy = GatewayProxy::new("http://127.0.0.1:9").unwrap();
        let status = |raw: &str| proxy.forward(&request(raw)).status.to_string();
        assert_eq!(
            status("PUT /gw/season HTTP/1.1\r\n\r\n"),
            "405 Method Not Allowed"
        );
        assert_eq!(
            status("GET /gw/../operator/roster HTTP/1.1\r\n\r\n"),
            "400 Bad Request"
        );
        assert_eq!(status("GET /gw/a\"b HTTP/1.1\r\n\r\n"), "400 Bad Request");
        assert_eq!(
            status(&format!(
                "POST /gw/relay HTTP/1.1\r\nContent-Length: {}\r\n\r\n{}",
                MAX_BODY + 1,
                "x".repeat(MAX_BODY + 1)
            )),
            "413 Payload Too Large"
        );
        assert_eq!(
            status(
                "POST /gw/relay HTTP/1.1\r\nX-PAYMENT: a\rInjected: 1\r\nContent-Length: 0\r\n\r\n"
            ),
            "400 Bad Request"
        );
        assert_eq!(status("GET /gw/season HTTP/1.1\r\n\r\n"), "502 Bad Gateway");
        assert!(
            is_proxied("/gw")
                && is_proxied("/gw/tick")
                && !is_proxied("/gwx")
                && !is_proxied("/api/gw")
        );
    }
}
