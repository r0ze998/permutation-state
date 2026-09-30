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
//! verdicts. W6T-4: idle province-days exclude days with a GATHER or
//! CLASH (§13.4 A1); criterion 4 lists V5's by-rule reasons and an honest
//! march refused by rule is a criterion-5 violation (A2); failed
//! transactions by class (expected / redundancy / waste); the keeper
//! status from the last answered sample with its unanswered bells; the
//! per-bell load average; stalls of ≥ 20 slots after a bell start; and row
//! E, the §13.4 environment (not exit-grade when a hold was skipped).

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

/// The class of a failed transaction by (kind, error) (W6T-4, from the
/// w6-s7 triage's table): `expected` (a persona, adversary, race or drain
/// refusal the run plans for), `redundancy` (another sender landed the same
/// work first: A/B races, duplicates) or `waste` (work that could never
/// land: keeper, bot-policy or bot bugs); anything else `unclassified`.
/// Returns `(class, cause)`. Reported, not gating.
pub fn classify_failure(kind: &str, err: &str) -> (&'static str, &'static str) {
    // Error names come from `failures` (the program's code name, else the
    // transaction error JSON).
    let pftc = err.contains("ProgramFailedToComplete") || err.contains("exceeded CUs meter");
    match (kind, err) {
        // Keeper waste: CU exhaustion of the closes (w6-s7: re-planned ~270
        // times each in the drain) and anchors for bells ≥ end_bell.
        ("CloseArrivalDay" | "CloseArrivalSlot" | "CloseClashInputs", _) if pftc => {
            ("waste", "keeper")
        }
        (_, _) if pftc => ("waste", "keeper"),
        ("PostAnchor" | "PostAnchorMulti", "BadData") => ("waste", "keeper"),
        // Another sender landed the same work first.
        ("SettleDeparture" | "SettleTransit" | "Reveal", "AlreadyDone") => {
            ("redundancy", "a/b race")
        }
        ("SettleTransit" | "SettleDeparture", "TransitState") => ("redundancy", "a/b race"),
        (_, "AlreadyDone") => ("redundancy", "duplicate"),
        // Bot bugs (W6T-3 fixes them): a war target while shielded, an
        // arrival at or after end_bell.
        ("Reveal", "Shielded" | "WrongStatus") => ("waste", "bot-bug"),
        ("Depart" | "Reveal", "ArrivalBell") => ("waste", "bot-bug"),
        // Bot policy: acting on a stale view.
        (_, "NotResident" | "ProvinceFull" | "QueueFull" | "Insufficient") => {
            ("waste", "bot-policy")
        }
        // Persona and adversary traffic, races the program settles, drain.
        ("Depart", "TipTooLow" | "TipNotPreset") => ("expected", "persona"),
        ("Reveal", "WindowClosed") => ("expected", "persona"),
        ("Reveal", "BadAddress") => ("expected", "adversary"),
        (_, "HostInTransit") => ("expected", "persona"),
        ("SettleTicket", "NoTicket") => ("expected", "race"),
        ("ResolveFromInputs" | "SkipQuiet", "OutOfOrder") => ("expected", "bounded duplicate"),
        ("SkipQuiet", "NotQuiet") => ("expected", "race"),
        ("FoldOccupancy", "FoldStale") => ("expected", "race"),
        _ => ("unclassified", ""),
    }
}

/// Failed transactions by (kind, error) with their class, and the totals
/// per class.
pub fn failure_classes(failures: &Value) -> Value {
    let mut rows = vec![];
    let mut by_class: BTreeMap<&str, u64> = BTreeMap::new();
    let mut by_cause: BTreeMap<String, u64> = BTreeMap::new();
    let mut total = 0u64;
    for (k, n) in failures.as_object().into_iter().flatten() {
        let n = n.as_u64().unwrap_or(0);
        let (kind, err) = k.split_once(": ").unwrap_or((k.as_str(), ""));
        let (class, cause) = classify_failure(kind, err);
        total += n;
        *by_class.entry(class).or_default() += n;
        if !cause.is_empty() {
            *by_cause.entry(cause.to_string()).or_default() += n;
        }
        rows.push(json!({"kind": kind, "error": err, "n": n, "class": class, "cause": cause}));
    }
    rows.sort_by(|a, b| b["n"].as_u64().cmp(&a["n"].as_u64()));
    json!({"total": total, "by_class": by_class, "by_cause": by_cause,
        "unclassified": by_class.get("unclassified").copied().unwrap_or(0), "rows": rows,
        "note": "reported, not gating: expected = persona/adversary/race/drain refusals; redundancy = another sender landed the work first; waste = could never land (keeper, bot-policy, bot-bug)"})
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
///
/// **S → resolve (integ-W6, the reading criterion 3's close → resolve
/// target is judged on):** ResolveFromInputs needs the seed of THE anchor,
/// and `S(b, r) = first_round_from(close + Δ)` with `Δ = seed_margin ≥ 60`
/// (§5.1), so no resolve can land before the seed round is public, about
/// `Δ + 1 s` after the close: a "close → resolve ≤ 60 s" reading is
/// unattainable by construction. `s_to_resolve_*` therefore counts like
/// round → anchor: from the first slot whose Clock shows `S(b, r)` public
/// (slots) or from its publication instant (game seconds) to the CLASH
/// landing. `close_to_resolve_*` is still reported (it is the seed margin
/// plus this).
pub fn latencies(
    recs: &[Rec],
    clock: &SlotClock,
    drand: &fclient::clock::Drand,
    delay_s: f64,
    reveal_window: u32,
    seed_margin: u32,
) -> Value {
    let slot_secs = clock.slot_secs;
    let to_slots = |secs: f64| (secs / slot_secs).max(0.0);
    let mut region_of: HashMap<(i32, i32), u8> = HashMap::new();
    let mut anchor_a: HashMap<(u32, u8), i64> = HashMap::new();
    let (mut ra_slots, mut ra_secs, mut ra_pub) = (vec![], vec![], vec![]);
    let (mut reveal_last, mut close_resolve) = (vec![], vec![]);
    let (mut sr_slots, mut sr_secs) = (vec![], vec![]);
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
                    let s = drand.seed_round(close, seed_margin);
                    if let Some((slots, secs, _)) = public_lag(s, x) {
                        sr_slots.push(slots);
                        sr_secs.push(secs);
                    }
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
        "s_to_resolve_slots": stats(&sr_slots),
        "s_to_resolve_game_secs": stats(&sr_secs),
        "rounds_without_a_mapped_slot": unmapped,
        "targets_slots_p99": {"round_to_anchor": 2, "s_to_first_cache": 2, "anchor_to_last_reveal": 4, "s_to_resolve": 8, "skips_per_idle_day": 6},
        "targets_game_secs_p99": {"round_to_anchor": 5, "s_to_first_cache": 5, "anchor_to_last_reveal": 30, "s_to_resolve": 60},
        "definition": "round -> anchor, S -> first cache and S -> resolve in *_slots: landing slot minus the first slot whose Clock is at or after round_time + drand delay (publication), judged at 20x; *_game_secs: game seconds from publication to the landing slot's Clock, judged at 2x. Anchor -> last valid reveal from THE anchor's A; close -> resolve from A + W, reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so criterion 3's close -> resolve target is judged on S -> resolve (W5-B F5, pinned by W6-A; integ-W6)",
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
    catch_up_with(
        txs,
        program,
        genesis_ts,
        bell_secs,
        u32::MAX,
        &std::collections::BTreeSet::new(),
    )
}

/// Bells a SkipQuiet batch spans at most (the keeper's `SKIP_MAX`): a
/// resident action at bell `b` can only have split a batch that started at
/// `b − SPLIT_REACH` or later (integ-W6t review, §13.4 A1 as amended).
pub const SPLIT_REACH: u32 = 26;

/// The instructions that need their Province resolved through `b − 2` and
/// so make a keeper catch it up at once (a nudge): §5.1 resident actions
/// (integ-W6t review: 49 of the 58 short batches of `w6-s7`'s A1-idle days
/// over 6 were a bot's nudge before an Explore).
pub const RESIDENT_IXS: &[Ix] = &[
    Ix::Muster,
    Ix::Dissolve,
    Ix::Garrison,
    Ix::Explore,
    Ix::Depart,
];

/// The days a resident action or a nudge of a province at `bell` can have
/// added a SkipQuiet split to: the day of `bell` and of the batch start it
/// cut (`bell − SPLIT_REACH`).
pub fn split_days(bell: u32) -> [u32; 2] {
    [bell.saturating_sub(SPLIT_REACH) / 144, bell / 144]
}

/// [`catch_up`] with the season's `end_bell` and the keepers' nudges
/// (integ-W6t review, §13.4 A1 as amended in v1.13):
///
/// - a province-day with a **resident action** (a Muster, Dissolve,
///   Garrison, Explore or Depart transaction naming the Province, landed or
///   refused) or a **nudge** a keeper served for it, on that day or within
///   the batch it could have cut ([`split_days`]), is `resident`: each nudge
///   costs one SkipQuiet split by design, so it is reported, not judged;
/// - the SkipQuiet that reaches `end_bell` (`b0 + n ≥ end_bell`, the
///   season-end flush) is not counted (reported as `season_end_flush`).
pub fn catch_up_with(
    txs: &[TxRecord],
    program: &Address,
    genesis_ts: i64,
    bell_secs: i64,
    end_bell: u32,
    nudges: &std::collections::BTreeSet<(i32, i32, u32)>,
) -> Value {
    use std::collections::BTreeSet;
    let mut epoch: HashMap<(i32, i32), u32> = HashMap::new();
    let mut churned: BTreeSet<(i32, i32, u32)> = BTreeSet::new();
    let mut skips: BTreeMap<(i32, i32, u32), u64> = BTreeMap::new();
    let mut active: BTreeSet<(i32, i32, u32)> = BTreeSet::new();
    let mut resident: BTreeSet<(i32, i32, u32)> = BTreeSet::new();
    let mut flush: BTreeMap<(i32, i32, u32), u64> = BTreeMap::new();
    let mut pv_at: HashMap<Address, (i32, i32)> = HashMap::new();
    let mut with_post = 0usize;
    let n_off = field(Kind::SKIP, "n").unwrap_or(12);
    let mut order: Vec<&TxRecord> = txs.iter().collect();
    order.sort_by_key(|t| t.seq);
    let bell_of = |t: &TxRecord| ((t.block_time - genesis_ts).max(0) / bell_secs.max(1)) as u32;
    for t in order.iter().filter(|t| t.err.is_none()) {
        let mut skip_day: HashMap<(i32, i32), u32> = HashMap::new();
        for b in fclient::log::bodies_from_logs(&t.logs, program).unwrap_or_default() {
            let Ok(r) = Record::decode_with(&b, &lens) else {
                continue;
            };
            if r.kind == Kind::SKIP as u8 {
                let kp = &r.key_payload;
                let (p, q, b0) = (le_i32(kp, 0), le_i32(kp, 4), le_u32(kp, 8));
                let n = kp.get(n_off).copied().unwrap_or(0) as u32;
                if b0.saturating_add(n) >= end_bell {
                    *flush.entry((p, q, b0 / 144)).or_default() += 1;
                } else {
                    *skips.entry((p, q, b0 / 144)).or_default() += 1;
                }
                skip_day.insert((p, q), b0 / 144);
            } else if r.kind == Kind::GATHER as u8 || r.kind == Kind::CLASH as u8 {
                // §13.4 A1: a day with a GATHER or CLASH is not idle.
                let kp = &r.key_payload;
                let (p, q, bell) = (le_i32(kp, 0), le_i32(kp, 4), le_u32(kp, 8));
                active.insert((p, q, bell / 144));
            }
        }
        let landing_day = bell_of(t) / 144;
        for (key, a) in &t.post {
            let Some(a) = a else { continue };
            if a.data.len() != fclient::abi::size::PROVINCE {
                continue;
            }
            let Ok(pv) = Province::decode(&a.data) else {
                continue;
            };
            with_post += 1;
            let k = (pv.p as i32, pv.q as i32);
            pv_at.insert(*key, k);
            let prev = epoch.insert(k, pv.roster_epoch);
            if prev.is_some_and(|e| e != pv.roster_epoch) {
                let d = skip_day.get(&k).copied().unwrap_or(landing_day);
                churned.insert((k.0, k.1, d));
            }
        }
    }
    // Resident actions, landed or refused (a refused one was still a
    // player acting there; the Province is named by its account key).
    let tags: Vec<u8> = RESIDENT_IXS.iter().map(|ix| *ix as u8).collect();
    let mut resident_actions = 0usize;
    for t in &order {
        let Some(tag) = tag_of(t, program) else {
            continue;
        };
        if !tags.contains(&tag) {
            continue;
        }
        let Ok(tx) = fclient::tx::from_wire(&t.tx) else {
            continue;
        };
        let bell = bell_of(t);
        for k in &tx.message.account_keys {
            if let Some(&(p, q)) = pv_at.get(k) {
                resident_actions += 1;
                for d in split_days(bell) {
                    resident.insert((p, q, d));
                }
            }
        }
    }
    for &(p, q, bell) in nudges {
        for d in split_days(bell) {
            resident.insert((p, q, d));
        }
    }
    let seen: BTreeSet<(i32, i32)> = epoch.keys().copied().collect();
    let mut v = classify_days_with(&skips, &churned, &active, &resident, &seen);
    v["province_post_states"] = json!(with_post);
    v["resident_actions"] = json!(resident_actions);
    v["nudges_served"] = json!(nudges.len());
    v["season_end_flush"] = json!(flush.values().sum::<u64>());
    v
}

/// The keepers' served nudges `(P, Q, bell)` from their per-bell status
/// samples (`status.play.nudges_recent`, integ-W6t review): the union over
/// every sample of both keepers (each sample lists the last two days').
pub fn nudges_from_samples(samples: &[Value]) -> std::collections::BTreeSet<(i32, i32, u32)> {
    let mut out = std::collections::BTreeSet::new();
    for s in samples {
        for n in s["status"]["play"]["nudges_recent"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let (Some(p), Some(q), Some(b)) = (n[0].as_i64(), n[1].as_i64(), n[2].as_u64()) {
                out.insert((p as i32, q as i32, b as u32));
            }
        }
    }
    out
}

/// [`classify_days_with`] without resident days (W6T-4's classes).
pub fn classify_days(
    skips: &BTreeMap<(i32, i32, u32), u64>,
    churned: &std::collections::BTreeSet<(i32, i32, u32)>,
    active: &std::collections::BTreeSet<(i32, i32, u32)>,
    seen: &std::collections::BTreeSet<(i32, i32)>,
) -> Value {
    classify_days_with(
        skips,
        churned,
        active,
        &std::collections::BTreeSet::new(),
        seen,
    )
}

/// Province-days by class for criterion 3's catch-up (W6T-4, §13.4 A1 as
/// amended by the integ-W6t review): **churned** (the roster epoch moved),
/// **active** (roster unchanged but the province had a GATHER or CLASH that
/// day: each resolved bell needs its own SkipQuiet split), **resident** (a
/// resident action or a served nudge: each nudge is one split by design),
/// else **idle** (judged: ≤ 6); a province never seen in a post-state is
/// not judged.
pub fn classify_days_with(
    skips: &BTreeMap<(i32, i32, u32), u64>,
    churned: &std::collections::BTreeSet<(i32, i32, u32)>,
    active: &std::collections::BTreeSet<(i32, i32, u32)>,
    resident: &std::collections::BTreeSet<(i32, i32, u32)>,
    seen: &std::collections::BTreeSet<(i32, i32)>,
) -> Value {
    let (mut idle, mut churn, mut act, mut res, mut unknown) =
        (vec![], vec![], vec![], vec![], 0usize);
    let mut idle_over: Vec<String> = vec![];
    let mut churn_over: Vec<String> = vec![];
    let mut act_over: Vec<String> = vec![];
    let mut res_over: Vec<String> = vec![];
    for ((p, q, d), n) in skips {
        let k = (*p, *q, *d);
        let (bucket, over) = if !seen.contains(&(*p, *q)) {
            unknown += 1;
            continue;
        } else if churned.contains(&k) {
            (&mut churn, &mut churn_over)
        } else if active.contains(&k) {
            (&mut act, &mut act_over)
        } else if resident.contains(&k) {
            (&mut res, &mut res_over)
        } else {
            (&mut idle, &mut idle_over)
        };
        bucket.push(*n as f64);
        if *n > 6 {
            over.push(format!("({p},{q}) day {d}: {n}"));
        }
    }
    let all: Vec<f64> = skips.values().map(|n| *n as f64).collect();
    json!({
        "skip_txs_per_province_day": stats(&all),
        "skip_txs_per_idle_province_day": stats(&idle),
        "skip_txs_per_churned_province_day": stats(&churn),
        "skip_txs_per_active_province_day": stats(&act),
        "skip_txs_per_resident_province_day": stats(&res),
        "idle_province_days_over_6": idle_over,
        "churned_province_days_over_6": churn_over,
        "active_province_days_over_6": act_over,
        "resident_province_days_over_6": res_over,
        "province_days_not_judged": unknown,
        "definition": "idle = the Province's roster_epoch unchanged over the day (post-states), no GATHER or CLASH of the province that day (§13.4 A1), and no resident action (Muster/Dissolve/Garrison/Explore/Depart naming it, landed or refused) or keeper-served nudge of it within the day or the 26 bells before (A1 as amended, v1.13); active = roster unchanged with a GATHER/CLASH (reported); resident = with a resident action or nudge (reported: each nudge is one split); churned = roster moved (reported); a SkipQuiet's change counts for its b0's day; the SkipQuiet reaching end_bell (the season-end flush) is not counted",
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
    // W6T-4: the last answered sample (w6-s7's last one was null, so every
    // field read null), and the unanswered ones counted.
    let last = samples
        .iter()
        .rev()
        .map(|s| &s["status"])
        .find(|s| s.is_object());
    let unanswered: Vec<&Value> = samples
        .iter()
        .filter(|s| !s["status"].is_object())
        .collect();
    let timeouts = unanswered
        .iter()
        .filter(|s| s["error"].as_str().is_some_and(|e| e.contains("timeout")))
        .count();
    let ms: Vec<f64> = samples.iter().filter_map(|s| s["ms"].as_f64()).collect();
    json!({"samples": samples.len(), "min_reveal_effective_n_in_play": min_n,
        "effective_n_ok": min_n.is_some_and(|n| n >= 150), "bells_below_150": low_bells,
        "status_unanswered": unanswered.len(), "status_timeouts": timeouts,
        "status_unanswered_bells": unanswered.iter().filter_map(|s| s["bell"].as_i64()).collect::<Vec<_>>(),
        "status_ms": stats(&ms),
        "last": last.map(|l| json!({"bell": l["bell"], "alerts": l["alerts"], "spend_by_day": l["spend_by_day"],
            "pools": l["pools"], "anchor_latency_slots_p99": l["duties"]["anchor_latency_slots_p99"],
            "seed_latency_slots_p99": l["duties"]["seed_latency_slots_p99"], "provinces_opened": l["duties"]["provinces_opened"],
            "archived_bells": l["duties"]["archived_bells"], "rings_complete": l["duties"]["rings_complete"],
            "tickets_open": l["duties"]["tickets_open"], "sweeps_sent": l["duties"]["sweeps_sent"],
            "contested_bells": l["duties"]["contested_bells"].as_array().map(|a| a.len())}))})
}

/// The per-bell load average (`metrics/loadavg.jsonl`, W6T-4): the
/// distribution of the 1-minute figure over the bells, its maximum per game
/// day, and the series itself.
pub fn loadavg_summary(samples: &[Value]) -> Value {
    let l1: Vec<f64> = samples.iter().filter_map(|s| s["load1"].as_f64()).collect();
    let mut by_day: BTreeMap<i64, f64> = BTreeMap::new();
    let mut worst: Option<(f64, i64)> = None;
    for s in samples {
        let (Some(b), Some(x)) = (s["bell"].as_i64(), s["load1"].as_f64()) else {
            continue;
        };
        let e = by_day.entry(b.div_euclid(144)).or_insert(x);
        *e = e.max(x);
        if worst.is_none_or(|(w, _)| x > w) {
            worst = Some((x, b));
        }
    }
    json!({"bells": l1.len(), "load1": stats(&l1),
        "load5": stats(&samples.iter().filter_map(|s| s["load5"].as_f64()).collect::<Vec<_>>()),
        "max_by_day": by_day.values().collect::<Vec<_>>(), "worst_bell": worst.map(|w| w.1),
        "series": samples.iter().map(|s| json!([s["bell"], s["load1"]])).collect::<Vec<_>>()})
}

/// Windows of at least `min_slots` consecutive slots in which no
/// transaction landed **after a bell started** (W6T-4; w6-s7 had one of 202
/// slots, bells 8-10): every bell start owes 16 anchors within slots, so
/// no landing for `min_slots` slots after one is a stall, while a quiet
/// stretch inside a bell is normal (counted, not listed). From the verify
/// input's landed transactions after genesis, each with the chaos kills or
/// crashes of the chain inside it (a killed localnet makes no slots, so a
/// gap there has its reason). Bells from `end_bell` on owe no anchor (the
/// drain), so their starts are not stalls (counted separately).
#[allow(clippy::too_many_arguments)]
pub fn no_landing_windows(
    txs: &[TxRecord],
    genesis_ts: i64,
    bell_secs: i64,
    slot_secs: f64,
    min_slots: u64,
    end_bell: i64,
    events: &[Value],
) -> Value {
    let mut pts: Vec<(u64, i64)> = txs
        .iter()
        .filter(|t| t.err.is_none() && t.block_time >= genesis_ts)
        .map(|t| (t.slot, t.block_time))
        .collect();
    pts.sort_unstable();
    pts.dedup_by_key(|p| p.0);
    let bs = bell_secs.max(1);
    let bell = |t: i64| (t - genesis_ts).max(0) / bs;
    let mut out = vec![];
    let (mut quiet, mut drain) = (0usize, 0usize);
    for w in pts.windows(2) {
        let ((s0, t0), (s1, t1)) = (w[0], w[1]);
        let empty = s1 - s0 - 1;
        if empty < min_slots {
            continue;
        }
        // The first bell start inside the window, and the slots from it to
        // the next landing.
        let k = (t0 - genesis_ts).div_euclid(bs) + 1;
        let start = genesis_ts + k * bs;
        let after = if start <= t1 {
            ((t1 - start) as f64 / slot_secs.max(1e-9)).floor() as u64
        } else {
            0
        };
        if after < min_slots {
            quiet += 1;
            continue;
        }
        if k >= end_bell {
            drain += 1;
            continue;
        }
        let why: Vec<String> = events
            .iter()
            .filter(|e| {
                (e["event"] == "chaos-kill" || e["event"] == "crash")
                    && e["detail"]["component"] == "localnet"
                    && e["game"].as_i64().is_some_and(|g| g >= t0 && g <= t1)
            })
            .map(|e| format!("{} localnet", f(&e["event"])))
            .collect();
        out.push(json!({"from_slot": s0 + 1, "to_slot": s1 - 1, "slots": empty,
            "slots_after_bell_start": after, "bell_started": k,
            "from_bell": bell(t0), "to_bell": bell(t1), "game_from": t0, "game_to": t1, "explained_by": why}));
    }
    json!({"min_slots": min_slots, "windows": out, "quiet_windows_inside_a_bell": quiet,
        "drain_windows_after_end_bell": drain})
}

/// Whether the run is exit-grade as an environment (W6T-4, §13.4): every
/// adversary hold fired (a `hold-skipped` hold means the schedule was not
/// exercised), the release `.so` pinned, real rounds.
pub fn exit_grade(events: &[Value], config: &Value) -> Value {
    let mut reasons: Vec<String> = vec![];
    if config["adversary"] == true {
        for e in events.iter().filter(|e| e["event"] == "hold-skipped") {
            reasons.push(format!(
                "hold-skipped {} ({})",
                f(&e["detail"]["kind"]),
                f(&e["detail"]["why"])
            ));
        }
    } else {
        reasons.push("no adversary schedule".into());
    }
    if !config["expect_so_sha256"].is_string() {
        reasons.push("no release .so pin (--expect-so-sha256)".into());
    }
    if config["beacon"] != "archive" {
        reasons.push(format!("beacon {} (not real rounds)", f(&config["beacon"])));
    }
    json!({"exit_grade": reasons.is_empty(), "reasons": reasons})
}

/// The by-rule unrevealed marches (verify V5 `unrevealed_by_rule`) by
/// reason (W6T-4, §13.4 A2).
pub fn by_rule_reasons(by_rule: &Value) -> BTreeMap<String, u64> {
    let mut m: BTreeMap<String, u64> = BTreeMap::new();
    for x in by_rule.as_array().into_iter().flatten() {
        let r =
            x["reason"]
                .as_str()
                .map(String::from)
                .unwrap_or_else(|| match x["outcome"].as_u64() {
                    Some(o) => format!("outcome {o} (no reason: pre-W6T-3 verifier)"),
                    None => "unknown".into(),
                });
        *m.entry(r).or_default() += 1;
    }
    m
}

/// The §5.11 step-6 reasons (a Reveal refused by rule); `bounced` is a
/// settlement outcome, not a refusal.
pub const STEP6_REASONS: &[&str] = &["shielded-own", "shielded-dest", "path", "arrival-bell"];

/// By-rule refusals of honest marches (§13.4 A2: each is a criterion-5
/// persona violation): a step-6 reason whose march the bots' report lists
/// with an honest persona (none, `honest`, or an archetype `arch:*`), or
/// that it does not list at all (the fleet is honest but for its
/// personas).
pub fn honest_rule_refusals(by_rule: &Value, bots_unrevealed: &Value) -> Vec<String> {
    let host = |v: &Value| -> Option<u64> {
        match v {
            Value::Number(n) => n.as_u64(),
            Value::String(s) => match s.strip_prefix("0x") {
                Some(h) => u64::from_str_radix(h, 16).ok(),
                None => s.parse().ok(),
            },
            _ => None,
        }
    };
    let persona_of: HashMap<(u64, u64), String> = bots_unrevealed
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|b| {
            Some((
                (host(&b["host"])?, b["arrive"].as_u64()?),
                // A garbage or bad-plaintext seal is persona traffic even
                // without a persona name (W6T-3's `seal` field).
                match b["seal"].as_str() {
                    Some(s) if s != "honest" => format!("seal:{s}"),
                    _ => b["persona"].as_str().unwrap_or("").to_string(),
                },
            ))
        })
        .collect();
    let honest = |p: &str| p.is_empty() || p == "honest" || p.starts_with("arch:");
    let mut out = vec![];
    for x in by_rule.as_array().into_iter().flatten() {
        let reason = x["reason"].as_str().unwrap_or("");
        if !STEP6_REASONS.contains(&reason) {
            continue;
        }
        let (Some(h), Some(a)) = (host(&x["host_id"]), x["arrive_bell"].as_u64()) else {
            continue;
        };
        let p = persona_of.get(&(h, a)).map(String::as_str).unwrap_or("");
        if honest(p) {
            out.push(format!(
                "{h}@{a} {reason}{}",
                if persona_of.contains_key(&(h, a)) {
                    format!(" ({})", if p.is_empty() { "honest" } else { p })
                } else {
                    " (not in the bots' list)".into()
                }
            ));
        }
    }
    out
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
           "holds_rearmed": count("hold-rearmed"), "holds_rearm_expired": count("hold-rearm-expired"),
           "holds": holds, "end_season": ev.iter().find(|e| e["event"] == "end-season").map(|e| e["detail"].clone())})
}

/// Personas whose expected outcome the run did not show (integ-W6t
/// review, §13.4 criterion 5): `pending` ones, and chain-judged
/// (`needs-chain`) ones that never did the thing their outcome is about
/// (a landed Depart, a prefund, a ticket or hold); a locally checkable
/// persona left `needs-chain` was refused with a code the contract does
/// not name (listed with its results).
pub fn persona_gaps(personas: &Value) -> Vec<String> {
    let ok = |p: &Value, action: &str| -> u64 {
        p["results"]
            .as_object()
            .map(|m| {
                m.iter()
                    .filter(|(k, _)| k.starts_with(&format!("{action}@")) && k.ends_with(":ok"))
                    .filter_map(|(_, v)| v.as_u64())
                    .sum()
            })
            .unwrap_or(0)
    };
    let mut gaps = vec![];
    for p in personas.as_array().into_iter().flatten() {
        let name = p["persona"].as_str().unwrap_or("?");
        match p["verdict"].as_str().unwrap_or("") {
            "observed" | "violated" => {}
            "pending" => gaps.push(format!("{name} pending (never reached its test)")),
            "needs-chain" => {
                let exercised = match name {
                    "min_tip" | "garbage_seal" | "bad_plaintext" | "squatter" | "self_tip" => {
                        ok(p, "depart") >= 1
                    }
                    "double_arrival" => ok(p, "depart") >= 2,
                    "prefunder" => ok(p, "prefund") >= 1,
                    "ticket_holder" => ok(p, "file_ticket") + ok(p, "hold") >= 1,
                    // Locally checkable: refused, but with a code its rule
                    // does not name.
                    "settle_racer" | "late_revealer" | "forger" | "spammer" | "zero_tip" => {
                        gaps.push(format!(
                            "{name} refused with a code its rule does not name ({})",
                            p["results"]
                        ));
                        continue;
                    }
                    _ => false,
                };
                if !exercised {
                    gaps.push(format!("{name} not exercised ({})", p["results"]));
                }
            }
            other => gaps.push(format!("{name} verdict {other:?}")),
        }
    }
    if personas.as_array().is_none_or(|a| a.is_empty()) {
        gaps.push("no persona in the bots' report".into());
    }
    gaps
}

/// The bots' reports of every fleet lifetime merged (integ-W6t review: a
/// chaos-killed `frontier-bots` starts a new report; `w6-s7`'s final
/// report covered 2,107 of the season's steps). `frontier-bots` keeps each
/// earlier lifetime's report as `report-life-<n>.json` next to
/// `report.json`. Counts add up; a persona's verdict is the strongest seen
/// (violated > observed > needs-chain > pending); unrevealed marches are
/// kept once per (host, arrive), the latest lifetime's entry winning.
pub fn merge_bots_reports(reports: &[Value]) -> Value {
    fn add(into: &mut Value, from: &Value) {
        match (into, from) {
            (Value::Object(a), Value::Object(b)) => {
                for (k, v) in b {
                    add(a.entry(k.clone()).or_insert(Value::Null), v);
                }
            }
            (a @ Value::Null, b) => *a = b.clone(),
            (a, b) if a.is_u64() && b.is_u64() => {
                *a = json!(a.as_u64().unwrap_or(0) + b.as_u64().unwrap_or(0))
            }
            _ => {}
        }
    }
    let rank = |v: &str| match v {
        "violated" => 3,
        "observed" => 2,
        "needs-chain" => 1,
        _ => 0,
    };
    let reports: Vec<&Value> = reports.iter().filter(|r| r.is_object()).collect();
    if reports.is_empty() {
        return Value::Null;
    }
    let (mut groups, mut errors, mut nudges) = (json!({}), json!({}), json!({}));
    let mut personas: BTreeMap<String, Value> = BTreeMap::new();
    let mut unrevealed: BTreeMap<(String, u64), Value> = BTreeMap::new();
    let (mut steps, mut bots) = (0u64, 0u64);
    for r in &reports {
        add(&mut groups, &r["groups"]);
        add(&mut errors, &r["errors"]);
        add(&mut nudges, &r["nudges"]);
        steps += r["steps"].as_u64().unwrap_or(0);
        bots = bots.max(r["bots"].as_u64().unwrap_or(0));
        for p in r["personas"].as_array().into_iter().flatten() {
            let name = p["persona"].as_str().unwrap_or("?").to_string();
            match personas.get_mut(&name) {
                None => {
                    personas.insert(name, p.clone());
                }
                Some(have) => {
                    let mut res = have["results"].clone();
                    add(&mut res, &p["results"]);
                    have["results"] = res;
                    let (a, b) = (
                        have["verdict"].as_str().unwrap_or("").to_string(),
                        p["verdict"].as_str().unwrap_or(""),
                    );
                    if rank(b) > rank(&a) {
                        have["verdict"] = json!(b);
                    }
                }
            }
        }
        for u in r["unrevealed"].as_array().into_iter().flatten() {
            let host = match &u["host"] {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            unrevealed.insert((host, u["arrive"].as_u64().unwrap_or(0)), u.clone());
        }
    }
    let unrevealed: Vec<Value> = unrevealed.into_values().collect();
    let honest = unrevealed.iter().filter(|u| u["seal"] == "honest").count();
    json!({
        "v": 1, "lifetimes": reports.len(), "bots": bots, "steps": steps, "groups": groups,
        "personas": personas.into_values().collect::<Vec<_>>(), "errors": errors, "nudges": nudges,
        "unrevealed": unrevealed, "unrevealed_honest": honest,
    })
}

/// `bots/report.json` and the earlier lifetimes' `report-life-<n>.json`,
/// merged ([`merge_bots_reports`]).
pub fn read_bots_reports(dir: &std::path::Path) -> Value {
    let mut lives: Vec<(u64, Value)> = std::fs::read_dir(dir)
        .map(|d| {
            d.filter_map(|e| e.ok())
                .filter_map(|e| {
                    let n = e.file_name().to_string_lossy().into_owned();
                    let k = n.strip_prefix("report-life-")?.strip_suffix(".json")?;
                    Some((k.parse().ok()?, read_json(&e.path())))
                })
                .collect()
        })
        .unwrap_or_default();
    lives.sort_by_key(|x| x.0);
    let mut all: Vec<Value> = lives.into_iter().map(|x| x.1).collect();
    all.push(read_json(&dir.join("report.json")));
    merge_bots_reports(&all)
}

/// The fleet's bad-seal transits (integ-W6t review, §13.4 criterion 8):
/// the marchbooks' `sealed` lines of kind `garbage` or `bad_plaintext`
/// whose Depart was `sent` and not `failed`, as `(host, arrive, kind)`.
pub fn bad_seal_marches(bots_dir: &std::path::Path) -> Option<Vec<(u64, u32, String)>> {
    use std::collections::BTreeSet;
    let rd = std::fs::read_dir(bots_dir).ok()?;
    let mut sealed: BTreeMap<(u64, u64), (u32, String)> = BTreeMap::new();
    let mut sent: BTreeSet<(u64, u64)> = BTreeSet::new();
    let mut failed: BTreeSet<(u64, u64)> = BTreeSet::new();
    let mut books = 0;
    for e in rd.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if !(n.starts_with("marchbook-") && n.ends_with(".jsonl")) {
            continue;
        }
        books += 1;
        let Ok(text) = std::fs::read_to_string(e.path()) else {
            continue;
        };
        for l in text.lines() {
            let Ok(v) = serde_json::from_str::<Value>(l) else {
                continue;
            };
            let host = match &v["host"] {
                Value::String(s) => s.parse::<u64>().ok(),
                x => x.as_u64(),
            };
            let (Some(h), Some(d)) = (host, v["depart_bell"].as_u64()) else {
                continue;
            };
            match v["ev"].as_str() {
                Some("sealed") => {
                    let kind = v["kind"].as_str().unwrap_or("honest");
                    if kind != "honest" {
                        if let Some(a) = v["arrive_bell"].as_u64() {
                            sealed.insert((h, d), (a as u32, kind.to_string()));
                        }
                    }
                }
                Some("sent") => {
                    sent.insert((h, d));
                }
                Some("failed") => {
                    failed.insert((h, d));
                }
                _ => {}
            }
        }
    }
    if books == 0 {
        return None;
    }
    Some(
        sealed
            .into_iter()
            .filter(|(k, _)| sent.contains(k) && !failed.contains(k))
            .map(|((h, _), (a, kind))| (h, a, kind))
            .collect(),
    )
}

/// Each transit's settlement `(host, arrive) → (outcome, seal code)`,
/// matched as [`outcomes`] does (a host's TRANSIT_SETTLED settles its
/// earliest unsettled DEPART).
pub fn settles_by_march(recs: &[Rec]) -> BTreeMap<(u64, u32), (u8, u8)> {
    let mut open: BTreeMap<u64, Vec<(u32, bool)>> = BTreeMap::new();
    let mut out = BTreeMap::new();
    for x in recs {
        let kp = &x.r.key_payload;
        match Kind::from_u8(x.r.kind) {
            Some(Kind::DEPART) => {
                let arrive = le_u32(kp, field(Kind::DEPART, "arrive_bell").unwrap_or(21));
                open.entry(le_u64(kp, 0)).or_default().push((arrive, false));
            }
            Some(Kind::TRANSIT_SETTLED) => {
                let host = le_u64(kp, 0);
                if let Some(d) = open
                    .get_mut(&host)
                    .and_then(|v| v.iter_mut().find(|d| !d.1))
                {
                    d.1 = true;
                    out.insert((host, d.0), (kp[8], kp[9]));
                }
            }
            _ => {}
        }
    }
    out
}

/// Criterion 8's facts from the marchbooks and the settlements.
pub fn bad_seal_facts(
    marches: Option<Vec<(u64, u32, String)>>,
    settles: &BTreeMap<(u64, u32), (u8, u8)>,
    last_bell: u32,
) -> Value {
    let Some(ms) = marches else {
        return Value::Null;
    };
    let mut due: BTreeMap<String, u64> = BTreeMap::new();
    let mut codes: BTreeMap<String, u64> = BTreeMap::new();
    let mut not_bad = vec![];
    for (h, a, kind) in ms {
        if a + 3 > last_bell {
            continue;
        }
        *due.entry(kind.clone()).or_default() += 1;
        let want5 = kind == "bad_plaintext";
        match settles.get(&(h, a)) {
            Some(&(o, c))
                if o == frontier_abi::log::transit_outcome::BAD_SEAL
                    && c != 0
                    && (!want5 || c == frontier_abi::log::seal_code::BAD_PLAINTEXT) =>
            {
                *codes.entry(format!("{kind}:{c}")).or_default() += 1;
            }
            Some(&(o, c)) => not_bad.push(format!("{kind} {h:#x}@{a} outcome {o} seal {c}")),
            None => not_bad.push(format!("{kind} {h:#x}@{a} unsettled")),
        }
    }
    json!({"due": due, "codes": codes, "not_bad_seal": not_bad})
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
        // The close → resolve target is judged on S → resolve (integ-W6:
        // the resolve needs the seed, public `seed_margin` after the
        // close; see [`latencies`]); close → resolve itself is reported.
        let (checks, close_key): ([(&str, f64, &str); 4], &str) = if slots_mode {
            (
                [
                    ("round_to_anchor_slots", 2.0, "slots"),
                    ("s_to_first_cache_slots", 2.0, "slots"),
                    ("anchor_to_last_reveal_slots", 4.0, "slots"),
                    ("s_to_resolve_slots", 8.0, "slots"),
                ],
                "close_to_resolve_slots",
            )
        } else {
            (
                [
                    ("round_to_anchor_game_secs", 5.0, "s"),
                    ("s_to_first_cache_game_secs", 5.0, "s"),
                    ("anchor_to_last_reveal_game_secs", 30.0, "s"),
                    ("s_to_resolve_game_secs", 60.0, "s"),
                ],
                "close_to_resolve_game_secs",
            )
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
        if let Some(x) = p99(close_key) {
            seen.push(format!(
                "{close_key} p99 {x:.2} reported (the resolve waits for S(b, r), public seed_margin after the close)"
            ));
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
    // W6T-4 (§13.4 A2): valid seals whose Reveal the program refuses by
    // rule are listed by reason, not as liveness misses.
    let reasons = by_rule_reasons(&facts["unrevealed_by_rule"]);
    if !reasons.is_empty() {
        notes.push(format!(
            "unrevealed by rule: {}",
            reasons
                .iter()
                .map(|(r, n)| format!("{r} {n}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
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
    // 5 — personas; W6T-4 (§13.4 A2): a by-rule refusal of an honest
    // march is a persona violation.
    let violated = len(&facts["violated"]);
    let honest = honest_rule_refusals(&facts["unrevealed_by_rule"], &facts["bots_unrevealed"]);
    v.push(if facts["personas"].is_null() && honest.is_empty() {
        verdict("5", "n.a.", "no bots report")
    } else if violated > 0 || !honest.is_empty() {
        let mut why = vec![];
        if violated > 0 {
            why.push(format!("violated: {}", facts["violated"]));
        }
        if !honest.is_empty() {
            why.push(format!(
                "{} honest marches refused by rule (§13.4 A2): {}",
                honest.len(),
                honest.join(", ")
            ));
        }
        verdict("5", "fail", why.join("; "))
    } else {
        // integ-W6t review: §13.4 needs every persona's expected outcome
        // observed; a persona still `pending`, or one only the chain can
        // judge that never acted (no transit, no prefund, no ticket), was
        // not exercised: the criterion is not decided (n.a.), not passed.
        let gaps = persona_gaps(&facts["personas"]);
        if gaps.is_empty() {
            verdict(
                "5",
                "pass",
                "no persona violated; no honest march refused by rule; every persona observed or exercised (the chain-judged ones by verify and criteria 4, 8, 9)",
            )
        } else {
            verdict(
                "5",
                "n.a.",
                format!(
                    "no persona violated, but not every expected outcome was observed: {}",
                    gaps.join("; ")
                ),
            )
        }
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
    // integ-W6t review: every garbage / bad-plaintext transit the fleet
    // sent (the bots' marchbooks) and that is due must have settled as
    // bad-seal (bad plaintext with code 5); a run with none of either kind
    // did not exercise the criterion (n.a., not pass).
    let bs = &facts["bad_seal_marches"];
    for x in bs["not_bad_seal"].as_array().into_iter().flatten() {
        bad.push(format!("not settled as bad-seal: {}", f(x)));
    }
    let missing: Vec<&str> = ["garbage", "bad_plaintext"]
        .into_iter()
        .filter(|k| bs["due"][k].as_u64().unwrap_or(0) == 0)
        .collect();
    v.push(if facts["verify_verdict"].is_null() {
        verdict("8", "n.a.", "no verify report")
    } else if !bad.is_empty() {
        verdict("8", "fail", bad.join("; "))
    } else if bs.is_null() {
        verdict(
            "8",
            "n.a.",
            "no bad seal survived, but the bots' marchbooks were not read (the bad-seal transits are unknown)",
        )
    } else if !missing.is_empty() {
        verdict(
            "8",
            "n.a.",
            format!(
                "no bad seal survived, but no {} transit was sent and due: the bad-seal personas were not exercised",
                missing.join(" or ")
            ),
        )
    } else {
        verdict(
            "8",
            "pass",
            format!(
                "no bad seal survived; {} garbage and {} bad-plaintext transits sent and due, each settled as bad-seal (codes {})",
                f(&bs["due"]["garbage"]),
                f(&bs["due"]["bad_plaintext"]),
                f(&bs["codes"])
            ),
        )
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
    // E — the §13.4 environment (W6T-4): not a criterion, never an exit
    // failure by itself; a skipped hold makes the run not exit-grade.
    let eg = &facts["exit_grade"];
    v.push(if eg.is_null() {
        verdict("E", "n.a.", "environment not read")
    } else if eg["exit_grade"] == true {
        verdict(
            "E",
            "exit-grade",
            "every adversary hold fired; release .so pinned; real rounds",
        )
    } else {
        verdict("E", "not exit-grade", f(&eg["reasons"]))
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
        frontier_abi::presets::M1_LOCAL_7D.seed_margin,
    );
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
    let bots = read_bots_reports(&rd.path("bots"));
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
    let mut samples = read_jsonl(&rd.path("metrics/keeper-a.jsonl"));
    samples.extend(read_jsonl(&rd.path("metrics/keeper-b.jsonl")));
    let catch = catch_up_with(
        &inp.txs,
        &program,
        genesis,
        bell_secs,
        end_bell,
        &nudges_from_samples(&samples),
    );
    let bsf = bad_seal_facts(
        bad_seal_marches(&rd.path("bots")),
        &settles_by_march(&recs),
        last_bell,
    );
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
        "8_bad_seals": {"bad_seal_codes": out["bad_seal_codes"], "BadSealSurvived": bad_seal_survived, "marchbook": bsf},
        "9_tickets": {"cohorts_open_past_24_bells": pc["cohorts_open_past_24_bells"]},
    });
    let verify_report = read_json(&rd.path("verify/report.json"));
    let events = rd.events();
    let eg = exit_grade(&events, &st["config"]);
    let fails = failures(&inp.txs, &program);
    let fclasses = failure_classes(&fails);
    let load_avg = loadavg_summary(&read_jsonl(&rd.path("metrics/loadavg.jsonl")));
    let no_landing = no_landing_windows(
        &inp.txs,
        genesis,
        bell_secs,
        0.4 * scale,
        20,
        st["season"]["end_bell"].as_i64().unwrap_or(i64::MAX),
        &events,
    );
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
        "unrevealed_by_rule": verify_report["liveness"]["unrevealed_by_rule"],
        "bots_unrevealed": bots["unrevealed"], "exit_grade": eg,
        "bad_seal_marches": bsf,
    });
    let decided = decide(&facts);
    let holds = hold_effects(&inp.txs, &rd.events());
    let rep = json!({
        "format": "frontier-stack-report-v1",
        "decided": decided,
        "run_id": st["run_id"], "phase": st["phase"], "config": st["config"], "so": st["so"], "program": st["program"],
        "season": st["season"], "play": st["play"], "source": source, "txs": inp.txs.len(), "last_bell": last_bell,
        "cu": cu, "reveal_cu": stats(&reveal), "failures": fails, "failure_classes": fclasses,
        "loadavg": load_avg, "no_landing_windows": no_landing, "exit_grade": eg,
        "unrevealed_by_rule": by_rule_reasons(&verify_report["liveness"]["unrevealed_by_rule"]),
        "outcomes": out, "provinces": pc, "keepers": {"a": ka, "b": kb}, "herald": herald,
        "events": ev, "bots": {"personas": pers, "errors": bots["errors"], "bots": bots["bots"], "lifetimes": bots["lifetimes"]},
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
    let eg = &r["exit_grade"];
    if !eg.is_null() {
        m.push_str(&format!(
            "- **{}**{}\n",
            if eg["exit_grade"] == true {
                "exit-grade environment"
            } else {
                "NOT exit-grade"
            },
            if eg["exit_grade"] == true {
                String::new()
            } else {
                format!(": {}", f(&eg["reasons"]))
            }
        ));
    }
    let la = &r["loadavg"];
    if la["bells"].as_u64().unwrap_or(0) > 0 {
        m.push_str(&format!(
            "- machine load average (1 min, sampled each bell; the machine is shared): p50 {}, p99 {}, max {} at bell {}; max per game day {}\n",
            f(&la["load1"]["p50"]), f(&la["load1"]["p99"]), f(&la["load1"]["max"]), f(&la["worst_bell"]), la["max_by_day"]
        ));
    }
    m.push('\n');
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
            "- load {} ({}, {} viewers, {} game h): p99 file {} ms, error rate {} ({} errors / {} requests + WS sessions), ingest → WS p99 {} s ({}; fold lag p99 {} s), WS coverage {} outside {} outage windows, stale retries {} / WS reconnects {} (outside outages {} / {}), unavailable {} ({} ms), generator recovery {} → **{}**{}\n",
            f(&l["tag"]), f(&l["window"]), f(&l["viewers"]), f(&l["game_hours"]), f(&v["p99_file_ms"]), f(&v["error_rate"]),
            f(&v["errors"]), f(&v["denominator"]),
            f(&v["ingest_p99_s"]), f(&v["ingest_measure"]), f(&v["ingest_lag_p99_s"]),
            f(&v["ws_coverage"]), v["coverage"]["outage_windows"].as_array().map_or(0, |a| a.len()),
            f(&v["stale_retries"]), f(&v["ws_reconnects"]), f(&v["stale_retries_outside"]), f(&v["ws_reconnects_outside"]),
            f(&v["unavailable"]), f(&v["unavailable_ms"]), f(&v["generator_recovery"]),
            if v["pass"] == true { "pass" } else { "fail" },
            if v["misses"].as_array().is_some_and(|a| !a.is_empty()) { format!(" ({})", v["misses"]) } else { String::new() }
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
        "\n## Keeper latencies (a slot is {} game s)\n\nRound → anchor, S → first cache and S → resolve in slots count from the first slot whose Clock shows the round public (round time + drand delay) to the landing slot; in game seconds from the publication instant (W5-B F5, pinned by W6-A). Criterion 3: the slot targets at 20×, the game-second targets at 2×. Close → resolve is reported: the resolve needs S(b, r) = first_round_from(close + seed_margin), so its target is judged on S → resolve (integ-W6).\n\n| measure | n | p50 | p99 | max | target p99 (20×) | game s p50 | game s p99 | target p99 (2×) |\n|---|---|---|---|---|---|---|---|---|\n",
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
        ("s_to_resolve_slots", "8", "s_to_resolve_game_secs", "60 s"),
        (
            "close_to_resolve_slots",
            "reported",
            "close_to_resolve_game_secs",
            "reported",
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
        (
            "active (GATHER/CLASH, reported)",
            "skip_txs_per_active_province_day",
            "active_province_days_over_6",
        ),
        (
            "resident (resident action or nudge, reported)",
            "skip_txs_per_resident_province_day",
            "resident_province_days_over_6",
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
    m.push_str(&format!(
        "\nResident actions {} (landed or refused), keeper-served nudges {}, season-end flush SkipQuiet {} (not counted).\n",
        f(&c["resident_actions"]),
        f(&c["nudges_served"]),
        f(&c["season_end_flush"])
    ));
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
    let fc = &r["failure_classes"];
    if fc.is_object() {
        m.push_str(&format!(
            "\n### Failed transactions by class (reported, not gating)\n\n{} failed: {} (by cause {}); unclassified {}.\n\n| kind | error | n | class | cause |\n|---|---|---|---|---|\n",
            f(&fc["total"]), fc["by_class"], fc["by_cause"], f(&fc["unclassified"])
        ));
        for row in fc["rows"].as_array().cloned().unwrap_or_default() {
            m.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                f(&row["kind"]),
                f(&row["error"]).replace('|', "\\|"),
                f(&row["n"]),
                f(&row["class"]),
                f(&row["cause"])
            ));
        }
        m.push('\n');
    }
    let nl = r["no_landing_windows"]["windows"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    m.push_str(&format!(
        "- no landed transaction for ≥ 20 slots after a bell started: {} windows ({} quiet stretches inside a bell and {} drain gaps after end_bell not listed)\n",
        nl.len(),
        f(&r["no_landing_windows"]["quiet_windows_inside_a_bell"]),
        f(&r["no_landing_windows"]["drain_windows_after_end_bell"])
    ));
    for w in nl.iter().take(20) {
        m.push_str(&format!(
            "  - slots {}–{} ({} slots, {} after bell {} started; bells {}–{}){}\n",
            f(&w["from_slot"]),
            f(&w["to_slot"]),
            f(&w["slots"]),
            f(&w["slots_after_bell_start"]),
            f(&w["bell_started"]),
            f(&w["from_bell"]),
            f(&w["to_bell"]),
            if w["explained_by"].as_array().is_some_and(|a| !a.is_empty()) {
                format!(": {}", w["explained_by"])
            } else {
                ": **unexplained**".into()
            }
        ));
    }
    if !r["unrevealed_by_rule"].is_null() {
        m.push_str(&format!(
            "- valid seals unrevealed by rule (verify V5, §13.4 A2) by reason: {}\n",
            r["unrevealed_by_rule"]
        ));
    }
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
        "\n## Keepers, herald, bots\n\n- keeper A: min reveal effective N in play {} (≥ 150: {}); status unanswered in {} of {} bells ({} timeouts; answer ms p99 {}); last answered status {}\n- keeper B: min reveal effective N in play {}; status unanswered in {} of {} bells ({} timeouts); last answered status {}\n- herald: fold lag slots p99 {}, alarms {}\n- personas violated: {}\n",
        f(&k["a"]["min_reveal_effective_n_in_play"]), f(&k["a"]["effective_n_ok"]),
        f(&k["a"]["status_unanswered"]), f(&k["a"]["samples"]), f(&k["a"]["status_timeouts"]), f(&k["a"]["status_ms"]["p99"]), k["a"]["last"],
        f(&k["b"]["min_reveal_effective_n_in_play"]), f(&k["b"]["status_unanswered"]), f(&k["b"]["samples"]), f(&k["b"]["status_timeouts"]), k["b"]["last"],
        f(&r["herald"]["fold_lag_slots"]["p99"]),
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
        let lat = latencies(&recs, &clock, &drand, 1.0, 600, 60);
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
        // integ-W6: S → resolve has a sample per CLASH and is shorter than
        // close → resolve by the seed margin.
        assert_eq!(
            lat["s_to_resolve_slots"]["n"], lat["close_to_resolve_slots"]["n"],
            "{lat}"
        );
        assert!(
            lat["s_to_resolve_game_secs"]["p50"].as_f64().unwrap()
                < lat["close_to_resolve_game_secs"]["p50"].as_f64().unwrap(),
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
                "anchor_to_last_reveal_slots": {"p99": 3.0}, "close_to_resolve_slots": {"p99": 12.0},
                "s_to_resolve_slots": {"p99": 6.0},
                "round_to_anchor_game_secs": {"p99": 12.0}, "s_to_first_cache_game_secs": {"p99": 16.0},
                "anchor_to_last_reveal_game_secs": {"p99": 24.0}, "close_to_resolve_game_secs": {"p99": 96.0},
                "s_to_resolve_game_secs": {"p99": 48.0}},
            "catch_up": {"idle_province_days_over_6": [], "churned_province_days_over_6": ["(1,1) day 0: 9"],
                "skip_txs_per_idle_province_day": {"p99": 4.0, "max": 4.0}, "skip_txs_per_churned_province_day": {"p99": 9.0, "max": 9.0}},
            "clash_inputs": {"closed": 0, "open": 3, "closable_after_grace": 2, "pending": 1, "blocked": []},
            "valid_unrevealed": [{"host_id": "1", "arrive_bell": 50}],
            "above_cap_holds": [{"bell": 49, "game_secs": 600.0}],
            "personas": [{"persona": "min_tip", "verdict": "observed"}], "violated": [],
            "keeper_a": {"min_reveal_effective_n_in_play": 150, "bells_below_150": []}, "keeper_b": null,
            "loads": [{"window": "in-run", "verdict": {"pass": true, "summary": "ok"}}],
            "bad_seal_survived": false, "verify_verdict": "PASS",
            "bad_seal_marches": {"due": {"garbage": 2, "bad_plaintext": 1},
                "codes": {"garbage:2": 2, "bad_plaintext:5": 1}, "not_bad_seal": []},
        })
    }

    /// integ-W6t review: criteria 5 and 8 are not passed vacuously (R5: 490
    /// bots joined, no garbage / bad-plaintext / settle-racer transit, the
    /// personas mostly `needs-chain` or `pending`, both criteria "pass").
    #[test]
    fn criteria_5_and_8_need_their_personas_exercised() {
        let st = |f: &Value, id: &str| {
            decide(f)
                .into_iter()
                .find(|c| c["criterion"] == id)
                .unwrap()
        };
        let f = good_facts();
        assert_eq!(st(&f, "5")["status"], "pass");
        assert_eq!(st(&f, "8")["status"], "pass", "{}", st(&f, "8"));
        // R5's shape: no bad-seal transit due.
        let mut g = good_facts();
        g["bad_seal_marches"] = json!({"due": {}, "codes": {}, "not_bad_seal": []});
        let c = st(&g, "8");
        assert_eq!(c["status"], "n.a.", "{c}");
        assert!(c["why"]
            .as_str()
            .unwrap()
            .contains("garbage or bad_plaintext"));
        g["bad_seal_marches"] = json!({"due": {"garbage": 3}, "codes": {}, "not_bad_seal": []});
        assert!(st(&g, "8")["why"]
            .as_str()
            .unwrap()
            .contains("no bad_plaintext"));
        g["bad_seal_marches"] = Value::Null;
        assert_eq!(st(&g, "8")["status"], "n.a.");
        // A due bad-seal march that settled otherwise fails.
        g["bad_seal_marches"] = json!({"due": {"garbage": 1, "bad_plaintext": 1}, "codes": {},
            "not_bad_seal": ["garbage 0x5@40 outcome 1 seal 0"]});
        assert_eq!(st(&g, "8")["status"], "fail");
        // R5's personas: pending and never-departed needs-chain ones.
        let mut h = good_facts();
        h["personas"] = json!([
            {"persona": "garbage_seal", "verdict": "needs-chain", "results": {"build@relay:ok": 2, "file_ticket@relay:QuotaExceeded": 131}},
            {"persona": "settle_racer", "verdict": "pending", "results": {}},
            {"persona": "min_tip", "verdict": "needs-chain", "results": {"depart@relay:ok": 1}},
            {"persona": "zero_tip", "verdict": "observed", "results": {}},
        ]);
        let c = st(&h, "5");
        assert_eq!(c["status"], "n.a.", "{c}");
        let why = c["why"].as_str().unwrap();
        assert!(
            why.contains("garbage_seal not exercised") && why.contains("settle_racer pending"),
            "{why}"
        );
        assert!(!why.contains("min_tip"), "{why}");
        // A violation still fails.
        h["violated"] = json!(["forger"]);
        assert_eq!(st(&h, "5")["status"], "fail");
    }

    /// integ-W6t review: a restarted fleet's lifetimes merge; verdicts keep
    /// the strongest; counts add up; unrevealed marches once each.
    #[test]
    fn bots_lifetimes_merge() {
        let a = json!({"bots": 1000, "steps": 5, "groups": {"arch:bot": {"depart": {"ok": 2}}},
            "personas": [{"persona": "late_revealer", "verdict": "observed", "results": {"reveal@direct:WindowClosed": 1}},
                         {"persona": "forger", "verdict": "pending", "results": {}}],
            "errors": {"x": 1}, "nudges": {"sent": 3},
            "unrevealed": [{"host": "7", "arrive": 40, "seal": "honest", "tries": 1}]});
        let b = json!({"bots": 1000, "steps": 7, "groups": {"arch:bot": {"depart": {"ok": 1}, "muster": {"ok": 1}}},
            "personas": [{"persona": "late_revealer", "verdict": "pending", "results": {"reveal@direct:WindowClosed": 2}},
                         {"persona": "forger", "verdict": "observed", "results": {"forge@direct:BadAccount": 1}}],
            "errors": {}, "nudges": {"sent": 1, "failed": 1},
            "unrevealed": [{"host": "7", "arrive": 40, "seal": "honest", "tries": 3},
                           {"host": "9", "arrive": 41, "seal": "garbage", "tries": 0}]});
        let m = merge_bots_reports(&[a, Value::Null, b]);
        assert_eq!(m["lifetimes"], 2);
        assert_eq!(m["steps"], 12);
        assert_eq!(m["groups"]["arch:bot"]["depart"]["ok"], 3);
        assert_eq!(m["groups"]["arch:bot"]["muster"]["ok"], 1);
        assert_eq!(m["nudges"], json!({"sent": 4, "failed": 1}));
        let p = |n: &str| {
            m["personas"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["persona"] == n)
                .unwrap()
                .clone()
        };
        assert_eq!(p("late_revealer")["verdict"], "observed");
        assert_eq!(
            p("late_revealer")["results"]["reveal@direct:WindowClosed"],
            3
        );
        assert_eq!(p("forger")["verdict"], "observed");
        assert_eq!(m["unrevealed"].as_array().unwrap().len(), 2);
        assert_eq!(m["unrevealed_honest"], 1);
        assert_eq!(merge_bots_reports(&[Value::Null]), Value::Null);
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
        f["latency"]["s_to_resolve_slots"]["p99"] = json!(9.0);
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

    /// W6T-4 (§13.4 A1): a province-day with a GATHER or CLASH is not idle
    /// (w6-s7: 13 of the 36 "idle" days over 6 had 7-15 resolved bells).
    #[test]
    fn day_with_clash_is_not_idle() {
        use std::collections::BTreeSet;
        let skips: BTreeMap<(i32, i32, u32), u64> =
            [((-4, -2, 5), 15), ((1, 1, 2), 9), ((2, 2, 0), 3)]
                .into_iter()
                .collect();
        let seen: BTreeSet<(i32, i32)> = [(-4, -2), (1, 1), (2, 2)].into_iter().collect();
        let active: BTreeSet<(i32, i32, u32)> = [(-4, -2, 5)].into_iter().collect();
        let v = classify_days(&skips, &BTreeSet::new(), &active, &seen);
        assert_eq!(
            v["idle_province_days_over_6"],
            json!(["(1,1) day 2: 9"]),
            "{v}"
        );
        assert_eq!(
            v["active_province_days_over_6"],
            json!(["(-4,-2) day 5: 15"]),
            "{v}"
        );
        assert_eq!(v["skip_txs_per_active_province_day"]["n"], 1);
        assert_eq!(v["skip_txs_per_idle_province_day"]["n"], 2);
        // Churned wins over active.
        let churned: BTreeSet<(i32, i32, u32)> = [(-4, -2, 5)].into_iter().collect();
        let v = classify_days(&skips, &churned, &active, &seen);
        assert_eq!(
            v["churned_province_days_over_6"],
            json!(["(-4,-2) day 5: 15"])
        );
        assert_eq!(v["skip_txs_per_active_province_day"]["n"], 0);
        // Criterion 3 judges idle days only.
        let mut f = good_facts();
        f["catch_up"]["active_province_days_over_6"] = json!(["(-4,-2) day 5: 15"]);
        assert_eq!(decide(&f)[2]["status"], "pass");
    }

    /// integ-W6t review (§13.4 A1 as amended, v1.13): a province-day with a
    /// resident action or a served nudge within the batch it could cut is
    /// `resident` (reported), not idle; w6-s7's 24 A1-idle days over 6 had
    /// 49 short batches right before an EXPLORE (a bot's nudge).
    #[test]
    fn nudged_or_resident_day_is_not_idle() {
        use std::collections::BTreeSet;
        // A nudge at bell 870 (day 6) can have cut a batch from bell 844
        // (day 5): both days are resident.
        assert_eq!(split_days(870), [5, 6]);
        assert_eq!(split_days(900), [6, 6]);
        assert_eq!(split_days(3), [0, 0]);
        let skips: BTreeMap<(i32, i32, u32), u64> =
            [((-7, 3, 6), 7), ((-2, 0, 5), 7), ((4, 4, 5), 7)]
                .into_iter()
                .collect();
        let seen: BTreeSet<(i32, i32)> = [(-7, 3), (-2, 0), (4, 4)].into_iter().collect();
        let mut resident: BTreeSet<(i32, i32, u32)> = BTreeSet::new();
        for d in split_days(875) {
            resident.insert((-7, 3, d));
        }
        let samples = vec![
            json!({"bell": 760, "status": {"play": {"nudges_recent": [[-2, 0, 760]]}}}),
            json!({"bell": 761, "status": null}),
            json!({"bell": 762, "status": {"play": {"nudges_recent": [[-2, 0, 760], [-2, 0, 734]]}}}),
        ];
        let n = nudges_from_samples(&samples);
        assert_eq!(n.len(), 2);
        for &(p, q, b) in &n {
            for d in split_days(b) {
                resident.insert((p, q, d));
            }
        }
        let v = classify_days_with(&skips, &BTreeSet::new(), &BTreeSet::new(), &resident, &seen);
        assert_eq!(
            v["idle_province_days_over_6"],
            json!(["(4,4) day 5: 7"]),
            "{v}"
        );
        assert_eq!(
            v["resident_province_days_over_6"],
            json!(["(-7,3) day 6: 7", "(-2,0) day 5: 7"]),
            "{v}"
        );
        assert_eq!(v["skip_txs_per_resident_province_day"]["n"], 2);
        // The old classes are unchanged without resident days.
        let v = classify_days(&skips, &BTreeSet::new(), &BTreeSet::new(), &seen);
        assert_eq!(v["idle_province_days_over_6"].as_array().unwrap().len(), 3);
    }

    /// integ-W6t review: the SkipQuiet reaching `end_bell` (the season-end
    /// flush) is not counted, and a refused resident action (no post-state)
    /// still names its Province by account key.
    #[test]
    fn season_end_flush_and_refused_resident_actions() {
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
        let empty = std::collections::BTreeSet::new();
        let all = catch_up_with(&inp.txs, &program, genesis, 600, u32::MAX, &empty);
        assert_eq!(all["season_end_flush"], 0, "{all}");
        let n_all = all["skip_txs_per_province_day"]["n"].as_u64().unwrap();
        // The last skipped bell of the recording as end_bell: every skip
        // reaching it is the flush and leaves the counts.
        let mut last = 0u32;
        let n_off = field(Kind::SKIP, "n").unwrap();
        for r in records(&inp.txs, &program) {
            if r.r.kind == Kind::SKIP as u8 {
                let kp = &r.r.key_payload;
                last = last.max(le_u32(kp, 8) + kp[n_off] as u32);
            }
        }
        assert!(last > 0);
        let cut = catch_up_with(&inp.txs, &program, genesis, 600, last, &empty);
        let flushed = cut["season_end_flush"].as_u64().unwrap();
        assert!(flushed > 0, "{cut}");
        assert!(cut["skip_txs_per_province_day"]["n"].as_u64().unwrap() <= n_all);
        // The recording's Musters/Explores/Departs name their Provinces.
        assert!(all["resident_actions"].as_u64().unwrap() > 0, "{all}");
        // Refused ones too: strip the post-states and error every resident
        // action; the Province keys are learnt from the other transactions.
        let mut txs = inp.txs.clone();
        let tags: Vec<u8> = RESIDENT_IXS.iter().map(|ix| *ix as u8).collect();
        let mut refused = 0;
        for t in txs.iter_mut() {
            if tag_of(t, &program).is_some_and(|g| tags.contains(&g)) {
                t.err = Some("NotResident".into());
                t.post.clear();
                t.logs.clear();
                refused += 1;
            }
        }
        assert!(refused > 0);
        let v = catch_up_with(&txs, &program, genesis, 600, u32::MAX, &empty);
        assert!(v["resident_actions"].as_u64().unwrap() > 0, "{v}");
    }

    /// W6T-4 (§13.4 A2): a by-rule refusal of an honest march fails
    /// criterion 5; persona marches and `bounced` do not.
    #[test]
    fn honest_rule_refusal_is_persona_violation() {
        let by_rule = json!([
            {"host_id": "1231457318076419", "arrive_bell": 366, "outcome": 6, "reason": "shielded-own"},
            {"host_id": "357345573994498", "arrive_bell": 135, "outcome": 6, "reason": "bounced"},
            {"host_id": "77", "arrive_bell": 400, "outcome": 6, "reason": "path"},
        ]);
        let bots = json!([
            {"bot": 936, "persona": "honest", "host": "1231457318076419", "arrive": 366, "route": "keeper", "last_code": "Shielded"},
            {"bot": 12, "persona": "garbage_seal", "host": 77, "arrive": 400, "route": "self", "last_code": "Path"},
        ]);
        let h = honest_rule_refusals(&by_rule, &bots);
        assert_eq!(h.len(), 1, "{h:?}");
        assert!(
            h[0].contains("1231457318076419@366") && h[0].contains("shielded-own"),
            "{h:?}"
        );
        let mut f = good_facts();
        f["unrevealed_by_rule"] = by_rule.clone();
        f["bots_unrevealed"] = bots;
        let d = decide(&f);
        let c = |id: &str| d.iter().find(|c| c["criterion"] == id).unwrap().clone();
        assert_eq!(c("5")["status"], "fail", "{d:?}");
        assert!(c("5")["why"].as_str().unwrap().contains("§13.4 A2"));
        assert_eq!(
            c("4")["status"],
            "pass",
            "by rule, not a liveness miss: {d:?}"
        );
        assert!(
            c("4")["why"].as_str().unwrap().contains("shielded-own 1"),
            "{d:?}"
        );
        // No bots list: an unlisted march counts as honest.
        f["bots_unrevealed"] = Value::Null;
        assert_eq!(
            honest_rule_refusals(&f["unrevealed_by_rule"], &Value::Null).len(),
            2
        );
        // Only bounced: nothing to blame.
        let only_bounced = json!([{"host_id": "5", "arrive_bell": 9, "reason": "bounced"}]);
        assert!(honest_rule_refusals(&only_bounced, &Value::Null).is_empty());
        // Hex and decimal host ids match.
        // W6T-3's format: persona null for the fleet, `seal`, host as a
        // decimal string.
        let u3 = json!([{"bot": 5, "persona": null, "seal": "honest", "host": "1231457318076419", "arrive": 366},
                        {"bot": 6, "persona": null, "seal": "garbage", "host": "77", "arrive": 400}]);
        assert_eq!(honest_rule_refusals(&by_rule, &u3).len(), 1);
        let hx = json!([{"host_id": "255", "arrive_bell": 9, "reason": "path"}]);
        let bx = json!([{"persona": "zero_tip", "host": "0xff", "arrive": 9}]);
        assert!(honest_rule_refusals(&hx, &bx).is_empty());
    }

    /// W6T-4: every (kind, error) of the w6-s7 run has its class, with the
    /// triage's totals (keeper waste 32,873; A/B redundancy 3,570 + the
    /// other duplicates; bot waste 248).
    #[test]
    fn failed_tx_classes_follow_the_triage() {
        let w6s7 = json!({
            "Build: QueueFull": 2,
            "CloseArrivalDay: {\"InstructionError\":[3,\"ProgramFailedToComplete\"]}": 12874,
            "CloseArrivalDay: {\"InstructionError\":[4,\"ProgramFailedToComplete\"]}": 12781,
            "CloseArrivalSlot: {\"InstructionError\":[3,\"ProgramFailedToComplete\"]}": 1500,
            "CloseArrivalSlot: {\"InstructionError\":[4,\"ProgramFailedToComplete\"]}": 1500,
            "Depart: NotResident": 19, "Depart: TipTooLow": 21, "Explore: NotResident": 14,
            "FoldOccupancy: FoldStale": 1, "Join: AlreadyDone": 10, "Muster: NotResident": 14,
            "Muster: ProvinceFull": 111, "OpenProvince: AlreadyDone": 20, "PostAnchor: BadData": 3552,
            "PostAnchorMulti: BadData": 666, "PostBeacon: AlreadyDone": 208, "ResolveFromInputs: OutOfOrder": 19,
            "Reveal: AlreadyDone": 350, "Reveal: BadAddress": 8, "Reveal: Shielded": 61, "Reveal: WindowClosed": 30,
            "Reveal: WrongStatus": 27, "SettleDeparture: AlreadyDone": 1716, "SettleTicket: AlreadyDone": 1,
            "SettleTicket: NoTicket": 31, "SettleTransit: TransitState": 1504, "SkipQuiet: NotQuiet": 2
        });
        let c = failure_classes(&w6s7);
        assert_eq!(c["total"], 37_042, "{c}");
        assert_eq!(c["unclassified"], 0, "{c}");
        assert_eq!(c["by_class"]["waste"], 32_873 + 160 + 88, "{c}");
        assert_eq!(c["by_cause"]["keeper"], 32_873, "{c}");
        assert_eq!(c["by_cause"]["bot-policy"], 160, "{c}");
        assert_eq!(c["by_cause"]["bot-bug"], 88, "{c}");
        assert_eq!(c["by_cause"]["a/b race"], 3_570, "{c}");
        assert_eq!(
            c["by_class"]["redundancy"].as_u64().unwrap(),
            3_570 + 208 + 20 + 10 + 1,
            "{c}"
        );
        assert_eq!(
            c["by_class"]["expected"].as_u64().unwrap(),
            21 + 8 + 30 + 31 + 19 + 2 + 1,
            "{c}"
        );
        assert_eq!(
            classify_failure(
                "CloseArrivalDay",
                "{\"InstructionError\":[3,\"ProgramFailedToComplete\"]}"
            )
            .0,
            "waste"
        );
        assert_eq!(classify_failure("Reveal", "Shielded"), ("waste", "bot-bug"));
        assert_eq!(
            classify_failure("Depart", "ArrivalBell"),
            ("waste", "bot-bug")
        );
        assert_eq!(
            classify_failure("SettleTransit", "HostInTransit").0,
            "expected"
        );
        assert_eq!(classify_failure("Nope", "Whatever").0, "unclassified");
        // Seen in the U4 R3 run.
        assert_eq!(classify_failure("SkipQuiet", "OutOfOrder").0, "expected");
        assert_eq!(
            classify_failure("Train", "Insufficient"),
            ("waste", "bot-policy")
        );
    }

    /// W6T-4: the keeper status comes from the last answered sample and
    /// its nested `duties`; unanswered samples are counted.
    #[test]
    fn keeper_status_from_last_non_null_sample() {
        let ks = vec![
            json!({"bell": 1, "status": {"bell": 1, "alerts": 0, "pools": {"reveal": {"effective_n": 150}},
                   "duties": {"anchor_latency_slots_p99": 4, "seed_latency_slots_p99": 5, "provinces_opened": 169, "archived_bells": 448}}}),
            json!({"bell": 2, "status": null}),
            json!({"bell": 3, "status": null, "error": "timeout 5 s"}),
        ];
        let k = keeper_summary(&ks, 0, 144);
        assert_eq!(k["last"]["bell"], 1, "{k}");
        assert_eq!(k["last"]["anchor_latency_slots_p99"], 4);
        assert_eq!(k["last"]["provinces_opened"], 169);
        assert_eq!(k["status_unanswered"], 2);
        assert_eq!(k["status_timeouts"], 1);
        assert_eq!(k["status_unanswered_bells"], json!([2, 3]));
    }

    /// W6T-4: the per-bell load average is summarised.
    #[test]
    fn loadavg_per_bell() {
        let v: Vec<Value> = (0..300)
            .map(|b| json!({"bell": b, "load1": if b == 150 { 22.0 } else { 4.0 }, "load5": 4.0, "load15": 4.0}))
            .collect();
        let s = loadavg_summary(&v);
        assert_eq!(s["bells"], 300);
        assert_eq!(s["load1"]["max"], 22.0);
        assert_eq!(s["load1"]["p50"], 4.0);
        assert_eq!(s["max_by_day"], json!([4.0, 22.0, 4.0]));
        assert_eq!(s["worst_bell"], 150);
        assert!(loadavg_summary(&[])["bells"] == 0);
        assert_eq!(
            crate::up::parse_loadavg("{ 3.17 3.34 3.80 }"),
            Some((3.17, 3.34, 3.80))
        );
        assert_eq!(
            crate::up::parse_loadavg("0.5 0.6 0.7 1/200 42"),
            Some((0.5, 0.6, 0.7))
        );
    }

    /// W6T-4: slots with no landing (w6-s7: 827-1028) are found and a
    /// chain kill inside one is named.
    #[test]
    fn no_landing_window_detected() {
        let tx = |slot: u64, t: i64, ok: bool| TxRecord {
            seq: slot,
            slot,
            signature: Default::default(),
            block_time: t,
            tx: vec![],
            logs: vec![],
            err: (!ok).then(|| "x".to_string()),
            code: None,
            units: 0,
            fee: 0,
            post: vec![],
        };
        let g = 10_000;
        let mut txs: Vec<TxRecord> = (100..=826)
            .map(|s| tx(s, g + (s as i64 - 100) * 8, true))
            .collect();
        txs.push(tx(900, g + 800 * 8, false)); // a failure does not land
        txs.extend((1029..1100).map(|s| tx(s, g + (s as i64 - 100) * 8, true)));
        txs.push(tx(1200, g + 1100 * 8, true));
        // A quiet stretch inside a bell (24 slots, ending on a bell start's
        // landing) is not a stall.
        txs.push(tx(1225, g + 1125 * 8, true));
        txs.push(tx(5, g - 100, true)); // before genesis: ignored
        let ev = vec![
            json!({"event": "chaos-kill", "game": g + 1_050 * 8, "detail": {"component": "localnet"}}),
        ];
        let w = no_landing_windows(&txs, g, 600, 8.0, 20, i64::MAX, &ev);
        assert_eq!(w["quiet_windows_inside_a_bell"], 1, "{w}");
        let a = w["windows"].as_array().unwrap();
        assert_eq!(a.len(), 2, "{w}");
        assert_eq!(a[0]["bell_started"], 10);
        assert_eq!(a[0]["slots_after_bell_start"], (7_432 - 6_000) / 8);
        assert_eq!(
            (a[0]["from_slot"].as_u64(), a[0]["to_slot"].as_u64()),
            (Some(827), Some(1028))
        );
        assert_eq!(a[0]["slots"], 202);
        assert_eq!(a[0]["from_bell"], (726 * 8 + 8) / 600);
        assert_eq!(a[0]["explained_by"], json!([]));
        assert_eq!(a[1]["slots"], 100);
        assert_eq!(a[1]["explained_by"], json!(["chaos-kill localnet"]));
        // From end_bell on (the drain) a bell start owes no anchor (U4 R5:
        // five 80-slot drain gaps were listed).
        let w = no_landing_windows(&txs, g, 600, 8.0, 20, 13, &ev);
        assert_eq!(w["windows"].as_array().unwrap().len(), 1, "{w}");
        assert_eq!(w["drain_windows_after_end_bell"], 1);
    }

    /// W6T-4: a skipped hold makes the run not exit-grade (reported as
    /// row E, not an exit failure).
    #[test]
    fn hold_skipped_is_not_exit_grade() {
        let cfg = json!({"adversary": true, "beacon": "archive", "expect_so_sha256": "ab"});
        let ev = vec![
            json!({"event": "hold", "detail": {"kind": "ticket"}}),
            json!({"event": "hold-skipped", "detail": {"kind": "lag", "why": "nothing to hold before the deadline"}}),
        ];
        let e = exit_grade(&ev, &cfg);
        assert_eq!(e["exit_grade"], false, "{e}");
        assert!(e["reasons"].to_string().contains("lag"), "{e}");
        assert_eq!(exit_grade(&ev[..1], &cfg)["exit_grade"], true);
        let test_key = json!({"adversary": true, "beacon": "test-key", "expect_so_sha256": null});
        assert_eq!(exit_grade(&ev[..1], &test_key)["exit_grade"], false);
        let mut f = good_facts();
        f["exit_grade"] = e;
        let d = decide(&f);
        let row = d.iter().find(|c| c["criterion"] == "E").unwrap();
        assert_eq!(row["status"], "not exit-grade");
        assert_eq!(criteria_exit(&d), 0, "reported, not an exit failure");
    }
}
