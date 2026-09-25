//! Planning (§8): which attacks are valid against the pre-combat state,
//! protection zones, and the forecast shown to players and agents.

use super::*;
use crate::buildings::Building;
use crate::checks::Blocked;
use crate::combat::{resolve_engagement, Combatant, Situation};
use crate::gov::Credit;
use crate::orders::AttackTarget;
use crate::params::Ruleset;
use crate::state::{City, CivId, Owner, WorldState};
use crate::tech::Tech;
use crate::units::{stats, UnitClass};

pub(crate) fn is_protected(
    state: &WorldState,
    rules: &Ruleset,
    attacker: CivId,
    hex: crate::hex::Hex,
) -> bool {
    let radius = rules.protection_radius(state.tick) as u32;
    radius > 0
        && state.civs.iter().any(|c| {
            c.id != attacker
                && !c.protection_lost
                && c.capital
                    .and_then(|id| state.cities.get(id as usize))
                    .is_some_and(|cap| cap.alive && cap.hex.distance(hex) <= radius)
        })
}

pub(crate) fn hostile(state: &WorldState, civ: CivId, owner: Owner) -> bool {
    match owner {
        Owner::Barbarian => true,
        Owner::Civ(o) => o != civ && state.at_war(civ, o),
    }
}

pub(super) fn garrison_of(state: &WorldState, city: &City) -> Option<usize> {
    let owner = city.owner?;
    state.units.iter().position(|u| {
        u.alive && u.hex == city.hex && u.owner == Owner::Civ(owner) && !u.unit_type.is_civilian()
    })
}

pub(super) fn city_at(state: &WorldState, hex: crate::hex::Hex) -> Option<usize> {
    state.cities.iter().position(|c| c.alive && c.hex == hex)
}

pub(super) fn plan(
    state: &WorldState,
    rules: &Ruleset,
    civ: CivId,
    army: u32,
    target: AttackTarget,
) -> Result<Plan, Blocked> {
    let a_idx = army as usize;
    let a = state
        .units
        .get(a_idx)
        .filter(|u| u.alive)
        .ok_or(Blocked::UnknownUnit)?;
    if a.owner != Owner::Civ(civ) {
        return Err(Blocked::NotYours);
    }
    if a.unit_type.is_civilian() {
        return Err(Blocked::CivilianCannotAttack);
    }
    let a_stats = stats(a.unit_type);

    // Resolve the target to (hex, defender, city_target, neutral).
    let (hex, defender, city_target, neutral) = match target {
        AttackTarget::Unit(id) => {
            let t = state
                .units
                .get(id as usize)
                .filter(|t| t.alive)
                .ok_or(Blocked::TargetGone)?;
            if !hostile(state, civ, t.owner) {
                return Err(Blocked::TargetNotHostile);
            }
            match city_at(state, t.hex).filter(|&c| state.cities[c].owner == t.owner.civ()) {
                // A unit standing in its own city: this is an attack on the city.
                Some(c) => {
                    let d = garrison_of(state, &state.cities[c])
                        .map_or(Defender::City(c), Defender::Unit);
                    (t.hex, d, Some(c), false)
                }
                None if t.unit_type.is_civilian() => {
                    let lone = state
                        .units
                        .iter()
                        .filter(|u| u.alive && u.hex == t.hex)
                        .count()
                        == 1;
                    let melee = a_stats.class != UnitClass::Ranged;
                    let d = a.hex.distance(t.hex);
                    if !(lone && melee && d == 1) {
                        return Err(Blocked::OutOfRange {
                            distance: d,
                            range: 1,
                        });
                    }
                    if is_protected(state, rules, civ, t.hex) {
                        return Err(Blocked::TargetProtected);
                    }
                    return Ok(Plan::CaptureCivilian {
                        attacker: a_idx,
                        civ,
                        target: id as usize,
                    });
                }
                None => (t.hex, Defender::Unit(id as usize), None, false),
            }
        }
        AttackTarget::City(id) => {
            let c = state
                .cities
                .get(id as usize)
                .filter(|c| c.alive)
                .ok_or(Blocked::TargetGone)?;
            let neutral = match c.owner {
                Some(o) if o == civ => return Err(Blocked::NotYours),
                Some(o) if !state.at_war(civ, o) => return Err(Blocked::NotAtWar),
                Some(_) => false,
                None => true, // Free City
            };
            let d = garrison_of(state, c).map_or(Defender::City(id as usize), Defender::Unit);
            (c.hex, d, Some(id as usize), neutral)
        }
        AttackTarget::CityState(id) => {
            let cs = state
                .city_states
                .get(id as usize)
                .filter(|cs| cs.captured_by.is_none())
                .ok_or(Blocked::TargetGone)?;
            (cs.hex, Defender::CityState(id as usize), None, true)
        }
    };

    let d = a.hex.distance(hex);
    let in_range = match a_stats.class {
        UnitClass::Ranged => (1..=a_stats.range as u32).contains(&d),
        _ => d == 1,
    };
    if !in_range {
        let range = if a_stats.class == UnitClass::Ranged {
            a_stats.range as u32
        } else {
            1
        };
        return Err(Blocked::OutOfRange { distance: d, range });
    }
    if is_protected(state, rules, civ, hex) {
        return Err(Blocked::TargetProtected);
    }

    let tile = |h| state.map.tile(h);
    // A city strikes back at a ranged attacker (v0.2 C5), garrisoned or not.
    let city_strike = if a_stats.class == UnitClass::Ranged {
        match (defender, city_target) {
            (Defender::CityState(c), _) => state.city_states[c].defense,
            (_, Some(c)) => state.cities[c].defense,
            _ => 0,
        }
    } else {
        0
    };
    let sit = Situation {
        city_strike,
        ranged_attack: a_stats.class == UnitClass::Ranged,
        defender_on_rough_terrain: matches!(defender, Defender::Unit(u)
            if tile(state.units[u].hex).is_some_and(|t| t.terrain.info().defense_bps < 10_000)),
        defender_fortified: matches!(defender, Defender::Unit(u)
            if state.units[u].last_moved.is_none_or(|m| state.tick.saturating_sub(m) >= 2)),
        attacker_exhausted: a.used_full_mp,
        attacker_on_river: tile(a.hex).is_some_and(|t| t.river),
        city_walls: matches!(defender, Defender::City(c) if state.cities[c].buildings.has(Building::Walls)),
        engineering: matches!(defender, Defender::City(c)
            if state.cities[c].owner.is_some_and(|o| state.civs[o as usize].techs.has(Tech::Engineering))),
    };
    Ok(Plan::Engage(Engagement {
        attacker: a_idx,
        civ,
        defender,
        city_target,
        neutral,
        sit,
        credit: Credit::NONE,
    }))
}

/// Expected outcome of one attack at mean variance (v = 10000), for
/// previews. The real tick draws variance from `seed_t` and resolves all
/// attacks together, so several attackers change the picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackForecast {
    /// Milli-troops each side is expected to lose.
    Engagement {
        to_defender: u32,
        to_attacker: u32,
        defender_troops: u32,
        neutral: bool,
        city: bool,
    },
    CaptureCivilian,
}

pub fn forecast_attack(
    state: &WorldState,
    rules: &Ruleset,
    civ: CivId,
    army: u32,
    target: AttackTarget,
) -> Result<AttackForecast, Blocked> {
    match plan(state, rules, civ, army, target)? {
        Plan::CaptureCivilian { .. } => Ok(AttackForecast::CaptureCivilian),
        Plan::Engage(e) => {
            let a = &state.units[e.attacker];
            let attacker = Combatant::Army {
                unit: a.unit_type,
                troops: a.troops,
            };
            let defender = combatant(state, e.defender);
            let (to_defender, to_attacker) =
                resolve_engagement(rules, attacker, defender, e.sit, 10_000, 10_000);
            let defender_troops = match defender {
                Combatant::Army { troops, .. } => troops,
                Combatant::City { defense } => defense,
            };
            Ok(AttackForecast::Engagement {
                to_defender,
                to_attacker,
                defender_troops,
                neutral: e.neutral,
                city: e.city_target.is_some() || matches!(e.defender, Defender::CityState(_)),
            })
        }
    }
}
