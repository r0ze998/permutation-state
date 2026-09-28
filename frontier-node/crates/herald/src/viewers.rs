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
    let p99 = f(&["p99_ms"]);
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
        let w = f(&["ws", "ingest_p99_ms"]);
        if !at_most(w, TARGET_WS_P99_MS) {
            miss.push(format!("ingest → WS p99 {w} ms > {TARGET_WS_P99_MS} ms"));
        }
    }
    miss
}

impl Stats {
    pub fn report(&self, elapsed: Duration) -> Value {
        let n = self.requests.load(Ordering::SeqCst);
        let errors = self.errors.load(Ordering::SeqCst);
        let ws_connected = self.ws_connected.load(Ordering::SeqCst);
        let ws_errors = self.ws_errors.load(Ordering::SeqCst);
        // Failed requests and failed or dropped WS sessions over every
        // request and WS session.
        let attempts = n + ws_connected + ws_errors;
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
            "max_ms": self.max_us.load(Ordering::SeqCst) as f64 / 1_000.0,
            "rps": n as f64 / elapsed.as_secs_f64().max(1e-9),
            "elapsed_s": elapsed.as_secs_f64(),
            "ws": {
                "connected": self.ws_connected.load(Ordering::SeqCst),
                "messages": self.ws_messages.load(Ordering::SeqCst),
                "gaps": self.ws_gaps.load(Ordering::SeqCst),
                "errors": ws_errors,
                "timed": self.ws_lat.count(),
                "ingest_p50_ms": self.ws_lat.quantile_ms(0.50),
                "ingest_p99_ms": self.ws_lat.quantile_ms(0.99),
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
}

impl KeepAlive {
    pub async fn connect(host: &str) -> std::io::Result<KeepAlive> {
        let s = TcpStream::connect(host).await?;
        s.set_nodelay(true)?;
        Ok(KeepAlive {
            s,
            host: host.into(),
            buf: Vec::with_capacity(16 * 1024),
        })
    }

    /// `GET path` → (status, body length). Errors mean the connection is gone.
    pub async fn get(&mut self, path: &str) -> std::io::Result<(u16, usize)> {
        let req = format!(
            "GET {path} HTTP/1.1\r\nHost: {}\r\nAccept-Encoding: gzip\r\n\r\n",
            self.host
        );
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
        self.buf.drain(..need);
        if close {
            return Err(std::io::Error::other("server closed"));
        }
        Ok((status, len))
    }
}

fn pick(cfg: &ViewerCfg, r: &mut Rng) -> String {
    let bell = r.below(cfg.bells.max(1) as u64);
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
        let path = pick(&cfg, &mut r);
        let t0 = Instant::now();
        if conn.is_none() {
            conn = KeepAlive::connect(&cfg.herald).await.ok();
        }
        let res = match conn.as_mut() {
            Some(c) => c.get(&path).await,
            None => Err(std::io::Error::other("connect")),
        };
        let dt = t0.elapsed();
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
                stats.errors.fetch_add(1, Ordering::Relaxed);
            }
            Err(_) => {
                stats.errors.fetch_add(1, Ordering::Relaxed);
                conn = None;
            }
        }
        let pause = cfg.think.mul_f64(0.5 + r.below(1_000) as f64 / 1_000.0);
        tokio::time::sleep(pause).await;
    }
}

async fn ws_viewer(id: usize, cfg: Arc<ViewerCfg>, stats: Arc<Stats>, deadline: Instant) {
    let mut r = Rng(cfg.seed ^ (id as u64 + 7).wrapping_mul(0xD1B5_4A32_D192_ED03) | 1);
    let Ok(mut c) = ws::Client::connect(&cfg.herald, "/h/ws").await else {
        stats.ws_errors.fetch_add(1, Ordering::Relaxed);
        return;
    };
    stats.ws_connected.fetch_add(1, Ordering::Relaxed);
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
    if c.send_text(&sub.to_string()).await.is_err() {
        stats.ws_errors.fetch_add(1, Ordering::Relaxed);
        return;
    }
    let mut last: Option<u64> = None;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
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
                stats.ws_errors.fetch_add(1, Ordering::Relaxed);
                break;
            }
            Err(_) => break,
        }
    }
}

/// Runs the load for `cfg.duration` and reports.
pub async fn run(cfg: ViewerCfg, stats: Arc<Stats>) -> Value {
    let t0 = Instant::now();
    let deadline = t0 + cfg.duration;
    let cfg = Arc::new(cfg);
    let mut tasks = vec![];
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
        assert_eq!(h.count(), 100);
    }
}
