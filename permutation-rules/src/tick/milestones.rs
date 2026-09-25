//! Phase 10 (V5 §6–§7): suzerainty records and envoy merit, office banks,
//! milestone tiers and eras, and per-tick merit.

use crate::gov::{Credit, Path};
use crate::merit;
use crate::movement::allied;
use crate::orders::next_bank;
use crate::params::Ruleset;
use crate::state::WorldState;
use alloc::vec::Vec;

pub(crate) fn phase_scoring(state: &mut WorldState, rules: &Ruleset) {
    let n = state.civs.len();
    // Suzerainty: the milestone record and merit for the envoys (V5 §7.3).
    for cs in 0..state.city_states.len() {
        let (Some(civ), None) = (
            state.city_states[cs].suzerain,
            state.city_states[cs].captured_by,
        ) else {
            continue;
        };
        state.civs[civ as usize].achievements.ever_suzerain = true;
        let shares: Vec<(Credit, u64)> = state.city_states[cs]
            .envoys
            .iter()
            .filter(|e| e.civ == civ)
            .map(|e| (e.credit, e.influence))
            .collect();
        merit::credit_shared(
            state,
            &shares,
            Path::Concord,
            rules.merit_suzerain as u64,
            b"suzerain",
        );
    }
    // Office banks (§4.1 per office, V5 §5.2).
    let spent: Vec<[u16; 4]> = (0..n)
        .map(|c| {
            state
                .tick_orders
                .iter()
                .find(|a| a.civ as usize == c)
                .map_or([0; 4], |a| a.spent)
        })
        .collect();
    for (civ, used) in spent.iter().enumerate() {
        let b = state.civs[civ].tick_budget;
        let in_alliance = (0..n as u16).any(|o| allied(state, civ as u16, o));
        state.civs[civ].ever_allied |= in_alliance;
        let nation = &mut state.nations[civ];
        for (role, u) in used.iter().enumerate() {
            nation.role_bank[role] = next_bank(
                rules,
                nation.role_bank[role],
                rules.role_budget(b, role),
                *u as u32,
            );
        }
    }
    // Milestones and eras, announced when they change (V5 §6.3).
    let facts = crate::scoring::facts_all(state, rules);
    for (civ, f) in facts.iter().enumerate() {
        let score = crate::scoring::score_of(rules, f);
        let a = &state.civs[civ].achievements;
        let (old_tiers, old_era) = (a.tiers, a.era);
        for (p, (new, old)) in score.tiers.iter().zip(old_tiers).enumerate() {
            if *new != old {
                state.push_event(b"milestone", &[civ as u8, p as u8, *new]);
            }
        }
        if score.era != old_era {
            state.push_event(b"era", &[civ as u8, score.era]);
        }
        let a = &mut state.civs[civ].achievements;
        a.tiers = score.tiers;
        a.era = score.era;
    }
}
