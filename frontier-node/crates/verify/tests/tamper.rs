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

macro_rules! tamper_test {
    ($name:ident, $f:ident, $fx:ident) => {
        #[test]
        fn $name() {
            judge(tamper::$f(&$fx()));
        }
    };
}

tamper_test!(tamper_t01_drop_a_depart, t01, march);
tamper_test!(tamper_t02_flip_a_reveal_plaintext_byte, t02, march);
tamper_test!(tamper_t03_shift_an_anchor_a, t03, land);
tamper_test!(tamper_t04_inject_a_second_anchor, t04, land);
tamper_test!(tamper_t05_swap_a_cache_signature, t05, land);
tamper_test!(tamper_t06_set_account_on_a_province, t06, march);
tamper_test!(tamper_t06b_rogue_write_between_resolves, t06b, march);
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
    let cases = tamper::all(&land(), &march());
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
