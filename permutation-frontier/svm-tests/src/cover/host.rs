//! Coverage of the host area (§5.10, §5.11): Muster, Dissolve, Garrison,
//! Explore, DisbandStranded, Depart, SettleDeparture (W3-B).

use super::{Cover, Err, Lands, Loaded, E};

/// Ignored tests of this area waiting for a fix: (test, unit).
pub const PENDING: &[(&str, &str)] = &[];

pub const MUSTER: &[Cover] = &[
    Cover::Test(
        "host::host_muster_enters_pending_then_joins_the_roster",
        &[Lands("hix::muster(")],
    ),
    Cover::Test(
        "host::host_muster_refusals",
        &[
            Err(E::Insufficient),
            Err(E::Kernel),
            Err(E::BadData),
            Err(E::ProvinceFull),
            Err(E::NotResident),
            Err(E::NotFinal),
            Err(E::BadAddress),
            Err(E::NotOwner),
            Lands("hix::muster("),
        ],
    ),
    Cover::Test(
        "host::host_provisional_holding_turns_final_when_its_cohort_closes",
        &[Lands("hix::muster(")],
    ),
    Cover::Test(
        "host::host_player_prologue_refusals",
        &[
            Err(E::WrongStatus),
            Err(E::RulesetMismatch),
            Err(E::Bucket),
            Err(E::SessionExpired),
            Err(E::Auth),
            Err(E::BadAddress),
            Err(E::TooManyAccounts),
        ],
    ),
    Cover::Test("g13_complete::g13_muster_and_disband_loaded", &[Loaded]),
];

pub const DISSOLVE: &[Cover] = &[
    Cover::Test(
        "host::host_dissolve_marks_the_host_leaving",
        &[
            Lands("hix::dissolve("),
            Err(E::HostBusy),
            Err(E::NotOwner),
            Err(E::NotResident),
            Err(E::HostInTransit),
        ],
    ),
    Cover::Test(
        "g13_complete::g13_dissolve_not_final_prologue_and_loaded",
        &[
            Err(E::NotFinal),
            Err(E::WrongStatus),
            Err(E::RulesetMismatch),
            Err(E::Bucket),
            Err(E::SessionExpired),
            Err(E::Auth),
            Err(E::BadAddress),
            Err(E::TooManyAccounts),
            Loaded,
        ],
    ),
];

pub const GARRISON: &[Cover] = &[
    Cover::Test(
        "host::host_garrison_moves_reserve_into_the_mirror",
        &[
            Lands("hix::garrison("),
            Err(E::BadData),
            Err(E::Insufficient),
            Err(E::Kernel),
        ],
    ),
    Cover::Test(
        "g13_complete::g13_garrison_final_resident_busy_prologue_and_loaded",
        &[
            Err(E::NotFinal),
            Err(E::NotResident),
            Err(E::HostBusy),
            Err(E::WrongStatus),
            Err(E::RulesetMismatch),
            Err(E::Bucket),
            Err(E::SessionExpired),
            Err(E::Auth),
            Err(E::BadAddress),
            Err(E::TooManyAccounts),
            Loaded,
        ],
    ),
];

pub const EXPLORE: &[Cover] = &[
    Cover::Test(
        "holding::holding_explore_and_settle_explore",
        &[Lands("hx::explore("), Err(E::HostBusy), Err(E::Explored)],
    ),
    Cover::Test(
        "holding::holding_explore_refusals",
        &[
            Err(E::BadData),
            Err(E::NotOwner),
            Err(E::NotResident),
            Err(E::HostInTransit),
            Err(E::NotFinal),
            Lands("hx::explore("),
        ],
    ),
    Cover::Test(
        "g13_complete::g13_explore_and_settle_explore",
        &[Loaded, Lands("Explore")],
    ),
];

pub const DISBAND_STRANDED: &[Cover] = &[
    Cover::Test(
        "host::host_disband_stranded_frees_a_host_of_a_gone_holding",
        &[
            Err(E::NotDormant),
            Lands("hix::disband_stranded("),
            Err(E::BadData),
        ],
    ),
    Cover::Test("g13_complete::g13_muster_and_disband_loaded", &[Loaded]),
];

pub const DEPART: &[Cover] = &[
    Cover::Test(
        "host::host_depart_escrows_and_settle_departure_moves_the_values",
        &[Lands("w.depart_ix(")],
    ),
    Cover::Test(
        "host::host_depart_refusals",
        &[
            Err(E::TipTooLow),
            Err(E::ArrivalBell),
            Err(E::BadData),
            Err(E::TransitState),
            Err(E::HostInTransit),
            Err(E::HostBusy),
            Err(E::Cooldown),
            Err(E::NotOwner),
            Err(E::NotResident),
            Err(E::Insufficient),
            Err(E::NotFinal),
        ],
    ),
    // W6T-1 (w6-s7 triage): step 4 refuses an arrival at or after end_bell.
    Cover::Test(
        "host::host_depart_arrival_at_or_after_end_bell_refused",
        &[Err(E::ArrivalBell), Lands("w.depart_ix(")],
    ),
    Cover::Test(
        "g13_complete::g13_depart_and_settle_departure",
        &[Loaded, Lands("Depart")],
    ),
];

pub const SETTLE_DEPARTURE: &[Cover] = &[
    Cover::Test(
        "host::host_depart_escrows_and_settle_departure_moves_the_values",
        &[
            Err(E::TooEarly),
            Err(E::BadAddress),
            Lands("hix::settle_departure("),
            Err(E::AlreadyDone),
            Err(E::TransitState),
        ],
    ),
    Cover::Test(
        "host::host_settle_departure_of_a_host_destroyed_at_its_origin",
        &[Lands("hix::settle_departure(")],
    ),
    // The return settle (`transit_slot = 0xFF`, W4-A D10; integ-W4 review).
    Cover::Test(
        "clash::clash_dissolve_returns_troops_to_the_reserve",
        &[Lands("cix::settle_return("), Err(E::AlreadyDone)],
    ),
    Cover::Test(
        "clash::clash_bounced_resident_leaves_and_returns",
        &[Lands("ResolveFromInputs"), Err(E::AlreadyDone)],
    ),
    Cover::Test(
        "clash::clash_return_settle_is_bounded",
        &[Lands("return 1"), Err(E::AlreadyDone)],
    ),
    Cover::Test(
        "g13_complete::g13_depart_and_settle_departure",
        &[
            Err(E::WrongStatus),
            Err(E::NotResident),
            Loaded,
            Lands("SettleDeparture"),
        ],
    ),
];
