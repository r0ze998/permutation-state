//! RevealRoster and FinishSeason's roster branches (`tests/roster.rs`).

use super::{Chain, Cover, Lands};
use crate::error::ChainError as E;

pub const REVEAL_ROSTER: &[Cover] = &[
    Cover::Test(
        "roster::reveal_roster_checks",
        &[
            Chain(E::WrongStatus),
            Chain(E::SeasonNotOver),
            Chain(E::Unauthorized),
            Chain(E::MissingSignature),
            Chain(E::WrongWorld),
            Chain(E::WrongPda),
            Chain(E::InvalidParams),
            Chain(E::RosterMismatch),
            Lands("reveal_ai_ix("),
        ],
    ),
    Cover::Test(
        "roster::a_wrong_order_or_blind_is_redone_from_zero",
        &[Chain(E::RosterMismatch), Lands("reveal_roster_ix(")],
    ),
    Cover::Test(
        "roster::reveal_roster_needs_the_operator_and_the_end",
        &[
            Chain(E::Unauthorized),
            Chain(E::SeasonNotOver),
            Lands("reveal_ai_ix("),
        ],
    ),
];

/// Ignored tests of this area waiting for their fix: (test, WP).
pub const PENDING: &[(&str, &str)] = &[];
