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
    // Contracts first: what diplomacy and combat did this tick decides them
    // (V5 §18.6).
    crate::contracts::settle_contracts(state, rules);
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
    eurekas(state, &facts);
    dark_age(state, rules);
}

/// Eurekas (revised 2026-09-25): doing something related makes a tech 40%
/// cheaper (`eureka_cost_bps`), with no order spent — so what a nation
/// does (fighting, trading, befriending) shapes what it researches. A fired
/// eureka stays; the Star Gate techs have none.
fn eurekas(state: &mut WorldState, facts: &[crate::scoring::Facts]) {
    use crate::buildings::Building;
    use crate::tech::Tech::*;
    for (civ, f) in facts.iter().enumerate() {
        let c = &state.civs[civ];
        let owns = |b: Building| {
            state
                .living_cities_of(civ as u16)
                .any(|x| x.buildings.has(b))
        };
        let fired = [
            (Agriculture, f.pop >= 3),
            (
                BronzeWorking,
                owns(Building::Workshop) || owns(Building::Barracks),
            ),
            (Archery, c.achievements.kills >= 1),
            (HorsebackRiding, c.horses >= 5_000),
            (IronWorking, c.iron >= 5_000),
            (Masonry, state.city_count(civ as u16) >= 3),
            (Mysticism, f.envoy_sent),
            (Writing, f.partners >= 1),
            (Currency, f.trade >= 100),
            (Mathematics, owns(Building::Market)),
            (Chivalry, f.conquests >= 1),
            (Philosophy, f.ever_suzerain),
            (Engineering, f.captured_held >= 1 || owns(Building::Walls)),
            (Astronomy, f.pop >= 30),
        ];
        let boosted = &mut state.civs[civ].achievements.boosted;
        for (tech, now) in fired {
            if now {
                boosted.insert(tech);
            }
        }
    }
}

/// Dark age (catch-up, revised 2026-09-25): at `dark_age_tick`, a nation
/// with members whose points are below `dark_age_share_bps` of the leader's
/// researches cheaper and orders more until `dark_age_until`.
fn dark_age(state: &mut WorldState, rules: &Ruleset) {
    if state.tick != rules.dark_age_tick {
        return;
    }
    let scores = crate::scoring::nation_scores(state, rules);
    let counted = |c: usize| state.nations[c].members > 0 && state.city_count(c as u16) > 0;
    let lead = (0..scores.len())
        .filter(|c| counted(*c))
        .map(|c| scores[c].total())
        .max()
        .unwrap_or(0);
    let behind: Vec<usize> = (0..scores.len())
        .filter(|c| {
            counted(*c)
                && scores[*c].total() * (crate::fixed::BPS_ONE as u64)
                    < lead * rules.dark_age_share_bps as u64
        })
        .collect();
    for c in behind {
        state.civs[c].achievements.dark_age_until = Some(rules.dark_age_until);
        state.push_event(b"dark_age", &[c as u8]);
    }
}
