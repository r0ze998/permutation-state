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
        retry_budget: Duration::from_secs(5),
        follow: None,
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
        // Every diff carries the herald's ingest stamp (W5-C).
        assert_eq!(rep["ws"]["timed"], rep["ws"]["messages"], "{rep}");
    }
    assert_eq!(rep["ws"]["gaps"], 0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A herald in its own runtime, so a "kill -9" drops the listener and
/// every open connection at once (`shutdown_background`), as a chaos kill
/// of the process does; restarted on the same address.
struct Killable {
    rt: Option<tokio::runtime::Runtime>,
}

impl Killable {
    fn start(addr: std::net::SocketAddr, app: impl Fn() -> App + Send + 'static) -> Killable {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        rt.spawn(async move {
            // The old listener may take a moment to close.
            let l = loop {
                match tokio::net::TcpListener::bind(addr).await {
                    Ok(l) => break l,
                    Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
                }
            };
            let _ = server::serve(l, Arc::new(app())).await;
        });
        Killable { rt: Some(rt) }
    }
    fn kill(&mut self) {
        if let Some(rt) = self.rt.take() {
            rt.shutdown_background();
        }
    }
}

/// W6T-3 (w6-s7 criterion 6: 9,000 errors = 4,000 pollers × 2 herald
/// kills + 1,000 WS viewers, with 0 herald serving errors). Failing first
/// on 7dcacdf: every poller lost one request per restart (its reused
/// keep-alive connection was dead) and every WS viewer was gone after the
/// first. With the client's standard recovery (one retry of the idempotent
/// GET on a stale connection, a budgeted connect, WS reconnect) a herald
/// that is back within the budget costs no error.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn viewers_ride_out_two_herald_restarts() {
    let (pollers, wsv, kills) = (40usize, 10usize, 2u64);
    let dir = tmp("viewers-restart");
    let bells = 10;
    let txs = fixture::mini_season(bells);
    let (diffs, _) = broadcast::channel(8_192);
    let cfg = IngestCfg::new(&dir, fixture::program(), fixture::SEASON_ID);
    let mut ing = Ingest::open(cfg.clone(), diffs.clone()).unwrap();
    let mut src = VecSource::new(txs, 10_000);
    while ing.step(&mut src).await.unwrap() > 0 {}
    let fold = ing.fold.clone();
    let app = move || {
        let mut app = App::new(
            fold.clone(),
            herald_fold::files::Out::new(cfg.files_dir()),
            SeasonStatic {
                cluster: "localnet".into(),
                drand: fclient::beacon::TestKey::new().info(),
                quotas: json!({}),
            },
            diffs.clone(),
        );
        app.index = Some(cfg.index_path());
        app
    };
    let addr = {
        let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        l.local_addr().unwrap()
    };
    let mut h = Killable::start(addr, app.clone());
    tokio::time::sleep(Duration::from_millis(300)).await;
    let stats = Arc::new(Stats::default());
    let vc = restart_cfg(addr.to_string(), pollers, wsv, bells);
    let run = tokio::spawn(viewers::run(vc, stats.clone()));
    for k in 0..kills {
        // Every poller has answered at least once (think 1 s, start spread
        // over one think) before the first kill.
        tokio::time::sleep(Duration::from_millis(if k == 0 { 2_500 } else { 2_000 })).await;
        h.kill();
        // Down for 0.2 s: less than the shortest pause (0.5 s).
        tokio::time::sleep(Duration::from_millis(200)).await;
        h = Killable::start(addr, app.clone());
    }
    let rep = run.await.unwrap();
    h.kill();
    println!("{}", serde_json::to_string_pretty(&rep).unwrap());
    assert!(rep["requests"].as_u64().unwrap() > 3 * pollers as u64, "{rep}");
    assert_eq!(rep["errors"], 0, "{rep}");
    assert_eq!(rep["ws"]["errors"], 0, "{rep}");
    assert_eq!(rep["ws"]["connected"], wsv as u64, "{rep}");
    assert_eq!(rep["ws"]["reconnects"], kills * wsv as u64, "{rep}");
    assert_eq!(rep["errorRate"], 0.0, "{rep}");
    let _ = std::fs::remove_dir_all(&dir);
}

fn restart_cfg(herald: String, pollers: usize, wsv: usize, bells: u32) -> ViewerCfg {
    ViewerCfg {
        herald,
        viewers: pollers,
        ws: wsv,
        duration: Duration::from_millis(7_500),
        think: Duration::from_millis(1_000),
        rings: vec![2, 3],
        provinces: fixture::PROVINCES.iter().map(|p| (p.0, p.1)).collect(),
        bells,
        seed: 9,
        retry_budget: Duration::from_secs(5),
        follow: None,
    }
}

/// `--follow-status` (W6T-3): the per-bell requests follow the live bell
/// the herald's `/h/status` shows (w6-s7's went to bells 0–5 for 24 game
/// hours).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn viewers_follow_the_live_bell() {
    let dir = tmp("viewers-follow");
    let bells = 10;
    let txs = fixture::mini_season(bells);
    let (diffs, _) = broadcast::channel(8_192);
    let cfg = IngestCfg::new(&dir, fixture::program(), fixture::SEASON_ID);
    let mut ing = Ingest::open(cfg.clone(), diffs.clone()).unwrap();
    let mut src = VecSource::new(txs, 10_000);
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
    let l = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(server::serve(l, Arc::new(app)));
    // The chain's Clock at bell 7 (the runner records it from the RPC).
    let mut c = viewers::KeepAlive::connect(&addr.to_string()).await.unwrap();
    let season: serde_json::Value =
        serde_json::from_slice(&c.get_body("/h/season").await.unwrap().1).unwrap();
    let g = season["genesisTs"].as_i64().unwrap();
    let secs = season["bellSecs"].as_i64().unwrap();
    let live = 7;
    ing.observe_clock(4_242, g + live * secs + 5);
    let status: serde_json::Value =
        serde_json::from_slice(&c.get_body("/h/status").await.unwrap().1).unwrap();
    assert_eq!(status["live"][1], g + live * secs + 5, "{status}");
    let stats = Arc::new(Stats::default());
    let mut vc = restart_cfg(addr.to_string(), 20, 0, 1);
    vc.duration = Duration::from_secs(3);
    vc.think = Duration::from_millis(200);
    vc.follow = Some(addr.to_string());
    let rep = viewers::run(vc, stats).await;
    assert_eq!(rep["follow"]["liveBell"], live, "{rep}");
    assert_eq!(rep["follow"]["errors"], 0, "{rep}");
    assert_eq!(rep["errors"], 0, "{rep}");
    // Not following: no live bell.
    let stats = Arc::new(Stats::default());
    let mut vc = restart_cfg(addr.to_string(), 2, 0, bells);
    vc.duration = Duration::from_millis(500);
    let rep = viewers::run(vc, stats).await;
    assert!(rep["follow"]["liveBell"].is_null(), "{rep}");
    let _ = std::fs::remove_dir_all(&dir);
}
