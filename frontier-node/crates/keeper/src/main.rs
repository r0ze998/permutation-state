//! `frontier-keeper --config keeper.toml [--dev] [--peace-start f] [--init-seed]`
//!
//! Loads `keeper.toml` (M1 contract §8.2), takes the process lock of its
//! journal, reconciles in-flight attempts, then ticks once per slot against
//! the first RPC URL (the local chain node or a validator) and the drand
//! URLs (`drand-replay` on loopback; HTTPS endpoints wait for a TLS client,
//! W1-F request R4). The loopback API serves when `api` is set, with the
//! `/v1/reveal` accept path reading the same RPC (W3-C).
//! `--init-seed` writes a fresh 32-byte master seed (mode 0600) and exits.
//! With `FRONTIER_KEEPER_TICK_LOG=1` every tick prints one `TICK` line to
//! stderr (slot, the chain's slot at the first send, versions sent, wall
//! ms per phase; W6T-2), and an idle tick that sent (the in-slot drand
//! retry) an `IDLE` line.

use std::path::PathBuf;
use std::time::Duration;

use fclient::beacon::{parse_info_json, HttpDrand};
use fclient::rpc::RpcPort;
use keeper_core::config::KeeperConfig;
use keeper_core::journal::{self, Journal};
use keeper_core::{api, Keeper};

fn usage() -> ! {
    eprintln!(
        "usage: frontier-keeper --config <keeper.toml> [--dev] [--peace-start <fraction>] [--init-seed]"
    );
    std::process::exit(2);
}

fn read_seed(p: &PathBuf) -> Result<[u8; 32], String> {
    let raw = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
    let text = String::from_utf8_lossy(&raw);
    let hexed = hex::decode(text.trim()).ok();
    let b = hexed.as_deref().unwrap_or(&raw);
    b.try_into()
        .map_err(|_| format!("{}: a master seed is 32 bytes (raw or hex)", p.display()))
}

fn write_seed(p: &PathBuf) -> Result<(), String> {
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    if p.exists() {
        return Err(format!("{} exists; not overwritten", p.display()));
    }
    let mut seed = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut seed);
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    o.mode(0o600);
    let mut f = o.open(p).map_err(|e| e.to_string())?;
    f.write_all(hex::encode(seed).as_bytes())
        .map_err(|e| e.to_string())
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut cfg_path = None;
    let (mut dev, mut init) = (false, false);
    let mut peace: Option<f64> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                cfg_path = args.get(i + 1).cloned();
                i += 1;
            }
            "--dev" => dev = true,
            "--peace-start" => {
                let f: f64 = args
                    .get(i + 1)
                    .and_then(|x| x.parse().ok())
                    .filter(|f| (0.0..=1.0).contains(f))
                    .unwrap_or_else(|| usage());
                peace = Some(f);
                i += 1;
            }
            "--init-seed" => init = true,
            _ => usage(),
        }
        i += 1;
    }
    let Some(cfg_path) = cfg_path else { usage() };
    let text = std::fs::read_to_string(&cfg_path).unwrap_or_else(|e| {
        eprintln!("{cfg_path}: {e}");
        std::process::exit(2)
    });
    let mut cfg = KeeperConfig::from_toml(&text).unwrap_or_else(|e| {
        eprintln!("{cfg_path}: {e}");
        std::process::exit(2)
    });
    cfg.dev |= dev;
    if peace.is_some() {
        cfg.peace_start = peace;
    }
    let seed_path = cfg
        .master_seed_file
        .clone()
        .unwrap_or_else(|| PathBuf::from("keeper.seed"));
    if init {
        match write_seed(&seed_path) {
            Ok(()) => {
                eprintln!("wrote {}", seed_path.display());
                return;
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1)
            }
        }
    }
    let master = read_seed(&seed_path).unwrap_or_else(|e| {
        eprintln!("{e} (run with --init-seed once)");
        std::process::exit(2)
    });
    let jpath = cfg
        .journal
        .clone()
        .unwrap_or_else(|| PathBuf::from("keeper.journal.sqlite"));
    let _lock = journal::lock(&jpath).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let journal = Journal::open(&jpath).unwrap_or_else(|e| {
        eprintln!("{}: {e}", jpath.display());
        std::process::exit(1)
    });
    let Some(rpc) = cfg.rpc.first().cloned() else {
        eprintln!("keeper.toml: `rpc` is empty");
        std::process::exit(2)
    };
    if cfg.drand.is_empty() {
        eprintln!("keeper.toml: `drand` is empty");
        std::process::exit(2)
    }
    // Chain info from the first drand URL that answers (drand-replay serves
    // the test key's info in --test-key mode, I-53).
    let mut info = None;
    for _attempt in 0..40 {
        for u in &cfg.drand {
            let url = format!("{}/info", u.trim_end_matches('/'));
            if let Ok(r) = fclient::http::get(&url).await {
                if let Some(i) = serde_json::from_slice(&r.body)
                    .ok()
                    .and_then(|v| parse_info_json(&v))
                {
                    info = Some(i);
                    break;
                }
            }
        }
        if info.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let Some(info) = info else {
        eprintln!("no drand URL answered /info");
        std::process::exit(1)
    };
    let port = RpcPort::localnet(rpc.clone(), cfg.program);
    // The `/v1/reveal` accept path reads the chain through its own client.
    let gate: std::sync::Arc<dyn keeper_core::reveal_accept::RevealGate> =
        std::sync::Arc::new(keeper_core::reveal_accept::ChainGate {
            port: RpcPort::localnet(rpc, cfg.program),
            addrs: fclient::addr::Addresses::new(cfg.program, cfg.season_id),
        });
    let drand = HttpDrand::new(cfg.drand.clone(), info);
    let api_addr = cfg.api;
    let token = match &cfg.token_file {
        Some(p) => std::fs::read_to_string(p)
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|e| {
                eprintln!("{}: {e}", p.display());
                std::process::exit(2)
            }),
        None => String::new(),
    };
    let claim_seed = cfg.beneficiary_key_file.clone().map(|p| {
        read_seed(&p).unwrap_or_else(|e| {
            eprintln!("beneficiary_key_file: {e}");
            std::process::exit(2)
        })
    });
    let mut k = Keeper::new(cfg, port, drand, &master, Some(journal)).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    });
    if let Some(seed) = claim_seed {
        let kp = fclient::Keypair::new_from_array(seed);
        if let Err(e) = k.set_claim_key(kp) {
            eprintln!("{e}");
            std::process::exit(2)
        }
    }
    let running = match api_addr {
        Some(a) => Some(
            api::serve_with(a, k.shared.clone(), token, Some(gate))
                .await
                .unwrap_or_else(|e| {
                    eprintln!("api: {e}");
                    std::process::exit(1)
                }),
        ),
        None => None,
    };
    match k.start().await {
        Ok(n) => eprintln!("frontier-keeper: started; {n} in-flight attempts adopted"),
        Err(e) => {
            eprintln!("start: {e}");
            std::process::exit(1)
        }
    }
    let tick_log = std::env::var("FRONTIER_KEEPER_TICK_LOG").is_ok_and(|v| v == "1");
    let mut iv = tokio::time::interval(Duration::from_millis(100));
    iv.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = iv.tick() => {
                let t0 = std::time::Instant::now();
                match k.tick().await {
                    Ok(r) if tick_log && !r.idle => {
                        eprintln!("{}", r.line(t0.elapsed().as_millis() as u64));
                    }
                    Ok(r) if tick_log && r.sent > 0 => {
                        eprintln!("IDLE{}", &r.line(t0.elapsed().as_millis() as u64)[4..]);
                    }
                    Ok(_) => {}
                    Err(e) => eprintln!("tick: {e}"),
                }
            }
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    if let Some(r) = running {
        r.stop();
    }
}
