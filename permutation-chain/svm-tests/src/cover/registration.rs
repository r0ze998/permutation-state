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
    Cover::Test(
        "registration::create_season_bounds",
        &[Chain(E::InvalidParams), Lands("create_ix_from(")],
    ),
    Cover::Test(
        "registration::create_season_follows_a_v7_finalized_season",
        &[Chain(E::InvalidParams), Chain(E::NotInitialized)],
    ),
    Cover::Test(
        "genesis_every_creatable::create_season_accepts_exactly_the_creatable_pairs",
        &[Chain(E::InvalidParams)],
    ),
];

pub const ALLOC_WORLD: &[Cover] = &[Cover::Test(
    "registration::alloc_world_checks",
    &[
        Chain(E::InvalidParams),
        Chain(E::WrongStatus),
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
        Chain(E::WrongStatus),
        Chain(E::WrongPda),
        Chain(E::AlreadyInitialized),
        Lands("alloc_nation_ix("),
    ],
)];

pub const REGISTER: &[Cover] = &[
    Cover::Test(
        "registration::register_lands_and_splits_the_fee",
        &[Chain(E::InvalidParams), Lands("register_ix(")],
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
    Cover::Test(
        "registration::register_needs_the_session_signature",
        &[Chain(E::MissingSignature), Lands("register_ix(")],
    ),
    Cover::Test(
        "registration::register_is_operator_paid_in_ai_seasons",
        &[
            Chain(E::Unauthorized),
            Chain(E::SeasonFull),
            Lands("register_ix("),
        ],
    ),
    Cover::Test(
        "registration::register_rejects_votes_in_ai_seasons",
        &[Chain(E::InvalidParams)],
    ),
    Cover::Test(
        "register_session::the_fee_payer_is_never_a_session_key",
        &[Chain(E::Unauthorized), Lands("register_ix_from(")],
    ),
    Cover::Test(
        "register_session::a_delegate_cannot_fund_its_registration_from_anothers_account",
        &[Chain(E::WrongTokenAccount), Lands("register_ix(")],
    ),
    Cover::Test(
        "registration_caps::register_refuses_past_the_season_cap",
        &[Chain(E::SeasonFull)],
    ),
    Cover::Test(
        "upgrade_trust::another_rules_hash_or_logic_halts_the_season_but_not_the_claims",
        &[Chain(E::RulesMismatch)],
    ),
];

pub const UPDATE_MEMBER: &[Cover] = &[
    Cover::Test(
        "registration::update_member_checks",
        &[
            Chain(E::Unauthorized),
            Chain(E::InvalidParams),
            Chain(E::WrongStatus),
            Chain(E::NotInitialized),
            Lands("update_member_ix("),
        ],
    ),
    Cover::Test(
        "registration::register_rejects_votes_in_ai_seasons",
        &[Chain(E::InvalidParams), Lands("update_member_ix(")],
    ),
];

pub const POST_BOND: &[Cover] = &[Cover::Test(
    "registration::post_bond_checks",
    &[
        Chain(E::Unauthorized),
        Chain(E::InvalidParams),
        Chain(E::WrongTokenAccount),
        Chain(E::WrongMint),
        Chain(E::WrongStatus),
        Token,
        Lands("post_bond_ix("),
    ],
)];

/// Ignored tests of this area waiting for their fix: (test, WP).
pub const PENDING: &[(&str, &str)] = &[];
