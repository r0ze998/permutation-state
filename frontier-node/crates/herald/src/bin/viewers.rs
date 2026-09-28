//! `frontier-viewers`: the herald's viewer load generator (offchain design
//! §8.3, §12; stats port 41075 per contract §10.3).
//!
//! ```text
//! frontier-viewers --herald 127.0.0.1:41040 [--viewers 4000] [--ws 1000]
//!     [--seconds 600] [--think-ms 5000] [--rings 0,1,2] [--provinces 2,0;1,1]
//!     [--bells 144] [--seed 1] [--stats 127.0.0.1:41075] [--gate]
//! ```
//! Prints the report as JSON (with `errorRate`, the WS `ingest_p99_ms`
//! and `targetsMet` / `targetMisses`: §13.4 criterion 6); with `--stats`
//! it also serves the running counters at `GET /stats`. With `--gate` the
//! exit code is 1 when a criterion-6 target is missed (the stack's `load`
//! step, Gate W5).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use herald_fold::viewers::{self, Stats, ViewerCfg};

fn args() -> Result<HashMap<String, String>, String> {
    let mut m = HashMap::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let k = a
            .strip_prefix("--")
            .ok_or(format!("unexpected argument {a}"))?;
        if k == "gate" {
            m.insert(k.to_string(), "1".into());
            continue;
        }
        m.insert(
            k.to_string(),
            it.next().ok_or(format!("--{k} needs a value"))?,
        );
    }
    Ok(m)
}

async fn main_inner() -> Result<(), String> {
    let a = args()?;
    let num = |k: &str, d: u64| -> Result<u64, String> {
        a.get(k).map_or(Ok(d), |v| {
            v.parse().map_err(|_| format!("--{k}: not a number"))
        })
    };
    let herald = a
        .get("herald")
        .cloned()
        .ok_or("--herald host:port is required")?;
    let rings = a
        .get("rings")
        .map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect())
        .unwrap_or_else(|| vec![0, 1, 2]);
    let provinces = a
        .get("provinces")
        .map(|s| {
            s.split(';')
                .filter_map(|pq| {
                    let (p, q) = pq.split_once(',')?;
                    Some((p.parse().ok()?, q.parse().ok()?))
                })
                .collect()
        })
        .unwrap_or_else(|| vec![(0, 0), (1, 0), (0, 1), (2, 0)]);
    let cfg = ViewerCfg {
        herald,
        viewers: num("viewers", 4_000)? as usize,
        ws: num("ws", 1_000)? as usize,
        duration: Duration::from_secs(num("seconds", 600)?),
        think: Duration::from_millis(num("think-ms", 5_000)?),
        rings,
        provinces,
        bells: num("bells", 144)? as u32,
        seed: num("seed", 1)?,
    };
    let stats = Arc::new(Stats::default());
    if let Some(addr) = a.get("stats") {
        let port: u16 = addr
            .rsplit(':')
            .next()
            .and_then(|p| p.parse().ok())
            .ok_or("--stats host:port")?;
        if !herald_fold::port_allowed(port) {
            return Err(format!(
                "--stats port {port}: M1 services use 41000-41999 and never a reserved port"
            ));
        }
        let l = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| format!("{addr}: {e}"))?;
        let st = stats.clone();
        let t0 = Instant::now();
        let app = axum::Router::new().route(
            "/stats",
            axum::routing::get(move || {
                let st = st.clone();
                async move { axum::Json(st.report(t0.elapsed())) }
            }),
        );
        tokio::spawn(async move {
            let _ = axum::serve(l, app).await;
        });
    }
    let rep = viewers::run(cfg, stats).await;
    println!("{}", serde_json::to_string_pretty(&rep).unwrap_or_default());
    if a.contains_key("gate") && rep["targetsMet"] != true {
        eprintln!(
            "frontier-viewers: criterion 6 missed: {}",
            rep["targetMisses"]
        );
        std::process::exit(1);
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(e) = main_inner().await {
        eprintln!("frontier-viewers: {e}");
        std::process::exit(2);
    }
}
