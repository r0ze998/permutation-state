//! G5 (§13.3): no choice among published seeds — one anchor per (bell,
//! region), every cache nonce gives the same seed, no second anchor after
//! archive, and the rounds are fixed by the rule (`T(b)` for anchors,
//! `S(A)` of THE anchor for caches, the announced genesis round). The
//! ResolveFromInputs half ("refuses a cache whose A differs from THE
//! anchor's") is W4-A's.

mod common;

use frontier_abi::layout::beacon::{bell_anchor as BA, seed_cache as SC};
use frontier_abi::layout::world::season as S;
use frontier_abi::log::{EntityKind, Kind};
use permutation_frontier_svm_tests::chain::{assert_code, expect_lands, Chain};
use permutation_frontier_svm_tests::ix::beacon::{post_anchor, post_anchor_regions, post_seed};
use permutation_frontier_svm_tests::ix::season::consume_genesis_seed;
use permutation_frontier_svm_tests::records::{self, le, ChainWatch};
use permutation_frontier_svm_tests::world::{day_of, World};
use permutation_frontier_svm_tests::{FrontierError as E, Signer};

#[test]
fn g05_one_anchor_per_bell_region() {
    let (mut c, w) = common::test_beacon();
    let l = expect_lands(w.post_anchor(&mut c, 4, 9), "PostAnchor");
    assert_eq!(records::of_kind(&l.logs, Kind::ANCHOR).len(), 1);
    let first = c.data(&w.a.anchor(4, 9));
    let lamports = c.lamports(&w.a.anchor(4, 9));
    // Later posts of the same (bell, region): success, no-op, A unchanged.
    for delay in [1i64, 60, 900] {
        c.advance(delay);
        let other_keeper = c.funded(format!("keeper-{delay}").as_bytes(), 1);
        let arg = w.beacons.must(w.tlock_round(4));
        let ix = post_anchor(
            &w.a,
            other_keeper.pubkey(),
            9,
            4,
            &arg,
            &other_keeper.pubkey(),
        );
        let l = expect_lands(c.send(&[ix], &[&other_keeper]), "PostAnchor repeat");
        assert!(
            records::of_kind(&l.logs, Kind::ANCHOR).is_empty(),
            "a no-op logs nothing"
        );
        assert_eq!(c.data(&w.a.anchor(4, 9)), first, "THE anchor is unchanged");
        assert_eq!(c.lamports(&w.a.anchor(4, 9)), lamports);
    }
    // PostAnchorMulti skips it and creates the others (one verification).
    let arg = w.beacons.must(w.tlock_round(4));
    let ix = post_anchor_regions(
        &w.a,
        w.keeper.pubkey(),
        4,
        &arg,
        &[8, 9, 10],
        &w.keeper.pubkey(),
    );
    let l = expect_lands(c.send(&[ix], &[&w.keeper]), "PostAnchorMulti");
    let recs = records::of_kind(&l.logs, Kind::ANCHOR);
    assert_eq!(
        recs.len(),
        2,
        "two anchors created, the present one skipped"
    );
    assert_eq!(c.data(&w.a.anchor(4, 9)), first);
    assert_eq!(w.anchor_a(&c, 4, 8), Some(c.now));
    assert_eq!(w.anchor_a(&c, 4, 10), Some(c.now));
    // A multi whose anchors are all present is a no-op too.
    let ix = post_anchor_regions(
        &w.a,
        w.keeper.pubkey(),
        4,
        &arg,
        &[8, 9, 10],
        &w.keeper.pubkey(),
    );
    let l = expect_lands(c.send(&[ix], &[&w.keeper]), "PostAnchorMulti repeat");
    assert!(records::of_kind(&l.logs, Kind::ANCHOR).is_empty());
}

#[test]
fn g05_multi_anchor_equals_single_anchors() {
    let (base, w) = common::test_beacon();
    let regions = [1u8, 2, 5, 6, 11, 12, 13];
    let mut single = base.fork();
    let mut multi = base.fork();
    w.to_anchor_time(&mut single, 6);
    w.to_anchor_time(&mut multi, 6);
    let arg = w.beacons.must(w.tlock_round(6));
    for r in regions {
        let ix = post_anchor(&w.a, w.keeper.pubkey(), r, 6, &arg, &w.keeper.pubkey());
        single.svm.expire_blockhash();
        expect_lands(single.send(&[ix], &[&w.keeper]), "PostAnchor");
    }
    let ix = post_anchor_regions(
        &w.a,
        w.keeper.pubkey(),
        6,
        &arg,
        &regions,
        &w.keeper.pubkey(),
    );
    expect_lands(
        multi.send(&[ix], &[&w.keeper]),
        "PostAnchorMulti (7 regions)",
    );
    for r in regions {
        let (a, b) = (
            single.data(&w.a.anchor(6, r)),
            multi.data(&w.a.anchor(6, r)),
        );
        assert_eq!(
            a[..BA::EV_PRICE],
            b[..BA::EV_PRICE],
            "region {r}: same anchor fields"
        );
    }
    // More than MULTI_MAX_REGIONS (7) is refused.
    let mut c = base.fork();
    w.to_anchor_time(&mut c, 7);
    let arg = w.beacons.must(w.tlock_round(7));
    let ix = post_anchor_regions(
        &w.a,
        w.keeper.pubkey(),
        7,
        &arg,
        &[0, 1, 2, 3, 4, 5, 6, 7],
        &w.keeper.pubkey(),
    );
    assert!(c.send(&[ix], &[&w.keeper]).is_err(), "8 regions refused");
}

#[test]
fn g05_every_cache_nonce_gives_the_same_seed() {
    let (mut c, w) = common::test_beacon();
    let (bell, r) = (3u32, 12u8);
    expect_lands(w.post_anchor(&mut c, bell, r), "PostAnchor");
    let s = w.seed_round(&c, bell, r);
    let want = w.beacons.seed(s).expect("seed");
    let a = w.anchor_a(&c, bell, r).unwrap();
    for nonce in [0u8, 1, 7, 255] {
        c.advance(3);
        let l = expect_lands(w.post_seed(&mut c, bell, r, nonce), "PostSeed");
        assert_eq!(w.cache_seed(&c, bell, r, nonce), want, "nonce {nonce}");
        let d = c.data(&w.a.seed_cache(bell, r, nonce));
        assert_eq!(le(&d[SC::ROUND..SC::ROUND + 8]), s);
        assert_eq!(
            le(&d[SC::A..SC::A + 8]) as i64,
            a,
            "the cache records THE anchor's A"
        );
        assert_eq!(
            d[SC::ANCHOR_KEY..SC::ANCHOR_KEY + 32],
            w.a.anchor(bell, r).to_bytes()
        );
        let rec = records::one(&l.logs, Kind::SEED);
        assert_eq!(rec.field("seed", true), want);
    }
    // A repeat of a nonce is a no-op.
    let before = c.data(&w.a.seed_cache(bell, r, 1));
    let l = expect_lands(w.post_seed(&mut c, bell, r, 1), "PostSeed repeat");
    assert!(records::of_kind(&l.logs, Kind::SEED).is_empty());
    assert_eq!(c.data(&w.a.seed_cache(bell, r, 1)), before);
}

#[test]
fn g05_seed_round_is_s_of_the_anchor() {
    let (mut c, w) = common::test_beacon();
    let (bell, r) = (2u32, 4u8);
    // No anchor yet: NoAnchor.
    let s_guess = w.tlock_round(bell) + 400;
    let t = World::round_time(s_guess + 2);
    c.set_time(t);
    assert_code(
        c.send(&[w.seed_ix(bell, r, 0, s_guess)], &[&w.keeper]),
        E::NoAnchor,
    );
    expect_lands(w.post_anchor(&mut c, bell, r), "PostAnchor");
    let s = w.seed_round(&c, bell, r);
    c.set_time(World::round_time(s + 2).max(c.now));
    for wrong in [s - 1, s + 1, w.tlock_round(bell)] {
        assert_code(
            c.send(&[w.seed_ix(bell, r, 0, wrong)], &[&w.keeper]),
            E::WrongRound,
        );
    }
    let forged = w.beacons.forged(s);
    let ix = post_seed(
        &w.a,
        w.keeper.pubkey(),
        r,
        bell,
        0,
        &forged,
        &w.keeper.pubkey(),
    );
    assert_code(c.send(&[ix], &[&w.keeper]), E::Crypto);
    expect_lands(
        c.send(&[w.seed_ix(bell, r, 0, s)], &[&w.keeper]),
        "PostSeed with S(A)",
    );
}

#[test]
fn g05_anchor_round_is_t_of_the_bell() {
    let (mut c, w) = common::test_beacon();
    let bell = 9u32;
    w.to_anchor_time(&mut c, bell);
    c.advance(30);
    let t = w.tlock_round(bell);
    for wrong in [t - 1, t + 1, w.tlock_round(bell - 1)] {
        let arg = w.beacons.must(wrong);
        let ix = post_anchor(&w.a, w.keeper.pubkey(), 3, bell, &arg, &w.keeper.pubkey());
        assert_code(c.send(&[ix], &[&w.keeper]), E::WrongRound);
    }
    let forged = w.beacons.forged(t);
    let ix = post_anchor(
        &w.a,
        w.keeper.pubkey(),
        3,
        bell,
        &forged,
        &w.keeper.pubkey(),
    );
    assert_code(c.send(&[ix], &[&w.keeper]), E::Crypto);
    let ix = post_anchor_regions(
        &w.a,
        w.keeper.pubkey(),
        bell,
        &forged,
        &[3, 4],
        &w.keeper.pubkey(),
    );
    assert_code(c.send(&[ix], &[&w.keeper]), E::Crypto);
    expect_lands(w.post_anchor(&mut c, bell, 3), "PostAnchor with T(b)");
    assert!(
        c.is_absent(&w.a.anchor(bell, 4)),
        "refusals created nothing"
    );
}

#[test]
fn g05_no_second_anchor_after_archive() {
    // ArchiveAnchors (W4-B) sets the tombstone bit before closing THE anchor
    // (§5.8); crafted here. A new anchor for the bell would carry a new A
    // and so a new seed round: refused (`Archived`), single and multi.
    let (mut c, w) = common::test_beacon();
    let (bell, r) = (1u32, 13u8);
    expect_lands(w.post_anchor(&mut c, bell, r), "PostAnchor");
    let old_a = w.anchor_a(&c, bell, r).unwrap();
    c.advance(172_800);
    w.craft_archive(&mut c, r, day_of(bell), &[bell]);
    c.remove(&w.a.anchor(bell, r));
    assert_code(w.post_anchor(&mut c, bell, r), E::Archived);
    let arg = w.beacons.must(w.tlock_round(bell));
    let ix = post_anchor_regions(
        &w.a,
        w.keeper.pubkey(),
        bell,
        &arg,
        &[r],
        &w.keeper.pubkey(),
    );
    assert_code(c.send(&[ix], &[&w.keeper]), E::Archived);
    assert!(c.is_absent(&w.a.anchor(bell, r)));
    let _ = old_a;
}

#[test]
fn g05_genesis_seed_is_the_announced_rounds() {
    // On the release binary with a real quicknet round (the SP-V2 fixture
    // the world places its genesis on).
    let mut c = Chain::release();
    let w = World::created(&mut c, 1);
    c.set_time(World::round_time(w.genesis_round()) + 5);
    let rounds = w.beacons.rounds();
    // Another real round: WrongRound.
    let other = rounds[1];
    let arg = w.beacons.must(other);
    assert_code(
        c.send(
            &[consume_genesis_seed(&w.a, w.keeper.pubkey(), &arg)],
            &[&w.keeper],
        ),
        E::WrongRound,
    );
    // The genesis round with another round's signature: Crypto.
    let mut forged = w.genesis_arg();
    forged.sig48 = w.beacons.must(other).sig48;
    assert_code(
        c.send(
            &[consume_genesis_seed(&w.a, w.keeper.pubkey(), &forged)],
            &[&w.keeper],
        ),
        E::Crypto,
    );
    // The genesis round: Seeded, seed = seed_of(round), chained record.
    let watch = ChainWatch::new(&c, w.a.season, EntityKind::Season);
    let l = expect_lands(w.consume_genesis(&mut c), "ConsumeGenesisSeed");
    assert_eq!(w.status(&c), S::STATUS_SEEDED);
    let seed = w.beacons.seed(w.genesis_round()).unwrap();
    assert_eq!(
        c.data(&w.a.season)[S::GENESIS_SEED..S::GENESIS_SEED + 32],
        seed
    );
    let rec = records::one(&l.logs, Kind::GENESIS_SEED);
    assert_eq!(rec.field("seed", true), seed);
    watch.check(&c, &l.logs, 1);
    // A second consume: the season is no longer Created.
    assert_code(w.consume_genesis(&mut c), E::WrongStatus);
}
