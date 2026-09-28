//! Tamper classes T1–T22 (contract §8.5): each MUST FAIL with its code on
//! the recorded fixtures; built with the `mutate-<check>` feature of the
//! check it targets it must PASS instead (the checks of the checks,
//! `mutate.sh` builds each feature and runs these tests).

mod common;

use common::*;
use verify_core::tamper::{self, Case};
use verify_core::Verdict;

/// Whether this build disables `feature`'s check.
fn disabled(feature: &str) -> bool {
    match feature {
        "mutate-v1" => cfg!(feature = "mutate-v1"),
        "mutate-v2" => cfg!(feature = "mutate-v2"),
        "mutate-v3" => cfg!(feature = "mutate-v3"),
        "mutate-v4" => cfg!(feature = "mutate-v4"),
        "mutate-v5" => cfg!(feature = "mutate-v5"),
        "mutate-v6" => cfg!(feature = "mutate-v6"),
        "mutate-v7" => cfg!(feature = "mutate-v7"),
        "mutate-v8" => cfg!(feature = "mutate-v8"),
        "mutate-v9" => cfg!(feature = "mutate-v9"),
        "mutate-v11" => cfg!(feature = "mutate-v11"),
        "mutate-v12" => cfg!(feature = "mutate-v12"),
        "mutate-v13" => cfg!(feature = "mutate-v13"),
        other => panic!("unknown feature {other}"),
    }
}

fn judge(case: Case) {
    let r = verify_core::verify(&case.input);
    if case.feature.is_some_and(disabled) {
        assert_eq!(
            r.verdict,
            Verdict::Pass,
            "{} ({}) with its check disabled must PASS:\n{}",
            case.class,
            case.what,
            show(&r)
        );
        println!(
            "{}: PASS with {} (the check of the check)",
            case.class,
            case.feature.unwrap_or("")
        );
    } else {
        assert_eq!(
            r.verdict,
            Verdict::Fail,
            "{} ({}) must FAIL:\n{}",
            case.class,
            case.what,
            show(&r)
        );
        assert!(
            case.codes.iter().any(|c| r.fails_with(c)),
            "{} ({}) must FAIL with one of {:?}; got:\n{}",
            case.class,
            case.what,
            case.codes,
            show(&r)
        );
        let hit: Vec<&str> = case
            .codes
            .iter()
            .copied()
            .filter(|c| r.fails_with(c))
            .collect();
        println!("{}: FAIL with {hit:?}", case.class);
    }
}

/// A class that has no consistent forgery on this fixture (its lie
/// reaches other checks too, e.g. a wrong drand key also leaves every seal
/// unopenable): it must FAIL with its code in the default build; with its
/// check disabled it must no longer FAIL with that code.
fn judge_codes_only(case: Case) {
    let r = verify_core::verify(&case.input);
    let hit = case.codes.iter().any(|c| r.fails_with(c));
    if case.feature.is_some_and(disabled) {
        assert!(
            !hit,
            "{} with its check disabled still FAILS with its code",
            case.class
        );
        println!(
            "{}: its code gone with {} (codes only)",
            case.class,
            case.feature.unwrap_or("")
        );
    } else {
        assert_eq!(
            r.verdict,
            Verdict::Fail,
            "{} must FAIL:\n{}",
            case.class,
            show(&r)
        );
        assert!(
            hit,
            "{} must FAIL with one of {:?}:\n{}",
            case.class,
            case.codes,
            show(&r)
        );
    }
}

macro_rules! tamper_codes_only {
    ($name:ident, $f:ident, $fx:ident) => {
        #[test]
        fn $name() {
            judge_codes_only(tamper::$f(&$fx()).unwrap_or_else(|e| panic!("{e}")));
        }
    };
}

macro_rules! tamper_test {
    ($name:ident, $f:ident, $fx:ident) => {
        #[test]
        fn $name() {
            judge(tamper::$f(&$fx()).unwrap_or_else(|e| panic!("{e}")));
        }
    };
}

tamper_test!(tamper_t01_drop_a_depart, t01, march);
tamper_test!(tamper_t01b_drop_an_unchained_transaction, t01b, land);
tamper_test!(tamper_t02_flip_a_reveal_plaintext_byte, t02, march);
tamper_test!(tamper_t03_shift_an_anchor_a, t03, land);
tamper_test!(tamper_t04_inject_a_second_anchor, t04, land);
tamper_test!(tamper_t05_swap_a_cache_signature, t05, land);
tamper_test!(tamper_t06_set_account_on_a_province, t06, march);
tamper_test!(
    tamper_t06b_rogue_write_between_resolves,
    t06b,
    march_program
);
tamper_test!(tamper_t07_valid_seal_marked_bad, t07, march);
tamper_test!(tamper_t08_displacement_slot_changed, t08, march);
tamper_test!(tamper_t09_departure_mass_altered, t09, march);
tamper_test!(tamper_t10_wrong_quicknet_key, t10, land);
tamper_test!(tamper_t11_wrong_ruleset_hash, t11, land);
tamper_test!(tamper_t12_truncate_the_last_game_day, t12, march);
tamper_test!(tamper_t13_reveal_past_the_close, t13, march);
tamper_test!(tamper_t14_duplicate_a_transaction, t14, land);
tamper_test!(tamper_t15_origin_values_from_a_later_bell, t15, march);
tamper_test!(tamper_t16_wrong_genesis_round, t16, land);
tamper_test!(tamper_t17_skip_over_an_arrival, t17, march);
tamper_test!(tamper_t18_tampered_ticket_score, t18, land);
tamper_test!(tamper_t19_tampered_terrain_digest, t19, land);
tamper_test!(tamper_t20_defence_claim_above_the_formula, t20, march);
tamper_test!(tamper_t21_tampered_explore_find, t21, march);
tamper_test!(tamper_t22_bad_seal_logged_as_surviving, t22, march);

/// Every class of §8.5 has a case, each with a `mutate-` feature.
#[test]
fn tamper_classes_are_complete() {
    let cases = tamper::all(&land(), &march(), &march_program());
    for n in 1..=22 {
        let c = format!("T{n}");
        assert!(cases.iter().any(|x| x.class == c), "{c} missing");
    }
    assert!(
        cases.iter().all(|c| c.feature.is_some()),
        "every class names its check"
    );
}

tamper_test!(tamper_v9a_non_canonical_address, v9a, land);

// The classes that apply to the season recorded from the merged program
// (`march-program.json.gz`, integ-W4 review: T1–T22 had run only on the
// synthetic season and the wave-3 land recording).
tamper_test!(
    tamper_program_t02_flip_a_reveal_plaintext_byte,
    t02,
    march_program
);
tamper_test!(
    tamper_program_t06_set_account_on_a_province,
    t06,
    march_program
);
tamper_test!(tamper_program_t07_valid_seal_marked_bad, t07, march_program);
tamper_test!(
    tamper_program_t09_departure_mass_altered,
    t09,
    march_program
);
tamper_test!(
    tamper_program_t12_truncate_the_last_game_day,
    t12,
    march_program
);
tamper_test!(
    tamper_program_t15_origin_values_from_a_later_bell,
    t15,
    march_program
);
tamper_test!(tamper_program_t21_tampered_explore_find, t21, march_program);
tamper_test!(tamper_program_t23_forged_writeback, t23, march_program);

// W5-D: the checks of V7's new parts (the bell-by-bell skip replay, the
// holding replay) and the run fallbacks of T8, T13, T14 and T20 on the
// program's recording (no displacement, every Reveal before its anchor,
// no defence claim).
tamper_test!(
    tamper_program_t23b_forged_skip_writeback,
    t23b,
    march_program
);
tamper_test!(tamper_program_h1_forged_harvest, h1, march_program);
// Wave-5 review: the Holding forged at a write that is not an owner
// action (the replays start from the forged state and agree; only the
// continuity check sees it).
tamper_test!(
    tamper_program_h1b_forged_at_a_non_owner_write,
    h1b,
    march_program
);
// Wave-5 review: T13's last resort (any Reveal with an anchor, moved past
// its close whatever lies between): codes only.
tamper_codes_only!(
    tamper_program_t13_forced_past_the_close,
    t13_forced,
    march_program
);
tamper_test!(
    tamper_program_t08_fill_slot_changed,
    t08_fill,
    march_program
);
// The integ-head recording has no Reveal that moves unseen (its later
// transactions touch the shared ArrivalDay): the moved variant runs on the
// second recording, the forced one on the first (wave-5 review).
tamper_test!(
    tamper_program_t13_reveal_moved_past_close,
    t13_moved,
    march_program_w6base
);
tamper_test!(
    tamper_program_t14_duplicate_any_transaction,
    t14_any,
    march_program
);
tamper_test!(
    tamper_program_t20_claim_injected,
    t20_injected,
    march_program
);
tamper_test!(
    tamper_program_t01b_drop_an_unchained_transaction,
    t01b,
    march_program
);
tamper_test!(tamper_program_t03_shift_an_anchor_a, t03, march_program);
tamper_test!(
    tamper_program_t04_inject_a_second_anchor,
    t04,
    march_program
);
tamper_test!(
    tamper_program_t05_swap_a_cache_signature,
    t05,
    march_program
);
// A wrong key also leaves the program recording's seals unopenable (V5
// MissingData): no consistent forgery, codes only.
tamper_codes_only!(tamper_program_t10_wrong_quicknet_key, t10, march_program);
tamper_test!(tamper_program_t11_wrong_ruleset_hash, t11, march_program);
tamper_test!(tamper_program_t16_wrong_genesis_round, t16, march_program);
tamper_test!(tamper_program_t17_skip_over_an_arrival, t17, march_program);
tamper_test!(tamper_program_t18_tampered_ticket_score, t18, march_program);
tamper_test!(
    tamper_program_t19_tampered_terrain_digest,
    t19,
    march_program
);
tamper_test!(
    tamper_program_t22_bad_seal_logged_as_surviving,
    t22,
    march_program
);
tamper_test!(tamper_program_v9a_non_canonical_address, v9a, march_program);

/// The suite over one run (`frontier-verify tamper`, the stack's `tamper`;
/// W5-D): on the program's recording every required class T1–T22 is
/// built (fallbacks where the run lacks the fixture's selection) and
/// FAILS with its code. In a `mutate-<check>` build, exactly the classes
/// of the disabled check are missed. `VERIFY_RUN=<fixture>` runs it over
/// another run instead (a stack or nightly run saved with
/// `frontier-verify … --save`): `VERIFY_RUN=… crates/verify/mutate.sh`.
#[test]
fn tamper_suite_on_the_program_recording() {
    let run = match std::env::var("VERIFY_RUN") {
        Ok(p) if !p.is_empty() => verify_core::Input::load(std::path::Path::new(&p))
            .unwrap_or_else(|e| panic!("VERIFY_RUN: {e}")),
        _ => march_program(),
    };
    suite_on(&run);
}

/// The same suite on a second, independent recording (wave-5 review of
/// W5-D: "all 22 detected" must not be a property of one recording).
#[test]
fn tamper_suite_on_a_second_recording() {
    suite_on(&march_program_w6base());
}

fn suite_on(run: &verify_core::Input) {
    let jobs = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(8);
    let rep = tamper::run_suite(run, jobs);
    println!("{}", rep.markdown());
    assert_eq!(rep.base, Verdict::Pass, "the base run must PASS");
    for o in &rep.outcomes {
        assert_ne!(
            o.status,
            tamper::Status::NotApplicable,
            "{}: {}",
            o.class,
            o.detail
        );
        assert_ne!(
            o.status,
            tamper::Status::Panicked,
            "{}: {}",
            o.class,
            o.detail
        );
        let off = o.feature.is_some_and(disabled);
        let want = if off {
            tamper::Status::Missed
        } else {
            tamper::Status::Detected
        };
        assert_eq!(
            o.status, want,
            "{} ({}, {}): codes {:?}",
            o.class, o.variant, o.what, o.fail_codes
        );
    }
    let required = rep.outcomes.iter().filter(|o| o.required).count();
    assert_eq!(required, 22);
    if !MUTATED {
        assert!(rep.all_detected());
        assert_eq!(rep.exit_code(), verify_core::EXIT_PASS);
    }
}

/// Whether this build disables any check.
const MUTATED: bool = cfg!(any(
    feature = "mutate-v1",
    feature = "mutate-v2",
    feature = "mutate-v3",
    feature = "mutate-v4",
    feature = "mutate-v5",
    feature = "mutate-v6",
    feature = "mutate-v7",
    feature = "mutate-v8",
    feature = "mutate-v9",
    feature = "mutate-v11",
    feature = "mutate-v12",
    feature = "mutate-v13"
));
