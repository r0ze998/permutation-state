//! 4. The ticks: the chain of state roots followed through every `PS_TICK`
//!    record of this season, read from the ER's transaction logs. The
//!    gateway's `/ticks` index only says which transactions to read first;
//!    when data is missing, the ER history of world chunk 0 is scanned in
//!    the slot window where it must lie. Then the index is checked against
//!    what the chain says.

use crate::chain::{
    fetch_many, signatures_in, window_for, LiveWorld, PrefixData, ScanError, Window,
};
use crate::report::Status;
use crate::setup::officers;
use crate::Ctx;
use permutation_rules::state::WorldState;
use permutation_server::codec::{from_hex, hex};
use permutation_server::replay::{follow, short, ChainData, Missing, Outcome, SealCtx};
use serde_json::Value;
use std::collections::HashSet;

/// What the replay must reach, read from the chain before the index.
pub enum Target {
    /// Finalized: `Season.final_root`.
    Final([u8; 32]),
    /// Running: the live world.
    Live(LiveWorld),
    /// Running, but the world could not be read (why).
    Unread(String),
}

impl Target {
    fn root(&self) -> Option<&[u8; 32]> {
        match self {
            Target::Final(r) => Some(r),
            Target::Live(w) => Some(&w.root),
            Target::Unread(_) => None,
        }
    }

    fn reached(&self, out: &Outcome) -> bool {
        match self {
            Target::Final(r) => out.fail.is_none() && out.end_root == *r,
            Target::Live(w) => out.visited.contains(&w.root),
            Target::Unread(_) => false,
        }
    }
}

/// The transactions an index line names (the resolve part, its input's and
/// its close), in order, once each.
fn hinted(lines: &[Value]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for l in lines {
        let sigs = std::iter::once(&l["signature"])
            .chain(l["inputSignatures"].as_array().into_iter().flatten())
            .chain(std::iter::once(&l["commitSignature"]));
        for s in sigs.filter_map(|s| s.as_str()) {
            if seen.insert(s.to_string()) {
                out.push(s.to_string());
            }
        }
    }
    out
}

/// Scan one window of world chunk 0's ER history into `data`. Returns the
/// transactions read, or why the scan stopped.
fn scan(
    ctx: &Ctx,
    data: &mut ChainData,
    w: &Window,
    unreadable: &mut usize,
) -> Result<usize, String> {
    let sigs = match signatures_in(
        &ctx.er,
        &ctx.anchor,
        w.lo,
        w.hi,
        w.start.as_deref(),
        &ctx.budget,
    ) {
        Ok(s) => s,
        Err(ScanError::Budget) => return Err(budget_note(ctx)),
        Err(ScanError::Rpc(e)) => return Err(format!("the ER history could not be listed: {e}")),
    };
    let new: Vec<String> = sigs
        .into_iter()
        .map(|(s, _)| s)
        .filter(|s| !data.fetched.contains_key(s))
        .collect();
    let got = fetch_many(
        &ctx.er,
        &ctx.program_id,
        &new,
        ctx.threads,
        Some(&ctx.budget),
        &format!("world chunk 0, {}", w.slots()),
    );
    for (_, r) in &got {
        match r {
            Ok(tx) => data.add_tx(tx),
            Err(_) => *unreadable += 1,
        }
    }
    if ctx.budget.exhausted() {
        return Err(budget_note(ctx));
    }
    Ok(got.len())
}

fn budget_note(ctx: &Ctx) -> String {
    format!(
        "scan limit reached: {} transactions (rerun with --max-scan)",
        ctx.budget.max()
    )
}

/// Why the replay could not go on at `m` (after its window was scanned).
fn missing_note(m: &Missing, data: &ChainData, w: Option<&Window>) -> String {
    let at = w
        .map(|w| format!(" (not in the ER history of world chunk 0, {})", w.slots()))
        .unwrap_or_default();
    match m {
        Missing::Successor { tick, root } => {
            format!(
                "tick {tick}: no tick record continues from root {}{at}",
                &hex(root)[..16]
            )
        }
        Missing::Input { tick, hash } => format!(
            "tick {tick}: its input is not fully published on chain ({})",
            data.input_gap(hash)
        ),
        Missing::Commits { tick } => {
            format!("tick {tick}: no CloseCommits of this season found{at}")
        }
        Missing::Salts { tick } => {
            format!("tick {tick}: no PS_SALTS on its pre-state root found{at}")
        }
    }
}

/// Replay every tick from `state` (after the first election) to `target`,
/// print the lines, and check the gateway's index (`lines`, `/ticks`)
/// against the chain. Returns the number of records replayed.
pub fn replay(
    ctx: &mut Ctx,
    state: &mut WorldState,
    lines: &[Value],
    prefix: &PrefixData,
    target: &Target,
) -> usize {
    let start = state.clone();
    let mut data = ChainData::new(&ctx.anchor);
    let hints = hinted(lines);
    let mut unreadable = 0;
    for (_, r) in fetch_many(
        &ctx.er,
        &ctx.program_id,
        &hints,
        ctx.threads,
        None,
        "the index",
    ) {
        match r {
            Ok(tx) => data.add_tx(&tx),
            Err(_) => unreadable += 1,
        }
    }
    let season = SealCtx {
        nations: ctx.season.nations as u16,
        season_id: ctx.season.season_id,
        anchor: ctx.anchor.clone(),
    };
    let mut scanned: Vec<Window> = Vec::new();
    let mut stopped: Option<String> = None; // why scanning stopped (budget, RPC)
    let mut scan_txs = 0;
    if ctx.scan {
        let all = Window {
            lo: 0,
            hi: None,
            start: None,
        };
        match scan(ctx, &mut data, &all, &mut unreadable) {
            Ok(n) => scan_txs += n,
            Err(e) => stopped = Some(e),
        }
        scanned.push(all);
    }
    let (out, window) = loop {
        *state = start.clone();
        let out = follow(state, &ctx.rules, &data, &season, target.root());
        if out.fail.is_some() || target.reached(&out) || stopped.is_some() {
            break (out, None);
        }
        let Some(m) = &out.missing else {
            break (out, None);
        };
        let w = window_for(m, &out, &data);
        if scanned
            .iter()
            .any(|s| s.lo <= w.lo && s.hi.is_none_or(|h| w.hi.is_some_and(|x| x <= h)))
        {
            break (out, Some(w));
        }
        match scan(ctx, &mut data, &w, &mut unreadable) {
            Ok(n) => scan_txs += n,
            Err(e) => stopped = Some(e),
        }
        scanned.push(w);
    };
    let reached = target.reached(&out);
    let r = &mut ctx.report;

    // Where the records came from.
    let mut source = if scanned.is_empty() {
        format!(
            "{} transactions named by the gateway's index",
            data.fetched.len()
        )
    } else {
        format!(
            "the gateway's index named {} transactions{}; the ER history of world chunk 0 was scanned in {} window(s) ({} transactions: {})",
            hints.len(),
            if ctx.scan { "" } else { " and was incomplete" },
            scanned.len(),
            scan_txs,
            scanned.iter().map(|w| w.slots()).collect::<Vec<_>>().join(", ")
        )
    };
    if data.foreign > 0 {
        source.push_str(&format!(
            "; {} PS_COMMITS of other seasons ignored",
            data.foreign
        ));
    }
    if unreadable > 0 {
        source.push_str(&format!("; {unreadable} transactions could not be fetched"));
    }
    r.info("tick records located", source);
    if !data.malformed.is_empty() {
        r.check(
            "every record of the season program decodes",
            false,
            format!(
                "{} do not; first: {}",
                data.malformed.len(),
                data.malformed[0]
            ),
        );
    }

    // The replay.
    let what = format!(
        "{} tick records replayed from the ER's logs; every pre- and post-state root matches",
        out.steps
    );
    let seal_fail = out.fail.as_ref().filter(|_| out.seal_fail);
    match (&out.fail, seal_fail) {
        (Some(_), Some(_)) => r.info(
            what,
            format!("stopped at tick {} (sealed orders below)", state.tick),
        ),
        (Some(f), None) => r.check(what, false, f.clone()),
        (None, _) if reached => r.check(
            what,
            true,
            format!(
                "now at tick {}, {} office batches and {} governance actions",
                state.tick, out.seals.batches, out.seals.gov
            ),
        ),
        (None, _) => r.incomplete(
            what,
            match (&stopped, &out.missing) {
                (Some(why), _) => why.clone(),
                (None, Some(m)) => missing_note(m, &data, window.as_ref()),
                (None, None) => format!("the replay stops at tick {}", state.tick),
            },
        ),
    }

    // A running season: the replay must reach the live world.
    if let Some((status, detail)) = live_line(target, reached, state.tick) {
        r.line(status, "the replay reaches the live world", detail);
    }

    // The sealed orders.
    let s = &out.seals;
    let what = "sealed orders: every commitment was made by the office holder, every revealed batch matches its commitment (PS_COMMITS) and its salt (PS_SALTS), and each tick's randomness follows from the revealed salts";
    let counts = format!(
        "{} batches against {} commitments over {} ticks",
        s.batches, s.commitments, s.ticks
    );
    match seal_fail {
        Some(f) => r.check(what, false, f.clone()),
        None if reached => r.check(what, true, counts),
        None => r.incomplete(what, format!("{counts}; the rest could not be read")),
    }
    if !s.unrevealed.is_empty() {
        let first: Vec<String> = s
            .unrevealed
            .iter()
            .take(20)
            .map(|(t, c, role, m)| format!("t{t} nation {c} office {role} member {m}"))
            .collect();
        r.info(
            format!("committed but not revealed: {} offices", s.unrevealed.len()),
            format!(
                "first {}: {} — a batch not revealed in the window does not run; a reveal the ER dropped would also show here",
                first.len(),
                first.join(", ")
            ),
        );
    }
    let adopted: u32 = state.nations.iter().map(|n| n.adopted).sum();
    r.info(
        "governance replayed with the orders",
        format!(
            "{adopted} proposals adopted; officers now {}",
            officers(state)
        ),
    );

    // The gateway's index, checked against the chain.
    let bad = index_contradictions(lines, &data, &ctx.info, prefix, &ctx.season.season_seed);
    r.check(
        "the gateway's index agrees with the chain",
        bad.is_empty(),
        if bad.is_empty() {
            format!("{} lines", lines.len())
        } else {
            format!("{} contradictions; first: {}", bad.len(), bad[0])
        },
    );
    out.steps
}

/// The live-world line of a running season (none for a finalized one):
/// reached, or where the replay stopped short of it.
fn live_line(target: &Target, reached: bool, tick: u16) -> Option<(Status, String)> {
    match target {
        Target::Final(_) => None,
        Target::Live(w) => {
            let at = format!(
                "tick {}, phase {}, root {}, slot {}, read from the {}",
                w.tick,
                w.phase,
                &hex(&w.root)[..16],
                w.slot,
                w.layer
            );
            Some(if reached {
                (Status::Pass, at)
            } else {
                (
                    Status::Incomplete,
                    format!("the replay stops at tick {tick} but the live world is at {at}"),
                )
            })
        }
        Target::Unread(why) => Some((Status::Incomplete, why.clone())),
    }
}

fn root_of(v: &Value) -> Option<[u8; 32]> {
    from_hex(v.as_str()?).try_into().ok()
}

/// Every claim of the gateway's index that the chain contradicts: a line
/// whose transaction holds no `PS_TICK` with its (tick, stop, roots, input
/// hash), an input that does not hash to its `inputHash`, a
/// `commitSignature` without this season's `PS_COMMITS` for the tick, and
/// `/season` genesis, seating and open roots that differ from the records
/// in their transactions. A gap (a line absent, a transaction not read, a
/// null `commitSignature`) is not a contradiction.
pub fn index_contradictions(
    lines: &[Value],
    data: &ChainData,
    info: &Value,
    prefix: &PrefixData,
    seed: &[u8; 32],
) -> Vec<String> {
    let mut bad = Vec::new();
    for l in lines {
        let tick = l["tick"].as_u64().unwrap_or(u64::MAX);
        let to = l["to"].as_u64().unwrap_or(u64::MAX).min(12);
        let (pre, post, hash) = (
            root_of(&l["preRoot"]),
            root_of(&l["root"]),
            root_of(&l["inputHash"]),
        );
        let degraded = l["degraded"].as_bool().unwrap_or(false);
        if let Some(sig) = l["signature"]
            .as_str()
            .filter(|s| data.fetched.contains_key(*s))
        {
            let found = data.ticks.iter().any(|t| {
                t.sig == sig
                    && t.tick as u64 == tick
                    && t.stop as u64 == to
                    && t.degraded == degraded
                    && Some(t.pre) == pre
                    && Some(t.post) == post
                    && Some(t.input_hash) == hash
            });
            if !found {
                bad.push(format!(
                    "tick {tick} line: its transaction {} has no PS_TICK from root {} to {} (to {to})",
                    short(sig),
                    short(l["preRoot"].as_str().unwrap_or("")),
                    short(l["root"].as_str().unwrap_or(""))
                ));
            }
        }
        if let Some(input) = l["input"].as_str().filter(|i| !i.is_empty()) {
            if Some(permutation_rules::hash::sha256(&[&from_hex(input)])) != hash {
                bad.push(format!(
                    "tick {tick} line: its input does not hash to its inputHash"
                ));
            }
        }
        if let Some(sig) = l["commitSignature"]
            .as_str()
            .filter(|s| data.fetched.contains_key(*s))
        {
            let bound = data
                .commits
                .get(&(tick as u16))
                .is_some_and(|c| c.iter().any(|(s, _)| s == sig));
            if !bound {
                bad.push(format!(
                    "tick {tick} line: commitSignature {} holds no PS_COMMITS of this season for the tick",
                    short(sig)
                ));
            }
        }
    }
    // The /season claims about the base layer.
    fn read<'a>(v: &'a Value, prefix: &PrefixData) -> Option<&'a str> {
        v["signature"]
            .as_str()
            .filter(|s| prefix.fetched.contains(*s))
    }
    if let Some(sig) = read(&info["genesis"], prefix) {
        let root = root_of(&info["genesis"]["root"]);
        if !prefix
            .genesis
            .iter()
            .any(|g| g.2 == sig && g.1 == *seed && Some(g.0) == root)
        {
            bad.push(format!("/season genesis: its transaction {} holds no PS_GENESIS of this season with that root", short(sig)));
        }
    }
    for (i, s) in info["seating"].as_array().into_iter().flatten().enumerate() {
        if let Some(sig) = read(s, prefix) {
            let root = root_of(&s["root"]);
            if !prefix
                .seats
                .iter()
                .any(|x| x.2 == sig && x.3 && Some(x.0) == root)
            {
                bad.push(format!("/season seating[{i}]: its transaction {} holds no PS_SEAT of this season with that root", short(sig)));
            }
        }
    }
    if let Some(sig) = read(&info["open"], prefix) {
        let root = root_of(&info["open"]["root"]);
        if !prefix
            .open
            .iter()
            .any(|o| o.1 == sig && o.2 && Some(o.0) == root)
        {
            bad.push(format!(
                "/season open: its transaction {} holds no PS_OPEN of this season with that root",
                short(sig)
            ));
        }
    }
    bad
}

#[cfg(test)]
mod tests {
    use super::*;
    use permutation_rules::genesis::{nation_entries, new_season};
    use permutation_rules::gov;
    use permutation_rules::tick::{run_phase, TickInput};
    use permutation_rules::{Preset, Ruleset};
    use permutation_server::replay::{Emitted, TxRecords};
    use serde_json::json;

    const X: &str = "X0";

    fn emitted(fields: Vec<Vec<u8>>, anchor: &str) -> Emitted {
        Emitted {
            record: fields,
            accounts: Some(vec![anchor.into()]),
        }
    }

    /// A 2-nation world with no members (every office vacant, so no sealed
    /// orders) after the first election, and `n` ticks of it as the program
    /// logs them: per tick a close, an input and a resolve transaction.
    /// Returns the start, the end and the transactions.
    fn played(n: u16) -> (Ruleset, WorldState, WorldState, Vec<TxRecords>) {
        let rules = Ruleset::new(Preset::Blitz);
        let mut s = new_season(&rules, &[1; 32], &[2; 32], &nation_entries(2)).unwrap();
        gov::first_election(&mut s, &rules).unwrap();
        let start = s.clone();
        let mut txs = Vec::new();
        for t in 0..n {
            let pre = s.state_root().unwrap();
            let input = TickInput {
                vrf: permutation_server::replay::tick_vrf_v8(&pre, &[]),
                ..Default::default()
            };
            let bytes = borsh::to_vec(&input).unwrap();
            let hash = permutation_rules::hash::sha256(&[&bytes]);
            let empty = borsh::to_vec(&Vec::<u8>::new()).unwrap();
            let tick = t.to_le_bytes().to_vec();
            for p in 0..12 {
                run_phase(&mut s, &rules, &input, p).unwrap();
            }
            let post = s.state_root().unwrap();
            let slot = 100 + 3 * t as u64;
            let tx = |sig: String, slot: u64, e: Emitted| TxRecords {
                sig,
                slot,
                emitted: vec![e],
            };
            txs.push(tx(
                format!("close{t}"),
                slot,
                emitted(vec![b"PS_COMMITS".to_vec(), tick.clone(), empty.clone()], X),
            ));
            txs.push(TxRecords {
                sig: format!("input{t}"),
                slot: slot + 1,
                emitted: vec![
                    emitted(
                        vec![
                            b"PS_SALTS".to_vec(),
                            tick.clone(),
                            pre.to_vec(),
                            empty.clone(),
                        ],
                        X,
                    ),
                    emitted(
                        vec![
                            b"PS_INPUT".to_vec(),
                            tick.clone(),
                            0u16.to_le_bytes().to_vec(),
                            1u16.to_le_bytes().to_vec(),
                            hash.to_vec(),
                            bytes,
                        ],
                        X,
                    ),
                ],
            });
            txs.push(tx(
                format!("resolve{t}"),
                slot + 2,
                emitted(
                    vec![
                        b"PS_TICK".to_vec(),
                        tick.clone(),
                        vec![12],
                        pre.to_vec(),
                        post.to_vec(),
                        hash.to_vec(),
                    ],
                    X,
                ),
            ));
        }
        (rules, start, s, txs)
    }

    fn seal_ctx() -> SealCtx {
        SealCtx {
            nations: 2,
            season_id: 7,
            anchor: X.into(),
        }
    }

    #[test]
    fn live_world_target() {
        let (rules, start, end, txs) = played(3);
        let live = Target::Live(LiveWorld {
            root: end.state_root().unwrap(),
            tick: 3,
            phase: 0,
            slot: 120,
            layer: "ER",
        });
        // Nothing read (an empty index, nothing found): not verified.
        let data = ChainData::new(X);
        let mut s = start.clone();
        let out = follow(&mut s, &rules, &data, &seal_ctx(), live.root());
        assert!(!live.reached(&out));
        let (status, detail) = live_line(&live, false, s.tick).unwrap();
        assert_eq!(status, Status::Incomplete);
        assert!(
            detail.starts_with("the replay stops at tick 0 but the live world is at tick 3"),
            "{detail}"
        );
        // With the three ticks' records: reached.
        let mut data = ChainData::new(X);
        txs.iter().for_each(|t| data.add_tx(t));
        let mut s = start.clone();
        let out = follow(&mut s, &rules, &data, &seal_ctx(), live.root());
        assert_eq!(
            (out.fail.clone(), out.missing.clone(), out.steps),
            (None, None, 3)
        );
        assert!(live.reached(&out));
        assert_eq!(live_line(&live, true, s.tick).unwrap().0, Status::Pass);
        assert!(live_line(&Target::Final([0; 32]), true, 3).is_none());
        let unread = Target::Unread("could not read the world accounts: down".into());
        assert_eq!(live_line(&unread, false, 0).unwrap().0, Status::Incomplete);
    }

    /// The index lines an honest gateway writes for `txs` (tickLine).
    fn lines_of(txs: &[TxRecords]) -> Vec<Value> {
        let mut out = Vec::new();
        for t in 0..txs.len() / 3 {
            let (close, input, resolve) = (&txs[3 * t], &txs[3 * t + 1], &txs[3 * t + 2]);
            let f = &resolve.emitted[0].record;
            out.push(json!({
                "tick": t, "to": f[2][0], "preRoot": hex(&f[3]), "root": hex(&f[4]),
                "input": hex(&input.emitted[1].record[5]), "inputHash": hex(&f[5]),
                "inputSignatures": [input.sig], "signature": resolve.sig, "commitSignature": close.sig,
            }));
        }
        out
    }

    fn contradictions(lines: &[Value], txs: &[TxRecords]) -> Vec<String> {
        let mut data = ChainData::new(X);
        txs.iter().for_each(|t| data.add_tx(t));
        index_contradictions(lines, &data, &Value::Null, &PrefixData::default(), &[2; 32])
    }

    #[test]
    fn index_consistency() {
        let (_, _, _, txs) = played(3);
        let lines = lines_of(&txs);
        assert_eq!(contradictions(&lines, &txs), Vec::<String>::new());
        assert_eq!(hinted(&lines).len(), 9);
        // A root that differs from its record.
        let mut bad = lines.clone();
        bad[1]["root"] = json!(hex(&[7; 32]));
        let c = contradictions(&bad, &txs);
        assert_eq!(c.len(), 1);
        assert!(c[0].contains("has no PS_TICK"), "{}", c[0]);
        // commitSignature naming a transaction without PS_COMMITS...
        let mut bad = lines.clone();
        bad[0]["commitSignature"] = json!("input0");
        assert!(contradictions(&bad, &txs)[0].contains("holds no PS_COMMITS"));
        // ...or another season's close for the same tick.
        let mut txs2 = txs.clone();
        txs2.push(TxRecords {
            sig: "foreign-close".into(),
            slot: 99,
            emitted: vec![emitted(
                vec![
                    b"PS_COMMITS".to_vec(),
                    0u16.to_le_bytes().to_vec(),
                    vec![0, 0, 0, 0],
                ],
                "Y0",
            )],
        });
        let mut bad = lines.clone();
        bad[0]["commitSignature"] = json!("foreign-close");
        assert_eq!(contradictions(&bad, &txs2).len(), 1);
        // A missing line or a null commitSignature is a gap, not a contradiction.
        let mut gaps = lines.clone();
        gaps.remove(1);
        gaps[0]["commitSignature"] = Value::Null;
        assert!(contradictions(&gaps, &txs).is_empty());
        // An input that does not hash to its inputHash.
        let mut bad = lines.clone();
        bad[2]["input"] = json!("00");
        assert!(contradictions(&bad, &txs)[0].contains("does not hash"));
        // An old line with `to: 255` whose record has `to = 255`: consistent.
        let mut raw = txs.clone();
        raw[8].emitted[0].record[2] = vec![255];
        let mut old = lines.clone();
        old[2]["to"] = json!(255);
        assert!(contradictions(&old, &raw).is_empty());
        // /season claims against the base layer's records.
        let mut p = PrefixData::default();
        p.fetched.extend(["g".to_string(), "o".to_string()]);
        p.genesis.push(([5; 32], [2; 32], "g".into()));
        p.open.push(([6; 32], "o".into(), true));
        let info = json!({"genesis": {"signature": "g", "root": hex(&[5; 32])}, "open": {"signature": "o", "root": hex(&[6; 32])}});
        assert!(index_contradictions(&[], &ChainData::new(X), &info, &p, &[2; 32]).is_empty());
        let info = json!({"genesis": {"signature": "g", "root": hex(&[5; 32])}, "open": {"signature": "o", "root": hex(&[8; 32])}});
        assert_eq!(
            index_contradictions(&[], &ChainData::new(X), &info, &p, &[2; 32]).len(),
            1
        );
    }
}
