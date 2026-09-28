//! Coverage of Reveal (§5.11, W3-B).

use super::{Cover, Err, Lands, Loaded, E};

/// Ignored tests of this area waiting for a fix: (test, unit).
pub const PENDING: &[(&str, &str)] = &[];

pub const REVEAL: &[Cover] = &[
    Cover::Test(
        "reveal::reveal_fills_a_slot_and_creates_the_day",
        &[Lands("s.ix(0, true)"), Err(E::AlreadyDone)],
    ),
    Cover::Test(
        "reveal::reveal_quota_displaces_the_lowest_and_refuses_the_rest",
        &[
            Err(E::SlotMoved),
            Err(E::QuotaRefused),
            Lands("s.ix(slot, false)"),
        ],
    ),
    Cover::Test(
        "reveal::g04_reveal_window_closes_at_a_plus_w",
        &[Err(E::WindowClosed), Lands("s.ix(0, true) before close")],
    ),
    Cover::Test(
        "reveal::g04_reveal_refused_once_the_beacon_log_holds_s",
        &[Err(E::WindowClosed)],
    ),
    Cover::Test(
        "reveal::g04_reveal_latch_after_the_first_gather_or_resolve",
        &[Err(E::LatchClosed)],
    ),
    Cover::Test(
        "reveal::g04_reveal_refused_for_a_tombstoned_bell",
        &[Err(E::Archived)],
    ),
    Cover::Test(
        "reveal::g03_forgery_reveal_accounts",
        &[Err(E::BadAddress), Err(E::BadAccount), Err(E::WrongRegion)],
    ),
    Cover::Test(
        "reveal::g03_forgery_reveal_path_provinces",
        &[Err(E::Path), Err(E::BadAddress), Err(E::BadAccount)],
    ),
    Cover::Test(
        "reveal::reveal_refusals",
        &[
            Err(E::BadPlaintext),
            Err(E::CommitMismatch),
            Err(E::TransitState),
            Err(E::NeedArrivalDay),
            Err(E::BadData),
            Err(E::BadAccount),
            Err(E::Auth),
            Err(E::WrongStatus),
            Err(E::RulesetMismatch),
            Err(E::Shielded),
            Err(E::TooManyAccounts),
        ],
    ),
    Cover::Test(
        "reveal::reveal_arrival_bell_must_fit_the_path",
        &[Err(E::ArrivalBell)],
    ),
    Cover::Test("reveal::reveal_refuses_a_cpi", &[Err(E::NotTopLevel)]),
    Cover::Test(
        "reveal::g02_prefund_arrival_slot_and_day_reveal",
        &[Lands("Reveal on pre-funded addresses")],
    ),
    Cover::Test("reveal::g09_reveal_quota_is_order_free", &[Lands("Reveal")]),
    Cover::Test("reveal::g01_reveal_worst", &[Loaded]),
];
