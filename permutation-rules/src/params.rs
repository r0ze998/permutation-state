//! Ruleset parameters and presets (§1). Every number is part of the ruleset
//! and hashed into `ruleset_hash` before a season opens.
//!
//! "One ruleset, two clocks": `Preset` only changes wall-clock tick length,
//! civilization count, map radius and the entry window. Game numbers are
//! identical for Season and Blitz.

use crate::fixed::Bps;
use crate::{buildings, map, tech, units};
use borsh::{BorshDeserialize, BorshSerialize};

// v8: v7 + flatter milestone points (10/20/30/40/55), V5 §18.13.
pub const RULES_VERSION: u16 = 9; // v8 + batch caps, adoption skip, degraded step (WP04); bounded standing rules (WP05); ballots, one-pass election (WP06); proposal/support/unit caps, reveals at intake, dead units cleared, skip cap, envoy share cap (WP07); bounded checked market (WP08); treasury governance (WP10)

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
    pub start_gold: u32,
    pub start_spearmen: u32,
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
    /// A captured city's defence restarts at this share of its maximum
    /// (§8.3, v0.1 balance fix against instant recapture).
    pub capture_defense_bps: Bps,

    // --- society (§9) ---
    pub casus_belli_threshold: u16,
    pub aggressor_window: u16,
    pub loyalty_pressure_radius: u8,

    // --- diplomacy (§10) ---
    pub nap_ticks: u16,
    pub nap_min_bond: u32,
    pub alliance_leave_delay: u16,
    /// Open proposals expire after this many ticks (§10.6).
    pub proposal_ttl: u16,
    /// After peace takes effect, neither side may declare war on the other
    /// for this many ticks, casus belli or not (§10.2, v0.1 balance fix).
    pub truce_ticks: u16,

    // --- markets (§11) ---
    pub amm_fee_bps: Bps,
    pub hub_fee_bps: Bps,
    pub amm_seed_goods: u32,
    pub amm_seed_gold: u32,
    pub amm_limit_iterations: u8,
    pub exchange_fee_bps: Bps,
    pub vault_share_bps: Bps,
    /// Entry fee in USDC base units (6 decimals), the same for every member (V5 D4).
    pub entry_fee_usdc: u64,

    // --- neutral actors (§12) ---
    pub suzerain_threshold: u32,
    pub suzerain_lock_ticks: u16,
    pub crisis_interval: u16,

    // --- captures (§14.1, V5 §6.2) ---
    /// A captured city counts as "held" only if it was at least this old when taken.
    pub capture_min_founded_age: u16,

    // --- production fixes (v0.2 C1–C3) ---
    /// Ticks between two Star Gate stages of one civ (C2).
    pub star_gate_spacing: u16,
    /// A city attacked by a ranged army strikes back at this share (C5).
    pub ranged_city_strike_bps: Bps,

    // --- nations and governance (V5 §4–§5) ---
    /// Members per season; equals the chain's `SEASON_MEMBER_CAP` (48 is
    /// measured on the WP07 prototype; the joint capacity gate is pending).
    pub max_members: u32,
    pub max_offices_per_member: u8,
    pub term_ticks: u16,
    /// Votes for a term are taken in its last `vote_window` ticks before it starts.
    pub vote_window: u16,
    /// A recall stays open this many ticks.
    pub recall_ticks: u16,
    /// "Active in the last N ticks", the electorate of a recall.
    pub recall_electorate_ticks: u16,
    /// An officer with no executed order for this long faces an automatic recall.
    pub idle_recall_ticks: u16,
    /// An officer counts as active (for merit credited to "the officer") within this many ticks of an order.
    pub officer_active_ticks: u16,
    pub proposal_ttl_ticks: u16,
    /// Open proposals in one nation at once (adopted ones leave at intake).
    pub max_open_proposals: u16,
    pub max_proposal_orders: u8,
    /// Open proposals one member may have in its nation at once.
    pub max_member_proposals: u8,
    /// Open proposals one member may support at once.
    pub max_member_supports: u8,
    /// Largest `borsh(orders)` of a proposal, in bytes.
    pub max_proposal_bytes: u16,
    /// Units one nation may build in a season (genesis units not counted);
    /// production of a unit waits once it is reached (WP07).
    pub max_units_built: u16,
    /// Role budget split `[general, steward, science, diplomat]` by `B` = 0..=8 (V5 §5.2).
    pub role_split: [[u8; 4]; 9],
    /// Treasury spending above this per term needs a second officer's
    /// consent (V5 §7.5 ④): the no-consent allowance of a term.
    pub spend_consent_usdc: u64,

    // --- achievements (V5 §6), calibrated with `sim` (see V5 §6.5) ---
    pub tier_points: [u32; 5],
    pub hegemony_tiles: [u32; 5],
    /// Captured cities held, by tier (tier 3 accepts either the tiles or one city).
    pub hegemony_cities: [u32; 5],
    pub prosperity_pop: [u32; 5],
    pub prosperity_wealth: [u32; 5],
    pub science_techs: [u32; 3],
    /// Treaty partners by tier (tier 1: or an envoy; tier 4 also needs an alliance).
    pub concord_partners: [u32; 5],
    /// Suzerainties held at the end, by tier (tier 2: suzerain for at least one tick).
    pub concord_suzerains: [u32; 5],
    pub concord_trade: [u32; 2],
    /// A single counterparty counts for at most this share of trade volume.
    pub trade_counterparty_bps: Bps,

    // --- merit (V5 §7.3), in milli-merit ---
    pub merit_capture_per_pop: u32,
    pub merit_per_troop: u32,
    pub merit_city_held: u32,
    pub merit_found_city: u32,
    pub merit_found_per_tile: u32,
    pub merit_pop: u32,
    /// Divisors: merit = amount × 1000 / divisor.
    pub merit_building_div: u32,
    pub merit_gold_div: u32,
    pub merit_tech_div: u32,
    pub merit_trade_div: u32,
    pub merit_star_gate: u32,
    pub merit_treaty: u32,
    pub merit_suzerain: u32,
    pub merit_office_tick: u32,

    // --- payout (V5 §7) ---
    pub ops_share_bps: Bps,
    pub equal_share_bps: Bps,
    /// Equal share cap per member, as a share of the entry fee.
    pub equal_cap_bps: Bps,
    /// V5 §18.5: the merit unit for taking operator AIs' payouts, as a share
    /// of the revealed AIs' average merit. A person takes at most
    /// `equal_cap × max(1, merit / unit)` of AI payouts in total.
    pub redistribute_merit_unit_bps: Bps,
    pub activity_window_ticks: u16,
    pub active_windows_needed: u8,

    // --- USDC market (V5 §7.5) ---
    pub market_enabled: bool,
    pub delivery_ticks: u16,
    /// Tariff in bps at cumulative spend 0, 10%, …, 100% of `tariff_full_usdc`.
    pub tariff_table_bps: [u32; 11],
    pub tariff_full_usdc: u64,
    /// Highest `ExchangeOrder.price` (USDC base units per whole unit).
    pub exchange_max_price: u64,
    /// Highest `amount` of one `ExchangeOrder`, `MarketTrade` or `Transfer`.
    pub max_trade_amount: u32,

    // --- paths and pacing (2026-09-25) ---
    /// A conquest counts for hegemony tier 3 once (banked) when the city had
    /// at least this population when taken.
    pub conquest_min_pop: u32,
    /// Peace after a war of at least this many ticks becomes a pact
    /// (non-aggression, no bond) for `nap_ticks`: a treaty partner.
    pub peace_pact_min_war: u16,
    /// Eureka (a triggered tech boost): research cost in bps.
    pub eureka_cost_bps: Bps,
    /// Crisis (§12.2, revised): from `crisis_start`, every `crisis_interval`
    /// ticks, the `crisis_targets` leading nations by points each lose
    /// `crisis_pop` population and `crisis_loyalty` loyalty in their most
    /// populous city.
    pub crisis_targets: u8,
    pub crisis_pop: u32,
    pub crisis_loyalty: i32,
    /// Dark age: at `dark_age_tick`, a nation with members and points below
    /// `dark_age_share_bps` of the leader's researches at
    /// `dark_age_research_bps` of the cost and has `dark_age_budget` extra
    /// orders per tick, until `dark_age_until`.
    pub dark_age_tick: u16,
    pub dark_age_until: u16,
    pub dark_age_share_bps: Bps,
    pub dark_age_research_bps: Bps,
    pub dark_age_budget: u16,

    // --- operator AI members, bounties, contracts (V5 §18) ---
    /// At the end of this tick every nation's cities are recorded; an
    /// operator AI's home city is drawn among its nation's (V5 §18.3), and
    /// only captures after it count toward a bounty.
    pub ai_home_tick: u16,
    /// No bounty when the captor and the victim had a NAP or an alliance
    /// within this many ticks before the capture (V5 §18.4).
    pub bounty_pact_window: u16,
    /// Contracts a nation may have offered and not yet settled (V5 §18.6).
    pub contract_max_open: u8,
    /// Latest deadline of a contract, in ticks after it is offered.
    pub contract_max_ticks: u16,
    /// A "keep the NAP" contract pays in at most this many installments.
    pub contract_max_installments: u8,
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

            start_min_distance: 7,
            start_fairness_max_bps: 11_000,
            start_generation_attempts: 64,
            resource_reserve: 90,

            start_gold: 20,
            start_spearmen: 3,
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
            capture_defense_bps: 5_000,

            casus_belli_threshold: 30,
            aggressor_window: 12,
            loyalty_pressure_radius: 6,

            nap_ticks: 30,
            nap_min_bond: 30,
            alliance_leave_delay: 6,
            proposal_ttl: 6,
            truce_ticks: 12,

            amm_fee_bps: 300,
            hub_fee_bps: 100,
            amm_seed_goods: 200,
            amm_seed_gold: 2_000,
            amm_limit_iterations: 3,
            exchange_fee_bps: 500,
            vault_share_bps: 8_000,
            entry_fee_usdc: 10_000_000,

            suzerain_threshold: 60,
            suzerain_lock_ticks: 45,
            crisis_interval: 6,

            capture_min_founded_age: 12,

            star_gate_spacing: 6,
            ranged_city_strike_bps: 5_000,

            max_members: 48,
            max_offices_per_member: 2,
            term_ticks: 30,
            vote_window: 10,
            recall_ticks: 5,
            recall_electorate_ticks: 10,
            idle_recall_ticks: 30,
            officer_active_ticks: 10,
            proposal_ttl_ticks: 10,
            max_open_proposals: 8,
            max_proposal_orders: 4,
            max_member_proposals: 2,
            max_member_supports: 4,
            max_proposal_bytes: 256,
            max_units_built: 56,
            role_split: [
                [0, 0, 0, 0],
                [1, 0, 0, 0],
                [1, 1, 0, 0],
                [1, 1, 0, 1],
                [1, 1, 1, 1],
                [2, 1, 1, 1],
                [2, 2, 1, 1],
                [2, 3, 1, 1],
                [3, 3, 1, 1],
            ],
            spend_consent_usdc: 5_000_000,

            tier_points: [10, 20, 30, 40, 55],
            hegemony_tiles: [25, 40, 60, 80, 100],
            hegemony_cities: [0, 0, 1, 1, 2],
            prosperity_pop: [9, 15, 22, 31, 42],
            prosperity_wealth: [0, 700, 1_400, 2_400, 3_600],
            science_techs: [6, 11, 15],
            concord_partners: [1, 1, 1, 1, 2],
            concord_suzerains: [0, 0, 1, 1, 1],
            concord_trade: [400, 1_000],
            trade_counterparty_bps: 4_000,

            merit_capture_per_pop: 10_000,
            merit_per_troop: 1_000,
            merit_city_held: 5_000,
            merit_found_city: 20_000,
            merit_found_per_tile: 1_000,
            merit_pop: 5_000,
            merit_building_div: 10,
            merit_gold_div: 20,
            merit_tech_div: 10,
            merit_trade_div: 20,
            merit_star_gate: 100_000,
            merit_treaty: 20_000,
            merit_suzerain: 2_000,
            merit_office_tick: 1_000,

            ops_share_bps: 2_000,
            equal_share_bps: 2_000,
            equal_cap_bps: 5_000,
            redistribute_merit_unit_bps: 1_000,
            activity_window_ticks: 10,
            active_windows_needed: 9,

            market_enabled: true,
            delivery_ticks: 3,
            // 5% + 95% × x^1.3 at x = 0, 0.1, …, 1.0 (V5 §7.5).
            tariff_table_bps: [
                500, 976, 1_672, 2_486, 3_387, 4_358, 5_391, 6_475, 7_608, 8_784, 10_000,
            ],
            tariff_full_usdc: 100_000_000,
            exchange_max_price: 100_000_000,
            max_trade_amount: 10_000,

            conquest_min_pop: 3,
            peace_pact_min_war: 6,
            eureka_cost_bps: 7_500,
            crisis_targets: 2,
            crisis_pop: 1,
            crisis_loyalty: 20,
            dark_age_tick: 90,
            dark_age_until: 150,
            dark_age_share_bps: 2_000,
            dark_age_research_bps: 7_500,
            dark_age_budget: 1,

            ai_home_tick: 45,
            bounty_pact_window: 10,
            contract_max_open: 4,
            contract_max_ticks: 60,
            contract_max_installments: 10,
        }
    }

    /// `min(3, max(2, civs / 3))` (§10.4).
    pub fn alliance_cap(&self, civs: usize) -> usize {
        (civs / 3).clamp(2, 3)
    }

    /// A role's share of the nation budget `b` (V5 §5.2). Role index: general,
    /// steward, science, diplomat.
    pub fn role_budget(&self, b: u16, role: usize) -> u16 {
        self.role_split[(b as usize).min(self.role_split.len() - 1)][role] as u16
    }

    /// Market tariff in bps for a nation that has spent `spent` USDC base
    /// units this season: linear between the table points (V5 §7.5).
    pub fn tariff_bps(&self, spent: u64) -> u32 {
        let full = self.tariff_full_usdc.max(1);
        if spent >= full {
            return self.tariff_table_bps[10];
        }
        let x = spent as u128 * 10; // position in tenths of `full`
        let i = (x / full as u128) as usize;
        let frac = x % full as u128;
        let (a, b) = (
            self.tariff_table_bps[i] as u128,
            self.tariff_table_bps[i + 1] as u128,
        );
        (a + (b - a) * frac / full as u128) as u32
    }

    /// Number of activity windows in a season (V5 §7.3).
    pub fn activity_windows(&self) -> u16 {
        self.ticks_per_season.div_ceil(self.activity_window_ticks)
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
        crate::hash::sha256(&[
            b"permutation-rules/ruleset",
            &borsh::to_vec(self).expect("ruleset serializes"),
            &borsh::to_vec(&units::UNIT_STATS[..]).expect("units serialize"),
            &borsh::to_vec(&tech::TECHS[..]).expect("techs serialize"),
            &borsh::to_vec(&buildings::BUILDINGS[..]).expect("buildings serialize"),
            &borsh::to_vec(&map::TERRAIN_TABLE[..]).expect("terrain serializes"),
        ])
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
    fn role_split_sums_to_the_budget() {
        let r = Ruleset::new(Preset::Blitz);
        for b in 0..=8u16 {
            let sum: u16 = (0..4).map(|role| r.role_budget(b, role)).sum();
            assert_eq!(sum, b, "B = {b}");
        }
        assert_eq!(r.role_budget(12, 0), 3); // capped at the B = 8 row
    }

    #[test]
    fn tariff_curve_matches_the_design_table() {
        let r = Ruleset::new(Preset::Blitz);
        let usdc = |x: u64| x * 1_000_000;
        assert_eq!(r.tariff_bps(0), 500);
        assert_eq!(r.tariff_bps(usdc(10)), 976);
        assert_eq!(r.tariff_bps(usdc(30)), 2_486);
        assert_eq!(r.tariff_bps(usdc(50)), 4_358);
        assert_eq!(r.tariff_bps(usdc(100)), 10_000);
        assert_eq!(r.tariff_bps(usdc(500)), 10_000);
        assert_eq!(r.tariff_bps(usdc(5)), (500 + 976) / 2);
        let mut last = 0;
        for s in (0..120).map(usdc) {
            let t = r.tariff_bps(s);
            assert!(t >= last);
            last = t;
        }
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
        let mut c = a.clone();
        c.tier_points[4] += 1;
        assert_ne!(a.hash(), c.hash());
        assert_ne!(a.hash(), b.hash());
        assert_eq!(a.hash(), Ruleset::new(Preset::Blitz).hash());
    }
}
