//! The season loop: 28 days of 144 bells, agents acting in sessions, the
//! map growing by the crowding rule, clashes and sieges resolved by the
//! rules-v10 kernels, laurels credited through per-province reward
//! indices, and the season settled and claimed with the closed-form
//! payout kernel.
//!
//! What is simulated with the real kernels, and what is abstracted, is
//! listed in RESULTS.md ("Model").

use std::collections::{BTreeSet, HashMap};

use crate::config::{Config, Emission, LateStake, OfficePay};
use crate::model::*;
use crate::rng::Rng;
use permutation_rules::fixed::{Bps, Milli, MilliTroops, BPS_ONE, MILLI};
use permutation_rules::frontier::clash::{
    frontier_ruleset, ready_bell_after, resolve_clash, ClashInput, Fate, Fighter, Garrison,
    Relations, NEUTRAL,
};
use permutation_rules::frontier::geometry::{
    march_of, provinces_within, ring_provinces, ProvinceCoord, PROVINCE_TILES,
};
use permutation_rules::frontier::holding::{
    duplicate_cost, troop_upkeep_per_hour, Effect, Holding, Resource, Tier as HTier, RESOURCES,
};
use permutation_rules::frontier::host::{
    strength as host_strength, supply_attrition, Stamina, MIN_HOST_TROOPS,
};
use permutation_rules::frontier::index::{FactionFacts, FACTIONS, PATHS};
use permutation_rules::frontier::laurel::{
    capture_transfer, emission_quarters, mandate_reserve_split, occupation_split, relic_credit_at,
    siege_settle, strength_weight, PairHistory, RewardIndex, Stake, Tier as LTier, EMIT_FULL,
    LAUREL_ONE, RELIC_EMISSION_PER_BELL_REV2, SIEGE_STAKE,
};
use permutation_rules::frontier::mandate::{MandateTerm, Reserve, TERM_DAYS};
use permutation_rules::frontier::pools::{Entry, EntrySchedule, Pools};
use permutation_rules::frontier::siege::{
    auto_reinforce, completion, may_besiege, BellReport, Completion, Donor, HoldingKind, Relation,
    Siege, SiegeCheck, SiegeStatus, Vigil,
};
use permutation_rules::frontier::stance::{Posture, Stance};
use permutation_rules::frontier::terrain::{generate_province, ProvinceTerrain};
use permutation_rules::frontier::travel::{
    earliest_arrival_bell, march_stamina, open_ground_secs, BELLS_PER_DAY, BELL_SECS,
};
use permutation_rules::hash::sha256;
use permutation_rules::params::Ruleset;
use permutation_rules::units::UnitType;

pub const NONE: u32 = u32::MAX;
const HOUR_BELLS: u32 = 6;
/// Militia every new holding starts with (troops). [sim]
const START_GARRISON: i64 = 100;
/// Garrison of a Free City (a released first holding). [sim]
const FREE_CITY_GARRISON: i64 = 300;
/// Largest distance (provinces) a march may cover: ≤ 4 provinces touched.
const MAX_MARCH_DIST: u32 = 3;
const CAMP_BIT: u64 = 1 << 40;

pub fn now_of(b: u32) -> i64 {
    b as i64 * BELL_SECS
}

fn ltier(t: HTier) -> LTier {
    match t {
        HTier::Hamlet => LTier::Hamlet,
        HTier::Town => LTier::Town,
        HTier::City => LTier::City,
        HTier::Stronghold => LTier::Stronghold,
    }
}

fn tier_idx(t: HTier) -> usize {
    t as usize
}

fn troops(n: i64) -> MilliTroops {
    (n.max(0) * MILLI) as MilliTroops
}

fn bps_mul(x: i64, b: Bps) -> i64 {
    x * b as i64 / BPS_ONE as i64
}

// ------------------------------------------------------------ state

pub struct Camp {
    pub tile: u8,
    pub troops: MilliTroops,
}

pub struct Prov {
    pub coord: ProvinceCoord,
    pub ring: u32,
    pub wedge: u8,
    pub march: u32,
    pub terrain: Box<ProvinceTerrain>,
    /// Holding per site slot, or `NONE`.
    pub site_h: Vec<u32>,
    pub index: RewardIndex,
    /// Hosts standing in the province (roster from `from_bell`).
    pub stationed: Vec<u32>,
    pub camp: Option<Camp>,
    pub relic: Option<u8>,
}

impl Prov {
    fn site_tile(&self, slot: usize) -> u8 {
        self.terrain.sites[slot]
    }
}

pub struct SiegeRec {
    pub s: Siege,
    pub attacker: u32,
    /// The escrowed stake came from the attacker's counted laurels.
    pub stake_counted: bool,
    pub host: u32,
    pub kind: HoldingKind,
}

pub struct Hold {
    pub owner: u32,
    pub faction: u8,
    pub prov: u32,
    pub tile: u8,
    pub h: Holding,
    pub garrison: MilliTroops,
    /// Troops of this holding's hosts that are away (they eat here).
    pub away: MilliTroops,
    pub buildings: [u32; 6],
    pub base_tier: HTier,
    pub stake: Stake,
    pub attached: bool,
    pub vigil: Vigil,
    pub siege: Option<SiegeRec>,
    pub occupier: Option<(u32, u32)>,
    /// The occupier's pair history with the owner at the occupation's start.
    pub occ_pair: PairHistory,
    pub posture: Posture,
    pub alive: bool,
    pub explores: u32,
    pub report: Option<(u32, BellReport)>,
}

impl Hold {
    fn free_city(&self) -> bool {
        self.owner == NONE
    }
    fn kind(&self) -> HoldingKind {
        if self.free_city() {
            HoldingKind::FreeCity
        } else if self.h.order <= 1 {
            HoldingKind::First
        } else {
            HoldingKind::Other
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mission {
    Camp,
    Siege(u32),
    Relic,
    Defend(u32),
    Occupy(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HState {
    Marching { arrive: u32 },
    Stationed { from: u32 },
    Returning,
    Dead,
}

pub struct HostS {
    pub owner: u32,
    pub home: u32,
    pub faction: u8,
    pub unit: UnitType,
    pub troops: MilliTroops,
    pub stamina: Stamina,
    pub ready_bell: u32,
    pub state: HState,
    pub prov: u32,
    pub tile: u8,
    pub stance: Stance,
    pub retreat: Option<Bps>,
    pub mission: Mission,
    pub unrevealed: bool,
    pub back_bells: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinState {
    NotYet,
    Pending { since: u32 },
    Settled,
    Withdrawn,
}

pub struct Agent {
    pub arch: Arch,
    pub shade: bool,
    pub faction: u8,
    pub join_bell: u32,
    pub join_day: u32,
    pub fee: u64,
    pub stake: u64,
    /// Adds its stake late (`AddStake` on the last join day).
    pub stake_later: bool,
    pub state: JoinState,
    pub holdings: Vec<u32>,
    /// Banked laurels (base units).
    pub laurels: u64,
    /// Laurels already banked when a late stake was added: never counted
    /// (`CitizenRecord::laurels_at_stake`).
    pub laurels_at_stake: u64,
    pub works: u64,
    pub works_day: u32,
    pub works_today: u64,
    pub active_days: u32,
    pub last_active_day: u32,
    pub pledged: u64,
    pub facts: [u64; PATHS],
    pub minister_terms: u32,
    pub warden_terms: u32,
    pub mandate_term: u32,
    pub tz: u32,
    pub pairs: Vec<(u32, u32)>,
    pub sessions: u32,
    pub pledge_day: u32,
    pub earned: Earned,
}

/// Where an agent's laurels came from (for the report).
#[derive(Clone, Copy, Debug, Default)]
pub struct Earned {
    pub holding: u64,
    pub occupation: u64,
    pub relic: u64,
    pub mandate: u64,
    pub capture_in: u64,
    pub capture_out: u64,
    pub siege_net: i64,
}

/// Counters for the report.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub clashes: u64,
    pub engagements: u64,
    pub clash_errors: u64,
    pub sessions: u64,
    pub sieges_declared: u64,
    pub sieges_completed: u64,
    pub sieges_failed: u64,
    pub occupations: u64,
    pub liberations: u64,
    pub captures: u64,
    pub free_city_captures: u64,
    pub camps_won: u64,
    pub relics_spawned: u64,
    pub relic_minted: u64,
    pub rings_opened: u32,
    pub final_ring: u32,
    pub withdrawn_joins: u64,
    pub releases: u64,
    pub dormancies: u64,
    pub second_holdings: u64,
    pub engine_stage_days: Vec<u32>,
    pub routs: u64,
    pub disarray: u64,
    pub mandate_paid: u64,
    pub mandate_left: u64,
    /// Mandate shares registered (stakers who completed) and completions
    /// by fee-only wallets (no share, O10).
    pub mandate_shares: u64,
    pub mandate_unshared: u64,
    /// Office-terms held: Minister, paid Warden; by bots.
    pub minister_terms: u64,
    pub warden_terms: u64,
    pub bot_office_terms: u64,
    /// World holding emission per day (laurel units), for the stake ramp.
    pub emission_by_day: Vec<u128>,
    pub siege_stake_orphaned: u64,
    /// Siege stakes burned: uncounted (fee-only) stakes, pair rules.
    pub laurels_burned: u64,
    /// Late stakes added (`AddStake` on the last join day).
    pub late_stakes: u64,
    pub starved: u64,
    /// Siege failures: never held within the start window / held then lost.
    pub fail_never_held: u64,
    pub fail_lost: u64,
    /// Of the lost: attacker host destroyed / bounced or withdrew / other.
    pub lost_destroyed: u64,
    pub lost_left: u64,
}

#[derive(Clone, Copy)]
enum Ev {
    Dormancy(u32),
    Release(u32),
    TierDone(u32),
    Return(u32),
}

pub struct Sim {
    pub cfg: Config,
    pub profiles: [Profile; 6],
    pub rng: Rng,
    rules: Ruleset,
    pub sched: EntrySchedule,
    pub end_bell: u32,
    pub doctrine: [Doctrine; 6],
    pub provs: Vec<Option<Prov>>,
    pub open_ring: u32,
    ring_seeds: Vec<[u8; 32]>,
    wedge_open: [u64; 6],
    wedge_used: [u64; 6],
    pub open_sites: u64,
    pub used_sites: u64,
    /// Free sites by ring and wedge: (province index, slot).
    free: Vec<[Vec<(u32, u8)>; 6]>,
    marches: HashMap<(i32, i32), u32>,
    pub holds: Vec<Hold>,
    pub hosts: Vec<HostS>,
    pub agents: Vec<Agent>,
    pub pools: Pools,
    pub paid_in: u64,
    pub withdrawn: u64,
    joins: Vec<Vec<u32>>,
    pending: Vec<u32>,
    arrivals: Vec<Vec<u32>>,
    events: Vec<Vec<Ev>>,
    sessions: Vec<Vec<u32>>,
    stationed_provs: BTreeSet<u32>,
    siege_holds: BTreeSet<u32>,
    /// Mandate reserve per faction (`mandate::Reserve`).
    pub reserve: [Reserve; 6],
    /// The open Mandate term per faction.
    pub mandate: [MandateTerm; 6],
    /// Completers of the open term and their shares.
    term_completers: Vec<Vec<(u32, u64)>>,
    pub engine_total: u64,
    pub engine_stages: u8,
    pub relics: Vec<u32>,
    crisis: [bool; 6],
    pub escrow: u64,
    pub members: [u64; 6],
    pub stats: Stats,
    pub laurel_credited: u128,
    pub laurel_orphan_extra: u128,
}

// ------------------------------------------------------------ setup

impl Sim {
    pub fn new(cfg: &Config) -> Sim {
        let mut rng = Rng::new(cfg.seed);
        let end_bell = cfg.days * BELLS_PER_DAY;
        let table = doctrine_table(cfg.doctrine_set, &cfg.doctrine_tweaks);
        let doctrine: [Doctrine; 6] = core::array::from_fn(|k| {
            if cfg.doctrines {
                table[(k + cfg.doctrine_rotation) % 6]
            } else {
                NEUTRAL_DOCTRINE
            }
        });
        let n_prov = provinces_within(cfg.r_max) as usize;
        let mut profiles = ARCHS.map(profile);
        if let Some(q) = cfg.bot_q {
            profiles[Arch::Bot.idx()].q = q;
        }
        if let Some(x) = cfg.bot_aggression {
            profiles[Arch::Bot.idx()].aggression = x;
        }
        let mut s = Sim {
            cfg: cfg.clone(),
            profiles,
            rng: Rng::fork(cfg.seed, 7),
            rules: frontier_ruleset(),
            sched: EntrySchedule {
                stake_ramp_bps: cfg.stake_ramp_bps,
                ..EntrySchedule::FRONTIER_28
            },
            end_bell,
            doctrine,
            provs: (0..n_prov).map(|_| None).collect(),
            open_ring: 1,
            ring_seeds: Vec::new(),
            wedge_open: [0; 6],
            wedge_used: [0; 6],
            open_sites: 0,
            used_sites: 0,
            free: (0..=cfg.r_max).map(|_| Default::default()).collect(),
            marches: HashMap::new(),
            holds: Vec::new(),
            hosts: Vec::new(),
            agents: Vec::new(),
            pools: Pools::default(),
            paid_in: 0,
            withdrawn: 0,
            joins: vec![Vec::new(); end_bell as usize + 1],
            pending: Vec::new(),
            arrivals: vec![Vec::new(); end_bell as usize + 400],
            events: vec![Vec::new(); end_bell as usize + 400],
            sessions: vec![Vec::new(); end_bell as usize + 1],
            stationed_provs: BTreeSet::new(),
            siege_holds: BTreeSet::new(),
            reserve: [Reserve::default(); 6],
            mandate: [MandateTerm::new(0); 6],
            term_completers: vec![Vec::new(); 6],
            engine_total: 0,
            engine_stages: 0,
            relics: Vec::new(),
            crisis: [false; 6],
            escrow: 0,
            members: [0; 6],
            stats: Stats::default(),
            laurel_credited: 0,
            laurel_orphan_extra: 0,
        };
        for d in 0..=cfg.r_max {
            s.ring_seeds.push(sha256(&[
                b"sim/ring",
                &cfg.seed.to_le_bytes(),
                &d.to_le_bytes(),
            ]));
        }
        for d in 2..=cfg.genesis_rings {
            s.open_ring(d, 0);
        }
        s.make_agents(&mut rng);
        s
    }

    fn make_agents(&mut self, rng: &mut Rng) {
        let cfg = self.cfg.clone();
        let n = cfg.agents;
        let n_bots = (n as f64 * cfg.bot_share).round() as usize;
        let n_shades = (n as u64 * cfg.shade_bps as u64).div_ceil(10_000) as usize;
        let humans = n - n_bots - n_shades;
        let mut archs: Vec<(Arch, bool)> = Vec::with_capacity(n);
        let mix_sum: f64 = cfg.human_mix.iter().sum();
        let mut acc = 0.0;
        let mut given = 0usize;
        for (i, share) in cfg.human_mix.iter().enumerate() {
            acc += share / mix_sum;
            let upto = if i == 4 {
                humans
            } else {
                (acc * humans as f64).round() as usize
            };
            for _ in given..upto {
                archs.push((ARCHS[i], false));
            }
            given = upto;
        }
        archs.extend(std::iter::repeat_n((Arch::Bot, false), n_bots));
        archs.extend(std::iter::repeat_n((Arch::Bot, true), n_shades));
        // Factions by weight, dealt so each archetype is spread in
        // proportion (a shuffled deck per archetype).
        let wsum: u32 = cfg.faction_weights.iter().sum();
        let mut order: Vec<usize> = (0..n).collect();
        for i in (1..n).rev() {
            let j = rng.below(i as u64 + 1) as usize;
            order.swap(i, j);
        }
        let mut by_arch: HashMap<(Arch, bool), u64> = HashMap::new();
        for &i in &order {
            let (arch, shade) = archs[i];
            let c = by_arch.entry((arch, shade)).or_insert(0);
            // Stratified: every faction gets the same archetype mix. Random:
            // each wallet picks by weight (composition varies by chance).
            let slot = if cfg.stratified {
                *c % wsum as u64
            } else {
                rng.below(wsum as u64)
            };
            *c += 1;
            let mut f = 0u8;
            let mut run = 0u64;
            for (k, w) in cfg.faction_weights.iter().enumerate() {
                run += *w as u64;
                if slot < run {
                    f = k as u8;
                    break;
                }
            }
            let day = if rng.chance(cfg.day0_share) {
                0
            } else {
                1 + rng.below(21) as u32
            };
            let join_bell = day * BELLS_PER_DAY + rng.below(BELLS_PER_DAY as u64) as u32;
            let stake_p = cfg.stake_optin[arch.idx()];
            let fee = self.sched.citizen_fee(day).unwrap_or(0);
            let wants_stake = rng.chance(stake_p);
            let stake_later = wants_stake
                && day < self.sched.last_join_day
                && match cfg.late_stake {
                    LateStake::None => false,
                    LateStake::Bots => arch == Arch::Bot,
                    LateStake::Stakers => true,
                };
            let stake = if wants_stake && !stake_later {
                self.sched.laurel_stake(day).unwrap_or(0)
            } else {
                0
            };
            let id = self.agents.len() as u32;
            self.agents.push(Agent {
                arch,
                shade,
                faction: f,
                join_bell,
                join_day: day,
                fee,
                stake,
                stake_later,
                state: JoinState::NotYet,
                holdings: Vec::new(),
                laurels: 0,
                laurels_at_stake: 0,
                works: 0,
                works_day: NONE,
                works_today: 0,
                active_days: 0,
                last_active_day: NONE,
                pledged: 0,
                facts: [0; PATHS],
                minister_terms: 0,
                warden_terms: 0,
                mandate_term: NONE,
                tz: rng.below(BELLS_PER_DAY as u64) as u32,
                pairs: Vec::new(),
                sessions: 0,
                pledge_day: NONE,
                earned: Earned::default(),
            });
            self.joins[join_bell as usize].push(id);
        }
    }

    // -------------------------------------------------------- map

    fn open_ring(&mut self, d: u32, bell: u32) {
        if d > self.cfg.r_max || d <= self.open_ring {
            return;
        }
        let seed = self.ring_seeds[d as usize];
        for p in ring_provinces(d) {
            let terrain = Box::new(generate_province(&seed, p));
            let wedge = p.wedge().unwrap_or(0);
            let mc = march_of(p);
            let nm = self.marches.len() as u32;
            let march = *self.marches.entry((mc.m, mc.n)).or_insert(nm);
            let n_sites = terrain.site_count as usize;
            let idx = p.index();
            let mut pr = Prov {
                coord: p,
                ring: d,
                wedge,
                march,
                terrain,
                site_h: vec![NONE; n_sites],
                index: RewardIndex::new(bell as u64),
                stationed: Vec::new(),
                camp: None,
                relic: None,
            };
            pr.camp = None;
            for slot in 0..n_sites {
                self.free[d as usize][wedge as usize].push((idx, slot as u8));
            }
            self.wedge_open[wedge as usize] += n_sites as u64;
            self.open_sites += n_sites as u64;
            self.provs[idx as usize] = Some(pr);
        }
        self.open_ring = d;
        self.stats.rings_opened += 1;
        self.stats.final_ring = d;
    }

    /// Design §3.4: open ring d+1 when some wedge's occupied sites reach
    /// θ of its open sites (55% for the first 72 hours, 65% after).
    fn maybe_open_ring(&mut self, b: u32) {
        let theta_pct = if b < 72 * 6 { 55 } else { 65 };
        let crowded = (0..6).any(|k| self.wedge_used[k] * 100 >= theta_pct * self.wedge_open[k]);
        if crowded && self.open_ring < self.cfg.r_max {
            let d = self.open_ring + 1;
            self.open_ring(d, b);
        }
    }

    pub fn prov(&self, i: u32) -> &Prov {
        self.provs[i as usize].as_ref().expect("open province")
    }
    fn prov_mut(&mut self, i: u32) -> &mut Prov {
        self.provs[i as usize].as_mut().expect("open province")
    }
    fn prov_at(&self, c: ProvinceCoord) -> Option<u32> {
        if c.ring() > self.open_ring || c.ring() < 2 {
            return None;
        }
        let i = c.index();
        self.provs.get(i as usize)?.as_ref().map(|_| i)
    }

    /// Open provinces within `r` of `c` (rings ≥ 2), nearest first.
    fn provinces_near(&self, c: ProvinceCoord, r: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let r = r as i32;
        for dq in -r..=r {
            for dr in (-r).max(-dq - r)..=r.min(-dq + r) {
                let p = ProvinceCoord::new(c.p + dq, c.q + dr);
                if let Some(i) = self.prov_at(p) {
                    out.push((c.distance(p), i));
                }
            }
        }
        out.sort_unstable();
        out.into_iter().map(|x| x.1).collect()
    }

    fn take_site(&mut self, pi: u32, slot: u8) {
        let (ring, wedge) = {
            let p = self.prov(pi);
            (p.ring as usize, p.wedge as usize)
        };
        let v = &mut self.free[ring][wedge];
        if let Some(pos) = v.iter().position(|&x| x == (pi, slot)) {
            v.swap_remove(pos);
        }
        self.wedge_used[wedge] += 1;
        self.used_sites += 1;
    }

    /// First-holding placement (design §2.2 step 4, §2.6): own wedge; day-0
    /// joiners take the innermost ring with room, later joiners the
    /// outermost (a frontier cohort); overflow to the adjacent wedges.
    fn place_first(&mut self, faction: u8, early: bool) -> Option<(u32, u8)> {
        let wedges = [faction, (faction + 1) % 6, (faction + 5) % 6];
        for (wi, &w) in wedges.iter().enumerate() {
            let rings: Vec<u32> = if early && wi == 0 {
                (2..=self.open_ring).collect()
            } else {
                (2..=self.open_ring).rev().collect()
            };
            for d in rings {
                let v = &self.free[d as usize][w as usize];
                if !v.is_empty() {
                    let k = self.rng.below(v.len() as u64) as usize;
                    return Some(v[k]);
                }
            }
        }
        None
    }

    // -------------------------------------------------------- holdings

    fn doct(&self, f: u8) -> &Doctrine {
        &self.doctrine[f as usize % 6]
    }

    fn base_prod(&self, faction: u8, t: HTier) -> [i64; RESOURCES] {
        let d = self.doct(faction);
        core::array::from_fn(|r| {
            let mut x = BASE_PROD[r] * (100 + tier_bonus_pct(t)) / 100 * MILLI;
            if r == Resource::Food as usize {
                x = bps_mul(x, d.food_bps);
            }
            if r == Resource::Ore as usize {
                x = bps_mul(x, d.ore_bps);
            }
            if r == Resource::Science as usize {
                x = bps_mul(x, d.science_bps);
            }
            x
        })
    }

    fn found(&mut self, agent: u32, pi: u32, slot: u8, b: u32, kit_mult: i64) -> u32 {
        let now = now_of(b);
        let a = &self.agents[agent as usize];
        let faction = a.faction;
        let order = a.holdings.len() as u8 + 1;
        let tz = a.tz;
        let mut h = Holding::found(now, b / BELLS_PER_DAY, order);
        let base = self.base_prod(faction, HTier::Hamlet);
        h.production = base;
        let _ = h.set_upkeep(now, Resource::Food, 0);
        for (r, &k) in STARTER_KIT.iter().enumerate() {
            if k > 0 && kit_mult > 0 {
                let _ = h.credit(now, Resource::ALL[r], k * kit_mult / 100 * MILLI);
            }
        }
        let tile = self.prov(pi).site_tile(slot as usize);
        let hid = self.holds.len() as u32;
        self.holds.push(Hold {
            owner: agent,
            faction,
            prov: pi,
            tile,
            h,
            garrison: troops(START_GARRISON),
            away: 0,
            buildings: [0; 6],
            base_tier: HTier::Hamlet,
            stake: Stake::default(),
            attached: false,
            vigil: Vigil::new(tz * BELL_SECS as u32).expect("vigil"),
            siege: None,
            occupier: None,
            occ_pair: PairHistory::default(),
            posture: Posture::default(),
            alive: true,
            explores: 0,
            report: None,
        });
        self.prov_mut(pi).site_h[slot as usize] = hid;
        self.take_site(pi, slot);
        self.agents[agent as usize].holdings.push(hid);
        self.refresh_upkeep(hid, b);
        self.reweigh(hid, b);
        self.schedule_dormancy(hid, b);
        hid
    }

    fn schedule_dormancy(&mut self, hid: u32, b: u32) {
        let h = &self.holds[hid as usize].h;
        let at = (h.dormant_at() / BELL_SECS) as u32;
        let rel = ((h.last_owner_action + permutation_rules::frontier::holding::RELEASE_AFTER)
            / BELL_SECS) as u32;
        if at > b && (at as usize) < self.events.len() {
            self.events[at as usize].push(Ev::Dormancy(hid));
        }
        if h.order == 1 && rel > b && (rel as usize) < self.events.len() {
            self.events[rel as usize].push(Ev::Release(hid));
        }
    }

    /// Strength weight, whether the holding is live (non-dormant), and its
    /// emission quarters: the order factor (K3 default), 4 for every holding
    /// (revision 2) or 4 for first holdings only (variant); 0 while occupied.
    fn weight_of(&self, hid: u32, b: u32) -> (u64, bool, u8) {
        let x = &self.holds[hid as usize];
        let now = now_of(b);
        let dormant = x.free_city() || x.h.is_dormant(now);
        let order = x.h.order.saturating_sub(1);
        let w = strength_weight(ltier(x.h.tier), x.garrison, order);
        let q = if x.occupier.is_some() {
            0
        } else {
            match self.cfg.emission {
                Emission::Full => EMIT_FULL,
                Emission::FirstOnly => {
                    if order == 0 {
                        EMIT_FULL
                    } else {
                        0
                    }
                }
                Emission::OrderWeighted => emission_quarters(order),
            }
        };
        (w, !dormant, q)
    }

    /// Accrue a province's index to bell `b` (`RewardIndex::accrue`).
    fn accrue(&mut self, pi: u32, b: u32) {
        let end = self.end_bell as u64;
        let p = self.provs[pi as usize].as_mut().expect("prov");
        p.index.accrue(b as u64, end).expect("accrue");
    }

    /// Bring the holding's stake in its province's index to its current
    /// weight (accruing the index first), crediting what it earned.
    fn reweigh(&mut self, hid: u32, b: u32) {
        let pi = self.holds[hid as usize].prov;
        let (w, live, q) = self.weight_of(hid, b);
        self.accrue(pi, b);
        let x = &mut self.holds[hid as usize];
        let p = self.provs[pi as usize].as_mut().expect("prov");
        let credit = if x.attached {
            if live && x.alive {
                p.index.reweigh(&mut x.stake, w, q).expect("reweigh")
            } else {
                x.attached = false;
                p.index.detach(&x.stake).expect("detach")
            }
        } else if live && x.alive {
            x.stake = p.index.attach(w, q).expect("attach");
            x.attached = true;
            0
        } else {
            0
        };
        self.distribute(hid, credit);
    }

    /// Credit a holding's laurels: 10% to the faction's Mandate reserve,
    /// then 50% of the rest to an occupier (design §4.2, §6.3).
    fn distribute(&mut self, hid: u32, credit: u64) {
        if credit == 0 {
            return;
        }
        self.laurel_credited += credit as u128;
        let x = &self.holds[hid as usize];
        if x.free_city() {
            self.laurel_orphan_extra += credit as u128;
            return;
        }
        let (owner, occ, faction, pair) = (x.owner, x.occupier, x.faction, x.occ_pair);
        let m = mandate_reserve_split(credit);
        self.reserve[faction as usize]
            .deposit(m.give)
            .expect("reserve");
        match occ {
            Some((o, _)) => {
                let owner_staker = self.agents[owner as usize].stake > 0;
                let p = occupation_split(m.keep, owner_staker, pair);
                self.agents[owner as usize].laurels += p.keep;
                self.agents[owner as usize].earned.holding += p.keep;
                self.agents[o as usize].laurels += p.give;
                self.agents[o as usize].earned.occupation += p.give;
            }
            None => {
                self.agents[owner as usize].laurels += m.keep;
                self.agents[owner as usize].earned.holding += m.keep;
            }
        }
    }

    fn refresh_upkeep(&mut self, hid: u32, b: u32) {
        let now = now_of(b);
        let x = &self.holds[hid as usize];
        if x.free_city() {
            return;
        }
        let d = self.doctrine[x.faction as usize];
        let per = troop_upkeep_per_hour(&[(UnitType::Spearman, x.garrison), (d.k.unit, x.away)]);
        let mut per = d.k.upkeep(per);
        if self.crisis[x.faction as usize] {
            per = per * 11 / 10;
        }
        let x = &mut self.holds[hid as usize];
        let _ = x.h.set_upkeep(now, Resource::Food, per);
    }

    /// Owner touch at bell b: settle, re-activate, apply a finished
    /// tier's production, starvation, and re-attach if dormant.
    fn touch(&mut self, hid: u32, b: u32) {
        let now = now_of(b);
        let was_dormant = self.holds[hid as usize].h.is_dormant(now);
        {
            let x = &mut self.holds[hid as usize];
            let short0 = x.h.food_shortfall;
            let _ = x.h.touch_owner(now);
            if x.h.food_shortfall > short0 && x.garrison > MIN_HOST_TROOPS {
                x.garrison = x.garrison * 9 / 10;
                self.stats.starved += 1;
            }
        }
        self.apply_tier_bonus(hid, b);
        let _ = was_dormant;
        self.schedule_dormancy(hid, b);
        self.refresh_upkeep(hid, b);
        self.reweigh(hid, b);
    }

    fn apply_tier_bonus(&mut self, hid: u32, b: u32) {
        let now = now_of(b);
        let (faction, tier, base_tier) = {
            let x = &mut self.holds[hid as usize];
            let _ = x.h.settle(now);
            (x.faction, x.h.tier, x.base_tier)
        };
        if tier != base_tier {
            let old = self.base_prod(faction, base_tier);
            let new = self.base_prod(faction, tier);
            let x = &mut self.holds[hid as usize];
            for r in 0..RESOURCES {
                x.h.production[r] += new[r] - old[r];
            }
            x.base_tier = tier;
            let up = x.h.upkeep[Resource::Food as usize];
            let _ = x.h.set_upkeep(now, Resource::Food, up);
        }
    }

    // -------------------------------------------------------- step

    pub fn step(&mut self, b: u32) {
        if b % BELLS_PER_DAY == 0 {
            self.new_day(b / BELLS_PER_DAY, b);
        }
        if b == self.sched.last_join_day * BELLS_PER_DAY {
            self.late_stakes(b);
        }
        let evs = std::mem::take(&mut self.events[b as usize]);
        for e in evs {
            self.event(e, b);
        }
        let joins = std::mem::take(&mut self.joins[b as usize]);
        for a in joins {
            self.join(a, b);
        }
        self.retry_pending(b);
        self.maybe_open_ring(b);
        let ss = std::mem::take(&mut self.sessions[b as usize]);
        for a in ss {
            self.session(a, b);
        }
        self.engine_check(b);
        self.resolve_bell(b);
        self.relic_credit(b);
        if b % HOUR_BELLS == HOUR_BELLS - 1 {
            self.hourly_fold(b);
        }
    }

    /// `AddStake` on the last join day for the wallets that planned it:
    /// bank every holding it owns or occupies first, pay the day's price,
    /// and never count the laurels banked so far (`laurels_at_stake`).
    fn late_stakes(&mut self, b: u32) {
        let price = match self.sched.laurel_stake(self.sched.last_join_day) {
            Some(p) => p,
            None => return,
        };
        let who: Vec<u32> = (0..self.agents.len() as u32)
            .filter(|&a| {
                let ag = &self.agents[a as usize];
                ag.stake_later && ag.stake == 0 && ag.state == JoinState::Settled
            })
            .collect();
        if who.is_empty() {
            return;
        }
        let mut occupied: HashMap<u32, Vec<u32>> = HashMap::new();
        for (i, x) in self.holds.iter().enumerate() {
            if let Some((o, _)) = x.occupier {
                occupied.entry(o).or_default().push(i as u32);
            }
        }
        for a in who {
            let mut banks = self.agents[a as usize].holdings.clone();
            banks.extend(occupied.get(&a).cloned().unwrap_or_default());
            for hid in banks {
                if self.holds[hid as usize].alive {
                    self.reweigh(hid, b);
                }
            }
            self.pools
                .pay_settled(&self.sched, Entry::LaurelStake, price)
                .expect("add stake");
            self.paid_in += price;
            let ag = &mut self.agents[a as usize];
            ag.stake = price;
            ag.laurels_at_stake = ag.laurels;
            self.stats.late_stakes += 1;
        }
    }

    /// Counted laurels (`CitizenRecord::counted_laurels`): banked while a
    /// staker; the only ones a wallet can transfer.
    fn counted(&self, a: u32) -> u64 {
        let ag = &self.agents[a as usize];
        if ag.stake > 0 {
            ag.laurels.saturating_sub(ag.laurels_at_stake)
        } else {
            0
        }
    }

    /// Keep `laurels_at_stake` in step when a siege stake of uncounted
    /// laurels is escrowed (`out`) or returned.
    fn escrow_uncounted(&mut self, a: u32, uncounted: bool, out: bool) {
        let ag = &mut self.agents[a as usize];
        if !uncounted || ag.stake == 0 {
            return;
        }
        if out {
            ag.laurels_at_stake = ag.laurels_at_stake.saturating_sub(SIEGE_STAKE);
        } else {
            ag.laurels_at_stake += SIEGE_STAKE;
        }
    }

    fn pair_history(&self, a: u32, b: u32) -> PairHistory {
        PairHistory {
            prior_captures: self.agents[a as usize]
                .pairs
                .iter()
                .find(|x| x.0 == b)
                .map_or(0, |x| x.1),
            other_ties: false,
        }
    }

    /// Count a laurel transfer between a pair (captures count themselves).
    fn note_pair(&mut self, a: u32, b: u32) {
        for (x, y) in [(a, b), (b, a)] {
            let v = &mut self.agents[x as usize].pairs;
            match v.iter_mut().find(|p| p.0 == y) {
                Some(p) => p.1 += 1,
                None => v.push((y, 1)),
            }
        }
    }

    fn event(&mut self, e: Ev, b: u32) {
        match e {
            Ev::Dormancy(hid) => {
                let x = &self.holds[hid as usize];
                if x.alive && !x.free_city() && x.h.is_dormant(now_of(b)) && x.attached {
                    self.stats.dormancies += 1;
                    self.reweigh(hid, b);
                }
            }
            Ev::Release(hid) => {
                let x = &self.holds[hid as usize];
                if x.alive && !x.free_city() && x.h.is_released(now_of(b)) {
                    self.release(hid, b);
                }
            }
            Ev::TierDone(hid) => {
                if self.holds[hid as usize].alive {
                    self.apply_tier_bonus(hid, b);
                    self.reweigh(hid, b);
                }
            }
            Ev::Return(host) => self.host_home(host, b),
        }
    }

    /// A first holding with no owner action for 10 days becomes a Free
    /// City site (design §3.4); the owner keeps citizenship.
    fn release(&mut self, hid: u32, b: u32) {
        self.stats.releases += 1;
        let x = &mut self.holds[hid as usize];
        let owner = x.owner;
        x.alive = false;
        self.reweigh(hid, b); // detaches, final credit to the owner
        let x = &mut self.holds[hid as usize];
        if let Some((_, host)) = x.occupier.take() {
            self.send_home(host, b);
        }
        let x = &mut self.holds[hid as usize];
        x.owner = NONE;
        x.faction = NEUTRAL;
        x.alive = true;
        x.garrison = troops(FREE_CITY_GARRISON);
        x.away = 0;
        self.agents[owner as usize].holdings.retain(|&h| h != hid);
    }

    fn new_day(&mut self, day: u32, b: u32) {
        if day > 0 && day % TERM_DAYS == 0 {
            self.term_end(day / TERM_DAYS - 1);
        }
        // World holding emission per bell at the day's start (stake ramp).
        let per_bell: u128 = self
            .provs
            .iter()
            .flatten()
            .map(|p| p.index.emission_per_bell() as u128)
            .sum();
        self.stats
            .emission_by_day
            .push(per_bell * BELLS_PER_DAY as u128);
        // Crisis (V5): from day 14 the top two factions by index pay +10%.
        if day >= 14 {
            let facts = self.faction_facts(false, day);
            let s = permutation_rules::frontier::index::faction_index(&facts, &self.cfg.index);
            let mut order: Vec<usize> = (0..6).collect();
            order.sort_by_key(|&k| std::cmp::Reverse(s[k]));
            self.crisis = [false; 6];
            self.crisis[order[0]] = true;
            self.crisis[order[1]] = true;
        }
        // Barbarian camps respawn: half the settled provinces get one.
        let idxs: Vec<u32> = self
            .provs
            .iter()
            .enumerate()
            .filter_map(|(i, p)| p.as_ref().map(|_| i as u32))
            .collect();
        for i in idxs {
            let has_holding = self.prov(i).site_h.iter().any(|&h| h != NONE);
            if !has_holding || !self.rng.chance(0.5) {
                continue;
            }
            let tile = {
                let p = self.prov(i);
                let mut cands: Vec<u8> = (1..PROVINCE_TILES as u8)
                    .filter(|&t| {
                        p.terrain.passable(t) && !p.terrain.is_site(t) && p.relic != Some(t)
                    })
                    .collect();
                cands.sort_unstable();
                let k = self.rng.below(cands.len().max(1) as u64) as usize;
                cands.get(k).copied()
            };
            if let Some(tile) = tile {
                let t = 100 + self.rng.below(301) as i64;
                self.prov_mut(i).camp = Some(Camp {
                    tile,
                    troops: troops(t),
                });
            }
        }
        // Sessions for the day.
        for a in 0..self.agents.len() as u32 {
            let ag = &self.agents[a as usize];
            if ag.state != JoinState::Settled || ag.holdings.is_empty() && ag.arch == Arch::Idle {
                continue;
            }
            let p = self.profiles[ag.arch.idx()];
            if p.sessions == 0 || !self.rng.chance(p.day_p) {
                continue;
            }
            let tz = ag.tz;
            let arch = ag.arch;
            let mut bells: Vec<u32> = Vec::new();
            if arch == Arch::Bot {
                for k in 0..p.sessions {
                    bells.push((tz % HOUR_BELLS) + HOUR_BELLS * k);
                }
            } else {
                // Waking hours: local 08:00–24:00 (the vigil is 00:00–08:00).
                for _ in 0..p.sessions {
                    let off = 48 + self.rng.below(96) as u32;
                    bells.push((tz + off) % BELLS_PER_DAY);
                }
            }
            for bb in bells {
                let at = b + bb;
                if at < self.end_bell && at >= self.agents[a as usize].join_bell {
                    self.sessions[at as usize].push(a);
                }
            }
        }
    }

    // -------------------------------------------------------- joining

    fn join(&mut self, a: u32, b: u32) {
        let (fee, stake) = {
            let ag = &self.agents[a as usize];
            (ag.fee, ag.stake)
        };
        self.pools.pay_pending(fee + stake).expect("pay");
        self.paid_in += fee + stake;
        self.agents[a as usize].state = JoinState::Pending { since: b };
        let f = self.agents[a as usize].faction;
        self.members[f as usize] += 1;
        if !self.try_settle(a, b) {
            self.pending.push(a);
        }
    }

    fn retry_pending(&mut self, b: u32) {
        if self.pending.is_empty() {
            return;
        }
        let pend = std::mem::take(&mut self.pending);
        for a in pend {
            if self.try_settle(a, b) {
                continue;
            }
            let JoinState::Pending { since } = self.agents[a as usize].state else {
                continue;
            };
            if b >= since + BELLS_PER_DAY {
                // WithdrawJoin: no site within 24 hours, 100% back.
                let ag = &mut self.agents[a as usize];
                let amt = ag.fee + ag.stake;
                self.pools.withdraw_pending(amt).expect("withdraw");
                self.withdrawn += amt;
                ag.state = JoinState::Withdrawn;
                self.members[ag.faction as usize] -= 1;
                self.stats.withdrawn_joins += 1;
            } else {
                self.pending.push(a);
            }
        }
    }

    fn try_settle(&mut self, a: u32, b: u32) -> bool {
        let (faction, day, early) = {
            let ag = &self.agents[a as usize];
            (ag.faction, ag.join_day, ag.join_day == 0)
        };
        let Some((pi, slot)) = self.place_first(faction, early) else {
            return false;
        };
        // Catch-up kit for late joiners (capped at 3× the day-0 kit) and
        // the Frontier Grant (+50%) for a faction below the average size.
        let mut kit = 100 * (1 + day as i64 / 7).min(3);
        let avg = self.members.iter().sum::<u64>() / 6;
        if self.members[faction as usize] < avg {
            kit += 50 * self.doct(faction).grant_mult;
        }
        self.found(a, pi, slot, b, kit);
        let (fee, stake) = {
            let ag = &mut self.agents[a as usize];
            ag.state = JoinState::Settled;
            (ag.fee, ag.stake)
        };
        self.pools
            .release(&self.sched, Entry::CitizenFee, fee)
            .expect("release fee");
        if stake > 0 {
            self.pools
                .release(&self.sched, Entry::LaurelStake, stake)
                .expect("release stake");
        }
        if self.agents[a as usize].arch != Arch::Idle {
            self.session(a, b);
        }
        true
    }

    // -------------------------------------------------------- sessions

    fn mark_active(&mut self, a: u32, b: u32) {
        let day = b / BELLS_PER_DAY;
        let ag = &mut self.agents[a as usize];
        if ag.last_active_day != day {
            ag.last_active_day = day;
            ag.active_days += 1;
        }
        ag.sessions += 1;
    }

    fn add_works(&mut self, a: u32, b: u32, w: u64) {
        let day = b / BELLS_PER_DAY;
        let ag = &mut self.agents[a as usize];
        if ag.works_day != day {
            ag.works_day = day;
            ag.works_today = 0;
        }
        let room = self.cfg.works_cap.saturating_sub(ag.works_today);
        let got = w.min(room);
        ag.works_today += got;
        ag.works += got;
    }

    fn session(&mut self, a: u32, b: u32) {
        if self.agents[a as usize].state != JoinState::Settled {
            return;
        }
        self.stats.sessions += 1;
        self.mark_active(a, b);
        let arch = self.agents[a as usize].arch;
        let p = self.profiles[arch.idx()];
        let mut budget = p.actions as i64;
        if self.agents[a as usize].holdings.is_empty() {
            // Back after a release: re-found with a refugee kit (§3.4).
            let f = self.agents[a as usize].faction;
            match self.place_first(f, false) {
                Some((pi, slot)) => {
                    self.found(a, pi, slot, b, 100);
                }
                None => return,
            }
        }
        let hs = self.agents[a as usize].holdings.clone();
        for &hid in &hs {
            self.touch(hid, b);
        }
        // Defence first: postures, defenders, liberation.
        for &hid in &hs {
            budget -= self.defend(a, hid, b, &p);
        }
        // Economy.
        for &hid in &hs {
            if budget <= 0 {
                break;
            }
            budget -= self.economy(a, hid, b, &p);
        }
        if budget > 0 {
            budget -= self.expand(a, b, &p);
        }
        let day = b / BELLS_PER_DAY;
        if budget > 0 && self.agents[a as usize].pledge_day != day {
            self.agents[a as usize].pledge_day = day;
            budget -= self.pledge(a, b, &p);
        }
        // Mandate of the term.
        let term = b / BELLS_PER_DAY / TERM_DAYS;
        if budget > 2 && self.agents[a as usize].mandate_term != term && self.rng.chance(p.mandate)
        {
            let f = self.agents[a as usize].faction as usize;
            self.agents[a as usize].mandate_term = term;
            // O10: a share only for a wallet that is a staker when it
            // completes (the M0 variant registers everyone).
            let staker = self.agents[a as usize].stake > 0 || !self.cfg.mandate_stakers_only;
            let sh = self.mandate[f].complete(staker).expect("mandate");
            if sh > 0 {
                self.term_completers[f].push((a, sh));
                self.stats.mandate_shares += sh;
            } else {
                self.stats.mandate_unshared += 1;
            }
            self.add_works(a, b, WORKS_MANDATE);
            budget -= 3;
        }
        // Explore for Works (the skilled stop at the daily cap).
        let explores = if arch == Arch::Bot {
            4
        } else {
            1 + (p.q * 2.0) as i64
        };
        let capped = |s: &Sim| {
            let ag = &s.agents[a as usize];
            ag.works_day == b / BELLS_PER_DAY && ag.works_today >= s.cfg.works_cap
        };
        let knows_cap = self.rng.chance(p.q);
        for _ in 0..explores {
            if budget <= 0 || knows_cap && capped(self) {
                break;
            }
            let hid = hs[0];
            let first = self.holds[hid as usize].explores < 3;
            self.holds[hid as usize].explores += 1;
            if first || self.rng.chance(0.5) {
                self.add_works(a, b, WORKS_EXPLORE);
            }
            budget -= 1;
        }
        // Military.
        if budget > 2 && self.rng.chance(p.aggression) {
            budget -= self.war(a, b, &p);
        }
        if budget > 2 && !(knows_cap && capped(self)) && self.rng.chance(p.aggression) {
            budget -= self.camp_raid(a, b, &p);
        }
        if budget > 2 && !self.relics.is_empty() && self.rng.chance(p.aggression) {
            self.relic_move(a, b, &p);
        }
        // Bank everything (a touch credits the reward index).
        let hs = self.agents[a as usize].holdings.clone();
        for &hid in &hs {
            self.reweigh(hid, b);
        }
    }

    /// Build, tier up, walls, garrison. Returns actions used.
    fn economy(&mut self, a: u32, hid: u32, b: u32, p: &Profile) -> i64 {
        let now = now_of(b);
        // The sim resolves every bell promptly, so walls finished before
        // this bell have been read by its clashes and can be committed.
        self.holds[hid as usize].h.commit_walls(now);
        let mut used = 0i64;
        let faction = self.holds[hid as usize].faction;
        let d = self.doctrine[faction as usize];
        let q = p.q;
        // Fill the queue.
        for _ in 0..4 {
            let x = &self.holds[hid as usize];
            let slots = x.h.tier.queue_slots();
            let busy = x.h.queue.iter().filter(|q| q.is_some()).count();
            if busy >= slots {
                break;
            }
            let stock = x.h.stock_at(now);
            let tier = x.h.tier;
            let prod = x.h.production;
            let food_tight = x.h.upkeep[0] * 2 > x.h.production[0];
            let tier_queued =
                x.h.queue
                    .iter()
                    .flatten()
                    .any(|i| matches!(i.effect, Effect::TierUp));
            // Tier up when affordable.
            if let (Some((cost, secs)), false) = (tier_up(tier), tier_queued) {
                let c: [Milli; RESOURCES] = core::array::from_fn(|r| cost[r] * MILLI);
                if (0..RESOURCES).all(|r| stock[r] >= c[r]) {
                    let x = &mut self.holds[hid as usize];
                    if x.h.pay(now, &c).is_ok() {
                        if let Ok(done) = x.h.enqueue(now, secs, Effect::TierUp) {
                            let at = (done / BELL_SECS) as usize + 1;
                            if at < self.events.len() {
                                self.events[at].push(Ev::TierDone(hid));
                            }
                        }
                        used += 1;
                        continue;
                    }
                }
            }
            // A building: the skilled pick what the next tier needs most.
            let k = if self.rng.chance(q) {
                let need = tier_up(tier)
                    .map(|x| x.0)
                    .unwrap_or([0, 1, 1, 0, 0, 1, 0, 0]);
                let mut best = 1usize;
                let mut best_v = f64::MIN;
                for (bi, bd) in BUILDINGS.iter().enumerate().take(5) {
                    let r = bd.resource as usize;
                    let v = (need[r] as f64 + 50.0) / (prod[r] as f64 / MILLI as f64 + 1.0);
                    if v > best_v {
                        best_v = v;
                        best = bi;
                    }
                }
                // keep food ahead of upkeep
                if food_tight {
                    0
                } else {
                    best
                }
            } else {
                self.rng.below(BUILDINGS.len() as u64) as usize
            };
            let bd = BUILDINGS[k];
            let n = self.holds[hid as usize].buildings[k] + 1;
            let c: [Milli; RESOURCES] =
                core::array::from_fn(|r| duplicate_cost(bd.cost[r] as u64, n) as i64 * MILLI);
            // A careful player (probability q) buys a building only if it
            // pays back within 3 days and leaves the next tier's cost.
            if self.rng.chance(p.thrift) {
                let total: i64 = c.iter().sum::<i64>() / MILLI;
                let payback = bd.per_hour * 72;
                let reserve = tier_up(tier).map(|x| x.0).unwrap_or([0; RESOURCES]);
                let keeps =
                    (0..RESOURCES).all(|r| stock[r] - c[r] >= reserve[r] * MILLI || c[r] == 0);
                if total > payback || !keeps && !tier_queued {
                    break;
                }
            }
            let mut delta = bd.per_hour * MILLI;
            if bd.resource == Resource::Food {
                delta = bps_mul(delta, d.food_bps);
            }
            if bd.resource == Resource::Ore {
                delta = bps_mul(delta, d.ore_bps);
            }
            if bd.resource == Resource::Science {
                delta = bps_mul(delta, d.science_bps);
            }
            let x = &mut self.holds[hid as usize];
            if x.h.pay(now, &c).is_err() {
                break;
            }
            let _ = x.h.enqueue(
                now,
                build_secs(n),
                Effect::Production {
                    resource: bd.resource,
                    delta,
                },
            );
            x.buildings[k] += 1;
            used += 1;
        }
        // Walls from Town up.
        {
            let x = &self.holds[hid as usize];
            let tier = x.h.tier;
            let want = WALL_STEP * tier_idx(tier) as u32;
            let stock = x.h.stock_at(now);
            let cost = d.k.wall_cost(WALL_COST_STONE) * MILLI;
            let reserve = tier_up(tier).map(|c| c.0[2] * MILLI).unwrap_or(0);
            let slots = x.h.tier.queue_slots();
            let busy = x.h.queue.iter().filter(|q| q.is_some()).count();
            if x.h.walls_at(now).unwrap_or(x.h.walls) < want
                && busy < slots
                && stock[2] >= cost + reserve * (q > 0.5) as i64
                && self.rng.chance(q)
            {
                let mut c = [0; RESOURCES];
                c[2] = cost;
                let x = &mut self.holds[hid as usize];
                if x.h.pay(now, &c).is_ok() {
                    let _ =
                        x.h.enqueue(now, 4 * 3_600, Effect::Walls { delta: WALL_STEP });
                    used += 1;
                }
            }
        }
        // Garrison toward the tier's target, keeping a reserve for the
        // next tier if skilled.
        {
            let x = &self.holds[hid as usize];
            let target = troops(garrison_target(x.h.tier));
            let have = x.garrison + x.away;
            if have < target {
                let stock = x.h.stock_at(now);
                let reserve_food = if q > 0.5 { 200 * MILLI } else { 0 };
                let per100 = TROOP_COST_PER_100;
                let afford = [
                    (stock[0] - reserve_food) / (per100[0] * MILLI),
                    stock[3] / (per100[1] * MILLI),
                    stock[5] / (per100[2] * MILLI),
                ]
                .into_iter()
                .min()
                .unwrap_or(0)
                .max(0);
                let want = ((target - have) as i64 / (100 * MILLI)).min(afford);
                if want > 0 {
                    let c: [Milli; RESOURCES] = [
                        want * per100[0] * MILLI,
                        0,
                        0,
                        want * per100[1] * MILLI,
                        0,
                        want * per100[2] * MILLI,
                        0,
                        0,
                    ];
                    let x = &mut self.holds[hid as usize];
                    if x.h.pay(now, &c).is_ok() {
                        x.garrison += troops(want * 100);
                        used += 1;
                        self.refresh_upkeep(hid, b);
                        self.reweigh(hid, b);
                    }
                }
            }
        }
        let _ = a;
        used
    }

    /// Found a second or third holding near the first (design §3.4 gate:
    /// only while free sites are ≥ 20% of open sites).
    fn expand(&mut self, a: u32, b: u32, p: &Profile) -> i64 {
        let ag = &self.agents[a as usize];
        if ag.holdings.len() >= 3 || ag.holdings.is_empty() {
            return 0;
        }
        if (self.open_sites - self.used_sites) * 5 < self.open_sites {
            return 0;
        }
        let first = ag.holdings[0];
        let n = ag.holdings.len() as u32 + 1;
        let faction = ag.faction;
        if self.holds[first as usize].h.tier < HTier::Town || !self.rng.chance(p.q) {
            return 0;
        }
        let now = now_of(b);
        let d = self.doctrine[faction as usize];
        let c: [Milli; RESOURCES] = core::array::from_fn(|r| {
            bps_mul(
                duplicate_cost(SETTLER_COST[r] as u64, n - 1) as i64,
                d.settler_cost_bps,
            ) * MILLI
        });
        let stock = self.holds[first as usize].h.stock_at(now);
        if (0..RESOURCES).any(|r| stock[r] < c[r]) {
            return 0;
        }
        // A free site within 2 provinces, own wedge first.
        let centre = self.prov(self.holds[first as usize].prov).coord;
        let near = self.provinces_near(centre, 2);
        let mut best: Option<(u32, u32, u8)> = None;
        for pi in near {
            let pr = self.prov(pi);
            for (slot, &h) in pr.site_h.iter().enumerate() {
                if h == NONE {
                    let score = (pr.wedge != faction) as u32 * 10 + centre.distance(pr.coord);
                    if best.is_none_or(|x| score < x.0) {
                        best = Some((score, pi, slot as u8));
                    }
                    break;
                }
            }
        }
        let Some((_, pi, slot)) = best else { return 0 };
        if self.holds[first as usize].h.pay(now, &c).is_err() {
            return 0;
        }
        self.found(a, pi, slot, b, 0);
        self.stats.second_holdings += 1;
        2
    }

    /// Pledge surplus to the Engine (Concord) and science to research
    /// (Knowledge).
    fn pledge(&mut self, a: u32, b: u32, p: &Profile) -> i64 {
        if p.pledge <= 0.0 {
            return 0;
        }
        let now = now_of(b);
        let hs = self.agents[a as usize].holdings.clone();
        let faction = self.agents[a as usize].faction;
        let kb = self.doctrine[faction as usize].knowledge_bps;
        let mut engine = 0i64;
        let mut science = 0i64;
        for hid in hs {
            let x = &mut self.holds[hid as usize];
            let stock = x.h.stock_at(now);
            let reserve = tier_up(x.h.tier).map(|c| c.0).unwrap_or([0; RESOURCES]);
            let mut c = [0 as Milli; RESOURCES];
            // Civic players pledge part of what exceeds half the next
            // tier's cost (the rest is saved).
            for r in [1usize, 2, 5] {
                let spare = stock[r] - reserve[r] * MILLI / 2;
                if spare > 0 {
                    c[r] = (spare as f64 * p.pledge) as i64;
                }
            }
            c[6] = stock[6];
            if x.h.pay(now, &c).is_ok() {
                engine += c[1] + c[2] + c[5];
                science += c[6];
            }
        }
        let e_units = (engine / MILLI) as u64;
        let s_units = (science / MILLI) as u64;
        if e_units + s_units == 0 {
            return 0;
        }
        let ag = &mut self.agents[a as usize];
        ag.pledged += e_units + s_units;
        ag.facts[3] += e_units;
        ag.facts[2] += s_units * kb as u64 / BPS_ONE as u64;
        self.engine_total += e_units;
        self.add_works(a, b, WORKS_PLEDGE);
        1
    }

    fn engine_check(&mut self, b: u32) {
        if self.engine_stages >= 5 {
            return;
        }
        let k = self.engine_stages as u64 + 1;
        let need = k * (k + 1) / 2 * ENGINE_PER_HOLDING * self.used_sites.max(1);
        if self.engine_total >= need {
            self.engine_stages += 1;
            self.stats.engine_stage_days.push(b / BELLS_PER_DAY);
            self.spawn_relics(b);
        }
    }

    /// Six Relic Sites, one per wedge, three rings inside the rim.
    fn spawn_relics(&mut self, _b: u32) {
        if !self.cfg.relics {
            return;
        }
        let d = self.open_ring.saturating_sub(3).max(4).min(self.open_ring);
        let ring = ring_provinces(d);
        for k in 0..6u8 {
            let in_wedge: Vec<ProvinceCoord> = ring
                .iter()
                .copied()
                .filter(|p| p.wedge() == Some(k))
                .collect();
            let n = in_wedge.len();
            for j in 0..n {
                let c = in_wedge[(n / 2 + j) % n];
                let Some(pi) = self.prov_at(c) else { continue };
                if self.prov(pi).relic.is_some() {
                    continue;
                }
                let tile = {
                    let p = self.prov(pi);
                    (1..PROVINCE_TILES as u8)
                        .find(|&t| p.terrain.passable(t) && !p.terrain.is_site(t))
                };
                if let Some(t) = tile {
                    let pr = self.prov_mut(pi);
                    pr.relic = Some(t);
                    if pr.camp.as_ref().is_some_and(|c| c.tile == t) {
                        pr.camp = None;
                    }
                    self.relics.push(pi);
                    self.stats.relics_spawned += 1;
                    break;
                }
            }
        }
    }

    // -------------------------------------------------------- military

    fn travel(&self, faction: u8, from: u32, to: u32, depart: u32) -> Option<(u32, u16)> {
        let dist = self.prov(from).coord.distance(self.prov(to).coord);
        if dist > MAX_MARCH_DIST {
            return None;
        }
        let d = self.doctrine[faction as usize];
        let hexes = 9 * dist + 3;
        let secs = d.k.travel_secs(open_ground_secs(hexes, false, d.k.unit));
        let arrive = earliest_arrival_bell(0, now_of(depart), secs as u32).max(depart + 2);
        Some((arrive, march_stamina(hexes.min(32))))
    }

    fn pick_stance(&mut self, faction: u8, q: f64, against: Option<u8>) -> Stance {
        let d = self.doctrine[faction as usize];
        if let Some(enemy) = against {
            // A defender who sees the siege horn counters the attacker's
            // doctrine stance.
            if self.rng.chance(q) {
                if let Some((s, _)) = self.doctrine[enemy as usize].k.drill {
                    return match s {
                        Stance::Assault => Stance::Brace,
                        Stance::Brace => Stance::Flank,
                        Stance::Flank => Stance::Assault,
                        Stance::Hold => Stance::Hold,
                    };
                }
                if let Some((s, _)) = d.k.drill {
                    return s;
                }
                return Stance::Hold;
            }
            return Stance::Hold;
        }
        if let Some((s, _)) = d.k.drill {
            if self.rng.chance(q) {
                return s;
            }
        }
        [Stance::Hold, Stance::Assault, Stance::Flank, Stance::Brace][self.rng.below(4) as usize]
    }

    /// Muster a host from a holding's garrison and send it.
    #[allow(clippy::too_many_arguments)]
    fn send(
        &mut self,
        a: u32,
        from: u32,
        n_troops: MilliTroops,
        to_prov: u32,
        tile: u8,
        mission: Mission,
        b: u32,
        p: &Profile,
        against: Option<u8>,
    ) -> Option<u32> {
        let faction = self.agents[a as usize].faction;
        let d = self.doctrine[faction as usize];
        let src = self.holds[from as usize].prov;
        let (arrive, cost) = self.travel(faction, src, to_prov, b)?;
        if n_troops < MIN_HOST_TROOPS || self.holds[from as usize].garrison < n_troops {
            return None;
        }
        // Unit variant surcharge over a Spearman (horses, iron).
        let extra = permutation_rules::units::stats(d.k.unit).prod_cost as i64 - 6;
        if extra > 0 {
            let k = n_troops as i64 / (100 * MILLI);
            let c: [Milli; RESOURCES] = [
                0,
                0,
                0,
                k * TROOP_COST_PER_100[1] * extra / 6 * MILLI,
                0,
                k * TROOP_COST_PER_100[2] * extra / 6 * MILLI,
                0,
                0,
            ];
            if self.holds[from as usize].h.pay(now_of(b), &c).is_err() {
                return None;
            }
        }
        let stance = self.pick_stance(faction, p.q, against);
        let id = self.hosts.len() as u32;
        let mut stamina = Stamina::full(b);
        if stamina.spend(b, cost).is_err() {
            return None;
        }
        let x = &mut self.holds[from as usize];
        x.garrison -= n_troops;
        x.away += n_troops;
        let unrevealed = self.rng.chance(p.withhold);
        self.hosts.push(HostS {
            owner: a,
            home: from,
            faction,
            unit: d.k.unit,
            troops: n_troops,
            stamina,
            ready_bell: b,
            state: HState::Marching { arrive },
            prov: to_prov,
            tile,
            stance,
            retreat: if self.rng.chance(p.q) {
                Some(6_667)
            } else {
                None
            },
            mission,
            unrevealed,
            back_bells: arrive - b,
        });
        self.arrivals[arrive as usize].push(id);
        self.refresh_upkeep(from, b);
        self.reweigh(from, b);
        Some(id)
    }

    /// Put a host straight onto its own holding's hex (muster; in the
    /// roster from the next bell).
    fn muster_in_place(
        &mut self,
        a: u32,
        hid: u32,
        n_troops: MilliTroops,
        b: u32,
        mission: Mission,
    ) {
        let faction = self.agents[a as usize].faction;
        let d = self.doctrine[faction as usize];
        let x = &mut self.holds[hid as usize];
        if n_troops < MIN_HOST_TROOPS || x.garrison < n_troops {
            return;
        }
        x.garrison -= n_troops;
        x.away += n_troops;
        let (pi, tile) = (x.prov, x.tile);
        let posture = x.posture;
        let stance = match posture {
            Posture::Stance(s) => s,
            Posture::Disarray => Stance::Hold,
        };
        let id = self.hosts.len() as u32;
        self.hosts.push(HostS {
            owner: a,
            home: hid,
            faction,
            unit: d.k.unit,
            troops: n_troops,
            stamina: Stamina::full(b),
            ready_bell: b,
            state: HState::Stationed { from: b + 1 },
            prov: pi,
            tile,
            stance,
            retreat: None,
            mission,
            unrevealed: false,
            back_bells: 1,
        });
        self.prov_mut(pi).stationed.push(id);
        self.stationed_provs.insert(pi);
        self.refresh_upkeep(hid, b);
        self.reweigh(hid, b);
    }

    fn defend(&mut self, a: u32, hid: u32, b: u32, p: &Profile) -> i64 {
        let mut used = 0;
        let (sieged, attacker_f, occupied, garrison, pi, tile) = {
            let x = &self.holds[hid as usize];
            (
                x.siege.is_some(),
                x.siege.as_ref().map(|s| s.s.attacker_faction),
                x.occupier.is_some(),
                x.garrison,
                x.prov,
                x.tile,
            )
        };
        if !(sieged || occupied) {
            if self.holds[hid as usize].posture != Posture::default() {
                self.holds[hid as usize].posture = Posture::default();
            }
            return 0;
        }
        // Posture against the horn.
        if sieged {
            let s = self.pick_stance(self.agents[a as usize].faction, p.q, attacker_f);
            self.holds[hid as usize].posture = if self.rng.chance(p.withhold) {
                self.stats.disarray += 1;
                Posture::Disarray
            } else {
                Posture::Stance(s)
            };
            used += 1;
        }
        // Already a defender on the hex?
        let present = self.prov(pi).stationed.iter().any(|&h| {
            let x = &self.hosts[h as usize];
            x.owner == a && x.tile == tile && x.state != HState::Dead
        });
        if present {
            return used;
        }
        // Hostile strength on the hex.
        let hostile: u64 = self
            .prov(pi)
            .stationed
            .iter()
            .map(|&h| &self.hosts[h as usize])
            .filter(|x| x.tile == tile && x.faction != self.agents[a as usize].faction)
            .map(|x| host_strength(x.unit, x.troops))
            .sum();
        let est = hostile as f64 * self.rng.lognormal(0.6 * (1.0 - p.q) + 0.05);
        let mine = garrison as f64 * 10.0;
        if mine >= 0.8 * est && garrison >= 2 * MIN_HOST_TROOPS {
            let n = garrison / 2;
            self.muster_in_place(a, hid, n, b, Mission::Defend(hid));
            used += 1;
        } else {
            // Help from the owner's other holdings in range.
            let hs = self.agents[a as usize].holdings.clone();
            for other in hs {
                if other == hid {
                    continue;
                }
                let g = self.holds[other as usize].garrison;
                if g >= 2 * MIN_HOST_TROOPS
                    && self
                        .send(
                            a,
                            other,
                            g * 2 / 3,
                            pi,
                            tile,
                            Mission::Defend(hid),
                            b,
                            p,
                            attacker_f,
                        )
                        .is_some()
                {
                    used += 2;
                    break;
                }
            }
        }
        used
    }

    fn perceived_defence(&mut self, hid: u32, q: f64) -> f64 {
        let x = &self.holds[hid as usize];
        let mut s = x.garrison as f64 * 10.0 * if x.h.walls > 0 { 1.5 } else { 1.0 };
        let f = x.faction;
        let (pi, tile) = (x.prov, x.tile);
        for &h in &self.prov(pi).stationed {
            let y = &self.hosts[h as usize];
            if y.tile == tile && y.faction == f {
                s += host_strength(y.unit, y.troops) as f64;
            }
        }
        s * self.rng.lognormal(0.6 * (1.0 - q) + 0.05)
    }

    /// Strength the target's March would send under the standing order
    /// (the 4 largest 25% shares).
    fn expected_reinforcement(&self, t: u32) -> f64 {
        let x = &self.holds[t as usize];
        if x.free_city() {
            return 0.0;
        }
        let m = self.prov(x.prov).march;
        let mut v: Vec<MilliTroops> = self
            .holds
            .iter()
            .filter(|y| {
                y.alive && y.faction == x.faction && !y.free_city() && self.prov(y.prov).march == m
            })
            .map(|y| y.garrison / 4)
            .collect();
        v.sort_unstable_by(|a, b| b.cmp(a));
        v.iter().take(4).map(|&g| g as f64 * 10.0).sum()
    }

    /// Look for a holding to besiege near one's own holdings.
    fn war(&mut self, a: u32, b: u32, p: &Profile) -> i64 {
        let now = now_of(b);
        let (faction, hs, laurels, n_hold) = {
            let ag = &self.agents[a as usize];
            (
                ag.faction,
                ag.holdings.clone(),
                ag.laurels,
                ag.holdings.len(),
            )
        };
        if laurels < SIEGE_STAKE {
            return 0;
        }
        let d = self.doctrine[faction as usize];
        let per_troop = permutation_rules::units::stats(d.k.unit).strength as f64;
        // Best source: the largest garrison.
        let Some(&src) = hs.iter().max_by_key(|&&h| self.holds[h as usize].garrison) else {
            return 0;
        };
        let keep = troops(garrison_target(self.holds[src as usize].h.tier) / 4);
        let avail = self.holds[src as usize].garrison.saturating_sub(keep);
        if avail < 2 * MIN_HOST_TROOPS {
            return 0;
        }
        let centre = self.prov(self.holds[src as usize].prov).coord;
        let near = self.provinces_near(centre, 2);
        let mut cands: Vec<(f64, u32)> = Vec::new();
        for pi in near {
            for &t in &self.prov(pi).site_h {
                if t == NONE {
                    continue;
                }
                let x = &self.holds[t as usize];
                if x.faction == faction || x.siege.is_some() || !x.alive {
                    continue;
                }
                let kind = x.kind();
                if kind != HoldingKind::First && n_hold >= 3 {
                    continue;
                }
                if x.occupier.is_some() {
                    continue;
                }
                let pc = self.prov(pi).coord;
                let chk = SiegeCheck {
                    province: pc,
                    kind,
                    owner_faction: if x.free_city() { NEUTRAL } else { x.faction },
                    attacker_faction: faction,
                    relation: Relation::Rivalry,
                    march_hostility: false,
                    march_truce: false,
                    founded_ts: x.h.founded_ts,
                    founded_day: x.h.founded_day,
                    shield_until: x.h.shield_until(),
                    dormant: x.free_city() || x.h.is_dormant(now),
                    attacker_nearby: true,
                    now,
                };
                if may_besiege(&chk).is_err() {
                    continue;
                }
                // Value: a live stake to occupy, a holding to capture.
                let dormant = x.free_city() || x.h.is_dormant(now);
                let value = match kind {
                    HoldingKind::First if dormant => continue,
                    HoldingKind::First => 1.0,
                    _ => 1.5,
                };
                cands.push((value, t));
            }
        }
        if cands.is_empty() {
            return 0;
        }
        // Skilled players weigh value against defence; others take the
        // first plausible target.
        let mut best: Option<(f64, u32, f64)> = None;
        for (v, t) in cands {
            let def = self.perceived_defence(t, p.q).max(1.0);
            let score = v / def;
            if best.is_none_or(|x| score > x.0) {
                best = Some((score, t, def));
            }
        }
        let Some((_, t, mut def)) = best else {
            return 0;
        };
        // Skilled attackers expect the March's standing reinforcements.
        def += p.q * self.expected_reinforcement(t);
        let margin = 3.0 + (1.0 - p.q);
        let need = def * margin / per_troop; // milli-troops
        if (avail as f64) < need {
            return 0;
        }
        let n = (need * 1.2).min(avail as f64) as MilliTroops;
        let (tp, tt, tf, walls, kind) = {
            let x = &self.holds[t as usize];
            (
                x.prov,
                x.tile,
                x.faction,
                x.h.walls_at(now_of(b)).unwrap_or(x.h.walls),
                x.kind(),
            )
        };
        let Some(host) = self.send(a, src, n, tp, tt, Mission::Siege(t), b, p, None) else {
            return 0;
        };
        // Declare: the siege horn, 5 laurels escrowed (counted laurels if
        // the attacker has them).
        let stake_counted = self.counted(a) >= SIEGE_STAKE;
        self.escrow_uncounted(a, !stake_counted, true);
        self.agents[a as usize].laurels -= SIEGE_STAKE;
        self.agents[a as usize].earned.siege_net -= SIEGE_STAKE as i64;
        self.escrow += SIEGE_STAKE;
        self.stats.sieges_declared += 1;
        self.holds[t as usize].siege = Some(SiegeRec {
            s: Siege::declare(faction, b, walls, 0),
            attacker: a,
            stake_counted,
            host,
            kind,
        });
        self.siege_holds.insert(t);
        // Auto-reinforce: friendly holdings of the March send 25%.
        if tf != NEUTRAL {
            let m = march_of(self.prov(tp).coord);
            let donors: Vec<Donor> = self
                .holds
                .iter()
                .enumerate()
                .filter(|(i, y)| {
                    *i as u32 != t
                        && y.alive
                        && y.faction == tf
                        && !y.free_city()
                        && self.agents[y.owner as usize].arch != Arch::Idle
                        && march_of(self.prov(y.prov).coord) == m
                })
                .map(|(i, y)| Donor {
                    id: i as u64,
                    province: self.prov(y.prov).coord,
                    faction: y.faction,
                    garrison: y.garrison,
                    order_bps: 2_500,
                })
                .collect();
            // The standing order sends hosts (≥ 100 troops) from the
            // largest donors; a faction has 4 arrival slots per province
            // and bell, so only the 4 largest are sent.
            let mut out = auto_reinforce(t as u64, self.prov(tp).coord, tf, &donors);
            out.sort_by_key(|x| (std::cmp::Reverse(x.1), x.0));
            let mut sent = 0;
            for (id, n) in out {
                if sent == 4 || n < MIN_HOST_TROOPS {
                    break;
                }
                let owner = self.holds[id as usize].owner;
                let prof = self.profiles[self.agents[owner as usize].arch.idx()];
                if self
                    .send(
                        owner,
                        id as u32,
                        n,
                        tp,
                        tt,
                        Mission::Defend(t),
                        b,
                        &prof,
                        Some(faction),
                    )
                    .is_some()
                {
                    sent += 1;
                }
            }
        }
        2
    }

    /// Attack a barbarian camp for Works and loot.
    fn camp_raid(&mut self, a: u32, b: u32, p: &Profile) -> i64 {
        let hs = self.agents[a as usize].holdings.clone();
        let Some(&src) = hs.iter().max_by_key(|&&h| self.holds[h as usize].garrison) else {
            return 0;
        };
        let centre = self.prov(self.holds[src as usize].prov).coord;
        let near = self.provinces_near(centre, 1);
        let mut target = None;
        for pi in near {
            if let Some(c) = &self.prov(pi).camp {
                target = Some((pi, c.tile, c.troops));
                break;
            }
        }
        let Some((pi, tile, ct)) = target else {
            return 0;
        };
        let est = ct as f64 * self.rng.lognormal(0.6 * (1.0 - p.q) + 0.05);
        let faction = self.agents[a as usize].faction;
        let per = permutation_rules::units::stats(self.doctrine[faction as usize].k.unit).strength
            as f64
            / 10.0;
        let need = (est * 2.5 / per) as MilliTroops;
        let keep = troops(garrison_target(self.holds[src as usize].h.tier) / 3);
        let avail = self.holds[src as usize].garrison.saturating_sub(keep);
        if avail < need.max(MIN_HOST_TROOPS) {
            return 0;
        }
        let n = need.max(MIN_HOST_TROOPS);
        if self
            .send(a, src, n, pi, tile, Mission::Camp, b, p, None)
            .is_some()
        {
            2
        } else {
            0
        }
    }

    /// Send a host to a Relic Site in range that no friendly host holds.
    fn relic_move(&mut self, a: u32, b: u32, p: &Profile) {
        let faction = self.agents[a as usize].faction;
        let hs = self.agents[a as usize].holdings.clone();
        let Some(&src) = hs.iter().max_by_key(|&&h| self.holds[h as usize].garrison) else {
            return;
        };
        let from = self.prov(self.holds[src as usize].prov).coord;
        let mut best: Option<(u32, u32)> = None;
        for &pi in &self.relics {
            let dist = from.distance(self.prov(pi).coord);
            if dist <= MAX_MARCH_DIST && best.is_none_or(|x| dist < x.0) {
                best = Some((dist, pi));
            }
        }
        let Some((_, pi)) = best else { return };
        let tile = self.prov(pi).relic.expect("relic");
        let mut friendly = false;
        let mut hostile = 0u64;
        for &h in &self.prov(pi).stationed {
            let x = &self.hosts[h as usize];
            if x.tile == tile && x.state != HState::Dead {
                if x.faction == faction {
                    friendly = true;
                } else {
                    hostile += host_strength(x.unit, x.troops);
                }
            }
        }
        // Friendly hosts already marching there?
        if friendly {
            return;
        }
        let per =
            permutation_rules::units::stats(self.doctrine[faction as usize].k.unit).strength as f64;
        let est = hostile as f64 * self.rng.lognormal(0.6 * (1.0 - p.q) + 0.05);
        let need = ((est * 1.5 / per) as MilliTroops).max(troops(300));
        let keep = troops(garrison_target(self.holds[src as usize].h.tier) / 3);
        let avail = self.holds[src as usize].garrison.saturating_sub(keep);
        if avail < need {
            return;
        }
        let _ = self.send(
            a,
            src,
            need.max(avail / 2),
            pi,
            tile,
            Mission::Relic,
            b,
            p,
            None,
        );
    }

    // -------------------------------------------------------- the bell

    fn resolve_bell(&mut self, b: u32) {
        // Provinces to resolve: arrivals this bell, and stationed hosts
        // facing something hostile on their hex.
        let arrivals = std::mem::take(&mut self.arrivals[b as usize]);
        let mut by_prov: HashMap<u32, Vec<u32>> = HashMap::new();
        for h in arrivals {
            if self.hosts[h as usize].state != (HState::Marching { arrive: b }) {
                continue;
            }
            // Unrevealed by the reveal close: routed (design §6.2).
            if self.hosts[h as usize].unrevealed {
                self.stats.routs += 1;
                let x = &mut self.hosts[h as usize];
                let before = x.troops;
                x.troops = permutation_rules::frontier::host::rout_survivors(x.troops);
                x.stamina.set(b, 0);
                let lost = before - x.troops;
                let home = x.home;
                self.holds[home as usize].away =
                    self.holds[home as usize].away.saturating_sub(lost);
                self.send_home(h, b);
                continue;
            }
            by_prov
                .entry(self.hosts[h as usize].prov)
                .or_default()
                .push(h);
        }
        let mut provs: BTreeSet<u32> = by_prov.keys().copied().collect();
        let stationed: Vec<u32> = self.stationed_provs.iter().copied().collect();
        for pi in stationed {
            if self.prov(pi).stationed.is_empty() {
                self.stationed_provs.remove(&pi);
                continue;
            }
            if self.contested(pi, b) {
                provs.insert(pi);
            }
        }
        for pi in provs {
            let arr = by_prov.remove(&pi).unwrap_or_default();
            self.clash(pi, b, arr);
        }
        // Sieges advance one resolved bell.
        let sieges: Vec<u32> = self.siege_holds.iter().copied().collect();
        for t in sieges {
            self.advance_siege(t, b);
        }
    }

    fn contested(&self, pi: u32, b: u32) -> bool {
        let p = self.prov(pi);
        for &h in &p.stationed {
            let x = &self.hosts[h as usize];
            if !matches!(x.state, HState::Stationed { from } if from <= b) {
                continue;
            }
            for &g in &p.site_h {
                if g != NONE {
                    let y = &self.holds[g as usize];
                    if y.tile == x.tile && y.faction != x.faction {
                        return true;
                    }
                }
            }
            for &o in &p.stationed {
                let y = &self.hosts[o as usize];
                if y.tile == x.tile && y.faction != x.faction {
                    return true;
                }
            }
            if p.camp.as_ref().is_some_and(|c| c.tile == x.tile) {
                return true;
            }
        }
        false
    }

    fn fighter(&self, h: u32, b: u32, arrival: bool) -> Fighter {
        let x = &self.hosts[h as usize];
        let d = self.doctrine[x.faction as usize];
        let posture = if arrival {
            Posture::Stance(x.stance)
        } else {
            match x.mission {
                Mission::Defend(hid) => self.holds[hid as usize].posture,
                _ => Posture::Stance(x.stance),
            }
        };
        let dealt = d.k.dealt_bps(posture, arrival);
        Fighter {
            id: h as u64 + 1,
            faction: x.faction,
            unit: x.unit,
            troops: x.troops,
            stamina: x.stamina.at(b),
            tile: x.tile,
            posture: match posture {
                Posture::Disarray if arrival => Posture::Stance(Stance::Hold),
                p => p,
            },
            retreat_bps: if arrival { x.retreat } else { None },
            dealt_bps: dealt,
        }
    }

    fn clash(&mut self, pi: u32, b: u32, arrivals: Vec<u32>) {
        // Arrival slots: one per citizen, 4 per faction, by mass.
        let mut arrivals = arrivals;
        arrivals.sort_by_key(|&h| {
            let x = &self.hosts[h as usize];
            (std::cmp::Reverse(x.troops), h)
        });
        let mut per_f = [0usize; 8];
        let mut seen: Vec<u32> = Vec::new();
        let mut admitted = Vec::new();
        for h in arrivals {
            let x = &self.hosts[h as usize];
            if per_f[x.faction as usize] >= 4 || seen.contains(&x.owner) {
                self.send_home(h, b);
                continue;
            }
            per_f[x.faction as usize] += 1;
            seen.push(x.owner);
            admitted.push(h);
        }
        let p = self.prov(pi);
        let residents_ids: Vec<u32> = p
            .stationed
            .iter()
            .copied()
            .filter(|&h| matches!(self.hosts[h as usize].state, HState::Stationed { from } if from <= b))
            .collect();
        let mut residents: Vec<Fighter> = residents_ids
            .iter()
            .map(|&h| self.fighter(h, b, false))
            .collect();
        if let Some(c) = &p.camp {
            residents.push(Fighter {
                id: CAMP_BIT | pi as u64,
                faction: NEUTRAL,
                unit: UnitType::Spearman,
                troops: c.troops,
                stamina: 120,
                tile: c.tile,
                posture: Posture::default(),
                retreat_bps: None,
                dealt_bps: BPS_ONE,
            });
        }
        let garrisons: Vec<Garrison> = p
            .site_h
            .iter()
            .filter(|&&g| g != NONE)
            .map(|&g| {
                let y = &self.holds[g as usize];
                Garrison {
                    id: g as u64,
                    faction: y.faction,
                    tile: y.tile,
                    troops: y.garrison,
                    walls: y.h.walls_at(now_of(b)).unwrap_or(y.h.walls) > 0,
                    posture: y.posture,
                }
            })
            .collect();
        let arr: Vec<Fighter> = admitted.iter().map(|&h| self.fighter(h, b, true)).collect();
        let seed = sha256(&[b"sim/bell", &self.cfg.seed.to_le_bytes(), &b.to_le_bytes()]);
        let inp = ClashInput {
            province: p.coord,
            bell: b,
            seed,
            terrain: &p.terrain,
            residents: &residents,
            garrisons: &garrisons,
            arrivals: &arr,
            relations: Relations::ALL_HOSTILE,
        };
        let out = match resolve_clash(&self.rules, &inp) {
            Ok(o) => o,
            Err(_) => {
                self.stats.clash_errors += 1;
                for h in admitted {
                    self.send_home(h, b);
                }
                return;
            }
        };
        self.stats.clashes += 1;
        self.stats.engagements += out.engagements as u64;
        // Garrisons.
        for g in &out.garrisons {
            let hid = g.id as u32;
            let changed = self.holds[hid as usize].garrison != g.troops;
            self.holds[hid as usize].garrison = g.troops;
            self.holds[hid as usize].report = Some((
                b,
                BellReport {
                    holders: g.holders,
                    defender_present: g.defender_present,
                },
            ));
            if changed {
                self.refresh_upkeep(hid, b);
                self.reweigh(hid, b);
            }
        }
        // Hosts and the camp.
        let mut camp_beaten = false;
        let mut camp_winners: Vec<u32> = Vec::new();
        for f in &out.fighters {
            if f.id & CAMP_BIT != 0 {
                if !matches!(f.fate, Fate::Stays { .. }) || f.troops == 0 {
                    camp_beaten = true;
                } else if let Some(c) = self.prov_mut(pi).camp.as_mut() {
                    c.troops = f.troops;
                }
                continue;
            }
            let h = (f.id - 1) as u32;
            let before = self.hosts[h as usize].troops;
            let home = self.hosts[h as usize].home;
            {
                let x = &mut self.hosts[h as usize];
                x.troops = f.troops;
                x.stamina.set(b, f.stamina);
                if f.engaged {
                    x.ready_bell = ready_bell_after(b);
                }
            }
            if before != f.troops {
                let y = &mut self.holds[home as usize];
                y.away = y.away.saturating_sub(before - f.troops);
                self.refresh_upkeep(home, b);
            }
            match f.fate {
                Fate::Stays { tile } | Fate::Withdrew { tile } => {
                    self.hosts[h as usize].tile = tile;
                    if f.arrival {
                        self.hosts[h as usize].state = HState::Stationed { from: b + 1 };
                        self.prov_mut(pi).stationed.push(h);
                        self.stationed_provs.insert(pi);
                    }
                    if self.hosts[h as usize].mission == Mission::Camp {
                        camp_winners.push(h);
                    }
                }
                Fate::Bounced | Fate::Retreated => {
                    self.unstation(pi, h);
                    self.send_home(h, b);
                }
                Fate::Destroyed => {
                    self.unstation(pi, h);
                    self.hosts[h as usize].state = HState::Dead;
                }
            }
        }
        if camp_beaten {
            if let Some(c) = self.prov_mut(pi).camp.take() {
                for &h in &camp_winners {
                    if self.hosts[h as usize].tile == c.tile {
                        let a = self.hosts[h as usize].owner;
                        self.stats.camps_won += 1;
                        self.add_works(a, b, WORKS_CAMP);
                        let home = self.hosts[h as usize].home;
                        let _ = self.holds[home as usize].h.credit(
                            now_of(b),
                            Resource::Food,
                            100 * MILLI,
                        );
                        break;
                    }
                }
            }
        }
        // Camp hosts go home unless the camp still stands on their hex.
        for h in camp_winners {
            let tile = self.hosts[h as usize].tile;
            let camp_here = self.prov(pi).camp.as_ref().is_some_and(|c| c.tile == tile);
            if !camp_here && self.hosts[h as usize].state != HState::Dead {
                self.unstation(pi, h);
                self.send_home(h, b);
            }
        }
        // Occupations end when the occupier no longer stands on the hex.
        let sites: Vec<u32> = self.prov(pi).site_h.clone();
        for g in sites {
            if g == NONE {
                continue;
            }
            if let Some((o, host)) = self.holds[g as usize].occupier {
                let x = &self.hosts[host as usize];
                let there = matches!(x.state, HState::Stationed { .. })
                    && x.tile == self.holds[g as usize].tile;
                if !there {
                    self.stats.liberations += 1;
                    self.holds[g as usize].occupier = None;
                    self.reweigh(g, b);
                    let _ = o;
                    if self.hosts[host as usize].state != HState::Dead {
                        self.unstation(pi, host);
                        self.send_home(host, b);
                    }
                }
            }
        }
    }

    fn unstation(&mut self, pi: u32, h: u32) {
        let p = self.prov_mut(pi);
        if let Some(i) = p.stationed.iter().position(|&x| x == h) {
            p.stationed.swap_remove(i);
        }
    }

    fn send_home(&mut self, h: u32, b: u32) {
        let x = &mut self.hosts[h as usize];
        if matches!(x.state, HState::Dead | HState::Returning) {
            return;
        }
        x.state = HState::Returning;
        let at = (b + x.back_bells.max(1)) as usize;
        if at < self.events.len() {
            self.events[at].push(Ev::Return(h));
        }
    }

    fn host_home(&mut self, h: u32, b: u32) {
        let (owner, home, t) = {
            let x = &mut self.hosts[h as usize];
            if x.state != HState::Returning {
                return;
            }
            x.state = HState::Dead;
            (x.owner, x.home, x.troops)
        };
        let y = &mut self.holds[home as usize];
        y.away = y.away.saturating_sub(t);
        let dest = if y.owner == owner && y.alive {
            Some(home)
        } else {
            self.agents[owner as usize].holdings.first().copied()
        };
        if let Some(d) = dest {
            self.holds[d as usize].garrison += t;
            self.refresh_upkeep(d, b);
            self.reweigh(d, b);
        }
        if dest != Some(home) {
            self.refresh_upkeep(home, b);
        }
    }

    fn advance_siege(&mut self, t: u32, b: u32) {
        let (report, vigil) = {
            let x = &self.holds[t as usize];
            let r = match x.report {
                Some((rb, r)) if rb == b => r,
                _ => BellReport {
                    holders: 0,
                    defender_present: false,
                },
            };
            (r, x.vigil)
        };
        let Some(rec) = self.holds[t as usize].siege.as_mut() else {
            self.siege_holds.remove(&t);
            return;
        };
        if rec.s.last_bell >= b {
            return;
        }
        let status = rec
            .s
            .advance(b, now_of(b), report, &vigil)
            .unwrap_or(SiegeStatus::Failed);
        match status {
            SiegeStatus::Active => {}
            SiegeStatus::Completed => self.siege_done(t, b, true),
            SiegeStatus::Failed => self.siege_done(t, b, false),
        }
    }

    fn siege_done(&mut self, t: u32, b: u32, ok: bool) {
        let Some(rec) = self.holds[t as usize].siege.take() else {
            return;
        };
        self.siege_holds.remove(&t);
        let att = rec.attacker;
        let owner = self.holds[t as usize].owner;
        let pair = if owner != NONE {
            self.pair_history(att, owner)
        } else {
            PairHistory::default()
        };
        let st = siege_settle(ok, rec.stake_counted, pair);
        self.escrow -= SIEGE_STAKE;
        if st.to_attacker > 0 {
            self.escrow_uncounted(att, !rec.stake_counted, false);
        }
        self.agents[att as usize].laurels += st.to_attacker;
        self.agents[att as usize].earned.siege_net += st.to_attacker as i64;
        self.stats.laurels_burned += st.burned;
        if st.to_defender > 0 {
            if owner != NONE {
                self.agents[owner as usize].laurels += st.to_defender;
                self.agents[owner as usize].earned.siege_net += st.to_defender as i64;
                self.note_pair(att, owner);
            } else {
                self.stats.siege_stake_orphaned += st.to_defender;
            }
        }
        // Defenders of this holding go home.
        let pi = self.holds[t as usize].prov;
        let defenders: Vec<u32> = self
            .prov(pi)
            .stationed
            .iter()
            .copied()
            .filter(|&h| self.hosts[h as usize].mission == Mission::Defend(t))
            .collect();
        for h in defenders {
            self.unstation(pi, h);
            self.send_home(h, b);
        }
        self.holds[t as usize].posture = Posture::default();
        let host = rec.host;
        let host_there = matches!(self.hosts[host as usize].state, HState::Stationed { .. });
        if !ok {
            self.stats.sieges_failed += 1;
            if rec.s.held {
                self.stats.fail_lost += 1;
                match self.hosts[host as usize].state {
                    HState::Dead => self.stats.lost_destroyed += 1,
                    HState::Returning => self.stats.lost_left += 1,
                    _ => {}
                }
            } else {
                self.stats.fail_never_held += 1;
            }
            if host_there {
                self.unstation(pi, host);
                self.send_home(host, b);
            }
            return;
        }
        self.stats.sieges_completed += 1;
        match completion(rec.kind) {
            Some(Completion::Occupy) => {
                if host_there && owner != NONE {
                    self.stats.occupations += 1;
                    self.hosts[host as usize].mission = Mission::Occupy(t);
                    self.holds[t as usize].occupier = Some((att, host));
                    self.holds[t as usize].occ_pair = self.pair_history(att, owner);
                    self.note_pair(att, owner);
                    self.reweigh(t, b);
                } else if host_there {
                    self.unstation(pi, host);
                    self.send_home(host, b);
                }
            }
            Some(Completion::Capture) => {
                if self.agents[att as usize].holdings.len() >= 3 || !host_there {
                    if host_there {
                        self.unstation(pi, host);
                        self.send_home(host, b);
                    }
                    return;
                }
                self.capture(t, att, host, b);
            }
            None => {}
        }
    }

    fn capture(&mut self, t: u32, att: u32, host: u32, b: u32) {
        let pi = self.holds[t as usize].prov;
        let victim = self.holds[t as usize].owner;
        // Final credit to the old owner, then detach.
        self.holds[t as usize].alive = false;
        self.reweigh(t, b);
        if victim != NONE {
            self.stats.captures += 1;
            let prior = self.agents[att as usize]
                .pairs
                .iter()
                .find(|x| x.0 == victim)
                .map_or(0, |x| x.1);
            let tr = capture_transfer(
                self.counted(victim),
                PairHistory {
                    prior_captures: prior,
                    other_ties: false,
                },
            );
            let (mut vl, mut al) = (
                self.agents[victim as usize].laurels,
                self.agents[att as usize].laurels,
            );
            permutation_rules::frontier::laurel::transfer(&mut vl, &mut al, tr.laurels)
                .expect("transfer");
            self.agents[victim as usize].laurels = vl;
            self.agents[att as usize].laurels = al;
            self.agents[victim as usize].earned.capture_out += tr.laurels;
            self.agents[att as usize].earned.capture_in += tr.laurels;
            for (x, y) in [(att, victim), (victim, att)] {
                let v = &mut self.agents[x as usize].pairs;
                match v.iter_mut().find(|p| p.0 == y) {
                    Some(p) => p.1 += 1,
                    None => v.push((y, 1)),
                }
            }
            self.agents[victim as usize].holdings.retain(|&h| h != t);
        } else {
            self.stats.free_city_captures += 1;
        }
        self.agents[att as usize].facts[0] += 36 * 1000;
        let faction = self.agents[att as usize].faction;
        let order = self.agents[att as usize].holdings.len() as u8 + 1;
        let keep_walls = self.doctrine[faction as usize].k.keeps_walls_on_capture;
        let ht = self.hosts[host as usize].troops;
        self.unstation(pi, host);
        {
            let home = self.hosts[host as usize].home;
            let y = &mut self.holds[home as usize];
            y.away = y.away.saturating_sub(ht);
            self.hosts[host as usize].state = HState::Dead;
            self.refresh_upkeep(home, b);
        }
        let now = now_of(b);
        let x = &mut self.holds[t as usize];
        let was_free = x.owner == NONE;
        x.owner = att;
        x.faction = faction;
        x.h.order = order.max(2);
        x.h.last_owner_action = now;
        x.garrison = ht;
        x.away = 0;
        x.alive = true;
        x.occupier = None;
        if was_free && !keep_walls {
            x.h.walls = 0;
        }
        self.agents[att as usize].holdings.push(t);
        self.apply_tier_bonus(t, b);
        self.refresh_upkeep(t, b);
        self.reweigh(t, b);
        self.schedule_dormancy(t, b);
    }

    /// Relic Sites pay 1 laurel per bell to the hosts holding them, split
    /// by troops (design §5.4 item 5).
    fn relic_credit(&mut self, b: u32) {
        let relics = self.relics.clone();
        for pi in relics {
            let tile = self.prov(pi).relic.expect("relic");
            let holders: Vec<(u32, u64)> = self
                .prov(pi)
                .stationed
                .iter()
                .map(|&h| (h, &self.hosts[h as usize]))
                .filter(|(_, x)| {
                    x.tile == tile && matches!(x.state, HState::Stationed { from } if from <= b)
                })
                .map(|(h, x)| (h, x.troops as u64))
                .collect();
            let total: u64 = holders.iter().map(|x| x.1).sum();
            if total == 0 {
                continue;
            }
            // Revision-2 comparison runs only: K3 Relic Sites mint nothing
            // (`laurel::RELIC_EMISSION_PER_BELL`), and the K3 default does
            // not spawn them (`Config::relics`).
            let mint = relic_credit_at(1, RELIC_EMISSION_PER_BELL_REV2).expect("relic credit");
            self.stats.relic_minted += mint;
            let mut paid = 0;
            for (i, (h, t)) in holders.iter().enumerate() {
                let c = if i + 1 == holders.len() {
                    mint - paid
                } else {
                    (mint as u128 * *t as u128 / total as u128) as u64
                };
                paid += c;
                let a = self.hosts[*h as usize].owner;
                let f = self.agents[a as usize].faction;
                let m = mandate_reserve_split(c);
                self.reserve[f as usize].deposit(m.give).expect("reserve");
                self.agents[a as usize].laurels += m.keep;
                self.agents[a as usize].earned.relic += m.keep;
            }
            // Supply: hosts far from any friendly holding lose 1% a bell.
            if b % HOUR_BELLS == 0 {
                for (h, _) in holders {
                    let f = self.hosts[h as usize].faction;
                    let d = self.doctrine[f as usize].k;
                    let c = self.prov(pi).coord;
                    let range = d.supply_range(3);
                    let supplied = self.provinces_near(c, range).into_iter().any(|q| {
                        self.prov(q)
                            .site_h
                            .iter()
                            .any(|&g| g != NONE && self.holds[g as usize].faction == f)
                    });
                    if !supplied {
                        let x = &mut self.hosts[h as usize];
                        let before = x.troops;
                        x.troops = d.attrition(before, supply_attrition(before, HOUR_BELLS));
                        let home = x.home;
                        let lost = before - x.troops;
                        self.holds[home as usize].away =
                            self.holds[home as usize].away.saturating_sub(lost);
                    }
                }
            }
        }
    }

    // -------------------------------------------------------- folds

    /// Hourly `FoldMarch` (Dominion: a faction controls a March in a bell
    /// when it holds ≥ 50% of its strength weight) and production facts.
    fn hourly_fold(&mut self, b: u32) {
        let now = now_of(b);
        let nm = self.marches.len();
        let mut w = vec![[0u64; 7]; nm];
        for x in &self.holds {
            if !x.alive || x.free_city() || !x.attached {
                continue;
            }
            let m = self.prov(x.prov).march as usize;
            w[m][x.faction as usize] += x.stake.weight;
        }
        let mut control = vec![NONE; nm];
        for (m, ws) in w.iter().enumerate() {
            let tot: u64 = ws.iter().sum();
            if tot == 0 {
                continue;
            }
            for (f, &v) in ws.iter().enumerate().take(6) {
                if v * 2 >= tot {
                    control[m] = f as u32;
                }
            }
        }
        for i in 0..self.holds.len() {
            let x = &self.holds[i];
            if !x.alive || x.free_city() {
                continue;
            }
            let dormant = x.h.is_dormant(now);
            let owner = x.owner as usize;
            if !dormant {
                let prod: i64 =
                    x.h.production
                        .iter()
                        .enumerate()
                        .filter(|(r, _)| *r != Resource::Science as usize)
                        .map(|(_, v)| *v)
                        .sum();
                self.agents[owner].facts[1] += (prod / MILLI).max(0) as u64;
            }
            if x.attached {
                let m = self.prov(x.prov).march as usize;
                if control[m] == x.faction as u32 {
                    let tot = w[m][x.faction as usize].max(1);
                    self.agents[owner].facts[0] +=
                        (6_000u128 * x.stake.weight as u128 / tot as u128) as u64;
                }
            }
        }
    }

    /// Per-faction facts as `FoldFaction` would read them on `day`:
    /// members are paid citizens, active those with an active day in the
    /// last term (4 days). With `void_shades`, each Shade's own facts are
    /// removed as `RevealShade` does.
    pub fn faction_facts(&self, void_shades: bool, day: u32) -> [FactionFacts; FACTIONS] {
        let mut facts = [FactionFacts::default(); FACTIONS];
        let from = day.saturating_sub(TERM_DAYS);
        let row = |a: &Agent| FactionFacts {
            members: 1,
            active: (a.last_active_day != NONE && a.last_active_day >= from) as u64,
            path: a.facts,
        };
        for a in &self.agents {
            if a.state == JoinState::Settled {
                facts[a.faction as usize].add(&row(a)).expect("facts");
            }
        }
        if void_shades {
            for a in self
                .agents
                .iter()
                .filter(|a| a.shade && a.state == JoinState::Settled)
            {
                facts[a.faction as usize].remove(&row(a)).expect("void");
            }
        }
        facts
    }

    /// End of a term: Ministers and Wardens seated for the term, then the
    /// term's Mandate budget closed and claimed through the closed-form
    /// kernel (`mandate::MandateTerm`: stakers who completed, capped at 2×
    /// the average holding's term emission, the rest back to the reserve).
    fn term_end(&mut self, term: u32) {
        let officers = self.seat_officers(term);
        let laurel_pay = self.cfg.office_pay == OfficePay::Laurels;
        for f in 0..6 {
            let mut who = std::mem::take(&mut self.term_completers[f]);
            if laurel_pay {
                // Variant (O3): seated staker officers get extra shares.
                for &(a, sh, _) in officers.iter().filter(|x| x.2 == f as u8) {
                    let ag = &self.agents[a as usize];
                    if ag.stake == 0 {
                        continue;
                    }
                    let mut got = 0;
                    for _ in 0..sh {
                        got += self.mandate[f].complete(true).expect("office share");
                    }
                    who.push((a, got));
                }
            }
            let mut t = self.mandate[f];
            t.close(&mut self.reserve[f]).expect("close term");
            for (a, sh) in who {
                let pay = t.claim(&mut self.reserve[f], sh).expect("mandate claim");
                self.agents[a as usize].laurels += pay;
                self.agents[a as usize].earned.mandate += pay;
                self.stats.mandate_paid += pay;
            }
            t.sweep(&mut self.reserve[f], false).expect("sweep term");
            self.mandate[f] = MandateTerm::new(term + 1);
        }
    }

    /// Ministers and paid Wardens of the term (they served it: the sim
    /// seats them at its end from the term's activity). Returns (wallet,
    /// Mandate shares if offices pay in laurels, faction).
    fn seat_officers(&mut self, term: u32) -> Vec<(u32, u64, u8)> {
        let mut out = Vec::new();
        let bot_officers = self.cfg.bot_officers;
        // Ministers: 4 per faction among active humans who stand (skilled
        // and very skilled), weighted by sessions this term.
        let term_start = term * TERM_DAYS;
        for f in 0..6u8 {
            let mut cands: Vec<(u64, u32)> = self
                .agents
                .iter()
                .enumerate()
                .filter(|(_, a)| {
                    a.faction == f
                        && !a.shade
                        && (matches!(a.arch, Arch::Skilled | Arch::VerySkilled)
                            || (a.arch == Arch::Bot && bot_officers))
                        && a.last_active_day != NONE
                        && a.last_active_day >= term_start
                })
                .map(|(i, a)| (self.rng.below(1_000) * (1 + a.sessions as u64), i as u32))
                .collect();
            cands.sort_unstable_by_key(|x| std::cmp::Reverse(x.0));
            for &(_, a) in cands.iter().take(4) {
                self.agents[a as usize].minister_terms += 1;
                self.stats.minister_terms += 1;
                self.stats.bot_office_terms += (self.agents[a as usize].arch == Arch::Bot) as u64;
                out.push((a, 2, f));
            }
        }
        // Wardens: per (March, faction) with ≥ 3 holdings; paid when ≥ 12
        // distinct active voters (turnout 50% of active holders).
        let mut per: HashMap<(u32, u8), Vec<u32>> = HashMap::new();
        for x in &self.holds {
            if !x.alive || x.free_city() {
                continue;
            }
            let m = self.prov(x.prov).march;
            per.entry((m, x.faction)).or_default().push(x.owner);
        }
        let mut keys: Vec<(u32, u8)> = per.keys().copied().collect();
        keys.sort_unstable();
        for k in keys {
            let mut owners = per.remove(&k).unwrap_or_default();
            if owners.len() < 3 {
                continue;
            }
            owners.sort_unstable();
            owners.dedup();
            let active: Vec<u32> = owners
                .iter()
                .copied()
                .filter(|&a| {
                    let ag = &self.agents[a as usize];
                    ag.last_active_day != NONE && ag.last_active_day >= term_start
                })
                .collect();
            let ballots = active.iter().filter(|_| self.rng.chance(0.5)).count();
            if ballots < 12 {
                continue;
            }
            // The most engaged non-Shade human stands and wins (bots too in
            // the `bot_officers` variant).
            let warden = active
                .iter()
                .copied()
                .filter(|&a| {
                    !self.agents[a as usize].shade
                        && (bot_officers || self.agents[a as usize].arch != Arch::Bot)
                })
                .max_by_key(|&a| (self.agents[a as usize].sessions, a));
            if let Some(w) = warden {
                self.agents[w as usize].warden_terms += 1;
                self.stats.warden_terms += 1;
                self.stats.bot_office_terms += (self.agents[w as usize].arch == Arch::Bot) as u64;
                out.push((w, 1, k.1));
            }
        }
        out
    }

    pub fn finish(&mut self) {
        let b = self.end_bell;
        let last_term = self.cfg.days / TERM_DAYS - 1;
        // Freeze and bank every holding (BankAfterEnd) first, so the last
        // credits' 10% reach the last term's Mandate budget.
        for hid in 0..self.holds.len() as u32 {
            let pi = self.holds[hid as usize].prov;
            self.accrue(pi, b);
            let p = self.provs[pi as usize].as_mut().expect("prov");
            let x = &mut self.holds[hid as usize];
            if x.attached {
                let c = p.index.settle(&mut x.stake).expect("settle");
                self.distribute(hid, c);
            }
        }
        // Open sieges at T_end: stakes back to the attackers.
        let sieges: Vec<u32> = self.siege_holds.iter().copied().collect();
        for t in sieges {
            if let Some(rec) = self.holds[t as usize].siege.take() {
                self.escrow -= SIEGE_STAKE;
                self.escrow_uncounted(rec.attacker, !rec.stake_counted, false);
                self.agents[rec.attacker as usize].laurels += SIEGE_STAKE;
                self.agents[rec.attacker as usize].earned.siege_net += SIEGE_STAKE as i64;
            }
        }
        self.siege_holds.clear();
        self.term_end(last_term);
        self.stats.mandate_left = self.reserve.iter().map(|r| r.balance).sum();
    }

    pub fn laurels_minted(&self) -> (u128, u128) {
        let mut emitted = 0u128;
        let mut orphaned = 0u128;
        for p in self.provs.iter().flatten() {
            emitted += p.index.emitted;
            orphaned += p.index.orphaned;
        }
        (emitted, orphaned)
    }
}

pub const LAUREL: u64 = LAUREL_ONE;

impl Sim {
    /// A first holding's province at T_end: attached holdings, how many
    /// of them are first holdings, and its weight over the province mean.
    pub fn neighbourhood(&self, hid: u32) -> (u32, u32, f64) {
        let x = &self.holds[hid as usize];
        let p = self.prov(x.prov);
        let (mut n, mut firsts, mut w) = (0u32, 0u32, 0u64);
        for &g in &p.site_h {
            if g == NONE {
                continue;
            }
            let y = &self.holds[g as usize];
            if y.attached {
                n += 1;
                w += y.stake.weight;
                firsts += (y.h.order == 1) as u32;
            }
        }
        let rel = if x.attached && w > 0 {
            x.stake.weight as f64 * n as f64 / w as f64
        } else {
            f64::NAN
        };
        (n, firsts, rel)
    }
}
