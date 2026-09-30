//! The named pass conditions of an in-process run (`inproc_day`, Gate W4;
//! `g14_two_game_days`, Gate W5 G14), read from [`DayOut`]: shared so the
//! two gates judge a run the same way.

use crate::checks::Cond;
use crate::day::DayOut;

pub use crate::checks::NOT_RESIDENT_MAX_PCT;

pub fn count(o: &DayOut, kind: &str) -> u64 {
    o.facts.records.get(kind).copied().unwrap_or(0)
}

/// The named conditions of the run.
pub fn day_conditions(o: &DayOut) -> Vec<(&'static str, Cond)> {
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
    // Liveness (E5 criterion 4, W5-C): no valid seal is lost. A valid seal
    // left unrevealed must have settled bounced-unranked (outranked or its
    // citizen's second arrival: no loss, by rule); one that settled routed
    // was a march the keepers failed to reveal (no hold in process).
    let routed: Vec<(u64, u32)> = f
        .departs
        .iter()
        .filter(|d| d.stock_code == 0)
        .filter(|d| {
            f.settlement(d).map(|(o, _)| o) == Some(frontier_abi::log::transit_outcome::ROUTED)
        })
        .map(|d| (d.host_id, d.arrive))
        .collect();
    let unrevealed_ok = f
        .departs
        .iter()
        .filter(|d| d.stock_code == 0 && !f.revealed.contains_key(&(d.host_id, d.arrive)))
        .filter(|d| f.settlement(d).is_some())
        .count();
    v.push((
        "no-valid-seal-routed",
        if routed.is_empty() {
            Cond::Pass(format!(
                "no valid seal settled routed ({unrevealed_ok} valid seals settled unrevealed, each by a no-loss rule)"
            ))
        } else {
            Cond::Fail(format!("valid seals routed: {routed:?}"))
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
        crate::checks::keeper_writes_cond(
            &o.keeper_alerts,
            &o.keeper_alert_samples,
            &o.keeper_writes,
        ),
    ));
    // Resident liveness (integ-W4 review, W4-F): resident actions the relay
    // refused `NotResident` against the ones it sent.
    v.push((
        "resident-liveness",
        crate::checks::resident_liveness_cond(&o.relay.by, &o.bots, stubs),
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
