//! Damage (§8.1–§8.3): every engagement from pre-combat counts, applied at once.

use super::*;
use crate::combat::{resolve_engagement, variance, Combatant};
use crate::gov::Path;
use crate::merit;
use crate::params::Ruleset;
use crate::state::{Owner, WorldState};
use alloc::vec;

pub(super) fn combatant(state: &WorldState, d: Defender) -> Combatant {
    match d {
        Defender::Unit(u) => Combatant::Army {
            unit: state.units[u].unit_type,
            troops: state.units[u].troops,
        },
        Defender::City(c) => Combatant::City {
            defense: state.cities[c].defense,
        },
        Defender::CityState(c) => Combatant::City {
            defense: state.city_states[c].defense,
        },
    }
}

pub(super) fn apply_damage(state: &mut WorldState, rules: &Ruleset, engagements: &[Engagement]) {
    let mut to_unit = vec![0u64; state.units.len()];
    let mut to_city = vec![0u64; state.cities.len()];
    let mut to_cs = vec![0u64; state.city_states.len()];
    // Damage each engagement dealt to a defending unit, for merit.
    let mut dealt = vec![0u64; engagements.len()];

    // All damage from pre-combat counts (§8.1).
    for (id, e) in engagements.iter().enumerate() {
        let a = &state.units[e.attacker];
        let attacker = Combatant::Army {
            unit: a.unit_type,
            troops: a.troops,
        };
        let defender = combatant(state, e.defender);
        let va = variance(rules, &state.tick_seed, id as u32, 0);
        let vd = variance(rules, &state.tick_seed, id as u32, 1);
        let (to_def, to_att) = resolve_engagement(rules, attacker, defender, e.sit, va, vd);
        to_unit[e.attacker] += to_att as u64;
        match e.defender {
            Defender::Unit(u) => {
                to_unit[u] += to_def as u64;
                dealt[id] = to_def as u64;
            }
            Defender::City(c) => to_city[c] += to_def as u64,
            Defender::CityState(c) => to_cs[c] += to_def as u64,
        }
    }

    // Apply at once.
    let mut lost_by_unit = vec![0u64; state.units.len()];
    for (i, dmg) in to_unit.iter().enumerate() {
        if *dmg == 0 {
            continue;
        }
        let u = &mut state.units[i];
        let before = u.troops;
        u.troops = before.saturating_sub((*dmg).min(u32::MAX as u64) as u32);
        if u.troops < rules.army_min_milli {
            u.troops = 0;
            u.alive = false;
            u.path.clear();
        }
        let lost = before - u.troops;
        lost_by_unit[i] = lost as u64;
        if let Owner::Civ(c) = u.owner {
            state.civs[c as usize].troops_lost += lost;
        }
    }
    // Enemy troops destroyed: 1 merit per troop to the attack's issuer; when
    // several attacks hit one unit, its losses are shared by damage dealt
    // (V5 §7.3).
    for (id, e) in engagements.iter().enumerate() {
        let Defender::Unit(u) = e.defender else {
            continue;
        };
        if dealt[id] == 0 || lost_by_unit[u] == 0 {
            continue;
        }
        let lost = lost_by_unit[u] * dealt[id] / to_unit[u];
        let m = lost * rules.merit_per_troop as u64 / 1000;
        merit::credit(state, e.credit, Path::Hegemony, m, b"troops");
        // Whole enemy troops destroyed, per attacking civ (a eureka trigger).
        if let Owner::Civ(c) = state.units[e.attacker].owner {
            let a = &mut state.civs[c as usize].achievements;
            a.kills = a.kills.saturating_add((lost / 1000) as u32);
        }
    }
    for (i, dmg) in to_city.into_iter().enumerate() {
        let c = &mut state.cities[i];
        c.defense = c.defense.saturating_sub(dmg.min(u32::MAX as u64) as u32);
    }
    for (i, dmg) in to_cs.into_iter().enumerate() {
        let c = &mut state.city_states[i];
        c.defense = c.defense.saturating_sub(dmg.min(u32::MAX as u64) as u32);
    }
    // A city under attack (garrison or walls) does not regenerate this tick.
    for e in engagements {
        if let Some(c) = e.city_target {
            state.cities[c].attacked_this_tick = true;
        }
    }
}
