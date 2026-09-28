//! G7 (§13.3) in LiteSVM, W5-A: **lag invariance through the program**.
//! One march from an origin Province to a destination with a hostile
//! resident, run twice on the test-beacon binary:
//!
//! - **unheld**: the origin resolves its departure bell (the crafted
//!   stand-in of `world::holding`, as every transit test), SettleDeparture
//!   lands at once, the origin region's anchor is posted, then the march is
//!   revealed and the destination gathers and resolves at its close;
//! - **held**: the origin Province, the origin region's anchor and
//!   SettleDeparture are all held **past the destination's close** (the
//!   reveal still lands: Reveal accepts a transit whose departure is not
//!   settled). While held, the destination's gather is refused
//!   `DepartureUnsettled` (the hold took effect: lag waits); three bells
//!   later the holds are lifted in the order a late keeper would, and the
//!   destination gathers and resolves.
//!
//! The destination's result must be byte-identical: the ClashInputs'
//! records, the `CLASH` record's input and outcome digests, and the
//! Province's game state (every byte but the event-chain header and the
//! resolve summary, which records when and by whom it resolved).

mod common;

use frontier_abi::layout::clash::clash_inputs as CI;
use frontier_abi::layout::province::province as P;
use frontier_abi::log::Kind;
use permutation_frontier_svm_tests::chain::{assert_code, expect_lands, Chain};
use permutation_frontier_svm_tests::fixtures::tlock::SealCase;
use permutation_frontier_svm_tests::ix::host as hix;
use permutation_frontier_svm_tests::records;
use permutation_frontier_svm_tests::world::clash::THIRDS;
use permutation_frontier_svm_tests::world::holding::open_path;
use permutation_frontier_svm_tests::world::World;
use permutation_frontier_svm_tests::{FrontierError as E, Signer};

const B0: u32 = 10;
const LEAD: u32 = 6;
const HOME: (i16, i16) = (2, 0);
const DIRS: [u8; 2] = [0, 0];

/// What the destination's resolve produced.
#[derive(Debug, PartialEq, Eq)]
struct Result {
    inputs: Vec<u8>,
    input_digest: Vec<u8>,
    outcome_digest: Vec<u8>,
    province: Vec<u8>,
}

fn run(hold: bool) -> Result {
    let mut c = Chain::test_beacon();
    let w = World::running(&mut c, 1);
    w.to_bell(&mut c, B0, 5);
    // The march: a 900-troop host of faction 0, two provinces east.
    let e = w.craft_estate(&mut c, "lag-a", 0, HOME, 0);
    let origin = (e.p as i32, e.q as i32, e.tile);
    let steps = permutation_frontier_svm_tests::world::holding::trace(origin, &DIRS);
    let last = *steps.last().unwrap();
    let dest = (last.p, last.q);
    // The defender: a faction-1 estate at the destination with a resident
    // host on the arrival tile.
    let d = w.craft_estate(&mut c, "lag-d", 1, (dest.0 as i16, dest.1 as i16), 0);
    open_path(&w, &mut c, origin, &DIRS);
    w.craft_host(&mut c, &d, &d.province, 0, 0, 0, 600, last.tile);
    let id = w.craft_host(&mut c, &e, &e.province, 0, 0, 0, 900, e.tile);
    let arrive = B0 + LEAD;
    let m = w.plan_march(id, 1, origin, &DIRS, arrive, 1, 0, SealCase::Valid);
    let tip = w.tip_min(&c);
    expect_lands(
        c.send(&[w.depart_ix(&e, HOME, &m, tip)], &[&e.wallet]),
        "Depart",
    );
    let settle = hix::settle_departure(&w.a, w.keeper.pubkey(), HOME, e.href(), 1);
    let origin_region = World::region(origin.0, origin.1);
    let dest_region = World::region(dest.0, dest.1);
    let release = |c: &mut permutation_frontier_svm_tests::chain::Chain| {
        w.resolve_through(c, &e.province, B0);
        expect_lands(
            c.send(std::slice::from_ref(&settle), &[&w.keeper]),
            "SettleDeparture",
        );
        if w.anchor_a(c, B0, origin_region).is_none() {
            expect_lands(w.post_anchor(c, B0, origin_region), "origin anchor");
        }
    };
    if !hold {
        release(&mut c);
    }
    // Reveal inside the destination's window.
    if w.anchor_a(&c, arrive, dest_region).is_none() {
        expect_lands(w.post_anchor(&mut c, arrive, dest_region), "PostAnchor");
    }
    let rv = w.reveal_ix(&w.keeper.pubkey(), &e, &m, 0, true);
    expect_lands(c.send(&[rv], &[&w.keeper]), "Reveal");
    // The destination resolved through arrive − 1 (both runs alike).
    w.set_resolved_next(&mut c, &d.province, arrive);
    w.ready_bell(&mut c, arrive, dest_region, None);
    if hold {
        // Past the close, the departure still unsettled: the gather waits.
        let g = w.gather_parts(&c, dest, arrive, &THIRDS);
        assert_code(
            c.fork().send(&[g[0].clone()], &[&w.keeper]),
            E::DepartureUnsettled,
        );
        c.advance(3 * 600);
        release(&mut c);
    }
    for ix in w.gather_parts(&c, dest, arrive, &THIRDS) {
        expect_lands(c.send(&[ix], &[&w.keeper]), "GatherClash");
    }
    let l = expect_lands(
        c.send(&[w.resolve_ix(dest, arrive)], &[&w.keeper]),
        "ResolveFromInputs",
    );
    let clash = records::one(&l.logs, Kind::CLASH);
    let ci = c.data(&w.a.clash_inputs(dest.0, dest.1, arrive));
    assert_eq!(
        ci[CI::N_PRESENT],
        1,
        "the march is in the destination's inputs"
    );
    assert!(
        clash.u64("engagements") > 0,
        "the arrival fought the resident"
    );
    let pd = c.data(&d.province);
    let mut province = pd[64..P::RESOLVE_SUMMARY].to_vec();
    province.extend_from_slice(&pd[P::RESOLVE_SUMMARY + 32..]);
    Result {
        inputs: ci[CI::ARRIVALS..CI::RESOLVER].to_vec(),
        input_digest: clash.field("input_digest", true).to_vec(),
        outcome_digest: clash.field("outcome_digest", true).to_vec(),
        province,
    }
}

#[test]
fn g07_lag_gate_in_litesvm() {
    let unheld = run(false);
    let held = run(true);
    assert_eq!(unheld.input_digest, held.input_digest, "CLASH input digest");
    assert_eq!(
        unheld.outcome_digest, held.outcome_digest,
        "CLASH outcome digest"
    );
    assert_eq!(unheld.inputs, held.inputs, "ClashInputs records");
    assert_eq!(
        unheld.province, held.province,
        "the destination's game state"
    );
    let _ = common::prefunds(0);
    println!(
        "G7: destination outcome digest {} identical held and unheld",
        unheld
            .outcome_digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
}
