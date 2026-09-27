//! V7 — replay (contract §5.11, §8.5).
//!
//! - **Clashes:** every CLASH is re-run with `clash::resolve_clash` from the
//!   chain's own inputs — the Province as the last transaction before the
//!   resolve left it, the gathered ClashInputs, THE seed `S(bell, r)` — by
//!   [`ContractBuilder`] (§5.11 ResolveFromInputs); the outcome digest, the
//!   engagements and the packed fates must be the record's, the Province
//!   must be resolved through the bell afterwards, and each province's
//!   CLASH and SKIP records must advance `resolved_next` one bell at a time
//!   from its opening.
//! - **Quiet skips:** no bell of a SKIP run has a REVEAL (an ArrivalSlot)
//!   before it (`SkipOverArrival`), and the Province before the run is
//!   quiet by `clash::is_quiet` (checked once per run when no entry has a
//!   pending change; a run whose roster changes inside it is checked at
//!   its first bell only).
//! - **Transits:** each TRANSIT_SETTLED outcome follows §5.11 D5 from the
//!   logged seal code and the destination's gathered-and-resolved inputs:
//!   a bad seal code → bad-seal; the host in the records → its fate (and a
//!   Stays/Withdrew host's troops after the clash); not in the records →
//!   `bounced-unranked` if the faction's final four outrank it
//!   (`admit_arrival` refuses it) else `routed`; no resolved inputs →
//!   `routed`.
//! - **Holdings:** each holding a SETTLE founds is written with the SETTLE's
//!   owner, generation, score, cohort and `final_ts` (the lazy accrual
//!   replay of Harvest/Build/Train is deferred to W5, W4-D notes).
//!
//! Codes: `ClashReplayMismatch`, `SkipNotQuiet`, `SkipOverArrival`,
//! `TransitOutcomeMismatch`, `HoldingReplayMismatch`.

use std::collections::{BTreeMap, HashMap};

use frontier_abi::layout::clash::clash_inputs as CI;
use frontier_abi::layout::player::holding as H;
use frontier_abi::layout::province::province as PV;
use frontier_abi::log::{self as plog, settle_outcome, transit_outcome, Kind};
use permutation_rules::frontier::clash::{
    admit_arrival, FactionSlots, Fate, SlotDecision, SlotEntry,
};
use permutation_rules::frontier::geometry::ProvinceCoord;

use super::v6_quotas::tag_of_host;
use super::Ctx;
use crate::clash_input::{ClashBuilder, ContractBuilder};
use crate::codes::*;
use crate::facts::Facts;
use crate::world::le;

const V: &str = "V7";

/// ClashInputs fate code of a kernel fate (1–5, the transit outcomes).
pub fn fate_code(f: &Fate) -> u8 {
    match f {
        Fate::Stays { .. } => transit_outcome::STAYS,
        Fate::Withdrew { .. } => transit_outcome::WITHDREW,
        Fate::Bounced => transit_outcome::BOUNCED,
        Fate::Retreated => transit_outcome::RETREATED,
        Fate::Destroyed => transit_outcome::DESTROYED,
    }
}

/// The packed fates of an outcome at the arrivals' ClashInputs positions.
pub fn packed_fates(
    b: &crate::clash_input::Built,
    o: &permutation_rules::frontier::clash::ClashOutcome,
) -> [u8; 9] {
    let mut fates = [0u8; 24];
    for (a, &pos) in b.arrivals.iter().zip(&b.arrival_pos) {
        if let Some(fr) = o.fighter(a.id) {
            fates[pos] = fate_code(&fr.fate);
        }
    }
    plog::pack_fates(&fates)
}

pub fn run(cx: &mut Ctx) {
    clashes_and_skips(cx);
    transits(cx);
    holdings(cx);
}

fn clashes_and_skips(cx: &mut Ctx) {
    let w = cx.w;
    let f = cx.f;
    let builder = ContractBuilder;
    let mut next: HashMap<(i32, i32), u32> = HashMap::new();
    let mut revealed: BTreeMap<(i32, i32, u32), usize> = BTreeMap::new();
    for (n, r) in w.recs.iter().enumerate() {
        match r.kind {
            Kind::PROVINCE_OPEN => {
                let b = if r.bell == plog::NO_BELL { 0 } else { r.bell };
                next.insert(r.pq(), b);
            }
            Kind::REVEAL => {
                let (p, q) = r.pq();
                revealed.entry((p, q, r.ku32("arrive"))).or_insert(n);
            }
            Kind::CLASH => {
                let (p, q) = r.pq();
                let b = r.ku32("bell");
                let what = format!("clash ({p}, {q}) bell {b}");
                let want = next.get(&(p, q)).copied();
                if want != Some(b) {
                    cx.fail(
                        V,
                        CLASH_REPLAY_MISMATCH,
                        what.clone(),
                        b,
                        Some(r.tx),
                        format!("resolved out of order (next was {want:?})"),
                    );
                }
                next.insert((p, q), b + 1);
                let pk = f.ctx.province(p, q);
                let (Some(pv), Some(ci)) = (
                    w.state_before(&pk, r.tx),
                    w.state_before(&f.ctx.clash_inputs(p, q, b), r.tx),
                ) else {
                    cx.missing(V, what, b, Some(r.tx), "no post-states of the Province or the ClashInputs (an archive with post-states is needed)");
                    continue;
                };
                let Some(seed) = f.bell_seed(b, Facts::region(p, q)) else {
                    cx.missing(V, what, b, Some(r.tx), "no seed of the bell");
                    continue;
                };
                let built = match builder.build(pv, ci, b, &seed) {
                    Ok(x) => x,
                    Err(e) => {
                        cx.fail(
                            V,
                            CLASH_REPLAY_MISMATCH,
                            what,
                            b,
                            Some(r.tx),
                            format!("the logged inputs do not build: {e}"),
                        );
                        continue;
                    }
                };
                match built.resolve() {
                    Ok(o) => {
                        let fates = packed_fates(&built, &o);
                        if o.digest() != r.p32("outcome_digest")
                            || o.engagements != r.pu32("engagements")
                            || fates[..] != *r.p("fates")
                        {
                            cx.fail(
                                V,
                                CLASH_REPLAY_MISMATCH,
                                what.clone(),
                                b,
                                Some(r.tx),
                                format!("replayed digest {} ({} engagements) differs from the logged one", hex::encode(&o.digest()[..8]), o.engagements),
                            );
                        }
                        let factions: std::collections::BTreeSet<u8> = built
                            .residents
                            .iter()
                            .map(|x| x.faction)
                            .chain(built.arrivals.iter().map(|x| x.faction))
                            .collect();
                        if factions.len() > 1 && !built.arrivals.is_empty() {
                            cx.liveness.contested_bells.push((p, q, b));
                        }
                    }
                    Err(e) => cx.fail(
                        V,
                        CLASH_REPLAY_MISMATCH,
                        what.clone(),
                        b,
                        Some(r.tx),
                        format!("the kernel refuses the logged inputs: {e}"),
                    ),
                }
                if let Some(after) = w.txs[r.tx].post_data(&pk) {
                    if le(&after[PV::RESOLVED_NEXT..PV::RESOLVED_NEXT + 4]) as u32 != b + 1 {
                        cx.fail(
                            V,
                            CLASH_REPLAY_MISMATCH,
                            what,
                            b,
                            Some(r.tx),
                            "the Province is not resolved through the bell after its CLASH",
                        );
                    }
                }
            }
            Kind::SKIP => {
                let (p, q) = r.pq();
                let (b0, k) = (r.pu32("b0"), r.pu8("n") as u32);
                let what = format!("skip ({p}, {q}) bells {b0}..{}", b0 + k);
                let want = next.get(&(p, q)).copied();
                if want != Some(b0) || k == 0 {
                    cx.fail(
                        V,
                        CLASH_REPLAY_MISMATCH,
                        what.clone(),
                        b0,
                        Some(r.tx),
                        format!("skip out of order (next was {want:?})"),
                    );
                }
                next.insert((p, q), b0 + k);
                for b in b0..b0 + k {
                    if let Some(&rv) = revealed.get(&(p, q, b)) {
                        if rv < n {
                            cx.fail(
                                V,
                                SKIP_OVER_ARRIVAL,
                                what.clone(),
                                b,
                                Some(r.tx),
                                format!("bell {b} has a revealed arrival"),
                            );
                        }
                    }
                }
                let Some(pv) = w.state_before(&f.ctx.province(p, q), r.tx) else {
                    cx.missing(
                        V,
                        what,
                        b0,
                        Some(r.tx),
                        "no post-state of the Province before the skip",
                    );
                    continue;
                };
                match builder.build(pv, &[], b0, &[0; 32]).and_then(|x| x.quiet()) {
                    Ok(true) => {}
                    Ok(false) => cx.fail(
                        V,
                        SKIP_NOT_QUIET,
                        what,
                        b0,
                        Some(r.tx),
                        "the province is not quiet at the run's first bell",
                    ),
                    Err(e) => cx.fail(
                        V,
                        SKIP_NOT_QUIET,
                        what,
                        b0,
                        Some(r.tx),
                        format!("the province does not build: {e}"),
                    ),
                }
            }
            _ => {}
        }
    }
}

/// The destination `(P, Q)` and ClashInputs key of a SettleTransit.
/// `(destination, ClashInputs key, slot key)` of a SettleTransit.
type SettleDest = ((i32, i32), [u8; 32], [u8; 32]);

fn settle_dest(cx: &Ctx, tx: usize) -> Option<SettleDest> {
    let ix = cx.w.txs[tx].ix(frontier_abi::tags::Ix::SettleTransit.tag())?;
    let dest = *cx.f.provinces.get(&ix.key(3)?)?;
    Some((dest, ix.key(4)?, ix.key(5)?))
}

fn transits(cx: &mut Ctx) {
    let w = cx.w;
    let f = cx.f;
    for (n, r) in w
        .recs
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == Kind::TRANSIT_SETTLED)
    {
        if !w.has_post && r.pu8("seal_code") == 0 {
            cx.missing(
                V,
                format!("transit of host {}", r.ku64("host_id")),
                r.bell,
                Some(r.tx),
                "no post-states: the destination's inputs cannot be read",
            );
            continue;
        }
        let host = r.ku64("host_id");
        let what = format!("transit of host {host}");
        let (outcome, code) = (r.pu8("outcome"), r.pu8("seal_code"));
        let Some(d) = f.depart_of(w, host, n, None) else {
            cx.fail(
                V,
                TRANSIT_OUTCOME_MISMATCH,
                what,
                r.bell,
                Some(r.tx),
                "no DEPART",
            );
            continue;
        };
        let arrive = w.recs[d].pu32("arrive_bell");
        let expected: Option<(u8, Option<u32>)> = if code > 0 {
            Some((transit_outcome::BAD_SEAL, Some(0)))
        } else {
            let Some(((p, q), ck, _)) = settle_dest(cx, r.tx) else {
                cx.missing(
                    V,
                    what,
                    r.bell,
                    Some(r.tx),
                    "the SettleTransit's accounts are not in the archive",
                );
                continue;
            };
            let inputs = w
                .state_before(&ck, r.tx)
                .and_then(|x| fclient::decode::ClashInputs::decode(x).ok())
                .filter(|c| c.flags & CI::FLAG_RESOLVED != 0 && c.bell == arrive);
            match inputs {
                None => Some((transit_outcome::ROUTED, None)),
                Some(ci) => match ci
                    .arrivals
                    .iter()
                    .find(|a| a.present == 1 && a.host_id == host)
                {
                    Some(a) => {
                        let tr =
                            matches!(a.fate, transit_outcome::STAYS | transit_outcome::WITHDREW)
                                .then_some(a.troops_after);
                        Some((a.fate, tr))
                    }
                    None => {
                        let (tag, mass) = (tag_of_host(cx, host, n), w.recs[d].pu32("dep_mass"));
                        let faction = ci
                            .arrivals
                            .iter()
                            .find(|a| a.host_id == host)
                            .map(|a| a.faction)
                            .or_else(|| {
                                f.owner_of_host(host, n)
                                    .and_then(|o| o.owner)
                                    .and_then(|c| f.citizens.get(&c))
                                    .map(|c| c.faction)
                            });
                        match (tag, faction) {
                            (Some(tag), Some(fct)) => {
                                let mut s: FactionSlots = [None; 4];
                                for (i, x) in s.iter_mut().enumerate() {
                                    let a = &ci.arrivals[fct as usize * 4 + i];
                                    if a.present == 1 {
                                        *x = Some(SlotEntry {
                                            host_id: a.host_id,
                                            citizen: a.citizen_tag,
                                            troops: a.dep_mass,
                                        });
                                    }
                                }
                                let e = SlotEntry {
                                    host_id: host,
                                    citizen: tag,
                                    troops: mass,
                                };
                                match admit_arrival(ProvinceCoord::new(p, q), arrive, &s, e) {
                                    SlotDecision::Refuse(_) => {
                                        Some((transit_outcome::BOUNCED_UNRANKED, None))
                                    }
                                    _ => Some((transit_outcome::ROUTED, None)),
                                }
                            }
                            _ => None,
                        }
                    }
                },
            }
        };
        let Some((want, troops)) = expected else {
            cx.missing(
                V,
                what,
                r.bell,
                Some(r.tx),
                "the host's owner or faction is unknown",
            );
            continue;
        };
        if outcome != want || troops.is_some_and(|t| t != r.pu32("troops")) {
            cx.fail(
                V,
                TRANSIT_OUTCOME_MISMATCH,
                what,
                arrive,
                Some(r.tx),
                format!(
                    "outcome {outcome} troops {}, the rule gives {want} troops {troops:?}",
                    r.pu32("troops")
                ),
            );
        }
    }
}

fn holdings(cx: &mut Ctx) {
    let w = cx.w;
    let f = cx.f;
    for r in w.of(Kind::SETTLE) {
        let o = r.pu8("outcome");
        if o != settle_outcome::FRESH && o != settle_outcome::DISPLACE {
            continue;
        }
        let (p, q, site) = r.pqs();
        let Some(h) = w.txs[r.tx].post_data(&f.ctx.holding(p, q, site)) else {
            continue;
        };
        let owner = f.by_tag8.get(&r.pu64("citizen_tag")).copied();
        let ok = owner
            .is_some_and(|c| h.get(H::OWNER_CITIZEN..H::OWNER_CITIZEN + 32) == Some(&c[..]))
            && h.get(H::GEN).copied() == Some(r.pu8("gen"))
            && le(&h[H::TICKET_SCORE..H::TICKET_SCORE + 8]) == r.pu64("score")
            && le(&h[H::TICKET_BELL..H::TICKET_BELL + 4]) as u32 == r.pu32("ticket_bell")
            && le(&h[H::FINAL_TS..H::FINAL_TS + 8]) as i64 == r.pi64("final_ts");
        if !ok {
            cx.fail(
                V,
                HOLDING_REPLAY_MISMATCH,
                format!("holding ({p}, {q}, {site})"),
                r.bell,
                Some(r.tx),
                "the founded Holding is not the SETTLE's (owner, gen, score, cohort, final_ts)",
            );
        }
    }
}
