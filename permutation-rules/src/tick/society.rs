//! Phases 7–9: upkeep (§6.1), society — war weariness, loyalty, grievance
//! decay, city regeneration (§9) — and neutral actors (§12).

use crate::economy::{amenities, city_upkeep, effective_troops_milli, upkeep_of_effective};
use crate::fixed::MILLI;
use crate::movement::allied;
use crate::params::Ruleset;
use crate::state::{CivId, Owner, Relation, Unit, WorldState};
use crate::units::stats;
use alloc::vec::Vec;

pub(crate) fn phase_upkeep(state: &mut WorldState) {
    for civ in 0..state.civs.len() as u16 {
        let cities = state.city_count(civ);
        let owned = |u: &&Unit| u.alive && u.owner == Owner::Civ(civ);
        // Effective troops are summed once and updated as troops disband.
        let mut eff: u64 = state
            .units
            .iter()
            .filter(owned)
            .map(effective_troops_milli)
            .sum();
        let upkeep = |eff: u64| (upkeep_of_effective(eff) + city_upkeep(cities)) as i64 * MILLI;
        let mut balance = state.civs[civ as usize].gold - upkeep(eff);
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
            eff -= effective_troops_milli(u);
            u.troops = u.troops.saturating_sub(1000);
            if u.troops < 500 {
                u.alive = false;
            } else {
                eff += effective_troops_milli(u);
            }
            balance = state.civs[civ as usize].gold - upkeep(eff);
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

    // Loyalty (§9.4) and city defence regeneration (§8.3). Armies neither
    // move nor fall here (freeing a city displaces only civilians), and a
    // city only changes hands by going free, so garrisons are indexed by
    // tile once and city counts are kept as cities go free.
    let mut garrisons: Vec<(usize, CivId)> = state
        .units
        .iter()
        .filter(|u| u.alive && !u.unit_type.is_civilian())
        .filter_map(|u| match (state.map.index_of(u.hex), u.owner) {
            (Some(t), Owner::Civ(c)) => Some((t, c)),
            _ => None,
        })
        .collect();
    garrisons.sort_unstable();
    let mut city_counts: Vec<u32> = (0..n).map(|c| state.city_count(c)).collect();
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
        let garrisoned = match state.map.index_of(city.hex) {
            Some(t) => garrisons.binary_search(&(t, owner)).is_ok(),
            None => false, // no unit stands off the map
        };
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
        let mut amen = amenities(city, city_counts[owner as usize], civ.war_weariness);
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
            city_counts[owner as usize] -= 1;
            city.owner = None; // Free City
            city.standing = crate::state::CityStanding::DEFAULT;
            let (id, hex) = (city.id, city.hex);
            state.push_event(b"free_city", &id.to_le_bytes());
            crate::battle::displace_all(state, hex);
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
    crisis(state, rules);
}

/// The crisis (§12.2, revised 2026-09-25): from `crisis_start`, every
/// `crisis_interval` ticks, the `crisis_targets` leading nations with
/// members (by points; ties: lower id) each lose `crisis_pop` population
/// and `crisis_loyalty` loyalty in their most populous city (ties: lower
/// id). A city at 0 loyalty becomes a Free City (phase 8), so the leaders
/// must keep their cities content (temples) or lose them.
fn crisis(state: &mut WorldState, rules: &Ruleset) {
    let t = state.tick;
    if t < rules.crisis_start
        || rules.crisis_interval == 0
        || (t - rules.crisis_start) % rules.crisis_interval != 0
    {
        return;
    }
    let scores = crate::scoring::nation_scores(state, rules);
    let mut leaders: Vec<(u64, usize)> = (0..scores.len())
        .filter(|c| state.nations[*c].members > 0 && state.city_count(*c as u16) > 0)
        .map(|c| (scores[c].total(), c))
        .collect();
    leaders.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for (_, civ) in leaders.into_iter().take(rules.crisis_targets as usize) {
        let Some(id) = state
            .living_cities_of(civ as u16)
            .max_by_key(|c| (c.pop, core::cmp::Reverse(c.id)))
            .map(|c| c.id)
        else {
            continue;
        };
        let c = &mut state.cities[id as usize];
        c.pop = c.pop.saturating_sub(rules.crisis_pop).max(1);
        c.loyalty = (c.loyalty - rules.crisis_loyalty).max(0);
        let mut payload = [0u8; 6];
        payload[..2].copy_from_slice(&(civ as u16).to_le_bytes());
        payload[2..].copy_from_slice(&id.to_le_bytes());
        state.push_event(b"crisis", &payload);
    }
}
