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

use std::collections::{BTreeMap, BTreeSet, HashMap};

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
    let mut settled: BTreeSet<u64> = BTreeSet::new();
    let mut transit: BTreeMap<String, u64> = BTreeMap::new();
    let mut departs: Vec<(u64, u32)> = vec![];
    let mut bad_seal_codes: BTreeMap<u8, u64> = BTreeMap::new();
    for x in recs {
        let k = Kind::from_u8(x.r.kind);
        *kinds.entry(k.map_or("?", |k| k.name())).or_default() += 1;
        let kp = &x.r.key_payload;
        match k {
            Some(Kind::DEPART) => {
                let host = le_u64(kp, 0);
                let arrive = le_u32(kp, field(Kind::DEPART, "arrive_bell").unwrap_or(21));
                departs.push((host, arrive));
            }
            Some(Kind::TRANSIT_SETTLED) => {
                let host = le_u64(kp, 0);
                let (o, c) = (kp[8], kp[9]);
                settled.insert(host);
                *transit.entry(format!("outcome {o} seal {c}")).or_default() += 1;
                if o == frontier_abi::log::transit_outcome::BAD_SEAL {
                    *bad_seal_codes.entry(c).or_default() += 1;
                }
            }
            _ => {}
        }
    }
    let due: Vec<&(u64, u32)> = departs.iter().filter(|(_, a)| a + 3 <= last_bell).collect();
    let unsettled: Vec<String> = due
        .iter()
        .filter(|(h, _)| !settled.contains(h))
        .map(|(h, a)| format!("{h:#x}@{a}"))
        .collect();
    json!({"records": kinds, "transits": transit, "bad_seal_codes": bad_seal_codes,
           "departs": departs.len(), "departs_due": due.len(), "unsettled_due": unsettled})
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
    let during: Vec<&Value> = samples
        .iter()
        .filter(|s| {
            s["bell"]
                .as_i64()
                .is_some_and(|b| b >= 2 && b >= play_from && b < play_to)
        })
        .collect();
    let min_n = during
        .iter()
        .filter_map(|s| s["status"]["pools"]["reveal"]["effective_n"].as_u64())
        .min();
    let last = samples.last().map(|s| &s["status"]);
    json!({"samples": samples.len(), "min_reveal_effective_n_in_play": min_n,
        "effective_n_ok": min_n.is_some_and(|n| n >= 150),
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
    let last_bell = ((last_time - genesis).max(0) / 600) as u32;
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
    let rep = json!({
        "format": "frontier-stack-report-v1",
        "run_id": st["run_id"], "phase": st["phase"], "config": st["config"], "so": st["so"], "program": st["program"],
        "season": st["season"], "play": st["play"], "source": source, "txs": inp.txs.len(), "last_bell": last_bell,
        "cu": cu, "reveal_cu": stats(&reveal), "failures": failures(&inp.txs, &program),
        "outcomes": out, "provinces": pc, "keepers": {"a": ka, "b": kb}, "herald": herald,
        "events": ev, "bots": {"personas": pers, "errors": bots["errors"], "bots": bots["bots"]},
        "verify": verify, "tamper": {"ok": tamper["ok"], "total": tamper["total"], "on_run": tamper["on_run"]},
        "loads": loads, "criteria": criteria,
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
    0
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
    m.push_str("## Verdicts\n\n");
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

    #[test]
    fn summaries() {
        let ks = vec![
            json!({"bell": 0, "status": {"pools": {"reveal": {"effective_n": 3}}}}),
            json!({"bell": 3, "status": {"pools": {"reveal": {"effective_n": 150}}}}),
            json!({"bell": 4, "status": {"pools": {"reveal": {"effective_n": 149}}}}),
        ];
        let k = keeper_summary(&ks, 0, 144);
        assert_eq!(k["min_reveal_effective_n_in_play"], 149);
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
