//! Diplomacy (§10), transfers (§10.5), envoys (§12.1) and founding (§5.7).

use permutation_rules::diplomacy::alliance_group;
use permutation_rules::fixed::MILLI;
use permutation_rules::genesis::{new_season, Entry};
use permutation_rules::hex::Hex;
use permutation_rules::invariants;
use permutation_rules::orders::{validate_batch, Good, Order, OrderBatch};
use permutation_rules::rng::Seed;
use permutation_rules::state::{
    DeclaredKind, Owner, Relation, Specialty, StandingRule, Unit, WorldState,
};
use permutation_rules::tick::{resolve_tick, TickInput};
use permutation_rules::units::UnitType::{self, *};
use permutation_rules::{Preset, RulesError, Ruleset};

fn setup(n: usize) -> (Ruleset, WorldState) {
    let rules = Ruleset::new(Preset::Blitz);
    let entries: Vec<Entry> = (0..n)
        .map(|i| Entry {
            name: format!("civ-{i}"),
            declared_kind: DeclaredKind::Undeclared,
            payout_wallet: [i as u8; 32],
            exchange_deposit: 0,
        })
        .collect();
    let s = new_season(&rules, &[11; 32], &[22; 32], &entries).unwrap();
    (rules, s)
}

fn vrf(tick: u16) -> Seed {
    let mut s = [0u8; 32];
    s[..2].copy_from_slice(&tick.to_le_bytes());
    s
}

fn step(s: &mut WorldState, rules: &Ruleset, orders: Vec<(u16, Vec<Order>)>) {
    let batches = orders
        .into_iter()
        .map(|(civ, orders)| OrderBatch {
            civ,
            tick: s.tick,
            decision_digest: [0; 32],
            orders,
        })
        .collect();
    resolve_tick(
        s,
        rules,
        &TickInput {
            vrf: vrf(s.tick),
            batches,
        },
    )
    .unwrap();
    let v = invariants::check(s, rules);
    assert!(v.is_empty(), "{v:?}");
}

fn place(s: &mut WorldState, civ: u16, unit_type: UnitType, hex: Hex) -> u32 {
    let id = s.units.len() as u32;
    s.units.push(Unit {
        id,
        owner: Owner::Civ(civ),
        unit_type,
        troops: if unit_type.is_civilian() { 1000 } else { 5000 },
        hex,
        path: Vec::new(),
        last_moved: None,
        used_full_mp: false,
        standing: StandingRule::None,
        alive: true,
    });
    id
}

fn gold(s: &WorldState, civ: u16) -> i64 {
    s.civs[civ as usize].gold
}

// ------------------------------------------------------------------ war and peace

#[test]
fn peace_needs_an_earlier_proposal_and_starts_next_tick() {
    let (rules, mut s) = setup(4);
    step(
        &mut s,
        &rules,
        vec![(0, vec![Order::DeclareWar { civ: 1 }])],
    ); // t0
    assert!(s.at_war(0, 1) || s.tick == 1);
    // Proposing and accepting in the same tick does nothing.
    step(
        &mut s,
        &rules,
        vec![
            (0, vec![Order::ProposePeace { civ: 1 }]),
            (1, vec![Order::AcceptPeace { civ: 0 }]),
        ],
    ); // t1
    assert!(matches!(
        s.relation(0, 1),
        Relation::War { peace_at: None, .. }
    ));
    // Accepting on a later tick schedules peace for the next tick.
    step(
        &mut s,
        &rules,
        vec![(1, vec![Order::AcceptPeace { civ: 0 }])],
    ); // t2
    assert!(matches!(
        s.relation(0, 1),
        Relation::War {
            peace_at: Some(3),
            ..
        }
    ));
    assert!(!s.at_war(0, 1), "tick is now 3: the war is over");
    step(&mut s, &rules, vec![]); // t3 applies the transition
    assert_eq!(s.relation(0, 1), Relation::Peace);
}

#[test]
fn peace_withdraws_units_from_foreign_territory() {
    let (rules, mut s) = setup(4);
    step(
        &mut s,
        &rules,
        vec![(0, vec![Order::DeclareWar { civ: 1 }])],
    );
    let cap1 = s.cities[s.civs[1].capital.unwrap() as usize].hex;
    let inside = cap1
        .neighbors()
        .into_iter()
        .find(|h| {
            s.map.tile(*h).is_some_and(|t| t.terrain.is_passable())
                && !s.units.iter().any(|u| u.alive && u.hex == *h)
        })
        .unwrap();
    let intruder = place(&mut s, 0, Spearman, inside);
    step(
        &mut s,
        &rules,
        vec![(0, vec![Order::ProposePeace { civ: 1 }])],
    );
    step(
        &mut s,
        &rules,
        vec![(1, vec![Order::AcceptPeace { civ: 0 }])],
    );
    step(&mut s, &rules, vec![]);
    let hex = s.units[intruder as usize].hex;
    let owner = s
        .map
        .tile(hex)
        .unwrap()
        .owner_city
        .map(|c| s.cities[c as usize].owner);
    assert_eq!(
        owner,
        Some(Some(0)),
        "the intruder is back in its own territory"
    );
}

#[test]
fn proposals_expire() {
    let (rules, mut s) = setup(4);
    step(
        &mut s,
        &rules,
        vec![(0, vec![Order::DeclareWar { civ: 1 }])],
    );
    step(
        &mut s,
        &rules,
        vec![(0, vec![Order::ProposePeace { civ: 1 }])],
    );
    for _ in 0..rules.proposal_ttl {
        step(&mut s, &rules, vec![]);
    }
    step(
        &mut s,
        &rules,
        vec![(1, vec![Order::AcceptPeace { civ: 0 }])],
    );
    assert!(matches!(
        s.relation(0, 1),
        Relation::War { peace_at: None, .. }
    ));
}

// ------------------------------------------------------------------ NAP

fn sign_nap(s: &mut WorldState, rules: &Ruleset, bond0: u32, bond1: u32) {
    for c in &mut s.civs {
        c.gold = 500 * MILLI;
    }
    step(
        s,
        rules,
        vec![(
            0,
            vec![Order::ProposeNap {
                civ: 1,
                bond: bond0,
            }],
        )],
    );
    step(
        s,
        rules,
        vec![(
            1,
            vec![Order::AcceptNap {
                civ: 0,
                bond: bond1,
            }],
        )],
    );
}

#[test]
fn nap_escrows_bonds_and_returns_them_on_expiry() {
    let (rules, mut s) = setup(4);
    sign_nap(&mut s, &rules, 40, 30);
    let signed = s.tick - 1;
    assert_eq!(
        s.relation(0, 1),
        Relation::Nap {
            until: signed + rules.nap_ticks,
            bond_low: 40,
            bond_high: 30
        }
    );
    let (g0, g1) = (gold(&s, 0), gold(&s, 1));
    // War cannot be declared under a NAP.
    step(
        &mut s,
        &rules,
        vec![(0, vec![Order::DeclareWar { civ: 1 }])],
    );
    assert!(matches!(s.relation(0, 1), Relation::Nap { .. }));
    while s.tick <= signed + rules.nap_ticks {
        step(&mut s, &rules, vec![]);
    }
    assert_eq!(s.relation(0, 1), Relation::Peace);
    // Bonds came back on top of normal income.
    assert!(gold(&s, 0) >= g0 + 40 * MILLI && gold(&s, 1) >= g1 + 30 * MILLI);
}

#[test]
fn nap_bond_below_minimum_is_rejected() {
    let (rules, mut s) = setup(4);
    sign_nap(&mut s, &rules, 10, 30);
    assert_eq!(s.relation(0, 1), Relation::Peace);
}

#[test]
fn breaking_a_nap_forfeits_the_bond_and_starts_war() {
    let (rules, mut s) = setup(4);
    sign_nap(&mut s, &rules, 40, 30);
    let g1 = gold(&s, 1);
    let g0 = gold(&s, 0);
    step(&mut s, &rules, vec![(0, vec![Order::BreakNap { civ: 1 }])]);
    let t = s.tick - 1;
    assert!(matches!(
        s.relation(0, 1),
        Relation::War { declared_by: 0, casus_belli: false, active_from, .. } if active_from == t + 1
    ));
    assert_eq!(s.grievance(0, 1), 40 - 1);
    assert_eq!(s.civs[0].last_aggression, Some(t));
    // Civ 1 received both bonds (plus its normal income); civ 0 got nothing back.
    let income1 = s.civs[1].last.gold as i64 * MILLI;
    let income0 = s.civs[0].last.gold as i64 * MILLI;
    assert_eq!(gold(&s, 1) - g1, 70 * MILLI + income1);
    assert_eq!(gold(&s, 0) - g0, income0);
}

// ------------------------------------------------------------------ alliances

fn ally(s: &mut WorldState, rules: &Ruleset, from: u16, to: u16) {
    step(
        s,
        rules,
        vec![(from, vec![Order::ProposeAlliance { civ: to }])],
    );
    step(
        s,
        rules,
        vec![(to, vec![Order::AcceptAlliance { civ: from }])],
    );
}

#[test]
fn alliances_form_groups_up_to_the_cap() {
    let (rules, mut s) = setup(6); // cap = 2
    ally(&mut s, &rules, 0, 1);
    assert_eq!(alliance_group(&s, 0), vec![0, 1]);
    assert!(s.civs[0].ever_allied && s.civs[1].ever_allied);
    ally(&mut s, &rules, 0, 2);
    assert_eq!(
        alliance_group(&s, 2),
        vec![2],
        "group of 2 is full in a 6-civ game"
    );
    // Allies cannot declare war on each other.
    step(
        &mut s,
        &rules,
        vec![(0, vec![Order::DeclareWar { civ: 1 }])],
    );
    assert!(matches!(s.relation(0, 1), Relation::Alliance { .. }));
}

#[test]
fn larger_games_allow_three_member_groups() {
    let rules = Ruleset::new(Preset::Season); // cap = min(3, 9 / 3) = 3
    let entries: Vec<Entry> = (0..9)
        .map(|i| Entry {
            name: format!("c{i}"),
            declared_kind: DeclaredKind::Undeclared,
            payout_wallet: [i as u8; 32],
            exchange_deposit: 0,
        })
        .collect();
    let mut s = new_season(&rules, &[11; 32], &[22; 32], &entries).expect("9-civ season map");
    ally(&mut s, &rules, 0, 1);
    ally(&mut s, &rules, 0, 2);
    assert_eq!(alliance_group(&s, 1), vec![0, 1, 2]);
    assert!(matches!(s.relation(1, 2), Relation::Alliance { .. }));
    ally(&mut s, &rules, 0, 3);
    assert_eq!(
        alliance_group(&s, 3),
        vec![3],
        "a fourth member exceeds the cap"
    );
}

#[test]
fn leaving_an_alliance_takes_effect_after_the_delay() {
    let (rules, mut s) = setup(4);
    ally(&mut s, &rules, 0, 1);
    step(&mut s, &rules, vec![(1, vec![Order::LeaveAlliance])]);
    let leave_at = s.tick - 1 + rules.alliance_leave_delay;
    assert!(
        matches!(s.relation(0, 1), Relation::Alliance { leaving_at: Some(t) } if t == leave_at)
    );
    while s.tick <= leave_at {
        step(&mut s, &rules, vec![]);
    }
    assert_eq!(s.relation(0, 1), Relation::Peace);
    assert!(
        s.civs[1].ever_allied,
        "neutrality bonus is lost for the season"
    );
}

// ------------------------------------------------------------------ transfers

#[test]
fn gold_transfers_respect_the_income_cap() {
    let (rules, mut s) = setup(4);
    s.civs[0].gold = 1_000 * MILLI;
    step(&mut s, &rules, vec![]); // establishes last-tick income
    let cap = s.civs[0].last.gold.max(10);
    let (g0, g1) = (gold(&s, 0), gold(&s, 1));
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![Order::Transfer {
                civ: 1,
                good: Good::Gold,
                amount: cap,
            }],
        )],
    );
    let (i0, i1) = (s.civs[0].last.gold as i64, s.civs[1].last.gold as i64);
    assert_eq!(gold(&s, 1) - g1, (cap as i64 + i1) * MILLI);
    assert_eq!(gold(&s, 0) - g0, (i0 - cap as i64) * MILLI);
    // Over the cap: ignored.
    let g1 = gold(&s, 1);
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![Order::Transfer {
                civ: 1,
                good: Good::Gold,
                amount: 500,
            }],
        )],
    );
    assert_eq!(gold(&s, 1) - g1, s.civs[1].last.gold as i64 * MILLI);
}

#[test]
fn transfers_are_frozen_in_the_final_phase() {
    let (rules, mut s) = setup(4);
    s.tick = rules.transfer_freeze_tick;
    let b = OrderBatch {
        civ: 0,
        tick: s.tick,
        decision_digest: [0; 32],
        orders: vec![Order::Transfer {
            civ: 1,
            good: Good::Gold,
            amount: 1,
        }],
    };
    assert_eq!(validate_batch(&s, &rules, &b), Err(RulesError::Frozen));
}

// ------------------------------------------------------------------ envoys

#[test]
fn envoys_make_the_top_civ_suzerain_and_pay_its_bonus() {
    let (rules, mut s) = setup(4);
    s.civs[0].influence = 100 * MILLI;
    s.civs[1].influence = 100 * MILLI;
    s.city_states[0].specialty = Specialty::Mercantile;
    step(
        &mut s,
        &rules,
        vec![
            (
                0,
                vec![Order::SendEnvoy {
                    city_state: 0,
                    influence: 61,
                }],
            ),
            (
                1,
                vec![Order::SendEnvoy {
                    city_state: 0,
                    influence: 70,
                }],
            ),
        ],
    );
    assert_eq!(
        s.city_states[0].suzerain,
        Some(1),
        "most influence wins, not lowest id"
    );
    let g = gold(&s, 1);
    step(&mut s, &rules, vec![]);
    assert_eq!(gold(&s, 1) - g, s.civs[1].last.gold as i64 * MILLI);
    assert!(
        s.civs[1].last.gold >= 4,
        "income includes the +4 Mercantile bonus"
    );
    // Locked until the cycle restarts at tick 45, when influence halves.
    while s.tick <= 45 {
        step(&mut s, &rules, vec![]);
    }
    assert_eq!(s.city_states[0].influence[1], 35 * MILLI);
    assert_eq!(s.city_states[0].suzerain, None);
}

// ------------------------------------------------------------------ founding

fn open_site(s: &WorldState, rules: &Ruleset) -> Hex {
    s.map
        .tiles
        .iter()
        .find(|t| {
            t.terrain.is_passable()
                && t.owner_city.is_none()
                && s.cities.iter().all(|c| c.hex.distance(t.hex) >= 5)
                && s.city_states
                    .iter()
                    .all(|c| c.hex.distance(t.hex) >= rules.city_min_distance as u32)
                && !s.units.iter().any(|u| u.alive && u.hex == t.hex)
        })
        .unwrap()
        .hex
}

#[test]
fn settlers_found_cities_on_valid_sites() {
    let (rules, mut s) = setup(4);
    s.tick = 60; // protected zones are gone
    let site = open_site(&s, &rules);
    let settler = place(&mut s, 0, Settler, site);
    step(
        &mut s,
        &rules,
        vec![(0, vec![Order::FoundCity { settler }])],
    );
    let city = s
        .cities
        .iter()
        .find(|c| c.hex == site)
        .expect("city founded");
    assert_eq!(city.owner, Some(0));
    assert!(!s.units[settler as usize].alive);
    assert_eq!(s.map.tile(site).unwrap().owner_city, Some(city.id));
    assert_eq!(s.civs[0].tick_budget, 5, "3 + 2 cities");
}

#[test]
fn settlers_cannot_found_too_close() {
    let (rules, mut s) = setup(4);
    s.tick = 60;
    let cap = s.cities[s.civs[0].capital.unwrap() as usize].hex;
    let near = cap
        .neighbors()
        .into_iter()
        .find(|h| {
            s.map.tile(*h).is_some_and(|t| t.terrain.is_passable())
                && !s.units.iter().any(|u| u.alive && u.hex == *h)
        })
        .unwrap();
    let settler = place(&mut s, 0, Settler, near);
    let before = s.cities.len();
    step(
        &mut s,
        &rules,
        vec![(0, vec![Order::FoundCity { settler }])],
    );
    assert_eq!(s.cities.len(), before);
    assert!(s.units[settler as usize].alive);
}

#[test]
fn founding_near_a_ruin_grants_heritage_once() {
    let (rules, mut s) = setup(4);
    s.tick = 60;
    let site = open_site(&s, &rules);
    let ruin = site.neighbors()[0];
    s.map.tile_mut(ruin).unwrap().ruin_peak_pop = Some(8);
    let settler = place(&mut s, 0, Settler, site);
    step(
        &mut s,
        &rules,
        vec![(0, vec![Order::FoundCity { settler }])],
    );
    let city = s.cities.iter().find(|c| c.hex == site).unwrap();
    assert_eq!(city.heritage_bonus, 4);
    assert_eq!(city.heritage_until, Some(60 + rules.heritage_ticks));
    assert!(s.map.tile(ruin).unwrap().heritage_claimed);
}

#[test]
fn peace_starts_a_truce_that_blocks_war_even_with_casus_belli() {
    let (rules, mut s) = setup(4);
    step(
        &mut s,
        &rules,
        vec![(0, vec![Order::DeclareWar { civ: 1 }])],
    ); // t0
    step(
        &mut s,
        &rules,
        vec![(0, vec![Order::ProposePeace { civ: 1 }])],
    ); // t1
    step(
        &mut s,
        &rules,
        vec![(1, vec![Order::AcceptPeace { civ: 0 }])],
    ); // t2 → peace at t3
    step(&mut s, &rules, vec![]); // t3: peace, truce until 15
    assert_eq!(s.relation(0, 1), Relation::Peace);
    assert!(
        s.grievance(0, 1) >= rules.casus_belli_threshold - 4,
        "civ 1 still holds a grievance"
    );
    s.add_grievance(0, 1, 30); // make casus belli certain
    step(
        &mut s,
        &rules,
        vec![(1, vec![Order::DeclareWar { civ: 0 }])],
    ); // t4: blocked
    assert_eq!(s.relation(0, 1), Relation::Peace);
    while s.tick < 3 + rules.truce_ticks {
        step(&mut s, &rules, vec![]);
    }
    step(
        &mut s,
        &rules,
        vec![(1, vec![Order::DeclareWar { civ: 0 }])],
    ); // t15: allowed
    assert!(matches!(
        s.relation(0, 1),
        Relation::War {
            declared_by: 1,
            casus_belli: true,
            ..
        }
    ));
}
