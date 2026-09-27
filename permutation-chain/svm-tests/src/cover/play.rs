//! StartClock, CommitOrders, CloseCommits, RevealOrders, SubmitGov,
//! FreezeTick, ConsumeTickRandomness, RetryTickRandomness, LogTickInput,
//! ResolveTick, AnchorTalk, SubmitOrders (`tests/play.rs`,
//! `tests/tick_randomness.rs`, `tests/batch_caps.rs`, `tests/play_gate.rs`).

use super::{Chain, Cover, Lands};
use crate::error::ChainError as E;

pub const START_CLOCK: &[Cover] = &[Cover::Test(
    "play::start_clock_checks",
    &[
        Chain(E::Unauthorized),
        Chain(E::MissingSignature),
        Chain(E::WrongPhase),
        Chain(E::WrongTick),
        Lands("start_clock_ix("),
    ],
)];

pub const COMMIT_ORDERS: &[Cover] = &[Cover::Test(
    "play::commit_orders_checks",
    &[
        Chain(E::Unauthorized),
        Chain(E::WrongTick),
        Chain(E::InvalidParams),
        Chain(E::WrongPhase),
        Chain(E::TickFrozen),
        Lands("commit_orders_ix("),
    ],
)];

pub const CLOSE_COMMITS: &[Cover] = &[Cover::Test(
    "play::close_commits_checks",
    &[
        Chain(E::TooEarly),
        Chain(E::WrongPhase),
        Chain(E::MissingNation),
        Chain(E::NotAlone),
        Chain(E::WrongStatus),
        Chain(E::WrongTick),
        Lands("close_ix("),
    ],
)];

pub const REVEAL_ORDERS: &[Cover] = &[
    Cover::Test(
        "play::reveal_orders_checks",
        &[
            Chain(E::CommitMismatch),
            Chain(E::WrongPhase),
            Chain(E::TickFrozen),
            Chain(E::Rules),
            Chain(E::WrongOffice),
            Chain(E::OverBudget),
            Chain(E::WrongTick),
            Lands("reveal_orders_ix("),
        ],
    ),
    Cover::Test(
        "batch_caps::reveals_are_capped",
        &[Chain(E::OverBudget), Lands("reveal_in_fork(")],
    ),
];

pub const SUBMIT_GOV: &[Cover] = &[Cover::Test(
    "play::submit_gov_checks",
    &[
        Chain(E::WrongStatus),
        Chain(E::Unauthorized),
        Chain(E::InvalidParams),
        Chain(E::InboxFull),
        Chain(E::TickFrozen),
        Lands("submit_gov_ix("),
    ],
)];

pub const FREEZE_TICK: &[Cover] = &[
    Cover::Test(
        "tick_randomness::freeze_tick_checks",
        &[
            Chain(E::WrongPhase),
            Chain(E::TooEarly),
            Chain(E::MissingSignature),
            Chain(E::WrongTick),
            Lands("freeze_ix("),
        ],
    ),
    Cover::Test(
        "tick_randomness::oracle_accounts_are_checked",
        &[Chain(E::WrongOracle), Lands("freeze_ix(")],
    ),
];

pub const CONSUME_TICK_RANDOMNESS: &[Cover] = &[Cover::Test(
    "tick_randomness::callback_authenticated",
    &[
        Chain(E::WrongOracle),
        Chain(E::WrongWorld),
        Lands("fulfil("),
    ],
)];

pub const RETRY_TICK_RANDOMNESS: &[Cover] = &[Cover::Test(
    "tick_randomness::retry_tick_randomness_checks",
    &[
        Chain(E::WrongPhase),
        Chain(E::WrongOracle),
        Chain(E::TooEarly),
        Lands("retry_ix("),
    ],
)];

pub const LOG_TICK_INPUT: &[Cover] = &[Cover::Test(
    "play::log_tick_input_checks",
    &[
        Chain(E::WrongPhase),
        Chain(E::RandomnessPending),
        Chain(E::NotAlone),
        Chain(E::InvalidParams),
        Chain(E::WrongStatus),
        Lands("log_ix("),
    ],
)];

pub const RESOLVE_TICK: &[Cover] = &[
    Cover::Test(
        "play::resolve_tick_checks",
        &[
            Chain(E::InputNotPublished),
            Chain(E::NotAlone),
            Chain(E::RandomnessPending),
            Chain(E::RulesMismatch),
            Chain(E::WrongWorld),
            Chain(E::WrongTick),
            Chain(E::WrongStatus),
            Lands("resolve_ix("),
        ],
    ),
    Cover::Test(
        "batch_caps::degraded_step_needs_the_timer",
        &[Chain(E::TooEarly), Lands("resolve_ix(")],
    ),
];

pub const ANCHOR_TALK: &[Cover] = &[Cover::Test(
    "play::anchor_talk_checks",
    &[
        Chain(E::Unauthorized),
        Chain(E::MissingSignature),
        Lands("anchor_talk_ix("),
    ],
)];

/// Retired: always refused, no positive path.
pub const SUBMIT_ORDERS: &[Cover] = &[Cover::Test(
    "play::submit_orders_is_retired",
    &[Chain(E::Retired)],
)];

/// Ignored tests of this area waiting for their fix: (test, WP).
pub const PENDING: &[(&str, &str)] = &[];
