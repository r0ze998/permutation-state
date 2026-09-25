//! End-to-end behaviour of genesis and `resolve_tick`.

mod common;
use common::vrf;
use permutation_rules::genesis::{new_season, Entry};
use permutation_rules::gov::Role;
use permutation_rules::hex::Hex;
use permutation_rules::invariants;
use permutation_rules::orders::{office_batches, Order, OrderBatch};
use permutation_rules::rng::Seed;
use permutation_rules::state::{QueueItem, WorldState};
use permutation_rules::tech::Tech;
use permutation_rules::tick::{resolve_tick, run_phase, TickInput};
use permutation_rules::units::UnitType;
use permutation_rules::{Preset, RulesError, Ruleset};

const WORLD: Seed = [11; 32];
const SEASON: Seed = [22; 32];

fn entries(n: usize) -> Vec<Entry> {
    (0..n)
        .map(|i| Entry {
            name: format!("civ-{i}"),
            treasury: 0,
        })
        .collect()
}

fn setup(n: usize) -> (Ruleset, WorldState) {
    let rules = Ruleset::new(Preset::Blitz);
    let state = new_season(&rules, &WORLD, &SEASON, &entries(n)).expect("genesis");
    (rules, state)
}

fn batch(state: &WorldState, civ: u16, orders: Vec<Order>) -> Vec<OrderBatch> {
    office_batches(state, civ, [0; 32], orders)
}

/// A simple scripted opening used by several tests.
fn opening_orders(state: &WorldState, civ: u16) -> Vec<Order> {
    let capital = state.civs[civ as usize].capital.unwrap();
    match state.tick {
        0 => vec![
            Order::SetQueue {
                city: capital,
                items: vec![
                    QueueItem::Building(permutation_rules::buildings::Building::Granary),
                    QueueItem::Troops {
                        unit: UnitType::Spearman,
                        n: 3,
                    },
                ],
            },
            Order::SetResearch {
                techs: vec![Tech::Agriculture, Tech::BronzeWorking, Tech::Writing],
            },
        ],
        _ => Vec::new(),
    }
}

fn run(state: &mut WorldState, rules: &Ruleset, ticks: u16) -> Vec<[u8; 32]> {
    let mut roots = Vec::new();
    for _ in 0..ticks {
        let batches = (0..state.civs.len() as u16)
            .flat_map(|c| batch(state, c, opening_orders(state, c)))
            .collect();
        let before = state.clone();
        let root = resolve_tick(
            state,
            rules,
            &TickInput {
                vrf: vrf(state.tick),
                batches,
                ..Default::default()
            },
        )
        .expect("tick");
        assert!(
            invariants::check(state, rules).is_empty(),
            "{:?}",
            invariants::check(state, rules)
        );
        assert!(invariants::check_monotonic(&before, state).is_empty());
        roots.push(root);
    }
    roots
}

#[test]
fn genesis_places_capitals_and_units() {
    let (rules, s) = setup(6);
    assert_eq!(s.civs.len(), 6);
    assert_eq!(s.cities.len(), 6);
    assert_eq!(s.units.len(), 12);
    assert_eq!(s.relations.len(), 15);
    assert_eq!(s.ruleset_hash, rules.hash());
    assert!(invariants::check(&s, &rules).is_empty());
    for civ in &s.civs {
        assert_eq!(civ.tick_budget, 4); // 3 + 1 city
    }
}

#[test]
fn replay_is_deterministic() {
    let (rules, mut a) = setup(6);
    let (_, mut b) = setup(6);
    assert_eq!(run(&mut a, &rules, 30), run(&mut b, &rules, 30));
}

#[test]
fn different_vrf_changes_the_root() {
    let (rules, mut a) = setup(4);
    let (_, mut b) = setup(4);
    let ra = resolve_tick(
        &mut a,
        &rules,
        &TickInput {
            vrf: [1; 32],
            batches: vec![],
            ..Default::default()
        },
    )
    .unwrap();
    let rb = resolve_tick(
        &mut b,
        &rules,
        &TickInput {
            vrf: [2; 32],
            batches: vec![],
            ..Default::default()
        },
    )
    .unwrap();
    assert_ne!(ra, rb);
}

#[test]
fn full_season_holds_invariants_and_grows() {
    let (rules, mut s) = setup(6);
    run(&mut s, &rules, rules.ticks_per_season);
    assert_eq!(s.tick, 180);
    for civ in &s.civs {
        let pop: u32 = s.living_cities_of(civ.id).map(|c| c.pop).sum();
        assert!(pop > 3, "civ {} pop {pop}", civ.id);
        assert!(
            civ.techs.has(Tech::Writing),
            "civ {} research stalled",
            civ.id
        );
        assert!(civ.achievements.wealth > 0);
    }
    // Season over: further ticks are rejected.
    let err = resolve_tick(&mut s, &rules, &TickInput::default()).unwrap_err();
    assert_eq!(err, RulesError::SeasonOver);
}

#[test]
fn over_budget_batches_are_ignored() {
    let (rules, mut s) = setup(4);
    let capital = s.civs[0].capital.unwrap();
    let too_many: Vec<Order> = (0..5)
        .map(|_| Order::SetFocus {
            city: capital,
            focus: permutation_rules::state::Focus::Food,
        })
        .collect();
    let b = batch(&s, 0, too_many);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].role, Role::Steward);
    // B = 4 gives the steward 1 order (V5 §5.2).
    assert!(matches!(
        permutation_rules::orders::validate_batch(&s, &rules, &b[0]),
        Err(RulesError::OverBudget {
            cost: 5,
            spendable: 1
        })
    ));
    resolve_tick(
        &mut s,
        &rules,
        &TickInput {
            vrf: vrf(0),
            batches: b,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        s.cities[capital as usize].focus,
        permutation_rules::state::Focus::Balanced
    );
    // Unused budget was banked instead, per office.
    assert_eq!(s.nations[0].role_bank, [1, 1, 1, 1]);
    assert!(s
        .last_skipped
        .iter()
        .any(|k| k.civ == 0 && k.role == Role::Steward as u8 && k.index == u16::MAX));
}

#[test]
fn a_war_declaration_gives_the_victim_casus_belli() {
    let (rules, mut s) = setup(4);
    let b = batch(&s, 0, vec![Order::DeclareWar { civ: 1 }]);
    resolve_tick(
        &mut s,
        &rules,
        &TickInput {
            vrf: vrf(0),
            batches: b,
            ..Default::default()
        },
    )
    .unwrap();
    // v0.2 C7: grievance created this tick does not decay this tick.
    assert_eq!(s.grievance(0, 1), rules.casus_belli_threshold);
    assert!(s.civs[0].protection_lost);
    assert!(s.civs[0].is_aggressor(s.tick, rules.aggressor_window));
}

#[test]
fn phases_must_run_in_order_and_can_resume() {
    let (rules, mut s) = setup(4);
    let input = TickInput {
        vrf: vrf(0),
        batches: vec![],
        ..Default::default()
    };
    assert_eq!(
        run_phase(&mut s, &rules, &input, 3),
        Err(RulesError::PhaseOutOfOrder {
            expected: 0,
            got: 3
        })
    );
    // Split resolution: phases 0–5 now, the rest "in another transaction".
    let mut split = s.clone();
    for p in 0..6 {
        run_phase(&mut split, &rules, &input, p).unwrap();
    }
    assert_eq!(split.phase_cursor, 6);
    let a = resolve_tick(&mut split, &rules, &input).unwrap();
    let b = resolve_tick(&mut s, &rules, &input).unwrap();
    assert_eq!(a, b);
}

#[test]
fn scouts_move_along_paths() {
    let (rules, mut s) = setup(4);
    let scout = s
        .units
        .iter()
        .find(|u| {
            u.unit_type == UnitType::Scout && u.owner == permutation_rules::state::Owner::Civ(0)
        })
        .unwrap()
        .clone();
    // Walk toward the map centre over passable tiles.
    let mut path = Vec::new();
    let mut here = scout.hex;
    for _ in 0..3 {
        let next = here
            .neighbors()
            .into_iter()
            .filter(|h| s.map.tile(*h).is_some_and(|t| t.terrain.is_passable()))
            .min_by_key(|h| h.distance(Hex::ORIGIN))
            .unwrap();
        path.push(next);
        here = next;
    }
    let b = batch(
        &s,
        0,
        vec![Order::MoveUnit {
            unit: scout.id,
            path: path.clone(),
        }],
    );
    resolve_tick(
        &mut s,
        &rules,
        &TickInput {
            vrf: vrf(0),
            batches: b,
            ..Default::default()
        },
    )
    .unwrap();
    let moved = &s.units[scout.id as usize];
    assert_ne!(moved.hex, scout.hex, "scout did not move");
    assert_eq!(moved.last_moved, Some(0));
}

#[test]
fn entering_a_protected_capital_says_whose_and_until_when() {
    use permutation_rules::checks::{enter, Blocked};
    use permutation_rules::tick::may_enter;
    let (rules, mut s) = setup(6);
    let cap = s.cities[s.civs[1].capital.unwrap() as usize].hex;
    // A passable hex two steps from civ 1's capital.
    let hex = s
        .map
        .tiles
        .iter()
        .find(|t| t.hex.distance(cap) == 2 && t.terrain.is_passable())
        .expect("passable hex two steps out")
        .hex;
    s.tick = 20; // radius 3 until tick 30, then 2 (still covers distance 2) until 45
    assert_eq!(
        enter(&s, &rules, Some(0), hex),
        Err(Blocked::ProtectedCapital { civ: 1, until: 45 })
    );
    assert!(!may_enter(&s, &rules, Some(0), hex));
    assert_eq!(enter(&s, &rules, Some(1), hex), Ok(()), "own zone is open");
    s.civs[1].protection_lost = true;
    assert_ne!(
        enter(&s, &rules, Some(0), hex).err(),
        Some(Blocked::ProtectedCapital { civ: 1, until: 45 })
    );
}
