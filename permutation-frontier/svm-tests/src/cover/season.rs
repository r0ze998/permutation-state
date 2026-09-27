//! Coverage of the season lifecycle (§5.7). W2-A's six instructions are
//! covered from wave 2; EndSeason, AbortSeason and CloseSeason are W4-B's
//! (this file is handed over to W4-B in wave 4, §11).

use super::{Cover, Err, Lands, Loaded, Refused, E};

/// Ignored tests of this area waiting for a fix: (test, unit).
pub const PENDING: &[(&str, &str)] = &[];

pub const ANNOUNCE_SEASON: &[Cover] = &[
    Cover::Test(
        "g02_prefund::g02_prefund_season_announce_season",
        &[Lands("w.announce(")],
    ),
    Cover::Test(
        "g02_prefund::g02_recreate_season_id_refused",
        &[Err(E::Announce)],
    ),
    Cover::Test(
        "g13_season_beacon::g13_announce_season_lead_and_bond",
        &[Err(E::Announce), Lands("w.announce(")],
    ),
    Cover::Test(
        "g03_forgery::g03_announce_season_addresses_and_authority",
        &[
            Err(E::BadAddress),
            Err(E::Auth),
            Refused("fake_pd"),
            Lands("w.announce("),
        ],
    ),
    Cover::Test(
        "g01_loaded_limit::g01_loaded_limit_announce_season",
        &[Loaded],
    ),
    Cover::Pending("W5-A: TooManyAccounts, BadData, Insufficient"),
];

pub const CREATE_SEASON: &[Cover] = &[
    Cover::Test(
        "g02_prefund::g02_prefund_create_season_accounts",
        &[Lands("w.create(")],
    ),
    Cover::Test(
        "g13_season_beacon::g13_create_season_window_hash_params_status",
        &[
            Err(E::Announce),
            Err(E::WrongStatus),
            Err(E::TooEarly),
            Err(E::BadData),
            Lands("CreateSeason"),
        ],
    ),
    Cover::Test(
        "g03_forgery::g03_create_season_targets_must_be_canonical",
        &[Err(E::BadAddress), Err(E::Auth)],
    ),
    Cover::Test(
        "g01_loaded_limit::g01_loaded_limit_create_season",
        &[Loaded],
    ),
    Cover::Pending("W5-A: BadAccount (present targets), TooManyAccounts"),
];

pub const INIT_BEACON_LOGS: &[Cover] = &[
    Cover::Test(
        "g02_prefund::g02_prefund_beacon_logs",
        &[Lands("w.init_logs(")],
    ),
    Cover::Test(
        "g13_season_beacon::g13_init_beacon_logs_and_shards_status_authority_repeat",
        &[Err(E::WrongStatus), Err(E::Auth), Err(E::AlreadyDone)],
    ),
    Cover::Test(
        "g03_forgery::g03_init_logs_and_shards_targets_must_be_canonical",
        &[Err(E::BadAddress)],
    ),
    Cover::Test(
        "g01_loaded_limit::g01_loaded_limit_init_beacon_logs",
        &[Loaded],
    ),
];

pub const INIT_SHARDS: &[Cover] = &[
    Cover::Test(
        "g02_prefund::g02_prefund_join_shards",
        &[Lands("w.init_shards(")],
    ),
    Cover::Test(
        "g13_season_beacon::g13_init_beacon_logs_and_shards_status_authority_repeat",
        &[
            Err(E::WrongStatus),
            Err(E::Auth),
            Err(E::BadData),
            Err(E::AlreadyDone),
        ],
    ),
    Cover::Test(
        "g03_forgery::g03_init_logs_and_shards_targets_must_be_canonical",
        &[Err(E::BadAddress)],
    ),
    Cover::Test("g01_loaded_limit::g01_loaded_limit_init_shards", &[Loaded]),
];

pub const CONSUME_GENESIS_SEED: &[Cover] = &[
    Cover::Test(
        "g05_one_anchor::g05_genesis_seed_is_the_announced_rounds",
        &[
            Err(E::WrongRound),
            Err(E::Crypto),
            Err(E::WrongStatus),
            Lands("w.consume_genesis("),
        ],
    ),
    Cover::Test(
        "g13_season_beacon::g13_top_level_only_instructions_refuse_a_cpi",
        &[Err(E::NotTopLevel)],
    ),
    Cover::Test(
        "g01_loaded_limit::g01_loaded_limit_consume_genesis_seed",
        &[Loaded],
    ),
];

pub const SET_WINDOW_SCHEDULE: &[Cover] = &[
    Cover::Test(
        "g04_reveal_window::g04_window_schedule_needs_notice_and_range",
        &[
            Err(E::Auth),
            Err(E::AlreadyDone),
            Refused("c.send("),
            Lands("SetWindowSchedule"),
        ],
    ),
    Cover::Test(
        "g04_reveal_window::g04_window_schedule_from_bell_inside_the_season",
        &[Err(E::BadData), Lands("SetWindowSchedule at the last bell")],
    ),
    Cover::Test(
        "g03_forgery::g03_season_forged_in_keeper_and_authority_instructions",
        &[Err(E::BadAddress), Err(E::BadAccount)],
    ),
    Cover::Test(
        "g01_loaded_limit::g01_loaded_limit_set_window_schedule",
        &[Loaded],
    ),
];

pub const END_SEASON: &[Cover] = &[Cover::Pending("W4-B")];
pub const CLOSE_SEASON: &[Cover] = &[Cover::Pending("W4-B")];
pub const ABORT_SEASON: &[Cover] = &[Cover::Pending("W4-B")];
