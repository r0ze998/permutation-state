//! Orders, costs and submit-time validation (§4).
//!
//! Submission checks only what cannot change before resolution: the tick,
//! the budget, freezes and per-unit uniqueness. Everything state-dependent is
//! re-checked at resolution, and an order that has become invalid is dropped
//! without refund (§4.1).

use crate::gov::{Credit, MemberId, Role, NOBODY};
use crate::hex::Hex;
use crate::params::Ruleset;
use crate::state::{CityId, CivId, Focus, QueueItem, UnitId, WorldState};
use crate::tech::Tech;
use crate::units::UnitType;
use crate::RulesError;
use alloc::vec::Vec;
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Good {
    Gold,
    Iron,
    Horses,
    /// Delivered into a named city's food store.
    Food(CityId),
    /// Delivered into a named city's production store.
    Production(CityId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum AttackTarget {
    Unit(UnitId),
    City(CityId),
    CityState(u16),
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Order {
    MoveUnit {
        unit: UnitId,
        path: Vec<Hex>,
    },
    Attack {
        army: UnitId,
        target: AttackTarget,
    },
    FoundCity {
        settler: UnitId,
    },
    SetQueue {
        city: CityId,
        items: Vec<QueueItem>,
    },
    SetFocus {
        city: CityId,
        focus: Focus,
    },
    Purchase {
        city: CityId,
        gold: u32,
    },
    SetResearch {
        techs: Vec<Tech>,
    },
    DeclareWar {
        civ: CivId,
    },
    ProposePeace {
        civ: CivId,
    },
    AcceptPeace {
        civ: CivId,
    },
    ProposeNap {
        civ: CivId,
        bond: u32,
    },
    AcceptNap {
        civ: CivId,
        bond: u32,
    },
    BreakNap {
        civ: CivId,
    },
    ProposeAlliance {
        civ: CivId,
    },
    AcceptAlliance {
        civ: CivId,
    },
    LeaveAlliance,
    SendEnvoy {
        city_state: u16,
        influence: u32,
    },
    Transfer {
        civ: CivId,
        good: Good,
        amount: u32,
    },
    MarketTrade {
        good: Good,
        side: Side,
        amount: u32,
        limit_gold: u32,
    },
    /// USDC P2P Exchange (§11.3). `price` in USDC base units (6 decimals) per whole good.
    ExchangeOrder {
        good: Good,
        side: Side,
        amount: u32,
        price: u64,
    },
    Raze {
        city: CityId,
    },
    /// Set or clear a standing rule (§13). Costs 1; execution is free.
    SetStanding {
        target: StandingTarget,
        rule: StandingOrder,
    },
    /// Opens the commitment made at `tick` (§4.3): anyone can then check
    /// `decision_digest == digest(tick, obs_root, policy_id(policy), rationale_hash(salt, text))`.
    RevealRationale {
        tick: u16,
        policy: Vec<u8>,
        salt: [u8; 16],
        text: Vec<u8>,
    },
    /// The general or steward agrees to this tick's `DeclareWar` or
    /// `BreakNap` against `civ` (V5 §5.6). Free.
    ConsentWar {
        civ: CivId,
    },
    /// Another officer allows the diplomat to spend up to `usdc` from the
    /// treasury this tick, above `spend_consent_usdc` (V5 §7.5). Free.
    ConsentSpend {
        usdc: u64,
    },
    /// Escrow `usdc` from the treasury for `term`, paid to `to` (or, for a
    /// `Capture` offer, to whichever nation captures the city) if the world
    /// shows it by `deadline`, else returned (V5 §18.6).
    OfferContract {
        to: Option<CivId>,
        term: crate::contracts::ContractTerm,
        usdc: u64,
        deadline: u16,
    },
    /// Accept a contract offered to this nation on an earlier tick.
    AcceptContract {
        id: u32,
    },
    /// Withdraw an offer not yet accepted (not a `Capture` offer).
    CancelContract {
        id: u32,
    },
}

/// What a `SetStanding` order applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum StandingTarget {
    Unit(UnitId),
    City(CityId),
}

/// The rule requested by `SetStanding` (§13). Unit rules: `Clear`,
/// `AutoDefend`, `Retreat`, `Patrol`. City rules: `QueueRepeat`, `AutoPurchase`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum StandingOrder {
    Clear,
    AutoDefend {
        radius: u8,
    },
    Retreat {
        ratio_bps: u32,
    },
    /// Waypoints, 1..=6.
    Patrol {
        route: Vec<Hex>,
    },
    QueueRepeat {
        on: bool,
    },
    AutoPurchase {
        max_gold: u32,
    },
}

pub const MAX_PATROL: usize = 6;

impl Order {
    /// This order in a world turned by `k` × 60° (`mapgen::rotate_world`):
    /// its hexes (move paths, patrol routes) turned the same way.
    pub fn rotate(&mut self, k: u8) {
        match self {
            Order::MoveUnit { path, .. } => path.iter_mut().for_each(|h| *h = h.rotate_by(k)),
            Order::SetStanding {
                rule: StandingOrder::Patrol { route },
                ..
            } => route.iter_mut().for_each(|h| *h = h.rotate_by(k)),
            _ => {}
        }
    }

    /// Budget cost (§4.2).
    pub const fn cost(&self) -> u32 {
        match self {
            Order::ExchangeOrder { .. }
            | Order::RevealRationale { .. }
            | Order::ConsentWar { .. }
            | Order::ConsentSpend { .. } => 0,
            _ => 1,
        }
    }

    /// The nation this order goes to war with (`DeclareWar`, `BreakNap`):
    /// such an order needs a second officer's `ConsentWar` (V5 §5.6).
    pub const fn war_target(&self) -> Option<CivId> {
        match self {
            Order::DeclareWar { civ } | Order::BreakNap { civ } => Some(*civ),
            _ => None,
        }
    }

    /// Whether the order does something (anything but a reveal, v0.2 C9).
    pub const fn is_action(&self) -> bool {
        !matches!(self, Order::RevealRationale { .. })
    }

    /// The unit this order commands, for the one-manual-order-per-unit rule.
    pub fn commanded_unit(&self) -> Option<UnitId> {
        match self {
            Order::MoveUnit { unit, .. } => Some(*unit),
            Order::SetStanding {
                target: StandingTarget::Unit(unit),
                ..
            } => Some(*unit),
            Order::Attack { army, .. } => Some(*army),
            Order::FoundCity { settler } => Some(*settler),
            _ => None,
        }
    }
}

/// One office's orders for one tick (§4.3, V5 §5.1).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct OrderBatch {
    pub civ: CivId,
    pub tick: u16,
    pub role: Role,
    /// The office holder who issued it (`NOBODY` for a vacant office, whose
    /// batches the caretaker replaces). The chain program sets it from the
    /// signer.
    pub member: MemberId,
    /// `sha256(tick ‖ obs_root ‖ policy_id ‖ rationale_hash)`, stored, never
    /// interpreted. Required (non-zero) from a member (V5 D17).
    pub decision_digest: [u8; 32],
    pub orders: Vec<Order>,
    /// Proposals adopted (V5 §5.4): their orders run after `orders`, with
    /// the merit shared with the proposer.
    pub adopt: Vec<u32>,
}

/// An officer's sealed orders (commit–reveal, 2026-09-25): the commitment
/// sent before the tick's deadline is `sha256("permutation-rules/orders" ‖
/// borsh(batch) ‖ salt)`; the batch and the salt are revealed after the
/// deadline and must hash to it. The batch names the civ, tick, office,
/// member, decision digest, orders and adopted proposals, so a commitment
/// binds all of them.
pub fn order_commitment(batch: &OrderBatch, salt: &[u8; 32]) -> [u8; 32] {
    let bytes = borsh::to_vec(batch).expect("borsh into a Vec cannot fail");
    crate::hash::sha256(&[b"permutation-rules/orders", &bytes, salt])
}

/// One civ's accepted orders for the current tick, all offices merged in
/// `Role::ALL` order (phase 0).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CivOrders {
    pub civ: CivId,
    pub orders: Vec<Order>,
    /// Merit credit of each order.
    pub credits: Vec<Credit>,
    /// Office and position within that office's batch of each order (for `Skip`).
    pub origin: Vec<(u8, u16)>,
    /// Budget spent per office.
    pub spent: [u16; 4],
}

impl CivOrders {
    /// `(order, credit, origin)` triples.
    pub fn iter(&self) -> impl Iterator<Item = (&Order, Credit, (u8, u16))> {
        self.orders
            .iter()
            .zip(self.credits.iter().copied())
            .zip(self.origin.iter().copied())
            .map(|((o, c), g)| (o, c, g))
    }
}

/// The office an order belongs to (V5 §5.1). Unit orders depend on the
/// unit: settlers belong to the steward, armies and scouts to the general.
/// `ConsentWar` belongs to the general and the steward, `ConsentSpend` to
/// any office but the diplomat, reveals to every office.
pub fn role_allows(state: &WorldState, role: Role, order: &Order) -> bool {
    let settler = |u: &UnitId| {
        state
            .units
            .get(*u as usize)
            .is_some_and(|x| x.unit_type == UnitType::Settler)
    };
    match order {
        Order::MoveUnit { unit, .. }
        | Order::SetStanding {
            target: StandingTarget::Unit(unit),
            ..
        } => {
            role == if settler(unit) {
                Role::Steward
            } else {
                Role::General
            }
        }
        _ => role_allows_static(role, order),
    }
}

/// Split one civ's mixed orders by office (V5 §5.1), keeping their order.
/// Reveals go to the general. For an agent (or a bot) that plans several
/// offices at once.
pub fn split_by_office(state: &WorldState, orders: Vec<Order>) -> [Vec<Order>; 4] {
    let mut out: [Vec<Order>; 4] = Default::default();
    for o in orders {
        let role = Role::ALL
            .into_iter()
            .find(|r| role_allows(state, *r, &o))
            .unwrap_or(Role::General);
        out[role.index()].push(o);
    }
    out
}

/// One batch per office for `civ`, from mixed orders, issued by the
/// current office holders (`NOBODY` for vacant offices, which the caretaker
/// fills instead: `tick::intake` ignores their batches). Every
/// `DeclareWar`/`BreakNap` gets a `ConsentWar` in the general's batch, as
/// when one agent runs the offices involved. Offices with no orders are
/// omitted.
pub fn office_batches(
    state: &WorldState,
    civ: CivId,
    digest: [u8; 32],
    orders: Vec<Order>,
) -> Vec<OrderBatch> {
    let mut parts = split_by_office(state, orders);
    let wars: Vec<CivId> = parts[Role::Diplomat.index()]
        .iter()
        .filter_map(Order::war_target)
        .collect();
    parts[Role::General.index()].extend(wars.into_iter().map(|civ| Order::ConsentWar { civ }));
    let holders = state
        .nations
        .get(civ as usize)
        .map_or([NOBODY; 4], |n| n.offices);
    Role::ALL
        .into_iter()
        .zip(parts)
        .filter(|(_, o)| !o.is_empty())
        .map(|(role, orders)| OrderBatch {
            civ,
            tick: state.tick,
            role,
            member: holders[role.index()],
            decision_digest: digest,
            orders,
            adopt: Vec::new(),
        })
        .collect()
}

/// `role_allows` without the world: unit orders are accepted from the
/// general or the steward (the engine decides by unit type). Used by the
/// chain program at submission.
pub fn role_allows_static(role: Role, order: &Order) -> bool {
    use Role::*;
    match order {
        Order::MoveUnit { .. }
        | Order::SetStanding {
            target: StandingTarget::Unit(_),
            ..
        } => matches!(role, General | Steward),
        Order::Attack { .. } | Order::Raze { .. } => role == General,
        Order::FoundCity { .. }
        | Order::SetQueue { .. }
        | Order::SetFocus { .. }
        | Order::Purchase { .. }
        | Order::SetStanding {
            target: StandingTarget::City(_),
            ..
        } => role == Steward,
        Order::SetResearch { .. } => role == Science,
        Order::DeclareWar { .. }
        | Order::ProposePeace { .. }
        | Order::AcceptPeace { .. }
        | Order::ProposeNap { .. }
        | Order::AcceptNap { .. }
        | Order::BreakNap { .. }
        | Order::ProposeAlliance { .. }
        | Order::AcceptAlliance { .. }
        | Order::LeaveAlliance
        | Order::SendEnvoy { .. }
        | Order::Transfer { .. }
        | Order::MarketTrade { .. }
        | Order::ExchangeOrder { .. }
        | Order::OfferContract { .. }
        | Order::AcceptContract { .. }
        | Order::CancelContract { .. } => role == Diplomat,
        Order::ConsentWar { .. } => matches!(role, General | Steward),
        Order::ConsentSpend { .. } => role != Diplomat,
        Order::RevealRationale { .. } => true,
    }
}

pub const MAX_QUEUE: usize = 3;
pub const MAX_RESEARCH_QUEUE: usize = 3;

/// Spendable orders of one office this tick: its share of the nation
/// budget plus its bank (§4.1, V5 §5.2). The budget was fixed when the
/// previous tick committed (cities counted at the start of this tick).
pub fn spendable(state: &WorldState, rules: &Ruleset, civ: CivId, role: Role) -> u32 {
    match (
        state.civs.get(civ as usize),
        state.nations.get(civ as usize),
    ) {
        (Some(c), Some(n)) => {
            rules.role_budget(c.tick_budget, role.index()) as u32 + n.role_bank[role.index()] as u32
        }
        _ => 0,
    }
}

/// The orders a batch runs: its own, then those of the proposals it adopts.
/// Fails if an adopted proposal does not exist for this civ and office.
pub fn batch_orders(
    state: &WorldState,
    batch: &OrderBatch,
) -> Result<Vec<(Order, Credit)>, RulesError> {
    let own = Credit::officer(batch.member);
    let mut out: Vec<(Order, Credit)> = batch.orders.iter().map(|o| (o.clone(), own)).collect();
    let nation = state
        .nations
        .get(batch.civ as usize)
        .ok_or(RulesError::UnknownCiv(batch.civ))?;
    for (i, id) in batch.adopt.iter().enumerate() {
        if batch.adopt[..i].contains(id) {
            return Err(RulesError::UnknownProposal(*id));
        }
        let p = nation
            .proposal(*id)
            .filter(|p| p.role == batch.role && !p.adopted)
            .ok_or(RulesError::UnknownProposal(*id))?;
        let credit = Credit {
            officer: batch.member,
            proposer: p.proposer,
        };
        out.extend(p.orders.iter().map(|o| (o.clone(), credit)));
    }
    Ok(out)
}

/// Validation of one office's batch against the world (the engine runs it
/// in phase 0; the chain program runs the parts it can without the world at
/// submission). Returns the cost.
pub fn validate_batch(
    state: &WorldState,
    rules: &Ruleset,
    batch: &OrderBatch,
) -> Result<u32, RulesError> {
    if state.tick >= rules.ticks_per_season {
        return Err(RulesError::SeasonOver);
    }
    if batch.tick != state.tick {
        return Err(RulesError::WrongTick {
            expected: state.tick,
            got: batch.tick,
        });
    }
    let nation = state
        .nations
        .get(batch.civ as usize)
        .ok_or(RulesError::UnknownCiv(batch.civ))?;
    if nation.holder(batch.role) != batch.member {
        return Err(RulesError::NotOfficer);
    }
    if batch.member != NOBODY && batch.decision_digest == [0; 32] {
        return Err(RulesError::MissingRationale);
    }
    let orders: Vec<Order> = batch_orders(state, batch)?
        .into_iter()
        .map(|(o, _)| o)
        .collect();
    if let Some(i) = orders
        .iter()
        .position(|o| !role_allows_static(batch.role, o))
    {
        return Err(RulesError::WrongOffice(i as u16));
    }
    let cost = check_structure(rules, state.tick, &orders)?;
    let spendable = spendable(state, rules, batch.civ, batch.role);
    if cost > spendable {
        return Err(RulesError::OverBudget { cost, spendable });
    }
    Ok(cost)
}

/// The checks on a batch that need no world state (§4.2): freezes, list
/// lengths, one manual order per unit, reveal timing. Returns the order cost.
/// `validate_batch` runs these plus the tick and budget; the on-chain
/// `RevealOrders` runs them to reject malformed batches cheaply.
pub fn check_structure(
    rules: &Ruleset,
    open_tick: u16,
    orders: &[Order],
) -> Result<u32, RulesError> {
    let mut seen: Vec<UnitId> = Vec::new();
    let mut cost = 0u32;
    for order in orders {
        match order {
            Order::Transfer { .. } if open_tick >= rules.transfer_freeze_tick => {
                return Err(RulesError::Frozen)
            }
            Order::ExchangeOrder { .. }
            | Order::ConsentSpend { .. }
            | Order::OfferContract { .. }
                if open_tick >= rules.exchange_freeze_tick || !rules.market_enabled =>
            {
                return Err(RulesError::Frozen)
            }
            Order::SetQueue { items, .. } if items.len() > MAX_QUEUE => {
                return Err(RulesError::TooLong)
            }
            Order::SetResearch { techs } if techs.len() > MAX_RESEARCH_QUEUE => {
                return Err(RulesError::TooLong)
            }
            Order::MoveUnit { path, .. } if path.len() > rules.max_path_len as usize => {
                return Err(RulesError::TooLong)
            }
            Order::SetStanding {
                rule: StandingOrder::Patrol { route },
                ..
            } if route.len() > MAX_PATROL => return Err(RulesError::TooLong),
            Order::RevealRationale { policy, text, .. }
                if policy.len() > crate::decision::MAX_POLICY
                    || text.len() > crate::decision::MAX_RATIONALE =>
            {
                return Err(RulesError::TooLong)
            }
            // Only a decision whose tick has resolved can be opened.
            Order::RevealRationale { tick, .. } if *tick >= open_tick => {
                return Err(RulesError::RevealTooEarly { tick: *tick })
            }
            _ => {}
        }
        if let Some(unit) = order.commanded_unit() {
            if seen.contains(&unit) {
                return Err(RulesError::DuplicateUnitOrder(unit));
            }
            seen.push(unit);
        }
        cost += order.cost();
    }
    Ok(cost)
}

/// Bank update after a tick: unused budget accrues up to `bank_ticks × budget`.
/// Spending draws from this tick's budget first, then from the bank.
pub fn next_bank(rules: &Ruleset, bank: u16, budget: u16, spent: u32) -> u16 {
    let total = budget as u32 + bank as u32;
    let left = total.saturating_sub(spent);
    left.min(rules.bank_ticks as u32 * budget as u32) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::Preset;

    #[test]
    fn bank_accrues_and_caps() {
        let r = Ruleset::new(Preset::Blitz);
        assert_eq!(next_bank(&r, 0, 4, 0), 4);
        assert_eq!(next_bank(&r, 4, 4, 2), 6);
        assert_eq!(next_bank(&r, 16, 4, 0), 16); // cap = 4 ticks × 4
        assert_eq!(next_bank(&r, 16, 4, 20), 0); // burst spend
    }

    #[test]
    fn exchange_and_reveal_are_free() {
        let o = Order::ExchangeOrder {
            good: Good::Iron,
            side: Side::Buy,
            amount: 1,
            price: 1,
        };
        assert_eq!(o.cost(), 0);
        assert_eq!(Order::LeaveAlliance.cost(), 1);
    }
}
