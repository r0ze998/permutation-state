//! Errors returned by rule validation and resolution.

use core::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RulesError {
    /// Batch submitted for a tick other than the open one.
    WrongTick {
        expected: u16,
        got: u16,
    },
    /// The season has already resolved its last tick.
    SeasonOver,
    UnknownCiv(u16),
    UnknownUnit(u32),
    UnknownCity(u32),
    NotOwner,
    /// Orders cost more than `budget + bank` (§4.1).
    OverBudget {
        cost: u32,
        spendable: u32,
    },
    /// More than one manual order for the same unit in one tick (§4.2).
    DuplicateUnitOrder(u32),
    /// Transfers frozen from tick 162, Exchange from tick 120 (§1.1).
    Frozen,
    /// A list in an order exceeds its maximum length.
    TooLong,
    MapGeneration(&'static str),
    /// More civilizations for one payout wallet than allowed (§3.1).
    WalletCap,
    /// `resolve_tick` phases must run in order (§15.1).
    PhaseOutOfOrder {
        expected: u8,
        got: u8,
    },
    Serialization,
    /// A rationale may only be revealed after its tick resolved (§4.3).
    RevealTooEarly {
        tick: u16,
    },
}

impl fmt::Display for RulesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RulesError::WrongTick { expected, got } => {
                write!(f, "orders for tick {got}, but tick {expected} is open")
            }
            RulesError::SeasonOver => write!(f, "season is over"),
            RulesError::UnknownCiv(id) => write!(f, "unknown civilization {id}"),
            RulesError::UnknownUnit(id) => write!(f, "unknown unit {id}"),
            RulesError::UnknownCity(id) => write!(f, "unknown city {id}"),
            RulesError::NotOwner => write!(f, "entity is not owned by this civilization"),
            RulesError::OverBudget { cost, spendable } => {
                write!(f, "orders cost {cost}, only {spendable} spendable")
            }
            RulesError::DuplicateUnitOrder(id) => {
                write!(f, "more than one manual order for unit {id}")
            }
            RulesError::Frozen => write!(f, "this action is frozen in the current phase"),
            RulesError::TooLong => write!(f, "list exceeds its maximum length"),
            RulesError::MapGeneration(why) => write!(f, "map generation failed: {why}"),
            RulesError::WalletCap => write!(f, "too many civilizations for one payout wallet"),
            RulesError::PhaseOutOfOrder { expected, got } => {
                write!(f, "phase {got} requested, phase {expected} is next")
            }
            RulesError::Serialization => write!(f, "state serialization failed"),
            RulesError::RevealTooEarly { tick } => {
                write!(f, "the decision for tick {tick} has not resolved yet")
            }
        }
    }
}
