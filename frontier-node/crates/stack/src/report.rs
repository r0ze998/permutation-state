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

pub struct Latency {
    pub json: Value,
}

/// Keeper latencies in slots at `scale` (I-54: a slot is 0.4 × scale game
/// seconds), from the records: round → anchor, S → first cache, anchor →
/// last valid reveal, close → resolve (CLASH), plus SKIP per province-day.
pub fn latencies(
    recs: &[Rec],
    scale: f64,
    drand: &fclient::clock::Drand,
    delay_s: f64,
    reveal_window: u32,
) -> Value {
    let slot_secs = 0.4 * scale;
    let to_slots = |secs: f64| (secs / slot_secs).max(0.0);
    let mut region_of: HashMap<(i32, i32), u8> = HashMap::new();
    let mut anchor_a: HashMap<(u32, u8), i64> = HashMap::new();
    let (mut round_anchor, mut s_cache, mut reveal_last, mut close_resolve) =
        (vec![], vec![], vec![], vec![]);
    let mut first_cache: HashMap<(u32, u8), f64> = HashMap::new();
    let mut last_reveal: HashMap<(i32, i32, u32), i64> = HashMap::new();
    let mut skips: HashMap<(i32, i32, u32), u64> = HashMap::new();
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
                let public = drand.round_time(round) as f64 + delay_s;
                round_anchor.push(to_slots(x.time as f64 - public));
            }
            Some(Kind::SEED) => {
                let (bell, region) = (le_u32(kp, 0), kp[4]);
                let round = le_u64(kp, field(Kind::SEED, "round").unwrap_or(6));
                let public = drand.round_time(round) as f64 + delay_s;
                let l = to_slots(x.time as f64 - public);
                let e = first_cache.entry((bell, region)).or_insert(l);
                *e = e.min(l);
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
            Some(Kind::SKIP) => {
                let (p, q) = (le_i32(kp, 0), le_i32(kp, 4));
                let b0 = le_u32(kp, 8);
                *skips.entry((p, q, b0 / 144)).or_default() += 1;
            }
            _ => {}
        }
    }
    s_cache.extend(first_cache.values().copied());
    for ((p, q, arrive), t) in &last_reveal {
        if let Some(a) = region_of
            .get(&(*p, *q))
            .and_then(|r| anchor_a.get(&(*arrive, *r)))
        {
            reveal_last.push(to_slots((t - a) as f64));
        }
    }
    let skip_counts: Vec<f64> = skips.values().map(|n| *n as f64).collect();
    let over6 = skip_counts.iter().filter(|n| **n > 6.0).count();
    json!({
        "slot_game_secs": slot_secs,
        "round_to_anchor_slots": stats(&round_anchor),
        "s_to_first_cache_slots": stats(&s_cache),
        "anchor_to_last_reveal_slots": stats(&reveal_last),
        "close_to_resolve_slots": stats(&close_resolve),
        "skip_txs_per_province_day": stats(&skip_counts),
        "province_days_over_6_skips": over6,
        "targets_slots_p99": {"round_to_anchor": 2, "s_to_first_cache": 2, "anchor_to_last_reveal": 4, "close_to_resolve": 8, "skips_per_idle_day": 6},
        "note": "times are game seconds of the landing slot; round publication = round_time + drand delay; close = THE anchor's A + W. The slot targets are criterion 3's for the 20x run; at other scales a slot is another length of game time",
    })
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
    v.push(if !bad.is_empty() {
        verdict("1", "fail", bad.join("; "))
    } else if play_bells < end_bell {
        verdict("1", "pass", "the run stops before end_bell (EndSeason not expected): no stuck province-bell, every due transit settled exactly once; ClashInputs closability is judged by the exit run")
    } else {
        verdict("1", "pass", "EndSeason landed; no stuck province-bell; every due transit settled exactly once")
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
    // 3 — keeper latencies: slots at 20x, game seconds at 2x.
    let scale = facts["scale"].as_f64().unwrap_or(0.0);
    let lat = &facts["latency"];
    let slot_secs = lat["slot_game_secs"].as_f64().unwrap_or(0.4 * scale);
    let p99 = |k: &str| lat[k]["p99"].as_f64();
    let checks = [
        ("round_to_anchor_slots", 2.0, 5.0),
        ("s_to_first_cache_slots", 2.0, 5.0),
        ("anchor_to_last_reveal_slots", 4.0, 30.0),
        ("close_to_resolve_slots", 8.0, 60.0),
    ];
    let slots_mode = (scale - 20.0).abs() < 1e-9;
    let secs_mode = (scale - 2.0).abs() < 1e-9;
    if !slots_mode && !secs_mode {
        v.push(verdict("3", "n.a.", format!("criterion 3's targets are defined at 20x (slots) and 2x (game seconds); this run is {scale}x (figures reported)")));
    } else {
        let mut miss = vec![];
        let mut none = vec![];
        for (k, slots, secs) in checks {
            match p99(k) {
                None => none.push(k),
                Some(x) if slots_mode && x > slots => {
                    miss.push(format!("{k} p99 {x:.2} > {slots} slots"))
                }
                Some(x) if secs_mode && x * slot_secs > secs => {
                    miss.push(format!("{k} p99 {:.1} s > {secs} s", x * slot_secs))
                }
                _ => {}
            }
        }
        let over6 = lat["province_days_over_6_skips"].as_u64().unwrap_or(0);
        let skip_note = format!("{over6} province-days over 6 SkipQuiet (idle and churned not told apart: reported, not decided)");
        v.push(if !miss.is_empty() {
            verdict("3", "fail", format!("{}; {skip_note}", miss.join("; ")))
        } else if !none.is_empty() {
            verdict(
                "3",
                "n.a.",
                format!("no samples for {}; {skip_note}", none.join(", ")),
            )
        } else {
            verdict(
                "3",
                "pass",
                format!("every p99 within its target; {skip_note}"),
            )
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
    let lat = latencies(
        &recs,
        scale,
        &drand,
        delay_s,
        frontier_abi::presets::M1_LOCAL_7D.reveal_window,
    );
    let out = outcomes(&recs, last_bell);
    let ps = final_provinces(&inp);
    let pc = province_checks(&ps, play_bells, last_bell);
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
            "note": if play_bells < end_bell { "the run stops before end_bell: EndSeason not expected" } else { "" }},
        "2_cu": {"over_budget": over, "reveal": stats(&reveal)},
        "3_latency_slots": lat,
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
        "verify": verify, "tamper": {"ok": tamper["ok"], "total": tamper["total"], "on_run": tamper["on_run"]},
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
        "- verify: **{}** (fail codes {})\n- tamper: {}/{} classes FAIL with their codes ({} built from the run)\n",
        f(&r["verify"]["verdict"]), f(&r["verify"]["fail_codes"]), f(&r["tamper"]["ok"]), f(&r["tamper"]["total"]), f(&r["tamper"]["on_run"])
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
        "\n## Keeper latencies (slots of {} game s)\n\n| measure | n | p50 | p99 | max | target p99 |\n|---|---|---|---|---|---|\n",
        f(&l["slot_game_secs"])
    ));
    for (k, t) in [
        ("round_to_anchor_slots", "2"),
        ("s_to_first_cache_slots", "2"),
        ("anchor_to_last_reveal_slots", "4"),
        ("close_to_resolve_slots", "8"),
        ("skip_txs_per_province_day", "6 (idle)"),
    ] {
        let s = &l[k];
        m.push_str(&format!(
            "| {k} | {} | {} | {} | {} | {t} |\n",
            f(&s["n"]),
            f(&s["p50"]),
            f(&s["p99"]),
            f(&s["max"])
        ));
    }
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
        let lat = latencies(&recs, 20.0, &drand, 1.0, 600);
        assert!(
            lat["round_to_anchor_slots"]["n"].as_u64().unwrap() > 0,
            "{lat}"
        );
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
                "anchor_to_last_reveal_slots": {"p99": 3.0}, "close_to_resolve_slots": {"p99": 6.0}, "province_days_over_6_skips": 0},
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
