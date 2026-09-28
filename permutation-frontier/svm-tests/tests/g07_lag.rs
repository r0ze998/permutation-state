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
//!
//! Wave-5 review (§13.3 lists G7 as a property): the pair runs over
//! [`CASES`] — arrival bells (so seeds), hold lengths and march sizes,
//! a weak march against a strong resident, a second arrival of another
//! faction from another Holding (never held), and cases whose origin
//! resolves its departure bell through the real GatherClash +
//! ResolveFromInputs of the origin instead of the crafted stand-in. The
//! suite must see at least one arrival that stays and one that does not.

mod common;

use frontier_abi::layout::clash::{arrival_slot as AS, clash_inputs as CI};
use frontier_abi::layout::province::province as P;
use frontier_abi::log::Kind;
use permutation_frontier_svm_tests::chain::{assert_code, expect_lands, Chain};
use permutation_frontier_svm_tests::fixtures::tlock::SealCase;
use permutation_frontier_svm_tests::ix::host as hix;
use permutation_frontier_svm_tests::records;
use permutation_frontier_svm_tests::world::clash::THIRDS;
use permutation_frontier_svm_tests::world::holding::{find_path, open_path};
use permutation_frontier_svm_tests::world::World;
use permutation_frontier_svm_tests::{FrontierError as E, Signer};

const B0: u32 = 10;
const HOME: (i16, i16) = (2, 0);
const DIRS: [u8; 2] = [0, 0];

/// One G7 pair.
#[derive(Clone, Copy, Debug)]
struct Case {
    /// Arrival bell − B0 (the seed and the anchors differ per bell).
    lead: u32,
    /// Troops of the held march and of the defending resident.
    troops: u32,
    defender: u32,
    /// Bells the origin, its anchor and SettleDeparture are held past the
    /// destination's close.
    hold_bells: i64,
    /// A second march (faction 2, another Holding in HOME) onto the same
    /// tile in the same bell, never held.
    second: bool,
    /// The origin resolves its departure bell through the real
    /// GatherClash + ResolveFromInputs (else the crafted stand-in).
    real_origin: bool,
}

const CASES: [Case; 4] = [
    Case {
        lead: 6,
        troops: 900,
        defender: 600,
        hold_bells: 3,
        second: false,
        real_origin: false,
    },
    Case {
        lead: 7,
        troops: 200,
        defender: 1_800,
        hold_bells: 1,
        second: false,
        real_origin: true,
    },
    Case {
        lead: 8,
        troops: 900,
        defender: 600,
        hold_bells: 5,
        second: true,
        real_origin: true,
    },
    Case {
        lead: 6,
        troops: 2_400,
        defender: 300,
        hold_bells: 2,
        second: true,
        real_origin: false,
    },
];

/// What the destination's resolve produced.
#[derive(Debug, PartialEq, Eq)]
struct Result {
    inputs: Vec<u8>,
    input_digest: Vec<u8>,
    outcome_digest: Vec<u8>,
    province: Vec<u8>,
    /// Fates of the gathered positions (`clash_inputs` fate codes).
    fates: Vec<u8>,
}

fn run(k: Case, hold: bool) -> Result {
    let lead = k.lead;
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
    w.craft_host(&mut c, &d, &d.province, 0, 0, 0, k.defender, last.tile);
    let id = w.craft_host(&mut c, &e, &e.province, 0, 0, 0, k.troops, e.tile);
    let arrive = B0 + lead;
    let m = w.plan_march(id, 1, origin, &DIRS, arrive, 1, 0, SealCase::Valid);
    let tip = w.tip_min(&c);
    expect_lands(
        c.send(&[w.depart_ix(&e, HOME, &m, tip)], &[&e.wallet]),
        "Depart",
    );
    let origin_region = World::region(origin.0, origin.1);
    let dest_region = World::region(dest.0, dest.1);
    // Resolves `province` through B0 (the real gather + resolve of the
    // origin, or the crafted stand-in), lands the SettleDeparture of
    // transit slot 1 of `est` and makes sure the origin region's anchor of
    // B0 exists.
    let settle_origin =
        |c: &mut Chain, est: &permutation_frontier_svm_tests::world::holding::Estate| {
            let o = (est.p as i32, est.q as i32);
            let region = World::region(o.0, o.1);
            if k.real_origin {
                w.set_resolved_next(c, &est.province, B0);
                w.ready_bell(c, B0, region, None);
                for ix in w.gather_parts(c, o, B0, &THIRDS) {
                    expect_lands(c.send(&[ix], &[&w.keeper]), "GatherClash (origin)");
                }
                expect_lands(
                    c.send(&[w.resolve_ix(o, B0)], &[&w.keeper]),
                    "ResolveFromInputs (origin)",
                );
            } else {
                w.resolve_through(c, &est.province, B0);
            }
            let settle =
                hix::settle_departure(&w.a, w.keeper.pubkey(), (est.p, est.q), est.href(), 1);
            expect_lands(c.send(&[settle], &[&w.keeper]), "SettleDeparture");
            if w.anchor_a(c, B0, region).is_none() {
                expect_lands(w.post_anchor(c, B0, region), "origin anchor");
            }
        };
    // The second march: faction 2 from a Province next to the destination
    // (another region, so the first march's anchor hold stays a hold),
    // onto the same tile in the same bell; its origin is never held.
    let second = if k.second {
        let home2 = [(1, 0), (0, 1), (-1, 1), (0, -1), (1, -1)]
            .iter()
            .map(|(dp, dq)| ((dest.0 + dp) as i16, (dest.1 + dq) as i16))
            .find(|&(p, q)| {
                World::region(p as i32, q as i32) != origin_region
                    && (p as i32, q as i32) != (origin.0, origin.1)
            })
            .expect("a neighbour in another region");
        let e2 = w.craft_estate(&mut c, "lag-b", 2, home2, 0);
        let from = (e2.p as i32, e2.q as i32, e2.tile);
        let dirs = find_path(&c, &w.a, from, (dest.0, dest.1, last.tile)).expect("a path");
        open_path(&w, &mut c, from, &dirs);
        let id2 = w.craft_host(&mut c, &e2, &e2.province, 0, 0, 0, 700, e2.tile);
        let m2 = w.plan_march(id2, 1, from, &dirs, arrive, 1, 0, SealCase::Valid);
        expect_lands(
            c.send(&[w.depart_ix(&e2, home2, &m2, tip)], &[&e2.wallet]),
            "Depart (second)",
        );
        settle_origin(&mut c, &e2);
        Some((e2, m2))
    } else {
        None
    };
    if !hold {
        settle_origin(&mut c, &e);
    }
    // Reveal inside the destination's window.
    if w.anchor_a(&c, arrive, dest_region).is_none() {
        expect_lands(w.post_anchor(&mut c, arrive, dest_region), "PostAnchor");
    }
    let rv = w.reveal_ix(&w.keeper.pubkey(), &e, &m, 0, true);
    expect_lands(c.send(&[rv], &[&w.keeper]), "Reveal");
    if let Some((e2, m2)) = &second {
        let rv2 = w.reveal_ix(&w.keeper.pubkey(), e2, m2, 0, true);
        expect_lands(c.send(&[rv2], &[&w.keeper]), "Reveal (second)");
    }
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
        c.advance(k.hold_bells * 600);
        settle_origin(&mut c, &e);
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
        ci[CI::N_PRESENT] as usize,
        1 + second.is_some() as usize,
        "{k:?}: the marches are in the destination's inputs"
    );
    assert!(
        clash.u64("engagements") > 0,
        "{k:?}: the arrival fought the resident"
    );
    let packed: [u8; 9] = clash.field("fates", true).try_into().expect("9 B");
    let fates = frontier_abi::log::unpack_fates(&packed).to_vec();
    let _ = AS::SIZE;
    let pd = c.data(&d.province);
    let mut province = pd[64..P::RESOLVE_SUMMARY].to_vec();
    province.extend_from_slice(&pd[P::RESOLVE_SUMMARY + 32..]);
    Result {
        inputs: ci[CI::ARRIVALS..CI::RESOLVER].to_vec(),
        input_digest: clash.field("input_digest", true).to_vec(),
        outcome_digest: clash.field("outcome_digest", true).to_vec(),
        province,
        fates,
    }
}

#[test]
fn g07_lag_gate_in_litesvm() {
    use frontier_abi::layout::clash::arrival::{
        FATE_BOUNCED, FATE_DESTROYED, FATE_NONE, FATE_STAYS,
    };
    let (mut stayed, mut moved_on, mut bounced, mut destroyed) = (0, 0, 0, 0);
    for k in CASES {
        let unheld = run(k, false);
        let held = run(k, true);
        assert_eq!(
            unheld.input_digest, held.input_digest,
            "{k:?}: CLASH input digest"
        );
        assert_eq!(
            unheld.outcome_digest, held.outcome_digest,
            "{k:?}: CLASH outcome digest"
        );
        assert_eq!(unheld.inputs, held.inputs, "{k:?}: ClashInputs records");
        assert_eq!(unheld.fates, held.fates, "{k:?}: fates");
        assert_eq!(
            unheld.province, held.province,
            "{k:?}: the destination's game state"
        );
        for f in &unheld.fates {
            match *f {
                FATE_NONE => {}
                FATE_STAYS => stayed += 1,
                x => {
                    moved_on += 1;
                    bounced += (x == FATE_BOUNCED) as u32;
                    destroyed += (x == FATE_DESTROYED) as u32;
                }
            }
        }
        println!(
            "G7 {k:?}: outcome digest {} identical held and unheld; fates {:?}",
            unheld
                .outcome_digest
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
            unheld
                .fates
                .iter()
                .filter(|f| **f != FATE_NONE)
                .collect::<Vec<_>>()
        );
    }
    assert!(stayed > 0, "some arrival stays");
    assert!(
        moved_on > 0,
        "some arrival is bounced, retreats or is destroyed"
    );
    assert!(bounced > 0, "some arrival is bounced");
    assert!(destroyed > 0, "some arrival is destroyed");
    let _ = common::prefunds(0);
}
