//! Beacon builders (§5.8). W2-A's instructions (PostAnchor, PostAnchorMulti,
//! PostSeed, PostBeacon) from wave 2; ArchiveAnchors and CloseSeedCache are
//! W4-B's (handed over with this file, §11 wave 4).

use fclient::addr::Addresses;
use solana_address::Address;
use solana_instruction::Instruction;

pub use fclient::ix::{
    archive_anchors, close_seed_cache, post_anchor, post_anchor_multi, post_beacon, post_seed,
    ArchiveItem, BeaconArg,
};

/// The mask of `regions` (PostAnchorMulti lists them in ascending order).
pub fn mask_of(regions: &[u8]) -> u16 {
    regions.iter().fold(0u16, |m, r| m | (1 << r))
}

/// PostAnchorMulti over `regions` (ascending order is the builder's).
pub fn post_anchor_regions(
    a: &Addresses,
    fee_payer: Address,
    bell: u32,
    b: &BeaconArg,
    regions: &[u8],
    beneficiary: &Address,
) -> Instruction {
    post_anchor_multi(a, fee_payer, bell, b, mask_of(regions), beneficiary)
}

/// Positions of PostAnchor's accounts (§5.8), for forgeries.
pub mod at {
    pub const FEE_PAYER: usize = 0;
    pub const SEASON: usize = 1;
    pub const ANCHOR: usize = 2;
    pub const ARCHIVE: usize = 3;
    pub const IX_SYSVAR: usize = 4;
    pub const SYSTEM: usize = 5;
}

/// Positions of PostSeed's accounts (§5.8).
pub mod seed_at {
    pub const FEE_PAYER: usize = 0;
    pub const SEASON: usize = 1;
    pub const ANCHOR: usize = 2;
    pub const CACHE: usize = 3;
    pub const IX_SYSVAR: usize = 4;
}
