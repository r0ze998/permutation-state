//! V6 — arrival quotas (contract §5.11 Reveal step 7, §8.5; design §6.2).
//!
//! Every REVEAL of `(P, Q, b, faction)` is replayed in landing order
//! through the kernel's `clash::admit_arrival` over that faction's four
//! slots, with `SlotEntry{host, citizen_tag of its holding's owner,
//! dep_mass of its DEPART}`: the record's slot `i`, fill/displace and
//! displaced host must be the kernel's decision, and a refused arrival must
//! not have landed. The ArrivalSlot each Reveal wrote carries DEPART's mass.
//! At the resolve, the gathered ClashInputs hold exactly the replayed final
//! slots (the four largest revealed arrivals by `(mass, slot_key)`, one per
//! citizen) with DEPART's masses.
//!
//! Codes: `QuotaSetMismatch`, `TransitMassMismatch`.

use std::collections::{BTreeMap, BTreeSet};

use frontier_abi::layout::clash::arrival_slot as AS;
use frontier_abi::log::Kind;
use permutation_rules::frontier::clash::{
    admit_arrival, apply_slot, FactionSlots, SlotDecision, SlotEntry,
};
use permutation_rules::frontier::geometry::ProvinceCoord;

use super::Ctx;
use crate::codes::*;
use crate::world::le;

const V: &str = "V6";

type Slots = BTreeMap<(i32, i32, u32, u8), FactionSlots>;

/// The citizen tag of a host's holding owner at record `n`.
pub fn tag_of_host(cx: &Ctx, host: u64, n: usize) -> Option<u64> {
    let o = cx.f.owner_of_host(host, n)?.owner?;
    Some(u64::from_le_bytes(o[..8].try_into().ok()?))
}

/// Replays every REVEAL through `admit_arrival`, reporting mismatches, and
/// returns the final slots per `(P, Q, bell, faction)`.
pub fn replay_slots(cx: &mut Ctx, report: bool) -> Slots {
    let w = cx.w;
    let f = cx.f;
    let mut slots: Slots = BTreeMap::new();
    for (n, r) in w
        .recs
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == Kind::REVEAL)
    {
        let (p, q) = r.pq();
        let (b, fct, i) = (r.ku32("arrive"), r.ku8("faction"), r.ku8("i"));
        let host = r.pu64("host_id");
        let what = format!("slots ({p}, {q}) bell {b} faction {fct}");
        let (Some(d), Some(tag)) = (f.depart_of(w, host, n, Some(b)), tag_of_host(cx, host, n))
        else {
            if report {
                cx.missing(
                    V,
                    what,
                    b,
                    Some(r.tx),
                    format!("host {host}: no DEPART or no owner"),
                );
            }
            continue;
        };
        let mass = w.recs[d].pu32("dep_mass");
        let e = SlotEntry {
            host_id: host,
            citizen: tag,
            troops: mass,
        };
        let s = slots.entry((p, q, b, fct)).or_default();
        let dec = admit_arrival(ProvinceCoord::new(p, q), b, s, e);
        let (disp, dhost) = (r.pu8("displace") != 0, r.pu64("displaced_host"));
        let ok = match dec {
            SlotDecision::Fill { slot } => slot == i && !disp,
            SlotDecision::Displace { slot, displaced } => {
                slot == i && disp && dhost == displaced.host_id
            }
            SlotDecision::Refuse(_) => false,
        };
        if !ok && report {
            cx.fail(
                V,
                QUOTA_SET_MISMATCH,
                what.clone(),
                b,
                Some(r.tx),
                format!("host {host}: logged slot {i} (displace {disp}, host {dhost}); admit_arrival gives {dec:?}"),
            );
        }
        apply_slot(s, e, dec);
        // The slot the Reveal wrote carries DEPART's mass.
        if report {
            let key = f.ctx.arrival_slot(p, q, b, fct, i);
            if let Some(sd) = w.txs[r.tx].post_data(&key) {
                if le(&sd[AS::DEP_MASS..AS::DEP_MASS + 4]) as u32 != mass
                    || le(&sd[AS::HOST_ID..AS::HOST_ID + 8]) != host
                {
                    cx.fail(
                        V,
                        TRANSIT_MASS_MISMATCH,
                        format!("host {host}"),
                        b,
                        Some(r.tx),
                        format!("the ArrivalSlot's mass is not DEPART's {mass}"),
                    );
                }
            }
        }
    }
    slots
}

pub fn run(cx: &mut Ctx) {
    let w = cx.w;
    let f = cx.f;
    let slots = replay_slots(cx, true);
    for (n, r) in w
        .recs
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == Kind::CLASH)
    {
        let (p, q) = r.pq();
        let b = r.ku32("bell");
        let ck = f.ctx.clash_inputs(p, q, b);
        let Some(inputs) = w.state_before(&ck, r.tx) else {
            cx.missing(
                V,
                format!("inputs ({p}, {q}) bell {b}"),
                b,
                Some(r.tx),
                "no gathered ClashInputs state",
            );
            continue;
        };
        let Ok(ci) = fclient::decode::ClashInputs::decode(inputs) else {
            cx.missing(
                V,
                format!("inputs ({p}, {q}) bell {b}"),
                b,
                Some(r.tx),
                "ClashInputs do not decode",
            );
            continue;
        };
        for fct in 0..6u8 {
            let want: BTreeSet<u64> = slots
                .get(&(p, q, b, fct))
                .map(|s| s.iter().flatten().map(|e| e.host_id).collect())
                .unwrap_or_default();
            let got: BTreeSet<u64> = (0..4)
                .map(|i| &ci.arrivals[fct as usize * 4 + i])
                .filter(|a| a.present == 1)
                .map(|a| a.host_id)
                .collect();
            if want != got {
                cx.fail(
                    V,
                    QUOTA_SET_MISMATCH,
                    format!("inputs ({p}, {q}) bell {b} faction {fct}"),
                    b,
                    Some(r.tx),
                    format!("gathered {got:?}, the reveals give {want:?}"),
                );
            }
        }
        for a in ci.arrivals.iter().filter(|a| a.present == 1) {
            let Some(d) = f.depart_of(w, a.host_id, n, Some(b)) else {
                continue;
            };
            if a.dep_mass != w.recs[d].pu32("dep_mass") {
                cx.fail(
                    V,
                    TRANSIT_MASS_MISMATCH,
                    format!("host {}", a.host_id),
                    b,
                    Some(r.tx),
                    format!(
                        "gathered mass {}, DEPART {}",
                        a.dep_mass,
                        w.recs[d].pu32("dep_mass")
                    ),
                );
            }
        }
    }
}
