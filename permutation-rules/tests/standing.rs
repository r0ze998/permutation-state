//! Standing rules (§13): AutoDefend, Retreat, Patrol, CityQueueRepeat, AutoPurchase.

use permutation_rules::checks::{standing, Blocked};
use permutation_rules::genesis::{new_season, Entry};
use permutation_rules::gov::{Role, NOBODY};
use permutation_rules::hex::Hex;
use permutation_rules::invariants;
use permutation_rules::map::Terrain;
use permutation_rules::orders::{office_batches, validate_batch, AttackTarget, Order, OrderBatch, StandingOrder, StandingTarget};
use permutation_rules::rng::Seed;
use permutation_rules::state::{CityStanding, Owner, QueueItem, StandingRule, Unit, WorldState};
use permutation_rules::tick::{resolve_tick, TickInput};
use permutation_rules::units::UnitType::{self, *};
use permutation_rules::{Preset, RulesError, Ruleset};

fn setup() -> (Ruleset, WorldState) {
    let rules = Ruleset::new(Preset::Blitz);
    let entries: Vec<Entry> = (0..4)
        .map(|i| Entry { name: format!("civ-{i}"), treasury: 0 })
        .collect();
    let mut s = new_season(&rules, &[11; 32], &[22; 32], &entries).unwrap();
    for t in &mut s.map.tiles {
        if t.terrain != Terrain::Water {
            t.terrain = Terrain::Grassland;
        }
    }
    (rules, s)
}

fn vrf(tick: u16) -> Seed {
    let mut s = [0u8; 32];
    s[..2].copy_from_slice(&tick.to_le_bytes());
    s
}

fn step(s: &mut WorldState, rules: &Ruleset, orders: Vec<(u16, Vec<Order>)>) {
    for n in &mut s.nations {
        n.role_bank = [20; 4];
    }
    let batches = orders.into_iter().flat_map(|(civ, orders)| office_batches(s, civ, [0; 32], orders)).collect();
    resolve_tick(s, rules, &TickInput { vrf: vrf(s.tick), batches, ..Default::default() }).unwrap();
    let v = invariants::check(s, rules);
    assert!(v.is_empty(), "{v:?}");
}

fn place(s: &mut WorldState, owner: Owner, unit_type: UnitType, troops: u32, hex: Hex) -> u32 {
    let id = s.units.len() as u32;
    s.units.push(Unit { id, owner, unit_type, troops, hex, path: Vec::new(), last_moved: None, used_full_mp: false, standing: StandingRule::None, alive: true });
    id
}

fn free(s: &WorldState, h: Hex) -> bool {
    s.map.tile(h).is_some_and(|t| t.terrain.is_passable())
        && !s.units.iter().any(|u| u.alive && u.hex == h)
        && !s.cities.iter().any(|c| c.alive && c.hex == h)
        && !s.city_states.iter().any(|c| c.hex == h)
}

/// A free tile `d` steps from civ 0's capital, toward open ground, away from other cities.
fn near_home(s: &WorldState, d: u32) -> Hex {
    let cap = s.cities[s.civs[0].capital.unwrap() as usize].hex;
    s.map
        .tiles
        .iter()
        .map(|t| t.hex)
        .find(|h| {
            h.distance(cap) == d
                && free(s, *h)
                && h.neighbors().iter().all(|n| free(s, *n) || n.distance(cap) < d)
                && s.cities.iter().filter(|c| c.hex != cap).all(|c| c.hex.distance(*h) >= 6)
                && s.city_states.iter().all(|c| c.hex.distance(*h) >= 3)
        })
        .expect("open tile near the capital")
}

/// Civ 0 at war with civ 1 (active from tick 1); civ 1 protection lifted.
fn war(s: &mut WorldState, rules: &Ruleset) {
    step(s, rules, vec![(0, vec![Order::DeclareWar { civ: 1 }])]);
    s.civs[1].protection_lost = true;
    s.civs[0].protection_lost = true;
}

fn set(target: StandingTarget, rule: StandingOrder) -> Order {
    Order::SetStanding { target, rule }
}

#[test]
fn setting_a_rule_costs_one_order_and_is_checked() {
    let (rules, s) = setup();
    let spear = s.units.iter().find(|u| u.owner == Owner::Civ(0) && u.unit_type == Spearman).unwrap().id;
    let scout = s.units.iter().find(|u| u.owner == Owner::Civ(0) && u.unit_type == Scout).unwrap().id;
    let city = s.civs[0].capital.unwrap();
    let batch = OrderBatch { civ: 0, tick: 0, role: Role::General, member: NOBODY, adopt: vec![], decision_digest: [0; 32], orders: vec![set(StandingTarget::Unit(spear), StandingOrder::AutoDefend { radius: 2 })] };
    assert_eq!(validate_batch(&s, &rules, &batch), Ok(1));
    let long = OrderBatch { civ: 0, tick: 0, role: Role::General, member: NOBODY, adopt: vec![], decision_digest: [0; 32], orders: vec![set(StandingTarget::Unit(scout), StandingOrder::Patrol { route: vec![Hex::ORIGIN; 7] })] };
    assert_eq!(validate_batch(&s, &rules, &long), Err(RulesError::TooLong));

    use StandingTarget as T;
    assert_eq!(standing(&s, 0, T::Unit(scout), &StandingOrder::AutoDefend { radius: 1 }), Err(Blocked::CivilianCannotAttack));
    assert_eq!(standing(&s, 0, T::Unit(spear), &StandingOrder::AutoDefend { radius: 4 }), Err(Blocked::OutOfBounds { min: 1, max: 3 }));
    assert_eq!(standing(&s, 0, T::Unit(spear), &StandingOrder::QueueRepeat { on: false }), Err(Blocked::WrongStandingTarget));
    assert_eq!(standing(&s, 0, T::City(city), &StandingOrder::Retreat { ratio_bps: 15_000 }), Err(Blocked::WrongStandingTarget));
    assert_eq!(standing(&s, 1, T::Unit(spear), &StandingOrder::Clear), Err(Blocked::NotYours));
    assert_eq!(standing(&s, 0, T::Unit(scout), &StandingOrder::Patrol { route: vec![Hex::new(1, 0)] }), Ok(()));
}

#[test]
fn auto_defend_strikes_the_weakest_hostile_army_near_its_anchor() {
    let (rules, mut s) = setup();
    war(&mut s, &rules);
    let a = near_home(&s, 3);
    let me = place(&mut s, Owner::Civ(0), Spearman, 10_000, a);
    step(&mut s, &rules, vec![(0, vec![set(StandingTarget::Unit(me), StandingOrder::AutoDefend { radius: 1 })])]);
    assert!(matches!(s.units[me as usize].standing, StandingRule::AutoDefend { radius: 1, anchor } if anchor == a));

    let (n1, n2) = (a.neighbors()[0], a.neighbors()[3]);
    let weak = place(&mut s, Owner::Civ(1), Spearman, 3_000, n1);
    let strong = place(&mut s, Owner::Civ(1), Spearman, 9_000, n2);
    step(&mut s, &rules, vec![]); // no manual orders at all
    assert!(s.units[weak as usize].troops < 3_000 || !s.units[weak as usize].alive, "the weakest adjacent enemy was hit");
    assert_eq!(s.units[strong as usize].troops, 9_000);
    assert_eq!(s.units[me as usize].hex, a, "AutoDefend holds its position");
}

#[test]
fn a_manual_order_overrides_the_rule_for_that_tick() {
    let (rules, mut s) = setup();
    war(&mut s, &rules);
    let a = near_home(&s, 3);
    let me = place(&mut s, Owner::Civ(0), Spearman, 10_000, a);
    step(&mut s, &rules, vec![(0, vec![set(StandingTarget::Unit(me), StandingOrder::AutoDefend { radius: 1 })])]);
    let enemy = place(&mut s, Owner::Civ(1), Spearman, 3_000, a.neighbors()[0]);
    // A manual move this tick: no automatic attack.
    let away = a.neighbors().into_iter().find(|h| free(&s, *h)).unwrap();
    step(&mut s, &rules, vec![(0, vec![Order::MoveUnit { unit: me, path: vec![away] }])]);
    assert_eq!(s.units[enemy as usize].troops, 3_000);
    // The rule is still set for later ticks.
    assert!(matches!(s.units[me as usize].standing, StandingRule::AutoDefend { .. }));
}

#[test]
fn retreat_steps_toward_home_only_when_outmatched() {
    let (rules, mut s) = setup();
    war(&mut s, &rules);
    let cap = s.cities[s.civs[0].capital.unwrap() as usize].hex;
    let a = near_home(&s, 3);
    let me = place(&mut s, Owner::Civ(0), Spearman, 5_000, a);
    step(&mut s, &rules, vec![(0, vec![set(StandingTarget::Unit(me), StandingOrder::Retreat { ratio_bps: 15_000 })])]);

    // Equal strength next to it: ratio 1.0 ≤ 1.5, it stays.
    let far_side = a.neighbors().into_iter().max_by_key(|h| h.distance(cap)).unwrap();
    let e = place(&mut s, Owner::Civ(1), Spearman, 5_000, far_side);
    step(&mut s, &rules, vec![]);
    assert_eq!(s.units[me as usize].hex, a);

    // Twice its strength: it falls back one tile closer to the capital.
    s.units[e as usize].troops = 10_000;
    step(&mut s, &rules, vec![]);
    let now = s.units[me as usize].hex;
    assert_eq!(now.distance(a), 1);
    assert_eq!(now.distance(cap), 2);
}

#[test]
fn patrols_loop_through_their_waypoints() {
    let (rules, mut s) = setup();
    let a = near_home(&s, 3);
    let scout = place(&mut s, Owner::Civ(0), Scout, 1_000, a);
    let b = a.neighbors().into_iter().find(|h| free(&s, *h)).unwrap();
    step(&mut s, &rules, vec![(0, vec![set(StandingTarget::Unit(scout), StandingOrder::Patrol { route: vec![a, b] })])]);
    let mut seen = Vec::new();
    for _ in 0..6 {
        step(&mut s, &rules, vec![]);
        seen.push(s.units[scout as usize].hex);
    }
    assert!(seen.contains(&a) && seen.contains(&b), "{seen:?}");
    assert!(seen.windows(2).any(|w| w[0] == b && w[1] == a), "it comes back: {seen:?}");
}

#[test]
fn city_rules_control_repeat_and_auto_purchase() {
    let (rules, mut s) = setup();
    let city = s.civs[0].capital.unwrap();
    assert_eq!(s.cities[city as usize].standing, CityStanding::DEFAULT);
    s.civs[0].gold = 500_000;
    step(
        &mut s,
        &rules,
        vec![(0, vec![
            Order::SetQueue { city, items: vec![QueueItem::Scout] },
            set(StandingTarget::City(city), StandingOrder::AutoPurchase { max_gold: 30 }),
        ])],
    );
    assert_eq!(s.cities[city as usize].standing.auto_purchase, 30);
    let gold = s.civs[0].gold;
    step(&mut s, &rules, vec![]);
    assert!(s.civs[0].gold < gold, "AutoPurchase spends gold without an order");
    assert!(gold - s.civs[0].gold <= 30_000 + 20_000, "within the cap (plus normal income/upkeep noise)");

    // Repeat off: once the scout is built the queue stays empty.
    step(&mut s, &rules, vec![(0, vec![set(StandingTarget::City(city), StandingOrder::QueueRepeat { on: false })])]);
    for _ in 0..10 {
        step(&mut s, &rules, vec![]);
    }
    assert!(s.cities[city as usize].queue.is_empty());
    let scouts = s.units.iter().filter(|u| u.alive && u.owner == Owner::Civ(0) && u.unit_type == Scout).count();
    assert!(scouts >= 2, "at least one scout was produced");
}

#[test]
fn captured_units_and_cities_drop_their_rules() {
    let (rules, mut s) = setup();
    war(&mut s, &rules);
    let a = near_home(&s, 3);
    let settler = place(&mut s, Owner::Civ(0), Settler, 1_000, a);
    s.units[settler as usize].standing = StandingRule::Patrol { route: [a; 6], len: 1, next: 0 };
    let raider = place(&mut s, Owner::Civ(1), Spearman, 8_000, a.neighbors()[0]);
    step(&mut s, &rules, vec![(1, vec![Order::Attack { army: raider, target: AttackTarget::Unit(settler) }])]);
    assert_eq!(s.units[settler as usize].owner, Owner::Civ(1));
    assert_eq!(s.units[settler as usize].standing, StandingRule::None);
}
