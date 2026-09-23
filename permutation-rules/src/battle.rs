//! Phase 5: combat, captures and razing (§8, §9.1–9.2, §12.1, §15 phase 5).
//!
//! Order of operations within the phase:
//! 1. advance cities already being razed;
//! 2. plan every valid `Attack` against the **pre-combat** state;
//! 3. compute all damage from pre-combat counts, then apply it at once;
//! 4. capture civilians, then cities, then city-states (ascending id);
//! 5. apply `Raze` orders;
//! 6. record aggression for attacks on neutrals.
//!
//! v0.1 clarifications (documented in the spec, §8.6):
//! - several attackers on one defender each fight a full engagement against the
//!   defender's pre-combat count; damage to the defender is summed;
//! - an army with an `Attack` order holds its position this tick (phase 4);
//! - targets inside another civilization's active protected zone cannot be attacked;
//! - attacking a Free City counts as attacking a neutral (like a city-state);
//! - a captured capital passes to the victim's lowest-id remaining city.

use crate::buildings::Building;
use crate::combat::{resolve_engagement, variance, Combatant, Situation};
use crate::map::territory_radius;
use crate::orders::{AttackTarget, Order};
use crate::params::Ruleset;
use crate::rng::tie_key;
use crate::state::{City, CivId, Owner, Relation, WorldState};
use crate::tech::Tech;
use crate::tick::{accepted, unit_civ, TickInput};
use crate::units::{stats, UnitClass, UnitType};
use alloc::vec;
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Defender {
    Unit(usize),
    City(usize),
    CityState(usize),
}

#[derive(Clone, Copy, Debug)]
struct Engagement {
    attacker: usize,
    civ: CivId,
    defender: Defender,
    /// City whose defence/capture this attack contributes to (garrison or walls).
    city_target: Option<usize>,
    /// Attacking a neutral (city-state or Free City) is aggression (§9.2).
    neutral: bool,
    sit: Situation,
}

enum Plan {
    Engage(Engagement),
    /// A melee army steps onto a lone enemy civilian (§7.2).
    CaptureCivilian {
        attacker: usize,
        civ: CivId,
        target: usize,
    },
}

pub fn phase_combat(state: &mut WorldState, rules: &Ruleset, input: &TickInput) {
    advance_razing(state);

    let orders = accepted(state, rules, input);
    let mut attacks: Vec<(CivId, u32, AttackTarget)> = Vec::new();
    let mut razes: Vec<(CivId, u32)> = Vec::new();
    for (civ, _, list) in &orders {
        for order in list {
            match *order {
                Order::Attack { army, target } => attacks.push((*civ, army, target)),
                Order::Raze { city } => razes.push((*civ, city)),
                _ => {}
            }
        }
    }
    // Engagement ids are positions in this order: (civ, army) ascending.
    attacks.sort_by_key(|(civ, army, _)| (*civ, *army));

    let mut engagements = Vec::new();
    let mut civilian_captures = Vec::new();
    for (civ, army, target) in attacks {
        match plan(state, rules, civ, army, target) {
            Some(Plan::Engage(e)) => engagements.push(e),
            Some(Plan::CaptureCivilian {
                attacker,
                civ,
                target,
            }) => civilian_captures.push((attacker, civ, target)),
            None => {} // invalid at resolution: dropped without refund (§4.1)
        }
    }

    apply_damage(state, rules, &engagements);
    for (attacker, civ, target) in civilian_captures {
        capture_civilian(state, attacker, civ, target);
    }
    resolve_city_captures(state, rules, &engagements);
    resolve_city_state_captures(state, &engagements);
    for (civ, city) in razes {
        order_raze(state, civ, city);
    }
    record_neutral_aggression(state, &engagements);
}

// ------------------------------------------------------------------ planning

fn is_protected(
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

fn hostile(state: &WorldState, civ: CivId, owner: Owner) -> bool {
    match owner {
        Owner::Barbarian => true,
        Owner::Civ(o) => o != civ && state.at_war(civ, o),
    }
}

fn garrison_of(state: &WorldState, city: &City) -> Option<usize> {
    let owner = city.owner?;
    state.units.iter().position(|u| {
        u.alive && u.hex == city.hex && u.owner == Owner::Civ(owner) && !u.unit_type.is_civilian()
    })
}

fn city_at(state: &WorldState, hex: crate::hex::Hex) -> Option<usize> {
    state.cities.iter().position(|c| c.alive && c.hex == hex)
}

fn plan(
    state: &WorldState,
    rules: &Ruleset,
    civ: CivId,
    army: u32,
    target: AttackTarget,
) -> Option<Plan> {
    let a_idx = army as usize;
    let a = state
        .units
        .get(a_idx)
        .filter(|u| u.alive && u.owner == Owner::Civ(civ) && !u.unit_type.is_civilian())?;
    let a_stats = stats(a.unit_type);

    // Resolve the target to (hex, defender, city_target, neutral).
    let (hex, defender, city_target, neutral) = match target {
        AttackTarget::Unit(id) => {
            let t = state.units.get(id as usize).filter(|t| t.alive)?;
            if !hostile(state, civ, t.owner) {
                return None;
            }
            match city_at(state, t.hex).filter(|&c| state.cities[c].owner == unit_civ(t)) {
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
                    if !(lone && melee && a.hex.distance(t.hex) == 1) {
                        return None;
                    }
                    if is_protected(state, rules, civ, t.hex) {
                        return None;
                    }
                    return Some(Plan::CaptureCivilian {
                        attacker: a_idx,
                        civ,
                        target: id as usize,
                    });
                }
                None => (t.hex, Defender::Unit(id as usize), None, false),
            }
        }
        AttackTarget::City(id) => {
            let c = state.cities.get(id as usize).filter(|c| c.alive)?;
            let neutral = match c.owner {
                Some(o) if o == civ || !state.at_war(civ, o) => return None,
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
                .filter(|cs| cs.captured_by.is_none())?;
            (cs.hex, Defender::CityState(id as usize), None, true)
        }
    };

    let d = a.hex.distance(hex);
    let in_range = match a_stats.class {
        UnitClass::Ranged => (1..=a_stats.range as u32).contains(&d),
        _ => d == 1,
    };
    if !in_range || is_protected(state, rules, civ, hex) {
        return None;
    }

    let tile = |h| state.map.tile(h);
    let sit = Situation {
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
    Some(Plan::Engage(Engagement {
        attacker: a_idx,
        civ,
        defender,
        city_target,
        neutral,
        sit,
    }))
}

// ------------------------------------------------------------------ damage

fn combatant(state: &WorldState, d: Defender) -> Combatant {
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

fn apply_damage(state: &mut WorldState, rules: &Ruleset, engagements: &[Engagement]) {
    let mut to_unit = vec![0u64; state.units.len()];
    let mut to_city = vec![0u64; state.cities.len()];
    let mut to_cs = vec![0u64; state.city_states.len()];

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
            Defender::Unit(u) => to_unit[u] += to_def as u64,
            Defender::City(c) => to_city[c] += to_def as u64,
            Defender::CityState(c) => to_cs[c] += to_def as u64,
        }
    }

    // Apply at once.
    for (i, dmg) in to_unit.into_iter().enumerate() {
        if dmg == 0 {
            continue;
        }
        let u = &mut state.units[i];
        let before = u.troops;
        u.troops = before.saturating_sub(dmg.min(u32::MAX as u64) as u32);
        if u.troops < rules.army_min_milli {
            u.troops = 0;
            u.alive = false;
            u.path.clear();
        }
        let lost = before - u.troops;
        if let Owner::Civ(c) = u.owner {
            state.civs[c as usize].troops_lost += lost;
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

// ------------------------------------------------------------------ captures

fn declared_on(state: &WorldState, victim: CivId, captor: CivId) -> bool {
    matches!(state.relation(victim, captor), Relation::War { declared_by, .. } if declared_by == victim)
}

fn capture_civilian(state: &mut WorldState, attacker: usize, civ: CivId, target: usize) {
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
    if let (Owner::Civ(v), UnitType::Settler) = (victim, t.unit_type) {
        state.add_grievance(civ, v, 5);
    }
    state.push_event(b"capture_civilian", &(target as u32).to_le_bytes());
}

/// Surviving melee/mounted attacker with the most troops; ties by tie-break.
fn captor_among<'a>(
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

fn resolve_city_captures(state: &mut WorldState, rules: &Ruleset, engagements: &[Engagement]) {
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
        capture_city(state, rules, ci, captor, army);
    }
}

fn capture_city(state: &mut WorldState, rules: &Ruleset, ci: usize, captor: CivId, army: usize) {
    let tick = state.tick;
    let prev = state.cities[ci].owner;
    let hex = state.cities[ci].hex;
    let had_star_gate = state.cities[ci].buildings.star_gate_stages() > 0;
    {
        let c = &mut state.cities[ci];
        c.owner = Some(captor);
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
        c.defense = 0;
        c.razing = None;
        c.capture_scores = c.founder != captor
            && tick >= c.founded_tick.saturating_add(rules.capture_min_founded_age);
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
        }
    }
    let mut payload = [0u8; 6];
    payload[..4].copy_from_slice(&(ci as u32).to_le_bytes());
    payload[4..].copy_from_slice(&captor.to_le_bytes());
    state.push_event(b"capture_city", &payload);
}

fn resolve_city_state_captures(state: &mut WorldState, engagements: &[Engagement]) {
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
            captured_tick: Some(state.tick),
            captured_from: None,
            scored_by: Vec::new(),
            capture_scores: false,
            razing: None,
            heritage_until: None,
            heritage_bonus: 0,
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

// ------------------------------------------------------------------ razing

fn order_raze(state: &mut WorldState, civ: CivId, city: u32) {
    let tick = state.tick;
    let Some(c) = state.cities.get_mut(city as usize) else {
        return;
    };
    let recent = c.captured_tick.is_some_and(|t| tick.saturating_sub(t) <= 1);
    if !c.alive || c.owner != Some(civ) || !recent || c.razing.is_some() {
        return;
    }
    c.razing = Some(3);
    let from = c.captured_from;
    if let Some(v) = from {
        state.add_grievance(civ, v, 60);
        if !declared_on(state, v, civ) {
            state.civs[civ as usize].last_aggression = Some(tick);
        }
    }
    state.push_event(b"raze", &city.to_le_bytes());
}

fn advance_razing(state: &mut WorldState) {
    for ci in 0..state.cities.len() {
        let Some(left) = state.cities[ci].razing else {
            continue;
        };
        if !state.cities[ci].alive {
            continue;
        }
        if left > 1 {
            state.cities[ci].razing = Some(left - 1);
            continue;
        }
        let (hex, pop, owner) = (
            state.cities[ci].hex,
            state.cities[ci].pop,
            state.cities[ci].owner,
        );
        let c = &mut state.cities[ci];
        c.alive = false;
        c.razing = None;
        for t in &mut state.map.tiles {
            if t.owner_city == Some(ci as u32) {
                t.owner_city = None;
            }
        }
        if let Some(t) = state.map.tile_mut(hex) {
            t.ruin_peak_pop = Some(pop.min(u16::MAX as u32) as u16);
        }
        displace_civilians(state, hex);
        if let Some(o) = owner {
            let civ = &mut state.civs[o as usize];
            if civ.capital == Some(ci as u32) {
                let next = state.living_cities_of(o).map(|c| c.id).min();
                state.civs[o as usize].capital = next;
            }
        }
        state.push_event(b"ruin", &(ci as u32).to_le_bytes());
    }
}

/// When a tile stops being an own city (ruin, Free City), an army and a
/// civilian may no longer share it (§7.2). Each civilian steps to the first
/// free passable neighbour in §0.3 order; with none free it is disbanded.
pub(crate) fn displace_civilians(state: &mut WorldState, hex: crate::hex::Hex) {
    let has_army = state
        .units
        .iter()
        .any(|u| u.alive && u.hex == hex && !u.unit_type.is_civilian());
    if !has_army {
        return;
    }
    for i in 0..state.units.len() {
        let u = &state.units[i];
        if !(u.alive && u.hex == hex && u.unit_type.is_civilian()) {
            continue;
        }
        let owner = u.owner;
        let own_city = state
            .cities
            .iter()
            .any(|c| c.alive && c.hex == hex && owner == Owner::Civ(c.owner.unwrap_or(u16::MAX)));
        if own_city {
            continue;
        }
        let target = hex.neighbors().into_iter().find(|n| {
            state.map.tile(*n).is_some_and(|t| t.terrain.is_passable())
                && !state.units.iter().any(|o| o.alive && o.hex == *n)
                && !state.cities.iter().any(|c| c.alive && c.hex == *n)
        });
        let u = &mut state.units[i];
        u.path.clear();
        match target {
            Some(n) => u.hex = n,
            None => u.alive = false,
        }
    }
}

// ------------------------------------------------------------------ neutrals

fn record_neutral_aggression(state: &mut WorldState, engagements: &[Engagement]) {
    let tick = state.tick;
    let mut hit_city_state = vec![false; state.civs.len()];
    for e in engagements.iter().filter(|e| e.neutral) {
        let civ = &mut state.civs[e.civ as usize];
        civ.last_aggression = Some(tick);
        civ.protection_lost = true;
        if matches!(e.defender, Defender::CityState(_)) {
            hit_city_state[e.civ as usize] = true;
        }
    }
    // Attacking a city-state removes the attacker's influence with every city-state (§12.1).
    for (civ, hit) in hit_city_state.into_iter().enumerate() {
        if hit {
            for cs in &mut state.city_states {
                cs.influence[civ] = 0;
                if cs.suzerain == Some(civ as u16) {
                    cs.suzerain = None;
                }
            }
        }
    }
}
