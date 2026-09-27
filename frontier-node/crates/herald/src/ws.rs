//! `WS /h/ws` (contract §8.4): the herald's diff fan-out.
//!
//! Client → server: `{"op":"sub","provinces":[[P,Q]…≤64],"rings":[…],
//! "wallet":"<b58>"?, "bells":true}` (each `sub` replaces the previous
//! one). Server → client: `{"seq","kind":"acct|bell|event","key","slot",
//! "head","bytes_b64"}` numbered **per connection** from 1 in the order
//! sent; a `sub` is acknowledged by `{"op":"subscribed",…}` (no `seq`).
//!
//! Fan-out: one broadcast channel of [`Diff`]s from the fold, a bounded
//! queue per socket (the channel's capacity). A socket that falls behind
//! loses the diffs it missed; the next message it gets **skips a sequence
//! number**, so the client sees the gap and resyncs from the files (§8.4).
//! A socket that falls behind [`WsCfg::max_lags`] times is dropped
//! (close 1013) and reconnects. Heartbeat: a ping every 15 s; a socket
//! silent for 3 heartbeats is closed.
//!
//! The protocol is RFC 6455 over hyper's upgrade (no `ws` feature, no
//! tungstenite): text, ping/pong and close frames; client frames must be
//! masked, server frames are not; fragmented client messages are joined
//! up to [`MAX_MESSAGE`].

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{broadcast, mpsc};

use crate::fold::{Diff, Scope};
use crate::records::B64;

/// The RFC 6455 key GUID.
pub const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
/// Largest client message (a `sub` is small).
pub const MAX_MESSAGE: usize = 64 * 1024;
/// Provinces one socket may subscribe to.
pub const MAX_PROVINCES: usize = 64;

pub const OP_CONT: u8 = 0;
pub const OP_TEXT: u8 = 1;
pub const OP_BINARY: u8 = 2;
pub const OP_CLOSE: u8 = 8;
pub const OP_PING: u8 = 9;
pub const OP_PONG: u8 = 10;

/// SHA-1 (RFC 3174), for `Sec-WebSocket-Accept` only.
pub fn sha1(msg: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut m = msg.to_vec();
    let bits = (msg.len() as u64).wrapping_mul(8);
    m.push(0x80);
    while m.len() % 64 != 56 {
        m.push(0);
    }
    m.extend_from_slice(&bits.to_be_bytes());
    for chunk in m.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
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
        for (x, y) in h.iter_mut().zip([a, b, c, d, e]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 20];
    for (i, v) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

/// `Sec-WebSocket-Accept` for a client key.
pub fn accept_key(key: &str) -> String {
    B64.encode(sha1(format!("{}{GUID}", key.trim()).as_bytes()))
}

/// One frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub fin: bool,
    pub op: u8,
    pub payload: Vec<u8>,
}

/// Why a frame could not be read.
#[derive(Debug)]
pub enum FrameError {
    Io(std::io::Error),
    TooLarge,
    Unmasked,
    Masked,
}

/// Reads one frame; `server` requires a masked frame (client → server),
/// a client requires an unmasked one.
pub async fn read_frame<R: AsyncRead + Unpin>(
    r: &mut R,
    server: bool,
) -> Result<Frame, FrameError> {
    let mut h = [0u8; 2];
    r.read_exact(&mut h).await.map_err(FrameError::Io)?;
    let fin = h[0] & 0x80 != 0;
    let op = h[0] & 0x0F;
    let masked = h[1] & 0x80 != 0;
    let mut len = (h[1] & 0x7F) as u64;
    if len == 126 {
        let mut b = [0u8; 2];
        r.read_exact(&mut b).await.map_err(FrameError::Io)?;
        len = u16::from_be_bytes(b) as u64;
    } else if len == 127 {
        let mut b = [0u8; 8];
        r.read_exact(&mut b).await.map_err(FrameError::Io)?;
        len = u64::from_be_bytes(b);
    }
    if server && !masked {
        return Err(FrameError::Unmasked);
    }
    if !server && masked {
        return Err(FrameError::Masked);
    }
    let limit = if server { MAX_MESSAGE } else { 64 << 20 };
    if len as usize > limit {
        return Err(FrameError::TooLarge);
    }
    let mut key = [0u8; 4];
    if masked {
        r.read_exact(&mut key).await.map_err(FrameError::Io)?;
    }
    let mut payload = vec![0u8; len as usize];
    r.read_exact(&mut payload).await.map_err(FrameError::Io)?;
    if masked {
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= key[i % 4];
        }
    }
    Ok(Frame { fin, op, payload })
}

/// Encodes a final frame (`mask` = client frames).
pub fn encode_frame(op: u8, payload: &[u8], mask: Option<[u8; 4]>) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 14);
    out.push(0x80 | (op & 0x0F));
    let m = if mask.is_some() { 0x80 } else { 0 };
    match payload.len() {
        n if n < 126 => out.push(m | n as u8),
        n if n <= u16::MAX as usize => {
            out.push(m | 126);
            out.extend_from_slice(&(n as u16).to_be_bytes());
        }
        n => {
            out.push(m | 127);
            out.extend_from_slice(&(n as u64).to_be_bytes());
        }
    }
    match mask {
        Some(k) => {
            out.extend_from_slice(&k);
            out.extend(payload.iter().enumerate().map(|(i, b)| b ^ k[i % 4]));
        }
        None => out.extend_from_slice(payload),
    }
    out
}

/// A socket's subscription.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sub {
    pub provinces: Vec<(i32, i32)>,
    pub rings: Vec<u16>,
    pub wallet: Option<[u8; 32]>,
    pub bells: bool,
}

impl Sub {
    /// Parses a `sub` message; `Err(code)` for a refusal.
    pub fn parse(v: &Value) -> Result<Sub, &'static str> {
        if v.get("op").and_then(|o| o.as_str()) != Some("sub") {
            return Err("UnknownOp");
        }
        let mut s = Sub::default();
        if let Some(a) = v.get("provinces").and_then(|x| x.as_array()) {
            if a.len() > MAX_PROVINCES {
                return Err("TooManyProvinces");
            }
            for pq in a {
                let p = pq.get(0).and_then(|x| x.as_i64()).ok_or("BadProvince")?;
                let q = pq.get(1).and_then(|x| x.as_i64()).ok_or("BadProvince")?;
                s.provinces.push((p as i32, q as i32));
            }
        }
        if let Some(a) = v.get("rings").and_then(|x| x.as_array()) {
            if a.len() > 256 {
                return Err("TooManyRings");
            }
            for d in a {
                s.rings
                    .push(d.as_u64().ok_or("BadRing")?.min(u16::MAX as u64) as u16);
            }
        }
        if let Some(w) = v.get("wallet").and_then(|x| x.as_str()) {
            let a: solana_address::Address = w.parse().map_err(|_| "BadWallet")?;
            s.wallet = Some(a.to_bytes());
        }
        s.bells = v.get("bells").and_then(|x| x.as_bool()).unwrap_or(false);
        Ok(s)
    }

    pub fn wants(&self, scope: &Scope) -> bool {
        match scope {
            Scope::Province(p, q) => self.provinces.contains(&(*p, *q)),
            Scope::Ring(d) => self.rings.contains(d),
            Scope::Wallet(w) => self.wallet.as_ref() == Some(w),
            Scope::Bells => self.bells,
            Scope::None => false,
        }
    }
}

/// A diff as a numbered message.
pub fn message(seq: u64, d: &Diff) -> String {
    json!({
        "seq": seq, "kind": d.kind, "key": d.key, "slot": d.slot,
        "head": d.head.map(hex::encode), "bytes_b64": B64.encode(&d.bytes),
    })
    .to_string()
}

#[derive(Clone, Copy, Debug)]
pub struct WsCfg {
    pub heartbeat: Duration,
    /// Lags tolerated before the socket is dropped.
    pub max_lags: u32,
    pub max_sockets: usize,
}

impl Default for WsCfg {
    fn default() -> Self {
        WsCfg {
            heartbeat: Duration::from_secs(15),
            max_lags: 3,
            max_sockets: 8_192,
        }
    }
}

/// Counters shared by every socket.
#[derive(Default)]
pub struct WsStats {
    pub open: AtomicUsize,
    pub total: AtomicUsize,
    pub dropped_slow: AtomicUsize,
}

enum Ctl {
    Sub(Sub),
    Refused(&'static str),
    Pong(Vec<u8>),
    Close,
    Seen,
}

/// Serves one upgraded socket until it closes.
pub async fn serve<S>(
    io: S,
    mut rx: broadcast::Receiver<Arc<Diff>>,
    cfg: WsCfg,
    stats: Arc<WsStats>,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    stats.open.fetch_add(1, Ordering::SeqCst);
    stats.total.fetch_add(1, Ordering::SeqCst);
    let (mut rd, mut wr) = tokio::io::split(io);
    let (ctl_tx, mut ctl) = mpsc::channel::<Ctl>(16);
    let reader = tokio::spawn(async move {
        let mut buf: Vec<u8> = vec![];
        loop {
            let f = match read_frame(&mut rd, true).await {
                Ok(f) => f,
                Err(_) => {
                    let _ = ctl_tx.send(Ctl::Close).await;
                    return;
                }
            };
            let _ = ctl_tx.send(Ctl::Seen).await;
            match f.op {
                OP_TEXT | OP_BINARY | OP_CONT => {
                    buf.extend_from_slice(&f.payload);
                    if buf.len() > MAX_MESSAGE {
                        let _ = ctl_tx.send(Ctl::Close).await;
                        return;
                    }
                    if !f.fin {
                        continue;
                    }
                    let msg = std::mem::take(&mut buf);
                    let c = match serde_json::from_slice::<Value>(&msg) {
                        Ok(v) => match Sub::parse(&v) {
                            Ok(s) => Ctl::Sub(s),
                            Err(code) => Ctl::Refused(code),
                        },
                        Err(_) => Ctl::Refused("BadJson"),
                    };
                    if ctl_tx.send(c).await.is_err() {
                        return;
                    }
                }
                OP_PING => {
                    let _ = ctl_tx.send(Ctl::Pong(f.payload)).await;
                }
                OP_PONG => {}
                _ => {
                    let _ = ctl_tx.send(Ctl::Close).await;
                    return;
                }
            }
        }
    });
    let mut sub = Sub::default();
    let mut seq: u64 = 0;
    let mut lags = 0u32;
    let mut silent = 0u32;
    let mut hb = tokio::time::interval(cfg.heartbeat);
    hb.tick().await;
    let close_code: u16 = loop {
        tokio::select! {
            d = rx.recv() => match d {
                Ok(d) => {
                    if sub.wants(&d.scope) {
                        seq += 1;
                        let m = message(seq, &d);
                        if wr.write_all(&encode_frame(OP_TEXT, m.as_bytes(), None)).await.is_err() {
                            break 1006;
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    lags += 1;
                    // The gap: the next message skips a number.
                    seq += 1;
                    if lags > cfg.max_lags {
                        stats.dropped_slow.fetch_add(1, Ordering::SeqCst);
                        break 1013;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break 1001,
            },
            c = ctl.recv() => match c {
                Some(Ctl::Sub(s)) => {
                    let ack = json!({"op": "subscribed", "provinces": s.provinces.len(), "rings": s.rings.len(),
                        "wallet": s.wallet.is_some(), "bells": s.bells}).to_string();
                    sub = s;
                    if wr.write_all(&encode_frame(OP_TEXT, ack.as_bytes(), None)).await.is_err() {
                        break 1006;
                    }
                }
                Some(Ctl::Refused(code)) => {
                    let m = json!({"op": "error", "code": code}).to_string();
                    if wr.write_all(&encode_frame(OP_TEXT, m.as_bytes(), None)).await.is_err() {
                        break 1006;
                    }
                }
                Some(Ctl::Pong(p)) => {
                    if wr.write_all(&encode_frame(OP_PONG, &p, None)).await.is_err() {
                        break 1006;
                    }
                }
                Some(Ctl::Seen) => silent = 0,
                Some(Ctl::Close) | None => break 1000,
            },
            _ = hb.tick() => {
                silent += 1;
                if silent > 3 {
                    break 1001;
                }
                if wr.write_all(&encode_frame(OP_PING, b"hb", None)).await.is_err() {
                    break 1006;
                }
            }
        }
    };
    if close_code != 1006 {
        let _ = wr
            .write_all(&encode_frame(OP_CLOSE, &close_code.to_be_bytes(), None))
            .await;
    }
    let _ = wr.shutdown().await;
    reader.abort();
    stats.open.fetch_sub(1, Ordering::SeqCst);
}

// ------------------------------------------------------------------ client

/// A minimal WebSocket client (the viewer generator and the tests).
pub struct Client {
    pub rd: tokio::io::ReadHalf<tokio::net::TcpStream>,
    pub wr: tokio::io::WriteHalf<tokio::net::TcpStream>,
    mask: u32,
}

impl Client {
    /// Connects to `ws://host:port/path` (given as `host:port` and `path`).
    pub async fn connect(hostport: &str, path: &str) -> std::io::Result<Client> {
        let s = tokio::net::TcpStream::connect(hostport).await?;
        s.set_nodelay(true)?;
        let (mut rd, mut wr) = tokio::io::split(s);
        let key = B64.encode(&sha1(format!("{hostport}{path}").as_bytes())[..16]);
        let req = format!(
            "GET {path} HTTP/1.1\r\nHost: {hostport}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        wr.write_all(req.as_bytes()).await?;
        let mut head = vec![];
        let mut b = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            rd.read_exact(&mut b).await?;
            head.push(b[0]);
            if head.len() > 8192 {
                return Err(std::io::Error::other("handshake too long"));
            }
        }
        let text = String::from_utf8_lossy(&head);
        let want = accept_key(&key);
        if !text.starts_with("HTTP/1.1 101") || !text.contains(&want) {
            return Err(std::io::Error::other(format!("handshake refused: {text}")));
        }
        Ok(Client {
            rd,
            wr,
            mask: 0x9E37_79B9,
        })
    }

    fn next_mask(&mut self) -> [u8; 4] {
        self.mask = self
            .mask
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        self.mask.to_le_bytes()
    }

    pub async fn send_text(&mut self, s: &str) -> std::io::Result<()> {
        let m = self.next_mask();
        self.wr
            .write_all(&encode_frame(OP_TEXT, s.as_bytes(), Some(m)))
            .await
    }

    pub async fn send_frame(&mut self, op: u8, p: &[u8]) -> std::io::Result<()> {
        let m = self.next_mask();
        self.wr.write_all(&encode_frame(op, p, Some(m))).await
    }

    /// The next text message (pings answered, pongs skipped); `None` on close.
    pub async fn next_text(&mut self) -> Option<String> {
        loop {
            let f = read_frame(&mut self.rd, false).await.ok()?;
            match f.op {
                OP_TEXT => return String::from_utf8(f.payload).ok(),
                OP_PING => {
                    let m = self.next_mask();
                    self.wr
                        .write_all(&encode_frame(OP_PONG, &f.payload, Some(m)))
                        .await
                        .ok()?;
                }
                OP_CLOSE => return None,
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_and_the_rfc_6455_example() {
        assert_eq!(
            hex::encode(sha1(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hex::encode(sha1(b"")),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
        assert_eq!(
            hex::encode(sha1(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
        // RFC 6455 §1.3.
        assert_eq!(
            accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[tokio::test]
    async fn frames_round_trip_masked_and_long() {
        for n in [0usize, 5, 125, 126, 65_535, 65_536] {
            let p: Vec<u8> = (0..n).map(|i| i as u8).collect();
            let enc = encode_frame(OP_TEXT, &p, Some([1, 2, 3, 4]));
            let f = read_frame(&mut &enc[..], true).await.unwrap();
            assert_eq!((f.fin, f.op, f.payload.len()), (true, OP_TEXT, n));
            assert_eq!(f.payload, p);
            let enc = encode_frame(OP_BINARY, &p, None);
            assert!(matches!(
                read_frame(&mut &enc[..], true).await,
                Err(FrameError::Unmasked)
            ));
            let f = read_frame(&mut &enc[..], false).await.unwrap();
            assert_eq!(f.payload, p);
        }
    }

    #[test]
    fn subscriptions_parse_and_route() {
        let w = solana_address::Address::new_from_array([7; 32]);
        let s = Sub::parse(
            &json!({"op": "sub", "provinces": [[2, 0], [-1, 3]], "rings": [2],
            "wallet": w.to_string(), "bells": true}),
        )
        .unwrap();
        assert!(s.wants(&Scope::Province(-1, 3)));
        assert!(!s.wants(&Scope::Province(3, -1)));
        assert!(s.wants(&Scope::Ring(2)) && !s.wants(&Scope::Ring(3)));
        assert!(s.wants(&Scope::Wallet([7; 32])) && !s.wants(&Scope::Wallet([8; 32])));
        assert!(s.wants(&Scope::Bells) && !s.wants(&Scope::None));
        let many: Vec<Value> = (0..65).map(|i| json!([i, 0])).collect();
        assert_eq!(
            Sub::parse(&json!({"op": "sub", "provinces": many})),
            Err("TooManyProvinces")
        );
        assert_eq!(Sub::parse(&json!({"op": "nope"})), Err("UnknownOp"));
    }
}
