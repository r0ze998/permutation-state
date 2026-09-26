//! CommitOrders, CloseCommits, RevealOrders, SubmitGov, LogTickInput,
//! ResolveTick, AnchorTalk, SubmitOrders (`tests/play.rs`, `tests/play_gate.rs`).

use super::{Chain, Cover, Lands};
use crate::error::ChainError as E;

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
        Lands("close_ix("),
    ],
)];

pub const REVEAL_ORDERS: &[Cover] = &[Cover::Test(
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
)];

pub const SUBMIT_GOV: &[Cover] = &[Cover::Test(
    "play::submit_gov_checks",
    &[
        Chain(E::WrongStatus),
        Chain(E::TickFrozen),
        Chain(E::InboxFull),
        Lands("submit_gov_ix("),
    ],
)];

pub const LOG_TICK_INPUT: &[Cover] = &[Cover::Test(
    "play::log_tick_input_checks",
    &[
        Chain(E::InputNotPublished),
        Chain(E::WrongPhase),
        Chain(E::TooEarly),
        Chain(E::InvalidParams),
        Chain(E::WrongStatus),
        Lands("log_ix("),
    ],
)];

pub const RESOLVE_TICK: &[Cover] = &[Cover::Test(
    "play::resolve_tick_checks",
    &[
        Chain(E::InputNotPublished),
        Chain(E::WrongStatus),
        Chain(E::WrongWorld),
        Lands("resolve_ix("),
    ],
)];

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
pub const PENDING: &[(&str, &str)] = &[
    ("play_gate::seating_attacks_fail", "WP01"),
    ("budget::inbox_full_quota_log_input", "WP03/WP07"),
];
