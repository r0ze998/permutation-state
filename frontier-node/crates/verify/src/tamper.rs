//! Tamper classes T1–T22 (contract §8.5): each builds a tampered copy of a
//! recorded fixture that **must FAIL with the named code**, and — built
//! with the `mutate-<check>` feature that disables the check it targets —
//! must then PASS (the checks of the checks; `tests/tamper.rs`,
//! `mutate.sh`).
//!
//! Some classes are byte edits the chain itself exposes (a dropped or
//! duplicated transaction, a truncated archive, a rogue account write).
//! The others are **consistent forgeries**: the edited record is re-chained
//! ([`rechain`]: every later head, every post-state header and every final
//! header recomputed) and whatever else a careful forger would adjust is
//! adjusted (payments, dependent digests), so only the check under test can
//! see the lie — which is what lets its `mutate-` build PASS.

use std::collections::HashMap;

use fclient::log::{bodies_from_logs, log_line};
use fclient::ports::TxRecord;
use frontier_abi::layout::clash::{arrival as AR, clash_inputs as CI};
use frontier_abi::layout::header as HD;
use frontier_abi::layout::player::holding as H;
use frontier_abi::log::{self as plog, transit_outcome, Kind, Link};
use frontier_abi::tags::Ix;

use crate::checks::v1_chains::match_links;
use crate::checks::v7_replay::packed_fates;
use crate::clash_input::{ClashBuilder, ContractBuilder};
use crate::codes::*;
use crate::facts::Facts;
use crate::input::Input;
use crate::world::{le, Key, World};

/// Chain state per account.
type ChainState = HashMap<Key, (u64, [u8; 32])>;
/// Per transaction, the chain states of the accounts it wrote.
type PostHeads = Vec<(usize, Vec<(Key, (u64, [u8; 32]))>)>;

/// One tamper case.
pub struct Case {
    /// `T1` … `T22` (and `T6b`).
    pub class: &'static str,
    pub what: &'static str,
    /// The check whose `mutate-` feature must let it PASS (`None`: no such
    /// requirement, e.g. T6b's second detection path).
    pub feature: Option<&'static str>,
    /// Any of these FAIL codes satisfies the class.
    pub codes: &'static [&'static str],
    pub input: Input,
}

/// A tamper case, or why the run has nothing it could tamper with.
pub type Made = Result<Case, String>;

/// `Option`/`Result` → `Result<_, String>` with the selection's name (a
/// run that lacks what a class needs is reported, not a panic: W5-D, the
/// classes run on any stack run).
pub trait Need<T> {
    fn need(self, what: &str) -> Result<T, String>;
}

impl<T> Need<T> for Option<T> {
    fn need(self, what: &str) -> Result<T, String> {
        self.ok_or_else(|| format!("the run has no {what}"))
    }
}

impl<T, E: std::fmt::Debug> Need<T> for Result<T, E> {
    fn need(self, what: &str) -> Result<T, String> {
        self.map_err(|e| format!("{what}: {e:?}"))
    }
}

// ------------------------------------------------------------ helpers

/// PS2 bodies of a transaction (the program's frame).
pub fn bodies(inp: &Input, t: usize) -> Vec<Vec<u8>> {
    bodies_from_logs(&inp.txs[t].logs, &inp.cfg.program).unwrap_or_default()
}

/// Rewrites the program's PS2 lines of transaction `t` (same count).
pub fn set_bodies(inp: &mut Input, t: usize, new: &[Vec<u8>]) {
    let id = inp.cfg.program.to_string();
    let tx: &mut TxRecord = &mut inp.txs[t];
    let mut stack: Vec<String> = vec![];
    let mut i = 0;
    for l in tx.logs.iter_mut() {
        if let Some(rest) = l.strip_prefix("Program data: ") {
            let ours = stack.last().map(|s| s == &id).unwrap_or(false);
            let is_ps2 = fclient::log::body_of_line(&format!("Program data: {rest}"))
                .ok()
                .flatten()
                .is_some();
            if ours && is_ps2 {
                if let Some(b) = new.get(i) {
                    *l = log_line(b);
                }
                i += 1;
            }
            continue;
        }
        let Some(rest) = l.strip_prefix("Program ") else {
            continue;
        };
        let Some((who, tail)) = rest.split_once(' ') else {
            continue;
        };
        if tail.starts_with("invoke [") {
            stack.push(who.to_string());
        } else if (tail == "success" || tail.starts_with("failed"))
            && stack.last().map(|s| s.as_str()) == Some(who)
        {
            stack.pop();
        }
    }
}

/// `(transaction, position)` of every record of `kind`, in order.
pub fn find(inp: &Input, kind: Kind) -> Vec<(usize, usize)> {
    let mut v = vec![];
    for t in 0..inp.txs.len() {
        if inp.txs[t].err.is_some() {
            continue;
        }
        for (i, b) in bodies(inp, t).iter().enumerate() {
            if b.get(1) == Some(&(kind as u8)) {
                v.push((t, i));
            }
        }
    }
    v
}

/// The `n`-th of `v`, or its last (a shorter run than the fixture).
pub fn pick(v: &[(usize, usize)], n: usize, what: &str) -> Result<(usize, usize), String> {
    v.get(n)
        .or(v.last())
        .copied()
        .ok_or_else(|| format!("the run has no {what}"))
}

/// Edits the payload of record `(t, i)` (`f` gets the payload bytes).
pub fn edit_payload(inp: &mut Input, t: usize, i: usize, f: impl FnOnce(&mut [u8])) {
    let mut bs = bodies(inp, t);
    let b = &mut bs[i];
    let r = plog::decode(b).expect("record decodes");
    let off = plog::HEAD_LEN + r.key.len();
    let n = r.payload.len();
    f(&mut b[off..off + n]);
    set_bodies(inp, t, &bs);
}

/// Edits the key of record `(t, i)`.
pub fn edit_key(inp: &mut Input, t: usize, i: usize, f: impl FnOnce(&mut [u8])) {
    let mut bs = bodies(inp, t);
    let b = &mut bs[i];
    let r = plog::decode(b).expect("record decodes");
    let n = r.key.len();
    f(&mut b[plog::HEAD_LEN..plog::HEAD_LEN + n]);
    set_bodies(inp, t, &bs);
}

/// A payload field's `(offset, width)`.
pub fn field(kind: Kind, name: &str) -> (usize, usize) {
    plog::field(kind, name, true).expect("field")
}

/// Edits the data of the first program instruction with `tag` in
/// transaction `t` (the signatures are left as they were: the verifier
/// reads what the archive says the chain accepted).
pub fn edit_ix(inp: &mut Input, t: usize, tag: Ix, f: impl FnOnce(&mut Vec<u8>)) {
    let mut tx = fclient::tx::from_wire(&inp.txs[t].tx).expect("tx");
    let program = inp.cfg.program;
    let keys = tx.message.account_keys.clone();
    let ci = tx
        .message
        .instructions
        .iter_mut()
        .find(|c| {
            keys[c.program_id_index as usize] == program && c.data.first() == Some(&tag.tag())
        })
        .expect("instruction");
    f(&mut ci.data);
    inp.txs[t].tx = fclient::tx::wire(&tx);
}

/// Re-chains every record of the archive (the consistent forger): every
/// tail link and CLOSE payload is recomputed from the records as they now
/// stand, and every post-state and final header of a chained account is
/// set to the chain's state after that transaction.
pub fn rechain(inp: &mut Input) {
    let w = World::parse(
        &inp.cfg.program,
        &inp.txs,
        inp.finals.clone(),
        inp.final_slot,
    );
    let f = Facts::build(&w, &inp.cfg);
    let mut kinds = HashMap::new();
    let mut st: ChainState = HashMap::new();
    let mut per_tx: HashMap<usize, Vec<Vec<u8>>> = HashMap::new();
    let mut posts: PostHeads = vec![];
    let mut last_tx = None;
    let flush = |t: usize, st: &ChainState, w: &World, posts: &mut PostHeads| {
        let v: Vec<(Key, (u64, [u8; 32]))> = w.txs[t]
            .post
            .keys()
            .filter_map(|k| st.get(k).map(|x| (*k, *x)))
            .collect();
        posts.push((t, v));
    };
    for (n, r) in w.recs.iter().enumerate() {
        if last_tx.is_some_and(|t| t != r.tx) {
            flush(last_tx.unwrap_or(0), &st, &w, &mut posts);
        }
        last_tx = Some(r.tx);
        let mut bwt = r.bwt.clone();
        let links = match_links(&w, &f, &mut kinds, n, r).unwrap_or_default();
        if r.kind == Kind::CLOSE {
            if let Some((_, _, k)) = links.first() {
                let cur = st.get(k).copied().unwrap_or((0, [0; 32]));
                let o = plog::HEAD_LEN + r.key.len();
                bwt[o..o + 8].copy_from_slice(&cur.0.to_le_bytes());
                bwt[o + 8..o + 40].copy_from_slice(&cur.1);
            }
        }
        let mut new_links: Vec<Link> = r.links.clone();
        for (li, e, k) in &links {
            let (seq, head) = st.get(k).copied().unwrap_or((0, [0; 32]));
            let l = plog::advance(*e, seq, &head, &bwt).expect("seq");
            new_links[*li] = l;
            if r.kind == Kind::CLOSE {
                st.remove(k);
            } else {
                st.insert(*k, (l.seq, l.head));
            }
        }
        let mut body = bwt;
        body.push(new_links.len() as u8);
        for l in &new_links {
            body.push(l.entity as u8);
            body.extend_from_slice(&l.seq.to_le_bytes());
            body.extend_from_slice(&l.head);
        }
        per_tx.entry(r.tx).or_default().push(body);
    }
    if let Some(t) = last_tx {
        flush(t, &st, &w, &mut posts);
    }
    for (t, bs) in per_tx {
        set_bodies(inp, t, &bs);
    }
    for (t, v) in posts {
        for (k, (seq, head)) in v {
            if let Some((_, Some(a))) = inp.txs[t].post.iter_mut().find(|(x, _)| x.to_bytes() == k)
            {
                set_header(&mut a.data, seq, &head);
            }
        }
    }
    for (k, (seq, head)) in &st {
        if let Some(Some(a)) = inp.finals.get_mut(k) {
            set_header(&mut a.data, *seq, head);
        }
    }
}

fn set_header(d: &mut [u8], seq: u64, head: &[u8; 32]) {
    if d.len() >= HD::H_SIZE {
        d[HD::EVENT_SEQ..HD::EVENT_SEQ + 8].copy_from_slice(&seq.to_le_bytes());
        d[HD::EVENT_HEAD..HD::EVENT_HEAD + 32].copy_from_slice(head);
    }
}

/// Sets bytes of account `k` in every post-state from transaction `from`
/// on and in the finals.
fn edit_account_from(inp: &mut Input, k: &Key, from: usize, f: impl Fn(&mut Vec<u8>)) {
    for t in inp.txs.iter_mut().skip(from) {
        for (x, a) in t.post.iter_mut() {
            if x.to_bytes() == *k {
                if let Some(a) = a {
                    f(&mut a.data);
                }
            }
        }
    }
    if let Some(Some(a)) = inp.finals.get_mut(k) {
        f(&mut a.data);
    }
}

fn world(inp: &Input) -> (World, Facts) {
    let w = World::parse(
        &inp.cfg.program,
        &inp.txs,
        inp.finals.clone(),
        inp.final_slot,
    );
    let f = Facts::build(&w, &inp.cfg);
    (w, f)
}

/// The record index (in [`World::recs`]) of `(t, i)`.
fn rec_at(w: &World, t: usize, i: usize) -> usize {
    w.txs[t].recs[i]
}

// ------------------------------------------------------------ the classes

/// T1: drop the Depart of the march still in flight at the end.
pub fn t01(march: &Input) -> Made {
    let mut inp = march.clone();
    let (t, _) = *find(&inp, Kind::DEPART).last().need("a DEPART")?;
    inp.txs.remove(t);
    Ok(Case {
        class: "T1",
        what: "drop a Depart",
        feature: Some("mutate-v1"),
        codes: &[CHAIN_GAP, HEAD_MISMATCH],
        input: inp,
    })
}

/// T2: flip a byte of a Reveal's plaintext (the stance).
pub fn t02(march: &Input) -> Made {
    let mut inp = march.clone();
    let (t, _) = pick(&find(&inp, Kind::REVEAL), 1, "REVEAL record")?;
    edit_ix(&mut inp, t, Ix::Reveal, |d| d[1 + 2 + 20] ^= 1);
    Ok(Case {
        class: "T2",
        what: "flip a Reveal plaintext byte",
        feature: Some("mutate-v5"),
        codes: &[REVEAL_COMMIT_MISMATCH],
        input: inp,
    })
}

/// T3: shift an anchor's A (an anchor no ticket, explore or clash reads).
pub fn t03(land: &Input) -> Made {
    let mut inp = land.clone();
    let (w, _) = world(&inp);
    let used: Vec<u32> = w.of(Kind::SETTLE).map(|r| r.pu32("ticket_bell")).collect();
    let (t, i) = *find(&inp, Kind::ANCHOR)
        .iter()
        .rev()
        .find(|(t, i)| {
            let r = &w.recs[rec_at(&w, *t, *i)];
            !used.contains(&r.ku32("bell"))
        })
        .need("an unused anchor")?;
    let (o, _) = field(Kind::ANCHOR, "a");
    edit_payload(&mut inp, t, i, |p| {
        let a = le(&p[o..o + 8]) as i64 + 7;
        p[o..o + 8].copy_from_slice(&a.to_le_bytes());
    });
    Ok(Case {
        class: "T3",
        what: "shift an anchor's A",
        feature: Some("mutate-v3"),
        codes: &[SEED_ROUND_RULE],
        input: inp,
    })
}

/// T4: inject a second anchor for a (bell, region).
pub fn t04(land: &Input) -> Made {
    let mut inp = land.clone();
    let (t, i) = pick(&find(&inp, Kind::ANCHOR), 3, "ANCHOR record")?;
    let mut bs = bodies(&inp, t);
    let dup = bs[i].clone();
    bs.push(dup.clone());
    // One more PS2 line in the same frame (before its success line).
    let pos = inp.txs[t]
        .logs
        .iter()
        .rposition(|l| l.ends_with(" success"))
        .need("frame")?;
    inp.txs[t].logs.insert(pos, log_line(&dup));
    Ok(Case {
        class: "T4",
        what: "inject a second anchor",
        feature: Some("mutate-v3"),
        codes: &[DUPLICATE_ANCHOR],
        input: inp,
    })
}

/// T5: swap a cache signature for another round's.
pub fn t05(land: &Input) -> Made {
    let mut inp = land.clone();
    let seeds = find(&inp, Kind::SEED);
    let anchors = find(&inp, Kind::ANCHOR);
    let (ta, _) = anchors[0];
    let mut other = [0u8; 48];
    let (w, _) = world(&inp);
    if let Some(ix) = w.txs[ta]
        .ix(Ix::PostAnchor.tag())
        .or_else(|| w.txs[ta].ix(Ix::PostAnchorMulti.tag()))
    {
        let o = if ix.tag() == Some(Ix::PostAnchor.tag()) {
            1 + 1 + 4 + 8
        } else {
            1 + 4 + 8
        };
        other.copy_from_slice(&ix.data[o..o + 48]);
    }
    let (ts, _) = pick(&seeds, seeds.len() / 2, "SEED record")?;
    edit_ix(&mut inp, ts, Ix::PostSeed, |d| {
        d[1 + 1 + 4 + 1 + 8..1 + 1 + 4 + 1 + 8 + 48].copy_from_slice(&other)
    });
    Ok(Case {
        class: "T5",
        what: "swap a cache signature",
        feature: Some("mutate-v3"),
        codes: &[SEED_ROUND_RULE, BEACON_SIG_INVALID],
        input: inp,
    })
}

/// T6: `set_account` on a Province after its last resolve (the final bytes
/// differ from the last transaction's write).
pub fn t06(march: &Input) -> Made {
    let mut inp = march.clone();
    let (w, f) = world(&inp);
    let (t, i) = *find(&inp, Kind::CLASH).last().need("a CLASH")?;
    let r = &w.recs[rec_at(&w, t, i)];
    let pk = f.ctx.province(r.pq().0, r.pq().1);
    if let Some(Some(a)) = inp.finals.get_mut(&pk) {
        let o = frontier_abi::layout::province::province::entry(0)
            + frontier_abi::layout::province::entry::TROOPS;
        let v = le(&a.data[o..o + 4]) as u32 + 50_000;
        a.data[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }
    Ok(Case {
        class: "T6",
        what: "set_account on a Province after a resolve",
        feature: Some("mutate-v1"),
        codes: &[HEAD_MISMATCH, CLASH_REPLAY_MISMATCH],
        input: inp,
    })
}

/// T6b: a rogue write between two resolves of one province; the program
/// resolves the second from the rogue state (its CLASH re-chained): the
/// replay from the chain's last write disagrees.
pub fn t06b(march: &Input) -> Made {
    let mut inp = march.clone();
    let (w, f) = world(&inp);
    // A province with two CLASH records.
    let clashes = find(&inp, Kind::CLASH);
    let mut by: HashMap<(i32, i32), Vec<(usize, usize)>> = HashMap::new();
    for &(t, i) in &clashes {
        by.entry(w.recs[rec_at(&w, t, i)].pq())
            .or_default()
            .push((t, i));
    }
    // A later clash of a province whose state before it has a resident
    // (integ-W4: the program recording has clashes of arrivals into empty
    // provinces too).
    let resident = |d: &[u8]| {
        (0..56).find(|&k| frontier_abi::entry::read_entry(d, k).is_ok_and(|x| x.state == 1))
    };
    type ByProvince = Vec<((i32, i32), Vec<(usize, usize)>)>;
    let mut by: ByProvince = by.into_iter().collect();
    by.sort();
    let (pq, (t2, i2)) = by
        .iter()
        .filter(|(_, v)| v.len() >= 2)
        .flat_map(|(pq, v)| v[1..].iter().map(move |x| (*pq, *x)))
        .find(|(pq, (t2, _))| {
            w.state_before(&f.ctx.province(pq.0, pq.1), *t2)
                .and_then(resident)
                .is_some()
        })
        .need("two clashes in one province, a resident before the second")?;
    let r2 = &w.recs[rec_at(&w, t2, i2)];
    let b2 = r2.ku32("bell");
    let pk = f.ctx.province(pq.0, pq.1);
    let mut before = w.state_before(&pk, t2).need("state")?.to_vec();
    // The rogue write: the first roster entry loses troops.
    let e = resident(&before).need("a resident")?;
    let o = frontier_abi::layout::province::province::entry(e)
        + frontier_abi::layout::province::entry::TROOPS;
    let v2 = le(&before[o..o + 4]) as u32 / 2;
    before[o..o + 4].copy_from_slice(&v2.to_le_bytes());
    let ci = w
        .state_before(&f.ctx.clash_inputs(pq.0, pq.1, b2), t2)
        .need("inputs")?;
    let seed = f.bell_seed(b2, Facts::region(pq.0, pq.1)).need("seed")?;
    let built = ContractBuilder
        .build(&before, ci, b2, &seed)
        .need("build")?;
    let out = built.resolve().need("resolve")?;
    let fates = packed_fates(&built, &out);
    let (od, _) = field(Kind::CLASH, "outcome_digest");
    let (oe, _) = field(Kind::CLASH, "engagements");
    let (of, _) = field(Kind::CLASH, "fates");
    edit_payload(&mut inp, t2, i2, |p| {
        p[od..od + 32].copy_from_slice(&out.digest());
        p[oe..oe + 4].copy_from_slice(&out.engagements.to_le_bytes());
        p[of..of + 9].copy_from_slice(&fates);
    });
    rechain(&mut inp);
    Ok(Case {
        class: "T6b",
        what: "a rogue Province write between two resolves",
        feature: Some("mutate-v7"),
        codes: &[CLASH_REPLAY_MISMATCH],
        input: inp,
    })
}

/// T7: a valid seal's settlement logged as a bad seal (consistently: the
/// bad-seal outcome and payments).
pub fn t07(march: &Input) -> Made {
    let mut inp = march.clone();
    let (w, f) = world(&inp);
    let (t, i) = *find(&inp, Kind::TRANSIT_SETTLED)
        .iter()
        .find(|(t, i)| {
            let r = &w.recs[rec_at(&w, *t, *i)];
            r.pu8("seal_code") == 0 && r.pu8("outcome") == transit_outcome::STAYS
        })
        .need("a valid settlement")?;
    let r = &w.recs[rec_at(&w, t, i)];
    let host = r.ku64("host_id");
    let tip = f
        .depart_of(&w, host, rec_at(&w, t, i), None)
        .map(|d| w.recs[d].pu64("tip"))
        .unwrap_or(0);
    let p = f.params.need("params")?;
    let settler: [u8; 32] = w.txs[t]
        .ix(Ix::SettleTransit.tag())
        .and_then(|x| x.key(11))
        .need("settler")?;
    // A careful forger keeps `pool_owed_delta` = the pool's pairs + the
    // transaction's DIVERTs (V13 since the integ-W4 review).
    let diverted: u64 = w.txs[t]
        .recs
        .iter()
        .map(|&n| &w.recs[n])
        .filter(|x| x.kind == Kind::DIVERT)
        .map(|x| x.pu64("amount"))
        .sum();
    forge_transit(
        &mut inp,
        t,
        i,
        transit_outcome::BAD_SEAL,
        1,
        0,
        [
            (0, [0; 8]),
            (0, [0; 8]),
            (0, [0; 8]),
            (
                tip + p.march_fee + p.seal_bond,
                settler[..8].try_into().unwrap_or([0; 8]),
            ),
        ],
        diverted,
    );
    rechain(&mut inp);
    Ok(Case {
        class: "T7",
        what: "a valid seal marked bad at settlement",
        feature: Some("mutate-v5"),
        codes: &[VERDICT_DISAGREES_WITH_TLOCK],
        input: inp,
    })
}

/// Writes a TRANSIT_SETTLED's outcome, code, troops and payments.
#[allow(clippy::too_many_arguments)]
fn forge_transit(
    inp: &mut Input,
    t: usize,
    i: usize,
    outcome: u8,
    code: u8,
    troops: u32,
    pays: [(u64, [u8; 8]); 4],
    pool: u64,
) {
    let k = Kind::TRANSIT_SETTLED;
    let fo = |n: &str| field(k, n).0;
    let names = [
        ("tip_to", "tip"),
        ("fee_to", "fee"),
        ("bond_to", "bond"),
        ("reward_to", "reward"),
    ];
    let offs: Vec<(usize, usize)> = names.iter().map(|(a, b)| (fo(a), fo(b))).collect();
    let (oo, oc, ot, op) = (
        fo("outcome"),
        fo("seal_code"),
        fo("troops"),
        fo("pool_owed_delta"),
    );
    edit_payload(inp, t, i, |p| {
        p[oo] = outcome;
        p[oc] = code;
        p[ot..ot + 4].copy_from_slice(&troops.to_le_bytes());
        for ((to, amt), (v, who)) in offs.iter().zip(pays) {
            p[*to..*to + 8].copy_from_slice(&who);
            p[*amt..*amt + 8].copy_from_slice(&v.to_le_bytes());
        }
        p[op..op + 8].copy_from_slice(&pool.to_le_bytes());
    });
}

/// T8: change which slot a displacement hit.
pub fn t08(march: &Input) -> Made {
    let mut inp = march.clone();
    let (w, _) = world(&inp);
    let (t, i) = *find(&inp, Kind::REVEAL)
        .iter()
        .find(|(t, i)| w.recs[rec_at(&w, *t, *i)].pu8("displace") == 1)
        .need("a displacement")?;
    let (oi, _) = plog::field(Kind::REVEAL, "i", false).need("i")?;
    edit_key(&mut inp, t, i, |k| k[oi] = (k[oi] + 1) % 4);
    Ok(Case {
        class: "T8",
        what: "change which slot a displacement hit",
        feature: Some("mutate-v6"),
        codes: &[QUOTA_SET_MISMATCH],
        input: inp,
    })
}

/// T9: alter a departure mass (of an arrival that stayed; re-chained).
pub fn t09(march: &Input) -> Made {
    let mut inp = march.clone();
    let (w, f) = world(&inp);
    let stayed: Vec<u64> = w
        .of(Kind::TRANSIT_SETTLED)
        .filter(|r| r.pu8("outcome") == transit_outcome::STAYS)
        .map(|r| r.ku64("host_id"))
        .collect();
    let (t, i) = *find(&inp, Kind::DEPART)
        .iter()
        .find(|(t, i)| {
            let r = &w.recs[rec_at(&w, *t, *i)];
            stayed.contains(&r.ku64("host_id")) && f.departs[&r.ku64("host_id")].len() == 1
        })
        .need("a departure that stayed")?;
    let (o, _) = field(Kind::DEPART, "dep_mass");
    edit_payload(&mut inp, t, i, |p| {
        let v = le(&p[o..o + 4]) as u32 + 1_000;
        p[o..o + 4].copy_from_slice(&v.to_le_bytes());
    });
    rechain(&mut inp);
    Ok(Case {
        class: "T9",
        what: "alter a departure mass",
        feature: Some("mutate-v6"),
        codes: &[TRANSIT_MASS_MISMATCH],
        input: inp,
    })
}

/// T10: the wrong drand key.
pub fn t10(land: &Input) -> Made {
    let mut inp = land.clone();
    inp.cfg.quicknet_pk =
        crate::input::hex_arr(fclient::beacon::QUICKNET_PK).need("quicknet key")?;
    Ok(Case {
        class: "T10",
        what: "wrong quicknet key",
        feature: Some("mutate-v3"),
        codes: &[BEACON_SIG_INVALID],
        input: inp,
    })
}

/// T11: the wrong ruleset hash.
pub fn t11(land: &Input) -> Made {
    let mut inp = land.clone();
    inp.cfg.ruleset_hash[0] ^= 0xFF;
    Ok(Case {
        class: "T11",
        what: "wrong ruleset hash",
        feature: Some("mutate-v2"),
        codes: &[RULESET_MISMATCH],
        input: inp,
    })
}

/// T12: truncate the last game day (the final states stay the chain's).
pub fn t12(march: &Input) -> Made {
    let mut inp = march.clone();
    let (w, f) = world(&inp);
    let last_day = w
        .txs
        .iter()
        .filter_map(|t| beacon_day(&f, t.time))
        .max()
        .unwrap_or(0);
    let start = f.genesis_ts + last_day as i64 * 144 * 600;
    inp.txs.retain(|t| t.block_time < start);
    Ok(Case {
        class: "T12",
        what: "truncate the last game day",
        feature: Some("mutate-v1"),
        codes: &[HEAD_MISMATCH, CHAIN_GAP],
        input: inp,
    })
}

fn beacon_day(f: &Facts, t: i64) -> Option<u32> {
    permutation_rules::frontier::beacon::bell_at(f.genesis_ts, t).map(|b| b / 144)
}

/// T13: move a Reveal past `A + W`.
pub fn t13(march: &Input) -> Made {
    let mut inp = march.clone();
    let (w, f) = world(&inp);
    // A Reveal that landed after its bell's THE anchor (an owner's in-bell
    // Reveal before the anchor is allowed at any Clock; integ-W4: the
    // program recording's second Reveal is one).
    let (t, a, arrive) = find(&inp, Kind::REVEAL)
        .into_iter()
        .filter_map(|(t, i)| {
            let r = &w.recs[rec_at(&w, t, i)];
            let (p, q) = r.pq();
            let a = f.anchor(r.ku32("arrive"), Facts::region(p, q))?;
            (w.recs[a.rec].tx < t).then_some((t, a, r.ku32("arrive")))
        })
        .nth(1)
        .need("a Reveal after its anchor")?;
    inp.txs[t].block_time = f.close(arrive, a.a) + 5;
    Ok(Case {
        class: "T13",
        what: "move a Reveal past A + W",
        feature: Some("mutate-v4"),
        codes: &[REVEAL_AFTER_CLOSE],
        input: inp,
    })
}

/// T14: duplicate a transaction.
pub fn t14(land: &Input) -> Made {
    let mut inp = land.clone();
    let (t, _) = pick(&find(&inp, Kind::FOLD), 2, "FOLD record")?;
    let dup = inp.txs[t].clone();
    inp.txs.insert(t + 1, dup);
    Ok(Case {
        class: "T14",
        what: "duplicate a transaction",
        feature: Some("mutate-v1"),
        codes: &[DUPLICATE_EVENT],
        input: inp,
    })
}

/// T15: origin values from a later bell — a gathered arrival's troops are
/// not its departure's; the forger recomputes the CLASH and the transit.
pub fn t15(march: &Input) -> Made {
    let mut inp = march.clone();
    let (w, f) = world(&inp);
    // A clash with no later clash of its province, and an arrival that
    // stayed (and settled as Stays: a valid seal).
    let stays: Vec<u64> = w
        .of(Kind::TRANSIT_SETTLED)
        .filter(|r| r.pu8("outcome") == transit_outcome::STAYS)
        .map(|r| r.ku64("host_id"))
        .collect();
    let clashes = find(&inp, Kind::CLASH);
    let (t, i, pq, b, host, pos) = clashes
        .iter()
        .filter_map(|&(t, i)| {
            let r = &w.recs[rec_at(&w, t, i)];
            let pq = r.pq();
            let later = clashes
                .iter()
                .any(|&(t2, i2)| (t2, i2) > (t, i) && w.recs[rec_at(&w, t2, i2)].pq() == pq);
            if later {
                return None;
            }
            let b = r.ku32("bell");
            let ci = fclient::decode::ClashInputs::decode(
                w.txs[t].post_data(&f.ctx.clash_inputs(pq.0, pq.1, b))?,
            )
            .ok()?;
            let (pos, a) = ci.arrivals.iter().enumerate().find(|(_, a)| {
                a.present == 1 && a.fate == transit_outcome::STAYS && stays.contains(&a.host_id)
            })?;
            Some((t, i, pq, b, a.host_id, pos))
        })
        .next()
        .need("a clash to forge")?;
    let ck = f.ctx.clash_inputs(pq.0, pq.1, b);
    let o = CI::arrival(pos) + AR::TROOPS;
    // The gathered troops, as the forger logs them.
    let gather_tx = (0..t)
        .rev()
        .find(|&x| w.txs[x].post.contains_key(&ck))
        .need("gather")?;
    edit_account_from(&mut inp, &ck, gather_tx, |d| {
        let v = le(&d[o..o + 4]) as u32 - 1_000;
        d[o..o + 4].copy_from_slice(&v.to_le_bytes());
    });
    let (w2, f2) = world(&inp);
    let pk = f2.ctx.province(pq.0, pq.1);
    let built = ContractBuilder
        .build(
            w2.state_before(&pk, t).need("province")?,
            w2.state_before(&ck, t).need("inputs")?,
            b,
            &f2.bell_seed(b, Facts::region(pq.0, pq.1)).need("seed")?,
        )
        .need("build")?;
    let out = built.resolve().need("resolve")?;
    let fates = packed_fates(&built, &out);
    let troops_after = out.fighter(host).map(|x| x.troops).unwrap_or(0);
    let (od, _) = field(Kind::CLASH, "outcome_digest");
    let (oe, _) = field(Kind::CLASH, "engagements");
    let (of, _) = field(Kind::CLASH, "fates");
    let (oi, _) = field(Kind::CLASH, "input_digest");
    let in_digest = crate::clash_input::input_digest(
        w2.state_before(&pk, t).need("province")?,
        w2.state_before(&ck, t).need("inputs")?,
        b,
        &f2.bell_seed(b, Facts::region(pq.0, pq.1)).need("seed")?,
    );
    edit_payload(&mut inp, t, i, |p| {
        p[od..od + 32].copy_from_slice(&out.digest());
        p[oe..oe + 4].copy_from_slice(&out.engagements.to_le_bytes());
        p[of..of + 9].copy_from_slice(&fates);
        p[oi..oi + 32].copy_from_slice(&in_digest);
    });
    let oa = CI::arrival(pos) + AR::TROOPS_AFTER;
    edit_account_from(&mut inp, &ck, t, |d| {
        d[oa..oa + 4].copy_from_slice(&troops_after.to_le_bytes())
    });
    // The write-back the forger's resolve would leave (V7 checks it since
    // the integ-W4 review): every fighter whose post-clash troops moved,
    // in the Province it wrote and every later state of it.
    {
        use frontier_abi::entry::{find_entry, read_entry, write_entry};
        let post = w2.txs[t]
            .post_data(&pk)
            .need("the Province written")?
            .to_vec();
        let fixes: Vec<(u64, u32, u32)> = out
            .fighters
            .iter()
            .filter_map(|fr| {
                let e = read_entry(&post, find_entry(&post, fr.id)?).ok()?;
                (e.troops != fr.troops).then_some((fr.id, e.troops, fr.troops))
            })
            .collect();
        edit_account_from(&mut inp, &pk, t, |d| {
            for &(id, old, new) in &fixes {
                if let Some(i) = find_entry(d, id) {
                    if let Ok(mut e) = read_entry(d, i) {
                        if e.troops == old {
                            e.troops = new;
                            let _ = write_entry(d, i, &e);
                        }
                    }
                }
            }
        });
    }
    // Every later SKIP of the province digests the forged state (v1.6).
    {
        let (w3, _) = world(&inp);
        let (oq, _) = field(Kind::SKIP, "quiet_digest");
        for (t3, i3) in find(&inp, Kind::SKIP) {
            if t3 <= t {
                continue;
            }
            let r3 = &w3.recs[rec_at(&w3, t3, i3)];
            if r3.pq() != pq {
                continue;
            }
            let (b0, n) = (r3.pu32("b0"), r3.pu8("n"));
            let Some(after) = w3.txs[t3].post_data(&pk) else {
                continue;
            };
            let qd = crate::clash_input::quiet_digest(after, b0, n);
            edit_payload(&mut inp, t3, i3, |p| p[oq..oq + 32].copy_from_slice(&qd));
        }
    }
    // The transit settles with the forged troops.
    let (tt, ti) = *find(&inp, Kind::TRANSIT_SETTLED)
        .iter()
        .find(|(x, y)| w.recs[rec_at(&w, *x, *y)].ku64("host_id") == host)
        .need("settlement")?;
    let (ot, _) = field(Kind::TRANSIT_SETTLED, "troops");
    edit_payload(&mut inp, tt, ti, |p| {
        p[ot..ot + 4].copy_from_slice(&troops_after.to_le_bytes())
    });
    rechain(&mut inp);
    Ok(Case {
        class: "T15",
        what: "origin values from a later bell",
        feature: Some("mutate-v8"),
        codes: &[ORIGIN_VALUE_MISMATCH],
        input: inp,
    })
}

/// T16: the wrong genesis round (another round's signature and seed, as a
/// forged program would accept them; re-chained).
pub fn t16(land: &Input) -> Made {
    let mut inp = land.clone();
    let (w, _) = world(&inp);
    let (ta, _) = pick(&find(&inp, Kind::ANCHOR), 0, "ANCHOR record")?;
    let ix = w.txs[ta]
        .ix(Ix::PostAnchor.tag())
        .or_else(|| w.txs[ta].ix(Ix::PostAnchorMulti.tag()))
        .need("anchor ix")?;
    let (ro, so) = if ix.tag() == Some(Ix::PostAnchor.tag()) {
        (6, 14)
    } else {
        (5, 13)
    };
    let round = le(&ix.data[ro..ro + 8]);
    let sig: [u8; 48] = ix.data[so..so + 48].try_into().need("48")?;
    let hints = ix.data[so + 48..so + 48 + frontier_abi::ix::HINTS_LEN].to_vec();
    let seed = crate::checks::v3_random::seed_of48(round, &sig).need("seed")?;
    let (t, i) = pick(&find(&inp, Kind::GENESIS_SEED), 0, "GENESIS_SEED record")?;
    edit_ix(&mut inp, t, Ix::ConsumeGenesisSeed, |d| {
        d[1..9].copy_from_slice(&round.to_le_bytes());
        d[9..57].copy_from_slice(&sig);
        d[57..57 + hints.len()].copy_from_slice(&hints);
    });
    let (or, _) = field(Kind::GENESIS_SEED, "round");
    let (os, _) = field(Kind::GENESIS_SEED, "seed");
    edit_payload(&mut inp, t, i, |p| {
        p[or..or + 8].copy_from_slice(&round.to_le_bytes());
        p[os..os + 32].copy_from_slice(&seed);
    });
    rechain(&mut inp);
    Ok(Case {
        class: "T16",
        what: "wrong genesis round",
        feature: Some("mutate-v3"),
        codes: &[GENESIS_SEED_RULE],
        input: inp,
    })
}

/// T17: a SkipQuiet over a bell with an ArrivalSlot (a run extended over
/// the next bell, which had a revealed arrival; re-chained).
pub fn t17(march: &Input) -> Made {
    let mut inp = march.clone();
    let (w, _) = world(&inp);
    let reveals: Vec<(i32, i32, u32)> = w
        .of(Kind::REVEAL)
        .map(|r| (r.pq().0, r.pq().1, r.ku32("arrive")))
        .collect();
    let (t, i) = *find(&inp, Kind::SKIP)
        .iter()
        .find(|(t, i)| {
            let r = &w.recs[rec_at(&w, *t, *i)];
            let end = r.pu32("b0") + r.pu8("n") as u32;
            reveals.contains(&(r.pq().0, r.pq().1, end))
                && reveals.iter().any(|x| *x == (r.pq().0, r.pq().1, end))
        })
        .need("a skip just before an arrival bell")?;
    let (on, _) = field(Kind::SKIP, "n");
    edit_payload(&mut inp, t, i, |p| p[on] += 1);
    rechain(&mut inp);
    Ok(Case {
        class: "T17",
        what: "a SkipQuiet over a bell with an ArrivalSlot",
        feature: Some("mutate-v7"),
        codes: &[SKIP_NOT_QUIET, SKIP_OVER_ARRIVAL],
        input: inp,
    })
}

/// T18: a tampered SETTLE score (the Holding written with it; re-chained).
pub fn t18(land: &Input) -> Made {
    let mut inp = land.clone();
    let (w, f) = world(&inp);
    let (t, i) = *find(&inp, Kind::SETTLE)
        .iter()
        .find(|(t, i)| w.recs[rec_at(&w, *t, *i)].pu8("outcome") == plog::settle_outcome::FRESH)
        .need("a fresh settlement")?;
    let r = &w.recs[rec_at(&w, t, i)];
    let (p, q, s) = r.pqs();
    let hk = f.ctx.holding(p, q, s);
    let (o, _) = field(Kind::SETTLE, "score");
    let new = r.pu64("score") ^ 0x5A5A;
    edit_payload(&mut inp, t, i, |pl| {
        pl[o..o + 8].copy_from_slice(&new.to_le_bytes())
    });
    edit_account_from(&mut inp, &hk, t, |d| {
        d[H::TICKET_SCORE..H::TICKET_SCORE + 8].copy_from_slice(&new.to_le_bytes())
    });
    rechain(&mut inp);
    Ok(Case {
        class: "T18",
        what: "a tampered SETTLE score",
        feature: Some("mutate-v11"),
        codes: &[TICKET_SCORE_MISMATCH],
        input: inp,
    })
}

/// T19: a tampered terrain digest (re-chained).
pub fn t19(land: &Input) -> Made {
    let mut inp = land.clone();
    let (t, i) = pick(&find(&inp, Kind::PROVINCE_OPEN), 5, "PROVINCE_OPEN record")?;
    let (o, _) = field(Kind::PROVINCE_OPEN, "terrain_digest");
    edit_payload(&mut inp, t, i, |p| p[o] ^= 1);
    rechain(&mut inp);
    Ok(Case {
        class: "T19",
        what: "a tampered terrain digest",
        feature: Some("mutate-v11"),
        codes: &[TERRAIN_MISMATCH],
        input: inp,
    })
}

/// T20: a ClaimDefence amount above the formula.
pub fn t20(march: &Input) -> Made {
    let mut inp = march.clone();
    let (t, i) = pick(&find(&inp, Kind::DEFENCE_CLAIM), 0, "DEFENCE_CLAIM record")?;
    let (o, _) = field(Kind::DEFENCE_CLAIM, "amount");
    edit_payload(&mut inp, t, i, |p| {
        let v = le(&p[o..o + 8]) + 1_000;
        p[o..o + 8].copy_from_slice(&v.to_le_bytes());
    });
    Ok(Case {
        class: "T20",
        what: "a ClaimDefence amount above the formula",
        feature: Some("mutate-v13"),
        codes: &[DEFENCE_REFUND_MISMATCH],
        input: inp,
    })
}

/// T21: a tampered explore find (re-chained).
pub fn t21(march: &Input) -> Made {
    let mut inp = march.clone();
    let (t, i) = pick(
        &find(&inp, Kind::EXPLORE_RESULT),
        0,
        "EXPLORE_RESULT record",
    )?;
    let (o, _) = field(Kind::EXPLORE_RESULT, "works_per_tile");
    let (ow, _) = field(Kind::EXPLORE_RESULT, "works");
    edit_payload(&mut inp, t, i, |p| {
        let v = le(&p[o..o + 4]) as u32 + 1;
        p[o..o + 4].copy_from_slice(&v.to_le_bytes());
        let tw = le(&p[ow..ow + 4]) as u32 + 1;
        p[ow..ow + 4].copy_from_slice(&tw.to_le_bytes());
    });
    rechain(&mut inp);
    Ok(Case {
        class: "T21",
        what: "a tampered explore find",
        feature: Some("mutate-v12"),
        codes: &[EXPLORE_ROLL_MISMATCH],
        input: inp,
    })
}

/// T22: a bad-seal transit logged as surviving (its clash fate, a zero
/// code, the fate's troops and payments; re-chained).
pub fn t22(march: &Input) -> Made {
    let mut inp = march.clone();
    let (w, f) = world(&inp);
    let (t, i, host, pq, b) = find(&inp, Kind::TRANSIT_SETTLED)
        .iter()
        .filter_map(|&(t, i)| {
            let r = &w.recs[rec_at(&w, t, i)];
            if r.pu8("outcome") != transit_outcome::BAD_SEAL {
                return None;
            }
            let host = r.ku64("host_id");
            let ix = w.txs[t].ix(Ix::SettleTransit.tag())?;
            let pq = *f.provinces.get(&ix.key(3)?)?;
            let d = f.depart_of(&w, host, rec_at(&w, t, i), None)?;
            let b = w.recs[d].pu32("arrive_bell");
            let ci = fclient::decode::ClashInputs::decode(
                w.state_before(&f.ctx.clash_inputs(pq.0, pq.1, b), t)?,
            )
            .ok()?;
            ci.arrivals
                .iter()
                .any(|a| a.present == 1 && a.host_id == host)
                .then_some((t, i, host, pq, b))
        })
        .next()
        .need("a revealed bad seal")?;
    let ci = fclient::decode::ClashInputs::decode(
        w.state_before(&f.ctx.clash_inputs(pq.0, pq.1, b), t)
            .need("inputs")?,
    )
    .need("decode")?;
    let a = ci
        .arrivals
        .iter()
        .find(|a| a.present == 1 && a.host_id == host)
        .need("arrival")?;
    let troops = match a.fate {
        transit_outcome::STAYS | transit_outcome::WITHDREW => a.troops_after,
        transit_outcome::DESTROYED => 0,
        _ => a.troops,
    };
    let ix = w.txs[t].ix(Ix::SettleTransit.tag()).need("ix")?;
    let pre = |k: usize| -> [u8; 8] {
        ix.key(k)
            .map(|x| x[..8].try_into().unwrap_or([0; 8]))
            .unwrap_or([0; 8])
    };
    let p = f.params.need("params")?;
    let tip = f
        .depart_of(&w, host, rec_at(&w, t, i), None)
        .map(|d| w.recs[d].pu64("tip"))
        .unwrap_or(0);
    let resolver: [u8; 8] = ci.resolver.to_bytes()[..8].try_into().unwrap_or([0; 8]);
    // `pool_owed_delta` = the pool's pairs + the transaction's DIVERTs (V13).
    let diverted: u64 = w.txs[t]
        .recs
        .iter()
        .map(|&n| &w.recs[n])
        .filter(|x| x.kind == Kind::DIVERT)
        .map(|x| x.pu64("amount"))
        .sum();
    forge_transit(
        &mut inp,
        t,
        i,
        a.fate,
        0,
        troops,
        [
            (tip, pre(8)),
            (p.march_fee, resolver),
            (p.seal_bond, pre(10)),
            (0, [0; 8]),
        ],
        diverted,
    );
    rechain(&mut inp);
    Ok(Case {
        class: "T22",
        what: "a bad-seal transit logged as surviving",
        feature: Some("mutate-v5"),
        codes: &[BAD_SEAL_SURVIVED],
        input: inp,
    })
}

/// T1b (integ-W4 review, W4-D major): drop an **unchained** transaction —
/// the last PostAnchor (Multi) and the PostSeeds of its bell — keeping the
/// anchor and cache accounts in the finals. Nothing chains them, so before
/// the review V1 passed it; now an account no transaction explains is a
/// gap.
pub fn t01b(land: &Input) -> Made {
    let mut inp = land.clone();
    let (w, _) = world(&inp);
    let anchors = find(&inp, Kind::ANCHOR);
    let (ta, ia) = *anchors.last().need("an ANCHOR")?;
    let bell = w.recs[rec_at(&w, ta, ia)].ku32("bell");
    let mut drop: Vec<usize> = vec![ta];
    for (t, i) in find(&inp, Kind::SEED) {
        if w.recs[rec_at(&w, t, i)].ku32("bell") == bell {
            drop.push(t);
        }
    }
    drop.sort_unstable();
    drop.dedup();
    for t in drop.into_iter().rev() {
        inp.txs.remove(t);
    }
    Ok(Case {
        class: "T1b",
        what: "drop an unchained transaction (an anchor and its seeds)",
        feature: Some("mutate-v1"),
        codes: &[CHAIN_GAP],
        input: inp,
    })
}

/// T23 (integ-W4 review, W4-D major): a resolve's **write-back** forged —
/// +25,000 milli-troops written to a resident in the last CLASH's Province
/// post-state (and every later state of it, and the final). The digests,
/// fates and chains are untouched (account bytes are not chained), so only
/// V7's write-back replay sees it.
pub fn t23(march: &Input) -> Made {
    let mut inp = march.clone();
    let (w, f) = world(&inp);
    let (t, i) = *find(&inp, Kind::CLASH).last().need("a CLASH")?;
    let r = &w.recs[rec_at(&w, t, i)];
    let pk = f.ctx.province(r.pq().0, r.pq().1);
    let post = w.txs[t].post_data(&pk).need("the Province's post-state")?;
    // A resident that stays put (no later departure: V8 would read it).
    let later: std::collections::BTreeSet<u64> = w
        .recs
        .iter()
        .filter(|x| x.tx > t && x.kind == Kind::DEPART)
        .map(|x| x.ku64("host_id"))
        .collect();
    let e = (0..frontier_abi::layout::province::province::ENTRIES_N)
        .find(|&k| {
            frontier_abi::entry::read_entry(post, k)
                .is_ok_and(|x| x.state == 1 && !later.contains(&x.id))
        })
        .need("a resident after the clash")?;
    let o = frontier_abi::layout::province::province::entry(e)
        + frontier_abi::layout::province::entry::TROOPS;
    edit_account_from(&mut inp, &pk, t, |d| {
        let v = le(&d[o..o + 4]) as u32 + 25_000;
        d[o..o + 4].copy_from_slice(&v.to_le_bytes());
    });
    Ok(Case {
        class: "T23",
        what: "a resolve's write-back forged in its own transaction",
        feature: Some("mutate-v7"),
        codes: &[CLASH_REPLAY_MISMATCH],
        input: inp,
    })
}

/// V9 (no §8.5 class; the check of its check): an account at a
/// non-canonical address — an anchor whose stored region is not the one
/// its address derives (its post-state and final state agree, so only V9
/// sees it).
pub fn v9a(land: &Input) -> Made {
    use frontier_abi::layout::beacon::bell_anchor as BA;
    let mut inp = land.clone();
    let (w, f) = world(&inp);
    let a = w.recs[rec_at(
        &w,
        pick(&find(&inp, Kind::ANCHOR), 2, "ANCHOR record")?.0,
        pick(&find(&inp, Kind::ANCHOR), 2, "ANCHOR record")?.1,
    )]
    .clone();
    let k = f.ctx.bell_anchor(a.ku32("bell"), a.ku8("region"));
    edit_account_from(&mut inp, &k, 0, |d| d[BA::REGION] ^= 1);
    Ok(Case {
        class: "V9a",
        what: "an account at a non-canonical address",
        feature: Some("mutate-v9"),
        codes: &[NON_CANONICAL_ADDRESS],
        input: inp,
    })
}

// ------------------------------------------------------------ run fallbacks (W5-D)

/// T8 on a run without a displacement: the slot `i` a **filling** Reveal
/// logged is changed (the kernel's `admit_arrival` names another slot).
pub fn t08_fill(run: &Input) -> Made {
    let mut inp = run.clone();
    let (w, _) = world(&inp);
    let (t, i) = *find(&inp, Kind::REVEAL)
        .iter()
        .find(|(t, i)| w.recs[rec_at(&w, *t, *i)].pu8("displace") == 0)
        .need("filling Reveal")?;
    let (oi, _) = plog::field(Kind::REVEAL, "i", false).need("REVEAL.i")?;
    edit_key(&mut inp, t, i, |k| k[oi] = (k[oi] + 1) % 4);
    Ok(Case {
        class: "T8",
        what: "change which slot a Reveal filled (no displacement in the run)",
        feature: Some("mutate-v6"),
        codes: &[QUOTA_SET_MISMATCH],
        input: inp,
    })
}

/// T13 on a run whose Reveals all landed before THE anchor of their bell
/// (the program's in-bell reveals, integ-W4): one Reveal is **moved** to
/// after the anchor with a Clock past `A + W` — to just before the first
/// transaction at or after the close, provided nothing in between touches
/// an account the Reveal wrote (so only V4 can tell).
pub fn t13_moved(run: &Input) -> Made {
    let mut inp = run.clone();
    let (w, f) = world(&inp);
    let mut pick_ = None;
    for (t, i) in find(&inp, Kind::REVEAL).into_iter().rev() {
        let r = &w.recs[rec_at(&w, t, i)];
        let (p, q) = r.pq();
        let arrive = r.ku32("arrive");
        let Some(a) = f.anchor(arrive, Facts::region(p, q)) else {
            continue;
        };
        let ta = w.recs[a.rec].tx;
        if ta <= t {
            continue;
        }
        let close = f.close(arrive, a.a);
        let Some(j) = (ta + 1..w.txs.len()).find(|&j| w.txs[j].time >= close) else {
            continue;
        };
        let wrote: Vec<Key> = w.txs[t]
            .keys
            .iter()
            .filter(|(_, wr)| *wr)
            .map(|(k, _)| *k)
            .filter(|k| inp.cfg.program.to_bytes() != *k)
            .skip(1) // the fee payer
            .collect();
        let touched = (t + 1..j).any(|x| w.txs[x].keys.iter().any(|(k, _)| wrote.contains(k)));
        if !touched {
            pick_ = Some((t, j));
            break;
        }
    }
    let (t, j) = pick_.need("Reveal that can be moved past its close unseen")?;
    let mut tx = inp.txs.remove(t);
    let at = j - 1; // indices after `t` shifted down by one
    tx.block_time = inp.txs[at].block_time;
    tx.slot = inp.txs[at].slot;
    inp.txs.insert(at, tx);
    Ok(Case {
        class: "T13",
        what: "move a Reveal past A + W (moved after its anchor)",
        feature: Some("mutate-v4"),
        codes: &[REVEAL_AFTER_CLOSE],
        input: inp,
    })
}

/// T20 on a run without a defence claim: a ClaimDefence transaction is
/// **injected** at the end — a keeper's claim of no eligible slot with an
/// amount above the formula's (0). DEFENCE_CLAIM is not chained, so only
/// V13 can tell.
pub fn t20_injected(run: &Input) -> Made {
    use fclient::{Keypair, Signer};
    let mut inp = run.clone();
    let (_, f) = world(&inp);
    let last = inp.txs.last().need("transaction")?.clone();
    let bell = permutation_rules::frontier::beacon::bell_at(f.genesis_ts, last.block_time)
        .need("bell of the run's end")?;
    let day = bell / 144;
    let keeper = Keypair::new_from_array([0x20; 32]);
    let a = fclient::addr::Addresses::new(inp.cfg.program, inp.cfg.season_id);
    let ix = fclient::ix::claim_defence(&a, keeper.pubkey(), day, &[]);
    let t = fclient::tx::build(
        &[ix],
        &fclient::tx::TxBudget {
            cu_limit: 30_000,
            cu_price: 0,
            loaded_limit: 1 << 20,
            heap: None,
        },
        &[&keeper],
        &fclient::Hash::new_from_array([0x20; 32]),
    )
    .need("claim transaction")?;
    let mut key = [0u8; 32];
    key.copy_from_slice(&keeper.pubkey().to_bytes());
    let mut payload = day.to_le_bytes().to_vec();
    payload.push(0);
    payload.extend_from_slice(&50_000u64.to_le_bytes());
    payload.push(0);
    let mut body = vec![0u8; 512];
    let n = plog::write_body(Kind::DEFENCE_CLAIM, bell, &key, &payload, &mut body)
        .need("DEFENCE_CLAIM body")?;
    let m = plog::write_tail(&[], &mut body, n).need("DEFENCE_CLAIM tail")?;
    body.truncate(m);
    inp.txs.push(TxRecord {
        seq: last.seq + 1,
        slot: last.slot + 1,
        signature: fclient::tx::signature(&t),
        block_time: last.block_time + 1,
        tx: fclient::tx::wire(&t),
        logs: fclient::log::in_frame(&inp.cfg.program, [log_line(&body)]),
        err: None,
        code: None,
        units: 0,
        fee: fclient::tx::fee_lamports(&t.message),
        post: vec![],
    });
    Ok(Case {
        class: "T20",
        what: "a ClaimDefence amount above the formula (a claim injected)",
        feature: Some("mutate-v13"),
        codes: &[DEFENCE_REFUND_MISMATCH],
        input: inp,
    })
}

/// T14 on a run without FOLD records: any landed program transaction
/// listed twice.
pub fn t14_any(run: &Input) -> Made {
    let mut inp = run.clone();
    let t = (0..inp.txs.len())
        .find(|&t| inp.txs[t].err.is_none() && !bodies(&inp, t).is_empty())
        .need("landed program transaction")?;
    let dup = inp.txs[t].clone();
    inp.txs.insert(t + 1, dup);
    Ok(Case {
        class: "T14",
        what: "duplicate a transaction",
        feature: Some("mutate-v1"),
        codes: &[DUPLICATE_EVENT],
        input: inp,
    })
}

// ------------------------------------------------------------ checks of the new V7 parts (W5-D)

/// T23b: a SkipQuiet's **write-back** forged — +25,000 milli-troops to a
/// resident in the Province the skip wrote (and every later state of it),
/// the skip's and every later skip's quiet digest recomputed over the
/// forged bytes, re-chained. Only V7's bell-by-bell skip replay sees it.
pub fn t23b(run: &Input) -> Made {
    let mut inp = run.clone();
    let (w, f) = world(&inp);
    let clashes: Vec<((i32, i32), usize)> = w.of(Kind::CLASH).map(|r| (r.pq(), r.tx)).collect();
    let departs: Vec<(u64, usize)> = w
        .of(Kind::DEPART)
        .map(|r| (r.ku64("host_id"), r.tx))
        .collect();
    let mut chosen = None;
    for (t, i) in find(&inp, Kind::SKIP).into_iter().rev() {
        let r = &w.recs[rec_at(&w, t, i)];
        let pq = r.pq();
        if clashes.iter().any(|(x, ct)| *x == pq && *ct > t) {
            continue;
        }
        let pk = f.ctx.province(pq.0, pq.1);
        let Some(post) = w.txs[t].post_data(&pk) else {
            continue;
        };
        let e = (0..frontier_abi::layout::province::province::ENTRIES_N).find(|&k| {
            frontier_abi::entry::read_entry(post, k)
                .is_ok_and(|x| x.state == 1 && !departs.iter().any(|(h, dt)| *h == x.id && *dt > t))
        });
        if let Some(e) = e {
            chosen = Some((t, pq, e));
            break;
        }
    }
    let (t, pq, e) = chosen.need("SKIP of a province with a resident and no later clash")?;
    let pk = f.ctx.province(pq.0, pq.1);
    let o = frontier_abi::layout::province::province::entry(e)
        + frontier_abi::layout::province::entry::TROOPS;
    edit_account_from(&mut inp, &pk, t, |d| {
        let v = le(&d[o..o + 4]) as u32 + 25_000;
        d[o..o + 4].copy_from_slice(&v.to_le_bytes());
    });
    let (w2, _) = world(&inp);
    let (oq, _) = field(Kind::SKIP, "quiet_digest");
    for (t3, i3) in find(&inp, Kind::SKIP) {
        if t3 < t {
            continue;
        }
        let r3 = &w2.recs[rec_at(&w2, t3, i3)];
        if r3.pq() != pq {
            continue;
        }
        let Some(after) = w2.txs[t3].post_data(&pk) else {
            continue;
        };
        let qd = crate::clash_input::quiet_digest(after, r3.pu32("b0"), r3.pu8("n"));
        edit_payload(&mut inp, t3, i3, |p| p[oq..oq + 32].copy_from_slice(&qd));
    }
    rechain(&mut inp);
    Ok(Case {
        class: "T23b",
        what: "a SkipQuiet's write-back forged (quiet digests recomputed)",
        feature: Some("mutate-v7"),
        codes: &[CLASH_REPLAY_MISMATCH],
        input: inp,
    })
}

/// H1: a Harvest's **Holding** forged — +1 food (1,000 milli) in the
/// store the transaction wrote (and every later state of the Holding),
/// every later HARVEST digest of it recomputed, re-chained. Only V7's
/// holding replay sees it.
pub fn h1(run: &Input) -> Made {
    let mut inp = run.clone();
    let (w, f) = world(&inp);
    let (t, i) = *find(&inp, Kind::HARVEST)
        .iter()
        .rev()
        .find(|(t, i)| {
            let (p, q, s) = w.recs[rec_at(&w, *t, *i)].pqs();
            w.txs[*t].post_data(&f.ctx.holding(p, q, s)).is_some()
        })
        .need("HARVEST with a post-state")?;
    let (p, q, s) = w.recs[rec_at(&w, t, i)].pqs();
    let hk = f.ctx.holding(p, q, s);
    let o = H::store(0) + frontier_abi::layout::player::accrual::VALUE;
    edit_account_from(&mut inp, &hk, t, |d| {
        let v = le(&d[o..o + 8]) as i64 + 1_000;
        d[o..o + 8].copy_from_slice(&v.to_le_bytes());
    });
    let (w2, _) = world(&inp);
    let (od, _) = field(Kind::HARVEST, "stores_digest");
    for (t3, i3) in find(&inp, Kind::HARVEST) {
        if t3 < t {
            continue;
        }
        let r3 = &w2.recs[rec_at(&w2, t3, i3)];
        if r3.pqs() != (p, q, s) {
            continue;
        }
        let Some(after) = w2.txs[t3].post_data(&hk) else {
            continue;
        };
        let Ok(dg) = crate::holding::stores_digest(after) else {
            continue;
        };
        edit_payload(&mut inp, t3, i3, |pl| pl[od..od + 32].copy_from_slice(&dg));
    }
    rechain(&mut inp);
    Ok(Case {
        class: "H1",
        what: "a Harvest's Holding forged (stores digest recomputed)",
        feature: Some("mutate-v7"),
        codes: &[HOLDING_REPLAY_MISMATCH],
        input: inp,
    })
}

// ------------------------------------------------------------ the suite over one run (W5-D)

/// A class builder.
pub type Builder = fn(&Input) -> Made;

/// One class of the suite: its builders, tried in order (the fixture's
/// selection first, then the run fallbacks), and whether §8.5 requires it.
pub struct Class {
    pub class: &'static str,
    pub required: bool,
    pub tries: Vec<(&'static str, Builder)>,
}

/// T1–T22 (§8.5, required) and the extra checks of the checks (T1b, T6b,
/// T23, T23b, H1, V9a) over **one** run — a stack run, a nightly, a
/// recorded fixture.
pub fn classes() -> Vec<Class> {
    fn c(class: &'static str, required: bool, tries: Vec<(&'static str, Builder)>) -> Class {
        Class {
            class,
            required,
            tries,
        }
    }
    vec![
        c("T1", true, vec![("drop the last Depart", t01)]),
        c("T2", true, vec![("flip a Reveal plaintext byte", t02)]),
        c("T3", true, vec![("shift an unused anchor's A", t03)]),
        c("T4", true, vec![("inject a second anchor", t04)]),
        c("T5", true, vec![("swap a cache signature", t05)]),
        c("T6", true, vec![("set_account on a Province", t06)]),
        c("T7", true, vec![("valid seal settled as bad", t07)]),
        c(
            "T8",
            true,
            vec![
                ("displacement slot", t08),
                ("fill slot (no displacement)", t08_fill),
            ],
        ),
        c("T9", true, vec![("departure mass", t09)]),
        c("T10", true, vec![("wrong drand key", t10)]),
        c("T11", true, vec![("wrong ruleset hash", t11)]),
        c("T12", true, vec![("truncate the last game day", t12)]),
        c(
            "T13",
            true,
            vec![
                ("Reveal Clock past A + W", t13),
                ("Reveal moved past A + W", t13_moved),
            ],
        ),
        c(
            "T14",
            true,
            vec![
                ("duplicate a FOLD", t14),
                ("duplicate any transaction", t14_any),
            ],
        ),
        c("T15", true, vec![("origin values", t15)]),
        c("T16", true, vec![("wrong genesis round", t16)]),
        c("T17", true, vec![("skip over an arrival", t17)]),
        c("T18", true, vec![("SETTLE score", t18)]),
        c("T19", true, vec![("terrain digest", t19)]),
        c(
            "T20",
            true,
            vec![("claim amount", t20), ("claim injected", t20_injected)],
        ),
        c("T21", true, vec![("explore find", t21)]),
        c("T22", true, vec![("bad seal logged as surviving", t22)]),
        c("T1b", false, vec![("drop an unchained transaction", t01b)]),
        c("T6b", false, vec![("rogue write between resolves", t06b)]),
        c("T23", false, vec![("forged clash write-back", t23)]),
        c("T23b", false, vec![("forged skip write-back", t23b)]),
        c("H1", false, vec![("forged Harvest", h1)]),
        c("V9a", false, vec![("non-canonical address", v9a)]),
    ]
}

/// What happened to one class on the run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// The tampered run FAILS with one of the class's codes.
    Detected,
    /// The tampered run does not FAIL with one of its codes.
    Missed,
    /// No builder could tamper with this run (the reasons are listed).
    NotApplicable,
}

#[derive(Clone, Debug)]
pub struct Outcome {
    pub class: &'static str,
    pub required: bool,
    pub variant: &'static str,
    pub what: String,
    pub feature: Option<&'static str>,
    pub expected: Vec<&'static str>,
    pub status: Status,
    pub verdict: Option<crate::Verdict>,
    /// The FAIL codes of the tampered run.
    pub fail_codes: Vec<String>,
    pub detail: String,
}

/// The suite's result over one run.
#[derive(Clone, Debug)]
pub struct SuiteReport {
    pub base: crate::Verdict,
    pub base_fail_codes: Vec<String>,
    pub outcomes: Vec<Outcome>,
}

impl SuiteReport {
    /// Every required class detected on a base run that PASSES.
    pub fn all_detected(&self) -> bool {
        self.base == crate::Verdict::Pass
            && self
                .outcomes
                .iter()
                .filter(|o| o.required)
                .all(|o| o.status == Status::Detected)
    }
    /// Exit code: 0 every required class detected, 1 a required class
    /// missed, 2 the base run does not PASS or a required class could not
    /// be built.
    pub fn exit_code(&self) -> i32 {
        let req = self.outcomes.iter().filter(|o| o.required);
        if self.base != crate::Verdict::Pass {
            return crate::EXIT_UNVERIFIABLE;
        }
        if req.clone().any(|o| o.status == Status::Missed) {
            return crate::EXIT_FAIL;
        }
        if req.clone().any(|o| o.status == Status::NotApplicable) {
            return crate::EXIT_UNVERIFIABLE;
        }
        crate::EXIT_PASS
    }
    pub fn json(&self) -> serde_json::Value {
        use serde_json::json;
        json!({
            "base": self.base.name(),
            "base_fail_codes": self.base_fail_codes,
            "all_required_detected": self.all_detected(),
            "classes": self.outcomes.iter().map(|o| json!({
                "class": o.class,
                "required": o.required,
                "variant": o.variant,
                "what": o.what,
                "feature": o.feature,
                "expected": o.expected,
                "status": match o.status { Status::Detected => "detected", Status::Missed => "missed", Status::NotApplicable => "not-applicable" },
                "verdict": o.verdict.map(|v| v.name()),
                "fail_codes": o.fail_codes,
                "detail": o.detail,
            })).collect::<Vec<_>>(),
        })
    }
    pub fn markdown(&self) -> String {
        let mut s = format!(
            "# Tamper suite\n\nBase run: **{}**{}. Required classes detected: **{}**.\n\n| class | variant | expected | status | FAIL codes |\n|---|---|---|---|---|\n",
            self.base.name(),
            if self.base_fail_codes.is_empty() {
                String::new()
            } else {
                format!(" ({})", self.base_fail_codes.join(", "))
            },
            if self.all_detected() { "yes" } else { "no" }
        );
        for o in &self.outcomes {
            s.push_str(&format!(
                "| {}{} | {} | {} | {} | {} |\n",
                o.class,
                if o.required { "" } else { " (extra)" },
                o.variant,
                o.expected.join("/"),
                match o.status {
                    Status::Detected => "detected".to_string(),
                    Status::Missed => format!("**missed** ({})", o.detail),
                    Status::NotApplicable => format!("**not applicable** ({})", o.detail),
                },
                o.fail_codes.join(", ")
            ));
        }
        s
    }
}

fn fail_codes(r: &crate::Report) -> Vec<String> {
    let mut v: Vec<String> = r
        .findings
        .iter()
        .filter(|f| f.fail())
        .map(|f| f.code.clone())
        .collect();
    v.sort();
    v.dedup();
    v
}

/// Builds and judges one class (a builder that panics counts as not
/// applicable, with its message).
pub fn judge_class(c: &Class, run: &Input) -> Outcome {
    let mut why = vec![];
    for (variant, b) in &c.tries {
        let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| b(run)));
        let case = match made {
            Ok(Ok(x)) => x,
            Ok(Err(e)) => {
                why.push(format!("{variant}: {e}"));
                continue;
            }
            Err(p) => {
                let m = p
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default();
                why.push(format!("{variant}: builder panicked: {m}"));
                continue;
            }
        };
        let r = crate::verify(&case.input);
        let hit = case.codes.iter().any(|x| r.fails_with(x));
        return Outcome {
            class: c.class,
            required: c.required,
            variant,
            what: case.what.to_string(),
            feature: case.feature,
            expected: case.codes.to_vec(),
            status: if r.verdict == crate::Verdict::Fail && hit {
                Status::Detected
            } else {
                Status::Missed
            },
            verdict: Some(r.verdict),
            fail_codes: fail_codes(&r),
            detail: if hit {
                String::new()
            } else {
                format!("verdict {}", r.verdict.name())
            },
        };
    }
    Outcome {
        class: c.class,
        required: c.required,
        variant: "",
        what: String::new(),
        feature: None,
        expected: vec![],
        status: Status::NotApplicable,
        verdict: None,
        fail_codes: vec![],
        detail: why.join("; "),
    }
}

/// The whole suite over one run, `jobs` classes at a time (each class
/// verifies its own tampered copy of the run).
pub fn run_suite(run: &Input, jobs: usize) -> SuiteReport {
    let base = crate::verify(run);
    let cs = classes();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let out: std::sync::Mutex<Vec<(usize, Outcome)>> = Default::default();
    std::thread::scope(|sc| {
        for _ in 0..jobs.max(1) {
            sc.spawn(|| loop {
                let k = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let Some(c) = cs.get(k) else { break };
                let o = judge_class(c, run);
                if let Ok(mut v) = out.lock() {
                    v.push((k, o));
                }
            });
        }
    });
    let mut v = out.into_inner().unwrap_or_default();
    v.sort_by_key(|(k, _)| *k);
    SuiteReport {
        base: base.verdict,
        base_fail_codes: fail_codes(&base),
        outcomes: v.into_iter().map(|(_, o)| o).collect(),
    }
}

/// Every class over the recorded fixtures (`program`: the march season
/// recorded from the merged program, integ-W4 review).
pub fn all(land: &Input, march: &Input, program: &Input) -> Vec<Case> {
    let tries: Vec<(&Input, Builder)> = vec![
        (march, t01),
        (land, t01b),
        (march, t02),
        (land, t03),
        (land, t04),
        (land, t05),
        (march, t06),
        (program, t06b),
        (program, t23),
        (program, t23b),
        (program, h1),
        (march, t07),
        (march, t08),
        (march, t09),
        (land, t10),
        (land, t11),
        (march, t12),
        (march, t13),
        (land, t14),
        (march, t15),
        (land, t16),
        (march, t17),
        (land, t18),
        (land, t19),
        (march, t20),
        (march, t21),
        (march, t22),
        (land, v9a),
    ];
    tries
        .into_iter()
        .map(|(i, b)| b(i).unwrap_or_else(|e| panic!("a fixture class: {e}")))
        .collect()
}
