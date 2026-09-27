//! Builders for Join, SetSession, SetVigil, FileTicket, SettleTicket,
//! ReleaseDormant, CloseHolding and CloseCitizen (§5.9, W3-A). The raw
//! builders are `fclient::ix`'s; this file adds the account positions the
//! forgery tests substitute.

pub use fclient::ix::{
    close_citizen, close_holding, file_ticket, join, release_dormant, seed_pair, set_session,
    set_vigil, settle_ticket, ticket_provinces, Displaced, HoldingRef, Player, SeedSource, Site,
};

/// Positions of Join's accounts.
pub mod join_at {
    pub const WALLET: usize = 0;
    pub const PAYER: usize = 1;
    pub const SEASON: usize = 2;
    pub const FRONTIER: usize = 3;
    pub const CITIZEN: usize = 4;
    pub const JOINSHARD: usize = 5;
    pub const SYSTEM: usize = 6;
    pub const JOIN_GATE: usize = 7;
}

/// Positions of the player prologue (every P instruction).
pub mod player_at {
    pub const ACTOR: usize = 0;
    pub const PAYER: usize = 1;
    pub const SEASON: usize = 2;
    pub const CITIZEN: usize = 3;
    /// FileTicket: the Frontier, then the Provinces.
    pub const FRONTIER: usize = 4;
    pub const PROVINCE0: usize = 5;
}

/// Positions of SettleTicket's accounts.
pub mod settle_at {
    pub const PAYER: usize = 0;
    pub const SEASON: usize = 1;
    pub const CITIZEN: usize = 2;
    pub const HOLDING: usize = 3;
    pub const PROVINCE: usize = 4;
    pub const JOINSHARD: usize = 5;
    pub const SEED: usize = 6;
    pub const ANCHOR: usize = 7;
    pub const OTHER0: usize = 8;
}

/// Positions of ReleaseDormant's accounts.
pub mod release_at {
    pub const ANY: usize = 0;
    pub const SEASON: usize = 1;
    pub const HOLDING: usize = 2;
    pub const PROVINCE: usize = 3;
    pub const CITIZEN: usize = 4;
    pub const JOINSHARD: usize = 5;
    pub const RENT_PAYER: usize = 6;
    pub const DPOOL: usize = 7;
}
