//! `frontier-verify` (M1 contract §8.5): verifies a season and writes
//! `report.json` and `report.md`. Exit 0 PASS, 1 FAIL, 2 cannot verify.
//!
//! ```text
//! frontier-verify [tamper] --fixture FILE [options]
//! frontier-verify [tamper] --program ID --season N --archive DIR (--finals FILE | --rpc URL) [options]
//! frontier-verify [tamper] --program ID --season N --rpc URL [--localnet] [options]
//!
//! options: --test-key | --quicknet-pk HEX   the pinned drand key (default: quicknet)
//!          --ruleset HEX                    expected ruleset hash (default: this build's)
//!          --program-hash HEX               expected .so sha256 (V2)
//!          --out DIR                        report directory (default: verify-report)
//!          --json                           print the JSON report instead of the Markdown
//!          --save FILE                      also write the input read as a fixture (.gz: gzip)
//!          --jobs N                         tamper: classes judged at a time (default: cores, ≤ 8)
//!          --fallback-fixtures DIR          tamper: judge a class the run cannot express on the
//!                                           committed fixtures in DIR instead (labelled)
//!          --strict                         tamper: any such fallback exits 2
//! ```
//!
//! `tamper` (W5-D) runs T1–T22 and the extra checks of the checks over the
//! run instead ([`verify_core::tamper::run_suite`]) and writes `tamper.json`
//! and `tamper.md`: exit 0 every required class FAILS with its code, 1 a
//! class was missed, 2 the run itself does not PASS or a class could not
//! be built on it.
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
            .take(14)
            .map(|l| {
                l.strip_prefix("//! ")
                    .or_else(|| l.strip_prefix("//!"))
                    .unwrap_or(l)
            })
            .collect::<Vec<_>>()
            .join("\n")
    );
    std::process::exit(verify_core::EXIT_UNVERIFIABLE);
}

fn main() {
    // A `mutate-*` build has a check disabled (the checks of the checks):
    // it never passes for the verifier (wave-5 review of W5-D).
    if let Some(m) = verify_core::mutated() {
        eprintln!("frontier-verify: MUTATED BUILD ({m}): a check is disabled; this binary is not a verifier");
        if std::env::var(verify_core::ALLOW_MUTATED_ENV).as_deref() != Ok("1") {
            eprintln!(
                "frontier-verify: refusing to run (rebuild without --features {m}, or set {}=1 for the checks of the checks)",
                verify_core::ALLOW_MUTATED_ENV
            );
            std::process::exit(verify_core::EXIT_UNVERIFIABLE);
        }
    }
    let mut a = std::env::args().skip(1).peekable();
    let tamper = a.peek().is_some_and(|x| x == "tamper");
    if tamper {
        a.next();
    }
    let mut save: Option<PathBuf> = None;
    let mut jobs: usize = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(8);
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
    let mut fallback: Option<PathBuf> = None;
    let mut strict = false;
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
            "--save" => save = Some(val().into()),
            "--jobs" => jobs = val().parse().unwrap_or_else(|_| usage("bad --jobs")),
            "--fallback-fixtures" => fallback = Some(val().into()),
            "--strict" => strict = true,
            "-h" | "--help" => usage("help"),
            other => usage(&format!("unknown argument {other}")),
        }
    }
    let quicknet = || hex_arr::<96>(fclient::beacon::QUICKNET_PK).expect("pinned quicknet key");
    let from_fixture = fixture.is_some();
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
            (None, Some(u)) => {
                // Every program account of the season, not only those the
                // archive's transactions wrote (integ-W4 review).
                let mut keys = input::written_keys(&txs);
                match rt.block_on(input::program_accounts(u, &program, season_id)) {
                    Ok(v) => {
                        for k in v {
                            if !keys.contains(&k) {
                                keys.push(k);
                            }
                        }
                    }
                    Err(e) => eprintln!(
                        "frontier-verify: getProgramAccounts failed ({e}): only the archive's written accounts are read"
                    ),
                }
                rt.block_on(input::fetch_finals(u, &keys))
                    .unwrap_or_else(|e| usage(&e))
            }
            (None, None) => usage("--finals or --rpc is required with --archive"),
        };
        // V2's `.so` hash from the ProgramData account (integ-W4 review).
        let program_hashes = match &rpc {
            Some(u) => match rt.block_on(input::program_hash(u, &program)) {
                Ok((slot, h)) => vec![(slot, h)],
                Err(e) => {
                    eprintln!("frontier-verify: the deployed .so hash: {e}");
                    vec![]
                }
            },
            None => vec![],
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
            program_hashes,
            provenance: archive
                .map(|d| format!("archive {}", d.display()))
                .unwrap_or_else(|| format!("rpc {}", rpc.clone().unwrap_or_default())),
            scenarios: vec![],
        }
    };
    // A fixture carries its own trust roots; say so when the operator did
    // not pin them (integ-W4 review, W4-D minor).
    if from_fixture && pk.is_none() {
        eprintln!("frontier-verify: warning: the drand key is the fixture's own (pass --test-key or --quicknet-pk to pin it)");
    }
    if from_fixture && ruleset.is_none() {
        eprintln!("frontier-verify: warning: the ruleset hash is the fixture's own (pass --ruleset to pin it)");
    }
    if let Some(k) = pk {
        inp.cfg.quicknet_pk = k;
    }
    if let Some(h) = ruleset {
        inp.cfg.ruleset_hash = h;
    }
    if program_hash.is_some() {
        inp.cfg.program_hash = program_hash;
    }
    if let Some(p) = &save {
        if let Err(e) = inp.save(p) {
            eprintln!(
                "frontier-verify: cannot save the input to {}: {e}",
                p.display()
            );
            std::process::exit(verify_core::EXIT_UNVERIFIABLE);
        }
        eprintln!(
            "frontier-verify: saved the input ({} transactions) to {}",
            inp.txs.len(),
            p.display()
        );
    }
    if tamper {
        let rep = match &fallback {
            Some(dir) => {
                let fx = verify_core::tamper::committed_fixtures(dir).unwrap_or_else(|e| usage(&e));
                verify_core::tamper::run_suite_with_fixtures(&inp, &fx, jobs)
            }
            None => verify_core::tamper::run_suite(&inp, jobs),
        };
        let j = serde_json::to_string_pretty(&rep.json()).unwrap_or_default();
        if let Err(e) = std::fs::create_dir_all(&out)
            .and_then(|_| std::fs::write(out.join("tamper.json"), &j))
            .and_then(|_| std::fs::write(out.join("tamper.md"), rep.markdown()))
        {
            eprintln!(
                "frontier-verify: cannot write the tamper report to {}: {e}",
                out.display()
            );
            std::process::exit(verify_core::EXIT_UNVERIFIABLE);
        }
        if json {
            println!("{j}");
        } else {
            print!("{}", rep.markdown());
        }
        std::process::exit(if strict {
            rep.exit_code_strict()
        } else {
            rep.exit_code()
        });
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
        // A report that cannot be written is not a verification (integ-W4
        // review, W4-D minor).
        std::process::exit(verify_core::EXIT_UNVERIFIABLE);
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
