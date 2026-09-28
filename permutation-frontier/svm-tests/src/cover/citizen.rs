//! Coverage of the citizen area (§5.6, §5.9: joins, sessions, vigils,
//! tickets, dormancy, season-end closes), W3-A. Codes the wave-3 tests do
//! not reach yet are `Pending` for W5-A (G13 completion).

use super::{Cover, Err, Lands, Loaded, E};

/// Ignored tests of this area waiting for a fix: (test, unit).
pub const PENDING: &[(&str, &str)] = &[];

pub const JOIN: &[Cover] = &[
    Cover::Test(
        "citizen::citizen_join_creates_the_citizen",
        &[
            Err(E::BadData),
            Err(E::BadAddress),
            Err(E::AlreadyDone),
            Err(E::TooManyAccounts),
            Err(E::Capacity),
            Err(E::BadAccount),
            Err(E::WrongStatus),
            Err(E::RulesetMismatch),
            Lands("\"Join\""),
        ],
    ),
    Cover::Test(
        "citizen::citizen_join_gate_needs_the_gate_signature",
        &[Err(E::JoinGate), Lands("Join with the gate")],
    ),
    Cover::Test(
        "citizen::citizen_join_refused_before_genesis",
        &[Err(E::WrongStatus)],
    ),
    Cover::Test(
        "citizen::g02_prefund_citizen_join",
        &[Lands("Join pre-funded")],
    ),
    Cover::Test(
        "citizen::g01_join_seeded_wallets",
        &[Lands("Join at the 25k limit")],
    ),
    Cover::Test(
        "citizen::citizen_season_end_closes_holding_and_citizen",
        &[Err(E::WrongStatus)],
    ),
    Cover::Test(
        "citizen::citizen_g13_join_session_vigil",
        &[Err(E::Auth), Loaded],
    ),
];

pub const SET_SESSION: &[Cover] = &[
    Cover::Test(
        "citizen::citizen_session_and_vigil_through_the_player_prologue",
        &[
            Lands("SetSession"),
            Err(E::BadData),
            Err(E::Auth),
            Err(E::Bucket),
        ],
    ),
    Cover::Test(
        "citizen::citizen_g13_join_session_vigil",
        &[Err(E::WrongStatus), Err(E::BadAccount)],
    ),
    Cover::Test("citizen::g01_land_other_instructions", &[Loaded]),
];

pub const SET_VIGIL: &[Cover] = &[
    Cover::Test(
        "citizen::citizen_session_and_vigil_through_the_player_prologue",
        &[
            Err(E::BadData),
            Lands("SetVigil"),
            Err(E::Cooldown),
            Err(E::SessionExpired),
            Err(E::Auth),
            Err(E::WrongStatus),
        ],
    ),
    Cover::Test(
        "citizen::g03_player_prologue_refuses_forged_citizens_and_seasons",
        &[Err(E::BadAddress), Err(E::BadAccount)],
    ),
    Cover::Test(
        "citizen::citizen_g13_join_session_vigil",
        &[Err(E::RulesetMismatch)],
    ),
    Cover::Test("citizen::g01_land_other_instructions", &[Loaded]),
];

pub const FILE_TICKET: &[Cover] = &[
    Cover::Test(
        "citizen::citizen_file_and_settle_a_fresh_holding",
        &[
            Err(E::ReservedSite),
            Err(E::BadData),
            Err(E::BadAccount),
            Err(E::Insufficient),
            Err(E::TicketState),
            Lands("FileTicket"),
        ],
    ),
    Cover::Test(
        "citizen::citizen_cohort_table_full_refuses_a_ninth_bell",
        &[Err(E::CohortFull)],
    ),
    Cover::Test(
        "citizen::citizen_cohort_expiry_ends_the_ticket",
        &[Lands("refile")],
    ),
    Cover::Test(
        "citizen::g01_file_ticket_three_provinces_full_cohorts",
        &[Lands("FileTicket at the 14k limit")],
    ),
    Cover::Test(
        "citizen::citizen_g13_file_ticket_forgery_shape_loaded",
        &[Err(E::BadAddress), Err(E::TooManyAccounts), Loaded],
    ),
];

pub const SETTLE_TICKET: &[Cover] = &[
    Cover::Test(
        "citizen::citizen_file_and_settle_a_fresh_holding",
        &[
            Err(E::NoAnchor),
            Err(E::SeedNotReady),
            Err(E::TicketState),
            Err(E::NoTicket),
            Lands("SettleTicket (fresh)"),
        ],
    ),
    Cover::Test(
        "citizen::citizen_cohort_displacement_has_no_deadline_and_finality_waits",
        &[
            Err(E::TooManyAccounts),
            Err(E::AlreadyDone),
            Lands("SettleTicket hi (displace)"),
            Lands("SettleTicket mid (taken)"),
        ],
    ),
    Cover::Test(
        "citizen::citizen_cohort_expiry_ends_the_ticket",
        &[Lands("SettleTicket (expired)")],
    ),
    Cover::Test(
        "citizen::citizen_cohort_displacement_within_one_join_shard",
        &[Lands("displace in one shard")],
    ),
    Cover::Test(
        "citizen::g03_settle_ticket_refuses_forged_accounts",
        &[
            Err(E::BadAddress),
            Err(E::BadAccount),
            Err(E::TooManyAccounts),
            Err(E::WrongStatus),
            Err(E::Auth),
            Err(E::SeedNotReady),
        ],
    ),
    Cover::Test(
        "citizen::g02_prefund_holding_settle_ticket_fresh",
        &[Lands("SettleTicket pre-funded")],
    ),
    Cover::Test(
        "citizen::g01_settle_ticket_displacement_three_provinces",
        &[Lands("SettleTicket at the 40k limit")],
    ),
    Cover::Test(
        "citizen::citizen_g13_settle_ticket_archive_path_and_loaded",
        &[Lands("via the archive"), Loaded],
    ),
];

pub const RELEASE_DORMANT: &[Cover] = &[
    Cover::Test(
        "citizen::citizen_release_dormant_frees_the_site_and_strands_the_gen",
        &[
            Err(E::NotDormant),
            Err(E::WrongStatus),
            Err(E::BadAddress),
            Lands("ReleaseDormant"),
        ],
    ),
    Cover::Test(
        "citizen::citizen_g13_season_end_closes_refuse_forgeries",
        &[Err(E::BadAccount), Lands("ReleaseDormant")],
    ),
    Cover::Test("citizen::g01_land_other_instructions", &[Loaded]),
];

pub const CLOSE_HOLDING: &[Cover] = &[
    Cover::Test(
        "citizen::citizen_season_end_closes_holding_and_citizen",
        &[
            Err(E::WrongStatus),
            Err(E::TooEarly),
            Err(E::BadAddress),
            Lands("CloseHolding"),
        ],
    ),
    Cover::Test(
        "lifecycle::close_holding_and_citizen_on_the_tombstone",
        &[Err(E::BadAddress), Lands("CloseHolding on the tombstone")],
    ),
    Cover::Test(
        "citizen::citizen_g13_season_end_closes_refuse_forgeries",
        &[Err(E::BadAccount), Lands("CloseHolding")],
    ),
    Cover::Test("citizen::g01_land_other_instructions", &[Loaded]),
];

pub const CLOSE_CITIZEN: &[Cover] = &[
    Cover::Test(
        "citizen::citizen_season_end_closes_holding_and_citizen",
        &[
            Err(E::TooEarly),
            Err(E::TooManyAccounts),
            Err(E::BadAddress),
            Lands("CloseCitizen with escrow"),
            Lands("CloseCitizen (Aborted)"),
        ],
    ),
    Cover::Test(
        "lifecycle::close_holding_and_citizen_on_the_tombstone",
        &[Lands("CloseCitizen on the tombstone")],
    ),
    Cover::Test(
        "citizen::citizen_g13_season_end_closes_refuse_forgeries",
        &[Err(E::BadAccount), Lands("CloseCitizen")],
    ),
    Cover::Test("citizen::g01_land_other_instructions", &[Loaded]),
];
