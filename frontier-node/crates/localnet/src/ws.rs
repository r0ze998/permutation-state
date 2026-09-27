//! The WebSocket side of the RPC (§8.7, optional in the contract; every
//! component also works by polling): `slotSubscribe`, `signatureSubscribe`,
//! `logsSubscribe` (`"all"` or `{"mentions": [key]}`) and their
//! `…Unsubscribe`, with Agave's notification shapes, so `@solana/web3.js`
//! 1.99's `confirmTransaction` and `onLogs` work against the node.
//!
//! RFC 6455 is implemented here directly (handshake with SHA-1, text,
//! ping/pong and close frames, client masking, fragmented messages) rather
//! than through a WebSocket crate: the workspace's `axum` is built without
//! its `ws` feature and the contract's expected dependency set has none.
//! The listener binds 127.0.0.1 only, on its own port (the RPC port + 1 by
//! default, as web3.js assumes).

use std::collections::HashMap;

use base64::Engine;
use serde_json::{json, Value};
use solana_address::Address;
use solana_signature::Signature;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};

use crate::chain::Event;
use crate::server::{lock, Shared};

const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
/// Largest message a client may send.
const MAX_MESSAGE: usize = 1 << 20;

/// SHA-1 (RFC 3174), for the handshake only.
pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut msg = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[4 * i..4 * i + 4].try_into().expect("4"));
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let t = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for (i, x) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&x.to_be_bytes());
    }
    out
}

/// `Sec-WebSocket-Accept` for a client key.
pub fn accept_key(key: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(sha1(format!("{key}{GUID}").as_bytes()))
}

/// A server frame (never masked).
pub fn frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut f = vec![0x80 | opcode];
    let n = payload.len();
    if n < 126 {
        f.push(n as u8);
    } else if n <= 0xFFFF {
        f.push(126);
        f.extend_from_slice(&(n as u16).to_be_bytes());
    } else {
        f.push(127);
        f.extend_from_slice(&(n as u64).to_be_bytes());
    }
    f.extend_from_slice(payload);
    f
}

enum Incoming {
    Text(String),
    Ping(Vec<u8>),
    Close,
}

async fn read_frame(r: &mut OwnedReadHalf) -> std::io::Result<(bool, u8, Vec<u8>)> {
    let mut h = [0u8; 2];
    r.read_exact(&mut h).await?;
    let fin = h[0] & 0x80 != 0;
    let opcode = h[0] & 0x0F;
    let masked = h[1] & 0x80 != 0;
    let mut len = (h[1] & 0x7F) as u64;
    if len == 126 {
        let mut b = [0u8; 2];
        r.read_exact(&mut b).await?;
        len = u16::from_be_bytes(b) as u64;
    } else if len == 127 {
        let mut b = [0u8; 8];
        r.read_exact(&mut b).await?;
        len = u64::from_be_bytes(b);
    }
    if len as usize > MAX_MESSAGE {
        return Err(std::io::Error::other("frame too large"));
    }
    let mut mask = [0u8; 4];
    if masked {
        r.read_exact(&mut mask).await?;
    }
    let mut p = vec![0u8; len as usize];
    r.read_exact(&mut p).await?;
    if masked {
        for (i, b) in p.iter_mut().enumerate() {
            *b ^= mask[i % 4];
        }
    }
    Ok((fin, opcode, p))
}

async fn reader(mut r: OwnedReadHalf, tx: mpsc::Sender<Incoming>) {
    let mut msg: Vec<u8> = vec![];
    loop {
        let Ok((fin, op, p)) = read_frame(&mut r).await else {
            let _ = tx.send(Incoming::Close).await;
            return;
        };
        let m = match op {
            0x0..=0x2 => {
                msg.extend_from_slice(&p);
                if msg.len() > MAX_MESSAGE {
                    let _ = tx.send(Incoming::Close).await;
                    return;
                }
                if !fin {
                    continue;
                }
                let text = String::from_utf8_lossy(&msg).into_owned();
                msg.clear();
                Incoming::Text(text)
            }
            0x8 => Incoming::Close,
            0x9 => Incoming::Ping(p),
            _ => continue,
        };
        let close = matches!(m, Incoming::Close);
        if tx.send(m).await.is_err() || close {
            return;
        }
    }
}

enum Sub {
    Slot,
    Signature(Signature),
    Logs(Option<Address>),
}

struct Conn {
    w: OwnedWriteHalf,
    subs: HashMap<u64, Sub>,
    next: u64,
}

impl Conn {
    async fn send(&mut self, v: &Value) -> std::io::Result<()> {
        self.w
            .write_all(&frame(0x1, v.to_string().as_bytes()))
            .await
    }

    async fn notify(&mut self, method: &str, id: u64, result: Value) -> std::io::Result<()> {
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": {"result": result, "subscription": id}}))
            .await
    }
}

fn sig_result(slot: u64, err: &Value) -> Value {
    json!({"context": {"slot": slot}, "value": {"err": err}})
}

async fn handle_text(state: &Shared, c: &mut Conn, text: &str) -> std::io::Result<()> {
    let Ok(req) = serde_json::from_str::<Value>(text) else {
        return c
            .send(&json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "Parse error"}}))
            .await;
    };
    let Some(id) = req.get("id").cloned() else {
        // A notification (web3.js sends `ping` ones): no answer.
        return Ok(());
    };
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let p = req.get("params").cloned().unwrap_or(json!([]));
    let answer = |result: Value| json!({"jsonrpc": "2.0", "id": id, "result": result});
    let error = |code: i64, m: &str| json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": m}});
    match method {
        "slotSubscribe" => {
            let n = c.next;
            c.next += 1;
            c.subs.insert(n, Sub::Slot);
            c.send(&answer(json!(n))).await
        }
        "signatureSubscribe" => {
            let Some(sig) = p
                .get(0)
                .and_then(|s| s.as_str())
                .and_then(|s| s.parse::<Signature>().ok())
            else {
                return c.send(&error(-32602, "Invalid signature")).await;
            };
            let n = c.next;
            c.next += 1;
            c.send(&answer(json!(n))).await?;
            // Already landed: notify at once (a subscription is one-shot).
            let seen = {
                let ch = lock(state);
                ch.transaction(&sig).map(|l| (l.slot, l.err_json.clone()))
            };
            match seen {
                Some((slot, err)) => {
                    c.notify("signatureNotification", n, sig_result(slot, &err))
                        .await
                }
                None => {
                    c.subs.insert(n, Sub::Signature(sig));
                    Ok(())
                }
            }
        }
        "logsSubscribe" => {
            let f = p.get(0).cloned().unwrap_or(json!("all"));
            let filter = if let Some(m) = f.get("mentions").and_then(|m| m.as_array()) {
                match m
                    .first()
                    .and_then(|k| k.as_str())
                    .and_then(|k| k.parse::<Address>().ok())
                {
                    Some(k) if m.len() == 1 => Some(k),
                    _ => {
                        return c
                            .send(&error(-32602, "Invalid Request: Only 1 address supported"))
                            .await
                    }
                }
            } else if f
                .as_str()
                .is_some_and(|s| s == "all" || s == "allWithVotes")
            {
                None
            } else {
                return c.send(&error(-32602, "Invalid logs filter")).await;
            };
            let n = c.next;
            c.next += 1;
            c.subs.insert(n, Sub::Logs(filter));
            c.send(&answer(json!(n))).await
        }
        "slotUnsubscribe" | "signatureUnsubscribe" | "logsUnsubscribe" => {
            let n = p.get(0).and_then(|x| x.as_u64()).unwrap_or(u64::MAX);
            let ok = c.subs.remove(&n).is_some();
            if ok {
                c.send(&answer(json!(true))).await
            } else {
                c.send(&error(-32602, "Invalid subscription id.")).await
            }
        }
        m => {
            c.send(&error(-32601, &format!("Method not found: {m}")))
                .await
        }
    }
}

async fn on_event(c: &mut Conn, ev: &Event) -> std::io::Result<()> {
    match ev {
        Event::Slot(s) => {
            let ids: Vec<u64> = c
                .subs
                .iter()
                .filter(|(_, x)| matches!(x, Sub::Slot))
                .map(|(i, _)| *i)
                .collect();
            for i in ids {
                c.notify(
                    "slotNotification",
                    i,
                    json!({"parent": s.saturating_sub(1), "root": s, "slot": s}),
                )
                .await?;
            }
        }
        Event::Tx {
            slot,
            signature,
            err,
            logs,
            keys,
        } => {
            let mut done = vec![];
            let mut out: Vec<(u64, &str, Value)> = vec![];
            for (i, s) in &c.subs {
                match s {
                    Sub::Signature(x) if x == signature => {
                        out.push((*i, "signatureNotification", sig_result(*slot, err)));
                        done.push(*i);
                    }
                    Sub::Logs(f) if f.is_none_or(|k| keys.contains(&k)) => out.push((
                        *i,
                        "logsNotification",
                        json!({"context": {"slot": slot}, "value": {"signature": signature.to_string(), "err": err, "logs": logs}}),
                    )),
                    _ => {}
                }
            }
            for i in done {
                c.subs.remove(&i);
            }
            out.sort_by_key(|x| x.0);
            for (i, m, r) in out {
                c.notify(m, i, r).await?;
            }
        }
    }
    Ok(())
}

async fn serve_conn(state: Shared, s: TcpStream) {
    let (mut r, mut w) = s.into_split();
    // Handshake.
    let mut head = vec![];
    let mut b = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() > 16_384 || r.read_exact(&mut b).await.is_err() {
            return;
        }
        head.push(b[0]);
    }
    let text = String::from_utf8_lossy(&head).into_owned();
    let key = text.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        k.trim()
            .eq_ignore_ascii_case("sec-websocket-key")
            .then(|| v.trim().to_string())
    });
    let upgrade = text.lines().any(|l| {
        l.to_ascii_lowercase().starts_with("upgrade:")
            && l.to_ascii_lowercase().contains("websocket")
    });
    let Some(key) = key.filter(|_| upgrade) else {
        let _ = w
            .write_all(
                b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await;
        return;
    };
    let resp = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
        accept_key(&key)
    );
    if w.write_all(resp.as_bytes()).await.is_err() {
        return;
    }
    let mut events = lock(&state).subscribe();
    let (tx, mut rx) = mpsc::channel(64);
    let rd = tokio::spawn(reader(r, tx));
    let mut c = Conn {
        w,
        subs: HashMap::new(),
        next: 1,
    };
    loop {
        let ok = tokio::select! {
            m = rx.recv() => match m {
                Some(Incoming::Text(t)) => handle_text(&state, &mut c, &t).await.is_ok(),
                Some(Incoming::Ping(p)) => c.w.write_all(&frame(0xA, &p)).await.is_ok(),
                Some(Incoming::Close) | None => {
                    let _ = c.w.write_all(&frame(0x8, &[])).await;
                    false
                }
            },
            e = events.recv() => match e {
                Ok(ev) => on_event(&mut c, &ev).await.is_ok(),
                Err(broadcast::error::RecvError::Lagged(_)) => true,
                Err(broadcast::error::RecvError::Closed) => false,
            },
        };
        if !ok {
            break;
        }
    }
    rd.abort();
}

/// Serves WebSocket subscriptions on `listener` until aborted.
pub fn spawn(state: Shared, listener: TcpListener) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let Ok((s, _peer)) = listener.accept().await else {
                continue;
            };
            tokio::spawn(serve_conn(state.clone(), s));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_and_the_rfc6455_example() {
        assert_eq!(
            hex::encode(sha1(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hex::encode(sha1(b"")),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
        let long = vec![b'a'; 1_000];
        assert_eq!(
            hex::encode(sha1(&long)),
            "291e9a6c66994949b57ba5e650361e98fc36b1ba"
        );
        // RFC 6455 §1.3.
        assert_eq!(
            accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn frames_use_the_short_and_extended_lengths() {
        assert_eq!(frame(1, b"hi"), vec![0x81, 2, b'h', b'i']);
        let f = frame(1, &[0u8; 300]);
        assert_eq!(&f[..4], &[0x81, 126, 1, 44]);
        let f = frame(1, &vec![0u8; 70_000]);
        assert_eq!(f[1], 127);
        assert_eq!(u64::from_be_bytes(f[2..10].try_into().unwrap()), 70_000);
    }
}
