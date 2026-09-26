//! CreateSeason, AllocWorld, AllocNation, Register, UpdateMember
//! (`tests/registration.rs`).

use super::{Chain, Cover, Lands, Program, Token};
use crate::error::ChainError as E;

pub const CREATE_SEASON: &[Cover] = &[
    Cover::Test(
        "registration::create_season_lands_with_its_vault",
        &[Lands("create_ix(")],
    ),
    Cover::Test(
        "registration::create_season_with_ais_escrows_into_the_vault",
        &[Lands("create_ai_ix(")],
    ),
    Cover::Test(
        "registration::create_season_checks",
        &[
            Chain(E::MissingSignature),
            Chain(E::InvalidParams),
            Chain(E::WrongMint),
            Program("IncorrectProgramId"),
            Chain(E::WrongPda),
            Chain(E::AlreadyInitialized),
            Token,
            Lands("create_ix_from("),
        ],
    ),
];

pub const ALLOC_WORLD: &[Cover] = &[Cover::Test(
    "registration::alloc_world_checks",
    &[
        Chain(E::InvalidParams),
        Chain(E::WrongPda),
        Chain(E::AlreadyInitialized),
        Chain(E::MissingSignature),
        Chain(E::NotInitialized),
        Lands("alloc_world_ix("),
    ],
)];

pub const ALLOC_NATION: &[Cover] = &[Cover::Test(
    "registration::alloc_nation_checks",
    &[
        Chain(E::InvalidParams),
        Chain(E::WrongPda),
        Chain(E::AlreadyInitialized),
        Lands("alloc_nation_ix("),
    ],
)];

pub const REGISTER: &[Cover] = &[
    Cover::Test(
        "registration::register_lands_and_splits_the_fee",
        &[Lands("register_ix(")],
    ),
    Cover::Test(
        "registration::register_checks",
        &[
            Chain(E::MissingSignature),
            Chain(E::WrongStatus),
            Chain(E::InvalidParams),
            Chain(E::SeasonFull),
            Chain(E::InvalidName),
            Chain(E::WrongMint),
            Chain(E::WrongTokenAccount),
            Chain(E::WrongPda),
            Chain(E::AlreadyInitialized),
            Token,
        ],
    ),
];

pub const UPDATE_MEMBER: &[Cover] = &[Cover::Test(
    "registration::update_member_checks",
    &[
        Chain(E::Unauthorized),
        Chain(E::InvalidParams),
        Chain(E::WrongStatus),
        Chain(E::NotInitialized),
        Lands("update_member_ix("),
    ],
)];

/// Ignored tests of this area waiting for their fix: (test, WP).
pub const PENDING: &[(&str, &str)] =
    &[("registration::register_needs_the_session_signature", "WP17")];
