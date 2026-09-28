//! G13 completion (W5-A, §3.5, §13.3): the (instruction, error code) rows
//! the wave-2 to wave-4 tests left `Pending`. One test per instruction (or
//! a small group sharing a setup); each asserts the codes its registry row
//! names. Shape refusals use the same mutations for every instruction: one
//! account too many or too few (`TooManyAccounts`), a writable account
//! passed read-only or a builtin replaced (`BadAccount`), a signer that did
//! not sign (`Auth`), data one byte short (`BadData`).

mod common;

use frontier_abi::layout::player::{citizen as C, holding as H};
use frontier_abi::layout::world::season as S;
use frontier_abi::presets;
use frontier_abi::tags::Ix;
use permutation_frontier_svm_tests::chain::{
    assert_code, expect_lands, with_account, with_writable, without_signer, Chain, SendResult,
};
use permutation_frontier_svm_tests::ix::season::{abort_season, close_part, end_season};
use permutation_frontier_svm_tests::world::holding::Estate;
use permutation_frontier_svm_tests::world::World;
use permutation_frontier_svm_tests::{
    keypair, probe, sha256, AccountMeta, Address, FrontierError as E, Instruction, Keypair, Signer,
};

// ------------------------------------------------------------ helpers

/// `ix` with one more (read-only, unrelated) account at the end.
fn plus_one(mut ix: Instruction) -> Instruction {
    ix.accounts.push(AccountMeta::new_readonly(
        keypair(b"g13 extra").pubkey(),
        false,
    ));
    ix
}

/// `ix` with its last account dropped.
fn minus_one(mut ix: Instruction) -> Instruction {
    ix.accounts.pop();
    ix
}

/// `ix` with its data one byte short.
fn short_data(mut ix: Instruction) -> Instruction {
    ix.data.pop();
    ix
}

/// The probe deployed next to the Frontier program, for CPIs.
fn with_probe(c: &mut Chain) -> Address {
    let so = probe::load();
    let k = Address::new_from_array(sha256(&[b"cpi probe"]));
    c.deploy(k, &so, so.len(), None);
    k
}

fn send1(c: &mut Chain, ix: Instruction, k: &Keypair) -> SendResult {
    c.send(&[ix], &[k])
}

// ================================================================ season (§5.7)

/// AnnounceSeason: an account too many, the data one byte short, an
/// authority that cannot pay the Season's rent and bond (`Insufficient`),
/// and a bond so large that rent + bond overflows (`Overflow`).
#[test]
fn g13_announce_season_shape_funds_overflow() {
    let mut c = Chain::release();
    let mut w = World::new(&mut c, 1);
    c.set_time(w.announce_time());
    let ix = w.announce_ix();
    assert_code(
        send1(&mut c.fork(), plus_one(ix.clone()), &w.authority),
        E::TooManyAccounts,
    );
    assert_code(
        send1(&mut c.fork(), short_data(ix.clone()), &w.authority),
        E::BadData,
    );
    let mut poor = c.fork();
    let au = w.authority.pubkey();
    poor.edit_lamports(&au, 1_000_000);
    assert_code(send1(&mut poor, ix, &w.authority), E::Insufficient);
    w.bond = u64::MAX;
    assert_code(
        send1(&mut c.fork(), w.announce_ix(), &w.authority),
        E::Overflow,
    );
    w.bond = presets::MIN_CREATION_BOND;
    expect_lands(w.announce(&mut c), "AnnounceSeason");
}

/// CreateSeason: a target already present at its canonical address
/// (`BadAccount`: init refuses a non-absent target), an account too many.
#[test]
fn g13_create_season_present_target_and_shape() {
    let mut c = Chain::release();
    let w = World::announced(&mut c, 1);
    c.set_time(w.t_create_min);
    let ix = w.create_ix();
    assert_code(
        send1(&mut c.fork(), plus_one(ix.clone()), &w.authority),
        E::TooManyAccounts,
    );
    let mut f = c.fork();
    let fr = w.a.frontier();
    f.put(
        fr,
        keypair(b"g13 other program").pubkey(),
        vec![1; 8],
        10_000_000,
    );
    assert_code(send1(&mut f, ix.clone(), &w.authority), E::BadAccount);
    expect_lands(send1(&mut c, ix, &w.authority), "CreateSeason");
}

/// EndSeason: the Season passed read-only (`BadAccount`), a Season copy
/// at another address (`BadAddress`), an account too many.
#[test]
fn g13_end_season_accounts() {
    let mut c = Chain::release();
    let w = World::running(&mut c, 1);
    c.set_time(w.end_ts());
    let any = c.funded(b"g13-end", 1);
    let ix = end_season(&w.a, any.pubkey());
    assert_code(
        send1(&mut c.fork(), with_writable(ix.clone(), 1, false), &any),
        E::BadAccount,
    );
    let copy = common::copy_to_fresh(&mut c, &w.a.season, b"g13 season copy");
    assert_code(
        send1(&mut c.fork(), with_account(ix.clone(), 1, copy), &any),
        E::BadAddress,
    );
    assert_code(
        send1(&mut c.fork(), plus_one(ix.clone()), &any),
        E::TooManyAccounts,
    );
    expect_lands(send1(&mut c, ix, &any), "EndSeason");
}

/// CloseSeason on a Running season (`WrongStatus`); a Season copy at
/// another address (`BadAddress`); a part without its accounts
/// (`TooManyAccounts`).
#[test]
fn g13_close_season_status_and_accounts() {
    let mut c = Chain::release();
    let w = World::running(&mut c, 1);
    let au = w.authority.pubkey();
    let ix = close_part(&w.a, au, 0);
    assert_code(
        send1(&mut c.fork(), ix.clone(), &w.authority),
        E::WrongStatus,
    );
    c.set_time(w.end_ts());
    let any = c.funded(b"g13-end", 1);
    expect_lands(
        send1(&mut c, end_season(&w.a, any.pubkey()), &any),
        "EndSeason",
    );
    c.set_time(w.end_ts() + 72 * 3_600);
    let copy = common::copy_to_fresh(&mut c, &w.a.season, b"g13 season copy");
    assert_code(
        send1(
            &mut c.fork(),
            with_account(ix.clone(), 1, copy),
            &w.authority,
        ),
        E::BadAddress,
    );
    assert_code(
        send1(&mut c.fork(), minus_one(ix.clone()), &w.authority),
        E::TooManyAccounts,
    );
    expect_lands(send1(&mut c, ix, &w.authority), "CloseSeason part 0");
}

/// AbortSeason: the Season passed read-only (`BadAccount`), the
/// incinerator replaced (`BadAccount`), an account too many.
#[test]
fn g13_abort_season_accounts() {
    let mut c = Chain::release();
    let w = World::announced(&mut c, 1);
    let au = w.authority.pubkey();
    let ix = abort_season(&w.a, au, au);
    assert_code(
        send1(
            &mut c.fork(),
            with_writable(ix.clone(), 1, false),
            &w.authority,
        ),
        E::BadAccount,
    );
    let other = keypair(b"g13 not the incinerator").pubkey();
    assert_code(
        send1(
            &mut c.fork(),
            with_account(ix.clone(), 3, other),
            &w.authority,
        ),
        E::BadAccount,
    );
    assert_code(
        send1(&mut c.fork(), plus_one(ix.clone()), &w.authority),
        E::TooManyAccounts,
    );
    expect_lands(send1(&mut c, ix, &w.authority), "AbortSeason");
    assert_eq!(w.status(&c), S::STATUS_ABORTED);
}

// ================================================================ beacons (§5.8)

const REGION: u8 = 3;

/// PostAnchor: a region ≥ 16 and a bell ≥ `end_bell` (`BadData`), an
/// Aborted season (`WrongStatus`), and on the test-beacon build a round
/// the Clock has not reached (`TooEarly`).
#[test]
fn g13_post_anchor_data_status_early() {
    let (mut c, w) = common::test_beacon();
    let bell = 5u32;
    let region16 = {
        let mut ix = w.anchor_ix(bell, REGION);
        ix.data[1] = 16;
        ix
    };
    w.to_anchor_time(&mut c, bell);
    assert_code(send1(&mut c.fork(), region16, &w.keeper), E::BadData);
    let end = w.params.season.end_bell;
    let mut late = c.fork();
    w.to_anchor_time(&mut late, end);
    assert_code(
        send1(&mut late, w.anchor_ix(end, REGION), &w.keeper),
        E::BadData,
    );
    let mut ab = c.fork();
    w.craft_status(&mut ab, S::STATUS_ABORTED);
    assert_code(
        send1(&mut ab, w.anchor_ix(bell, REGION), &w.keeper),
        E::WrongStatus,
    );
    let mut early = c.fork();
    let t = World::round_time(w.tlock_round(bell + 1));
    early.set_time(t - 1);
    assert_code(
        send1(&mut early, w.anchor_ix(bell + 1, REGION), &w.keeper),
        E::TooEarly,
    );
    expect_lands(w.post_anchor(&mut c, bell, REGION), "PostAnchor");
}

/// PostAnchorMulti with a round other than `T(bell)`: `WrongRound`.
#[test]
fn g13_post_anchor_multi_wrong_round() {
    use permutation_frontier_svm_tests::ix::beacon as bix;
    let (mut c, w) = common::test_beacon();
    let bell = 6u32;
    let r = w.tlock_round(bell) + 1;
    c.set_time(World::round_time(r) + 1);
    let arg = w.beacons.must(r);
    let ix = bix::post_anchor_regions(
        &w.a,
        w.keeper.pubkey(),
        bell,
        &arg,
        &[1, 2],
        &w.keeper.pubkey(),
    );
    assert_code(send1(&mut c, ix, &w.keeper), E::WrongRound);
    let good = bix::post_anchor_regions(
        &w.a,
        w.keeper.pubkey(),
        bell,
        &w.beacons.must(w.tlock_round(bell)),
        &[1, 2],
        &w.keeper.pubkey(),
    );
    expect_lands(send1(&mut c, good, &w.keeper), "PostAnchorMulti");
}

/// An anchored and cached bell past `A + archive_after`.
fn archivable(bell: u32) -> (Chain, World) {
    let (mut c, w) = common::test_beacon();
    expect_lands(w.post_anchor(&mut c, bell, REGION), "PostAnchor");
    expect_lands(w.post_seed(&mut c, bell, REGION, 0), "PostSeed");
    let a = w.anchor_a(&c, bell, REGION).unwrap();
    c.set_time(a + w.params.season.archive_after as i64);
    (c, w)
}

/// ArchiveAnchors: an Aborted season (`WrongStatus`), a bell triple one
/// account short (`TooManyAccounts`), a payer position that did not sign
/// (`Auth`).
#[test]
fn g13_archive_anchors_status_shape_auth() {
    use permutation_frontier_svm_tests::ix::beacon::{archive_anchors, ArchiveItem};
    use permutation_frontier_svm_tests::world::archive_part;
    let bell = 10u32;
    let (mut c, w) = archivable(bell);
    let ix = archive_anchors(
        &w.a,
        w.keeper.pubkey(),
        REGION,
        archive_part(bell),
        &[ArchiveItem {
            bell,
            cache_nonce: 0,
            anchor_rent_to: w.keeper.pubkey(),
        }],
    );
    let mut ab = c.fork();
    w.craft_status(&mut ab, S::STATUS_ABORTED);
    assert_code(send1(&mut ab, ix.clone(), &w.keeper), E::WrongStatus);
    assert_code(
        send1(&mut c.fork(), minus_one(ix.clone()), &w.keeper),
        E::TooManyAccounts,
    );
    let outer = c.funded(b"g13-outer-payer", 1);
    let stranger = keypair(b"g13-unsigned").pubkey();
    let unsigned = without_signer(with_account(ix.clone(), 0, stranger), 0);
    assert_code(c.fork().send(&[unsigned], &[&outer]), E::Auth);
    expect_lands(send1(&mut c, ix, &w.keeper), "ArchiveAnchors");
}

/// CloseSeedCache: an Aborted season (`WrongStatus`), the cache passed
/// read-only (`BadAccount`), an account too many.
#[test]
fn g13_close_seed_cache_status_account_shape() {
    use permutation_frontier_svm_tests::ix::beacon::{
        archive_anchors, close_seed_cache, ArchiveItem,
    };
    use permutation_frontier_svm_tests::world::archive_part;
    let bell = 10u32;
    let (mut c, w) = archivable(bell);
    let keeper = w.keeper.pubkey();
    expect_lands(
        send1(
            &mut c,
            archive_anchors(
                &w.a,
                keeper,
                REGION,
                archive_part(bell),
                &[ArchiveItem {
                    bell,
                    cache_nonce: 0,
                    anchor_rent_to: keeper,
                }],
            ),
            &w.keeper,
        ),
        "ArchiveAnchors",
    );
    let any = c.funded(b"g13-any", 1);
    let close = close_seed_cache(&w.a, any.pubkey(), bell, REGION, 0, keeper);
    let mut ab = c.fork();
    w.craft_status(&mut ab, S::STATUS_ABORTED);
    assert_code(send1(&mut ab, close.clone(), &any), E::WrongStatus);
    assert_code(
        send1(&mut c.fork(), with_writable(close.clone(), 2, false), &any),
        E::BadAccount,
    );
    assert_code(
        send1(&mut c.fork(), plus_one(close.clone()), &any),
        E::TooManyAccounts,
    );
    expect_lands(send1(&mut c, close, &any), "CloseSeedCache");
}

// ================================================================ map (§5.9)

/// The fee payer at position 0 replaced by an unsigned key; the
/// transaction paid by `outer` (`Auth`: §5.6's signer rule).
fn unsigned_payer(ix: Instruction, label: &[u8]) -> Instruction {
    without_signer(with_account(ix, 0, keypair(label).pubkey()), 0)
}

/// A Season whose stored ruleset hash is not the binary's.
fn foreign_ruleset(c: &mut Chain, w: &World) {
    c.edit(&w.a.season, |d| d[S::RULESET_HASH] ^= 1);
}

/// OpenRing: a payer that did not sign (`Auth`), an account too many, a
/// Season of another ruleset (`RulesetMismatch`); `L(OpenRing)` on the
/// release binary.
#[test]
fn g13_open_ring_auth_shape_ruleset() {
    let (mut c, w) = common::release();
    let ix = w.open_ring_ix(0);
    let outer = c.funded(b"g13-outer", 1);
    assert_code(
        c.fork()
            .send(&[unsigned_payer(ix.clone(), b"g13-u")], &[&outer]),
        E::Auth,
    );
    assert_code(
        send1(&mut c.fork(), plus_one(ix.clone()), &w.keeper),
        E::TooManyAccounts,
    );
    let mut f = c.fork();
    foreign_ruleset(&mut f, &w);
    assert_code(send1(&mut f, ix.clone(), &w.keeper), E::RulesetMismatch);
    common::loaded_check(&c, Ix::OpenRing, std::slice::from_ref(&ix), &[&w.keeper]);
    expect_lands(send1(&mut c, ix, &w.keeper), "OpenRing");
}

/// A requested RingSeed `d` at its canonical address waiting for `round`.
fn requested_ring(c: &mut Chain, w: &World, d: u16, round: u64) {
    use frontier_abi::layout::world::ring_seed as RS;
    let mut data = vec![0u8; RS::SIZE];
    data[..8].copy_from_slice(&RS::MAGIC);
    data[8..16].copy_from_slice(&w.id.to_le_bytes());
    data[RS::D..RS::D + 2].copy_from_slice(&d.to_le_bytes());
    data[RS::STATUS] = RS::STATUS_REQUESTED;
    data[RS::ROUND..RS::ROUND + 8].copy_from_slice(&round.to_le_bytes());
    data[RS::PAYER..RS::PAYER + 32].copy_from_slice(w.keeper.pubkey().as_ref());
    c.put_program_account(w.a.ring_seed(d), data);
}

/// ConsumeRingSeed: through a CPI (`NotTopLevel`), a fee payer that did
/// not sign (`Auth`); G1 over the 32 real quicknet rounds on the release
/// binary (the worst hint path; CU ≤ 345k, heap, bytes, locks); `L(kind)`.
#[test]
fn g13_consume_ring_seed_top_level_auth_and_g01() {
    use permutation_frontier_svm_tests::budget::{assert_within, ceilings};
    use permutation_frontier_svm_tests::ix::map as mix;
    use permutation_frontier_svm_tests::probe as pr;
    let mut c = Chain::release();
    let probe_id = with_probe(&mut c);
    let w = World::running(&mut c, 1);
    let rounds = w.beacons.rounds();
    assert_eq!(rounds.len(), 32);
    let first = rounds[0];
    requested_ring(&mut c, &w, 4, first);
    c.set_time(c.now.max(World::round_time(first) + 1));
    let ix = mix::consume_ring_seed(&w.a, w.keeper.pubkey(), 4, &w.beacons.must(first));
    assert_code(
        send1(&mut c.fork(), pr::cpi(probe_id, &ix), &w.keeper),
        E::NotTopLevel,
    );
    let outer = c.funded(b"g13-outer", 1);
    assert_code(
        c.fork()
            .send(&[unsigned_payer(ix.clone(), b"g13-u")], &[&outer]),
        E::Auth,
    );
    common::loaded_check(&c, Ix::ConsumeRingSeed, &[ix], &[&w.keeper]);
    let (mut worst, mut at) = (0u64, 0u64);
    for r in rounds {
        let mut f = c.fork();
        requested_ring(&mut f, &w, 4, r);
        f.set_time(f.now.max(World::round_time(r) + 1));
        let ix = mix::consume_ring_seed(&w.a, w.keeper.pubkey(), 4, &w.beacons.must(r));
        let need = f
            .measure(&[ix], &[&w.keeper])
            .unwrap_or_else(|e| panic!("round {r}: {e:?}"));
        assert_within(
            &format!("ConsumeRingSeed round {r}"),
            &need,
            &ceilings(Ix::ConsumeRingSeed, 0, f.programdata_len()),
        );
        if need.cu > worst {
            (worst, at) = (need.cu, r);
        }
    }
    println!("ConsumeRingSeed over 32 quicknet rounds: worst {worst} CU (round {at})");
}

/// OpenProvince through a CPI (`NotTopLevel`); `L(OpenProvince)`.
#[test]
fn g13_open_province_top_level_and_loaded() {
    use permutation_frontier_svm_tests::probe as pr;
    let mut c = Chain::release();
    let probe_id = with_probe(&mut c);
    let w = World::running(&mut c, 1);
    w.open_genesis_rings(&mut c);
    let ix = w.open_province_ix(2, 0);
    assert_code(
        send1(&mut c.fork(), pr::cpi(probe_id, &ix), &w.keeper),
        E::NotTopLevel,
    );
    common::loaded_check(
        &c,
        Ix::OpenProvince,
        std::slice::from_ref(&ix),
        &[&w.keeper],
    );
    expect_lands(send1(&mut c, ix, &w.keeper), "OpenProvince");
}

/// FoldOccupancy: an account too many (`TooManyAccounts`); G1 and
/// `L(FoldOccupancy)` for each of the three parts (24 shards, 24 shards,
/// 6 funds) with the genesis rings' provinces open.
#[test]
fn g13_fold_occupancy_shape_and_g01_per_part() {
    use permutation_frontier_svm_tests::budget::{assert_within, ceilings};
    let (mut c, w) = common::release();
    w.open_genesis_rings(&mut c);
    w.open_provinces(&mut c, 2, None);
    assert_code(
        send1(&mut c.fork(), plus_one(w.fold_ix(0)), &w.keeper),
        E::TooManyAccounts,
    );
    for part in 0..3u8 {
        let ix = w.fold_ix(part);
        common::loaded_check(
            &c,
            Ix::FoldOccupancy,
            std::slice::from_ref(&ix),
            &[&w.keeper],
        );
        let need = c
            .measure(std::slice::from_ref(&ix), &[&w.keeper])
            .expect("fold part");
        assert_within(
            &format!("FoldOccupancy part {part}"),
            &need,
            &ceilings(Ix::FoldOccupancy, 0, c.programdata_len()),
        );
        expect_lands(send1(&mut c, ix, &w.keeper), "FoldOccupancy");
    }
}

/// CloseProvince: G1 and `L(CloseProvince)` (an Aborted season closes at
/// once).
#[test]
fn g13_close_province_g01() {
    use permutation_frontier_svm_tests::budget::{assert_within, ceilings};
    use permutation_frontier_svm_tests::ix::map as mix;
    let (mut c, w) = common::release();
    w.open_genesis_rings(&mut c);
    expect_lands(w.open_province(&mut c, 2, 0), "OpenProvince");
    w.craft_status(&mut c, S::STATUS_ABORTED);
    let ix = mix::close_province(&w.a, w.keeper.pubkey(), 2, 0);
    common::loaded_check(
        &c,
        Ix::CloseProvince,
        std::slice::from_ref(&ix),
        &[&w.keeper],
    );
    let need = c
        .measure(std::slice::from_ref(&ix), &[&w.keeper])
        .expect("close");
    assert_within(
        "CloseProvince",
        &need,
        &ceilings(Ix::CloseProvince, 0, c.programdata_len()),
    );
    expect_lands(send1(&mut c, ix, &w.keeper), "CloseProvince");
}

// ================================================================ player (§5.6, §5.10)

use permutation_frontier_svm_tests::ix::citizen::Player;
use permutation_frontier_svm_tests::ix::holding as hx;
use permutation_frontier_svm_tests::ix::host as hix;

const B0: u32 = 10;

/// A release chain in bell `B0` with an estate: a funded player with a
/// final first holding in a crafted province (2, 0).
fn estate() -> (Chain, World, Estate) {
    let mut c = Chain::release();
    let w = World::running(&mut c, 1);
    w.to_bell(&mut c, B0, 5);
    let e = w.craft_estate(&mut c, "g13", 0, (2, 0), 0);
    (c, w, e)
}

fn send_e(c: &mut Chain, e: &Estate, ix: Instruction) -> SendResult {
    c.send(&[ix], &[&e.wallet])
}

/// The seven player-prologue refusals (§5.6) of `build`, asserted in the
/// order of `codes`: before genesis, a Season of another ruleset, an
/// empty action bucket, an expired session key acting, a stranger acting,
/// the Citizen's bytes at another address, the last account dropped.
#[track_caller]
fn prologue_rows(
    c: &Chain,
    w: &World,
    e: &Estate,
    build: &dyn Fn(&Player) -> Instruction,
    codes: [E; 7],
) {
    let ix = build(&e.player());
    let mut f = c.fork();
    f.set_time(w.genesis_ts() - 1);
    assert_code(send_e(&mut f, e, ix.clone()), codes[0]);
    let mut f = c.fork();
    f.edit(&w.a.season, |d| d[S::RULESET_HASH] ^= 1);
    assert_code(send_e(&mut f, e, ix.clone()), codes[1]);
    let mut f = c.fork();
    let t = (c.now - w.genesis_ts()) as u32;
    f.edit(&e.citizen, |d| {
        d[C::BUCKET_MILLI..C::BUCKET_MILLI + 4].copy_from_slice(&0u32.to_le_bytes());
        d[C::BUCKET_T..C::BUCKET_T + 4].copy_from_slice(&t.to_le_bytes());
    });
    assert_code(send_e(&mut f, e, ix.clone()), codes[2]);
    let mut f = c.fork();
    let session = f.funded(b"g13-session", 1);
    f.edit(&e.citizen, |d| {
        d[C::SESSION..C::SESSION + 32].copy_from_slice(session.pubkey().as_ref());
        d[C::SESSION_EXPIRY..C::SESSION_EXPIRY + 8].copy_from_slice(&1i64.to_le_bytes());
    });
    let mut p = e.player();
    p.actor = session.pubkey();
    assert_code(f.send(&[build(&p)], &[&e.wallet, &session]), codes[3]);
    let mut f = c.fork();
    let stranger = f.funded(b"g13-stranger", 1);
    let mut p = e.player();
    p.actor = stranger.pubkey();
    assert_code(f.send(&[build(&p)], &[&e.wallet, &stranger]), codes[4]);
    let mut f = c.fork();
    let fake = common::copy_to_fresh(&mut f, &e.citizen, b"g13 citizen copy");
    assert_code(
        send_e(&mut f, e, with_account(ix.clone(), hx::at::CITIZEN, fake)),
        codes[5],
    );
    assert_code(send_e(&mut c.fork(), e, minus_one(ix)), codes[6]);
}

/// Harvest: the player-prologue rows; `L(Harvest)`.
#[test]
fn g13_harvest_prologue_and_loaded() {
    let (c, w, e) = estate();
    let build = |p: &Player| hx::harvest(&w.a, p, e.href());
    prologue_rows(
        &c,
        &w,
        &e,
        &build,
        [
            E::WrongStatus,
            E::RulesetMismatch,
            E::Bucket,
            E::SessionExpired,
            E::Auth,
            E::BadAddress,
            E::TooManyAccounts,
        ],
    );
    common::loaded_check(&c, Ix::Harvest, &[build(&e.player())], &[&e.wallet]);
}

/// Build: the player-prologue rows; `L(Build)`.
#[test]
fn g13_build_prologue_and_loaded() {
    let (mut c, w, e) = estate();
    w.enrich(&mut c, &e, 1_000_000);
    let build = |p: &Player| hx::build_item(&w.a, p, e.href(), 0, false);
    prologue_rows(
        &c,
        &w,
        &e,
        &build,
        [
            E::WrongStatus,
            E::RulesetMismatch,
            E::Bucket,
            E::SessionExpired,
            E::Auth,
            E::BadAddress,
            E::TooManyAccounts,
        ],
    );
    common::loaded_check(&c, Ix::Build, &[build(&e.player())], &[&e.wallet]);
}

/// Train: the player-prologue rows; `L(Train)`.
#[test]
fn g13_train_prologue_and_loaded() {
    let (mut c, w, e) = estate();
    w.enrich(&mut c, &e, 1_000_000);
    let build = |p: &Player| hx::train(&w.a, p, e.href(), 0, 300);
    prologue_rows(
        &c,
        &w,
        &e,
        &build,
        [
            E::WrongStatus,
            E::RulesetMismatch,
            E::Bucket,
            E::SessionExpired,
            E::Auth,
            E::BadAddress,
            E::TooManyAccounts,
        ],
    );
    common::loaded_check(&c, Ix::Train, &[build(&e.player())], &[&e.wallet]);
}

/// A provisional holding (I-29) whose finality is far away.
fn provisional(c: &mut Chain, e: &Estate) {
    c.edit(&e.holding, |d| {
        d[H::STATE] = H::STATE_PROVISIONAL;
        d[H::FINAL_TS..H::FINAL_TS + 8].copy_from_slice(&i64::MAX.to_le_bytes());
    });
}

/// Dissolve: a provisional holding (`NotFinal`), the player-prologue rows;
/// `L(Dissolve)`.
#[test]
fn g13_dissolve_not_final_prologue_and_loaded() {
    let (mut c, w, e) = estate();
    let id = w.craft_host(&mut c, &e, &e.province, 3, 0, 0, 700, e.tile);
    let build = |p: &Player| hix::dissolve(&w.a, p, e.href(), id);
    let mut f = c.fork();
    provisional(&mut f, &e);
    assert_code(send_e(&mut f, &e, build(&e.player())), E::NotFinal);
    prologue_rows(
        &c,
        &w,
        &e,
        &build,
        [
            E::WrongStatus,
            E::RulesetMismatch,
            E::Bucket,
            E::SessionExpired,
            E::Auth,
            E::BadAddress,
            E::TooManyAccounts,
        ],
    );
    common::loaded_check(&c, Ix::Dissolve, &[build(&e.player())], &[&e.wallet]);
}

/// Garrison: a provisional holding (`NotFinal`), a province two bells
/// behind (`NotResident`), both pending garrison changes taken by other
/// bells (`HostBusy`), the player-prologue rows; `L(Garrison)`.
#[test]
fn g13_garrison_final_resident_busy_prologue_and_loaded() {
    use frontier_abi::layout::province::{province as P, site as SM};
    let (mut c, w, e) = estate();
    w.set_reserve(&mut c, &e, 0, 2_000);
    let build = |p: &Player| hix::garrison(&w.a, p, e.href(), 100);
    let mut f = c.fork();
    provisional(&mut f, &e);
    assert_code(send_e(&mut f, &e, build(&e.player())), E::NotFinal);
    let mut f = c.fork();
    w.set_resolved_next(&mut f, &e.province, B0 - 2);
    assert_code(send_e(&mut f, &e, build(&e.player())), E::NotResident);
    let mut f = c.fork();
    let o = P::site(e.site as usize);
    f.edit(&e.province, |d| {
        for (bell_at, delta_at, b) in [
            (SM::PEND0_BELL, SM::PEND0_DELTA, B0 + 1),
            (SM::PEND1_BELL, SM::PEND1_DELTA, B0 + 2),
        ] {
            d[o + bell_at..o + bell_at + 4].copy_from_slice(&b.to_le_bytes());
            d[o + delta_at..o + delta_at + 8].copy_from_slice(&1_000i64.to_le_bytes());
        }
    });
    assert_code(send_e(&mut f, &e, build(&e.player())), E::HostBusy);
    prologue_rows(
        &c,
        &w,
        &e,
        &build,
        [
            E::WrongStatus,
            E::RulesetMismatch,
            E::Bucket,
            E::SessionExpired,
            E::Auth,
            E::BadAddress,
            E::TooManyAccounts,
        ],
    );
    common::loaded_check(&c, Ix::Garrison, &[build(&e.player())], &[&e.wallet]);
}

/// `L(Muster)` and `L(DisbandStranded)` on the release binary.
#[test]
fn g13_muster_and_disband_loaded() {
    let (mut c, w, e) = estate();
    w.set_reserve(&mut c, &e, 0, 500);
    let m = hix::muster(&w.a, &e.player(), e.href(), 0, 100, e.tile);
    common::loaded_check(&c, Ix::Muster, &[m], &[&e.wallet]);
    let id = w.craft_host(&mut c, &e, &e.province, 5, 0, 0, 300, e.tile);
    c.edit(&e.holding, |d| d[H::GEN] = 2);
    let any = c.funded(b"g13-any", 1);
    let d = hix::disband_stranded(&w.a, any.pubkey(), e.p, e.q, 5, id);
    common::loaded_check(&c, Ix::DisbandStranded, &[d], &[&any]);
}

/// A march of host `id` from the estate two provinces east, arriving at
/// `arrive` (the release binary seals to the quicknet key).
fn march_east(
    w: &World,
    c: &mut Chain,
    e: &Estate,
    id: u64,
    slot: u8,
    arrive: u32,
) -> permutation_frontier_svm_tests::world::holding::March {
    use permutation_frontier_svm_tests::fixtures::tlock::SealCase;
    let dirs = [0u8, 0];
    let origin = (e.p as i32, e.q as i32, e.tile);
    permutation_frontier_svm_tests::world::holding::open_path(w, c, origin, &dirs);
    w.plan_march(id, slot, origin, &dirs, arrive, 0, 0, SealCase::Valid)
}

/// `L(Depart)`; SettleDeparture on an Aborted season (`WrongStatus`) and
/// with the departed entry gone from the origin (`NotResident`);
/// `L(SettleDeparture)`.
#[test]
fn g13_depart_and_settle_departure() {
    use frontier_abi::entry::{write_entry, Entry};
    use permutation_frontier_svm_tests::world::holding::entry_of;
    let (mut c, w, e) = estate();
    let id = w.craft_host(&mut c, &e, &e.province, 0, 0, 0, 900, e.tile);
    let m = march_east(&w, &mut c, &e, id, 1, B0 + 6);
    let tip = w.tip_min(&c);
    let dep = w.depart_ix(&e, (e.p, e.q), &m, tip);
    common::loaded_check(&c, Ix::Depart, std::slice::from_ref(&dep), &[&e.wallet]);
    expect_lands(send_e(&mut c, &e, dep), "Depart");
    w.to_bell(&mut c, B0 + 2, 0);
    w.resolve_through(&mut c, &e.province, B0);
    let any = c.funded(b"g13-settler", 1);
    let settle = hix::settle_departure(&w.a, any.pubkey(), (e.p, e.q), e.href(), 1);
    let mut ab = c.fork();
    w.craft_status(&mut ab, S::STATUS_ABORTED);
    assert_code(send1(&mut ab, settle.clone(), &any), E::WrongStatus);
    let mut gone = c.fork();
    let i = entry_of(&gone.data(&e.province), id).expect("departed entry");
    gone.edit(&e.province, |d| write_entry(d, i, &Entry::FREE).unwrap());
    assert_code(send1(&mut gone, settle.clone(), &any), E::NotResident);
    common::loaded_check(
        &c,
        Ix::SettleDeparture,
        std::slice::from_ref(&settle),
        &[&any],
    );
    expect_lands(send1(&mut c, settle, &any), "SettleDeparture");
}

/// `L(Explore)`; SettleExplore on an Aborted season (`WrongStatus`), with
/// the Citizen or the seed cache at another address (`BadAddress`), and
/// through the archive while none exists (`SeedNotReady`);
/// `L(SettleExplore)` with a crafted seed (the release build verifies
/// real rounds only).
#[test]
fn g13_explore_and_settle_explore() {
    use permutation_frontier_svm_tests::world::World as Wd;
    use permutation_rules::frontier::beacon as kb;
    use permutation_rules::frontier::clash::QUICKNET;
    let (mut c, w, e) = estate();
    let tile = e.tile;
    let id = w.craft_host(&mut c, &e, &e.province, 0, 0, 6, 200, tile);
    let near: Vec<u8> = (0..61u8)
        .filter(|&t| {
            t != tile
                && permutation_rules::frontier::geometry::tile_offset(t)
                    .zip(permutation_rules::frontier::geometry::tile_offset(tile))
                    .is_some_and(|(a, b)| a.distance(b) == 1)
        })
        .take(2)
        .collect();
    let ex = hx::explore(&w.a, &e.player(), e.href(), (e.p, e.q), id, &near);
    common::loaded_check(&c, Ix::Explore, std::slice::from_ref(&ex), &[&e.wallet]);
    expect_lands(send_e(&mut c, &e, ex), "Explore");
    let region = Wd::region(e.p as i32, e.q as i32);
    let a = kb::bell_end(w.genesis_ts(), B0) + 30;
    w.craft_seed(&mut c, B0, region, a, [7u8; 32]);
    let close = kb::reveal_close(a, w.window(&c, B0));
    let round = kb::seed_round(&QUICKNET, close, w.params.season.seed_margin);
    c.set_time(c.now.max(Wd::round_time(round) + 1));
    let any = c.funded(b"g13-explore-settler", 1);
    let settle = |src| {
        hx::settle_explore(
            &w.a,
            any.pubkey(),
            e.href(),
            &e.wallet.pubkey(),
            B0,
            region,
            src,
        )
    };
    let ok = settle(hx::SeedSource::Cache { nonce: 0 });
    let mut ab = c.fork();
    w.craft_status(&mut ab, S::STATUS_ABORTED);
    assert_code(send1(&mut ab, ok.clone(), &any), E::WrongStatus);
    let fake = common::copy_to_fresh(&mut c, &e.citizen, b"g13 explore citizen");
    assert_code(
        send1(
            &mut c.fork(),
            with_account(ok.clone(), hx::settle_at::CITIZEN, fake),
            &any,
        ),
        E::BadAddress,
    );
    let cache_copy = common::copy_to_fresh(&mut c, &w.a.seed_cache(B0, region, 0), b"g13 cache");
    assert_code(
        send1(
            &mut c.fork(),
            with_account(ok.clone(), hx::settle_at::SEED, cache_copy),
            &any,
        ),
        E::BadAddress,
    );
    assert_code(
        send1(&mut c.fork(), settle(hx::SeedSource::Archive), &any),
        E::SeedNotReady,
    );
    common::loaded_check(&c, Ix::SettleExplore, std::slice::from_ref(&ok), &[&any]);
    expect_lands(send1(&mut c, ok, &any), "SettleExplore");
}
