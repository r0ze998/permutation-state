//! Replay verifier (V4 §11 Must 8, V5 §11): recompute a season from public
//! data and check every root the chain claims, the governance included.
//!
//!     cargo run --release --bin verify -- --gateway http://127.0.0.1:4191 \
//!         --base http://127.0.0.1:18899 [--er http://127.0.0.1:17799] [--json]
//!
//! 1. Read the Season account straight from the base-layer RPC.
//! 2. Rebuild genesis (the season's nations, seeds and treasuries) with the
//!    same `permutation-rules` crate and compare with `PS_GENESIS`.
//! 3. Replay the seating of members (`PS_SEAT`: each member's nation, key,
//!    candidacy and first-election votes) and the first election
//!    (`PS_OPEN`), checking each root.
//! 4. Replay every tick record and check that each `pre_root` matches the
//!    previous state and each `post_root` the recomputed one. `PS_TICK`
//!    carries the roots and the input's hash; the input itself (the tick
//!    seed, every office's batch and every governance action) was published
//!    as `PS_INPUT` chunks before the program would resolve the tick. With
//!    `--er`, both are re-read from the ER's transaction logs, so the gateway
//!    is only an index, never a source of truth.
//! 5. If the season is finalized, recompute the settlement (V5 §7): every
//!    member's payout, the pool and operations totals, and the treasuries.

use borsh::BorshDeserialize;
use permutation_chain::state::{MemberAccount, Season};
use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::gov::{self, GovAction, GovEntry, Role, NOBODY};
use permutation_rules::payout::settle;
use permutation_rules::state::WorldState;
use permutation_rules::tick::{run_phase, TickInput};
use permutation_rules::{Preset, Ruleset};
use permutation_server::chainlink::ChainLink;
use permutation_server::codec::{base64, base64_encode, from_hex, hex};
use serde_json::{json, Value};

struct Report {
    checks: Vec<(String, bool, String)>,
    json: bool,
}

impl Report {
    fn check(&mut self, what: impl Into<String>, ok: bool, detail: impl Into<String>) {
        let (what, detail) = (what.into(), detail.into());
        if !self.json {
            println!(
                "{} {what}{}",
                if ok { "✓" } else { "✗" },
                if detail.is_empty() {
                    String::new()
                } else {
                    format!(" — {detail}")
                }
            );
        }
        self.checks.push((what, ok, detail));
    }
    fn failed(&self) -> usize {
        self.checks.iter().filter(|c| !c.1).count()
    }
}

fn arg(args: &[String], k: &str) -> Option<String> {
    args.iter()
        .position(|a| a == k)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// A record's fields, re-read from the chain's transaction logs when an RPC
/// is given (`tag` first), else as the gateway indexed them.
fn fields(
    rpc: Option<&ChainLink>,
    sig: Option<&str>,
    tag: &[u8],
) -> Result<Option<Vec<Vec<u8>>>, String> {
    let (Some(rpc), Some(sig)) = (rpc, sig) else {
        return Ok(None);
    };
    Ok(rpc
        .log_records(sig)?
        .into_iter()
        .find(|f| f.first().map(|t| t.as_slice()) == Some(tag)))
}

/// A tick input reassembled from its PS_INPUT records (the LogTickInput
/// transactions), if every chunk is there and the whole matches `hash`.
fn published_input(er: &ChainLink, sigs: Option<&Vec<Value>>, hash: &[u8]) -> Option<Vec<u8>> {
    let mut chunks: Vec<Option<Vec<u8>>> = Vec::new();
    for sig in sigs?.iter().filter_map(|s| s.as_str()) {
        for f in er
            .log_records(sig)
            .ok()?
            .into_iter()
            .filter(|f| f.first().map(|t| t.as_slice()) == Some(b"PS_INPUT".as_slice()))
        {
            let (chunk, total) = (
                u16::from_le_bytes(f.get(2)?[..2].try_into().ok()?) as usize,
                u16::from_le_bytes(f.get(3)?[..2].try_into().ok()?) as usize,
            );
            if f.get(4)?.as_slice() != hash {
                continue;
            }
            chunks.resize(total, None);
            *chunks.get_mut(chunk)? = Some(f.get(5).cloned().unwrap_or_default());
        }
    }
    let bytes: Vec<u8> = chunks.into_iter().collect::<Option<Vec<_>>>()?.concat();
    (permutation_rules::hash::sha256(&[&bytes]).as_slice() == hash).then_some(bytes)
}

/// Every Member account of the season on the base layer, in registration order.
fn season_members(
    base: &ChainLink,
    _season: &str,
    info: &Value,
    season_id: u64,
) -> Vec<MemberAccount> {
    let program = info["programId"].as_str().unwrap_or_default();
    let id = base64_encode(&season_id.to_le_bytes());
    let magic = base64_encode(b"PSMEMBR5");
    let res = base
        .rpc(
            "getProgramAccounts",
            json!([program, {"encoding": "base64", "commitment": "confirmed", "filters": [
                {"memcmp": {"offset": 0, "bytes": magic, "encoding": "base64"}},
                {"memcmp": {"offset": 8, "bytes": id, "encoding": "base64"}}]}]),
        )
        .unwrap_or(Value::Null);
    let mut out: Vec<MemberAccount> = res
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|a| base64(a["account"]["data"][0].as_str()?).ok())
        .filter_map(|d| MemberAccount::deserialize(&mut &d[..]).ok())
        .collect();
    out.sort_by_key(|m| m.index);
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let gateway =
        ChainLink::new(&arg(&args, "--gateway").unwrap_or_else(|| "http://127.0.0.1:4191".into()))
            .expect("--gateway URL");
    let base = arg(&args, "--base").map(|u| ChainLink::new(&u).expect("--base URL"));
    let er = arg(&args, "--er").map(|u| ChainLink::new(&u).expect("--er URL"));
    let mut r = Report {
        checks: Vec::new(),
        json: args.iter().any(|a| a == "--json"),
    };

    // 1. Season account.
    let info = gateway.get_json("/season").expect("gateway /season");
    let season_addr = info["accounts"]["season"]
        .as_str()
        .expect("season address")
        .to_string();
    let Some(b) = &base else {
        eprintln!("pass --base: the season is read from the base layer, not from the gateway");
        std::process::exit(2);
    };
    let data = b
        .account_data(&season_addr)
        .expect("base RPC")
        .expect("season account exists");
    let season = Season::deserialize(&mut &data[..]).expect("decode season");
    r.check(
        format!(
            "season {} read from the base layer ({})",
            season.season_id, season_addr
        ),
        true,
        format!(
            "{} nations, {} members, entry fee {}",
            season.nations, season.member_count, season.entry_fee
        ),
    );
    let mut rules = if season.preset == 1 {
        Ruleset::new(Preset::Season)
    } else {
        Ruleset::new(Preset::Blitz)
    };
    rules.market_enabled = season.market;

    // 2. Genesis.
    let mut entries = nation_entries(season.nations as usize);
    for (e, t) in entries.iter_mut().zip(&season.treasury) {
        e.treasury = *t;
    }
    let mut state: WorldState =
        new_season(&rules, &season.world_seed, &season.season_seed, &entries)
            .expect("genesis recomputes");
    let genesis_root = state.state_root().unwrap();
    match info["genesis"]["root"].as_str().map(from_hex) {
        Some(claimed) => {
            let on_chain = fields(
                base.as_ref(),
                info["genesis"]["signature"].as_str(),
                b"PS_GENESIS",
            )
            .ok()
            .flatten()
            .map(|f| f[1].clone());
            let ok =
                claimed == genesis_root.to_vec() && on_chain.as_ref().is_none_or(|c| *c == claimed);
            r.check(
                "genesis recomputed from the season's nations, seeds and treasuries",
                ok,
                format!(
                    "root {} {}",
                    &hex(&genesis_root)[..16],
                    if on_chain.is_some() {
                        "(matched the PS_GENESIS log on chain)"
                    } else {
                        ""
                    }
                ),
            );
        }
        None => r.check(
            "genesis record available",
            false,
            "the gateway has no PS_GENESIS record for this season",
        ),
    }

    // 3. Seating and the first election. The members come from their own
    // accounts on the base layer (final once registration closed), in
    // registration order; each PS_SEAT root, when the gateway indexed it, is
    // checked too, and tick 0's pre-state root anchors the result below.
    let mut seat_ok = true;
    let mut seat_detail = String::new();
    let seat_roots: Vec<(usize, Vec<u8>)> = {
        let mut n = 0usize;
        info["seating"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|rec| {
                let f = fields(base.as_ref(), rec["signature"].as_str(), b"PS_SEAT")
                    .ok()
                    .flatten()?;
                let seated: Vec<(u16, [u8; 32], u8, [u32; 4])> =
                    Vec::try_from_slice(f.get(2)?).ok()?;
                n += seated.len();
                Some((n, f.get(1)?.clone()))
            })
            .collect()
    };
    let members = season_members(b, &season_addr, &info, season.season_id);
    if members.len() as u32 != season.member_count {
        seat_ok = false;
        seat_detail = format!(
            "{} member accounts found for {} registered",
            members.len(),
            season.member_count
        );
    }
    {
        let seated: Vec<(u16, [u8; 32], u8, [u32; 4])> = members
            .iter()
            .map(|m| (m.civ, m.session, m.stand, m.votes))
            .collect();
        for (civ, key, stand, votes) in seated {
            let Ok(id) = gov::join(&mut state, &rules, civ, key) else {
                seat_ok = false;
                break;
            };
            let mut e = vec![GovEntry {
                member: id,
                signer: key,
                action: GovAction::Stand { roles: stand },
            }];
            for role in Role::ALL {
                if votes[role.index()] != NOBODY {
                    e.push(GovEntry {
                        member: id,
                        signer: key,
                        action: GovAction::Vote {
                            role,
                            candidate: votes[role.index()],
                        },
                    });
                }
            }
            seat_ok &= gov::apply_pre_season(&mut state, &rules, &e).is_ok();
            if let Some((_, root)) = seat_roots.iter().find(|(n, _)| *n == state.members.len()) {
                if state.state_root().unwrap().to_vec() != *root {
                    seat_ok = false;
                    seat_detail = format!(
                        "root after seating {} members differs from PS_SEAT",
                        state.members.len()
                    );
                }
            }
        }
    }
    r.check(
        format!(
            "{} members seated in registration order, with their candidacy and votes",
            state.members.len()
        ),
        seat_ok && state.members.len() as u32 == season.member_count,
        seat_detail,
    );
    let _ = gov::first_election(&mut state, &rules);
    let open_root = info["open"]["root"].as_str().map(from_hex);
    let open_chain = fields(
        base.as_ref(),
        info["open"]["signature"].as_str(),
        b"PS_OPEN",
    )
    .ok()
    .flatten()
    .map(|f| f[1].clone());
    let opened = open_chain.or(open_root);
    let first_pre = gateway
        .get_json("/ticks?from=0")
        .ok()
        .and_then(|v| v["records"].as_array().and_then(|r| r.first().cloned()))
        .and_then(|r| r["preRoot"].as_str().map(from_hex));
    let anchor = opened.or(first_pre);
    r.check(
        "first election recomputed (PS_OPEN, or tick 0's pre-state root)",
        anchor.as_ref() == Some(&state.state_root().unwrap().to_vec()),
        format!(
            "officers {:?}",
            state
                .nations
                .iter()
                .map(|n| n.offices.map(|m| if m == NOBODY { -1 } else { m as i64 }))
                .collect::<Vec<_>>()
        ),
    );

    // 4. Ticks.
    let records = gateway.get_json("/ticks?from=0").expect("gateway /ticks");
    let records = records["records"].as_array().cloned().unwrap_or_default();
    let mut replayed = 0;
    let (mut gov_actions, mut batches_seen) = (0usize, 0usize);
    let mut first_bad: Option<String> = None;
    let mut published: std::collections::HashMap<Vec<u8>, Option<Vec<u8>>> = Default::default();
    for rec in &records {
        let tick = rec["tick"].as_u64().unwrap_or(0) as u16;
        let to = rec["to"].as_u64().unwrap_or(12) as u8;
        let (mut pre, mut post, mut input_raw) = (
            from_hex(rec["preRoot"].as_str().unwrap_or("")),
            from_hex(rec["root"].as_str().unwrap_or("")),
            from_hex(rec["input"].as_str().unwrap_or("")),
        );
        if let Some(er) = &er {
            // The PS_TICK record carries the roots and the input's hash; the
            // input itself is in the PS_INPUT records published before the
            // tick could resolve. Both are read back from the ER's logs.
            match fields(Some(er), rec["signature"].as_str(), b"PS_TICK") {
                Ok(Some(f)) if f.len() >= 6 => {
                    let hash = f[5].clone();
                    if !published.contains_key(&hash) {
                        let chunks = published_input(er, rec["inputSignatures"].as_array(), &hash);
                        published.insert(hash.clone(), chunks);
                    }
                    let same = f[3] == pre
                        && f[4] == post
                        && permutation_rules::hash::sha256(&[&input_raw]).to_vec() == hash;
                    if !same && first_bad.is_none() {
                        first_bad = Some(format!(
                            "tick {tick}: gateway index differs from the ER log"
                        ));
                    }
                    match published.get(&hash).cloned().flatten() {
                        Some(bytes) => input_raw = bytes,
                        None => {
                            first_bad.get_or_insert(format!(
                                "tick {tick}: its input is not fully published on chain (PS_INPUT)"
                            ));
                        }
                    }
                    (pre, post) = (f[3].clone(), f[4].clone());
                }
                Ok(_) => {
                    first_bad.get_or_insert(format!(
                        "tick {tick}: no PS_TICK record in its transaction"
                    ));
                }
                Err(e) => {
                    first_bad.get_or_insert(format!("tick {tick}: {e}"));
                }
            }
        }
        if state.state_root().unwrap().to_vec() != pre {
            first_bad.get_or_insert(format!(
                "tick {tick}: pre-state root does not match the replayed state"
            ));
            break;
        }
        let input = TickInput::try_from_slice(&input_raw).unwrap_or_default();
        if state.phase_cursor == 0 {
            gov_actions += input.gov.len();
            batches_seen += input.batches.len();
        }
        while state.phase_cursor < to.min(12) {
            let p = state.phase_cursor;
            if run_phase(&mut state, &rules, &input, p).is_err() || state.phase_cursor == 0 {
                break;
            }
        }
        if state.state_root().unwrap().to_vec() != post {
            first_bad.get_or_insert(format!(
                "tick {tick}: recomputed root {} ≠ on-chain {}",
                &hex(&state.state_root().unwrap())[..16],
                &hex(&post)[..16]
            ));
            break;
        }
        replayed += 1;
    }
    r.check(
        format!("{replayed} tick records replayed; every pre- and post-state root matches{}", if er.is_some() { " (records read from the ER's transaction logs)" } else { "" }),
        first_bad.is_none() && replayed == records.len(),
        first_bad.clone().unwrap_or_else(|| format!("now at tick {}, {batches_seen} office batches and {gov_actions} governance actions", state.tick)),
    );
    let adopted: u32 = state.nations.iter().map(|n| n.adopted).sum();
    r.check(
        "governance replayed with the orders",
        true,
        format!(
            "{adopted} proposals adopted; officers now {:?}",
            state
                .nations
                .iter()
                .map(|n| n.offices.map(|m| if m == NOBODY { -1 } else { m as i64 }))
                .collect::<Vec<_>>()
        ),
    );

    // 5. Settlement.
    if season.status == permutation_chain::state::SeasonStatus::Finalized {
        r.check(
            "final world root matches the Season account",
            state.state_root().unwrap() == season.final_root,
            hex(&season.final_root)[..16].to_string(),
        );
        let fee_ops = season.entry_fee * rules.ops_share_bps as u64 / 10_000;
        let pool = season.member_count as u64 * (season.entry_fee - fee_ops) + state.exchange_vault;
        let p = settle(&state, &rules, pool, season.entry_fee);
        let ops = season.member_count as u64 * fee_ops + state.exchange_ops + p.dust;
        r.check(
            "every member's payout recomputed (V5 §7) matches the Season account",
            p.per_member == season.payouts && pool == season.pool,
            format!(
                "pool {} to {} members{}",
                pool,
                p.per_member.iter().filter(|x| **x > 0).count(),
                if p.refund {
                    " (refund: nobody achieved anything)"
                } else {
                    ""
                }
            ),
        );
        r.check(
            "operations share = 20% of fees + 20% of in-play income + rounding",
            ops == season.ops,
            format!("{ops}"),
        );
        let treasury: Vec<u64> = state.civs.iter().map(|c| c.usdc).collect();
        r.check(
            "treasuries returned to depositors match the final world",
            treasury == season.treasury_final,
            format!("{treasury:?}"),
        );
    } else {
        r.check(
            "season not finalized yet: settlement not checked",
            true,
            format!("status {:?}", season.status),
        );
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
        println!(
            "{}",
            if failed == 0 {
                "VERIFIED: the season replays exactly as the chain recorded it."
            } else {
                "FAILED"
            }
        );
    }
    std::process::exit(if failed == 0 { 0 } else { 1 });
}
