//! V5 — seals (contract §5.11, §8.5; I-06, I-27, I-28, I-44).
//!
//! Every DEPART's seal is opened with the **stock `tlock` crate** (the
//! round `T(arrive)` signature some anchor transaction carried, verified
//! under the pinned key) and the PS-KS body, its commitment recomputed and
//! its plaintext validated (`fclient::seal::judge`, independent of the
//! program's opener). Then:
//!
//! - DEPART's `seal_root = sha256(commit ‖ sha256(seal))`;
//! - every Reveal's plaintext and salt give DEPART's commitment and root,
//!   and a valid seal's revealed plaintext is the opened one;
//! - SettleTransit judged the logged pair (its commit and seal = DEPART's);
//! - **every TRANSIT_SETTLED seal code is the stock opener's** (exactly;
//!   codes 1 and 3 are not separable off chain, integ-W4 review);
//! - **a bad seal whose transit settled with another outcome is a FAIL**.
//!
//! A seal the verifier cannot open — no transaction carried a verified
//! signature of `T(arrive)` — is `MissingData` wherever a verdict needs it:
//! a Reveal, a settlement, an unsettled march whose arrival bell ended a
//! bell before the archive does (W5-D, the integ-W4 review).
//!
//! Warnings (liveness, E5 gates them): a valid seal never revealed
//! (`ValidSealUnrevealed`, with the signatures of failed Reveal attempts),
//! a bad seal still unsettled at the end (`BadSealUnsettled`).
//!
//! Codes: `RevealCommitMismatch`, `VerdictDisagreesWithTlock`,
//! `BadSealSurvived`; warn `ValidSealUnrevealed`, `BadSealUnsettled`.

use std::collections::{HashMap, HashSet};

use fclient::seal::{self, Plain, SEAL_LEN};
use frontier_abi::ix as aix;
use frontier_abi::log::{transit_outcome, Kind};
use frontier_abi::tags::Ix;

use super::Ctx;
use crate::codes::*;
use crate::world::{Rec, World};

const V: &str = "V5";

/// The stock judgement of one DEPART.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Judged {
    pub code: u8,
    pub plain: Option<Plain>,
}

/// Judges the seal of DEPART record `d` (`None`: no verified signature of
/// `T(arrive)` in the archive).
pub fn judge(cx: &mut Ctx, d: &Rec) -> Option<Judged> {
    let f = cx.f;
    let arrive = d.pu32("arrive_bell");
    let sig = f.good_sig(f.tlock_round(arrive), &mut cx.sigs)?;
    let seal: [u8; SEAL_LEN] = d.p("seal").try_into().ok()?;
    let (code, plain) = seal::judge(&seal, &d.p32("commit"), &sig, d.ku64("host_id"), arrive);
    Some(Judged { code, plain })
}

/// The Reveal instruction of a REVEAL record's transaction.
pub fn reveal_ix(w: &World, r: &Rec) -> Option<aix::Reveal> {
    w.txs[r.tx]
        .ix(Ix::Reveal.tag())
        .and_then(|i| aix::Reveal::decode(&i.data).ok())
}

pub fn run(cx: &mut Ctx) {
    let w = cx.w;
    let f = cx.f;
    let mut judged: HashMap<usize, Option<Judged>> = HashMap::new();
    let mut revealed: HashSet<usize> = HashSet::new();
    let mut settled: HashSet<usize> = HashSet::new();
    for (n, r) in w.recs.iter().enumerate() {
        match r.kind {
            Kind::DEPART => {
                let host = r.ku64("host_id");
                let root = seal::seal_root(&r.p32("commit"), &seal::ct_hash(r.p("seal")));
                if root != r.p32("seal_root") {
                    cx.fail(
                        V,
                        REVEAL_COMMIT_MISMATCH,
                        format!("host {host}"),
                        r.bell,
                        Some(r.tx),
                        "DEPART's seal_root is not sha256(commit ‖ sha256(seal))",
                    );
                }
            }
            Kind::REVEAL => {
                let host = r.pu64("host_id");
                let b = r.ku32("arrive");
                let what = format!("host {host} bell {b}");
                let Some(d) = f.depart_of(w, host, n, Some(b)) else {
                    cx.fail(
                        V,
                        REVEAL_COMMIT_MISMATCH,
                        what,
                        b,
                        Some(r.tx),
                        "a reveal of a march with no DEPART",
                    );
                    continue;
                };
                revealed.insert(d);
                let dr = &w.recs[d];
                let Some(x) = reveal_ix(w, r) else {
                    cx.missing(
                        V,
                        what,
                        b,
                        Some(r.tx),
                        "the Reveal's instruction data is not in the archive",
                    );
                    continue;
                };
                let c = seal::commit(&x.plain, &x.salt);
                if c != dr.p32("commit") || seal::seal_root(&c, &x.ct_hash) != dr.p32("seal_root") {
                    cx.fail(
                        V,
                        REVEAL_COMMIT_MISMATCH,
                        what.clone(),
                        b,
                        Some(r.tx),
                        "the revealed plaintext and salt do not give the departure's commitment",
                    );
                    continue;
                }
                let j = *judged.entry(d).or_insert_with(|| judge(cx, dr));
                if j.is_none() {
                    // W5-D (integ-W4 review): a seal that cannot be opened
                    // is not a seal that was checked.
                    cx.missing(
                        V,
                        what.clone(),
                        b,
                        Some(r.tx),
                        "no verified signature of T(arrive): the revealed plaintext cannot be compared with the opened seal",
                    );
                }
                if let Some(Judged {
                    code: 0,
                    plain: Some(p),
                }) = j
                {
                    if seal::pack(&p) != x.plain {
                        cx.fail(
                            V,
                            REVEAL_COMMIT_MISMATCH,
                            what,
                            b,
                            Some(r.tx),
                            "the revealed plaintext is not the one the seal opens to",
                        );
                    }
                }
            }
            Kind::TRANSIT_SETTLED => {
                let host = r.ku64("host_id");
                let what = format!("host {host}");
                let Some(d) = f.departs.get(&host).and_then(|v| {
                    v.iter()
                        .rev()
                        .copied()
                        .find(|&d| d < n && !settled.contains(&d))
                }) else {
                    cx.fail(
                        V,
                        VERDICT_DISAGREES_WITH_TLOCK,
                        what,
                        r.bell,
                        Some(r.tx),
                        "a settlement of a march with no DEPART",
                    );
                    continue;
                };
                settled.insert(d);
                let dr = &w.recs[d];
                let x = w.txs[r.tx]
                    .ix(Ix::SettleTransit.tag())
                    .and_then(|i| aix::SettleTransit::decode(&i.data).ok());
                match x {
                    Some(x) if x.commit == dr.p32("commit") && x.seal[..] == *dr.p("seal") => {}
                    Some(_) => cx.fail(
                        V,
                        REVEAL_COMMIT_MISMATCH,
                        what.clone(),
                        r.bell,
                        Some(r.tx),
                        "SettleTransit judged another pair than the one DEPART logged",
                    ),
                    None => cx.missing(
                        V,
                        what.clone(),
                        r.bell,
                        Some(r.tx),
                        "SettleTransit's data is not in the archive",
                    ),
                }
                let j = *judged.entry(d).or_insert_with(|| judge(cx, dr));
                let Some(j) = j else {
                    cx.missing(
                        V,
                        what,
                        r.bell,
                        Some(r.tx),
                        "no verified signature of T(arrive) to open the seal",
                    );
                    continue;
                };
                let (code, outcome) = (r.pu8("seal_code"), r.pu8("outcome"));
                // The exact code, but for the documented ambiguity: codes 1
                // (FO check) and 3 are not separable off chain (integ-W4
                // review, W4-D minor: only `> 0` was compared).
                let same = |a: u8, b: u8| a == b || (matches!(a, 1 | 3) && matches!(b, 1 | 3));
                if !same(code, j.code) {
                    cx.fail(
                        V,
                        VERDICT_DISAGREES_WITH_TLOCK,
                        what.clone(),
                        r.bell,
                        Some(r.tx),
                        format!("seal code {code}, the stock opener gives {}", j.code),
                    );
                }
                if j.code > 0 && outcome != transit_outcome::BAD_SEAL {
                    cx.fail(
                        V,
                        BAD_SEAL_SURVIVED,
                        what.clone(),
                        r.bell,
                        Some(r.tx),
                        format!(
                            "a bad seal (stock code {}) settled with outcome {outcome}",
                            j.code
                        ),
                    );
                }
                if j.code == 0 && outcome == transit_outcome::BAD_SEAL {
                    cx.fail(
                        V,
                        VERDICT_DISAGREES_WITH_TLOCK,
                        what.clone(),
                        r.bell,
                        Some(r.tx),
                        "a valid seal destroyed as bad",
                    );
                }
                if j.code == 0 && !revealed.contains(&d) {
                    let attempts = failed_reveals(w, host);
                    cx.warn(
                        V,
                        VALID_SEAL_UNREVEALED,
                        what,
                        dr.pu32("arrive_bell"),
                        Some(r.tx),
                        format!(
                            "a valid seal never revealed ({} failed attempts)",
                            attempts.len()
                        ),
                    );
                    cx.liveness
                        .valid_unrevealed
                        .push((host, dr.pu32("arrive_bell"), attempts));
                }
            }
            _ => {}
        }
    }
    // Bad seals never settled (liveness; E5 criterion 1).
    let last_time = w.txs.iter().map(|t| t.time).max().unwrap_or(0);
    let all: Vec<usize> = f.departs.values().flatten().copied().collect();
    for d in all {
        if settled.contains(&d) {
            continue;
        }
        let dr = &w.recs[d];
        let j = judge(cx, dr);
        // W5-D: a march whose arrival bell ended a whole bell before the
        // archive does, and whose T(arrive) no transaction carried, cannot
        // be judged (a bad seal could hide there).
        let arrive = dr.pu32("arrive_bell");
        let ended = permutation_rules::frontier::beacon::bell_end(f.genesis_ts, arrive);
        if j.is_none() && f.genesis_ts > 0 && ended + 600 <= last_time {
            cx.missing(
                V,
                format!("host {}", dr.ku64("host_id")),
                arrive,
                Some(dr.tx),
                "an unsettled march with no verified signature of T(arrive) in the archive",
            );
        }
        if let Some(Judged { code, .. }) = j {
            if code > 0 {
                cx.warn(
                    V,
                    BAD_SEAL_UNSETTLED,
                    format!("host {}", dr.ku64("host_id")),
                    dr.pu32("arrive_bell"),
                    Some(dr.tx),
                    format!("a bad seal (code {code}) still unsettled at the end"),
                );
            }
        }
    }
}

/// Signatures of failed Reveal transactions of `host`.
fn failed_reveals(w: &World, host: u64) -> Vec<String> {
    w.txs
        .iter()
        .filter(|t| !t.ok)
        .filter(|t| {
            t.ix(Ix::Reveal.tag())
                .and_then(|i| aix::Reveal::decode(&i.data).ok())
                .is_some_and(|x| seal::unpack(&x.plain).host_id == host)
        })
        .map(|t| t.sig.clone())
        .collect()
}
