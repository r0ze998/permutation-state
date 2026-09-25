//! Authoritative world state. Everything here is serialized with borsh and
//! hashed into `state_root` (§15 phase 11). Entities live in `Vec`s indexed
//! by id; ids are never reused within a season (§0.1), so iteration by index
//! is iteration by ascending id (§0.2).

use crate::buildings::{Building, BuildingSet};
use crate::fixed::{Milli, MilliTroops};
use crate::gov::{Credit, Member, MemberId, Nation};
use crate::hex::Hex;
use crate::map::Map;
use crate::rng::Seed;
use crate::tech::{Tech, TechSet};
use crate::units::UnitType;
use crate::RulesError;
use alloc::string::String;
use alloc::vec::Vec;
use borsh::{BorshDeserialize, BorshSerialize};

pub type CivId = u16;
pub type CityId = u32;
pub type UnitId = u32;

/// Self-declared kind of a member (V5 D16). Cosmetic: no rule reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum DeclaredKind {
    Human,
    Agent,
    Undeclared,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Scores {
    pub science_total: u64,
    /// Stages standing now (lost with the city).
    pub star_gate_stages: u8,
    pub star_gate_tick: Option<u16>,
}

/// Progress a nation keeps for its milestones (V5 §6). Every field only
/// grows, except the dark-age mark.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Achievements {
    /// Gold earned by the nation's own economy this season, whole units ("富").
    pub wealth: u64,
    /// Trade volume in whole gold per counterparty: index = civ id, and the
    /// last slot is the gold market (V5 §6.2 "交易量").
    pub trade: Vec<u64>,
    pub ever_suzerain: bool,
    pub envoy_sent: bool,
    /// Most Star Gate stages ever completed.
    pub star_gate_max: u8,
    /// Cities of at least `conquest_min_pop` taken from another nation
    /// (banked: hegemony tier 3).
    pub conquests: u32,
    /// Enemy troops destroyed, whole troops (a eureka trigger).
    pub kills: u32,
    /// Techs whose eureka fired (cheaper to research).
    pub boosted: crate::tech::TechSet,
    /// Dark age (catch-up) until this tick, if the nation fell far behind.
    pub dark_age_until: Option<u16>,
    /// Provisional tiers per path and era at the last scoring phase, for announcements.
    pub tiers: [u8; 4],
    pub era: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Civ {
    pub id: CivId,
    pub name: String,

    pub gold: Milli,
    pub science_store: Milli,
    pub influence: Milli,
    pub iron: Milli,
    pub horses: Milli,

    pub techs: TechSet,
    pub research_queue: Vec<Tech>,
    /// Who set the research queue (merit for completed techs, V5 §7.3).
    pub research_credit: Credit,

    /// Nation budget `B` fixed at the start of the current tick (§4.1:
    /// cities counted at tick start), split among the offices (V5 §5.2).
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
    /// Tick of the last Star Gate stage completed (v0.2 C2 spacing).
    pub last_star_gate: Option<u16>,

    /// The nation treasury on the ER, USDC base units: deposits + sales −
    /// purchases (V5 §7.5).
    pub usdc: u64,
    /// USDC spent on the market this season, notional + fee + tariff: the
    /// position on the tariff curve (V5 §7.5).
    pub market_spent: u64,
    /// Whole units bought on the market this season, by `GoodKind` (re-sale rule).
    pub exchange_bought: [u32; 5],
    /// USDC received from contracts (V5 §18.6), part of `usdc`: it may not
    /// be spent on the market, only refunded to depositors at the end.
    pub contract_income: u64,

    pub scores: Scores,
    pub achievements: Achievements,
}

impl Civ {
    /// Treasury USDC the nation may spend: all of it but what contracts
    /// paid in, which only goes back to depositors (V5 §18.6).
    pub fn free_usdc(&self) -> u64 {
        self.usdc.saturating_sub(self.contract_income)
    }

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
    /// Last officer to set the queue (merit for completed items, V5 §7.3).
    pub queue_credit: Credit,
    /// Last officer to set the queue or the focus (merit for growth).
    pub steward_credit: Credit,
    pub captured_tick: Option<u16>,
    pub captured_from: Option<CivId>,
    /// Whether the current holding counts as a captured city held (V5 §6.2):
    /// founded by another civ, and old enough when taken.
    pub capture_scores: bool,
    /// Remaining ticks of razing, if being razed (§8.3).
    pub razing: Option<u8>,
    pub heritage_until: Option<u16>,
    pub heritage_bonus: u32,
    pub standing: CityStanding,
    pub alive: bool,
    /// The first conquest of this city after `ai_home_tick` (V5 §18.3): an
    /// operator AI's home city pays its bounty to that nation.
    pub first_conquest: Option<Conquest>,
}

impl City {
    /// A city `civ` founds at `hex` on `tick`: pop 1, full loyalty, the base
    /// defense, nothing queued. Genesis capitals, founded cities and captured
    /// city-states start from it.
    pub fn founded(
        id: CityId,
        civ: CivId,
        hex: Hex,
        tick: u16,
        rules: &crate::params::Ruleset,
    ) -> City {
        City {
            id,
            owner: Some(civ),
            founder: civ,
            founded_tick: tick,
            hex,
            pop: 1,
            food: 0,
            prod: 0,
            buildings: BuildingSet::default(),
            loyalty: 100,
            defense: (rules.city_defense_base + 1) * 1000,
            attacked_this_tick: false,
            focus: Focus::Balanced,
            queue: Vec::new(),
            queue_credit: Credit::NONE,
            steward_credit: Credit::NONE,
            captured_tick: None,
            captured_from: None,
            capture_scores: false,
            razing: None,
            heritage_until: None,
            heritage_bonus: 0,
            standing: CityStanding::DEFAULT,
            alive: true,
            first_conquest: None,
        }
    }
}

/// A city's first conquest after the home cities were drawn (V5 §18.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Conquest {
    pub by: CivId,
    pub tick: u16,
    /// False when the captor and the victim had a NAP or an alliance within
    /// `bounty_pact_window` ticks: a bounty is not paid (V5 §18.4).
    pub bounty: bool,
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

/// A unit's standing rule (§13), executed free in phase 3 on ticks without a
/// manual order for that unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum StandingRule {
    None,
    /// Attack the weakest hostile army in range while it is within `radius` of `anchor`.
    AutoDefend {
        radius: u8,
        anchor: Hex,
    },
    /// Step toward the nearest own city when adjacent hostile strength exceeds `ratio_bps` of own.
    Retreat {
        ratio_bps: u32,
    },
    /// Walk `route[..len]` in a loop; `next` is the waypoint being approached.
    Patrol {
        route: [Hex; 6],
        len: u8,
        next: u8,
    },
}

/// A city's standing rules (§13).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CityStanding {
    /// `CityQueueRepeat`: when the queue empties, repeat the last unit item.
    pub repeat_queue: bool,
    /// `AutoPurchase`: gold per tick applied to the current item; 0 = off.
    pub auto_purchase: u32,
}

impl CityStanding {
    pub const DEFAULT: CityStanding = CityStanding {
        repeat_queue: true,
        auto_purchase: 0,
    };
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

impl Relation {
    pub const fn is_alliance(self) -> bool {
        matches!(self, Relation::Alliance { .. })
    }

    pub const fn is_nap(self) -> bool {
        matches!(self, Relation::Nap { .. })
    }

    /// A NAP or an alliance: a treaty partner ("条約相手", V5 §6.2).
    pub const fn is_pact(self) -> bool {
        matches!(self, Relation::Nap { .. } | Relation::Alliance { .. })
    }

    /// A war declared and not yet ended by a peace (it may not be active
    /// yet: war starts the tick after its declaration).
    pub const fn is_declared_war(self) -> bool {
        matches!(self, Relation::War { peace_at: None, .. })
    }
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
    /// The proposing diplomat (treaty merit, V5 §7.3).
    pub credit: Credit,
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
    /// Influence sent, per (civ, officer): suzerain merit is shared by it (V5 §7.3).
    pub envoys: Vec<EnvoyShare>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EnvoyShare {
    pub civ: CivId,
    pub credit: Credit,
    pub influence: u64,
}

/// An order that did not take effect in the tick just resolved (v0.2 C12).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Skip {
    pub civ: CivId,
    pub role: u8,
    /// Position in that office's batch (own orders, then adopted proposals);
    /// `u16::MAX` = the whole batch.
    pub index: u16,
    /// `checks::Blocked::code`.
    pub reason: u8,
}

/// Goods bought on the market, arriving at `due` (V5 §7.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Delivery {
    pub civ: CivId,
    pub good: crate::orders::Good,
    pub qty: u32,
    pub due: u16,
}

/// Merit credited during the current tick, for clients (not hashed: the
/// totals are in `Member::merit`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeritEntry {
    pub member: MemberId,
    pub path: crate::gov::Path,
    pub milli: u32,
    /// Event kind, e.g. `b"capture"`.
    pub what: &'static [u8],
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
    /// Orders compiled from standing rules in phase 3, consumed by phases 4–5
    /// and cleared at commit (§13). Kept in state so phases can resume.
    pub implicit: Vec<(CivId, crate::orders::Order)>,
    /// Grievance added to each `grievance` entry during the current tick: it
    /// does not decay in that tick (v0.2 C7). Cleared by the decay in phase 8.
    pub grievance_fresh: Vec<u16>,

    // --- V5 ---
    pub members: Vec<Member>,
    /// One government per civ.
    pub nations: Vec<Nation>,
    /// The orders accepted for this tick, merged per civ in phase 0 and
    /// cleared at commit.
    pub tick_orders: Vec<crate::orders::CivOrders>,
    pub last_skipped: Vec<Skip>,
    pub deliveries: Vec<Delivery>,

    // --- V5 §18 ---
    /// Living cities of each nation at the end of `ai_home_tick`, by id
    /// (empty before): operator AI home cities are drawn from them.
    pub home_snapshot: Vec<Vec<CityId>>,
    /// Per pair (see `pair_index`): the last tick the pair had a NAP or an
    /// alliance.
    pub pact_last: Vec<Option<u16>>,
    /// Contracts escrowed from nation treasuries (V5 §18.6), by ascending id.
    pub contracts: Vec<crate::contracts::Contract>,
    pub next_contract: u32,
    /// Not hashed; see `MeritEntry`.
    #[borsh(skip)]
    pub merit_log: Vec<MeritEntry>,
}

/// Unordered pairs among `n` civs: the length of the per-pair tables
/// (`relations`, `truce_until`, `pact_last`).
pub const fn pair_count(n: usize) -> usize {
    n * n.saturating_sub(1) / 2
}

impl WorldState {
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

    /// At war now, or a war declared that is not active yet.
    pub fn war_pending(&self, a: CivId, b: CivId) -> bool {
        a != b && (self.at_war(a, b) || self.relation(a, b).is_declared_war())
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
        let i = aggressor as usize * n + victim as usize;
        self.grievance[i] = self.grievance[i].saturating_add(amount);
        self.grievance_fresh[i] = self.grievance_fresh[i].saturating_add(amount);
    }

    /// Record an order that did not take effect.
    pub fn skip(&mut self, civ: CivId, origin: (u8, u16), reason: u8) {
        self.last_skipped.push(Skip {
            civ,
            role: origin.0,
            index: origin.1,
            reason,
        });
    }

    /// The nation whose living city owns the territory `tile` lies in.
    pub fn tile_owner(&self, tile: &crate::map::Tile) -> Option<CivId> {
        tile.owner_city
            .and_then(|c| self.cities.get(c as usize))
            .filter(|c| c.alive)?
            .owner
    }

    /// The nation whose living city owns the territory at `hex`.
    pub fn territory_owner(&self, hex: Hex) -> Option<CivId> {
        self.tile_owner(self.map.tile(hex)?)
    }

    pub fn living_cities_of(&self, civ: CivId) -> impl Iterator<Item = &City> {
        self.cities
            .iter()
            .filter(move |c| c.alive && c.owner == Some(civ))
    }

    pub fn city_count(&self, civ: CivId) -> u32 {
        self.living_cities_of(civ).count() as u32
    }

    /// Append an event to the hash chain: `head = sha256(head ‖ tick ‖ kind ‖ payload)`.
    pub fn push_event(&mut self, kind: &[u8], payload: &[u8]) {
        self.event_head = crate::hash::sha256(&[
            &self.event_head,
            &self.tick.to_le_bytes(),
            &[kind.len() as u8],
            kind,
            payload,
        ]);
    }

    /// `state_root = sha256(borsh(state))` (§15 phase 11).
    pub fn state_root(&self) -> Result<[u8; 32], RulesError> {
        let bytes = borsh::to_vec(self).map_err(|_| RulesError::Serialization)?;
        Ok(crate::hash::sha256(&[&bytes]))
    }
}
