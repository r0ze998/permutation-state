//! Host and departure builders (§5.10, §5.11): Muster, Dissolve, Garrison,
//! DisbandStranded, Depart, SettleDeparture (W3-B). The builders are
//! `fclient::ix`'s; the positions below serve forgeries.

pub use fclient::ix::{
    depart, disband_stranded, dissolve, garrison, muster, settle_departure, DepartArgs,
};

/// Positions of Depart's accounts (§5.11).
pub mod depart_at {
    pub const ACTOR: usize = 0;
    pub const PAYER: usize = 1;
    pub const SEASON: usize = 2;
    pub const CITIZEN: usize = 3;
    pub const HOLDING: usize = 4;
    pub const PROVINCE: usize = 5;
    pub const SYSTEM: usize = 6;
}

/// Positions of SettleDeparture's accounts (§5.11).
pub mod settle_at {
    pub const PAYER: usize = 0;
    pub const SEASON: usize = 1;
    pub const PROVINCE: usize = 2;
    pub const HOLDING: usize = 3;
}

/// Positions of DisbandStranded's accounts (§5.10).
pub mod disband_at {
    pub const ANY: usize = 0;
    pub const SEASON: usize = 1;
    pub const PROVINCE: usize = 2;
    pub const HOLDING: usize = 3;
}
