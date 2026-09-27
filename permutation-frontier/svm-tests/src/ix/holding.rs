//! Holding builders (§5.10): Harvest, Build, Train, Explore, SettleExplore
//! (W3-B). The builders are `fclient::ix`'s; the positions below serve
//! forgeries.

pub use fclient::ix::{build_item, explore, harvest, settle_explore, train, SeedSource};

/// Positions of the player prologue and a resident instruction's accounts
/// (§5.6, §5.10).
pub mod at {
    pub const ACTOR: usize = 0;
    pub const PAYER: usize = 1;
    pub const SEASON: usize = 2;
    pub const CITIZEN: usize = 3;
    pub const HOLDING: usize = 4;
    pub const PROVINCE: usize = 5;
}

/// Positions of SettleExplore's accounts (§5.10).
pub mod settle_at {
    pub const PAYER: usize = 0;
    pub const SEASON: usize = 1;
    pub const HOLDING: usize = 2;
    pub const CITIZEN: usize = 3;
    pub const SEED: usize = 4;
    pub const ANCHOR: usize = 5;
}
