//! The bots' control and metrics listener (M1 contract §10.3: bots control /
//! metrics on 41070; K11, W5-C): a minimal HTTP/1.1 server on loopback,
//! no dependency beyond tokio.
//!
//! | Route | Answer |
//! |---|---|
//! | `GET /metrics` | the fleet report so far (`Report::to_json`) |
//! | `GET /health` | `{"ok": true}` |
//! | `POST /stop` | `{"stopping": true}`; the fleet stops at its next step and writes its report |
//!
//! The port must be 41000–41999 and not reserved (or 0 in tests); only
//! `127.0.0.1` is accepted as the host.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Ports M1 never binds (§10.3).
pub const RESERVED_PORTS: [u16; 11] = [
    4185, 4190, 4191, 4194, 18899, 17799, 28899, 27799, 26699, 5185, 5191,
];

/// Whether `addr` (`127.0.0.1:<port>`) is a port the bots may listen on.
pub fn allowed(addr: &str) -> bool {
    let Some(("127.0.0.1", port)) = addr.rsplit_once(':') else {
        return false;
    };
    match port.parse::<u16>() {
        Ok(0) => true,
        Ok(p) => (41_000..=41_999).contains(&p) && !RESERVED_PORTS.contains(&p),
        Err(_) => false,
    }
}

/// Binds `addr` and serves until the process ends; returns the bound
/// address.
pub async fn serve(
    addr: &str,
    report: Arc<dyn Fn() -> Value + Send + Sync>,
    stop: Arc<AtomicBool>,
) -> std::io::Result<std::net::SocketAddr> {
    if !allowed(addr) {
        return Err(std::io::Error::other(format!(
            "{addr}: the control listener takes 127.0.0.1 and a port in 41000-41999 (not reserved)"
        )));
    }
    let l = TcpListener::bind(addr).await?;
    let local = l.local_addr()?;
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = l.accept().await else {
                continue;
            };
            let report = report.clone();
            let stop = stop.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let mut n = 0;
                while n < buf.len() {
                    match s.read(&mut buf[n..]).await {
                        Ok(0) | Err(_) => break,
                        Ok(k) => n += k,
                    }
                    if buf[..n].windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                // Read the body too (a small one): closing a socket with
                // unread bytes resets it before the answer is read.
                if let Some(end) = buf[..n].windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&buf[..end]).to_ascii_lowercase();
                    let len = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(0)
                        .min(buf.len() - end - 4);
                    while n < end + 4 + len {
                        match s.read(&mut buf[n..]).await {
                            Ok(0) | Err(_) => break,
                            Ok(k) => n += k,
                        }
                    }
                }
                let head = String::from_utf8_lossy(&buf[..n]);
                let mut line = head.lines().next().unwrap_or("").split(' ');
                let (method, path) = (line.next().unwrap_or(""), line.next().unwrap_or(""));
                let (status, body) = match (method, path) {
                    ("GET", "/metrics") => ("200 OK", report()),
                    ("GET", "/health") => ("200 OK", json!({"ok": true})),
                    ("POST", "/stop") => {
                        stop.store(true, Ordering::Relaxed);
                        ("200 OK", json!({"stopping": true}))
                    }
                    _ => ("404 Not Found", json!({"code": "NotFound"})),
                };
                let body = body.to_string();
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = s.write_all(resp.as_bytes()).await;
                let _ = s.shutdown().await;
            });
        }
    });
    Ok(local)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_follow_the_contract() {
        assert!(allowed("127.0.0.1:41070"));
        assert!(allowed("127.0.0.1:0"));
        for bad in [
            "127.0.0.1:4185",
            "127.0.0.1:40999",
            "0.0.0.0:41070",
            "127.0.0.1:x",
            "41070",
        ] {
            assert!(!allowed(bad), "{bad}");
        }
    }

    #[tokio::test]
    async fn metrics_health_and_stop() {
        let stop = Arc::new(AtomicBool::new(false));
        let a = serve("127.0.0.1:0", Arc::new(|| json!({"bots": 3})), stop.clone())
            .await
            .unwrap();
        let base = format!("http://{a}");
        let m = fclient::http::get(&format!("{base}/metrics"))
            .await
            .unwrap();
        assert_eq!(m.status, 200);
        assert_eq!(serde_json::from_slice::<Value>(&m.body).unwrap()["bots"], 3);
        assert_eq!(
            fclient::http::get(&format!("{base}/health"))
                .await
                .unwrap()
                .status,
            200
        );
        assert_eq!(
            fclient::http::get(&format!("{base}/nope"))
                .await
                .unwrap()
                .status,
            404
        );
        assert!(!stop.load(Ordering::Relaxed));
        let s = fclient::http::post_json(&format!("{base}/stop"), &json!({}))
            .await
            .unwrap();
        assert_eq!(s.status, 200);
        assert!(stop.load(Ordering::Relaxed));
    }
}
