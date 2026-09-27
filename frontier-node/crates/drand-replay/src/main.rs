//! `drand-replay`
//!
//! ```text
//! drand-replay [serve] [--port 41020] (--archive DIR [--info FILE] | --test-key)
//!              [--clock chain:http://127.0.0.1:41010 | --clock fixed:UNIX | --clock wall:G0:SCALE]
//!              [--delay-ms 1000]
//! drand-replay plan     [--test-key] --g0 UNIX [--days 7] [--pre-days 1.5] [--margin-secs 3600]
//!                       [--rate 20] [--endpoints 3]
//! drand-replay prefetch --archive DIR [--test-key] (--g0 UNIX [--days 7] [--pre-days 1.5] | --from R --to R)
//!                       --endpoint URL [--endpoint URL]... [--rate 20] [--fetcher native|curl]
//!                       [--approved O-M1-12]
//! drand-replay verify   --archive DIR [--test-key | --info FILE] [--all] [--threads N]
//! drand-replay pack     --from-dir DIR --archive OUT [--test-key | --info FILE]
//! ```
//! Every archive is pinned to quicknet's compiled-in chain info, or to the
//! test key's with `--test-key`, or to `--info FILE`. `prefetch` refuses any
//! non-loopback endpoint until the owner approves the download (O-M1-12)
//! and `--approved O-M1-12` is passed.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use drand_replay::archive::Archive;
use drand_replay::prefetch::{self, Fetcher, Options};
use drand_replay::{spawn_chain_clock, ClockSource, Replay, Source};
use fclient::beacon;
use fclient::ports::ChainInfo;
use fclient::rpc::RpcPort;

fn usage() -> ! {
    eprintln!(
        "usage: drand-replay [serve] [--port 41020] (--archive DIR [--info FILE] | --test-key) \
         [--clock chain:URL | fixed:UNIX | wall:G0:SCALE] [--delay-ms 800..2000]\n\
         \x20      drand-replay plan [--test-key] --g0 UNIX [--days 7] [--pre-days 1.5] [--margin-secs 3600] [--rate 20] [--endpoints 3]\n\
         \x20      drand-replay prefetch --archive DIR [--test-key] (--g0 UNIX [--days D] [--pre-days P] | --from R --to R) \
         --endpoint URL... [--rate 20] [--fetcher native|curl] [--approved O-M1-12]\n\
         \x20      drand-replay verify --archive DIR [--test-key | --info FILE] [--all] [--threads N]\n\
         \x20      drand-replay pack --from-dir DIR --archive OUT [--test-key | --info FILE]"
    );
    std::process::exit(2)
}

fn die(m: String) -> ! {
    eprintln!("{m}");
    std::process::exit(1)
}

fn num<T: std::str::FromStr>(v: Option<String>) -> T {
    v.and_then(|v| v.parse().ok()).unwrap_or_else(|| usage())
}

fn info_from_file(f: &str) -> ChainInfo {
    let v: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(f).unwrap_or_else(|e| die(format!("{f}: {e}"))),
    )
    .unwrap_or_else(|e| die(format!("{f}: {e}")));
    beacon::parse_info_json(&v).unwrap_or_else(|| die(format!("{f}: not a drand info")))
}

/// The pinned chain: `--test-key`, `--info FILE`, or quicknet.
fn pinned(test_key: bool, info: &Option<String>) -> ChainInfo {
    match (test_key, info) {
        (true, None) => beacon::TestKey::new().info(),
        (false, Some(f)) => info_from_file(f),
        (false, None) => beacon::quicknet_info(),
        _ => usage(),
    }
}

#[tokio::main]
async fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = match args.first().map(|s| s.as_str()) {
        Some("serve" | "plan" | "prefetch" | "verify" | "pack") => args.remove(0),
        _ => "serve".into(),
    };
    match cmd.as_str() {
        "serve" => serve(args).await,
        "plan" => plan(args),
        "prefetch" => fetch(args).await,
        "verify" => verify(args),
        "pack" => pack(args),
        _ => usage(),
    }
}

async fn serve(args: Vec<String>) {
    let mut port: u16 = 41_020;
    let mut archive: Option<String> = None;
    let mut info_file: Option<String> = None;
    let mut test_key = false;
    let mut clock = "chain:http://127.0.0.1:41010".to_string();
    let mut delay_ms: i64 = 1_000;
    let mut args = args.into_iter();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--port" => port = num(args.next()),
            "--archive" => archive = Some(args.next().unwrap_or_else(|| usage())),
            "--info" => info_file = Some(args.next().unwrap_or_else(|| usage())),
            "--test-key" => test_key = true,
            "--clock" => clock = args.next().unwrap_or_else(|| usage()),
            "--delay-ms" => delay_ms = num(args.next()),
            _ => usage(),
        }
    }
    if !(800..=2_000).contains(&delay_ms) {
        die(format!(
            "--delay-ms {delay_ms}: quicknet publishes 0.8–2.0 s after the round time"
        ));
    }
    let source = match (archive, test_key) {
        (Some(dir), false) => Source::open(std::path::Path::new(&dir), pinned(false, &info_file))
            .unwrap_or_else(|e| die(e)),
        (None, true) => {
            eprintln!("drand-replay: TEST KEY — rounds are signed locally; only a `test-beacon` program build accepts them");
            Source::TestKey(beacon::TestKey::new())
        }
        _ => usage(),
    };
    if let Source::Packed(a) = &source {
        let (f, l) = a.range().unwrap_or((0, 0));
        eprintln!(
            "drand-replay: packed archive, {} rounds in {} segments ({f}..={l})",
            a.rounds(),
            a.segments.len()
        );
    }
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
    let chain = hex::encode(source.info().chain_hash);
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

struct PlanArgs {
    g0: Option<i64>,
    days: f64,
    pre_days: f64,
    margin: i64,
    from: Option<u64>,
    to: Option<u64>,
}

impl PlanArgs {
    fn new() -> PlanArgs {
        PlanArgs {
            g0: None,
            days: 7.0,
            pre_days: 1.5,
            margin: 3_600,
            from: None,
            to: None,
        }
    }
    fn take(&mut self, a: &str, next: &mut impl Iterator<Item = String>) -> bool {
        match a {
            "--g0" => self.g0 = Some(num(next.next())),
            "--days" => self.days = num(next.next()),
            "--pre-days" => self.pre_days = num(next.next()),
            "--margin-secs" => self.margin = num(next.next()),
            "--from" => self.from = Some(num(next.next())),
            "--to" => self.to = Some(num(next.next())),
            _ => return false,
        }
        true
    }
    fn plan(&self, info: &ChainInfo) -> prefetch::Plan {
        match (self.g0, self.from, self.to) {
            (Some(g0), None, None) => {
                prefetch::plan(info, g0, self.pre_days, self.days, self.margin)
            }
            (None, Some(first), Some(last)) if first >= 1 && last >= first => {
                prefetch::Plan { first, last }
            }
            _ => usage(),
        }
    }
}

fn plan(args: Vec<String>) {
    let mut pa = PlanArgs::new();
    let mut test_key = false;
    let mut rate = 20.0;
    let mut endpoints = 3usize;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if pa.take(&a, &mut it) {
            continue;
        }
        match a.as_str() {
            "--test-key" => test_key = true,
            "--rate" => rate = num(it.next()),
            "--endpoints" => endpoints = num(it.next()),
            _ => usage(),
        }
    }
    let info = pinned(test_key, &None);
    let p = pa.plan(&info);
    println!(
        "{}",
        serde_json::json!({"first": p.first, "last": p.last, "rounds": p.count(), "packed_bytes": p.bytes(),
            "first_time": info.round_time(p.first), "last_time": info.round_time(p.last),
            "fetch_hours": p.fetch_secs(rate, endpoints) / 3_600.0, "rate_per_endpoint": rate, "endpoints": endpoints})
    );
}

async fn fetch(args: Vec<String>) {
    let mut pa = PlanArgs::new();
    let mut dir: Option<PathBuf> = None;
    let mut test_key = false;
    let mut o = Options::default();
    let mut approved = false;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if pa.take(&a, &mut it) {
            continue;
        }
        match a.as_str() {
            "--archive" => dir = Some(it.next().unwrap_or_else(|| usage()).into()),
            "--test-key" => test_key = true,
            "--endpoint" => o.endpoints.push(it.next().unwrap_or_else(|| usage())),
            "--rate" => o.rate = num(it.next()),
            "--fetcher" => {
                o.fetcher = match it.next().as_deref() {
                    Some("native") => Fetcher::Native,
                    Some("curl") => Fetcher::Curl,
                    _ => usage(),
                }
            }
            "--approved" => {
                approved = it.next().as_deref() == Some("O-M1-12");
                if !approved {
                    usage()
                }
            }
            _ => usage(),
        }
    }
    o.allow_public = approved;
    let dir = dir.unwrap_or_else(|| usage());
    let info = pinned(test_key, &None);
    let p = pa.plan(&info);
    prefetch::check_endpoints(&o).unwrap_or_else(|e| die(e));
    eprintln!(
        "prefetch rounds {}..={} ({} rounds) from {} endpoint(s) at {} req/s each: ≈ {:.2} h",
        p.first,
        p.last,
        p.count(),
        o.endpoints.len(),
        o.rate,
        p.fetch_secs(o.rate, o.endpoints.len()) / 3_600.0
    );
    match prefetch::run(&dir, &info, p, &o).await {
        Ok(r) => println!(
            "{}",
            serde_json::json!({"first": r.plan.first, "last": r.plan.last, "resumed": r.resumed,
                "fetched": r.fetched, "rejected": r.rejected, "complete": r.complete, "archive": dir.display().to_string()})
        ),
        Err(e) => die(e),
    }
}

fn verify(args: Vec<String>) {
    let mut dir: Option<PathBuf> = None;
    let mut test_key = false;
    let mut info_file: Option<String> = None;
    let mut all = false;
    let mut threads = std::thread::available_parallelism().map_or(4, |x| x.get());
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--archive" => dir = Some(it.next().unwrap_or_else(|| usage()).into()),
            "--test-key" => test_key = true,
            "--info" => info_file = Some(it.next().unwrap_or_else(|| usage())),
            "--all" => all = true,
            "--threads" => threads = num(it.next()),
            _ => usage(),
        }
    }
    let dir = dir.unwrap_or_else(|| usage());
    let info = pinned(test_key, &info_file);
    let t0 = Instant::now();
    let a = Archive::load(&dir, &info).unwrap_or_else(|e| die(format!("FAIL: {e}")));
    let bad = if all { a.verify_all(threads) } else { vec![] };
    let (f, l) = a.range().unwrap_or((0, 0));
    println!(
        "{}",
        serde_json::json!({"verdict": if bad.is_empty() { "PASS" } else { "FAIL" }, "rounds": a.rounds(),
            "segments": a.segments.len(), "first": f, "last": l, "all": all, "bad": bad, "secs": t0.elapsed().as_secs_f64()})
    );
    if !bad.is_empty() {
        std::process::exit(1);
    }
}

fn pack(args: Vec<String>) {
    let mut from: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut test_key = false;
    let mut info_file: Option<String> = None;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--from-dir" => from = Some(it.next().unwrap_or_else(|| usage()).into()),
            "--archive" => out = Some(it.next().unwrap_or_else(|| usage()).into()),
            "--test-key" => test_key = true,
            "--info" => info_file = Some(it.next().unwrap_or_else(|| usage())),
            _ => usage(),
        }
    }
    let (from, out) = (
        from.unwrap_or_else(|| usage()),
        out.unwrap_or_else(|| usage()),
    );
    let info = pinned(test_key, &info_file);
    // Every round is verified against the pinned key on load.
    let f = beacon::FixtureDrand::load(&from, info.clone()).unwrap_or_else(|e| die(e));
    let rounds = f.rounds.iter().map(|(r, b)| (*r, b.sig48)).collect();
    let a = Archive::from_rounds(info, &rounds);
    a.write(&out).unwrap_or_else(|e| die(e));
    println!(
        "{}",
        serde_json::json!({"rounds": a.rounds(), "segments": a.segments.len(), "archive": out.display().to_string()})
    );
}
