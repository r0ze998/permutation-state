//! The rule fixes of spec v0.2 (C1–C9) that V5 carries over: one item per
//! tick, Star Gate spacing, purchases, transfer caps, the city strike, the
//! Science focus, and what counts as an office acting.

mod common;
use common::nations::*;
use permutation_rules::buildings::Building;
use permutation_rules::checks::Blocked;
use permutation_rules::fixed::MILLI;
use permutation_rules::gov::{self, Role, NOBODY};
use permutation_rules::orders::{office_batches, Good, Order};
use permutation_rules::state::{Focus, QueueItem, WorldState};
use permutation_rules::tech::Tech;
use permutation_rules::{Preset, Ruleset};

// ------------------------------------------------------------------ v0.2 fixes carried into V5

#[test]
fn a_city_completes_one_item_per_tick_and_banks_no_production() {
    let (rules, mut s) = world(&[]);
    let capital = s.civs[0].capital.unwrap() as usize;
    s.cities[capital].queue = vec![QueueItem::Scout, QueueItem::Scout, QueueItem::Scout];
    s.cities[capital].prod = 500 * MILLI;
    let units = s.units.len();
    idle(&mut s, &rules, 1);
    assert_eq!(s.units.len(), units + 1, "one item per tick (v0.2 C1)");
    let scout_cost = permutation_rules::units::stats(permutation_rules::units::UnitType::Scout)
        .prod_cost as i64
        * MILLI;
    assert!(
        s.cities[capital].prod <= scout_cost,
        "the store is capped at the next item's cost"
    );
}

#[test]
fn star_gate_stages_are_spaced_and_never_bought() {
    let (rules, mut s) = world(&[]);
    let capital = s.civs[0].capital.unwrap();
    for t in permutation_rules::tech::TECHS {
        s.civs[0].techs.insert(t.tech);
    }
    s.civs[0].gold = 10_000 * MILLI;
    s.cities[capital as usize].queue = vec![
        QueueItem::Building(Building::StarGate1),
        QueueItem::Building(Building::StarGate2),
    ];
    s.nations[0].role_bank = [4; 4];
    let buy = |s: &WorldState| {
        batch(
            s,
            0,
            Role::Steward,
            NOBODY,
            vec![Order::Purchase {
                city: capital,
                gold: 1_000,
            }],
        )
    };
    let b = buy(&s);
    step(&mut s, &rules, vec![b], vec![]);
    assert!(s
        .last_skipped
        .iter()
        .any(|k| k.reason == Blocked::CannotBuyStarGate.code()));
    // Complete stage I by hand, then stage II must wait `star_gate_spacing` ticks.
    s.cities[capital as usize].prod = 1_000_000 * MILLI;
    idle(&mut s, &rules, 1);
    let first = s.civs[0].last_star_gate.unwrap();
    for _ in 0..rules.star_gate_spacing - 1 {
        let c = &mut s.cities[capital as usize];
        c.prod = 1_000_000 * MILLI;
        idle(&mut s, &rules, 1);
        assert_eq!(s.civs[0].scores.star_gate_stages, 1, "stage II waits");
    }
    s.cities[capital as usize].prod = 1_000_000 * MILLI;
    idle(&mut s, &rules, 1);
    assert_eq!(s.civs[0].scores.star_gate_stages, 2);
    assert_eq!(
        s.civs[0].last_star_gate,
        Some(first + rules.star_gate_spacing)
    );
    assert_eq!(s.civs[0].achievements.star_gate_max, 2);
}

#[test]
fn one_purchase_per_city_per_tick() {
    let (rules, mut s) = world(&[]);
    let capital = s.civs[0].capital.unwrap();
    s.cities[capital as usize].queue = vec![QueueItem::Building(Building::Granary)];
    s.civs[0].gold = 1_000 * MILLI;
    s.nations[0].role_bank = [4; 4];
    let two = vec![
        Order::Purchase {
            city: capital,
            gold: 3,
        },
        Order::Purchase {
            city: capital,
            gold: 3,
        },
    ];
    let b = batch(&s, 0, Role::Steward, NOBODY, two);
    step(&mut s, &rules, vec![b], vec![]);
    assert!(s
        .last_skipped
        .iter()
        .any(|k| k.reason == Blocked::AlreadyPurchased.code()));
}

#[test]
fn transfer_caps_apply_per_pair_per_tick() {
    let (rules, mut s) = world(&[]);
    idle(&mut s, &rules, 1);
    let cap = s.civs[0].last.gold.max(10);
    s.civs[0].gold = 1_000 * MILLI;
    s.nations[0].role_bank = [4; 4];
    let g1 = s.civs[1].gold;
    let two = vec![
        Order::Transfer {
            civ: 1,
            good: Good::Gold,
            amount: cap,
        },
        Order::Transfer {
            civ: 1,
            good: Good::Gold,
            amount: cap,
        },
    ];
    let b = batch(&s, 0, Role::Diplomat, NOBODY, two);
    step(&mut s, &rules, vec![b], vec![]);
    let received = (s.civs[1].gold - g1) / MILLI - s.civs[1].last.gold as i64;
    assert_eq!(
        received, cap as i64,
        "the second transfer exceeds the per-pair cap (v0.2 C4)"
    );
    assert!(s.civs[0].achievements.trade[1] > 0 && s.civs[1].achievements.trade[0] > 0);
}

#[test]
fn a_city_strikes_back_at_archers() {
    use permutation_rules::combat::{resolve_engagement, Combatant, Situation};
    use permutation_rules::units::UnitType;
    let rules = Ruleset::new(Preset::Blitz);
    let archers = Combatant::Army {
        unit: UnitType::Archer,
        troops: 10_000,
    };
    let city = Combatant::City { defense: 8_000 };
    let sit = Situation {
        ranged_attack: true,
        city_strike: 8_000,
        ..Default::default()
    };
    let (_, to_att) = resolve_engagement(&rules, archers, city, sit, 10_000, 10_000);
    assert!(to_att > 0, "v0.2 C5");
    let field = Situation {
        ranged_attack: true,
        ..Default::default()
    };
    let spear = Combatant::Army {
        unit: UnitType::Spearman,
        troops: 10_000,
    };
    assert_eq!(
        resolve_engagement(&rules, archers, spear, field, 10_000, 10_000).1,
        0,
        "armies still do not retaliate"
    );
}

#[test]
fn the_science_focus_raises_science() {
    let (rules, mut s) = world(&[]);
    let capital = s.civs[0].capital.unwrap() as usize;
    let mut science = s.clone();
    science.cities[capital].focus = Focus::Science;
    idle(&mut s, &rules, 1);
    idle(&mut science, &rules, 1);
    assert!(
        science.civs[0].science_store > s.civs[0].science_store,
        "v0.2 C6"
    );
}

#[test]
fn a_reveal_alone_does_not_make_an_officer_active() {
    let (rules, mut s) = world(&[1, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::General]);
    gov::open_government(&mut s, &rules, &e).unwrap();
    idle(&mut s, &rules, 1);
    let reveal = Order::RevealRationale {
        tick: 0,
        policy: b"human".to_vec(),
        salt: [0; 16],
        text: b"x".to_vec(),
    };
    let b = batch(&s, 0, Role::General, 0, vec![reveal]);
    step(&mut s, &rules, vec![b], vec![]);
    assert_eq!(
        s.nations[0].office_last_act[Role::General.index()],
        gov::NEVER,
        "v0.2 C9"
    );
}

#[test]
fn bot_nations_run_every_office_through_the_acting_official() {
    let (rules, mut s) = world(&[]);
    let capital = s.civs[0].capital.unwrap();
    let orders = vec![
        Order::SetQueue {
            city: capital,
            items: vec![QueueItem::Settler],
        },
        Order::SetResearch {
            techs: vec![Tech::Agriculture],
        },
        Order::DeclareWar { civ: 1 },
    ];
    let batches = office_batches(&s, 0, [0; 32], orders);
    assert_eq!(
        batches.iter().map(|b| b.role).collect::<Vec<_>>(),
        vec![Role::General, Role::Steward, Role::Science, Role::Diplomat]
    );
    step(&mut s, &rules, batches, vec![]);
    assert!(
        matches!(
            s.relation(0, 1),
            permutation_rules::state::Relation::War { .. }
        ),
        "the acting official consents to itself"
    );
    assert_eq!(s.civs[0].research_queue, vec![Tech::Agriculture]);
}
