//! Coverage of the citizen area (§5.6, §5.9: joins, sessions, vigils,
//! tickets, dormancy, season-end closes), W3-A. Codes the wave-3 tests do
//! not reach yet are `Pending` for W5-A (G13 completion).

use super::{Cover, Err, Lands, E};

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
    Cover::Pending("W5-A: Auth (unsigned wallet), g01_loaded_limit"),
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
    Cover::Pending("W5-A: WrongStatus, BadAccount (G3 citizen forgeries through SetSession), g01"),
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
    Cover::Pending("W5-A: RulesetMismatch, g01"),
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
    Cover::Pending("W5-A: BadAddress (Province forgery), TooManyAccounts, g01_loaded_limit"),
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
    Cover::Pending("W5-A: archive-entry seed path with a real archive (ArchiveAnchors, W4-B), g01_loaded_limit"),
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
    Cover::Pending("W5-A: BadAccount forgeries, g01 budget and loaded limit"),
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
    Cover::Pending("W5-A: BadAccount forgeries, g01 budget and loaded limit"),
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
    Cover::Pending("W5-A: BadAccount forgeries, g01 budget and loaded limit"),
];
