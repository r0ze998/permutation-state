//! Coverage of the transit area (§5.11, §5.12): SettleTransit and
//! SweepPoolOwed (W4-B).

use super::{Cover, Err, Lands, Loaded, E};

/// Ignored tests of this area waiting for a fix: (test, unit).
pub const PENDING: &[(&str, &str)] = &[];

pub const SETTLE_TRANSIT: &[Cover] = &[
    // v1.7 (integ-W4 review): the camp's Works, an absent destination, the
    // gather stamp.
    Cover::Test(
        "transit::g12_settle_credits_the_camp_works_to_one_winner",
        &[
            Err(E::BadAccount),
            Err(E::BadAddress),
            Lands("settle of the camp's winner"),
        ],
    ),
    Cover::Test(
        "transit::g12_valid_seal_to_an_absent_province_routes",
        &[
            Err(E::BadAddress),
            Err(E::BadAccount),
            Lands("settle to an absent Province"),
        ],
    ),
    Cover::Test(
        "transit::g12_gathered_bad_seal_settles_only_at_its_destination",
        &[
            Err(E::BadAddress),
            Lands("settle at the gathered destination"),
        ],
    ),
    Cover::Test(
        "transit::g12_settle_stays_pays_and_frees_the_host",
        &[
            Err(E::HostInTransit),
            Err(E::TooEarly),
            Err(E::TransitState),
            Lands("w.settle_ix("),
        ],
    ),
    Cover::Test(
        "transit::g10_settle_transit_seal_codes_match_the_stock_opener",
        &[
            Err(E::CommitMismatch),
            Err(E::NoAnchor),
            Lands("w.settle_ix("),
        ],
    ),
    Cover::Test(
        "transit::g13_settle_transit_refusals",
        &[
            Err(E::TooEarly),
            Err(E::DepartureUnsettled),
            Err(E::BadData),
            Err(E::Auth),
            Err(E::TooManyAccounts),
            Err(E::BadAddress),
            Err(E::BadAccount),
            Err(E::WrongStatus),
            Lands("w.settle_ix("),
        ],
    ),
    Cover::Test(
        "transit::g12_settle_fates_return_or_keep_the_host",
        &[Lands("w.settle_ix(")],
    ),
    Cover::Test(
        "transit::g12_settle_bounce_by_rank_and_a_drained_resolver",
        &[Lands("w.settle_ix(")],
    ),
    Cover::Test(
        "transit::g12_settle_routs_an_unrevealed_host",
        &[Lands("w.settle_ix(")],
    ),
    Cover::Test(
        "transit::g12_settle_returns_to_the_reserve_when_home_is_full",
        &[Lands("w.settle_ix(")],
    ),
    Cover::Test(
        "transit::g12_settle_races_proof_destroys_a_revealed_bad_seal",
        &[Lands("w.settle_ix(")],
    ),
    Cover::Test(
        "transit::g12_bad_seal_after_archive_forfeits_a_stays_host",
        &[Lands("w.settle_ix(")],
    ),
    Cover::Test(
        "transit::g12_stays_host_cannot_act_before_its_settlement",
        &[Err(E::HostInTransit), Lands("w.settle_ix(")],
    ),
    Cover::Test(
        "transit::g12_claim_after_settle_keeps_the_slot",
        &[Lands("w.settle_ix(")],
    ),
    Cover::Test("transit::g01_loaded_limit_w4b_transit", &[Loaded]),
];

pub const SWEEP_POOL_OWED: &[Cover] = &[
    Cover::Test(
        "transit::g12_settle_bounce_by_rank_and_a_drained_resolver",
        &[Err(E::AlreadyDone), Lands("sweep_pool_owed(")],
    ),
    Cover::Test(
        "transit::g13_sweep_pool_owed_refusals",
        &[
            Err(E::AlreadyDone),
            Err(E::BadAddress),
            Err(E::WrongStatus),
            Lands("sweep_pool_owed("),
        ],
    ),
    Cover::Test("transit::g01_loaded_limit_w4b_transit", &[Loaded]),
    Cover::Pending("W5-A: BadAccount, TooManyAccounts"),
];
