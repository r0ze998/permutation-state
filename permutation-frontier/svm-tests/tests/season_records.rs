//! The Season's PS2 records and event chain through W2-A's lifecycle
//! instructions (§4.3, §6): every chained account a transaction touches
//! ends on the last link its records carry, each link continuing the chain
//! (`head = sha256(prev ‖ le64(seq) ‖ body_without_tail)`).

mod common;

use frontier_abi::layout::world::season as S;
use frontier_abi::log::{EntityKind, Kind};
use frontier_abi::presets::RULESET_HASH;
use permutation_frontier_svm_tests::chain::{expect_lands, Chain};
use permutation_frontier_svm_tests::ix::season::set_window_schedule;
use permutation_frontier_svm_tests::records::{self, head_of, ChainWatch};
use permutation_frontier_svm_tests::world::World;
use permutation_frontier_svm_tests::Signer;

#[test]
fn season_records_and_chain_through_the_lifecycle() {
    let mut c = Chain::release();
    let w = World::new(&mut c, 2);

    let season = ChainWatch::new(&c, w.a.season, EntityKind::Season);
    let l = expect_lands(w.announce(&mut c), "AnnounceSeason");
    let rec = records::one(&l.logs, Kind::ANNOUNCE);
    assert_eq!(rec.key_u64("season_id"), 2);
    assert_eq!(rec.field("params_hash", true), w.params.hash());
    assert_eq!(rec.u64("t_create_min") as i64, w.t_create_min);
    assert_eq!(rec.u64("bond"), w.bond);
    season.check(&c, &l.logs, 1);
    assert_eq!(
        head_of(&c, &w.a.season).0,
        1,
        "the creation record is seq 1"
    );

    let season = ChainWatch::new(&c, w.a.season, EntityKind::Season);
    let frontier = ChainWatch::new(&c, w.a.frontier(), EntityKind::Frontier);
    let l = expect_lands(w.create(&mut c), "CreateSeason");
    let rec = records::one(&l.logs, Kind::SEASON_CREATED);
    assert_eq!(rec.u64("genesis_round"), w.genesis_round());
    assert_eq!(rec.u64("genesis_ts") as i64, w.genesis_ts());
    assert_eq!(rec.field("ruleset_hash", true), RULESET_HASH);
    assert_eq!(rec.field("quicknet_pk_hash", true), w.beacons.pk_hash());
    season.check(&c, &l.logs, 1);
    frontier.check(&c, &l.logs, 0);
    let d = c.data(&w.a.season);
    assert_eq!(d[S::STATUS], S::STATUS_CREATED);
    assert_eq!(d[S::RULESET_HASH..S::RULESET_HASH + 32], RULESET_HASH);
    assert_eq!(
        u32::from_le_bytes(
            d[S::REVEAL_LOADED_LIMIT..S::REVEAL_LOADED_LIMIT + 4]
                .try_into()
                .unwrap()
        ),
        w.params.season.reveal_loaded_limit
    );
    assert_eq!(
        d[S::JOIN_GATE..S::JOIN_GATE + 32],
        w.params.season.join_gate
    );

    expect_lands(w.init_logs(&mut c), "InitBeaconLogs");
    for f in 0..6 {
        let shard = ChainWatch::new(&c, w.a.join_shard(f, 0), EntityKind::JoinShard);
        let l = expect_lands(w.init_shards(&mut c, f), "InitShards");
        shard.check(&c, &l.logs, 0);
    }

    let season = ChainWatch::new(&c, w.a.season, EntityKind::Season);
    let l = expect_lands(w.consume_genesis(&mut c), "ConsumeGenesisSeed");
    records::one(&l.logs, Kind::GENESIS_SEED);
    season.check(&c, &l.logs, 1);

    c.set_time(w.genesis_ts() + 600);
    let from = w.bell_at(c.now).unwrap() + 150;
    let season = ChainWatch::new(&c, w.a.season, EntityKind::Season);
    let l = expect_lands(
        c.send(
            &[set_window_schedule(&w.a, w.authority.pubkey(), 1_500, from)],
            &[&w.authority],
        ),
        "SetWindowSchedule",
    );
    let rec = records::one(&l.logs, Kind::WINDOW);
    assert_eq!(rec.u64("window_from_bell") as u32, from);
    season.check(&c, &l.logs, 1);
    assert_eq!(head_of(&c, &w.a.season).0, 4, "four Season records so far");
}
