//! StartSeason, RetrySeasonSeed, ConsumeSeasonSeed, GenesisStep,
//! SeatMembers, OpenGovernment (`tests/genesis.rs`, `season_seed_vrf.rs`,
//! `genesis_every_creatable.rs`, `upgrade_trust.rs`).

use super::{Chain, Cover, Lands};
use crate::error::ChainError as E;

/// A season created under other rules or logic (WP15): every seating step.
const RULES: Cover = Cover::Test(
    "upgrade_trust::another_rules_hash_or_logic_halts_the_season_but_not_the_claims",
    &[Chain(E::RulesMismatch)],
);

pub const START_SEASON: &[Cover] = &[
    Cover::Test(
        "genesis::start_season_checks",
        &[
            Chain(E::WrongStatus),
            Chain(E::Unauthorized),
            Chain(E::WrongOracle),
            Lands("start_ix("),
        ],
    ),
    Cover::Test(
        "genesis::start_season_needs_the_bond_floor",
        &[Chain(E::BondTooSmall), Lands("start_ix(")],
    ),
    Cover::Test(
        "genesis_every_creatable::start_season_refuses_uncreatable",
        &[Chain(E::InvalidParams)],
    ),
    RULES,
];

pub const RETRY_SEASON_SEED: &[Cover] = &[Cover::Test(
    "season_seed_vrf::retry_season_seed_requests_on_the_base_queue",
    &[
        Chain(E::WrongOracle),
        Chain(E::TooEarly),
        Chain(E::WrongStatus),
        Lands("retry_seed_ix("),
    ],
)];

pub const CONSUME_SEASON_SEED: &[Cover] = &[Cover::Test(
    "season_seed_vrf::consume_season_seed_is_the_oracles",
    &[Chain(E::WrongOracle), Chain(E::WrongPda), Lands("fulfil(")],
)];

pub const GENESIS_STEP: &[Cover] = &[
    Cover::Test(
        "genesis::genesis_step_reaches_seating",
        &[
            Chain(E::WrongStatus),
            Chain(E::WrongPda),
            Chain(E::WorldTooSmall),
            Chain(E::WrongWorld),
            Lands("genesis_step_ix("),
        ],
    ),
    Cover::Test(
        "genesis_every_creatable::genesis_work_is_clamped",
        &[Lands("genesis_step_ix(")],
    ),
    RULES,
];

pub const SEAT_MEMBERS: &[Cover] = &[
    Cover::Test(
        "genesis::seat_members_checks",
        &[
            Chain(E::Unauthorized),
            Chain(E::InvalidParams),
            Chain(E::NotInitialized),
            Chain(E::WrongStatus),
            Lands("seat_ix("),
        ],
    ),
    Cover::Test(
        "genesis::an_outsider_takes_over_seating",
        &[Chain(E::Unauthorized), Lands("seat_ix(")],
    ),
    RULES,
];

pub const OPEN_GOVERNMENT: &[Cover] = &[
    Cover::Test(
        "genesis::open_government_checks",
        &[
            Chain(E::Unauthorized),
            Chain(E::WrongStatus),
            Chain(E::MissingNation),
            Lands("open_ix("),
        ],
    ),
    Cover::Test(
        "genesis::an_outsider_takes_over_seating",
        &[Lands("open_ix(")],
    ),
    RULES,
];

/// Ignored tests of this area waiting for their fix: (test, WP).
pub const PENDING: &[(&str, &str)] = &[];
