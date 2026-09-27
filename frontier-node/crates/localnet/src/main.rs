//! `frontier-localnet`: serves the local chain on 127.0.0.1.
//!
//! ```text
//! frontier-localnet [--port 41010] [--scale 20] [--g0 <unix>] [--allow-tamper]
//!                   [--program <id>=<path.so>[:<max_len>][@<authority>]]...
//! ```
//! Ports: 41000–41999 only, never a reserved one (§10.3).

use std::sync::{Arc, Mutex};

use localnet::{server, Chain, Config};

fn usage() -> ! {
    eprintln!("usage: frontier-localnet [--port 41010] [--scale S] [--g0 UNIX] [--allow-tamper] [--program ID=PATH[:MAXLEN][@AUTHORITY]]...");
    std::process::exit(2)
}

#[tokio::main]
async fn main() {
    let mut cfg = Config::default();
    let mut port: u16 = 41_010;
    let mut programs: Vec<String> = vec![];
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--port" => {
                port = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| usage())
            }
            "--scale" => {
                cfg.scale = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| usage())
            }
            "--g0" => {
                cfg.g0 = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| usage())
            }
            "--allow-tamper" => cfg.allow_tamper = true,
            "--program" => programs.push(args.next().unwrap_or_else(|| usage())),
            _ => usage(),
        }
    }
    if port == 0 {
        eprintln!("--port 0 is for tests; pick a port in 41000-41999");
        std::process::exit(2);
    }
    let mut chain = Chain::new(cfg.clone());
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
        let so = std::fs::read(path).unwrap_or_else(|e| {
            eprintln!("{path}: {e}");
            std::process::exit(1)
        });
        let max_len = max_len.unwrap_or(fclient::fees::deploy_max_len(so.len() as u64) as usize);
        chain
            .deploy(id.parse().unwrap_or_else(|_| usage()), &so, max_len, auth)
            .unwrap_or_else(|e| {
                eprintln!("deploy {id}: {e}");
                std::process::exit(1)
            });
        eprintln!("deployed {id}: {} B, max_len {max_len}", so.len());
    }
    let state = Arc::new(Mutex::new(chain));
    let running = server::start(state, port).await.unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    eprintln!(
        "frontier-localnet on {} (scale {}, g0 {}, 400-ms slots)",
        running.url(),
        cfg.scale,
        cfg.g0
    );
    let _ = tokio::signal::ctrl_c().await;
    running.stop();
}
