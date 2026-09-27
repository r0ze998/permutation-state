//! Behaviour and economy assumptions of the simulator. **Everything in this
//! file is an assumption** (labelled [sim] in RESULTS.md): play profiles,
//! placeholder in-game costs and the draft doctrine replays. The rules
//! themselves (clash, siege, holding accrual, laurel index, faction index,
//! pools, claims, and the doctrine table) come from
//! `permutation_rules::frontier` unchanged.

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

use permutation_rules::fixed::BPS_ONE;
pub use permutation_rules::frontier::doctrine::Doctrine as KernelDoctrine;
use permutation_rules::frontier::doctrine::{DOCTRINES as KERNEL_DOCTRINES, NEUTRAL};

/// A doctrine as the simulator plays it: the kernel's knobs
/// (`permutation_rules::frontier::doctrine`) plus the draft table's
/// multipliers on scored facts and growth, which O5 removed from the
/// kernel. They exist here only to replay the draft (§E1) and the M0
/// proposal; in the kernel set they are all neutral.
#[derive(Clone, Copy, Debug)]
pub struct Doctrine {
    pub k: KernelDoctrine,
    /// Production multipliers (draft only).
    pub food_bps: Bps,
    pub ore_bps: Bps,
    /// Settler (new holding) cost multiplier (draft only).
    pub settler_cost_bps: Bps,
    /// Frontier Grant multiplier for below-average-size cohorts (draft only).
    pub grant_mult: i64,
    /// Science production multiplier (draft only: research focus slot).
    pub science_bps: Bps,
    /// Science pledges count this much toward Knowledge (draft only).
    pub knowledge_bps: Bps,
}

impl Doctrine {
    pub const fn kernel(k: KernelDoctrine) -> Doctrine {
        Doctrine {
            k,
            food_bps: BPS_ONE,
            ore_bps: BPS_ONE,
            settler_cost_bps: BPS_ONE,
            grant_mult: 1,
            science_bps: BPS_ONE,
            knowledge_bps: BPS_ONE,
        }
    }
    pub const fn name(&self) -> &'static str {
        self.k.name
    }
}

pub const NEUTRAL_DOCTRINE: Doctrine = Doctrine::kernel(NEUTRAL);

/// Which doctrine table a run plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoctrineSet {
    /// The rules-v10 table (`permutation_rules::frontier::doctrine::DOCTRINES`).
    Kernel,
    /// The draft table of design §4.1 (with its multipliers on scored facts).
    Draft,
    /// The M0 proposal: the draft without those multipliers (M0 §E2).
    M0,
}

impl DoctrineSet {
    pub fn parse(s: &str) -> DoctrineSet {
        match s {
            "kernel" => DoctrineSet::Kernel,
            "draft" => DoctrineSet::Draft,
            "m0" => DoctrineSet::M0,
            x => panic!("doctrine set {x}: kernel, draft or m0"),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            DoctrineSet::Kernel => "rules-v10 kernel table",
            DoctrineSet::Draft => "draft table of design §4.1",
            DoctrineSet::M0 => "M0 proposal (draft minus multipliers on scored facts)",
        }
    }
}

const fn kd(name: &'static str) -> KernelDoctrine {
    KernelDoctrine { name, ..NEUTRAL }
}

/// Draft set, design §4.1 (A–F), as knobs (M0's `DOCTRINES`).
pub const DRAFT_DOCTRINES: [Doctrine; 6] = [
    Doctrine::kernel(KernelDoctrine {
        drill: Some((Stance::Brace, 11_000)),
        wall_cost_bps: 9_000,
        ..kd("A Wardens of Stone")
    }),
    Doctrine::kernel(KernelDoctrine {
        unit: UnitType::Horseman,
        travel_bps: 8_000,
        ..kd("B Tide")
    }),
    Doctrine::kernel(KernelDoctrine {
        drill: Some((Stance::Assault, 11_000)),
        arrival_bps: 11_000,
        ..kd("C Ember")
    }),
    Doctrine {
        food_bps: 11_000,
        settler_cost_bps: 8_000,
        grant_mult: 2,
        ..Doctrine::kernel(KernelDoctrine {
            drill: Some((Stance::Flank, 11_000)),
            ..kd("D Verdant")
        })
    },
    Doctrine {
        science_bps: 12_000,
        knowledge_bps: 12_000,
        ..Doctrine::kernel(KernelDoctrine {
            travel_bps: 7_500,
            ..kd("E Lumen")
        })
    },
    Doctrine {
        ore_bps: 11_000,
        ..Doctrine::kernel(KernelDoctrine {
            unit: UnitType::Knight,
            upkeep_bps: 9_000,
            keeps_walls_on_capture: true,
            ..kd("F Iron")
        })
    },
];

/// The M0 proposal (M0's `DOCTRINES_TUNED`): the draft set without direct
/// multipliers on scored facts or on growth.
pub const M0_DOCTRINES: [Doctrine; 6] = [
    DRAFT_DOCTRINES[0],
    DRAFT_DOCTRINES[1],
    DRAFT_DOCTRINES[2],
    Doctrine::kernel(DRAFT_DOCTRINES[3].k),
    Doctrine::kernel(DRAFT_DOCTRINES[4].k),
    Doctrine::kernel(DRAFT_DOCTRINES[5].k),
];

/// The table a run plays, with `tweaks` applied (`frontier-sim --dx`).
pub fn doctrine_table(set: DoctrineSet, tweaks: &str) -> [Doctrine; 6] {
    let mut t = match set {
        DoctrineSet::Kernel => KERNEL_DOCTRINES.map(Doctrine::kernel),
        DoctrineSet::Draft => DRAFT_DOCTRINES,
        DoctrineSet::M0 => M0_DOCTRINES,
    };
    apply_tweaks(&mut t, tweaks);
    t
}

/// Tuning overrides: `"C.arrival=10500,D.drill=10500,F.unit=pikeman"`.
/// Keys: drill (bps of the existing drill), stance (hold/assault/flank/
/// brace/none), arrival, travel, upkeep, walls, attrition, supply,
/// keepwalls (0/1), unit (spearman/archer/horseman/pikeman/crossbowman/
/// knight).
pub fn apply_tweaks(t: &mut [Doctrine; 6], tweaks: &str) {
    for tw in tweaks.split(',').map(str::trim).filter(|x| !x.is_empty()) {
        let (lhs, v) = tw.split_once('=').expect("tweak: X.key=value");
        let (d, key) = lhs.split_once('.').expect("tweak: X.key=value");
        let i = (d.as_bytes()[0].to_ascii_uppercase() - b'A') as usize;
        assert!(i < 6, "tweak doctrine {d}");
        let k = &mut t[i].k;
        let num = || v.parse::<u32>().expect("tweak value");
        match key {
            "drill" => {
                let (s, _) = k.drill.expect("tweak drill: doctrine has no drill");
                k.drill = Some((s, num()));
            }
            "stance" => {
                let bps = k.drill.map_or(11_000, |x| x.1);
                k.drill = match v {
                    "none" => None,
                    "assault" => Some((Stance::Assault, bps)),
                    "flank" => Some((Stance::Flank, bps)),
                    "brace" => Some((Stance::Brace, bps)),
                    x => panic!("stance {x}"),
                };
            }
            "arrival" => k.arrival_bps = num(),
            "variant" => k.variant_bps = num(),
            "travel" => k.travel_bps = num(),
            "upkeep" => k.upkeep_bps = num(),
            "walls" => k.wall_cost_bps = num(),
            "attrition" => k.attrition_bps = num(),
            "supply" => k.supply_range_bonus = num() as u8,
            "keepwalls" => k.keeps_walls_on_capture = num() != 0,
            "unit" => {
                k.unit = match v {
                    "spearman" => UnitType::Spearman,
                    "archer" => UnitType::Archer,
                    "horseman" => UnitType::Horseman,
                    "pikeman" => UnitType::Pikeman,
                    "crossbowman" => UnitType::Crossbowman,
                    "knight" => UnitType::Knight,
                    x => panic!("unit {x}"),
                }
            }
            x => panic!("tweak key {x}"),
        }
    }
}

#[cfg(test)]
mod economy_parity {
    use super::*;

    /// The shipping (kernel) doctrine set has no economic multipliers: the
    /// program's neutral `catalog::base_production` and building deltas
    /// (W3-B's Build and tier-up touch) are the simulator's economy. The
    /// food/ore/science multipliers exist only in the draft table (wave-3
    /// review, W3-B "doctrine economy": rebutted by this pin; a change here
    /// needs the kernel catalog to carry the multipliers first, I-56).
    #[test]
    fn kernel_set_economy_is_neutral() {
        for d in doctrine_table(DoctrineSet::Kernel, "") {
            assert_eq!(
                (d.food_bps, d.ore_bps, d.science_bps),
                (BPS_ONE, BPS_ONE, BPS_ONE),
                "{}",
                d.name()
            );
        }
    }
}
