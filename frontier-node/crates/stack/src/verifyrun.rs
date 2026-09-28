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
//! `tamper` builds every class of `verify_core::tamper` (T1–T22 plus T1b,
//! T6b, T23 and V9a) **from the run's own input**, verifies each copy and
//! checks it FAILs with one of the class's codes. A class the run cannot
//! express (its builder finds nothing to edit, e.g. T13 needs a late
//! Reveal the program never lets land) is `not-applicable` on the run and
//! is then run on the committed fixtures instead (`fixture` column), so
//! every class still gets a verdict; the report counts both. Exit 0 when
//! every class FAILs with its code (on the run or the fixture), 1
//! otherwise; `--strict` also fails on any fixture fallback.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::Instant;

use fclient::Address;
use serde_json::{json, Value};
use verify_core::input::{self, Config, Input};
use verify_core::tamper::{self, Case};
use verify_core::Verdict;

use crate::run::RunDir;

/// A class builder (W5-D: `Err` when the run lacks what the class edits).
type Builder = fn(&Input) -> tamper::Made;

/// Every tamper class and which recorded fixture its builder uses when
/// the run cannot express it (`land` or `march`, the committed ones).
pub const CLASSES: &[(&str, Builder, &str)] = &[
    ("T1", tamper::t01, "march"),
    ("T1b", tamper::t01b, "land"),
    ("T2", tamper::t02, "march"),
    ("T3", tamper::t03, "land"),
    ("T4", tamper::t04, "land"),
    ("T5", tamper::t05, "land"),
    ("T6", tamper::t06, "march"),
    ("T6b", tamper::t06b, "program"),
    ("T7", tamper::t07, "march"),
    ("T8", tamper::t08, "march"),
    ("T9", tamper::t09, "march"),
    ("T10", tamper::t10, "land"),
    ("T11", tamper::t11, "land"),
    ("T12", tamper::t12, "march"),
    ("T13", tamper::t13, "march"),
    ("T14", tamper::t14, "land"),
    ("T15", tamper::t15, "march"),
    ("T16", tamper::t16, "land"),
    ("T17", tamper::t17, "march"),
    ("T18", tamper::t18, "land"),
    ("T19", tamper::t19, "land"),
    ("T20", tamper::t20, "march"),
    ("T21", tamper::t21, "march"),
    ("T22", tamper::t22, "march"),
    ("T23", tamper::t23, "program"),
    ("V9a", tamper::v9a, "land"),
];

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
    let so = st["so"]["sha256"]
        .as_str()
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

/// One class judged.
#[derive(Clone, Debug)]
pub struct Judged {
    pub class: &'static str,
    pub on: &'static str,
    pub what: String,
    pub codes: Vec<&'static str>,
    pub verdict: String,
    pub hit: Vec<String>,
    pub ok: bool,
    pub note: Option<String>,
}

fn quiet<T>(f: impl FnOnce() -> T) -> std::thread::Result<T> {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = catch_unwind(AssertUnwindSafe(f));
    std::panic::set_hook(prev);
    r
}

fn judge_case(class: &'static str, on: &'static str, case: Case) -> Judged {
    let r = verify_core::verify(&case.input);
    let hit: Vec<String> = case
        .codes
        .iter()
        .filter(|c| r.fails_with(c))
        .map(|c| c.to_string())
        .collect();
    Judged {
        class,
        on,
        what: case.what.to_string(),
        codes: case.codes.to_vec(),
        verdict: r.verdict.name().to_string(),
        ok: r.verdict == Verdict::Fail && !hit.is_empty(),
        hit,
        note: None,
    }
}

fn fixture(repo: &Path, which: &str) -> Result<Input, String> {
    let f = match which {
        "land" => "frontier-node/fixtures/verify/land-program.json",
        "program" => "frontier-node/fixtures/verify/march-program.json.gz",
        _ => "frontier-node/fixtures/verify/march-synth.json",
    };
    Input::load(&repo.join(f))
}

/// Runs every class on `run` (falling back to the fixtures).
pub fn run_classes(run: &Input, repo: &Path) -> Vec<Judged> {
    let mut out = vec![];
    for (class, build, fx) in CLASSES {
        match quiet(|| build(run)) {
            Ok(Ok(case)) => {
                let j = judge_case(class, "run", case);
                out.push(j);
            }
            Ok(Err(_)) | Err(_) => {
                let mut j = match fixture(repo, fx).map(|inp| quiet(|| build(&inp))) {
                    Ok(Ok(Ok(case))) => judge_case(class, "fixture", case),
                    Ok(Ok(Err(_))) | Ok(Err(_)) | Err(_) => Judged {
                        class,
                        on: "none",
                        what: String::new(),
                        codes: vec![],
                        verdict: "NOT-BUILT".into(),
                        hit: vec![],
                        ok: false,
                        note: Some(format!("no {fx} fixture case either")),
                    },
                };
                j.note.get_or_insert_with(|| {
                    format!("not applicable on this run (the builder found nothing to edit); judged on the {fx} fixture")
                });
                out.push(j);
            }
        }
    }
    out
}

pub fn render(js: &[Judged]) -> (Value, String) {
    let rows: Vec<Value> = js
        .iter()
        .map(|j| json!({"class": j.class, "on": j.on, "what": j.what, "codes": j.codes, "verdict": j.verdict,
            "hit": j.hit, "ok": j.ok, "note": j.note}))
        .collect();
    let on_run = js.iter().filter(|j| j.on == "run").count();
    let ok = js.iter().filter(|j| j.ok).count();
    let mut md = String::from("# Tamper suite on the run\n\n| class | on | verdict | codes hit | expected | note |\n|---|---|---|---|---|---|\n");
    for j in js {
        md.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            j.class,
            j.on,
            j.verdict,
            j.hit.join(", "),
            j.codes.join(" / "),
            j.note.clone().unwrap_or_default()
        ));
    }
    md.push_str(&format!(
        "\n{ok}/{} classes FAIL with their codes; {on_run} built from the run, {} from the fixtures.\n",
        js.len(),
        js.len() - on_run
    ));
    (
        json!({"classes": rows, "ok": ok, "total": js.len(), "on_run": on_run, "on_fixture": js.len() - on_run}),
        md,
    )
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
    let js = run_classes(&run, &repo);
    let (v, md) = render(&js);
    let dir = rd.path("tamper");
    let _ = std::fs::create_dir_all(&dir);
    let mut v = v;
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
        json!({"ok": v["ok"], "total": v["total"], "on_run": v["on_run"]}),
    );
    print!("{md}");
    let all = js.iter().all(|j| j.ok);
    let fallback = js.iter().any(|j| j.on != "run");
    if all && !(strict && fallback) {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
    }

    #[test]
    fn every_class_is_listed_once() {
        let names: std::collections::BTreeSet<_> = CLASSES.iter().map(|c| c.0).collect();
        assert_eq!(names.len(), CLASSES.len());
        for i in 1..=22 {
            assert!(names.contains(format!("T{i}").as_str()), "T{i}");
        }
    }

    #[test]
    fn the_suite_on_the_committed_program_recording() {
        // The strict in-process day of the program (W4-D/integ-W4): the
        // closest thing to a stack run in the repository.
        let run = fixture(&repo(), "program").expect("march-program fixture");
        assert_eq!(
            verify_core::verify(&run).verdict,
            Verdict::Pass,
            "the honest recording passes"
        );
        let js = run_classes(&run, &repo());
        let bad: Vec<_> = js
            .iter()
            .filter(|j| !j.ok)
            .map(|j| (j.class, j.verdict.clone(), j.note.clone()))
            .collect();
        assert!(bad.is_empty(), "{bad:?}");
        let (v, md) = render(&js);
        assert_eq!(v["ok"], js.len());
        assert!(md.contains("| T22 |"));
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
