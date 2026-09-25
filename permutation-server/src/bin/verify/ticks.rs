//! 4. The ticks: every tick record replayed on the recomputed world, its
//!    sealed orders (with `--er`), and the governance replayed with them.

use crate::chain::{fields, published_input, tagged};
use crate::setup::officers;
use crate::Ctx;
use borsh::BorshDeserialize;
use permutation_rules::state::WorldState;
use permutation_rules::tick::{run_phase, TickInput};
use permutation_server::chainlink::ChainLink;
use permutation_server::codec::{from_hex, hex};
use serde_json::Value;
use std::collections::HashMap;

/// A tick's roots and its serialized input.
struct TickData {
    pre: Vec<u8>,
    post: Vec<u8>,
    input: Vec<u8>,
}

/// Replay every tick record (the gateway's `/ticks` index) on `state`,
/// checking each `pre_root` against the previous state and each `post_root`
/// against the recomputed one. Returns the number of records replayed.
pub fn replay(ctx: &mut Ctx, state: &mut WorldState, records: &[Value]) -> usize {
    let rules = &ctx.rules;
    let er = ctx.er.as_ref();
    let mut replayed = 0;
    let (mut gov_actions, mut batches_seen) = (0usize, 0usize);
    let mut first_bad: Option<String> = None;
    // Tick inputs reassembled from their PS_INPUT records, by hash.
    let mut published: HashMap<Vec<u8>, Option<Vec<u8>>> = Default::default();
    let (mut sealed, mut seal_bad): (usize, Option<String>) = (0, None);
    for rec in records {
        let tick = rec["tick"].as_u64().unwrap_or(0) as u16;
        let to = rec["to"].as_u64().unwrap_or(12) as u8;
        let mut t = TickData {
            pre: from_hex(rec["preRoot"].as_str().unwrap_or("")),
            post: from_hex(rec["root"].as_str().unwrap_or("")),
            input: from_hex(rec["input"].as_str().unwrap_or("")),
        };
        if let Some(er) = er {
            read_from_er(er, rec, tick, &mut t, &mut published, &mut first_bad);
        }
        if state.state_root().unwrap().to_vec() != t.pre {
            first_bad.get_or_insert(format!(
                "tick {tick}: pre-state root does not match the replayed state"
            ));
            break;
        }
        let input = TickInput::try_from_slice(&t.input).unwrap_or_default();
        if state.phase_cursor == 0 {
            gov_actions += input.gov.len();
            batches_seen += input.batches.len();
            if let Some(er) = er {
                match check_seals(er, rec, &input, &t.pre) {
                    Ok(n) => sealed += n,
                    Err(e) => {
                        seal_bad.get_or_insert(e);
                    }
                }
            }
        }
        while state.phase_cursor < to.min(12) {
            let p = state.phase_cursor;
            if run_phase(state, rules, &input, p).is_err() || state.phase_cursor == 0 {
                break;
            }
        }
        if state.state_root().unwrap().to_vec() != t.post {
            first_bad.get_or_insert(format!(
                "tick {tick}: recomputed root {} ≠ on-chain {}",
                &hex(&state.state_root().unwrap())[..16],
                &hex(&t.post)[..16]
            ));
            break;
        }
        replayed += 1;
    }
    let from_er = er.is_some();
    ctx.report.check(
        format!("{replayed} tick records replayed; every pre- and post-state root matches{}", if from_er { " (records read from the ER's transaction logs)" } else { "" }),
        first_bad.is_none() && replayed == records.len(),
        first_bad.clone().unwrap_or_else(|| format!("now at tick {}, {batches_seen} office batches and {gov_actions} governance actions", state.tick)),
    );
    if from_er {
        ctx.report.check(
            "sealed orders: every revealed batch matches its commitment, and each tick's randomness comes from the revealed salts",
            seal_bad.is_none(),
            seal_bad
                .clone()
                .unwrap_or_else(|| format!("{sealed} revealed batches checked against PS_COMMITS and PS_SALTS")),
        );
    }
    let adopted: u32 = state.nations.iter().map(|n| n.adopted).sum();
    ctx.report.check(
        "governance replayed with the orders",
        true,
        format!(
            "{adopted} proposals adopted; officers now {}",
            officers(state)
        ),
    );
    replayed
}

/// Replace a tick's gateway-indexed roots and input with the ER's own: the
/// PS_TICK record carries the roots and the input's hash; the input itself is
/// in the PS_INPUT records published before the tick could resolve. The
/// first discrepancy found goes to `first_bad`.
fn read_from_er(
    er: &ChainLink,
    rec: &Value,
    tick: u16,
    t: &mut TickData,
    published: &mut HashMap<Vec<u8>, Option<Vec<u8>>>,
    first_bad: &mut Option<String>,
) {
    match fields(er, rec["signature"].as_str(), b"PS_TICK") {
        Ok(Some(f)) if f.len() >= 6 => {
            let hash = f[5].clone();
            if !published.contains_key(&hash) {
                let chunks = published_input(er, rec["inputSignatures"].as_array(), &hash);
                published.insert(hash.clone(), chunks);
            }
            let same = f[3] == t.pre
                && f[4] == t.post
                && permutation_rules::hash::sha256(&[&t.input]).to_vec() == hash;
            if !same && first_bad.is_none() {
                *first_bad = Some(format!(
                    "tick {tick}: gateway index differs from the ER log"
                ));
            }
            match published.get(&hash).cloned().flatten() {
                Some(bytes) => t.input = bytes,
                None => {
                    first_bad.get_or_insert(format!(
                        "tick {tick}: its input is not fully published on chain (PS_INPUT)"
                    ));
                }
            }
            (t.pre, t.post) = (f[3].clone(), f[4].clone());
        }
        Ok(_) => {
            first_bad.get_or_insert(format!("tick {tick}: no PS_TICK record in its transaction"));
        }
        Err(e) => {
            first_bad.get_or_insert(format!("tick {tick}: {e}"));
        }
    }
}

/// The sealed orders of one tick (commit–reveal): every revealed batch must
/// hash to a commitment the chain logged when the commitments closed
/// (`PS_COMMITS`), and the tick randomness must be `tick_vrf(pre_root,
/// salts)` with the salts the chain logged when the input froze (`PS_SALTS`).
/// Returns the number of batches checked.
fn check_seals(
    er: &ChainLink,
    rec: &Value,
    input: &TickInput,
    pre_root: &[u8],
) -> Result<usize, String> {
    use permutation_rules::orders::order_commitment;
    use permutation_rules::rng::{tick_vrf, Salt};
    let tick = rec["tick"].as_u64().unwrap_or(0);
    let commits_rec = fields(er, rec["commitSignature"].as_str(), b"PS_COMMITS")?.ok_or(
        format!("tick {tick}: no PS_COMMITS record (commitSignature)"),
    )?;
    let commits: Vec<(u16, u8, u32, [u8; 32])> =
        BorshDeserialize::try_from_slice(commits_rec.get(2).ok_or("short PS_COMMITS")?)
            .map_err(|e| format!("tick {tick}: PS_COMMITS: {e}"))?;
    // Every chunk-0 transaction logs the salts with the root it saw; take the
    // one made on this tick's pre-state root.
    let mut salts_rec = None;
    for sig in rec["inputSignatures"].as_array().into_iter().flatten() {
        let Some(sig) = sig.as_str() else { continue };
        if let Some(f) = er
            .log_records(sig)?
            .into_iter()
            .find(|f| tagged(f, b"PS_SALTS") && f.get(2).map(|r| r.as_slice()) == Some(pre_root))
        {
            salts_rec = Some(f);
            break;
        }
    }
    let salts_rec = salts_rec.ok_or(format!(
        "tick {tick}: no PS_SALTS record on its pre-state root"
    ))?;
    let salts: Vec<Salt> =
        BorshDeserialize::try_from_slice(salts_rec.get(3).ok_or("short PS_SALTS")?)
            .map_err(|e| format!("tick {tick}: PS_SALTS: {e}"))?;
    let root: [u8; 32] = pre_root.try_into().map_err(|_| "pre-root length")?;
    if tick_vrf(&root, &salts) != input.vrf {
        return Err(format!(
            "tick {tick}: the randomness is not derived from the revealed salts"
        ));
    }
    if salts.len() != input.batches.len() {
        return Err(format!(
            "tick {tick}: {} salts for {} batches",
            salts.len(),
            input.batches.len()
        ));
    }
    for (b, (civ, role, salt)) in input.batches.iter().zip(&salts) {
        if b.civ != *civ || b.role as u8 != *role {
            return Err(format!("tick {tick}: salts and batches out of order"));
        }
        let c = order_commitment(b, salt);
        if !commits
            .iter()
            .any(|(cc, rr, m, h)| *cc == b.civ && *rr == b.role as u8 && *m == b.member && *h == c)
        {
            return Err(format!(
                "tick {tick}: nation {} office {:?}: revealed orders match no commitment",
                b.civ, b.role
            ));
        }
    }
    Ok(input.batches.len())
}
