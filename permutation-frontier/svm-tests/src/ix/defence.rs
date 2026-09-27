//! ClaimDefence builder (§5.12, W4-B): `fclient::ix`'s; the positions
//! below serve forgeries.

pub use fclient::ix::{claim_defence, ClaimSlot};

/// Positions of ClaimDefence's accounts (§5.12); slot `k`'s pair starts at
/// `FIRST_SLOT + 2k`.
pub mod at {
    pub const KEEPER: usize = 0;
    pub const SEASON: usize = 1;
    pub const DPOOL: usize = 2;
    pub const CLAIM: usize = 3;
    pub const SYSTEM: usize = 4;
    pub const FIRST_SLOT: usize = 5;
}
