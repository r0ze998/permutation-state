//! Phase 11 (V5 §5.3–§5.5): recalls, idle recalls and elections, taking
//! effect from the next tick.

use super::*;
use crate::params::Ruleset;
use crate::state::{CivId, WorldState};
use alloc::vec::Vec;

/// Phase 11 of tick `t`: resolve recalls, open automatic recalls, hold the
/// election for a term starting at `t + 1`, and expire proposals. Changes
/// take effect from tick `t + 1`.
pub fn end_of_tick(state: &mut WorldState, rules: &Ruleset) {
    let t = state.tick;
    for civ in 0..state.nations.len() {
        resolve_recalls(state, rules, civ as CivId);
        open_idle_recalls(state, rules, civ as CivId);
    }
    if (t + 1) % rules.term_ticks.max(1) == 0 && t + 1 < rules.ticks_per_season {
        run_election(state, rules, t + 1);
    }
    for n in &mut state.nations {
        n.proposals
            .retain(|p| !p.adopted && t + 1 - p.tick < rules.proposal_ttl_ticks);
    }
}

/// Members of `civ` active in the last `recall_electorate_ticks`: who may
/// carry a recall (V5 §5.5).
pub fn electorate(state: &WorldState, rules: &Ruleset, civ: CivId) -> usize {
    let t = state.tick;
    state
        .members
        .iter()
        .filter(|m| {
            m.civ == civ
                && m.last_active != NEVER
                && t - m.last_active < rules.recall_electorate_ticks
        })
        .count()
}

pub(super) fn resolve_recalls(state: &mut WorldState, rules: &Ruleset, civ: CivId) {
    let t = state.tick;
    let voters = electorate(state, rules, civ);
    let recalls = core::mem::take(&mut state.nations[civ as usize].recalls);
    let mut keep = Vec::new();
    for r in recalls {
        let n = &state.nations[civ as usize];
        if n.offices[r.role.index()] != r.holder {
            continue; // the office changed hands: the recall lapses
        }
        if r.yes.len() * 2 > voters {
            vacate(state, rules, civ, r.role);
            let mut payload = [0u8; 7];
            payload[..2].copy_from_slice(&civ.to_le_bytes());
            payload[2] = r.role as u8;
            payload[3..].copy_from_slice(&r.holder.to_le_bytes());
            state.push_event(b"recalled", &payload);
            continue;
        }
        if t + 1 - r.opened < rules.recall_ticks {
            keep.push(r);
        }
    }
    state.nations[civ as usize].recalls = keep;
}

/// Remove the holder; the last election's runner-up succeeds, else the office is vacant (the caretaker runs it).
pub(super) fn vacate(state: &mut WorldState, rules: &Ruleset, civ: CivId, role: Role) {
    let next = state.tick + 1;
    let n = &mut state.nations[civ as usize];
    let i = role.index();
    let old = n.offices[i];
    let r = n.runner_up[i];
    n.offices[i] = NOBODY;
    let successor =
        if r != NOBODY && r != old && n.offices_held(r) < rules.max_offices_per_member as usize {
            r
        } else {
            NOBODY
        };
    n.offices[i] = successor;
    n.runner_up[i] = NOBODY;
    n.office_since[i] = next;
    n.office_last_act[i] = NEVER;
    n.office_seen[i] = NEVER;
}

pub(super) fn open_idle_recalls(state: &mut WorldState, rules: &Ruleset, civ: CivId) {
    let t = state.tick;
    let n = &mut state.nations[civ as usize];
    for role in Role::ALL {
        let i = role.index();
        let holder = n.offices[i];
        if holder == NOBODY || n.recalls.iter().any(|r| r.role == role) {
            continue;
        }
        let since = match n.office_seen[i] {
            NEVER => n.office_since[i],
            last => last.max(n.office_since[i]),
        };
        if t + 1 >= since && t + 1 - since >= rules.idle_recall_ticks {
            n.recalls.push(Recall {
                role,
                holder,
                opened: t,
                automatic: true,
                yes: Vec::new(),
            });
        }
    }
}

/// Elect every office of every nation for the term starting at `start`
/// (V5 §5.3). Offices are filled in `Role::ALL` order; a member already
/// holding `max_offices_per_member` offices is passed over. Most votes wins;
/// ties go to the on-chain random tie-break. A candidate with no votes can
/// still win an uncontested office.
pub fn run_election(state: &mut WorldState, rules: &Ruleset, start: u16) {
    let seed = crate::hash::sha256(&[
        b"PS/election",
        &state.season_seed,
        &state.tick_seed,
        &start.to_le_bytes(),
    ]);
    for civ in 0..state.nations.len() {
        let civ_id = civ as CivId;
        let mut new_offices = [NOBODY; 4];
        let mut runners = [NOBODY; 4];
        for role in Role::ALL {
            let mut ranked: Vec<(u32, u64, MemberId)> = state
                .members
                .iter()
                .enumerate()
                .filter(|(_, m)| m.civ == civ_id && m.standing_for & role.bit() != 0)
                .map(|(id, _)| {
                    let id = id as MemberId;
                    let votes = state.nations[civ]
                        .votes
                        .iter()
                        .filter(|v| v.role == role && v.candidate == id)
                        .count() as u32;
                    let key = ((civ as u64) << 40) | ((role as u64) << 32) | id as u64;
                    (votes, crate::rng::tie_key(&seed, key), id)
                })
                .collect();
            ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            let eligible: Vec<MemberId> = ranked
                .into_iter()
                .map(|(_, _, id)| id)
                .filter(|id| {
                    new_offices.iter().filter(|o| *o == id).count()
                        < rules.max_offices_per_member as usize
                })
                .collect();
            new_offices[role.index()] = eligible.first().copied().unwrap_or(NOBODY);
            runners[role.index()] = eligible.get(1).copied().unwrap_or(NOBODY);
        }
        let n = &mut state.nations[civ];
        // A re-elected holder keeps its tenure (and its idle clock, and any
        // open recall); a new holder starts fresh. Recalls of replaced
        // holders lapse on their own.
        for (i, new) in new_offices.iter().enumerate() {
            if n.offices[i] != *new {
                n.office_last_act[i] = NEVER;
                n.office_seen[i] = NEVER;
                n.office_since[i] = start;
            }
        }
        n.offices = new_offices;
        n.runner_up = runners;
        n.votes.clear();
        for (i, m) in new_offices.iter().enumerate() {
            let mut payload = [0u8; 9];
            payload[..2].copy_from_slice(&civ_id.to_le_bytes());
            payload[2] = i as u8;
            payload[3..7].copy_from_slice(&m.to_le_bytes());
            payload[7..].copy_from_slice(&start.to_le_bytes());
            state.push_event(b"elected", &payload);
        }
    }
}
