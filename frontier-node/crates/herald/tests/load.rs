//! The herald's 5,000-viewer load test (M1 contract §13.4 criterion 6,
//! Gate W5's `load` line; W5-C): **4,000 polling viewers and 1,000 WS
//! viewers** against a herald on `127.0.0.1:0` while it ingests **one game
//! hour of the program's own season at 20×** (450 slots of 400 ms, paced by
//! the recorded slots: the chain's real rate, not a burst), then the
//! criterion: file p99 ≤ 250 ms, ingest → WS p99 ≤ 2 s (every WS message
//! carries the herald's ingest stamp `t`), error rate < 0.1%.
//!
//! The season is `fixtures/verify/march-program.json.gz` (read only: the
//! strict `inproc_day` recorded from the program, 100 bots over a game
//! day). The first `HERALD_LOAD_FROM_BELL` bells (default 72) are folded
//! before the viewers start (the herald of a running season); the paced
//! hour follows. Environment: `HERALD_VIEWERS` (4,000), `HERALD_WS`
//! (1,000), `HERALD_THINK_MS` (5,000), `HERALD_LOAD_GAME_HOURS` (1),
//! `HERALD_LOAD_SCALE` (20), `HERALD_LOAD_REPORT=<file>`.

mod common;

use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::tmp;
use fclient::ports::{PortResult, TxRecord};
use herald_fold::runner::{self, Ingest, IngestCfg};
use herald_fold::server::{self, App};
use herald_fold::viewers::{self, Stats, ViewerCfg};
use herald_fold::views::SeasonStatic;
use serde_json::{json, Value};
use solana_address::Address;
use tokio::sync::{broadcast, watch};

fn env_num(k: &str, d: u64) -> u64 {
    std::env::var(k)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(d)
}

/// The recorded season: (program, season id, transactions in feed order).
fn recording() -> (Address, u64, Vec<TxRecord>) {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/verify/march-program.json.gz");
    let raw = std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    let mut json = vec![];
    flate2::read::GzDecoder::new(&raw[..])
        .read_to_end(&mut json)
        .unwrap();
    let v: Value = serde_json::from_slice(&json).unwrap();
    let program: Address = v["config"]["program"].as_str().unwrap().parse().unwrap();
    let season = v["config"]["season_id"]
        .as_u64()
        .or_else(|| {
            v["config"]["season_id"]
                .as_str()
                .and_then(|s| s.parse().ok())
        })
        .unwrap();
    let txs = v["txs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| findex::archive::record_from_json(t).unwrap())
        .collect();
    (program, season, txs)
}

/// A source that releases the recorded transactions of each slot when the
/// paced clock reaches it (`slot_ms` of wall time per slot from `t0`).
struct Paced {
    txs: Vec<TxRecord>,
    pos: usize,
    start_slot: u64,
    t0: Arc<Mutex<Option<Instant>>>,
    slot_ms: u64,
}

impl findex::Source for Paced {
    async fn pull(&mut self) -> PortResult<Vec<TxRecord>> {
        let Some(t0) = *self.t0.lock().unwrap() else {
            return Ok(vec![]);
        };
        let now_slot = self.start_slot + t0.elapsed().as_millis() as u64 / self.slot_ms;
        let mut end = self.pos;
        while end < self.txs.len() && self.txs[end].slot <= now_slot {
            end += 1;
        }
        let out = self.txs[self.pos..end].to_vec();
        self.pos = end;
        Ok(out)
    }
    fn cursor(&self) -> Value {
        json!({"pos": self.pos})
    }
    fn restore(&mut self, _v: &Value) {}
}

/// A source over a fixed prefix (the season before the viewers arrive).
struct Prefix(Vec<TxRecord>, usize);

impl findex::Source for Prefix {
    async fn pull(&mut self) -> PortResult<Vec<TxRecord>> {
        let end = (self.1 + 500).min(self.0.len());
        let out = self.0[self.1..end].to_vec();
        self.1 = end;
        Ok(out)
    }
    fn cursor(&self) -> Value {
        json!({"prefix": self.1})
    }
    fn restore(&mut self, _v: &Value) {}
}

#[test]
fn criterion_6_targets_read_the_report() {
    let ok = json!({"requests": 10, "p99_ms": 12.0, "errorRate": 0.0, "wsViewers": 1,
        "ws": {"timed": 5, "ingest_p99_ms": 300.0}});
    assert!(viewers::targets(&ok).is_empty());
    let slow = json!({"requests": 10, "p99_ms": 251.0, "errorRate": 0.001, "wsViewers": 1,
        "ws": {"timed": 0, "ingest_p99_ms": 2_001.0}});
    let m = viewers::targets(&slow);
    assert_eq!(m.len(), 4, "{m:?}");
    // No WS viewers: the WS target does not apply; no request fails.
    let none = json!({"requests": 0, "p99_ms": 0.0, "errorRate": 0.0, "wsViewers": 0});
    assert_eq!(
        viewers::targets(&none),
        vec!["no request was answered".to_string()]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "one game hour at 20× (3 minutes of wall time) with 5,000 viewers; Gate W5 (W5-C) runs it with --ignored herald_5000"]
async fn herald_5000_viewers_meet_criterion_6() {
    let pollers = env_num("HERALD_VIEWERS", 4_000) as usize;
    let wsv = env_num("HERALD_WS", 1_000) as usize;
    let think = env_num("HERALD_THINK_MS", 5_000);
    let hours = env_num("HERALD_LOAD_GAME_HOURS", 1);
    let scale = env_num("HERALD_LOAD_SCALE", 20);
    let from_bell = env_num("HERALD_LOAD_FROM_BELL", 72) as i64;
    let (program, season_id, txs) = recording();
    assert!(!txs.is_empty());
    // Split at the first slot of `from_bell` (block times are game time).
    let genesis = txs
        .iter()
        .flat_map(|t| t.post.iter())
        .filter_map(|(_, a)| {
            let a = a.as_ref()?;
            (a.data.len() > 8 && a.data[..8] == *b"PSF1SEAS")
                .then(|| fclient::decode::Season::decode(&a.data).ok())
                .flatten()
                .map(|s| s.genesis_ts)
        })
        // The created Season (the announced one has no genesis yet).
        .max()
        .expect("the Season account in the recording");
    let split_ts = genesis + from_bell * 600;
    let cut = txs
        .iter()
        .position(|t| t.block_time >= split_ts)
        .unwrap_or(txs.len());
    let (pre, rest) = txs.split_at(cut);
    let start_slot = rest.first().map(|t| t.slot).unwrap_or(0);
    let slot_ms = 400u64;
    // One game hour = 3,600 / (0.4 × scale) slots.
    let slots = hours * 3_600 * 10 / (4 * scale);
    let rest: Vec<TxRecord> = rest
        .iter()
        .filter(|t| t.slot < start_slot + slots)
        .cloned()
        .collect();

    let dir = tmp("load");
    let (diffs, _) = broadcast::channel(65_536);
    let cfg = IngestCfg::new(&dir, program, season_id);
    let mut ing = Ingest::open(cfg.clone(), diffs.clone()).unwrap();
    let mut src = Prefix(pre.to_vec(), 0);
    while ing.step(&mut src).await.unwrap() > 0 {}
    let (provinces, rings, bell) = {
        let f = ing.fold.read().unwrap();
        let pv: Vec<(i16, i16)> = f.provinces.keys().copied().collect();
        let rg: Vec<u16> = f.rings.keys().copied().collect();
        (pv, rg, from_bell as u32)
    };
    assert!(
        !provinces.is_empty() && !rings.is_empty(),
        "the prefix folds the season's provinces ({} txs)",
        pre.len()
    );
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

    // The paced hour, through the runner loop the binary runs (200-ms poll).
    let t0 = Arc::new(Mutex::new(None));
    let paced = Paced {
        txs: rest.clone(),
        pos: 0,
        start_slot,
        t0: t0.clone(),
        slot_ms,
    };
    let (stop_tx, stop_rx) = watch::channel(false);
    let stats = Arc::new(Stats::default());
    let duration = Duration::from_millis(slots * slot_ms + 5_000);
    let vc = ViewerCfg {
        herald: addr.to_string(),
        viewers: pollers,
        ws: wsv,
        duration,
        think: Duration::from_millis(think),
        rings,
        provinces,
        bells: bell,
        seed: 11,
    };
    let run = tokio::spawn(viewers::run(vc, stats.clone()));
    // The viewers ramp over one think time; the chain starts with them.
    *t0.lock().unwrap() = Some(Instant::now());
    tokio::spawn(async move {
        tokio::time::sleep(duration).await;
        let _ = stop_tx.send(true);
    });
    // The runner is not `Send` (the index's SQLite handle): it runs on this
    // task, as in the binary.
    runner::run(
        ing,
        paced,
        None::<localnet::InProcess>,
        Duration::from_millis(200),
        stop_rx,
    )
    .await
    .unwrap();
    let mut rep = run.await.unwrap();
    rep["ingested"] = json!({"prefixTxs": pre.len(), "pacedTxs": rest.len(), "slots": slots,
        "gameHours": hours, "scale": scale, "fromBell": from_bell});
    println!("{}", serde_json::to_string_pretty(&rep).unwrap());
    if let Ok(p) = std::env::var("HERALD_LOAD_REPORT") {
        std::fs::write(p, serde_json::to_string_pretty(&rep).unwrap()).unwrap();
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(rep["ws"]["connected"], wsv as u64, "{rep}");
    assert_eq!(rep["ws"]["gaps"], 0, "{rep}");
    assert_eq!(
        rep["targetsMet"], true,
        "criterion 6: {}",
        rep["targetMisses"]
    );
}
