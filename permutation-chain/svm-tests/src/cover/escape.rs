//! Abort, RequestUndelegation, RollbackUndelegation, CloseSeasonAccounts
//! (WP14, `tests/escape_hatches.rs`).

use super::{Chain, Cover, Lands, Program};
use crate::error::ChainError as E;

pub const ABORT: &[Cover] = &[
    Cover::Test(
        "escape_hatches::a_frozen_seating_season_refunds_everyone",
        &[
            Chain(E::TooEarly),
            Chain(E::MissingSignature),
            Chain(E::WrongStatus),
            Lands("abort_ix("),
        ],
    ),
    Cover::Test(
        "escape_hatches::registering_abort_rules",
        &[
            Chain(E::TooEarly),
            Chain(E::WrongStatus),
            Lands("abort_ix("),
        ],
    ),
    Cover::Test(
        "escape_hatches::running_never_delegated_aborts_a_day_after_the_tick_deadline",
        &[
            Chain(E::TooEarly),
            Chain(E::WorldTooSmall),
            Chain(E::WrongPda),
            Lands("abort_ix("),
        ],
    ),
    Cover::Test(
        "escape_hatches::running_delegated_world_aborts_only_at_the_running_deadline",
        &[Chain(E::TooEarly), Lands("abort_ix(")],
    ),
    Cover::Test(
        "escape_hatches::a_finished_world_back_on_base_must_finish_not_abort",
        &[Chain(E::TooEarly), Lands("abort_ix(")],
    ),
];

pub const REQUEST_UNDELEGATION: &[Cover] = &[
    Cover::Test(
        "escape_hatches::request_undelegation_gates",
        &[
            Chain(E::WrongStatus),
            Chain(E::InvalidParams),
            Chain(E::Unauthorized),
            Chain(E::WrongPda),
            Chain(E::WrongDelegationProgram),
            Program("IncorrectProgramId"),
            Lands("request_undelegation_ix("),
        ],
    ),
    Cover::Test(
        "escape_hatches::request_undelegation_while_running_is_too_early_before_the_running_deadline",
        &[Chain(E::TooEarly), Lands("request_undelegation_ix(")],
    ),
];

pub const ROLLBACK_UNDELEGATION: &[Cover] = &[Cover::Test(
    "escape_hatches::rollback_restores_the_saved_bytes_and_blocks_finish",
    &[
        Chain(E::WrongDelegationProgram),
        Program("IncorrectProgramId"),
        Chain(E::WorldRolledBack),
        Lands("rollback_ix("),
    ],
)];

pub const CLOSE_SEASON_ACCOUNTS: &[Cover] = &[Cover::Test(
    "escape_hatches::close_season_accounts_gates",
    &[
        Chain(E::WrongStatus),
        Chain(E::Unauthorized),
        Chain(E::InvalidParams),
        Chain(E::WrongPda),
        Chain(E::WrongWorld),
        Lands("close_accounts_ix("),
    ],
)];

/// Ignored tests of this area waiting for their fix: (test, WP).
pub const PENDING: &[(&str, &str)] = &[(
    "escape_hatches::a_closed_account_is_not_re_allocated",
    "WP14 (unit P2)",
)];
