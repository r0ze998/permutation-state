//! `frontier-stack` (M1 contract §8.7, §10.3, §12, §13.4).
//!
//! ```text
//! frontier-stack check-ports (--config FILE [--base-port P] | --ports P1,P2,...)
//! frontier-stack up [--config FILE] [--mode accel|realtime] [--beacon test-key|archive]
//!                   [--scale S] [--days D | --game-hours H] [--bots N] [--run-id ID]
//!                   [--base-port P] [--chaos] [--adversary] [--viewers N]
//!                   [--viewer-window-hours H] [--archive DIR] [--g0 UNIX] [--so PATH]
//!                   [--seed N] [--drain-scale S] [--expect-so-sha256 HEX] [--keep-running] ...
//! frontier-stack verify --run-id ID
//! frontier-stack tamper --run-id ID [--strict]
//! frontier-stack load   --run-id ID [--viewers 5000] [--game-hours 1]
//! frontier-stack report --run-id ID
//! frontier-stack down   --run-id ID
//! ```
//! Every subcommand but `check-ports` also takes `--runs-dir DIR` (default
//! `frontier-node/.local/frontier`). Exit codes: 0 pass, 1 fail, 2 bad
//! arguments or cannot run, 3 PENDING-OWNER (an unapproved install: Mode
//! R, Agave >= 4.0), 4 PENDING (the approved round archive is still being
//! fetched: `--beacon archive` before its `manifest.json`).

use frontier_stack::config::{self, StackConfig};
use frontier_stack::run::{self, RunDir};
use frontier_stack::{load, ports, report, up, verifyrun};

fn usage() -> ! {
    eprintln!(
        "{}",
        include_str!("main.rs")
            .lines()
            .skip(3)
            .take(13)
            .map(|l| l.trim_start_matches("//! "))
            .collect::<Vec<_>>()
            .join("\n")
    );
    std::process::exit(2)
}

fn die(m: impl std::fmt::Display) -> ! {
    eprintln!("frontier-stack: {m}");
    std::process::exit(2)
}

fn get<'a>(f: &'a [(String, Option<String>)], k: &str) -> Option<&'a str> {
    f.iter()
        .rev()
        .find(|(x, _)| x == k)
        .and_then(|(_, v)| v.as_deref())
}

fn has(f: &[(String, Option<String>)], k: &str) -> bool {
    f.iter().any(|(x, _)| x == k)
}

/// The run directory of `--run-id` (and `--runs-dir`).
fn run_dir(f: &[(String, Option<String>)]) -> RunDir {
    let id = get(f, "run-id").unwrap_or_else(|| die("--run-id is required"));
    let repo = run::repo_root().unwrap_or_else(|e| die(e));
    let runs = get(f, "runs-dir")
        .map(|d| run::resolve(&repo, std::path::Path::new(d)))
        .unwrap_or_else(|| RunDir::default_runs(&repo));
    RunDir::new(&runs, id)
}

fn load_config(f: &[(String, Option<String>)]) -> StackConfig {
    match get(f, "config") {
        Some(p) => {
            let repo = run::repo_root().ok();
            let path = std::path::PathBuf::from(p);
            let path = if path.exists() {
                path
            } else {
                repo.map(|r| r.join(p)).unwrap_or(path)
            };
            StackConfig::from_file(&path).unwrap_or_else(|e| die(e))
        }
        None => StackConfig::default(),
    }
}

fn check_ports(f: &[(String, Option<String>)]) -> i32 {
    let mut bad = 0;
    if let Some(list) = get(f, "ports") {
        for p in list.split(',').filter(|s| !s.trim().is_empty()) {
            match p.trim().parse::<u16>() {
                Ok(p) => {
                    if let Err(e) = ports::check(p) {
                        eprintln!("{e}");
                        bad += 1;
                    }
                }
                Err(_) => {
                    eprintln!("`{p}` is not a port");
                    bad += 1;
                }
            }
        }
    } else if has(f, "config") {
        let mut c = load_config(f);
        if let Some(b) = get(f, "base-port") {
            c.base_port = b
                .parse()
                .unwrap_or_else(|_| die("--base-port is not a port"));
        }
        let ps = c.ports().unwrap_or_else(|e| die(e));
        let probs = ports::problems(&ps, true);
        for p in &probs {
            eprintln!("{p}");
        }
        bad = probs.len();
        if bad == 0 {
            println!(
                "ports ok: {}",
                ps.all()
                    .iter()
                    .map(|(n, p)| format!("{n} {p}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    } else {
        usage()
    }
    if bad == 0 {
        0
    } else {
        1
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first().cloned() else {
        usage()
    };
    let flags = config::split_flags(&args[1..]).unwrap_or_else(|e| die(e));
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|e| die(e));
    let code = match cmd.as_str() {
        "check-ports" => check_ports(&flags),
        "up" => {
            let mut c = load_config(&flags);
            let rest: Vec<(String, Option<String>)> = flags
                .iter()
                .filter(|(k, _)| k != "config")
                .cloned()
                .collect();
            config::apply_flags(&mut c, &rest).unwrap_or_else(|e| die(e));
            rt.block_on(up::up(c))
        }
        "verify" => rt.block_on(verifyrun::verify(&run_dir(&flags))),
        "tamper" => verifyrun::tamper(&run_dir(&flags), has(&flags, "strict")),
        "load" => {
            let n: usize = get(&flags, "viewers")
                .map(|v| v.parse().unwrap_or_else(|_| die("--viewers")))
                .unwrap_or(5_000);
            let h: f64 = get(&flags, "game-hours")
                .map(|v| v.parse().unwrap_or_else(|_| die("--game-hours")))
                .unwrap_or(1.0);
            rt.block_on(load::load(&run_dir(&flags), n, h))
        }
        "report" => rt.block_on(report::report(&run_dir(&flags))),
        "down" => up::down(&run_dir(&flags)),
        "-h" | "--help" | "help" => usage(),
        other => die(format!("unknown subcommand `{other}`")),
    };
    std::process::exit(code);
}
