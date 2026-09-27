//! `drand-replay`
//!
//! ```text
//! drand-replay [--port 41020] (--archive DIR [--info FILE] | --test-key)
//!              [--clock chain:http://127.0.0.1:41010 | --clock fixed:UNIX | --clock wall:G0:SCALE]
//!              [--delay-ms 1000]
//! ```
//! `--archive` defaults its chain info to quicknet's (or `--info` a drand
//! `/info` JSON). The default clock is the local chain node on 41010.

use std::sync::Arc;
use std::time::{Duration, Instant};

use drand_replay::{spawn_chain_clock, ClockSource, Replay, Source};
use fclient::beacon;
use fclient::rpc::RpcPort;

fn usage() -> ! {
    eprintln!(
        "usage: drand-replay [--port 41020] (--archive DIR [--info FILE] | --test-key) \
         [--clock chain:URL | fixed:UNIX | wall:G0:SCALE] [--delay-ms 800..2000]"
    );
    std::process::exit(2)
}

fn die(m: String) -> ! {
    eprintln!("{m}");
    std::process::exit(1)
}

#[tokio::main]
async fn main() {
    let mut port: u16 = 41_020;
    let mut archive: Option<String> = None;
    let mut info_file: Option<String> = None;
    let mut test_key = false;
    let mut clock = "chain:http://127.0.0.1:41010".to_string();
    let mut delay_ms: i64 = 1_000;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--port" => {
                port = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| usage())
            }
            "--archive" => archive = Some(args.next().unwrap_or_else(|| usage())),
            "--info" => info_file = Some(args.next().unwrap_or_else(|| usage())),
            "--test-key" => test_key = true,
            "--clock" => clock = args.next().unwrap_or_else(|| usage()),
            "--delay-ms" => {
                delay_ms = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| usage())
            }
            _ => usage(),
        }
    }
    if !(800..=2_000).contains(&delay_ms) {
        die(format!(
            "--delay-ms {delay_ms}: quicknet publishes 0.8–2.0 s after the round time"
        ));
    }
    let source = match (archive, test_key) {
        (Some(dir), false) => {
            let info = match info_file {
                Some(f) => {
                    let v: serde_json::Value = serde_json::from_str(
                        &std::fs::read_to_string(&f).unwrap_or_else(|e| die(format!("{f}: {e}"))),
                    )
                    .unwrap_or_else(|e| die(format!("{f}: {e}")));
                    beacon::parse_info_json(&v)
                        .unwrap_or_else(|| die(format!("{f}: not a drand info")))
                }
                None => beacon::quicknet_info(),
            };
            Source::archive(std::path::Path::new(&dir), info).unwrap_or_else(|e| die(e))
        }
        (None, true) => {
            eprintln!("drand-replay: TEST KEY — rounds are signed locally; only a `test-beacon` program build accepts them");
            Source::TestKey(beacon::TestKey::new())
        }
        _ => usage(),
    };
    let clock = if let Some(url) = clock.strip_prefix("chain:") {
        let (gc, _h) = spawn_chain_clock(
            RpcPort::localnet(url, fclient::addr::system_program()),
            Duration::from_millis(200),
        );
        ClockSource::Chain { clock: gc }
    } else if let Some(t) = clock.strip_prefix("fixed:") {
        ClockSource::Fixed(t.parse().unwrap_or_else(|_| usage()))
    } else if let Some(rest) = clock.strip_prefix("wall:") {
        let (g0, s) = rest.split_once(':').unwrap_or_else(|| usage());
        ClockSource::Wall {
            g0: g0.parse().unwrap_or_else(|_| usage()),
            scale: s.parse().unwrap_or_else(|_| usage()),
            start: Instant::now(),
        }
    } else {
        usage()
    };
    let chain = source
        .info()
        .chain_hash
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let running = drand_replay::start(
        Arc::new(Replay {
            source,
            clock,
            delay_ms,
        }),
        port,
    )
    .await
    .unwrap_or_else(|e| die(e));
    eprintln!(
        "drand-replay on {}/{chain} (delay {delay_ms} ms of game time)",
        running.url()
    );
    let _ = tokio::signal::ctrl_c().await;
    running.stop();
}
