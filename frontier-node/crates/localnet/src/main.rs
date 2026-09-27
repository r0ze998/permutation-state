//! `frontier-localnet`: serves the local chain on 127.0.0.1.
//!
//! ```text
//! frontier-localnet [--port 41010] [--ws-port 41011] [--scale 20] [--g0 <unix>]
//!                   [--data-dir DIR] [--snapshot-every-bells 36] [--keep-snapshots 3]
//!                   [--no-fsync] [--allow-tamper] [--start-paused]
//!                   [--program <id>=<path.so>[:<max_len>][@<authority>]]...
//! ```
//! Ports: 41000–41999 only, never a reserved one (§10.3). With `--data-dir`
//! every block goes to the ledger WAL first and a snapshot is taken every
//! 36 game bells; a restart with the same directory resumes the chain
//! (snapshot + ledger re-execution) at its last slot and game time, and the
//! header's `g0` and scale win over the flags. `--program` deploys are
//! skipped for programs the recovered chain already has. `--start-paused`
//! produces no block until `frontier_resume` (the stack deploys and checks
//! a recovered state before time moves).

use std::sync::{Arc, Mutex};

use localnet::{server, Chain, Config};

fn usage() -> ! {
    eprintln!(
        "usage: frontier-localnet [--port 41010] [--ws-port P] [--scale S] [--g0 UNIX] \
         [--data-dir DIR] [--snapshot-every-bells N] [--keep-snapshots N] [--no-fsync] \
         [--allow-tamper] [--start-paused] [--program ID=PATH[:MAXLEN][@AUTHORITY]]..."
    );
    std::process::exit(2)
}

fn num<T: std::str::FromStr>(v: Option<String>) -> T {
    v.and_then(|v| v.parse().ok()).unwrap_or_else(|| usage())
}

#[tokio::main]
async fn main() {
    let mut cfg = Config::default();
    let mut port: u16 = 41_010;
    let mut ws_port: Option<u16> = None;
    let mut programs: Vec<String> = vec![];
    let mut start_paused = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--port" => port = num(args.next()),
            "--ws-port" => ws_port = Some(num(args.next())),
            "--scale" => cfg.scale = num(args.next()),
            "--g0" => cfg.g0 = num(args.next()),
            "--data-dir" => cfg.data_dir = Some(args.next().unwrap_or_else(|| usage()).into()),
            "--snapshot-every-bells" => {
                let b: i64 = num(args.next());
                cfg.snapshot_every_secs = b.max(1) * 600;
            }
            "--keep-snapshots" => cfg.keep_snapshots = num(args.next()),
            "--no-fsync" => cfg.fsync = false,
            "--allow-tamper" => cfg.allow_tamper = true,
            "--start-paused" => start_paused = true,
            "--program" => programs.push(args.next().unwrap_or_else(|| usage())),
            _ => usage(),
        }
    }
    if port == 0 {
        eprintln!("--port 0 is for tests; pick a port in 41000-41999");
        std::process::exit(2);
    }
    if !(cfg.scale > 0.0 && cfg.scale <= 100_000.0) {
        eprintln!("--scale in (0, 100000]");
        std::process::exit(2);
    }
    let mut chain = Chain::open(cfg.clone()).unwrap_or_else(|e| {
        eprintln!("frontier-localnet: cannot open the chain: {e}");
        std::process::exit(1)
    });
    if cfg.data_dir.is_some() && chain.transaction_count() > 0 {
        eprintln!(
            "recovered: slot {}, game time {}, {} history entries, state {}",
            chain.slot(),
            chain.unix_timestamp(),
            chain.transaction_count(),
            hex_of(&chain.state_hash())
        );
    }
    for spec in programs {
        let (id, rest) = spec.split_once('=').unwrap_or_else(|| usage());
        let (rest, auth) = match rest.split_once('@') {
            Some((r, a)) => (r, Some(a.parse().unwrap_or_else(|_| usage()))),
            None => (rest, None),
        };
        let (path, max_len) = match rest.rsplit_once(':') {
            Some((p, m)) if m.chars().all(|c| c.is_ascii_digit()) => (p, m.parse::<usize>().ok()),
            _ => (rest, None),
        };
        let pid: fclient::Address = id.parse().unwrap_or_else(|_| usage());
        if chain.account(&pid).is_some() {
            eprintln!("{id}: already deployed on the recovered chain; skipped");
            continue;
        }
        let so = std::fs::read(path).unwrap_or_else(|e| {
            eprintln!("{path}: {e}");
            std::process::exit(1)
        });
        let max_len = max_len.unwrap_or(fclient::fees::deploy_max_len(so.len() as u64) as usize);
        chain.deploy(pid, &so, max_len, auth).unwrap_or_else(|e| {
            eprintln!("deploy {id}: {e}");
            std::process::exit(1)
        });
        eprintln!("deployed {id}: {} B, max_len {max_len}", so.len());
    }
    chain.set_paused(start_paused);
    let (scale, g0) = (chain.scale(), chain.config().g0);
    let state = Arc::new(Mutex::new(chain));
    let running = server::start_with(state, port, ws_port.unwrap_or(port + 1))
        .await
        .unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1)
        });
    eprintln!(
        "frontier-localnet on {} (ws {}; scale {scale}, g0 {g0}, 400-ms slots)",
        running.url(),
        running.ws_url(),
    );
    let _ = tokio::signal::ctrl_c().await;
    running.stop();
}

fn hex_of(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
