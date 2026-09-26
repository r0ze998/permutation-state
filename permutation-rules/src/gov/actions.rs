//! Phase 0 (V5 §5): members' governance actions — stand, vote, propose,
//! support, recall — applied in input order.

use super::*;
use crate::orders::Order;
use crate::params::Ruleset;
use crate::state::{CivId, WorldState};
use alloc::vec::Vec;

/// First tick of the term after `tick`, if the season has one.
pub fn next_term_start(rules: &Ruleset, tick: u16) -> Option<u16> {
    let term = rules.term_ticks.max(1);
    let next = (tick / term + 1) * term;
    (next < rules.ticks_per_season).then_some(next)
}

/// Whether votes for the coming term are accepted at `tick`.
pub fn vote_open(rules: &Ruleset, tick: u16) -> bool {
    next_term_start(rules, tick).is_some_and(|t| t - tick <= rules.vote_window)
}

/// Phase 0: apply this tick's governance actions in input order.
pub fn apply_actions(state: &mut WorldState, rules: &Ruleset, entries: &[GovEntry]) {
    for e in entries {
        apply(state, rules, e, false);
    }
}

pub(super) fn apply(state: &mut WorldState, rules: &Ruleset, e: &GovEntry, pre_season: bool) {
    let Some(member) = state.members.get(e.member as usize) else {
        return;
    };
    if member.key != e.signer {
        return;
    }
    let civ = member.civ;
    let ok = match &e.action {
        GovAction::Stand { roles } => {
            state.members[e.member as usize].standing_for = roles & 0x0f;
            true
        }
        GovAction::Vote { role, candidate } => {
            let ok = if pre_season {
                true
            } else {
                vote_open(rules, state.tick) && is_member_of(state, *candidate, civ)
            };
            if ok {
                // One vote per office: a new vote replaces the member's last one.
                state.cast(e.member, *role, *candidate);
            }
            ok
        }
        GovAction::Propose { role, orders } => propose(state, rules, civ, e.member, *role, orders),
        GovAction::Support { proposal } => {
            let n = &mut state.nations[civ as usize];
            // A member backs at most `max_member_supports` open proposals.
            let backing = n
                .proposals
                .iter()
                .filter(|p| p.supporters.contains(&e.member))
                .count();
            match n.proposals.iter_mut().find(|p| p.id == *proposal) {
                _ if backing >= rules.max_member_supports as usize => false,
                Some(p) if p.proposer != e.member && !p.supporters.contains(&e.member) => {
                    p.supporters.push(e.member);
                    true
                }
                _ => false,
            }
        }
        GovAction::Recall { role } => recall_vote(state, civ, e.member, *role),
    };
    if ok && e.action.counts_as_activity() {
        mark_active(state, rules, e.member);
    }
}

/// The checks on a proposal that need no world: 1..=`max_proposal_orders`
/// orders, none of them a reveal (a reveal opens the officer's own decision,
/// and it is the largest order, free of budget), at most
/// `max_proposal_bytes` encoded, and a valid structure (WP07). The engine
/// runs it when the proposal is made; the server and clients before sending.
pub fn proposal_shape_ok(rules: &Ruleset, open_tick: u16, orders: &[Order]) -> bool {
    !orders.is_empty()
        && orders.len() <= rules.max_proposal_orders as usize
        && orders.iter().all(|o| o.is_action())
        && borsh::object_length(orders).is_ok_and(|len| len <= rules.max_proposal_bytes as usize)
        && crate::orders::check_structure(rules, open_tick, orders).is_ok()
}

pub(super) fn propose(
    state: &mut WorldState,
    rules: &Ruleset,
    civ: CivId,
    m: MemberId,
    role: Role,
    orders: &[Order],
) -> bool {
    if !proposal_shape_ok(rules, state.tick, orders) {
        return false;
    }
    // Treasury orders are the officers' own (V5 §7.5 ④): never proposals.
    if orders.iter().any(Order::is_treasury_order) {
        return false;
    }
    if !orders
        .iter()
        .all(|o| crate::orders::role_allows(state, role, o))
    {
        return false;
    }
    let n = &mut state.nations[civ as usize];
    if n.proposals.len() >= rules.max_open_proposals as usize
        || n.proposals.iter().filter(|p| p.proposer == m).count()
            >= rules.max_member_proposals as usize
    {
        return false;
    }
    let id = n.next_proposal;
    n.next_proposal += 1;
    n.proposals.push(GovProposal {
        id,
        role,
        proposer: m,
        tick: state.tick,
        orders: orders.to_vec(),
        supporters: Vec::new(),
        adopted: false,
    });
    true
}

pub(super) fn recall_vote(state: &mut WorldState, civ: CivId, m: MemberId, role: Role) -> bool {
    let tick = state.tick;
    let n = &mut state.nations[civ as usize];
    let holder = n.offices[role.index()];
    if holder == NOBODY {
        return false;
    }
    match n
        .recalls
        .iter_mut()
        .find(|r| r.role == role && r.holder == holder)
    {
        Some(r) if r.yes.contains(&m) => false,
        Some(r) => {
            r.yes.push(m);
            true
        }
        None => {
            n.recalls.push(Recall {
                role,
                holder,
                opened: tick,
                automatic: false,
                yes: alloc::vec![m],
            });
            true
        }
    }
}
