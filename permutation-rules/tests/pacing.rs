//! Paths and pacing (revised 2026-09-25): eurekas, the crisis on the
//! leaders, and the dark age for nations far behind.

mod common;
use common::nations::*;
use permutation_rules::economy::{civ_tech_cost, tech_cost};
use permutation_rules::fixed::MILLI;
use permutation_rules::tech::{Tech, TECHS};

/// Horses in stock fire the Horseback Riding eureka: 40% off, no order spent.
#[test]
fn a_eureka_makes_its_tech_cheaper() {
    let (rules, mut s) = world(&[]);
    let base = tech_cost(&rules, Tech::HorsebackRiding, 1);
    assert_eq!(civ_tech_cost(&s, &rules, 0, Tech::HorsebackRiding), base);
    s.civs[0].horses = 5 * MILLI;
    idle(&mut s, &rules, 1);
    assert!(s.civs[0].achievements.boosted.has(Tech::HorsebackRiding));
    assert_eq!(
        civ_tech_cost(&s, &rules, 0, Tech::HorsebackRiding),
        base * rules.eureka_cost_bps as u64 / 10_000
    );
    assert!(!s.civs[1].achievements.boosted.has(Tech::HorsebackRiding));
}

/// Nations 0 and 1 have members; 0 leads (every tech), 1 has nothing.
fn leader_and_laggard() -> (
    permutation_rules::Ruleset,
    permutation_rules::state::WorldState,
) {
    let (rules, mut s) = world(&[1, 1, 0, 0, 0, 0]);
    for t in TECHS {
        s.civs[0].techs.insert(t.tech);
    }
    (rules, s)
}

/// From `crisis_start`, every `crisis_interval` ticks, the leading nations
/// with members lose population and loyalty in their largest city.
#[test]
fn the_crisis_strikes_the_leaders() {
    let (rules, mut s) = leader_and_laggard();
    let cap0 = s.civs[0].capital.unwrap() as usize;
    let cap2 = s.civs[2].capital.unwrap() as usize;
    s.cities[cap0].pop = 5;
    s.cities[cap2].pop = 5;
    s.tick = rules.crisis_start - 1;
    idle(&mut s, &rules, 1); // no crisis before crisis_start
    assert_eq!(s.cities[cap0].pop, 5);
    let loyalty = s.cities[cap0].loyalty;
    idle(&mut s, &rules, 1); // tick crisis_start
    assert_eq!(s.cities[cap0].pop, 4, "the leader's largest city shrinks");
    assert!(s.cities[cap0].loyalty <= loyalty - rules.crisis_loyalty + 5);
    assert_eq!(
        s.cities[cap2].pop, 5,
        "a nation without members is not a target"
    );
}

/// At `dark_age_tick`, a nation with members far behind the leader researches
/// cheaper and gets an extra order until `dark_age_until`.
#[test]
fn a_nation_far_behind_enters_a_dark_age() {
    let (rules, mut s) = leader_and_laggard();
    s.tick = rules.dark_age_tick;
    let before = s.civs[1].tick_budget;
    idle(&mut s, &rules, 1);
    assert_eq!(
        s.civs[1].achievements.dark_age_until,
        Some(rules.dark_age_until)
    );
    assert_eq!(
        s.civs[0].achievements.dark_age_until, None,
        "not the leader"
    );
    assert_eq!(
        s.civs[2].achievements.dark_age_until, None,
        "not a nation without members"
    );
    assert_eq!(s.civs[1].tick_budget, before + rules.dark_age_budget);
    let base = tech_cost(&rules, Tech::Writing, 1);
    assert_eq!(
        civ_tech_cost(&s, &rules, 1, Tech::Writing),
        base * rules.dark_age_research_bps as u64 / 10_000
    );
    s.tick = rules.dark_age_until;
    assert_eq!(
        civ_tech_cost(&s, &rules, 1, Tech::Writing),
        base,
        "over at dark_age_until"
    );
}
