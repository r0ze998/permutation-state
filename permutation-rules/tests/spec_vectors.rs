//! Reference values that the spec (PERMUTATION_STATE_RULES_SPEC_v0.1.md)
//! states explicitly. Any implementation must reproduce them exactly.

use permutation_rules::combat::{damage, modifier, resolve_engagement, Combatant, Situation};
use permutation_rules::economy::unit_upkeep;
use permutation_rules::hex::Hex;
use permutation_rules::state::{Owner, StandingRule, Unit};
use permutation_rules::units::UnitType::*;
use permutation_rules::{Preset, Ruleset};

fn rules() -> Ruleset {
    Ruleset::new(Preset::Blitz)
}

const V1: u32 = 10_000;

#[test]
fn vector_a_equal_spearmen() {
    let r = rules();
    let a = Combatant::Army {
        unit: Spearman,
        troops: 10_000,
    };
    let b = Combatant::Army {
        unit: Spearman,
        troops: 10_000,
    };
    assert_eq!(
        resolve_engagement(&r, a, b, Situation::default(), V1, V1),
        (3020, 3020)
    );
}

#[test]
fn vector_b_spearmen_counter_horsemen() {
    let r = rules();
    let a = Combatant::Army {
        unit: Spearman,
        troops: 20_000,
    };
    let b = Combatant::Army {
        unit: Horseman,
        troops: 10_000,
    };
    assert_eq!(
        resolve_engagement(&r, a, b, Situation::default(), V1, V1),
        (8358, 2786)
    );
}

#[test]
fn vector_c_archers_ranged_on_hills() {
    let r = rules();
    let a = Combatant::Army {
        unit: Archer,
        troops: 10_000,
    };
    let b = Combatant::Army {
        unit: Spearman,
        troops: 12_000,
    };
    let sit = Situation {
        ranged_attack: true,
        defender_on_rough_terrain: true,
        ..Situation::default()
    };
    assert_eq!(resolve_engagement(&r, a, b, sit, V1, V1), (2844, 0));
}

#[test]
fn vector_d_spearmen_vs_walled_city() {
    let r = rules();
    let a = Combatant::Army {
        unit: Spearman,
        troops: 20_000,
    };
    let city = Combatant::City { defense: 14_000 }; // pop 10: (4 + 10) troops
    let sit = Situation {
        city_walls: true,
        ..Situation::default()
    };
    assert_eq!(resolve_engagement(&r, a, city, sit, V1, V1), (3622, 1902));
}

#[test]
fn vector_e_pikemen_vs_double_spearmen() {
    let r = rules();
    let a = Combatant::Army {
        unit: Pikeman,
        troops: 10_000,
    };
    let b = Combatant::Army {
        unit: Spearman,
        troops: 20_000,
    };
    assert_eq!(
        resolve_engagement(&r, a, b, Situation::default(), V1, V1),
        (6129, 2532)
    );
}

#[test]
fn vector_f_variance_bounds() {
    let r = rules();
    assert_eq!(damage(&r, 10_000, 10, 10_000, 10, &[], 9_000), 2718);
    assert_eq!(damage(&r, 10_000, 10, 10_000, 10, &[], 11_000), 3322);
}

#[test]
fn modifier_constants_match_spec() {
    assert_eq!(modifier::COUNTER, 15_000);
    assert_eq!(modifier::WALLS, 6_667);
    assert_eq!(modifier::CITY_RETALIATION, 5_000);
}

#[test]
fn unit_upkeep_table() {
    let army = |troops: u32| Unit {
        id: 0,
        owner: Owner::Civ(0),
        unit_type: Spearman,
        troops,
        hex: Hex::ORIGIN,
        path: Vec::new(),
        last_moved: None,
        used_full_mp: false,
        standing: StandingRule::None,
        alive: true,
    };
    let got: Vec<u64> = [10, 20, 30, 40, 60]
        .iter()
        .map(|t| {
            // Split into ≤20-troop armies like real play.
            let armies: Vec<Unit> = (0..(t / 20))
                .map(|_| army(20_000))
                .chain((t % 20 > 0).then(|| army((t % 20) * 1000)))
                .collect();
            unit_upkeep(armies.iter())
        })
        .collect();
    assert_eq!(got, [3, 10, 18, 30, 60]);
}
