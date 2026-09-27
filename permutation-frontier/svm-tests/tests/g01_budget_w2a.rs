//! G1 (§13.1) for W2-A's instructions, committed (integ-W2 review of W2-A:
//! the §5.5 margins lived only in an uncommitted scratch harness). Each
//! instruction is measured on a fork at the retry-ladder limit
//! (`Chain::measure`) in the worst state wave 2 can build, and asserted
//! against `frontier_abi::budgets` (CU, tx bytes, locks, loaded data; heap
//! on the trace build only). Season lifecycle on the release build, the
//! beacon writes on the test-beacon build (same verification, any round,
//! I-53). Every measured figure is printed (`--nocapture`).
//!
//! Budgets amended in v1.3 (§19): AnnounceSeason 25k, InitBeaconLogs 80k,
//! InitShards 45k.

mod common;

use frontier_abi::tags::Ix;
use permutation_frontier_svm_tests::budget::{assert_within, ceilings};
use permutation_frontier_svm_tests::chain::{expect_lands, Chain};
use permutation_frontier_svm_tests::ix::beacon::post_anchor_regions;
use permutation_frontier_svm_tests::ix::season as six;
use permutation_frontier_svm_tests::world::{archive_part, World};
use permutation_frontier_svm_tests::{Instruction, Keypair, Signer};

fn within(c: &Chain, ix: Ix, label: &str, ixs: &[Instruction], signer: &Keypair) -> u64 {
    let need = c
        .measure(ixs, &[signer])
        .unwrap_or_else(|f| panic!("{label}: refused while measuring: {f:?}"));
    assert_within(label, &need, &ceilings(ix, 0, c.programdata_len()));
    need.cu
}

#[test]
fn g01_budget_w2a_season_lifecycle() {
    let mut c = Chain::release();
    // AnnounceSeason: the PDA search costs ≈ 1.5k CU per extra bump try,
    // so measure 64 ids and keep the worst.
    let mut worst = (0u64, 0u64);
    for id in 1..=64u64 {
        let mut f = c.fork();
        let w = World::new(&mut f, id);
        f.set_time(w.announce_time());
        let cu = within(
            &f,
            Ix::AnnounceSeason,
            &format!("AnnounceSeason id {id}"),
            &[w.announce_ix()],
            &w.authority,
        );
        worst = worst.max((cu, id));
    }
    println!(
        "AnnounceSeason worst of 64 ids: {} CU (id {})",
        worst.0, worst.1
    );

    let w = World::announced(&mut c, 1);
    c.set_time(w.t_create_min);
    within(
        &c,
        Ix::CreateSeason,
        "CreateSeason",
        &[w.create_ix()],
        &w.authority,
    );
    expect_lands(w.create(&mut c), "CreateSeason");
    let logs = six::init_beacon_logs(&w.a, w.authority.pubkey());
    within(
        &c,
        Ix::InitBeaconLogs,
        "InitBeaconLogs (16 logs)",
        &[logs],
        &w.authority,
    );
    expect_lands(w.init_logs(&mut c), "InitBeaconLogs");
    for f in 0..6u8 {
        let shards = six::init_shards(&w.a, w.authority.pubkey(), f);
        within(
            &c,
            Ix::InitShards,
            &format!("InitShards({f})"),
            &[shards],
            &w.authority,
        );
        expect_lands(w.init_shards(&mut c, f), "InitShards");
    }
    let t = World::round_time(w.genesis_round());
    if c.now < t {
        c.set_time(t);
    }
    let genesis = six::consume_genesis_seed(&w.a, w.keeper.pubkey(), &w.genesis_arg());
    within(
        &c,
        Ix::ConsumeGenesisSeed,
        "ConsumeGenesisSeed",
        &[genesis],
        &w.keeper,
    );
    expect_lands(w.consume_genesis(&mut c), "ConsumeGenesisSeed");
    let g = w.genesis_ts();
    if c.now < g {
        c.set_time(g);
    }
    let from = w.bell_at(c.now).expect("running") + 144;
    let win = six::set_window_schedule(&w.a, w.authority.pubkey(), 1_200, from);
    within(
        &c,
        Ix::SetWindowSchedule,
        "SetWindowSchedule",
        &[win],
        &w.authority,
    );
    let round = w.beacons.rounds()[20];
    within(
        &c,
        Ix::PostBeacon,
        "PostBeacon (release)",
        &[w.beacon_ix(9, round)],
        &w.keeper,
    );
}

#[test]
fn g01_budget_w2a_beacon_writes() {
    let (mut c, w) = common::test_beacon();
    let bell = 4u32;
    w.to_anchor_time(&mut c, bell);
    // A present archive of the region-day (other bells archived) is the
    // read-heavy path; absent is measured too.
    within(
        &c,
        Ix::PostAnchor,
        "PostAnchor (archive absent)",
        &[w.anchor_ix(bell, 3)],
        &w.keeper,
    );
    let mut g = c.fork();
    w.craft_archive(&mut g, 3, archive_part(bell), &[bell + 1]);
    within(
        &g,
        Ix::PostAnchor,
        "PostAnchor (archive present)",
        &[w.anchor_ix(bell, 3)],
        &w.keeper,
    );
    // PostAnchorMulti at MULTI_MAX_REGIONS (7) with every archive present.
    let regions = [1u8, 2, 5, 6, 11, 12, 13];
    let arg = w.beacons.must(w.tlock_round(bell));
    let mut g = c.fork();
    for r in regions {
        w.craft_archive(&mut g, r, archive_part(bell), &[bell + 1]);
    }
    let multi = post_anchor_regions(
        &w.a,
        w.keeper.pubkey(),
        bell,
        &arg,
        &regions,
        &w.keeper.pubkey(),
    );
    within(
        &g,
        Ix::PostAnchorMulti,
        "PostAnchorMulti (7 regions, archives present)",
        &[multi],
        &w.keeper,
    );
    // The no-op re-post of a present anchor.
    expect_lands(w.post_anchor(&mut c, bell, 3), "PostAnchor");
    within(
        &c,
        Ix::PostAnchor,
        "PostAnchor (present: no-op)",
        &[w.anchor_ix(bell, 3)],
        &w.keeper,
    );
    // PostSeed of THE anchor's round, and its no-op re-post.
    let round = w.seed_round(&c, bell, 3);
    let t = World::round_time(round);
    if c.now < t {
        c.set_time(t);
    }
    within(
        &c,
        Ix::PostSeed,
        "PostSeed",
        &[w.seed_ix(bell, 3, 0, round)],
        &w.keeper,
    );
    expect_lands(w.post_seed(&mut c, bell, 3, 0), "PostSeed");
    within(
        &c,
        Ix::PostSeed,
        "PostSeed (present: no-op)",
        &[w.seed_ix(bell, 3, 0, round)],
        &w.keeper,
    );
    // PostBeacon on the test-beacon build (+ the round-due check).
    within(
        &c,
        Ix::PostBeacon,
        "PostBeacon (test-beacon)",
        &[w.beacon_ix(3, round)],
        &w.keeper,
    );
}
