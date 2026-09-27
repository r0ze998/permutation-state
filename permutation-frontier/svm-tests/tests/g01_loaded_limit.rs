//! `g01_loaded_limit_<kind>` (§13.1, I-45) for W2-A's instructions, on the
//! **release** binary deployed at its `--max-len`: the kind's worst account
//! set sent with `L(kind)` (the §10.1 formula at the deployed programdata
//! length, `frontier_abi::budgets::loaded_limit_for`) loads and lands; one
//! page below the tight limit it fails `MaxLoadedAccountsDataSizeExceeded`
//! with the fee charged (the harness enforces SIMD-0186 as the validator
//! does: `g01_loaded_limit_control_*` in tests/harness.rs, and the W2-B
//! drill).
//!
//! Worst sets: every account the table (`frontier_abi::prologue`) lists at
//! its largest size — targets pre-funded (+64 B each), region-day archives
//! present (12,192 B each), PostAnchorMulti at 7 regions.
//!
//! On the release binary only real quicknet rounds verify, and the 32 SP-V2
//! fixture rounds are not 200 rounds apart, so PostAnchor and PostSeed use a
//! **crafted** `genesis_ts` (the Season field edited after
//! ConsumeGenesisSeed) that puts `T(0)` on fixture round 32,551,361, and a
//! Clock at the anchor that puts `S(A)` on fixture round 32,551,652.

mod common;

use frontier_abi::budgets::loaded_limit_for;
use frontier_abi::layout::world::season as S;
use frontier_abi::tags::Ix;
use permutation_frontier_svm_tests::chain::{
    assert_loaded_exceeded, expect_lands, Chain, Profile, PAGE,
};
use permutation_frontier_svm_tests::ix::beacon::post_anchor_regions;
use permutation_frontier_svm_tests::ix::season::set_window_schedule;
use permutation_frontier_svm_tests::world::{day_of, World};
use permutation_frontier_svm_tests::{Instruction, Keypair, Signer};

/// Fixture round `T(0)` is placed on.
const R_T: u64 = 32_551_361;
/// Fixture round `S(A)` is placed on (≥ 220 rounds after `R_T`, so `A`
/// falls after `T(0)`'s publication).
const R_S: u64 = 32_551_652;

/// Sends `ixs` at `L(ix)` (must load and land) and one page below the
/// tight limit (must fail, charged); prints the need and the slack.
fn check(c: &Chain, ix: Ix, ixs: &[Instruction], signers: &[&Keypair]) {
    let pd = c.programdata_len();
    let l = loaded_limit_for(ix, pd);
    let p = Profile::ladder(ix, pd).with_loaded(l);
    let t = c.transaction(&p, ixs, signers);
    let need = c.loaded_size(&t.message);
    let tight = (need.div_ceil(PAGE as u64) * PAGE as u64) as u32;
    println!(
        "g01 L({}) = {l} B at programdata {pd} B: need {need} B, tight {tight} B, slack {} B",
        ix.name(),
        l as u64 - need.min(l as u64)
    );
    assert!(
        need <= l as u64,
        "{}: need {need} B > L(kind) {l} B",
        ix.name()
    );
    let mut f = c.fork();
    let landed = expect_lands(f.send_with(&p, ixs, signers), ix.name());
    assert_eq!(landed.loaded, need);
    let mut f = c.fork();
    assert_loaded_exceeded(f.send_with(&p.with_loaded(tight - PAGE), ixs, signers));
    // L(kind) over-counts (the instructions sysvar and builtins, see the
    // W2-B notes) by < 1 page: it is the tight limit or one page above.
    assert!(
        l == tight || l == tight + PAGE,
        "{}: L(kind) {l} B is not within a page of the tight {tight} B",
        ix.name()
    );
    if l == tight {
        let mut f = c.fork();
        assert_loaded_exceeded(f.send_with(&p.with_loaded(l - PAGE), ixs, signers));
    } else {
        println!(
            "g01 note: L({}) is one page above the tight limit",
            ix.name()
        );
    }
}

#[test]
fn g01_loaded_limit_announce_season() {
    let mut c = Chain::release();
    let w = World::new(&mut c, 1);
    c.set_time(w.announce_time());
    c.prefund(&w.a.season, 1);
    check(&c, Ix::AnnounceSeason, &[w.announce_ix()], &[&w.authority]);
}

#[test]
fn g01_loaded_limit_create_season() {
    let mut c = Chain::release();
    let w = World::announced(&mut c, 1);
    c.set_time(w.t_create_min);
    for k in std::iter::once(w.a.frontier())
        .chain(w.a.province_funds())
        .chain(std::iter::once(w.a.defence_pool()))
    {
        c.prefund(&k, 1);
    }
    check(&c, Ix::CreateSeason, &[w.create_ix()], &[&w.authority]);
}

#[test]
fn g01_loaded_limit_init_beacon_logs() {
    let mut c = Chain::release();
    let w = World::created(&mut c, 1);
    for r in 0..16 {
        c.prefund(&w.a.beacon_log(r), 1);
    }
    let ix =
        permutation_frontier_svm_tests::ix::season::init_beacon_logs(&w.a, w.authority.pubkey());
    check(&c, Ix::InitBeaconLogs, &[ix], &[&w.authority]);
}

#[test]
fn g01_loaded_limit_init_shards() {
    let mut c = Chain::release();
    let w = World::created(&mut c, 1);
    for s in 0..8 {
        c.prefund(&w.a.join_shard(5, s), 1);
    }
    let ix = permutation_frontier_svm_tests::ix::season::init_shards(&w.a, w.authority.pubkey(), 5);
    check(&c, Ix::InitShards, &[ix], &[&w.authority]);
}

#[test]
fn g01_loaded_limit_consume_genesis_seed() {
    let mut c = Chain::release();
    let w = World::created(&mut c, 1);
    c.set_time(World::round_time(w.genesis_round()) + 3);
    let ix = permutation_frontier_svm_tests::ix::season::consume_genesis_seed(
        &w.a,
        w.keeper.pubkey(),
        &w.genesis_arg(),
    );
    check(&c, Ix::ConsumeGenesisSeed, &[ix], &[&w.keeper]);
}

#[test]
fn g01_loaded_limit_set_window_schedule() {
    let (c, w) = common::release();
    let from = w.bell_at(c.now).expect("running") + 144;
    let ix = set_window_schedule(&w.a, w.authority.pubkey(), 900, from);
    check(&c, Ix::SetWindowSchedule, &[ix], &[&w.authority]);
}

#[test]
fn g01_loaded_limit_post_beacon() {
    let (c, w) = common::release();
    let r = *w.beacons.rounds().last().expect("fixture");
    check(&c, Ix::PostBeacon, &[w.beacon_ix(15, r)], &[&w.keeper]);
}

/// A running release season whose `genesis_ts` is crafted so `T(0) = R_T`
/// (see the module text), the Clock at `A` such that `S(A) = R_S`, and
/// the region-day archives of `regions` present.
fn crafted_for_anchors(regions: &[u8]) -> (Chain, World) {
    let (mut c, w) = common::release();
    let rt = World::round_time(R_T);
    // bell_end(0) = genesis_ts + 600 = rt, so T(0) = first_round_from(rt) = R_T.
    let genesis_ts = rt - 600;
    c.edit(&w.a.season, |d| {
        d[S::GENESIS_TS..S::GENESIS_TS + 8].copy_from_slice(&genesis_ts.to_le_bytes())
    });
    assert_eq!(w_tlock(&c, &w, 0), R_T);
    let win = w.window(&c, 0) as i64;
    let a = World::round_time(R_S) - win - w.params.season.seed_margin as i64;
    assert!(a >= rt, "A after T(0)'s publication");
    c.set_time(a);
    for r in regions {
        w.craft_archive(&mut c, *r, day_of(0), &[]);
    }
    (c, w)
}

/// `T(b)` from the Season's (possibly crafted) `genesis_ts`.
fn w_tlock(c: &Chain, w: &World, bell: u32) -> u64 {
    let g = w.season_u64(c, S::GENESIS_TS) as i64;
    permutation_rules::frontier::beacon::tlock_round(
        &permutation_rules::frontier::clash::QUICKNET,
        g,
        bell,
    )
}

#[test]
fn g01_loaded_limit_post_anchor() {
    let (c, w) = crafted_for_anchors(&[6]);
    let ix = permutation_frontier_svm_tests::ix::beacon::post_anchor(
        &w.a,
        w.keeper.pubkey(),
        6,
        0,
        &w.beacons.must(R_T),
        &w.keeper.pubkey(),
    );
    check(&c, Ix::PostAnchor, &[ix], &[&w.keeper]);
}

#[test]
fn g01_loaded_limit_post_anchor_multi() {
    let regions = [0u8, 2, 4, 6, 8, 10, 12];
    let (c, w) = crafted_for_anchors(&regions);
    let ix = post_anchor_regions(
        &w.a,
        w.keeper.pubkey(),
        0,
        &w.beacons.must(R_T),
        &regions,
        &w.keeper.pubkey(),
    );
    check(&c, Ix::PostAnchorMulti, &[ix], &[&w.keeper]);
}

#[test]
fn g01_loaded_limit_post_seed() {
    let (mut c, w) = crafted_for_anchors(&[9]);
    let ix = permutation_frontier_svm_tests::ix::beacon::post_anchor(
        &w.a,
        w.keeper.pubkey(),
        9,
        0,
        &w.beacons.must(R_T),
        &w.keeper.pubkey(),
    );
    expect_lands(c.send(&[ix], &[&w.keeper]), "PostAnchor (crafted genesis)");
    assert_eq!(w.seed_round(&c, 0, 9), R_S, "S(A) is the fixture round");
    c.set_time(World::round_time(R_S) + 2);
    c.prefund(&w.a.seed_cache(0, 9, 0), 1);
    check(&c, Ix::PostSeed, &[w.seed_ix(0, 9, 0, R_S)], &[&w.keeper]);
}
