//! The prefetch tool (M1 contract §8.7, I-53): fetches a contiguous range
//! of drand rounds, one round per request, from one or more drand HTTP
//! endpoints at a bounded rate per endpoint, **verifies every round with
//! blstrs against the pinned key as it arrives**, keeps a resumable partial
//! file, and packs the result into an [`Archive`].
//!
//! **The public download waits for the owner (O-M1-12).** Endpoints must be
//! loopback unless `allow_public` is set, and the CLI sets it only with
//! `--approved O-M1-12`. Plain `http://` endpoints use `fclient`'s client;
//! `https://` ones (drand's public API) need the `curl` fetcher, since the
//! workspace has no TLS client. Tests fetch from a local `drand-replay`
//! (test key) over loopback.
//!
//! **Plan.** The exit's game clock runs contiguously from G0 through the
//! pre-season and 7 game days, so [`plan`] asks for every round from the
//! first at or after G0 to the last before `G0 + (pre_days + days) × 86,400
//! + margin`: ≈ 8.5 game days ≈ 245,000 rounds (≈ 12 MB packed).

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fclient::beacon;
use fclient::ports::{Beacon, ChainInfo};

use crate::archive::Archive;

/// A contiguous range of rounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub first: u64,
    pub last: u64,
}

impl Plan {
    pub fn count(&self) -> u64 {
        self.last + 1 - self.first
    }
    /// Packed bytes (48 per round).
    pub fn bytes(&self) -> u64 {
        48 * self.count()
    }
    /// Wall seconds to fetch at `rate` requests/s on each of `endpoints`.
    pub fn fetch_secs(&self, rate: f64, endpoints: usize) -> f64 {
        self.count() as f64 / (rate * endpoints.max(1) as f64)
    }
}

/// The rounds a season needs: from the first round at or after `g0`
/// through `(pre_days + days)` game days after it, plus `margin_secs`.
pub fn plan(info: &ChainInfo, g0: i64, pre_days: f64, days: f64, margin_secs: i64) -> Plan {
    let span = ((pre_days + days) * 86_400.0).ceil() as i64 + margin_secs;
    let first = info.first_round_from(g0);
    let last = info.first_round_from(g0 + span);
    Plan { first, last }
}

/// How rounds are fetched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fetcher {
    /// `fclient::http` (plain `http://` only).
    Native,
    /// `curl -sfS` (any scheme curl supports; needed for `https://`).
    Curl,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub endpoints: Vec<String>,
    /// Requests per second per endpoint (≤ 20 for public endpoints).
    pub rate: f64,
    pub fetcher: Fetcher,
    /// Non-loopback endpoints allowed (the owner approved O-M1-12).
    pub allow_public: bool,
    /// Attempts per round before the fetch fails.
    pub attempts: u32,
    /// Stop after this many newly fetched rounds (tests of resume).
    pub max_new: Option<u64>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            endpoints: vec![],
            rate: 20.0,
            fetcher: Fetcher::Native,
            allow_public: false,
            attempts: 6,
            max_new: None,
        }
    }
}

/// What a fetch did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub plan: Plan,
    /// Rounds present before this run (resume) and fetched by it.
    pub resumed: u64,
    pub fetched: u64,
    /// Responses refused: not a beacon, wrong round, or not verifying.
    pub rejected: u64,
    /// The archive was completed and written.
    pub complete: bool,
}

/// Whether a URL's host is loopback (`127.0.0.1`, `[::1]`, `localhost`).
pub fn is_loopback(url: &str) -> bool {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let host_port = rest.split('/').next().unwrap_or("");
    let host = if let Some(h) = host_port.strip_prefix('[') {
        h.split(']').next().unwrap_or("")
    } else {
        host_port.split(':').next().unwrap_or("")
    };
    matches!(host, "127.0.0.1" | "::1" | "localhost")
}

/// Refuses endpoints the options do not allow (checked before any request).
pub fn check_endpoints(o: &Options) -> Result<(), String> {
    if o.endpoints.is_empty() {
        return Err("no endpoint".into());
    }
    for e in &o.endpoints {
        if !is_loopback(e) && !o.allow_public {
            return Err(format!(
                "{e}: fetching from a public drand endpoint waits for the owner's approval (O-M1-12); \
                 pass --approved O-M1-12 once it is given"
            ));
        }
        if e.starts_with("https://") && o.fetcher == Fetcher::Native {
            return Err(format!(
                "{e}: https needs --fetcher curl (the workspace has no TLS client)"
            ));
        }
        if !(e.starts_with("http://") || e.starts_with("https://")) {
            return Err(format!("{e}: not an http(s) URL"));
        }
    }
    if !(o.rate > 0.0 && o.rate <= 1_000.0) {
        return Err("rate in (0, 1000] requests/s per endpoint".into());
    }
    if o.endpoints.iter().any(|e| !is_loopback(e)) && o.rate > 20.0 {
        return Err("public endpoints: at most 20 requests/s each (§8.7)".into());
    }
    Ok(())
}

async fn get_body(fetcher: Fetcher, url: &str) -> Result<(u16, Vec<u8>), String> {
    match fetcher {
        Fetcher::Native => fclient::http::get(url)
            .await
            .map(|r| (r.status, r.body))
            .map_err(|e| format!("{e:?}")),
        Fetcher::Curl => {
            let url = url.to_string();
            let out = tokio::task::spawn_blocking(move || {
                std::process::Command::new("curl")
                    .args(["-sS", "--max-time", "15", "-w", "\n%{http_code}", &url])
                    .output()
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("curl: {e}"))?;
            if !out.status.success() {
                return Err(format!("curl: {}", String::from_utf8_lossy(&out.stderr)));
            }
            let mut b = out.stdout;
            let nl = b
                .iter()
                .rposition(|&c| c == b'\n')
                .ok_or("curl: no status")?;
            let code = std::str::from_utf8(&b[nl + 1..])
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .ok_or("curl: bad status")?;
            b.truncate(nl);
            Ok((code, b))
        }
    }
}

/// Fetches and verifies one round from one endpoint.
pub async fn fetch_round(
    fetcher: Fetcher,
    endpoint: &str,
    info: &ChainInfo,
    r: u64,
) -> Result<Beacon, String> {
    let url = format!(
        "{}/{}/public/{r}",
        endpoint.trim_end_matches('/'),
        hex::encode(info.chain_hash)
    );
    let (status, body) = get_body(fetcher, &url).await?;
    if status != 200 {
        return Err(format!("{url}: HTTP {status}"));
    }
    let v: serde_json::Value = serde_json::from_slice(&body).map_err(|e| format!("{url}: {e}"))?;
    let b = beacon::parse_beacon_json(&v).ok_or_else(|| format!("{url}: not a beacon"))?;
    if b.round != r {
        return Err(format!("{url}: round {} instead of {r}", b.round));
    }
    if !beacon::verify(r, &b.sig48, &info.public_key) {
        return Err(format!(
            "{url}: round {r} does not verify against the pinned key"
        ));
    }
    Ok(b)
}

/// The resumable partial file: `count × 48` bytes, zero = not yet fetched
/// (no valid signature is all zeros).
fn partial_path(dir: &Path, p: &Plan) -> PathBuf {
    dir.join(format!("partial-{}-{}.bin", p.first, p.last))
}

fn load_partial(path: &Path, p: &Plan, info: &ChainInfo) -> Vec<Option<[u8; 48]>> {
    let n = p.count() as usize;
    let mut v = vec![None; n];
    let Ok(b) = std::fs::read(path) else {
        return v;
    };
    if b.len() != 48 * n {
        return v;
    }
    let sigs: Vec<[u8; 48]> = b
        .chunks_exact(48)
        .map(|c| c.try_into().expect("48"))
        .collect();
    // Re-verified on all cores: a partial file is not trusted either.
    let threads = std::thread::available_parallelism().map_or(4, |x| x.get());
    let chunk = n.div_ceil(threads).max(1);
    let pk = info.public_key;
    let first = p.first;
    let ok: Vec<bool> = std::thread::scope(|sc| {
        let hs: Vec<_> = sigs
            .chunks(chunk)
            .enumerate()
            .map(|(ci, c)| {
                sc.spawn(move || {
                    c.iter()
                        .enumerate()
                        .map(|(i, s)| {
                            *s != [0; 48] && beacon::verify(first + (ci * chunk + i) as u64, s, &pk)
                        })
                        .collect::<Vec<bool>>()
                })
            })
            .collect();
        hs.into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    });
    for (i, good) in ok.into_iter().enumerate() {
        if good {
            v[i] = Some(sigs[i]);
        }
    }
    v
}

fn save_partial(path: &Path, v: &[Option<[u8; 48]>]) -> Result<(), String> {
    let mut b = Vec::with_capacity(48 * v.len());
    for s in v {
        b.extend_from_slice(&s.unwrap_or([0; 48]));
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &b).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Shared state of the fetch workers.
struct Work {
    queue: VecDeque<u64>,
    have: Vec<Option<[u8; 48]>>,
    fetched: u64,
    rejected: u64,
    failed: Option<String>,
    in_flight: u64,
    /// Per round: failed attempts and a bitmask of the endpoints that failed it.
    failed_by: BTreeMap<u64, (u32, u64)>,
}

impl Work {
    /// The next round for endpoint `ei`: the first queued round it has not
    /// failed, or any queued round once every endpoint has failed it.
    fn take(&mut self, ei: usize, n_eps: usize) -> Option<u64> {
        let all = if n_eps >= 64 {
            u64::MAX
        } else {
            (1u64 << n_eps) - 1
        };
        let me = 1u64 << (ei % 64);
        let pos = self
            .queue
            .iter()
            .position(|r| match self.failed_by.get(r) {
                None => true,
                Some((_, mask)) => mask & me == 0 || mask & all == all,
            })?;
        let r = self.queue.remove(pos)?;
        self.in_flight += 1;
        Some(r)
    }
}

/// Fetches `plan` into the archive at `dir` (pinned to `info`), resuming a
/// partial file. Rounds already in a packed archive there are kept.
pub async fn run(dir: &Path, info: &ChainInfo, plan: Plan, o: &Options) -> Result<Report, String> {
    check_endpoints(o)?;
    if plan.first == 0 || plan.last < plan.first {
        return Err("empty plan".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let existing = if Archive::is_packed(dir) {
        Some(Archive::load(dir, info)?)
    } else {
        None
    };
    let ppath = partial_path(dir, &plan);
    let mut have = load_partial(&ppath, &plan, info);
    if let Some(a) = &existing {
        for (i, h) in have.iter_mut().enumerate() {
            if h.is_none() {
                *h = a.get(plan.first + i as u64).map(|b| b.sig48);
            }
        }
    }
    let resumed = have.iter().filter(|x| x.is_some()).count() as u64;
    let queue: VecDeque<u64> = have
        .iter()
        .enumerate()
        .filter(|(_, h)| h.is_none())
        .map(|(i, _)| plan.first + i as u64)
        .collect();
    let limit = o.max_new.unwrap_or(u64::MAX);
    let shared = Arc::new(Mutex::new(Work {
        queue,
        have,
        fetched: 0,
        rejected: 0,
        failed: None,
        in_flight: 0,
        failed_by: BTreeMap::new(),
    }));
    let interval = Duration::from_secs_f64(1.0 / o.rate);
    let n_eps = o.endpoints.len();
    let mut tasks = vec![];
    for (ei, ep) in o.endpoints.iter().enumerate() {
        let shared = shared.clone();
        let ep = ep.clone();
        let info = info.clone();
        let fetcher = o.fetcher;
        let attempts = o.attempts;
        tasks.push(tokio::spawn(async move {
            let mut next = Instant::now() + interval.mul_f64(ei as f64 / n_eps as f64);
            loop {
                let pick = {
                    let mut g = shared.lock().unwrap_or_else(|p| p.into_inner());
                    if g.failed.is_some() || g.fetched >= limit {
                        return;
                    }
                    match g.take(ei, n_eps) {
                        Some(r) => Some(r),
                        None if g.queue.is_empty() && g.in_flight == 0 => return,
                        None => None,
                    }
                };
                let Some(r) = pick else {
                    // Only rounds this endpoint already failed are left, or
                    // others are in flight: wait.
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    continue;
                };
                tokio::time::sleep_until(next.into()).await;
                next = Instant::now().max(next) + interval;
                let got = fetch_round(fetcher, &ep, &info, r).await;
                let mut g = shared.lock().unwrap_or_else(|p| p.into_inner());
                g.in_flight -= 1;
                match got {
                    Ok(b) => {
                        g.have[(r - plan.first) as usize] = Some(b.sig48);
                        g.fetched += 1;
                        g.failed_by.remove(&r);
                    }
                    Err(e) => {
                        g.rejected += 1;
                        let f = g.failed_by.entry(r).or_default();
                        f.0 += 1;
                        f.1 |= 1u64 << (ei % 64);
                        if f.0 >= attempts {
                            g.failed = Some(format!("round {r}: {e}"));
                        } else {
                            // Another endpoint (or this one, once every
                            // endpoint has failed it) retries it.
                            g.queue.push_back(r);
                        }
                    }
                }
            }
        }));
    }
    // Save progress every few seconds while the workers run.
    let saver = {
        let shared = shared.clone();
        let ppath = ppath.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                let snap = shared
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .have
                    .clone();
                let _ = save_partial(&ppath, &snap);
            }
        })
    };
    for t in tasks {
        let _ = t.await;
    }
    saver.abort();
    let (have, fetched, rejected, failed) = {
        let g = shared.lock().unwrap_or_else(|p| p.into_inner());
        (g.have.clone(), g.fetched, g.rejected, g.failed.clone())
    };
    save_partial(&ppath, &have)?;
    let complete = have.iter().all(|h| h.is_some());
    if complete {
        let mut rounds: BTreeMap<u64, [u8; 48]> = BTreeMap::new();
        if let Some(a) = &existing {
            for s in &a.segments {
                for (i, sig) in s.sigs.iter().enumerate() {
                    rounds.insert(s.first + i as u64, *sig);
                }
            }
        }
        for (i, h) in have.iter().enumerate() {
            rounds.insert(plan.first + i as u64, h.expect("complete"));
        }
        Archive::from_rounds(info.clone(), &rounds).write(dir)?;
        let _ = std::fs::remove_file(&ppath);
    }
    if let Some(e) = failed {
        return Err(format!(
            "prefetch stopped: {e} ({fetched} fetched this run, progress saved in {})",
            ppath.display()
        ));
    }
    Ok(Report {
        plan,
        resumed,
        fetched,
        rejected,
        complete,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exit_plan_is_about_250k_rounds() {
        let q = beacon::quicknet_info();
        // G0 = 2026-08-01T00:00:00Z, 1.5 pre-season days + 7 game days + 1 h.
        let p = plan(&q, 1_785_542_400, 1.5, 7.0, 3_600);
        assert_eq!(p.first, q.first_round_from(1_785_542_400));
        assert!((244_000..=250_000).contains(&p.count()), "{}", p.count());
        assert!(p.bytes() < 12_500_000);
        // ≈ 1.2 h over 3 endpoints at 20 requests/s.
        let h = p.fetch_secs(20.0, 3) / 3_600.0;
        assert!((1.0..1.3).contains(&h), "{h}");
    }

    #[test]
    fn public_endpoints_wait_for_the_owner() {
        let o = |eps: &[&str], allow: bool, f: Fetcher| Options {
            endpoints: eps.iter().map(|s| s.to_string()).collect(),
            allow_public: allow,
            fetcher: f,
            ..Options::default()
        };
        assert!(check_endpoints(&o(&["http://127.0.0.1:41020"], false, Fetcher::Native)).is_ok());
        assert!(check_endpoints(&o(&["http://[::1]:41020/"], false, Fetcher::Native)).is_ok());
        let e = check_endpoints(&o(&["https://api.drand.sh"], false, Fetcher::Curl)).unwrap_err();
        assert!(e.contains("O-M1-12"), "{e}");
        assert!(check_endpoints(&o(&["https://api.drand.sh"], true, Fetcher::Native)).is_err());
        assert!(check_endpoints(&o(&["https://api.drand.sh"], true, Fetcher::Curl)).is_ok());
        let mut fast = o(&["https://api.drand.sh"], true, Fetcher::Curl);
        fast.rate = 50.0;
        assert!(
            check_endpoints(&fast).is_err(),
            "≤ 20/s on public endpoints"
        );
        assert!(!is_loopback("http://127.0.0.2.example.com/"));
        assert!(!is_loopback("http://drand.cloudflare.com"));
    }
}
