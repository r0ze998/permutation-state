//! Run configuration. Defaults are the design's Season 1 preset and the
//! archetype mix of `rev2.py` §C2 (passive 15%, casual 45%, regular 30%,
//! core 9%, whale 1%), plus a scripted-bot share.

use permutation_rules::frontier::index::IndexParams;

/// How holdings emit laurels into their province.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emission {
    /// Design §5.4: every non-dormant, non-occupied holding emits 1/12.
    Full,
    /// Variant: holdings 2–3 collect but do not emit.
    FirstOnly,
    /// Variant: a holding emits 1/12 × its order factor (1, ½, ¼).
    OrderWeighted,
}

/// Who adds the laurel stake late (`AddStake` on the last join day, at
/// that day's price) instead of at Join: a review variant (late stakes used
/// to count laurels banked before them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LateStake {
    /// Everyone who stakes does so at Join (design default).
    None,
    /// Scripted bots that would stake do it on the last join day.
    Bots,
    /// Every wallet that would stake does it on the last join day.
    Stakers,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub seed: u64,
    pub agents: usize,
    /// Relative faction sizes (members), faction 0..6.
    pub faction_weights: [u32; 6],
    /// Human archetype shares: idle, casual, daily, skilled, very skilled.
    pub human_mix: [f64; 5],
    /// Share of all wallets that are scripted bots.
    pub bot_share: f64,
    /// Shades (operator AIs, bot policy, voided at the Reckoning), bps of
    /// wallets (design §7.1: 0.5%).
    pub shade_bps: u32,
    /// Probability of adding the laurel stake, by archetype (`Arch` order).
    /// Idle and casual stake as a small probe, and 10% of very skilled
    /// players and bots do not, so every cell of the table has wallets.
    pub stake_optin: [f64; 6],
    /// Share of wallets joining on day 0; the rest join uniformly on days
    /// 1..=21.
    pub day0_share: f64,
    /// Doctrines on (faction k gets `DOCTRINES[(k + rotation) % 6]`).
    pub doctrines: bool,
    pub doctrine_rotation: usize,
    /// Factions get identical archetype mixes (true) or random ones.
    pub stratified: bool,
    /// Use `DOCTRINES_TUNED` instead of the draft table.
    pub doctrines_tuned: bool,
    /// Season days (28) and genesis rings (2..=g open at bell 0).
    pub days: u32,
    pub genesis_rings: u32,
    pub r_max: u32,
    /// Index parameters used for the run's main settlement.
    pub index: IndexParams,
    /// Scripted-bot strategy overrides (decision quality, aggression);
    /// `None` = the SDK default of `model::profile`.
    pub bot_q: Option<f64>,
    pub bot_aggression: Option<f64>,
    /// Holding emission (design: `Full`).
    pub emission: Emission,
    /// Relic Sites spawn at Engine stages (design). `false`: a variant
    /// without them.
    pub relics: bool,
    /// Works credited per wallet per day at most [sim].
    pub works_cap: u64,
    /// Late `AddStake` variant (review).
    pub late_stake: LateStake,
    /// Scripted bots stand for office like the humans the sim elects (the
    /// most engaged win): a review variant; by default bots never hold a
    /// paid office.
    pub bot_officers: bool,
    /// Print progress to stderr.
    pub verbose: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            seed: 1,
            agents: 10_000,
            faction_weights: [1; 6],
            human_mix: [0.15, 0.45, 0.30, 0.09, 0.01],
            bot_share: 0.05,
            shade_bps: 50,
            stake_optin: [0.05, 0.10, 0.50, 0.90, 0.90, 0.90],
            day0_share: 0.6,
            doctrines: false,
            doctrine_rotation: 0,
            doctrines_tuned: false,
            stratified: true,
            days: 28,
            genesis_rings: 3,
            r_max: 64,
            index: IndexParams::REV2,
            bot_q: None,
            bot_aggression: None,
            emission: Emission::Full,
            relics: true,
            works_cap: crate::model::WORKS_DAY_CAP,
            late_stake: LateStake::None,
            bot_officers: false,
            verbose: false,
        }
    }
}
