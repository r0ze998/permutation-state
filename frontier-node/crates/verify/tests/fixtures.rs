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
    // W6-C (W5-C F4): the refused arrival settled bounced-unranked (no
    // loss by rule) is listed apart. W6T-3: V5 now judges §5.11 step 6 on
    // the post-states, and the synthetic generator's plaintexts carry a
    // one-step placeholder path (its model program never checked step 6),
    // so the low-tip rout is a march the real program could never have
    // revealed (`path`), not a liveness miss. The liveness warning of a
    // rout the rules allowed is `march_program_passes`' (the program's own
    // recording) and `unshielded_rout_stays_a_liveness_warning`'s.
    assert!(r.liveness.valid_unrevealed.is_empty(), "{:?}", r.liveness);
    assert_eq!(
        r.liveness
            .unrevealed_by_rule
            .iter()
            .map(|x| (x.outcome, x.reason))
            .collect::<Vec<_>>(),
        vec![(o::ROUTED, "path"), (o::BOUNCED_UNRANKED, "bounced")],
        "the low-tip rout (placeholder path) and the refused arrival, bounced by rule"
    );
    assert_eq!(
        r.findings
            .iter()
            .filter(|f| f.code == "ValidSealUnrevealed")
            .count(),
        0,
        "no warning"
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

// ------------------------------------------------ W6T-3: rule refusals

/// The w6-s7 slice of host 1909855992414208 (bot 987, `honest`): two valid
/// seals, arrivals 394 and 552, whose every Reveal the program refused
/// `Shielded` (§5.11 step 6: the host's own holding was inside its 48-h
/// founding shield and the destination was another faction's holding
/// site), both settled ROUTED. Cut by `examples/march_slice.rs` from
/// `w6-s7/verify/input.json.gz` with all of the host's failed Reveals (six,
/// one of another march of the host).
const SHIELDED_HOST: u64 = 1_909_855_992_414_208;

fn shielded() -> verify_core::Input {
    fixture("w6s7-shielded-march.json.gz")
}

/// The failed Reveal attempts the report gives the march `(host, arrive)`,
/// wherever it lists it (the warning's signatures, or the by-rule count).
fn attempts_of(j: &serde_json::Value, host: u64, arrive: u32) -> Option<usize> {
    let l = &j["liveness"];
    let hit = |x: &&serde_json::Value| {
        x["host_id"] == host.to_string() && x["arrive_bell"] == arrive
    };
    l["valid_unrevealed"]
        .as_array()?
        .iter()
        .find(hit)
        .and_then(|x| x["attempts"].as_array().map(|a| a.len()))
        .or_else(|| {
            l["unrevealed_by_rule"]
                .as_array()?
                .iter()
                .find(hit)
                .and_then(|x| x["failed_attempts"].as_u64().map(|n| n as usize))
        })
}

/// Failing first on 7dcacdf: the attempts were counted per host (6 for
/// each march of the host), not per march (2 and 3: keeper A once or
/// twice in one slot, keeper B about 75 slots later).
#[test]
fn failed_attempts_counted_per_march() {
    let j = verify_core::verify(&shielded()).json();
    assert_eq!(attempts_of(&j, SHIELDED_HOST, 394), Some(2), "{}", j["liveness"]);
    assert_eq!(attempts_of(&j, SHIELDED_HOST, 552), Some(3), "{}", j["liveness"]);
}

/// A valid seal refused `Shielded` by rule is not a liveness miss: it is
/// listed in `unrevealed_by_rule` with the reason judged from the
/// post-states, and no `ValidSealUnrevealed` is raised.
#[test]
fn shielded_refused_march_is_unrevealed_by_rule() {
    use frontier_abi::log::transit_outcome as o;
    let r = verify_core::verify(&shielded());
    assert!(r.liveness.valid_unrevealed.is_empty(), "{:?}", r.liveness);
    let got: Vec<_> = r
        .liveness
        .unrevealed_by_rule
        .iter()
        .map(|x| (x.host, x.arrive, x.outcome, x.reason, x.failed_attempts))
        .collect();
    assert_eq!(
        got,
        vec![
            (SHIELDED_HOST, 394, o::ROUTED, "shielded-own", 2),
            (SHIELDED_HOST, 552, o::ROUTED, "shielded-own", 3),
        ]
    );
    assert!(!r.findings.iter().any(|f| f.code == "ValidSealUnrevealed"));
    let j = r.json();
    assert_eq!(j["liveness"]["unrevealed_by_reason"]["shielded-own"], 2);
    assert_eq!(j["liveness"]["unrevealed_by_rule"][0]["reason"], "shielded-own");
}

/// The check of the check: with the host's own shield lapsed in the
/// archived Holding (and the destination unshielded), nothing refuses the
/// Reveals by rule, so both routs are liveness warnings again.
#[test]
fn unshielded_rout_stays_a_liveness_warning() {
    use frontier_abi::layout::player::holding as H;
    let mut inp = shielded();
    let w = verify_core::world::World::parse(&inp.cfg.program, &inp.txs, Default::default(), 0);
    let f = verify_core::facts::Facts::build(&w, &inp.cfg);
    let hk = f.ctx.holding_of_host(SHIELDED_HOST).expect("host id");
    let mut n = 0;
    for t in inp.txs.iter_mut() {
        for (k, a) in t.post.iter_mut() {
            if k.to_bytes() == hk {
                if let Some(a) = a.as_mut() {
                    a.data[H::SHIELD_UNTIL..H::SHIELD_UNTIL + 8].copy_from_slice(&0i64.to_le_bytes());
                    n += 1;
                }
            }
        }
    }
    assert!(n > 0, "the Holding's writers are in the slice");
    let r = verify_core::verify(&inp);
    assert!(r.liveness.unrevealed_by_rule.is_empty(), "{:?}", r.liveness);
    assert_eq!(
        r.findings
            .iter()
            .filter(|f| f.code == "ValidSealUnrevealed")
            .count(),
        2
    );
}

/// Sets the synthetic season's `end_bell` in CreateSeason's data only (the
/// announce hash then disagrees too; this test reads V5's code alone).
fn with_end_bell(inp: &verify_core::Input, end: u32) -> verify_core::Input {
    use frontier_abi::presets::SEASON_PARAMS_LEN;
    use frontier_abi::tags::Ix;
    let mut inp = inp.clone();
    let w = verify_core::world::World::parse(&inp.cfg.program, &inp.txs, Default::default(), 0);
    let f = verify_core::facts::Facts::build(&w, &inp.cfg);
    let mut p = f.params.expect("CreateSeason");
    p.end_bell = end;
    let raw = p.to_bytes();
    verify_core::tamper::edit_ix(&mut inp, f.create_tx.unwrap(), Ix::CreateSeason, |d| {
        d[1..1 + SEASON_PARAMS_LEN].copy_from_slice(&raw)
    });
    inp
}

/// W6T-3 (w6-s7 criterion 1): a landed DEPART whose arrival bell is at or
/// after `end_bell` is a FAIL (`ArrivalAfterEnd`, §5.11 Depart step 4
/// v1.12), one finding per such DEPART; an arrival at `end_bell − 1` is
/// not.
#[test]
fn arrival_after_end_is_fail() {
    let inp = march();
    let w = verify_core::world::World::parse(&inp.cfg.program, &inp.txs, Default::default(), 0);
    let arrivals: Vec<u32> = w.of(Kind::DEPART).map(|r| r.pu32("arrive_bell")).collect();
    let last = *arrivals.iter().max().expect("departures");
    let at_last = arrivals.iter().filter(|&&a| a == last).count();
    let r = verify_core::verify(&with_end_bell(&inp, last));
    assert_eq!(r.verdict, Verdict::Fail);
    let hits: Vec<_> = r
        .findings
        .iter()
        .filter(|f| f.code == "ArrivalAfterEnd")
        .collect();
    assert_eq!(hits.len(), at_last, "{}", show(&r));
    assert!(hits.iter().all(|f| f.fail() && f.entity.starts_with("host ")));
    let r = verify_core::verify(&with_end_bell(&inp, last + 1));
    assert!(!r.fails_with("ArrivalAfterEnd"), "{}", show(&r));
    // The recorded season (end_bell far away) has none.
    assert!(!verify_core::verify(&inp).fails_with("ArrivalAfterEnd"));
}
