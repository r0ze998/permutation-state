//! Delegate, the undelegate callback, Commit, CommitAndUndelegate,
//! CommitPart, UndelegatePart (`tests/delegation.rs`).

use super::{Chain, Cover, Lands, Program};
use crate::error::ChainError as E;

pub const DELEGATE: &[Cover] = &[Cover::Test(
    "delegation::delegate_checks_and_happy_path",
    &[
        Chain(E::WrongStatus),
        Chain(E::Unauthorized),
        Chain(E::WrongDelegationProgram),
        Program("IncorrectProgramId"),
        Chain(E::InvalidParams),
        Lands("delegate_ix("),
    ],
)];

pub const CALLBACK: &[Cover] = &[Cover::Test(
    "delegation::undelegate_callback_restores_the_account",
    &[
        Chain(E::WrongDelegationProgram),
        Chain(E::WrongPda),
        Program("MissingRequiredSignature"),
        Chain(E::InvalidInstruction),
        Lands("undelegate_callback("),
    ],
)];

/// Retired (WP02): always `Retired`.
pub const COMMIT: &[Cover] = &[Cover::Test(
    "delegation::whole_world_commits_are_retired",
    &[Chain(E::Retired)],
)];

pub const COMMIT_AND_UNDELEGATE: &[Cover] = &[Cover::Test(
    "delegation::whole_world_commits_are_retired",
    &[Chain(E::Retired)],
)];

pub const COMMIT_PART: &[Cover] = &[Cover::Test(
    "delegation::commit_part_checks",
    &[
        Chain(E::Unauthorized),
        Chain(E::WrongMagicProgram),
        Chain(E::InvalidParams),
        Chain(E::WrongWorld),
        Lands("part_ix("),
    ],
)];

pub const UNDELEGATE_PART: &[Cover] = &[Cover::Test(
    "delegation::undelegate_part_after_the_last_tick",
    &[Chain(E::SeasonNotOver), Lands("part_ix(")],
)];

/// Ignored tests of this area waiting for their fix: (test, WP).
pub const PENDING: &[(&str, &str)] = &[
    (
        "delegation::undelegation_is_anyones_in_the_crank_shape",
        "WP02",
    ),
    (
        "delegation::undelegation_intents_have_the_crank_shape",
        "WP02",
    ),
    ("budget::light_intents_at_every_target", "WP02"),
];
