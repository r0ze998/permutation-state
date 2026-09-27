//! `frontier-verify` (M1 contract §8.5): verifies a season and writes
//! `report.json` and `report.md`. Exit 0 PASS, 1 FAIL, 2 cannot verify.
//!
//! ```text
//! frontier-verify --fixture FILE [options]
//! frontier-verify --program ID --season N --archive DIR (--finals FILE | --rpc URL) [options]
//! frontier-verify --program ID --season N --rpc URL [--localnet] [options]
//!
//! options: --test-key | --quicknet-pk HEX   the pinned drand key (default: quicknet)
//!          --ruleset HEX                    expected ruleset hash (default: this build's)
//!          --program-hash HEX               expected .so sha256 (V2)
//!          --out DIR                        report directory (default: verify-report)
//!          --json                           print report.json instead of report.md
//! ```
//!
//! A fixture file carries its own configuration; the options override it
//! (the tamper classes T10 and T11 are a wrong key and a wrong ruleset).

use std::path::PathBuf;

use verify_core::input::{self, hex_arr, Config, Input};

fn usage(msg: &str) -> ! {
    eprintln!("frontier-verify: {msg}\n");
    eprintln!(
        "{}",
        include_str!("main.rs")
            .lines()
            .skip(4)
            .take(12)
            .map(|l| l.trim_start_matches("//! "))
            .collect::<Vec<_>>()
            .join("\n")
    );
    std::process::exit(verify_core::EXIT_UNVERIFIABLE);
}

fn main() {
    let mut a = std::env::args().skip(1);
    let mut fixture: Option<PathBuf> = None;
    let mut archive: Option<PathBuf> = None;
    let mut finals: Option<PathBuf> = None;
    let mut rpc: Option<String> = None;
    let mut localnet = false;
    let mut program: Option<String> = None;
    let mut season: Option<u64> = None;
    let mut pk: Option<[u8; 96]> = None;
    let mut ruleset: Option<[u8; 32]> = None;
    let mut program_hash: Option<[u8; 32]> = None;
    let mut out = PathBuf::from("verify-report");
    let mut json = false;
    while let Some(x) = a.next() {
        let mut val = || {
            a.next()
                .unwrap_or_else(|| usage(&format!("{x} needs a value")))
        };
        match x.as_str() {
            "--fixture" => fixture = Some(val().into()),
            "--archive" => archive = Some(val().into()),
            "--finals" => finals = Some(val().into()),
            "--rpc" => rpc = Some(val()),
            "--localnet" => localnet = true,
            "--program" => program = Some(val()),
            "--season" => season = Some(val().parse().unwrap_or_else(|_| usage("bad --season"))),
            "--test-key" => pk = Some(fclient::beacon::TestKey::new().pk96),
            "--quicknet-pk" => pk = Some(hex_arr(&val()).unwrap_or_else(|e| usage(&e))),
            "--ruleset" => ruleset = Some(hex_arr(&val()).unwrap_or_else(|e| usage(&e))),
            "--program-hash" => program_hash = Some(hex_arr(&val()).unwrap_or_else(|e| usage(&e))),
            "--out" => out = val().into(),
            "--json" => json = true,
            "-h" | "--help" => usage("help"),
            other => usage(&format!("unknown argument {other}")),
        }
    }
    let quicknet = || hex_arr::<96>(fclient::beacon::QUICKNET_PK).expect("pinned quicknet key");
    let mut inp = if let Some(f) = fixture {
        Input::load(&f).unwrap_or_else(|e| usage(&e))
    } else {
        let program = program
            .unwrap_or_else(|| usage("--program is required"))
            .parse()
            .unwrap_or_else(|_| usage("bad --program"));
        let season_id = season.unwrap_or_else(|| usage("--season is required"));
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let txs = match (&archive, &rpc) {
            (Some(d), _) => input::read_archive(d).unwrap_or_else(|e| usage(&e)),
            (None, Some(u)) => rt
                .block_on(input::fetch_txs(u, &program, localnet))
                .unwrap_or_else(|e| usage(&e)),
            (None, None) => usage("--archive or --rpc is required"),
        };
        let (finals, final_slot) = match (&finals, &rpc) {
            (Some(f), _) => {
                let s = std::fs::read_to_string(f).unwrap_or_else(|e| usage(&e.to_string()));
                let v: serde_json::Value =
                    serde_json::from_str(&s).unwrap_or_else(|e| usage(&e.to_string()));
                (
                    input::finals_from_json(v.get("finals").unwrap_or(&v))
                        .unwrap_or_else(|e| usage(&e)),
                    v.get("final_slot").and_then(|x| x.as_u64()).unwrap_or(0),
                )
            }
            (None, Some(u)) => rt
                .block_on(input::fetch_finals(u, &input::written_keys(&txs)))
                .unwrap_or_else(|e| usage(&e)),
            (None, None) => usage("--finals or --rpc is required with --archive"),
        };
        Input {
            cfg: Config {
                program,
                season_id,
                quicknet_pk: quicknet(),
                ruleset_hash: frontier_abi::presets::RULESET_HASH,
                program_hash: None,
            },
            txs,
            finals,
            final_slot,
            program_hashes: vec![],
            provenance: archive
                .map(|d| format!("archive {}", d.display()))
                .unwrap_or_else(|| format!("rpc {}", rpc.clone().unwrap_or_default())),
            scenarios: vec![],
        }
    };
    if let Some(k) = pk {
        inp.cfg.quicknet_pk = k;
    }
    if let Some(h) = ruleset {
        inp.cfg.ruleset_hash = h;
    }
    if program_hash.is_some() {
        inp.cfg.program_hash = program_hash;
    }
    let r = verify_core::verify(&inp);
    if let Err(e) = std::fs::create_dir_all(&out)
        .and_then(|_| {
            std::fs::write(
                out.join("report.json"),
                serde_json::to_string_pretty(&r.json()).unwrap_or_default(),
            )
        })
        .and_then(|_| std::fs::write(out.join("report.md"), r.markdown()))
    {
        eprintln!(
            "frontier-verify: cannot write the report to {}: {e}",
            out.display()
        );
    }
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&r.json()).unwrap_or_default()
        );
    } else {
        print!("{}", r.markdown());
    }
    std::process::exit(verify_core::exit_code(r.verdict));
}
