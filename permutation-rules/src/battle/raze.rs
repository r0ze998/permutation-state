//! Razing (§8.3.1) and displacing civilians who may no longer share a tile.

use super::*;
use crate::checks::Blocked;
use crate::state::{CivId, Owner, WorldState};

pub(super) fn order_raze(state: &mut WorldState, civ: CivId, city: u32) -> Result<(), Blocked> {
    let tick = state.tick;
    let c = state
        .cities
        .get_mut(city as usize)
        .ok_or(Blocked::UnknownCity)?;
    let recent = c.captured_tick.is_some_and(|t| tick.saturating_sub(t) <= 1);
    if !c.alive || c.owner != Some(civ) {
        return Err(Blocked::NotYours);
    }
    if !recent || c.razing.is_some() {
        return Err(Blocked::OutOfBounds { min: 0, max: 1 });
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
    Ok(())
}

pub(super) fn advance_razing(state: &mut WorldState) {
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
