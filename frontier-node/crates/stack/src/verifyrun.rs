//! `verify` and `tamper` on a stack run (M1 contract §8.5, §13.5).
//!
//! `verify` reads the run's chain the way `frontier-verify --rpc --localnet`
//! does (every program transaction from `frontier_feed`, every program
//! account of the season, the deployed `.so`'s hash from its ProgramData),
//! pins the trust roots the run used (the drand key of its beacon mode, the
//! ruleset hash of this build, the `.so` sha256 the stack deployed), keeps
//! the input as `verify/input.json.gz` (the copy the tamper suite edits)
//! and writes `verify/report.{json,md}`. Exit 0 PASS, 1 FAIL, 2 cannot
//! verify.
//!
//! `tamper` runs W5-D's suite (`verify_core::tamper::run_suite`, wave-5
//! review: one implementation for the CLI and the stack) **on the run's own
//! input**: T1–T22 and the extra checks of the checks (T1b, T6b, T23,
//! T23b, H1, H1b, V9a) with every run fallback (T8 fill, T13 moved or
//! forced, T14 any, T20 injected). A class the run still cannot express is
//! judged on the committed fixtures instead and labelled (`on` column:
//! `fixture <name>`); a class whose builder or verification panics is
//! PANICKED and fails. Exit 0 when every class FAILs with its code (on the
//! run or a fixture) and the extra classes are not missed, 1 otherwise;
//! `--strict` also fails on any fixture fallback.

use std::path::{Path, PathBuf};
use std::time::Instant;

use fclient::Address;
use serde_json::{json, Value};
use verify_core::input::{self, Config, Input};
use verify_core::tamper;

use crate::run::RunDir;

pub fn input_path(rd: &RunDir) -> PathBuf {
    rd.path("verify/input.json.gz")
}

/// The trust roots of a run from its state.
pub fn pins(st: &Value) -> Result<([u8; 96], Option<[u8; 32]>), String> {
    let pk = match st["beacon"].as_str() {
        Some("test-key") => fclient::beacon::TestKey::new().pk96,
        Some("archive") => input::hex_arr::<96>(fclient::beacon::QUICKNET_PK)?,
        other => return Err(format!("state: beacon {other:?}")),
    };
    // The release build's recorded hash when the run pinned one (an
    // independent trust root for V2); else the deployed file's own.
    let so = st["so"]["expected_sha256"]
        .as_str()
        .or_else(|| st["so"]["sha256"].as_str())
        .map(input::hex_arr::<32>)
        .transpose()?;
    Ok((pk, so))
}

/// Reads the run's chain into a verifier input.
pub async fn read_input(st: &Value) -> Result<Input, String> {
    let rpc = format!(
        "http://127.0.0.1:{}",
        st["ports"]["localnet"].as_u64().ok_or("state: ports")?
    );
    let program: Address = st["program"]
        .as_str()
        .ok_or("state: program")?
        .parse()
        .map_err(|_| "state: program is not base58")?;
    let season_id = st["season_id"].as_u64().ok_or("state: season_id")?;
    let (pk, so) = pins(st)?;
    let txs = input::fetch_txs(&rpc, &program, true).await?;
    let mut keys = input::written_keys(&txs);
    for k in input::program_accounts(&rpc, &program, season_id).await? {
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    let (finals, final_slot) = input::fetch_finals(&rpc, &keys).await?;
    let program_hashes = match input::program_hash(&rpc, &program).await {
        Ok((slot, h)) => vec![(slot, h)],
        Err(e) => {
            eprintln!("frontier-stack: the deployed .so hash: {e}");
            vec![]
        }
    };
    Ok(Input {
        cfg: Config {
            program,
            season_id,
            quicknet_pk: pk,
            ruleset_hash: frontier_abi::presets::RULESET_HASH,
            program_hash: so,
        },
        txs,
        finals,
        final_slot,
        program_hashes,
        provenance: format!(
            "frontier-stack run {} ({}, {}x, {} bots) over {rpc}",
            st["run_id"].as_str().unwrap_or("?"),
            st["beacon"].as_str().unwrap_or("?"),
            st["config"]["scale"],
            st["config"]["bots"]
        ),
        scenarios: vec![],
    })
}

pub fn write_report(dir: &Path, r: &verify_core::Report) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    crate::run::write_atomic(
        &dir.join("report.json"),
        serde_json::to_string_pretty(&r.json())
            .unwrap_or_default()
            .as_bytes(),
    )?;
    crate::run::write_atomic(&dir.join("report.md"), r.markdown().as_bytes())
}

/// `verify --run-id R`.
pub async fn verify(rd: &RunDir) -> i32 {
    let st = match rd.load_state() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("frontier-stack: {e}");
            return verify_core::EXIT_UNVERIFIABLE;
        }
    };
    // A running chain is read at one instant only if it stands still.
    let rpc = format!(
        "http://127.0.0.1:{}",
        st["ports"]["localnet"].as_u64().unwrap_or(41_010)
    );
    let chain = crate::chain::Chain::new(&rpc, Address::default());
    let was = chain.status().await;
    if let Ok(s) = &was {
        if !s.paused {
            let _ = chain.pause().await;
        }
    }
    let t0 = Instant::now();
    let inp = match read_input(&st).await {
        Ok(i) => i,
        Err(e) => {
            eprintln!("frontier-stack: cannot read the run: {e}");
            return verify_core::EXIT_UNVERIFIABLE;
        }
    };
    let read_secs = t0.elapsed().as_secs_f64();
    // Wave-5 review: a chain that was running is running again.
    if let Ok(s) = &was {
        if !s.paused {
            let _ = chain.resume().await;
        }
    }
    if let Err(e) = inp.save(&input_path(rd)) {
        eprintln!("frontier-stack: {e}");
        return verify_core::EXIT_UNVERIFIABLE;
    }
    let t1 = Instant::now();
    let r = verify_core::verify(&inp);
    let verify_secs = t1.elapsed().as_secs_f64();
    if let Err(e) = write_report(&rd.path("verify"), &r) {
        eprintln!("frontier-stack: cannot write the report: {e}");
        return verify_core::EXIT_UNVERIFIABLE;
    }
    let fails: Vec<&str> = r.codes();
    let summary = json!({"verdict": r.verdict.name(), "fail_codes": fails, "txs": inp.txs.len(),
        "accounts": inp.finals.len(), "final_slot": inp.final_slot, "read_secs": read_secs, "verify_secs": verify_secs,
        "chain_was_paused": was.as_ref().map(|s| s.paused).ok()});
    let _ = crate::run::write_atomic(
        &rd.path("verify/summary.json"),
        serde_json::to_string_pretty(&summary)
            .unwrap_or_default()
            .as_bytes(),
    );
    rd.event(None, "verify", summary.clone());
    print!("{}", r.markdown());
    println!("\nfrontier-stack verify: {} ({} txs, {} accounts; read {read_secs:.1} s, verify {verify_secs:.1} s)",
        r.verdict.name(), inp.txs.len(), inp.finals.len());
    verify_core::exit_code(r.verdict)
}

/// The committed fixtures of the repository (`frontier-node/fixtures/verify`).
fn fixtures(repo: &Path) -> Result<Vec<(&'static str, Input)>, String> {
    tamper::committed_fixtures(&repo.join("frontier-node/fixtures/verify"))
}

/// The suite on `run` with the committed fixtures as fallback.
pub fn run_suite(run: &Input, repo: &Path) -> Result<tamper::SuiteReport, String> {
    let fx = fixtures(repo)?;
    let jobs = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(8);
    Ok(tamper::run_suite_with_fixtures(run, &fx, jobs))
}

/// `tamper/report.json`: the suite's JSON plus the counts the stack's
/// report reads (`ok`, `total`, `on_run`, `on_fixture`).
pub fn render(rep: &tamper::SuiteReport) -> (Value, String) {
    let mut v = rep.json();
    let ok = rep
        .outcomes
        .iter()
        .filter(|o| o.status == tamper::Status::Detected)
        .count();
    v["ok"] = json!(ok);
    v["total"] = json!(rep.outcomes.len());
    (v, rep.markdown())
}

/// `tamper --run-id R [--strict]`.
pub fn tamper(rd: &RunDir, strict: bool) -> i32 {
    let p = input_path(rd);
    let run = match Input::load(&p) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("frontier-stack: {e} (run `frontier-stack verify` first)");
            return 2;
        }
    };
    let repo = match crate::run::repo_root() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("frontier-stack: {e}");
            return 2;
        }
    };
    let t0 = Instant::now();
    let rep = match run_suite(&run, &repo) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("frontier-stack: {e}");
            return 2;
        }
    };
    let (mut v, md) = render(&rep);
    let dir = rd.path("tamper");
    let _ = std::fs::create_dir_all(&dir);
    v["secs"] = json!(t0.elapsed().as_secs_f64());
    let _ = crate::run::write_atomic(
        &dir.join("report.json"),
        serde_json::to_string_pretty(&v)
            .unwrap_or_default()
            .as_bytes(),
    );
    let _ = crate::run::write_atomic(&dir.join("report.md"), md.as_bytes());
    rd.event(
        None,
        "tamper",
        json!({"ok": v["ok"], "total": v["total"], "on_run": v["on_run"], "on_fixture": v["on_fixture"]}),
    );
    print!("{md}");
    let code = if strict {
        rep.exit_code_strict()
    } else {
        rep.exit_code()
    };
    if code == verify_core::EXIT_PASS {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use verify_core::Verdict;

    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
    }

    #[test]
    fn the_suite_on_the_committed_program_recording() {
        // The strict in-process day of the program: the closest thing to
        // a stack run in the repository. Every class is built from the run
        // itself (wave-5 review: a regression that pushed classes onto the
        // fixtures would pass the old test).
        let fx = fixtures(&repo()).expect("fixtures");
        let run = fx
            .iter()
            .find(|(n, _)| *n == "march-program")
            .expect("march-program")
            .1
            .clone();
        let rep = run_suite(&run, &repo()).expect("suite");
        assert_eq!(rep.base, Verdict::Pass, "the honest recording passes");
        let bad: Vec<_> = rep
            .outcomes
            .iter()
            .filter(|o| o.status != tamper::Status::Detected)
            .map(|o| (o.class, o.status.clone(), o.detail.clone()))
            .collect();
        assert!(bad.is_empty(), "{bad:?}");
        assert!(rep.fallbacks().is_empty(), "every class on the run");
        assert_eq!(rep.exit_code_strict(), verify_core::EXIT_PASS);
        let (v, md) = render(&rep);
        assert_eq!(v["ok"], rep.outcomes.len());
        assert!(md.contains("| T22 | run |"));
        for c in ["T23b", "H1", "H1b"] {
            assert!(rep.outcomes.iter().any(|o| o.class == c), "{c} counted");
        }
    }

    #[test]
    fn a_run_without_marches_falls_back_labelled_and_strict_refuses() {
        // The land recording has no march: the march classes are judged on
        // a fixture, labelled, and --strict refuses the result.
        let fx = fixtures(&repo()).expect("fixtures");
        let run = fx
            .iter()
            .find(|(n, _)| *n == "land-program")
            .expect("land-program")
            .1
            .clone();
        let rep = run_suite(&run, &repo()).expect("suite");
        assert!(!rep.fallbacks().is_empty());
        assert!(rep.fallbacks().iter().all(
            |o| o.on.starts_with("fixture ") && o.detail.contains("not applicable on the run")
        ));
        assert!(rep
            .outcomes
            .iter()
            .filter(|o| o.required)
            .all(|o| o.status == tamper::Status::Detected));
        assert_eq!(rep.exit_code(), verify_core::EXIT_PASS);
        assert_eq!(rep.exit_code_strict(), verify_core::EXIT_UNVERIFIABLE);
    }

    #[test]
    fn pins_follow_the_beacon() {
        let tk = pins(&json!({"beacon": "test-key", "so": {"sha256": "00".repeat(32)}})).unwrap();
        assert_eq!(tk.0, fclient::beacon::TestKey::new().pk96);
        assert_eq!(tk.1, Some([0u8; 32]));
        let qn = pins(&json!({"beacon": "archive", "so": {}})).unwrap();
        assert_eq!(
            qn.0.to_vec(),
            hex::decode(fclient::beacon::QUICKNET_PK).unwrap()
        );
        assert!(pins(&json!({})).is_err());
    }
}
