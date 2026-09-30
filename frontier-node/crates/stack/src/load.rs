//! `frontier-stack load`: the herald viewer load (M1 contract §13.4
//! criterion 6, Gate W5): `frontier-viewers` with 4/5 polling viewers and
//! 1/5 WebSocket viewers for `--game-hours` of game time (wall = game /
//! scale), its stats on the stack's viewers port, the herald's fold lag
//! sampled every second meanwhile, then the targets.
//!
//! Targets: p99 file latency ≤ 250 ms, error rate < 0.1%, ingest → WS
//! p99 ≤ 2 s. The generator measures file latency, errors and WS
//! delivery counts; **ingest → WS is measured here as the herald's fold
//! lag** (the program's newest transaction slot − the herald's last
//! folded slot, × 0.4 s of wall per slot), sampled once a second: the WS fan-out after the fold is not in
//! it (reported as such).
//!
//! W6T-4 (§13.4 A3): the generator gets the recovery budget (chaos restart
//! maximum at the run's scale + 2 s), the think time, the herald to follow
//! for the live bell, the opened provinces and every ring; the error rate
//! is over requests + WS sessions; every second the herald's open WS count
//! and the generator's counters are sampled, and criterion 6 fails when WS
//! viewers are connected < 99% of the window outside the herald outage
//! windows (chaos kills in `events.jsonl`) or when stale retries or WS
//! reconnects happen outside them.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::run::RunDir;

pub const P99_FILE_MS: f64 = 250.0;
pub const INGEST_P99_S: f64 = 2.0;
pub const ERROR_RATE: f64 = 0.001;
/// WS viewers connected over the window outside the outages (§13.4 A3).
pub const WS_COVERAGE: f64 = 0.99;

/// Spawns `frontier-viewers` for `wall_secs`; stdout (its JSON report) to
/// `load/<tag>.json`.
pub fn spawn_viewers(
    st: &crate::up::Stack,
    viewers: usize,
    wall_secs: f64,
    bell: u32,
    tag: &str,
    plan: &ViewerPlan,
) -> Result<Child, String> {
    spawn_with(
        &st.bin.join("frontier-viewers"),
        &st.run,
        st.ports.herald,
        st.ports.viewers,
        viewers,
        wall_secs,
        bell,
        tag,
        plan,
    )
}

/// What the viewers are told beyond their counts (W6T-4, §13.4 A3): the
/// recovery budget, the think time, whether to follow the live bell from
/// the herald's `/h/status`, and the opened provinces and all rings.
/// `frontier-viewers` ignores flags it does not know (an older build runs
/// with its defaults; the verdict says so: `generator_recovery`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewerPlan {
    pub retry_budget_ms: u64,
    pub think_ms: u64,
    /// The herald's base URL, when the viewers follow its live bell.
    pub follow_status: Option<String>,
    pub provinces: Vec<(i16, i16)>,
    pub rings: Vec<u16>,
}

/// The `frontier-viewers` arguments of one window.
#[allow(clippy::too_many_arguments)]
pub fn viewer_args(
    herald: u16,
    stats: u16,
    viewers: usize,
    wall_secs: f64,
    bell: u32,
    plan: &ViewerPlan,
) -> Vec<String> {
    let ws = viewers / 5;
    let polling = viewers - ws;
    let mut a: Vec<String> = vec![
        "--herald".into(),
        format!("127.0.0.1:{herald}"),
        "--viewers".into(),
        polling.to_string(),
        "--ws".into(),
        ws.to_string(),
        "--seconds".into(),
        (wall_secs.ceil().max(1.0) as u64).to_string(),
        "--bells".into(),
        bell.max(1).to_string(),
        "--stats".into(),
        format!("127.0.0.1:{stats}"),
        // W6T-4 (§13.4 A3): the client's standard recovery budget and the
        // think time; the load follows the live bell over the opened
        // provinces and every ring.
        "--retry-budget-ms".into(),
        plan.retry_budget_ms.to_string(),
        "--think-ms".into(),
        plan.think_ms.to_string(),
    ];
    if let Some(u) = &plan.follow_status {
        a.push("--follow-status".into());
        a.push(u.clone());
    }
    if !plan.provinces.is_empty() {
        a.push("--provinces".into());
        a.push(
            plan.provinces
                .iter()
                .map(|(p, q)| format!("{p},{q}"))
                .collect::<Vec<_>>()
                .join(";"),
        );
    }
    if !plan.rings.is_empty() {
        a.push("--rings".into());
        a.push(
            plan.rings
                .iter()
                .map(|r| r.to_string())
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    a
}

#[allow(clippy::too_many_arguments)]
fn spawn_with(
    bin: &Path,
    run: &RunDir,
    herald: u16,
    stats: u16,
    viewers: usize,
    wall_secs: f64,
    bell: u32,
    tag: &str,
    plan: &ViewerPlan,
) -> Result<Child, String> {
    let dir = run.path("load");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let out = std::fs::File::create(dir.join(format!("{tag}.json"))).map_err(|e| e.to_string())?;
    let err = std::fs::File::create(dir.join(format!("{tag}.log"))).map_err(|e| e.to_string())?;
    let args = viewer_args(herald, stats, viewers, wall_secs, bell, plan);
    let _ = std::fs::write(
        dir.join(format!("{tag}.args.json")),
        json!(args).to_string(),
    );
    #[cfg(unix)]
    use std::os::unix::process::CommandExt;
    let mut c = Command::new(bin);
    c.args(&args).stdin(Stdio::null()).stdout(out).stderr(err);
    #[cfg(unix)]
    c.process_group(0);
    c.spawn().map_err(|e| format!("{}: {e}", bin.display()))
}

/// The viewer plan of a stack: the opened provinces (sorted) and every ring
/// from 0 to the outermost opened one.
pub fn viewer_plan(
    retry_budget_ms: u64,
    think_ms: u64,
    follow_status: bool,
    herald: u16,
    ps: &[fclient::decode::Province],
) -> ViewerPlan {
    let mut provinces: Vec<(i16, i16)> = ps.iter().map(|p| (p.p, p.q)).collect();
    provinces.sort_unstable();
    provinces.dedup();
    let max_ring = ps.iter().map(|p| p.ring).max();
    ViewerPlan {
        retry_budget_ms,
        think_ms,
        follow_status: follow_status.then(|| format!("http://127.0.0.1:{herald}")),
        provinces,
        rings: max_ring.map_or_else(Vec::new, |r| (0..=r).collect()),
    }
}

pub fn plan_json(p: &ViewerPlan) -> Value {
    json!({"retry_budget_ms": p.retry_budget_ms, "think_ms": p.think_ms, "follow_status": p.follow_status,
           "provinces": p.provinces.len(), "rings": p.rings})
}

/// One per-second sample of the viewer window (W6T-4): the herald's open
/// WS count and fold lag, and the generator's running counters from its
/// `/stats` (cumulative), at a wall time.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sample {
    pub wall_ms: u64,
    pub ws_open: Option<u64>,
    pub lag_s: Option<f64>,
    pub stale_retries: Option<u64>,
    pub ws_reconnects: Option<u64>,
    pub errors: Option<u64>,
}

impl Sample {
    pub fn to_json(&self) -> Value {
        json!({"wall_ms": self.wall_ms, "ws_open": self.ws_open, "lag_s": self.lag_s,
               "stale_retries": self.stale_retries, "ws_reconnects": self.ws_reconnects, "errors": self.errors})
    }
    pub fn from_json(v: &Value) -> Option<Sample> {
        Some(Sample {
            wall_ms: v["wall_ms"].as_u64()?,
            ws_open: v["ws_open"].as_u64(),
            lag_s: v["lag_s"].as_f64(),
            stale_retries: v["stale_retries"].as_u64(),
            ws_reconnects: v["ws_reconnects"].as_u64(),
            errors: v["errors"].as_u64(),
        })
    }
}

/// The viewer window's coverage evidence: the WS viewers asked for, the
/// per-second samples and the herald outage windows (wall ms) chaos made.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Coverage {
    pub ws_viewers: u64,
    pub samples: Vec<Sample>,
    pub outages: Vec<(u64, u64)>,
    /// The generator's run (wall ms): its start and start + `--seconds`
    /// (its WS viewers close at the end while the pollers finish their
    /// last think); `None`: the first and last sample.
    pub window: Option<(u64, u64)>,
}

/// The herald outage windows of a run (wall ms), from `events.jsonl`: each
/// chaos kill of the herald until its restart plus the time a viewer needs
/// to notice and recover (the retry budget, 1.5 think times for a poller's
/// next request on its stale connection, and a second of slack). A kill
/// without a restart is open to the end.
pub fn outage_windows(events: &[Value], retry_budget_ms: u64, think_ms: u64) -> Vec<(u64, u64)> {
    let is_herald = |e: &Value| e["detail"]["component"] == "herald";
    let grace = retry_budget_ms + think_ms * 3 / 2 + 1_000;
    let mut out = vec![];
    for (i, e) in events.iter().enumerate() {
        if e["event"] != "chaos-kill" || !is_herald(e) {
            continue;
        }
        let Some(k) = e["wall_ms"].as_u64() else {
            continue;
        };
        let back = events[i + 1..]
            .iter()
            .find(|x| x["event"] == "restart" && is_herald(x))
            .and_then(|x| x["wall_ms"].as_u64());
        out.push((k, back.map_or(u64::MAX, |r| r.saturating_add(grace))));
    }
    out
}

/// The coverage and recovery checks of a viewer window (W6T-4, §13.4 A3):
/// - **WS coverage**: the herald's open WS count over the WS viewers,
///   averaged over the samples outside the outage windows (the first 5 s
///   and last 3 s of the window are the generator's ramp and wind-down and
///   are left out); ≥ 99% required;
/// - **recovery outside the outages**: stale keep-alive retries and WS
///   reconnects the generator counted at a sample outside every outage
///   window mean the herald dropped connections by itself: flagged, and
///   criterion 6 fails.
pub fn coverage_check(cov: &Coverage) -> Value {
    let inside = |t: u64| cov.outages.iter().any(|(a, b)| t >= *a && t <= *b);
    let (first, last) = cov.window.unwrap_or_else(|| {
        (
            cov.samples.iter().map(|s| s.wall_ms).min().unwrap_or(0),
            cov.samples.iter().map(|s| s.wall_ms).max().unwrap_or(0),
        )
    });
    let w = cov.ws_viewers;
    let (mut n, mut sum, mut in_outage, mut unanswered) = (0u64, 0f64, 0u64, 0u64);
    for s in &cov.samples {
        if s.wall_ms < first + 5_000 || s.wall_ms + 3_000 > last {
            continue;
        }
        if inside(s.wall_ms) {
            in_outage += 1;
            continue;
        }
        match s.ws_open {
            Some(o) => {
                n += 1;
                sum += if w == 0 {
                    1.0
                } else {
                    o.min(w) as f64 / w as f64
                };
            }
            None => unanswered += 1,
        }
    }
    let coverage = (n > 0).then(|| sum / n as f64);
    let coverage_ok = w == 0 || coverage.is_some_and(|c| c >= WS_COVERAGE);
    let deltas = |get: &dyn Fn(&Sample) -> Option<u64>| {
        let (mut prev, mut ins, mut out) = (0u64, 0u64, 0u64);
        let mut when: Vec<u64> = vec![];
        for s in &cov.samples {
            if let Some(x) = get(s) {
                let d = x.saturating_sub(prev);
                if d > 0 {
                    if inside(s.wall_ms) {
                        ins += d;
                    } else {
                        out += d;
                        when.push(s.wall_ms);
                    }
                }
                prev = x;
            }
        }
        (ins, out, when)
    };
    let (stale_in, stale_out, stale_when) = deltas(&|s| s.stale_retries);
    let (rec_in, rec_out, rec_when) = deltas(&|s| s.ws_reconnects);
    json!({
        "ws_viewers": w, "ws_coverage": coverage, "ws_coverage_target": WS_COVERAGE, "ws_coverage_ok": coverage_ok,
        "coverage_samples": n, "samples_in_outages": in_outage, "samples_unanswered": unanswered,
        "outage_windows": cov.outages.iter().map(|(a, b)| json!([a, if *b == u64::MAX { Value::Null } else { json!(b) }])).collect::<Vec<_>>(),
        "stale_retries_inside": stale_in, "stale_retries_outside": stale_out,
        "ws_reconnects_inside": rec_in, "ws_reconnects_outside": rec_out,
        "recovery_outside_at_wall_ms": stale_when.iter().chain(rec_when.iter()).take(20).collect::<Vec<_>>(),
    })
}

/// A `/stats` or `/h/status` probe of the running window.
pub async fn probe(
    herald: u16,
    stats: u16,
    chain: &crate::chain::Chain,
    program: &fclient::Address,
) -> Sample {
    let get = |url: String| async move {
        let r = tokio::time::timeout(Duration::from_secs(3), fclient::http::get(&url))
            .await
            .ok()?
            .ok()?;
        serde_json::from_slice::<Value>(&r.body).ok()
    };
    let newest = chain.latest_program_slot(program).await.ok().flatten();
    let h = get(format!("http://127.0.0.1:{herald}/h/status")).await;
    let v = get(format!("http://127.0.0.1:{stats}/stats")).await;
    let lag_s = match (newest, h.as_ref().and_then(|h| h["lastSlot"].as_u64())) {
        (Some(n), Some(x)) => Some(n.saturating_sub(x) as f64 * 0.4),
        _ => None,
    };
    Sample {
        wall_ms: crate::run::wall_ms(),
        ws_open: h.as_ref().and_then(|h| h["ws"]["open"].as_u64()),
        lag_s,
        stale_retries: v.as_ref().and_then(|v| v["staleRetries"].as_u64()),
        ws_reconnects: v.as_ref().and_then(|v| v["ws"]["reconnects"].as_u64()),
        errors: v
            .as_ref()
            .and_then(|v| Some(v["errors"].as_u64()? + v["ws"]["errors"].as_u64().unwrap_or(0))),
    }
}

/// p-quantile of `v` (nearest rank).
pub fn quantile(v: &[f64], q: f64) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let i = ((q * s.len() as f64).ceil() as usize).clamp(1, s.len()) - 1;
    Some(s[i])
}

/// The verdict on one load report plus the fold-lag samples (seconds).
///
/// Wave-5 review of W5-B:
/// - **ingest → WS** is the WS messages' own stamp (`t`, the herald's
///   ingest time; `ws.ingest_p99_upper_ms`, the histogram bucket's upper
///   bound) when the generator timed any; the fold lag is the fallback
///   and is labelled as such;
/// - the file p99 is the bucket's upper bound when reported;
/// - **WS gaps** (a sequence number skipped) must be 0;
/// - **notFound** (404) are requests for a per-bell file of a bell in which
///   that province or overview did not change: §8.4 writes per-bell files
///   only for such bells, so a 404 there is the contract's answer, counted
///   and reported, not an error.
pub fn judge(rep: &Value, lag_secs: &[f64], cov: Option<&Coverage>) -> Value {
    let req = rep["requests"].as_f64().unwrap_or(0.0);
    let ws_errs = rep["ws"]["errors"].as_f64().unwrap_or(0.0);
    let errs = rep["errors"].as_f64().unwrap_or(0.0) + ws_errs;
    // W6T-4 (§13.4 A3): the generator's denominator, every request and
    // every WS session (connected or failed).
    let sessions = rep["ws"]["connected"].as_f64().unwrap_or(0.0) + ws_errs;
    let denom = req + sessions;
    let rate = if denom > 0.0 { errs / denom } else { 1.0 };
    let p99 = rep["p99_upper_ms"]
        .as_f64()
        .or_else(|| rep["p99_ms"].as_f64())
        .unwrap_or(f64::INFINITY);
    let timed = rep["ws"]["timed"].as_f64().unwrap_or(0.0);
    let ws_p99 = rep["ws"]["ingest_p99_upper_ms"]
        .as_f64()
        .or_else(|| rep["ws"]["ingest_p99_ms"].as_f64())
        .map(|ms| ms / 1_000.0);
    let fold = quantile(lag_secs, 0.99);
    let (ingest, how) = if timed > 0.0 {
        (ws_p99, "ws-stamp")
    } else {
        (fold, "fold-lag")
    };
    let gaps = rep["ws"]["gaps"].as_u64().unwrap_or(0);
    let not_found = rep["notFound"].as_u64().unwrap_or(0);
    let ok_file = p99 <= P99_FILE_MS;
    let ok_err = req > 0.0 && rate < ERROR_RATE;
    let ok_ingest = ingest.is_some_and(|x| x <= INGEST_P99_S);
    let ok_gaps = gaps == 0;
    let cc = cov.map(coverage_check);
    let ok_cov = cc.as_ref().is_none_or(|c| c["ws_coverage_ok"] == true);
    let (stale_out, rec_out) = cc.as_ref().map_or((0, 0), |c| {
        (
            c["stale_retries_outside"].as_u64().unwrap_or(0),
            c["ws_reconnects_outside"].as_u64().unwrap_or(0),
        )
    });
    let ok_recovery = stale_out == 0 && rec_out == 0;
    let mut misses = vec![];
    if !ok_file {
        misses.push(format!("file p99 {p99} ms > {P99_FILE_MS} ms"));
    }
    if !ok_err {
        misses.push(format!("error rate {rate} (requests {req})"));
    }
    if !ok_ingest {
        misses.push(format!(
            "ingest -> WS p99 {ingest:?} s ({how}) > {INGEST_P99_S} s"
        ));
    }
    if !ok_gaps {
        misses.push(format!("{gaps} WS gaps"));
    }
    if let Some(c) = &cc {
        if !ok_cov {
            misses.push(match c["ws_coverage"].as_f64() {
                Some(x) => format!(
                    "WS coverage {:.1}% < {:.0}% outside the herald outage windows",
                    x * 100.0,
                    WS_COVERAGE * 100.0
                ),
                None => {
                    "WS coverage not measured (no herald ws.open sample outside the outages)".into()
                }
            });
        }
        if !ok_recovery {
            misses.push(format!(
                "{stale_out} stale-connection retries and {rec_out} WS reconnects outside the herald outage windows (the herald dropped connections by itself)"
            ));
        }
    }
    let recovery = rep.get("staleRetries").is_some();
    let mut v = json!({
        "p99_file_ms": p99, "p99_file_target_ms": P99_FILE_MS, "p99_file_ok": ok_file,
        "requests": req, "ws_sessions": sessions, "denominator": denom,
        "errors": errs, "error_rate": rate, "error_rate_target": ERROR_RATE, "error_rate_ok": ok_err,
        "generator_recovery": recovery,
        "generator_recovery_note": if recovery { "the generator retries stale keep-alive connections and reconnects WS within its budget (§13.4 A3)" } else { "this frontier-viewers has no recovery (pre-W6T-3 build): a herald kill is counted as errors and drops its WS viewers" },
        "stale_retries": rep["staleRetries"], "unavailable": rep["unavailable"], "unavailable_ms": rep["unavailable_ms"],
        "ws_reconnects": rep["ws"]["reconnects"],
        "coverage": cc, "ws_coverage": cc.as_ref().map(|c| c["ws_coverage"].clone()), "ws_coverage_ok": ok_cov,
        "stale_retries_outside": stale_out, "ws_reconnects_outside": rec_out, "recovery_ok": ok_recovery,
        "ingest_p99_s": ingest, "ingest_measure": how, "ingest_target_s": INGEST_P99_S, "ingest_ok": ok_ingest,
        "ingest_lag_p99_s": fold, "ingest_lag_samples": lag_secs.len(),
        "ws_timed": timed, "ws_gaps": gaps, "ws_gaps_ok": ok_gaps,
        "not_found": not_found,
        "not_found_note": "404 for a per-bell file of a bell without a change: the contract's answer (§8.4), not an error",
        "ingest_note": "ws-stamp: the WS message's ingest stamp t to its receipt; fold-lag (fallback): newest program tx slot - last folded slot, x 0.4 s, sampled each second",
        "summary": format!("p99 file {p99:.1} ms (answered only {}), ingest->WS p99 {} s ({how}; herald share p99 {} ms, delivery p99 {} ms), error rate {rate:.5}, gaps {gaps}, 404 {not_found}, WS coverage {}, recovery outside outages {}",
            rep["answered"]["p99_upper_ms"].as_f64().map_or("-".into(), |x| format!("{x:.1} ms")),
            ingest.map_or("-".into(), |x| format!("{x:.2}")),
            rep["ws"]["herald_p99_upper_ms"].as_f64().map_or("-".into(), |x| format!("{x:.0}")),
            rep["ws"]["delivery_p99_upper_ms"].as_f64().map_or("-".into(), |x| format!("{x:.0}")),
            cc.as_ref().and_then(|c| c["ws_coverage"].as_f64()).map_or("-".into(), |x| format!("{:.1}%", x * 100.0)),
            stale_out + rec_out),
        "misses": misses,
        "pass": ok_file && ok_err && ok_ingest && ok_gaps && ok_cov && ok_recovery,
    });
    // integ-W6t review (reported, not judged): the answered requests' own
    // file latency (the 404s are cheap and pull the p99 down), and who owns
    // the ingest → WS tail: the herald (`s − t`) or the delivery to this
    // generator (receipt − `s`).
    v["p99_file_answered_ms"] = rep["answered"]["p99_upper_ms"].clone();
    v["file_answered"] = rep["answered"]["requests"].clone();
    let w = &rep["ws"];
    v["ws_split"] = json!({"send_stamped": w["send_stamped"],
        "herald_p50_ms": w["herald_p50_ms"], "herald_p99_upper_ms": w["herald_p99_upper_ms"], "herald_max_ms": w["herald_max_ms"],
        "delivery_p50_ms": w["delivery_p50_ms"], "delivery_p99_upper_ms": w["delivery_p99_upper_ms"], "delivery_max_ms": w["delivery_max_ms"]});
    v
}

/// One fold-lag sample (seconds): the program's newest transaction slot
/// minus the herald's last folded slot, × 0.4 s.
pub async fn fold_lag(
    chain: &crate::chain::Chain,
    program: &fclient::Address,
    herald: u16,
) -> Option<f64> {
    let url = format!("http://127.0.0.1:{herald}/h/status");
    let newest = chain.latest_program_slot(program).await.ok()??;
    let r = tokio::time::timeout(Duration::from_secs(3), fclient::http::get(&url))
        .await
        .ok()?
        .ok()?;
    let v: Value = serde_json::from_slice(&r.body).ok()?;
    Some(newest.saturating_sub(v["lastSlot"].as_u64()?) as f64 * 0.4)
}

/// `load --run-id R --viewers N --game-hours H`.
pub async fn load(rd: &RunDir, viewers: usize, game_hours: f64) -> i32 {
    let st = match rd.load_state() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("frontier-stack: {e}");
            return 2;
        }
    };
    let scale = st["config"]["scale"].as_f64().unwrap_or(20.0);
    let herald = st["ports"]["herald"].as_u64().unwrap_or(41_040) as u16;
    let stats = st["ports"]["viewers"].as_u64().unwrap_or(41_075) as u16;
    let rpc = format!(
        "http://127.0.0.1:{}",
        st["ports"]["localnet"].as_u64().unwrap_or(41_010)
    );
    if let Err(e) = crate::ports::check(stats) {
        eprintln!("frontier-stack: viewers stats port: {e}");
        return 2;
    }
    let program: fclient::Address = st["program"]
        .as_str()
        .and_then(|p| p.parse().ok())
        .unwrap_or_default();
    let chain = crate::chain::Chain::new(&rpc, program);
    let before = chain.status().await.ok();
    let now = before.map(|s| s.now).unwrap_or(0);
    // `up` pauses a complete run; the load needs live blocks for the
    // herald to ingest, so the chain runs for the load and is paused again.
    let resumed = before.is_some_and(|s| s.paused);
    if resumed {
        if let Err(e) = chain.resume().await {
            eprintln!("frontier-stack: resume for the load: {e}");
            return 2;
        }
    }
    let genesis = st["play"]["genesis_ts"].as_i64().unwrap_or(now);
    let bell_secs = st["season"]["bell_secs"].as_i64().unwrap_or(600).max(1);
    let bell = ((now - genesis).max(0) / bell_secs) as u32;
    let wall = game_hours * 3_600.0 / scale;
    let bin = match crate::run::bin_dir() {
        Ok(b) => b.join("frontier-viewers"),
        Err(e) => {
            eprintln!("frontier-stack: {e}");
            return 2;
        }
    };
    let n = std::fs::read_dir(rd.path("load"))
        .map(|d| d.count())
        .unwrap_or(0);
    let tag = format!("load-{}", n / 2 + 1);
    eprintln!(
        "frontier-stack: {viewers} viewers ({} polling, {} WS) for {game_hours} game hours = {wall:.0} s of wall time at {scale}x",
        viewers - viewers / 5,
        viewers / 5
    );
    let season_id = st["season_id"].as_u64().unwrap_or(7);
    let ps = chain
        .provinces(&program, season_id)
        .await
        .unwrap_or_default();
    let c = &st["config"];
    let scale_c = c["scale"].as_f64().unwrap_or(scale).max(1e-9);
    let budget = c["viewer_flags"]["retry_budget_ms"]
        .as_u64()
        .unwrap_or_else(|| {
            (c["chaos"]["restart_max_secs"].as_f64().unwrap_or(60.0) / scale_c * 1_000.0).ceil()
                as u64
                + 2_000
        });
    let think = c["viewer_flags"]["think_ms"].as_u64().unwrap_or(5_000);
    let follow = c["viewer_flags"]["follow_status"].as_bool().unwrap_or(true);
    let plan = viewer_plan(budget, think, follow, herald, &ps);
    let spawn_ms = crate::run::wall_ms();
    let mut child = match spawn_with(&bin, rd, herald, stats, viewers, wall, bell, &tag, &plan) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("frontier-stack: {e}");
            return 2;
        }
    };
    let mut stv = st.clone();
    let pids = stv["viewers_pids"].as_array().cloned().unwrap_or_default();
    let mut pids = pids;
    pids.push(json!(child.id()));
    stv["viewers_pids"] = json!(pids);
    let _ = rd.save_state(&stv);
    let t0 = Instant::now();
    let mut samples: Vec<Sample> = vec![];
    let code = loop {
        if let Ok(Some(s)) = child.try_wait() {
            break s.code();
        }
        let smp = probe(herald, stats, &chain, &program).await;
        crate::up::append(
            &rd.path(&format!("load/{tag}.samples.jsonl")),
            &smp.to_json(),
        );
        samples.push(smp);
        if t0.elapsed() > Duration::from_secs_f64(wall + 300.0) {
            let _ = child.kill();
            break None;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    };
    let lags: Vec<f64> = samples.iter().filter_map(|s| s.lag_s).collect();
    let rep: Value = std::fs::read_to_string(rd.path(&format!("load/{tag}.json")))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    if resumed {
        let _ = chain.pause().await;
    }
    let cov = Coverage {
        ws_viewers: (viewers / 5) as u64,
        samples,
        outages: outage_windows(&rd.events(), budget, think),
        window: Some((spawn_ms, spawn_ms + (wall.ceil().max(1.0) as u64) * 1_000)),
    };
    let verdict = judge(&rep, &lags, Some(&cov));
    // Wave-5 review: a load after `up` completed runs against a chain whose
    // play and drain are over (bots stopped): it says nothing about
    // criterion 6 under play. The in-run window (`up --viewers`) is the
    // criterion-6 evidence.
    let window = if st["phase"] == "complete" || resumed {
        "post-play"
    } else {
        "live"
    };
    let out = json!({"tag": tag, "window": window, "criterion6_evidence": false,
        "note": "a `load` is reported, not criterion-6 evidence (that is the in-run window of `up --viewers`)",
        "chain_resumed_for_load": resumed, "viewers": viewers, "game_hours": game_hours, "wall_secs": wall, "scale": scale,
        "exit": code, "generator": rep, "plan": plan_json(&plan), "verdict": verdict});
    let _ = crate::run::write_atomic(
        &rd.path(&format!("load/{tag}.verdict.json")),
        serde_json::to_string_pretty(&out)
            .unwrap_or_default()
            .as_bytes(),
    );
    rd.event(Some(now), "load", out.clone());
    println!(
        "{}",
        serde_json::to_string_pretty(&verdict).unwrap_or_default()
    );
    if verdict["pass"] == true {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantiles_and_verdicts() {
        assert_eq!(quantile(&[], 0.99), None);
        let v: Vec<f64> = (1..=100).map(|x| x as f64).collect();
        assert_eq!(quantile(&v, 0.99), Some(99.0));
        assert_eq!(quantile(&v, 0.5), Some(50.0));
        let good = json!({"requests": 100_000, "errors": 10, "p99_ms": 12.0, "ws": {"errors": 0}});
        assert_eq!(judge(&good, &[0.4, 0.8], None)["pass"], true);
        let slow = json!({"requests": 100_000, "errors": 10, "p99_ms": 300.0, "ws": {"errors": 0}});
        assert_eq!(judge(&slow, &[0.4], None)["pass"], false);
        let errs = json!({"requests": 1_000, "errors": 5, "p99_ms": 10.0, "ws": {"errors": 0}});
        assert_eq!(judge(&errs, &[0.4], None)["pass"], false);
        let lag = json!({"requests": 100_000, "errors": 0, "p99_ms": 10.0, "ws": {"errors": 0}});
        assert_eq!(judge(&lag, &[0.4, 3.2], None)["pass"], false);
        assert_eq!(
            judge(&lag, &[], None)["pass"],
            false,
            "no lag samples is not a pass"
        );
        assert_eq!(judge(&Value::Null, &[0.4], None)["pass"], false);
        // Wave-5 review: the WS stamp wins over the fold lag when timed,
        // gaps fail, 404 are counted but not errors, upper bounds judge.
        let ws = json!({"requests": 100_000, "errors": 0, "notFound": 567, "p99_ms": 245.8, "p99_upper_ms": 253.9,
            "ws": {"errors": 0, "timed": 1000, "gaps": 0, "ingest_p99_ms": 900.0, "ingest_p99_upper_ms": 950.0}});
        let v = judge(&ws, &[5.0], None);
        assert_eq!(v["ingest_measure"], "ws-stamp");
        assert_eq!(v["p99_file_ok"], false, "the upper bound 253.9 > 250");
        assert_eq!(v["not_found"], 567);
        assert_eq!(v["error_rate"], 0.0);
        let mut ok = ws.clone();
        ok["p99_upper_ms"] = json!(20.0);
        assert_eq!(
            judge(&ok, &[5.0], None)["pass"],
            true,
            "the fold lag is not used when WS is timed"
        );
        ok["ws"]["gaps"] = json!(2);
        assert_eq!(judge(&ok, &[0.1], None)["pass"], false, "gaps fail");
    }

    /// The w6-s7 in-run generator report (`load/in-run.json`).
    fn w6s7() -> Value {
        json!({"elapsed_s": 4327.402541791, "errorRate": 0.00260228497969784, "errors": 8000,
            "max_ms": 845.934, "notFound": 202350, "p50_ms": 0.176, "p99_ms": 6.912, "p99_upper_ms": 7.168,
            "requests": 3456499, "viewers": 4000,
            "ws": {"connected": 1000, "errors": 1000, "gaps": 0, "ingest_p99_ms": 393.216,
                   "ingest_p99_upper_ms": 409.6, "messages": 1942352, "timed": 1942352},
            "wsViewers": 1000})
    }

    /// A clean report of the fixed generator (recovery counters present).
    fn clean() -> Value {
        json!({"requests": 100_000, "errors": 0, "p99_ms": 7.0, "p99_upper_ms": 7.2,
            "staleRetries": 0, "unavailable": 0, "unavailable_ms": 0.0, "errorSeconds": [],
            "ws": {"connected": 1000, "errors": 0, "gaps": 0, "timed": 5000, "reconnects": 0,
                   "ingest_p99_ms": 400.0, "ingest_p99_upper_ms": 409.6}})
    }

    fn samples(
        n: u64,
        open: u64,
        stale: impl Fn(u64) -> u64,
        rec: impl Fn(u64) -> u64,
    ) -> Vec<Sample> {
        (0..n)
            .map(|i| Sample {
                wall_ms: 1_000_000 + i * 1_000,
                ws_open: Some(open),
                lag_s: Some(0.4),
                stale_retries: Some(stale(i)),
                ws_reconnects: Some(rec(i)),
                errors: Some(0),
            })
            .collect()
    }

    /// W6T-4 (§13.4 A3): the error rate is over requests + WS sessions,
    /// the generator's own denominator (w6-s7: 0.0026023, not 0.0026038).
    #[test]
    fn denominator_includes_ws_sessions() {
        let v = judge(&w6s7(), &[0.4], None);
        let want = 9_000.0 / (3_456_499.0 + 1_000.0 + 1_000.0);
        assert!(
            (v["error_rate"].as_f64().unwrap() - want).abs() < 1e-12,
            "{v}"
        );
        assert!(
            (v["error_rate"].as_f64().unwrap() - w6s7()["errorRate"].as_f64().unwrap()).abs()
                < 1e-12,
            "the generator's own figure"
        );
        assert_eq!(v["denominator"], 3_458_499.0);
        assert_eq!(v["pass"], false, "9,000 errors are still errors");
    }

    /// W6T-4: WS viewers connected for 12% of the window (w6-s7: 1,000 open
    /// for 17 of ~144 bells, then 0) fail criterion 6 even with no error.
    #[test]
    fn ws_open_12pct_fails() {
        let mut s = samples(100, 1_000, |_| 0, |_| 0);
        for x in s.iter_mut().skip(20) {
            x.ws_open = Some(0);
        }
        let cov = Coverage {
            ws_viewers: 1_000,
            samples: s,
            outages: vec![],
            window: None,
        };
        let v = judge(&clean(), &[0.4], Some(&cov));
        assert_eq!(v["pass"], false, "{v}");
        assert_eq!(v["ws_coverage_ok"], false);
        assert!(v["ws_coverage"].as_f64().unwrap() < 0.25, "{v}");
        assert!(v["misses"].to_string().contains("WS coverage"), "{v}");
        // Fully connected: pass.
        let cov = Coverage {
            ws_viewers: 1_000,
            samples: samples(100, 1_000, |_| 0, |_| 0),
            outages: vec![],
            window: None,
        };
        let v = judge(&clean(), &[0.4], Some(&cov));
        assert_eq!(v["pass"], true, "{v}");
        assert_eq!(v["ws_coverage"], 1.0);
        // Closed inside an outage window only: not counted against it.
        let mut s = samples(100, 1_000, |_| 0, |_| 0);
        for x in s.iter_mut().skip(40).take(5) {
            x.ws_open = Some(0);
        }
        let cov = Coverage {
            ws_viewers: 1_000,
            samples: s,
            outages: vec![(1_039_500, 1_045_500)],
            window: None,
        };
        assert_eq!(judge(&clean(), &[0.4], Some(&cov))["ws_coverage_ok"], true);
        // The generator's WS viewers close at its --seconds while its
        // pollers finish: samples after the window end do not count.
        let mut s = samples(44, 1_000, |_| 0, |_| 0);
        for x in s.iter_mut().skip(37) {
            x.ws_open = Some(0);
        }
        let cov = Coverage {
            ws_viewers: 1_000,
            samples: s,
            outages: vec![],
            window: Some((1_000_000, 1_036_000)),
        };
        assert_eq!(judge(&clean(), &[0.4], Some(&cov))["ws_coverage"], 1.0);
        // Sampled but never answered: not measured, not a pass.
        let mut s = samples(30, 1_000, |_| 0, |_| 0);
        for x in s.iter_mut() {
            x.ws_open = None;
        }
        let cov = Coverage {
            ws_viewers: 1_000,
            samples: s,
            outages: vec![],
            window: None,
        };
        assert_eq!(judge(&clean(), &[0.4], Some(&cov))["pass"], false);
    }

    /// W6T-4: stale-connection retries and WS reconnects inside a herald
    /// kill window are the recovery the criterion allows (§13.4 A3).
    #[test]
    fn stale_retries_inside_kill_window_pass() {
        let mut rep = clean();
        rep["staleRetries"] = json!(4_000);
        rep["ws"]["reconnects"] = json!(1_000);
        let s = samples(
            100,
            1_000,
            |i| if i >= 52 { 4_000 } else { 0 },
            |i| if i >= 51 { 1_000 } else { 0 },
        );
        let cov = Coverage {
            ws_viewers: 1_000,
            samples: s,
            outages: vec![(1_049_000, 1_060_000)],
            window: None,
        };
        let v = judge(&rep, &[0.4], Some(&cov));
        assert_eq!(v["pass"], true, "{v}");
        assert_eq!(v["stale_retries_outside"], 0);
        assert_eq!(v["ws_reconnects_outside"], 0);
        assert_eq!(v["stale_retries"], 4_000);
    }

    /// W6T-4: the same retries with no herald kill around them mean the
    /// herald dropped connections by itself: flagged, criterion 6 fails.
    #[test]
    fn stale_retries_outside_flagged() {
        let mut rep = clean();
        rep["staleRetries"] = json!(4_000);
        rep["ws"]["reconnects"] = json!(1_000);
        let s = samples(
            100,
            1_000,
            |i| if i >= 52 { 4_000 } else { 0 },
            |i| if i >= 51 { 1_000 } else { 0 },
        );
        let cov = Coverage {
            ws_viewers: 1_000,
            samples: s,
            outages: vec![(1_010_000, 1_020_000)],
            window: None,
        };
        let v = judge(&rep, &[0.4], Some(&cov));
        assert_eq!(v["pass"], false, "{v}");
        assert_eq!(v["stale_retries_outside"], 4_000);
        assert_eq!(v["ws_reconnects_outside"], 1_000);
        assert!(
            v["misses"]
                .to_string()
                .contains("outside the herald outage windows"),
            "{v}"
        );
    }

    /// W6T-4: outage windows from the chaos log (kill -9 of the herald to
    /// its restart, plus the recovery budget and 1.5 think times).
    #[test]
    fn outage_windows_from_the_chaos_log() {
        let ev = vec![
            json!({"event": "chaos-kill", "wall_ms": 1_000, "detail": {"component": "herald"}}),
            json!({"event": "chaos-kill", "wall_ms": 1_500, "detail": {"component": "keeper-a"}}),
            json!({"event": "restart", "wall_ms": 1_120, "detail": {"component": "herald"}}),
            json!({"event": "restart", "wall_ms": 1_600, "detail": {"component": "keeper-a"}}),
            json!({"event": "chaos-kill", "wall_ms": 50_000, "detail": {"component": "herald"}}),
        ];
        let w = outage_windows(&ev, 5_000, 5_000);
        assert_eq!(
            w,
            vec![(1_000, 1_120 + 5_000 + 7_500 + 1_000), (50_000, u64::MAX)]
        );
    }

    /// W6T-4: the viewers get the recovery budget, the think time, the live
    /// bell, the opened provinces and every ring.
    #[test]
    fn viewer_args_carry_the_plan() {
        let plan = ViewerPlan {
            retry_budget_ms: 5_000,
            think_ms: 5_000,
            follow_status: Some("http://127.0.0.1:41340".into()),
            provinces: vec![(-1, 0), (0, 0), (2, -1)],
            rings: vec![0, 1, 2, 3],
        };
        let a = viewer_args(41_340, 41_375, 5_000, 4_320.0, 6, &plan);
        let val = |k: &str| a.iter().position(|x| x == k).map(|i| a[i + 1].clone());
        assert_eq!(val("--viewers").as_deref(), Some("4000"));
        assert_eq!(val("--ws").as_deref(), Some("1000"));
        assert_eq!(val("--retry-budget-ms").as_deref(), Some("5000"));
        assert_eq!(val("--think-ms").as_deref(), Some("5000"));
        assert_eq!(
            val("--follow-status").as_deref(),
            Some("http://127.0.0.1:41340")
        );
        assert_eq!(val("--provinces").as_deref(), Some("-1,0;0,0;2,-1"));
        assert_eq!(val("--rings").as_deref(), Some("0,1,2,3"));
        assert_eq!(val("--bells").as_deref(), Some("6"));
        // No follow, no provinces: those flags are left to the defaults.
        let a = viewer_args(
            41_340,
            41_375,
            500,
            60.0,
            0,
            &ViewerPlan {
                retry_budget_ms: 2_000,
                think_ms: 3_000,
                ..ViewerPlan::default()
            },
        );
        assert!(!a
            .iter()
            .any(|x| x == "--follow-status" || x == "--provinces" || x == "--rings"));
        assert!(a.iter().any(|x| x == "--retry-budget-ms"));
        // The w6-s7 budget: 60 game s of restart at 20x + 2 s.
        let mut c = crate::config::StackConfig {
            scale: 20.0,
            ..Default::default()
        };
        assert_eq!(c.viewer_retry_budget_ms(), 5_000);
        c.scale = 100.0;
        assert_eq!(c.viewer_retry_budget_ms(), 2_600);
    }
}
