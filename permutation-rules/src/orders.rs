//! Orders, costs and submit-time validation (§4).
//!
//! Submission checks only what cannot change before resolution: the tick,
//! the budget, freezes and per-unit uniqueness. Everything state-dependent is
//! re-checked at resolution, and an order that has become invalid is dropped
//! without refund (§4.1).

use crate::buildings::Building;
use crate::hex::Hex;
use crate::params::Ruleset;
use crate::state::{CityId, CivId, Focus, QueueItem, UnitId, WorldState};
use crate::tech::Tech;
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
    AutoDefend { radius: u8 },
    Retreat { ratio_bps: u32 },
    /// Waypoints, 1..=6.
    Patrol { route: Vec<Hex> },
    QueueRepeat { on: bool },
    AutoPurchase { max_gold: u32 },
}

pub const MAX_PATROL: usize = 6;

impl Order {
    /// Budget cost (§4.2).
    pub const fn cost(&self) -> u32 {
        match self {
            Order::ExchangeOrder { .. } | Order::RevealRationale { .. } => 0,
            _ => 1,
        }
    }

    /// The unit this order commands, for the one-manual-order-per-unit rule.
    pub fn commanded_unit(&self) -> Option<UnitId> {
        match self {
            Order::MoveUnit { unit, .. } => Some(*unit),
            Order::SetStanding { target: StandingTarget::Unit(unit), .. } => Some(*unit),
            Order::Attack { army, .. } => Some(*army),
            Order::FoundCity { settler } => Some(*settler),
            _ => None,
        }
    }
}

/// One civilization's orders for one tick (§4.3).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct OrderBatch {
    pub civ: CivId,
    pub tick: u16,
    /// `sha256(tick ‖ obs_root ‖ policy_id ‖ rationale_hash)`, stored, never interpreted.
    pub decision_digest: [u8; 32],
    pub orders: Vec<Order>,
}

pub const MAX_QUEUE: usize = 3;
pub const MAX_RESEARCH_QUEUE: usize = 3;

/// Spendable orders this tick: `budget + bank` (§4.1). The budget was fixed
/// when the previous tick committed (cities counted at the start of this tick).
pub fn spendable(state: &WorldState, civ: CivId) -> u32 {
    state
        .civs
        .get(civ as usize)
        .map_or(0, |c| c.tick_budget as u32 + c.order_bank as u32)
}

/// Submit-time validation, run by `submit_orders` on chain.
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
    if batch.civ as usize >= state.civs.len() {
        return Err(RulesError::UnknownCiv(batch.civ));
    }
    let mut seen: Vec<UnitId> = Vec::new();
    let mut cost = 0u32;
    for order in &batch.orders {
        match order {
            Order::Transfer { .. } if state.tick >= rules.transfer_freeze_tick => {
                return Err(RulesError::Frozen)
            }
            Order::ExchangeOrder { .. } if state.tick >= rules.exchange_freeze_tick => {
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
            Order::SetStanding { rule: StandingOrder::Patrol { route }, .. } if route.len() > MAX_PATROL => {
                return Err(RulesError::TooLong)
            }
            Order::RevealRationale { policy, text, .. }
                if policy.len() > crate::decision::MAX_POLICY || text.len() > crate::decision::MAX_RATIONALE =>
            {
                return Err(RulesError::TooLong)
            }
            // Only a decision whose tick has resolved can be opened.
            Order::RevealRationale { tick, .. } if *tick >= state.tick => {
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
    let spendable = spendable(state, batch.civ);
    if cost > spendable {
        return Err(RulesError::OverBudget { cost, spendable });
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

/// Building queue items require their tech; exposed for clients and resolution.
pub fn building_unlocked(techs: crate::tech::TechSet, b: Building) -> bool {
    crate::buildings::info(b).tech.is_none_or(|t| techs.has(t))
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
