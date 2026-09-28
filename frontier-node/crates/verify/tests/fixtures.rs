//! The recorded fixtures PASS (contract §8.5, Gate W4 "verifier V1–V13
//! PASS on the recorded fixtures"), and they contain the honest-but-adverse
//! scenarios they claim.

mod common;

use common::*;
use frontier_abi::log::Kind;
use verify_core::Verdict;

fn kinds(inp: &verify_core::Input) -> std::collections::BTreeMap<u8, usize> {
    let mut m = std::collections::BTreeMap::new();
    for t in inp.txs.iter().filter(|t| t.err.is_none()) {
        for b in fclient::log::bodies_from_logs(&t.logs, &inp.cfg.program).unwrap() {
            *m.entry(b[1]).or_default() += 1;
        }
    }
    m
}

#[test]
fn land_program_passes() {
    let inp = land();
    let r = verify_core::verify(&inp);
    assert_eq!(r.verdict, Verdict::Pass, "{}", show(&r));
    // What the recording covers (a regression would silently shrink it).
    let k = kinds(&inp);
    for (kind, min) in [
        (Kind::SEASON_CREATED, 1),
        (Kind::GENESIS_SEED, 1),
        (Kind::RING_OPEN, 4),
        (Kind::PROVINCE_OPEN, 37),
        (Kind::JOIN, 7),
        (Kind::TICKET, 7),
        (Kind::SETTLE, 9),
        (Kind::ANCHOR, 30),
        (Kind::SEED, 30),
    ] {
        assert!(
            k.get(&(kind as u8)).copied().unwrap_or(0) >= min,
            "{kind:?}: {k:?}"
        );
    }
    for s in [
        "keeper-crash-mid-bell",
        "held-anchor",
        "prefunded-addresses",
        "displacement",
        "expired-ticket",
    ] {
        assert!(inp.scenarios.iter().any(|x| x == s), "{s}");
    }
    assert!(
        r.liveness.max_anchor_delay_s >= 60.0,
        "the held anchor landed late"
    );
}

/// The march season recorded from the merged program (`itest::inproc_day`
/// with `VERIFY_DUMP`, 100 bots over one game day and the drain; integ-W4
/// review, W4-D blocker: the verifier had never judged the program's own
/// clash, transit, camp and payment records, and FAILed them).
#[test]
fn march_program_passes() {
    let inp = march_program();
    let r = verify_core::verify(&inp);
    assert_eq!(r.verdict, Verdict::Pass, "{}", show(&r));
    let k = kinds(&inp);
    for (kind, min) in [
        (Kind::JOIN, 100),
        (Kind::SETTLE, 90),
        (Kind::DEPART, 40),
        (Kind::REVEAL, 30),
        (Kind::DEPARTURE_SETTLED, 40),
        (Kind::GATHER, 40),
        (Kind::CLASH, 40),
        (Kind::SKIP, 500),
        (Kind::TRANSIT_SETTLED, 40),
        (Kind::EXPLORE_RESULT, 10),
        (Kind::CAMP, 1),
        // W5-D: V7 replays every owner action through the kernel's lazy
        // holding functions and every skipped bell.
        (Kind::HARVEST, 100),
        (Kind::BUILD, 50),
        (Kind::TRAIN, 50),
    ] {
        assert!(
            k.get(&(kind as u8)).copied().unwrap_or(0) >= min,
            "{kind:?}: {k:?}"
        );
    }
    // Bad seals destroyed at settlement are in it.
    let bad = inp
        .txs
        .iter()
        .filter(|t| t.err.is_none())
        .flat_map(|t| fclient::log::bodies_from_logs(&t.logs, &inp.cfg.program).unwrap())
        .filter(|b| b[1] == Kind::TRANSIT_SETTLED as u8 && b[14] == 8)
        .count();
    assert!(bad >= 5, "{bad} bad-seal settlements");
}

#[test]
fn march_synth_passes() {
    let inp = march();
    let r = verify_core::verify(&inp);
    assert_eq!(r.verdict, Verdict::Pass, "{}", show(&r));
    let k = kinds(&inp);
    for (kind, min) in [
        (Kind::DEPART, 12),
        (Kind::REVEAL, 7),
        (Kind::DEPARTURE_SETTLED, 11),
        // integ-W4: regenerated with the program's clash rules (the camp,
        // its day check), one contested bell of the old season is quiet.
        (Kind::GATHER, 2),
        (Kind::CLASH, 2),
        (Kind::SKIP, 10),
        (Kind::TRANSIT_SETTLED, 11),
        (Kind::DEFENCE_CLAIM, 1),
        (Kind::EXPLORE_RESULT, 1),
        (Kind::ARCHIVE, 13),
        (Kind::CLOSE, 13),
    ] {
        assert!(
            k.get(&(kind as u8)).copied().unwrap_or(0) >= min,
            "{kind:?}: {k:?}"
        );
    }
    for s in [
        "lagging-origin",
        "low-tip-rout",
        "bad-seal-revealed-destroyed",
        "bad-seal-unrevealed-destroyed",
        "bad-plaintext-destroyed",
        "bad-seal-settled-after-archive",
        "slot-displacement",
        "quota-refused-arrival-bounced",
        "skip-quiet-runs",
        "archived-anchors-and-tombstones",
    ] {
        assert!(inp.scenarios.iter().any(|x| x == s), "{s}");
    }
    // The outcomes the scenarios name, as the chain logged them.
    let w = verify_core::world::World::parse(
        &inp.cfg.program,
        &inp.txs,
        inp.finals.clone(),
        inp.final_slot,
    );
    let outcomes: Vec<(u8, u8)> = w
        .of(Kind::TRANSIT_SETTLED)
        .map(|r| (r.pu8("outcome"), r.pu8("seal_code")))
        .collect();
    use frontier_abi::log::transit_outcome as o;
    assert_eq!(
        outcomes.iter().filter(|x| x.0 == o::BAD_SEAL).count(),
        3,
        "{outcomes:?}"
    );
    assert!(
        outcomes.iter().any(|x| x.0 == o::ROUTED),
        "the low-tip rout"
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|x| x.0 == o::BOUNCED_UNRANKED)
            .count(),
        2,
        "displaced and refused"
    );
    assert!(
        outcomes.iter().any(|x| x.1 == 5),
        "the bad plaintext's code 5"
    );
    assert_eq!(
        r.liveness.valid_unrevealed.len(),
        2,
        "the rout and the refused arrival"
    );
    assert!(
        w.txs.iter().any(|t| !t.ok && t.code == Some(34)),
        "the QuotaRefused Reveal"
    );
    assert!(!r.liveness.contested_bells.is_empty());
}

/// The committed march fixture is what the generator builds today (the
/// seals' IBE randomness differs run to run, so the check is structural:
/// the same transactions, record kinds and verdict).
#[test]
fn march_fixture_is_fresh() {
    let fresh = verify_core::fixture::march::march();
    let committed = march();
    assert_eq!(
        kinds(&fresh),
        kinds(&committed),
        "re-record: cargo test -p verify --test record -- --ignored record_march_synth"
    );
    assert_eq!(fresh.txs.len(), committed.txs.len());
    let tags = |i: &verify_core::Input| -> Vec<Vec<u8>> {
        i.txs
            .iter()
            .map(|t| {
                let tx = fclient::tx::from_wire(&t.tx).unwrap();
                tx.message.instructions.iter().map(|c| c.data[0]).collect()
            })
            .collect()
    };
    assert_eq!(tags(&fresh), tags(&committed));
    assert_eq!(verify_core::verify(&fresh).verdict, Verdict::Pass);
}

/// The CLI's exit codes: 0 PASS, 1 FAIL, 2 cannot verify.
#[test]
fn cli_exit_codes() {
    let bin = env!("CARGO_BIN_EXE_frontier-verify");
    let fx = fixtures_dir().join("land-program.json");
    let out = std::env::temp_dir().join(format!("verify-cli-{}", std::process::id()));
    let run = |args: &[&str]| {
        std::process::Command::new(bin)
            .env(verify_core::ALLOW_MUTATED_ENV, "1")
            .args(args)
            .arg("--out")
            .arg(&out)
            .output()
            .unwrap()
            .status
            .code()
    };
    assert_eq!(run(&["--fixture", fx.to_str().unwrap()]), Some(0));
    let rep: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("report.json")).unwrap()).unwrap();
    assert_eq!(rep["verdict"], "PASS");
    for k in [
        "season",
        "program",
        "slot_range",
        "entities",
        "bells",
        "counts",
        "findings",
        "liveness",
    ] {
        assert!(rep.get(k).is_some(), "report.json has {k}");
    }
    assert!(out.join("report.md").exists());
    assert_eq!(
        run(&[
            "--fixture",
            fx.to_str().unwrap(),
            "--ruleset",
            &"00".repeat(32)
        ]),
        Some(1)
    );
    assert_eq!(run(&["--fixture", "/nonexistent/fixture.json"]), Some(2));
    let _ = std::fs::remove_dir_all(&out);
}
