//! Diplomacy (§10) and city-state envoys (§12.1).
//!
//! Phase 1 applies scheduled transitions first (peace taking effect, NAP
//! expiry, alliance departures), expires stale proposals, then processes this
//! tick's diplomatic orders in civ order. An `Accept*` only matches a
//! proposal made on an **earlier** tick, so proposing and accepting can never
//! race inside one tick (v0.1 §10.6).
//!
//! Transfers (§10.5) and envoys (§12.1) run in phase 2 via `apply_transfers`
//! and `apply_envoys`.

use crate::fixed::MILLI;
use crate::orders::{Good, Order};
use crate::params::Ruleset;
use crate::rng::tie_key;
use crate::state::{CivId, Owner, Proposal, ProposalKind, Relation, WorldState};
use crate::tick::{accepted, allied, TickInput};
use alloc::vec::Vec;

pub fn phase_diplomacy(state: &mut WorldState, rules: &Ruleset, input: &TickInput) {
    scheduled_transitions(state);
    let now = state.tick;
    state
        .proposals
        .retain(|p| now.saturating_sub(p.tick) < rules.proposal_ttl);

    for (civ, _, orders) in accepted(state, rules, input) {
        for order in orders {
            match order {
                Order::DeclareWar { civ: t } => {
                    if valid_pair(state, civ, t)
                        && matches!(state.relation(civ, t), Relation::Peace)
                    {
                        start_war(state, rules, civ, t);
                    }
                }
                Order::ProposePeace { civ: t } => {
                    if valid_pair(state, civ, t)
                        && matches!(state.relation(civ, t), Relation::War { peace_at: None, .. })
                    {
                        propose(state, ProposalKind::Peace, civ, t);
                    }
                }
                Order::AcceptPeace { civ: t } => accept_peace(state, civ, t),
                Order::ProposeNap { civ: t, bond } => {
                    if valid_pair(state, civ, t)
                        && bond >= rules.nap_min_bond
                        && matches!(state.relation(civ, t), Relation::Peace)
                    {
                        propose(state, ProposalKind::Nap { bond }, civ, t);
                    }
                }
                Order::AcceptNap { civ: t, bond } => accept_nap(state, rules, civ, t, bond),
                Order::BreakNap { civ: t } => break_nap(state, civ, t),
                Order::ProposeAlliance { civ: t } => {
                    if valid_pair(state, civ, t)
                        && matches!(
                            state.relation(civ, t),
                            Relation::Peace | Relation::Nap { .. }
                        )
                    {
                        propose(state, ProposalKind::Alliance, civ, t);
                    }
                }
                Order::AcceptAlliance { civ: t } => accept_alliance(state, rules, civ, t),
                Order::LeaveAlliance => leave_alliance(state, rules, civ),
                _ => {}
            }
        }
    }
}

fn valid_pair(state: &WorldState, a: CivId, b: CivId) -> bool {
    a != b && (b as usize) < state.civs.len()
}

fn propose(state: &mut WorldState, kind: ProposalKind, from: CivId, to: CivId) {
    let same = |p: &Proposal| {
        p.from == from
            && p.to == to
            && core::mem::discriminant(&p.kind) == core::mem::discriminant(&kind)
    };
    state.proposals.retain(|p| !same(p));
    state.proposals.push(Proposal {
        kind,
        from,
        to,
        tick: state.tick,
    });
}

/// Remove and return an earlier-tick proposal of this kind from `from` to `to`.
fn take_proposal(
    state: &mut WorldState,
    from: CivId,
    to: CivId,
    peace: bool,
    nap: bool,
) -> Option<Proposal> {
    let now = state.tick;
    let pos = state.proposals.iter().position(|p| {
        p.from == from
            && p.to == to
            && p.tick < now
            && match p.kind {
                ProposalKind::Peace => peace,
                ProposalKind::Nap { .. } => nap,
                ProposalKind::Alliance => !peace && !nap,
            }
    })?;
    Some(state.proposals.remove(pos))
}

fn event(state: &mut WorldState, kind: &[u8], a: CivId, b: CivId) {
    let mut payload = [0u8; 4];
    payload[..2].copy_from_slice(&a.to_le_bytes());
    payload[2..].copy_from_slice(&b.to_le_bytes());
    state.push_event(kind, &payload);
}

// ------------------------------------------------------------------ war

/// Start a war declared by `civ` (§10.2). Casus belli is judged against the
/// target's grievances (§9.1). Used by `DeclareWar` and `BreakNap`.
pub(crate) fn start_war(state: &mut WorldState, rules: &Ruleset, civ: CivId, target: CivId) {
    let casus_belli = state.grievance(target, civ) >= rules.casus_belli_threshold;
    state.set_relation(
        civ,
        target,
        Relation::War {
            declared_by: civ,
            casus_belli,
            active_from: state.tick + 1,
            peace_at: None,
        },
    );
    if !casus_belli {
        state.add_grievance(civ, target, 30);
        state.civs[civ as usize].last_aggression = Some(state.tick);
    }
    state.civs[civ as usize].protection_lost = true;
    event(state, b"declare_war", civ, target);
}

fn accept_peace(state: &mut WorldState, civ: CivId, from: CivId) {
    if !valid_pair(state, civ, from) {
        return;
    }
    let Relation::War {
        declared_by,
        casus_belli,
        active_from,
        peace_at: None,
    } = state.relation(civ, from)
    else {
        return;
    };
    if take_proposal(state, from, civ, true, false).is_none() {
        return;
    }
    // Peace takes effect at the start of the next tick (§10.2).
    let peace_at = Some(state.tick + 1);
    state.set_relation(
        civ,
        from,
        Relation::War {
            declared_by,
            casus_belli,
            active_from,
            peace_at,
        },
    );
    event(state, b"peace", from, civ);
}

// ------------------------------------------------------------------ NAP

fn accept_nap(state: &mut WorldState, rules: &Ruleset, civ: CivId, from: CivId, bond: u32) {
    if !valid_pair(state, civ, from)
        || bond < rules.nap_min_bond
        || !matches!(state.relation(civ, from), Relation::Peace)
    {
        return;
    }
    let Some(Proposal {
        kind: ProposalKind::Nap { bond: their_bond },
        ..
    }) = take_proposal(state, from, civ, false, true)
    else {
        return;
    };
    // Both bonds are escrowed now; if either side cannot pay, the pact fails (§10.3).
    let (mine, theirs) = (bond as i64 * MILLI, their_bond as i64 * MILLI);
    if state.civs[civ as usize].gold < mine || state.civs[from as usize].gold < theirs {
        return;
    }
    state.civs[civ as usize].gold -= mine;
    state.civs[from as usize].gold -= theirs;
    let (bond_low, bond_high) = if civ < from {
        (bond, their_bond)
    } else {
        (their_bond, bond)
    };
    state.set_relation(
        civ,
        from,
        Relation::Nap {
            until: state.tick + rules.nap_ticks,
            bond_low,
            bond_high,
        },
    );
    event(state, b"nap", from, civ);
}

fn bonds(state: &WorldState, a: CivId, b: CivId) -> Option<(u32, u32)> {
    match state.relation(a, b) {
        // (bond of a, bond of b)
        Relation::Nap {
            bond_low,
            bond_high,
            ..
        } => Some(if a < b {
            (bond_low, bond_high)
        } else {
            (bond_high, bond_low)
        }),
        _ => None,
    }
}

fn return_bonds(state: &mut WorldState, a: CivId, b: CivId) {
    if let Some((ba, bb)) = bonds(state, a, b) {
        state.civs[a as usize].gold += ba as i64 * MILLI;
        state.civs[b as usize].gold += bb as i64 * MILLI;
    }
}

fn break_nap(state: &mut WorldState, civ: CivId, other: CivId) {
    if !valid_pair(state, civ, other) {
        return;
    }
    let Some((mine, theirs)) = bonds(state, civ, other) else {
        return;
    };
    // The breaker's bond goes to the other party, whose own bond is returned (§10.3).
    state.civs[other as usize].gold += (mine as i64 + theirs as i64) * MILLI;
    state.add_grievance(civ, other, 40);
    state.civs[civ as usize].last_aggression = Some(state.tick);
    state.civs[civ as usize].protection_lost = true;
    state.set_relation(
        civ,
        other,
        Relation::War {
            declared_by: civ,
            casus_belli: false,
            active_from: state.tick + 1,
            peace_at: None,
        },
    );
    event(state, b"break_nap", civ, other);
}

// ------------------------------------------------------------------ alliances

/// `{civ} ∪ current allies` (alliances are closed groups, §10.4).
pub fn alliance_group(state: &WorldState, civ: CivId) -> Vec<CivId> {
    let mut g: Vec<CivId> = (0..state.civs.len() as u16)
        .filter(|o| allied(state, civ, *o))
        .collect();
    g.push(civ);
    g.sort();
    g
}

fn accept_alliance(state: &mut WorldState, rules: &Ruleset, civ: CivId, from: CivId) {
    if !valid_pair(state, civ, from) {
        return;
    }
    let joiner_allies = alliance_group(state, civ);
    let group = alliance_group(state, from);
    let ok = joiner_allies.len() == 1
        && group.len() < rules.alliance_cap(state.civs.len())
        && group
            .iter()
            .all(|m| matches!(state.relation(civ, *m), Relation::Peace | Relation::Nap { .. }))
        // A group with a pending departure cannot take new members.
        && !(0..state.civs.len() as u16)
            .any(|o| o != from && matches!(state.relation(from, o), Relation::Alliance { leaving_at: Some(_) }));
    if !ok || take_proposal(state, from, civ, false, false).is_none() {
        return;
    }
    for m in group {
        return_bonds(state, civ, m); // an alliance supersedes a NAP
        state.set_relation(civ, m, Relation::Alliance { leaving_at: None });
        state.civs[m as usize].ever_allied = true;
    }
    state.civs[civ as usize].ever_allied = true;
    event(state, b"alliance", from, civ);
}

fn leave_alliance(state: &mut WorldState, rules: &Ruleset, civ: CivId) {
    let at = state.tick + rules.alliance_leave_delay;
    for o in 0..state.civs.len() as u16 {
        if o != civ
            && matches!(
                state.relation(civ, o),
                Relation::Alliance { leaving_at: None }
            )
        {
            state.set_relation(
                civ,
                o,
                Relation::Alliance {
                    leaving_at: Some(at),
                },
            );
        }
    }
}

// ------------------------------------------------------------------ transitions

fn scheduled_transitions(state: &mut WorldState) {
    let n = state.civs.len() as u16;
    let now = state.tick;
    for a in 0..n {
        for b in a + 1..n {
            match state.relation(a, b) {
                Relation::War {
                    peace_at: Some(p), ..
                } if now >= p => {
                    state.set_relation(a, b, Relation::Peace);
                    withdraw(state, a, b);
                    withdraw(state, b, a);
                }
                Relation::Nap { until, .. } if now >= until => {
                    return_bonds(state, a, b);
                    state.set_relation(a, b, Relation::Peace);
                }
                Relation::Alliance {
                    leaving_at: Some(t),
                } if now >= t => {
                    state.set_relation(a, b, Relation::Peace);
                    withdraw(state, a, b);
                    withdraw(state, b, a);
                }
                _ => {}
            }
        }
    }
}

fn territory_owner(state: &WorldState, hex: crate::hex::Hex) -> Option<CivId> {
    state
        .map
        .tile(hex)?
        .owner_city
        .and_then(|c| state.cities.get(c as usize))
        .filter(|c| c.alive)?
        .owner
}

/// Units of `civ` inside `other`'s territory go to the nearest free tile of
/// their own territory (distance, then tile index). With none free they stay.
fn withdraw(state: &mut WorldState, civ: CivId, other: CivId) {
    for i in 0..state.units.len() {
        let u = &state.units[i];
        if !u.alive || u.owner != Owner::Civ(civ) || territory_owner(state, u.hex) != Some(other) {
            continue;
        }
        let from = u.hex;
        let unit = u.clone();
        let target = state
            .map
            .tiles
            .iter()
            .filter(|t| t.terrain.is_passable() && territory_owner(state, t.hex) == Some(civ))
            .filter(|t| crate::tick::tile_free_for(state, &unit, t.hex))
            .min_by_key(|t| (t.hex.distance(from), t.hex))
            .map(|t| t.hex);
        if let Some(h) = target {
            let u = &mut state.units[i];
            u.hex = h;
            u.path.clear();
        }
    }
}

// ------------------------------------------------------------------ phase 2: transfers

/// Transfers between civilizations (§10.5). Frozen from tick 162 at submission.
pub fn apply_transfers(state: &mut WorldState, rules: &Ruleset, input: &TickInput) {
    for (civ, _, orders) in accepted(state, rules, input) {
        for order in orders {
            if let Order::Transfer {
                civ: to,
                good,
                amount,
            } = order
            {
                transfer(state, civ, to, good, amount);
            }
        }
    }
}

fn stock_mut(civ: &mut crate::state::Civ, good: Good) -> &mut i64 {
    match good {
        Good::Gold => &mut civ.gold,
        Good::Iron => &mut civ.iron,
        _ => &mut civ.horses,
    }
}

fn transfer(state: &mut WorldState, from: CivId, to: CivId, good: Good, amount: u32) {
    if !valid_pair(state, from, to) || amount == 0 || state.at_war(from, to) {
        return;
    }
    let last = state.civs[from as usize].last;
    let qty = amount as i64 * MILLI;
    match good {
        Good::Gold | Good::Iron | Good::Horses => {
            let cap = match good {
                Good::Gold => last.gold.max(10),
                Good::Iron => last.iron.max(1),
                _ => last.horses.max(1),
            };
            if amount > cap || *stock_mut(&mut state.civs[from as usize], good) < qty {
                return;
            }
            *stock_mut(&mut state.civs[from as usize], good) -= qty;
            *stock_mut(&mut state.civs[to as usize], good) += qty;
        }
        Good::Food(city) | Good::Production(city) => {
            let is_food = matches!(good, Good::Food(_));
            let cap = if is_food {
                last.max_city_food_surplus
            } else {
                last.max_city_prod
            };
            let target_ok = state
                .cities
                .get(city as usize)
                .is_some_and(|c| c.alive && c.owner == Some(to));
            if amount > cap || !target_ok {
                return;
            }
            // Taken from the sender's city holding the most of that good (then lowest id).
            let store = |c: &crate::state::City| if is_food { c.food } else { c.prod };
            let Some(src) = state
                .living_cities_of(from)
                .filter(|c| store(c) >= qty)
                .max_by_key(|c| (store(c), core::cmp::Reverse(c.id)))
                .map(|c| c.id as usize)
            else {
                return;
            };
            if is_food {
                state.cities[src].food -= qty;
                state.cities[city as usize].food += qty;
            } else {
                state.cities[src].prod -= qty;
                state.cities[city as usize].prod += qty;
            }
        }
    }
    event(state, b"transfer", from, to);
}

// ------------------------------------------------------------------ phase 2: envoys

/// Envoys (§12.1): all envoys of the tick are applied, then each unclaimed
/// city-state goes to the civ with the most influence at or above the
/// threshold (ties by tie-break), so civ order never decides suzerainty.
pub fn apply_envoys(state: &mut WorldState, rules: &Ruleset, input: &TickInput) {
    for (civ, _, orders) in accepted(state, rules, input) {
        for order in orders {
            if let Order::SendEnvoy {
                city_state,
                influence,
            } = order
            {
                let qty = influence as i64 * MILLI;
                let ok = qty > 0
                    && state.civs[civ as usize].influence >= qty
                    && state
                        .city_states
                        .get(city_state as usize)
                        .is_some_and(|cs| cs.captured_by.is_none());
                if ok {
                    state.civs[civ as usize].influence -= qty;
                    state.city_states[city_state as usize].influence[civ as usize] += qty;
                }
            }
        }
    }
    let threshold = rules.suzerain_threshold as i64 * MILLI;
    let seed = state.tick_seed;
    for cs in &mut state.city_states {
        if cs.suzerain.is_some() || cs.captured_by.is_some() {
            continue;
        }
        cs.suzerain = cs
            .influence
            .iter()
            .enumerate()
            .filter(|(_, v)| **v >= threshold)
            .max_by_key(|(c, v)| (**v, core::cmp::Reverse(tie_key(&seed, *c as u64))))
            .map(|(c, _)| c as CivId);
    }
}
