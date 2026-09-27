//! V9 — accounts at canonical addresses (contract §4.1, §8.5; I-02).
//!
//! Every program account of this season the archive or the final states
//! show — by its magic — sits at the address its own key fields derive
//! (`frontier_abi::addr::AddrCtx`, with-seed from the Season PDA). A
//! pre-funded address never blocks an init (§4.2): failed transactions
//! whose account was pre-funded are listed for information only.
//!
//! Codes: `NonCanonicalAddress`; info `PrefundedAddress`.

use frontier_abi::addr::AddrCtx;
use frontier_abi::layout::{
    beacon::{anchor_archive as AA, bell_anchor as BA, defence_claim as DC, seed_cache as SC},
    clash::{arrival_day as AD, arrival_slot as AS, clash_inputs as CI},
    header as HD,
    player::{citizen as C, holding as H},
    province::province as PV,
    world::{beacon_log as BL, join_shard as JS, province_fund as PF, ring_seed as RS},
    AccountKind,
};

use super::v1_chains::kind_by_magic;
use super::{ent, Ctx};
use crate::codes::*;
use crate::world::{le, Key};

const V: &str = "V9";

fn i16at(d: &[u8], o: usize) -> i32 {
    le(d.get(o..o + 2).unwrap_or(&[0, 0])) as u16 as i16 as i32
}
fn u8at(d: &[u8], o: usize) -> u8 {
    d.get(o).copied().unwrap_or(0)
}
fn u32at(d: &[u8], o: usize) -> u32 {
    le(d.get(o..o + 4).unwrap_or(&[0; 4])) as u32
}
fn arr32(d: &[u8], o: usize) -> [u8; 32] {
    d.get(o..o + 32)
        .and_then(|s| s.try_into().ok())
        .unwrap_or([0; 32])
}

/// The canonical address of an account of `kind` with bytes `d` (`None`:
/// a kind without a derivable key here, e.g. another season's Season).
pub fn canonical(ctx: &AddrCtx, kind: AccountKind, d: &[u8]) -> Option<Key> {
    use AccountKind::*;
    Some(match kind {
        Season => ctx.season,
        Frontier => ctx.frontier(),
        RingSeed => ctx.ring_seed(le(d.get(RS::D..RS::D + 2)?) as u16),
        ProvinceFund => ctx.province_fund(u8at(d, PF::WEDGE)),
        JoinShard => ctx.join_shard(u8at(d, JS::FACTION), u8at(d, JS::SHARD)),
        BeaconLog => ctx.beacon_log(u8at(d, BL::REGION)),
        DefencePool => ctx.defence_pool(),
        Citizen => ctx.citizen(&arr32(d, C::WALLET)),
        Holding => ctx.holding(i16at(d, H::P), i16at(d, H::Q), u8at(d, H::SITE)),
        Province => ctx.province(i16at(d, PV::P), i16at(d, PV::Q)),
        ArrivalSlot => ctx.arrival_slot(
            i16at(d, AS::P),
            i16at(d, AS::Q),
            u32at(d, AS::BELL),
            u8at(d, AS::FACTION),
            u8at(d, AS::I),
        ),
        ArrivalDay => ctx.arrival_day(i16at(d, AD::P), i16at(d, AD::Q), u32at(d, AD::DAY)),
        ClashInputs => ctx.clash_inputs(i16at(d, CI::P), i16at(d, CI::Q), u32at(d, CI::BELL)),
        BellAnchor => ctx.bell_anchor(u32at(d, BA::BELL), u8at(d, BA::REGION)),
        SeedCache => ctx.seed_cache(u32at(d, SC::BELL), u8at(d, SC::REGION), u8at(d, SC::NONCE)),
        AnchorArchive => ctx.anchor_archive(u8at(d, AA::REGION), u32at(d, AA::PART)),
        DefenceClaim => ctx.defence_claim(&arr32(d, DC::BENEFICIARY), u32at(d, DC::DAY)),
    })
}

pub fn run(cx: &mut Ctx) {
    let w = cx.w;
    let f = cx.f;
    let sid = cx.cfg.season_id;
    let mut seen = std::collections::BTreeSet::new();
    let mut check = |cx: &mut Ctx, k: &Key, d: &[u8], tx: Option<usize>| {
        if !seen.insert(*k) {
            return;
        }
        let Some(kind) = kind_by_magic(d) else { return };
        if le(d.get(HD::SEASON_ID..HD::SEASON_ID + 8).unwrap_or(&[])) != sid {
            return;
        }
        if let Some(want) = canonical(&f.ctx, kind, d) {
            if want != *k {
                cx.fail(
                    V,
                    NON_CANONICAL_ADDRESS,
                    ent(kind.name(), k),
                    0,
                    tx,
                    "the account is not at the address its key derives",
                );
            }
        }
    };
    for (i, t) in w.txs.iter().enumerate().filter(|(_, t)| t.ok) {
        for (k, a) in &t.post {
            if let Some(a) = a {
                if a.owner.to_bytes() == w.program {
                    check(cx, k, &a.data, Some(i));
                }
            }
        }
    }
    for (k, a) in &w.finals {
        if let Some(a) = a {
            if a.owner.to_bytes() == w.program {
                check(cx, k, &a.data, None);
            }
        }
    }
    // Pre-funded addresses (informational, §4.2): an init at an address
    // that already held lamports keeps them, so such an account ends above
    // its rent minimum; none of them blocked an init (the init that
    // created it landed, or V1 would miss its records).
    let mut prefunded = 0u64;
    for a in w.finals.values().flatten() {
        if a.owner.to_bytes() == w.program {
            if let Some(kind) = kind_by_magic(&a.data) {
                if kind != AccountKind::Season && a.lamports > kind.rent() && !is_pool(kind) {
                    prefunded += 1;
                }
            }
        }
    }
    if prefunded > 0 {
        cx.warn(
            V,
            PREFUNDED,
            "accounts",
            0,
            None,
            format!("{prefunded} accounts hold more than their rent (pre-funded or escrow; informational)"),
        );
    }
}

/// Kinds that hold lamports above rent by design (funds, pools, escrows).
fn is_pool(k: AccountKind) -> bool {
    matches!(
        k,
        AccountKind::ProvinceFund
            | AccountKind::DefencePool
            | AccountKind::Holding
            | AccountKind::Citizen
    )
}
