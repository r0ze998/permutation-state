//! Coverage of the beacon instructions (§5.8). W2-A's four are covered from
//! wave 2; ArchiveAnchors and CloseSeedCache are W4-B's (this file is
//! handed over to W4-B in wave 4, §11).

use super::{Cover, Err, Lands, Loaded, Refused, E};

/// Ignored tests of this area waiting for a fix: (test, unit).
pub const PENDING: &[(&str, &str)] = &[];

pub const POST_ANCHOR: &[Cover] = &[
    Cover::Test(
        "g04_reveal_window::g04_anchor_records_the_clock_at_creation",
        &[Lands("w.post_anchor(")],
    ),
    Cover::Test(
        "g04_reveal_window::g04_tombstoned_bell_refuses_an_anchor",
        &[Err(E::Archived)],
    ),
    Cover::Test(
        "g05_one_anchor::g05_anchor_round_is_t_of_the_bell",
        &[Err(E::WrongRound), Err(E::Crypto), Lands("w.post_anchor(")],
    ),
    Cover::Test(
        "g05_one_anchor::g05_one_anchor_per_bell_region",
        &[Lands("PostAnchor repeat")],
    ),
    Cover::Test(
        "g03_forgery::g03_anchor_and_archive_forged_in_post_anchor",
        &[
            Err(E::BadAddress),
            Err(E::BadAccount),
            Lands("w.post_anchor("),
        ],
    ),
    Cover::Test(
        "g03_forgery::g03_ruleset_of_the_season_must_match_the_binary",
        &[Err(E::RulesetMismatch)],
    ),
    Cover::Test(
        "g03_forgery::g03_instructions_sysvar_must_be_the_sysvar",
        &[Refused("IX_SYSVAR")],
    ),
    Cover::Test(
        "g13_season_beacon::g13_top_level_only_instructions_refuse_a_cpi",
        &[Err(E::NotTopLevel)],
    ),
    Cover::Test(
        "g13_season_beacon::g13_keeper_writes_need_the_fee_payer_signature",
        &[Err(E::Auth)],
    ),
    Cover::Test(
        "g02_prefund::g02_recreate_bell_anchor_refused_after_archive",
        &[Err(E::Archived)],
    ),
    Cover::Test("g01_loaded_limit::g01_loaded_limit_post_anchor", &[Loaded]),
    Cover::Pending(
        "W5-A: BadData (region ≥ 16, bell ≥ end_bell), WrongStatus, TooEarly (test-beacon)",
    ),
];

pub const POST_ANCHOR_MULTI: &[Cover] = &[
    Cover::Test(
        "g05_one_anchor::g05_multi_anchor_equals_single_anchors",
        &[Lands("PostAnchorMulti (7 regions)")],
    ),
    Cover::Test(
        "g05_one_anchor::g05_no_second_anchor_after_archive",
        &[Err(E::Archived)],
    ),
    Cover::Test(
        "g05_one_anchor::g05_anchor_round_is_t_of_the_bell",
        &[Err(E::Crypto)],
    ),
    Cover::Test(
        "g13_season_beacon::g13_top_level_only_instructions_refuse_a_cpi",
        &[Err(E::NotTopLevel)],
    ),
    Cover::Test(
        "g02_prefund::g02_prefund_bell_anchor_post_anchor_multi",
        &[Lands("PostAnchorMulti")],
    ),
    Cover::Test(
        "g01_loaded_limit::g01_loaded_limit_post_anchor_multi",
        &[Loaded],
    ),
    Cover::Pending("W5-A: BadData (mask 0 or > 7 regions), WrongRound, BadAddress, BadAccount"),
];

pub const POST_SEED: &[Cover] = &[
    Cover::Test(
        "g05_one_anchor::g05_every_cache_nonce_gives_the_same_seed",
        &[Lands("w.post_seed(")],
    ),
    Cover::Test(
        "g05_one_anchor::g05_seed_round_is_s_of_the_anchor",
        &[
            Err(E::NoAnchor),
            Err(E::WrongRound),
            Err(E::Crypto),
            Lands("PostSeed with S(A)"),
        ],
    ),
    Cover::Test(
        "g03_forgery::g03_anchor_and_cache_forged_in_post_seed",
        &[Err(E::BadAddress), Err(E::BadAccount)],
    ),
    Cover::Test(
        "g02_prefund::g02_recreate_seed_cache_refused_after_archive",
        &[Err(E::NoAnchor)],
    ),
    Cover::Test(
        "g13_season_beacon::g13_top_level_only_instructions_refuse_a_cpi",
        &[Err(E::NotTopLevel)],
    ),
    Cover::Test("g01_loaded_limit::g01_loaded_limit_post_seed", &[Loaded]),
];

pub const POST_BEACON: &[Cover] = &[
    Cover::Test(
        "g04_reveal_window::g04_beacon_log_only_moves_forward",
        &[
            Lands("w.post_beacon("),
            Refused("w.post_beacon("),
            Err(E::Crypto),
        ],
    ),
    Cover::Test(
        "g03_forgery::g03_beacon_log_forged_in_post_beacon",
        &[Err(E::BadAddress), Err(E::BadAccount)],
    ),
    Cover::Test(
        "g03_forgery::g03_season_forged_in_keeper_and_authority_instructions",
        &[Err(E::BadAddress), Err(E::BadAccount)],
    ),
    Cover::Test(
        "g13_season_beacon::g13_top_level_only_instructions_refuse_a_cpi",
        &[Err(E::NotTopLevel)],
    ),
    Cover::Test("g01_loaded_limit::g01_loaded_limit_post_beacon", &[Loaded]),
];

pub const ARCHIVE_ANCHORS: &[Cover] = &[Cover::Pending("W4-B")];
pub const CLOSE_SEED_CACHE: &[Cover] = &[Cover::Pending("W4-B")];
