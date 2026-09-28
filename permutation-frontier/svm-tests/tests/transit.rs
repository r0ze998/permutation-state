//! SettleTransit (0x54) and SweepPoolOwed (0x55), W4-B, on the test-beacon
//! build (any round, I-53): G10 (the program's seal code equals the stock
//! `tlock` opener's, before and after ArchiveAnchors), G12 (every branch:
//! fates, bounce by rank, routs, bad seals, settle racing the proof, a
//! Stays host blocked until settled, drained recipients, claim after
//! settle), G1 (the §13.1 worst cases), G13 codes.
//!
//! Crafted state (`world::transit`, module note): the origin's resolve of
//! the departure bell, the destination's resolved ClashInputs and
//! Stays/Withdrew entries, and a skipped destination — ResolveFromInputs,
//! GatherClash and SkipQuiet are W4-A's (same wave). Everything else goes
//! through the program: Depart, SettleDeparture, PostAnchor, PostSeed,
//! Reveal, ArchiveAnchors, SettleTransit, SweepPoolOwed.

mod common;

use frontier_abi::entry::{read_entry, EntryOp};
use frontier_abi::layout::clash::{arrival as AR, arrival_slot as AS, clash_inputs as CI};
use frontier_abi::layout::player::{holding as H, transit as T};
use frontier_abi::layout::province::entry as EN;
use frontier_abi::layout::world::defence_pool as DP;
use frontier_abi::log::{seal_code, transit_outcome as TO, EntityKind, Kind};
use frontier_abi::tags::Ix;
use permutation_frontier_svm_tests::budget::{assert_within, ceilings};
use permutation_frontier_svm_tests::chain::{
    assert_code, expect_lands, with_account, Chain, Landed, Profile, SendResult,
};
use permutation_frontier_svm_tests::fixtures::tlock::{SealCase, SealKit};
use permutation_frontier_svm_tests::ix::transit::{at, sweep_pool_owed};
use permutation_frontier_svm_tests::records::{self, ChainWatch};
use permutation_frontier_svm_tests::world::holding::{u32_at, u64_at};
use permutation_frontier_svm_tests::world::transit::{free_entry, Rec, Trip, EAST8};
use permutation_frontier_svm_tests::world::World;
use permutation_frontier_svm_tests::{Address, FrontierError as E, Instruction, Keypair, Signer};
use permutation_rules::frontier::host::rout_survivors;

const B0: u32 = 10;
/// Troops of every test host (whole).
const TROOPS: u32 = 900;
const MILLI: u32 = TROOPS * 1_000;

fn world() -> (Chain, World) {
    let (mut c, w) = common::test_beacon();
    w.to_bell(&mut c, B0, 5);
    (c, w)
}

/// A march of estate `label` on site `site` of (2, 0), sealed as `case`.
fn trip(c: &mut Chain, w: &World, label: &str, site: u8, case: SealCase) -> Trip {
    w.trip(c, label, 0, (2, 0), site, TROOPS, &EAST8, 6, case)
}

fn send(c: &mut Chain, ix: Instruction, k: &Keypair) -> SendResult {
    c.send(&[ix], &[k])
}

/// The transit record's state.
fn transit_state(c: &Chain, t: &Trip) -> u8 {
    c.data(&t.e.holding)[H::transit(t.m.transit_slot as usize) + T::STATE]
}

fn ts(l: &Landed) -> records::Rec {
    records::one(&l.logs, Kind::TRANSIT_SETTLED)
}

fn funded(c: &mut Chain, label: &str) -> Keypair {
    c.funded(format!("w4b-{label}").as_bytes(), 1)
}

/// The province entry of a host, if any (states 1–3).
fn entry_of(c: &Chain, province: &Address, host: u64) -> Option<frontier_abi::entry::Entry> {
    let d = c.data(province);
    frontier_abi::entry::find_entry(&d, host).map(|i| read_entry(&d, i).unwrap())
}

fn dest_key(w: &World, t: &Trip) -> Address {
    w.a.province(t.dest().0, t.dest().1)
}

fn inputs_key(w: &World, t: &Trip) -> Address {
    w.a.clash_inputs(t.dest().0, t.dest().1, t.arrive())
}

fn slot_key(w: &World, t: &Trip, i: u8) -> Address {
    w.a.arrival_slot(t.dest().0, t.dest().1, t.arrive(), t.faction(), i)
}

// ------------------------------------------------------------ fates

/// A revealed host that stays: the slot's beneficiary gets the tip, the
/// resolver the march fee, the rent payer the bond; the final set's
/// settled bit is set, the slot closed to its `rent_to`, the transit
/// freed, and the host may act at its destination from now on.
#[test]
fn g12_settle_stays_pays_and_frees_the_host() {
    let (mut c, w) = world();
    let t = trip(&mut c, &w, "stay", 0, SealCase::Valid);
    expect_lands(w.trip_reveal(&mut c, &t, 0), "Reveal");
    let resolver = funded(&mut c, "resolver");
    w.craft_resolved_inputs(
        &mut c,
        &t,
        &[Rec::of(&t, 0, AR::FATE_STAYS, 850_000)],
        &resolver.pubkey(),
    );
    w.craft_stayer(&mut c, &t, 850_000);
    // Before the settlement the host cannot act at its destination.
    let dest = dest_key(&w, &t);
    let b = w.bell(&c);
    w.set_resolved_next(&mut c, &dest, b);
    let dissolve = |w: &World, t: &Trip| {
        // The host stands at its destination (account 5).
        with_account(
            fclient::ix::dissolve(&w.a, &t.e.player(), t.href(), t.m.host_id),
            5,
            dest_key(w, t),
        )
    };
    assert_code(
        send(&mut c, dissolve(&w, &t), &t.e.wallet),
        E::HostInTransit,
    );
    // Too early: before close + 600.
    let keeper = w.keeper.pubkey();
    let mut s = w.settle_default();
    s.resolver = resolver.pubkey();
    let ix = w.settle_ix(&t, &keeper, &s);
    assert_code(send(&mut c, ix.clone(), &w.keeper), E::TooEarly);
    w.to_settle(&mut c, &t, 0);
    let b = w.bell(&c);
    w.set_resolved_next(&mut c, &dest, b);
    let hw = ChainWatch::new(&c, t.e.holding, EntityKind::Holding);
    let iw = ChainWatch::new(&c, inputs_key(&w, &t), EntityKind::ClashInputs);
    let (k0, r0, p0) = (
        c.lamports(&keeper),
        c.lamports(&resolver.pubkey()),
        c.lamports(&t.e.wallet.pubkey()),
    );
    let slot_rent = c.lamports(&slot_key(&w, &t, 0));
    let ix = w.settle_ix(&t, &keeper, &s);
    let l = expect_lands(send(&mut c, ix.clone(), &w.keeper), "w.settle_ix(");
    let r = ts(&l);
    assert_eq!(r.u64("outcome") as u8, TO::STAYS);
    assert_eq!(r.u64("seal_code") as u8, seal_code::VALID);
    assert_eq!(r.u64("troops"), 850_000);
    assert_eq!(r.u64("tip"), t.tip);
    assert_eq!(r.u64("fee"), 10_000);
    assert_eq!(r.u64("bond"), 20_000);
    assert_eq!(r.u64("reward"), 0);
    assert_eq!(r.u64("pool_owed_delta"), 0);
    assert_eq!(r.u64("slot_kept"), 0);
    // Payments: the keeper (slot beneficiary and rent_to) got the tip and
    // the slot's rent back, minus the fee it paid.
    assert_eq!(c.lamports(&keeper) + l.fee, k0 + t.tip + slot_rent);
    assert_eq!(c.lamports(&resolver.pubkey()), r0 + 10_000);
    assert_eq!(c.lamports(&t.e.wallet.pubkey()), p0 + 20_000);
    assert!(c.is_absent(&slot_key(&w, &t, 0)), "slot closed to rent_to");
    let close = records::one(&l.logs, Kind::CLOSE);
    assert_eq!(
        close.key[0],
        frontier_abi::layout::AccountKind::ArrivalSlot as u8
    );
    // The Holding: transit freed, escrow released; the final set settled.
    assert_eq!(transit_state(&c, &t), T::STATE_FREE);
    assert_eq!(u64_at(&c.data(&t.e.holding), H::ESCROW), 0);
    let cd = c.data(&inputs_key(&w, &t));
    assert_eq!(u32_at(&cd, CI::SETTLED_MASK), 1 << CI::position(0, 0));
    hw.check(&c, &l.logs, 1);
    iw.check(&c, &l.logs, 1);
    // A repeat finds the record free.
    assert_code(send(&mut c, ix, &w.keeper), E::TransitState);
    // The host acts at its destination now.
    expect_lands(
        send(&mut c, dissolve(&w, &t), &t.e.wallet),
        "Dissolve after the settlement",
    );
}

/// Bounced and Retreated hosts return home as muster-pending entries with
/// the final set's `troops_after`; a Destroyed host returns nothing;
/// Withdrew stays at the destination. Every fate pays like Stays.
#[test]
fn g12_settle_fates_return_or_keep_the_host() {
    for (k, fate, after) in [
        (0u8, AR::FATE_WITHDREW, 800_000u32),
        (1, AR::FATE_BOUNCED, MILLI),
        (2, AR::FATE_RETREATED, 600_000),
        (3, AR::FATE_DESTROYED, 0),
    ] {
        let (mut c, w) = world();
        let t = trip(&mut c, &w, &format!("fate{k}"), 0, SealCase::Valid);
        expect_lands(w.trip_reveal(&mut c, &t, 0), "Reveal");
        let keeper = w.keeper.pubkey();
        w.craft_resolved_inputs(&mut c, &t, &[Rec::of(&t, 0, fate, after)], &keeper);
        if fate == AR::FATE_WITHDREW {
            w.craft_stayer(&mut c, &t, after);
        }
        w.to_settle(&mut c, &t, 0);
        let home = t.e.province;
        let pw = ChainWatch::new(&c, home, EntityKind::Province);
        let l = expect_lands(
            send(
                &mut c,
                w.settle_ix(&t, &keeper, &w.settle_default()),
                &w.keeper,
            ),
            "w.settle_ix(",
        );
        let r = ts(&l);
        assert_eq!(r.u64("outcome") as u8, fate, "fate {fate}");
        assert_eq!(r.u64("tip"), t.tip);
        let back = entry_of(&c, &home, t.m.host_id);
        match fate {
            AR::FATE_BOUNCED | AR::FATE_RETREATED => {
                let e = back.expect("returned home");
                assert_eq!(e.state, EN::STATE_MUSTER_PENDING);
                assert_eq!(e.troops, after);
                assert_eq!(e.from_bell, w.bell(&c) + 1);
                assert_eq!(e.op, EntryOp::None);
                assert_eq!(r.u64("troops"), after as u64);
                pw.check(&c, &l.logs, 1);
            }
            AR::FATE_WITHDREW => {
                assert!(back.is_none());
                assert!(entry_of(&c, &dest_key(&w, &t), t.m.host_id).is_some());
            }
            _ => {
                assert!(back.is_none(), "destroyed: nothing returns");
                assert_eq!(r.u64("troops"), 0);
            }
        }
        assert_eq!(transit_state(&c, &t), T::STATE_FREE);
    }
}

// ------------------------------------------------------------ not in the final set

/// Four larger arrivals of the faction fill the final set: the host was
/// outranked, so it bounces with no loss (tip and bond to the rent payer,
/// fee to the resolver). The resolver is a drained wallet: its fee is
/// diverted to `pool_owed` (`DIVERT`), and SweepPoolOwed moves it to the
/// DefencePool.
#[test]
fn g12_settle_bounce_by_rank_and_a_drained_resolver() {
    let (mut c, w) = world();
    let t = trip(&mut c, &w, "rank", 0, SealCase::Valid);
    w.trip_anchor(&mut c, &t);
    let drained = Keypair::new_from_array([0x4d; 32]);
    let set: Vec<Rec> = (0..4)
        .map(|i| {
            Rec::other(
                0,
                i,
                0x7000 + i as u64,
                0x1000 + i as u64,
                950_000,
                AR::FATE_STAYS,
            )
        })
        .collect();
    w.craft_resolved_inputs(&mut c, &t, &set, &drained.pubkey());
    w.to_settle(&mut c, &t, 0);
    let mut s = w.settle_default();
    s.resolver = drained.pubkey();
    let p0 = c.lamports(&t.e.wallet.pubkey());
    let l = expect_lands(
        send(&mut c, w.settle_ix(&t, &w.keeper.pubkey(), &s), &w.keeper),
        "w.settle_ix(",
    );
    let r = ts(&l);
    assert_eq!(r.u64("outcome") as u8, TO::BOUNCED_UNRANKED);
    assert_eq!(r.u64("pool_owed_delta"), 10_000);
    assert_eq!(c.lamports(&t.e.wallet.pubkey()), p0 + t.tip + 20_000);
    assert_eq!(
        c.lamports(&drained.pubkey()),
        0,
        "nothing reached the drained wallet"
    );
    let dv = records::one(&l.logs, Kind::DIVERT);
    assert_eq!(dv.key, drained.pubkey().as_ref().to_vec());
    assert_eq!(dv.u64("amount"), 10_000);
    assert_eq!(u64_at(&c.data(&t.e.holding), H::POOL_OWED), 10_000);
    let e = entry_of(&c, &t.e.province, t.m.host_id).expect("home");
    assert_eq!(e.troops, MILLI, "no loss");
    // The sweep (class N).
    let any = funded(&mut c, "sweeper");
    let pool = w.a.defence_pool();
    let (h0, d0) = (c.lamports(&t.e.holding), c.lamports(&pool));
    let hw = ChainWatch::new(&c, t.e.holding, EntityKind::Holding);
    let sweep = sweep_pool_owed(&w.a, any.pubkey(), t.href());
    let l = expect_lands(send(&mut c, sweep.clone(), &any), "sweep_pool_owed(");
    assert_eq!(c.lamports(&t.e.holding), h0 - 10_000);
    assert_eq!(c.lamports(&pool), d0 + 10_000);
    assert_eq!(u64_at(&c.data(&pool), DP::DIVERTED_TOTAL), 10_000);
    assert_eq!(u64_at(&c.data(&t.e.holding), H::POOL_OWED), 0);
    assert_eq!(
        records::one(&l.logs, Kind::POOL_SWEEP).u64("amount"),
        10_000
    );
    hw.check(&c, &l.logs, 1);
    assert_code(send(&mut c, sweep, &any), E::AlreadyDone);
}

/// Not revealed: with room in the final set the host would have been
/// admitted, so it is routed (`rout_survivors`, stamina 0; tip to
/// `pool_owed`, fee to the resolver); with no resolved inputs (a skipped
/// bell) it is routed and the fee goes to `pool_owed` too.
#[test]
fn g12_settle_routs_an_unrevealed_host() {
    // (resolved, gathered): a resolved final set with room; a skipped bell
    // (no inputs); inputs gathered as "no arrivals" and never resolved
    // (flag 1 only: SettleTransit uses inputs only with flag 2, I-46).
    for (resolved, gathered) in [(true, true), (false, false), (false, true)] {
        let (mut c, w) = world();
        let t = trip(&mut c, &w, "rout", 0, SealCase::Valid);
        w.trip_anchor(&mut c, &t);
        let keeper = w.keeper.pubkey();
        if gathered {
            let set = [Rec::other(0, 0, 0x7000, 0x1000, 100_000, AR::FATE_STAYS)];
            let k = w.craft_resolved_inputs(&mut c, &t, &set, &keeper);
            if !resolved {
                c.edit(&k, |d| d[CI::FLAGS] = CI::FLAG_NO_ARRIVALS);
            }
        } else {
            w.skip_dest(&mut c, &t);
        }
        w.to_settle(&mut c, &t, 0);
        let l = expect_lands(
            send(
                &mut c,
                w.settle_ix(&t, &keeper, &w.settle_default()),
                &w.keeper,
            ),
            "w.settle_ix(",
        );
        let r = ts(&l);
        assert_eq!(r.u64("outcome") as u8, TO::ROUTED);
        let owed = if resolved { t.tip } else { t.tip + 10_000 };
        assert_eq!(r.u64("pool_owed_delta"), owed);
        assert_eq!(u64_at(&c.data(&t.e.holding), H::POOL_OWED), owed);
        let e = entry_of(&c, &t.e.province, t.m.host_id).expect("home");
        assert_eq!(e.troops, rout_survivors(MILLI));
        assert_eq!(e.stamina_value, 0);
        assert_eq!(r.links_count(), 2, "Holding and home Province");
    }
}

/// The home Province has no room (8 residents of the faction): a returning
/// host's troops go to the Holding's reserve (whole troops).
#[test]
fn g12_settle_returns_to_the_reserve_when_home_is_full() {
    let (mut c, w) = world();
    let t = trip(&mut c, &w, "full", 0, SealCase::Valid);
    w.trip_anchor(&mut c, &t);
    // Seven more roster hosts of faction 0 at home (the departed one left).
    for k in 0..8u32 {
        let i = free_entry(&c.data(&t.e.province));
        w.craft_host(&mut c, &t.e, &t.e.province, i, 100 + k, 0, 200, t.e.tile);
    }
    w.skip_dest(&mut c, &t);
    w.to_settle(&mut c, &t, 0);
    let r0 = u32_at(&c.data(&t.e.holding), H::reserve(0));
    let keeper = w.keeper.pubkey();
    let l = expect_lands(
        send(
            &mut c,
            w.settle_ix(&t, &keeper, &w.settle_default()),
            &w.keeper,
        ),
        "w.settle_ix(",
    );
    assert_eq!(ts(&l).u64("outcome") as u8, TO::ROUTED);
    assert!(entry_of(&c, &t.e.province, t.m.host_id).is_none());
    assert_eq!(
        u32_at(&c.data(&t.e.holding), H::reserve(0)),
        r0 + rout_survivors(MILLI) / 1_000
    );
}

trait Links {
    fn links_count(&self) -> usize;
}
impl Links for records::Rec {
    fn links_count(&self) -> usize {
        self.links.len()
    }
}

// ------------------------------------------------------------ G10: the seal proof

/// The cases of G10: every SealKit case (valid, wrong round, tampered V/W,
/// garbage U, forged commitment, invalid plaintext) and a tampered U.
const G10_CASES: [SealCase; 7] = [
    SealCase::Valid,
    SealCase::WrongRound,
    SealCase::TamperedVW,
    SealCase::Garbage,
    SealCase::CommitMismatch,
    SealCase::BadPlaintext,
    SealCase::Valid, // re-sealed below with another seal's U
];

/// Seven marches from two estates of (2, 0) (four transit slots each), all
/// departing in the same bell to the same destination.
fn g10_trips(c: &mut Chain, w: &World, tag: &str, site0: u8) -> Vec<Trip> {
    use permutation_frontier_svm_tests::world::transit::clone_estate;
    let mut out: Vec<Trip> = vec![];
    for (k, case) in G10_CASES.iter().enumerate() {
        let slot = (k % 4) as u8;
        let e = if slot == 0 {
            w.craft_estate(c, &format!("{tag}{k}"), 0, (2, 0), site0 + (k / 4) as u8)
        } else {
            clone_estate(&out[k - slot as usize].e)
        };
        if k == 6 {
            // Tampered U: the valid march re-sealed with trip 1's U.
            let slot_e = free_entry(&c.data(&e.province));
            let seq = u32_at(&c.data(&e.holding), H::HOST_SEQ);
            let id = w.craft_host(c, &e, &e.province, slot_e, seq, 0, TROOPS, e.tile);
            let origin = (e.p as i32, e.q as i32, e.tile);
            let b = w.bell(c);
            let mut m = w.plan_march(id, slot, origin, &EAST8, b + 6, 1, 0, SealCase::Valid);
            let other = &out[1];
            m.made.seal[..96].copy_from_slice(&other.m.made.seal[..96]);
            m.made.ct_hash = fclient::seal::ct_hash(&m.made.seal);
            m.made.seal_root = fclient::seal::seal_root(&m.made.commit, &m.made.ct_hash);
            out.push(w.depart_trip(c, e, m, b, TROOPS));
        } else {
            out.push(w.trip_from(c, e, slot, TROOPS, &EAST8, 6, *case));
        }
    }
    out
}

/// G10 (§13.3, I-44): SettleTransit's seal code equals the stock `tlock`
/// opener's (`fclient::seal::judge` over tlock 0.0.10) with the logged
/// commitment and `Plain::validate`, for a valid seal, a wrong round, a
/// tampered V/W, a tampered U, garbage, a forged commitment and an invalid
/// plaintext — settled from THE anchor, and again from the archive entry
/// after ArchiveAnchors closed THE anchor. A logged pair that is not the
/// committed one is `CommitMismatch`.
#[test]
fn g10_settle_transit_seal_codes_match_the_stock_opener() {
    let (mut c, w) = world();
    w.craft_province(&mut c, 2, 0);
    let n_sites = c.data(&w.a.province(2, 0))[frontier_abi::layout::province::province::SITE_COUNT];
    assert!(n_sites >= 4, "(2, 0) has {n_sites} sites");
    let before = g10_trips(&mut c, &w, "g10b", 0);
    let after = g10_trips(&mut c, &w, "g10a", 2);
    let t0 = &before[0];
    assert!(before
        .iter()
        .chain(&after)
        .all(|t| t.arrive() == t0.arrive()));
    for t in before.iter().chain(&after) {
        w.trip_anchor(&mut c, t);
        w.skip_dest(&mut c, t);
    }
    for t in before.iter().chain(&after) {
        w.to_settle(&mut c, t, 0);
    }
    let round = w.tlock_round(t0.arrive());
    let kit = SealKit::new(&w.beacons);
    let keeper = w.keeper.pubkey();
    let judge = |t: &Trip| kit.judge(&t.m.made, round, t.m.host_id, t.arrive());
    let mut codes = vec![];
    for t in &before {
        let want = judge(t);
        let l = expect_lands(
            send(
                &mut c,
                w.settle_ix(t, &keeper, &w.settle_default()),
                &w.keeper,
            ),
            "w.settle_ix(",
        );
        let r = ts(&l);
        assert_eq!(
            r.u64("seal_code") as u8,
            want,
            "before archive, {:?}",
            t.m.made.case
        );
        let outcome = if want == 0 { TO::ROUTED } else { TO::BAD_SEAL };
        assert_eq!(r.u64("outcome") as u8, outcome);
        codes.push(want);
    }
    assert_eq!(
        codes,
        vec![0, 1, 1, 2, 4, 5, 1],
        "stock codes of the seven cases"
    );
    // The logged pair must be the committed one.
    let mut s = w.settle_default();
    s.anchor_present = true;
    let mut ix = w.settle_ix(&after[0], &keeper, &s);
    ix.data[2..34].copy_from_slice(&after[1].m.made.commit);
    assert_code(send(&mut c, ix, &w.keeper), E::CommitMismatch);
    // After ArchiveAnchors: THE anchor is gone, the archive entry keeps its
    // signature (I-44).
    for t in &after {
        w.archive_trip(&mut c, t);
        assert!(c.is_absent(&w.a.anchor(t.arrive(), t.region)));
    }
    assert_code(
        send(&mut c, w.settle_ix(&after[0], &keeper, &s), &w.keeper),
        E::NoAnchor,
    );
    s.anchor_present = false;
    for (t, want) in after.iter().zip(&codes) {
        assert_eq!(judge(t), *want);
        let l = expect_lands(
            send(&mut c, w.settle_ix(t, &keeper, &s), &w.keeper),
            "w.settle_ix(",
        );
        assert_eq!(ts(&l).u64("seal_code") as u8, *want, "after archive");
        assert_eq!(transit_state(&c, t), T::STATE_FREE);
    }
}

// ------------------------------------------------------------ bad seals at the destination

/// Settle races the proof (§13.3 G12): a garbage seal (code 2) is revealed
/// in-bell by its owner (who knows the salt), the host stays at the
/// destination (crafted final set and entry), and the earliest settlement
/// (`close + 600`) destroys it: its entry gets the pending `Forfeit` of
/// the current bell (troops lost at that bell's settle; past clashes
/// unchanged), the whole escrow goes to the settler's beneficiary, the
/// final set's settled bit is set. The slot's revealer is not paid.
#[test]
fn g12_settle_races_proof_destroys_a_revealed_bad_seal() {
    let (mut c, w) = world();
    let t = trip(&mut c, &w, "racer", 0, SealCase::Garbage);
    expect_lands(w.trip_reveal(&mut c, &t, 0), "Reveal of the garbage seal");
    let keeper = w.keeper.pubkey();
    w.craft_resolved_inputs(
        &mut c,
        &t,
        &[Rec::of(&t, 0, AR::FATE_STAYS, MILLI)],
        &keeper,
    );
    let i = w.craft_stayer(&mut c, &t, MILLI);
    w.to_settle(&mut c, &t, 0);
    assert_eq!(c.now, w.trip_close(&c, &t) + 600, "the earliest instant");
    let prover = funded(&mut c, "prover");
    let mut s = w.settle_default();
    s.beneficiary = prover.pubkey();
    let p0 = c.lamports(&prover.pubkey());
    let dest = dest_key(&w, &t);
    let dw = ChainWatch::new(&c, dest, EntityKind::Province);
    let l = expect_lands(
        send(&mut c, w.settle_ix(&t, &keeper, &s), &w.keeper),
        "w.settle_ix(",
    );
    let r = ts(&l);
    assert_eq!(r.u64("outcome") as u8, TO::BAD_SEAL);
    assert_eq!(r.u64("seal_code") as u8, seal_code::BAD_POINT);
    let total = t.tip + 10_000 + 20_000;
    assert_eq!(r.u64("reward"), total);
    assert_eq!((r.u64("tip"), r.u64("fee"), r.u64("bond")), (0, 0, 0));
    assert_eq!(c.lamports(&prover.pubkey()), p0 + total);
    let e = read_entry(&c.data(&dest), i).unwrap();
    assert_eq!(e.op, EntryOp::Forfeit);
    assert_eq!(e.pend_bell, w.bell(&c));
    dw.check(&c, &l.logs, 1);
    assert_eq!(
        u32_at(&c.data(&inputs_key(&w, &t)), CI::SETTLED_MASK),
        1 << CI::position(0, 0)
    );
    assert!(
        entry_of(&c, &t.e.province, t.m.host_id).is_none(),
        "never returned"
    );
    assert_eq!(transit_state(&c, &t), T::STATE_FREE);
}

/// A bad seal (FO failure, code 1: a byte of V flipped) on a revealed
/// Stays host, settled after ArchiveAnchors from the archive entry's
/// signature (I-44): destroyed, `Forfeit` at the destination.
#[test]
fn g12_bad_seal_after_archive_forfeits_a_stays_host() {
    let (mut c, w) = world();
    let t = trip(&mut c, &w, "late", 0, SealCase::TamperedVW);
    expect_lands(w.trip_reveal(&mut c, &t, 0), "Reveal");
    let keeper = w.keeper.pubkey();
    w.craft_resolved_inputs(
        &mut c,
        &t,
        &[Rec::of(&t, 0, AR::FATE_STAYS, MILLI)],
        &keeper,
    );
    let i = w.craft_stayer(&mut c, &t, MILLI);
    w.archive_trip(&mut c, &t);
    let mut s = w.settle_default();
    s.anchor_present = false;
    let l = expect_lands(
        send(&mut c, w.settle_ix(&t, &keeper, &s), &w.keeper),
        "w.settle_ix(",
    );
    let r = ts(&l);
    assert_eq!(r.u64("outcome") as u8, TO::BAD_SEAL);
    assert_eq!(r.u64("seal_code") as u8, seal_code::FO_FAILED);
    let e = read_entry(&c.data(&dest_key(&w, &t)), i).unwrap();
    assert_eq!(e.op, EntryOp::Forfeit);
    // The slot is past any claim grace: closed to its rent_to.
    assert!(c.is_absent(&slot_key(&w, &t, 0)));
}

/// A Stays host whose transit is unsettled cannot re-depart, dissolve or
/// explore (`HostInTransit`, I-44); its settlement lets it act.
#[test]
fn g12_stays_host_cannot_act_before_its_settlement() {
    let (mut c, w) = world();
    let t = trip(&mut c, &w, "busy", 0, SealCase::Valid);
    expect_lands(w.trip_reveal(&mut c, &t, 0), "Reveal");
    let keeper = w.keeper.pubkey();
    w.craft_resolved_inputs(
        &mut c,
        &t,
        &[Rec::of(&t, 0, AR::FATE_STAYS, MILLI)],
        &keeper,
    );
    w.craft_stayer(&mut c, &t, MILLI);
    w.to_settle(&mut c, &t, 0);
    let dest = dest_key(&w, &t);
    let b = w.bell(&c);
    w.set_resolved_next(&mut c, &dest, b);
    let at_dest = (t.dest().0 as i16, t.dest().1 as i16);
    let depart = w.depart_ix(&t.e, at_dest, &t.m, t.tip);
    assert_code(send(&mut c, depart, &t.e.wallet), E::HostInTransit);
    let dissolve = with_account(
        fclient::ix::dissolve(&w.a, &t.e.player(), t.href(), t.m.host_id),
        5,
        dest,
    );
    assert_code(
        send(&mut c, dissolve.clone(), &t.e.wallet),
        E::HostInTransit,
    );
    let explore = fclient::ix::explore(
        &w.a,
        &t.e.player(),
        t.href(),
        at_dest,
        t.m.host_id,
        &[t.m.dest_tile],
    );
    assert_code(send(&mut c, explore, &t.e.wallet), E::HostInTransit);
    expect_lands(
        send(
            &mut c,
            w.settle_ix(&t, &keeper, &w.settle_default()),
            &w.keeper,
        ),
        "w.settle_ix(",
    );
    expect_lands(send(&mut c, dissolve, &t.e.wallet), "Dissolve once settled");
}

// ------------------------------------------------------------ claims (I-52)

/// A Reveal landing `late` slots after THE anchor at `price` µlamports/CU.
fn reveal_late(c: &mut Chain, w: &World, t: &Trip, target_i: u8, late: u32, price: u64) -> Landed {
    w.trip_anchor(c, t);
    for _ in 0..late {
        c.next_slot();
    }
    let ix = w.reveal_ix(&w.keeper.pubkey(), &t.e, &t.m, target_i, true);
    let p = Profile::ladder(Ix::Reveal, c.programdata_len()).with_price(price);
    expect_lands(c.send_with(&p, &[ix], &[&w.keeper]), "late Reveal")
}

fn claim_ix(w: &World, c: &Chain, keeper: &Address, t: &Trip, i: u8) -> Instruction {
    fclient::ix::claim_defence(
        &w.a,
        *keeper,
        w.bell(c) / 144,
        &[fclient::ix::ClaimSlot {
            p: t.dest().0,
            q: t.dest().1,
            bell: t.arrive(),
            faction: t.faction(),
            i,
        }],
    )
}

/// Claim after settle (I-52): a late, overpaid keeper Reveal leaves
/// claim-eligible evidence; SettleTransit keeps the slot open with its
/// `settled` flag for the claim grace and pays the tip; ClaimDefence then
/// refunds the keeper from the pool.
#[test]
fn g12_claim_after_settle_keeps_the_slot() {
    let (mut c, w) = world();
    let t = trip(&mut c, &w, "claim", 0, SealCase::Valid);
    reveal_late(&mut c, &w, &t, 0, 5, 20_000);
    let keeper = w.keeper.pubkey();
    w.craft_resolved_inputs(
        &mut c,
        &t,
        &[Rec::of(&t, 0, AR::FATE_STAYS, MILLI)],
        &keeper,
    );
    w.craft_stayer(&mut c, &t, MILLI);
    w.to_settle(&mut c, &t, 0);
    let l = expect_lands(
        send(
            &mut c,
            w.settle_ix(&t, &keeper, &w.settle_default()),
            &w.keeper,
        ),
        "w.settle_ix(",
    );
    assert_eq!(ts(&l).u64("slot_kept"), 1);
    let sd = c.data(&slot_key(&w, &t, 0));
    assert_eq!(sd[AS::FLAGS] & AS::FLAG_SETTLED, AS::FLAG_SETTLED);
    let k0 = c.lamports(&keeper);
    let l = expect_lands(
        {
            let ix = claim_ix(&w, &c, &keeper, &t, 0);
            send(&mut c, ix, &w.keeper)
        },
        "claim_ix(",
    );
    let r = records::one(&l.logs, Kind::DEFENCE_CLAIM);
    let paid = r.u64("amount");
    assert!(paid > 0);
    assert_eq!(r.u64("partial"), 0);
    // 28,000 lamports of priority (20,000 µlamports × 1.4M CU) less the
    // tip level (tip_min − 2,500).
    assert_eq!(paid, 28_000 - (t.tip - 2_500));
    let claim_rent = c.rent(frontier_abi::layout::beacon::defence_claim::SIZE);
    assert_eq!(c.lamports(&keeper) + l.fee + claim_rent, k0 + paid);
    assert_eq!(c.data(&slot_key(&w, &t, 0))[AS::CLAIMED], 1);
    assert_code(
        {
            let ix = claim_ix(&w, &c, &keeper, &t, 0);
            send(&mut c, ix, &w.keeper)
        },
        E::NotEligible,
    );
}

/// Claim after displacement (I-52): a slot claimed by its first revealer
/// and then displaced by a larger arrival carries the new reveal's
/// evidence and beneficiary with `claimed = 0`; the displaced revealer's
/// claim is `NotEligible`, the displacer's lands.
#[test]
fn g12_claim_after_displacement_resets_the_evidence() {
    let (mut c, w) = world();
    let t = trip(&mut c, &w, "displace", 0, SealCase::Valid);
    w.trip_anchor(&mut c, &t);
    let first = funded(&mut c, "first-keeper");
    let masses = [100_000u32, 500_000, 800_000, 950_000];
    for (i, m) in masses.iter().enumerate() {
        let k = w.craft_slot(
            &mut c,
            t.dest(),
            t.arrive(),
            0,
            i as u8,
            0x7000 + i as u64,
            0x1000 + i as u64,
            *m,
        );
        c.edit(&k, |d| {
            d[AS::BENEFICIARY..AS::BENEFICIARY + 32].copy_from_slice(first.pubkey().as_ref());
            d[AS::CLAIMED] = 1;
        });
    }
    let day = permutation_frontier_svm_tests::world::day_of(t.arrive());
    w.craft_day(&mut c, t.dest(), day, &[t.arrive()]);
    reveal_late(&mut c, &w, &t, 0, 5, 20_000);
    let d = c.data(&slot_key(&w, &t, 0));
    assert_eq!(
        d[AS::BENEFICIARY..AS::BENEFICIARY + 32],
        *w.keeper.pubkey().as_ref()
    );
    assert_eq!(d[AS::CLAIMED], 0, "reset by the displacement");
    assert_code(
        {
            let ix = claim_ix(&w, &c, &first.pubkey(), &t, 0);
            send(&mut c, ix, &first)
        },
        E::NotEligible,
    );
    expect_lands(
        {
            let ix = claim_ix(&w, &c, &w.keeper.pubkey(), &t, 0);
            send(&mut c, ix, &w.keeper)
        },
        "claim_ix(",
    );
}

// ------------------------------------------------------------ G13 codes

/// SettleTransit's refusals (G13) in the order of §5.11: forged accounts,
/// the transit record's state, the timing, the logged pair, the anchor,
/// the recipients and the destination a valid seal names.
#[test]
fn g13_settle_transit_refusals() {
    use permutation_frontier_svm_tests::chain::without_signer;
    use permutation_frontier_svm_tests::world::holding::March;
    let (mut c, w) = world();
    let keeper = w.keeper.pubkey();
    // DepartureUnsettled: a transit still in state 1 (no SettleDeparture).
    let e = w.craft_estate(&mut c, "unsettled", 0, (2, 0), 1);
    let slot = free_entry(&c.data(&e.province));
    let host = w.craft_host(&mut c, &e, &e.province, slot, 0, 0, TROOPS, e.tile);
    let origin = (e.p as i32, e.q as i32, e.tile);
    permutation_frontier_svm_tests::world::holding::open_path(&w, &mut c, origin, &EAST8);
    let b = w.bell(&c);
    let m: March = w.plan_march(host, 0, origin, &EAST8, b + 6, 1, 0, SealCase::Valid);
    let tip = w.tip_min(&c);
    expect_lands(
        send(&mut c, w.depart_ix(&e, (e.p, e.q), &m, tip), &e.wallet),
        "Depart",
    );
    let pending = Trip {
        e,
        region: World::region(m.dest.0, m.dest.1),
        m,
        tip,
        depart_bell: b,
        troops: MILLI,
    };
    let t = trip(&mut c, &w, "codes", 0, SealCase::Valid);
    expect_lands(w.trip_reveal(&mut c, &t, 0), "Reveal");
    let s = w.settle_default();
    // THE anchor present but the destination not resolved: TooEarly.
    w.to_settle(&mut c, &t, 0);
    assert_code(
        send(&mut c, w.settle_ix(&t, &keeper, &s), &w.keeper),
        E::TooEarly,
    );
    w.craft_resolved_inputs(
        &mut c,
        &t,
        &[Rec::of(&t, 0, AR::FATE_BOUNCED, MILLI)],
        &keeper,
    );
    w.to_settle(&mut c, &pending, 0);
    assert_code(
        send(&mut c, w.settle_ix(&pending, &keeper, &s), &w.keeper),
        E::DepartureUnsettled,
    );
    let ix = w.settle_ix(&t, &keeper, &s);
    // BadData: transit slot ≥ 4.
    let mut bad = ix.clone();
    bad.data[1] = 4;
    assert_code(send(&mut c, bad, &w.keeper), E::BadData);
    // Auth: the payer must sign (another key, unsigned); TooManyAccounts:
    // the exact list.
    let unsigned = funded(&mut c, "unsigned").pubkey();
    assert_code(
        send(
            &mut c,
            without_signer(with_account(ix.clone(), at::PAYER, unsigned), at::PAYER),
            &w.keeper,
        ),
        E::Auth,
    );
    let mut short = ix.clone();
    short.accounts.pop();
    assert_code(send(&mut c, short, &w.keeper), E::TooManyAccounts);
    // Forged Holding: another address (BadAddress), a wrong magic (BadAccount).
    let fresh = common::copy_to_fresh(&mut c, &t.e.holding, b"holding");
    assert_code(
        send(
            &mut c,
            with_account(ix.clone(), at::HOLDING, fresh),
            &w.keeper,
        ),
        E::BadAddress,
    );
    let mut f = c.fork();
    f.edit(&t.e.holding, |d| d[0] ^= 1);
    assert_code(send(&mut f, ix.clone(), &w.keeper), E::BadAccount);
    // The anchor account: neither THE anchor nor its archive.
    let other_anchor = w.a.anchor(t.arrive() + 1, t.region);
    assert_code(
        send(
            &mut c,
            with_account(ix.clone(), at::ANCHOR, other_anchor),
            &w.keeper,
        ),
        E::BadAddress,
    );
    // Recipients: the settle beneficiary is the data's; the rent payer the
    // Holding's; the resolver the inputs'; the slot's index the final set's.
    let stranger = funded(&mut c, "stranger").pubkey();
    for pos in [
        at::SETTLE_BENEFICIARY,
        at::RENT_PAYER,
        at::RESOLVER,
        at::SLOT_BENEFICIARY,
    ] {
        assert_code(
            send(&mut c, with_account(ix.clone(), pos, stranger), &w.keeper),
            E::BadAddress,
        );
    }
    let mut s1 = s;
    s1.slot_i = 1;
    assert_code(
        send(&mut c, w.settle_ix(&t, &keeper, &s1), &w.keeper),
        E::BadAddress,
    );
    // WrongStatus: an aborted season.
    let mut f = c.fork();
    w.craft_status(&mut f, frontier_abi::layout::world::season::STATUS_ABORTED);
    assert_code(send(&mut f, ix.clone(), &w.keeper), E::WrongStatus);
    // A valid seal names its destination: another Province is BadAddress.
    let (dp, dq) = t.dest();
    let other = (dp, dq + 1);
    w.craft_province(&mut c, other.0 as i16, other.1 as i16);
    let alt = with_account(ix.clone(), at::DEST, w.a.province(other.0, other.1));
    let alt = with_account(
        alt,
        at::INPUTS,
        w.a.clash_inputs(other.0, other.1, t.arrive()),
    );
    let alt = with_account(
        alt,
        at::SLOT,
        w.a.arrival_slot(other.0, other.1, t.arrive(), 0, 0),
    );
    let region = World::region(other.0, other.1);
    let alt = with_account(alt, at::ANCHOR, w.a.anchor(t.arrive(), region));
    if w.anchor_a(&c, t.arrive(), region).is_none() {
        expect_lands(w.post_anchor(&mut c, t.arrive(), region), "PostAnchor");
    }
    let k = w.a.province(other.0, other.1);
    w.set_resolved_next(&mut c, &k, t.arrive() + 1);
    let a2 = w.anchor_a(&c, t.arrive(), region).unwrap();
    let at2 = permutation_rules::frontier::beacon::reveal_close(a2, w.window(&c, t.arrive())) + 600;
    if c.now < at2 {
        c.set_time(at2);
    }
    assert_code(send(&mut c, alt, &w.keeper), E::BadAddress);
    // And the real one lands.
    expect_lands(send(&mut c, ix, &w.keeper), "w.settle_ix(");
}

/// SweepPoolOwed's refusals (G13): nothing owed, forged accounts, status.
#[test]
fn g13_sweep_pool_owed_refusals() {
    let (mut c, w) = world();
    let t = trip(&mut c, &w, "sweep", 0, SealCase::Valid);
    let any = funded(&mut c, "any");
    let ix = sweep_pool_owed(&w.a, any.pubkey(), t.href());
    assert_code(send(&mut c, ix.clone(), &any), E::AlreadyDone);
    c.edit(&t.e.holding, |d| {
        d[H::POOL_OWED..H::POOL_OWED + 8].copy_from_slice(&5u64.to_le_bytes())
    });
    let not_pool = w.a.frontier();
    assert_code(
        send(
            &mut c,
            with_account(
                ix.clone(),
                permutation_frontier_svm_tests::ix::transit::sweep_at::DPOOL,
                not_pool,
            ),
            &any,
        ),
        E::BadAddress,
    );
    let fresh = common::copy_to_fresh(&mut c, &t.e.holding, b"sweep-holding");
    assert_code(
        send(
            &mut c,
            with_account(
                ix.clone(),
                permutation_frontier_svm_tests::ix::transit::sweep_at::HOLDING,
                fresh,
            ),
            &any,
        ),
        E::BadAddress,
    );
    let mut f = c.fork();
    w.craft_status(&mut f, frontier_abi::layout::world::season::STATUS_ABORTED);
    assert_code(send(&mut f, ix.clone(), &any), E::WrongStatus);
    // (the 5 lamports were crafted into pool_owed and are in the Holding's
    // lamports above its rent and escrow)
    expect_lands(send(&mut c, ix, &any), "sweep_pool_owed(");
}

// ------------------------------------------------------------ G1

/// Fills a Province with `n` roster entries of factions 1–5 (8 each at
/// most), host ids of a fictitious holding: the worst scans for the
/// Forfeit search and the return's caps (crafted, as resolves leave
/// rosters).
fn fill_entries(c: &mut Chain, province: &Address, n: usize) {
    use frontier_abi::entry::{write_entry, Entry};
    c.edit(province, |d| {
        let mut added = 0;
        for i in 0..frontier_abi::layout::province::province::ENTRIES_N {
            if added == n {
                break;
            }
            if read_entry(d, i).unwrap().state != EN::STATE_FREE {
                continue;
            }
            let faction = 1 + (added / 8) as u8 % 5;
            let e = Entry {
                id: 0x0000_0000_0100_0000 + added as u64,
                faction,
                unit: 0,
                tile: 0,
                state: EN::STATE_ROSTER,
                troops: 500_000,
                stamina_value: 100,
                dealt_bps: 10_000,
                stamina_bell: 0,
                ready_bell: 0,
                from_bell: 0,
                pend_bell: 0,
                op: EntryOp::None,
            };
            write_entry(d, i, &e).unwrap();
            d[frontier_abi::layout::province::province::N_ENTRIES] += 1;
            added += 1;
        }
    });
}

fn measured(c: &Chain, ix: Ix, label: &str, ixs: &[Instruction], signer: &Keypair) -> u64 {
    let need = c
        .measure(ixs, &[signer])
        .unwrap_or_else(|f| panic!("{label}: refused while measuring: {f:?}"));
    assert_within(label, &need, &ceilings(ix, 0, c.programdata_len()));
    need.cu
}

/// G1 (§13.1) for SettleTransit on the test-beacon build (the verification
/// and the opener are the release code; only the beacon key differs):
/// every branch with the seal opener; the worst named ones are a bad seal
/// (code 1, FO failure after the pairing) on a Stays host settled after
/// archive, and a bounce by rank against a full final set with a drained
/// recipient. Also SweepPoolOwed. Heap: the trace build verifies only real
/// quicknet rounds, so the heap peak is left to W5-A (the handler
/// allocates the account views and a 4-entry chain list, < 2 KiB).
#[test]
fn g01_budget_w4b_settle_transit() {
    let mut worst = 0u64;
    let keeper_of = |w: &World| w.keeper.insecure_clone();
    // (a) Bad seal on a Stays host after archive.
    {
        let (mut c, w) = world();
        let t = trip(&mut c, &w, "g1a", 0, SealCase::TamperedVW);
        expect_lands(w.trip_reveal(&mut c, &t, 0), "Reveal");
        let k = w.keeper.pubkey();
        w.craft_resolved_inputs(&mut c, &t, &[Rec::of(&t, 0, AR::FATE_STAYS, MILLI)], &k);
        // A full destination (40 entries before the stayer): the Forfeit
        // search scans them.
        fill_entries(&mut c, &dest_key(&w, &t), 40);
        w.craft_stayer(&mut c, &t, MILLI);
        w.archive_trip(&mut c, &t);
        let mut s = w.settle_default();
        s.anchor_present = false;
        let cu = measured(
            &c,
            Ix::SettleTransit,
            "SettleTransit bad seal, Stays, after archive",
            &[w.settle_ix(&t, &k, &s)],
            &keeper_of(&w),
        );
        worst = worst.max(cu);
    }
    // (b) Bounce by rank against a full final set, drained recipient.
    {
        let (mut c, w) = world();
        let t = trip(&mut c, &w, "g1b", 0, SealCase::Valid);
        w.trip_anchor(&mut c, &t);
        let drained = Keypair::new_from_array([0x4e; 32]);
        let set: Vec<Rec> = (0..4)
            .map(|i| {
                Rec::other(
                    0,
                    i,
                    0x7000 + i as u64,
                    0x1000 + i as u64,
                    950_000,
                    AR::FATE_STAYS,
                )
            })
            .collect();
        w.craft_resolved_inputs(&mut c, &t, &set, &drained.pubkey());
        w.to_settle(&mut c, &t, 0);
        let mut s = w.settle_default();
        s.resolver = drained.pubkey();
        let cu = measured(
            &c,
            Ix::SettleTransit,
            "SettleTransit bounce by rank, drained resolver",
            &[w.settle_ix(&t, &w.keeper.pubkey(), &s)],
            &keeper_of(&w),
        );
        worst = worst.max(cu);
    }
    // (c) Every other branch.
    for (label, case, fate) in [
        ("Stays, slot closed", SealCase::Valid, Some(AR::FATE_STAYS)),
        (
            "Retreated, returns home",
            SealCase::Valid,
            Some(AR::FATE_RETREATED),
        ),
        ("routed, no inputs", SealCase::Valid, None),
        ("bad seal code 2, unrevealed", SealCase::Garbage, None),
        ("bad seal code 4", SealCase::CommitMismatch, None),
        ("bad seal code 5", SealCase::BadPlaintext, None),
    ] {
        let (mut c, w) = world();
        let t = trip(&mut c, &w, "g1c", 0, case);
        let k = w.keeper.pubkey();
        // 40 residents at home: the return's caps scan them.
        fill_entries(&mut c, &t.e.province, 40);
        match fate {
            Some(f) => {
                expect_lands(w.trip_reveal(&mut c, &t, 0), "Reveal");
                w.craft_resolved_inputs(&mut c, &t, &[Rec::of(&t, 0, f, MILLI)], &k);
                if f == AR::FATE_STAYS {
                    w.craft_stayer(&mut c, &t, MILLI);
                }
            }
            None => {
                w.trip_anchor(&mut c, &t);
                w.skip_dest(&mut c, &t);
            }
        }
        w.to_settle(&mut c, &t, 0);
        let cu = measured(
            &c,
            Ix::SettleTransit,
            &format!("SettleTransit {label}"),
            &[w.settle_ix(&t, &k, &w.settle_default())],
            &keeper_of(&w),
        );
        worst = worst.max(cu);
    }
    println!("SettleTransit worst measured: {worst} CU (budget 85,000)");
    // SweepPoolOwed.
    let (mut c, w) = world();
    let t = trip(&mut c, &w, "g1s", 0, SealCase::Valid);
    c.edit(&t.e.holding, |d| {
        d[H::POOL_OWED..H::POOL_OWED + 8].copy_from_slice(&5u64.to_le_bytes())
    });
    let any = funded(&mut c, "g1-any");
    measured(
        &c,
        Ix::SweepPoolOwed,
        "SweepPoolOwed",
        &[sweep_pool_owed(&w.a, any.pubkey(), t.href())],
        &any,
    );
}

/// `g01_loaded_limit_*` (I-45) for SettleTransit (a Stays host with its
/// slot and resolved inputs: every account present) and SweepPoolOwed.
#[test]
fn g01_loaded_limit_w4b_transit() {
    use permutation_frontier_svm_tests::world::transit::loaded_check;
    let (mut c, w) = world();
    let t = trip(&mut c, &w, "loaded", 0, SealCase::Valid);
    expect_lands(w.trip_reveal(&mut c, &t, 0), "Reveal");
    let k = w.keeper.pubkey();
    w.craft_resolved_inputs(&mut c, &t, &[Rec::of(&t, 0, AR::FATE_STAYS, MILLI)], &k);
    w.craft_stayer(&mut c, &t, MILLI);
    w.to_settle(&mut c, &t, 0);
    let keeper = w.keeper.insecure_clone();
    loaded_check(
        &c,
        Ix::SettleTransit,
        &[w.settle_ix(&t, &k, &w.settle_default())],
        &[&keeper],
    );
    c.edit(&t.e.holding, |d| {
        d[H::POOL_OWED..H::POOL_OWED + 8].copy_from_slice(&5u64.to_le_bytes())
    });
    let any = funded(&mut c, "loaded-any");
    loaded_check(
        &c,
        Ix::SweepPoolOwed,
        &[sweep_pool_owed(&w.a, any.pubkey(), t.href())],
        &[&any],
    );
}

/// Settlement goes on after EndSeason (status Ended): a transit left
/// unsettled at the end settles, and the sweep still runs.
#[test]
fn g12_settle_after_the_end() {
    let (mut c, w) = world();
    let t = trip(&mut c, &w, "end", 0, SealCase::Valid);
    w.trip_anchor(&mut c, &t);
    w.skip_dest(&mut c, &t);
    c.set_time(w.end_ts());
    let any = funded(&mut c, "ender");
    expect_lands(
        send(&mut c, fclient::ix::end_season(&w.a, any.pubkey()), &any),
        "EndSeason",
    );
    let keeper = w.keeper.pubkey();
    let l = expect_lands(
        send(
            &mut c,
            w.settle_ix(&t, &keeper, &w.settle_default()),
            &w.keeper,
        ),
        "w.settle_ix(",
    );
    assert_eq!(ts(&l).u64("outcome") as u8, TO::ROUTED);
    expect_lands(
        send(&mut c, sweep_pool_owed(&w.a, any.pubkey(), t.href()), &any),
        "sweep_pool_owed(",
    );
}

// ------------------------------------------------------------ wave-4 review (v1.7)

/// The Citizen's `works`.
fn works_of(c: &Chain, citizen: &Address) -> u64 {
    u64_at(
        &c.data(citizen),
        frontier_abi::layout::player::citizen::WORKS,
    )
}

/// v1.7 (I-56, wave-4 review): the camp's loot. The arrival at the lowest
/// `camp_mask` position that stays earns `WORKS_CAMP` for its owner's
/// Citizen (the optional last account: `BadAccount` without it, the wrong
/// Citizen `BadAddress`); `TRANSIT_SETTLED` chains the Citizen. A host
/// that is not the camp's winner may list its Citizen: nothing credited,
/// no Citizen link.
#[test]
fn g12_settle_credits_the_camp_works_to_one_winner() {
    let (mut c, w) = world();
    let t = trip(&mut c, &w, "camp", 0, SealCase::Valid);
    expect_lands(w.trip_reveal(&mut c, &t, 0), "Reveal");
    let keeper = w.keeper.pubkey();
    let k = w.craft_resolved_inputs(
        &mut c,
        &t,
        &[Rec::of(&t, 0, AR::FATE_STAYS, MILLI)],
        &keeper,
    );
    w.craft_stayer(&mut c, &t, MILLI);
    w.to_settle(&mut c, &t, 0);
    let pos = CI::position(t.faction(), 0);
    let set_mask = |c: &mut Chain, m: u32| {
        c.edit(&k, |d| {
            d[CI::RSV_76..CI::RSV_76 + 4].copy_from_slice(&m.to_le_bytes())
        })
    };
    let w0 = works_of(&c, &t.e.citizen);
    let mut with_citizen = w.settle_default();
    with_citizen.camp_citizen = Some(t.e.citizen);
    // Not the winner (a later position took the camp): nothing credited.
    let mut g = c.fork();
    set_mask(&mut g, 1 << 20);
    let l = expect_lands(
        send(&mut g, w.settle_ix(&t, &keeper, &with_citizen), &w.keeper),
        "settle, not the camp's winner",
    );
    assert_eq!(works_of(&g, &t.e.citizen), w0);
    assert!(ts(&l).link(EntityKind::Citizen).is_none());
    // The winner (the lowest position of two).
    set_mask(&mut c, (1 << pos) | (1 << 20));
    assert_code(
        send(
            &mut c,
            w.settle_ix(&t, &keeper, &w.settle_default()),
            &w.keeper,
        ),
        E::BadAccount,
    );
    let mut wrong = w.settle_default();
    wrong.camp_citizen = Some(Address::new_unique());
    assert_code(
        send(&mut c, w.settle_ix(&t, &keeper, &wrong), &w.keeper),
        E::BadAddress,
    );
    let zw = ChainWatch::new(&c, t.e.citizen, EntityKind::Citizen);
    let l = expect_lands(
        send(&mut c, w.settle_ix(&t, &keeper, &with_citizen), &w.keeper),
        "settle of the camp's winner",
    );
    assert_eq!(ts(&l).u64("outcome") as u8, TO::STAYS);
    assert_eq!(
        works_of(&c, &t.e.citizen),
        w0 + permutation_rules::frontier::camp::loot() as u64
    );
    zw.check(&c, &l.logs, 1);
}

/// v1.7 (wave-4 review, W4-B major): a valid seal whose destination
/// Province is absent (never opened; the review's probe removes it) settles
/// as routed with no resolved inputs at the canonical absent address,
/// timed by the anchor of the plaintext's region (another region's anchor:
/// `BadAddress`). Before v1.7 it was `BadAccount` for ever. A bad seal
/// still needs a present Province.
#[test]
fn g12_valid_seal_to_an_absent_province_routes() {
    let (mut c, w) = world();
    let t = trip(&mut c, &w, "absent", 0, SealCase::Valid);
    w.trip_anchor(&mut c, &t);
    w.to_settle(&mut c, &t, 0);
    let dest = dest_key(&w, &t);
    c.remove(&dest);
    let keeper = w.keeper.pubkey();
    let ix = w.settle_ix(&t, &keeper, &w.settle_default());
    // another region's anchor of the bell
    let other = (t.region + 1) % 16;
    expect_lands(w.post_anchor(&mut c, t.arrive(), other), "PostAnchor");
    // past both anchors' settle time
    let later = c.now + 3_600;
    c.set_time(later);
    assert_code(
        send(
            &mut c,
            with_account(ix.clone(), 7, w.a.anchor(t.arrive(), other)),
            &w.keeper,
        ),
        E::BadAddress,
    );
    let l = expect_lands(send(&mut c, ix, &w.keeper), "settle to an absent Province");
    let r = ts(&l);
    assert_eq!(r.u64("outcome") as u8, TO::ROUTED);
    assert_eq!(r.u64("seal_code") as u8, seal_code::VALID);
    assert_eq!(r.u64("pool_owed_delta"), t.tip + 10_000, "no resolver");
    assert!(c.is_absent(&dest), "nothing written there");
    assert_eq!(transit_state(&c, &t), T::STATE_FREE);
    let e = entry_of(&c, &t.e.province, t.m.host_id).expect("home");
    assert_eq!(e.troops, rout_survivors(MILLI));
    // a bad seal against an absent Province: BadAccount
    let (mut c, w) = world();
    let t = trip(&mut c, &w, "absent-bad", 0, SealCase::Garbage);
    w.trip_anchor(&mut c, &t);
    w.to_settle(&mut c, &t, 0);
    c.remove(&dest_key(&w, &t));
    assert_code(
        send(
            &mut c,
            w.settle_ix(&t, &w.keeper.pubkey(), &w.settle_default()),
            &w.keeper,
        ),
        E::BadAccount,
    );
}

/// v1.7 (W4-B F1): GatherClash stamps the transit it records with its
/// destination (and chains the Holding); SettleTransit then settles it
/// only there. A revealed bad seal that stays can no longer be settled
/// against another Province (resolved past the arrival, its own anchor
/// posted) to escape the `Forfeit`: `BadAddress`. Without the stamp the
/// same transaction lands and the host keeps its place (the exploit).
#[test]
fn g12_gathered_bad_seal_settles_only_at_its_destination() {
    let (mut c, w) = world();
    let t = trip(&mut c, &w, "stamp", 0, SealCase::Garbage);
    expect_lands(w.trip_reveal(&mut c, &t, 0), "Reveal of the garbage seal");
    w.to_settle(&mut c, &t, 0);
    let hw = ChainWatch::new(&c, t.e.holding, EntityKind::Holding);
    let parts = w.gather_parts(
        &c,
        t.dest(),
        t.arrive(),
        &permutation_frontier_svm_tests::world::clash::THIRDS,
    );
    let mut stamped_by = None;
    for ix in parts {
        let l = expect_lands(send(&mut c, ix, &w.keeper), "GatherClash");
        if records::one(&l.logs, Kind::GATHER)
            .link(EntityKind::Holding)
            .is_some()
        {
            stamped_by = Some(l);
        }
    }
    let l = stamped_by.expect("the part with the Holding chains it");
    hw.check(&c, &l.logs, 1);
    let hd = c.data(&t.e.holding);
    let o = H::transit(t.m.transit_slot as usize);
    assert_ne!(hd[o + T::FLAGS] & T::FLAG_GATHERED, 0, "stamped");
    let (dp, dq) = t.dest();
    assert_eq!(
        (
            i16::from_le_bytes([hd[o + T::DEST_P], hd[o + T::DEST_P + 1]]),
            i16::from_le_bytes([hd[o + T::DEST_Q], hd[o + T::DEST_Q + 1]])
        ),
        (dp as i16, dq as i16)
    );
    // The clash: the host stays.
    let keeper = w.keeper.pubkey();
    w.craft_resolved_inputs(
        &mut c,
        &t,
        &[Rec::of(&t, 0, AR::FATE_STAYS, MILLI)],
        &keeper,
    );
    let i = w.craft_stayer(&mut c, &t, MILLI);
    // Another present Province, resolved past the arrival, with its own
    // region's anchor: the owner's escape route.
    let home = t.e.province;
    w.set_resolved_next(&mut c, &home, t.arrive() + 1);
    let hr = permutation_rules::frontier::geometry::region_of(
        permutation_rules::frontier::geometry::ProvinceCoord::new(t.e.p as i32, t.e.q as i32),
    );
    if w.anchor_a(&c, t.arrive(), hr).is_none() {
        expect_lands(w.post_anchor(&mut c, t.arrive(), hr), "PostAnchor (home)");
    }
    let (hp, hq) = (t.e.p as i32, t.e.q as i32);
    let escape = {
        let ix = w.settle_ix(&t, &keeper, &w.settle_default());
        let ix = with_account(ix, 3, home);
        let ix = with_account(ix, 4, w.a.clash_inputs(hp, hq, t.arrive()));
        let ix = with_account(ix, 5, w.a.arrival_slot(hp, hq, t.arrive(), t.faction(), 0));
        with_account(ix, 7, w.a.anchor(t.arrive(), hr))
    };
    // Past the other anchor's settle time too.
    let later = c.now + 3_600;
    c.set_time(later);
    // Without the stamp (as before v1.7) the escape lands: no Forfeit.
    let mut g = c.fork();
    g.edit(&t.e.holding, |d| d[o + T::FLAGS] &= !T::FLAG_GATHERED);
    let l = expect_lands(
        send(&mut g, escape.clone(), &w.keeper),
        "the pre-v1.7 escape",
    );
    assert_eq!(ts(&l).u64("outcome") as u8, TO::BAD_SEAL);
    assert_eq!(
        read_entry(&g.data(&dest_key(&w, &t)), i).unwrap().op,
        EntryOp::None,
        "the host kept its place"
    );
    // With the stamp: refused; the real destination settles and forfeits.
    assert_code(send(&mut c, escape, &w.keeper), E::BadAddress);
    let l = expect_lands(
        send(
            &mut c,
            w.settle_ix(&t, &keeper, &w.settle_default()),
            &w.keeper,
        ),
        "settle at the gathered destination",
    );
    assert_eq!(ts(&l).u64("outcome") as u8, TO::BAD_SEAL);
    assert_eq!(
        read_entry(&c.data(&dest_key(&w, &t)), i).unwrap().op,
        EntryOp::Forfeit
    );
}
