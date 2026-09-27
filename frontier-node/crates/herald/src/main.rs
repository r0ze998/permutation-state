//! `frontier-herald` (M1 contract §8.4): ingest → fold → files, and the
//! origin serving `/h/*`, `WS /h/ws`, `/frontier/*` and `/gw/*`.
//!
//! ```text
//! frontier-herald --data DIR --program B58 --season ID --rpc http://127.0.0.1:41010
//!     [--listen 127.0.0.1:41040] [--source localnet|rpc] [--cluster localnet]
//!     [--web DIR] [--relay 127.0.0.1:41033] [--test-key | --drand-info FILE]
//!     [--checkpoint-slots 150] [--poll-ms 200] [--quotas JSON]
//! ```
//! The listen port must be 41000–41999 (or 0) and never a reserved port
//! (§10.3). Stop with Ctrl-C: the fold is checkpointed on the way out.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use fclient::rpc::RpcPort;
use findex::{Enriched, LocalnetFeed, RpcPoll};
use herald_fold::runner::{self, Ingest, IngestCfg};
use herald_fold::server::{self, App};
use herald_fold::views::SeasonStatic;
use solana_address::Address;
use tokio::sync::{broadcast, watch};

fn args() -> Result<HashMap<String, String>, String> {
    let mut m = HashMap::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let k = a
            .strip_prefix("--")
            .ok_or(format!("unexpected argument {a}"))?
            .to_string();
        if k == "test-key" {
            m.insert(k, "1".into());
            continue;
        }
        let v = it.next().ok_or(format!("--{k} needs a value"))?;
        m.insert(k, v);
    }
    Ok(m)
}

async fn main_inner() -> Result<(), String> {
    let a = args()?;
    let need = |k: &str| a.get(k).cloned().ok_or(format!("--{k} is required"));
    let data = PathBuf::from(need("data")?);
    let program: Address = need("program")?
        .parse()
        .map_err(|_| "--program: not base58")?;
    let season: u64 = need("season")?
        .parse()
        .map_err(|_| "--season: not a number")?;
    let rpc = need("rpc")?;
    let listen = a.get("listen").cloned().unwrap_or("127.0.0.1:41040".into());
    let port: u16 = listen
        .rsplit(':')
        .next()
        .and_then(|p| p.parse().ok())
        .ok_or("--listen: host:port")?;
    if !herald_fold::port_allowed(port) {
        return Err(format!(
            "--listen port {port}: M1 services use 41000-41999 and never a reserved port"
        ));
    }
    let drand = if a.contains_key("test-key") {
        fclient::beacon::TestKey::new().info()
    } else if let Some(p) = a.get("drand-info") {
        let v: serde_json::Value =
            serde_json::from_slice(&std::fs::read(p).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        fclient::beacon::parse_info_json(&v).ok_or("--drand-info: not a drand info JSON")?
    } else {
        fclient::beacon::quicknet_info()
    };
    let quotas = match a.get("quotas") {
        Some(q) => serde_json::from_str(q).map_err(|e| format!("--quotas: {e}"))?,
        None => serde_json::json!({"perDay": 40, "burst": 60}),
    };
    let mut cfg = IngestCfg::new(&data, program, season);
    cfg.exact_post = a.get("source").map(String::as_str).unwrap_or("localnet") == "localnet";
    if let Some(n) = a.get("checkpoint-slots") {
        cfg.checkpoint_slots = n.parse().map_err(|_| "--checkpoint-slots")?;
    }
    let poll = Duration::from_millis(
        a.get("poll-ms")
            .map(|v| v.parse().unwrap_or(200))
            .unwrap_or(200),
    );
    let (diffs, _) = broadcast::channel(4_096);
    let ing = Ingest::open(cfg.clone(), diffs.clone())?;
    let mut app = App::new(
        ing.fold.clone(),
        herald_fold::files::Out::new(cfg.files_dir()),
        SeasonStatic {
            cluster: a.get("cluster").cloned().unwrap_or("localnet".into()),
            drand,
            quotas,
        },
        diffs,
    );
    app.web = a.get("web").map(PathBuf::from);
    app.relay = a.get("relay").cloned();
    app.index = Some(cfg.index_path());
    let listener = tokio::net::TcpListener::bind(&listen)
        .await
        .map_err(|e| format!("{listen}: {e}"))?;
    eprintln!(
        "frontier-herald: {} season {season} on http://{}",
        program,
        listener.local_addr().map_err(|e| e.to_string())?
    );
    let srv = tokio::spawn(server::serve(listener, Arc::new(app)));
    let (stop_tx, stop) = watch::channel(false);
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = stop_tx.send(true);
    });
    let res = match a.get("source").map(String::as_str).unwrap_or("localnet") {
        "localnet" => {
            let feed = LocalnetFeed::new(RpcPort::localnet(rpc.clone(), program));
            let clock = RpcPort::localnet(rpc.clone(), program);
            runner::run(ing, feed, Some(clock), poll, stop).await
        }
        "rpc" => {
            let src = Enriched::new(RpcPoll::new(rpc.clone(), program), rpc.clone(), program);
            runner::run(ing, src, Some(RpcPort::new(rpc, program)), poll, stop).await
        }
        s => Err(format!("--source {s}: localnet or rpc")),
    };
    srv.abort();
    res
}

#[tokio::main]
async fn main() {
    if let Err(e) = main_inner().await {
        eprintln!("frontier-herald: {e}");
        std::process::exit(2);
    }
}
