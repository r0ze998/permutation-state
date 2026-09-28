//! V13 — payments (contract §5.11 SettleTransit D5, §5.12, §8.5; I-48,
//! I-52).
//!
//! - **TRANSIT_SETTLED:** the lamports each recipient receives (the four
//!   `(recipient prefix, amount)` pairs, DIVERT records of the same
//!   transaction moved back to their recipient, and `pool_owed_delta` for
//!   the Holding's pool) equal the rule's for the logged outcome, with `tip`
//!   = DEPART's, `fee = march_fee`, `bond = seal_bond`:
//!   bad seal → `tip + fee + bond` to the settle beneficiary; a fate of the
//!   records → tip to the slot's beneficiary, fee to the resolver, bond to
//!   the rent payer; `bounced-unranked` → tip and bond to the rent payer,
//!   fee to the resolver; `routed` with resolved inputs → tip to the pool,
//!   fee to the resolver, bond to the rent payer; `routed` without → tip
//!   and fee to the pool, bond to the rent payer. The recipients are the
//!   SettleTransit's own accounts; the slot beneficiary is the last Reveal's
//!   and the resolver the inputs'. The pairs name the intended recipients
//!   (W4-B P9: the Holding's own prefix is the pool), summed per recipient;
//!   `pool_owed_delta` = the pool's pairs + the transaction's DIVERT amounts
//!   (integ-W4 review).
//! - **DEFENCE_CLAIM:** the amount is `Σ fees::defence_refund(slot evidence,
//!   season)` over the claimed slots (each late by `≥ lateness_slots`
//!   against THE anchor's slot, unclaimed, the keeper's own), capped by the
//!   season's caps; a partial claim pays less, never more.
//! - SETTLE's escrow refunds and global lamport conservation are not
//!   judged in M1 (the contract makes conservation informational).
//!
//! Codes: `PaymentMismatch`, `DefenceRefundMismatch`.

use std::collections::BTreeMap;

use frontier_abi::ix as aix;
use frontier_abi::layout::clash::{arrival_slot as AS, clash_inputs as CI};
use frontier_abi::log::{transit_outcome, Kind};
use frontier_abi::tags::Ix;
use permutation_rules::frontier::fees::{self, DefenceParams};

use super::Ctx;
use crate::codes::*;
use crate::facts::Facts;
use crate::world::{le, Key};

const V: &str = "V13";

/// Lamports per recipient prefix; the pool is the zero prefix.
type Pay = BTreeMap<[u8; 8], u64>;

fn prefix(k: &Key) -> [u8; 8] {
    k[..8].try_into().unwrap_or([0; 8])
}

fn add(m: &mut Pay, k: [u8; 8], v: u64) {
    if v > 0 {
        *m.entry(k).or_default() += v;
    }
}

pub fn run(cx: &mut Ctx) {
    transits(cx);
    claims(cx);
}

fn transits(cx: &mut Ctx) {
    let w = cx.w;
    let f = cx.f;
    let Some(p) = f.params else { return };
    const POOL: [u8; 8] = [0; 8];
    for (n, r) in w
        .recs
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == Kind::TRANSIT_SETTLED)
    {
        let host = r.ku64("host_id");
        let what = format!("settlement of host {host}");
        if !w.has_post {
            cx.missing(
                V,
                what,
                r.bell,
                Some(r.tx),
                "no post-states: the slot's beneficiary and the resolver cannot be read",
            );
            continue;
        }
        let tx = &w.txs[r.tx];
        let Some(ix) = tx.ix(Ix::SettleTransit.tag()) else {
            cx.missing(
                V,
                what,
                r.bell,
                Some(r.tx),
                "SettleTransit's accounts are not in the archive",
            );
            continue;
        };
        let Some(d) = f.depart_of(w, host, n, None) else {
            continue;
        };
        let tip = w.recs[d].pu64("tip");
        let (fee, bond) = (p.march_fee, p.seal_bond);
        let acct = |i: usize| ix.key(i).map(|k| prefix(&k));
        let (Some(slot_ben), Some(resolver), Some(rent), Some(settler)) =
            (acct(8), acct(9), acct(10), acct(11))
        else {
            cx.missing(
                V,
                what,
                r.bell,
                Some(r.tx),
                "SettleTransit has fewer accounts than §5.11 lists",
            );
            continue;
        };
        // The recipients the accounts name must be the chain's: the slot's
        // beneficiary (its last Reveal), the inputs' resolver.
        let ins = ix
            .key(4)
            .and_then(|k| w.state_before(&k, r.tx))
            .filter(|d| d.len() >= CI::SIZE);
        let resolved = ins.is_some_and(|d| d[CI::FLAGS] & CI::FLAG_RESOLVED != 0);
        let chain_resolver = ins.map(|d| {
            prefix(
                &d[CI::RESOLVER..CI::RESOLVER + 32]
                    .try_into()
                    .unwrap_or([0; 32]),
            )
        });
        let chain_slot_ben = ix
            .key(5)
            .and_then(|k| w.state_before(&k, r.tx))
            .filter(|d| d.len() >= AS::SIZE && le(&d[AS::HOST_ID..AS::HOST_ID + 8]) == host)
            .map(|d| {
                prefix(
                    &d[AS::BENEFICIARY..AS::BENEFICIARY + 32]
                        .try_into()
                        .unwrap_or([0; 32]),
                )
            });
        let mut want = Pay::new();
        let outcome = r.pu8("outcome");
        match outcome {
            transit_outcome::BAD_SEAL => add(&mut want, settler, tip + fee + bond),
            transit_outcome::STAYS..=transit_outcome::DESTROYED => {
                add(&mut want, chain_slot_ben.unwrap_or(slot_ben), tip);
                add(&mut want, chain_resolver.unwrap_or(resolver), fee);
                add(&mut want, rent, bond);
            }
            transit_outcome::BOUNCED_UNRANKED => {
                add(&mut want, rent, tip + bond);
                add(&mut want, chain_resolver.unwrap_or(resolver), fee);
            }
            transit_outcome::ROUTED if resolved => {
                add(&mut want, POOL, tip);
                add(&mut want, chain_resolver.unwrap_or(resolver), fee);
                add(&mut want, rent, bond);
            }
            transit_outcome::ROUTED => {
                add(&mut want, POOL, tip + fee);
                add(&mut want, rent, bond);
            }
            _ => {}
        }
        // W4-B P9 (v1.6 §22): each pair names the **intended** recipient
        // (the Holding's own address for `pool_owed`); a payment the
        // runtime could not credit (an unfunded wallet below rent, a
        // program account) is diverted into `pool_owed` with a DIVERT
        // record, and `pool_owed_delta` counts routed and diverted
        // lamports. So the sums per intended recipient are the pairs'
        // (the Holding's prefix read as the pool), and the delta must be
        // the pool's pairs plus the transaction's DIVERTs (integ-W4
        // review: the DIVERTs were credited a second time).
        let holding = ix.key(2).map(|k| prefix(&k)).unwrap_or([0; 8]);
        let mut got = Pay::new();
        for (to, amt) in [
            ("tip_to", "tip"),
            ("fee_to", "fee"),
            ("bond_to", "bond"),
            ("reward_to", "reward"),
        ] {
            let k: [u8; 8] = r.p(to).try_into().unwrap_or([0; 8]);
            add(&mut got, if k == holding { POOL } else { k }, r.pu64(amt));
        }
        let diverted: u64 = tx
            .recs
            .iter()
            .map(|&i| &w.recs[i])
            .filter(|x| x.kind == Kind::DIVERT)
            .map(|dv| dv.pu64("amount"))
            .sum();
        let pool_ok = r.pu64("pool_owed_delta") == got.get(&POOL).copied().unwrap_or(0) + diverted;
        if !pool_ok {
            cx.fail(
                V,
                PAYMENT_MISMATCH,
                what.clone(),
                r.bell,
                Some(r.tx),
                format!(
                    "pool_owed_delta {} is not the pool's pairs {} plus the diverted {diverted}",
                    r.pu64("pool_owed_delta"),
                    got.get(&POOL).copied().unwrap_or(0)
                ),
            );
        }
        let named_ok = chain_slot_ben
            .is_none_or(|b| b == slot_ben || outcome != transit_outcome::STAYS)
            && chain_resolver.is_none_or(|b| b == resolver || !resolved);
        if want != got || !named_ok {
            cx.fail(
                V,
                PAYMENT_MISMATCH,
                what,
                r.bell,
                Some(r.tx),
                format!(
                    "outcome {outcome}: paid {}, the rule pays {}",
                    show(&got),
                    show(&want)
                ),
            );
        }
    }
}

fn show(m: &Pay) -> String {
    m.iter()
        .map(|(k, v)| {
            format!(
                "{}:{v}",
                if *k == [0; 8] {
                    "pool".into()
                } else {
                    hex::encode(k)
                }
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn claims(cx: &mut Ctx) {
    let w = cx.w;
    let f = cx.f;
    let Some(p) = f.params else { return };
    let s = DefenceParams {
        defence_cap_milli: p.defence_cap_milli,
        tip_min: fees::min_tip_lamports(
            p.min_reveal_priority_milli,
            p.reveal_cu_limit,
            p.reveal_loaded_limit,
        ),
    };
    let mut per_region: BTreeMap<(u32, u8), u64> = BTreeMap::new();
    let mut per_day: BTreeMap<([u8; 32], u32), u64> = BTreeMap::new();
    for r in w.of(Kind::DEFENCE_CLAIM) {
        let keeper: [u8; 32] = r.k("beneficiary").try_into().unwrap_or([0; 32]);
        let what = format!("claim of {}", crate::world::b58(&keeper));
        if !w.has_post {
            cx.missing(
                V,
                what,
                r.bell,
                Some(r.tx),
                "no post-states: the claimed slots' evidence cannot be read",
            );
            continue;
        }
        let tx = &w.txs[r.tx];
        let Some(ix) = tx.ix(Ix::ClaimDefence.tag()) else {
            cx.missing(
                V,
                what,
                r.bell,
                Some(r.tx),
                "ClaimDefence's accounts are not in the archive",
            );
            continue;
        };
        let day = aix::ClaimDefence::decode(&ix.data)
            .map(|x| x.day)
            .unwrap_or(0);
        let mut total = 0u64;
        let mut bad = None;
        let mut k = 5;
        while let (Some(sk), Some(ak)) = (ix.key(k), ix.key(k + 1)) {
            k += 2;
            let Some(sd) = w.state_before(&sk, r.tx) else {
                bad = Some("a claimed slot has no state before the claim".to_string());
                continue;
            };
            let Some(ev) = AS::evidence(sd) else { continue };
            let (bell, ev_slot) = (
                le(&sd[AS::BELL..AS::BELL + 4]) as u32,
                le(&sd[AS::EV_SLOT..AS::EV_SLOT + 8]),
            );
            let region = Facts::region(
                le(&sd[AS::P..AS::P + 2]) as u16 as i16 as i32,
                le(&sd[AS::Q..AS::Q + 2]) as u16 as i16 as i32,
            );
            let anchor = f.anchor(bell, region);
            if anchor.is_none_or(|a| {
                f.ctx.bell_anchor(bell, region) != ak || ev_slot < a.slot + p.lateness_slots as u64
            }) || sd[AS::BENEFICIARY..AS::BENEFICIARY + 32] != keeper
                || sd[AS::CLAIMED] != 0
            {
                bad = Some(format!(
                    "slot of bell {bell} is not eligible (lateness, beneficiary or claimed)"
                ));
                continue;
            }
            let mut x = fees::defence_refund(&ev, &s);
            let reg = per_region.entry((bell, region)).or_default();
            x = x.min(p.per_bell_region_cap.saturating_sub(*reg));
            *reg += x;
            total += x;
        }
        let dayc = per_day.entry((keeper, day)).or_default();
        total = total.min(p.per_keeper_day_cap.saturating_sub(*dayc));
        *dayc += total;
        let amount = r.pu64("amount");
        let partial = r.pu8("partial") != 0;
        if bad.is_some() || amount > total || (!partial && amount != total) {
            cx.fail(
                V,
                DEFENCE_REFUND_MISMATCH,
                what,
                r.bell,
                Some(r.tx),
                bad.unwrap_or_else(|| {
                    format!("claimed {amount} (partial {partial}), the formula gives {total}")
                }),
            );
        }
    }
}
