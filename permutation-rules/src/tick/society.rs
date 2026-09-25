//! Phases 7–9: upkeep (§6.1), society — war weariness, loyalty, grievance
//! decay, city regeneration (§9) — and neutral actors (§12).

use crate::economy::{amenities, city_upkeep, unit_upkeep};
use crate::fixed::MILLI;
use crate::movement::allied;
use crate::params::Ruleset;
use crate::state::{Owner, Relation, Unit, WorldState};
use crate::units::stats;

pub(crate) fn phase_upkeep(state: &mut WorldState) {
    for civ in 0..state.civs.len() as u16 {
        let cities = state.city_count(civ);
        let owned = |u: &&Unit| u.alive && u.owner == Owner::Civ(civ);
        let upkeep = |s: &WorldState| {
            (unit_upkeep(s.units.iter().filter(owned)) + city_upkeep(cities)) as i64 * MILLI
        };
        let mut balance = state.civs[civ as usize].gold - upkeep(state);
        if balance >= 0 {
            state.civs[civ as usize].gold = balance;
            continue;
        }
        // Deficit (§6.1): disband whole troops, T2 first, then largest army, then highest id.
        while balance < 0 {
            let victim = state
                .units
                .iter()
                .filter(owned)
                .filter(|u| !u.unit_type.is_civilian())
                .max_by_key(|u| (stats(u.unit_type).tier, u.troops, u.id))
                .map(|u| u.id as usize);
            let Some(v) = victim else { break };
            let u = &mut state.units[v];
            u.troops = u.troops.saturating_sub(1000);
            if u.troops < 500 {
                u.alive = false;
            }
            balance = state.civs[civ as usize].gold - upkeep(state);
        }
        let c = &mut state.civs[civ as usize];
        c.gold = balance.max(0);
        c.deficit = true;
    }
}

// ---------------------------------------------------------------- phase 8

pub(crate) fn phase_society(state: &mut WorldState, rules: &Ruleset) {
    let n = state.civs.len() as u16;
    // War weariness (§9.3), including +1 per 2 whole troops lost this tick.
    for a in 0..n {
        let mut gain = 0u32;
        let mut at_war = false;
        for b in 0..n {
            if a == b || !state.at_war(a, b) {
                continue;
            }
            at_war = true;
            gain += match state.relation(a, b) {
                Relation::War {
                    declared_by,
                    casus_belli: false,
                    ..
                } if declared_by == a => 2,
                _ => 1,
            };
        }
        let c = &mut state.civs[a as usize];
        gain += c.troops_lost / 2000;
        c.war_weariness = (c.war_weariness + gain).saturating_sub(if at_war { 1 } else { 3 });
    }

    // Loyalty (§9.4) and city defence regeneration (§8.3).
    for i in 0..state.cities.len() {
        let city = &state.cities[i];
        let (Some(owner), true) = (city.owner, city.alive) else {
            continue;
        };
        let civ = &state.civs[owner as usize];
        let capital_hex = civ
            .capital
            .and_then(|id| state.cities.get(id as usize))
            .filter(|c| c.alive)
            .map(|c| c.hex);
        let mut delta = 2;
        if capital_hex.is_some_and(|h| h.distance(city.hex) <= 6) {
            delta += 1;
        }
        let garrisoned = state.units.iter().any(|u| {
            u.alive
                && u.hex == city.hex
                && u.owner == Owner::Civ(owner)
                && !u.unit_type.is_civilian()
        });
        if garrisoned {
            delta += 3;
        }
        let pressure: u32 = state
            .cities
            .iter()
            .filter(|o| o.alive && o.hex.distance(city.hex) <= rules.loyalty_pressure_radius as u32)
            .filter(|o| matches!(o.owner, Some(x) if x != owner && !allied(state, x, owner)))
            .map(|o| o.pop)
            .sum();
        delta -= (pressure / 3) as i32;
        let mut amen = amenities(city, state.city_count(owner), civ.war_weariness);
        if civ.deficit {
            amen -= 2;
        }
        if amen <= -3 {
            delta -= 2;
        }
        let max_def = (rules.city_defense_base + city.pop) * 1000;
        let attacked = city.attacked_this_tick;
        let city = &mut state.cities[i];
        city.loyalty = (city.loyalty + delta).clamp(0, 100);
        if !attacked {
            city.defense = (city.defense + rules.city_regen_milli).min(max_def);
        }
        if city.loyalty == 0 {
            city.owner = None; // Free City
            city.standing = crate::state::CityStanding::DEFAULT;
            let (id, hex) = (city.id, city.hex);
            state.push_event(b"free_city", &id.to_le_bytes());
            crate::battle::displace_civilians(state, hex);
        }
    }

    // Grievance decays by 1, but what was added this tick does not (v0.2 C7).
    for (g, fresh) in state
        .grievance
        .iter_mut()
        .zip(state.grievance_fresh.iter_mut())
    {
        if *g > *fresh {
            *g -= 1;
        }
        *fresh = 0;
    }
}

// ---------------------------------------------------------------- phase 9

pub(crate) fn phase_neutral(state: &mut WorldState, rules: &Ruleset) {
    let tick = state.tick;
    for cs in &mut state.city_states {
        if cs.captured_by.is_some() {
            continue;
        }
        if tick > 0 && tick % 30 == 0 && cs.pop < 6 {
            cs.pop += 1;
        }
        cs.defense = (cs.defense + rules.city_regen_milli).min((8 + 2 * cs.pop) * 1000);
        // Suzerainty contest cycles restart every `suzerain_lock_ticks` (§12.1).
        if tick > 0 && tick % rules.suzerain_lock_ticks == 0 {
            cs.suzerain = None;
            for v in &mut cs.influence {
                *v /= 2;
            }
        }
    }
    // TODO(§12.2): Crisis waves at ticks 120, 126, …, 156.
}
