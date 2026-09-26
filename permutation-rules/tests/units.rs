//! The world's unit list is bounded (WP07): a nation builds at most
//! `max_units_built` units a season, dead units carry no path or rule, and
//! production grows the list once per tick.

mod common;
use common::step;
use permutation_rules::buildings::Building;
use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::hex::{Hex, DIRECTIONS};
use permutation_rules::orders::{Good, Order, StandingOrder, StandingTarget};
use permutation_rules::state::{Owner, QueueItem, StandingRule, WorldState};
use permutation_rules::tick::{run_phase, TickInput};
use permutation_rules::units::UnitType;
use permutation_rules::{Preset, Ruleset};

fn world() -> (Ruleset, WorldState) {
    let rules = Ruleset::new(Preset::Blitz);
    let s = new_season(&rules, &[11; 32], &[22; 32], &nation_entries(6)).unwrap();
    (rules, s)
}

fn owned(s: &WorldState, civ: u16) -> usize {
    s.units
        .iter()
        .filter(|u| u.owner == Owner::Civ(civ))
        .count()
}

/// Queue `items` in `civ`'s capital with production to spare.
fn queue(s: &mut WorldState, civ: u16, items: Vec<QueueItem>) -> usize {
    let city = s.civs[civ as usize].capital.unwrap() as usize;
    s.cities[city].queue = items;
    s.cities[city].prod = 1_000_000;
    city
}

#[test]
fn units_wait_at_the_build_cap() {
    let (rules, mut s) = world();
    assert_eq!(rules.max_units_built, 56);
    s.civs[0].units_built = 55;
    let city = queue(&mut s, 0, vec![QueueItem::Scout, QueueItem::Scout]);
    queue(&mut s, 1, vec![QueueItem::Scout, QueueItem::Scout]);
    let (mine, theirs) = (owned(&s, 0), owned(&s, 1));
    step(&mut s, &rules, vec![]);
    assert_eq!(owned(&s, 0), mine + 1, "the 56th unit is built");
    assert_eq!(s.civs[0].units_built, 56);
    let prod = s.cities[city].prod;
    step(&mut s, &rules, vec![]);
    assert_eq!(owned(&s, 0), mine + 1, "the next one waits");
    assert_eq!(s.cities[city].queue.first(), Some(&QueueItem::Scout));
    assert!(s.cities[city].prod >= prod, "its production is kept");
    assert_eq!(owned(&s, 1), theirs + 2, "another nation still builds");
    assert_eq!(s.civs[1].units_built, 2);
    // A building still completes.
    queue(&mut s, 0, vec![QueueItem::Building(Building::Granary)]);
    step(&mut s, &rules, vec![]);
    assert!(s.cities[city].buildings.has(Building::Granary));
    assert_eq!(
        permutation_rules::checks::can_build_unit(&s, &rules, 0),
        Err(permutation_rules::checks::Blocked::OverCap { cap: 56 })
    );
    assert_eq!(
        permutation_rules::checks::can_build_unit(&s, &rules, 1),
        Ok(())
    );
}

/// A whole season of churn: every nation buys spearmen in every city every
/// tick, half of them send their gold away (so upkeep kills armies), armies
/// go on patrols, and nation 0 goes to war. The unit list never passes genesis +
/// nations × `max_units_built`.
#[test]
fn unit_churn_stays_within_the_build_cap() {
    let (rules, mut s) = world();
    let bound = s.units.len() + s.civs.len() * rules.max_units_built as usize;
    for c in &mut s.civs {
        c.units_built = 40; // reach the cap within the season
    }
    let mut dead_seen = false;
    for t in 0..rules.ticks_per_season {
        let mut all = vec![];
        for civ in 0..6u16 {
            let mut orders = vec![];
            let cities: Vec<u32> = s.living_cities_of(civ).map(|c| c.id).collect();
            for c in cities {
                if t % 5 == 0 {
                    let item = QueueItem::Troops {
                        unit: UnitType::Spearman,
                        n: 1,
                    };
                    orders.push(Order::SetQueue {
                        city: c,
                        items: vec![item],
                    });
                }
                orders.push(Order::Purchase {
                    city: c,
                    gold: 1_000_000,
                });
            }
            let gold = (s.civs[civ as usize].gold / 1000).max(0) as u32;
            // Nations 0–2 give their gold away (upkeep then disbands their
            // armies, and the freed tiles take new ones).
            if civ < 3 && gold > 0 && t < rules.transfer_freeze_tick {
                orders.push(Order::Transfer {
                    civ: (civ + 3) % 6,
                    good: Good::Gold,
                    amount: gold.min(rules.max_trade_amount),
                });
            }
            // Armies march off (freeing the tiles around their city) ...
            for u in s
                .units
                .iter()
                .filter(|u| u.alive && u.owner == Owner::Civ(civ))
                .filter(|u| !u.unit_type.is_civilian())
                .take(6)
            {
                let (dq, dr) = DIRECTIONS[(u.id as usize + t as usize) % 6];
                let path = (1..=3)
                    .map(|k| Hex::new(u.hex.q + dq * k, u.hex.r + dr * k))
                    .collect();
                orders.push(Order::MoveUnit { unit: u.id, path });
            }
            // ... and some patrol.
            for u in s
                .units
                .iter()
                .filter(|u| u.alive && u.owner == Owner::Civ(civ))
                .filter(|u| !u.unit_type.is_civilian() && u.standing == StandingRule::None)
                .skip(6)
                .take(2)
            {
                let route = vec![u.hex, Hex::new(u.hex.q + 1, u.hex.r)];
                orders.push(Order::SetStanding {
                    target: StandingTarget::Unit(u.id),
                    rule: StandingOrder::Patrol { route },
                });
            }
            if civ == 0 && t == 20 {
                orders.push(Order::DeclareWar { civ: 1 });
                orders.push(Order::ConsentWar { civ: 1 });
            }
            all.push((civ, orders));
            // Now and then nations 0–2 run a deficit: upkeep disbands armies.
            if civ < 3 && t % 10 == 9 {
                s.civs[civ as usize].gold = -100_000;
            }
        }
        step(&mut s, &rules, all);
        assert!(s.units.len() <= bound, "tick {t}: {} units", s.units.len());
        assert!(s
            .civs
            .iter()
            .all(|c| c.units_built <= rules.max_units_built));
        for u in s.units.iter().filter(|u| !u.alive) {
            dead_seen = true;
            assert!(u.path.is_empty() && u.standing == StandingRule::None);
        }
    }
    assert!(s
        .civs
        .iter()
        .any(|c| c.units_built == rules.max_units_built));
    assert!(dead_seen, "upkeep killed some armies");
}

#[test]
fn dead_units_carry_nothing() {
    let (rules, mut s) = world();
    let i = s
        .units
        .iter()
        .position(|u| u.owner == Owner::Civ(0))
        .unwrap();
    let hex = s.units[i].hex;
    s.units[i].path = (0..12).map(|k| Hex::new(hex.q + k, hex.r)).collect();
    s.units[i].standing = StandingRule::Patrol {
        route: [hex; 6],
        len: 2,
        next: 0,
    };
    // It dies this tick (as upkeep would leave it: path and rule kept).
    s.units[i].alive = false;
    let id = s.units[i].id;
    step(&mut s, &rules, vec![]);
    let u = &s.units[i];
    assert_eq!(u.id, id, "its id stays");
    assert!(u.path.is_empty());
    assert_eq!(u.standing, StandingRule::None);
}

#[test]
fn production_grows_units_once() {
    let (rules, mut s) = world();
    for civ in 0..6 {
        queue(&mut s, civ, vec![QueueItem::Scout]);
    }
    let input = TickInput {
        vrf: common::vrf(s.tick),
        ..Default::default()
    };
    for p in 0..6 {
        run_phase(&mut s, &rules, &input, p).unwrap();
    }
    s.units.shrink_to_fit();
    let before = s.units.len();
    let living = s.cities.iter().filter(|c| c.alive).count();
    run_phase(&mut s, &rules, &input, 6).unwrap();
    assert!(s.units.len() > before, "units were built");
    assert!(
        s.units.capacity() <= before + living,
        "{} > {before} + {living}",
        s.units.capacity()
    );
}
