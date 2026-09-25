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

use crate::checks::{self, Blocked};
use crate::fixed::MILLI;
use crate::gov::{Credit, Path};
use crate::merit;
use crate::orders::Order;
use crate::params::Ruleset;
use crate::state::{CivId, Owner, Proposal, ProposalKind, Relation, WorldState};
use crate::tick::{accepted, allied};
use alloc::vec::Vec;

pub fn phase_diplomacy(state: &mut WorldState, rules: &Ruleset) {
    scheduled_transitions(state, rules);
    let now = state.tick;
    state
        .proposals
        .retain(|p| now.saturating_sub(p.tick) < rules.proposal_ttl);

    let diplomatic = |o: &Order| {
        matches!(
            o,
            Order::DeclareWar { .. }
                | Order::ProposePeace { .. }
                | Order::AcceptPeace { .. }
                | Order::ProposeNap { .. }
                | Order::AcceptNap { .. }
                | Order::BreakNap { .. }
                | Order::ProposeAlliance { .. }
                | Order::AcceptAlliance { .. }
                | Order::LeaveAlliance
        )
    };
    for (civ, order, credit, origin) in accepted(state, diplomatic) {
        let result = match order {
            Order::DeclareWar { civ: t } => {
                checks::declare_war(state, civ, t).map(|_| start_war(state, rules, civ, t))
            }
            Order::ProposePeace { civ: t } => checks::propose_peace(state, civ, t)
                .map(|_| propose(state, ProposalKind::Peace, civ, t, credit)),
            Order::AcceptPeace { civ: t } => accept_peace(state, rules, civ, t, credit),
            Order::ProposeNap { civ: t, bond } => checks::propose_nap(state, rules, civ, t, bond)
                .map(|_| propose(state, ProposalKind::Nap { bond }, civ, t, credit)),
            Order::AcceptNap { civ: t, bond } => accept_nap(state, rules, civ, t, bond, credit),
            Order::BreakNap { civ: t } => break_nap(state, civ, t),
            Order::ProposeAlliance { civ: t } => checks::propose_alliance(state, rules, civ, t)
                .map(|_| propose(state, ProposalKind::Alliance, civ, t, credit)),
            Order::AcceptAlliance { civ: t } => accept_alliance(state, rules, civ, t, credit),
            Order::LeaveAlliance => {
                leave_alliance(state, rules, civ);
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(why) = result {
            state.skip(civ, origin, why.code());
        }
    }
}

/// A treaty took effect: 20 merit to each side's diplomat (V5 §7.3).
fn treaty_merit(state: &mut WorldState, rules: &Ruleset, proposer: Credit, acceptor: Credit) {
    merit::credit(
        state,
        proposer,
        Path::Concord,
        rules.merit_treaty as u64,
        b"treaty",
    );
    merit::credit(
        state,
        acceptor,
        Path::Concord,
        rules.merit_treaty as u64,
        b"treaty",
    );
}

pub(crate) fn valid_pair(state: &WorldState, a: CivId, b: CivId) -> bool {
    a != b && (b as usize) < state.civs.len()
}

fn propose(state: &mut WorldState, kind: ProposalKind, from: CivId, to: CivId, credit: Credit) {
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
        credit,
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

pub(crate) fn event(state: &mut WorldState, kind: &[u8], a: CivId, b: CivId) {
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

fn accept_peace(
    state: &mut WorldState,
    rules: &Ruleset,
    civ: CivId,
    from: CivId,
    credit: Credit,
) -> Result<(), Blocked> {
    checks::accept_peace(state, civ, from)?;
    let Relation::War {
        declared_by,
        casus_belli,
        active_from,
        ..
    } = state.relation(civ, from)
    else {
        unreachable!("checked by accept_peace");
    };
    let Some(p) = take_proposal(state, from, civ, true, false) else {
        return Err(Blocked::NoProposal);
    };
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
    treaty_merit(state, rules, p.credit, credit);
    Ok(())
}

// ------------------------------------------------------------------ NAP

fn accept_nap(
    state: &mut WorldState,
    rules: &Ruleset,
    civ: CivId,
    from: CivId,
    bond: u32,
    credit: Credit,
) -> Result<(), Blocked> {
    checks::accept_nap(state, rules, civ, from, bond)?;
    let Some(Proposal {
        kind: ProposalKind::Nap { bond: their_bond },
        credit: proposer,
        ..
    }) = take_proposal(state, from, civ, false, true)
    else {
        return Err(Blocked::NoProposal);
    };
    // Both bonds are escrowed now; if either side cannot pay, the pact fails (§10.3).
    checks::nap_bonds_payable(state, civ, from, bond, their_bond)?;
    let (mine, theirs) = (bond as i64 * MILLI, their_bond as i64 * MILLI);
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
    treaty_merit(state, rules, proposer, credit);
    Ok(())
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

fn break_nap(state: &mut WorldState, civ: CivId, other: CivId) -> Result<(), Blocked> {
    if !valid_pair(state, civ, other) {
        return Err(Blocked::UnknownCiv);
    }
    let Some((mine, theirs)) = bonds(state, civ, other) else {
        return Err(Blocked::NotAtPeace);
    };
    // A pact made by peace keeps the truce: it cannot be broken before the
    // truce ends (like a declaration of war, §10.2).
    let until = state.truce_until[state.pair_index(civ, other)];
    if state.tick < until {
        return Err(Blocked::InTruce { until });
    }
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
    Ok(())
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

fn accept_alliance(
    state: &mut WorldState,
    rules: &Ruleset,
    civ: CivId,
    from: CivId,
    credit: Credit,
) -> Result<(), Blocked> {
    checks::join_alliance(state, rules, civ, from)?;
    let group = alliance_group(state, from);
    let Some(p) = take_proposal(state, from, civ, false, false) else {
        return Err(Blocked::NoProposal);
    };
    for m in group {
        return_bonds(state, civ, m); // an alliance supersedes a NAP
        state.set_relation(civ, m, Relation::Alliance { leaving_at: None });
        state.civs[m as usize].ever_allied = true;
    }
    state.civs[civ as usize].ever_allied = true;
    event(state, b"alliance", from, civ);
    treaty_merit(state, rules, p.credit, credit);
    Ok(())
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

fn scheduled_transitions(state: &mut WorldState, rules: &Ruleset) {
    let n = state.civs.len() as u16;
    let now = state.tick;
    for a in 0..n {
        for b in a + 1..n {
            match state.relation(a, b) {
                Relation::War {
                    peace_at: Some(p),
                    active_from,
                    ..
                } if now >= p => {
                    // Peace after a real war is a pact (revised 2026-09-25):
                    // a non-aggression treaty without bonds for `nap_ticks`,
                    // so ending a war well counts for concord (treaty
                    // partners). Breaking it needs consent like any pact.
                    let fought = p.saturating_sub(active_from) >= rules.peace_pact_min_war;
                    let after = if fought {
                        Relation::Nap {
                            until: p + rules.nap_ticks,
                            bond_low: 0,
                            bond_high: 0,
                        }
                    } else {
                        Relation::Peace
                    };
                    state.set_relation(a, b, after);
                    let i = state.pair_index(a, b);
                    state.truce_until[i] = p + rules.truce_ticks;
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

/// Units of `civ` inside `other`'s territory go to the nearest free tile of
/// their own territory (distance, then hex as seen from the unit's sextant,
/// so the choice turns with a symmetric map). With none free they stay.
fn withdraw(state: &mut WorldState, civ: CivId, other: CivId) {
    for i in 0..state.units.len() {
        let u = &state.units[i];
        if !u.alive || u.owner != Owner::Civ(civ) || state.territory_owner(u.hex) != Some(other) {
            continue;
        }
        let from = u.hex;
        let unit = u.clone();
        let target = state
            .map
            .tiles
            .iter()
            .filter(|t| t.terrain.is_passable() && state.territory_owner(t.hex) == Some(civ))
            .filter(|t| crate::tick::tile_free_for(state, &unit, t.hex))
            .min_by_key(|t| (t.hex.distance(from), t.hex.turned(from.sextant())))
            .map(|t| t.hex);
        if let Some(h) = target {
            let u = &mut state.units[i];
            u.hex = h;
            u.path.clear();
        }
    }
}
