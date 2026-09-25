//! Captures (§8.3, §12.1): civilians, then cities, then city-states, with
//! grievance, merit and the capital handover.

use super::*;
use crate::buildings::Building;
use crate::gov::{Credit, Path};
use crate::map::territory_radius;
use crate::merit;
use crate::params::Ruleset;
use crate::rng::tie_key;
use crate::state::{City, CivId, Owner, Relation, WorldState};
use crate::units::UnitType;
use alloc::vec::Vec;

pub(super) fn declared_on(state: &WorldState, victim: CivId, captor: CivId) -> bool {
    matches!(state.relation(victim, captor), Relation::War { declared_by, .. } if declared_by == victim)
}

pub(super) fn capture_civilian(state: &mut WorldState, attacker: usize, civ: CivId, target: usize) {
    if !state.units[attacker].alive || !state.units[target].alive {
        return;
    }
    let victim = state.units[target].owner;
    if victim == Owner::Civ(civ) {
        return;
    }
    let t = &mut state.units[target];
    t.owner = Owner::Civ(civ);
    t.path.clear();
    t.standing = crate::state::StandingRule::None;
    if let (Owner::Civ(v), UnitType::Settler) = (victim, t.unit_type) {
        state.add_grievance(civ, v, 5);
    }
    state.push_event(b"capture_civilian", &(target as u32).to_le_bytes());
}

/// Capture merit: `pop × 10`, shared by the surviving attackers' troops (V5 §7.3).
pub(super) fn capture_merit<'a>(
    state: &mut WorldState,
    rules: &Ruleset,
    pop: u32,
    attackers: impl Iterator<Item = &'a Engagement>,
    captor: CivId,
) {
    let shares: Vec<(Credit, u64)> = attackers
        .filter(|e| e.civ == captor && state.units[e.attacker].alive)
        .map(|e| (e.credit, state.units[e.attacker].troops as u64))
        .collect();
    merit::credit_shared(
        state,
        &shares,
        Path::Hegemony,
        pop as u64 * rules.merit_capture_per_pop as u64,
        b"capture",
    );
}

/// Surviving melee/mounted attacker with the most troops; ties by tie-break.
pub(super) fn captor_among<'a>(
    state: &WorldState,
    candidates: impl Iterator<Item = &'a Engagement>,
) -> Option<(usize, CivId)> {
    candidates
        .filter(|e| {
            let u = &state.units[e.attacker];
            u.alive && u.unit_type.can_capture()
        })
        .map(|e| (e.attacker, e.civ))
        .max_by_key(|(a, _)| {
            let u = &state.units[*a];
            (
                u.troops,
                core::cmp::Reverse(tie_key(&state.tick_seed, u.id as u64)),
            )
        })
}

pub(super) fn resolve_city_captures(
    state: &mut WorldState,
    rules: &Ruleset,
    engagements: &[Engagement],
) {
    for ci in 0..state.cities.len() {
        let city = &state.cities[ci];
        if !city.alive || city.defense != 0 || garrison_of(state, city).is_some() {
            continue;
        }
        let Some((army, captor)) = captor_among(
            state,
            engagements.iter().filter(|e| e.city_target == Some(ci)),
        ) else {
            continue;
        };
        if let Some(owner) = city.owner {
            let last_city = state.city_count(owner) == 1;
            let recently_lost = state.civs[owner as usize]
                .last_city_lost
                .is_some_and(|t| state.tick.saturating_sub(t) < rules.last_city_protection_ticks);
            if last_city && recently_lost {
                continue; // last-city protection (§8.3)
            }
        }
        let pop = state.cities[ci].pop;
        capture_merit(
            state,
            rules,
            pop,
            engagements.iter().filter(|e| e.city_target == Some(ci)),
            captor,
        );
        capture_city(state, rules, ci, captor, army);
    }
}

pub(super) fn capture_city(
    state: &mut WorldState,
    rules: &Ruleset,
    ci: usize,
    captor: CivId,
    army: usize,
) {
    let tick = state.tick;
    let prev = state.cities[ci].owner;
    let hex = state.cities[ci].hex;
    let pop_before = state.cities[ci].pop;
    let had_star_gate = state.cities[ci].buildings.star_gate_stages() > 0;
    {
        let c = &mut state.cities[ci];
        c.owner = Some(captor);
        c.standing = crate::state::CityStanding::DEFAULT; // the captor does not inherit automation
        c.captured_from = prev;
        c.captured_tick = Some(tick);
        c.pop = c.pop.saturating_sub(1).max(1);
        for b in [
            Building::Walls,
            Building::Barracks,
            Building::StarGate1,
            Building::StarGate2,
            Building::StarGate3,
        ] {
            c.buildings.remove(b);
        }
        c.loyalty = 50;
        c.food /= 2;
        c.prod /= 2;
        c.queue.clear();
        c.queue_credit = Credit::NONE;
        c.steward_credit = Credit::NONE;
        // Half defence, not zero: an army adjacent to a freshly captured city
        // must fight for it instead of walking in next tick (§8.3).
        let max = (rules.city_defense_base + c.pop) * 1000;
        c.defense = (max as u64 * rules.capture_defense_bps as u64 / 10_000) as u32;
        c.razing = None;
        c.capture_scores = c.founder != captor
            && tick >= c.founded_tick.saturating_add(rules.capture_min_founded_age);
    }
    // A conquest is banked for hegemony tier 3 when the city was taken from
    // another nation (not a Free City), counts as a capture and was a real
    // city (not an outpost founded to be handed over).
    if prev.is_some() && state.cities[ci].capture_scores && pop_before >= rules.conquest_min_pop {
        state.civs[captor as usize].achievements.conquests += 1;
    }

    if let Some(v) = prev {
        state.add_grievance(captor, v, 20);
        if !declared_on(state, v, captor) {
            state.civs[captor as usize].last_aggression = Some(tick);
        }
        let victim = &mut state.civs[v as usize];
        victim.last_city_lost = Some(tick);
        if had_star_gate {
            victim.scores.star_gate_stages = 0;
            victim.scores.star_gate_tick = None;
        }
        if victim.capital == Some(ci as u32) {
            let next = state.living_cities_of(v).map(|c| c.id).min();
            state.civs[v as usize].capital = next;
        }
    }

    // The capturing army moves in; the old owner's civilians there are captured.
    let u = &mut state.units[army];
    u.hex = hex;
    u.path.clear();
    for u in &mut state.units {
        if u.alive
            && u.hex == hex
            && u.unit_type.is_civilian()
            && prev.is_some_and(|p| u.owner == Owner::Civ(p))
        {
            u.owner = Owner::Civ(captor);
            u.path.clear();
            u.standing = crate::state::StandingRule::None;
        }
    }
    // A third civ's civilian there may no longer share the tile with the
    // captor's army (§7.2): it steps out, or is disbanded with nowhere to go.
    displace_civilians(state, hex);
    let mut payload = [0u8; 6];
    payload[..4].copy_from_slice(&(ci as u32).to_le_bytes());
    payload[4..].copy_from_slice(&captor.to_le_bytes());
    state.push_event(b"capture_city", &payload);
}

pub(super) fn resolve_city_state_captures(state: &mut WorldState, engagements: &[Engagement]) {
    for csi in 0..state.city_states.len() {
        let cs = &state.city_states[csi];
        if cs.captured_by.is_some() || cs.defense != 0 {
            continue;
        }
        let Some((army, captor)) = captor_among(
            state,
            engagements
                .iter()
                .filter(|e| e.defender == Defender::CityState(csi)),
        ) else {
            continue;
        };
        let (hex, pop) = (cs.hex, cs.pop);
        let id = state.cities.len() as u32;
        state.cities.push(City {
            id,
            owner: Some(captor),
            founder: captor,
            founded_tick: state.tick,
            hex,
            pop,
            food: 0,
            prod: 0,
            buildings: Default::default(),
            loyalty: 50,
            defense: 0,
            attacked_this_tick: true,
            focus: crate::state::Focus::Balanced,
            queue: Vec::new(),
            queue_credit: Credit::NONE,
            steward_credit: Credit::NONE,
            captured_tick: Some(state.tick),
            captured_from: None,
            capture_scores: false,
            razing: None,
            heritage_until: None,
            heritage_bonus: 0,
            standing: crate::state::CityStanding::DEFAULT,
            alive: true,
        });
        state.map.claim_territory(id, hex, territory_radius(pop));
        let cs = &mut state.city_states[csi];
        cs.captured_by = Some(captor);
        cs.suzerain = None;
        let u = &mut state.units[army];
        u.hex = hex;
        u.path.clear();
        state.push_event(b"capture_city_state", &(csi as u16).to_le_bytes());
    }
}
