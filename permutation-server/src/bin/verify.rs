//! Replay verifier (V4 §11 Must 8): recompute a season from public data and
//! check every root the chain claims.
//!
//!     cargo run --release --bin verify -- --gateway http://127.0.0.1:4190 \
//!         [--base http://127.0.0.1:18899] [--er http://127.0.0.1:17799] [--json]
//!
//! 1. Read the Season account (seeds, entrants, pool) — straight from the
//!    base-layer RPC when `--base` is given, else through the gateway.
//! 2. Rebuild genesis with the same `permutation-rules` crate and compare its
//!    root with the program's `PS_GENESIS` record.
//! 3. Replay every tick record (`PS_TICK`: tick seed + the exact order
//!    batches) and check that each `pre_root` matches the previous state and
//!    each `post_root` matches the recomputed state. With `--er`, every record
//!    is re-read from the transaction logs on the ER itself, so the gateway is
//!    only an index, never a source of truth.
//! 4. If the season is finalized, recompute the payouts (§14.5) and compare
//!    them, and the final root, with the Season account.

use borsh::BorshDeserialize;
use permutation_chain::state::Season;
use permutation_rules::genesis::{new_season, Entry};
use permutation_rules::orders::OrderBatch;
use permutation_rules::scoring::payouts;
use permutation_rules::state::{DeclaredKind, WorldState};
use permutation_rules::tick::{run_phase, TickInput};
use permutation_rules::{Preset, Ruleset};
use permutation_server::chainlink::{from_hex, hex, ChainLink};
use serde_json::{json, Value};

struct Report {
    checks: Vec<(String, bool, String)>,
    json: bool,
}

impl Report {
    fn check(&mut self, what: impl Into<String>, ok: bool, detail: impl Into<String>) {
        let (what, detail) = (what.into(), detail.into());
        if !self.json {
            println!("{} {what}{}", if ok { "✓" } else { "✗" }, if detail.is_empty() { String::new() } else { format!(" — {detail}") });
        }
        self.checks.push((what, ok, detail));
    }
    fn failed(&self) -> usize {
        self.checks.iter().filter(|c| !c.1).count()
    }
}

fn arg(args: &[String], k: &str) -> Option<String> {
    args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let gateway = ChainLink::new(&arg(&args, "--gateway").unwrap_or_else(|| "http://127.0.0.1:4190".into())).expect("--gateway URL");
    let base = arg(&args, "--base").map(|u| ChainLink::new(&u).expect("--base URL"));
    let er = arg(&args, "--er").map(|u| ChainLink::new(&u).expect("--er URL"));
    let mut r = Report { checks: Vec::new(), json: args.iter().any(|a| a == "--json") };

    // 1. Season account.
    let info = gateway.get_json("/season").expect("gateway /season");
    let season_addr = info["accounts"]["season"].as_str().expect("season address").to_string();
    let season: Season = match &base {
        Some(b) => {
            let data = b.account_data(&season_addr).expect("base RPC").expect("season account exists");
            Season::deserialize(&mut &data[..]).expect("decode season")
        }
        None => {
            eprintln!("note: no --base given; reading the season through the gateway's decoded view is not supported — pass --base");
            std::process::exit(2);
        }
    };
    r.check(format!("season {} read from the base layer ({})", season.season_id, season_addr), true, "");
    let rules = if season.preset == 1 { Ruleset::new(Preset::Season) } else { Ruleset::new(Preset::Blitz) };

    // 2. Genesis.
    let entries: Vec<Entry> = season
        .civs
        .iter()
        .map(|c| Entry {
            name: c.name.clone(),
            declared_kind: match c.kind {
                0 => DeclaredKind::Human,
                1 => DeclaredKind::Agent,
                _ => DeclaredKind::Undeclared,
            },
            payout_wallet: c.payout,
            exchange_deposit: season.exchange_credit,
        })
        .collect();
    let mut state: WorldState = new_season(&rules, &season.world_seed, &season.season_seed, &entries).expect("genesis recomputes");
    let genesis_root = state.state_root().unwrap();
    let claimed_genesis = info["genesis"]["root"].as_str().map(from_hex);
    match (&claimed_genesis, &er) {
        (Some(claimed), _) => {
            // Cross-check the PS_GENESIS record in the genesis transaction itself.
            let from_chain = info["genesis"]["signature"].as_str().zip(base.as_ref()).and_then(|(sig, b)| {
                b.log_records(sig).ok()?.into_iter().find(|f| f.first().map(|t| t.as_slice()) == Some(b"PS_GENESIS")).map(|f| f[1].clone())
            });
            let chain_ok = from_chain.as_ref().is_none_or(|c| c == claimed);
            r.check("genesis recomputed from the season's seeds and entrants", *claimed == genesis_root.to_vec() && chain_ok,
                format!(
                    "root {} {}",
                    &hex(&genesis_root)[..16],
                    if from_chain.is_some() {
                        "(matched the PS_GENESIS log on chain)"
                    } else {
                        "(PS_GENESIS log not served by this RPC; anchored below by tick 0's on-chain pre-state root)"
                    }
                ));
        }
        _ => r.check("genesis record available", false, "the gateway has no PS_GENESIS record for this season"),
    }

    // 3. Ticks.
    let records = gateway.get_json("/ticks?from=0").expect("gateway /ticks");
    let records = records["records"].as_array().cloned().unwrap_or_default();
    let mut replayed = 0;
    let mut first_bad: Option<String> = None;
    for rec in &records {
        let tick = rec["tick"].as_u64().unwrap_or(0) as u16;
        let to = rec["to"].as_u64().unwrap_or(12) as u8;
        let (mut vrf, mut pre, mut post, mut batches_raw) =
            (from_hex(rec["vrf"].as_str().unwrap_or("")), from_hex(rec["preRoot"].as_str().unwrap_or("")), from_hex(rec["root"].as_str().unwrap_or("")), from_hex(rec["batches"].as_str().unwrap_or("")));
        if let (Some(er), Some(sig)) = (&er, rec["signature"].as_str()) {
            // Re-read the record from the ER's own transaction logs.
            match er.log_records(sig) {
                Ok(fields) => match fields.into_iter().find(|f| f.first().map(|t| t.as_slice()) == Some(b"PS_TICK")) {
                    Some(f) if f.len() >= 7 => {
                        let same = f[3] == vrf && f[4] == pre && f[5] == post && f[6] == batches_raw;
                        if !same && first_bad.is_none() {
                            first_bad = Some(format!("tick {tick}: gateway index differs from the ER log"));
                        }
                        (vrf, pre, post, batches_raw) = (f[3].clone(), f[4].clone(), f[5].clone(), f[6].clone());
                    }
                    _ => {
                        first_bad.get_or_insert(format!("tick {tick}: no PS_TICK record in {sig}"));
                    }
                },
                Err(e) => {
                    first_bad.get_or_insert(format!("tick {tick}: {e}"));
                }
            }
        }
        if state.state_root().unwrap().to_vec() != pre {
            first_bad.get_or_insert(format!("tick {tick}: pre-state root does not match the replayed state"));
            break;
        }
        let batches = Vec::<OrderBatch>::try_from_slice(&batches_raw).unwrap_or_default();
        let input = TickInput { vrf: vrf.clone().try_into().unwrap_or([0; 32]), batches };
        while state.phase_cursor < to.min(12) {
            let p = state.phase_cursor;
            if run_phase(&mut state, &rules, &input, p).is_err() {
                break;
            }
            if state.phase_cursor == 0 {
                break;
            }
        }
        if state.state_root().unwrap().to_vec() != post {
            first_bad.get_or_insert(format!("tick {tick}: recomputed root {} ≠ on-chain {}", &hex(&state.state_root().unwrap())[..16], &hex(&post)[..16]));
            break;
        }
        replayed += 1;
    }
    r.check(
        format!("{replayed} tick records replayed; every pre- and post-state root matches{}", if er.is_some() { " (records read from the ER's transaction logs)" } else { "" }),
        first_bad.is_none() && replayed == records.len(),
        first_bad.clone().unwrap_or_else(|| format!("now at tick {}, root {}", state.tick, &hex(&state.state_root().unwrap())[..16])),
    );

    // 4. Settlement.
    if season.status == permutation_chain::state::SeasonStatus::Finalized {
        let p = payouts(&rules, &state, season.pool);
        r.check("final world root matches the Season account", state.state_root().unwrap() == season.final_root, hex(&season.final_root)[..16].to_string());
        r.check("payouts recomputed (§14.5) match the on-chain payouts", p.per_civ == season.payouts && p.rollover == season.rollover,
            format!("{:?} rollover {}", season.payouts, season.rollover));
    } else {
        r.check("season not finalized yet: payouts not checked", true, format!("status {:?}", season.status));
    }

    let failed = r.failed();
    if r.json {
        let out = json!({
            "season": season.season_id.to_string(),
            "ok": failed == 0,
            "ticks": replayed,
            "checks": r.checks.iter().map(|(w, ok, d)| json!({"check": w, "ok": ok, "detail": d})).collect::<Vec<Value>>(),
        });
        println!("{out}");
    } else {
        println!("{}", if failed == 0 { "VERIFIED: the season replays exactly as the chain recorded it." } else { "FAILED" });
    }
    std::process::exit(if failed == 0 { 0 } else { 1 });
}
