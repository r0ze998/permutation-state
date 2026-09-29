//! The viewer load generator (`frontier-viewers`; offchain design §8.3,
//! §12): polling viewers and WS viewers against one herald origin.
//!
//! A **polling viewer** keeps one HTTP/1.1 keep-alive connection and loops:
//! pick a request by the web client's mix (web design §4.2: overviews and
//! province files dominate, `latest` polling with jitter, the season record
//! now and then, a page of events), send it, record the latency and status,
//! then pause `think × U(0.5, 1.5)`. A **WS viewer** subscribes to ≤ 12
//! provinces, the rings and the bell records, and counts messages and
//! sequence gaps, and the **ingest → WS latency** of every message (its `t`,
//! the herald's ingest stamp, against the viewer's clock: one machine in
//! the stack runs). The report gives request count, errors, the error rate,
//! p50/p99/max latency, the request rate, the WS latency quantiles and the
//! §13.4 criterion-6 verdict ([`targets`]: file p99 ≤ 250 ms, ingest → WS
//! p99 ≤ 2 s, error rate < 0.1%).
//!
//! W6T-3 (w6-s7 criterion 6: 9,000 errors, all from two herald chaos kills
//! meeting a client with no recovery): with a nonzero `retry_budget` the
//! viewers recover as a browser does. A GET that fails on a reused
//! keep-alive connection before any byte of its answer is sent once more
//! on a fresh connection (`staleRetries`, RFC 9110 §9.2.2); a refused
//! connect is retried with backoff within the budget (`unavailable`,
//! `unavailable_ms`: the wait is in the request's latency); a WS viewer
//! whose socket the herald closed reconnects after 0–500 ms of jitter,
//! re-subscribes and restarts its sequence (`ws.reconnects`). Only a
//! failure after that recovery is an error; `errorSeconds` maps each
//! second of the window to its errors, for the chaos log.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::ws;

#[derive(Clone, Debug)]
pub struct ViewerCfg {
    /// The herald, `host:port`.
    pub herald: String,
    pub viewers: usize,
    pub ws: usize,
    pub duration: Duration,
    /// Mean pause between one viewer's requests.
    pub think: Duration,
    pub rings: Vec<u16>,
    pub provinces: Vec<(i16, i16)>,
    /// Immutable per-bell files are drawn from bells `0..bells`.
    pub bells: u32,
    pub seed: u64,
    /// How long a viewer keeps retrying a refused connect (or a WS
    /// reconnect) before the request counts as an error. Zero: the old
    /// behaviour (the first failure is an error, WS viewers never return).
    pub retry_budget: Duration,
    /// W6T-3: follow the live bell. `Some(host:port)`: a background task
    /// reads the herald's `/h/season` (genesis, bell length) and polls
    /// `/h/status` (`live`: the chain's newest slot and Clock) once a
    /// second, and the per-bell requests go to the live bell and the two
    /// before it instead of `0..bells` (w6-s7's per-bell requests went to
    /// bells 0–5 for 24 game hours). `None`: `0..bells` as before.
    pub follow: Option<String>,
}

/// Latency histogram: 16 buckets per power of two of microseconds.
pub struct Histogram {
    buckets: Vec<AtomicU64>,
}

const SUB: u32 = 16;

impl Default for Histogram {
    fn default() -> Self {
        Histogram {
            buckets: (0..(40 * SUB)).map(|_| AtomicU64::new(0)).collect(),
        }
    }
}

impl Histogram {
    fn index(us: u64) -> usize {
        let us = us.max(1);
        let e = 63 - us.leading_zeros();
        let frac = if e >= 4 {
            ((us >> (e - 4)) & 0xF) as u32
        } else {
            ((us << (4 - e)) & 0xF) as u32
        };
        ((e * SUB + frac) as usize).min(40 * SUB as usize - 1)
    }
    fn value(i: usize) -> f64 {
        let e = i as u32 / SUB;
        let f = i as u32 % SUB;
        (1u64 << e) as f64 * (1.0 + f as f64 / SUB as f64)
    }
    pub fn record(&self, d: Duration) {
        self.buckets[Self::index(d.as_micros() as u64)].fetch_add(1, Ordering::Relaxed);
    }
    pub fn count(&self) -> u64 {
        self.buckets.iter().map(|b| b.load(Ordering::Relaxed)).sum()
    }
    /// The `q`-quantile in milliseconds (bucket lower bound; ≤ 6.25% low).
    pub fn quantile_ms(&self, q: f64) -> f64 {
        let n = self.count();
        if n == 0 {
            return 0.0;
        }
        let want = ((n as f64) * q).ceil().max(1.0) as u64;
        let mut acc = 0;
        for (i, b) in self.buckets.iter().enumerate() {
            acc += b.load(Ordering::Relaxed);
            if acc >= want {
                return Self::value(i) / 1_000.0;
            }
        }
        Self::value(self.buckets.len() - 1) / 1_000.0
    }
    /// The `q`-quantile's bucket **upper** bound in milliseconds: never
    /// below the true quantile (≤ 6.25 % high). Targets are judged on it
    /// (wave-5 review of W5-C: the lower bound let a p99 up to 6.25 %
    /// above a target pass).
    pub fn quantile_upper_ms(&self, q: f64) -> f64 {
        let n = self.count();
        if n == 0 {
            return 0.0;
        }
        let want = ((n as f64) * q).ceil().max(1.0) as u64;
        let mut acc = 0;
        for (i, b) in self.buckets.iter().enumerate() {
            acc += b.load(Ordering::Relaxed);
            if acc >= want {
                return Self::value(i + 1) / 1_000.0;
            }
        }
        f64::INFINITY
    }
}

#[derive(Default)]
pub struct Stats {
    pub hist: Histogram,
    pub requests: AtomicU64,
    pub errors: AtomicU64,
    pub not_found: AtomicU64,
    pub max_us: AtomicU64,
    pub ws_connected: AtomicU64,
    pub ws_messages: AtomicU64,
    pub ws_gaps: AtomicU64,
    pub ws_errors: AtomicU64,
    /// GETs retried once on a fresh connection after a reused keep-alive
    /// connection failed before any response byte (RFC 9110 §9.2.2).
    pub stale_retries: AtomicU64,
    /// Requests that waited for a refused connect to succeed (within the
    /// retry budget), and the total time they waited.
    pub unavailable: AtomicU64,
    pub unavailable_us: AtomicU64,
    /// WS sessions re-established after the herald closed them.
    pub ws_reconnects: AtomicU64,
    /// Errors per second of the window (offset → count), so a cluster can
    /// be matched against the chaos log.
    pub error_secs: std::sync::Mutex<std::collections::BTreeMap<u64, u64>>,
    pub t0: std::sync::OnceLock<Instant>,
    /// The live bell `--follow-status` last read (`u32::MAX`: not yet).
    pub live_bell: std::sync::atomic::AtomicU32,
    /// `/h/status` or `/h/season` reads of the follower that failed.
    pub follow_errors: AtomicU64,
    /// Ingest → WS latency of every diff message that carried `t`.
    pub ws_lat: Histogram,
    pub ws_lat_max_us: AtomicU64,
}

/// §13.4 criterion 6 (E5) and the Gate W5 load line: file p99 ≤ 250 ms.
pub const TARGET_FILE_P99_MS: f64 = 250.0;
/// Ingest → WS p99 ≤ 2 s.
pub const TARGET_WS_P99_MS: f64 = 2_000.0;
/// Error rate < 0.1%.
pub const TARGET_ERROR_RATE: f64 = 0.001;

/// The criterion-6 misses of a report (empty: met). A report with no
/// request, or WS viewers but no timed message, cannot meet it.
pub fn targets(rep: &Value) -> Vec<String> {
    let f = |p: &[&str]| {
        let mut v = rep;
        for k in p {
            v = &v[*k];
        }
        v.as_f64().unwrap_or(f64::NAN)
    };
    // NaN (a missing field) meets nothing.
    let above = |x: f64, lim: f64| x.is_finite() && x > lim;
    let below = |x: f64, lim: f64| x.is_finite() && x < lim;
    let at_most = |x: f64, lim: f64| x.is_finite() && x <= lim;
    let mut miss = vec![];
    if !above(f(&["requests"]), 0.0) {
        miss.push("no request was answered".to_string());
    }
    // The bucket upper bound when the report has it (never low).
    let or = |a: f64, b: f64| if a.is_finite() { a } else { b };
    let p99 = or(f(&["p99_upper_ms"]), f(&["p99_ms"]));
    if !at_most(p99, TARGET_FILE_P99_MS) {
        miss.push(format!("file p99 {p99} ms > {TARGET_FILE_P99_MS} ms"));
    }
    let er = f(&["errorRate"]);
    if !below(er, TARGET_ERROR_RATE) {
        miss.push(format!("error rate {er} ≥ {TARGET_ERROR_RATE}"));
    }
    if f(&["wsViewers"]) > 0.0 {
        if !above(f(&["ws", "timed"]), 0.0) {
            miss.push("no WS message carried an ingest stamp".to_string());
        }
        let w = or(
            f(&["ws", "ingest_p99_upper_ms"]),
            f(&["ws", "ingest_p99_ms"]),
        );
        if !at_most(w, TARGET_WS_P99_MS) {
            miss.push(format!("ingest → WS p99 {w} ms > {TARGET_WS_P99_MS} ms"));
        }
    }
    miss
}

impl Stats {
    fn error(&self, n: &AtomicU64) {
        n.fetch_add(1, Ordering::Relaxed);
        let t = self.t0.get().map_or(0, |t| t.elapsed().as_secs());
        if let Ok(mut m) = self.error_secs.lock() {
            *m.entry(t).or_default() += 1;
        }
    }
    pub fn report(&self, elapsed: Duration) -> Value {
        let n = self.requests.load(Ordering::SeqCst);
        let errors = self.errors.load(Ordering::SeqCst);
        let ws_connected = self.ws_connected.load(Ordering::SeqCst);
        let ws_errors = self.ws_errors.load(Ordering::SeqCst);
        // Failed requests and failed or dropped WS sessions over every
        // request and WS session.
        let attempts = n + ws_connected + ws_errors;
        let error_secs: serde_json::Map<String, Value> = self
            .error_secs
            .lock()
            .map(|m| m.iter().map(|(k, v)| (k.to_string(), json!(v))).collect())
            .unwrap_or_default();
        let rate = if attempts == 0 {
            0.0
        } else {
            (errors + ws_errors) as f64 / attempts as f64
        };
        json!({
            "requests": n,
            "errors": errors,
            "errorRate": rate,
            "notFound": self.not_found.load(Ordering::SeqCst),
            "p50_ms": self.hist.quantile_ms(0.50),
            "p99_ms": self.hist.quantile_ms(0.99),
            "p99_upper_ms": self.hist.quantile_upper_ms(0.99),
            "max_ms": self.max_us.load(Ordering::SeqCst) as f64 / 1_000.0,
            "rps": n as f64 / elapsed.as_secs_f64().max(1e-9),
            "elapsed_s": elapsed.as_secs_f64(),
            "staleRetries": self.stale_retries.load(Ordering::SeqCst),
            "unavailable": self.unavailable.load(Ordering::SeqCst),
            "unavailable_ms": self.unavailable_us.load(Ordering::SeqCst) as f64 / 1_000.0,
            "errorSeconds": error_secs,
            "follow": {
                "liveBell": match self.live_bell.load(Ordering::SeqCst) {
                    u32::MAX => Value::Null,
                    b => json!(b),
                },
                "errors": self.follow_errors.load(Ordering::SeqCst),
            },
            "ws": {
                "connected": self.ws_connected.load(Ordering::SeqCst),
                "messages": self.ws_messages.load(Ordering::SeqCst),
                "gaps": self.ws_gaps.load(Ordering::SeqCst),
                "errors": ws_errors,
                "reconnects": self.ws_reconnects.load(Ordering::SeqCst),
                "timed": self.ws_lat.count(),
                "ingest_p50_ms": self.ws_lat.quantile_ms(0.50),
                "ingest_p99_ms": self.ws_lat.quantile_ms(0.99),
                "ingest_p99_upper_ms": self.ws_lat.quantile_upper_ms(0.99),
                "ingest_max_ms": self.ws_lat_max_us.load(Ordering::SeqCst) as f64 / 1_000.0,
            },
        })
    }
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }
}

/// An HTTP/1.1 keep-alive connection.
pub struct KeepAlive {
    s: TcpStream,
    host: String,
    buf: Vec<u8>,
    /// Answers read on this connection (a reused one when > 0).
    pub answered: u64,
    /// The last `get` read at least one byte of its answer.
    pub got_bytes: bool,
}

impl KeepAlive {
    pub async fn connect(host: &str) -> std::io::Result<KeepAlive> {
        let s = TcpStream::connect(host).await?;
        s.set_nodelay(true)?;
        Ok(KeepAlive {
            s,
            host: host.into(),
            buf: Vec::with_capacity(16 * 1024),
            answered: 0,
            got_bytes: false,
        })
    }

    /// `GET path` → (status, body length). Errors mean the connection is gone.
    pub async fn get(&mut self, path: &str) -> std::io::Result<(u16, usize)> {
        self.request(path, true, false)
            .await
            .map(|(st, n, _)| (st, n))
    }

    /// `GET path` (not gzip) → (status, body): the follower's JSON reads.
    pub async fn get_body(&mut self, path: &str) -> std::io::Result<(u16, Vec<u8>)> {
        self.request(path, false, true)
            .await
            .map(|(st, _, b)| (st, b))
    }

    async fn request(
        &mut self,
        path: &str,
        gzip: bool,
        keep_body: bool,
    ) -> std::io::Result<(u16, usize, Vec<u8>)> {
        let req = format!(
            "GET {path} HTTP/1.1\r\nHost: {}\r\n{}\r\n",
            self.host,
            if gzip {
                "Accept-Encoding: gzip\r\n"
            } else {
                ""
            }
        );
        self.got_bytes = !self.buf.is_empty();
        self.s.write_all(req.as_bytes()).await?;
        // Head.
        let end = loop {
            if let Some(i) = self.buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break i;
            }
            let mut chunk = [0u8; 8192];
            let n = self.s.read(&mut chunk).await?;
            if n == 0 {
                return Err(std::io::Error::other("closed"));
            }
            self.got_bytes = true;
            self.buf.extend_from_slice(&chunk[..n]);
        };
        let head = String::from_utf8_lossy(&self.buf[..end]).to_string();
        let status: u16 = head
            .split(' ')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| std::io::Error::other("status"))?;
        let mut len = None;
        let mut close = false;
        for l in head.split("\r\n").skip(1) {
            if let Some((k, v)) = l.split_once(':') {
                let k = k.trim().to_ascii_lowercase();
                if k == "content-length" {
                    len = v.trim().parse::<usize>().ok();
                } else if k == "connection" && v.trim().eq_ignore_ascii_case("close") {
                    close = true;
                } else if k == "transfer-encoding" {
                    return Err(std::io::Error::other("chunked answer"));
                }
            }
        }
        let len = len.unwrap_or(0);
        let need = end + 4 + len;
        while self.buf.len() < need {
            let mut chunk = [0u8; 16384];
            let n = self.s.read(&mut chunk).await?;
            if n == 0 {
                return Err(std::io::Error::other("closed"));
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
        let body = if keep_body {
            self.buf[end + 4..need].to_vec()
        } else {
            vec![]
        };
        self.buf.drain(..need);
        self.answered += 1;
        if close {
            return Err(std::io::Error::other("server closed"));
        }
        Ok((status, len, body))
    }
}

/// Connects, retrying a refused or failed connect with a doubling backoff
/// (25 ms … 400 ms) until `budget` has passed. Returns the connection and
/// whether it had to wait.
async fn connect_within(host: &str, budget: Duration) -> (Option<KeepAlive>, bool) {
    let t0 = Instant::now();
    let mut back = Duration::from_millis(25);
    let mut waited = false;
    loop {
        if let Ok(c) = KeepAlive::connect(host).await {
            return (Some(c), waited);
        }
        if t0.elapsed() + back > budget {
            return (None, waited);
        }
        waited = true;
        tokio::time::sleep(back).await;
        back = (back * 2).min(Duration::from_millis(400));
    }
}

/// Bells behind the live one a following viewer also reads (the live
/// bell and the two before it: the web client's bell strip).
pub const FOLLOW_BACK: u64 = 3;

fn pick(cfg: &ViewerCfg, r: &mut Rng, live: Option<u32>) -> String {
    let bell = match live {
        Some(l) => l as u64 - r.below(FOLLOW_BACK.min(l as u64 + 1)),
        None => r.below(cfg.bells.max(1) as u64),
    };
    let roll = r.below(100);
    // No rings or provinces named (a season before genesis): the season
    // record only, instead of a panic in every viewer (W5-C).
    let (Some(&ring), Some(&(p, q))) = (
        cfg.rings.get(r.below(cfg.rings.len() as u64) as usize),
        cfg.provinces
            .get(r.below(cfg.provinces.len() as u64) as usize),
    ) else {
        return "/h/season".into();
    };
    match roll {
        0..=4 => "/h/season".into(),
        5..=24 => format!("/h/overview/{ring}/latest.bin"),
        25..=44 => format!("/h/overview/{ring}/{bell}.bin"),
        45..=69 => format!("/h/province/{p},{q}/latest"),
        70..=94 => format!("/h/province/{p},{q}/{bell}"),
        _ => format!("/h/events?after={}", r.below(50)),
    }
}

async fn poller(id: usize, cfg: Arc<ViewerCfg>, stats: Arc<Stats>, deadline: Instant) {
    let mut r = Rng(cfg.seed ^ (id as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut conn: Option<KeepAlive> = None;
    // Spread the start over one think time.
    let start = cfg.think.mul_f64(r.below(1_000) as f64 / 1_000.0);
    tokio::time::sleep(start).await;
    while Instant::now() < deadline {
        let path = pick(&cfg, &mut r, live(&cfg, &stats));
        let t0 = Instant::now();
        let mut waited = false;
        if conn.is_none() {
            let (c, w) = connect_within(&cfg.herald, cfg.retry_budget).await;
            conn = c;
            waited = w;
        }
        let mut res = match conn.as_mut() {
            Some(c) => c.get(&path).await,
            None => Err(std::io::Error::other("connect")),
        };
        // A reused keep-alive connection the server closed (a restart, an
        // idle timeout) fails before any byte of the answer: retry the GET
        // once on a fresh connection, as browsers and HTTP clients do.
        if !cfg.retry_budget.is_zero()
            && res.is_err()
            && conn
                .as_ref()
                .is_some_and(|c| c.answered > 0 && !c.got_bytes)
        {
            stats.stale_retries.fetch_add(1, Ordering::Relaxed);
            let (c, w) = connect_within(&cfg.herald, cfg.retry_budget).await;
            conn = c;
            waited |= w;
            res = match conn.as_mut() {
                Some(c) => c.get(&path).await,
                None => Err(std::io::Error::other("connect")),
            };
        }
        let dt = t0.elapsed();
        if waited {
            stats.unavailable.fetch_add(1, Ordering::Relaxed);
            stats
                .unavailable_us
                .fetch_add(dt.as_micros() as u64, Ordering::Relaxed);
        }
        stats.requests.fetch_add(1, Ordering::Relaxed);
        stats.hist.record(dt);
        stats
            .max_us
            .fetch_max(dt.as_micros() as u64, Ordering::Relaxed);
        match res {
            Ok((200, _)) | Ok((304, _)) => {}
            Ok((404, _)) => {
                stats.not_found.fetch_add(1, Ordering::Relaxed);
            }
            Ok(_) => {
                stats.error(&stats.errors);
            }
            Err(_) => {
                stats.error(&stats.errors);
                conn = None;
            }
        }
        let pause = cfg.think.mul_f64(0.5 + r.below(1_000) as f64 / 1_000.0);
        tokio::time::sleep(pause).await;
    }
}

async fn ws_connect_within(host: &str, budget: Duration, deadline: Instant) -> Option<ws::Client> {
    let t0 = Instant::now();
    let mut back = Duration::from_millis(50);
    loop {
        if let Ok(c) = ws::Client::connect(host, "/h/ws").await {
            return Some(c);
        }
        if t0.elapsed() + back > budget || Instant::now() + back >= deadline {
            return None;
        }
        tokio::time::sleep(back).await;
        back = (back * 2).min(Duration::from_millis(800));
    }
}

async fn ws_viewer(id: usize, cfg: Arc<ViewerCfg>, stats: Arc<Stats>, deadline: Instant) {
    let mut r = Rng(cfg.seed ^ (id as u64 + 7).wrapping_mul(0xD1B5_4A32_D192_ED03) | 1);
    let n = (1 + r.below(12)) as usize;
    let provs: Vec<Value> = (0..n)
        .filter_map(|_| {
            let &(p, q) = cfg
                .provinces
                .get(r.below(cfg.provinces.len() as u64) as usize)?;
            Some(json!([p, q]))
        })
        .collect();
    let sub = json!({"op": "sub", "provinces": provs, "rings": cfg.rings, "bells": true});
    let mut first = true;
    'session: loop {
        if !first {
            if cfg.retry_budget.is_zero() {
                return;
            }
            // A dropped socket reconnects (§8.4: the client resyncs from the
            // files); jitter spreads a thousand reconnects after a restart.
            let j = Duration::from_millis(r.below(500));
            if Instant::now() + j >= deadline {
                return;
            }
            tokio::time::sleep(j).await;
        }
        let budget = if first {
            Duration::ZERO
        } else {
            cfg.retry_budget
        };
        let Some(mut c) = ws_connect_within(&cfg.herald, budget, deadline).await else {
            if Instant::now() < deadline {
                stats.error(&stats.ws_errors);
            }
            return;
        };
        if first {
            stats.ws_connected.fetch_add(1, Ordering::Relaxed);
        } else {
            stats.ws_reconnects.fetch_add(1, Ordering::Relaxed);
        }
        first = false;
        if c.send_text(&sub.to_string()).await.is_err() {
            if cfg.retry_budget.is_zero() {
                stats.error(&stats.ws_errors);
                return;
            }
            continue 'session;
        }
        // Sequence numbers are per connection (ws.rs): a new session starts over.
        let mut last: Option<u64> = None;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return;
            }
            match tokio::time::timeout(left, c.next_text()).await {
                Ok(Some(t)) => {
                    let Ok(v) = serde_json::from_str::<Value>(&t) else {
                        continue;
                    };
                    if let Some(seq) = v.get("seq").and_then(|s| s.as_u64()) {
                        stats.ws_messages.fetch_add(1, Ordering::Relaxed);
                        if let Some(t) = v.get("t").and_then(|t| t.as_u64()).filter(|t| *t > 0) {
                            let us = crate::runner::unix_ms().saturating_sub(t) * 1_000;
                            stats.ws_lat.record(Duration::from_micros(us));
                            stats.ws_lat_max_us.fetch_max(us, Ordering::Relaxed);
                        }
                        if last.is_some_and(|l| seq != l + 1) {
                            stats.ws_gaps.fetch_add(1, Ordering::Relaxed);
                        }
                        last = Some(seq);
                    }
                }
                Ok(None) => {
                    if cfg.retry_budget.is_zero() {
                        stats.error(&stats.ws_errors);
                        return;
                    }
                    continue 'session;
                }
                Err(_) => return,
            }
        }
    }
}

/// The live bell when following (`None`: not following, or not read yet:
/// then `0..bells`).
fn live(cfg: &ViewerCfg, stats: &Stats) -> Option<u32> {
    cfg.follow.as_ref()?;
    match stats.live_bell.load(Ordering::Relaxed) {
        u32::MAX => None,
        b => Some(b),
    }
}

/// `--follow-status`: reads `/h/season` (genesis, bell length) and then
/// `/h/status` once a second until `deadline`, storing the live bell
/// (`live` = [slot, Clock] of the chain's newest slot). Its requests are
/// not viewer requests (not counted, not errors of the load).
async fn follower(host: String, stats: Arc<Stats>, deadline: Instant) {
    let mut conn: Option<KeepAlive> = None;
    let mut season: Option<(i64, i64)> = None;
    while Instant::now() < deadline {
        if conn.is_none() {
            conn = KeepAlive::connect(&host).await.ok();
        }
        let json_of = |b: &[u8]| serde_json::from_slice::<Value>(b).ok();
        let ok = match conn.as_mut() {
            None => false,
            Some(c) => {
                if season.is_none() {
                    season = match c.get_body("/h/season").await {
                        Ok((200, b)) => json_of(&b).and_then(|v| {
                            Some((v.get("genesisTs")?.as_i64()?, v.get("bellSecs")?.as_i64()?))
                        }),
                        _ => None,
                    };
                }
                match (season, c.get_body("/h/status").await) {
                    (Some((g, secs)), Ok((200, b))) => {
                        let ts = json_of(&b).and_then(|v| v.get("live")?.get(1)?.as_i64());
                        if let Some(ts) = ts.filter(|&t| t >= g && secs > 0) {
                            let bell = ((ts - g) / secs).min(u32::MAX as i64 - 1) as u32;
                            stats.live_bell.store(bell, Ordering::Relaxed);
                        }
                        true
                    }
                    _ => false,
                }
            }
        };
        if !ok {
            stats.follow_errors.fetch_add(1, Ordering::Relaxed);
            conn = None;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Runs the load for `cfg.duration` and reports.
pub async fn run(cfg: ViewerCfg, stats: Arc<Stats>) -> Value {
    let t0 = Instant::now();
    let _ = stats.t0.set(t0);
    stats.live_bell.store(u32::MAX, Ordering::SeqCst);
    let deadline = t0 + cfg.duration;
    let cfg = Arc::new(cfg);
    let mut tasks = vec![];
    if let Some(h) = cfg.follow.clone() {
        // The first live bell before the viewers start.
        let f = tokio::spawn(follower(h, stats.clone(), deadline));
        let wait = Instant::now() + Duration::from_secs(2);
        while stats.live_bell.load(Ordering::Relaxed) == u32::MAX && Instant::now() < wait {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        tasks.push(f);
    }
    for i in 0..cfg.viewers {
        tasks.push(tokio::spawn(poller(
            i,
            cfg.clone(),
            stats.clone(),
            deadline,
        )));
    }
    for i in 0..cfg.ws {
        tasks.push(tokio::spawn(ws_viewer(
            i,
            cfg.clone(),
            stats.clone(),
            deadline,
        )));
    }
    for t in tasks {
        let _ = t.await;
    }
    let mut rep = stats.report(t0.elapsed());
    rep["viewers"] = json!(cfg.viewers);
    rep["wsViewers"] = json!(cfg.ws);
    let miss = targets(&rep);
    rep["targetsMet"] = json!(miss.is_empty());
    rep["targetMisses"] = json!(miss);
    rep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_quantiles() {
        let h = Histogram::default();
        for ms in 1..=100u64 {
            h.record(Duration::from_millis(ms));
        }
        let p50 = h.quantile_ms(0.5);
        let p99 = h.quantile_ms(0.99);
        assert!((46.0..=50.0).contains(&p50), "{p50}");
        assert!((92.0..=99.0).contains(&p99), "{p99}");
        assert!(h.quantile_upper_ms(0.99) >= 99.0);
        assert_eq!(h.count(), 100);
    }

    /// Wave-5 review: a true p99 of 252 ms sits in the bucket [245.8,
    /// 254.0) ms; its lower bound passed the 250-ms target, the upper
    /// bound the targets now use does not.
    #[test]
    fn targets_use_the_bucket_upper_bound() {
        let h = Histogram::default();
        for _ in 0..100 {
            h.record(Duration::from_millis(252));
        }
        assert!(h.quantile_ms(0.99) <= TARGET_FILE_P99_MS);
        assert!(h.quantile_upper_ms(0.99) > TARGET_FILE_P99_MS);
        let rep = json!({"requests": 100, "p99_ms": h.quantile_ms(0.99),
            "p99_upper_ms": h.quantile_upper_ms(0.99), "errorRate": 0.0, "wsViewers": 0});
        let miss = targets(&rep);
        assert_eq!(miss.len(), 1, "{miss:?}");
        assert!(miss[0].starts_with("file p99"));
    }
}
