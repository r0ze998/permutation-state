//! `itest::inproc_day` (M1 contract §12 Gate W4): 100 bots + keeper +
//! herald fold over `ChainPort::InProcess` with the test-beacon program and
//! the test key, one game day (144 bells at 20×, 10,800 slots of virtual
//! time) and a 26-bell keeper-only drain (one whole SkipQuiet batch of the
//! keeper's and its close, `DayCfg::drain_bells`).
//!
//! Pass (strict mode, the gate): **zero stuck province-bells**; every
//! transit settled or routed by rule; every bad seal destroyed at
//! settlement with the code the stock `tlock` opener gives (and at least
//! one bad seal marched: the three bad-seal personas are in the roster);
//! no instruction answering `NotImplemented`; no persona outcome violated;
//! no keeper write dead or over budget; the herald folded every record and
//! every clash report matches its native recomputation; honest traffic
//! never tripped the relay's drain guard.
//!
//! `ITEST_STUBS=1` is the brief's first run before W4-A/W4-B/W4-C merge: the
//! resolution stand-in (`itest::standin`) moves provinces so resident
//! actions and marches happen, and every condition a missing unit decides
//! is `PENDING` (printed, never a pass). Environment: `ITEST_BOTS` (100),
//! `ITEST_BELLS` (144), `ITEST_SUMMARY=<file>` (the run's JSON summary),
//! `FRONTIER_RECORD_FIXTURES=1` (re-record `crates/agents/fixtures/
//! herald-recorded` from this run's herald), `PSF_FRONTIER_SO`,
//! `ITEST_NO_BUILD` (see `itest::program`).

use itest::checks::Cond;
use itest::day::{self, DayCfg, DayOut};

/// The largest share of resident actions (per action) the relay may refuse
/// `NotResident` in the gate day (a herald view a bell old, a nudge still
/// in flight).
const NOT_RESIDENT_MAX_PCT: u64 = 25;

fn env_num(k: &str, d: u64) -> u64 {
    std::env::var(k)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(d)
}

fn count(o: &DayOut, kind: &str) -> u64 {
    o.facts.records.get(kind).copied().unwrap_or(0)
}

/// The named conditions of the run.
fn conditions(o: &DayOut) -> Vec<(&'static str, Cond)> {
    let stubs = o.cfg.stubs;
    let f = &o.facts;
    let mut v = vec![];
    // Stubs of units not merged: failed transactions, relay refusals and
    // keeper alerts all count.
    let mut ni: Vec<String> = f.not_implemented.iter().cloned().collect();
    if !f.program_stubs.is_empty() {
        ni.push(format!("program stubs {:?}", f.program_stubs));
    }
    for (route, ix, res) in o.relay.by.keys() {
        if res == "NotImplemented" {
            ni.push(format!("{ix} (relay {route})"));
        }
    }
    if let Some(n) = o.keeper_alerts.get("not-implemented") {
        ni.push(format!(
            "keeper roles ({n} alerts: {:?})",
            o.keeper_unsupported
        ));
    }
    v.push((
        "no-stubs",
        if ni.is_empty() {
            Cond::Pass("no instruction answered NotImplemented".into())
        } else if stubs {
            Cond::Pending(format!("NotImplemented: {}", ni.join(", ")))
        } else {
            Cond::Fail(format!("NotImplemented: {}", ni.join(", ")))
        },
    ));
    let bots = o.cfg.bots as u64;
    let joined = count(o, "JOIN");
    let settled_sites = f.records.get("SETTLE").copied().unwrap_or(0);
    let departs = f.departs.len() as u64;
    let activity = format!(
        "JOIN {joined}/{bots}, SETTLE {settled_sites}, HARVEST {}, BUILD {}, TRAIN {}, MUSTER {}, EXPLORE {}, DEPART {departs}, REVEAL {}, CLASH {}, SKIP {}, TRANSIT_SETTLED {}",
        count(o, "HARVEST"),
        count(o, "BUILD"),
        count(o, "TRAIN"),
        count(o, "MUSTER"),
        count(o, "EXPLORE"),
        count(o, "REVEAL"),
        count(o, "CLASH"),
        count(o, "SKIP"),
        count(o, "TRANSIT_SETTLED"),
    );
    let want_joins = o
        .roster
        .iter()
        .filter(|s| s.join_bell < o.cfg.play_bells)
        .count() as u64;
    v.push((
        "activity",
        if joined * 100 >= want_joins * 95 && settled_sites * 2 >= joined && departs > 0 {
            Cond::Pass(activity)
        } else {
            Cond::Fail(format!(
                "{activity} (want ≥ 95% of {want_joins} joins, a site for half of them, marches)"
            ))
        },
    ));
    let resolution_real = count(o, "CLASH") + count(o, "SKIP") > 0;
    let stuck = f.stuck_province_bells();
    v.push((
        "zero-stuck-province-bells",
        if stubs {
            Cond::Pending(format!(
                "resolution by the stand-in ({} province moves, {stuck} province-bells behind): W4-A/W4-C",
                o.standin_moves
            ))
        } else if stuck == 0 && resolution_real {
            Cond::Pass(format!("{} provinces resolved through bell {}", f.provinces.len(), f.end_bell))
        } else {
            Cond::Fail(format!(
                "{stuck} province-bells behind bell {} over {} provinces (CLASH {}, SKIP {})",
                f.end_bell,
                f.provinces.len(),
                count(o, "CLASH"),
                count(o, "SKIP")
            ))
        },
    ));
    let unsettled = f.unsettled_departs();
    let stuck_t = f.stuck_transits();
    // One TRANSIT_SETTLED per DEPART, paired in feed order (integ-W4
    // review): no settlement without its departure.
    let settle_ok =
        unsettled.is_empty() && stuck_t.is_empty() && departs > 0 && f.orphan_settles.is_empty();
    v.push((
        "transits-settled-or-routed",
        if settle_ok {
            Cond::Pass(format!(
                "{departs} departs ({} hosts), {} settled one to one",
                f.departs
                    .iter()
                    .map(|d| d.host_id)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
                f.settled.len()
            ))
        } else if stubs {
            Cond::Pending(format!(
                "{departs} departs, {} settled, {} unsettled, {} stuck in state 1-3 (SettleTransit W4-B, keeper W4-C)",
                f.settled.len(),
                unsettled.len(),
                stuck_t.len()
            ))
        } else {
            Cond::Fail(format!(
                "{departs} departs, {} settled, unsettled {:?}, stuck {:?}, orphan settlements {:?}",
                f.settled.len(),
                &unsettled[..unsettled.len().min(8)],
                &stuck_t[..stuck_t.len().min(8)],
                f.orphan_settles
            ))
        },
    ));
    let dis = f.seal_disagreements();
    let surv = f.bad_seals_survived();
    // Only the bad seals old enough to have settled count (a march that
    // arrives after the drain is not due).
    let (bad, bad_settled) = f.bad_seals_due();
    v.push((
        "bad-seals-destroyed-with-stock-code",
        if bad > 0 && bad_settled == bad && dis.is_empty() && surv.is_empty() {
            Cond::Pass(format!(
                "{bad} bad seals due, all settled BAD_SEAL with the stock code ({} marched)",
                f.bad_seals()
            ))
        } else if stubs && dis.is_empty() && surv.is_empty() {
            Cond::Pending(format!(
                "{bad} bad seals marched (stock codes {:?}), {bad_settled} settled",
                f.departs
                    .iter()
                    .filter(|d| d.stock_code > 0)
                    .map(|d| d.stock_code)
                    .collect::<Vec<_>>()
            ))
        } else {
            Cond::Fail(format!(
                "{bad} bad seals, {bad_settled} settled, code disagreements {dis:?}, survived {surv:?}"
            ))
        },
    ));
    // min_tip never self-reveals: keepers reveal it (§8.6; E5 criterion 4).
    let min_tip: Vec<(u64, u32)> = o
        .persona_marches
        .iter()
        .filter(|(p, _, a)| *p == frontier_agents::Persona::MinTip && f.due(*a))
        .map(|&(_, h, a)| (h, a))
        .collect();
    let unrevealed: Vec<&(u64, u32)> = min_tip
        .iter()
        .filter(|(h, a)| !f.revealed.contains_key(&(*h, *a)))
        .collect();
    v.push((
        "min-tip-revealed-by-keepers",
        if !min_tip.is_empty() && unrevealed.is_empty() {
            Cond::Pass(format!("{} min_tip marches, all revealed", min_tip.len()))
        } else if stubs {
            Cond::Pending(format!(
                "{} min_tip marches due, {} unrevealed (keeper reveal pipeline W4-C)",
                min_tip.len(),
                unrevealed.len()
            ))
        } else {
            Cond::Fail(format!(
                "{} min_tip marches due, unrevealed {unrevealed:?}",
                min_tip.len()
            ))
        },
    ));
    let violated = o.violated();
    // Only the personas the bots can judge locally have a verdict; the
    // `needs-chain` ones are listed, not checked (integ-W4 review, W4-F:
    // the condition read as if every persona were judged).
    let unjudged: Vec<&str> = o
        .persona_verdicts()
        .iter()
        .filter(|(_, v)| **v == "needs-chain")
        .map(|(p, _)| *p)
        .collect();
    v.push((
        "locally-checkable-personas",
        if violated.is_empty() {
            Cond::Pass(format!(
                "{:?}; not judged here (need the chain): {unjudged:?}",
                o.persona_verdicts()
            ))
        } else {
            Cond::Fail(format!(
                "violated: {violated:?}; {:?}",
                o.persona_verdicts()
            ))
        },
    ));
    v.push((
        "keeper-writes",
        itest::checks::keeper_writes_cond(
            &o.keeper_alerts,
            &o.keeper_alert_samples,
            &o.keeper_writes,
        ),
    ));
    // Resident liveness (integ-W4 review, W4-F): resident actions the relay
    // refused `NotResident` against the ones it sent.
    let ratio = itest::checks::not_resident_ratio(&o.relay.by);
    let over: Vec<&(String, u64, u64)> = ratio
        .iter()
        .filter(|(_, refused, sent)| refused * 100 > (refused + sent) * NOT_RESIDENT_MAX_PCT)
        .collect();
    v.push((
        "resident-liveness",
        if over.is_empty() {
            Cond::Pass(format!(
                "NotResident / (sent + NotResident) ≤ {NOT_RESIDENT_MAX_PCT}% per action: {ratio:?}; nudges {:?}",
                o.bots.nudges
            ))
        } else if stubs {
            Cond::Pending(format!("{ratio:?}"))
        } else {
            Cond::Fail(format!(
                "over {NOT_RESIDENT_MAX_PCT}%: {over:?}; nudges {:?}",
                o.bots.nudges
            ))
        },
    ));
    let alarm = |k: &str| o.herald[k].as_u64().unwrap_or(u64::MAX);
    let alarms: u64 = [
        "badRecords",
        "rewrites",
        "clashMismatch",
        "clashUnchecked",
        "writeErrors",
    ]
    .iter()
    .map(|k| alarm(k))
    .sum();
    v.push((
        "herald",
        if alarms == 0 && o.herald_mismatches.is_empty() {
            Cond::Pass(format!(
                "{} events folded, no alarm, every clash report matches",
                o.herald["events"]
            ))
        } else {
            Cond::Fail(format!(
                "alarms {}, clash files not matching {:?}",
                o.herald, o.herald_mismatches
            ))
        },
    ));
    let drain: u64 = o
        .relay
        .by
        .iter()
        .filter(|((_, _, r), _)| r == "DrainGuard")
        .map(|(_, n)| n)
        .sum();
    v.push((
        "relay-drain-guard",
        if drain == 0 {
            Cond::Pass("no sponsored transaction moved more than its allowance".into())
        } else {
            Cond::Fail(format!(
                "{drain} sponsored transactions tripped the drain guard"
            ))
        },
    ));
    v
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "one game day in process (minutes of wall time); Gate W4 runs it with --include-ignored inproc_"]
async fn inproc_day() {
    let stubs = std::env::var("ITEST_STUBS").is_ok_and(|v| v == "1");
    let dir = std::env::temp_dir().join(format!("itest-inproc-day-{}", std::process::id()));
    let mut cfg = DayCfg::new(&dir);
    cfg.stubs = stubs;
    cfg.bots = env_num("ITEST_BOTS", 100) as usize;
    cfg.play_bells = env_num("ITEST_BELLS", 144) as u32;
    // The gate is 100 bots over one game day (integ-W4 review, W4-F: the
    // environment could shrink it silently).
    let small = cfg.bots < 100 || cfg.play_bells < 144;
    let allow_small = std::env::var("ITEST_ALLOW_SMALL").is_ok_and(|v| v == "1");
    assert!(
        !small || allow_small,
        "ITEST_BOTS ≥ 100 and ITEST_BELLS ≥ 144 for the gate (ITEST_ALLOW_SMALL=1 for a smaller, non-gate run)"
    );
    if std::env::var("FRONTIER_RECORD_FIXTURES").is_ok_and(|v| v == "1") {
        // The recorded herald must come from the strict day (integ-W4
        // review: the stub world's provinces never move).
        assert!(!stubs, "FRONTIER_RECORD_FIXTURES=1 needs the strict run");
        cfg.record = Some(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../agents/fixtures/herald-recorded"),
        );
    }
    if let Ok(p) = std::env::var("VERIFY_DUMP") {
        cfg.verify_dump = Some(p.into());
    }
    let out = day::run(cfg).await.expect("the run");
    let summary = out.summary();
    let text = serde_json::to_string_pretty(&summary).unwrap();
    println!("{text}");
    if let Ok(p) = std::env::var("ITEST_SUMMARY") {
        std::fs::write(&p, &text).expect("ITEST_SUMMARY");
    }
    let conds = conditions(&out);
    println!(
        "\ninproc_day ({} mode{}, {} bots, {} bells, {} slots, {:.0} s wall, program {}):",
        if stubs { "STUB" } else { "strict" },
        if small {
            ", SMALL (not a gate run)"
        } else {
            ""
        },
        out.cfg.bots,
        out.cfg.play_bells,
        out.slots,
        out.wall_secs,
        &out.so_sha256[..16]
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
    assert!(fails.is_empty(), "failed: {fails:?}");
    // A stub run is the brief's first run, never a gate pass (integ-W4
    // review, W4-F: ITEST_STUBS=1 left in the environment made every
    // missing-unit condition PENDING and the test exit 0).
    assert!(
        !stubs,
        "ITEST_STUBS=1: a stub run is not a gate run (conditions above)"
    );
}
