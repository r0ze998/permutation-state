//! Ruleset parameters and presets (§1). Every number is part of the ruleset
//! and hashed into `ruleset_hash` before a season opens.
//!
//! "One ruleset, two clocks": `Preset` only changes wall-clock tick length,
//! civilization count, map radius and the entry window. Game numbers are
//! identical for Season and Blitz.

use crate::fixed::Bps;
use crate::{buildings, map, tech, units};
use borsh::{BorshDeserialize, BorshSerialize};
use sha2::{Digest, Sha256};

pub const RULES_VERSION: u16 = 1; // Rules Specification v0.1

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Preset {
    Season,
    Blitz,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Ruleset {
    pub version: u16,
    pub preset: Preset,

    // --- clock and size (§1) ---
    pub tick_seconds: u32,
    pub ticks_per_season: u16,
    pub max_civs: u8,
    pub map_radius: u8,
    pub entry_close_tick: u16,

    // --- phases (§1.1): first tick of each later phase ---
    pub expansion_start: u16,
    pub contention_start: u16,
    pub crisis_start: u16,
    pub resolution_start: u16,
    pub transfer_freeze_tick: u16,
    pub exchange_freeze_tick: u16,

    // --- map (§2) ---
    pub start_min_distance: u8,
    pub start_fairness_max_bps: Bps,
    pub start_generation_attempts: u8,
    pub resource_reserve: u16,

    // --- entry and start (§3) ---
    pub max_civs_per_wallet: u8,
    pub start_gold: u32,
    pub start_spearmen: u32,
    pub late_join_gold_per_tick: u32,
    pub late_join_prod_per_tick: u32,
    /// Protected-zone radius by phase boundary tick: (from_tick, radius) (§3.3).
    pub protection_schedule: [(u16, u8); 5],
    pub heritage_radius: u8,
    pub heritage_ticks: u16,

    // --- orders (§4.1) ---
    pub budget_base: u16,
    pub budget_cap: u16,
    pub bank_ticks: u16,
    pub max_path_len: u8,

    // --- cities (§5) ---
    pub food_per_pop: u32,
    pub city_min_distance: u8,
    pub settler_min_pop: u32,
    pub purchase_gold_per_prod: u32,
    pub purchase_max_bps: Bps,
    pub stalemate_floor_bps: Bps,
    pub stalemate_step_bps: Bps,

    // --- economy (§6) ---
    pub tech_cost_per_city_bps: Bps,

    // --- armies and combat (§7, §8) ---
    pub army_cap_milli: u32,
    pub army_min_milli: u32,
    pub combat_k_bps: Bps,
    pub variance_min_bps: Bps,
    pub variance_span: u32,
    pub city_defense_base: u32,
    pub city_regen_milli: u32,
    pub last_city_protection_ticks: u16,

    // --- society (§9) ---
    pub casus_belli_threshold: u16,
    pub aggressor_window: u16,
    pub loyalty_pressure_radius: u8,

    // --- diplomacy (§10) ---
    pub nap_ticks: u16,
    pub nap_min_bond: u32,
    pub alliance_leave_delay: u16,

    // --- markets (§11) ---
    pub amm_fee_bps: Bps,
    pub hub_fee_bps: Bps,
    pub exchange_fee_bps: Bps,
    pub vault_share_bps: Bps,

    // --- neutral actors (§12) ---
    pub suzerain_threshold: u32,
    pub suzerain_lock_ticks: u16,
    pub crisis_interval: u16,

    // --- scoring and payout (§14) ---
    pub capture_hold_ticks: u16,
    pub capture_min_founded_age: u16,
    pub neutrality_mult_bps: Bps,
    pub coalition_handicap_bps: Bps,
    /// Dominion, Science, Concord, Participation. ON HOLD (V4 §12): placeholder.
    pub track_share_bps: [Bps; 4],
    pub winners_bps: Bps,
    pub winners_min: u16,
    pub winners_max: u16,
    pub payout_decay_bps: Bps,
}

impl Ruleset {
    pub fn new(preset: Preset) -> Self {
        let (tick_seconds, max_civs, map_radius, entry_close_tick) = match preset {
            Preset::Season => (4 * 60 * 60, 16, 19, 30),
            Preset::Blitz => (30, 8, 13, 0),
        };
        Ruleset {
            version: RULES_VERSION,
            preset,
            tick_seconds,
            ticks_per_season: 180,
            max_civs,
            map_radius,
            entry_close_tick,

            expansion_start: 18,
            contention_start: 60,
            crisis_start: 120,
            resolution_start: 162,
            transfer_freeze_tick: 162,
            exchange_freeze_tick: 120,

            start_min_distance: 8,
            start_fairness_max_bps: 11_000,
            start_generation_attempts: 64,
            resource_reserve: 90,

            max_civs_per_wallet: 2,
            start_gold: 20,
            start_spearmen: 3,
            late_join_gold_per_tick: 3,
            late_join_prod_per_tick: 1,
            protection_schedule: [(0, 4), (18, 3), (30, 2), (45, 1), (60, 0)],
            heritage_radius: 2,
            heritage_ticks: 10,

            budget_base: 3,
            budget_cap: 8,
            bank_ticks: 4,
            max_path_len: 12,

            food_per_pop: 2,
            city_min_distance: 3,
            settler_min_pop: 2,
            purchase_gold_per_prod: 3,
            purchase_max_bps: 5_000,
            stalemate_floor_bps: 6_000,
            stalemate_step_bps: 100,

            tech_cost_per_city_bps: 1_000,

            army_cap_milli: 20_000,
            army_min_milli: 500,
            combat_k_bps: 5_500,
            variance_min_bps: 9_000,
            variance_span: 2_001,
            city_defense_base: 4,
            city_regen_milli: 2_000,
            last_city_protection_ticks: 12,

            casus_belli_threshold: 30,
            aggressor_window: 12,
            loyalty_pressure_radius: 6,

            nap_ticks: 30,
            nap_min_bond: 30,
            alliance_leave_delay: 6,

            amm_fee_bps: 300,
            hub_fee_bps: 100,
            exchange_fee_bps: 500,
            vault_share_bps: 8_000,

            suzerain_threshold: 60,
            suzerain_lock_ticks: 45,
            crisis_interval: 6,

            capture_hold_ticks: 30,
            capture_min_founded_age: 12,
            neutrality_mult_bps: 12_500,
            coalition_handicap_bps: 2_500,
            track_share_bps: [3_000, 2_500, 3_000, 1_500],
            winners_bps: 2_000,
            winners_min: 3,
            winners_max: 50,
            payout_decay_bps: 7_500,
        }
    }

    /// `min(3, max(2, civs / 3))` (§10.4).
    pub fn alliance_cap(&self, civs: usize) -> usize {
        (civs / 3).clamp(2, 3)
    }

    /// Protected-zone radius at tick `t` (§3.3).
    pub fn protection_radius(&self, t: u16) -> u8 {
        let mut r = 0;
        for (from, radius) in self.protection_schedule {
            if t >= from {
                r = radius;
            }
        }
        r
    }

    /// `ruleset_hash = sha256(borsh(ruleset) ‖ borsh(tables))`. The static
    /// tables are part of the rules, so changing any table changes the hash.
    pub fn hash(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"permutation-rules/ruleset");
        h.update(borsh::to_vec(self).expect("ruleset serializes"));
        h.update(borsh::to_vec(&units::UNIT_STATS[..]).expect("units serialize"));
        h.update(borsh::to_vec(&tech::TECHS[..]).expect("techs serialize"));
        h.update(borsh::to_vec(&buildings::BUILDINGS[..]).expect("buildings serialize"));
        h.update(borsh::to_vec(&map::TERRAIN_TABLE[..]).expect("terrain serializes"));
        h.finalize().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_share_every_game_number() {
        let mut season = Ruleset::new(Preset::Season);
        let blitz = Ruleset::new(Preset::Blitz);
        // Normalise the fields that are allowed to differ.
        season.preset = blitz.preset;
        season.tick_seconds = blitz.tick_seconds;
        season.max_civs = blitz.max_civs;
        season.map_radius = blitz.map_radius;
        season.entry_close_tick = blitz.entry_close_tick;
        assert_eq!(season, blitz);
    }

    #[test]
    fn alliance_cap_formula() {
        let r = Ruleset::new(Preset::Blitz);
        assert_eq!(r.alliance_cap(6), 2);
        assert_eq!(r.alliance_cap(16), 3);
        assert_eq!(r.alliance_cap(3), 2);
    }

    #[test]
    fn protection_shrinks_on_schedule() {
        let r = Ruleset::new(Preset::Blitz);
        let got: [u8; 7] = [0, 17, 18, 30, 45, 59, 60].map(|t| r.protection_radius(t));
        assert_eq!(got, [4, 4, 3, 2, 1, 1, 0]);
    }

    #[test]
    fn hash_changes_with_any_parameter() {
        let a = Ruleset::new(Preset::Blitz);
        let mut b = a.clone();
        b.exchange_fee_bps += 1;
        assert_ne!(a.hash(), b.hash());
        assert_eq!(a.hash(), Ruleset::new(Preset::Blitz).hash());
    }
}
