//! StartSeason, GenesisStep, SeatMembers, OpenGovernment (`tests/genesis.rs`).

use super::{Chain, Cover, Lands};
use crate::error::ChainError as E;

pub const START_SEASON: &[Cover] = &[Cover::Test(
    "genesis::start_season_checks",
    &[
        Chain(E::Unauthorized),
        Chain(E::WrongStatus),
        Chain(E::InvalidParams),
        Chain(E::WorldTooSmall),
        Chain(E::WrongWorld),
        Lands("start_ix("),
    ],
)];

pub const GENESIS_STEP: &[Cover] = &[Cover::Test(
    "genesis::genesis_step_reaches_seating",
    &[
        Chain(E::WrongStatus),
        Chain(E::WrongPda),
        Lands("genesis_step_ix("),
    ],
)];

pub const SEAT_MEMBERS: &[Cover] = &[Cover::Test(
    "genesis::seat_members_checks",
    &[
        Chain(E::Unauthorized),
        Chain(E::InvalidParams),
        Chain(E::NotInitialized),
        Chain(E::WrongStatus),
        Lands("seat_ix("),
    ],
)];

pub const OPEN_GOVERNMENT: &[Cover] = &[Cover::Test(
    "genesis::open_government_checks",
    &[
        Chain(E::Unauthorized),
        Chain(E::WrongStatus),
        Chain(E::MissingNation),
        Lands("open_ix("),
    ],
)];

/// Ignored tests of this area waiting for their fix: (test, WP).
pub const PENDING: &[(&str, &str)] = &[("budget::only_buildable_seasons_are_creatable", "WP13")];
