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

use crate::combat::Situation;
use crate::gov::{active_officer, Credit, Path, Role};
use crate::merit;
use crate::orders::{AttackTarget, Order};
use crate::params::Ruleset;
use crate::state::{CivId, WorldState};
use crate::tick::accepted;
use alloc::vec;
use alloc::vec::Vec;

mod capture;
mod damage;
mod plan;
mod raze;

use capture::*;
use damage::*;
use plan::*;
pub use plan::{forecast_attack, AttackForecast};
pub(crate) use plan::{hostile, is_protected};
pub(crate) use raze::displace_civilians;
use raze::*;

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
    /// Who ordered the attack (merit, V5 §7.3).
    credit: Credit,
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

type Origin = Option<(u8, u16)>;

pub fn phase_combat(state: &mut WorldState, rules: &Ruleset) {
    advance_razing(state);

    let orders = accepted(state);
    let mut attacks: Vec<(CivId, u32, AttackTarget, Credit, Origin)> = Vec::new();
    let mut razes: Vec<(CivId, u32, (u8, u16))> = Vec::new();
    for a in &orders {
        for (order, credit, origin) in a.iter() {
            match *order {
                Order::Attack { army, target } => {
                    attacks.push((a.civ, army, target, credit, Some(origin)))
                }
                Order::Raze { city } => razes.push((a.civ, city, origin)),
                _ => {}
            }
        }
    }
    // Attacks compiled from standing rules (§13) join the manual ones; their
    // merit goes to the general in office, if active.
    for (civ, order) in &state.implicit {
        if let Order::Attack { army, target } = *order {
            let credit = active_officer(state, rules, *civ, Role::General)
                .map_or(Credit::NONE, Credit::officer);
            attacks.push((*civ, army, target, credit, None));
        }
    }
    // Engagement ids are positions in this order: (civ, army) ascending.
    attacks.sort_by_key(|(civ, army, ..)| (*civ, *army));

    let owners_before: Vec<Option<CivId>> = state.cities.iter().map(|c| c.owner).collect();
    let mut engagements = Vec::new();
    let mut civilian_captures = Vec::new();
    for (civ, army, target, credit, origin) in attacks {
        match plan(state, rules, civ, army, target) {
            Ok(Plan::Engage(mut e)) => {
                e.credit = credit;
                engagements.push(e)
            }
            Ok(Plan::CaptureCivilian {
                attacker,
                civ,
                target,
            }) => civilian_captures.push((attacker, civ, target)),
            // Invalid at resolution: dropped without refund (§4.1).
            Err(why) => {
                if let Some(g) = origin {
                    state.skip(civ, g, why.code());
                }
            }
        }
    }

    apply_damage(state, rules, &engagements);
    for (attacker, civ, target) in civilian_captures {
        capture_civilian(state, attacker, civ, target);
    }
    resolve_city_captures(state, rules, &engagements);
    resolve_city_state_captures(state, &engagements);
    for (civ, city, origin) in razes {
        if let Err(why) = order_raze(state, civ, city) {
            state.skip(civ, origin, why.code());
        }
    }
    record_neutral_aggression(state, &engagements);
    held_city_merit(state, rules, &owners_before);
}

/// A city that was attacked and is still its owner's earns the owner's
/// active general 5 merit (V5 §7.3).
fn held_city_merit(state: &mut WorldState, rules: &Ruleset, owners_before: &[Option<CivId>]) {
    for (ci, before) in owners_before.iter().enumerate() {
        let c = &state.cities[ci];
        let (true, Some(owner)) = (c.alive && c.attacked_this_tick, c.owner) else {
            continue;
        };
        if *before != Some(owner) {
            continue;
        }
        if let Some(m) = active_officer(state, rules, owner, Role::General) {
            merit::credit(
                state,
                Credit::officer(m),
                Path::Hegemony,
                rules.merit_city_held as u64,
                b"held",
            );
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
                cs.envoys.retain(|e| e.civ as usize != civ);
                if cs.suzerain == Some(civ as u16) {
                    cs.suzerain = None;
                }
            }
        }
    }
}
