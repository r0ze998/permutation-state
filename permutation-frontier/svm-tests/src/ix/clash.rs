//! Clash builders (§5.11, W4-A): GatherClash, ResolveFromInputs,
//! ResolveClash (`oracle`), SkipQuiet, CloseClashInputs, CloseArrivalDay,
//! CloseArrivalSlot and the return settle (SettleDeparture with
//! `transit_slot = 0xFF`, §21). The raw builders are `fclient::ix`'s; the
//! bundles below fix the common arguments, and the positions serve
//! forgeries.

use fclient::addr::Addresses;
use fclient::ix::{AnchorSource, HoldingRef, SeedSource};
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};

pub use fclient::ix::{
    close_arrival_day, close_arrival_slot, close_clash_inputs, gather_clash, resolve_clash_oracle,
    resolve_from_inputs, settle_departure, skip_quiet,
};

/// `transit_slot` of the return settle (§21).
pub const RETURN_SLOT: u8 = 0xFF;

/// Positions of GatherClash's fixed accounts.
pub mod gather_at {
    pub const FEE_PAYER: usize = 0;
    pub const SEASON: usize = 1;
    pub const PROVINCE: usize = 2;
    pub const ANCHOR: usize = 3;
    pub const DAY: usize = 4;
    pub const INPUTS: usize = 5;
    pub const IX_SYSVAR: usize = 6;
    pub const SYSTEM: usize = 7;
    /// The first slot.
    pub const SLOT0: usize = 8;
}

/// Positions of ResolveFromInputs' accounts.
pub mod resolve_at {
    pub const FEE_PAYER: usize = 0;
    pub const SEASON: usize = 1;
    pub const PROVINCE: usize = 2;
    pub const INPUTS: usize = 3;
    pub const SEED: usize = 4;
    pub const ANCHOR: usize = 5;
    pub const IX_SYSVAR: usize = 6;
}

/// Positions of SkipQuiet's accounts.
pub mod skip_at {
    pub const PAYER: usize = 0;
    pub const SEASON: usize = 1;
    pub const PROVINCE: usize = 2;
    pub const DAY0: usize = 3;
    pub const DAY1: usize = 4;
    pub const ANCHOR0: usize = 5;
}

/// GatherClash with the anchor source given.
#[allow(clippy::too_many_arguments)]
pub fn gather(
    a: &Addresses,
    fee_payer: Address,
    dest: (i32, i32),
    bell: u32,
    src: AnchorSource,
    start: u8,
    slots: &[(u8, u8)],
    holdings: &[Address],
    bitmap: u32,
    beneficiary: &Address,
) -> Instruction {
    gather_clash(
        a,
        fee_payer,
        dest,
        bell,
        src,
        start,
        slots,
        holdings,
        bitmap,
        beneficiary,
    )
}

/// ResolveFromInputs from THE anchor's SeedCache of nonce 0.
pub fn resolve(
    a: &Addresses,
    fee_payer: Address,
    dest: (i32, i32),
    bell: u32,
    beneficiary: &Address,
) -> Instruction {
    resolve_from_inputs(
        a,
        fee_payer,
        dest,
        bell,
        SeedSource::Cache { nonce: 0 },
        beneficiary,
    )
}

/// SkipQuiet with every anchor present.
pub fn skip(a: &Addresses, payer: Address, dest: (i32, i32), b0: u32, n: u8) -> Instruction {
    skip_quiet(
        a,
        payer,
        dest,
        b0,
        n,
        &vec![AnchorSource::Anchor; n as usize],
    )
}

/// ResolveClash (`oracle` build): `[seedcache] [anchor] [arrivalday] [slot
/// × 24] [holding × m]` after the Province (program note of
/// `proc::clash::resolve_clash`).
pub fn oracle(
    a: &Addresses,
    fee_payer: Address,
    dest: (i32, i32),
    bell: u32,
    holdings: &[Address],
    beneficiary: &Address,
) -> Instruction {
    let (p, q) = dest;
    let region = fclient::ix::region_of(p, q);
    let mut m = vec![
        AccountMeta::new_readonly(a.seed_cache(bell, region, 0), false),
        AccountMeta::new_readonly(a.anchor(bell, region), false),
        AccountMeta::new_readonly(a.arrival_day(p, q, fclient::addr::day_of(bell)), false),
    ];
    for k in 0..24u8 {
        m.push(AccountMeta::new_readonly(
            a.arrival_slot(p, q, bell, k / 4, k % 4),
            false,
        ));
    }
    m.extend(
        holdings
            .iter()
            .map(|h| AccountMeta::new_readonly(*h, false)),
    );
    resolve_clash_oracle(a, fee_payer, dest, bell, beneficiary, m)
}

/// The return settle of the Leave entries of `h` in `province` (§21).
pub fn settle_return(
    a: &Addresses,
    payer: Address,
    province: (i16, i16),
    h: HoldingRef,
) -> Instruction {
    settle_departure(a, payer, province, h, RETURN_SLOT)
}
