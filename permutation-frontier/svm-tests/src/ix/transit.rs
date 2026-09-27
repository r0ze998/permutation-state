//! Transit builders (§5.11): SettleTransit and SweepPoolOwed (W4-B). The
//! builders are `fclient::ix`'s; the positions below serve forgeries.

pub use fclient::ix::{settle_transit, sweep_pool_owed, SettleTransitArgs};

/// Positions of SettleTransit's accounts (§5.11).
pub mod at {
    pub const PAYER: usize = 0;
    pub const SEASON: usize = 1;
    pub const HOLDING: usize = 2;
    pub const DEST: usize = 3;
    pub const INPUTS: usize = 4;
    pub const SLOT: usize = 5;
    pub const HOME: usize = 6;
    pub const ANCHOR: usize = 7;
    pub const SLOT_BENEFICIARY: usize = 8;
    pub const RESOLVER: usize = 9;
    pub const RENT_PAYER: usize = 10;
    pub const SETTLE_BENEFICIARY: usize = 11;
    pub const SYSTEM: usize = 12;
}

/// Positions of SweepPoolOwed's accounts (§5.12).
pub mod sweep_at {
    pub const ANY: usize = 0;
    pub const SEASON: usize = 1;
    pub const HOLDING: usize = 2;
    pub const DPOOL: usize = 3;
}
