//! Coverage of the holding area (§5.10): Harvest, Build, Train,
//! SettleExplore (W3-B). Explore is listed in `host.rs` (the registry's
//! grouping); its tests live in `tests/holding.rs`.
//!
//! The player-prologue codes (WrongStatus, RulesetMismatch, Bucket,
//! SessionExpired, Auth, BadAddress of the Citizen, TooManyAccounts) run
//! through one shared path (`proc::holding::player`) and are asserted
//! once, for Muster (`host::host_player_prologue_refusals`); the per-
//! instruction rows are W5-A's G13 completion.

use super::{Cover, Err, Lands, E};

/// Ignored tests of this area waiting for a fix: (test, unit).
pub const PENDING: &[(&str, &str)] = &[];

const PROLOGUE: Cover = Cover::Pending(
    "W5-A: per-instruction rows of the shared player-prologue codes (asserted for Muster)",
);

pub const HARVEST: &[Cover] = &[
    Cover::Test(
        "holding::holding_harvest_settles_the_stores",
        &[Lands("hx::harvest("), Err(E::NotFinal)],
    ),
    PROLOGUE,
];

pub const BUILD: &[Cover] = &[
    Cover::Test(
        "holding::holding_build_buildings_walls_and_tier_up",
        &[Lands("hx::build_item("), Err(E::QueueFull)],
    ),
    Cover::Test(
        "holding::holding_build_refusals",
        &[
            Err(E::BadData),
            Err(E::TooManyAccounts),
            Err(E::Insufficient),
            Err(E::Kernel),
            Err(E::QueueFull),
            Err(E::BadAddress),
            Err(E::NotOwner),
            Lands("hx::build_item("),
        ],
    ),
    PROLOGUE,
];

pub const TRAIN: &[Cover] = &[
    Cover::Test(
        "holding::holding_train_is_immediate",
        &[Lands("hx::train("), Err(E::BadData), Err(E::Insufficient)],
    ),
    PROLOGUE,
];

pub const SETTLE_EXPLORE: &[Cover] = &[
    Cover::Test(
        "holding::holding_explore_and_settle_explore",
        &[
            Lands("hx::settle_explore("),
            Err(E::SeedNotReady),
            Err(E::AlreadyDone),
        ],
    ),
    Cover::Test(
        "holding::holding_settle_explore_after_the_floor_rolls",
        &[Lands("hx::settle_explore(")],
    ),
    Cover::Pending("W5-A: WrongStatus, BadAddress (citizen, cache), SeedNotReady via the archive"),
];
