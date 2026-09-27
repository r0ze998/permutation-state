//! The viewer load generator against a herald on 127.0.0.1:0 (a short
//! run; the one-game-day run with 4,000 polling and 1,000 WS viewers is the
//! `frontier-viewers` binary's job in the stack runs): no errors, latency
//! recorded, WS viewers receive the diffs of records folded during the run.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{tmp, VecSource};
use herald_fold::fixture;
use herald_fold::runner::{Ingest, IngestCfg};
use herald_fold::server::{self, App};
use herald_fold::viewers::{self, Stats, ViewerCfg};
use herald_fold::views::SeasonStatic;
use serde_json::json;
use tokio::sync::broadcast;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn viewers_load_a_herald_without_errors() {
    load(100, 20, 3, 20).await;
}

/// The measurement run (ignored; `HERALD_VIEWERS`, `HERALD_WS`,
/// `HERALD_SECONDS`, `HERALD_THINK_MS` set its size): the numbers in
/// `W3-D-NOTES.md` come from it.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore]
async fn viewers_measure() {
    let env = |k: &str, d: u64| {
        std::env::var(k)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(d)
    };
    load(
        env("HERALD_VIEWERS", 4_000) as usize,
        env("HERALD_WS", 1_000) as usize,
        env("HERALD_SECONDS", 30),
        env("HERALD_THINK_MS", 5_000),
    )
    .await;
}

async fn load(pollers: usize, wsv: usize, seconds: u64, think_ms: u64) {
    let dir = tmp("viewers");
    let bells = 10;
    let txs = fixture::mini_season(bells);
    let (diffs, _) = broadcast::channel(8_192);
    let cfg = IngestCfg::new(&dir, fixture::program(), fixture::SEASON_ID);
    let mut ing = Ingest::open(cfg.clone(), diffs.clone()).unwrap();
    let half = txs.len() / 2;
    let mut src = VecSource::new(txs[..half].to_vec(), 10_000);
    while ing.step(&mut src).await.unwrap() > 0 {}
    let mut app = App::new(
        ing.fold.clone(),
        herald_fold::files::Out::new(cfg.files_dir()),
        SeasonStatic {
            cluster: "localnet".into(),
            drand: fclient::beacon::TestKey::new().info(),
            quotas: json!({}),
        },
        diffs,
    );
    app.index = Some(cfg.index_path());
    let l = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(server::serve(l, Arc::new(app)));
    let stats = Arc::new(Stats::default());
    let vc = ViewerCfg {
        herald: addr.to_string(),
        viewers: pollers,
        ws: wsv,
        duration: Duration::from_secs(seconds),
        think: Duration::from_millis(think_ms),
        rings: vec![2, 3],
        provinces: fixture::PROVINCES.iter().map(|p| (p.0, p.1)).collect(),
        bells,
        seed: 7,
    };
    let run = tokio::spawn(viewers::run(vc, stats.clone()));
    // Fold the second half while the viewers poll.
    tokio::time::sleep(Duration::from_millis(1_000)).await;
    let mut src = VecSource::new(txs[half..].to_vec(), 25);
    while ing.step(&mut src).await.unwrap() > 0 {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let rep = run.await.unwrap();
    println!("{}", serde_json::to_string_pretty(&rep).unwrap());
    assert_eq!(rep["errors"], 0, "{rep}");
    assert!(rep["requests"].as_u64().unwrap() > pollers as u64, "{rep}");
    assert!(rep["p99_ms"].as_f64().unwrap() > 0.0);
    assert_eq!(rep["ws"]["connected"], wsv as u64);
    if wsv > 0 {
        assert!(rep["ws"]["messages"].as_u64().unwrap() > 0, "{rep}");
    }
    assert_eq!(rep["ws"]["gaps"], 0);
    let _ = std::fs::remove_dir_all(&dir);
}
