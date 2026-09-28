//! `frontier-stack report`: the run report (`report.json` + `report.md`;
//! offchain design §11.4, M1 contract §13.4).
//!
//! Sources: the run's transactions (the verifier input `verify/input.json.gz`
//! when `verify` ran, which is the state the verdict is about; otherwise
//! the chain's `frontier_feed`), the final accounts in that input, the
//! supervisor's `events.jsonl`, the per-bell keeper and herald samples in
//! `metrics/`, the bots' report, and the verify, tamper and load results.
//!
//! What it reports: max / p50 / p99 CU per instruction kind against the
//! §5.5 budget (whole-transaction units: the three ComputeBudget
//! instructions add ≈ 450), tx bytes; the full **Reveal CU distribution**
//! (the C4 input, CL-26/30); keeper latencies **in slots** at the run's
//! scale (round → anchor, S → first cache, anchor → last valid reveal,
//! close → resolve; I-54: 0.4 × scale game seconds per slot); SkipQuiet
//! transactions per idle province-day; failures by (kind, error);
//! records by kind; transit outcomes and seal codes; the §13.4 criteria it
//! can decide from those (stuck province-bells, unsettled transits,
//! cohorts, effective N, personas, bad seals, herald), chaos kills,
//! restarts and crashes, adversary holds, and the verify, tamper and load
//! verdicts.

use std::collections::{BTreeMap, HashMap};

use fclient::decode::Province;
use fclient::log::Record;
use fclient::ports::TxRecord;
use fclient::Address;
use frontier_abi::log::Kind;
use frontier_abi::tags::Ix;
use serde_json::{json, Value};

use crate::load::quantile;
use crate::run::RunDir;

fn le_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().expect("4"))
}
fn le_i32(b: &[u8], o: usize) -> i32 {
    i32::from_le_bytes(b[o..o + 4].try_into().expect("4"))
}
fn le_u64(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().expect("8"))
}
fn le_i64(b: &[u8], o: usize) -> i64 {
    i64::from_le_bytes(b[o..o + 8].try_into().expect("8"))
}

/// Offset of a field in a kind's `key ‖ payload`.
pub fn field(kind: Kind, name: &str) -> Option<usize> {
    let s = kind.spec();
    let mut o = 0;
    for (n, w) in s.key.iter().chain(s.payload.iter()) {
        if *n == name {
            return Some(o);
        }
        o += w;
    }
    None
}

fn lens(k: u8) -> Option<usize> {
    Kind::from_u8(k).map(|x| x.spec().key_len() + x.spec().payload_len())
}

/// The program instruction tag of a transaction (its first instruction
/// addressed to `program`).
pub fn tag_of(t: &TxRecord, program: &Address) -> Option<u8> {
    let tx = fclient::tx::from_wire(&t.tx).ok()?;
    let m = &tx.message;
    m.instructions
        .iter()
        .find(|ci| m.account_keys.get(ci.program_id_index as usize) == Some(program))
        .and_then(|ci| ci.data.first().copied())
}

/// A decoded record with the landing time and slot of its transaction.
#[derive(Clone)]
pub struct Rec {
    pub r: Record,
    pub slot: u64,
    pub time: i64,
}

pub fn records(txs: &[TxRecord], program: &Address) -> Vec<Rec> {
    let mut out = vec![];
    for t in txs.iter().filter(|t| t.err.is_none()) {
        for b in fclient::log::bodies_from_logs(&t.logs, program).unwrap_or_default() {
            if let Ok(r) = Record::decode_with(&b, &lens) {
                out.push(Rec {
                    r,
                    slot: t.slot,
                    time: t.block_time,
                });
            }
        }
    }
    out
}

fn stats(v: &[f64]) -> Value {
    json!({"n": v.len(), "p50": quantile(v, 0.5), "p90": quantile(v, 0.9), "p99": quantile(v, 0.99),
           "max": v.iter().cloned().fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.max(x))))})
}

/// Per instruction kind: count, failures, CU (successful), bytes, budget.
pub fn cu_table(txs: &[TxRecord], program: &Address) -> (Value, Vec<f64>) {
    let mut by: BTreeMap<u8, (Vec<f64>, u64, usize)> = BTreeMap::new();
    let mut reveal = vec![];
    for t in txs {
        let Some(tag) = tag_of(t, program) else {
            continue;
        };
        let e = by.entry(tag).or_default();
        if t.err.is_some() {
            e.1 += 1;
        } else {
            e.0.push(t.units as f64);
            if tag == Ix::Reveal.tag() {
                reveal.push(t.units as f64);
            }
        }
        e.2 = e.2.max(t.tx.len());
    }
    let rows: Vec<Value> = by
        .iter()
        .map(|(tag, (u, failed, bytes))| {
            let ix = Ix::from_tag(*tag);
            let b = ix.map(frontier_abi::budgets::budget);
            let max = u.iter().cloned().fold(0.0, f64::max);
            let budget = b.map(|b| b.cu_budget).unwrap_or(0);
            let per_unit = b.map(|b| b.cu_per_unit).unwrap_or(0);
            // A per-unit budget (SkipQuiet) is gated per transaction by its
            // unit count; the table flags only a max above the 24-unit bound.
            let ceiling = budget as f64 + per_unit as f64 * if per_unit > 0 { 24.0 } else { 0.0 };
            json!({"tag": tag, "kind": ix.map(|i| i.name()).unwrap_or("?"), "landed": u.len(), "failed": failed,
                "cu": stats(u), "budget": budget, "budget_per_unit": per_unit,
                "over_budget": budget > 0 && max > ceiling, "max_tx_bytes": bytes,
                "tx_ceiling": ix.map(frontier_abi::budgets::tx_ceiling)})
        })
        .collect();
    (json!(rows), reveal)
}

/// Failed transactions by (kind, error).
pub fn failures(txs: &[TxRecord], program: &Address) -> Value {
    let mut m: BTreeMap<String, u64> = BTreeMap::new();
    for t in txs.iter().filter(|t| t.err.is_some()) {
        let kind = tag_of(t, program)
            .and_then(Ix::from_tag)
            .map(|i| i.name())
            .unwrap_or("?");
        let err = t
            .code
            .map(|c| fclient::abi::error_name(c).map_or(format!("code {c}"), String::from))
            .unwrap_or_else(|| t.err.clone().unwrap_or_default());
        *m.entry(format!("{kind}: {err}")).or_default() += 1;
    }
    json!(m)
}

/// The chain's slot → Clock map, from the landing slots and Clock times of
/// the run's transactions (landed and failed). Between two observed slots
/// the Clock moves `slot_secs` per slot (I-54).
pub struct SlotClock {
    pts: Vec<(u64, i64)>,
    slot_secs: f64,
}

impl SlotClock {
    pub fn new<'a>(txs: impl IntoIterator<Item = &'a TxRecord>, slot_secs: f64) -> SlotClock {
        let mut pts: Vec<(u64, i64)> = txs.into_iter().map(|t| (t.slot, t.block_time)).collect();
        pts.sort_unstable();
        pts.dedup_by_key(|p| p.0);
        SlotClock { pts, slot_secs }
    }

    /// The first slot whose Clock is at or after `t`: between the last
    /// observed slot before `t` and the first observed slot at or after it,
    /// counted back from the latter at `slot_secs` per slot. `None` when no
    /// observed slot reaches `t`.
    pub fn first_slot_at(&self, t: f64) -> Option<u64> {
        let i = self.pts.partition_point(|p| (p.1 as f64) < t);
        let &(sb, tb) = self.pts.get(i)?;
        let floor = if i > 0 { self.pts[i - 1].0 + 1 } else { 0 };
        let back = ((tb as f64 - t) / self.slot_secs).floor().max(0.0) as u64;
        Some(sb.saturating_sub(back).max(floor).min(sb))
    }
}

/// Keeper latencies (§13.4 criterion 3) from the records.
///
/// **Reference points (W5-B F5, pinned by W6-A):** a round is *public* at
/// `round_time(r) + drand delay` (the replay's publication, which the
/// program's drand gate also waits for). Round → anchor and S → first
/// cache are reported two ways:
/// - `*_slots`: **whole slots from the first slot whose Clock shows the
///   round public to the landing slot** (`landing − first_slot_at(public)`).
///   A keeper that sees the round in that slot and lands in the next one
///   scores 1. This is the figure criterion 3's slot targets (p99 ≤ 2) are
///   judged on at 20×: it charges the keeper, not the Clock's 8-game-second
///   step (from the publication instant a landing is ≥ 1 slot plus the
///   fraction of a slot until the Clock reaches the round, F5).
/// - `*_game_secs`: game seconds from the publication instant to the
///   landing slot's Clock; the scale-2 run's game-second targets (≤ 5 s)
///   are judged on these. `*_from_publication_slots` is the same in slots.
///
/// Anchor → last valid reveal and close → resolve run from chain times
/// (THE anchor's `A`, `close = A + W`), so their slots are exact.
pub fn latencies(
    recs: &[Rec],
    clock: &SlotClock,
    drand: &fclient::clock::Drand,
    delay_s: f64,
    reveal_window: u32,
) -> Value {
    let slot_secs = clock.slot_secs;
    let to_slots = |secs: f64| (secs / slot_secs).max(0.0);
    let mut region_of: HashMap<(i32, i32), u8> = HashMap::new();
    let mut anchor_a: HashMap<(u32, u8), i64> = HashMap::new();
    let (mut ra_slots, mut ra_secs, mut ra_pub) = (vec![], vec![], vec![]);
    let (mut reveal_last, mut close_resolve) = (vec![], vec![]);
    // (bell, region) → the first cache's (slots, game secs).
    let mut first_cache: HashMap<(u32, u8), (u64, f64)> = HashMap::new();
    let mut last_reveal: HashMap<(i32, i32, u32), i64> = HashMap::new();
    let mut unmapped = 0usize;
    let mut public_lag = |round: u64, x: &Rec| -> Option<(f64, f64, f64)> {
        let public = drand.round_time(round) as f64 + delay_s;
        let secs = (x.time as f64 - public).max(0.0);
        match clock.first_slot_at(public) {
            Some(s0) => Some((x.slot.saturating_sub(s0) as f64, secs, to_slots(secs))),
            None => {
                unmapped += 1;
                None
            }
        }
    };
    for x in recs {
        let kp = &x.r.key_payload;
        match Kind::from_u8(x.r.kind) {
            Some(Kind::PROVINCE_OPEN) => {
                let (p, q) = (le_i32(kp, 0), le_i32(kp, 4));
                if let Some(o) = field(Kind::PROVINCE_OPEN, "region") {
                    region_of.insert((p, q), kp[o]);
                }
            }
            Some(Kind::ANCHOR) => {
                let (bell, region) = (le_u32(kp, 0), kp[4]);
                let round = le_u64(kp, field(Kind::ANCHOR, "round").unwrap_or(5));
                let a = le_i64(kp, field(Kind::ANCHOR, "a").unwrap_or(13));
                anchor_a.entry((bell, region)).or_insert(a);
                if let Some((s, secs, ps)) = public_lag(round, x) {
                    ra_slots.push(s);
                    ra_secs.push(secs);
                    ra_pub.push(ps);
                }
            }
            Some(Kind::SEED) => {
                let (bell, region) = (le_u32(kp, 0), kp[4]);
                let round = le_u64(kp, field(Kind::SEED, "round").unwrap_or(6));
                if let Some((s, secs, _)) = public_lag(round, x) {
                    let e = first_cache
                        .entry((bell, region))
                        .or_insert((s as u64, secs));
                    if secs < e.1 {
                        *e = (s as u64, secs);
                    }
                }
            }
            Some(Kind::REVEAL) => {
                let (p, q, arrive) = (le_i32(kp, 0), le_i32(kp, 4), le_u32(kp, 8));
                let e = last_reveal.entry((p, q, arrive)).or_insert(x.time);
                *e = (*e).max(x.time);
            }
            Some(Kind::CLASH) => {
                let (p, q, bell) = (le_i32(kp, 0), le_i32(kp, 4), le_u32(kp, 8));
                if let Some(a) = region_of
                    .get(&(p, q))
                    .and_then(|r| anchor_a.get(&(bell, *r)))
                {
                    let close = a + reveal_window as i64;
                    close_resolve.push(to_slots((x.time - close) as f64));
                }
            }
            _ => {}
        }
    }
    let sc_slots: Vec<f64> = first_cache.values().map(|v| v.0 as f64).collect();
    let sc_secs: Vec<f64> = first_cache.values().map(|v| v.1).collect();
    for ((p, q, arrive), t) in &last_reveal {
        if let Some(a) = region_of
            .get(&(*p, *q))
            .and_then(|r| anchor_a.get(&(*arrive, *r)))
        {
            reveal_last.push(to_slots((t - a) as f64));
        }
    }
    let secs = |v: &[f64]| -> Vec<f64> { v.iter().map(|x| x * slot_secs).collect() };
    json!({
        "slot_game_secs": slot_secs,
        "round_to_anchor_slots": stats(&ra_slots),
        "round_to_anchor_game_secs": stats(&ra_secs),
        "round_to_anchor_from_publication_slots": stats(&ra_pub),
        "s_to_first_cache_slots": stats(&sc_slots),
        "s_to_first_cache_game_secs": stats(&sc_secs),
        "anchor_to_last_reveal_slots": stats(&reveal_last),
        "anchor_to_last_reveal_game_secs": stats(&secs(&reveal_last)),
        "close_to_resolve_slots": stats(&close_resolve),
        "close_to_resolve_game_secs": stats(&secs(&close_resolve)),
        "rounds_without_a_mapped_slot": unmapped,
        "targets_slots_p99": {"round_to_anchor": 2, "s_to_first_cache": 2, "anchor_to_last_reveal": 4, "close_to_resolve": 8, "skips_per_idle_day": 6},
        "targets_game_secs_p99": {"round_to_anchor": 5, "s_to_first_cache": 5, "anchor_to_last_reveal": 30, "close_to_resolve": 60},
        "definition": "round -> anchor and S -> first cache in *_slots: landing slot minus the first slot whose Clock is at or after round_time + drand delay (publication), judged at 20x; *_game_secs: game seconds from publication to the landing slot's Clock, judged at 2x. Anchor -> last valid reveal from THE anchor's A; close -> resolve from A + W (W5-B F5, pinned by W6-A)",
    })
}

/// Catch-up cost (§13.4 criterion 3, I-50): SkipQuiet transactions (SKIP
/// records) per province-day, split into **idle** province-days (the
/// Province's `roster_epoch` did not move) and **churned** ones.
///
/// The epoch is read from the transactions' post-states (localnet feeds
/// carry them): a change written by a SkipQuiet counts for the day of the
/// first bell it commits (`b0`), any other for the day of its landing bell.
/// A province-day without post-states for its Province is not judged
/// (`unknown`).
pub fn catch_up(txs: &[TxRecord], program: &Address, genesis_ts: i64, bell_secs: i64) -> Value {
    use std::collections::BTreeSet;
    let mut epoch: HashMap<(i32, i32), u32> = HashMap::new();
    let mut churned: BTreeSet<(i32, i32, u32)> = BTreeSet::new();
    let mut skips: BTreeMap<(i32, i32, u32), u64> = BTreeMap::new();
    let mut with_post = 0usize;
    let mut order: Vec<&TxRecord> = txs.iter().filter(|t| t.err.is_none()).collect();
    order.sort_by_key(|t| t.seq);
    for t in order {
        let mut skip_day: HashMap<(i32, i32), u32> = HashMap::new();
        for b in fclient::log::bodies_from_logs(&t.logs, program).unwrap_or_default() {
            let Ok(r) = Record::decode_with(&b, &lens) else {
                continue;
            };
            if r.kind == Kind::SKIP as u8 {
                let kp = &r.key_payload;
                let (p, q, b0) = (le_i32(kp, 0), le_i32(kp, 4), le_u32(kp, 8));
                *skips.entry((p, q, b0 / 144)).or_default() += 1;
                skip_day.insert((p, q), b0 / 144);
            }
        }
        let landing_day = ((t.block_time - genesis_ts).max(0) / bell_secs.max(1) / 144) as u32;
        for (_, a) in &t.post {
            let Some(a) = a else { continue };
            if a.data.len() != fclient::abi::size::PROVINCE {
                continue;
            }
            let Ok(pv) = Province::decode(&a.data) else {
                continue;
            };
            with_post += 1;
            let k = (pv.p as i32, pv.q as i32);
            let prev = epoch.insert(k, pv.roster_epoch);
            if prev.is_some_and(|e| e != pv.roster_epoch) {
                let d = skip_day.get(&k).copied().unwrap_or(landing_day);
                churned.insert((k.0, k.1, d));
            }
        }
    }
    let seen: BTreeSet<(i32, i32)> = epoch.keys().copied().collect();
    let (mut idle, mut churn, mut unknown) = (vec![], vec![], 0usize);
    let mut idle_over: Vec<String> = vec![];
    let mut churn_over: Vec<String> = vec![];
    for ((p, q, d), n) in &skips {
        if !seen.contains(&(*p, *q)) {
            unknown += 1;
        } else if churned.contains(&(*p, *q, *d)) {
            churn.push(*n as f64);
            if *n > 6 {
                churn_over.push(format!("({p},{q}) day {d}: {n}"));
            }
        } else {
            idle.push(*n as f64);
            if *n > 6 {
                idle_over.push(format!("({p},{q}) day {d}: {n}"));
            }
        }
    }
    let all: Vec<f64> = skips.values().map(|n| *n as f64).collect();
    json!({
        "skip_txs_per_province_day": stats(&all),
        "skip_txs_per_idle_province_day": stats(&idle),
        "skip_txs_per_churned_province_day": stats(&churn),
        "idle_province_days_over_6": idle_over,
        "churned_province_days_over_6": churn_over,
        "province_days_not_judged": unknown,
        "province_post_states": with_post,
        "definition": "idle = the Province's roster_epoch unchanged over the day (post-states); a SkipQuiet's change counts for its b0's day",
    })
}

/// ClashInputs closability (§13.4 criterion 1: "every ClashInputs closable
/// after grace"), from the final accounts. Each open ClashInputs is
/// `closable` (resolved with every present record settled, or no-arrival
/// inputs whose bell the Province skipped past: CloseClashInputs lands once
/// `clash_close_grace` has run), `pending` (not yet resolved and the
/// Province still owes the bell; within `close + 2` bells of the run's last
/// bell), or **blocked** (resolved with a present record never settled; a
/// bell the Province passed without resolving or skipping it as
/// no-arrival; or pending past `close + 2` bells).
pub fn clash_inputs_check(
    inp: &verify_core::input::Input,
    ps: &[Province],
    recs: &[Rec],
    last_bell: u32,
) -> Value {
    let rn: HashMap<(i16, i16), u32> = ps.iter().map(|p| ((p.p, p.q), p.resolved_next)).collect();
    let (mut closable, mut pending) = (0usize, 0usize);
    let mut blocked: Vec<String> = vec![];
    let mut open = 0usize;
    for a in inp.finals.values().flatten() {
        if a.data.len() != fclient::abi::size::CLASH_INPUTS {
            continue;
        }
        let Ok(ci) = fclient::decode::ClashInputs::decode(&a.data) else {
            continue;
        };
        open += 1;
        let passed = rn.get(&(ci.p, ci.q)).is_some_and(|r| *r > ci.bell);
        let tag = format!("({},{}) bell {}", ci.p, ci.q, ci.bell);
        if ci.resolved() {
            let unsettled: Vec<usize> = (0..24)
                .filter(|&k| ci.arrivals[k].present != 0 && ci.settled_mask & (1 << k) == 0)
                .collect();
            if unsettled.is_empty() {
                closable += 1;
            } else if ci.bell + 3 <= last_bell {
                blocked.push(format!(
                    "{tag}: present records {unsettled:?} never settled"
                ));
            } else {
                pending += 1;
            }
        } else if ci.flags & 1 != 0 && passed {
            closable += 1;
        } else if passed {
            blocked.push(format!(
                "{tag}: the Province passed the bell without resolving these inputs"
            ));
        } else if ci.bell + 3 <= last_bell {
            blocked.push(format!(
                "{tag}: not resolved {} bells after its bell",
                last_bell - ci.bell
            ));
        } else {
            pending += 1;
        }
    }
    let closed = recs
        .iter()
        .filter(|x| {
            x.r.kind == Kind::CLOSE as u8
                && x.r.key_payload.first()
                    == Some(&(frontier_abi::layout::AccountKind::ClashInputs as u8))
        })
        .count();
    json!({"closed": closed, "open": open, "closable_after_grace": closable, "pending": pending, "blocked": blocked})
}

/// Records by kind, transit outcomes and seal codes, and the unsettled
/// transits due by `last_bell`.
pub fn outcomes(recs: &[Rec], last_bell: u32) -> Value {
    let mut kinds: BTreeMap<&str, u64> = BTreeMap::new();
    let mut transit: BTreeMap<String, u64> = BTreeMap::new();
    // Per host, its departs in order with how often each was settled
    // (wave-5 review: settlement counted per (host, depart), exactly once;
    // a host that departs again after a Stays is two transits).
    let mut departs: BTreeMap<u64, Vec<(u32, u32)>> = BTreeMap::new();
    let mut n_departs = 0usize;
    let mut orphan_settles: Vec<String> = vec![];
    let mut bad_seal_codes: BTreeMap<u8, u64> = BTreeMap::new();
    for x in recs {
        let k = Kind::from_u8(x.r.kind);
        *kinds.entry(k.map_or("?", |k| k.name())).or_default() += 1;
        let kp = &x.r.key_payload;
        match k {
            Some(Kind::DEPART) => {
                let host = le_u64(kp, 0);
                let arrive = le_u32(kp, field(Kind::DEPART, "arrive_bell").unwrap_or(21));
                departs.entry(host).or_default().push((arrive, 0));
                n_departs += 1;
            }
            Some(Kind::TRANSIT_SETTLED) => {
                let host = le_u64(kp, 0);
                let (o, c) = (kp[8], kp[9]);
                // The earliest depart of the host not yet settled; else the
                // last one again (a second settlement of one transit).
                match departs.get_mut(&host) {
                    Some(v) => match v.iter_mut().find(|d| d.1 == 0) {
                        Some(d) => d.1 += 1,
                        None => {
                            if let Some(d) = v.last_mut() {
                                d.1 += 1;
                            }
                        }
                    },
                    None => orphan_settles.push(format!("{host:#x}")),
                }
                *transit.entry(format!("outcome {o} seal {c}")).or_default() += 1;
                if o == frontier_abi::log::transit_outcome::BAD_SEAL {
                    *bad_seal_codes.entry(c).or_default() += 1;
                }
            }
            _ => {}
        }
    }
    let mut due = 0usize;
    let mut unsettled: Vec<String> = vec![];
    let mut settled_twice: Vec<String> = vec![];
    for (h, v) in &departs {
        for (a, n) in v {
            if *n > 1 {
                settled_twice.push(format!("{h:#x}@{a} x{n}"));
            }
            if a + 3 <= last_bell {
                due += 1;
                if *n == 0 {
                    unsettled.push(format!("{h:#x}@{a}"));
                }
            }
        }
    }
    json!({"records": kinds, "transits": transit, "bad_seal_codes": bad_seal_codes,
           "departs": n_departs, "departs_due": due, "unsettled_due": unsettled,
           "settled_more_than_once": settled_twice, "settles_without_depart": orphan_settles})
}

/// Final Provinces in the verifier input.
pub fn final_provinces(inp: &verify_core::input::Input) -> Vec<Province> {
    inp.finals
        .values()
        .flatten()
        .filter(|a| a.data.len() == fclient::abi::size::PROVINCE)
        .filter_map(|a| Province::decode(&a.data).ok())
        .collect()
}

/// Criterion 1's province part and criterion 9 from the final Provinces.
pub fn province_checks(ps: &[Province], play_bells: u32, last_bell: u32) -> Value {
    let stuck: Vec<String> = ps
        .iter()
        .filter(|p| p.resolved_next < play_bells)
        .map(|p| format!("({},{}) resolved_next {}", p.p, p.q, p.resolved_next))
        .collect();
    let old_cohorts: Vec<String> = ps
        .iter()
        .flat_map(|p| {
            p.cohorts
                .iter()
                .filter(|c| c.filed > c.settled && c.bell + 24 < last_bell)
                .map(move |c| {
                    format!(
                        "({},{}) bell {} filed {} settled {}",
                        p.p, p.q, c.bell, c.filed, c.settled
                    )
                })
        })
        .collect();
    json!({"provinces": ps.len(), "stuck_province_bells": stuck, "cohorts_open_past_24_bells": old_cohorts})
}

fn read_jsonl(p: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(p)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

fn read_json(p: &std::path::Path) -> Value {
    std::fs::read_to_string(p)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null)
}

/// Keeper samples: min reveal effective N during play (after the first two
/// bells, when payer care has filled the pool), last status summary.
pub fn keeper_summary(samples: &[Value], play_from: i64, play_to: i64) -> Value {
    // Every bell of play, the first two included (wave-5 review: I-49 says
    // every bell; those below 150 are listed).
    let during: Vec<&Value> = samples
        .iter()
        .filter(|s| {
            s["bell"]
                .as_i64()
                .is_some_and(|b| b >= play_from && b < play_to)
        })
        .collect();
    let min_n = during
        .iter()
        .filter_map(|s| s["status"]["pools"]["reveal"]["effective_n"].as_u64())
        .min();
    let low_bells: Vec<i64> = during
        .iter()
        .filter(|s| {
            s["status"]["pools"]["reveal"]["effective_n"]
                .as_u64()
                .is_some_and(|n| n < 150)
        })
        .filter_map(|s| s["bell"].as_i64())
        .collect();
    let last = samples.last().map(|s| &s["status"]);
    json!({"samples": samples.len(), "min_reveal_effective_n_in_play": min_n,
        "effective_n_ok": min_n.is_some_and(|n| n >= 150), "bells_below_150": low_bells,
        "last": last.map(|l| json!({"bell": l["bell"], "alerts": l["alerts"], "spend_by_day": l["spend_by_day"],
            "pools": l["pools"], "anchor_latency_slots_p99": l["duties"]["anchor_latency_slots_p99"],
            "seed_latency_slots_p99": l["duties"]["seed_latency_slots_p99"], "provinces_opened": l["duties"]["provinces_opened"],
            "archived_bells": l["duties"]["archived_bells"]}))})
}

pub fn herald_summary(samples: &[Value]) -> Value {
    let lags: Vec<f64> = samples
        .iter()
        .filter_map(|s| s["lag_slots"].as_f64())
        .collect();
    let last = samples
        .last()
        .map(|s| s["status"].clone())
        .unwrap_or(Value::Null);
    let alarms = &last["alarms"];
    let alarm_total: u64 = alarms
        .as_object()
        .map(|m| m.values().filter_map(|v| v.as_u64()).sum())
        .unwrap_or(0);
    json!({"samples": samples.len(), "fold_lag_slots": stats(&lags), "last": last, "alarms_total": alarm_total})
}

pub fn events_summary(ev: &[Value]) -> Value {
    let count = |k: &str| ev.iter().filter(|e| e["event"] == k).count();
    let holds: Vec<Value> = ev
        .iter()
        .filter(|e| e["event"] == "hold" || e["event"] == "hold-skipped")
        .map(|e| json!({"game": e["game"], "event": e["event"], "detail": e["detail"]}))
        .collect();
    let crashes: Vec<Value> = ev
        .iter()
        .filter(|e| e["event"] == "crash")
        .map(|e| e["detail"].clone())
        .collect();
    json!({"chaos_kills": count("chaos-kill"), "restarts": count("restart"), "crashes": crashes,
           "holds": holds, "end_season": ev.iter().find(|e| e["event"] == "end-season").map(|e| e["detail"].clone())})
}

pub fn personas(bots: &Value) -> Value {
    let list = bots["personas"].as_array().cloned().unwrap_or_default();
    let violated: Vec<Value> = list
        .iter()
        .filter(|p| p["verdict"] == "violated")
        .map(|p| p["persona"].clone())
        .collect();
    json!({"personas": list, "violated": violated})
}

/// Each hold's effect (wave-5 review of W5-B): from the hold's result
/// (its keys and slot window) and the run's transactions, how many landed
/// transactions wrote a held key inside the window (keepers or players
/// outbidding it), how many wrote one in the bell after it (the work it
/// delayed), and the first such landing after the window.
pub fn hold_effects(txs: &[TxRecord], events: &[Value]) -> Vec<Value> {
    let mut out = vec![];
    for e in events.iter().filter(|e| e["event"] == "hold") {
        let d = &e["detail"];
        let r = &d["result"];
        let (Some(from), Some(until)) = (r["fromSlot"].as_u64(), r["untilSlot"].as_u64()) else {
            out.push(
                json!({"kind": d["kind"], "effect": "no hold result (the hold did not start)"}),
            );
            continue;
        };
        let keys: BTreeMap<String, ()> = r["keys"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|k| k.as_str().map(|k| (k.to_string(), ())))
                    .collect()
            })
            .unwrap_or_default();
        let span = until.saturating_sub(from) + 1;
        let (mut inside, mut after, mut first_after) = (0u64, 0u64, None::<u64>);
        for t in txs.iter().filter(|t| t.err.is_none()) {
            if t.slot < from || t.slot > until + span {
                continue;
            }
            let Ok(tx) = fclient::tx::from_wire(&t.tx) else {
                continue;
            };
            let m = &tx.message;
            let hit = (0..m.account_keys.len()).any(|i| {
                fclient::tx::is_writable_index(m, i)
                    && keys.contains_key(&m.account_keys[i].to_string())
            });
            if !hit {
                continue;
            }
            if t.slot <= until {
                inside += 1;
            } else {
                after += 1;
                first_after = Some(first_after.map_or(t.slot, |x| x.min(t.slot)));
            }
        }
        out.push(json!({"kind": d["kind"], "priority_milli": d["priority_milli"], "above_keeper_cap": d["above_keeper_cap"],
            "from_slot": from, "until_slot": until, "keys": keys.len(),
            "writes_inside": inside, "writes_in_the_window_after": after, "first_write_after": first_after,
            "detail": d["detail"]}));
    }
    out
}

/// One §13.4 criterion decided on this run.
fn verdict(id: &str, status: &str, why: impl Into<String>) -> Value {
    json!({"criterion": id, "status": status, "why": why.into()})
}

/// The §13.4 criteria this run can decide (wave-5 review of W5-B): each
/// `pass`, `fail` or `n.a.` with the reason. `facts` carries the pieces
/// [`report`] gathers (see there); a missing piece makes its criterion
/// `n.a.`, never a pass.
pub fn decide(facts: &Value) -> Vec<Value> {
    let mut v = vec![];
    let len = |x: &Value| x.as_array().map_or(0, |a| a.len());
    // 1 — the season completes; transits settle exactly once; no stuck bells.
    let play_bells = facts["play_bells"].as_u64().unwrap_or(0);
    let end_bell = facts["end_bell"].as_u64().unwrap_or(u64::MAX);
    let mut bad = vec![];
    if play_bells >= end_bell && facts["end_season"]["ok"] != true {
        bad.push("EndSeason did not land".to_string());
    }
    for (k, what) in [
        ("stuck_province_bells", "province-bells stuck"),
        ("unsettled_due", "transits due and unsettled"),
        ("settled_more_than_once", "transits settled more than once"),
        ("settles_without_depart", "settlements without a DEPART"),
    ] {
        let n = len(&facts[k]);
        if n > 0 {
            bad.push(format!("{n} {what}"));
        }
    }
    // ClashInputs closability (W6-A; was "judged by the exit run").
    let ci = &facts["clash_inputs"];
    let ci_note = if ci.is_null() {
        "ClashInputs closability not read".to_string()
    } else {
        let blocked = len(&ci["blocked"]);
        if blocked > 0 {
            bad.push(format!(
                "{blocked} ClashInputs not closable after grace: {}",
                ci["blocked"]
            ));
        }
        format!(
            "ClashInputs: {} closed, {} closable after grace, {} pending, {blocked} blocked",
            f(&ci["closed"]),
            f(&ci["closable_after_grace"]),
            f(&ci["pending"])
        )
    };
    v.push(if !bad.is_empty() {
        verdict("1", "fail", format!("{}; {ci_note}", bad.join("; ")))
    } else if play_bells < end_bell {
        verdict("1", "pass", format!("the run stops before end_bell (EndSeason not expected): no stuck province-bell, every due transit settled exactly once; {ci_note}"))
    } else {
        verdict("1", "pass", format!("EndSeason landed; no stuck province-bell; every due transit settled exactly once; {ci_note}"))
    });
    // 2 — CU within the budgets.
    let over = len(&facts["over_budget"]);
    v.push(if facts["over_budget"].is_null() {
        verdict("2", "n.a.", "no CU table")
    } else if over > 0 {
        verdict(
            "2",
            "fail",
            format!(
                "{over} kinds above their §5.5 budget: {}",
                facts["over_budget"]
            ),
        )
    } else {
        verdict(
            "2",
            "pass",
            "every kind within its §5.5 budget (Reveal distribution reported)",
        )
    });
    // 3 — keeper latencies: slots at 20x, game seconds at 2x (W5-B F5:
    // the reference points are pinned in [`latencies`]); catch-up per idle
    // province-day at 20x (churned days reported).
    let scale = facts["scale"].as_f64().unwrap_or(0.0);
    let lat = &facts["latency"];
    let cu = &facts["catch_up"];
    let p99 = |k: &str| lat[k]["p99"].as_f64();
    let slots_mode = (scale - 20.0).abs() < 1e-9;
    let secs_mode = (scale - 2.0).abs() < 1e-9;
    let skip_note = if cu.is_null() {
        "catch-up not read".to_string()
    } else {
        format!(
            "SkipQuiet per idle province-day p99 {} max {} ({} idle days over 6), per churned day p99 {} max {} (reported)",
            f(&cu["skip_txs_per_idle_province_day"]["p99"]),
            f(&cu["skip_txs_per_idle_province_day"]["max"]),
            len(&cu["idle_province_days_over_6"]),
            f(&cu["skip_txs_per_churned_province_day"]["p99"]),
            f(&cu["skip_txs_per_churned_province_day"]["max"])
        )
    };
    if !slots_mode && !secs_mode {
        v.push(verdict("3", "n.a.", format!("criterion 3's targets are defined at 20x (slots) and 2x (game seconds); this run is {scale}x (figures reported); {skip_note}")));
    } else {
        let checks: [(&str, f64, &str); 4] = if slots_mode {
            [
                ("round_to_anchor_slots", 2.0, "slots"),
                ("s_to_first_cache_slots", 2.0, "slots"),
                ("anchor_to_last_reveal_slots", 4.0, "slots"),
                ("close_to_resolve_slots", 8.0, "slots"),
            ]
        } else {
            [
                ("round_to_anchor_game_secs", 5.0, "s"),
                ("s_to_first_cache_game_secs", 5.0, "s"),
                ("anchor_to_last_reveal_game_secs", 30.0, "s"),
                ("close_to_resolve_game_secs", 60.0, "s"),
            ]
        };
        let mut miss = vec![];
        let mut none = vec![];
        let mut seen = vec![];
        for (k, target, unit) in checks {
            match p99(k) {
                None => none.push(k),
                Some(x) if x > target => miss.push(format!("{k} p99 {x:.2} > {target} {unit}")),
                Some(x) => seen.push(format!("{k} p99 {x:.2} ≤ {target} {unit}")),
            }
        }
        if slots_mode {
            if cu.is_null() {
                none.push("catch_up");
            } else if len(&cu["idle_province_days_over_6"]) > 0 {
                miss.push(format!(
                    "idle province-days over 6 SkipQuiet: {}",
                    cu["idle_province_days_over_6"]
                ));
            }
        }
        v.push(if !miss.is_empty() {
            verdict("3", "fail", format!("{}; {skip_note}", miss.join("; ")))
        } else if !none.is_empty() {
            verdict(
                "3",
                "n.a.",
                format!("no samples for {}; {skip_note}", none.join(", ")),
            )
        } else {
            verdict("3", "pass", format!("{}; {skip_note}", seen.join("; ")))
        });
    }
    // 4 — liveness.
    let mut bad = vec![];
    let mut notes = vec![];
    let windows: Vec<(u64, u64)> = facts["above_cap_holds"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|h| {
                    let b = h["bell"].as_u64()?;
                    let bells = (h["game_secs"].as_f64().unwrap_or(0.0) / 600.0).ceil() as u64 + 1;
                    Some((b, b + bells))
                })
                .collect()
        })
        .unwrap_or_default();
    match facts["valid_unrevealed"].as_array() {
        None => notes.push("no verify report: ValidSealUnrevealed not read".to_string()),
        Some(list) => {
            let outside: Vec<&Value> = list
                .iter()
                .filter(|x| {
                    let b = x["arrive_bell"].as_u64().unwrap_or(0);
                    !windows.iter().any(|(lo, hi)| b >= *lo && b <= *hi)
                })
                .collect();
            if !outside.is_empty() {
                bad.push(format!(
                    "{} valid seals unrevealed outside the above-cap hold windows",
                    outside.len()
                ));
            }
            notes.push(format!(
                "{} unrevealed inside {} above-cap hold windows (expected)",
                list.len() - outside.len(),
                windows.len()
            ));
        }
    }
    let persona = |name: &str| {
        facts["personas"]
            .as_array()
            .and_then(|a| a.iter().find(|p| p["persona"] == name))
            .map(|p| p["verdict"].as_str().unwrap_or("").to_string())
    };
    match persona("min_tip").as_deref() {
        Some("violated") => bad.push("min_tip persona not fully revealed".into()),
        None => notes.push("no min_tip verdict".into()),
        _ => {}
    }
    for (w, k) in [("A", "keeper_a"), ("B", "keeper_b")] {
        let s = &facts[k];
        if s.is_null() {
            continue;
        }
        let low = len(&s["bells_below_150"]);
        if s["min_reveal_effective_n_in_play"].is_null() {
            notes.push(format!("keeper {w}: no status samples"));
        } else if low > 0 {
            bad.push(format!(
                "keeper {w}: reveal pool effective N below 150 in {low} bells ({})",
                s["bells_below_150"]
            ));
        }
    }
    v.push(if !bad.is_empty() {
        verdict(
            "4",
            "fail",
            format!("{}; {}", bad.join("; "), notes.join("; ")),
        )
    } else if facts["valid_unrevealed"].is_null() {
        verdict("4", "n.a.", notes.join("; "))
    } else {
        verdict("4", "pass", notes.join("; "))
    });
    // 5 — personas.
    let violated = len(&facts["violated"]);
    v.push(if facts["personas"].is_null() {
        verdict("5", "n.a.", "no bots report")
    } else if violated > 0 {
        verdict("5", "fail", format!("violated: {}", facts["violated"]))
    } else {
        verdict("5", "pass", "no persona violated")
    });
    // 6 — the herald under the in-run viewer window.
    let inrun = facts["loads"]
        .as_array()
        .and_then(|a| a.iter().find(|l| l["window"] == "in-run"));
    v.push(match inrun {
        None => verdict("6", "n.a.", "no in-run viewer window in this run (a post-play `load` is reported, not criterion-6 evidence)"),
        Some(l) if l["verdict"]["pass"] == true => verdict("6", "pass", format!("in-run window: {}", l["verdict"]["summary"])),
        Some(l) => verdict("6", "fail", format!("in-run window: {}", l["verdict"]["misses"])),
    });
    v.push(verdict("7", "n.a.", "reported, not gating (§13.4)"));
    // 8 — bad seals.
    let mut bad = vec![];
    if facts["bad_seal_survived"] == true {
        bad.push("BadSealSurvived".to_string());
    }
    for p in ["garbage_seal", "bad_plaintext", "settle_racer"] {
        if persona(p).as_deref() == Some("violated") {
            bad.push(format!("{p} violated"));
        }
    }
    v.push(if facts["verify_verdict"].is_null() {
        verdict("8", "n.a.", "no verify report")
    } else if bad.is_empty() {
        verdict(
            "8",
            "pass",
            "no bad seal survived; the bad-seal personas held",
        )
    } else {
        verdict("8", "fail", bad.join("; "))
    });
    // 9 — tickets.
    let old = len(&facts["cohorts_open_past_24_bells"]);
    v.push(if old > 0 {
        verdict("9", "fail", format!("{old} cohorts open past 24 bells"))
    } else if persona("ticket_holder").as_deref() == Some("violated") {
        verdict("9", "fail", "ticket_holder kept a site it did not win")
    } else {
        verdict("9", "pass", "every cohort closed within 24 bells")
    });
    v
}

/// Exit code of a criteria list: 1 when any decided criterion fails.
pub fn criteria_exit(v: &[Value]) -> i32 {
    if v.iter().any(|c| c["status"] == "fail") {
        1
    } else {
        0
    }
}

/// `report --run-id R`.
pub async fn report(rd: &RunDir) -> i32 {
    let st = match rd.load_state() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("frontier-stack: {e}");
            return 2;
        }
    };
    let program: Address = match st["program"].as_str().and_then(|s| s.parse().ok()) {
        Some(p) => p,
        None => {
            eprintln!("frontier-stack: state has no program");
            return 2;
        }
    };
    let input = crate::verifyrun::input_path(rd);
    let (inp, source) = match verify_core::input::Input::load(&input) {
        Ok(i) => (i, "verify input"),
        Err(_) => match crate::verifyrun::read_input(&st).await {
            Ok(i) => (i, "chain (no verify input)"),
            Err(e) => {
                eprintln!("frontier-stack: no verify input and the chain cannot be read: {e}");
                return 2;
            }
        },
    };
    let scale = st["config"]["scale"].as_f64().unwrap_or(20.0);
    let genesis = st["play"]["genesis_ts"].as_i64().unwrap_or(0);
    let play_bells = st["play"]["play_bells"].as_u64().unwrap_or(0) as u32;
    let last_time = inp
        .txs
        .iter()
        .map(|t| t.block_time)
        .max()
        .unwrap_or(genesis);
    let bell_secs = st["season"]["bell_secs"].as_i64().unwrap_or(600).max(1);
    let last_bell = ((last_time - genesis).max(0) / bell_secs) as u32;
    let delay_s = st["config"]["drand_delay_ms"].as_f64().unwrap_or(1_000.0) / 1_000.0;
    let drand = fclient::clock::Drand {
        genesis: frontier_abi::presets::M1_LOCAL_7D.drand_genesis,
        period: frontier_abi::presets::M1_LOCAL_7D.drand_period,
    };
    let recs = records(&inp.txs, &program);
    let (cu, reveal) = cu_table(&inp.txs, &program);
    // A drain at another scale (W6-A) is left out of the latencies: they
    // are judged over play, at the run's scale.
    let cut = st["play"]["drain_scale"]["from_game"]
        .as_i64()
        .unwrap_or(i64::MAX);
    let play_recs: Vec<Rec> = recs.iter().filter(|x| x.time < cut).cloned().collect();
    let clock = SlotClock::new(inp.txs.iter().filter(|t| t.block_time < cut), 0.4 * scale);
    let lat = latencies(
        &play_recs,
        &clock,
        &drand,
        delay_s,
        frontier_abi::presets::M1_LOCAL_7D.reveal_window,
    );
    let catch = catch_up(&inp.txs, &program, genesis, bell_secs);
    let out = outcomes(&recs, last_bell);
    let ps = final_provinces(&inp);
    let pc = province_checks(&ps, play_bells, last_bell);
    let cis = clash_inputs_check(&inp, &ps, &recs, last_bell);
    let ka = keeper_summary(
        &read_jsonl(&rd.path("metrics/keeper-a.jsonl")),
        0,
        play_bells as i64,
    );
    let kb = keeper_summary(
        &read_jsonl(&rd.path("metrics/keeper-b.jsonl")),
        0,
        play_bells as i64,
    );
    let herald = herald_summary(&read_jsonl(&rd.path("metrics/herald.jsonl")));
    let ev = events_summary(&rd.events());
    let bots = read_json(&rd.path("bots/report.json"));
    let pers = personas(&bots);
    let verify = read_json(&rd.path("verify/summary.json"));
    let tamper = read_json(&rd.path("tamper/report.json"));
    let loads: Vec<Value> = std::fs::read_dir(rd.path("load"))
        .map(|d| {
            let mut v: Vec<_> = d
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().ends_with(".verdict.json"))
                .map(|e| read_json(&e.path()))
                .collect();
            v.sort_by_key(|x| x["tag"].as_str().unwrap_or("").to_string());
            v
        })
        .unwrap_or_default();
    let over: Vec<Value> = cu
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|r| r["over_budget"] == true)
                .map(|r| r["kind"].clone())
                .collect()
        })
        .unwrap_or_default();
    let bad_seal_survived = verify["fail_codes"]
        .as_array()
        .is_some_and(|a| a.iter().any(|c| c == "BadSealSurvived"));
    let end_bell = st["season"]["end_bell"].as_u64().unwrap_or(1_008) as u32;
    let criteria = json!({
        "1_complete": {"reaches_end_bell": play_bells >= end_bell, "end_season": ev["end_season"],
            "stuck_province_bells": pc["stuck_province_bells"].as_array().map_or(0, |a| a.len()),
            "unsettled_due_transits": out["unsettled_due"].as_array().map_or(0, |a| a.len()),
            "clash_inputs": cis,
            "note": if play_bells < end_bell { "the run stops before end_bell: EndSeason not expected" } else { "" }},
        "2_cu": {"over_budget": over, "reveal": stats(&reveal)},
        "3_latency_slots": lat,
        "3_catch_up": catch,
        "4_liveness": {"keeper_a_min_effective_n": ka["min_reveal_effective_n_in_play"], "effective_n_ok": ka["effective_n_ok"]},
        "5_personas": {"violated": pers["violated"]},
        "6_herald": {"loads": loads.iter().map(|l| l["verdict"].clone()).collect::<Vec<_>>(), "alarms": herald["alarms_total"]},
        "8_bad_seals": {"bad_seal_codes": out["bad_seal_codes"], "BadSealSurvived": bad_seal_survived},
        "9_tickets": {"cohorts_open_past_24_bells": pc["cohorts_open_past_24_bells"]},
    });
    let verify_report = read_json(&rd.path("verify/report.json"));
    let above_cap: Vec<Value> = rd
        .events()
        .iter()
        .filter(|e| e["event"] == "hold" && e["detail"]["above_keeper_cap"] == true)
        .map(|e| e["detail"].clone())
        .collect();
    let facts = json!({
        "play_bells": play_bells, "end_bell": end_bell, "scale": scale, "end_season": ev["end_season"],
        "stuck_province_bells": pc["stuck_province_bells"], "cohorts_open_past_24_bells": pc["cohorts_open_past_24_bells"],
        "unsettled_due": out["unsettled_due"], "settled_more_than_once": out["settled_more_than_once"],
        "settles_without_depart": out["settles_without_depart"], "over_budget": over, "latency": lat,
        "catch_up": catch, "clash_inputs": cis,
        "valid_unrevealed": verify_report["liveness"]["valid_unrevealed"], "above_cap_holds": above_cap,
        "personas": pers["personas"], "violated": pers["violated"],
        "keeper_a": ka, "keeper_b": if st["config"]["keeper_b"] == false { Value::Null } else { kb.clone() },
        "loads": loads, "bad_seal_survived": bad_seal_survived, "verify_verdict": verify["verdict"],
    });
    let decided = decide(&facts);
    let holds = hold_effects(&inp.txs, &rd.events());
    let rep = json!({
        "format": "frontier-stack-report-v1",
        "decided": decided,
        "run_id": st["run_id"], "phase": st["phase"], "config": st["config"], "so": st["so"], "program": st["program"],
        "season": st["season"], "play": st["play"], "source": source, "txs": inp.txs.len(), "last_bell": last_bell,
        "cu": cu, "reveal_cu": stats(&reveal), "failures": failures(&inp.txs, &program),
        "outcomes": out, "provinces": pc, "keepers": {"a": ka, "b": kb}, "herald": herald,
        "events": ev, "bots": {"personas": pers, "errors": bots["errors"], "bots": bots["bots"]},
        "verify": verify, "tamper": {"ok": tamper["ok"], "total": tamper["total"], "on_run": tamper["on_run"], "secs": tamper["secs"]},
        "loads": loads, "criteria": criteria, "hold_effects": holds,
    });
    let md = markdown(&rep);
    let _ = crate::run::write_atomic(
        &rd.path("report.json"),
        serde_json::to_string_pretty(&rep)
            .unwrap_or_default()
            .as_bytes(),
    );
    let _ = crate::run::write_atomic(&rd.path("report.md"), md.as_bytes());
    print!("{md}");
    criteria_exit(&decided)
}

fn f(v: &Value) -> String {
    match v {
        Value::Null => "–".into(),
        Value::Number(n) => n
            .as_f64()
            .map(|x| {
                if x.fract() == 0.0 {
                    format!("{x:.0}")
                } else {
                    format!("{x:.2}")
                }
            })
            .unwrap_or_default(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub fn markdown(r: &Value) -> String {
    let c = &r["config"];
    let mut m = format!(
        "# Stack run `{}`\n\n- phase **{}**, beacon **{}**, scale {}×, {} bots, play {} bells + {} drain; program `{}` (`.so` sha256 `{}`, {} B)\n- source: {} ({} transactions, last bell {})\n\n",
        f(&r["run_id"]), f(&r["phase"]), f(&c["beacon"]), f(&c["scale"]), f(&c["bots"]),
        f(&r["play"]["play_bells"]), f(&r["play"]["drain_bells"]), f(&r["program"]), f(&r["so"]["sha256"]),
        f(&r["so"]["len"]), f(&r["source"]), f(&r["txs"]), f(&r["last_bell"])
    );
    m.push_str("## §13.4 criteria decided\n\n| criterion | status | why |\n|---|---|---|\n");
    for c in r["decided"].as_array().cloned().unwrap_or_default() {
        m.push_str(&format!(
            "| {} | **{}** | {} |\n",
            f(&c["criterion"]),
            f(&c["status"]),
            f(&c["why"]).replace('|', "\\|")
        ));
    }
    m.push_str("\n## Verdicts\n\n");
    m.push_str(&format!(
        "- verify: **{}** (fail codes {}; {} txs, read {} s, verify {} s — E6 wall time)\n- tamper: {}/{} classes FAIL with their codes ({} built from the run; {} s)\n- `.so` pin: expected sha256 {} ({})\n",
        f(&r["verify"]["verdict"]), f(&r["verify"]["fail_codes"]), f(&r["verify"]["txs"]), f(&r["verify"]["read_secs"]),
        f(&r["verify"]["verify_secs"]), f(&r["tamper"]["ok"]), f(&r["tamper"]["total"]), f(&r["tamper"]["on_run"]),
        f(&r["tamper"]["secs"]), f(&r["config"]["expect_so_sha256"]),
        if r["config"]["expect_so_sha256"].is_string() { "the release build record; V2 checks the deployed program against it" } else { "none: V2 checks the deployed file's own hash, not exit-grade" }
    ));
    for l in r["loads"].as_array().cloned().unwrap_or_default() {
        let v = &l["verdict"];
        m.push_str(&format!(
            "- load {} ({} viewers, {} game h): p99 file {} ms, error rate {}, ingest lag p99 {} s → **{}**\n",
            f(&l["tag"]), f(&l["viewers"]), f(&l["game_hours"]), f(&v["p99_file_ms"]), f(&v["error_rate"]),
            f(&v["ingest_lag_p99_s"]), if v["pass"] == true { "pass" } else { "fail" }
        ));
    }
    m.push_str("\n## CU per instruction kind (whole-transaction units)\n\n| kind | landed | failed | p50 | p99 | max | budget | over | max bytes |\n|---|---|---|---|---|---|---|---|---|\n");
    for row in r["cu"].as_array().cloned().unwrap_or_default() {
        m.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {}{} | {} | {} |\n",
            f(&row["kind"]),
            f(&row["landed"]),
            f(&row["failed"]),
            f(&row["cu"]["p50"]),
            f(&row["cu"]["p99"]),
            f(&row["cu"]["max"]),
            f(&row["budget"]),
            if row["budget_per_unit"].as_u64().unwrap_or(0) > 0 {
                format!(" + {}/unit", f(&row["budget_per_unit"]))
            } else {
                String::new()
            },
            if row["over_budget"] == true {
                "**yes**"
            } else {
                "no"
            },
            f(&row["max_tx_bytes"])
        ));
    }
    let rv = &r["reveal_cu"];
    m.push_str(&format!(
        "\n**Reveal CU distribution** (C4 input): n {}, p50 {}, p90 {}, p99 {}, max {}.\n",
        f(&rv["n"]),
        f(&rv["p50"]),
        f(&rv["p90"]),
        f(&rv["p99"]),
        f(&rv["max"])
    ));
    let l = &r["criteria"]["3_latency_slots"];
    m.push_str(&format!(
        "\n## Keeper latencies (a slot is {} game s)\n\nRound → anchor and S → first cache in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×.\n\n| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |\n|---|---|---|---|---|---|---|---|---|\n",
        f(&l["slot_game_secs"])
    ));
    for (k, t, ks, ts) in [
        (
            "round_to_anchor_slots",
            "2",
            "round_to_anchor_game_secs",
            "5 s",
        ),
        (
            "s_to_first_cache_slots",
            "2",
            "s_to_first_cache_game_secs",
            "5 s",
        ),
        (
            "anchor_to_last_reveal_slots",
            "4",
            "anchor_to_last_reveal_game_secs",
            "30 s",
        ),
        (
            "close_to_resolve_slots",
            "8",
            "close_to_resolve_game_secs",
            "60 s",
        ),
    ] {
        let s = &l[k];
        let g = &l[ks];
        m.push_str(&format!(
            "| {k} | {} | {} | {} | {} | {t} | {} | {} | {ts} |\n",
            f(&s["n"]),
            f(&s["p50"]),
            f(&s["p99"]),
            f(&s["max"]),
            f(&g["p50"]),
            f(&g["p99"])
        ));
    }
    let rp = &l["round_to_anchor_from_publication_slots"];
    m.push_str(&format!(
        "\nRound → anchor from the publication instant, in slots: p50 {}, p99 {}, max {}.\n",
        f(&rp["p50"]),
        f(&rp["p99"]),
        f(&rp["max"])
    ));
    let c = &r["criteria"]["3_catch_up"];
    m.push_str("\n### Catch-up (SkipQuiet transactions per province-day)\n\n| province-days | n | p50 | p99 | max | over 6 |\n|---|---|---|---|---|---|\n");
    for (name, k, over) in [
        (
            "idle (roster unchanged)",
            "skip_txs_per_idle_province_day",
            "idle_province_days_over_6",
        ),
        (
            "churned",
            "skip_txs_per_churned_province_day",
            "churned_province_days_over_6",
        ),
        ("all", "skip_txs_per_province_day", ""),
    ] {
        let s = &c[k];
        m.push_str(&format!(
            "| {name} | {} | {} | {} | {} | {} |\n",
            f(&s["n"]),
            f(&s["p50"]),
            f(&s["p99"]),
            f(&s["max"]),
            if over.is_empty() {
                "–".to_string()
            } else {
                c[over].as_array().map_or(0, |a| a.len()).to_string()
            }
        ));
    }
    let ci = &r["criteria"]["1_complete"]["clash_inputs"];
    m.push_str(&format!(
        "\n**ClashInputs:** {} closed, {} open: {} closable after grace, {} pending, {} blocked.\n",
        f(&ci["closed"]),
        f(&ci["open"]),
        f(&ci["closable_after_grace"]),
        f(&ci["pending"]),
        ci["blocked"].as_array().map_or(0, |a| a.len())
    ));
    let o = &r["outcomes"];
    m.push_str(&format!(
        "\n## Play\n\n- records: {}\n- transits: {}\n- departs {} (due {}), unsettled due: {}\n- bad-seal codes: {}\n- failed transactions: {}\n",
        o["records"], o["transits"], f(&o["departs"]), f(&o["departs_due"]),
        o["unsettled_due"].as_array().map_or(0, |a| a.len()), o["bad_seal_codes"], r["failures"]
    ));
    let p = &r["provinces"];
    m.push_str(&format!(
        "- provinces {}; stuck province-bells: {}; cohorts open past 24 bells: {}\n",
        f(&p["provinces"]),
        p["stuck_province_bells"].as_array().map_or(0, |a| a.len()),
        p["cohorts_open_past_24_bells"]
            .as_array()
            .map_or(0, |a| a.len())
    ));
    let k = &r["keepers"];
    m.push_str(&format!(
        "\n## Keepers, herald, bots\n\n- keeper A: min reveal effective N in play {} (≥ 150: {}), last status {}\n- keeper B: min reveal effective N in play {}\n- herald: fold lag slots p99 {}, alarms {}\n- personas violated: {}\n",
        f(&k["a"]["min_reveal_effective_n_in_play"]), f(&k["a"]["effective_n_ok"]), k["a"]["last"],
        f(&k["b"]["min_reveal_effective_n_in_play"]), f(&r["herald"]["fold_lag_slots"]["p99"]),
        f(&r["herald"]["alarms_total"]), r["bots"]["personas"]["violated"]
    ));
    let e = &r["events"];
    m.push_str(&format!(
        "\n## Chaos and adversary\n\n- chaos kills {}, restarts {}, crashes {}\n",
        f(&e["chaos_kills"]),
        f(&e["restarts"]),
        e["crashes"].as_array().map_or(0, |a| a.len())
    ));
    for h in e["holds"].as_array().cloned().unwrap_or_default() {
        let d = &h["detail"];
        m.push_str(&format!(
            "- {} {} at game {}: {} keys, {} milli, {} slots, above keeper cap {}\n",
            f(&h["event"]),
            f(&d["kind"]),
            f(&h["game"]),
            f(&d["keys"]),
            f(&d["priority_milli"]),
            f(&d["slots"]),
            f(&d["above_keeper_cap"])
        ));
    }
    m.push_str("\n### Hold effects (held keys written inside the window / in as long again after it)\n\n| kind | above cap | slots | keys | inside | after | first after |\n|---|---|---|---|---|---|---|\n");
    for h in r["hold_effects"].as_array().cloned().unwrap_or_default() {
        m.push_str(&format!(
            "| {} | {} | {}–{} | {} | {} | {} | {} |\n",
            f(&h["kind"]),
            f(&h["above_keeper_cap"]),
            f(&h["from_slot"]),
            f(&h["until_slot"]),
            f(&h["keys"]),
            f(&h["writes_inside"]),
            f(&h["writes_in_the_window_after"]),
            f(&h["first_write_after"])
        ));
    }
    m.push_str(&format!(
        "\n## §13.4 criteria (what this run decides)\n\n```json\n{}\n```\n",
        serde_json::to_string_pretty(&r["criteria"]).unwrap_or_default()
    ));
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn program_fixture() -> verify_core::input::Input {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/verify/march-program.json.gz");
        verify_core::input::Input::load(&p).expect("fixture")
    }

    #[test]
    fn field_offsets_follow_the_specs() {
        assert_eq!(field(Kind::ANCHOR, "bell"), Some(0));
        assert_eq!(field(Kind::ANCHOR, "round"), Some(5));
        assert_eq!(field(Kind::ANCHOR, "a"), Some(13));
        assert_eq!(field(Kind::DEPART, "arrive_bell"), Some(8 + 4 + 4 + 1 + 4));
        assert_eq!(field(Kind::PROVINCE_OPEN, "region"), Some(8 + 2 + 1));
        assert_eq!(field(Kind::SEED, "round"), Some(6));
        assert_eq!(field(Kind::TRANSIT_SETTLED, "seal_code"), Some(9));
    }

    #[test]
    fn the_report_pieces_on_the_program_recording() {
        let inp = program_fixture();
        let program = inp.cfg.program;
        let (cu, reveal) = cu_table(&inp.txs, &program);
        let rows = cu.as_array().unwrap();
        assert!(rows.iter().any(|r| r["kind"] == "Reveal"), "{cu}");
        assert!(!reveal.is_empty());
        assert!(rows.iter().all(|r| r["over_budget"] == false), "{cu}");
        let recs = records(&inp.txs, &program);
        let drand = fclient::clock::Drand {
            genesis: frontier_abi::presets::M1_LOCAL_7D.drand_genesis,
            period: 3,
        };
        let clock = SlotClock::new(&inp.txs, 8.0);
        let lat = latencies(&recs, &clock, &drand, 1.0, 600);
        assert!(
            lat["round_to_anchor_slots"]["n"].as_u64().unwrap() > 0,
            "{lat}"
        );
        assert_eq!(
            lat["round_to_anchor_slots"]["n"], lat["round_to_anchor_game_secs"]["n"],
            "{lat}"
        );
        assert_eq!(lat["rounds_without_a_mapped_slot"], 0, "{lat}");
        assert!(
            lat["close_to_resolve_slots"]["n"].as_u64().unwrap() > 0,
            "{lat}"
        );
        let out = outcomes(&recs, 144);
        assert!(out["records"]["CLASH"].as_u64().unwrap() > 0, "{out}");
        assert!(out["departs"].as_u64().unwrap() > 0);
        let ps = final_provinces(&inp);
        assert!(!ps.is_empty());
        let pc = province_checks(&ps, 144, 170);
        assert!(pc["provinces"].as_u64().unwrap() > 0);
        let fl = failures(&inp.txs, &program);
        assert!(fl.is_object());
    }

    fn good_facts() -> Value {
        json!({
            "play_bells": 144, "end_bell": 1008, "scale": 20.0, "end_season": null,
            "stuck_province_bells": [], "cohorts_open_past_24_bells": [], "unsettled_due": [],
            "settled_more_than_once": [], "settles_without_depart": [], "over_budget": [],
            "latency": {"slot_game_secs": 8.0, "round_to_anchor_slots": {"p99": 1.5}, "s_to_first_cache_slots": {"p99": 2.0},
                "anchor_to_last_reveal_slots": {"p99": 3.0}, "close_to_resolve_slots": {"p99": 6.0},
                "round_to_anchor_game_secs": {"p99": 12.0}, "s_to_first_cache_game_secs": {"p99": 16.0},
                "anchor_to_last_reveal_game_secs": {"p99": 24.0}, "close_to_resolve_game_secs": {"p99": 48.0}},
            "catch_up": {"idle_province_days_over_6": [], "churned_province_days_over_6": ["(1,1) day 0: 9"],
                "skip_txs_per_idle_province_day": {"p99": 4.0, "max": 4.0}, "skip_txs_per_churned_province_day": {"p99": 9.0, "max": 9.0}},
            "clash_inputs": {"closed": 0, "open": 3, "closable_after_grace": 2, "pending": 1, "blocked": []},
            "valid_unrevealed": [{"host_id": "1", "arrive_bell": 50}],
            "above_cap_holds": [{"bell": 49, "game_secs": 600.0}],
            "personas": [{"persona": "min_tip", "verdict": "observed"}], "violated": [],
            "keeper_a": {"min_reveal_effective_n_in_play": 150, "bells_below_150": []}, "keeper_b": null,
            "loads": [{"window": "in-run", "verdict": {"pass": true, "summary": "ok"}}],
            "bad_seal_survived": false, "verify_verdict": "PASS",
        })
    }

    /// Wave-5 review of W5-B: the report decides the criteria and fails.
    #[test]
    fn the_report_decides_the_criteria() {
        let d = decide(&good_facts());
        let st = |id: &str| d.iter().find(|c| c["criterion"] == id).unwrap()["status"].clone();
        assert_eq!(st("1"), "pass");
        assert_eq!(st("3"), "pass");
        assert_eq!(st("4"), "pass", "{d:?}");
        assert_eq!(st("6"), "pass");
        assert_eq!(st("7"), "n.a.");
        assert_eq!(criteria_exit(&d), 0);
        let mut f = good_facts();
        f["latency"]["close_to_resolve_slots"]["p99"] = json!(9.0);
        f["valid_unrevealed"] = json!([{"host_id": "1", "arrive_bell": 90}]);
        f["settled_more_than_once"] = json!(["0x1@12 x2"]);
        f["keeper_a"]["bells_below_150"] = json!([0, 1]);
        f["loads"] = json!([{"window": "post-play", "verdict": {"pass": true}}]);
        let d = decide(&f);
        let st = |id: &str| d.iter().find(|c| c["criterion"] == id).unwrap()["status"].clone();
        assert_eq!(st("1"), "fail");
        assert_eq!(st("3"), "fail");
        assert_eq!(st("4"), "fail");
        assert_eq!(
            st("6"),
            "n.a.",
            "a post-play load is not criterion-6 evidence"
        );
        assert_eq!(criteria_exit(&d), 1);
        // At 20x an idle province-day over 6 SkipQuiet fails criterion 3;
        // a churned one is reported only.
        let mut f = good_facts();
        f["catch_up"]["idle_province_days_over_6"] = json!(["(2,2) day 1: 7"]);
        let d = decide(&f);
        assert_eq!(d[2]["status"], "fail", "{d:?}");
        // At 2x the game-second targets are judged, not the slot ones.
        let mut f = good_facts();
        f["scale"] = json!(2.0);
        f["latency"]["slot_game_secs"] = json!(0.8);
        let d = decide(&f);
        assert_eq!(d[2]["status"], "fail", "12 s and 16 s are over 5 s: {d:?}");
        f["latency"]["round_to_anchor_game_secs"]["p99"] = json!(2.4);
        f["latency"]["s_to_first_cache_game_secs"]["p99"] = json!(4.0);
        f["catch_up"]["idle_province_days_over_6"] = json!(["(2,2) day 1: 7"]);
        let d = decide(&f);
        assert_eq!(d[2]["status"], "pass", "catch-up is a 20x target: {d:?}");
        // A blocked ClashInputs fails criterion 1.
        let mut f = good_facts();
        f["clash_inputs"]["blocked"] = json!(["(1,1) bell 9: present records [0] never settled"]);
        let d = decide(&f);
        assert_eq!(d[0]["status"], "fail", "{d:?}");
        // Other scales: criterion 3 is not decided.
        let mut f = good_facts();
        f["scale"] = json!(100.0);
        let d = decide(&f);
        assert_eq!(
            d.iter().find(|c| c["criterion"] == "3").unwrap()["status"],
            "n.a."
        );
        // The season's end: EndSeason must land.
        let mut f = good_facts();
        f["play_bells"] = json!(1008);
        f["end_season"] = json!({"ok": false});
        assert_eq!(decide(&f)[0]["status"], "fail");
    }

    #[test]
    fn hold_effects_count_writes_of_held_keys() {
        let inp = program_fixture();
        let t = inp
            .txs
            .iter()
            .find(|t| t.err.is_none())
            .expect("a landed tx");
        let tx = fclient::tx::from_wire(&t.tx).unwrap();
        let m = &tx.message;
        let k = (0..m.account_keys.len())
            .find(|&i| fclient::tx::is_writable_index(m, i))
            .map(|i| m.account_keys[i].to_string())
            .unwrap();
        let ev = vec![
            json!({"event": "hold", "detail": {"kind": "lag", "result": {"keys": [k], "fromSlot": t.slot, "untilSlot": t.slot}}}),
            json!({"event": "hold", "detail": {"kind": "anchor", "result": null}}),
        ];
        let h = hold_effects(&inp.txs, &ev);
        assert_eq!(h.len(), 2);
        assert!(h[0]["writes_inside"].as_u64().unwrap() >= 1, "{h:?}");
        assert!(h[1]["effect"].as_str().unwrap().contains("did not start"));
    }

    #[test]
    fn settlement_is_counted_per_depart() {
        let inp = program_fixture();
        let mut recs = records(&inp.txs, &inp.cfg.program);
        let out = outcomes(&recs, 144);
        assert_eq!(out["settled_more_than_once"], json!([]), "{out}");
        assert_eq!(out["settles_without_depart"], json!([]), "{out}");
        // A second settlement of a transit is caught.
        let i = recs
            .iter()
            .position(|x| x.r.kind == Kind::TRANSIT_SETTLED as u8)
            .expect("a settle");
        let dup = recs[i].clone();
        recs.push(dup);
        let out = outcomes(&recs, 144);
        assert_eq!(
            out["settled_more_than_once"].as_array().unwrap().len(),
            1,
            "{out}"
        );
    }

    /// W5-B F5: the slot reference is the first slot whose Clock reaches
    /// the publication instant.
    #[test]
    fn slot_clock_finds_the_first_slot_at_a_time() {
        let tx = |slot: u64, t: i64| TxRecord {
            seq: slot,
            slot,
            signature: Default::default(),
            block_time: t,
            tx: vec![],
            logs: vec![],
            err: None,
            code: None,
            units: 0,
            fee: 0,
            post: vec![],
        };
        // 8 game s per slot (20x); slots 10 and 14 observed.
        let c = SlotClock::new(&[tx(14, 1_032), tx(10, 1_000)], 8.0);
        // (the same from any iterator of records)
        assert_eq!(c.first_slot_at(1_000.0), Some(10));
        assert_eq!(c.first_slot_at(1_001.0), Some(11), "Clock 1,008 at slot 11");
        assert_eq!(c.first_slot_at(1_008.0), Some(11));
        assert_eq!(c.first_slot_at(1_009.0), Some(12));
        assert_eq!(c.first_slot_at(1_032.0), Some(14));
        assert_eq!(
            c.first_slot_at(990.0),
            Some(9),
            "counted back before the first point"
        );
        assert_eq!(c.first_slot_at(1_033.0), None);
        // A pause (the Clock stopped while the chain was down) never puts
        // the answer at or before an observed earlier slot.
        let c = SlotClock::new(&[tx(10, 1_000), tx(11, 1_100)], 8.0);
        assert_eq!(c.first_slot_at(1_050.0), Some(11));
    }

    #[test]
    fn catch_up_and_clash_inputs_on_the_program_recording() {
        let inp = program_fixture();
        let program = inp.cfg.program;
        let genesis = inp
            .txs
            .iter()
            .filter(|t| t.err.is_none())
            .flat_map(|t| fclient::log::bodies_from_logs(&t.logs, &program).unwrap_or_default())
            .filter_map(|b| Record::decode_with(&b, &lens).ok())
            .find(|r| r.kind == Kind::SEASON_CREATED as u8)
            .map(|r| {
                le_i64(
                    &r.key_payload,
                    field(Kind::SEASON_CREATED, "genesis_ts").unwrap(),
                )
            })
            .expect("SEASON_CREATED");
        let c = catch_up(&inp.txs, &program, genesis, 600);
        assert!(c["province_post_states"].as_u64().unwrap() > 0, "{c}");
        let all = c["skip_txs_per_province_day"]["n"].as_u64().unwrap();
        let idle = c["skip_txs_per_idle_province_day"]["n"].as_u64().unwrap();
        let churned = c["skip_txs_per_churned_province_day"]["n"]
            .as_u64()
            .unwrap();
        assert!(all > 0, "{c}");
        assert_eq!(
            idle + churned + c["province_days_not_judged"].as_u64().unwrap(),
            all,
            "{c}"
        );
        let recs = records(&inp.txs, &program);
        let ps = final_provinces(&inp);
        let ci = clash_inputs_check(&inp, &ps, &recs, 170);
        assert!(ci["open"].as_u64().unwrap() > 0, "{ci}");
        assert_eq!(
            ci["open"].as_u64().unwrap(),
            ci["closable_after_grace"].as_u64().unwrap()
                + ci["pending"].as_u64().unwrap()
                + ci["blocked"].as_array().unwrap().len() as u64,
            "{ci}"
        );
        assert_eq!(ci["blocked"], json!([]), "an honest recording: {ci}");
    }

    #[test]
    fn summaries() {
        let ks = vec![
            json!({"bell": 0, "status": {"pools": {"reveal": {"effective_n": 3}}}}),
            json!({"bell": 3, "status": {"pools": {"reveal": {"effective_n": 150}}}}),
            json!({"bell": 4, "status": {"pools": {"reveal": {"effective_n": 149}}}}),
        ];
        let k = keeper_summary(&ks, 0, 144);
        // Every bell counts (I-49), the first ones too (wave-5 review).
        assert_eq!(k["min_reveal_effective_n_in_play"], 3);
        assert_eq!(k["bells_below_150"], json!([0, 4]));
        assert_eq!(k["effective_n_ok"], false);
        let ev = vec![
            json!({"event": "chaos-kill", "detail": {}}),
            json!({"event": "restart", "detail": {}}),
            json!({"event": "crash", "detail": {"component": "herald"}}),
            json!({"event": "hold", "game": 5, "detail": {"kind": "anchor"}}),
        ];
        let e = events_summary(&ev);
        assert_eq!(e["chaos_kills"], 1);
        assert_eq!(e["crashes"].as_array().unwrap().len(), 1);
        assert_eq!(e["holds"].as_array().unwrap().len(), 1);
        let p = personas(
            &json!({"personas": [{"persona": "min_tip", "verdict": "observed"}, {"persona": "forger", "verdict": "violated"}]}),
        );
        assert_eq!(p["violated"], json!(["forger"]));
    }
}
