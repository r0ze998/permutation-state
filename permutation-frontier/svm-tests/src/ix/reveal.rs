//! Reveal builder (§5.11, W3-B): `fclient::ix::reveal`, plus the account
//! positions for forgeries.

pub use fclient::ix::{reveal, RevealArgs};

/// Positions of Reveal's accounts (§5.11).
pub mod at {
    pub const FEE_PAYER: usize = 0;
    pub const SEASON: usize = 1;
    pub const HOLDING: usize = 2;
    pub const ANCHOR: usize = 3;
    pub const ARCHIVE: usize = 4;
    pub const BEACONLOG: usize = 5;
    pub const INPUTS: usize = 6;
    pub const DEST: usize = 7;
    pub const DAY: usize = 8;
    pub const SLOT0: usize = 9;
    /// First path province (0–3 of them), then the instructions sysvar and
    /// the System program.
    pub const PATH0: usize = 13;
}
