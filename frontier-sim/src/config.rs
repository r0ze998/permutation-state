//! Run configuration. Defaults are the design's Season 1 preset and the
//! archetype mix of `rev2.py` §C2 (passive 15%, casual 45%, regular 30%,
//! core 9%, whale 1%), plus a scripted-bot share.

use permutation_rules::frontier::index::IndexParams;
use permutation_rules::frontier::payout::PayoutParams;

/// How holdings emit laurels into their province.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emission {
    /// Revision 2 (§5.4): every non-dormant, non-occupied holding emits 1/12.
    Full,
    /// Variant: holdings 2–3 collect but do not emit.
    FirstOnly,
    /// K3 default (O4 step 3, kernel `laurel::emission_quarters`): a
    /// holding emits 1/12 × its order factor (1, ½, ¼).
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

/// How officers are paid (owner decision O3; K3 compares the options).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OfficePay {
    /// USDC steward rows (§4.3) through the payout kernel, bounded by
    /// `PayoutParams::office_ceiling_bps` (the K3 choice; `u32::MAX` =
    /// revision 2, unbounded).
    Usdc,
    /// Variant: USDC rows capped at this share (bps) of what the officer
    /// paid, with no ceiling on the claim ("a share of what the officer
    /// paid", taken literally).
    ShareOfPaid(u32),
    /// Variant: no USDC rows; a seated officer who is a staker gets extra
    /// shares of the term's Mandate budget (Minister 2, paid Warden 1).
    Laurels,
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
    /// Doctrines on (faction k gets doctrine `(k + rotation) % 6` of the
    /// table `doctrine_set`).
    pub doctrines: bool,
    pub doctrine_rotation: usize,
    /// Factions get identical archetype mixes (true) or random ones.
    pub stratified: bool,
    /// Which doctrine table (kernel, draft, M0 proposal) …
    pub doctrine_set: crate::model::DoctrineSet,
    /// … with these tuning overrides (`model::apply_tweaks`).
    pub doctrine_tweaks: String,
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
    /// Relic Sites spawn at Engine stages and pay their revision-2 laurels
    /// (1 a bell to the holder). K3 default `false` (O4 step 4): they mint
    /// nothing, and with no other modelled purpose the sim does not spawn
    /// them.
    pub relics: bool,
    /// Works credited per wallet per day at most [sim].
    pub works_cap: u64,
    /// Late `AddStake` variant (review).
    pub late_stake: LateStake,
    /// Scripted bots stand for office like the humans the sim elects (the
    /// most engaged win): a review variant; by default bots never hold a
    /// paid office.
    pub bot_officers: bool,
    /// Scripted bots (not Shades) choose their join window: each joins on a
    /// day drawn uniformly from `lo..=hi` instead of the human join-day mix
    /// (`day0_share`). A bot operator picks its join day, so the bot
    /// criterion is taken as the maximum over these windows (review of
    /// K3). `None` = bots draw their day like humans (the SDK default).
    pub bot_join_days: Option<(u32, u32)>,
    /// Review variant (bots steer Mandates): in a faction-term whose
    /// Ministers are at least half bots (seated at the end of the previous
    /// term, as re-elected), the Ministers pick always-online tasks, and a
    /// human completes the term's Mandate with this multiple of its usual
    /// chance (0 = no human can). Bots complete as usual. `None` = off.
    pub bot_mandates: Option<f64>,
    /// At most this many office-terms (Minister or paid Warden) per wallet
    /// per season. D23 (decided 2026-09-27, CL-31): **1** in the K3 /
    /// Season-1 preset, so every suite, criterion and c4 default uses it;
    /// `None` = no limit (`--office-term-limit none`, the pre-D23 runs).
    /// A seat with no eligible candidate stays vacant for the term (no pay,
    /// no Assembly weight): `Stats::{minister,warden}_vacant`.
    pub office_term_limit: Option<u32>,
    /// D24 variant (CL-33, `--relic-to-mandate`): a Relic Site's emission
    /// is paid into the holder faction's Mandate reserve (which pays
    /// staking completers under the share floor) instead of to the
    /// holders. Only meaningful with `relics`. Off by default (D24 working
    /// default: Relic Sites pay Works and Dominion only, no laurels).
    pub relic_to_mandate: bool,
    /// Mandate share floor (m0c, kernel `mandate::share_floor`): the term's
    /// divisor is at least half of the faction's stakers active in it.
    /// `false` = the K3 kernel without the floor.
    pub mandate_floor: bool,
    /// Record one row per resolved clash (`Sim::clash_log`): the per-bell
    /// participation metric of the restated C4 (SP-FEE, m0c).
    pub clash_log: bool,
    /// C4 counterfactual (m0c): every arrival of faction `.0` landing in
    /// bells `.1..=.2` is excluded from the reveal window (routed at 50%, as
    /// an unrevealed arrival), and its holdings' committed postures in those
    /// bells fall into Disarray, as if an attacker bought every block of
    /// the reveal window at the keepers' price.
    pub attack: Option<(u8, u32, u32)>,
    /// Payout parameters of the settlement (office ceiling, Works rate).
    pub payout: PayoutParams,
    /// Laurel-stake accrual ramp, bps (`EntrySchedule::stake_ramp_bps`).
    pub stake_ramp_bps: u32,
    /// Officer pay scheme (O3).
    pub office_pay: OfficePay,
    /// Mandate reserve pays only completers who staked (O10). `false`: the
    /// M0 behaviour (every completer, equal split), for comparison.
    pub mandate_stakers_only: bool,
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
            doctrine_set: crate::model::DoctrineSet::Kernel,
            doctrine_tweaks: String::new(),
            stratified: true,
            days: 28,
            genesis_rings: 3,
            r_max: 64,
            index: IndexParams::REV2,
            bot_q: None,
            bot_aggression: None,
            emission: Emission::OrderWeighted,
            relics: false,
            works_cap: crate::model::WORKS_DAY_CAP,
            late_stake: LateStake::None,
            bot_officers: false,
            bot_join_days: None,
            bot_mandates: None,
            office_term_limit: Some(1),
            relic_to_mandate: false,
            mandate_floor: true,
            clash_log: false,
            attack: None,
            payout: PayoutParams::REV3,
            stake_ramp_bps: permutation_rules::frontier::pools::EntrySchedule::SEASON1
                .stake_ramp_bps,
            office_pay: OfficePay::Usdc,
            mandate_stakers_only: true,
            verbose: false,
        }
    }
}

impl Config {
    /// The economy of revision 2 as the M0 simulator ran it (`cea89be`):
    /// full emission for every holding, Relic Sites paying 1 laurel a bell,
    /// 140 Works per USDC, stakes priced by days left, officer pay without
    /// a ceiling, and the Mandate reserve split among every completer.
    pub fn set_rev2_economy(&mut self) {
        self.emission = Emission::Full;
        self.relics = true;
        self.payout = PayoutParams::REV2;
        self.stake_ramp_bps = 0;
        self.mandate_stakers_only = false;
        self.mandate_floor = false;
    }
}
