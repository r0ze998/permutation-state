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

/// V5 (W5-D, the integ-W4 review): a revealed march whose seal the
/// verifier cannot open — no transaction carries a valid signature of
/// `T(arrive)` — is `MissingData`, not a silent pass.
#[test]
fn unopenable_seal_is_missing_data() {
    use frontier_abi::log::Kind;
    let mut inp = march_program();
    let w = verify_core::world::World::parse(
        &inp.cfg.program,
        &inp.txs,
        inp.finals.clone(),
        inp.final_slot,
    );
    let f = verify_core::facts::Facts::build(&w, &inp.cfg);
    let r = w.of(Kind::REVEAL).next().expect("a REVEAL");
    let host = r.pu64("host_id");
    let d = f.departs[&host][0];
    let round = f.tlock_round(w.recs[d].pu32("arrive_bell"));
    let sigs: Vec<(usize, [u8; 48])> = f.sigs[&round].iter().map(|s| (s.tx, s.sig)).collect();
    assert!(!sigs.is_empty());
    for (t, sig) in sigs {
        let mut tx = fclient::tx::from_wire(&inp.txs[t].tx).unwrap();
        for ci in tx.message.instructions.iter_mut() {
            if let Some(o) = ci.data.windows(48).position(|x| x == sig) {
                ci.data[o + 47] ^= 1;
            }
        }
        inp.txs[t].tx = fclient::tx::wire(&tx);
    }
    let rep = verify_core::verify(&inp);
    assert!(
        rep.findings
            .iter()
            .any(|x| x.check == "V5" && x.code == codes::MISSING_DATA),
        "{}",
        show(&rep)
    );
}

/// The skip replay asks the kernel at **every** bell of a run (W5-D, the
/// integ-W4 review): a camp that spawns at the first bell of a new day on
/// a resident's hex makes that bell not quiet, although the run's first
/// bell is — the old first-bell-only check passed such a run.
#[test]
fn skip_replay_checks_every_bell() {
    use frontier_abi::entry::{read_entry, write_entry};
    use frontier_abi::layout::province::{camp as CP, entry as E, province as PV};
    use frontier_abi::log::Kind;
    use verify_core::skip;
    let inp = march_program();
    let w = verify_core::world::World::parse(
        &inp.cfg.program,
        &inp.txs,
        inp.finals.clone(),
        inp.final_slot,
    );
    let f = verify_core::facts::Facts::build(&w, &inp.cfg);
    let mut shown = 0;
    for r in w.of(Kind::SKIP) {
        let (p, q) = r.pq();
        let Some(post) = w.txs[r.tx].post_data(&f.ctx.province(p, q)) else {
            continue;
        };
        let mut pd = post.to_vec();
        // Nothing pending, the camp gone and due at day `d`.
        let Some(e) = (0..PV::ENTRIES_N).find(|&k| {
            read_entry(&pd, k).is_ok_and(|x| {
                x.state == E::STATE_ROSTER && matches!(x.op, frontier_abi::entry::EntryOp::None)
            })
        }) else {
            continue;
        };
        if (0..PV::ENTRIES_N).any(|k| {
            read_entry(&pd, k).is_ok_and(|x| !matches!(x.op, frontier_abi::entry::EntryOp::None))
        }) {
            continue;
        }
        pd[PV::CAMP + CP::STATE] = CP::STATE_NONE;
        for d in 3..40u32 {
            let mut x = pd.clone();
            x[PV::CAMP + CP::NEXT_CHECK_DAY..PV::CAMP + CP::NEXT_CHECK_DAY + 4]
                .copy_from_slice(&d.to_le_bytes());
            let pv = fclient::decode::Province::decode(&x).unwrap();
            let t = fclient::clash_model::terrain_of(&pv).unwrap();
            let c = fclient::clash_model::camp_at(&pv, &x, &t, d * 144).unwrap();
            if !c.spawned {
                continue;
            }
            let mut ent = read_entry(&x, e).unwrap();
            ent.tile = c.tile;
            write_entry(&mut x, e, &ent).unwrap();
            let b0 = d * 144 - 1;
            // The run's first bell is quiet …
            if skip::replay(&x, b0, 1).is_err() {
                continue;
            }
            // … the next (the day's camp spawns on the resident) is not.
            match skip::replay(&x, b0, 2) {
                Err((b, _)) => assert_eq!(b, d * 144),
                Ok(_) => panic!("a camp on a resident's hex was called quiet"),
            }
            shown += 1;
            break;
        }
        if shown >= 3 {
            break;
        }
    }
    assert!(shown > 0, "no province of the recording could show it");
}
