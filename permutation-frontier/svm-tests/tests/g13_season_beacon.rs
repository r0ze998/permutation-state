//! G13 (§3.5, §13.3) rows of W2-A's instructions that G2–G5 do not already
//! assert: one test per (instruction, error code), plus the refusals the
//! contract requires without pinning a code (asserted as refusals; the
//! coverage registry lists them as `Refused`). W5-A completes G13.

mod common;

use frontier_abi::presets;
use permutation_frontier_svm_tests::chain::{
    assert_code, assert_refused, expect_lands, with_account, Chain,
};
use permutation_frontier_svm_tests::ix::{beacon as bix, season as six};
use permutation_frontier_svm_tests::world::World;
use permutation_frontier_svm_tests::{
    probe, sha256, Address, FrontierError as E, Instruction, Signer,
};

#[test]
fn g13_announce_season_lead_and_bond() {
    let mut c = Chain::release();
    let mut w = World::new(&mut c, 1);
    c.set_time(w.announce_time());
    // t_create_min < now + 24 h: Announce.
    let t = w.t_create_min;
    w.t_create_min = c.now + presets::MIN_ANNOUNCE_LEAD_SECS - 1;
    assert_code(c.send(&[w.announce_ix()], &[&w.authority]), E::Announce);
    w.t_create_min = t;
    // bond below 1 SOL: Announce.
    w.bond = presets::MIN_CREATION_BOND - 1;
    assert_code(c.send(&[w.announce_ix()], &[&w.authority]), E::Announce);
    w.bond = presets::MIN_CREATION_BOND;
    expect_lands(w.announce(&mut c), "AnnounceSeason at 24 h and 1 SOL");
}

#[test]
fn g13_create_season_window_hash_params_status() {
    let mut c = Chain::release();
    let w = World::announced(&mut c, 1);
    // Before t_create_min.
    c.set_time(w.t_create_min - 1);
    assert_refused(
        c.send(&[w.create_ix()], &[&w.authority]),
        "CreateSeason before t_create_min",
    );
    // At or after t_create_min + 7 days.
    let mut late = c.fork();
    late.set_time(w.t_create_min + presets::CREATE_WINDOW_SECS);
    assert_refused(
        late.send(&[w.create_ix()], &[&w.authority]),
        "CreateSeason after the 7-day window",
    );
    c.set_time(w.t_create_min);
    // Parameters that do not hash to the announced params_hash: Announce.
    let mut other = w.params.clone();
    other.season.march_fee += 1;
    let ix = six::create(&w.a, w.authority.pubkey(), &other);
    assert_code(c.send(&[ix], &[&w.authority]), E::Announce);
    // Parameters that hash right but fail validation (announced that way).
    let mut bad = c.fork();
    let mut wb = World::new(&mut bad, 9);
    wb.t_create_min = bad.now + presets::MIN_ANNOUNCE_LEAD_SECS + 60;
    wb.params.season.reveal_window = 599;
    expect_lands(wb.announce(&mut bad), "AnnounceSeason of invalid params");
    assert_refused(wb.create(&mut bad), "CreateSeason with reveal_window 599");
    // Parameters naming another beacon key than the binary's.
    let mut bad = c.fork();
    let mut wk = World::new(&mut bad, 10);
    wk.t_create_min = bad.now + presets::MIN_ANNOUNCE_LEAD_SECS + 60;
    wk.params.season.quicknet_pk_hash = [7; 32];
    expect_lands(
        wk.announce(&mut bad),
        "AnnounceSeason of a foreign beacon key",
    );
    assert_refused(
        wk.create(&mut bad),
        "CreateSeason with another QUICKNET_PK_HASH",
    );
    // The genuine create lands; a second is WrongStatus (not Announced).
    expect_lands(w.create(&mut c), "CreateSeason");
    assert_code(w.create(&mut c), E::WrongStatus);
}

#[test]
fn g13_init_beacon_logs_and_shards_status_authority_repeat() {
    let mut c = Chain::release();
    let w = World::announced(&mut c, 1);
    // Announced (not Created): WrongStatus.
    assert_code(w.init_logs(&mut c), E::WrongStatus);
    assert_code(w.init_shards(&mut c, 0), E::WrongStatus);
    expect_lands(w.create(&mut c), "CreateSeason");
    // Not the authority: Auth.
    let stranger = c.funded(b"stranger", 10);
    let ix = with_account(
        six::init_beacon_logs(&w.a, w.authority.pubkey()),
        0,
        stranger.pubkey(),
    );
    assert_code(c.send(&[ix], &[&stranger]), E::Auth);
    let ix = with_account(
        six::init_shards(&w.a, w.authority.pubkey(), 1),
        0,
        stranger.pubkey(),
    );
    assert_code(c.send(&[ix], &[&stranger]), E::Auth);
    // Faction 6: refused.
    assert_refused(w.init_shards(&mut c, 6), "InitShards(6)");
    // Genuine, then repeats refused.
    expect_lands(w.init_logs(&mut c), "InitBeaconLogs");
    expect_lands(w.init_shards(&mut c, 1), "InitShards(1)");
    assert_refused(w.init_logs(&mut c), "InitBeaconLogs repeat");
    assert_refused(w.init_shards(&mut c, 1), "InitShards(1) repeat");
}

/// The probe deployed next to the Frontier program, for CPIs.
fn with_probe(c: &mut Chain) -> Address {
    let so = probe::load();
    let k = Address::new_from_array(sha256(&[b"cpi probe"]));
    c.deploy(k, &so, so.len(), None);
    k
}

#[test]
fn g13_top_level_only_instructions_refuse_a_cpi() {
    // §5.5: ConsumeGenesisSeed, PostAnchor, PostAnchorMulti, PostSeed and
    // PostBeacon refuse `get_stack_height() > 1` (NotTopLevel).
    let mut c = Chain::test_beacon();
    let probe_id = with_probe(&mut c);
    let w = World::created(&mut c, 1);
    c.set_time(World::round_time(w.genesis_round()) + 3);
    let via = |ix: &Instruction| probe::cpi(probe_id, ix);
    let genesis = six::consume_genesis_seed(&w.a, w.keeper.pubkey(), &w.genesis_arg());
    assert_code(c.send(&[via(&genesis)], &[&w.keeper]), E::NotTopLevel);
    expect_lands(w.init_logs(&mut c), "InitBeaconLogs");
    expect_lands(
        c.send(&[genesis], &[&w.keeper]),
        "ConsumeGenesisSeed at top level",
    );
    c.set_time(w.genesis_ts());
    w.to_anchor_time(&mut c, 0);
    let anchor = w.anchor_ix(0, 1);
    assert_code(c.send(&[via(&anchor)], &[&w.keeper]), E::NotTopLevel);
    let arg = w.beacons.must(w.tlock_round(0));
    let multi = bix::post_anchor_regions(
        &w.a,
        w.keeper.pubkey(),
        0,
        &arg,
        &[2, 3],
        &w.keeper.pubkey(),
    );
    assert_code(c.send(&[via(&multi)], &[&w.keeper]), E::NotTopLevel);
    expect_lands(c.send(&[anchor], &[&w.keeper]), "PostAnchor at top level");
    let s = w.seed_round(&c, 0, 1);
    c.set_time(World::round_time(s) + 1);
    let seed = w.seed_ix(0, 1, 0, s);
    assert_code(c.send(&[via(&seed)], &[&w.keeper]), E::NotTopLevel);
    let beacon = w.beacon_ix(1, s + 1);
    c.set_time(World::round_time(s + 1));
    assert_code(c.send(&[via(&beacon)], &[&w.keeper]), E::NotTopLevel);
    expect_lands(c.send(&[seed], &[&w.keeper]), "PostSeed at top level");
    expect_lands(c.send(&[beacon], &[&w.keeper]), "PostBeacon at top level");
}

#[test]
fn g13_keeper_writes_need_the_fee_payer_signature() {
    // §5.6 K: `[fee_payer s,w]`; a missing signature is `Auth` (v1.2).
    let (mut c, w) = common::test_beacon();
    w.to_anchor_time(&mut c, 0);
    let mut ix = w.anchor_ix(0, 2);
    let other = c.funded(b"outer payer", 1);
    ix.accounts[0].is_signer = false;
    assert_code(c.send(&[ix], &[&other]), E::Auth);
}
