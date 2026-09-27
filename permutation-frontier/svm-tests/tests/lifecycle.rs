//! EndSeason (0x04), AbortSeason (0x06) and CloseSeason (0x05), W4-B, on
//! the release build (no beacon needed past genesis): the end at
//! `end_bell`, the CL-24 bond rule (returned before the genesis round is
//! public, burned after), the season-end close in parts down to the 128-B
//! tombstone; G1 budgets, G13 codes. The only crafted state is a
//! ProvinceFund's `provinces_opened` (OpenProvince is W3-A's, and the test
//! needs the counter, not a province).

mod common;

use frontier_abi::layout::world::{
    beacon_log as BL, frontier as FR, join_shard as JS, province_fund as PF, season as S,
};
use frontier_abi::layout::AccountKind;
use frontier_abi::log::{bond_outcome, EntityKind, Kind};
use frontier_abi::tags::Ix;
use permutation_frontier_svm_tests::budget::{assert_within, ceilings};
use permutation_frontier_svm_tests::chain::{assert_code, expect_lands, with_account, Chain};
use permutation_frontier_svm_tests::ix::season::{abort_at, abort_season, close_part, end_season};
use permutation_frontier_svm_tests::records::{self, ChainWatch};
use permutation_frontier_svm_tests::world::World;
use permutation_frontier_svm_tests::{Address, FrontierError as E, Instruction, Keypair, Signer};

const INCINERATOR: Address = Address::new_from_array(frontier_abi::prologue::ids::INCINERATOR);

fn send(
    c: &mut Chain,
    ix: Instruction,
    k: &Keypair,
) -> permutation_frontier_svm_tests::chain::SendResult {
    c.send(&[ix], &[k])
}

/// A closed chained account's last record: the first `CLOSE` of `entity`
/// in `logs` continues the chain `before` (`final_seq`, `final_head` are the
/// chain before it; its tail link advances it once more).
#[track_caller]
fn check_close(logs: &[String], entity: EntityKind, before: (u64, [u8; 32])) {
    let r = records::of_kind(logs, Kind::CLOSE)
        .into_iter()
        .find(|r| r.link(entity).is_some())
        .expect("a CLOSE of the entity");
    assert_eq!(r.u64("final_seq"), before.0);
    assert_eq!(r.field("final_head", true), &before.1);
    let l = r.link(entity).unwrap();
    assert_eq!(l.seq, before.0 + 1);
    assert_eq!(
        l.head,
        frontier_abi::log::next_head(&before.1, l.seq, &r.body_without_tail)
    );
}

fn status_rec(l: &permutation_frontier_svm_tests::chain::Landed) -> (u8, u8, u8) {
    let r = records::one(&l.logs, Kind::SEASON_STATUS);
    (
        r.u64("old") as u8,
        r.u64("new") as u8,
        r.u64("bond_outcome") as u8,
    )
}

/// An ended season (EndSeason landed at `end_bell`).
fn ended(id: u64) -> (Chain, World) {
    let mut c = Chain::release();
    let w = World::running(&mut c, id);
    c.set_time(w.end_ts());
    let any = c.funded(b"w4b-end", 1);
    expect_lands(
        send(&mut c, end_season(&w.a, any.pubkey()), &any),
        "end_season(",
    );
    (c, w)
}

/// EndSeason: `TooEarly` before `end_bell`, then status Ended with
/// `SEASON_STATUS {3 → 4}`, a repeat `AlreadyDone`; not before genesis
/// (`WrongStatus`).
#[test]
fn g13_end_season() {
    let mut c = Chain::release();
    let w = World::seeded(&mut c, 1);
    let any = c.funded(b"w4b-any", 1);
    let ix = end_season(&w.a, any.pubkey());
    assert_code(send(&mut c, ix.clone(), &any), E::WrongStatus);
    c.set_time(w.genesis_ts());
    assert_code(send(&mut c, ix.clone(), &any), E::TooEarly);
    c.set_time(w.end_ts() - 1);
    assert_code(send(&mut c, ix.clone(), &any), E::TooEarly);
    c.set_time(w.end_ts());
    let sw = ChainWatch::new(&c, w.a.season, EntityKind::Season);
    let l = expect_lands(send(&mut c, ix.clone(), &any), "end_season(");
    assert_eq!(w.status(&c), S::STATUS_ENDED);
    assert_eq!(
        status_rec(&l),
        (S::STATUS_RUNNING, S::STATUS_ENDED, bond_outcome::NONE)
    );
    sw.check(&c, &l.logs, 1);
    assert_code(send(&mut c, ix, &any), E::AlreadyDone);
}

/// AbortSeason (CL-24): the authority aborts an Announced or Created season
/// and gets the bond back while the genesis round is not public; once it
/// is (Seeded, before `genesis_ts`) the bond is burned to the incinerator;
/// anyone aborts a season that missed its creation window (`TooEarly`
/// inside it); a running season is `WrongStatus`, a stranger on a Seeded
/// one `Auth`, a wrong authority account `BadAddress`.
#[test]
fn g13_abort_season_bond_rule() {
    // Announced: returned.
    let mut c = Chain::release();
    let w = World::announced(&mut c, 1);
    let au = w.authority.pubkey();
    let ix = abort_season(&w.a, au, au);
    let before = c.lamports(&au);
    let l = expect_lands(send(&mut c, ix, &w.authority), "abort_season(");
    assert_eq!(
        status_rec(&l),
        (
            S::STATUS_ANNOUNCED,
            S::STATUS_ABORTED,
            bond_outcome::RETURNED
        )
    );
    assert_eq!(c.lamports(&au) + l.fee, before + w.bond);
    assert_eq!(w.status(&c), S::STATUS_ABORTED);
    assert_eq!(w.season_u64(&c, S::CREATION_BOND), 0);
    // Created, before the genesis round: returned.
    let mut c = Chain::release();
    let w = World::created(&mut c, 1);
    assert!(c.now < World::round_time(w.genesis_round()));
    let l = expect_lands(
        send(&mut c, abort_season(&w.a, au, au), &w.authority),
        "abort_season(",
    );
    assert_eq!(status_rec(&l).2, bond_outcome::RETURNED);
    // Seeded (the genesis round public), before genesis_ts: burned.
    let mut c = Chain::release();
    let w = World::seeded(&mut c, 1);
    assert!(c.now < w.genesis_ts());
    let stranger = c.funded(b"w4b-stranger", 1);
    assert_code(
        send(&mut c, abort_season(&w.a, stranger.pubkey(), au), &stranger),
        E::Auth,
    );
    let wrong = abort_season(&w.a, au, stranger.pubkey());
    assert_code(send(&mut c, wrong, &w.authority), E::BadAddress);
    let i0 = c.lamports(&INCINERATOR);
    let l = expect_lands(
        send(&mut c, abort_season(&w.a, au, au), &w.authority),
        "abort_season(",
    );
    assert_eq!(
        status_rec(&l),
        (S::STATUS_SEEDED, S::STATUS_ABORTED, bond_outcome::BURNED)
    );
    assert_eq!(c.lamports(&INCINERATOR), i0 + w.bond);
    // Running: WrongStatus.
    let mut c = Chain::release();
    let w = World::running(&mut c, 1);
    assert_code(
        send(&mut c, abort_season(&w.a, au, au), &w.authority),
        E::WrongStatus,
    );
    // Anyone, after the creation window of an Announced season: burned.
    let mut c = Chain::release();
    let w = World::announced(&mut c, 1);
    let stranger = c.funded(b"w4b-stranger", 1);
    let ix = abort_season(&w.a, stranger.pubkey(), au);
    assert_code(send(&mut c, ix.clone(), &stranger), E::TooEarly);
    c.set_time(w.t_create_min + frontier_abi::presets::CREATE_WINDOW_SECS);
    let l = expect_lands(send(&mut c, ix, &stranger), "abort_season(");
    assert_eq!(status_rec(&l).2, bond_outcome::BURNED);
    let _ = abort_at::AUTHORITY;
}

/// CloseSeason in parts (W4-B's split): the shards of each faction, the
/// beacon logs, then the final part (Frontier, funds, pool, the Season
/// shrunk to its 128-B tombstone, status Closed, every lamport above its
/// rent to the authority). `TooEarly` before `end + 72 h` and while a fund
/// counts open provinces; `Auth` for another signer; `BadData` for a part
/// above 7; the final part again `AlreadyDone`; a shard part still runs on
/// the tombstone.
#[test]
fn g13_close_season_in_parts() {
    let (mut c, w) = ended(1);
    let au = w.authority.pubkey();
    let fin = close_part(&w.a, au, 7);
    assert_code(send(&mut c, fin.clone(), &w.authority), E::TooEarly);
    c.set_time(w.end_ts() + 72 * 3_600);
    let stranger = c.funded(b"w4b-close-stranger", 1);
    let theirs = close_part(&w.a, stranger.pubkey(), 0);
    assert_code(send(&mut c, theirs, &stranger), E::Auth);
    let bad = {
        let mut ix = close_part(&w.a, au, 7);
        ix.data[1] = 8;
        ix
    };
    assert_code(send(&mut c, bad, &w.authority), E::BadData);
    // Part 0: faction 0's eight JoinShards.
    let shard = w.a.join_shard(0, 0);
    let jw = records::head_of(&c, &shard);
    let a0 = c.lamports(&au);
    let rent_js = c.lamports(&shard);
    let l = expect_lands(
        send(&mut c, close_part(&w.a, au, 0), &w.authority),
        "close_part(",
    );
    for s in 0..8 {
        assert!(c.is_absent(&w.a.join_shard(0, s)));
    }
    assert_eq!(c.lamports(&au) + l.fee, a0 + 8 * rent_js);
    let closes = records::of_kind(&l.logs, Kind::CLOSE);
    assert_eq!(closes.len(), 8);
    assert!(closes
        .iter()
        .all(|r| r.key[0] == AccountKind::JoinShard as u8));
    check_close(&l.logs, EntityKind::JoinShard, jw);
    // Part 6: the 16 BeaconLogs.
    let l = expect_lands(
        send(&mut c, close_part(&w.a, au, 6), &w.authority),
        "close_part(",
    );
    assert_eq!(records::of_kind(&l.logs, Kind::CLOSE).len(), 16);
    assert!(c.is_absent(&w.a.beacon_log(15)));
    let _ = BL::SIZE;
    // A fund still counts an open province: TooEarly.
    let fund = w.a.province_funds()[2];
    let mut f = c.fork();
    f.edit(&fund, |d| {
        d[PF::PROVINCES_OPENED..PF::PROVINCES_OPENED + 4].copy_from_slice(&1u32.to_le_bytes())
    });
    assert_code(send(&mut f, fin.clone(), &w.authority), E::TooEarly);
    // The final part.
    let sw = ChainWatch::new(&c, w.a.season, EntityKind::Season);
    let fw = records::head_of(&c, &w.a.frontier());
    let a0 = c.lamports(&au);
    let released: u64 = [w.a.frontier(), w.a.defence_pool()]
        .iter()
        .chain(w.a.province_funds().iter())
        .map(|k| c.lamports(k))
        .sum::<u64>()
        + c.lamports(&w.a.season)
        - c.rent(S::TOMBSTONE_SIZE);
    let l = expect_lands(send(&mut c, fin.clone(), &w.authority), "close_part(");
    let d = c.data(&w.a.season);
    assert_eq!(d.len(), S::TOMBSTONE_SIZE);
    assert_eq!(d[S::STATUS], S::STATUS_CLOSED);
    assert_eq!(c.lamports(&w.a.season), c.rent(S::TOMBSTONE_SIZE));
    assert_eq!(c.lamports(&au) + l.fee, a0 + released);
    for k in [w.a.frontier(), w.a.defence_pool()] {
        assert!(c.is_absent(&k));
    }
    assert!(w.a.province_funds().iter().all(|k| c.is_absent(k)));
    assert_eq!(
        status_rec(&l),
        (S::STATUS_ENDED, S::STATUS_CLOSED, bond_outcome::NONE)
    );
    sw.check(&c, &l.logs, 1);
    check_close(&l.logs, EntityKind::Frontier, fw);
    let _ = FR::SIZE;
    assert_code(send(&mut c, fin, &w.authority), E::AlreadyDone);
    // Faction 1's shards on the tombstone.
    expect_lands(
        send(&mut c, close_part(&w.a, au, 1), &w.authority),
        "close_part(",
    );
    assert!(c.is_absent(&w.a.join_shard(1, 7)));
    let _ = JS::SIZE;
    // The used id stays used: a new AnnounceSeason is refused.
    let w2 = World::new(&mut c, 1);
    assert_code(w2.announce(&mut c), E::Announce);
}

/// An aborted season closes at once (no 72-h grace).
#[test]
fn close_season_after_abort() {
    let mut c = Chain::release();
    let w = World::created(&mut c, 1);
    let au = w.authority.pubkey();
    expect_lands(
        send(&mut c, abort_season(&w.a, au, au), &w.authority),
        "abort_season(",
    );
    expect_lands(
        send(&mut c, close_part(&w.a, au, 7), &w.authority),
        "close_part(",
    );
    assert_eq!(c.data(&w.a.season).len(), S::TOMBSTONE_SIZE);
    let _ = with_account;
}

/// G1 (§13.1): EndSeason, AbortSeason, CloseSeason (every part).
#[test]
fn g01_budget_w4b_lifecycle() {
    let (mut c, w) = ended(1);
    let au = w.authority.pubkey();
    c.set_time(w.end_ts() + 72 * 3_600);
    for part in [0u8, 6, 7] {
        let need = c
            .measure(&[close_part(&w.a, au, part)], &[&w.authority])
            .expect("measures");
        assert_within(
            &format!("CloseSeason part {part}"),
            &need,
            &ceilings(Ix::CloseSeason, 0, c.programdata_len()),
        );
        expect_lands(
            send(&mut c, close_part(&w.a, au, part), &w.authority),
            "close_part(",
        );
    }
    let mut c = Chain::release();
    let w = World::running(&mut c, 1);
    c.set_time(w.end_ts());
    let any = c.funded(b"w4b-g1", 1);
    let need = c
        .measure(&[end_season(&w.a, any.pubkey())], &[&any])
        .expect("measures");
    assert_within(
        "EndSeason",
        &need,
        &ceilings(Ix::EndSeason, 0, c.programdata_len()),
    );
    let mut c = Chain::release();
    let w = World::seeded(&mut c, 1);
    let au = w.authority.pubkey();
    let need = c
        .measure(&[abort_season(&w.a, au, au)], &[&w.authority])
        .expect("measures");
    assert_within(
        "AbortSeason (burn)",
        &need,
        &ceilings(Ix::AbortSeason, 0, c.programdata_len()),
    );
}

/// `g01_loaded_limit_*` (I-45) for EndSeason, AbortSeason and CloseSeason
/// (the 16-log part, the largest list).
#[test]
fn g01_loaded_limit_w4b_lifecycle() {
    use permutation_frontier_svm_tests::world::transit::loaded_check;
    let mut c = Chain::release();
    let w = World::running(&mut c, 1);
    c.set_time(w.end_ts());
    let any = c.funded(b"w4b-loaded", 1);
    loaded_check(
        &c,
        Ix::EndSeason,
        &[end_season(&w.a, any.pubkey())],
        &[&any],
    );
    expect_lands(
        send(&mut c, end_season(&w.a, any.pubkey()), &any),
        "end_season(",
    );
    c.set_time(w.end_ts() + 72 * 3_600);
    let au = w.authority.pubkey();
    loaded_check(
        &c,
        Ix::CloseSeason,
        &[close_part(&w.a, au, 6)],
        &[&w.authority],
    );
    let mut c = Chain::release();
    let w = World::seeded(&mut c, 1);
    let au = w.authority.pubkey();
    loaded_check(
        &c,
        Ix::AbortSeason,
        &[abort_season(&w.a, au, au)],
        &[&w.authority],
    );
}
