//! RevealRoster and FinishSeason's roster branches (`tests/roster.rs`).

use super::{Chain, Cover, Lands};
use crate::error::ChainError as E;

pub const REVEAL_ROSTER: &[Cover] = &[Cover::Test(
    "roster::reveal_roster_checks",
    &[
        Chain(E::WrongStatus),
        Chain(E::InvalidParams),
        Chain(E::RosterMismatch),
        Lands("reveal_ai_ix("),
    ],
)];

/// Ignored tests of this area waiting for their fix: (test, WP).
pub const PENDING: &[(&str, &str)] = &[
    (
        "roster::reveal_roster_needs_the_operator_and_the_end",
        "WP09",
    ),
    ("roster::self_reveal_cannot_break_the_roster", "WP09"),
];
