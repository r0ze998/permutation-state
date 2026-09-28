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

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::run::RunDir;

pub const P99_FILE_MS: f64 = 250.0;
pub const INGEST_P99_S: f64 = 2.0;
pub const ERROR_RATE: f64 = 0.001;

/// Spawns `frontier-viewers` for `wall_secs`; stdout (its JSON report) to
/// `load/<tag>.json`.
pub fn spawn_viewers(
    st: &crate::up::Stack,
    viewers: usize,
    wall_secs: f64,
    bell: u32,
    tag: &str,
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
    )
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
) -> Result<Child, String> {
    let dir = run.path("load");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let out = std::fs::File::create(dir.join(format!("{tag}.json"))).map_err(|e| e.to_string())?;
    let err = std::fs::File::create(dir.join(format!("{tag}.log"))).map_err(|e| e.to_string())?;
    let ws = viewers / 5;
    let polling = viewers - ws;
    #[cfg(unix)]
    use std::os::unix::process::CommandExt;
    let mut c = Command::new(bin);
    c.args([
        "--herald",
        &format!("127.0.0.1:{herald}"),
        "--viewers",
        &polling.to_string(),
        "--ws",
        &ws.to_string(),
        "--seconds",
        &(wall_secs.ceil().max(1.0) as u64).to_string(),
        "--bells",
        &bell.max(1).to_string(),
        "--stats",
        &format!("127.0.0.1:{stats}"),
    ])
    .stdin(Stdio::null())
    .stdout(out)
    .stderr(err);
    #[cfg(unix)]
    c.process_group(0);
    c.spawn().map_err(|e| format!("{}: {e}", bin.display()))
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

/// The verdict on one load report plus the lag samples (seconds).
pub fn judge(rep: &Value, lag_secs: &[f64]) -> Value {
    let req = rep["requests"].as_f64().unwrap_or(0.0);
    let errs = rep["errors"].as_f64().unwrap_or(0.0) + rep["ws"]["errors"].as_f64().unwrap_or(0.0);
    let rate = if req > 0.0 { errs / req } else { 1.0 };
    let p99 = rep["p99_ms"].as_f64().unwrap_or(f64::INFINITY);
    let ingest = quantile(lag_secs, 0.99);
    let ok_file = p99 <= P99_FILE_MS;
    let ok_err = req > 0.0 && rate < ERROR_RATE;
    let ok_ingest = ingest.is_some_and(|x| x <= INGEST_P99_S);
    json!({
        "p99_file_ms": p99, "p99_file_target_ms": P99_FILE_MS, "p99_file_ok": ok_file,
        "requests": req, "errors": errs, "error_rate": rate, "error_rate_target": ERROR_RATE, "error_rate_ok": ok_err,
        "ingest_lag_p99_s": ingest, "ingest_lag_samples": lag_secs.len(), "ingest_target_s": INGEST_P99_S, "ingest_ok": ok_ingest,
        "ingest_note": "herald fold lag (newest program tx slot - last folded slot) x 0.4 s, sampled each second; WS fan-out after the fold not included",
        "pass": ok_file && ok_err && ok_ingest,
    })
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
    let bell = ((now - genesis).max(0) / 600) as u32;
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
    let mut child = match spawn_with(&bin, rd, herald, stats, viewers, wall, bell, &tag) {
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
    let mut lags = vec![];
    let url = format!("http://127.0.0.1:{herald}/h/status");
    let code = loop {
        if let Ok(Some(s)) = child.try_wait() {
            break s.code();
        }
        if let (Ok(Some(newest)), Ok(Ok(r))) = (
            chain.latest_program_slot(&program).await,
            tokio::time::timeout(Duration::from_secs(3), fclient::http::get(&url)).await,
        ) {
            if let Ok(v) = serde_json::from_slice::<Value>(&r.body) {
                if let Some(ls) = v["lastSlot"].as_u64() {
                    lags.push(newest.saturating_sub(ls) as f64 * 0.4);
                }
            }
        }
        if t0.elapsed() > Duration::from_secs_f64(wall + 300.0) {
            let _ = child.kill();
            break None;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    };
    let rep: Value = std::fs::read_to_string(rd.path(&format!("load/{tag}.json")))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    if resumed {
        let _ = chain.pause().await;
    }
    let verdict = judge(&rep, &lags);
    let out = json!({"tag": tag, "chain_resumed_for_load": resumed, "viewers": viewers, "game_hours": game_hours, "wall_secs": wall, "scale": scale,
        "exit": code, "generator": rep, "verdict": verdict});
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
        assert_eq!(judge(&good, &[0.4, 0.8])["pass"], true);
        let slow = json!({"requests": 100_000, "errors": 10, "p99_ms": 300.0, "ws": {"errors": 0}});
        assert_eq!(judge(&slow, &[0.4])["pass"], false);
        let errs = json!({"requests": 1_000, "errors": 5, "p99_ms": 10.0, "ws": {"errors": 0}});
        assert_eq!(judge(&errs, &[0.4])["pass"], false);
        let lag = json!({"requests": 100_000, "errors": 0, "p99_ms": 10.0, "ws": {"errors": 0}});
        assert_eq!(judge(&lag, &[0.4, 3.2])["pass"], false);
        assert_eq!(
            judge(&lag, &[])["pass"],
            false,
            "no lag samples is not a pass"
        );
        assert_eq!(judge(&Value::Null, &[0.4])["pass"], false);
    }
}
