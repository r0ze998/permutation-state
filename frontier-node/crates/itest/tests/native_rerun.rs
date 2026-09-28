//! `itest::native` (G14's native kernel at every bell) over the committed
//! program recording `fixtures/verify/march-program.json.gz` (a strict
//! `inproc_day` of the program, read only): every CLASH and every skipped
//! bell re-runs to the chain's bytes; and it is not blind — a Province a
//! SKIP left with one byte changed, a CLASH digest altered, a roster the
//! kernel would fight, and a bell resolved twice are each caught.

use frontier_abi::layout::province::{entry as E, province as PL};
use frontier_abi::log::Kind;
use itest::native::{rerun, NativeRerun};
use verify_core::{tamper, Input};

fn recording() -> Input {
    Input::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/verify/march-program.json.gz"),
    )
    .expect("the program recording")
}

fn run(inp: &Input) -> NativeRerun {
    rerun(&inp.txs, &inp.cfg.program)
}

/// The transaction of the first SKIP whose run is `n ≥ min_n` bells, and
/// the Province account it wrote.
fn a_skip(inp: &Input, min_n: u8) -> (usize, usize) {
    for (t, i) in tamper::find(inp, Kind::SKIP) {
        let b = tamper::bodies(inp, t);
        let r = frontier_abi::log::decode(&b[i]).unwrap();
        if r.payload[4] >= min_n {
            let k = inp.txs[t]
                .post
                .iter()
                .position(|(_, a)| a.as_ref().is_some_and(|a| a.data[..8] == PL::MAGIC))
                .unwrap();
            return (t, k);
        }
    }
    panic!("no SKIP of {min_n} bells");
}

#[test]
fn every_resolved_and_skipped_bell_reruns_natively() {
    let inp = recording();
    let n = run(&inp);
    println!("{}", n.to_json());
    assert!(n.ok(), "{}", n.to_json());
    assert!(n.clashes >= 50 && n.clashes_matched == n.clashes);
    assert!(n.skips >= 500 && n.skips_matched == n.skips);
    assert!(n.skipped_bells > n.skips, "multi-bell skips");
    assert!(n.skipped_bells_changed > 0, "skips that settled something");
    assert!(n.resolved_next.len() >= 30);
}

#[test]
fn a_province_a_skip_left_differently_is_caught() {
    let mut inp = recording();
    let (t, k) = a_skip(&inp, 2);
    if let Some(a) = inp.txs[t].post[k].1.as_mut() {
        // The camp's troops, inside the state the quiet digest covers.
        a.data[PL::CAMP + 4] ^= 0x01;
    }
    let n = run(&inp);
    assert!(
        n.mismatches
            .iter()
            .any(|m| m.contains("skip") && m.contains("another Province")),
        "{:?}",
        n.mismatches
    );
}

#[test]
fn a_clash_digest_altered_is_caught() {
    let mut inp = recording();
    let (t, i) = tamper::find(&inp, Kind::CLASH)[3];
    tamper::edit_payload(&mut inp, t, i, |p| p[5] ^= 0x10);
    let n = run(&inp);
    assert!(
        n.mismatches.iter().any(|m| m.contains("outcome digest")),
        "{:?}",
        n.mismatches
    );
}

#[test]
fn a_skip_over_a_fight_is_loud() {
    // Two hostile residents on one hex before a skip: the kernel does not
    // find those bells quiet (the program would refuse NotQuiet).
    let mut inp = recording();
    let (t, _) = a_skip(&inp, 1);
    // The Province the skip read: the last post-state before it.
    let key = inp.txs[t]
        .post
        .iter()
        .find(|(_, a)| a.as_ref().is_some_and(|a| a.data[..8] == PL::MAGIC))
        .map(|(k, _)| *k)
        .unwrap();
    let before = (0..t)
        .rev()
        .find(|&u| inp.txs[u].err.is_none() && inp.txs[u].post.iter().any(|(k, _)| *k == key))
        .unwrap();
    let a = inp.txs[before]
        .post
        .iter_mut()
        .find(|(k, _)| *k == key)
        .and_then(|(_, a)| a.as_mut())
        .unwrap();
    let rn = u32::from_le_bytes(
        a.data[PL::RESOLVED_NEXT..PL::RESOLVED_NEXT + 4]
            .try_into()
            .unwrap(),
    );
    let mut put = |i: usize, id: u64, faction: u8| {
        let o = PL::entry(i);
        let e = &mut a.data[o..o + E::SIZE];
        e.fill(0);
        e[E::ID..E::ID + 8].copy_from_slice(&id.to_le_bytes());
        e[E::FACTION] = faction;
        e[E::TILE] = 30;
        e[E::STATE] = E::STATE_ROSTER;
        e[E::TROOPS..E::TROOPS + 4].copy_from_slice(&500_000u32.to_le_bytes());
        e[E::STAMINA_VALUE..E::STAMINA_VALUE + 2]
            .copy_from_slice(&permutation_rules::frontier::host::STAMINA_CAP.to_le_bytes());
        e[E::STAMINA_BELL..E::STAMINA_BELL + 4].copy_from_slice(&rn.to_le_bytes());
        e[E::FROM_BELL..E::FROM_BELL + 4].copy_from_slice(&0u32.to_le_bytes());
    };
    put(40, 0x7777_0001, 0);
    put(41, 0x7777_0002, 3);
    let n = run(&inp);
    assert!(!n.loud_bells.is_empty(), "{}", n.to_json());
}

#[test]
fn a_bell_resolved_twice_breaks_coverage() {
    let mut inp = recording();
    let (t, _) = tamper::find(&inp, Kind::CLASH)[0];
    let dup = inp.txs[t].clone();
    inp.txs.insert(t + 1, dup);
    let n = run(&inp);
    assert!(!n.coverage.is_empty(), "{}", n.to_json());
}
