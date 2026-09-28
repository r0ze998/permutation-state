//! V11 — land (contract §5.9, §8.5; I-30, I-47, I-56, W3-A notes §4).
//!
//! - **Scores:** every SETTLE (fresh, displace, taken) recomputes `score =
//!   rng(S(ticket_bell, r_site), "site", P‖Q‖site‖citizen_tag)` with S the
//!   seed THE anchor's cache (or archive) gives; `final_ts = round_time(S) +
//!   600`.
//! - **Displacement** only inside the cohort (same `ticket_bell`) of the
//!   site's current holder, by a better ticket (`beats`), naming the holder;
//!   a `taken` of the holder's own cohort must not beat it; a fresh
//!   settlement needs a free site; `expired` only from `ticket_bell + 24`.
//! - **Cohort counters:** every Province post-state's cohort records equal
//!   the tallies of TICKET (filed, per distinct province) and of the
//!   SETTLEs that end a ticket (settled, in every province of the ticket).
//! - **Terrain:** PROVINCE_OPEN's digest = `sha256("PSF-TERRAIN-v1" ‖
//!   terrain block)` of `terrain::generate_province(ring_seed, P)` in the
//!   program's pinned encoding, with its ring, wedge, region and site count.
//! - **Camps:** the initial camp = `camp::place(ring seed, …, initial)`;
//!   every CAMP spawn = the program's day check (`camp_seed` from the
//!   Province's terrain block, v1.6 §22), a clear (`troops = 0`) a present
//!   camp gone.
//!
//! Codes: `TicketScoreMismatch`, `DisplacementRule`, `CohortMismatch`,
//! `TerrainMismatch`, `CampMismatch`.

use std::collections::{BTreeMap, BTreeSet};

use frontier_abi::ix as aix;
use frontier_abi::layout::province::province as PV;
use frontier_abi::log::{settle_outcome, Kind, NO_BELL};
use permutation_rules::fixed::BPS_ONE;
use permutation_rules::frontier::camp;
use permutation_rules::frontier::geometry::{region_of, ProvinceCoord, PROVINCE_TILES};
use permutation_rules::frontier::terrain::{generate_province, ProvinceTerrain};
use permutation_rules::hash::sha256;

use super::Ctx;
use crate::codes::*;
use crate::world::le;

const V: &str = "V11";
/// Bells a ticket cohort stays open (I-47).
pub const COHORT_BELLS: u32 = 24;
pub const TERRAIN_DOMAIN: &[u8] = b"PSF-TERRAIN-v1";
/// The Concord's stored wedge.
pub const NO_WEDGE: u8 = 6;

/// The program's pinned Province encoding of a terrain (W3-A notes §4,
/// `proc/map.rs::encode_terrain`) written into a Province-sized buffer.
pub fn encode_terrain(t: &ProvinceTerrain, d: &mut [u8]) {
    let mut passable = 0u64;
    let mut rough = 0u64;
    for i in 0..PROVINCE_TILES {
        let tr = t.terrain[i];
        d[PV::TERRAIN + i] = tr as u8;
        d[PV::RESOURCE + i] = match t.resource[i] {
            None => 0,
            Some(r) => 1 + r as u8,
        };
        if tr.is_passable() {
            passable |= 1 << i;
        }
        if tr.info().defense_bps < BPS_ONE {
            rough |= 1 << i;
        }
    }
    d[PV::SITES..PV::SITES + 12].copy_from_slice(&t.sites);
    d[PV::SITE_COUNT] = t.site_count;
    d[PV::PASSABLE_MASK..PV::PASSABLE_MASK + 8].copy_from_slice(&passable.to_le_bytes());
    d[PV::ROUGH_MASK..PV::ROUGH_MASK + 8].copy_from_slice(&rough.to_le_bytes());
    d[PV::ROAD_MASK..PV::ROAD_MASK + 8].copy_from_slice(&0u64.to_le_bytes());
    d[PV::EXPLORED_MASK..PV::EXPLORED_MASK + 8].copy_from_slice(&0u64.to_le_bytes());
}

/// PROVINCE_OPEN's terrain digest of a generated terrain.
pub fn terrain_digest(t: &ProvinceTerrain) -> [u8; 32] {
    let mut d = vec![0u8; PV::SIZE];
    encode_terrain(t, &mut d);
    sha256(&[TERRAIN_DOMAIN, &d[PV::TERRAIN..PV::SITE_MIRROR]])
}

/// The ticket of a citizen: `(TICKET record, distinct provinces, n sites)`.
struct Ticket {
    bell: u32,
    provinces: Vec<(i32, i32)>,
    n: u8,
}

fn ticket_of(r: &crate::world::Rec) -> Ticket {
    let n = r.pu8("n");
    let sites = r.p("sites");
    let mut provinces = vec![];
    for i in 0..(n as usize).min(3) {
        let p = le(&sites[5 * i..5 * i + 2]) as u16 as i16 as i32;
        let q = le(&sites[5 * i + 2..5 * i + 4]) as u16 as i16 as i32;
        if !provinces.contains(&(p, q)) {
            provinces.push((p, q));
        }
    }
    Ticket {
        bell: r.pu32("ticket_bell"),
        provinces,
        n,
    }
}

pub fn run(cx: &mut Ctx) {
    let w = cx.w;
    let f = cx.f;
    // Terrain and initial camps.
    let mut terrains: BTreeMap<(i32, i32), ProvinceTerrain> = BTreeMap::new();
    for r in w.of(Kind::PROVINCE_OPEN) {
        let (p, q) = r.pq();
        let c = ProvinceCoord::new(p, q);
        let what = format!("province ({p}, {q})");
        let ring = c.ring() as u16;
        let Some(seed) = f.ring_seeds.get(&ring) else {
            cx.missing(
                V,
                what,
                r.bell,
                Some(r.tx),
                format!("no seed of ring {ring}"),
            );
            continue;
        };
        let t = generate_province(seed, c);
        if r.p32("terrain_digest") != terrain_digest(&t)
            || r.pu8("site_count") != t.site_count
            || r.pu16("ring") != ring
            || r.pu8("wedge") != c.wedge().unwrap_or(NO_WEDGE)
            || r.pu8("region") != region_of(c)
        {
            cx.fail(
                V,
                TERRAIN_MISMATCH,
                what.clone(),
                r.bell,
                Some(r.tx),
                "PROVINCE_OPEN is not generate_province(ring_seed, P) in the pinned encoding",
            );
        }
        let day = if r.bell == NO_BELL { 0 } else { r.bell / 144 };
        let cmp = camp::place(seed, c, &t, day, false, true);
        let want = cmp.map_or((0xFF, 0), |x| (x.tile, x.troops));
        if (r.pu8("camp_tile"), r.pu32("camp_troops")) != want {
            cx.fail(
                V,
                CAMP_MISMATCH,
                what,
                r.bell,
                Some(r.tx),
                format!(
                    "initial camp {:?}, camp::place gives {want:?}",
                    (r.pu8("camp_tile"), r.pu32("camp_troops"))
                ),
            );
        }
        terrains.insert((p, q), t);
    }
    // CAMP records (v1.6 §22, W4-A D3): a spawn of the day's check at the
    // first resolve or skip of a day, drawn from `sha256("PSF-CAMP-v1" ‖
    // province[TERRAIN ..= SITE_COUNT])` with `has_holding` of the Province
    // then (the program's `camp_check`, `fclient::clash_model::camp_at`);
    // or a clear (`troops = 0`: the camp present before, gone after).
    // Integ-W4 review: V11 read every CAMP as a ring-seed spawn.
    for r in w.recs.iter().filter(|r| r.kind == Kind::CAMP) {
        let (p, q) = r.pq();
        let what = format!("camp ({p}, {q})");
        let pk = f.ctx.province(p, q);
        let Some(before) = w.state_before(&pk, r.tx) else {
            cx.missing(
                V,
                what,
                r.bell,
                Some(r.tx),
                "no Province state before the camp's record",
            );
            continue;
        };
        let got = (r.pu8("tile"), r.pu32("troops"));
        let camp_state = |d: &[u8]| {
            fclient::decode::Province::decode(d)
                .ok()
                .map(|pv| pv.camp.state)
        };
        if got.1 == 0 {
            let after = w.txs[r.tx].post_data(&pk).and_then(camp_state);
            if camp_state(before) != Some(1) || after != Some(0) {
                cx.fail(
                    V,
                    CAMP_MISMATCH,
                    what,
                    r.bell,
                    Some(r.tx),
                    format!(
                        "a clear (troops 0) of a camp in state {:?} → {after:?}",
                        camp_state(before)
                    ),
                );
            }
            continue;
        }
        let want = fclient::decode::Province::decode(before)
            .map_err(|e| format!("{e:?}"))
            .and_then(|pv| {
                let t = fclient::clash_model::terrain_of(&pv)?;
                fclient::clash_model::camp_at(&pv, before, &t, r.pu32("day") * 144)
            })
            .map(|c| c.spawned.then_some((c.tile, c.troops)));
        let want = match want {
            Ok(x) => x,
            Err(e) => {
                cx.missing(V, what, r.bell, Some(r.tx), e);
                continue;
            }
        };
        if want != Some(got) {
            cx.fail(
                V,
                CAMP_MISMATCH,
                what,
                r.bell,
                Some(r.tx),
                format!(
                    "camp {got:?} on day {}, camp::place gives {want:?}",
                    r.pu32("day")
                ),
            );
        }
    }
    if !w.has_post && w.of(Kind::TICKET).next().is_some() {
        cx.missing(
            V,
            "cohorts",
            0,
            None,
            "no post-states: the Provinces' cohort counters cannot be compared",
        );
    }
    // Tickets and settlements, in order; cohort tallies per (P, Q, bell).
    let mut tickets: BTreeMap<[u8; 32], Ticket> = BTreeMap::new();
    let mut tally: BTreeMap<(i32, i32, u32), (u32, u32)> = BTreeMap::new();
    let mut checked_tx: BTreeSet<usize> = BTreeSet::new();
    for (n, r) in w.recs.iter().enumerate() {
        match r.kind {
            Kind::TICKET => {
                let tag15: [u8; 15] = r.k("citizen_tag15").try_into().unwrap_or([0; 15]);
                let t = ticket_of(r);
                for (p, q) in &t.provinces {
                    tally.entry((*p, *q, t.bell)).or_default().0 += 1;
                }
                tickets.insert(f.ctx.citizen_by_tag15(&tag15), t);
            }
            Kind::SETTLE => settle(cx, n, r, &mut tickets, &mut tally),
            _ => {}
        }
        // Compare the Province post-states of this transaction once its
        // last record is applied.
        let tx = &w.txs[r.tx];
        if tx.recs.last() == Some(&n) && checked_tx.insert(r.tx) {
            cohorts_match(cx, r.tx, &tally);
        }
    }
}

fn settle(
    cx: &mut Ctx,
    n: usize,
    r: &crate::world::Rec,
    tickets: &mut BTreeMap<[u8; 32], Ticket>,
    tally: &mut BTreeMap<(i32, i32, u32), (u32, u32)>,
) {
    let w = cx.w;
    let f = cx.f;
    let (p, q, site) = r.pqs();
    let what = format!("site ({p}, {q}, {site})");
    let outcome = r.pu8("outcome");
    let tag = r.pu64("citizen_tag");
    let tb = r.pu32("ticket_bell");
    let citizen = f.by_tag8.get(&tag).copied();
    // Does this settlement end the ticket? (fresh, displace, expired, or
    // the last site taken: SettleTicket's k.)
    let k = w.txs[r.tx]
        .ix(frontier_abi::tags::Ix::SettleTicket.tag())
        .and_then(|i| aix::SettleTicket::decode(&i.data).ok())
        .map(|x| x.k);
    let ticket = citizen.and_then(|c| tickets.get(&c));
    let ends = match outcome {
        settle_outcome::TAKEN => match (k, ticket) {
            (Some(k), Some(t)) => k as usize + 1 >= t.n as usize,
            _ => false,
        },
        _ => true,
    };
    if ends {
        if let Some(t) = citizen.and_then(|c| tickets.remove(&c)) {
            for (pp, qq) in &t.provinces {
                tally.entry((*pp, *qq, t.bell)).or_default().1 += 1;
            }
        }
    }
    if outcome == settle_outcome::EXPIRED {
        if r.bell == NO_BELL || r.bell < tb.saturating_add(COHORT_BELLS) {
            cx.fail(
                V,
                COHORT_MISMATCH,
                what,
                r.bell,
                Some(r.tx),
                format!("a ticket of bell {tb} settled expired at bell {}", r.bell),
            );
        }
        return;
    }
    let region = region_of(ProvinceCoord::new(p, q));
    let (Some(seed), Some(an)) = (f.bell_seed(tb, region), f.anchor_a(tb, region)) else {
        cx.missing(
            V,
            what,
            r.bell,
            Some(r.tx),
            format!("no seed of ({tb}, {region})"),
        );
        return;
    };
    let score = fclient::land::ticket_score(&seed, p, q, site, tag);
    if score != r.pu64("score") {
        cx.fail(
            V,
            TICKET_SCORE_MISMATCH,
            what.clone(),
            r.bell,
            Some(r.tx),
            format!(
                "score {} but S({tb}, {region}) gives {score}",
                r.pu64("score")
            ),
        );
    }
    let holder = f
        .owner_at((p, q, site), n)
        .filter(|o| o.owner.is_some())
        .copied();
    let holder_tag = holder
        .and_then(|h| h.owner)
        .map(|o| u64::from_le_bytes(o[..8].try_into().unwrap_or([0; 8])));
    match outcome {
        settle_outcome::FRESH | settle_outcome::DISPLACE => {
            let want_final = f.round_time(f.seed_round(tb, an)) + 600;
            if r.pi64("final_ts") != want_final {
                cx.fail(
                    V,
                    COHORT_MISMATCH,
                    what.clone(),
                    r.bell,
                    Some(r.tx),
                    format!(
                        "final_ts {} but round_time(S) + 600 = {want_final}",
                        r.pi64("final_ts")
                    ),
                );
            }
        }
        _ => {}
    }
    let tx_time = w.txs[r.tx].time;
    // v1.6 §22 (K9, integ-W4 review): a fresh settlement waits while an
    // earlier ticket cohort of the same Province is open (filed > settled,
    // within its 24 bells), as the Province stood before.
    if outcome == settle_outcome::FRESH && r.bell != NO_BELL {
        if let Some(pv) = w
            .state_before(&f.ctx.province(p, q), r.tx)
            .and_then(|d| fclient::decode::Province::decode(d).ok())
        {
            let open = pv.cohorts.iter().any(|c| {
                c.bell < tb && c.filed > c.settled && r.bell < c.bell.saturating_add(COHORT_BELLS)
            });
            if open {
                cx.fail(
                    V,
                    COHORT_MISMATCH,
                    what.clone(),
                    r.bell,
                    Some(r.tx),
                    format!("a fresh settlement of cohort {tb} while an earlier cohort was open"),
                );
            }
        }
    }
    match (outcome, holder, holder_tag) {
        // §8.5: displacement only of a provisional holding (before its
        // `final_ts`; integ-W4 review).
        (settle_outcome::DISPLACE, Some(h), Some(_)) if tx_time >= h.final_ts => cx.fail(
            V,
            DISPLACEMENT_RULE,
            what,
            r.bell,
            Some(r.tx),
            format!(
                "displacement at {tx_time} of a holding final since {}",
                h.final_ts
            ),
        ),
        (settle_outcome::FRESH, Some(_), _) => cx.fail(
            V,
            DISPLACEMENT_RULE,
            what,
            r.bell,
            Some(r.tx),
            "a fresh settlement on a held site",
        ),
        (settle_outcome::DISPLACE, Some(h), Some(ht))
            if h.ticket_bell != tb
                || !fclient::land::beats(score, tag, h.score, ht)
                || r.pu64("displaced_tag") != ht =>
        {
            cx.fail(
                V,
                DISPLACEMENT_RULE,
                what,
                r.bell,
                Some(r.tx),
                format!(
                    "displacement of a holder of cohort {} (score {}) by cohort {tb} (score {score})",
                    h.ticket_bell, h.score
                ),
            );
        }
        (settle_outcome::DISPLACE, Some(_), Some(_)) => {}
        (settle_outcome::DISPLACE, _, _) => cx.fail(
            V,
            DISPLACEMENT_RULE,
            what,
            r.bell,
            Some(r.tx),
            "a displacement on a site with no holder",
        ),
        (settle_outcome::TAKEN, Some(h), Some(ht))
            if h.ticket_bell == tb && fclient::land::beats(score, tag, h.score, ht) =>
        {
            cx.fail(
                V,
                DISPLACEMENT_RULE,
                what,
                r.bell,
                Some(r.tx),
                "a better ticket of the holder's own cohort was turned away",
            );
        }
        _ => {}
    }
}

/// Every cohort record of every Province this transaction wrote equals
/// the tallies.
fn cohorts_match(cx: &mut Ctx, tx: usize, tally: &BTreeMap<(i32, i32, u32), (u32, u32)>) {
    let w = cx.w;
    let t = &w.txs[tx];
    let mut bad = vec![];
    for (k, a) in &t.post {
        let Some(a) = a else { continue };
        if a.owner.to_bytes() != w.program || a.data.get(..8) != Some(&PV::MAGIC[..]) {
            continue;
        }
        let Ok(pv) = fclient::decode::Province::decode(&a.data) else {
            continue;
        };
        for c in pv.cohorts.iter().filter(|c| c.filed > 0) {
            let want = tally
                .get(&(pv.p as i32, pv.q as i32, c.bell))
                .copied()
                .unwrap_or((0, 0));
            if (c.filed as u32, c.settled as u32) != want {
                bad.push((*k, pv.p, pv.q, c.bell, (c.filed, c.settled), want));
            }
        }
    }
    for (_, p, q, bell, got, want) in bad {
        cx.fail(
            V,
            COHORT_MISMATCH,
            format!("province ({p}, {q})"),
            bell,
            Some(tx),
            format!("cohort {bell}: (filed, settled) {got:?}, the records give {want:?}"),
        );
    }
}
