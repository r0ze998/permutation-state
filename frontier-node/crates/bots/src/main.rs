//! `frontier-bots`: N bots in one process against a running season.
//!
//! ```text
//! frontier-bots --herald http://127.0.0.1:41040 --relay http://127.0.0.1:41033
//!               [--rpc http://127.0.0.1:41010]   (personas' own transactions, frontier-localnet)
//!               [--seed 1] [--bots 1000] [--first-index 0] [--days 7]
//!               [--personas default|off|<n per persona>]
//!               [--journal DIR] [--report FILE] [--invites FILE]
//!               [--game-hours H] [--scale S] [--control 127.0.0.1:41070]
//!               [--day0-share F] [--eager-personas]
//! ```
//!
//! `--day0-share F` (0–1) is the share of the roster that joins on day 0
//! (default: the mix's 0.6; `itest::inproc_day` uses 1.0); `--eager-personas`
//! gives the persona bots a session every bell (as `inproc_day` does), so a
//! short stack run observes them. W6-C (W5-B F2: in the stack 25 of 100 bots
//! joined in a one-day run and no persona marched).
//!
//! Binds no port (a client of the herald, the relay and the local chain),
//! except `--control` (§10.3 bots control / metrics: `GET /metrics`,
//! `GET /health`, `POST /stop`; `frontier_bots::control`).
//! Loopback URLs only (`http://`); the stack gives ports in 41000–41999.
//! Exit codes: 0 done, 2 bad arguments, 1 runtime failure.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use fclient::clock::GameClock;
use frontier_agents::profile::{roster, Mix};
use frontier_bots::bot::{ClockSource, Config, Shared};
use frontier_bots::fleet::Fleet;
use frontier_bots::journal::Journal;
use frontier_bots::ports::{HttpHerald, HttpRelay, NoDirect, RpcDirect};

struct Args {
    herald: String,
    relay: String,
    rpc: Option<String>,
    seed: u64,
    bots: usize,
    first_index: u32,
    days: u32,
    personas: Option<u32>,
    journal: Option<PathBuf>,
    report: Option<PathBuf>,
    invites: Option<PathBuf>,
    game_hours: Option<f64>,
    scale: f64,
    control: Option<String>,
    day0_share: Option<f64>,
    eager_personas: bool,
}

fn usage(e: &str) -> ! {
    eprintln!("frontier-bots: {e}\nusage: frontier-bots --herald URL --relay URL [--rpc URL] [--seed N] [--bots N] [--first-index N] [--days N] [--personas default|off|N] [--journal DIR] [--report FILE] [--invites FILE] [--game-hours H] [--scale S] [--control 127.0.0.1:PORT] [--day0-share F] [--eager-personas]");
    std::process::exit(2);
}

fn parse() -> Args {
    parse_from(std::env::args().skip(1))
}

fn parse_from(args: impl Iterator<Item = String>) -> Args {
    let mut a = Args {
        herald: String::new(),
        relay: String::new(),
        rpc: None,
        seed: 1,
        bots: 1_000,
        first_index: 0,
        days: 7,
        personas: None,
        journal: None,
        report: None,
        invites: None,
        game_hours: None,
        scale: 20.0,
        control: None,
        day0_share: None,
        eager_personas: false,
    };
    let mut it = args;
    while let Some(k) = it.next() {
        let mut v = || {
            it.next()
                .unwrap_or_else(|| usage(&format!("{k} needs a value")))
        };
        let num = |s: String| -> u64 {
            s.parse()
                .unwrap_or_else(|_| usage(&format!("{k}: not a number")))
        };
        match k.as_str() {
            "--herald" => a.herald = v(),
            "--relay" => a.relay = v(),
            "--rpc" => a.rpc = Some(v()),
            "--seed" => a.seed = num(v()),
            "--bots" => a.bots = num(v()) as usize,
            "--first-index" => a.first_index = num(v()) as u32,
            "--days" => a.days = num(v()) as u32,
            "--personas" => {
                a.personas = match v().as_str() {
                    "default" => None,
                    "off" => Some(0),
                    n => Some(
                        n.parse()
                            .unwrap_or_else(|_| usage("--personas: default, off or a number")),
                    ),
                }
            }
            "--journal" => a.journal = Some(v().into()),
            "--report" => a.report = Some(v().into()),
            "--invites" => a.invites = Some(v().into()),
            "--game-hours" => {
                a.game_hours = Some(v().parse().unwrap_or_else(|_| usage("--game-hours")))
            }
            "--scale" => a.scale = v().parse().unwrap_or_else(|_| usage("--scale")),
            "--control" => {
                let c = v();
                if !frontier_bots::control::allowed(&c) {
                    usage("--control: 127.0.0.1 and a port in 41000-41999 (not reserved)");
                }
                a.control = Some(c);
            }
            "--day0-share" => {
                let f: f64 = v()
                    .parse()
                    .unwrap_or_else(|_| usage("--day0-share: a number in [0, 1]"));
                if !(0.0..=1.0).contains(&f) {
                    usage("--day0-share: a number in [0, 1]");
                }
                a.day0_share = Some(f);
            }
            "--eager-personas" => a.eager_personas = true,
            "-h" | "--help" => usage("help"),
            _ => usage(&format!("unknown argument {k}")),
        }
    }
    for (n, u) in [("--herald", &a.herald), ("--relay", &a.relay)] {
        if u.is_empty() {
            usage(&format!("{n} is required"));
        }
    }
    for u in [Some(&a.herald), Some(&a.relay), a.rpc.as_ref()]
        .into_iter()
        .flatten()
    {
        if !is_loopback_url(u) {
            usage(&format!("{u}: loopback http:// URLs only"));
        }
    }
    a
}

/// `http://127.0.0.1:<port>` or `http://localhost:<port>` with an optional
/// path: the authority parsed, so userinfo (`http://127.0.0.1:1@other/`),
/// a missing or non-numeric port and other hosts are refused (wave-3
/// review, W3-E).
fn is_loopback_url(u: &str) -> bool {
    let Some(rest) = u.strip_prefix("http://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains(['@', '\\', '%']) || authority.chars().any(char::is_whitespace) {
        return false;
    }
    let Some((host, port)) = authority.rsplit_once(':') else {
        return false;
    };
    matches!(host, "127.0.0.1" | "localhost")
        && !port.is_empty()
        && port.bytes().all(|b| b.is_ascii_digit())
        && port.parse::<u16>().is_ok()
}

fn write_report(path: &Option<PathBuf>, v: &serde_json::Value) {
    if let Some(p) = path {
        let tmp = p.with_extension("tmp");
        let body = serde_json::to_vec_pretty(v).expect("json");
        if std::fs::write(&tmp, body)
            .and_then(|_| std::fs::rename(&tmp, p))
            .is_err()
        {
            eprintln!("frontier-bots: cannot write {}", p.display());
        }
    }
}

async fn go<D: frontier_bots::ports::DirectPort + 'static>(a: Args, direct: Option<D>) -> i32 {
    let mut cfg = Config::new(a.seed);
    if let Some(p) = &a.invites {
        match std::fs::read_to_string(p) {
            Ok(s) => {
                cfg.invites = s
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect()
            }
            Err(e) => {
                eprintln!("frontier-bots: {}: {e}", p.display());
                return 1;
            }
        }
    }
    cfg.slot_game_secs = 0.4 * a.scale;
    cfg.eager_personas = a.eager_personas;
    let clock = ClockSource::Game(Mutex::new(GameClock::new(a.scale)));
    let mut shared = Shared::new(
        HttpHerald::new(&a.herald),
        HttpRelay::new(&a.relay),
        direct,
        cfg,
        clock,
    );
    let journal_path = a
        .journal
        .as_ref()
        .map(|d| d.join(format!("marchbook-{}-{}.jsonl", a.seed, a.first_index)));
    if let Some(p) = &journal_path {
        match Journal::open(p) {
            Ok(j) => shared = shared.with_journal(j),
            Err(e) => {
                eprintln!("frontier-bots: {}: {e}", p.display());
                return 1;
            }
        }
    }
    let mut mix = Mix {
        persona_count: a.personas,
        ..Mix::for_season_days(a.days)
    };
    if let Some(f) = a.day0_share {
        mix.day0_share = f;
    }
    let mut r = roster(a.bots, a.seed, &mix);
    for s in &mut r {
        s.index += a.first_index;
    }
    let season = loop {
        match shared.season().await {
            Ok(s) => break s,
            Err(e) => {
                eprintln!("frontier-bots: waiting for the season ({e})");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    };
    // Late joins spread before this season's join_close_bell (wave-3
    // review, W3-E): the roster is dealt again for the season read.
    let close = season.join_close_bell();
    if r.iter().any(|s| s.join_bell >= close) {
        r = roster(a.bots, a.seed, &mix.clone().with_join_close(close));
        for s in &mut r {
            s.index += a.first_index;
        }
    }
    let end =
        season.genesis_ts + season.end_bell().min(a.days * 144) as i64 * season.bell_secs as i64;
    let until = match a.game_hours {
        Some(h) => season.latest_unix + (h * 3_600.0) as i64,
        None => end,
    };
    let mut fleet = Fleet::new(shared, &r);
    // W6-C: with `--rpc`, the game clock follows the chain's Clock sysvar
    // (read every 200 ms, as drand-replay does), not only the herald's
    // `latestSlot/latestUnix` sampled when a bot runs. The herald's samples
    // lag and arrive in bursts; a rate estimated from them ran the fleet's
    // clock ahead (every bot stopped at bell 102 of 144) or slow enough
    // that no bot came due again (actions stopped at bell 20-50) in the
    // W6-C stack runs. The herald's samples still count when they are
    // newer (never, in practice: the clock is monotone in slot).
    if let Some(url) = a.rpc.clone() {
        let sh = fleet.shared.clone();
        let port = fclient::rpc::RpcPort::localnet(url, fclient::addr::system_program());
        tokio::spawn(async move {
            use fclient::ports::ChainPort;
            loop {
                if let Ok(c) = port.clock().await {
                    sh.clock.observe(c.slot, c.unix_timestamp);
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        });
    }
    if let Some(p) = &journal_path {
        match fleet.restore(p) {
            Ok(n) if n > 0 => eprintln!("frontier-bots: restored {n} marches from {}", p.display()),
            Ok(_) => {}
            Err(e) => eprintln!("frontier-bots: journal {}: {e}", p.display()),
        }
    }
    eprintln!(
        "frontier-bots: {} bots (seed {}, first index {}), {} personas each, day-0 share {}, eager personas {}, {} join on day 0, until game time {until}",
        r.len(),
        a.seed,
        a.first_index,
        mix.personas_for(a.bots),
        mix.day0_share,
        a.eager_personas,
        r.iter().filter(|s| s.join_day == 0).count()
    );
    let sh = fleet.shared.clone();
    let rp = a.report.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let writer = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let v = sh.report.lock().expect("report").to_json();
            write_report(&rp, &v);
        }
    });
    if let Some(c) = &a.control {
        let sh = fleet.shared.clone();
        let rep: Arc<dyn Fn() -> serde_json::Value + Send + Sync> = Arc::new(move || {
            sh.report
                .lock()
                .map(|r| r.to_json())
                .unwrap_or(serde_json::Value::Null)
        });
        match frontier_bots::control::serve(c, rep, stop.clone()).await {
            Ok(at) => eprintln!("frontier-bots: control on {at}"),
            Err(e) => {
                eprintln!("frontier-bots: --control {c}: {e}");
                return 1;
            }
        }
    }
    let s2 = stop.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            s2.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    });
    let report = fleet.run(until, stop).await;
    writer.abort();
    let v = report.to_json();
    write_report(&a.report, &v);
    if a.report.is_none() {
        println!("{}", serde_json::to_string_pretty(&v).expect("json"));
    }
    0
}

#[tokio::main]
async fn main() {
    let a = parse();
    let code = match a.rpc.clone() {
        Some(url) => {
            // The program id comes from the herald's season file.
            let program =
                match fclient::http::get(&format!("{}/h/season", a.herald.trim_end_matches('/')))
                    .await
                {
                    Ok(r) if r.status == 200 => {
                        serde_json::from_slice::<serde_json::Value>(&r.body)
                            .ok()
                            .and_then(|v| {
                                v.get("programId")
                                    .and_then(|p| p.as_str())
                                    .and_then(|p| p.parse().ok())
                            })
                    }
                    _ => None,
                };
            match program {
                Some(p) => go(a, Some(RpcDirect::new(url, p))).await,
                None => {
                    eprintln!(
                        "frontier-bots: --rpc needs the herald's /h/season (programId) first"
                    );
                    1
                }
            }
        }
        None => go::<NoDirect>(a, None).await,
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::{is_loopback_url, parse_from};

    /// W6-C (W5-B R3): the stack passes `--day0-share` and
    /// `--eager-personas` through `bots_args`.
    #[test]
    fn day0_share_and_eager_personas_parse() {
        let args = |v: &[&str]| {
            v.iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .into_iter()
        };
        let base = [
            "--herald",
            "http://127.0.0.1:41040",
            "--relay",
            "http://127.0.0.1:41033",
        ];
        let a = parse_from(args(&base));
        assert_eq!((a.day0_share, a.eager_personas), (None, false));
        let mut v = base.to_vec();
        v.extend(["--day0-share", "1.0", "--eager-personas"]);
        let a = parse_from(args(&v));
        assert_eq!((a.day0_share, a.eager_personas), (Some(1.0), true));
        // The roster then joins everyone on day 0.
        let mix = frontier_agents::profile::Mix {
            day0_share: a.day0_share.unwrap(),
            ..frontier_agents::profile::Mix::for_season_days(1)
        };
        let r = frontier_agents::profile::roster(100, 1, &mix);
        assert!(r.iter().all(|s| s.join_day == 0));
    }

    #[test]
    fn loopback_urls_only() {
        for ok in [
            "http://127.0.0.1:41040",
            "http://localhost:41033/",
            "http://127.0.0.1:1/x",
        ] {
            assert!(is_loopback_url(ok), "{ok}");
        }
        for bad in [
            "http://127.0.0.1:1@other.host/",
            "http://127.0.0.1:x",
            "http://127.0.0.1",
            "https://127.0.0.1:1",
            "http://127.0.0.2:1",
            "http://localhost.evil:1",
            "http://127.0.0.1:1%40x",
            "http://127.0.0.1:99999",
        ] {
            assert!(!is_loopback_url(bad), "{bad}");
        }
    }
}
