//! Behaviour and economy assumptions of the simulator. **Everything in this
//! file is an assumption** (labelled [sim] in RESULTS.md): play profiles,
//! placeholder in-game costs and the draft doctrine knobs. The rules
//! themselves (clash, siege, holding accrual, laurel index, faction index,
//! pools, claims) come from `permutation_rules::frontier` unchanged.

use permutation_rules::fixed::Bps;
use permutation_rules::frontier::holding::Tier;
use permutation_rules::frontier::holding::{Resource, RESOURCES};
use permutation_rules::frontier::stance::Stance;
use permutation_rules::units::UnitType;

/// Player archetypes (design §2.7 and the M0 brief).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Arch {
    /// Pays and founds a holding, then never plays (passive or Sybil wallet).
    Idle,
    /// 15 minutes, 3 days a week.
    Casual,
    /// 30 minutes every day.
    Daily,
    /// 1–2 hours a day, good decisions.
    Skilled,
    /// ~4 hours a day, best decisions (the "whale, one wallet" of §2.7).
    VerySkilled,
    /// Scripted wallet at the public SDK's default strategy: acts every
    /// hour within the action bucket, never withholds a reveal.
    Bot,
}

pub const ARCHS: [Arch; 6] = [
    Arch::Idle,
    Arch::Casual,
    Arch::Daily,
    Arch::Skilled,
    Arch::VerySkilled,
    Arch::Bot,
];

impl Arch {
    pub fn name(self) -> &'static str {
        match self {
            Arch::Idle => "idle wallet",
            Arch::Casual => "casual 3x/week",
            Arch::Daily => "daily 30 min",
            Arch::Skilled => "skilled 1-2 h",
            Arch::VerySkilled => "very skilled 4 h",
            Arch::Bot => "scripted bot",
        }
    }
    pub fn idx(self) -> usize {
        self as usize
    }
}

/// How an archetype plays. [sim]
#[derive(Clone, Copy, Debug)]
pub struct Profile {
    /// Probability of playing on a given day.
    pub day_p: f64,
    /// Sessions on a day played.
    pub sessions: u32,
    /// Actions per session (the per-wallet bucket is 30/h, burst 60).
    pub actions: u32,
    /// Decision quality 0..1: estimate noise, stance play, build order.
    pub q: f64,
    /// Probability of looking for a fight in a session.
    pub aggression: f64,
    /// Share of surplus goods pledged to the Engine and research.
    pub pledge: f64,
    /// Probability of taking the term's Mandate when online.
    pub mandate: f64,
    /// Probability of checking a building's payback and saving for the
    /// next tier before buying (economic care; the SDK does it always).
    pub thrift: f64,
    /// Probability that a committed posture or a march goes unrevealed
    /// (tlock auto-reveal is the default, so this is small).
    pub withhold: f64,
}

pub fn profile(a: Arch) -> Profile {
    match a {
        Arch::Idle => Profile {
            day_p: 0.0,
            sessions: 0,
            actions: 0,
            q: 0.0,
            aggression: 0.0,
            pledge: 0.0,
            mandate: 0.0,
            thrift: 0.0,
            withhold: 0.0,
        },
        Arch::Casual => Profile {
            day_p: 3.0 / 7.0,
            sessions: 1,
            actions: 15,
            q: 0.3,
            aggression: 0.15,
            pledge: 0.10,
            mandate: 0.2,
            thrift: 0.4,
            withhold: 0.02,
        },
        Arch::Daily => Profile {
            day_p: 1.0,
            sessions: 1,
            actions: 30,
            q: 0.5,
            aggression: 0.3,
            pledge: 0.15,
            mandate: 0.5,
            thrift: 0.6,
            withhold: 0.01,
        },
        Arch::Skilled => Profile {
            day_p: 1.0,
            sessions: 2,
            actions: 45,
            q: 0.75,
            aggression: 0.5,
            pledge: 0.20,
            mandate: 0.8,
            thrift: 0.85,
            withhold: 0.005,
        },
        Arch::VerySkilled => Profile {
            day_p: 1.0,
            sessions: 4,
            actions: 60,
            q: 0.9,
            aggression: 0.6,
            pledge: 0.20,
            mandate: 0.95,
            thrift: 0.95,
            withhold: 0.0,
        },
        Arch::Bot => Profile {
            day_p: 1.0,
            sessions: 24,
            actions: 30,
            q: 0.6,
            aggression: 0.5,
            pledge: 0.20,
            mandate: 1.0,
            thrift: 1.0,
            withhold: 0.0,
        },
    }
}

// ------------------------------------------------------------ economy [sim]

/// Base production of a Hamlet per hour (units), by `Resource` order.
pub const BASE_PROD: [i64; RESOURCES] = [40, 30, 20, 15, 0, 15, 5, 0];
/// Extra base production by tier, percent (a wider worked radius).
pub fn tier_bonus_pct(t: Tier) -> i64 {
    match t {
        Tier::Hamlet => 0,
        Tier::Town => 50,
        Tier::City => 100,
        Tier::Stronghold => 175,
    }
}

/// Buildings: what they add per hour and their base cost.
#[derive(Clone, Copy, Debug)]
pub struct Building {
    pub resource: Resource,
    pub per_hour: i64,
    pub cost: [i64; RESOURCES],
}

pub const BUILDINGS: [Building; 6] = [
    Building {
        resource: Resource::Food,
        per_hour: 12,
        cost: [0, 80, 40, 0, 0, 0, 0, 0],
    },
    Building {
        resource: Resource::Wood,
        per_hour: 10,
        cost: [40, 0, 40, 0, 0, 0, 0, 0],
    },
    Building {
        resource: Resource::Stone,
        per_hour: 8,
        cost: [40, 80, 0, 0, 0, 0, 0, 0],
    },
    Building {
        resource: Resource::Ore,
        per_hour: 6,
        cost: [40, 80, 20, 0, 0, 0, 0, 0],
    },
    Building {
        resource: Resource::Gold,
        per_hour: 6,
        cost: [40, 60, 40, 0, 0, 0, 0, 0],
    },
    Building {
        resource: Resource::Science,
        per_hour: 3,
        cost: [0, 60, 60, 0, 0, 20, 0, 0],
    },
];

/// Seconds a building takes: 1 h × (1 + n/2) for the n-th copy.
pub fn build_secs(n: u32) -> i64 {
    3_600 + 1_800 * n as i64
}

/// Tier-up cost (to the next tier) and duration.
pub fn tier_up(t: Tier) -> Option<([i64; RESOURCES], i64)> {
    match t {
        Tier::Hamlet => Some(([0, 1_000, 600, 0, 0, 300, 0, 0], 6 * 3_600)),
        Tier::Town => Some(([0, 4_000, 3_000, 0, 0, 1_500, 0, 0], 12 * 3_600)),
        Tier::City => Some(([0, 12_000, 10_000, 0, 0, 5_000, 0, 0], 24 * 3_600)),
        Tier::Stronghold => None,
    }
}

/// Garrison an owner aims for, by tier (troops).
pub fn garrison_target(t: Tier) -> i64 {
    match t {
        Tier::Hamlet => 300,
        Tier::Town => 800,
        Tier::City => 2_000,
        Tier::Stronghold => 5_000,
    }
}

/// Cost per 100 troops of a Spearman-class unit: food, ore, gold (units);
/// scaled by the unit's production cost relative to a Spearman (6).
pub const TROOP_COST_PER_100: [i64; 3] = [60, 20, 10];

/// Walls: +100 wall points for this much stone (units) and 4 hours.
pub const WALL_STEP: u32 = 100;
pub const WALL_COST_STONE: i64 = 300;

/// A second or third holding (a settler): base cost, × `duplicate_cost`.
pub const SETTLER_COST: [i64; RESOURCES] = [800, 800, 400, 0, 0, 200, 0, 0];

/// Starter kit of a new first holding (units).
pub const STARTER_KIT: [i64; RESOURCES] = [300, 300, 200, 100, 0, 100, 0, 0];

/// Works: daily cap, and what each source gives. [sim] The cap was 60;
/// the review of 2026-09-27 made 20 the default (RESULTS F1a: a fee-only
/// bot returns 0.92× at 20 against 1.01× at 60, humans unchanged).
pub const WORKS_DAY_CAP: u64 = 20;
pub const WORKS_EXPLORE: u64 = 4;
pub const WORKS_CAMP: u64 = 10;
pub const WORKS_PLEDGE: u64 = 2;
pub const WORKS_MANDATE: u64 = 6;

/// A wallet is an Engine builder after pledging this much (units).
pub const BUILDER_THRESHOLD: u64 = 400;
/// Engine stage k completes when Σ Engine pledges ≥ k(k+1)/2 × this ×
/// settled holdings (design §5.5: thresholds scale with settled holdings;
/// calibrated so the five stages spread over the season).
pub const ENGINE_PER_HOLDING: u64 = 450;

// ------------------------------------------------------------ doctrines [sim]

/// The draft doctrine table of design §4.1, as simulator knobs. Only the
/// parts with a simulated mechanism are live; the rest are listed in
/// RESULTS.md as not simulated.
#[derive(Clone, Copy, Debug)]
pub struct Doctrine {
    pub name: &'static str,
    /// Host unit (the doctrine's unit variant).
    pub unit: UnitType,
    /// Stance the unit variant is good at, and the damage multiplier then.
    pub stance_bonus: Option<(Stance, Bps)>,
    /// Damage multiplier on the bell a host arrives.
    pub arrival_bps: Bps,
    /// Travel time multiplier (roads, Waystones) on top of cavalry.
    pub travel_bps: Bps,
    /// Production multipliers.
    pub food_bps: Bps,
    pub ore_bps: Bps,
    /// Troop upkeep multiplier.
    pub upkeep_bps: Bps,
    /// Wall cost multiplier.
    pub wall_cost_bps: Bps,
    /// Settler (new holding) cost multiplier.
    pub settler_cost_bps: Bps,
    /// Frontier Grant multiplier for its below-average-size cohorts.
    pub grant_mult: i64,
    /// Science production multiplier (research focus slot).
    pub science_bps: Bps,
    /// Science pledges count this much toward Knowledge.
    pub knowledge_bps: Bps,
}

pub const NEUTRAL_DOCTRINE: Doctrine = Doctrine {
    name: "none",
    unit: UnitType::Spearman,
    stance_bonus: None,
    arrival_bps: 10_000,
    travel_bps: 10_000,
    food_bps: 10_000,
    ore_bps: 10_000,
    upkeep_bps: 10_000,
    wall_cost_bps: 10_000,
    settler_cost_bps: 10_000,
    grant_mult: 1,
    science_bps: 10_000,
    knowledge_bps: 10_000,
};

/// Draft set, design §4.1 (A–F), as knobs.
pub const DOCTRINES: [Doctrine; 6] = [
    Doctrine {
        name: "A Wardens of Stone",
        stance_bonus: Some((Stance::Brace, 11_000)),
        wall_cost_bps: 9_000,
        ..NEUTRAL_DOCTRINE
    },
    Doctrine {
        name: "B Tide",
        unit: UnitType::Horseman,
        travel_bps: 8_000,
        ..NEUTRAL_DOCTRINE
    },
    Doctrine {
        name: "C Ember",
        stance_bonus: Some((Stance::Assault, 11_000)),
        arrival_bps: 11_000,
        ..NEUTRAL_DOCTRINE
    },
    Doctrine {
        name: "D Verdant",
        stance_bonus: Some((Stance::Flank, 11_000)),
        food_bps: 11_000,
        settler_cost_bps: 8_000,
        grant_mult: 2,
        ..NEUTRAL_DOCTRINE
    },
    Doctrine {
        name: "E Lumen",
        travel_bps: 7_500,
        science_bps: 12_000,
        knowledge_bps: 12_000,
        ..NEUTRAL_DOCTRINE
    },
    Doctrine {
        name: "F Iron",
        unit: UnitType::Knight,
        ore_bps: 11_000,
        upkeep_bps: 9_000,
        ..NEUTRAL_DOCTRINE
    },
];

/// Tuned proposal: the draft set without direct multipliers on scored
/// facts or on growth (Verdant's food, cheaper settlers and doubled
/// Frontier Grant; Lumen's science and Knowledge weight; Iron's ore).
/// Unit variants, stance bonuses, travel, walls and upkeep stay.
pub const DOCTRINES_TUNED: [Doctrine; 6] = [
    DOCTRINES[0],
    DOCTRINES[1],
    DOCTRINES[2],
    Doctrine {
        food_bps: 10_000,
        settler_cost_bps: 10_000,
        grant_mult: 1,
        ..DOCTRINES[3]
    },
    Doctrine {
        science_bps: 10_000,
        knowledge_bps: 10_000,
        ..DOCTRINES[4]
    },
    Doctrine {
        ore_bps: 10_000,
        ..DOCTRINES[5]
    },
];

pub fn doctrine_set(tuned: bool) -> &'static [Doctrine; 6] {
    if tuned {
        &DOCTRINES_TUNED
    } else {
        &DOCTRINES
    }
}
