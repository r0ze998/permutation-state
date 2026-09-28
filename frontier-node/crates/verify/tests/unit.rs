//! Unit-level properties of the verifier over the recorded fixtures.

mod common;

use common::*;
use verify_core::codes;
use verify_core::input::Input;

/// Re-chaining an untampered archive changes nothing: the verifier's link
/// resolution (and the forger's tool built on it) reproduces every tail,
/// every CLOSE payload and every header the program wrote — on the real
/// program's recording as on the synthetic one.
#[test]
fn rechain_is_the_identity_on_honest_archives() {
    for (name, inp) in [
        ("land", land()),
        ("march", march()),
        ("march-program", march_program()),
    ] {
        let mut again = inp.clone();
        verify_core::tamper::rechain(&mut again);
        for (a, b) in inp.txs.iter().zip(&again.txs) {
            assert_eq!(a.logs, b.logs, "{name}: tx {} logs", a.seq);
            assert_eq!(a.post, b.post, "{name}: tx {} post-states", a.seq);
        }
        assert_eq!(inp.finals, again.finals, "{name}: finals");
    }
}

/// A fixture survives save and load byte for byte.
#[test]
fn fixture_round_trip() {
    let inp = land();
    let dir = std::env::temp_dir().join(format!("verify-rt-{}", std::process::id()));
    let p = dir.join("x.json");
    inp.save(&p).unwrap();
    let back = Input::load(&p).unwrap();
    assert_eq!(back.cfg, inp.cfg);
    assert_eq!(back.txs, inp.txs);
    assert_eq!(back.finals, inp.finals);
    assert_eq!(back.program_hashes, inp.program_hashes);
    // gzip (the program recording's form)
    let pz = dir.join("x.json.gz");
    inp.save(&pz).unwrap();
    let back = Input::load(&pz).unwrap();
    assert_eq!(back.txs, inp.txs);
    assert_eq!(back.finals, inp.finals);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The report's JSON is the §8.5 schema, and FAIL/warn codes are distinct.
#[test]
fn report_schema_and_codes() {
    let mut inp = land();
    inp.cfg.ruleset_hash = [9; 32];
    let r = verify_core::verify(&inp);
    let j = r.json();
    assert_eq!(j["verdict"], "FAIL");
    for k in [
        "tx",
        "failed_tx",
        "seals",
        "reveals",
        "bad_seals",
        "clashes",
        "skips",
        "tickets",
        "explores",
    ] {
        assert!(j["counts"].get(k).is_some(), "counts.{k}");
    }
    for k in [
        "valid_unrevealed",
        "reveals_near_close",
        "max_anchor_delay_s",
        "contested_bells",
    ] {
        assert!(j["liveness"].get(k).is_some(), "liveness.{k}");
    }
    let f = &j["findings"][0];
    for k in ["code", "severity", "entity", "bell", "signature", "detail"] {
        assert!(f.get(k).is_some(), "finding.{k}");
    }
    assert_eq!(f["severity"], "fail");
    assert!(r.markdown().contains("RulesetMismatch"));
    assert_eq!(verify_core::exit_code(r.verdict), verify_core::EXIT_FAIL);
    let all: std::collections::BTreeSet<_> =
        codes::FAIL_CODES.iter().chain(codes::WARN_CODES).collect();
    assert_eq!(all.len(), codes::FAIL_CODES.len() + codes::WARN_CODES.len());
}

/// Missing post-states (a public-RPC archive) make the replay checks say
/// "cannot verify" instead of passing (fail closed, K2).
#[test]
fn no_post_states_is_unverifiable() {
    let mut inp = march();
    for t in inp.txs.iter_mut() {
        t.post.clear();
    }
    let r = verify_core::verify(&inp);
    assert_eq!(
        r.verdict,
        verify_core::Verdict::Unverifiable,
        "{}",
        show(&r)
    );
    assert!(r
        .findings
        .iter()
        .any(|f| f.code == codes::MISSING_DATA && f.check == "V7"));
}

/// A record that does not decode is a FAIL (K2).
#[test]
fn undecodable_record_fails() {
    let mut inp = land();
    let t = inp
        .txs
        .iter()
        .position(|t| t.logs.iter().any(|l| l.starts_with("Program data: ")))
        .unwrap();
    let l = inp.txs[t]
        .logs
        .iter()
        .position(|l| l.starts_with("Program data: "))
        .unwrap();
    // PS2 tag, then a body of kind 99.
    inp.txs[t].logs[l] = fclient::log::log_line(&[1, 99, 0, 0, 0, 0, 0]);
    let r = verify_core::verify(&inp);
    assert!(r.fails_with(codes::UNDECODABLE), "{}", show(&r));
}
