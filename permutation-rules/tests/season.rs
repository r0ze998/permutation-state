//! End-to-end behaviour of genesis and `resolve_tick`.

use permutation_rules::genesis::{new_season, Entry};
use permutation_rules::hex::Hex;
use permutation_rules::invariants;
use permutation_rules::orders::{Order, OrderBatch};
use permutation_rules::rng::Seed;
use permutation_rules::state::{DeclaredKind, QueueItem, WorldState};
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
            declared_kind: if i % 2 == 0 {
                DeclaredKind::Human
            } else {
                DeclaredKind::Agent
            },
            payout_wallet: [i as u8; 32],
        })
        .collect()
}

fn setup(n: usize) -> (Ruleset, WorldState) {
    let rules = Ruleset::new(Preset::Blitz);
    let state = new_season(&rules, &WORLD, &SEASON, &entries(n)).expect("genesis");
    (rules, state)
}

fn vrf(tick: u16) -> Seed {
    let mut s = [0u8; 32];
    s[..2].copy_from_slice(&tick.to_le_bytes());
    s
}

fn batch(state: &WorldState, civ: u16, orders: Vec<Order>) -> OrderBatch {
    OrderBatch {
        civ,
        tick: state.tick,
        decision_digest: [0; 32],
        orders,
    }
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
            .map(|c| batch(state, c, opening_orders(state, c)))
            .collect();
        let before = state.clone();
        let root = resolve_tick(
            state,
            rules,
            &TickInput {
                vrf: vrf(state.tick),
                batches,
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
        },
    )
    .unwrap();
    let rb = resolve_tick(
        &mut b,
        &rules,
        &TickInput {
            vrf: [2; 32],
            batches: vec![],
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
        assert!(civ.scores.dominion > 0 && civ.scores.concord_raw > 0);
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
    assert!(matches!(
        permutation_rules::orders::validate_batch(&s, &rules, &b),
        Err(RulesError::OverBudget {
            cost: 5,
            spendable: 4
        })
    ));
    resolve_tick(
        &mut s,
        &rules,
        &TickInput {
            vrf: vrf(0),
            batches: vec![b],
        },
    )
    .unwrap();
    assert_eq!(
        s.cities[capital as usize].focus,
        permutation_rules::state::Focus::Balanced
    );
    // Unused budget was banked instead.
    assert_eq!(s.civs[0].order_bank, 4);
}

#[test]
fn declaring_war_without_casus_belli_blocks_concord() {
    let (rules, mut s) = setup(4);
    let b = batch(&s, 0, vec![Order::DeclareWar { civ: 1 }]);
    resolve_tick(
        &mut s,
        &rules,
        &TickInput {
            vrf: vrf(0),
            batches: vec![b],
        },
    )
    .unwrap();
    assert_eq!(s.grievance(0, 1), 29); // +30 then −1 decay
    assert!(s.civs[0].protection_lost);
    let concord_after_war = s.civs[0].scores.concord_raw;
    assert_eq!(concord_after_war, 0, "aggressor accrues no Concord");
    assert!(s.civs[1].scores.concord_raw > 0, "the victim keeps Concord");
    assert!(!s.at_war(0, 1) || s.tick >= 1);
}

#[test]
fn phases_must_run_in_order_and_can_resume() {
    let (rules, mut s) = setup(4);
    let input = TickInput {
        vrf: vrf(0),
        batches: vec![],
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
            batches: vec![b],
        },
    )
    .unwrap();
    let moved = &s.units[scout.id as usize];
    assert_ne!(moved.hex, scout.hex, "scout did not move");
    assert_eq!(moved.last_moved, Some(0));
}
