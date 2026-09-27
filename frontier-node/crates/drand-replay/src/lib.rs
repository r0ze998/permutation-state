//! `drand-replay` (M1 contract §8.7, I-53): drand's HTTP API
//! (`/{chain}/info`, `/{chain}/public/latest`, `/{chain}/public/{round}`,
//! and the same paths without the chain hash) over
//!
//! - **a packed archive** ([`archive`]: segments of contiguous rounds with
//!   a manifest; sha256-checked and sample-verified with blstrs at load,
//!   fully verified by `drand-replay verify --all`) built by the
//!   [`prefetch`] tool, which verifies every round as it fetches it (the
//!   contiguous ≈ 250k-round quicknet archive waits for O-M1-12);
//! - **a fixture directory** of `{round}.json` (the 32 SP-V2 rounds), every
//!   round verified on load; or
//! - **`--test-key`**: rounds signed on demand by the deterministic local key
//!   of `fclient::beacon::TestKey` (accepted only by a `test-beacon` build of
//!   the program; never deployable), cached after the first signing.
//!
//! **Gating.** A round is served only once the *game* clock has reached its
//! publication time: `round_time(r) + delay ≤ game_now`, where `delay` is
//! quicknet's observed publication latency (0.8–2.0 s, default 1.0 s of
//! game time). Earlier requests get `425 Too Early`, so seal secrecy holds
//! for every component pointed at this server (contract v1.2 §8.7).
//!
//! The game clock comes from the local chain node's Clock sysvar
//! ([`ClockSource::Chain`]: the last observed Clock, never extrapolated), a
//! fixed value (tests), or wall time scaled from a start value.

pub mod archive;
pub mod prefetch;

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};

use fclient::beacon::{self, TestKey};
use fclient::clock::GameClock;
use fclient::ports::{Beacon, ChainInfo, ChainPort};

/// Where rounds come from.
pub enum Source {
    /// A fixture directory of `{round}.json`.
    Archive {
        info: ChainInfo,
        rounds: BTreeMap<u64, Beacon>,
    },
    /// A packed archive ([`archive::Archive`]).
    Packed(archive::Archive),
    TestKey(TestKey),
}

/// Test-key signatures already computed (one hash-to-curve and a scalar
/// multiplication each; the keeper, herald and bots ask for the same
/// rounds many times). The key is the fixed `TestKey::new()`.
fn test_key_sig(k: &TestKey, r: u64) -> [u8; 48] {
    static CACHE: std::sync::OnceLock<Mutex<HashMap<u64, [u8; 48]>>> = std::sync::OnceLock::new();
    let c = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(s) = c.lock().unwrap_or_else(|p| p.into_inner()).get(&r) {
        return *s;
    }
    let s = k.sign(r);
    let mut g = c.lock().unwrap_or_else(|p| p.into_inner());
    if g.len() >= 65_536 {
        g.clear();
    }
    g.insert(r, s);
    s
}

impl Source {
    /// Loads and verifies a directory of `{round}.json` against `info`.
    pub fn archive(dir: &Path, info: ChainInfo) -> Result<Source, String> {
        let f = beacon::FixtureDrand::load(dir, info)?;
        Ok(Source::Archive {
            info: f.info,
            rounds: f.rounds,
        })
    }

    /// A packed archive if `dir` has a manifest, else a fixture directory;
    /// either way pinned to `info`.
    pub fn open(dir: &Path, info: ChainInfo) -> Result<Source, String> {
        if archive::Archive::is_packed(dir) {
            Ok(Source::Packed(archive::Archive::load(dir, &info)?))
        } else {
            Source::archive(dir, info)
        }
    }

    pub fn info(&self) -> ChainInfo {
        match self {
            Source::Archive { info, .. } => info.clone(),
            Source::Packed(a) => a.info.clone(),
            Source::TestKey(k) => k.info(),
        }
    }

    fn get(&self, r: u64) -> Option<Beacon> {
        match self {
            Source::Archive { rounds, .. } => rounds.get(&r).copied(),
            Source::Packed(a) => a.get(r),
            Source::TestKey(k) => Some(Beacon {
                round: r,
                sig48: test_key_sig(k, r),
            }),
        }
    }

    /// The highest round ≤ `max` this source has.
    fn latest_upto(&self, max: u64) -> Option<Beacon> {
        match self {
            Source::Archive { rounds, .. } => rounds.range(..=max).next_back().map(|(_, b)| *b),
            Source::Packed(a) => a.latest_upto(max),
            Source::TestKey(k) => (max >= 1).then(|| Beacon {
                round: max,
                sig48: test_key_sig(k, max),
            }),
        }
    }
}

/// The game clock drand-replay gates on.
pub enum ClockSource {
    /// A fixed game time (tests).
    Fixed(i64),
    /// `g0 + (wall − start) × scale` (standalone runs without a chain).
    Wall { g0: i64, scale: f64, start: Instant },
    /// The chain's Clock sysvar, polled every `poll`: the **last observed**
    /// `unix_timestamp`, never extrapolated (integ-W1 review: the
    /// extrapolation ran up to one slot of game time ahead of any Clock the
    /// program can read, 8 s at 20×, 800 s at 2,000×, and after a pause it
    /// kept running and later stepped back). A round is served at most one
    /// poll late, never early; the value is monotone while the chain's is.
    Chain { clock: Arc<Mutex<GameClock>> },
}

impl ClockSource {
    pub fn now(&self) -> Option<i64> {
        match self {
            ClockSource::Fixed(t) => Some(*t),
            ClockSource::Wall { g0, scale, start } => {
                Some(g0 + (start.elapsed().as_secs_f64() * scale).floor() as i64)
            }
            ClockSource::Chain { clock } => clock
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .last()
                .map(|c| c.unix_timestamp),
        }
    }
}

/// Polls a `ChainPort`'s Clock into a shared `GameClock` until aborted.
pub fn spawn_chain_clock<P: ChainPort + 'static>(
    port: P,
    poll: Duration,
) -> (Arc<Mutex<GameClock>>, tokio::task::JoinHandle<()>) {
    let gc = Arc::new(Mutex::new(GameClock::new(1.0)));
    let g2 = gc.clone();
    let h = tokio::spawn(async move {
        loop {
            if let Ok(c) = port.clock().await {
                g2.lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .observe(c, Instant::now());
            }
            tokio::time::sleep(poll).await;
        }
    });
    (gc, h)
}

pub struct Replay {
    pub source: Source,
    pub clock: ClockSource,
    /// Publication latency in game milliseconds (800–2,000).
    pub delay_ms: i64,
}

/// Why a round is not served.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Not yet published on the game clock (425).
    TooEarly,
    /// Not in the archive (404).
    Missing,
    /// The game clock is unknown (503).
    NoClock,
}

impl Replay {
    /// The latest round published at game time `now` (ms precision).
    fn published_upto(&self, now: i64) -> u64 {
        let i = self.source.info();
        // round_time(r) + delay ≤ now  ⇔  r ≤ (now·1000 − delay − genesis·1000) / (period·1000) + 1
        let t_ms = now as i128 * 1_000 - self.delay_ms as i128 - i.genesis_time as i128 * 1_000;
        if t_ms < 0 {
            return 0;
        }
        (t_ms / (i.period as i128 * 1_000)) as u64 + 1
    }

    pub fn round(&self, r: u64) -> Result<Beacon, Refusal> {
        let now = self.clock.now().ok_or(Refusal::NoClock)?;
        if r == 0 || r > self.published_upto(now) {
            return Err(Refusal::TooEarly);
        }
        self.source.get(r).ok_or(Refusal::Missing)
    }

    pub fn latest(&self) -> Result<Beacon, Refusal> {
        let now = self.clock.now().ok_or(Refusal::NoClock)?;
        let max = self.published_upto(now);
        self.source.latest_upto(max).ok_or(Refusal::Missing)
    }
}

type Shared = Arc<Replay>;

fn refusal(r: Refusal) -> Response {
    let (code, msg) = match r {
        Refusal::TooEarly => (
            StatusCode::from_u16(425).expect("425"),
            "round not yet published on the game clock",
        ),
        Refusal::Missing => (StatusCode::NOT_FOUND, "round not in the archive"),
        Refusal::NoClock => (StatusCode::SERVICE_UNAVAILABLE, "game clock unknown"),
    };
    (code, Json(serde_json::json!({"error": msg}))).into_response()
}

fn chain_ok(s: &Replay, chain: &str) -> bool {
    chain.eq_ignore_ascii_case(&hex_chain(s))
}

fn hex_chain(s: &Replay) -> String {
    s.source
        .info()
        .chain_hash
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

async fn info(State(s): State<Shared>) -> Response {
    Json(beacon::info_json(&s.source.info())).into_response()
}

async fn info_chain(State(s): State<Shared>, UrlPath(chain): UrlPath<String>) -> Response {
    if !chain_ok(&s, &chain) {
        return (StatusCode::NOT_FOUND, "unknown chain").into_response();
    }
    info(State(s)).await
}

async fn public(State(s): State<Shared>, UrlPath(which): UrlPath<String>) -> Response {
    let r = if which == "latest" {
        s.latest()
    } else {
        match which.parse::<u64>() {
            Ok(n) => s.round(n),
            Err(_) => return (StatusCode::BAD_REQUEST, "bad round").into_response(),
        }
    };
    match r {
        Ok(b) => {
            // A published round never changes; `latest` does.
            let cache = if which == "latest" {
                "no-store"
            } else {
                "public, max-age=31536000, immutable"
            };
            (
                [(axum::http::header::CACHE_CONTROL, cache)],
                Json(beacon::beacon_json(&b)),
            )
                .into_response()
        }
        Err(e) => refusal(e),
    }
}

async fn public_chain(
    State(s): State<Shared>,
    UrlPath((chain, which)): UrlPath<(String, String)>,
) -> Response {
    if !chain_ok(&s, &chain) {
        return (StatusCode::NOT_FOUND, "unknown chain").into_response();
    }
    public(State(s), UrlPath(which)).await
}

pub fn router(r: Arc<Replay>) -> Router {
    Router::new()
        .route("/info", get(info))
        .route("/public/{which}", get(public))
        .route("/{chain}/info", get(info_chain))
        .route("/{chain}/public/{which}", get(public_chain))
        .with_state(r)
}

pub struct Running {
    pub addr: std::net::SocketAddr,
    server: tokio::task::JoinHandle<()>,
}

impl Running {
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
    pub fn stop(self) {
        self.server.abort();
    }
}

/// Serves on `127.0.0.1:port` (0 = any free port, tests only).
pub async fn start(r: Arc<Replay>, port: u16) -> Result<Running, String> {
    if port != 0 && (fclient_reserved(port) || !(41_000..=41_999).contains(&port)) {
        return Err(format!(
            "port {port}: M1 services use 41000-41999 and never a reserved port"
        ));
    }
    if port != 0 && fclient::ports::port_in_use(port) {
        return Err(format!("port {port} is busy (a listener on some address)"));
    }
    let l = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| format!("bind 127.0.0.1:{port}: {e}"))?;
    let addr = l.local_addr().map_err(|e| e.to_string())?;
    let app = router(r);
    let server = tokio::spawn(async move {
        let _ = axum::serve(l, app).await;
    });
    Ok(Running { addr, server })
}

fn fclient_reserved(p: u16) -> bool {
    [
        4185, 4190, 4191, 4194, 18899, 17799, 28899, 27799, 26699, 5185, 5191,
    ]
    .contains(&p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Source {
        Source::archive(&beacon::fixture_dir(), beacon::quicknet_info()).unwrap()
    }

    /// The chain source never runs ahead of the last observed Clock, even
    /// long after it (a paused chain): no round is served early.
    #[test]
    fn chain_clock_never_extrapolates() {
        let info = beacon::quicknet_info();
        let src = fixture();
        let r = match &src {
            Source::Archive { rounds, .. } => *rounds.keys().nth(3).unwrap(),
            _ => unreachable!(),
        };
        let t = info.round_time(r);
        let gc = Arc::new(Mutex::new(GameClock::new(2_000.0)));
        let seen = |ts: i64| fclient::ports::ClockSysvar {
            slot: 10,
            unix_timestamp: ts,
            ..Default::default()
        };
        let then = Instant::now() - Duration::from_secs(30);
        gc.lock().unwrap().observe(seen(t), then);
        let rep = Replay {
            source: src,
            clock: ClockSource::Chain { clock: gc.clone() },
            delay_ms: 1_000,
        };
        // 30 wall seconds at 2,000× would be 60,000 game seconds ahead.
        assert_eq!(rep.clock.now(), Some(t));
        assert_eq!(rep.round(r), Err(Refusal::TooEarly));
        gc.lock().unwrap().observe(seen(t + 1), Instant::now());
        assert!(rep.round(r).is_ok());
    }

    #[test]
    fn gates_on_the_game_clock() {
        let info = beacon::quicknet_info();
        let src = fixture();
        let r = match &src {
            Source::Archive { rounds, .. } => *rounds.keys().nth(3).unwrap(),
            _ => unreachable!(),
        };
        let t = info.round_time(r);
        let at = |now: i64| Replay {
            source: fixture(),
            clock: ClockSource::Fixed(now),
            delay_ms: 1_000,
        };
        assert_eq!(
            at(t).round(r),
            Err(Refusal::TooEarly),
            "not before round_time + delay"
        );
        assert!(at(t + 1).round(r).is_ok());
        assert_eq!(at(t + 1).round(r + 1), Err(Refusal::TooEarly));
        assert_eq!(
            at(t + 600).round(r + 1),
            Err(Refusal::Missing),
            "published but not recorded"
        );
        assert_eq!(at(t + 1).latest().unwrap().round, r);
        let tk = Replay {
            source: Source::TestKey(TestKey::new()),
            clock: ClockSource::Fixed(t + 30),
            delay_ms: 800,
        };
        // (30 s − 0.8 s) / 3 s = 9 rounds after r.
        assert_eq!(tk.latest().unwrap().round, r + 9);
        assert_eq!(tk.round(r + 10), Err(Refusal::TooEarly));
        assert!(beacon::verify(
            r + 9,
            &tk.round(r + 9).unwrap().sig48,
            &TestKey::new().pk96
        ));
    }
}
