//! Season lifecycle builders (§5.7). W2-A's instructions are here from wave
//! 2; EndSeason, AbortSeason and CloseSeason are W4-B's (handed over with
//! this file, §11 wave 4).

use fclient::addr::Addresses;
use frontier_abi::presets::{self, SeasonParams, SEASON_PARAMS_LEN};
use permutation_rules::frontier::payout::PayoutParams;
use solana_address::Address;
use solana_instruction::Instruction;

/// CreateSeason's data after the tag: `SeasonParams ‖ PayoutParams (borsh)`.
#[derive(Clone, Debug)]
pub struct Params {
    pub season: SeasonParams,
    pub payout: Vec<u8>,
}

impl Params {
    /// `season` with the rev-3 payout parameters.
    pub fn new(season: SeasonParams) -> Params {
        Params {
            season,
            payout: PayoutParams::REV3.to_borsh(),
        }
    }
    pub fn season_bytes(&self) -> [u8; SEASON_PARAMS_LEN] {
        self.season.to_bytes()
    }
    /// `sha256("PSF-PARAMS-v1" ‖ SeasonParams ‖ PayoutParams)` (§5.7).
    pub fn hash(&self) -> [u8; 32] {
        presets::params_hash(&self.season_bytes(), &self.payout)
    }
}

/// 0x08 AnnounceSeason, committing to `p`.
pub fn announce(
    a: &Addresses,
    authority: Address,
    p: &Params,
    t_create_min: i64,
    bond: u64,
) -> Instruction {
    fclient::ix::announce_season(a, authority, p.hash(), t_create_min, bond)
}

/// 0x01 CreateSeason with `p`.
pub fn create(a: &Addresses, authority: Address, p: &Params) -> Instruction {
    fclient::ix::create_season(a, authority, &p.season_bytes(), &p.payout)
}

pub use fclient::ix::{consume_genesis_seed, init_beacon_logs, init_shards, set_window_schedule};
