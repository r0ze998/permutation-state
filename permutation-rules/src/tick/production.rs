//! Phase 6 (§5.3–§5.6): yields, growth, production queues (one item per
//! tick, C1), Star Gate spacing (C2) and spawning units.

use super::city_orders::item_cost;
use crate::economy::{amenities, apply_amenities, city_yield, growth_threshold};
use crate::fixed::{apply_bps, milli, MILLI};
use crate::gov::{active_officer, Credit, Path, Role};
use crate::map::{territory_radius, TileResource};
use crate::merit;
use crate::movement::tile_free_for;
use crate::params::Ruleset;
use crate::state::{
    CivId, Focus, LastYields, Owner, QueueItem, Specialty, StandingRule, Unit, WorldState,
};
use crate::tech::Tech;
use crate::units::{stats, UnitType};
use alloc::vec::Vec;

/// A Science-focus city's science, per point, in milli units (v0.2 C6:
/// applied before rounding, so it always has an effect). Note: 12_500 milli
/// is ×12.5 a point (`MILLI` = 1000); 12_500 reads like 1.25× in bps. The
/// golden roots pin this value.
const SCIENCE_FOCUS_MILLI: i64 = 12_500;

pub(crate) fn phase_production(state: &mut WorldState, rules: &Ruleset) {
    let mut last = alloc::vec![LastYields::default(); state.civs.len()];
    let mut grown: Vec<Credit> = Vec::new();
    // No city changes hands or falls in phase 6: count each civ's cities once.
    let city_counts: Vec<u32> = (0..state.civs.len() as CivId)
        .map(|c| state.city_count(c))
        .collect();
    for i in 0..state.cities.len() {
        let (owner, alive) = (state.cities[i].owner, state.cities[i].alive);
        let razing = state.cities[i].razing.is_some();
        let (Some(civ), true, false) = (owner, alive, razing) else {
            continue;
        };
        let civ_cities = city_counts[civ as usize];
        city_turn(
            state,
            rules,
            i,
            civ,
            civ_cities,
            &mut last[civ as usize],
            &mut grown,
        );
    }
    crate::probe::probe("cu p6.cities");
    suzerain_bonuses(state, &mut last);
    crate::probe::probe("cu p6.suzerain");
    production_merit(state, rules, grown, &last);
    for (c, l) in state.civs.iter_mut().zip(last) {
        c.last = l;
    }
    crate::probe::probe("cu p6.merit");
    research(state, rules);
}

/// One city's tick: yields, food and growth (§5.3), territory, the civ's
/// share of its yields and strategic resources, then its queue.
fn city_turn(
    state: &mut WorldState,
    rules: &Ruleset,
    i: usize,
    civ: CivId,
    civ_cities: u32,
    last: &mut LastYields,
    grown: &mut Vec<Credit>,
) {
    let c_ref = &state.civs[civ as usize];
    let is_capital = c_ref.capital == Some(i as u32);
    let has_philosophy = c_ref.techs.has(Tech::Philosophy);
    let ww = c_ref.war_weariness;

    let trace = i < 3;
    if trace {
        crate::probe::probe("cu p6.start");
    }
    let (y, worked) = city_yield(&state.map, &state.cities[i], is_capital, has_philosophy);
    if trace {
        crate::probe::probe("cu p6.yield");
    }
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
    if trace {
        crate::probe::probe("cu p6.growth");
    }
    let (hex, pop, id) = (state.cities[i].hex, state.cities[i].pop, i as u32);
    state.map.claim_territory(id, hex, territory_radius(pop));
    if trace {
        crate::probe::probe("cu p6.territory");
    }

    // Civilization-level yields.
    last.gold += y.gold;
    last.max_city_food_surplus = last.max_city_food_surplus.max(surplus.max(0) as u32);
    last.max_city_prod = last.max_city_prod.max(y.prod);
    let science = if state.cities[i].focus == Focus::Science {
        y.science as i64 * SCIENCE_FOCUS_MILLI
    } else {
        y.science as i64 * MILLI
    };
    let c = &mut state.civs[civ as usize];
    c.gold += y.gold as i64 * MILLI;
    c.achievements.wealth += y.gold as u64;
    c.science_store += science;
    c.influence += y.influence as i64 * MILLI;
    c.scores.science_total += (science / MILLI) as u64;
    // Worked strategic resources, one unit per tile until its reserve runs out.
    for t in worked {
        let tile = &mut state.map.tiles[t];
        if tile.reserve == 0 {
            continue;
        }
        match tile.resource {
            Some(TileResource::Iron) => {
                state.civs[civ as usize].iron += MILLI;
                last.iron += 1;
            }
            Some(TileResource::Horses) => {
                state.civs[civ as usize].horses += MILLI;
                last.horses += 1;
            }
            _ => continue,
        }
        tile.reserve -= 1;
    }

    if trace {
        crate::probe::probe("cu p6.civ");
    }
    complete_queue(state, rules, civ, i);
    if trace {
        crate::probe::probe("cu p6.queue");
    }
}

/// City-state suzerain bonuses (§12.1).
fn suzerain_bonuses(state: &mut WorldState, last: &mut [LastYields]) {
    for cs in 0..state.city_states.len() {
        let (Some(civ), None) = (
            state.city_states[cs].suzerain,
            state.city_states[cs].captured_by,
        ) else {
            continue;
        };
        let c = &mut state.civs[civ as usize];
        match state.city_states[cs].specialty {
            Specialty::Scientific => {
                c.science_store += 3 * MILLI;
                c.scores.science_total += 3;
            }
            Specialty::Mercantile => {
                c.gold += 4 * MILLI;
                c.achievements.wealth += 4;
                last[civ as usize].gold += 4;
            }
            Specialty::Agrarian => {
                if let Some(cap) = c.capital {
                    state.cities[cap as usize].food += 2 * MILLI;
                }
            }
        }
    }
}

/// Prosperity merit: growth for the stewards credited with each city that
/// grew, and city gold for each nation's active steward (V5 §7.3).
fn production_merit(
    state: &mut WorldState,
    rules: &Ruleset,
    grown: Vec<Credit>,
    last: &[LastYields],
) {
    for c in grown {
        merit::credit(
            state,
            c,
            Path::Prosperity,
            rules.merit_pop as u64,
            b"growth",
        );
    }
    for (civ, l) in last.iter().enumerate() {
        if let Some(m) = active_officer(state, rules, civ as CivId, Role::Steward) {
            let amount = l.gold as u64 * MILLI as u64 / rules.merit_gold_div.max(1) as u64;
            merit::credit(state, Credit::officer(m), Path::Prosperity, amount, b"gold");
        }
    }
}

/// Research (§6.2): each civ completes queued techs while its science lasts.
fn research(state: &mut WorldState, rules: &Ruleset) {
    for civ in 0..state.civs.len() {
        loop {
            let c = &mut state.civs[civ];
            let Some(&tech) = c.research_queue.first() else {
                break;
            };
            if c.techs.has(tech) || !c.techs.prereqs_met(tech) {
                c.research_queue.remove(0);
                continue;
            }
            let cost = crate::economy::civ_tech_cost(state, rules, civ as u16, tech) as i64 * MILLI;
            let c = &mut state.civs[civ];
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
    // The first free neighbour in §0.3 order turned to the city's sextant
    // (so the choice turns with a symmetric map).
    let candidates = core::iter::once(center).chain(center.neighbors_in(center.sextant()));
    for hex in candidates {
        let passable = state.map.tile(hex).is_some_and(|t| t.terrain.is_passable());
        if passable && tile_free_for(state, &probe, hex) {
            state.units.push(Unit { hex, ..probe });
            return true;
        }
    }
    false
}
