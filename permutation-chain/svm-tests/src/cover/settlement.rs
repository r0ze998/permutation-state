//! FinishSeason, Claim, WithdrawOps (`tests/settlement.rs`, `tests/solvency.rs`;
//! FinishSeason's roster branches in `tests/roster.rs`).

use super::{Chain, Cover, Lands, Program};
use crate::error::ChainError as E;

pub const FINISH_SEASON: &[Cover] = &[
    Cover::Test(
        "settlement::finish_season_checks",
        &[
            Chain(E::WrongStatus),
            Chain(E::SeasonNotOver),
            Chain(E::WrongWorld),
            Chain(E::WorldRolledBack),
            Chain(E::WrongPda),
            Program("NotEnoughAccountKeys"),
            Lands("finish_ix("),
        ],
    ),
    Cover::Test(
        "settlement::a_mixed_world_is_refused",
        &[Chain(E::WrongWorld)],
    ),
    Cover::Test(
        "settlement::finish_season_refuses_a_world_under_other_rules",
        &[Chain(E::RulesMismatch), Lands("finish_ix(")],
    ),
    Cover::Test(
        "settlement::finish_season_refuses_an_underfunded_vault",
        &[Chain(E::Insolvent), Lands("finish_ix(")],
    ),
    Cover::Test(
        "roster::finish_season_roster_branches",
        &[Chain(E::RosterPending), Lands("finish_ix(")],
    ),
    Cover::Test(
        "solvency::voided_season_refunds_paid_in",
        &[Lands("finish_ix(")],
    ),
];

pub const CLAIM: &[Cover] = &[
    Cover::Test(
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
            Chain(E::Insolvent),
            Lands("claim_ix("),
        ],
    ),
    Cover::Test(
        "solvency::legacy_v7_season_still_claims",
        &[Lands("claim_ix(")],
    ),
    Cover::Test(
        "escape_hatches::a_frozen_seating_season_refunds_everyone",
        &[Chain(E::AlreadyClaimed), Lands("claim_ix(")],
    ),
];

pub const WITHDRAW_OPS: &[Cover] = &[
    Cover::Test(
        "settlement::withdraw_ops_checks",
        &[
            Chain(E::WrongStatus),
            Chain(E::Unauthorized),
            Chain(E::MissingSignature),
            Chain(E::WrongTokenAccount),
            Chain(E::WrongMint),
            Chain(E::WrongPda),
            Chain(E::Insolvent),
            Lands("withdraw_ix("),
        ],
    ),
    Cover::Test(
        "solvency::legacy_v7_season_still_claims",
        &[Lands("withdraw_ix(")],
    ),
];

/// Ignored tests of this area waiting for their fix: (test, WP).
pub const PENDING: &[(&str, &str)] = &[];
