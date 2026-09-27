//! Rings and provinces (W3-A, M1 contract §5.9, I-30, I-46, I-48, I-56):
//! OpenRing, ConsumeRingSeed, OpenProvince, FoldOccupancy, CloseProvince.
//! Functional tests (`map_…`), G1 (`g01_open_province`), G2 pre-funding and
//! re-creation (`g02_…`), G3 forgeries (`g03_…`). Every season runs on the
//! test-beacon build (any round, I-53) unless the test says otherwise.
//!
//! Crafted state (each named where used): the Season's status byte for
//! Ended/Aborted (EndSeason and AbortSeason are W4-B's), the Frontier's
//! folded `wedge_occupied` (occupancy that would need hundreds of
//! settlements), RingSeeds of rings 4–10 for the budget sweep, and a fund's
//! lamports for `Insufficient`.

mod common;

use common::{assert_program_account, copy_to_fresh, paid, prefunds};
use frontier_abi::layout::province::{camp as CP, province as PV, site as SM};
use frontier_abi::layout::world::{
    frontier as FR, join_shard as JS, province_fund as PF, ring_seed as RS, season as S,
};
use frontier_abi::log::{EntityKind, Kind};
use frontier_abi::tags::Ix;
use permutation_frontier_svm_tests::budget::{assert_within, ceilings};
use permutation_frontier_svm_tests::chain::{
    assert_code, expect_lands, with_account, Build, Chain,
};
use permutation_frontier_svm_tests::ix::map::{self as mix, consume_at, province_at, ring_at};
use permutation_frontier_svm_tests::records::{one, ChainWatch};
use permutation_frontier_svm_tests::world::land::{
    provinces_of, rd_u16, rd_u32, rd_u64, region, terrain, terrain_block, terrain_digest,
};
use permutation_frontier_svm_tests::world::World;
use permutation_frontier_svm_tests::{Address, FrontierError as E, Signer};
use permutation_rules::frontier::beacon as kb;
use permutation_rules::frontier::camp;
use permutation_rules::frontier::clash::QUICKNET;
use permutation_rules::frontier::geometry::ProvinceCoord;

fn running() -> (Chain, World) {
    let mut c = Chain::test_beacon();
    let w = World::running(&mut c, 1);
    (c, w)
}

fn genesis_ring_seed(w: &World, c: &Chain, d: u16) -> [u8; 32] {
    let g: [u8; 32] = c.data(&w.a.season)[S::GENESIS_SEED..S::GENESIS_SEED + 32]
        .try_into()
        .unwrap();
    permutation_frontier_svm_tests::sha256(&[b"PSF-RING", &g, &d.to_le_bytes()])
}

fn fund_u32(c: &Chain, w: &World, wedge: u8, off: usize) -> u32 {
    rd_u32(&c.data(&w.a.province_fund(wedge)), off)
}

#[test]
fn map_genesis_rings_are_seeded_at_once() {
    let (mut c, w) = running();
    let g = w.genesis_ring();
    assert_eq!(g, 3, "the 7-day preset's genesis ring");
    // Rings open in order: ring 1 before ring 0 is too early.
    assert_code(w.open_ring(&mut c, 1), E::TooEarly);
    assert_code(w.open_ring(&mut c, 17), E::BadData); // d > r_max (16)
    for d in 0..=g {
        let fw = ChainWatch::new(&c, w.a.frontier(), EntityKind::Frontier);
        let before = c.lamports(&w.keeper.pubkey());
        let l = expect_lands(w.open_ring(&mut c, d), "OpenRing (genesis ring)");
        fw.check(&c, &l.logs, 1);
        assert_eq!(
            paid(before, &c, &w.keeper, &l),
            c.rent(RS::SIZE),
            "the payer funds the RingSeed"
        );
        let k = w.a.ring_seed(d);
        assert_program_account(&c, &k, RS::MAGIC, RS::SIZE, 1);
        let rd = c.data(&k);
        assert_eq!(rd_u16(&rd, RS::D), d);
        assert_eq!(
            rd[RS::STATUS],
            RS::STATUS_SEEDED,
            "I-30: no ConsumeRingSeed"
        );
        assert_eq!(w.ring_seed_of(&c, d), genesis_ring_seed(&w, &c, d));
        assert_eq!(rd_u64(&rd, RS::ROUND), 0);
        assert_eq!(&rd[RS::PAYER..RS::PAYER + 32], w.keeper.pubkey().as_ref());
        let r = one(&l.logs, Kind::RING_OPEN);
        assert_eq!(r.key, d.to_le_bytes());
        assert_eq!(r.u64("round"), 0);
        assert_eq!(r.field("seed", true), &genesis_ring_seed(&w, &c, d));
        assert_eq!(r.u64("t_open") as i64, c.now);
    }
    assert_eq!(w.rings_opened(&c), g + 1);
    assert_code(w.open_ring(&mut c, 2), E::AlreadyDone);
}

#[test]
fn map_open_province_writes_the_kernels_land() {
    let (mut c, w) = running();
    w.open_genesis_rings(&mut c);
    let rent_p = c.rent(PV::SIZE);
    // The Concord: funded from wedge 0 (DECISIONS G10), no wedge, no sites,
    // no camp.
    let f0 = c.lamports(&w.a.province_fund(0));
    let l = expect_lands(w.open_province(&mut c, 0, 0), "OpenProvince (Concord)");
    assert_eq!(f0 - c.lamports(&w.a.province_fund(0)), rent_p);
    let pd = c.data(&w.a.province(0, 0));
    assert_eq!(pd[PV::WEDGE], 6);
    let r = one(&l.logs, Kind::PROVINCE_OPEN);
    assert_eq!(r.u64("reserved"), 1);
    assert_eq!(r.u64("camp_tile"), 0xFF);
    // A Seat (ring 1): every site reserved (I-30), no camp.
    let (p1, q1) = provinces_of(1, Some(2))[0];
    expect_lands(w.open_province(&mut c, p1, q1), "OpenProvince (ring 1)");
    let pd = c.data(&w.a.province(p1 as i32, q1 as i32));
    let n = pd[PV::SITE_COUNT] as usize;
    assert!(n > 0, "ring 1 has sites");
    for i in 0..12 {
        let want = if i < n {
            SM::STATE_RESERVED
        } else {
            SM::STATE_FREE
        };
        assert_eq!(pd[PV::site(i) + SM::STATE], want, "site {i}");
    }
    assert_eq!(pd[PV::CAMP + CP::STATE], CP::STATE_NONE);
    // Ring 2: the kernel's terrain, sites free, the initial camp (I-56).
    for (p, q) in provinces_of(2, None) {
        let wedge = mix::wedge_of(p as i32, q as i32);
        let fk = w.a.province_fund(wedge);
        let (f_before, opened, open_sites, spent) = (
            c.lamports(&fk),
            fund_u32(&c, &w, wedge, PF::PROVINCES_OPENED),
            fund_u32(&c, &w, wedge, PF::OPEN_SITES),
            rd_u64(&c.data(&fk), PF::SPENT_TOTAL),
        );
        let created = rd_u16(&c.data(&w.a.ring_seed(2)), RS::PROVINCES_CREATED);
        let pk = w.a.province(p as i32, q as i32);
        let pw = ChainWatch::new(&c, pk, EntityKind::Province);
        let keeper_before = c.lamports(&w.keeper.pubkey());
        let l = expect_lands(w.open_province(&mut c, p, q), "OpenProvince (ring 2)");
        pw.check(&c, &l.logs, 1);
        assert_eq!(
            paid(keeper_before, &c, &w.keeper, &l),
            0,
            "the fund pays the rent"
        );
        assert_program_account(&c, &pk, PV::MAGIC, PV::SIZE, 1);
        let seed = w.ring_seed_of(&c, 2);
        let t = terrain(&seed, p, q);
        let pd = c.data(&pk);
        assert_eq!(
            &pd[PV::TERRAIN..PV::SITE_MIRROR],
            terrain_block(&t).as_slice(),
            "({p},{q}): the pinned terrain encoding"
        );
        assert_eq!(rd_u16(&pd, PV::P) as i16, p);
        assert_eq!(rd_u16(&pd, PV::Q) as i16, q);
        assert_eq!(rd_u16(&pd, PV::RING), 2);
        assert_eq!(pd[PV::WEDGE], wedge);
        assert_eq!(pd[PV::REGION], region(p, q));
        assert_eq!(rd_u32(&pd, PV::RESOLVED_NEXT), w.now_bell(&c));
        assert_eq!(rd_u32(&pd, PV::OPENED_BELL), w.now_bell(&c));
        for i in 0..12 {
            assert_eq!(pd[PV::site(i) + SM::STATE], SM::STATE_FREE);
            assert_eq!(pd[PV::site(i) + SM::FACTION], 6, "NEUTRAL");
            assert_eq!(rd_u32(&pd, PV::site(i) + SM::PEND0_BELL), u32::MAX);
        }
        let coord = ProvinceCoord::new(p as i32, q as i32);
        let want = camp::place(&seed, coord, &t, 0, false, true).expect("ring 2 camp");
        assert!(camp::camp_tile_ok(&t, want.tile));
        assert_eq!(pd[PV::CAMP + CP::TILE], want.tile);
        assert_eq!(pd[PV::CAMP + CP::STATE], CP::STATE_PRESENT);
        assert_eq!(rd_u32(&pd, PV::CAMP + CP::TROOPS), want.troops);
        assert_eq!(rd_u32(&pd, PV::CAMP + CP::NEXT_CHECK_DAY), 1);
        // The wedge fund's live counters (I-48), the RingSeed's count.
        assert_eq!(f_before - c.lamports(&fk), rent_p);
        assert_eq!(fund_u32(&c, &w, wedge, PF::PROVINCES_OPENED), opened + 1);
        assert_eq!(
            fund_u32(&c, &w, wedge, PF::OPEN_SITES),
            open_sites + t.site_count as u32
        );
        assert_eq!(rd_u64(&c.data(&fk), PF::SPENT_TOTAL), spent + rent_p);
        assert_eq!(
            rd_u16(&c.data(&w.a.ring_seed(2)), RS::PROVINCES_CREATED),
            created + 1
        );
        let r = one(&l.logs, Kind::PROVINCE_OPEN);
        assert_eq!(r.field("terrain_digest", true), &terrain_digest(&t));
        assert_eq!(r.u64("site_count"), t.site_count as u64);
        assert_eq!(r.u64("camp_tile"), want.tile as u64);
        assert_eq!(r.u64("camp_troops"), want.troops as u64);
        assert_eq!(r.u64("reserved"), 0);
        assert_eq!(r.u64("wedge"), wedge as u64);
    }
    assert_code(w.open_province(&mut c, 2, 0), E::AlreadyDone);
    assert_code(w.open_province(&mut c, 17, 0), E::BadData); // outside r_max
                                                             // A wedge fund one lamport short of rent + the Province (crafted).
    let (p3, q3) = provinces_of(3, Some(1))[0];
    let fk = w.a.province_fund(1);
    let saved = c.account(&fk).unwrap();
    let mut f = c.fork();
    f.put(
        fk,
        f.program,
        saved.data.clone(),
        c.rent(PF::SIZE) + rent_p - 1,
    );
    assert_code(w.open_province(&mut f, p3, q3), E::Insufficient);
    assert_code(w.open_province(&mut c, 4, 0), E::SeedNotReady); // ring 4 not opened
                                                                 // No Frontier write (I-48): the Frontier was not listed at all.
    let ix = w.open_province_ix(3, 0);
    assert!(ix.accounts.iter().all(|m| m.pubkey != w.a.frontier()));
}

/// Opens rings beyond `g`: one bell after the last opening, a wedge at `θ`
/// occupancy (crafted: the folded `wedge_occupied` of wedge 0), every
/// fund covering the ring; then ConsumeRingSeed.
#[test]
fn map_ring_beyond_genesis_waits_for_occupancy_and_its_seed() {
    let (mut c, w) = running();
    w.open_genesis_rings(&mut c);
    w.open_provinces(&mut c, 2, Some(0));
    w.fold(&mut c);
    // Bell 0: the genesis rings opened this bell.
    assert_code(w.open_ring(&mut c, 4), E::TooEarly);
    c.advance(600);
    // Folded: wedge 0 has open sites, none occupied.
    let fr = c.data(&w.a.frontier());
    let open0 = rd_u32(&fr, FR::WEDGE_OPEN);
    assert!(open0 > 0);
    assert_code(w.open_ring(&mut c, 4), E::Capacity);
    // Crafted occupancy: wedge 0 at θ_early (55%).
    let occ = (open0 as u64 * 5_500).div_ceil(10_000) as u32;
    c.edit(&w.a.frontier(), |d| {
        d[FR::WEDGE_OCCUPIED..FR::WEDGE_OCCUPIED + 4].copy_from_slice(&(occ - 1).to_le_bytes())
    });
    assert_code(w.open_ring(&mut c, 4), E::Capacity);
    c.edit(&w.a.frontier(), |d| {
        d[FR::WEDGE_OCCUPIED..FR::WEDGE_OCCUPIED + 4].copy_from_slice(&occ.to_le_bytes())
    });
    // Crafted: fund 3 below `4 × rent(4,096)` above its rent.
    let f3 = w.a.province_fund(3);
    let saved = c.account(&f3).unwrap();
    let low = c.rent(PF::SIZE) + 4 * c.rent(PV::SIZE) - 1;
    c.put(f3, c.program, saved.data.clone(), low);
    assert_code(w.open_ring(&mut c, 4), E::Insufficient);
    c.put(f3, c.program, saved.data.clone(), saved.lamports);
    // Lands: the RingSeed waits for its round.
    let l = expect_lands(w.open_ring(&mut c, 4), "OpenRing(4)");
    let rd = c.data(&w.a.ring_seed(4));
    assert_eq!(rd[RS::STATUS], RS::STATUS_REQUESTED);
    let round = kb::ring_seed_round(&QUICKNET, c.now, w.params.season.seed_margin);
    assert_eq!(rd_u64(&rd, RS::ROUND), round);
    assert_eq!(one(&l.logs, Kind::RING_OPEN).u64("round"), round);
    assert_eq!(w.rings_opened(&c), 5);
    // Provinces of ring 4 wait for the seed.
    assert_code(w.open_province(&mut c, 4, 0), E::SeedNotReady);
    // One bell between openings.
    c.edit(&w.a.frontier(), |d| {
        d[FR::WEDGE_OCCUPIED..FR::WEDGE_OCCUPIED + 4].copy_from_slice(&open0.to_le_bytes())
    });
    assert_code(w.open_ring(&mut c, 5), E::TooEarly);
    // ConsumeRingSeed: wrong round, a round not yet due (test key), a
    // forged signature, then the round.
    let cons = |r: u64, forged: bool| {
        let arg = if forged {
            w.beacons.forged(r)
        } else {
            w.beacons.must(r)
        };
        mix::consume_ring_seed(&w.a, w.keeper.pubkey(), 4, &arg)
    };
    assert_code(
        c.send(&[cons(round + 1, false)], &[&w.keeper]),
        E::WrongRound,
    );
    assert_code(c.send(&[cons(round, false)], &[&w.keeper]), E::TooEarly);
    c.set_time(World::round_time(round));
    assert_code(c.send(&[cons(round, true)], &[&w.keeper]), E::Crypto);
    let l = expect_lands(
        c.send(&[cons(round, false)], &[&w.keeper]),
        "ConsumeRingSeed",
    );
    let rd = c.data(&w.a.ring_seed(4));
    assert_eq!(rd[RS::STATUS], RS::STATUS_SEEDED);
    assert_eq!(w.ring_seed_of(&c, 4), w.beacons.seed(round).unwrap());
    let r = one(&l.logs, Kind::RING_SEED);
    assert!(r.links.is_empty(), "RING_SEED chains nothing (W1-E)");
    assert_code(c.send(&[cons(round, false)], &[&w.keeper]), E::AlreadyDone);
    expect_lands(w.open_province(&mut c, 4, 0), "OpenProvince (ring 4)");
}

#[test]
fn map_land_instructions_refuse_outside_seeded_and_running() {
    // Seeded, before genesis: genesis rings open, a ring beyond g needs Running.
    let mut c = Chain::test_beacon();
    let w = World::seeded(&mut c, 1);
    assert!(c.now < w.genesis_ts());
    w.open_genesis_rings(&mut c);
    expect_lands(w.open_province(&mut c, 2, 0), "OpenProvince before genesis");
    assert_eq!(
        rd_u32(&c.data(&w.a.province(2, 0)), PV::RESOLVED_NEXT),
        0,
        "0 before genesis"
    );
    assert_code(w.open_ring(&mut c, 4), E::WrongStatus);
    expect_lands(w.fold_part(&mut c, 0), "FoldOccupancy before genesis");
    // Created: nothing.
    let mut c2 = Chain::test_beacon();
    let w2 = World::created(&mut c2, 1);
    assert_code(w2.open_ring(&mut c2, 0), E::WrongStatus);
    // Ended and Aborted (crafted status, W4-B's instructions): nothing
    // opens again (I-46).
    for status in [S::STATUS_ENDED, S::STATUS_ABORTED] {
        let mut f = c.fork();
        w.craft_status(&mut f, status);
        assert_code(w.open_ring(&mut f, 4), E::WrongStatus);
        assert_code(w.open_province(&mut f, 3, 0), E::WrongStatus);
        assert_code(w.fold_part(&mut f, 0), E::WrongStatus);
        let arg = w.beacons.must(100);
        let ix = mix::consume_ring_seed(&w.a, w.keeper.pubkey(), 3, &arg);
        assert_code(f.send(&[ix], &[&w.keeper]), E::WrongStatus);
    }
}

#[test]
fn map_fold_occupancy_runs_in_three_parts() {
    let (mut c, w) = running();
    w.open_genesis_rings(&mut c);
    w.open_provinces(&mut c, 2, Some(0));
    w.open_provinces(&mut c, 3, Some(4));
    assert_code(w.fold_part(&mut c, 1), E::FoldStale);
    assert_code(w.fold_part(&mut c, 2), E::FoldStale);
    let ix = mix::fold_occupancy(&w.a, w.keeper.pubkey(), 3);
    assert_code(c.send(&[ix], &[&w.keeper]), E::BadData);
    let fw = ChainWatch::new(&c, w.a.frontier(), EntityKind::Frontier);
    let mut logs = vec![];
    let l = expect_lands(w.fold_part(&mut c, 0), "FoldOccupancy part 0");
    logs.extend(l.logs);
    assert_code(w.fold_part(&mut c, 2), E::FoldStale); // part 1 first
    let l = expect_lands(w.fold_part(&mut c, 1), "FoldOccupancy part 1");
    logs.extend(l.logs);
    let l = expect_lands(w.fold_part(&mut c, 2), "FoldOccupancy part 2");
    logs.extend(l.logs);
    fw.check(&c, &logs, 3);
    let fr = c.data(&w.a.frontier());
    let mut open = 0u32;
    let mut provinces = 0u32;
    for wd in 0..6u8 {
        let o = fund_u32(&c, &w, wd, PF::OPEN_SITES);
        assert_eq!(rd_u32(&fr, FR::WEDGE_OPEN + 4 * wd as usize), o);
        open += o;
        provinces += fund_u32(&c, &w, wd, PF::PROVINCES_OPENED);
    }
    assert!(rd_u32(&fr, FR::WEDGE_OPEN) > 0 && rd_u32(&fr, FR::WEDGE_OPEN + 16) > 0);
    assert_eq!(rd_u32(&fr, FR::OPEN_SITES), open);
    assert_eq!(rd_u32(&fr, FR::PROVINCES_OPENED), provinces);
    assert_eq!(rd_u32(&fr, FR::OCCUPIED_SITES), 0);
    assert_eq!(fr[FR::FOLD_PART], 0);
    // A fold that spans two bells is stale.
    expect_lands(w.fold_part(&mut c, 0), "FoldOccupancy part 0");
    c.advance(600);
    assert_code(w.fold_part(&mut c, 1), E::FoldStale);
}

#[test]
fn map_close_province_after_the_season_end() {
    let (mut c, w) = running();
    w.open_genesis_rings(&mut c);
    expect_lands(w.open_province(&mut c, 2, 0), "OpenProvince");
    let pk = w.a.province(2, 0);
    let close = |c: &mut Chain| {
        let ix = mix::close_province(&w.a, w.keeper.pubkey(), 2, 0);
        c.send(&[ix], &[&w.keeper])
    };
    assert_code(close(&mut c), E::WrongStatus);
    // Aborted (crafted): at once.
    let mut f = c.fork();
    w.craft_status(&mut f, S::STATUS_ABORTED);
    expect_lands(close(&mut f), "CloseProvince (Aborted)");
    // Ended (crafted): after end + 72 h.
    w.craft_status(&mut c, S::STATUS_ENDED);
    c.set_time(w.end_ts() + 72 * 3_600 - 1);
    assert_code(close(&mut c), E::TooEarly);
    c.advance(1);
    let fk = w.a.province_fund(0);
    let (f_before, p_lamports) = (c.lamports(&fk), c.lamports(&pk));
    let opened = fund_u32(&c, &w, 0, PF::PROVINCES_OPENED);
    let pw = ChainWatch::new(&c, pk, EntityKind::Province);
    let before = pw.before;
    let l = expect_lands(close(&mut c), "CloseProvince");
    assert!(c.is_absent(&pk));
    assert_eq!(c.lamports(&fk), f_before + p_lamports);
    assert_eq!(fund_u32(&c, &w, 0, PF::PROVINCES_OPENED), opened - 1);
    let r = one(&l.logs, Kind::CLOSE);
    assert_eq!(r.u64("final_seq"), before.0);
    assert_eq!(r.field("final_head", true), &before.1);
    assert_eq!(r.field("recipient", true), fk.as_ref());
    assert_eq!(r.u64("lamports"), p_lamports);
    let link = r
        .link(EntityKind::Province)
        .expect("CLOSE chains the Province");
    assert_eq!(link.seq, before.0 + 1);
    assert_code(close(&mut c), E::BadAccount); // absent now
}

// ------------------------------------------------------------ G2

#[test]
fn g02_prefund_ring_seed_both_paths() {
    let (base, w) = {
        let (mut c, w) = running();
        w.open_genesis_rings(&mut c);
        w.open_provinces(&mut c, 2, Some(0));
        w.fold(&mut c);
        c.advance(600);
        (c, w)
    };
    let rent = base.rent(RS::SIZE);
    for pre in prefunds(rent) {
        // Genesis path: ring 0 of a fresh season.
        let mut c = Chain::test_beacon();
        let w0 = World::running(&mut c, 1);
        c.prefund(&w0.a.ring_seed(0), pre);
        let before = c.lamports(&w0.keeper.pubkey());
        let l = expect_lands(w0.open_ring(&mut c, 0), "OpenRing(0) pre-funded");
        assert_eq!(paid(before, &c, &w0.keeper, &l), rent.saturating_sub(pre));
        assert_program_account(&c, &w0.a.ring_seed(0), RS::MAGIC, RS::SIZE, 1);
        assert_eq!(c.data(&w0.a.ring_seed(0))[RS::STATUS], RS::STATUS_SEEDED);
        // Requested path: ring 4 (crafted occupancy, as above).
        let mut c = base.fork();
        let open0 = rd_u32(&c.data(&w.a.frontier()), FR::WEDGE_OPEN);
        c.edit(&w.a.frontier(), |d| {
            d[FR::WEDGE_OCCUPIED..FR::WEDGE_OCCUPIED + 4].copy_from_slice(&open0.to_le_bytes())
        });
        c.prefund(&w.a.ring_seed(4), pre);
        let before = c.lamports(&w.keeper.pubkey());
        let l = expect_lands(w.open_ring(&mut c, 4), "OpenRing(4) pre-funded");
        assert_eq!(paid(before, &c, &w.keeper, &l), rent.saturating_sub(pre));
        assert_eq!(c.data(&w.a.ring_seed(4))[RS::STATUS], RS::STATUS_REQUESTED);
    }
}

#[test]
fn g02_prefund_province_funded_path() {
    let (mut base, w) = running();
    w.open_genesis_rings(&mut base);
    let rent = base.rent(PV::SIZE);
    let pk = w.a.province(2, 0);
    for pre in prefunds(rent) {
        let mut c = base.fork();
        c.prefund(&pk, pre);
        let fk = w.a.province_fund(0);
        let f_before = c.lamports(&fk);
        let before = c.lamports(&w.keeper.pubkey());
        let l = expect_lands(w.open_province(&mut c, 2, 0), "OpenProvince pre-funded");
        assert_eq!(paid(before, &c, &w.keeper, &l), 0, "the fund pays");
        assert_eq!(
            f_before - c.lamports(&fk),
            rent.saturating_sub(pre),
            "only the shortfall"
        );
        assert_eq!(c.lamports(&pk), rent.max(pre));
        assert_program_account(&c, &pk, PV::MAGIC, PV::SIZE, 1);
    }
}

/// Re-creation (§13.2, I-46): after CloseProvince the Province cannot be
/// opened again (status), nor a RingSeed (status); FoldOccupancy is
/// refused too.
#[test]
fn g02_recreate_province_and_ring_seed_refused_after_close() {
    let (mut c, w) = running();
    w.open_genesis_rings(&mut c);
    expect_lands(w.open_province(&mut c, 2, 0), "OpenProvince");
    w.craft_status(&mut c, S::STATUS_ENDED);
    c.set_time(w.end_ts() + 72 * 3_600);
    let ix = mix::close_province(&w.a, w.keeper.pubkey(), 2, 0);
    expect_lands(c.send(&[ix], &[&w.keeper]), "CloseProvince");
    assert!(c.is_absent(&w.a.province(2, 0)));
    assert_code(w.open_province(&mut c, 2, 0), E::WrongStatus);
    assert_code(w.open_ring(&mut c, 4), E::WrongStatus);
}

// ------------------------------------------------------------ G3

#[test]
fn g03_land_accounts_forged_in_map_instructions() {
    let (mut c, w) = running();
    // OpenRing: the Frontier, the RingSeed, a fund.
    let ix = w.open_ring_ix(0);
    let fake = copy_to_fresh(&mut c, &w.a.frontier(), b"frontier");
    assert_code(
        c.send(
            &[with_account(ix.clone(), ring_at::FRONTIER, fake)],
            &[&w.keeper],
        ),
        E::BadAddress,
    );
    let fake_rs = Address::new_from_array(permutation_frontier_svm_tests::sha256(&[b"rs"]));
    assert_code(
        c.send(
            &[with_account(ix.clone(), ring_at::RINGSEED, fake_rs)],
            &[&w.keeper],
        ),
        E::BadAddress,
    );
    assert_code(
        c.send(
            &[with_account(
                ix.clone(),
                ring_at::PFUND0,
                w.a.province_fund(1),
            )],
            &[&w.keeper],
        ),
        E::BadAddress,
    );
    for (what, off, byte) in [("magic", 0usize, 0x55u8), ("season", 8, 0x77)] {
        let mut f = c.fork();
        f.edit(&w.a.frontier(), |d| d[off] ^= byte);
        assert_code(
            f.send(std::slice::from_ref(&ix), &[&w.keeper]),
            E::BadAccount,
        );
        let mut f = c.fork();
        f.edit(&w.a.province_fund(2), |d| d[off] ^= byte);
        assert_code(
            f.send(std::slice::from_ref(&ix), &[&w.keeper]),
            E::BadAccount,
        );
        let _ = what;
    }
    let mut f = c.fork();
    f.set_owner(&w.a.frontier(), Address::new_from_array([9; 32]));
    assert_code(
        f.send(std::slice::from_ref(&ix), &[&w.keeper]),
        E::BadAccount,
    );
    // A present RingSeed with a forged key field is not THE RingSeed.
    w.open_genesis_rings(&mut c);
    let ix = w.open_province_ix(2, 0);
    let mut f = c.fork();
    f.edit(&w.a.ring_seed(2), |d| d[RS::D] = 3);
    assert_code(
        f.send(std::slice::from_ref(&ix), &[&w.keeper]),
        E::BadAccount,
    );
    let mut f = c.fork();
    f.edit(&w.a.ring_seed(2), |d| d[0] ^= 1);
    assert_code(
        f.send(std::slice::from_ref(&ix), &[&w.keeper]),
        E::BadAccount,
    );
    assert_code(
        c.send(
            &[with_account(
                ix.clone(),
                province_at::RINGSEED,
                w.a.ring_seed(3),
            )],
            &[&w.keeper],
        ),
        E::BadAddress,
    );
    assert_code(
        c.send(
            &[with_account(
                ix.clone(),
                province_at::PFUND,
                w.a.province_fund(1),
            )],
            &[&w.keeper],
        ),
        E::BadAddress,
    );
    assert_code(
        c.send(
            &[with_account(
                ix.clone(),
                province_at::PROVINCE,
                w.a.province(3, 0),
            )],
            &[&w.keeper],
        ),
        E::BadAddress,
    );
    // A program-owned impostor at the canonical Province address.
    let mut f = c.fork();
    f.put_program_account(w.a.province(2, 0), vec![0u8; PV::SIZE]);
    assert_code(
        f.send(std::slice::from_ref(&ix), &[&w.keeper]),
        E::BadAccount,
    );
    // ConsumeRingSeed of a forged RingSeed.
    let arg = w.beacons.must(100);
    let cix = mix::consume_ring_seed(&w.a, w.keeper.pubkey(), 2, &arg);
    let mut f = c.fork();
    f.edit(&w.a.ring_seed(2), |d| d[8] ^= 1);
    assert_code(
        f.send(std::slice::from_ref(&cix), &[&w.keeper]),
        E::BadAccount,
    );
    assert_code(
        c.send(
            &[with_account(cix, consume_at::RINGSEED, w.a.ring_seed(3))],
            &[&w.keeper],
        ),
        E::BadAddress,
    );
    // FoldOccupancy: a JoinShard out of place, a forged one.
    let fix = w.fold_ix(0);
    let js = w.a.join_shard(0, 1);
    assert_code(
        c.send(&[with_account(fix.clone(), 3, js)], &[&w.keeper]),
        E::BadAddress,
    );
    let mut f = c.fork();
    f.edit(&w.a.join_shard(0, 0), |d| d[JS::FACTION] = 1);
    // key fields are not re-read by the fold (the address is canonical);
    // a forged magic is refused.
    f.edit(&w.a.join_shard(0, 0), |d| d[1] ^= 1);
    assert_code(f.send(&[fix], &[&w.keeper]), E::BadAccount);
    // CloseProvince with another wedge's fund.
    expect_lands(w.open_province(&mut c, 2, 0), "OpenProvince");
    w.craft_status(&mut c, S::STATUS_ABORTED);
    let ix = mix::close_province(&w.a, w.keeper.pubkey(), 2, 0);
    assert_code(
        c.send(&[with_account(ix, 3, w.a.province_fund(1))], &[&w.keeper]),
        E::BadAddress,
    );
}

// ------------------------------------------------------------ G1

/// Crafted RingSeeds for rings `from..=to` (status seeded, seed
/// `sha256("ring" ‖ d)`), written as ConsumeRingSeed leaves them: the
/// budget sweep of §13.1 needs rings 2..10, the season opens 0..3.
fn craft_ring_seeds(c: &mut Chain, w: &World, from: u16, to: u16) {
    for d in from..=to {
        let mut rd = vec![0u8; RS::SIZE];
        rd[..8].copy_from_slice(&RS::MAGIC);
        rd[8..16].copy_from_slice(&1u64.to_le_bytes());
        rd[RS::D..RS::D + 2].copy_from_slice(&d.to_le_bytes());
        rd[RS::STATUS] = RS::STATUS_SEEDED;
        rd[RS::SEED..RS::SEED + 32].copy_from_slice(&permutation_frontier_svm_tests::sha256(&[
            b"ring",
            &d.to_le_bytes(),
        ]));
        c.put_program_account(w.a.ring_seed(d), rd);
    }
}

/// §13.1 OpenProvince: the worst of rings 2..10 × 6 wedges (every
/// province of each, 324), sent at the retry ladder so every figure is
/// seen; the worst re-measured and asserted against every ceiling (CU
/// budget 220k, tx bytes, locks, loaded data); heap on the trace build
/// when `PSF_TRACE=1`.
#[test]
fn g01_open_province_worst_of_rings_2_to_10() {
    let (mut c, w) = running();
    w.open_genesis_rings(&mut c);
    craft_ring_seeds(&mut c, &w, 4, 10);
    let mut worst = (0u64, (0i16, 0i16), 0u8, false);
    let mut n = 0;
    for ring in 2..=10u32 {
        for wedge in 0..6u8 {
            for (p, q) in provinces_of(ring, Some(wedge)) {
                let ix = w.open_province_ix(p, q);
                let l = expect_lands(
                    c.send(&[ix], &[&w.keeper]),
                    &format!("OpenProvince ({p},{q})"),
                );
                let pd = c.data(&w.a.province(p as i32, q as i32));
                let has_camp = pd[PV::CAMP + CP::STATE] == CP::STATE_PRESENT;
                if l.cu > worst.0 {
                    worst = (l.cu, (p, q), pd[PV::SITE_COUNT], has_camp);
                }
                n += 1;
            }
        }
    }
    println!(
        "OpenProvince over {n} provinces of rings 2..10: worst {} CU at {:?} ({} sites, camp {})",
        worst.0, worst.1, worst.2, worst.3
    );
    // The worst again on a fresh chain, measured with every ceiling.
    let (mut c2, w2) = running();
    w2.open_genesis_rings(&mut c2);
    craft_ring_seeds(&mut c2, &w2, 4, 10);
    let (p, q) = worst.1;
    let need = c2
        .measure(&[w2.open_province_ix(p, q)], &[&w2.keeper])
        .expect("measure");
    // The trace build first (heap), so its figure is printed whatever the
    // plain build's CU verdict.
    if std::env::var("PSF_TRACE").is_ok_and(|v| v == "1") {
        let mut t = Chain::new(Build::Trace);
        let wt = World::running(&mut t, 1);
        wt.open_genesis_rings(&mut t);
        craft_ring_seeds(&mut t, &wt, 4, 10);
        let tn = t
            .measure(&[wt.open_province_ix(p, q)], &[&wt.keeper])
            .expect("measure (trace)");
        println!("OpenProvince (worst, trace build): {tn}");
        let heap = tn.heap.expect("the trace build reports heap");
        assert!(
            heap <= ceilings(Ix::OpenProvince, 0, 0).heap,
            "heap {heap} B"
        );
    }
    assert_within(
        "OpenProvince (worst)",
        &need,
        &ceilings(Ix::OpenProvince, 0, c2.programdata_len()),
    );
}
