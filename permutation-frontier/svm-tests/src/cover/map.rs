//! Coverage of the map area (§5.9: rings and provinces), W3-A. Codes the
//! wave-3 tests do not reach yet are `Pending` for W5-A (G13 completion).

use super::{Cover, Err, Lands, E};

/// Ignored tests of this area waiting for a fix: (test, unit).
pub const PENDING: &[(&str, &str)] = &[];

pub const OPEN_RING: &[Cover] = &[
    Cover::Test(
        "map::map_genesis_rings_are_seeded_at_once",
        &[
            Err(E::TooEarly),
            Err(E::BadData),
            Err(E::AlreadyDone),
            Lands("OpenRing (genesis ring)"),
        ],
    ),
    Cover::Test(
        "map::map_ring_beyond_genesis_waits_for_occupancy_and_its_seed",
        &[
            Err(E::TooEarly),
            Err(E::Capacity),
            Err(E::Insufficient),
            Lands("OpenRing(4)"),
        ],
    ),
    Cover::Test(
        "map::map_land_instructions_refuse_outside_seeded_and_running",
        &[Err(E::WrongStatus)],
    ),
    Cover::Test(
        "map::g03_land_accounts_forged_in_map_instructions",
        &[Err(E::BadAddress), Err(E::BadAccount)],
    ),
    Cover::Test(
        "map::g02_prefund_ring_seed_both_paths",
        &[
            Lands("OpenRing(0) pre-funded"),
            Lands("OpenRing(4) pre-funded"),
        ],
    ),
    Cover::Test(
        "map::g02_recreate_province_and_ring_seed_refused_after_close",
        &[Err(E::WrongStatus)],
    ),
    Cover::Pending("W5-A: Auth, TooManyAccounts, RulesetMismatch, g01_loaded_limit"),
];

pub const CONSUME_RING_SEED: &[Cover] = &[
    Cover::Test(
        "map::map_ring_beyond_genesis_waits_for_occupancy_and_its_seed",
        &[
            Err(E::WrongRound),
            Err(E::TooEarly),
            Err(E::Crypto),
            Err(E::AlreadyDone),
            Lands("ConsumeRingSeed"),
        ],
    ),
    Cover::Test(
        "map::map_land_instructions_refuse_outside_seeded_and_running",
        &[Err(E::WrongStatus)],
    ),
    Cover::Test(
        "map::g03_land_accounts_forged_in_map_instructions",
        &[Err(E::BadAccount), Err(E::BadAddress)],
    ),
    Cover::Pending(
        "W5-A: NotTopLevel (probe CPI), Auth, 32-round worst hint path (G1), g01_loaded_limit",
    ),
];

pub const OPEN_PROVINCE: &[Cover] = &[
    Cover::Test(
        "map::map_open_province_writes_the_kernels_land",
        &[
            Lands("OpenProvince (ring 2)"),
            Err(E::AlreadyDone),
            Err(E::BadData),
            Err(E::SeedNotReady),
            Err(E::Insufficient),
        ],
    ),
    Cover::Test(
        "map::map_land_instructions_refuse_outside_seeded_and_running",
        &[Err(E::WrongStatus), Lands("OpenProvince before genesis")],
    ),
    Cover::Test(
        "map::g03_land_accounts_forged_in_map_instructions",
        &[Err(E::BadAddress), Err(E::BadAccount)],
    ),
    Cover::Test(
        "map::g02_prefund_province_funded_path",
        &[Lands("OpenProvince pre-funded")],
    ),
    Cover::Test(
        "map::g02_recreate_province_and_ring_seed_refused_after_close",
        &[Err(E::WrongStatus)],
    ),
    Cover::Test(
        "map::g01_open_province_worst_of_rings_2_to_10",
        &[Lands("OpenProvince (")],
    ),
    Cover::Pending("W5-A: NotTopLevel (probe CPI), g01_loaded_limit"),
];

pub const FOLD_OCCUPANCY: &[Cover] = &[
    Cover::Test(
        "map::map_fold_occupancy_runs_in_three_parts",
        &[
            Err(E::FoldStale),
            Err(E::BadData),
            Lands("FoldOccupancy part 0"),
        ],
    ),
    Cover::Test(
        "map::map_land_instructions_refuse_outside_seeded_and_running",
        &[Err(E::WrongStatus), Lands("FoldOccupancy before genesis")],
    ),
    Cover::Test(
        "map::g03_land_accounts_forged_in_map_instructions",
        &[Err(E::BadAddress), Err(E::BadAccount)],
    ),
    Cover::Pending("W5-A: TooManyAccounts, g01 budget and loaded limit per part"),
];

pub const CLOSE_PROVINCE: &[Cover] = &[
    Cover::Test(
        "map::map_close_province_after_the_season_end",
        &[
            Err(E::WrongStatus),
            Err(E::TooEarly),
            Err(E::BadAccount),
            Lands("CloseProvince"),
        ],
    ),
    Cover::Test(
        "map::g03_land_accounts_forged_in_map_instructions",
        &[Err(E::BadAddress)],
    ),
    Cover::Pending("W5-A: g01 budget and loaded limit"),
];
