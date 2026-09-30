//! integ-W6t review (criterion 6, R5's ingest → WS p99 2.49 s): who owns
//! the tail of a burst, the herald or the viewer generator?
//!
//! A herald in this process replays a recorded run at its real pace (400 ms
//! per slot) from `HERALD_BURST_FROM_SLOT` (the season before it is folded
//! first, as a running herald has it), while `frontier-viewers` runs as a
//! **separate process** (`HERALD_BURST_VIEWERS_BIN`), so the generator's
//! own work is not on the herald's runtime. R5's first burst: the `ticket`
//! hold ended at slot 2074 and 72 transactions landed in slots 2075–2076
//! (≈ 1,100 diffs to each of 1,000 all-ring sockets at once).
//!
//! Every WS message carries the ingest stamp `t` and the herald's send
//! stamp `s`; the generator reports ingest → WS and its two shares (herald
//! `s − t`, delivery receipt − `s`). An older generator reports only
//! ingest → WS (it ignores `s`), which is how the two builds compare.
//!
//! ```text
//! HERALD_BURST_INPUT=…/integ-w6t-tri-20x/verify/input.json.gz \
//! HERALD_BURST_VIEWERS_BIN=…/frontier-viewers HERALD_BURST_PORT=41140 \
//! cargo test --release -p herald --test burst -- --ignored --nocapture
//! ```
//! Environment: `HERALD_BURST_FROM_SLOT` (2040), `HERALD_BURST_SLOTS`
//! (260), `HERALD_BURST_WARMUP_S` (30), `HERALD_BURST_POLLERS` (4,000),
//! `HERALD_BURST_WS` (1,000), `HERALD_BURST_REPORT=<file>`.

mod common;

use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::tmp;
use fclient::ports::{PortResult, TxRecord};
use herald_fold::runner::{self, Ingest, IngestCfg};
use herald_fold::server::{self, App};
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

fn recording(path: &str) -> (Address, u64, Vec<TxRecord>) {
    let raw = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
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
    let mut txs: Vec<TxRecord> = v["txs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| findex::archive::record_from_json(t).unwrap())
        .collect();
    txs.sort_by_key(|t| t.seq);
    (program, season, txs)
}

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

/// The recorded slots at their real pace once `t0` is set.
struct Paced {
    txs: Vec<TxRecord>,
    pos: usize,
    start_slot: u64,
    t0: Arc<Mutex<Option<Instant>>>,
}

impl findex::Source for Paced {
    async fn pull(&mut self) -> PortResult<Vec<TxRecord>> {
        let Some(t0) = *self.t0.lock().unwrap() else {
            return Ok(vec![]);
        };
        let now_slot = self.start_slot + t0.elapsed().as_millis() as u64 / 400;
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

#[tokio::test(flavor = "multi_thread")]
#[ignore = "a recorded burst against a separate frontier-viewers process (integ-W6t review); needs HERALD_BURST_INPUT and HERALD_BURST_VIEWERS_BIN"]
async fn burst_tail_owner() {
    let input = std::env::var("HERALD_BURST_INPUT").expect("HERALD_BURST_INPUT");
    let bin = std::env::var("HERALD_BURST_VIEWERS_BIN").expect("HERALD_BURST_VIEWERS_BIN");
    let port = env_num("HERALD_BURST_PORT", 41_140) as u16;
    assert!(herald_fold::port_allowed(port), "port {port}");
    let from = env_num("HERALD_BURST_FROM_SLOT", 2_040);
    let slots = env_num("HERALD_BURST_SLOTS", 260);
    let warmup = env_num("HERALD_BURST_WARMUP_S", 30);
    let pollers = env_num("HERALD_BURST_POLLERS", 4_000);
    let wsv = env_num("HERALD_BURST_WS", 1_000);
    let (program, season_id, txs) = recording(&input);
    let (pre, rest): (Vec<TxRecord>, Vec<TxRecord>) = txs.into_iter().partition(|t| t.slot <= from);
    let rest: Vec<TxRecord> = rest
        .into_iter()
        .filter(|t| t.slot <= from + slots)
        .collect();
    let dir = tmp("burst");
    // The binary's channel capacity (main.rs).
    let (diffs, _) = broadcast::channel(4_096);
    let cfg = IngestCfg::new(&dir, program, season_id);
    let mut ing = Ingest::open(cfg.clone(), diffs.clone()).unwrap();
    let mut src = Prefix(pre.clone(), 0);
    while ing.step(&mut src).await.unwrap() > 0 {}
    let (provinces, rings) = {
        let f = ing.fold.read().unwrap();
        let pv: Vec<(i16, i16)> = f.provinces.keys().copied().collect();
        let rg: Vec<u16> = f.rings.keys().copied().collect();
        (pv, rg)
    };
    let mut app = App::new(
        ing.fold.clone(),
        herald_fold::files::Out::new(cfg.files_dir()),
        SeasonStatic {
            cluster: "localnet".into(),
            drand: fclient::beacon::quicknet_info(),
            quotas: json!({}),
        },
        diffs,
    );
    app.index = Some(cfg.index_path());
    let l = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .unwrap();
    tokio::spawn(server::serve(l, Arc::new(app)));
    let seconds = warmup + slots * 400 / 1_000 + 10;
    let pv: Vec<String> = provinces.iter().map(|(p, q)| format!("{p},{q}")).collect();
    let rg: Vec<String> = rings.iter().map(|r| r.to_string()).collect();
    let child = std::process::Command::new(&bin)
        .args([
            "--herald",
            &format!("127.0.0.1:{port}"),
            "--viewers",
            &pollers.to_string(),
            "--ws",
            &wsv.to_string(),
            "--seconds",
            &seconds.to_string(),
            "--think-ms",
            "5000",
            "--rings",
            &rg.join(","),
            "--provinces",
            &pv.join(";"),
            "--bells",
            "20",
            "--retry-budget-ms",
            "5000",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("frontier-viewers");
    // The viewers connect and subscribe; then the recorded slots at pace.
    tokio::time::sleep(Duration::from_secs(warmup)).await;
    let t0 = Arc::new(Mutex::new(Some(Instant::now())));
    let paced = Paced {
        txs: rest.clone(),
        pos: 0,
        start_slot: from + 1,
        t0,
    };
    let (stop_tx, stop_rx) = watch::channel(false);
    let run_for = Duration::from_millis(slots * 400 + 2_000);
    tokio::spawn(async move {
        tokio::time::sleep(run_for).await;
        let _ = stop_tx.send(true);
    });
    runner::run(
        ing,
        paced,
        None::<localnet::InProcess>,
        Duration::from_millis(200),
        stop_rx,
    )
    .await
    .unwrap();
    let out = tokio::task::spawn_blocking(move || child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    let mut rep: Value = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
    let burst: usize = rest
        .iter()
        .filter(|t| (2_075..=2_076).contains(&t.slot))
        .count();
    rep["burst"] = json!({"input": input, "prefixTxs": pre.len(), "pacedTxs": rest.len(),
        "fromSlot": from, "slots": slots, "txsIn2075to2076": burst, "generator": bin});
    println!("{}", serde_json::to_string_pretty(&rep).unwrap());
    if let Ok(p) = std::env::var("HERALD_BURST_REPORT") {
        std::fs::write(p, serde_json::to_string_pretty(&rep).unwrap()).unwrap();
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(rep["ws"]["messages"].as_u64().unwrap_or(0) > 0, "{rep}");
}
