//! FinishSeason, Claim, WithdrawOps (`tests/settlement.rs`; FinishSeason's
//! roster branches in `tests/roster.rs`).

use super::{Chain, Cover, Lands};
use crate::error::ChainError as E;

pub const FINISH_SEASON: &[Cover] = &[
    Cover::Test(
        "settlement::finish_season_checks",
        &[
            Chain(E::WrongStatus),
            Chain(E::SeasonNotOver),
            Chain(E::WrongWorld),
            Lands("finish_ix("),
        ],
    ),
    Cover::Test(
        "roster::finish_season_roster_branches",
        &[Chain(E::RosterPending), Lands("finish_ix(")],
    ),
];

pub const CLAIM: &[Cover] = &[Cover::Test(
    "settlement::claim_checks",
    &[
        Chain(E::WrongStatus),
        Chain(E::Unauthorized),
        Chain(E::MissingSignature),
        Chain(E::WrongTokenAccount),
        Chain(E::WrongPda),
        Chain(E::NotInitialized),
        Chain(E::WrongMint),
        Chain(E::NothingToClaim),
        Chain(E::AlreadyClaimed),
        Lands("claim_ix("),
    ],
)];

pub const WITHDRAW_OPS: &[Cover] = &[Cover::Test(
    "settlement::withdraw_ops_checks",
    &[
        Chain(E::WrongStatus),
        Chain(E::Unauthorized),
        Chain(E::MissingSignature),
        Chain(E::WrongTokenAccount),
        Chain(E::WrongMint),
        Chain(E::WrongPda),
        Lands("withdraw_ix("),
    ],
)];

/// Ignored tests of this area waiting for their fix: (test, WP).
pub const PENDING: &[(&str, &str)] = &[];
