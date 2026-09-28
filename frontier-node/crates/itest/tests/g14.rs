//! **G14** (M1 contract §13.3, Gate W5; owner W5-C): 100 bots × 2 game
//! days in LiteSVM through the program with the native kernel re-run every
//! bell (test key); verifier PASS; one tampered log → FAIL.
//!
//! The run is `itest::day` (the `inproc_day` system: the test-beacon
//! program on `localnet` in process at 20×, the keeper with every role,
//! the herald fold, the relay stand-in and the bots) over **288 bells** of
//! play and the 26-bell keeper-only drain, recorded as a verifier input
//! (`VERIFY_DUMP`'s recorder). Pass:
//!
//! 1. every `inproc_day` condition (`itest::gate::day_conditions`: zero
//!    stuck province-bells, every transit settled one to one, bad seals
//!    destroyed with the stock code, min-tip marches revealed, personas,
//!    keeper writes, resident liveness, herald, drain guard);
//! 2. **native kernel every bell** (`itest::native`): every CLASH re-run
//!    with `clash::resolve_clash` from the chain's bytes (outcome digest,
//!    engagements, fates, input digest), every SKIP replayed bell by bell
//!    with `clash::is_quiet` at every bell and the Province it wrote
//!    reproduced byte for byte, and every Province's records covering each
//!    bell once from its opening to its final `resolved_next`;
//! 3. **verifier PASS** (`verify_core::verify`, V1–V13) on the recording;
//! 4. **one tampered log → FAIL**: a CLASH record's outcome digest altered
//!    and the whole archive re-chained (the consistent forger) fails
//!    `ClashReplayMismatch`; a dropped DEPART fails `ChainGap` /
//!    `HeadMismatch` (T1 on this recording).
//!
//! Environment: `ITEST_BOTS` (100), `ITEST_BELLS` (288; smaller only with
//! `ITEST_ALLOW_SMALL=1`, never a gate run), `G14_DUMP=<file.json.gz>`
//! keeps the recording, `G14_SUMMARY=<file>` writes the JSON summary,
//! `PSF_FRONTIER_SO` / `ITEST_NO_BUILD` as `inproc_day`.

use itest::checks::Cond;
use itest::day::{self, DayCfg};
use itest::gate::day_conditions;
use serde_json::json;
use verify_core::codes::{CHAIN_GAP, CLASH_REPLAY_MISMATCH, HEAD_MISMATCH};
use verify_core::tamper;
use verify_core::{Input, Verdict};

fn env_num(k: &str, d: u64) -> u64 {
    std::env::var(k)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(d)
}

/// The FAIL codes of a verification (for the printout).
fn fail_codes(r: &verify_core::Report) -> Vec<String> {
    let mut v: Vec<String> = r
        .findings
        .iter()
        .filter(|f| f.fail())
        .map(|f| format!("{} ({}, bell {}: {})", f.code, f.entity, f.bell, f.detail))
        .collect();
    v.truncate(12);
    v
}

/// The consistent forger's CLASH: the outcome digest of the middle CLASH
/// flipped, then every chain re-computed.
fn tampered_clash(inp: &Input) -> Option<Input> {
    let clashes = tamper::find(inp, frontier_abi::log::Kind::CLASH);
    let &(t, i) = clashes.get(clashes.len() / 2)?;
    let mut x = inp.clone();
    tamper::edit_payload(&mut x, t, i, |p| p[0] ^= 0x01);
    tamper::rechain(&mut x);
    Some(x)
}

/// The native re-run, the verifier and the tampers over a kept recording
/// (`G14_DUMP` from an earlier `g14_two_game_days`): re-checks a run
/// without replaying it (not a gate line: named without the `g14_` prefix
/// so Gate W5's `--include-ignored g14_` does not select it without a
/// recording, integ-W5).
#[test]
#[ignore = "needs G14_DUMP=<recording>"]
fn recheck_a_g14_recording() {
    let dump = std::env::var("G14_DUMP").expect("G14_DUMP");
    let inp = Input::load(std::path::Path::new(&dump)).expect("the recording");
    let nat = itest::native::rerun(&inp.txs, &inp.cfg.program);
    println!("native: {}", nat.to_json());
    let rep = verify_core::verify(&inp);
    println!("verify: {} {:?}", rep.verdict.name(), fail_codes(&rep));
    for f in rep.findings.iter().filter(|f| !f.fail()) {
        println!(
            "  warn {} {} bell {}: {}",
            f.code, f.entity, f.bell, f.detail
        );
    }
    let x = tampered_clash(&inp).expect("a CLASH");
    let r = verify_core::verify(&x);
    println!("tampered CLASH: {} {:?}", r.verdict.name(), fail_codes(&r));
    assert!(nat.ok(), "{}", nat.to_json());
    assert_eq!(rep.verdict, Verdict::Pass);
    assert!(r.fails_with(CLASH_REPLAY_MISMATCH));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "two game days in process (minutes of wall time); Gate W5 runs it with --include-ignored g14_"]
async fn g14_two_game_days() {
    let dir = std::env::temp_dir().join(format!("itest-g14-{}", std::process::id()));
    let mut cfg = DayCfg::new(&dir);
    cfg.bots = env_num("ITEST_BOTS", 100) as usize;
    cfg.play_bells = env_num("ITEST_BELLS", 288) as u32;
    let small = cfg.bots < 100 || cfg.play_bells < 288;
    assert!(
        !small || std::env::var("ITEST_ALLOW_SMALL").is_ok_and(|v| v == "1"),
        "G14 is 100 bots × 2 game days (ITEST_ALLOW_SMALL=1 for a smaller, non-gate run)"
    );
    let dump = std::env::var("G14_DUMP")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::temp_dir().join(format!("itest-g14-{}.json.gz", std::process::id()))
        });
    cfg.verify_dump = Some(dump.clone());
    let out = day::run(cfg).await.expect("the run");
    let mut conds = day_conditions(&out);

    // 2. The native kernel at every bell.
    let t0 = std::time::Instant::now();
    let inp = Input::load(&dump).expect("the recording");
    let nat = itest::native::rerun(&inp.txs, &inp.cfg.program);
    let final_rn: std::collections::BTreeMap<(i32, i32), u32> = out
        .facts
        .provinces
        .iter()
        .map(|&(p, q, rn)| ((p as i32, q as i32), rn))
        .collect();
    let uncovered: Vec<String> = final_rn
        .iter()
        .filter(|(pq, rn)| nat.resolved_next.get(pq) != Some(rn))
        .map(|(pq, rn)| {
            format!(
                "{pq:?}: final {rn}, records reach {:?}",
                nat.resolved_next.get(pq)
            )
        })
        .collect();
    let native_secs = t0.elapsed().as_secs_f64();
    conds.push((
        "native-kernel-every-bell",
        if nat.ok() && uncovered.is_empty() && nat.clashes > 0 && nat.skipped_bells > 0 {
            Cond::Pass(format!(
                "{} province-bells re-run natively over {} provinces: {} CLASH all matched (digest, engagements, fates, input digest), {} SKIP over {} bells all kernel-quiet at every bell and reproduced byte for byte ({native_secs:.1} s)",
                nat.bells(),
                final_rn.len(),
                nat.clashes,
                nat.skips,
                nat.skipped_bells
            ))
        } else {
            Cond::Fail(format!(
                "{}; not covered to the final state: {:?}",
                nat.to_json(),
                &uncovered[..uncovered.len().min(8)]
            ))
        },
    ));

    // 3. The verifier on the recording.
    let t1 = std::time::Instant::now();
    let rep = verify_core::verify(&inp);
    let verify_secs = t1.elapsed().as_secs_f64();
    let warns: std::collections::BTreeMap<String, usize> = rep
        .findings
        .iter()
        .filter(|f| !f.fail())
        .fold(Default::default(), |mut m, f| {
            *m.entry(f.code.clone()).or_default() += 1;
            m
        });
    conds.push((
        "verifier-pass",
        if rep.verdict == Verdict::Pass {
            Cond::Pass(format!(
                "PASS in {verify_secs:.1} s: {} txs, {} clashes, {} skips, {} seals, {} bad seals; warnings {warns:?}",
                rep.counts.tx, rep.counts.clashes, rep.counts.skips, rep.counts.seals, rep.counts.bad_seals
            ))
        } else {
            Cond::Fail(format!("{}: {:?}", rep.verdict.name(), fail_codes(&rep)))
        },
    ));

    // 4. One tampered log → FAIL (and T1 on this recording).
    let mut tampers = vec![];
    match tampered_clash(&inp) {
        Some(x) => {
            let r = verify_core::verify(&x);
            tampers.push((
                "CLASH outcome digest (re-chained)",
                r.verdict == Verdict::Fail && r.fails_with(CLASH_REPLAY_MISMATCH),
                fail_codes(&r),
            ));
        }
        None => tampers.push((
            "CLASH outcome digest (re-chained)",
            false,
            vec!["no CLASH".into()],
        )),
    }
    // W5-D: tamper builders return `Made = Result<Case, String>` (a run
    // may lack what a class needs); a T1 that cannot be built fails G14.
    match tamper::t01(&inp) {
        Ok(t01) => {
            let r = verify_core::verify(&t01.input);
            tampers.push((
                "T1 drop a DEPART",
                r.verdict == Verdict::Fail
                    && (r.fails_with(CHAIN_GAP) || r.fails_with(HEAD_MISMATCH)),
                fail_codes(&r),
            ));
        }
        Err(e) => tampers.push(("T1 drop a DEPART", false, vec![format!("not built: {e}")])),
    }
    conds.push((
        "tampered-log-fails",
        if tampers.iter().all(|(_, ok, _)| *ok) {
            Cond::Pass(format!(
                "{:?}",
                tampers
                    .iter()
                    .map(|(w, _, c)| format!(
                        "{w}: FAIL {}",
                        c.first().cloned().unwrap_or_default()
                    ))
                    .collect::<Vec<_>>()
            ))
        } else {
            Cond::Fail(format!("{tampers:?}"))
        },
    ));

    let mut summary = out.summary();
    summary["g14"] = json!({
        "native": nat.to_json(),
        "nativeSecs": native_secs,
        "verify": rep.json(),
        "verifySecs": verify_secs,
        "tampers": tampers.iter().map(|(w, ok, c)| json!({"what": w, "fails": ok, "codes": c})).collect::<Vec<_>>(),
    });
    if let Ok(p) = std::env::var("G14_SUMMARY") {
        std::fs::write(&p, serde_json::to_string_pretty(&summary).unwrap()).expect("G14_SUMMARY");
    }
    println!(
        "\ng14_two_game_days ({} bots, {} bells + {} drain, {} slots, {:.0} s wall, program {}{}):",
        out.cfg.bots,
        out.cfg.play_bells,
        out.cfg.drain_bells,
        out.slots,
        out.wall_secs,
        &out.so_sha256[..16],
        if small {
            ", SMALL (not a gate run)"
        } else {
            ""
        }
    );
    for (name, c) in &conds {
        let (l, d) = c.label();
        println!("  {l:8} {name}: {d}");
    }
    let fails: Vec<&str> = conds
        .iter()
        .filter(|(_, c)| c.is_fail())
        .map(|(n, _)| *n)
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    if std::env::var("G14_DUMP").is_err() {
        let _ = std::fs::remove_file(&dump);
    }
    assert!(fails.is_empty(), "failed: {fails:?}");
}
