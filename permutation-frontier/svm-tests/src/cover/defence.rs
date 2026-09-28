//! Coverage of the defence area (§5.12): ClaimDefence (W4-B).

use super::{Cover, Err, Lands, Loaded, E};

/// Ignored tests of this area waiting for a fix: (test, unit).
pub const PENDING: &[(&str, &str)] = &[];

pub const CLAIM_DEFENCE: &[Cover] = &[
    Cover::Test(
        "defence::g12_claim_defence_caps_and_partial_pool",
        &[Lands("s.claim(")],
    ),
    Cover::Test(
        "defence::g13_claim_defence_refusals",
        &[
            Err(E::BadData),
            Err(E::NotEligible),
            Err(E::BadAddress),
            Err(E::BadAccount),
            Err(E::NoAnchor),
            Err(E::WrongStatus),
            Err(E::WindowClosed),
            Lands("s.claim("),
        ],
    ),
    Cover::Test(
        "defence::g02_prefund_defence_claim",
        &[Lands("ClaimDefence on a pre-funded claim")],
    ),
    Cover::Test(
        "transit::g12_claim_after_settle_keeps_the_slot",
        &[Err(E::NotEligible), Lands("claim_ix(")],
    ),
    Cover::Test(
        "transit::g12_claim_after_displacement_resets_the_evidence",
        &[Err(E::NotEligible), Lands("claim_ix(")],
    ),
    Cover::Test("defence::g01_loaded_limit_w4b_claim_defence", &[Loaded]),
    Cover::Test(
        "defence::g13_claim_defence_refusals",
        &[Err(E::TooManyAccounts), Err(E::Auth)],
    ),
];
