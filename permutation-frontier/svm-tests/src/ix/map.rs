//! Builders for OpenRing, ConsumeRingSeed, OpenProvince, FoldOccupancy and
//! CloseProvince (§5.9, W3-A). The raw builders are `fclient::ix`'s (the
//! client's and the keeper's account order); this file adds the account
//! positions the forgery tests substitute.

pub use fclient::ix::{
    close_province, consume_ring_seed, fold_occupancy, open_province, open_ring, region_of,
    ring_of, wedge_of,
};

/// Positions of OpenRing's accounts.
pub mod ring_at {
    pub const PAYER: usize = 0;
    pub const SEASON: usize = 1;
    pub const FRONTIER: usize = 2;
    pub const RINGSEED: usize = 3;
    /// The first of the six wedge funds.
    pub const PFUND0: usize = 4;
}

/// Positions of ConsumeRingSeed's accounts.
pub mod consume_at {
    pub const FEE_PAYER: usize = 0;
    pub const SEASON: usize = 1;
    pub const RINGSEED: usize = 2;
}

/// Positions of OpenProvince's accounts.
pub mod province_at {
    pub const PAYER: usize = 0;
    pub const SEASON: usize = 1;
    pub const RINGSEED: usize = 2;
    pub const PFUND: usize = 3;
    pub const PROVINCE: usize = 4;
    pub const SYSTEM: usize = 5;
}

/// Positions of FoldOccupancy's accounts (the part's shards or funds follow).
pub mod fold_at {
    pub const PAYER: usize = 0;
    pub const SEASON: usize = 1;
    pub const FRONTIER: usize = 2;
    pub const FIRST: usize = 3;
}

/// Positions of CloseProvince's accounts.
pub mod close_at {
    pub const ANY: usize = 0;
    pub const SEASON: usize = 1;
    pub const PROVINCE: usize = 2;
    pub const PFUND: usize = 3;
}
