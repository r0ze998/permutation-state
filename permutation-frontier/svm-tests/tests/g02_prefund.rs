//! G2 (§13.2) for W2-A's creation paths: pre-fund the canonical address
//! (one lamport, rent, 10× rent), create through the program, the payer
//! pays only the shortfall; plus the re-creation refusals of the kinds
//! these instructions create (BellAnchor and SeedCache after archive, a used
//! Season id). The stand-alone probe that shows `CreateAccount` failing on
//! the same addresses is `harness::g02_probe_create_account_fails_on_a_prefunded_address`.
//!
//! Creation paths covered here (9 of the 19): Season (AnnounceSeason),
//! Frontier, ProvinceFund × 6, DefencePool (CreateSeason), BeaconLog
//! (InitBeaconLogs), JoinShard (InitShards), BellAnchor (PostAnchor and
//! PostAnchorMulti), SeedCache (PostSeed). The rest belong to W3-A (RingSeed
//! × 2, Province, Citizen, Holding), W3-B (ArrivalSlot, ArrivalDay), W4-A
//! (ClashInputs) and W4-B (AnchorArchive, DefenceClaim).

mod common;

use common::{assert_program_account, assert_shortfall_only, paid, prefunds};
use frontier_abi::layout::beacon::{bell_anchor as BA, seed_cache as SC};
use frontier_abi::layout::world::{
    beacon_log as BL, defence_pool as DP, frontier as FR, join_shard as JS, province_fund as PF,
    season as S,
};
use permutation_frontier_svm_tests::chain::{assert_code, expect_lands, Chain};
use permutation_frontier_svm_tests::world::{archive_part, World};
use permutation_frontier_svm_tests::{FrontierError as E, Signer};

#[test]
fn g02_prefund_season_announce_season() {
    let mut base = Chain::release();
    let w0 = World::new(&mut base, 1);
    base.set_time(w0.announce_time());
    let rent = base.rent(S::SIZE);
    for pre in prefunds(rent) {
        let mut c = base.fork();
        let w = World::new(&mut c, 1);
        c.prefund(&w.a.season, pre);
        let before = c.lamports(&w.authority.pubkey());
        let l = expect_lands(
            w.announce(&mut c),
            "AnnounceSeason on a pre-funded Season PDA",
        );
        assert_shortfall_only(
            "Season",
            paid(before, &c, &w.authority, &l),
            pre,
            rent,
            w.bond,
        );
        assert_program_account(&c, &w.a.season, S::MAGIC, S::SIZE, 1);
        assert_eq!(w.status(&c), S::STATUS_ANNOUNCED);
        assert_eq!(
            w.season_u8(&c, S::BUMP),
            w.a.bump,
            "the canonical bump is stored"
        );
        assert!(
            c.lamports(&w.a.season) >= rent + w.bond,
            "the bond is held above rent"
        );
    }
}

#[test]
fn g02_prefund_create_season_accounts() {
    let mut base = Chain::release();
    let w = World::announced(&mut base, 1);
    let p = w.params.season;
    let share = p.pfund_initial / 6;
    for mult in [1u64, 10] {
        let mut c = base.fork();
        let targets: Vec<_> = std::iter::once((w.a.frontier(), FR::SIZE, 0u64))
            .chain(
                w.a.province_funds()
                    .into_iter()
                    .map(|k| (k, PF::SIZE, share)),
            )
            .chain(std::iter::once((
                w.a.defence_pool(),
                DP::SIZE,
                p.dpool_initial,
            )))
            .collect();
        let mut expect_lo = 0u64;
        let mut expect_hi = 0u64;
        for (k, size, escrow) in &targets {
            let rent = c.rent(*size);
            c.prefund(k, mult * rent);
            expect_lo += (rent + escrow).saturating_sub(mult * rent);
            expect_hi += rent.saturating_sub(mult * rent) + escrow;
        }
        let before = c.lamports(&w.authority.pubkey());
        let l = expect_lands(w.create(&mut c), "CreateSeason over 9 pre-funded targets");
        let got = paid(before, &c, &w.authority, &l);
        assert!(
            got == expect_lo || got == expect_hi,
            "paid {got}, want {expect_lo} (topped up) or {expect_hi} (shortfall + escrows)"
        );
        assert_program_account(&c, &w.a.frontier(), FR::MAGIC, FR::SIZE, 1);
        for (i, k) in w.a.province_funds().iter().enumerate() {
            assert_program_account(&c, k, PF::MAGIC, PF::SIZE, 1);
            assert_eq!(c.data(k)[PF::WEDGE] as usize, i, "fund {i}'s wedge");
            assert!(
                c.lamports(k) >= c.rent(PF::SIZE) + share,
                "fund {i} holds its share"
            );
        }
        assert_program_account(&c, &w.a.defence_pool(), DP::MAGIC, DP::SIZE, 1);
        assert!(c.lamports(&w.a.defence_pool()) >= c.rent(DP::SIZE) + p.dpool_initial);
        assert_eq!(w.status(&c), S::STATUS_CREATED);
    }
}

#[test]
fn g02_prefund_beacon_logs() {
    let mut base = Chain::release();
    let w = World::created(&mut base, 1);
    let rent = base.rent(BL::SIZE);
    for pre in prefunds(rent) {
        let mut c = base.fork();
        for r in 0..16 {
            c.prefund(&w.a.beacon_log(r), pre);
        }
        let before = c.lamports(&w.authority.pubkey());
        let l = expect_lands(
            w.init_logs(&mut c),
            "InitBeaconLogs over 16 pre-funded logs",
        );
        assert_eq!(
            paid(before, &c, &w.authority, &l),
            16 * rent.saturating_sub(pre)
        );
        for r in 0..16u8 {
            let k = w.a.beacon_log(r);
            assert_program_account(&c, &k, BL::MAGIC, BL::SIZE, 1);
            assert_eq!(c.data(&k)[BL::REGION], r);
        }
    }
}

#[test]
fn g02_prefund_join_shards() {
    let mut base = Chain::release();
    let w = World::created(&mut base, 1);
    let rent = base.rent(JS::SIZE);
    for pre in prefunds(rent) {
        let mut c = base.fork();
        for f in 0..6u8 {
            for s in 0..8u8 {
                c.prefund(&w.a.join_shard(f, s), pre);
            }
            let before = c.lamports(&w.authority.pubkey());
            let l = expect_lands(
                w.init_shards(&mut c, f),
                "InitShards over pre-funded shards",
            );
            assert_eq!(
                paid(before, &c, &w.authority, &l),
                8 * rent.saturating_sub(pre)
            );
            for s in 0..8u8 {
                let k = w.a.join_shard(f, s);
                assert_program_account(&c, &k, JS::MAGIC, JS::SIZE, 1);
                assert_eq!((c.data(&k)[JS::FACTION], c.data(&k)[JS::SHARD]), (f, s));
            }
        }
    }
}

#[test]
fn g02_prefund_bell_anchor_post_anchor() {
    let (base, w) = common::test_beacon();
    let rent = base.rent(BA::SIZE);
    for pre in prefunds(rent) {
        let mut c = base.fork();
        c.prefund(&w.a.anchor(0, 5), pre);
        w.to_anchor_time(&mut c, 0);
        let before = c.lamports(&w.keeper.pubkey());
        let l = expect_lands(
            w.post_anchor(&mut c, 0, 5),
            "PostAnchor on a pre-funded anchor",
        );
        assert_eq!(paid(before, &c, &w.keeper, &l), rent.saturating_sub(pre));
        let k = w.a.anchor(0, 5);
        assert_program_account(&c, &k, BA::MAGIC, BA::SIZE, 1);
        assert_eq!(w.anchor_a(&c, 0, 5), Some(c.now), "A = Clock at creation");
        assert_eq!(
            c.data(&k)[BA::RENT_TO..BA::RENT_TO + 32],
            w.keeper.pubkey().to_bytes(),
            "rent_to = the fee payer (I-49)"
        );
    }
}

#[test]
fn g02_prefund_bell_anchor_post_anchor_multi() {
    let (base, w) = common::test_beacon();
    let rent = base.rent(BA::SIZE);
    let regions = [0u8, 3, 4, 9, 10, 14, 15];
    for pre in prefunds(rent) {
        let mut c = base.fork();
        for r in regions {
            c.prefund(&w.a.anchor(2, r), pre);
        }
        w.to_anchor_time(&mut c, 2);
        let arg = w.beacons.must(w.tlock_round(2));
        let ix = permutation_frontier_svm_tests::ix::beacon::post_anchor_regions(
            &w.a,
            w.keeper.pubkey(),
            2,
            &arg,
            &regions,
            &w.keeper.pubkey(),
        );
        let before = c.lamports(&w.keeper.pubkey());
        let l = expect_lands(
            c.send(&[ix], &[&w.keeper]),
            "PostAnchorMulti over 7 pre-funded anchors",
        );
        assert_eq!(
            paid(before, &c, &w.keeper, &l),
            7 * rent.saturating_sub(pre)
        );
        for r in regions {
            assert_program_account(&c, &w.a.anchor(2, r), BA::MAGIC, BA::SIZE, 1);
        }
    }
}

#[test]
fn g02_prefund_seed_cache() {
    let (mut base, w) = common::test_beacon();
    expect_lands(w.post_anchor(&mut base, 1, 7), "PostAnchor");
    let rent = base.rent(SC::SIZE);
    for pre in prefunds(rent) {
        let mut c = base.fork();
        c.prefund(&w.a.seed_cache(1, 7, 3), pre);
        let r = w.seed_round(&c, 1, 7);
        let t = World::round_time(r);
        if c.now < t {
            c.set_time(t);
        }
        let before = c.lamports(&w.keeper.pubkey());
        let l = expect_lands(
            w.post_seed(&mut c, 1, 7, 3),
            "PostSeed on a pre-funded cache",
        );
        assert_eq!(paid(before, &c, &w.keeper, &l), rent.saturating_sub(pre));
        assert_program_account(&c, &w.a.seed_cache(1, 7, 3), SC::MAGIC, SC::SIZE, 1);
    }
}

#[test]
fn g02_recreate_bell_anchor_refused_after_archive() {
    // I-46 / §4.2 tombstones: ArchiveAnchors (W4-B) sets the archive's
    // tombstone bit before it closes the anchor. Crafted here: the region-
    // day archive with bell 3 tombstoned and the anchor gone (absent, even
    // pre-funded) — PostAnchor and PostAnchorMulti refuse `Archived`.
    let (mut c, w) = common::test_beacon();
    expect_lands(w.post_anchor(&mut c, 3, 2), "PostAnchor");
    w.craft_archive(&mut c, 2, archive_part(3), &[3]);
    c.remove(&w.a.anchor(3, 2));
    c.prefund(&w.a.anchor(3, 2), c.rent(BA::SIZE));
    assert_code(w.post_anchor(&mut c, 3, 2), E::Archived);
    let arg = w.beacons.must(w.tlock_round(3));
    let multi = permutation_frontier_svm_tests::ix::beacon::post_anchor_regions(
        &w.a,
        w.keeper.pubkey(),
        3,
        &arg,
        &[1, 2],
        &w.keeper.pubkey(),
    );
    assert_code(c.send(&[multi], &[&w.keeper]), E::Archived);
    assert!(c.is_absent(&w.a.anchor(3, 2)));
}

#[test]
fn g02_recreate_seed_cache_refused_after_archive() {
    // A cache of an archived bell cannot be created again: its THE anchor
    // is closed, and PostSeed needs the anchor itself (§5.8: NoAnchor).
    let (mut c, w) = common::test_beacon();
    expect_lands(w.post_anchor(&mut c, 4, 1), "PostAnchor");
    let r = w.seed_round(&c, 4, 1);
    expect_lands(w.post_seed(&mut c, 4, 1, 0), "PostSeed");
    w.craft_archive(&mut c, 1, archive_part(4), &[4]);
    c.remove(&w.a.anchor(4, 1));
    c.remove(&w.a.seed_cache(4, 1, 0));
    let ix = w.seed_ix(4, 1, 0, r);
    assert_code(c.send(&[ix], &[&w.keeper]), E::NoAnchor);
}

#[test]
fn g02_recreate_season_id_refused() {
    // A Season is never closed (CloseSeason leaves a 128-B tombstone), so an
    // id is single-use: AnnounceSeason on a present Season is `Announce`.
    let mut c = Chain::release();
    let w = World::announced(&mut c, 7);
    let again = World::new(&mut c, 7);
    assert_code(again.announce(&mut c), E::Announce);
    // …also after the season moved on.
    expect_lands(w.create(&mut c), "CreateSeason");
    assert_code(again.announce(&mut c), E::Announce);
}
