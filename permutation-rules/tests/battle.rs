//! Phase 5 (combat, captures, razing) behaviour.

use permutation_rules::buildings::Building;
use permutation_rules::combat::{resolve_engagement, variance, Combatant, Situation};
use permutation_rules::genesis::{new_season, Entry};
use permutation_rules::hex::Hex;
use permutation_rules::invariants;
use permutation_rules::orders::{office_batches, AttackTarget, Order};
use permutation_rules::rng::{tick_seed, Seed};
use permutation_rules::state::{Owner, StandingRule, Unit, WorldState};
use permutation_rules::tick::{resolve_tick, TickInput};
use permutation_rules::units::UnitType::{self, *};
use permutation_rules::{Preset, Ruleset};

const WORLD: Seed = [11; 32];
const SEASON: Seed = [22; 32];

fn setup() -> (Ruleset, WorldState) {
    let rules = Ruleset::new(Preset::Blitz);
    let entries: Vec<Entry> = (0..4)
        .map(|i| Entry { name: format!("civ-{i}"), treasury: 0 })
        .collect();
    let state = new_season(&rules, &WORLD, &SEASON, &entries).unwrap();
    (rules, state)
}

fn vrf(tick: u16) -> Seed {
    let mut s = [0u8; 32];
    s[..2].copy_from_slice(&tick.to_le_bytes());
    s
}

/// Resolve one tick with the given orders per civ; check invariants.
fn step(s: &mut WorldState, rules: &Ruleset, orders: Vec<(u16, Vec<Order>)>) {
    // Tests exercise mechanics, not office budgets (V5 §5.2 has its own tests).
    for n in &mut s.nations {
        n.role_bank = [20; 4];
    }
    let batches = orders
        .into_iter()
        .flat_map(|(civ, orders)| office_batches(s, civ, [0; 32], orders))
        .collect();
    resolve_tick(
        s,
        rules,
        &TickInput {
            vrf: vrf(s.tick),
            batches, ..Default::default() },
    )
    .unwrap();
    let v = invariants::check(s, rules);
    assert!(v.is_empty(), "{v:?}");
}

fn place(s: &mut WorldState, civ: Owner, unit_type: UnitType, troops: u32, hex: Hex) -> u32 {
    let id = s.units.len() as u32;
    s.units.push(Unit {
        id,
        owner: civ,
        unit_type,
        troops,
        hex,
        path: Vec::new(),
        last_moved: None,
        used_full_mp: false,
        standing: StandingRule::None,
        alive: true,
    });
    id
}

fn free(s: &WorldState, h: Hex) -> bool {
    s.map.tile(h).is_some_and(|t| t.terrain.is_passable())
        && !s.units.iter().any(|u| u.alive && u.hex == h)
        && !s.cities.iter().any(|c| c.alive && c.hex == h)
        && !s.city_states.iter().any(|c| c.hex == h)
}

/// Open ground far from every city: returns (a, b, c) with a–b adjacent and c two steps from b.
fn open_ground(s: &WorldState) -> (Hex, Hex, Hex) {
    let far = |h: Hex| {
        s.cities.iter().all(|c| c.hex.distance(h) >= 6)
            && s.city_states.iter().all(|c| c.hex.distance(h) >= 3)
            && h.radius() + 2 <= s.map.radius as u32
    };
    for t in &s.map.tiles {
        let b = t.hex;
        if !free(s, b) || !far(b) {
            continue;
        }
        for a in b.neighbors() {
            if !free(s, a) || !far(a) {
                continue;
            }
            for c in a.neighbors() {
                if c != b && c.distance(b) == 2 && free(s, c) && far(c) {
                    return (a, b, c);
                }
            }
        }
    }
    panic!("no open ground");
}

/// Civ 0 declares war on civ 1 at tick 0; war is active from tick 1. The
/// victim's start protection is lifted so its units and capital can be hit.
fn war_between_0_and_1(s: &mut WorldState, rules: &Ruleset) {
    step(s, rules, vec![(0, vec![Order::DeclareWar { civ: 1 }])]);
    s.civs[1].protection_lost = true;
}

fn expected(
    s: &WorldState,
    rules: &Ruleset,
    engagement: u32,
    a: Combatant,
    d: Combatant,
    sit: Situation,
) -> (u32, u32) {
    let seed = tick_seed(&SEASON, &vrf(s.tick), s.tick);
    resolve_engagement(
        rules,
        a,
        d,
        sit,
        variance(rules, &seed, engagement, 0),
        variance(rules, &seed, engagement, 1),
    )
}

fn situation_on(s: &WorldState, defender_hex: Hex, attacker_hex: Hex) -> Situation {
    Situation {
        defender_on_rough_terrain: s.map.tile(defender_hex).unwrap().terrain.info().defense_bps
            < 10_000,
        defender_fortified: true, // placed units have never moved
        attacker_on_river: s.map.tile(attacker_hex).unwrap().river,
        ..Situation::default()
    }
}

#[test]
fn melee_engagement_matches_combat_math() {
    let (rules, mut s) = setup();
    war_between_0_and_1(&mut s, &rules);
    let (a_hex, b_hex, _) = open_ground(&s);
    let a = place(&mut s, Owner::Civ(0), Spearman, 10_000, a_hex);
    let b = place(&mut s, Owner::Civ(1), Spearman, 10_000, b_hex);
    let (to_b, to_a) = expected(
        &s,
        &rules,
        0,
        Combatant::Army {
            unit: Spearman,
            troops: 10_000,
        },
        Combatant::Army {
            unit: Spearman,
            troops: 10_000,
        },
        situation_on(&s, b_hex, a_hex),
    );
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![Order::Attack {
                army: a,
                target: AttackTarget::Unit(b),
            }],
        )],
    );
    assert_eq!(s.units[a as usize].troops, 10_000 - to_a);
    assert_eq!(s.units[b as usize].troops, 10_000 - to_b);
    assert!(to_a > 0 && to_b > 0);
}

#[test]
fn attacks_need_an_active_war() {
    let (rules, mut s) = setup();
    let (a_hex, b_hex, _) = open_ground(&s);
    let a = place(&mut s, Owner::Civ(0), Spearman, 10_000, a_hex);
    let b = place(&mut s, Owner::Civ(1), Spearman, 10_000, b_hex);
    // Declaring and attacking in the same tick: war is not active yet (§10.2).
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![
                Order::DeclareWar { civ: 1 },
                Order::Attack {
                    army: a,
                    target: AttackTarget::Unit(b),
                },
            ],
        )],
    );
    assert_eq!(s.units[a as usize].troops, 10_000);
    assert_eq!(s.units[b as usize].troops, 10_000);
}

#[test]
fn ranged_attacks_take_no_retaliation() {
    let (rules, mut s) = setup();
    war_between_0_and_1(&mut s, &rules);
    let (_, b_hex, c_hex) = open_ground(&s);
    let archer = place(&mut s, Owner::Civ(0), Archer, 10_000, c_hex);
    let spear = place(&mut s, Owner::Civ(1), Spearman, 12_000, b_hex);
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![Order::Attack {
                army: archer,
                target: AttackTarget::Unit(spear),
            }],
        )],
    );
    assert_eq!(s.units[archer as usize].troops, 10_000);
    assert!(s.units[spear as usize].troops < 12_000);
}

#[test]
fn several_attackers_each_use_pre_combat_counts() {
    let (rules, mut s) = setup();
    war_between_0_and_1(&mut s, &rules);
    let (a_hex, b_hex, _) = open_ground(&s);
    let second = b_hex
        .neighbors()
        .into_iter()
        .find(|h| *h != a_hex && free(&s, *h))
        .unwrap();
    let a1 = place(&mut s, Owner::Civ(0), Spearman, 10_000, a_hex);
    let a2 = place(&mut s, Owner::Civ(0), Spearman, 10_000, second);
    let d = place(&mut s, Owner::Civ(1), Spearman, 20_000, b_hex);
    let e = |id, att_hex| {
        expected(
            &s,
            &rules,
            id,
            Combatant::Army {
                unit: Spearman,
                troops: 10_000,
            },
            Combatant::Army {
                unit: Spearman,
                troops: 20_000,
            },
            situation_on(&s, b_hex, att_hex),
        )
    };
    let (d1, r1) = e(0, a_hex);
    let (d2, r2) = e(1, second);
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![
                Order::Attack {
                    army: a1,
                    target: AttackTarget::Unit(d),
                },
                Order::Attack {
                    army: a2,
                    target: AttackTarget::Unit(d),
                },
            ],
        )],
    );
    assert_eq!(s.units[d as usize].troops, 20_000 - d1 - d2);
    assert_eq!(s.units[a1 as usize].troops, 10_000 - r1);
    assert_eq!(s.units[a2 as usize].troops, 10_000 - r2);
}

#[test]
fn targets_in_an_active_protected_zone_are_immune() {
    let (rules, mut s) = setup();
    step(
        &mut s,
        &rules,
        vec![(0, vec![Order::DeclareWar { civ: 1 }])],
    );
    // Civ 1 keeps its protection: its capital's neighbourhood cannot be attacked.
    let cap = s.cities[s.civs[1].capital.unwrap() as usize].hex;
    let target_hex = cap.neighbors().into_iter().find(|h| free(&s, *h)).unwrap();
    let attacker_hex = target_hex
        .neighbors()
        .into_iter()
        .find(|h| free(&s, *h) && h.distance(cap) >= 2)
        .unwrap();
    let t = place(&mut s, Owner::Civ(1), Spearman, 10_000, target_hex);
    let a = place(&mut s, Owner::Civ(0), Spearman, 10_000, attacker_hex);
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![Order::Attack {
                army: a,
                target: AttackTarget::Unit(t),
            }],
        )],
    );
    assert_eq!(s.units[t as usize].troops, 10_000);
    assert_eq!(s.units[a as usize].troops, 10_000);
}

/// Put a civ-0 army next to civ 1's capital. Returns (army, city index).
fn besiege(s: &mut WorldState, unit: UnitType, troops: u32) -> (u32, usize) {
    let ci = s.civs[1].capital.unwrap() as usize;
    let cap = s.cities[ci].hex;
    let hex = cap.neighbors().into_iter().find(|h| free(s, *h)).unwrap();
    (place(s, Owner::Civ(0), unit, troops, hex), ci)
}

fn remove_garrison(s: &mut WorldState, ci: usize) {
    let hex = s.cities[ci].hex;
    for u in &mut s.units {
        if u.hex == hex && !u.unit_type.is_civilian() {
            u.alive = false;
        }
    }
}

#[test]
fn garrison_defends_before_the_city() {
    let (rules, mut s) = setup();
    war_between_0_and_1(&mut s, &rules);
    let (a, ci) = besiege(&mut s, Spearman, 20_000);
    let defense = s.cities[ci].defense;
    let garrison = s
        .units
        .iter()
        .find(|u| u.alive && u.hex == s.cities[ci].hex && !u.unit_type.is_civilian())
        .unwrap()
        .id;
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![Order::Attack {
                army: a,
                target: AttackTarget::City(ci as u32),
            }],
        )],
    );
    assert_eq!(s.cities[ci].owner, Some(1));
    assert!(s.units[garrison as usize].troops < 3_000);
    // City defence is untouched while a garrison exists (and does not regenerate while attacked).
    assert_eq!(s.cities[ci].defense, defense);
}

#[test]
fn melee_captures_an_undefended_city() {
    let (rules, mut s) = setup();
    war_between_0_and_1(&mut s, &rules);
    let (a, ci) = besiege(&mut s, Spearman, 20_000);
    remove_garrison(&mut s, ci);
    s.cities[ci].defense = 500;
    s.cities[ci].pop = 3;
    s.cities[ci].buildings.insert(Building::Walls);
    s.cities[ci].buildings.insert(Building::Granary);
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![Order::Attack {
                army: a,
                target: AttackTarget::City(ci as u32),
            }],
        )],
    );

    let c = &s.cities[ci];
    assert_eq!(c.owner, Some(0));
    assert_eq!(c.captured_from, Some(1));
    assert_eq!(c.captured_tick, Some(1));
    assert!(c.pop <= 3 && c.pop >= 1);
    assert!(!c.buildings.has(Building::Walls) && c.buildings.has(Building::Granary));
    assert!(
        !c.capture_scores,
        "a capital founded at tick 0 is too young to score at tick 1"
    );
    assert_eq!(s.units[a as usize].hex, c.hex, "the captor moves in");
    // Civ 1's scout in the capital is captured too.
    assert!(s
        .units
        .iter()
        .filter(|u| u.alive && u.hex == c.hex)
        .all(|u| u.owner == Owner::Civ(0)));
    // Grievance: 30 (war, not decayed the tick it was added, v0.2 C7) − 1 + 20 (capture).
    assert_eq!(s.grievance(0, 1), 49);
    assert_eq!(s.civs[0].last_aggression, Some(1));
    assert_eq!(s.civs[1].capital, None, "government in exile");
    assert_eq!(s.civs[1].last_city_lost, Some(1));
}

#[test]
fn a_third_civs_civilian_leaves_a_captured_city() {
    // Found by the season sim (seed 20, tick 97): a third civ's settler stood
    // in a city when it was captured, and stayed on the captor's army's tile.
    let (rules, mut s) = setup();
    war_between_0_and_1(&mut s, &rules);
    let (a, ci) = besiege(&mut s, Spearman, 20_000);
    remove_garrison(&mut s, ci);
    s.cities[ci].defense = 0;
    let hex = s.cities[ci].hex;
    let settler = place(&mut s, Owner::Civ(2), Settler, 1000, hex);
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![Order::Attack {
                army: a,
                target: AttackTarget::City(ci as u32),
            }],
        )],
    );
    assert_eq!(s.cities[ci].owner, Some(0));
    let u = &s.units[settler as usize];
    assert_eq!(u.owner, Owner::Civ(2), "only the old owner's civilians are captured");
    assert!(!u.alive || u.hex != hex, "the third civ's settler left the tile (or was disbanded)");
    // `step` already checked the occupancy invariant.
}

#[test]
fn capturing_from_the_declarer_is_not_aggression() {
    let (rules, mut s) = setup();
    // Civ 1 declares war on civ 0; civ 0 captures civ 1's capital.
    step(
        &mut s,
        &rules,
        vec![(1, vec![Order::DeclareWar { civ: 0 }])],
    );
    s.civs[1].protection_lost = true;
    let (a, ci) = besiege(&mut s, Spearman, 20_000);
    remove_garrison(&mut s, ci);
    s.cities[ci].defense = 0;
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![Order::Attack {
                army: a,
                target: AttackTarget::City(ci as u32),
            }],
        )],
    );
    assert_eq!(s.cities[ci].owner, Some(0));
    assert_eq!(s.civs[0].last_aggression, None);
    assert_eq!(s.civs[1].last_aggression, Some(0));
}

#[test]
fn ranged_armies_cannot_capture() {
    let (rules, mut s) = setup();
    war_between_0_and_1(&mut s, &rules);
    let (a, ci) = besiege(&mut s, Archer, 20_000);
    remove_garrison(&mut s, ci);
    s.cities[ci].defense = 0;
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![Order::Attack {
                army: a,
                target: AttackTarget::City(ci as u32),
            }],
        )],
    );
    assert_eq!(s.cities[ci].owner, Some(1));
}

#[test]
fn last_city_protection_blocks_capture() {
    let (rules, mut s) = setup();
    war_between_0_and_1(&mut s, &rules);
    let (a, ci) = besiege(&mut s, Spearman, 20_000);
    remove_garrison(&mut s, ci);
    s.cities[ci].defense = 0;
    s.civs[1].last_city_lost = Some(0); // lost another city last tick
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![Order::Attack {
                army: a,
                target: AttackTarget::City(ci as u32),
            }],
        )],
    );
    assert_eq!(s.cities[ci].owner, Some(1));
}

#[test]
fn razing_turns_a_captured_city_into_a_ruin() {
    let (rules, mut s) = setup();
    war_between_0_and_1(&mut s, &rules);
    let (a, ci) = besiege(&mut s, Spearman, 20_000);
    remove_garrison(&mut s, ci);
    s.cities[ci].defense = 0;
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![Order::Attack {
                army: a,
                target: AttackTarget::City(ci as u32),
            }],
        )],
    );
    assert_eq!(s.cities[ci].owner, Some(0));
    let g_before = s.grievance(0, 1);
    step(
        &mut s,
        &rules,
        vec![(0, vec![Order::Raze { city: ci as u32 }])],
    );
    assert_eq!(s.cities[ci].razing, Some(3));
    assert_eq!(s.grievance(0, 1), g_before + 60 - 1);
    for _ in 0..2 {
        step(&mut s, &rules, vec![]);
        assert!(s.cities[ci].alive);
    }
    step(&mut s, &rules, vec![]);
    let hex = s.cities[ci].hex;
    assert!(!s.cities[ci].alive);
    assert!(s.map.tile(hex).unwrap().ruin_peak_pop.is_some());
    assert!(s.map.tiles.iter().all(|t| t.owner_city != Some(ci as u32)));
}

#[test]
fn city_states_can_be_attacked_and_captured() {
    let (rules, mut s) = setup();
    let cs_hex = s.city_states[0].hex;
    let hex = cs_hex
        .neighbors()
        .into_iter()
        .find(|h| free(&s, *h))
        .unwrap();
    let a = place(&mut s, Owner::Civ(0), Spearman, 20_000, hex);
    s.city_states[0].influence[0] = 10_000;
    s.city_states[1].influence[0] = 10_000;
    s.city_states[0].defense = 1_000;
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![Order::Attack {
                army: a,
                target: AttackTarget::CityState(0),
            }],
        )],
    );
    assert_eq!(s.civs[0].last_aggression, Some(0));
    assert!(s.civs[0].protection_lost);
    assert!(s.city_states.iter().all(|cs| cs.influence[0] == 0));
    assert_eq!(s.city_states[0].captured_by, Some(0));
    let city = s.cities.iter().find(|c| c.hex == cs_hex).unwrap();
    assert_eq!(city.owner, Some(0));
    assert_eq!(s.units[a as usize].hex, cs_hex);
}

#[test]
fn lone_settlers_are_captured() {
    let (rules, mut s) = setup();
    war_between_0_and_1(&mut s, &rules);
    let (a_hex, b_hex, _) = open_ground(&s);
    let a = place(&mut s, Owner::Civ(0), Spearman, 5_000, a_hex);
    let settler = place(&mut s, Owner::Civ(1), Settler, 1_000, b_hex);
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![Order::Attack {
                army: a,
                target: AttackTarget::Unit(settler),
            }],
        )],
    );
    assert_eq!(s.units[settler as usize].owner, Owner::Civ(0));
    // 30 (war, not decayed the tick it was added, v0.2 C7) − 1 + 5 (captured settler).
    assert_eq!(s.grievance(0, 1), 30 - 1 + 5);
}

#[test]
fn casualties_raise_war_weariness() {
    let (rules, mut s) = setup();
    war_between_0_and_1(&mut s, &rules);
    let ww_before = s.civs[1].war_weariness;
    let (a_hex, b_hex, _) = open_ground(&s);
    let a = place(&mut s, Owner::Civ(0), Pikeman, 20_000, a_hex);
    let b = place(&mut s, Owner::Civ(1), Spearman, 20_000, b_hex);
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![Order::Attack {
                army: a,
                target: AttackTarget::Unit(b),
            }],
        )],
    );
    let lost = 20_000 - s.units[b as usize].troops;
    // Defender at war (+1), loses troops (+lost/2000), then −1 decay while at war.
    assert_eq!(s.civs[1].war_weariness, ww_before + 1 + lost / 2000 - 1);
    assert!(lost >= 4_000);
}

#[test]
fn a_capture_counts_as_a_city_held_and_earns_merit() {
    let (rules, mut s) = setup();
    for _ in 0..12 {
        step(&mut s, &rules, vec![]);
    }
    war_between_0_and_1(&mut s, &rules);
    let (a, ci) = besiege(&mut s, Spearman, 20_000);
    remove_garrison(&mut s, ci);
    s.cities[ci].defense = 0;
    let pop = s.cities[ci].pop;
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![Order::Attack {
                army: a,
                target: AttackTarget::City(ci as u32),
            }],
        )],
    );
    assert!(
        s.cities[ci].capture_scores,
        "founded ≥12 ticks before capture"
    );
    assert_eq!(permutation_rules::scoring::captured_held(&s, 0), 1);
    // The acting official issued the attack: the capture earns nobody merit.
    assert!(s.merit_log.iter().all(|m| m.what != b"capture"));
    let _ = pop;
}

#[test]
fn war_replay_is_deterministic() {
    let script = || {
        let (rules, mut s) = setup();
        war_between_0_and_1(&mut s, &rules);
        let (a, ci) = besiege(&mut s, Spearman, 20_000);
        let (x_hex, y_hex, z_hex) = open_ground(&s);
        let x = place(&mut s, Owner::Civ(0), Horseman, 12_000, x_hex);
        let y = place(&mut s, Owner::Civ(1), Archer, 15_000, y_hex);
        let z = place(&mut s, Owner::Civ(1), Crossbowman, 8_000, z_hex);
        let mut roots = Vec::new();
        for _ in 0..6 {
            let mut orders = vec![(
                0,
                vec![
                    Order::Attack {
                        army: a,
                        target: AttackTarget::City(ci as u32),
                    },
                    Order::Attack {
                        army: x,
                        target: AttackTarget::Unit(y),
                    },
                ],
            )];
            orders.push((
                1,
                vec![Order::Attack {
                    army: z,
                    target: AttackTarget::Unit(x),
                }],
            ));
            step(&mut s, &rules, orders);
            roots.push(s.state_root().unwrap());
        }
        roots
    };
    assert_eq!(script(), script());
}

#[test]
fn captured_cities_restart_at_half_defence() {
    let (rules, mut s) = setup();
    war_between_0_and_1(&mut s, &rules);
    let (a, ci) = besiege(&mut s, Spearman, 20_000);
    remove_garrison(&mut s, ci);
    s.cities[ci].pop = 5;
    s.cities[ci].defense = 0;
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![Order::Attack {
                army: a,
                target: AttackTarget::City(ci as u32),
            }],
        )],
    );
    let c = &s.cities[ci];
    assert_eq!(c.owner, Some(0));
    // pop 5 → 4 on capture; max defence (4 + 4) × 1000; half of it, no regen while attacked.
    assert_eq!(c.defense, (rules.city_defense_base + c.pop) * 1000 / 2);
}
