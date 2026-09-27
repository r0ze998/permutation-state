//! The M1 build, train and starter catalog (contract §7, I-41, I-56).
//!
//! **Normative source: `frontier-sim/src/model.rs`** and the `sim.rs`
//! rules the contract names (build l. 1480–1505, walls l. 1508–1530,
//! train l. 1535–1560 and the variant surcharge l. 1790–1800). Every
//! table here is copied field for field; `frontier-sim/tests/
//! catalog_equality.rs` (W1-D) proves the copy equal. The tables are part
//! of the ruleset hash ([`super::ruleset_hash`]).
//!
//! Amounts in the tables are whole units, as in `model.rs`; the functions
//! return [`Cost`] in milli-units (`Holding::pay` takes milli), and
//! production in milli-units per hour (`Effect::Production`).
//!
//! **Doctrine multipliers.** The simulator's draft doctrines scale food,
//! ore and science production; the kernel doctrine table (rules v10, O5)
//! has no such field, so every multiplier here is neutral and only
//! `Doctrine::wall_cost` applies (walls).
//!
//! **Training is immediate** (sim; I-56): [`train`] returns a cost, no
//! duration, and there is no `Effect::Troops`.

use crate::fixed::{Milli, MILLI};
use crate::units::{stats, UnitType};

use super::doctrine::Doctrine;
use super::holding::{Effect, Resource, Tier, HOUR, RESOURCES};

/// Kernel version of this module (part of the ruleset hash).
pub const CATALOG_VERSION: u16 = 1;

/// A cost in milli-units, one amount per `Resource` (the shape
/// `Holding::pay` takes).
pub type Cost = [Milli; RESOURCES];

/// Base production of a Hamlet per hour (units), by `Resource` order
/// (model.rs `BASE_PROD`).
pub const BASE_PROD: [i64; RESOURCES] = [40, 30, 20, 15, 0, 15, 5, 0];

/// Extra base production by tier, percent (model.rs `tier_bonus_pct`).
pub const fn tier_bonus_pct(t: Tier) -> i64 {
    match t {
        Tier::Hamlet => 0,
        Tier::Town => 50,
        Tier::City => 100,
        Tier::Stronghold => 175,
    }
}

/// A building kind (model.rs `Building`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Building {
    pub resource: Resource,
    /// Units per hour it adds.
    pub per_hour: i64,
    /// Base cost (units) of the first copy.
    pub cost: [i64; RESOURCES],
}

/// The six building kinds (model.rs `BUILDINGS`; item ids 0..6).
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

/// Build item ids: 0..6 are [`BUILDINGS`], then walls.
pub const ITEM_WALLS: u8 = 6;
/// Items [`building`] accepts: `0..=ITEM_WALLS`.
pub const ITEM_COUNT: u8 = 7;

/// Build seconds of a building's `n`-th copy (`n ≥ 1`): `3,600 + 1,800 n`
/// (model.rs `build_secs`; the sim passes the copy number).
pub const fn build_secs(n: u32) -> i64 {
    3_600 + 1_800 * n as i64
}

/// Tier-up cost (units) to the next tier and its duration (model.rs
/// `tier_up`).
pub const fn tier_up(t: Tier) -> Option<([i64; RESOURCES], i64)> {
    match t {
        Tier::Hamlet => Some(([0, 1_000, 600, 0, 0, 300, 0, 0], 6 * 3_600)),
        Tier::Town => Some(([0, 4_000, 3_000, 0, 0, 1_500, 0, 0], 12 * 3_600)),
        Tier::City => Some(([0, 12_000, 10_000, 0, 0, 5_000, 0, 0], 24 * 3_600)),
        Tier::Stronghold => None,
    }
}

/// Cost per 100 troops of a Spearman-class unit: food, ore, gold (units)
/// (model.rs `TROOP_COST_PER_100`).
pub const TROOP_COST_PER_100: [i64; 3] = [60, 20, 10];
/// The production cost of a Spearman, the unit `TROOP_COST_PER_100` is
/// stated for.
pub const TROOP_COST_BASE_PROD: i64 = 6;

/// Walls: +100 wall points for this much stone (units) and 4 hours
/// (model.rs `WALL_STEP`, `WALL_COST_STONE`; sim l. 1508–1530).
pub const WALL_STEP: u32 = 100;
pub const WALL_COST_STONE: i64 = 300;
pub const WALL_SECS: i64 = 4 * HOUR;

/// A second or third holding (M2+): base cost, × `duplicate_cost`
/// (model.rs `SETTLER_COST`; not used in M1, kept in the hash).
pub const SETTLER_COST: [i64; RESOURCES] = [800, 800, 400, 0, 0, 200, 0, 0];

/// Starter kit of a new first holding (units; model.rs `STARTER_KIT`).
pub const STARTER_KIT: [i64; RESOURCES] = [300, 300, 200, 100, 0, 100, 0, 0];

/// Works: daily cap, and what each source gives (model.rs `WORKS_*`).
pub const WORKS_DAY_CAP: u64 = 20;
pub const WORKS_EXPLORE: u64 = 4;
pub const WORKS_CAMP: u64 = 10;
pub const WORKS_PLEDGE: u64 = 2;
pub const WORKS_MANDATE: u64 = 6;

/// Base production per hour of a holding at tier `t`, milli-units:
/// `BASE_PROD[r] × (100 + tier_bonus) / 100 × 1,000` (sim `base_prod`
/// with neutral doctrine multipliers).
pub fn base_production(t: Tier) -> [i64; RESOURCES] {
    core::array::from_fn(|r| BASE_PROD[r] * (100 + tier_bonus_pct(t)) / 100 * MILLI)
}

/// The starter kit in milli-units (credited at found).
pub fn starter_kit() -> Cost {
    STARTER_KIT.map(|x| x * MILLI)
}

/// Whole units → milli-units, checked.
fn milli(x: i64) -> Option<Milli> {
    x.checked_mul(MILLI)
}

/// `holding::duplicate_cost` as `Option<u64>`: CL-03 (unit W1-B) changes
/// its return type from `u64` to `Option<u64>`; this adapter lets the
/// catalog build before and after that merge. (Integration: drop it once
/// W1-B is in.)
trait IntoChecked {
    fn into_checked(self) -> Option<u64>;
}
impl IntoChecked for u64 {
    fn into_checked(self) -> Option<u64> {
        Some(self)
    }
}
impl IntoChecked for Option<u64> {
    fn into_checked(self) -> Option<u64> {
        self
    }
}

/// Copies of one building kind a holding may reach before the
/// quadratic cost overflows anything real; the catalog refuses beyond it
/// (the holding's own caps, CL-02, are tighter).
pub const MAX_COPIES: u32 = 64;

fn dup(base: i64, n: u32) -> Option<Milli> {
    if n == 0 || n > MAX_COPIES || base < 0 {
        return None;
    }
    let c = super::holding::duplicate_cost(base as u64, n).into_checked()?;
    milli(i64::try_from(c).ok()?)
}

/// What building item `item` costs as the holding's `n`-th copy (`n ≥
/// 1`), what it does when done, and how long it takes (seconds):
///
/// - items 0..6 ([`BUILDINGS`]): `duplicate_cost(cost, n)` per resource,
///   `Effect::Production { resource, per_hour × 1,000 }`, `build_secs(n)`
///   (sim l. 1480–1505);
/// - [`ITEM_WALLS`]: `wall_cost(300)` stone, `Effect::Walls { 100 }`, 4 h
///   (sim l. 1508–1530; `n` is ignored).
///
/// `None` for an unknown item or `n` outside `1..=MAX_COPIES`.
pub fn building(item: u8, n: u32, d: &Doctrine) -> Option<(Cost, Effect, u32)> {
    if item == ITEM_WALLS {
        let mut c = [0; RESOURCES];
        c[Resource::Stone as usize] = milli(d.wall_cost(WALL_COST_STONE))?;
        return Some((c, Effect::Walls { delta: WALL_STEP }, WALL_SECS as u32));
    }
    let bd = BUILDINGS.get(item as usize)?;
    let mut c = [0; RESOURCES];
    for (r, v) in c.iter_mut().enumerate() {
        *v = dup(bd.cost[r], n)?;
    }
    let delta = milli(bd.per_hour)?;
    let secs = u32::try_from(build_secs(n)).ok()?;
    Some((
        c,
        Effect::Production {
            resource: bd.resource,
            delta,
        },
        secs,
    ))
}

/// The tier-up of a holding at tier `t`: cost (milli), `Effect::TierUp`,
/// seconds; `None` at the top tier.
pub fn tier_up_item(t: Tier) -> Option<(Cost, Effect, u32)> {
    let (c, secs) = tier_up(t)?;
    let mut m = [0; RESOURCES];
    for (r, v) in m.iter_mut().enumerate() {
        *v = milli(c[r])?;
    }
    Some((m, Effect::TierUp, secs as u32))
}

/// Unit ids [`train`] accepts: the six combat units and the Scout
/// (`UnitType as u8`, 0..=6); the Settler (7) is M2.
pub fn unit_of(unit: u8) -> Option<UnitType> {
    Some(match unit {
        0 => UnitType::Spearman,
        1 => UnitType::Archer,
        2 => UnitType::Horseman,
        3 => UnitType::Pikeman,
        4 => UnitType::Crossbowman,
        5 => UnitType::Knight,
        6 => UnitType::Scout,
        _ => return None,
    })
}

/// Training `n` troops of `unit` (immediate): with `k = ⌈n / 100⌉`,
/// food `60 k`, ore `⌊20 k × pc / 6⌋`, gold `⌊10 k × pc / 6⌋` (units,
/// returned in milli), `pc` = the unit's production cost. This is the
/// simulator's garrison purchase (l. 1535–1560, Spearman cost) plus its
/// unit-variant surcharge (l. 1790–1800, ore and gold only), folded into
/// the purchase because M1 trains immediately; for a Spearman (`pc = 6`)
/// it is exactly the sim's garrison cost. `None` for `n = 0`, an unknown
/// unit or overflow.
pub fn train(unit: u8, n: u32) -> Option<Cost> {
    if n == 0 {
        return None;
    }
    let pc = stats(unit_of(unit)?).prod_cost as i64;
    let k = n.div_ceil(100) as i64;
    let mut c = [0; RESOURCES];
    c[Resource::Food as usize] = milli(k.checked_mul(TROOP_COST_PER_100[0])?)?;
    c[Resource::Ore as usize] =
        milli(k.checked_mul(TROOP_COST_PER_100[1])?.checked_mul(pc)? / TROOP_COST_BASE_PROD)?;
    c[Resource::Gold as usize] =
        milli(k.checked_mul(TROOP_COST_PER_100[2])?.checked_mul(pc)? / TROOP_COST_BASE_PROD)?;
    Some(c)
}

/// Canonical little-endian bytes of every catalog table, for the ruleset
/// hash. Order is pinned; append only.
pub fn write_tables(out: &mut alloc::vec::Vec<u8>) {
    let i = |out: &mut alloc::vec::Vec<u8>, x: i64| out.extend_from_slice(&x.to_le_bytes());
    out.extend_from_slice(&CATALOG_VERSION.to_le_bytes());
    for x in BASE_PROD {
        i(out, x);
    }
    for t in [Tier::Hamlet, Tier::Town, Tier::City, Tier::Stronghold] {
        i(out, tier_bonus_pct(t));
        match tier_up(t) {
            Some((c, secs)) => {
                out.push(1);
                for x in c {
                    i(out, x);
                }
                i(out, secs);
            }
            None => out.push(0),
        }
    }
    for b in BUILDINGS {
        out.push(b.resource as u8);
        i(out, b.per_hour);
        for x in b.cost {
            i(out, x);
        }
    }
    i(out, build_secs(0));
    i(out, build_secs(1) - build_secs(0));
    for x in TROOP_COST_PER_100 {
        i(out, x);
    }
    i(out, TROOP_COST_BASE_PROD);
    out.extend_from_slice(&WALL_STEP.to_le_bytes());
    i(out, WALL_COST_STONE);
    i(out, WALL_SECS);
    for x in SETTLER_COST {
        i(out, x);
    }
    for x in STARTER_KIT {
        i(out, x);
    }
    for x in [
        WORKS_DAY_CAP,
        WORKS_EXPLORE,
        WORKS_CAMP,
        WORKS_PLEDGE,
        WORKS_MANDATE,
    ] {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out.extend_from_slice(&MAX_COPIES.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontier::doctrine::{DOCTRINES, NEUTRAL};

    #[test]
    fn buildings_follow_the_duplicate_rule() {
        let (c, e, s) = building(0, 1, &NEUTRAL).unwrap();
        assert_eq!(c, [0, 80_000, 40_000, 0, 0, 0, 0, 0]);
        assert_eq!(
            e,
            Effect::Production {
                resource: Resource::Food,
                delta: 12_000
            }
        );
        assert_eq!(s, 5_400);
        let (c, _, s) = building(0, 3, &NEUTRAL).unwrap();
        assert_eq!(c[1], 240_000); // 80 × 3
        assert_eq!(s, 9_000);
        assert!(building(0, 0, &NEUTRAL).is_none());
        assert!(building(0, MAX_COPIES + 1, &NEUTRAL).is_none());
        assert!(building(ITEM_COUNT, 1, &NEUTRAL).is_none());
    }

    #[test]
    fn walls_use_the_doctrine_wall_cost() {
        for d in DOCTRINES.iter().chain([&NEUTRAL]) {
            let (c, e, s) = building(ITEM_WALLS, 1, d).unwrap();
            assert_eq!(c[2], d.wall_cost(300) * MILLI);
            assert_eq!(c.iter().filter(|&&x| x != 0).count(), 1);
            assert_eq!(e, Effect::Walls { delta: 100 });
            assert_eq!(s, 14_400);
        }
    }

    #[test]
    fn train_is_the_sim_cost() {
        // Spearman: exactly the sim's garrison purchase per 100.
        assert_eq!(
            train(0, 100).unwrap(),
            [60_000, 0, 0, 20_000, 0, 10_000, 0, 0]
        );
        assert_eq!(train(0, 101).unwrap()[0], 120_000);
        // Knight (pc 16): ore 20 × 16 / 6 = 53, gold 10 × 16 / 6 = 26.
        assert_eq!(
            train(5, 100).unwrap(),
            [60_000, 0, 0, 53_000, 0, 26_000, 0, 0]
        );
        assert!(train(0, 0).is_none());
        assert!(train(7, 100).is_none());
        assert!(train(0, u32::MAX).is_some());
    }

    #[test]
    fn production_and_kit() {
        assert_eq!(base_production(Tier::Hamlet)[0], 40_000);
        assert_eq!(base_production(Tier::Stronghold)[0], 110_000);
        assert_eq!(starter_kit()[0], 300_000);
        let (c, e, s) = tier_up_item(Tier::Hamlet).unwrap();
        assert_eq!((c[1], e, s), (1_000_000, Effect::TierUp, 21_600));
        assert!(tier_up_item(Tier::Stronghold).is_none());
    }
}
