//! Phase 0 (§15, V5 §5): the tick seed, governance actions, deposits, and
//! merging every office's batch into the orders the later phases execute
//! (`state.tick_orders`), with proposal adoption and war consent.

use crate::checks::Blocked;
use crate::gov::{Credit, Path, Role, NOBODY};
use crate::merit;
use crate::orders::{batch_orders, role_allows, validate_batch, CivOrders, Order, OrderBatch};
use crate::params::Ruleset;
use crate::rng::tick_seed;
use crate::state::{CivId, WorldState};
use crate::tick::TickInput;
use alloc::vec::Vec;

pub(crate) fn phase_seed(state: &mut WorldState, rules: &Ruleset, input: &TickInput) {
    state.tick_seed = tick_seed(&state.season_seed, &input.vrf, state.tick);
    state.last_skipped.clear();
    state.merit_log.clear();
    for c in &mut state.cities {
        c.attacked_this_tick = false;
    }
    for civ in &mut state.civs {
        civ.deficit = false;
        civ.troops_lost = 0;
    }
    // Governance first: supports and proposals of this tick count for adoption.
    crate::gov::apply_actions(state, rules, &input.gov);
    for &(civ, amount) in &input.deposits {
        if state.tick < rules.exchange_freeze_tick
            && rules.market_enabled
            && (civ as usize) < state.civs.len()
        {
            state.civs[civ as usize].usdc += amount;
            state.usdc_deposited += amount;
        }
    }
    state.tick_orders = (0..state.civs.len() as CivId)
        .map(|civ| merge_orders(state, rules, input, civ))
        .collect();
}

/// An office's accepted batch, its orders with credits, and its cost.
type Chosen = (OrderBatch, Vec<(Order, Credit)>, u32);

/// Pick each office's batch (the first valid one), commit its decision,
/// and merge the offices' orders in `Role::ALL` order (V5 §5.1–§5.6).
fn merge_orders(
    state: &mut WorldState,
    rules: &Ruleset,
    input: &TickInput,
    civ: CivId,
) -> CivOrders {
    let tick = state.tick;
    let mut chosen: [Option<Chosen>; 4] = [None, None, None, None];
    for role in Role::ALL {
        // A vacant office takes no batch from anyone: the caretaker fills it
        // from the members' top proposal or a minimal default (gov::caretaker).
        if state.nations[civ as usize].holder(role) == NOBODY {
            if let Some((b, cost)) = crate::gov::caretaker::batch(state, rules, civ, role) {
                let orders = batch_orders(state, &b).unwrap_or_default();
                chosen[role.index()] = Some((b, orders, cost));
            }
            continue;
        }
        let mut rejected = false;
        for b in input
            .batches
            .iter()
            .filter(|b| b.civ == civ && b.role == role)
        {
            match validate_batch(state, rules, b) {
                Ok(cost) => {
                    let orders = batch_orders(state, b).unwrap_or_default();
                    chosen[role.index()] = Some((b.clone(), orders, cost));
                    break;
                }
                Err(_) => rejected = true,
            }
        }
        if rejected && chosen[role.index()].is_none() {
            state.skip(civ, (role as u8, u16::MAX), Blocked::BatchRejected.code());
        }
    }

    // Decision commitments enter the event chain before anything resolves (§4.3).
    for (b, _, _) in chosen.iter().flatten() {
        let mut payload = [0u8; 39];
        payload[..2].copy_from_slice(&civ.to_le_bytes());
        payload[2] = b.role as u8;
        payload[3..7].copy_from_slice(&b.member.to_le_bytes());
        payload[7..].copy_from_slice(&b.decision_digest);
        state.push_event(b"decision", &payload);
    }

    // War needs a second officer (V5 §5.6): the general's or the steward's
    // consent, from a different person than the diplomat.
    let diplomat = chosen[Role::Diplomat.index()].as_ref().map(|c| c.0.member);
    let consents: Vec<CivId> = [Role::General, Role::Steward]
        .iter()
        .filter_map(|r| chosen[r.index()].as_ref())
        .filter(|(b, _, _)| diplomat.is_none_or(|d| b.member != d || d == NOBODY))
        .flat_map(|(_, orders, _)| {
            orders.iter().filter_map(|(o, _)| match o {
                Order::ConsentWar { civ } => Some(*civ),
                _ => None,
            })
        })
        .collect();

    let mut out = CivOrders {
        civ,
        orders: Vec::new(),
        credits: Vec::new(),
        origin: Vec::new(),
        spent: [0; 4],
    };
    for role in Role::ALL {
        let Some((batch, orders, cost)) = chosen[role.index()].take() else {
            continue;
        };
        out.spent[role.index()] = cost as u16;
        if batch.member != NOBODY {
            state.nations[civ as usize].office_seen[role.index()] = tick;
        }
        for id in &batch.adopt {
            let n = &mut state.nations[civ as usize];
            if let Some(p) = n.proposals.iter_mut().find(|p| p.id == *id && !p.adopted) {
                p.adopted = true;
                n.adopted += 1;
            }
        }
        let mut acted = false;
        for (i, (order, mut credit)) in orders.into_iter().enumerate() {
            let origin = (role as u8, i as u16);
            if !role_allows(state, role, &order) {
                state.skip(civ, origin, Blocked::WrongOffice.code());
                continue;
            }
            let war = matches!(order, Order::DeclareWar { civ: t } | Order::BreakNap { civ: t } if !consents.contains(&t));
            if war {
                state.skip(civ, origin, Blocked::NeedsConsent.code());
                continue;
            }
            if credit.proposer == NOBODY && credit.officer != NOBODY {
                credit.proposer = auto_match(state, civ, role, &order);
            }
            acted |= order.is_action();
            out.orders.push(order);
            out.credits.push(credit);
            out.origin.push(origin);
        }
        // An office acted this tick (v0.2 C9: reveals alone do not count).
        if acted && batch.member != NOBODY {
            crate::gov::mark_active(state, rules, batch.member);
            state.nations[civ as usize].office_last_act[role.index()] = tick;
            merit::credit(
                state,
                Credit::officer(batch.member),
                Path::Common,
                rules.merit_office_tick as u64,
                b"office",
            );
        }
    }
    out
}

/// An officer's own order identical to one in a supported proposal made on
/// an earlier tick counts as adopting it (V5 §5.4, against claim-jumping).
fn auto_match(state: &mut WorldState, civ: CivId, role: Role, order: &Order) -> u32 {
    let tick = state.tick;
    let n = &mut state.nations[civ as usize];
    match n.proposals.iter_mut().find(|p| {
        p.role == role
            && !p.adopted
            && p.tick < tick
            && !p.supporters.is_empty()
            && p.orders.contains(order)
    }) {
        Some(p) => {
            p.adopted = true;
            let proposer = p.proposer;
            n.adopted += 1;
            proposer
        }
        None => NOBODY,
    }
}
