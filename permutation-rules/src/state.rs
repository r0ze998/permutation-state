//! Authoritative world state. Everything here is serialized with borsh and
//! hashed into `state_root` (§15 phase 11). Entities live in `Vec`s indexed
//! by id; ids are never reused within a season (§0.1), so iteration by index
//! is iteration by ascending id (§0.2).

use crate::buildings::{Building, BuildingSet};
use crate::fixed::{Milli, MilliTroops};
use crate::hex::Hex;
use crate::map::Map;
use crate::rng::Seed;
use crate::tech::{Tech, TechSet};
use crate::units::UnitType;
use crate::RulesError;
use alloc::string::String;
use alloc::vec::Vec;
use borsh::{BorshDeserialize, BorshSerialize};
use sha2::{Digest, Sha256};

pub type CivId = u16;
pub type CityId = u32;
pub type UnitId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum DeclaredKind {
    Human,
    Agent,
    Undeclared,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Scores {
    pub dominion: u64,
    pub concord_raw: u64,
    pub science_total: u64,
    pub star_gate_stages: u8,
    pub star_gate_tick: Option<u16>,
    /// Highest total population reached so far (Concord new-high term).
    pub max_pop: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Civ {
    pub id: CivId,
    pub name: String,
    pub declared_kind: DeclaredKind,
    pub payout_wallet: [u8; 32],
    pub joined_tick: u16,

    pub gold: Milli,
    pub science_store: Milli,
    pub influence: Milli,
    pub iron: Milli,
    pub horses: Milli,

    pub techs: TechSet,
    pub research_queue: Vec<Tech>,

    pub order_bank: u16,
    /// Budget fixed at the start of the current tick (§4.1: cities counted at tick start).
    pub tick_budget: u16,
    /// Gold went negative in phase 7 of this tick (§6.1); read by phase 8.
    pub deficit: bool,
    /// Milli-troops lost in combat this tick (war weariness, §9.3).
    pub troops_lost: u32,
    pub last: LastYields,
    pub war_weariness: u32,
    /// Last tick with an aggressive act (§9.2).
    pub last_aggression: Option<u16>,
    pub ever_allied: bool,
    pub capital: Option<CityId>,
    pub last_city_lost: Option<u16>,
    pub protection_lost: bool,

    /// Ticks with at least one order since joining (Participation, §14.5).
    pub active_ticks: u16,
    /// USDC (6 decimals) spent on the Exchange this season, notional + fee (§11.3).
    pub exchange_spent: u64,
    /// USDC (6 decimals) delegated to the ER for this civ: deposit + sales − purchases.
    pub usdc: u64,
    /// Whole units bought on the Exchange this season, by `GoodKind` (§11.3 re-sale rule).
    pub exchange_bought: [u32; 5],

    pub scores: Scores,
}

impl Civ {
    pub fn is_aggressor(&self, tick: u16, window: u16) -> bool {
        matches!(self.last_aggression, Some(t) if tick.saturating_sub(t) < window)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Focus {
    Balanced,
    Food,
    Production,
    Gold,
    Science,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum QueueItem {
    Building(Building),
    /// `n` whole troops (≤ 20) spawned as one army.
    Troops {
        unit: UnitType,
        n: u8,
    },
    Scout,
    Settler,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct City {
    pub id: CityId,
    /// `None` = Free City (§9.4).
    pub owner: Option<CivId>,
    pub founder: CivId,
    pub founded_tick: u16,
    pub hex: Hex,
    pub pop: u32,
    pub food: Milli,
    pub prod: Milli,
    pub buildings: BuildingSet,
    pub loyalty: i32,
    pub defense: MilliTroops,
    pub attacked_this_tick: bool,
    pub focus: Focus,
    pub queue: Vec<QueueItem>,
    pub captured_tick: Option<u16>,
    pub captured_from: Option<CivId>,
    /// Civs that have already scored a capture of this city this season (§14.1).
    pub scored_by: Vec<CivId>,
    /// Whether the current holding can earn Dominion capture points (§14.1).
    pub capture_scores: bool,
    /// Remaining ticks of razing, if being razed (§8.3).
    pub razing: Option<u8>,
    pub heritage_until: Option<u16>,
    pub heritage_bonus: u32,
    pub alive: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Owner {
    Civ(CivId),
    Barbarian,
}

impl Owner {
    /// The owning civ, or `None` for barbarians.
    pub const fn civ(self) -> Option<CivId> {
        match self {
            Owner::Civ(c) => Some(c),
            Owner::Barbarian => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum StandingRule {
    None,
    AutoDefend { radius: u8 },
    Retreat { ratio_bps: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Unit {
    pub id: UnitId,
    pub owner: Owner,
    pub unit_type: UnitType,
    /// Armies: 500..=20000. Civilians: always 1000.
    pub troops: MilliTroops,
    pub hex: Hex,
    pub path: Vec<Hex>,
    pub last_moved: Option<u16>,
    pub used_full_mp: bool,
    pub standing: StandingRule,
    pub alive: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Relation {
    Peace,
    /// War becomes active at `active_from` (declared one tick earlier, §10.2)
    /// and ends at `peace_at` once peace is accepted.
    War {
        declared_by: CivId,
        casus_belli: bool,
        active_from: u16,
        peace_at: Option<u16>,
    },
    /// Bonds are escrowed gold (whole units) of the lower- and higher-id civ.
    Nap {
        until: u16,
        bond_low: u32,
        bond_high: u32,
    },
    Alliance {
        leaving_at: Option<u16>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ProposalKind {
    Peace,
    Nap { bond: u32 },
    Alliance,
}

/// A diplomatic offer awaiting acceptance on a later tick (§10, v0.1 §10.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Proposal {
    pub kind: ProposalKind,
    pub from: CivId,
    pub to: CivId,
    pub tick: u16,
}

/// Previous tick's per-tick yields, in whole units. Transfer caps (§10.5)
/// are defined against income, which is only known after phase 6.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct LastYields {
    pub gold: u32,
    pub iron: u32,
    pub horses: u32,
    pub max_city_food_surplus: u32,
    pub max_city_prod: u32,
}

/// A constant-product pool of one strategic good against gold (§11.2), in milli-units.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Pool {
    pub goods: i64,
    pub gold: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Specialty {
    Scientific,
    Mercantile,
    Agrarian,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CityState {
    pub id: u16,
    pub hex: Hex,
    pub pop: u32,
    pub defense: MilliTroops,
    pub specialty: Specialty,
    /// Influence per civ (index = civ id), in milli.
    pub influence: Vec<Milli>,
    pub suzerain: Option<CivId>,
    pub captured_by: Option<CivId>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct WorldState {
    pub ruleset_hash: [u8; 32],
    pub season_seed: Seed,
    /// Next tick to resolve. `ticks_per_season` means the season is over.
    pub tick: u16,
    /// Next phase of `tick` to run (§15.1). 0 = not started.
    pub phase_cursor: u8,
    pub tick_seed: Seed,
    pub map: Map,
    pub civs: Vec<Civ>,
    pub cities: Vec<City>,
    pub units: Vec<Unit>,
    pub city_states: Vec<CityState>,
    pub hubs: Vec<Hex>,
    /// Upper-triangular pair table, see `pair_index`.
    pub relations: Vec<Relation>,
    /// `grievance[a * n + v]` = grievance victim `v` holds against `a` (§9.1).
    pub grievance: Vec<u16>,
    /// Open diplomatic proposals (§10.6).
    pub proposals: Vec<Proposal>,
    /// Per pair (see `pair_index`): no war may be declared before this tick (§10.2 truce).
    pub truce_until: Vec<u16>,
    /// Gold AMM pools (§11.2): Iron/Gold, Horses/Gold.
    pub pools: Vec<Pool>,
    /// USDC ledger on the ER (§11.3): total delegated, and fee accumulators.
    pub usdc_deposited: u64,
    pub exchange_vault: u64,
    pub exchange_ops: u64,
    /// Head of the append-only event hash chain.
    pub event_head: [u8; 32],
}

impl WorldState {
    pub fn civ_count(&self) -> usize {
        self.civs.len()
    }

    /// Index into `relations` for the unordered pair {a, b}, a ≠ b.
    pub fn pair_index(&self, a: CivId, b: CivId) -> usize {
        debug_assert_ne!(a, b);
        let n = self.civs.len();
        let (lo, hi) = if a < b {
            (a as usize, b as usize)
        } else {
            (b as usize, a as usize)
        };
        lo * n - lo * (lo + 1) / 2 + (hi - lo - 1)
    }

    pub fn relation(&self, a: CivId, b: CivId) -> Relation {
        self.relations[self.pair_index(a, b)]
    }

    pub fn set_relation(&mut self, a: CivId, b: CivId, rel: Relation) {
        let i = self.pair_index(a, b);
        self.relations[i] = rel;
    }

    pub fn at_war(&self, a: CivId, b: CivId) -> bool {
        a != b
            && matches!(self.relation(a, b), Relation::War { active_from, peace_at, .. }
                if self.tick >= active_from && peace_at.is_none_or(|p| self.tick < p))
    }

    pub fn grievance(&self, aggressor: CivId, victim: CivId) -> u16 {
        self.grievance[aggressor as usize * self.civs.len() + victim as usize]
    }

    pub fn add_grievance(&mut self, aggressor: CivId, victim: CivId, amount: u16) {
        let n = self.civs.len();
        let g = &mut self.grievance[aggressor as usize * n + victim as usize];
        *g = g.saturating_add(amount);
    }

    pub fn living_cities_of(&self, civ: CivId) -> impl Iterator<Item = &City> {
        self.cities
            .iter()
            .filter(move |c| c.alive && c.owner == Some(civ))
    }

    pub fn city_count(&self, civ: CivId) -> u32 {
        self.living_cities_of(civ).count() as u32
    }

    pub fn unit_at(&self, hex: Hex) -> impl Iterator<Item = &Unit> {
        self.units.iter().filter(move |u| u.alive && u.hex == hex)
    }

    /// Append an event to the hash chain: `head = sha256(head ‖ tick ‖ kind ‖ payload)`.
    pub fn push_event(&mut self, kind: &[u8], payload: &[u8]) {
        let mut h = Sha256::new();
        h.update(self.event_head);
        h.update(self.tick.to_le_bytes());
        h.update([kind.len() as u8]);
        h.update(kind);
        h.update(payload);
        self.event_head = h.finalize().into();
    }

    /// `state_root = sha256(borsh(state))` (§15 phase 11).
    pub fn state_root(&self) -> Result<[u8; 32], RulesError> {
        let bytes = borsh::to_vec(self).map_err(|_| RulesError::Serialization)?;
        Ok(Sha256::digest(bytes).into())
    }
}
