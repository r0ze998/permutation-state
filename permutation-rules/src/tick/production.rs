//! Phase 6 (§5.3–§5.6): yields, growth, production queues (one item per
//! tick, C1), Star Gate spacing (C2) and spawning units.

use super::city_orders::item_cost;
use crate::economy::{amenities, apply_amenities, city_yield, growth_threshold, tech_cost};
use crate::fixed::{apply_bps, milli, MILLI};
use crate::gov::{active_officer, Credit, Path, Role};
use crate::map::territory_radius;
use crate::merit;
use crate::movement::tile_free_for;
use crate::params::Ruleset;
use crate::state::{CivId, Owner, QueueItem, StandingRule, Unit, WorldState};
use crate::tech::Tech;
use crate::units::{stats, UnitType};
use alloc::vec::Vec;

pub(crate) fn phase_production(state: &mut WorldState, rules: &Ruleset) {
    let mut last = alloc::vec![crate::state::LastYields::default(); state.civs.len()];
    let mut grown: Vec<Credit> = Vec::new();
    for i in 0..state.cities.len() {
        let (owner, alive) = (state.cities[i].owner, state.cities[i].alive);
        let razing = state.cities[i].razing.is_some();
        let (Some(civ), true, false) = (owner, alive, razing) else {
            continue;
        };
        let c_ref = &state.civs[civ as usize];
        let is_capital = c_ref.capital == Some(i as u32);
        let has_philosophy = c_ref.techs.has(Tech::Philosophy);
        let ww = c_ref.war_weariness;
        let civ_cities = state.city_count(civ);

        let (y, worked) = city_yield(&state.map, &state.cities[i], is_capital, has_philosophy);
        let amen = amenities(&state.cities[i], civ_cities, ww);
        let (surplus, prod_bps) = apply_amenities(rules, y.food, state.cities[i].pop, amen);

        // Food and growth (§5.3).
        {
            let city = &mut state.cities[i];
            city.food += surplus * MILLI;
            let threshold = growth_threshold(city.pop) as i64 * MILLI;
            if city.food >= threshold {
                city.food -= threshold;
                city.pop += 1;
                grown.push(city.steward_credit);
            } else if city.food < 0 {
                city.pop = city.pop.saturating_sub(1).max(1);
                city.food = 0;
            }
            city.prod += apply_bps(y.prod as i64 * MILLI, prod_bps);
            if city.heritage_until.is_some_and(|t| state.tick >= t) {
                city.heritage_until = None;
                city.heritage_bonus = 0;
            }
        }
        let (hex, pop, id) = (state.cities[i].hex, state.cities[i].pop, i as u32);
        state.map.claim_territory(id, hex, territory_radius(pop));

        // Civilization-level yields.
        {
            let l = &mut last[civ as usize];
            l.gold += y.gold;
            l.max_city_food_surplus = l.max_city_food_surplus.max(surplus.max(0) as u32);
            l.max_city_prod = l.max_city_prod.max(y.prod);
            // The Science focus multiplies the city's science in milli units
            // (v0.2 C6: applied before rounding, so it always has an effect).
            let science = if state.cities[i].focus == crate::state::Focus::Science {
                y.science as i64 * 12_500
            } else {
                y.science as i64 * MILLI
            };
            let c = &mut state.civs[civ as usize];
            c.gold += y.gold as i64 * MILLI;
            c.achievements.wealth += y.gold as u64;
            c.science_store += science;
            c.influence += y.influence as i64 * MILLI;
            c.scores.science_total += (science / MILLI) as u64;
        }
        for t in worked {
            let tile = &mut state.map.tiles[t];
            if tile.reserve == 0 {
                continue;
            }
            match tile.resource {
                Some(crate::map::TileResource::Iron) => {
                    state.civs[civ as usize].iron += MILLI;
                    last[civ as usize].iron += 1;
                }
                Some(crate::map::TileResource::Horses) => {
                    state.civs[civ as usize].horses += MILLI;
                    last[civ as usize].horses += 1;
                }
                _ => continue,
            }
            tile.reserve -= 1;
        }

        complete_queue(state, rules, civ, i);
    }

    // City-state suzerain bonuses (§12.1).
    for cs in 0..state.city_states.len() {
        let (Some(civ), None) = (
            state.city_states[cs].suzerain,
            state.city_states[cs].captured_by,
        ) else {
            continue;
        };
        let c = &mut state.civs[civ as usize];
        match state.city_states[cs].specialty {
            crate::state::Specialty::Scientific => {
                c.science_store += 3 * MILLI;
                c.scores.science_total += 3;
            }
            crate::state::Specialty::Mercantile => {
                c.gold += 4 * MILLI;
                c.achievements.wealth += 4;
                last[civ as usize].gold += 4;
            }
            crate::state::Specialty::Agrarian => {
                if let Some(cap) = c.capital {
                    state.cities[cap as usize].food += 2 * MILLI;
                }
            }
        }
    }
    for c in grown {
        merit::credit(
            state,
            c,
            Path::Prosperity,
            rules.merit_pop as u64,
            b"growth",
        );
    }
    // City gold earns the steward merit (V5 §7.3).
    for (civ, l) in last.iter().enumerate() {
        if let Some(m) = active_officer(state, rules, civ as CivId, Role::Steward) {
            let amount = l.gold as u64 * MILLI as u64 / rules.merit_gold_div.max(1) as u64;
            merit::credit(state, Credit::officer(m), Path::Prosperity, amount, b"gold");
        }
    }
    for (c, l) in state.civs.iter_mut().zip(last) {
        c.last = l;
    }

    // Research (§6.2).
    for civ in 0..state.civs.len() {
        let cities = state.city_count(civ as u16);
        loop {
            let c = &mut state.civs[civ];
            let Some(&tech) = c.research_queue.first() else {
                break;
            };
            if c.techs.has(tech) || !c.techs.prereqs_met(tech) {
                c.research_queue.remove(0);
                continue;
            }
            let cost = tech_cost(rules, tech, cities) as i64 * MILLI;
            if c.science_store < cost {
                break;
            }
            c.science_store -= cost;
            c.techs.insert(tech);
            c.research_queue.remove(0);
            let credit = c.research_credit;
            let amount = (cost / rules.merit_tech_div.max(1) as i64) as u64;
            merit::credit(state, credit, Path::Science, amount, b"tech");
        }
    }
}

/// Complete at most one queue item (v0.2 C1), then cap the production
/// store at the next item's cost (nothing is banked without an item).
fn complete_queue(state: &mut WorldState, rules: &Ruleset, civ: CivId, i: usize) {
    // Overflow from a completion carries to the next item, up to its cost.
    try_complete(state, rules, civ, i);
    let cap = match state.cities[i].queue.first().copied() {
        Some(next) => item_cost(state, rules, &state.cities[i], &next),
        None => 0,
    };
    let city = &mut state.cities[i];
    city.prod = city.prod.min(cap);
}

fn try_complete(state: &mut WorldState, rules: &Ruleset, civ: CivId, i: usize) -> bool {
    {
        let Some(item) = state.cities[i].queue.first().copied() else {
            return false;
        };
        let cost = item_cost(state, rules, &state.cities[i], &item);
        if state.cities[i].prod < cost {
            return false;
        }
        match item {
            QueueItem::Building(b) => {
                if b.is_star_gate() {
                    // Stages of one civ are at least `star_gate_spacing` ticks apart (v0.2 C2).
                    let last = state.civs[civ as usize].last_star_gate;
                    if last.is_some_and(|t| state.tick < t + rules.star_gate_spacing) {
                        return false;
                    }
                }
                let city = &mut state.cities[i];
                city.buildings.insert(b);
                let credit = city.queue_credit;
                if b.is_star_gate() {
                    let stages = city.buildings.star_gate_stages();
                    let tick = state.tick;
                    let c = &mut state.civs[civ as usize];
                    c.scores.star_gate_stages = c.scores.star_gate_stages.max(stages);
                    c.scores.star_gate_tick = Some(tick);
                    c.last_star_gate = Some(tick);
                    c.achievements.star_gate_max = c.achievements.star_gate_max.max(stages);
                    let mut payload = [0u8; 3];
                    payload[..2].copy_from_slice(&civ.to_le_bytes());
                    payload[2] = stages;
                    state.push_event(b"star_gate", &payload);
                    // Half to the steward who queued it, half to the science officer.
                    let half = rules.merit_star_gate as u64 / 2;
                    merit::credit(
                        state,
                        credit,
                        Path::Science,
                        rules.merit_star_gate as u64 - half,
                        b"star_gate",
                    );
                    if let Some(m) = active_officer(state, rules, civ, Role::Science) {
                        merit::credit(state, Credit::officer(m), Path::Science, half, b"star_gate");
                    }
                } else {
                    let amount = (cost / rules.merit_building_div.max(1) as i64) as u64;
                    merit::credit(state, credit, Path::Prosperity, amount, b"building");
                }
            }
            QueueItem::Troops { unit, n } => {
                let st = stats(unit);
                let (iron, horses) = (
                    milli((st.iron * n as u32) as i64),
                    milli((st.horses * n as u32) as i64),
                );
                let c = &state.civs[civ as usize];
                if c.iron < iron || c.horses < horses {
                    return false; // wait for strategic resources
                }
                if !spawn(state, civ, i, unit, n as u32 * 1000) {
                    return false; // no free tile: delayed (§5.6)
                }
                let c = &mut state.civs[civ as usize];
                c.iron -= iron;
                c.horses -= horses;
            }
            QueueItem::Scout => {
                if !spawn(state, civ, i, UnitType::Scout, 1000) {
                    return false;
                }
            }
            QueueItem::Settler => {
                if state.cities[i].pop < rules.settler_min_pop {
                    return false;
                }
                if !spawn(state, civ, i, UnitType::Settler, 1000) {
                    return false;
                }
                state.cities[i].pop -= 1;
            }
        }
        let city = &mut state.cities[i];
        city.prod -= cost;
        city.queue.remove(0);
        // CityQueueRepeat (default on, §13): repeat the last unit item.
        if city.queue.is_empty()
            && city.standing.repeat_queue
            && !matches!(item, QueueItem::Building(_))
        {
            city.queue.push(item);
        }
        true
    }
}

/// Spawn on the city tile or the first free neighbour (§5.6). False = delayed.
fn spawn(
    state: &mut WorldState,
    civ: CivId,
    city: usize,
    unit_type: UnitType,
    troops: u32,
) -> bool {
    let center = state.cities[city].hex;
    let probe = Unit {
        id: state.units.len() as u32,
        owner: Owner::Civ(civ),
        unit_type,
        troops,
        hex: center,
        path: Vec::new(),
        last_moved: None,
        used_full_mp: false,
        standing: StandingRule::None,
        alive: true,
    };
    let candidates = core::iter::once(center).chain(center.neighbors());
    for hex in candidates {
        let passable = state.map.tile(hex).is_some_and(|t| t.terrain.is_passable());
        if passable && tile_free_for(state, &probe, hex) {
            state.units.push(Unit { hex, ..probe });
            return true;
        }
    }
    false
}
