//! Delegate, the undelegate callback, Commit, CommitAndUndelegate,
//! CommitPart, UndelegatePart (`tests/delegation.rs`, `tests/undelegation.rs`).

use super::{Chain, Cover, Lands, Program};
use crate::error::ChainError as E;

pub const DELEGATE: &[Cover] = &[
    Cover::Test(
        "delegation::delegate_checks_and_happy_path",
        &[
            Chain(E::WrongStatus),
            Chain(E::Unauthorized),
            Chain(E::WrongDelegationProgram),
            Program("IncorrectProgramId"),
            Program("NotEnoughAccountKeys"),
            Chain(E::InvalidParams),
            Lands("delegate_ix("),
        ],
    ),
    Cover::Test(
        "delegation::delegation_guards",
        &[
            Chain(E::DelegationOrder),
            Chain(E::WrongValidator),
            Chain(E::AlreadyDelegated),
            Lands("delegate_ix("),
        ],
    ),
];

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
        Chain(E::WrongPhase),
        Lands("part_ix("),
    ],
)];

pub const UNDELEGATE_PART: &[Cover] = &[
    Cover::Test(
        "delegation::undelegate_part_after_the_last_tick",
        &[Chain(E::SeasonNotOver), Lands("part_ix(")],
    ),
    Cover::Test(
        "undelegation::adversarial_shapes_and_orders_schedule_nothing",
        &[Chain(E::InvalidParams), Chain(E::UndelegationOrder)],
    ),
    Cover::Test(
        "undelegation::one_step_per_transaction",
        &[Chain(E::NotAlone), Lands("part_ix(")],
    ),
    Cover::Test(
        "undelegation::gone_targets_are_skipped_and_the_rest_winds_up",
        &[Chain(E::WrongWorld), Chain(E::WrongPda)],
    ),
];

/// Ignored tests of this area waiting for their fix: (test, WP).
pub const PENDING: &[(&str, &str)] = &[];
