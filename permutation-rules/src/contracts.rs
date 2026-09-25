//! Treasury contracts (V5 §18.6): a nation escrows USDC from its treasury
//! and the program pays it to another nation's treasury when a condition
//! on the world is met, or returns it at the deadline.
//!
//! Words are free and not binding; money moves only on what the world
//! shows. The escrow counts as treasury spending (the per-tick limit that
//! needs a second officer's consent, and the tariff position); what a
//! nation receives is locked out of the market (`Civ::contract_income`) and
//! goes back to its depositors at the end, so contracts cannot route around
//! the tariff. Contracts never count as trade, wealth or merit.
//!
//! Orders are placed in phase 2 (before the market, sharing its spend
//! limit); conditions are checked in phase 10, after diplomacy and combat.

use crate::checks::Blocked;
use crate::gov::Credit;
use crate::markets::{short_of_usdc, treasury_room};
use crate::orders::Order;
use crate::params::Ruleset;
use crate::state::{CityId, CivId, WorldState};
use crate::tick::accepted;
use alloc::vec;
use alloc::vec::Vec;
use borsh::{BorshDeserialize, BorshSerialize};

/// What a contract pays for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ContractTerm {
    /// The counterparty makes peace with the offerer by the deadline.
    Peace,
    /// The counterparty leaves its alliance with `with` by the deadline.
    LeaveAlliance { with: CivId },
    /// The counterparty keeps its NAP with the offerer: once accepted (by
    /// the deadline), one of `installments` equal parts is paid every
    /// `every` ticks while the NAP stands; the rest returns if it breaks.
    KeepNap { every: u16, installments: u8 },
    /// Open to every nation: whichever nation captures `city` by the
    /// deadline is paid (a bounty offer). Cannot be cancelled.
    Capture { city: CityId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Contract {
    pub id: u32,
    pub from: CivId,
    /// The counterparty; `None` for an open `Capture` offer.
    pub to: Option<CivId>,
    pub term: ContractTerm,
    /// USDC still escrowed.
    pub escrow: u64,
    /// USDC offered in all.
    pub total: u64,
    pub offered: u16,
    /// Last tick to accept (`KeepNap`) or to fulfil (the others).
    pub deadline: u16,
    pub accepted: Option<u16>,
    /// For `Capture`: the city's holder when offered.
    pub holder: Option<CivId>,
    /// `KeepNap` installments paid so far.
    pub paid: u8,
    pub credit: Credit,
}

impl Contract {
    /// Whether the counterparty must accept before the condition counts.
    pub const fn needs_acceptance(&self) -> bool {
        !matches!(self.term, ContractTerm::Capture { .. })
    }
}

/// Contracts `civ` offered that are still escrowed.
pub fn open_offers(state: &WorldState, civ: CivId) -> usize {
    state.contracts.iter().filter(|c| c.from == civ).count()
}

/// The terms of an offer, checked against the world now.
fn check_offer(
    state: &WorldState,
    rules: &Ruleset,
    civ: CivId,
    to: Option<CivId>,
    term: ContractTerm,
    usdc: u64,
    deadline: u16,
) -> Result<(), Blocked> {
    if !rules.market_enabled {
        return Err(Blocked::Frozen);
    }
    let n = state.civs.len() as CivId;
    let now = state.tick;
    let last = rules.ticks_per_season.saturating_sub(1);
    if usdc == 0
        || deadline <= now
        || deadline > last
        || deadline > now.saturating_add(rules.contract_max_ticks)
    {
        return Err(Blocked::BadContract);
    }
    if open_offers(state, civ) >= rules.contract_max_open as usize {
        return Err(Blocked::TooManyContracts);
    }
    match (term, to) {
        (ContractTerm::Capture { city }, None) => {
            let c = state
                .cities
                .get(city as usize)
                .ok_or(Blocked::UnknownCity)?;
            // Another nation's living city.
            if !c.alive || c.owner.is_none() || c.owner == Some(civ) {
                return Err(Blocked::BadContract);
            }
        }
        (ContractTerm::Capture { .. }, Some(_)) | (_, None) => return Err(Blocked::BadContract),
        (term, Some(t)) => {
            if t >= n {
                return Err(Blocked::UnknownCiv);
            }
            if t == civ {
                return Err(Blocked::SameCiv);
            }
            let ok = match term {
                ContractTerm::Peace => state.war_pending(civ, t),
                ContractTerm::LeaveAlliance { with } => {
                    with < n && with != civ && with != t && allied(state, t, with)
                }
                ContractTerm::KeepNap {
                    every,
                    installments,
                } => {
                    every > 0
                        && installments > 0
                        && installments <= rules.contract_max_installments
                        && under_nap(state, civ, t)
                }
                ContractTerm::Capture { .. } => false,
            };
            if !ok {
                return Err(Blocked::BadContract);
            }
        }
    }
    Ok(())
}

fn allied(state: &WorldState, a: CivId, b: CivId) -> bool {
    state.relation(a, b).is_alliance()
}

fn under_nap(state: &WorldState, a: CivId, b: CivId) -> bool {
    state.relation(a, b).is_nap()
}

/// Phase 2 (before the market): offers, acceptances and cancellations, in
/// engine order. Returns the USDC each civ escrowed this tick, which the
/// market's spend limit then counts.
pub fn apply_contract_orders(state: &mut WorldState, rules: &Ruleset) -> Vec<u64> {
    let mut escrowed = vec![0u64; state.civs.len()];
    let pick = |o: &Order| {
        matches!(
            o,
            Order::OfferContract { .. }
                | Order::AcceptContract { .. }
                | Order::CancelContract { .. }
        )
    };
    for (civ, order, credit, origin) in accepted(state, pick) {
        let result = match order {
            Order::OfferContract {
                to,
                term,
                usdc,
                deadline,
            } => check_offer(state, rules, civ, to, term, usdc, deadline).and_then(|_| {
                let room = treasury_room(state, rules, civ, escrowed[civ as usize]);
                if usdc > room {
                    return Err(short_of_usdc(usdc <= state.civs[civ as usize].free_usdc()));
                }
                escrowed[civ as usize] += usdc;
                offer(state, civ, to, term, usdc, deadline, credit);
                Ok(())
            }),
            Order::AcceptContract { id } => accept(state, civ, id),
            Order::CancelContract { id } => cancel(state, civ, id),
            _ => Ok(()),
        };
        if let Err(why) = result {
            state.skip(civ, origin, why.code());
        }
    }
    escrowed
}

fn offer(
    state: &mut WorldState,
    civ: CivId,
    to: Option<CivId>,
    term: ContractTerm,
    usdc: u64,
    deadline: u16,
    credit: Credit,
) {
    let c = &mut state.civs[civ as usize];
    c.usdc -= usdc;
    c.market_spent += usdc;
    let holder = match term {
        ContractTerm::Capture { city } => state.cities[city as usize].owner,
        _ => None,
    };
    let id = state.next_contract;
    state.next_contract += 1;
    state.contracts.push(Contract {
        id,
        from: civ,
        to,
        term,
        escrow: usdc,
        total: usdc,
        offered: state.tick,
        deadline,
        accepted: None,
        holder,
        paid: 0,
        credit,
    });
    event(state, b"contract_offer", id, civ);
}

fn accept(state: &mut WorldState, civ: CivId, id: u32) -> Result<(), Blocked> {
    let now = state.tick;
    let i = state
        .contracts
        .iter()
        .position(|c| c.id == id && c.to == Some(civ) && c.accepted.is_none())
        .ok_or(Blocked::UnknownContract)?;
    let c = state.contracts[i];
    // Offered on an earlier tick, and its condition still possible.
    let still = match c.term {
        ContractTerm::Peace => state.war_pending(c.from, civ),
        ContractTerm::LeaveAlliance { with } => allied(state, civ, with),
        ContractTerm::KeepNap { .. } => under_nap(state, c.from, civ),
        ContractTerm::Capture { .. } => false,
    };
    if c.offered >= now || now > c.deadline || !still {
        return Err(Blocked::BadContract);
    }
    state.contracts[i].accepted = Some(now);
    if let ContractTerm::KeepNap {
        every,
        installments,
    } = c.term
    {
        // The deadline becomes the last installment.
        state.contracts[i].deadline = now.saturating_add(every.saturating_mul(installments as u16));
    }
    event(state, b"contract_accept", id, civ);
    Ok(())
}

fn cancel(state: &mut WorldState, civ: CivId, id: u32) -> Result<(), Blocked> {
    let i = state
        .contracts
        .iter()
        .position(|c| c.id == id && c.from == civ && c.accepted.is_none() && c.needs_acceptance())
        .ok_or(Blocked::UnknownContract)?;
    let c = state.contracts.remove(i);
    state.civs[civ as usize].usdc += c.escrow;
    event(state, b"contract_cancel", id, civ);
    Ok(())
}

/// Phase 10: pay the contracts whose condition the world now shows, return
/// the escrow of those past their deadline or broken, and at the last tick
/// return every escrow (nothing may stay escrowed when the season ends).
pub fn settle_contracts(state: &mut WorldState, rules: &Ruleset) {
    let now = state.tick;
    let last = now + 1 >= rules.ticks_per_season;
    let mut keep = Vec::with_capacity(state.contracts.len());
    for mut c in core::mem::take(&mut state.contracts) {
        match outcome(state, &c, now) {
            Outcome::Pay(to, amount, paid) => {
                c.paid = paid;
                pay(state, &mut c, to, amount);
                if c.escrow > 0 && !last {
                    keep.push(c);
                    continue;
                }
            }
            Outcome::Wait if !last && now <= c.deadline => {
                keep.push(c);
                continue;
            }
            _ => {}
        }
        // Done, broken, expired or season over: what is left goes back.
        if c.escrow > 0 {
            state.civs[c.from as usize].usdc += c.escrow;
            event(state, b"contract_return", c.id, c.from);
        }
    }
    state.contracts = keep;
}

enum Outcome {
    /// Pay the civ this much; `KeepNap` installments paid after it.
    Pay(CivId, u64, u8),
    Wait,
    Void,
}

fn outcome(state: &WorldState, c: &Contract, now: u16) -> Outcome {
    let accepted = match (c.accepted, c.needs_acceptance()) {
        (Some(t), true) => t,
        (None, true) => return Outcome::Wait,
        (_, false) => c.offered,
    };
    match (c.term, c.to) {
        (ContractTerm::Peace, Some(to)) => {
            if !state.war_pending(c.from, to) {
                Outcome::Pay(to, c.escrow, 0)
            } else {
                Outcome::Wait
            }
        }
        (ContractTerm::LeaveAlliance { with }, Some(to)) => {
            if allied(state, to, with) {
                Outcome::Wait
            } else {
                Outcome::Pay(to, c.escrow, 0)
            }
        }
        (
            ContractTerm::KeepNap {
                every,
                installments,
            },
            Some(to),
        ) => {
            if !under_nap(state, c.from, to) {
                return Outcome::Void;
            }
            let due = (now.saturating_sub(accepted) / every).min(installments as u16) as u8;
            if due > c.paid && now > accepted {
                let each = c.total / installments as u64;
                // The last installment takes the remainder.
                let amount = if due >= installments {
                    c.escrow
                } else {
                    (each * (due - c.paid) as u64).min(c.escrow)
                };
                Outcome::Pay(to, amount, due)
            } else {
                Outcome::Wait
            }
        }
        (ContractTerm::Capture { city }, None) => {
            let x = &state.cities[city as usize];
            if !x.alive {
                return Outcome::Void;
            }
            match (x.owner, x.captured_tick) {
                (Some(o), Some(t)) if t > c.offered && o != c.from && Some(o) != c.holder => {
                    Outcome::Pay(o, c.escrow, 0)
                }
                _ => Outcome::Wait,
            }
        }
        _ => Outcome::Void,
    }
}

fn pay(state: &mut WorldState, c: &mut Contract, to: CivId, amount: u64) {
    let amount = amount.min(c.escrow);
    c.escrow -= amount;
    let r = &mut state.civs[to as usize];
    r.usdc += amount;
    r.contract_income += amount;
    event(state, b"contract_pay", c.id, to);
}

fn event(state: &mut WorldState, kind: &[u8], id: u32, civ: CivId) {
    let mut payload = [0u8; 6];
    payload[..4].copy_from_slice(&id.to_le_bytes());
    payload[4..].copy_from_slice(&civ.to_le_bytes());
    state.push_event(kind, &payload);
}

/// USDC escrowed in open contracts (the conservation invariant counts it).
pub fn escrowed(state: &WorldState) -> u64 {
    state.contracts.iter().map(|c| c.escrow).sum()
}
