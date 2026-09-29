//! Triage w6-s7 failure (2): V11 `CampMismatch` "a clear (troops 0) of a
//! camp in state Some(0) → Some(0)" at (0, -3) and (4, 2), bell 576.
//!
//! The fixture is cut from the w6-s7 archive (7dcacdf, `.so` 072b1205):
//! three ResolveFromInputs of bell 576 whose day check (I-56) spawned a
//! camp that the same clash cleared (residents on its hex, no arrivals),
//! each with the Province's previous writer. The program logs CAMP
//! (spawn), CAMP (clear), CLASH in one transaction (`emit_camps`), so the
//! camp the clear removes is the one spawned earlier in the same
//! transaction, not the one of the Province before it.

mod common;

use frontier_abi::log::Kind;
use verify_core::{codes, world::World};

fn slice() -> verify_core::Input {
    common::fixture("w6s7-camp-spawn-clear.json.gz")
}

fn camp_mismatches(r: &verify_core::Report) -> Vec<String> {
    r.findings
        .iter()
        .filter(|f| f.code == codes::CAMP_MISMATCH)
        .map(|f| format!("{} @{}: {}", f.entity, f.bell, f.detail))
        .collect()
}

/// The fixture is the case: a spawn then a clear of the same Province in
/// one successful transaction whose pre-state has no camp.
#[test]
fn w6s7_slice_holds_a_spawn_and_clear_in_one_resolve() {
    let inp = slice();
    let w = World::parse(&inp.cfg.program, &inp.txs, Default::default(), 0);
    let mut hits = 0;
    for t in w.txs.iter().filter(|t| t.ok) {
        let camps: Vec<_> = t
            .recs
            .iter()
            .map(|&n| &w.recs[n])
            .filter(|r| r.kind == Kind::CAMP)
            .collect();
        if camps.len() == 2 && camps[0].pu32("troops") > 0 && camps[1].pu32("troops") == 0 {
            hits += 1;
        }
    }
    assert_eq!(hits, 3, "three spawn-and-clear resolves");
}

/// Failing first on 7dcacdf: V11 must read the camp present at the clear
/// (spawned by this transaction's day check), not the pre-transaction one.
#[test]
fn v11_a_camp_spawned_and_cleared_in_one_resolve_is_no_mismatch() {
    let r = verify_core::verify(&slice());
    let bad = camp_mismatches(&r);
    assert!(bad.is_empty(), "{bad:#?}");
}

/// The check of the check: without the spawn record, the same clear of an
/// absent camp is still a `CampMismatch`.
#[test]
fn v11_a_clear_with_no_camp_and_no_spawn_still_fails() {
    let mut inp = slice();
    let w = World::parse(&inp.cfg.program, &inp.txs, Default::default(), 0);
    // (0, -3): the first CAMP record (the spawn) of its resolve.
    let (ti, pos) = w
        .recs
        .iter()
        .find(|r| r.kind == Kind::CAMP && r.pq() == (0, -3) && r.pu32("troops") > 0)
        .map(|r| (r.tx, r.pos))
        .expect("the spawn at (0, -3)");
    let logs = &mut inp.txs[ti].logs;
    let data: Vec<usize> = logs
        .iter()
        .enumerate()
        .filter(|(_, l)| l.starts_with("Program data: "))
        .map(|(i, _)| i)
        .collect();
    logs.remove(data[pos]);
    let r = verify_core::verify(&inp);
    let bad = camp_mismatches(&r);
    assert!(
        bad.iter().any(|b| b.starts_with("camp (0, -3)")),
        "{bad:#?}"
    );
}
