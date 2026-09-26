//! Phase 0 (§15, V5 §5): the tick seed, governance actions, deposits, and
//! merging every office's batch into the orders the later phases execute
//! (`state.tick_orders`), with proposal adoption and war consent.

use crate::checks::Blocked;
use crate::gov::{Credit, Path, Role, NOBODY};
use crate::merit;
use crate::orders::{accept_batch, batch_order_refs, role_allows, CivOrders, Order, OrderBatch};
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

/// An office's accepted batch: the input's (by reference) or the caretaker's.
enum Pick<'a> {
    Input(&'a OrderBatch),
    Caretaker(OrderBatch),
}

impl Pick<'_> {
    fn batch(&self) -> &OrderBatch {
        match self {
            Pick::Input(b) => b,
            Pick::Caretaker(b) => b,
        }
    }
}

/// Pick each office's batch (the first valid one), commit its decision,
/// and merge the offices' orders in `Role::ALL` order (V5 §5.1–§5.6).
/// Orders are copied once, into the result (the chain's heap never frees):
/// batches are picked and inspected by reference.
fn merge_orders(
    state: &mut WorldState,
    rules: &Ruleset,
    input: &TickInput,
    civ: CivId,
) -> CivOrders {
    let tick = state.tick;
    let mut chosen: [Option<(Pick, u32)>; 4] = [None, None, None, None];
    for role in Role::ALL {
        // A vacant office takes no batch from anyone: the caretaker fills it
        // from the members' top proposal or a minimal default (gov::caretaker).
        if state.nations[civ as usize].holder(role) == NOBODY {
            if let Some((b, cost)) = crate::gov::caretaker::batch(state, rules, civ, role) {
                chosen[role.index()] = Some((Pick::Caretaker(b), cost));
            }
            continue;
        }
        let mut rejected = false;
        for b in input
            .batches
            .iter()
            .filter(|b| b.civ == civ && b.role == role)
        {
            match accept_batch(state, rules, b) {
                Ok((cost, _, _)) => {
                    chosen[role.index()] = Some((Pick::Input(b), cost));
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
    for (pick, _) in chosen.iter().flatten() {
        let b = pick.batch();
        let mut payload = [0u8; 39];
        payload[..2].copy_from_slice(&civ.to_le_bytes());
        payload[2] = b.role as u8;
        payload[3..7].copy_from_slice(&b.member.to_le_bytes());
        payload[7..].copy_from_slice(&b.decision_digest);
        state.push_event(b"decision", &payload);
    }

    // War needs a second officer (V5 §5.6): the general's or the steward's
    // consent, from a different person than the diplomat. The orders are
    // counted here too, before anything changes the world.
    let diplomat = chosen[Role::Diplomat.index()]
        .as_ref()
        .map(|c| c.0.batch().member);
    let mut consents: Vec<CivId> = Vec::new();
    let mut total = 0usize;
    for (pick, _) in chosen.iter().flatten() {
        let b = pick.batch();
        let (refs, _) = batch_order_refs(state, b).unwrap_or_default();
        total += refs.len();
        if matches!(b.role, Role::General | Role::Steward)
            && diplomat.is_none_or(|d| b.member != d || d == NOBODY)
        {
            consents.extend(refs.iter().filter_map(|(o, _)| match o {
                Order::ConsentWar { civ } => Some(*civ),
                _ => None,
            }));
        }
    }

    // Sized once: three vectors growing in lockstep would each leave their
    // old buffers behind in the program's bump heap.
    let mut out = CivOrders {
        civ,
        orders: Vec::with_capacity(total),
        credits: Vec::with_capacity(total),
        origin: Vec::with_capacity(total),
        spent: [0; 4],
    };
    for role in Role::ALL {
        let Some((pick, cost)) = chosen[role.index()].take() else {
            continue;
        };
        let batch = pick.batch();
        let member = batch.member;
        // Adopted proposals' orders are copied before the proposals are
        // marked; the batch's own orders are copied one by one below. A
        // batch that does not expand (never an accepted one) runs nothing.
        let (own_orders, adopted, adopt): (&[Order], Vec<(Order, Credit)>, Vec<u32>) =
            match batch_order_refs(state, batch) {
                Ok((refs, adopt)) => (
                    &batch.orders,
                    refs.into_iter()
                        .skip(batch.orders.len())
                        .map(|(o, c)| (o.clone(), c))
                        .collect(),
                    adopt,
                ),
                Err(_) => (&[], Vec::new(), Vec::new()),
            };
        let own = Credit::officer(member);
        out.spent[role.index()] = cost as u16;
        if member != NOBODY {
            state.nations[civ as usize].office_seen[role.index()] = tick;
        }
        for id in &adopt {
            let n = &mut state.nations[civ as usize];
            if let Some(p) = n.proposals.iter_mut().find(|p| p.id == *id && !p.adopted) {
                p.adopted = true;
                n.adopted += 1;
            }
        }
        let mut acted = false;
        let orders = own_orders.iter().map(|o| (o.clone(), own)).chain(adopted);
        for (i, (order, mut credit)) in orders.enumerate() {
            let origin = (role as u8, i as u16);
            if !role_allows(state, role, &order) {
                state.skip(civ, origin, Blocked::WrongOffice.code());
                continue;
            }
            let war = order.war_target().is_some_and(|t| !consents.contains(&t));
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
        if acted && member != NOBODY {
            crate::gov::mark_active(state, rules, member);
            state.nations[civ as usize].office_last_act[role.index()] = tick;
            merit::credit(
                state,
                Credit::officer(member),
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
