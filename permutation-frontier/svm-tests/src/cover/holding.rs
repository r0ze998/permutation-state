//! Coverage of the holding area (§5.10): Harvest, Build, Train,
//! SettleExplore (W3-B). Explore is listed in `host.rs` (the registry's
//! grouping); its tests live in `tests/holding.rs`.
//!
//! The player-prologue codes (WrongStatus, RulesetMismatch, Bucket,
//! SessionExpired, Auth, BadAddress of the Citizen, TooManyAccounts) run
//! through one shared path (`proc::holding::player`); W5-A asserts them
//! per instruction (`g13_complete::prologue_rows`).

use super::{Cover, Err, Lands, Loaded, E};

/// Ignored tests of this area waiting for a fix: (test, unit).
pub const PENDING: &[(&str, &str)] = &[];

pub const HARVEST: &[Cover] = &[
    Cover::Test(
        "holding::holding_harvest_settles_the_stores",
        &[Lands("hx::harvest("), Err(E::NotFinal)],
    ),
    Cover::Test(
        "g13_complete::g13_harvest_prologue_and_loaded",
        &[
            Err(E::WrongStatus),
            Err(E::RulesetMismatch),
            Err(E::Bucket),
            Err(E::SessionExpired),
            Err(E::Auth),
            Err(E::BadAddress),
            Err(E::TooManyAccounts),
            Loaded,
        ],
    ),
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
    Cover::Test(
        "g13_complete::g13_build_prologue_and_loaded",
        &[
            Err(E::WrongStatus),
            Err(E::RulesetMismatch),
            Err(E::Bucket),
            Err(E::SessionExpired),
            Err(E::Auth),
            Err(E::BadAddress),
            Err(E::TooManyAccounts),
            Loaded,
        ],
    ),
];

pub const TRAIN: &[Cover] = &[
    Cover::Test(
        "holding::holding_train_is_immediate",
        &[Lands("hx::train("), Err(E::BadData), Err(E::Insufficient)],
    ),
    Cover::Test(
        "g13_complete::g13_train_prologue_and_loaded",
        &[
            Err(E::WrongStatus),
            Err(E::RulesetMismatch),
            Err(E::Bucket),
            Err(E::SessionExpired),
            Err(E::Auth),
            Err(E::BadAddress),
            Err(E::TooManyAccounts),
            Loaded,
        ],
    ),
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
    Cover::Test(
        "g13_complete::g13_explore_and_settle_explore",
        &[
            Err(E::WrongStatus),
            Err(E::BadAddress),
            Err(E::SeedNotReady),
            Loaded,
            Lands("SettleExplore"),
        ],
    ),
];
