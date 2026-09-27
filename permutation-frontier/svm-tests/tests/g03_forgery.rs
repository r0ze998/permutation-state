//! G3 (§13.2) for the keyed accounts W2-A's instructions read or write:
//! each refused with the pinned code — wrong address (`BadAddress`), wrong
//! owner, wrong magic, wrong season, wrong key fields (`BadAccount`),
//! absence claimed at a non-canonical address (`BadAddress`); a forged
//! anchor or archive where one is read.
//!
//! Forged accounts are written with `Chain::put`; on a real chain nobody
//! but the program can own an account at a with-seed address of the
//! Season PDA, so these are defence in depth (§3.3: "recompute every keyed
//! address a reader trusts").

mod common;

use common::copy_to_fresh;
use frontier_abi::layout::beacon::{anchor_archive as AA, bell_anchor as BA, seed_cache as SC};
use frontier_abi::layout::world::{beacon_log as BL, season as S};
use permutation_frontier_svm_tests::chain::{
    assert_code, assert_refused, expect_lands, with_account, Chain, Profile, SendResult,
};
use permutation_frontier_svm_tests::ix::beacon::{at, post_anchor_regions, seed_at};
use permutation_frontier_svm_tests::world::{archive_part, World};
use permutation_frontier_svm_tests::{
    sha256, Address, FrontierError as E, Instruction, Keypair, Signer,
};

/// The four "present but not ours" forgeries of an account at its canonical
/// address: another owner, another magic, another season id, another key
/// field (`key_off`, one byte flipped).
fn forgeries(c: &Chain, k: &Address, key_off: usize) -> Vec<(&'static str, Address, Vec<u8>)> {
    let d = c.data(k);
    let other_program = Address::new_from_array(sha256(&[b"another program"]));
    let mut magic = d.clone();
    magic[0] ^= 0x20;
    let mut season = d.clone();
    season[8] ^= 0x01;
    let mut keyf = d.clone();
    keyf[key_off] ^= 0x01;
    vec![
        ("owner", other_program, d),
        ("magic", c.program, magic),
        ("season", c.program, season),
        ("key fields", c.program, keyf),
    ]
}

/// Runs `send` on a fork where `k` holds each forgery; every one is
/// refused with `want` (§13.2: `BadAccount`).
fn each_forgery_is(
    want: E,
    c: &Chain,
    k: &Address,
    key_off: usize,
    mut send: impl FnMut(&mut Chain) -> SendResult,
) {
    for (what, owner, data) in forgeries(c, k, key_off) {
        let mut f = c.fork();
        let l = f.lamports(k).max(f.rent(data.len()));
        f.put(*k, owner, data, l);
        let r = send(&mut f);
        match r {
            Ok(_) => panic!("forged {what} at {k} accepted"),
            Err(_) => {
                assert_code(r, want);
            }
        }
    }
}

fn keeper_send(c: &mut Chain, w: &World, ix: Instruction) -> SendResult {
    c.send(&[ix], &[&w.keeper])
}

// ------------------------------------------------------------ Season

#[test]
fn g03_season_forged_in_keeper_and_authority_instructions() {
    // The Season's key is its id (a PDA), so "wrong season" and "wrong key
    // fields" are the non-canonical case: a program-owned copy elsewhere.
    let (c, w) = common::release();
    let round = w.beacons.rounds()[3];
    let from_bell = w.bell_at(c.now).expect("running") + 200;
    let cases: Vec<(&str, Instruction, &Keypair)> = vec![
        ("PostBeacon", w.beacon_ix(4, round), &w.keeper),
        (
            "SetWindowSchedule",
            permutation_frontier_svm_tests::ix::season::set_window_schedule(
                &w.a,
                w.authority.pubkey(),
                900,
                from_bell,
            ),
            &w.authority,
        ),
    ];
    for (label, ix, signer) in cases {
        let mut f = c.fork();
        let fake = copy_to_fresh(&mut f, &w.a.season, label.as_bytes());
        let r = f.send(&[with_account(ix.clone(), 1, fake)], &[signer]);
        assert!(r.is_err(), "{label}: a copied Season accepted");
        assert_code(r, E::BadAddress);
        for (what, owner, data) in forgeries(&c, &w.a.season, S::STATUS).into_iter().take(2) {
            let mut f = c.fork();
            let l = f.lamports(&w.a.season);
            f.put(w.a.season, owner, data, l);
            let r = f.send(std::slice::from_ref(&ix), &[signer]);
            assert!(r.is_err(), "{label}: Season with a forged {what} accepted");
            assert_code(r, E::BadAccount);
        }
    }
}

/// A program-owned copy of the Season at another address, and the genuine
/// Season with one byte of its id flipped (the stored id no longer gives
/// the canonical PDA): both `BadAddress` (§3.3, `prologue::{season,
/// keeper}` recompute the PDA).
fn season_copy_and_id_flip(c: &Chain, w: &World, label: &str, ix: &Instruction, signer: &Keypair) {
    let mut f = c.fork();
    let fake = copy_to_fresh(&mut f, &w.a.season, label.as_bytes());
    let r = f.send(&[with_account(ix.clone(), 1, fake)], &[signer]);
    assert!(r.is_err(), "{label}: a copied Season accepted");
    assert_code(r, E::BadAddress);
    let mut f = c.fork();
    f.edit(&w.a.season, |d| d[8] ^= 0x01);
    let r = f.send(std::slice::from_ref(ix), &[signer]);
    assert!(r.is_err(), "{label}: a Season with a flipped id accepted");
    assert_code(r, E::BadAddress);
}

#[test]
fn g03_season_copy_and_id_flip_in_every_w2a_instruction() {
    // Every W2-A instruction that lists the Season (integ-W2 review of W2-B:
    // one test per instruction, so a regression in any single prologue is
    // caught). CreateSeason reads an Announced Season, InitBeaconLogs,
    // InitShards and ConsumeGenesisSeed a Created one, the beacon writes
    // and SetWindowSchedule a Running one.
    let mut c = Chain::release();
    let w = World::announced(&mut c, 1);
    c.set_time(w.t_create_min);
    season_copy_and_id_flip(&c, &w, "CreateSeason", &w.create_ix(), &w.authority);
    expect_lands(w.create(&mut c), "CreateSeason");
    let six = |f: fn(&permutation_frontier_svm_tests::Addresses, Address) -> Instruction| {
        f(&w.a, w.authority.pubkey())
    };
    season_copy_and_id_flip(
        &c,
        &w,
        "InitBeaconLogs",
        &six(permutation_frontier_svm_tests::ix::season::init_beacon_logs),
        &w.authority,
    );
    let shards =
        permutation_frontier_svm_tests::ix::season::init_shards(&w.a, w.authority.pubkey(), 0);
    season_copy_and_id_flip(&c, &w, "InitShards", &shards, &w.authority);
    let t = World::round_time(w.genesis_round());
    if c.now < t {
        c.set_time(t);
    }
    let genesis = permutation_frontier_svm_tests::ix::season::consume_genesis_seed(
        &w.a,
        w.keeper.pubkey(),
        &w.genesis_arg(),
    );
    season_copy_and_id_flip(&c, &w, "ConsumeGenesisSeed", &genesis, &w.keeper);

    let (mut c, w) = common::test_beacon();
    let (bell, r) = (3u32, 4u8);
    w.to_anchor_time(&mut c, bell);
    season_copy_and_id_flip(&c, &w, "PostAnchor", &w.anchor_ix(bell, r), &w.keeper);
    let arg = w.beacons.must(w.tlock_round(bell));
    let multi = post_anchor_regions(
        &w.a,
        w.keeper.pubkey(),
        bell,
        &arg,
        &[r, r + 1],
        &w.keeper.pubkey(),
    );
    season_copy_and_id_flip(&c, &w, "PostAnchorMulti", &multi, &w.keeper);
    expect_lands(w.post_anchor(&mut c, bell, r), "PostAnchor");
    let round = w.seed_round(&c, bell, r);
    let t = World::round_time(round);
    if c.now < t {
        c.set_time(t);
    }
    season_copy_and_id_flip(&c, &w, "PostSeed", &w.seed_ix(bell, r, 0, round), &w.keeper);
    season_copy_and_id_flip(&c, &w, "PostBeacon", &w.beacon_ix(r, round), &w.keeper);
    let from = w.bell_at(c.now).expect("running") + 200;
    let win = permutation_frontier_svm_tests::ix::season::set_window_schedule(
        &w.a,
        w.authority.pubkey(),
        900,
        from,
    );
    season_copy_and_id_flip(&c, &w, "SetWindowSchedule", &win, &w.authority);
}

#[test]
fn g03_forged_present_targets_of_the_init_instructions() {
    // A program-owned account that is not a genuine instance at a target
    // address of CreateSeason, InitBeaconLogs or InitShards is `BadAccount`
    // (a genuine present one is `AlreadyDone` for the last two, g13).
    let mut c = Chain::release();
    let w = World::announced(&mut c, 1);
    c.set_time(w.t_create_min);
    let targets = [w.a.frontier(), w.a.province_fund(3), w.a.defence_pool()];
    for t in targets {
        let mut f = c.fork();
        f.put_program_account(t, vec![0x5a; 128]);
        let r = f.send(&[w.create_ix()], &[&w.authority]);
        assert!(r.is_err(), "CreateSeason over a forged {t}");
        assert_code(r, E::BadAccount);
    }
    expect_lands(w.create(&mut c), "CreateSeason");
    // A present-looking BeaconLog / JoinShard: a genuine one's bytes with
    // the season id flipped (every other field right).
    let mut g = c.fork();
    expect_lands(w.init_logs(&mut g), "InitBeaconLogs");
    expect_lands(w.init_shards(&mut g, 2), "InitShards");
    for (k, label) in [
        (w.a.beacon_log(7), "BeaconLog"),
        (w.a.join_shard(2, 5), "JoinShard"),
    ] {
        let mut d = g.data(&k);
        d[8] ^= 0x01;
        let mut f = c.fork();
        f.put_program_account(k, d);
        let r = if label == "BeaconLog" {
            w.init_logs(&mut f)
        } else {
            w.init_shards(&mut f, 2)
        };
        assert!(r.is_err(), "Init over a forged {label}");
        assert_code(r, E::BadAccount);
    }
}

#[test]
fn g03_announce_season_addresses_and_authority() {
    let mut c = Chain::release();
    let w = World::new(&mut c, 3);
    c.set_time(w.announce_time());
    // The Season PDA of another id (non-canonical for id 3).
    let other = permutation_frontier_svm_tests::Addresses::new(c.program, 4).season;
    let ix = with_account(w.announce_ix(), 1, other);
    assert_code(c.send(&[ix], &[&w.authority]), E::BadAddress);
    // A ProgramData that is not the program's canonical LoaderV3 address.
    let fake_pd = Address::new_from_array(sha256(&[b"fake programdata"]));
    let d = c.data(&c.programdata);
    let l = c.lamports(&c.programdata);
    c.put(
        fake_pd,
        permutation_frontier_svm_tests::chain::loader_v3(),
        d,
        l,
    );
    let ix = with_account(w.announce_ix(), 3, fake_pd);
    // The forged copy doubles the ProgramData the transaction loads: give
    // the loaded-data limit room for both, so the program decides and not
    // SIMD-0186 (integ-W3, W3-B finding F5: a release .so above ~418 KB
    // put two copies over the 1-MiB L(AnnounceSeason)).
    let p = c.profile_of(std::slice::from_ref(&ix), Profile::ladder);
    let p = p.with_loaded(2 * p.loaded_limit.unwrap_or(0));
    assert_refused(
        c.send_with(&p, &[ix], &[&w.authority]),
        "AnnounceSeason with a foreign ProgramData",
    );
    // A signer that is not the upgrade authority (I-51).
    let stranger = c.funded(b"stranger", 10);
    let ix = with_account(w.announce_ix(), 0, stranger.pubkey());
    assert_code(c.send(&[ix], &[&stranger]), E::Auth);
    // The real one lands.
    expect_lands(w.announce(&mut c), "AnnounceSeason");
}

#[test]
fn g03_create_season_targets_must_be_canonical() {
    let mut c = Chain::release();
    let w = World::announced(&mut c, 1);
    c.set_time(w.t_create_min);
    let n = w.create_ix().accounts.len();
    // Positions 2..n-1: Frontier, ProvinceFund × 6, DefencePool.
    for i in 2..n - 1 {
        let mut f = c.fork();
        let wrong = Address::new_from_array(sha256(&[b"non-canonical", &[i as u8]]));
        let ix = with_account(w.create_ix(), i, wrong);
        assert_code(f.send(&[ix], &[&w.authority]), E::BadAddress);
    }
    // Two funds swapped: each is canonical for another wedge.
    let mut ix = w.create_ix();
    ix.accounts.swap(3, 4);
    assert_code(c.send(&[ix], &[&w.authority]), E::BadAddress);
    // A signer that is not the season's authority.
    let stranger = c.funded(b"stranger", 100);
    let ix = with_account(w.create_ix(), 0, stranger.pubkey());
    assert_code(c.send(&[ix], &[&stranger]), E::Auth);
}

#[test]
fn g03_init_logs_and_shards_targets_must_be_canonical() {
    let (c, w) = {
        let mut c = Chain::release();
        let w = World::created(&mut c, 1);
        (c, w)
    };
    let logs =
        permutation_frontier_svm_tests::ix::season::init_beacon_logs(&w.a, w.authority.pubkey());
    let mut f = c.fork();
    // Region 5's position holds region 6's log (canonical, wrong place).
    let ix = with_account(logs.clone(), 2 + 5, w.a.beacon_log(6));
    assert_code(f.send(&[ix], &[&w.authority]), E::BadAddress);
    let mut f = c.fork();
    let ix = with_account(logs, 2 + 15, Address::new_from_array(sha256(&[b"x"])));
    assert_code(f.send(&[ix], &[&w.authority]), E::BadAddress);
    let shards =
        permutation_frontier_svm_tests::ix::season::init_shards(&w.a, w.authority.pubkey(), 2);
    let mut f = c.fork();
    let ix = with_account(shards.clone(), 2 + 3, w.a.join_shard(3, 3));
    assert_code(f.send(&[ix], &[&w.authority]), E::BadAddress);
    let mut f = c.fork();
    let ix = with_account(shards, 2, w.a.join_shard(2, 1));
    assert_code(f.send(&[ix], &[&w.authority]), E::BadAddress);
}

// ------------------------------------------------------------ BeaconLog

#[test]
fn g03_beacon_log_forged_in_post_beacon() {
    let (c, w) = common::release();
    let round = w.beacons.rounds()[5];
    // Non-canonical: region 9's log for region 8.
    let mut f = c.fork();
    let ix = with_account(w.beacon_ix(8, round), 2, w.a.beacon_log(9));
    assert_code(keeper_send(&mut f, &w, ix), E::BadAddress);
    // Absent at a non-canonical address.
    let mut f = c.fork();
    let ix = with_account(
        w.beacon_ix(8, round),
        2,
        Address::new_from_array(sha256(&[b"nowhere"])),
    );
    assert_code(keeper_send(&mut f, &w, ix), E::BadAddress);
    // Forgeries at the canonical address (key field: the region byte).
    each_forgery_is(E::BadAccount, &c, &w.a.beacon_log(8), BL::REGION, |f| {
        let ix = w.beacon_ix(8, round);
        keeper_send(f, &w, ix)
    });
    // The genuine one lands.
    let mut f = c.fork();
    expect_lands(w.post_beacon(&mut f, 8, round), "PostBeacon");
}

// ------------------------------------------------------------ BellAnchor, AnchorArchive (PostAnchor)

#[test]
fn g03_anchor_and_archive_forged_in_post_anchor() {
    let (mut c, w) = common::test_beacon();
    w.to_anchor_time(&mut c, 5);
    let bell = 5u32;
    let r = 11u8;
    // Anchor at a non-canonical address: another bell's canonical anchor.
    let mut f = c.fork();
    let ix = with_account(w.anchor_ix(bell, r), at::ANCHOR, w.a.anchor(bell + 1, r));
    assert_code(keeper_send(&mut f, &w, ix), E::BadAddress);
    // Archive at a non-canonical address: another day's, and a random one.
    for wrong in [
        w.a.archive(r, archive_part(bell) + 1),
        w.a.archive(r + 1, archive_part(bell)),
        Address::new_from_array(sha256(&[b"archive?"])),
    ] {
        let mut f = c.fork();
        let ix = with_account(w.anchor_ix(bell, r), at::ARCHIVE, wrong);
        assert_code(keeper_send(&mut f, &w, ix), E::BadAddress);
    }
    // A forged archive at the canonical address (key field: the day).
    let mut g = c.fork();
    w.craft_archive(&mut g, r, archive_part(bell), &[]);
    each_forgery_is(
        E::BadAccount,
        &g,
        &w.a.archive(r, archive_part(bell)),
        AA::PART,
        |f| {
            let ix = w.anchor_ix(bell, r);
            keeper_send(f, &w, ix)
        },
    );
    // A present anchor at the canonical address that is not THE anchor:
    // presence is authenticated by owner, magic, season and key fields
    // (§4.1), so a forgery is refused rather than taken for "present".
    let mut g = c.fork();
    expect_lands(w.post_anchor(&mut g, bell, r), "PostAnchor");
    each_forgery_is(E::BadAccount, &g, &w.a.anchor(bell, r), BA::BELL, |f| {
        let ix = w.anchor_ix(bell, r);
        keeper_send(f, &w, ix)
    });
    // The genuine path lands (with a present, untombstoned archive).
    let mut g = c.fork();
    w.craft_archive(&mut g, r, archive_part(bell), &[bell + 1]);
    expect_lands(
        w.post_anchor(&mut g, bell, r),
        "PostAnchor with an archive of other bells",
    );
}

// ------------------------------------------------------------ BellAnchor, AnchorArchive (PostAnchorMulti)

/// PostAnchorMulti's account positions for `k` regions: anchors at
/// `2..2+k`, archives at `2+k..2+2k` (§5.8).
fn multi_anchor_at(i: usize) -> usize {
    2 + i
}
fn multi_archive_at(k: usize, i: usize) -> usize {
    2 + k + i
}

#[test]
fn g03_anchor_and_archive_forged_in_post_anchor_multi() {
    // §13.2 G3 "a forged anchor/cache/archive in every instruction that
    // reads one": PostAnchorMulti reads k anchors and k archives, and skips
    // present anchors, so a forged "present" anchor must be refused rather
    // than skipped (integ-W2 review of W2-B).
    let (mut c, w) = common::test_beacon();
    let bell = 5u32;
    w.to_anchor_time(&mut c, bell);
    let regions = [2u8, 5, 11];
    let k = regions.len();
    let day = archive_part(bell);
    let arg = w.beacons.must(w.tlock_round(bell));
    let multi = |rs: &[u8]| {
        post_anchor_regions(&w.a, w.keeper.pubkey(), bell, &arg, rs, &w.keeper.pubkey())
    };
    // Non-canonical anchors: another bell's, another region's, a random one.
    for wrong in [
        w.a.anchor(bell + 1, 5),
        w.a.anchor(bell, 6),
        Address::new_from_array(sha256(&[b"anchor?"])),
    ] {
        let mut f = c.fork();
        let ix = with_account(multi(&regions), multi_anchor_at(1), wrong);
        assert_code(keeper_send(&mut f, &w, ix), E::BadAddress);
    }
    // Non-canonical archives: another day's, another region's, a random one.
    for wrong in [
        w.a.archive(5, day + 1),
        w.a.archive(6, day),
        Address::new_from_array(sha256(&[b"archive?"])),
    ] {
        let mut f = c.fork();
        let ix = with_account(multi(&regions), multi_archive_at(k, 1), wrong);
        assert_code(keeper_send(&mut f, &w, ix), E::BadAddress);
    }
    // Swapped positions: each account is canonical, for another region.
    let mut f = c.fork();
    let mut ix = multi(&regions);
    ix.accounts.swap(multi_anchor_at(0), multi_anchor_at(2));
    assert_code(keeper_send(&mut f, &w, ix), E::BadAddress);
    let mut f = c.fork();
    let mut ix = multi(&regions);
    ix.accounts
        .swap(multi_archive_at(k, 0), multi_archive_at(k, 1));
    assert_code(keeper_send(&mut f, &w, ix), E::BadAddress);
    // The mask names other regions than the accounts (region 12 for 11).
    let mut f = c.fork();
    let mut ix = multi(&[2, 5, 12]);
    ix.accounts = multi(&regions).accounts;
    assert_code(keeper_send(&mut f, &w, ix), E::BadAddress);
    // An empty mask.
    let mut f = c.fork();
    assert_code(keeper_send(&mut f, &w, multi(&[])), E::BadData);
    // A forged "present" anchor at a Multi position (key field: the bell),
    // with the other positions absent (so it would be skipped as present).
    let mut g = c.fork();
    expect_lands(w.post_anchor(&mut g, bell, 5), "PostAnchor");
    each_forgery_is(E::BadAccount, &g, &w.a.anchor(bell, 5), BA::BELL, |f| {
        keeper_send(f, &w, multi(&regions))
    });
    // …and with the region byte changed.
    each_forgery_is(E::BadAccount, &g, &w.a.anchor(bell, 5), BA::REGION, |f| {
        keeper_send(f, &w, multi(&regions))
    });
    // A forged archive at its canonical address (key field: the day).
    let mut g = c.fork();
    w.craft_archive(&mut g, 11, day, &[]);
    each_forgery_is(E::BadAccount, &g, &w.a.archive(11, day), AA::PART, |f| {
        keeper_send(f, &w, multi(&regions))
    });
    // …and a tombstoning one is `Archived` for its region.
    let mut g = c.fork();
    w.craft_archive(&mut g, 11, day, &[bell]);
    assert_code(keeper_send(&mut g, &w, multi(&regions)), E::Archived);
    // The genuine path: one present anchor skipped, the others created.
    let mut g = c.fork();
    expect_lands(w.post_anchor(&mut g, bell, 5), "PostAnchor");
    let before = g.data(&w.a.anchor(bell, 5));
    expect_lands(
        keeper_send(&mut g, &w, multi(&regions)),
        "PostAnchorMulti with one present anchor",
    );
    assert_eq!(
        g.data(&w.a.anchor(bell, 5)),
        before,
        "present anchor unchanged"
    );
    for r in [2u8, 11] {
        assert!(g.lamports(&w.a.anchor(bell, r)) > 0, "anchor {r} created");
    }
}

#[test]
fn g03_instructions_sysvar_must_be_the_sysvar() {
    let (mut c, w) = common::test_beacon();
    w.to_anchor_time(&mut c, 0);
    let fake = Address::new_from_array(sha256(&[b"not the instructions sysvar"]));
    let ix = with_account(w.anchor_ix(0, 0), at::IX_SYSVAR, fake);
    assert_refused(
        keeper_send(&mut c, &w, ix),
        "PostAnchor with a forged instructions sysvar",
    );
}

// ------------------------------------------------------------ BellAnchor, SeedCache (PostSeed)

#[test]
fn g03_anchor_and_cache_forged_in_post_seed() {
    let (mut c, w) = common::test_beacon();
    let (bell, r) = (2u32, 6u8);
    expect_lands(w.post_anchor(&mut c, bell, r), "PostAnchor");
    expect_lands(
        w.post_anchor(&mut c, bell + 1, r),
        "PostAnchor (another bell)",
    );
    let round = w.seed_round(&c, bell, r);
    let t = World::round_time(round);
    if c.now < t {
        c.set_time(t);
    }
    let seed_ix = |nonce| w.seed_ix(bell, r, nonce, round);
    // THE anchor at a non-canonical address: another bell's genuine anchor.
    let mut f = c.fork();
    let ix = with_account(seed_ix(0), seed_at::ANCHOR, w.a.anchor(bell + 1, r));
    assert_code(keeper_send(&mut f, &w, ix), E::BadAddress);
    // Absent THE anchor at a non-canonical address.
    let mut f = c.fork();
    let ix = with_account(
        seed_ix(0),
        seed_at::ANCHOR,
        Address::new_from_array(sha256(&[b"no anchor"])),
    );
    assert_code(keeper_send(&mut f, &w, ix), E::BadAddress);
    // Forgeries of THE anchor at its canonical address (key field: bell).
    each_forgery_is(E::BadAccount, &c, &w.a.anchor(bell, r), BA::BELL, |f| {
        keeper_send(f, &w, seed_ix(0))
    });
    // …and with the region byte changed.
    each_forgery_is(E::BadAccount, &c, &w.a.anchor(bell, r), BA::REGION, |f| {
        keeper_send(f, &w, seed_ix(0))
    });
    // The cache at a non-canonical address: another nonce's.
    let mut f = c.fork();
    let ix = with_account(seed_ix(0), seed_at::CACHE, w.a.seed_cache(bell, r, 1));
    assert_code(keeper_send(&mut f, &w, ix), E::BadAddress);
    // A present cache at the canonical address that is not a genuine one.
    let mut g = c.fork();
    expect_lands(keeper_send(&mut g, &w, seed_ix(0)), "PostSeed");
    each_forgery_is(
        E::BadAccount,
        &g,
        &w.a.seed_cache(bell, r, 0),
        SC::NONCE,
        |f| keeper_send(f, &w, seed_ix(0)),
    );
}

#[test]
fn g03_ruleset_of_the_season_must_match_the_binary() {
    // §5.6 step 1: a Running season whose ruleset hash differs from the
    // binary's is `RulesetMismatch` for every keeper write.
    let (mut c, w) = common::test_beacon();
    w.to_anchor_time(&mut c, 0);
    c.edit(&w.a.season, |d| d[S::RULESET_HASH] ^= 0xFF);
    let ix = w.anchor_ix(0, 3);
    assert_code(keeper_send(&mut c, &w, ix), E::RulesetMismatch);
}
