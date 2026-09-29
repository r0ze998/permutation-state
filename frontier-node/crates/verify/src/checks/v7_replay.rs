//! V7 — replay (contract §5.11, §8.5).
//!
//! - **Clashes:** every CLASH is re-run with `clash::resolve_clash` from the
//!   chain's own inputs — the Province as the last transaction before the
//!   resolve left it, the gathered ClashInputs, THE seed `S(bell, r)` — by
//!   [`ContractBuilder`] (§5.11 ResolveFromInputs); the outcome digest, the
//!   engagements and the packed fates must be the record's, the Province
//!   must be resolved through the bell afterwards, and each province's
//!   CLASH and SKIP records must advance `resolved_next` one bell at a time
//!   from its opening; the Province and ClashInputs the resolve wrote must
//!   be the outcome's write-back ([`writeback`], integ-W4 review).
//! - **Quiet skips:** no bell of a SKIP run has a REVEAL (an ArrivalSlot)
//!   before it (`SkipOverArrival`); the run is replayed bell by bell
//!   ([`crate::skip`], W5-D): the day's camp check, the kernel's
//!   `clash::is_quiet` at **every** bell (`SkipNotQuiet` names the first
//!   that is not), the bell's settle; the Province the skip wrote must be
//!   the replay's (`ClashReplayMismatch`), and the quiet digest is over it.
//! - **Transits:** each TRANSIT_SETTLED outcome follows §5.11 D5 from the
//!   logged seal code and the destination's gathered-and-resolved inputs:
//!   a bad seal code → bad-seal; the host in the records → its fate (and a
//!   Stays/Withdrew host's troops after the clash); not in the records →
//!   `bounced-unranked` if the faction's final four outrank it
//!   (`admit_arrival` refuses it) else `routed`; no resolved inputs →
//!   `routed`.
//! - **Holdings:** each holding a SETTLE founds is written with the SETTLE's
//!   owner, generation, score, cohort and `final_ts`; every Harvest, Build
//!   and Train is replayed through the kernel's lazy holding functions
//!   ([`crate::holding`], W5-D) and the Holding written, the record and a
//!   walls item must be the replay's (`HoldingReplayMismatch`).
//!
//! Codes: `ClashReplayMismatch`, `SkipNotQuiet`, `SkipOverArrival`,
//! `TransitOutcomeMismatch`, `HoldingReplayMismatch`.

use std::collections::{BTreeMap, HashMap};

use frontier_abi::layout::clash::clash_inputs as CI;
use frontier_abi::layout::header as HD;
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

/// The write-back of a replayed clash (§5.11 v1.6 §22, W4-A's pinned
/// rules; integ-W4 review, W4-D major: V7 compared only the digests, so a
/// Province or ClashInputs written back wrong passed). Against the
/// transaction's post-states: every resident the clash changed carries its
/// post-clash troops, stamina (at the bell) and tile, an unchanged one its
/// bytes of before; a destroyed resident is gone unless a departure, leave
/// or forfeit was pending; a bounced one leaves (`Leave`); every arrival
/// that stays or withdraws is a roster entry with its post-clash troops and
/// tile from the next bell; the ClashInputs fate table holds the fates and
/// troops; the camp is cleared exactly when it lost (below one troop, or
/// its hex held by hostile hosts), else keeps its whole troops.
pub fn writeback(
    built: &crate::clash_input::Built,
    o: &permutation_rules::frontier::clash::ClashOutcome,
    before: &[u8],
    after: &[u8],
    inputs_after: Option<&[u8]>,
) -> Vec<String> {
    use frontier_abi::entry::{find_entry, read_entry, EntryOp};
    use frontier_abi::layout::province::entry as E;
    use permutation_rules::frontier::host::STAMINA_CAP;
    let b = built.bell;
    let mut bad = vec![];
    let entry = |d: &[u8], id: u64| find_entry(d, id).and_then(|i| read_entry(d, i).ok());
    let tile_of = |f: &Fate| match f {
        Fate::Stays { tile } | Fate::Withdrew { tile } => Some(*tile),
        _ => None,
    };
    for f in &built.residents {
        let Some(fr) = o.fighter(f.id) else {
            bad.push(format!("resident {} missing from the outcome", f.id));
            continue;
        };
        let Some(pre) = entry(before, f.id) else {
            bad.push(format!("resident {} not in the Province before", f.id));
            continue;
        };
        let post = entry(after, f.id);
        let pending = !matches!(pre.op, EntryOp::None);
        match (&fr.fate, post) {
            (Fate::Destroyed, None) if !pending => {}
            (Fate::Destroyed, Some(_)) if !pending => {
                bad.push(format!("resident {} destroyed but still an entry", f.id))
            }
            (_, None) => {
                // Freed by the bell's settle: a forfeit (or a destroyed
                // host with a pending change) is gone afterwards.
                if !matches!(pre.op, EntryOp::Forfeit) && fr.fate != Fate::Destroyed {
                    bad.push(format!("resident {} vanished", f.id));
                }
            }
            (fate, Some(e)) => {
                let same = fr.troops == f.troops
                    && fr.stamina == f.stamina
                    && !fr.engaged
                    && tile_of(fate).is_none_or(|t| t == pre.tile);
                let (troops, stamina, tile) = if same {
                    (pre.troops, pre.stamina_value, pre.tile)
                } else {
                    (
                        fr.troops,
                        fr.stamina.min(STAMINA_CAP),
                        tile_of(fate).unwrap_or(pre.tile),
                    )
                };
                // A departure settled at this bell (`Spend` → state 3)
                // pays its march stamina afterwards: its stamina is
                // SettleDeparture's to judge (V8), not the clash's.
                let departed =
                    e.state == E::STATE_DEPARTED && matches!(pre.op, EntryOp::Spend { .. });
                if e.troops != troops || (!departed && e.stamina_value != stamina) || e.tile != tile
                {
                    bad.push(format!(
                        "resident {}: written ({}, {}, tile {}), the clash leaves ({troops}, {stamina}, tile {tile})",
                        f.id, e.troops, e.stamina_value, e.tile
                    ));
                }
                if !same && !departed && e.stamina_bell != b {
                    bad.push(format!(
                        "resident {}: stamina clock {} not the bell",
                        f.id, e.stamina_bell
                    ));
                }
                if matches!(fate, Fate::Bounced | Fate::Retreated)
                    && !pending
                    && !(e.state == E::STATE_DEPARTED && matches!(e.op, EntryOp::Leave))
                {
                    bad.push(format!("resident {} bounced but does not leave", f.id));
                }
            }
        }
    }
    for (a, &k) in built.arrivals.iter().zip(&built.arrival_pos) {
        let Some(fr) = o.fighter(a.id) else {
            bad.push(format!("arrival {} missing from the outcome", a.id));
            continue;
        };
        if let Some(t) = tile_of(&fr.fate) {
            match entry(after, a.id) {
                Some(e)
                    if e.state == E::STATE_ROSTER
                        && e.troops == fr.troops
                        && e.tile == t
                        && e.from_bell == b + 1 => {}
                e => bad.push(format!(
                    "arrival {} stays: entry {:?}, the clash leaves ({}, tile {t}) from bell {}",
                    a.id,
                    e.map(|e| (e.state, e.troops, e.tile, e.from_bell)),
                    fr.troops,
                    b + 1
                )),
            }
        }
        if let Some(ci) = inputs_after {
            use frontier_abi::layout::clash::arrival as AR;
            let o_ = CI::arrival(k);
            let (fate, ta) = (
                ci.get(o_ + AR::FATE).copied(),
                ci.get(o_ + AR::TROOPS_AFTER..o_ + AR::TROOPS_AFTER + 4)
                    .map(|x| u32::from_le_bytes(x.try_into().unwrap_or([0; 4]))),
            );
            if fate != Some(fate_code(&fr.fate)) || ta != Some(fr.troops) {
                bad.push(format!(
                    "inputs position {k}: fate {fate:?} troops {ta:?}, the clash gives {} {}",
                    fate_code(&fr.fate),
                    fr.troops
                ));
            }
        }
    }
    if let Some(g) = built
        .garrisons
        .iter()
        .find(|g| g.faction == permutation_rules::frontier::clash::NEUTRAL)
    {
        if let (Some(gr), Ok(pv)) = (
            o.garrisons.iter().find(|x| x.id == g.id),
            fclient::decode::Province::decode(after),
        ) {
            let whole = gr.troops / 1_000;
            let cleared = whole == 0 || gr.attackers_hold;
            let ok = if cleared {
                pv.camp.state == 0
            } else {
                pv.camp.state == 1 && pv.camp.troops == whole
            };
            if !ok {
                bad.push(format!(
                    "camp: written (state {}, {} troops), the clash leaves {}",
                    pv.camp.state,
                    pv.camp.troops,
                    if cleared {
                        "it cleared".to_string()
                    } else {
                        format!("{whole} troops")
                    }
                ));
            }
        }
    }
    bad
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
    // Every REVEAL of the run, first record per ArrivalSlot (integ-W6t: a
    // pre-pass, not only the reveals before the SKIP). The program refuses
    // a Reveal once the Province is resolved past its arrival bell
    // (§5.11 Reveal step 6, `LatchClosed`), so a skip over a bell whose
    // arrival is revealed at any point of the run is wrong, whichever of
    // the two records comes first.
    let mut revealed: BTreeMap<(i32, i32, u32), usize> = BTreeMap::new();
    for (n, r) in w.recs.iter().enumerate() {
        if r.kind == Kind::REVEAL {
            let (p, q) = r.pq();
            revealed.entry((p, q, r.ku32("arrive"))).or_insert(n);
        }
    }
    for (n, r) in w.recs.iter().enumerate() {
        match r.kind {
            Kind::PROVINCE_OPEN => {
                let b = if r.bell == plog::NO_BELL { 0 } else { r.bell };
                next.insert(r.pq(), b);
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
                // v1.6 §22: the CLASH input digest (integ-W4 review: pinned,
                // now judged).
                if crate::clash_input::input_digest(pv, ci, b, &seed) != r.p32("input_digest") {
                    cx.fail(
                        V,
                        CLASH_REPLAY_MISMATCH,
                        what.clone(),
                        b,
                        Some(r.tx),
                        "the input digest is not sha256(PSF-CLASH-INPUT-v1 ‖ bell ‖ seed ‖ Province state ‖ arrival records)",
                    );
                }
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
                        let tx = &w.txs[r.tx];
                        if let Some(after) = tx.post_data(&pk) {
                            let ci_after = tx.post_data(&f.ctx.clash_inputs(p, q, b));
                            let bad = writeback(&built, &o, pv, after, ci_after);
                            if !bad.is_empty() {
                                cx.fail(
                                    V,
                                    CLASH_REPLAY_MISMATCH,
                                    what.clone(),
                                    b,
                                    Some(r.tx),
                                    format!("write-back: {}", bad.join("; ")),
                                );
                            }
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
                        cx.fail(
                            V,
                            SKIP_OVER_ARRIVAL,
                            what.clone(),
                            b,
                            Some(r.tx),
                            if rv < n {
                                format!("bell {b} has a revealed arrival")
                            } else {
                                format!("bell {b} has an arrival revealed after the skip")
                            },
                        );
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
                if pv.len() != PV::SIZE {
                    cx.fail(
                        V,
                        CLASH_REPLAY_MISMATCH,
                        what,
                        b0,
                        Some(r.tx),
                        format!(
                            "the Province before the skip is {} B, the layout has {}",
                            pv.len(),
                            PV::SIZE
                        ),
                    );
                    continue;
                }
                // Wave-5 review: with post-states, a SKIP whose Province
                // has none cannot be checked (fail closed, not open).
                if w.has_post && w.txs[r.tx].post_data(&f.ctx.province(p, q)).is_none() {
                    cx.missing(
                        V,
                        what.clone(),
                        b0,
                        Some(r.tx),
                        "no post-state of the Province the skip wrote",
                    );
                }
                // v1.6 §22: SKIP's quiet digest over the Province it left.
                if let Some(after) = w.txs[r.tx].post_data(&f.ctx.province(p, q)) {
                    if crate::clash_input::quiet_digest(after, b0, k as u8) != r.p32("quiet_digest")
                    {
                        cx.fail(
                            V,
                            CLASH_REPLAY_MISMATCH,
                            what.clone(),
                            b0,
                            Some(r.tx),
                            "the quiet digest is not sha256(PSF-QUIET-v1 ‖ b0 ‖ n ‖ Province state after)",
                        );
                    }
                }
                // Every bell of the run replayed (W5-D; the integ-W4
                // review's "SKIP quiet at every bell"): the kernel's
                // `is_quiet` at each bell after its camp check, the bell's
                // settle, and the Province written = the replay's.
                match crate::skip::replay(pv, b0, k) {
                    Ok(pd) => {
                        if let Some(after) = w.txs[r.tx].post_data(&f.ctx.province(p, q)) {
                            let mask = |x: &[u8]| {
                                let mut v = x.to_vec();
                                if v.len() >= HD::H_SIZE {
                                    v[HD::EVENT_SEQ..HD::EVENT_HEAD + 32].fill(0);
                                }
                                v
                            };
                            let (ma, mp) = (mask(after), mask(&pd));
                            if ma != mp {
                                let at = ma
                                    .iter()
                                    .zip(mp.iter())
                                    .position(|(a, b)| a != b)
                                    .unwrap_or(ma.len().min(mp.len()));
                                cx.fail(
                                    V,
                                    CLASH_REPLAY_MISMATCH,
                                    what,
                                    b0,
                                    Some(r.tx),
                                    format!("skip write-back: the Province written differs from the bell-by-bell replay (first at byte {at})"),
                                );
                            }
                        }
                    }
                    Err((b, why)) => cx.fail(
                        V,
                        SKIP_NOT_QUIET,
                        what,
                        b,
                        Some(r.tx),
                        format!("bell {b} of the run is not quiet: {why}"),
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
    founded(cx);
    owner_actions(cx);
    continuity(cx);
}

/// `(transaction, (P, Q, site), bell, detail)` of a continuity failure.
type ContinuityFinding = (usize, (i32, i32, u8), u32, String);

/// Holding continuity (wave-5 review of W5-D): every Holding write that is
/// not a replayed owner action (Harvest, Build, Train: [`owner_actions`])
/// must leave the lazy-accrual bytes ([`crate::holding::ACCRUAL_RANGES`])
/// exactly as the rules allow — a founding SETTLE writes the founded
/// Hamlet ([`crate::holding::founded`]); an owner touch (Explore, Muster,
/// Dissolve, Garrison, Depart) writes the touch at its Clock
/// ([`crate::holding::touched`]); every other write (SettleExplore,
/// SettleTransit, SettleDeparture, GatherClash, SweepPoolOwed, …) leaves
/// them byte-identical. With the owner actions replayed from the previous
/// post-state, the Holding is carried forward from its founding through
/// every write (`HoldingReplayMismatch`).
fn continuity(cx: &mut Ctx) {
    use crate::holding as hr;
    use frontier_abi::tags::Ix;
    let w = cx.w;
    let f = cx.f;
    if !w.has_post {
        return;
    }
    let owner = [Ix::Harvest.tag(), Ix::Build.tag(), Ix::Train.tag()];
    let touch = [
        Ix::Explore.tag(),
        Ix::Muster.tag(),
        Ix::Dissolve.tag(),
        Ix::Garrison.tag(),
        Ix::Depart.tag(),
    ];
    let mut bad: Vec<ContinuityFinding> = vec![];
    for (t, tx) in w.txs.iter().enumerate() {
        if !tx.ok {
            continue;
        }
        for (k, a) in &tx.post {
            let Some(a) = a else { continue };
            let post = &a.data;
            if post.len() != H::SIZE || post[..8] != H::MAGIC {
                continue;
            }
            let writes = |tags: &[u8]| {
                tx.ixs
                    .iter()
                    .any(|i| i.tag().is_some_and(|x| tags.contains(&x)) && i.writes(k))
            };
            if writes(&owner) {
                continue;
            }
            let pqs = (
                le(&post[H::P..H::P + 2]) as u16 as i16 as i32,
                le(&post[H::Q..H::Q + 2]) as u16 as i16 as i32,
                post[H::SITE],
            );
            let bell = frontier_abi::prologue::bell_at(f.genesis_ts, tx.time).unwrap_or(0);
            let founding = tx.recs.iter().any(|&n| {
                let r = &w.recs[n];
                r.kind == Kind::SETTLE
                    && r.pqs() == pqs
                    && matches!(
                        r.pu8("outcome"),
                        settle_outcome::FRESH | settle_outcome::DISPLACE
                    )
            });
            let (what, want) = if founding {
                (
                    "the founding SETTLE",
                    hr::founded(post, tx.time, frontier_abi::addr::day_of(bell)),
                )
            } else {
                let Some(before) = w.state_before(k, t) else {
                    continue;
                };
                if before.len() != H::SIZE || before[..8] != H::MAGIC {
                    continue;
                }
                if writes(&touch) {
                    ("an owner touch", hr::touched(before, tx.time))
                } else {
                    ("a write that is not an owner action", Ok(before.to_vec()))
                }
            };
            match want {
                Ok(d) => {
                    if let Some(o) = hr::accrual_diff(post, &d) {
                        bad.push((
                            t,
                            pqs,
                            bell,
                            format!(
                                "{what} left the Holding's accrual state other than the rules do (first at byte {o})"
                            ),
                        ));
                    }
                }
                Err(e) => bad.push((t, pqs, bell, format!("{what}: the kernel refuses: {e}"))),
            }
        }
    }
    for (t, (p, q, s), bell, detail) in bad {
        cx.fail(
            V,
            HOLDING_REPLAY_MISMATCH,
            format!("holding ({p}, {q}, {s})"),
            bell,
            Some(t),
            detail,
        );
    }
}

/// Every Harvest, Build and Train replayed from the Holding the last
/// transaction before it left ([`crate::holding`], W5-D): the Holding the
/// transaction wrote (every byte but the event chain's header fields), the
/// record's payload (HARVEST's stores digest, BUILD's item, cost digest
/// and completion, TRAIN's unit, count and time) and a walls Build's item
/// in the site mirror must be the replay's. A transaction in which another
/// program instruction also *writes* the Holding (a declared writable
/// position, not a read) is not replayed and reported `MissingData` (none
/// in M1's clients; wave-5 review: no silent skip).
fn owner_actions(cx: &mut Ctx) {
    use crate::holding::{self as hr, Action};
    use frontier_abi::ix as aix;
    use frontier_abi::layout::header as HD;
    use frontier_abi::layout::province::site as SM;
    use frontier_abi::tags::Ix;
    let w = cx.w;
    let f = cx.f;
    let tags = [Ix::Harvest.tag(), Ix::Build.tag(), Ix::Train.tag()];
    for (t, tx) in w.txs.iter().enumerate() {
        if !tx.ok {
            continue;
        }
        let recs: Vec<usize> = tx
            .recs
            .iter()
            .copied()
            .filter(|&n| matches!(w.recs[n].kind, Kind::HARVEST | Kind::BUILD | Kind::TRAIN))
            .collect();
        if recs.is_empty() {
            continue;
        }
        let ixs: Vec<&crate::world::Ix> = tx
            .ixs
            .iter()
            .filter(|i| i.tag().is_some_and(|x| tags.contains(&x)))
            .collect();
        let first = &w.recs[recs[0]];
        let what0 = {
            let (p, q, s) = first.pqs();
            format!("holding ({p}, {q}, {s})")
        };
        if !w.has_post {
            cx.missing(
                V,
                what0,
                first.bell,
                Some(t),
                "no post-states: the Holding cannot be replayed",
            );
            continue;
        }
        if ixs.len() != recs.len() {
            cx.fail(
                V,
                HOLDING_REPLAY_MISMATCH,
                what0,
                first.bell,
                Some(t),
                format!(
                    "{} owner-action records for {} Harvest/Build/Train instructions",
                    recs.len(),
                    ixs.len()
                ),
            );
            continue;
        }
        let mut hstate: BTreeMap<[u8; 32], Vec<u8>> = BTreeMap::new();
        let mut pstate: BTreeMap<[u8; 32], (Vec<u8>, usize)> = BTreeMap::new();
        let mut skip: std::collections::BTreeSet<[u8; 32]> = Default::default();
        let mut not_replayed: Vec<(usize, (i32, i32, u8))> = vec![];
        let mut bad: Vec<(usize, String)> = vec![];
        for (&n, ix) in recs.iter().zip(&ixs) {
            let r = &w.recs[n];
            let (p, q, site) = r.pqs();
            let hk = f.ctx.holding(p, q, site);
            if ix.key(4) != Some(hk) {
                bad.push((
                    n,
                    "the record names another Holding than its instruction".into(),
                ));
                continue;
            }
            // Another program instruction of the transaction writes it
            // (declared writable there, not only listed: wave-5 review).
            if tx
                .ixs
                .iter()
                .any(|i| !i.tag().is_some_and(|x| tags.contains(&x)) && i.writes(&hk))
            {
                skip.insert(hk);
                not_replayed.push((n, (p, q, site)));
                continue;
            }
            let action = match ix.tag() {
                Some(x) if x == Ix::Harvest.tag() => Some(Action::Harvest),
                Some(x) if x == Ix::Build.tag() => aix::Build::decode(&ix.data)
                    .ok()
                    .map(|b| Action::Build { item: b.item }),
                Some(x) if x == Ix::Train.tag() => {
                    aix::Train::decode(&ix.data).ok().map(|b| Action::Train {
                        unit: b.unit,
                        n: b.n,
                    })
                }
                _ => None,
            };
            let Some(action) = action else {
                bad.push((n, "the instruction data does not decode".into()));
                continue;
            };
            let want_kind = match action {
                Action::Harvest => Kind::HARVEST,
                Action::Build { .. } => Kind::BUILD,
                Action::Train { .. } => Kind::TRAIN,
            };
            if r.kind != want_kind {
                bad.push((n, format!("{} logged for a {:?}", r.kind.name(), action)));
                continue;
            }
            let before = match hstate.get(&hk) {
                Some(d) => d.clone(),
                None => match w.state_before(&hk, t) {
                    Some(d) => d.to_vec(),
                    None => {
                        cx.missing(
                            V,
                            format!("holding ({p}, {q}, {site})"),
                            r.bell,
                            Some(t),
                            "no state of the Holding before its owner action",
                        );
                        skip.insert(hk);
                        continue;
                    }
                },
            };
            match hr::replay(&before, &action, tx.time, f.genesis_ts) {
                Ok(x) => {
                    if x.payload[..] != r.payload[..] {
                        bad.push((
                            n,
                            format!(
                                "{} payload {}, the replay logs {}",
                                r.kind.name(),
                                hex::encode(&r.payload),
                                hex::encode(&x.payload)
                            ),
                        ));
                    }
                    if let Some(item) = x.wall_item {
                        let pk = f.ctx.province(p, q);
                        let cur = match pstate.get(&pk) {
                            Some((d, _)) => Some(d.clone()),
                            None => w.state_before(&pk, t).map(|d| d.to_vec()),
                        };
                        match cur.and_then(|d| hr::wall_item(&d, site as usize, item)) {
                            Some(d) => {
                                pstate.insert(pk, (d, site as usize));
                            }
                            None => bad.push((
                                n,
                                "a walls Build the site mirror cannot take (both items busy or no Province)"
                                    .into(),
                            )),
                        }
                    }
                    hstate.insert(hk, x.holding);
                }
                Err(e) => bad.push((n, format!("the kernel refuses the replay: {e}"))),
            }
        }
        for (hk, d) in &hstate {
            if skip.contains(hk) {
                continue;
            }
            let Some(post) = tx.post_data(hk) else {
                bad.push((recs[0], "the Holding has no post-state".into()));
                continue;
            };
            let mask = |x: &[u8]| {
                let mut v = x.to_vec();
                if v.len() >= HD::H_SIZE {
                    v[HD::EVENT_SEQ..HD::EVENT_HEAD + 32].fill(0);
                }
                v
            };
            let (mp, md) = (mask(post), mask(d));
            if mp != md {
                let diff = mp
                    .iter()
                    .zip(md.iter())
                    .position(|(a, b)| a != b)
                    .unwrap_or(mp.len().min(md.len()));
                let n = recs
                    .iter()
                    .copied()
                    .find(|&n| {
                        let (p, q, s) = w.recs[n].pqs();
                        f.ctx.holding(p, q, s) == *hk
                    })
                    .unwrap_or(recs[0]);
                bad.push((
                    n,
                    format!("the Holding written differs from the replay (first at byte {diff})"),
                ));
            }
        }
        for (pk, (d, site)) in &pstate {
            let o = frontier_abi::layout::province::province::site(*site);
            let got = tx.post_data(pk).and_then(|x| x.get(o..o + SM::SIZE));
            if got != d.get(o..o + SM::SIZE) {
                bad.push((
                    recs[0],
                    "the walls item in the site mirror differs from the replay".into(),
                ));
            }
        }
        for (n, (p, q, s)) in not_replayed {
            cx.missing(
                V,
                format!("holding ({p}, {q}, {s})"),
                w.recs[n].bell,
                Some(t),
                "another instruction of the transaction also writes the Holding: the owner action is not replayed",
            );
        }
        for (n, detail) in bad {
            let r = &w.recs[n];
            let (p, q, s) = r.pqs();
            cx.fail(
                V,
                HOLDING_REPLAY_MISMATCH,
                format!("holding ({p}, {q}, {s})"),
                r.bell,
                Some(t),
                detail,
            );
        }
    }
}

/// Each Holding a SETTLE founds is written with the SETTLE's owner,
/// generation, score, cohort and `final_ts`.
fn founded(cx: &mut Ctx) {
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
