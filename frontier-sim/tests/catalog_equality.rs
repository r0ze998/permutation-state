//! I-56: `permutation_rules::frontier::catalog` (W1-C) equals the
//! simulator's `src/model.rs` and the `sim.rs` rules that use it, field by
//! field. `model.rs` is normative (M1 contract §7, the `catalog` row).
//!
//! Conventions the comparison pins (from `sim.rs`):
//! * `building(item, n, d)`: `n` is the copy being built, counted from 1
//!   (`n = buildings[item] + 1`, sim l. 1511); cost = `duplicate_cost(base,
//!   n)` per resource, time = `build_secs(n)`, effect = production of the
//!   building's resource at `per_hour` units an hour (milli-units in the
//!   `Effect`, as `Holding` stores them). Costs may be in units or in
//!   milli-units, one scale for every field.
//! * walls: `WALL_STEP` points for `WALL_COST_STONE × wall_cost_bps` stone,
//!   4 hours.
//! * `train(unit, n)` for `n` a multiple of 100 (the sim trains in
//!   hundreds): food `k × 60`, ore `k × 20 + ⌊k × 20 × extra / 6⌋`, gold
//!   `k × 10 + ⌊k × 10 × extra / 6⌋` with `k = n / 100` and `extra =
//!   max(0, prod_cost − 6)` (sim l. 1606–1640 and 1836–1853: the garrison
//!   is paid at the Spearman rate, the variant surcharge on ore and gold
//!   only). The contract text (§7) scales food too; the sim does not, and
//!   the sim is normative, so this test follows the sim. Units 0..=6 train;
//!   the Settler (7) is M2 and `train` refuses it.
//! * walls: exactly one building item has `Effect::Walls`; its item id is
//!   the rules crate's choice.

#[cfg(not(rules_catalog))]
#[test]
fn catalog_equality_waits_for_the_rules_catalog() {
    let msg = "permutation-rules has no `frontier::catalog` yet (W1-C): \
               catalog equality is not compared on this tree";
    if std::env::var_os("FRONTIER_REQUIRE_CATALOG").is_some() {
        panic!("{msg}");
    }
    eprintln!("{msg}");
}

#[cfg(rules_catalog)]
#[allow(dead_code, unused_imports, clippy::all)]
#[path = "../src/model.rs"]
mod model;

#[cfg(rules_catalog)]
mod eq {
    use super::model;
    use permutation_rules::fixed::{BPS_ONE, MILLI};
    use permutation_rules::frontier::catalog as cat;
    use permutation_rules::frontier::doctrine::{DOCTRINES, NEUTRAL};
    use permutation_rules::frontier::holding::{duplicate_cost, Effect, Tier};
    use permutation_rules::units::{stats, UnitType};

    const TIERS: [Tier; 4] = [Tier::Hamlet, Tier::Town, Tier::City, Tier::Stronghold];
    const UNITS: [UnitType; 7] = [
        UnitType::Spearman,
        UnitType::Archer,
        UnitType::Horseman,
        UnitType::Pikeman,
        UnitType::Crossbowman,
        UnitType::Knight,
        UnitType::Scout,
    ];

    fn n<T: TryInto<i128>>(x: T) -> i128 {
        x.try_into().ok().expect("fits i128")
    }

    fn v<T: Copy + TryInto<i128>>(xs: &[T]) -> Vec<i128> {
        xs.iter().map(|&x| n(x)).collect()
    }

    /// `duplicate_cost` is `u64` before W1-B's CL-03 and `Option<u64>` after.
    trait Cost {
        fn get(self) -> u64;
    }
    impl Cost for u64 {
        fn get(self) -> u64 {
            self
        }
    }
    impl Cost for Option<u64> {
        fn get(self) -> u64 {
            self.expect("duplicate_cost")
        }
    }

    /// Equal in units or in milli-units (one scale for the whole vector).
    fn same_cost(rules: &[i128], model: &[i128]) -> bool {
        rules == model
            || rules
                .iter()
                .zip(model)
                .all(|(r, m)| *r == *m * MILLI as i128)
    }

    #[test]
    fn tables_equal_the_simulator() {
        assert_eq!(cat::BUILDINGS.len(), model::BUILDINGS.len());
        for (r, m) in cat::BUILDINGS.iter().zip(model::BUILDINGS.iter()) {
            assert_eq!(r.resource, m.resource);
            assert_eq!(n(r.per_hour), n(m.per_hour));
            assert_eq!(v(&r.cost), v(&m.cost));
        }
        for k in 0..12u32 {
            assert_eq!(
                n(cat::build_secs(k)),
                n(model::build_secs(k)),
                "build_secs({k})"
            );
        }
        for t in TIERS {
            let (r, m) = (cat::tier_up(t), model::tier_up(t));
            assert_eq!(r.is_some(), m.is_some(), "tier_up({t:?})");
            if let (Some(r), Some(m)) = (r, m) {
                assert_eq!(v(&r.0), v(&m.0), "tier_up({t:?}) cost");
                assert_eq!(n(r.1), n(m.1), "tier_up({t:?}) secs");
            }
            assert_eq!(n(cat::tier_bonus_pct(t)), n(model::tier_bonus_pct(t)));
        }
        assert_eq!(v(&cat::TROOP_COST_PER_100), v(&model::TROOP_COST_PER_100));
        assert_eq!(n(cat::WALL_STEP), n(model::WALL_STEP));
        assert_eq!(n(cat::WALL_COST_STONE), n(model::WALL_COST_STONE));
        assert_eq!(v(&cat::STARTER_KIT), v(&model::STARTER_KIT));
        assert_eq!(v(&cat::BASE_PROD), v(&model::BASE_PROD));
        assert_eq!(n(cat::WORKS_EXPLORE), n(model::WORKS_EXPLORE));
        assert_eq!(n(cat::WORKS_CAMP), n(model::WORKS_CAMP));
    }

    #[test]
    fn buildings_cost_and_produce_as_the_simulator() {
        let mut doctrines: Vec<_> = DOCTRINES.to_vec();
        doctrines.push(NEUTRAL);
        for d in &doctrines {
            for (item, m) in model::BUILDINGS.iter().enumerate() {
                for copy in 1..=6u32 {
                    let (cost, effect, secs) =
                        cat::building(item as u8, copy, d).expect("a building");
                    let want: Vec<i128> = m
                        .cost
                        .iter()
                        .map(|&c| n(duplicate_cost(c as u64, copy).get()))
                        .collect();
                    assert!(
                        same_cost(&v(&cost), &want),
                        "{}: item {item} copy {copy}: {:?} vs {want:?}",
                        d.name,
                        v(&cost)
                    );
                    assert_eq!(n(secs), n(model::build_secs(copy)), "item {item} secs");
                    assert_eq!(
                        effect,
                        Effect::Production {
                            resource: m.resource,
                            delta: m.per_hour * MILLI,
                        },
                        "{}: item {item}",
                        d.name
                    );
                }
            }
            // Walls (sim l. 1566–1590): `WALL_STEP` points for
            // `wall_cost(WALL_COST_STONE)` stone, 4 hours.
            let walls: Vec<_> = (0..16u8)
                .filter_map(|i| cat::building(i, 1, d))
                .filter(|(_, e, _)| matches!(e, Effect::Walls { .. }))
                .collect();
            assert_eq!(walls.len(), 1, "{}: one walls item", d.name);
            let (cost, effect, secs) = walls[0];
            let mut want = vec![0i128; 8];
            want[2] = n(d.wall_cost(model::WALL_COST_STONE));
            assert!(same_cost(&v(&cost), &want), "{}: walls cost", d.name);
            assert_eq!(
                effect,
                Effect::Walls {
                    delta: model::WALL_STEP
                }
            );
            assert_eq!(n(secs), 4 * 3_600);
        }
    }

    #[test]
    fn training_costs_as_the_simulator() {
        for u in UNITS {
            let extra = (stats(u).prod_cost as i128 - 6).max(0);
            for hundreds in [1i128, 2, 5, 17, 100] {
                let per = model::TROOP_COST_PER_100;
                let (f, o, g) = (per[0] as i128, per[1] as i128, per[2] as i128);
                let mut want = vec![0i128; 8];
                want[0] = hundreds * f;
                want[3] = hundreds * o + hundreds * o * extra / 6;
                want[5] = hundreds * g + hundreds * g * extra / 6;
                let got = cat::train(u as u8, (hundreds * 100) as u32).expect("train");
                assert!(
                    same_cost(&v(&got), &want),
                    "{u:?} × {}: {:?} vs {want:?}",
                    hundreds * 100,
                    v(&got)
                );
            }
        }
        assert!(
            cat::train(UnitType::Settler as u8, 100).is_none(),
            "Settlers are M2"
        );
        let _ = BPS_ONE;
    }
}
