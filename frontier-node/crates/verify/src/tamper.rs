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
pub fn t01(march: &Input) -> Case {
    let mut inp = march.clone();
    let (t, _) = *find(&inp, Kind::DEPART).last().expect("a DEPART");
    inp.txs.remove(t);
    Case {
        class: "T1",
        what: "drop a Depart",
        feature: Some("mutate-v1"),
        codes: &[CHAIN_GAP, HEAD_MISMATCH],
        input: inp,
    }
}

/// T2: flip a byte of a Reveal's plaintext (the stance).
pub fn t02(march: &Input) -> Case {
    let mut inp = march.clone();
    let (t, _) = find(&inp, Kind::REVEAL)[1];
    edit_ix(&mut inp, t, Ix::Reveal, |d| d[1 + 2 + 20] ^= 1);
    Case {
        class: "T2",
        what: "flip a Reveal plaintext byte",
        feature: Some("mutate-v5"),
        codes: &[REVEAL_COMMIT_MISMATCH],
        input: inp,
    }
}

/// T3: shift an anchor's A (an anchor no ticket, explore or clash reads).
pub fn t03(land: &Input) -> Case {
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
        .expect("an unused anchor");
    let (o, _) = field(Kind::ANCHOR, "a");
    edit_payload(&mut inp, t, i, |p| {
        let a = le(&p[o..o + 8]) as i64 + 7;
        p[o..o + 8].copy_from_slice(&a.to_le_bytes());
    });
    Case {
        class: "T3",
        what: "shift an anchor's A",
        feature: Some("mutate-v3"),
        codes: &[SEED_ROUND_RULE],
        input: inp,
    }
}

/// T4: inject a second anchor for a (bell, region).
pub fn t04(land: &Input) -> Case {
    let mut inp = land.clone();
    let (t, i) = find(&inp, Kind::ANCHOR)[3];
    let mut bs = bodies(&inp, t);
    let dup = bs[i].clone();
    bs.push(dup.clone());
    // One more PS2 line in the same frame (before its success line).
    let pos = inp.txs[t]
        .logs
        .iter()
        .rposition(|l| l.ends_with(" success"))
        .expect("frame");
    inp.txs[t].logs.insert(pos, log_line(&dup));
    Case {
        class: "T4",
        what: "inject a second anchor",
        feature: Some("mutate-v3"),
        codes: &[DUPLICATE_ANCHOR],
        input: inp,
    }
}

/// T5: swap a cache signature for another round's.
pub fn t05(land: &Input) -> Case {
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
    let (ts, _) = seeds[seeds.len() / 2];
    edit_ix(&mut inp, ts, Ix::PostSeed, |d| {
        d[1 + 1 + 4 + 1 + 8..1 + 1 + 4 + 1 + 8 + 48].copy_from_slice(&other)
    });
    Case {
        class: "T5",
        what: "swap a cache signature",
        feature: Some("mutate-v3"),
        codes: &[SEED_ROUND_RULE, BEACON_SIG_INVALID],
        input: inp,
    }
}

/// T6: `set_account` on a Province after its last resolve (the final bytes
/// differ from the last transaction's write).
pub fn t06(march: &Input) -> Case {
    let mut inp = march.clone();
    let (w, f) = world(&inp);
    let (t, i) = *find(&inp, Kind::CLASH).last().expect("a CLASH");
    let r = &w.recs[rec_at(&w, t, i)];
    let pk = f.ctx.province(r.pq().0, r.pq().1);
    if let Some(Some(a)) = inp.finals.get_mut(&pk) {
        let o = frontier_abi::layout::province::province::entry(0)
            + frontier_abi::layout::province::entry::TROOPS;
        let v = le(&a.data[o..o + 4]) as u32 + 50_000;
        a.data[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }
    Case {
        class: "T6",
        what: "set_account on a Province after a resolve",
        feature: Some("mutate-v1"),
        codes: &[HEAD_MISMATCH, CLASH_REPLAY_MISMATCH],
        input: inp,
    }
}

/// T6b: a rogue write between two resolves of one province; the program
/// resolves the second from the rogue state (its CLASH re-chained): the
/// replay from the chain's last write disagrees.
pub fn t06b(march: &Input) -> Case {
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
        .expect("two clashes in one province, a resident before the second");
    let r2 = &w.recs[rec_at(&w, t2, i2)];
    let b2 = r2.ku32("bell");
    let pk = f.ctx.province(pq.0, pq.1);
    let mut before = w.state_before(&pk, t2).expect("state").to_vec();
    // The rogue write: the first roster entry loses troops.
    let e = resident(&before).expect("a resident");
    let o = frontier_abi::layout::province::province::entry(e)
        + frontier_abi::layout::province::entry::TROOPS;
    let v2 = le(&before[o..o + 4]) as u32 / 2;
    before[o..o + 4].copy_from_slice(&v2.to_le_bytes());
    let ci = w
        .state_before(&f.ctx.clash_inputs(pq.0, pq.1, b2), t2)
        .expect("inputs");
    let seed = f.bell_seed(b2, Facts::region(pq.0, pq.1)).expect("seed");
    let built = ContractBuilder
        .build(&before, ci, b2, &seed)
        .expect("build");
    let out = built.resolve().expect("resolve");
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
    Case {
        class: "T6b",
        what: "a rogue Province write between two resolves",
        feature: Some("mutate-v7"),
        codes: &[CLASH_REPLAY_MISMATCH],
        input: inp,
    }
}

/// T7: a valid seal's settlement logged as a bad seal (consistently: the
/// bad-seal outcome and payments).
pub fn t07(march: &Input) -> Case {
    let mut inp = march.clone();
    let (w, f) = world(&inp);
    let (t, i) = *find(&inp, Kind::TRANSIT_SETTLED)
        .iter()
        .find(|(t, i)| {
            let r = &w.recs[rec_at(&w, *t, *i)];
            r.pu8("seal_code") == 0 && r.pu8("outcome") == transit_outcome::STAYS
        })
        .expect("a valid settlement");
    let r = &w.recs[rec_at(&w, t, i)];
    let host = r.ku64("host_id");
    let tip = f
        .depart_of(&w, host, rec_at(&w, t, i), None)
        .map(|d| w.recs[d].pu64("tip"))
        .unwrap_or(0);
    let p = f.params.expect("params");
    let settler: [u8; 32] = w.txs[t]
        .ix(Ix::SettleTransit.tag())
        .and_then(|x| x.key(11))
        .expect("settler");
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
    Case {
        class: "T7",
        what: "a valid seal marked bad at settlement",
        feature: Some("mutate-v5"),
        codes: &[VERDICT_DISAGREES_WITH_TLOCK],
        input: inp,
    }
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
pub fn t08(march: &Input) -> Case {
    let mut inp = march.clone();
    let (w, _) = world(&inp);
    let (t, i) = *find(&inp, Kind::REVEAL)
        .iter()
        .find(|(t, i)| w.recs[rec_at(&w, *t, *i)].pu8("displace") == 1)
        .expect("a displacement");
    let (oi, _) = plog::field(Kind::REVEAL, "i", false).expect("i");
    edit_key(&mut inp, t, i, |k| k[oi] = (k[oi] + 1) % 4);
    Case {
        class: "T8",
        what: "change which slot a displacement hit",
        feature: Some("mutate-v6"),
        codes: &[QUOTA_SET_MISMATCH],
        input: inp,
    }
}

/// T9: alter a departure mass (of an arrival that stayed; re-chained).
pub fn t09(march: &Input) -> Case {
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
        .expect("a departure that stayed");
    let (o, _) = field(Kind::DEPART, "dep_mass");
    edit_payload(&mut inp, t, i, |p| {
        let v = le(&p[o..o + 4]) as u32 + 1_000;
        p[o..o + 4].copy_from_slice(&v.to_le_bytes());
    });
    rechain(&mut inp);
    Case {
        class: "T9",
        what: "alter a departure mass",
        feature: Some("mutate-v6"),
        codes: &[TRANSIT_MASS_MISMATCH],
        input: inp,
    }
}

/// T10: the wrong drand key.
pub fn t10(land: &Input) -> Case {
    let mut inp = land.clone();
    inp.cfg.quicknet_pk =
        crate::input::hex_arr(fclient::beacon::QUICKNET_PK).expect("quicknet key");
    Case {
        class: "T10",
        what: "wrong quicknet key",
        feature: Some("mutate-v3"),
        codes: &[BEACON_SIG_INVALID],
        input: inp,
    }
}

/// T11: the wrong ruleset hash.
pub fn t11(land: &Input) -> Case {
    let mut inp = land.clone();
    inp.cfg.ruleset_hash[0] ^= 0xFF;
    Case {
        class: "T11",
        what: "wrong ruleset hash",
        feature: Some("mutate-v2"),
        codes: &[RULESET_MISMATCH],
        input: inp,
    }
}

/// T12: truncate the last game day (the final states stay the chain's).
pub fn t12(march: &Input) -> Case {
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
    Case {
        class: "T12",
        what: "truncate the last game day",
        feature: Some("mutate-v1"),
        codes: &[HEAD_MISMATCH, CHAIN_GAP],
        input: inp,
    }
}

fn beacon_day(f: &Facts, t: i64) -> Option<u32> {
    permutation_rules::frontier::beacon::bell_at(f.genesis_ts, t).map(|b| b / 144)
}

/// T13: move a Reveal past `A + W`.
pub fn t13(march: &Input) -> Case {
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
        .expect("a Reveal after its anchor");
    inp.txs[t].block_time = f.close(arrive, a.a) + 5;
    Case {
        class: "T13",
        what: "move a Reveal past A + W",
        feature: Some("mutate-v4"),
        codes: &[REVEAL_AFTER_CLOSE],
        input: inp,
    }
}

/// T14: duplicate a transaction.
pub fn t14(land: &Input) -> Case {
    let mut inp = land.clone();
    let (t, _) = find(&inp, Kind::FOLD)[2];
    let dup = inp.txs[t].clone();
    inp.txs.insert(t + 1, dup);
    Case {
        class: "T14",
        what: "duplicate a transaction",
        feature: Some("mutate-v1"),
        codes: &[DUPLICATE_EVENT],
        input: inp,
    }
}

/// T15: origin values from a later bell — a gathered arrival's troops are
/// not its departure's; the forger recomputes the CLASH and the transit.
pub fn t15(march: &Input) -> Case {
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
        .expect("a clash to forge");
    let ck = f.ctx.clash_inputs(pq.0, pq.1, b);
    let o = CI::arrival(pos) + AR::TROOPS;
    // The gathered troops, as the forger logs them.
    let gather_tx = (0..t)
        .rev()
        .find(|&x| w.txs[x].post.contains_key(&ck))
        .expect("gather");
    edit_account_from(&mut inp, &ck, gather_tx, |d| {
        let v = le(&d[o..o + 4]) as u32 - 1_000;
        d[o..o + 4].copy_from_slice(&v.to_le_bytes());
    });
    let (w2, f2) = world(&inp);
    let pk = f2.ctx.province(pq.0, pq.1);
    let built = ContractBuilder
        .build(
            w2.state_before(&pk, t).expect("province"),
            w2.state_before(&ck, t).expect("inputs"),
            b,
            &f2.bell_seed(b, Facts::region(pq.0, pq.1)).expect("seed"),
        )
        .expect("build");
    let out = built.resolve().expect("resolve");
    let fates = packed_fates(&built, &out);
    let troops_after = out.fighter(host).map(|x| x.troops).unwrap_or(0);
    let (od, _) = field(Kind::CLASH, "outcome_digest");
    let (oe, _) = field(Kind::CLASH, "engagements");
    let (of, _) = field(Kind::CLASH, "fates");
    let (oi, _) = field(Kind::CLASH, "input_digest");
    let in_digest = crate::clash_input::input_digest(
        w2.state_before(&pk, t).expect("province"),
        w2.state_before(&ck, t).expect("inputs"),
        b,
        &f2.bell_seed(b, Facts::region(pq.0, pq.1)).expect("seed"),
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
            .expect("the Province written")
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
        .expect("settlement");
    let (ot, _) = field(Kind::TRANSIT_SETTLED, "troops");
    edit_payload(&mut inp, tt, ti, |p| {
        p[ot..ot + 4].copy_from_slice(&troops_after.to_le_bytes())
    });
    rechain(&mut inp);
    Case {
        class: "T15",
        what: "origin values from a later bell",
        feature: Some("mutate-v8"),
        codes: &[ORIGIN_VALUE_MISMATCH],
        input: inp,
    }
}

/// T16: the wrong genesis round (another round's signature and seed, as a
/// forged program would accept them; re-chained).
pub fn t16(land: &Input) -> Case {
    let mut inp = land.clone();
    let (w, _) = world(&inp);
    let (ta, _) = find(&inp, Kind::ANCHOR)[0];
    let ix = w.txs[ta]
        .ix(Ix::PostAnchor.tag())
        .or_else(|| w.txs[ta].ix(Ix::PostAnchorMulti.tag()))
        .expect("anchor ix");
    let (ro, so) = if ix.tag() == Some(Ix::PostAnchor.tag()) {
        (6, 14)
    } else {
        (5, 13)
    };
    let round = le(&ix.data[ro..ro + 8]);
    let sig: [u8; 48] = ix.data[so..so + 48].try_into().expect("48");
    let hints = ix.data[so + 48..so + 48 + frontier_abi::ix::HINTS_LEN].to_vec();
    let seed = crate::checks::v3_random::seed_of48(round, &sig).expect("seed");
    let (t, i) = find(&inp, Kind::GENESIS_SEED)[0];
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
    Case {
        class: "T16",
        what: "wrong genesis round",
        feature: Some("mutate-v3"),
        codes: &[GENESIS_SEED_RULE],
        input: inp,
    }
}

/// T17: a SkipQuiet over a bell with an ArrivalSlot (a run extended over
/// the next bell, which had a revealed arrival; re-chained).
pub fn t17(march: &Input) -> Case {
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
        .expect("a skip just before an arrival bell");
    let (on, _) = field(Kind::SKIP, "n");
    edit_payload(&mut inp, t, i, |p| p[on] += 1);
    rechain(&mut inp);
    Case {
        class: "T17",
        what: "a SkipQuiet over a bell with an ArrivalSlot",
        feature: Some("mutate-v7"),
        codes: &[SKIP_NOT_QUIET, SKIP_OVER_ARRIVAL],
        input: inp,
    }
}

/// T18: a tampered SETTLE score (the Holding written with it; re-chained).
pub fn t18(land: &Input) -> Case {
    let mut inp = land.clone();
    let (w, f) = world(&inp);
    let (t, i) = *find(&inp, Kind::SETTLE)
        .iter()
        .find(|(t, i)| w.recs[rec_at(&w, *t, *i)].pu8("outcome") == plog::settle_outcome::FRESH)
        .expect("a fresh settlement");
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
    Case {
        class: "T18",
        what: "a tampered SETTLE score",
        feature: Some("mutate-v11"),
        codes: &[TICKET_SCORE_MISMATCH],
        input: inp,
    }
}

/// T19: a tampered terrain digest (re-chained).
pub fn t19(land: &Input) -> Case {
    let mut inp = land.clone();
    let (t, i) = find(&inp, Kind::PROVINCE_OPEN)[5];
    let (o, _) = field(Kind::PROVINCE_OPEN, "terrain_digest");
    edit_payload(&mut inp, t, i, |p| p[o] ^= 1);
    rechain(&mut inp);
    Case {
        class: "T19",
        what: "a tampered terrain digest",
        feature: Some("mutate-v11"),
        codes: &[TERRAIN_MISMATCH],
        input: inp,
    }
}

/// T20: a ClaimDefence amount above the formula.
pub fn t20(march: &Input) -> Case {
    let mut inp = march.clone();
    let (t, i) = find(&inp, Kind::DEFENCE_CLAIM)[0];
    let (o, _) = field(Kind::DEFENCE_CLAIM, "amount");
    edit_payload(&mut inp, t, i, |p| {
        let v = le(&p[o..o + 8]) + 1_000;
        p[o..o + 8].copy_from_slice(&v.to_le_bytes());
    });
    Case {
        class: "T20",
        what: "a ClaimDefence amount above the formula",
        feature: Some("mutate-v13"),
        codes: &[DEFENCE_REFUND_MISMATCH],
        input: inp,
    }
}

/// T21: a tampered explore find (re-chained).
pub fn t21(march: &Input) -> Case {
    let mut inp = march.clone();
    let (t, i) = find(&inp, Kind::EXPLORE_RESULT)[0];
    let (o, _) = field(Kind::EXPLORE_RESULT, "works_per_tile");
    let (ow, _) = field(Kind::EXPLORE_RESULT, "works");
    edit_payload(&mut inp, t, i, |p| {
        let v = le(&p[o..o + 4]) as u32 + 1;
        p[o..o + 4].copy_from_slice(&v.to_le_bytes());
        let tw = le(&p[ow..ow + 4]) as u32 + 1;
        p[ow..ow + 4].copy_from_slice(&tw.to_le_bytes());
    });
    rechain(&mut inp);
    Case {
        class: "T21",
        what: "a tampered explore find",
        feature: Some("mutate-v12"),
        codes: &[EXPLORE_ROLL_MISMATCH],
        input: inp,
    }
}

/// T22: a bad-seal transit logged as surviving (its clash fate, a zero
/// code, the fate's troops and payments; re-chained).
pub fn t22(march: &Input) -> Case {
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
        .expect("a revealed bad seal");
    let ci = fclient::decode::ClashInputs::decode(
        w.state_before(&f.ctx.clash_inputs(pq.0, pq.1, b), t)
            .expect("inputs"),
    )
    .expect("decode");
    let a = ci
        .arrivals
        .iter()
        .find(|a| a.present == 1 && a.host_id == host)
        .expect("arrival");
    let troops = match a.fate {
        transit_outcome::STAYS | transit_outcome::WITHDREW => a.troops_after,
        transit_outcome::DESTROYED => 0,
        _ => a.troops,
    };
    let ix = w.txs[t].ix(Ix::SettleTransit.tag()).expect("ix");
    let pre = |k: usize| -> [u8; 8] {
        ix.key(k)
            .map(|x| x[..8].try_into().unwrap_or([0; 8]))
            .unwrap_or([0; 8])
    };
    let p = f.params.expect("params");
    let tip = f
        .depart_of(&w, host, rec_at(&w, t, i), None)
        .map(|d| w.recs[d].pu64("tip"))
        .unwrap_or(0);
    let resolver: [u8; 8] = ci.resolver.to_bytes()[..8].try_into().unwrap_or([0; 8]);
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
        0,
    );
    rechain(&mut inp);
    Case {
        class: "T22",
        what: "a bad-seal transit logged as surviving",
        feature: Some("mutate-v5"),
        codes: &[BAD_SEAL_SURVIVED],
        input: inp,
    }
}

/// T1b (integ-W4 review, W4-D major): drop an **unchained** transaction —
/// the last PostAnchor (Multi) and the PostSeeds of its bell — keeping the
/// anchor and cache accounts in the finals. Nothing chains them, so before
/// the review V1 passed it; now an account no transaction explains is a
/// gap.
pub fn t01b(land: &Input) -> Case {
    let mut inp = land.clone();
    let (w, _) = world(&inp);
    let anchors = find(&inp, Kind::ANCHOR);
    let (ta, ia) = *anchors.last().expect("an ANCHOR");
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
    Case {
        class: "T1b",
        what: "drop an unchained transaction (an anchor and its seeds)",
        feature: Some("mutate-v1"),
        codes: &[CHAIN_GAP],
        input: inp,
    }
}

/// T23 (integ-W4 review, W4-D major): a resolve's **write-back** forged —
/// +25,000 milli-troops written to a resident in the last CLASH's Province
/// post-state (and every later state of it, and the final). The digests,
/// fates and chains are untouched (account bytes are not chained), so only
/// V7's write-back replay sees it.
pub fn t23(march: &Input) -> Case {
    let mut inp = march.clone();
    let (w, f) = world(&inp);
    let (t, i) = *find(&inp, Kind::CLASH).last().expect("a CLASH");
    let r = &w.recs[rec_at(&w, t, i)];
    let pk = f.ctx.province(r.pq().0, r.pq().1);
    let post = w.txs[t].post_data(&pk).expect("the Province's post-state");
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
        .expect("a resident after the clash");
    let o = frontier_abi::layout::province::province::entry(e)
        + frontier_abi::layout::province::entry::TROOPS;
    edit_account_from(&mut inp, &pk, t, |d| {
        let v = le(&d[o..o + 4]) as u32 + 25_000;
        d[o..o + 4].copy_from_slice(&v.to_le_bytes());
    });
    Case {
        class: "T23",
        what: "a resolve's write-back forged in its own transaction",
        feature: Some("mutate-v7"),
        codes: &[CLASH_REPLAY_MISMATCH],
        input: inp,
    }
}

/// V9 (no §8.5 class; the check of its check): an account at a
/// non-canonical address — an anchor whose stored region is not the one
/// its address derives (its post-state and final state agree, so only V9
/// sees it).
pub fn v9a(land: &Input) -> Case {
    use frontier_abi::layout::beacon::bell_anchor as BA;
    let mut inp = land.clone();
    let (w, f) = world(&inp);
    let a = w.recs[rec_at(
        &w,
        find(&inp, Kind::ANCHOR)[2].0,
        find(&inp, Kind::ANCHOR)[2].1,
    )]
    .clone();
    let k = f.ctx.bell_anchor(a.ku32("bell"), a.ku8("region"));
    edit_account_from(&mut inp, &k, 0, |d| d[BA::REGION] ^= 1);
    Case {
        class: "V9a",
        what: "an account at a non-canonical address",
        feature: Some("mutate-v9"),
        codes: &[NON_CANONICAL_ADDRESS],
        input: inp,
    }
}

/// Every class over the recorded fixtures (`program`: the march season
/// recorded from the merged program, integ-W4 review).
pub fn all(land: &Input, march: &Input, program: &Input) -> Vec<Case> {
    vec![
        t01(march),
        t01b(land),
        t02(march),
        t03(land),
        t04(land),
        t05(land),
        t06(march),
        t06b(program),
        t23(program),
        t07(march),
        t08(march),
        t09(march),
        t10(land),
        t11(land),
        t12(march),
        t13(march),
        t14(land),
        t15(march),
        t16(land),
        t17(march),
        t18(land),
        t19(land),
        t20(march),
        t21(march),
        t22(march),
        v9a(land),
    ]
}
