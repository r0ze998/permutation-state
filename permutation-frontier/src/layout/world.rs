//! Season-wide accounts (§5.3): Season, Frontier, RingSeed, ProvinceFund,
//! JoinShard, BeaconLog, DefencePool. Offsets: `frontier_abi::layout::world`.

pub use frontier_abi::layout::world::{
    beacon_log, defence_pool, frontier, join_shard, province_fund, ring_seed, season,
};

use super::{Ro, Rw};
use crate::R;

/// The Season fields most handlers read after the prologue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeasonCore {
    pub id: u64,
    pub bump: u8,
    pub stored_status: u8,
    pub authority: [u8; 32],
    pub genesis_ts: i64,
    pub end_bell: u32,
    pub regions: u8,
}

impl SeasonCore {
    pub fn read(d: &[u8]) -> R<SeasonCore> {
        let r = Ro(d);
        Ok(SeasonCore {
            id: r.u64(season::SEASON_ID)?,
            bump: r.u8(season::BUMP)?,
            stored_status: r.u8(season::STATUS)?,
            authority: r.arr(season::AUTHORITY)?,
            genesis_ts: r.i64(season::GENESIS_TS)?,
            end_bell: r.u32(season::END_BELL)?,
            regions: r.u8(season::REGIONS)?,
        })
    }
}

/// A BeaconLog's latest round (§5.3): the reveal-window test reads it.
pub fn beacon_log_latest(d: &[u8]) -> R<u64> {
    Ro(d).u64(beacon_log::LATEST_ROUND)
}

/// Adds `v` to a u32 counter of a JoinShard / Frontier / fund.
pub fn bump_u32(d: &mut [u8], off: usize, v: u32) -> R<()> {
    Rw(d).add_u32(off, v)
}
