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
use permutation_frontier_svm_tests::ix::season::{
    abort_at, abort_season, close_float, close_part, end_season,
};
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
        ix.data[1] = 11;
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

/// v1.7 (W4-B F3, wave-4 review): a Holding and a Citizen left open when
/// CloseSeason's final part ran still close on the 128-B tombstone.
/// CloseHolding sends `pool_owed` to the Season's authority (position 4:
/// the DefencePool is gone and its lamports went to the authority;
/// anything else `BadAddress`) and the rest to the rent payer;
/// CloseCitizen closes to its rent payer. Before v1.7 both were refused
/// for ever (the tombstone is not a Season), locking the players' rent.
#[test]
fn close_holding_and_citizen_on_the_tombstone() {
    use frontier_abi::layout::player::holding as H;
    let (mut c, w) = ended(1);
    let e = w.craft_estate(&mut c, "late", 0, (2, 0), 0);
    let owed = 5_000u64;
    c.edit(&e.holding, |d| {
        d[H::POOL_OWED..H::POOL_OWED + 8].copy_from_slice(&owed.to_le_bytes())
    });
    let au = w.authority.pubkey();
    c.set_time(w.end_ts() + 72 * 3_600);
    for part in 0..=7u8 {
        expect_lands(
            send(&mut c, close_part(&w.a, au, part), &w.authority),
            "close_part(",
        );
    }
    assert_eq!(c.data(&w.a.season).len(), S::TOMBSTONE_SIZE);
    let rent_payer = Address::new_from_array(
        c.data(&e.holding)[H::RENT_PAYER..H::RENT_PAYER + 32]
            .try_into()
            .unwrap(),
    );
    let any = c.funded(b"w4r-tomb", 1);
    let ch = fclient::ix::close_holding(&w.a, any.pubkey(), e.href(), rent_payer);
    // The DefencePool's address (now absent) is not the authority.
    assert_code(send(&mut c, ch.clone(), &any), E::BadAddress);
    let (a0, r0, h0) = (
        c.lamports(&au),
        c.lamports(&rent_payer),
        c.lamports(&e.holding),
    );
    let l = expect_lands(
        send(&mut c, with_account(ch, 4, au), &any),
        "CloseHolding on the tombstone",
    );
    assert!(c.is_absent(&e.holding));
    assert_eq!(c.lamports(&au), a0 + owed, "pool_owed to the authority");
    assert_eq!(c.lamports(&rent_payer), r0 + h0 - owed);
    assert_eq!(records::one(&l.logs, Kind::POOL_SWEEP).u64("amount"), owed);
    let cz = fclient::ix::close_citizen(&w.a, any.pubkey(), &e.wallet.pubkey(), rent_payer, None);
    let cz = {
        // the Citizen's stored rent payer
        let d = c.data(&e.citizen);
        let rp = frontier_abi::layout::player::citizen::RENT_PAYER;
        let payer = Address::new_from_array(d[rp..rp + 32].try_into().unwrap());
        if payer == rent_payer {
            cz
        } else {
            fclient::ix::close_citizen(&w.a, any.pubkey(), &e.wallet.pubkey(), payer, None)
        }
    };
    expect_lands(send(&mut c, cz, &any), "CloseCitizen on the tombstone");
    assert!(c.is_absent(&e.citizen));
}

// ================================================================ v1.8 (W5-A): the season's float

/// A short-header account of `kind` for season 1 at `k` with `fields`
/// written at their offsets (the state OpenRing, ArchiveAnchors and
/// ClaimDefence leave; only the fields CloseSeason reads matter).
fn craft_short(c: &mut Chain, k: Address, magic: [u8; 8], size: usize, fields: &[(usize, &[u8])]) {
    let mut d = vec![0u8; size];
    d[..8].copy_from_slice(&magic);
    d[8..16].copy_from_slice(&1u64.to_le_bytes());
    for (o, b) in fields {
        d[*o..*o + b.len()].copy_from_slice(b);
    }
    c.put_program_account(k, d);
}

/// CloseSeason parts 8–10 (v1.8, W4-B F3): RingSeeds close to their payer
/// from `end + 72 h`; AnchorArchives and DefenceClaims to their `rent_to`
/// and beneficiary only once no reader can run (the tombstone, or
/// Aborted): `TooEarly` before. Each pair's recipient must be the stored
/// one (`BadAccount`), the target at its canonical address (`BadAddress`);
/// an odd or empty pair list is `TooManyAccounts`, a part above 10
/// `BadData`; absent targets are skipped (a repeat lands). Every close
/// logs `CLOSE` with its recipient; the rent goes there.
#[test]
fn g13_close_season_float_parts() {
    use frontier_abi::layout::beacon::{anchor_archive as AA, defence_claim as DCL};
    use frontier_abi::layout::world::ring_seed as RS;
    let (mut c, w) = ended(1);
    let au = w.authority.pubkey();
    let payer = c.funded(b"w5a-ring-payer", 1);
    let keeper = c.funded(b"w5a-claim-keeper", 1);
    let rings: Vec<Address> = [3u16, 4]
        .iter()
        .map(|&d| {
            let k = w.a.ring_seed(d);
            craft_short(
                &mut c,
                k,
                RS::MAGIC,
                RS::SIZE,
                &[
                    (RS::D, &d.to_le_bytes()),
                    (RS::PAYER, payer.pubkey().as_ref()),
                ],
            );
            k
        })
        .collect();
    let archives: Vec<Address> = [(0u8, 0u32), (5, 1)]
        .iter()
        .map(|&(r, part)| w.craft_archive(&mut c, r, part, &[part * 72]))
        .collect();
    let claims: Vec<Address> = [2u32, 3]
        .iter()
        .map(|&day| {
            let k = w.a.defence_claim(&keeper.pubkey(), day);
            craft_short(
                &mut c,
                k,
                DCL::MAGIC,
                DCL::SIZE,
                &[
                    (DCL::BENEFICIARY, keeper.pubkey().as_ref()),
                    (DCL::DAY, &day.to_le_bytes()),
                ],
            );
            k
        })
        .collect();
    let ring_pairs: Vec<(Address, Address)> = rings.iter().map(|k| (*k, payer.pubkey())).collect();
    let arch_pairs: Vec<(Address, Address)> =
        archives.iter().map(|k| (*k, w.keeper.pubkey())).collect();
    let claim_pairs: Vec<(Address, Address)> =
        claims.iter().map(|k| (*k, keeper.pubkey())).collect();
    // Before end + 72 h every part is TooEarly.
    assert_code(
        send(&mut c, close_float(&w.a, au, 8, &ring_pairs), &w.authority),
        E::TooEarly,
    );
    c.set_time(w.end_ts() + 72 * 3_600);
    // Archives and claims wait for the tombstone.
    assert_code(
        send(&mut c, close_float(&w.a, au, 9, &arch_pairs), &w.authority),
        E::TooEarly,
    );
    assert_code(
        send(
            &mut c,
            close_float(&w.a, au, 10, &claim_pairs),
            &w.authority,
        ),
        E::TooEarly,
    );
    // Shape and key refusals.
    let mut odd = close_float(&w.a, au, 8, &ring_pairs);
    odd.accounts.pop();
    assert_code(send(&mut c, odd, &w.authority), E::TooManyAccounts);
    assert_code(
        send(&mut c, close_float(&w.a, au, 8, &[]), &w.authority),
        E::TooManyAccounts,
    );
    let mut eleven = close_float(&w.a, au, 8, &ring_pairs);
    eleven.data[1] = 11;
    assert_code(send(&mut c, eleven, &w.authority), E::BadData);
    let wrong_to = close_float(&w.a, au, 8, &[(rings[0], au)]);
    assert_code(send(&mut c, wrong_to, &w.authority), E::BadAccount);
    let moved = common::copy_to_fresh(&mut c, &rings[0], b"w5a-moved-ring");
    let off = close_float(&w.a, au, 8, &[(moved, payer.pubkey())]);
    assert_code(send(&mut c, off, &w.authority), E::BadAddress);
    let not_a_ring = close_float(&w.a, au, 8, &[(archives[0], w.keeper.pubkey())]);
    assert_code(send(&mut c, not_a_ring, &w.authority), E::BadAccount);
    let stranger = c.funded(b"w5a-float-stranger", 1);
    let theirs = close_float(&w.a, stranger.pubkey(), 8, &ring_pairs);
    assert_code(send(&mut c, theirs, &stranger), E::Auth);
    // Part 8: both RingSeeds to their payer.
    let (p0, rent_rs) = (c.lamports(&payer.pubkey()), c.lamports(&rings[0]));
    let l = expect_lands(
        send(&mut c, close_float(&w.a, au, 8, &ring_pairs), &w.authority),
        "close_float(8",
    );
    assert!(rings.iter().all(|k| c.is_absent(k)));
    assert_eq!(c.lamports(&payer.pubkey()), p0 + 2 * rent_rs);
    let closes = records::of_kind(&l.logs, Kind::CLOSE);
    assert_eq!(closes.len(), 2);
    assert!(closes
        .iter()
        .all(|r| r.key[0] == AccountKind::RingSeed as u8));
    assert_eq!(closes[0].field("recipient", true), payer.pubkey().as_ref());
    let budget = ceilings(Ix::CloseSeason, 0, c.programdata_len());
    let need = c.measure(&[close_float(&w.a, au, 8, &ring_pairs)], &[&w.authority]);
    assert_within(
        "CloseSeason part 8 (repeat)",
        &need.expect("repeat lands"),
        &budget,
    );
    expect_lands(
        send(&mut c, close_float(&w.a, au, 8, &ring_pairs), &w.authority),
        "close_float(8 repeat",
    );
    // The final part, then archives and claims on the tombstone.
    expect_lands(send(&mut c, close_part(&w.a, au, 7), &w.authority), "final");
    let (k0, rent_aa) = (c.lamports(&w.keeper.pubkey()), c.lamports(&archives[0]));
    let l = expect_lands(
        send(&mut c, close_float(&w.a, au, 9, &arch_pairs), &w.authority),
        "close_float(9",
    );
    assert!(archives.iter().all(|k| c.is_absent(k)));
    assert_eq!(c.lamports(&w.keeper.pubkey()), k0 + 2 * rent_aa);
    assert!(records::of_kind(&l.logs, Kind::CLOSE)
        .iter()
        .all(|r| r.key[0] == AccountKind::AnchorArchive as u8));
    let _ = AA::SIZE;
    let (b0, rent_dc) = (c.lamports(&keeper.pubkey()), c.lamports(&claims[0]));
    expect_lands(
        send(
            &mut c,
            close_float(&w.a, au, 10, &claim_pairs),
            &w.authority,
        ),
        "close_float(10",
    );
    assert!(claims.iter().all(|k| c.is_absent(k)));
    assert_eq!(c.lamports(&keeper.pubkey()), b0 + 2 * rent_dc);
}

/// Parts 9 and 10 on an Aborted season (no reader of archives or claims
/// runs once Aborted): they land without the tombstone.
#[test]
fn g13_close_season_float_on_an_aborted_season() {
    let mut c = Chain::release();
    let w = World::announced(&mut c, 1);
    let au = w.authority.pubkey();
    expect_lands(
        send(&mut c, abort_season(&w.a, au, au), &w.authority),
        "abort_season(",
    );
    let arch = w.craft_archive(&mut c, 2, 0, &[1]);
    let l = expect_lands(
        send(
            &mut c,
            close_float(&w.a, au, 9, &[(arch, w.keeper.pubkey())]),
            &w.authority,
        ),
        "close_float(9 aborted",
    );
    assert!(c.is_absent(&arch));
    assert_eq!(records::of_kind(&l.logs, Kind::CLOSE).len(), 1);
}

/// G1 of the float parts at their maximal account lists (v1.8 wave-5
/// review): parts 8 and 10 at `CLOSE_FLOAT_PAIRS_MAX` = 10 pairs and part 9
/// at `CLOSE_ARCHIVE_PAIRS_MAX` = 2, each pair with its own recipient (the
/// most bytes), sent with the client profile — the budgets table's CU
/// limit and `L(CloseSeason)` — and asserted against CloseSeason's gate
/// (60k CU, 1,232 B, locks, `L`); one pair more is `TooManyAccounts`. The
/// part-9 cap is the most archive pairs whose loaded data stays within the
/// worst set `L(CloseSeason)` is computed from (checked here against the
/// unrounded need, so it holds at any programdata length).
#[test]
fn g01_budget_close_season_float_parts() {
    use frontier_abi::budgets::{self as ab, close_float_pairs_max};
    use frontier_abi::layout::beacon::defence_claim as DCL;
    use frontier_abi::layout::world::ring_seed as RS;
    let (mut c, w) = ended(1);
    let au = w.authority.pubkey();
    c.set_time(w.end_ts() + 72 * 3_600);
    expect_lands(send(&mut c, close_part(&w.a, au, 7), &w.authority), "final");
    assert_eq!(close_float_pairs_max(8), 10);
    assert_eq!(close_float_pairs_max(9), 2);
    assert_eq!(close_float_pairs_max(10), 10);
    let rings: Vec<(Address, Address)> = (1u16..=11)
        .map(|d| {
            let payer = c
                .funded(format!("w5a-ring-payer-{d}").as_bytes(), 1)
                .pubkey();
            let k = w.a.ring_seed(d);
            craft_short(
                &mut c,
                k,
                RS::MAGIC,
                RS::SIZE,
                &[(RS::D, &d.to_le_bytes()), (RS::PAYER, payer.as_ref())],
            );
            (k, payer)
        })
        .collect();
    let archives: Vec<(Address, Address)> = (0u8..3)
        .map(|r| {
            let to = c
                .funded(format!("w5a-archive-to-{r}").as_bytes(), 1)
                .pubkey();
            let k = w.craft_archive(&mut c, r, 3, &[216]);
            c.edit(&k, |d| {
                let o = frontier_abi::layout::beacon::anchor_archive::RENT_TO;
                d[o..o + 32].copy_from_slice(to.as_ref())
            });
            (k, to)
        })
        .collect();
    let claims: Vec<(Address, Address)> = (0u32..11)
        .map(|day| {
            let b = c
                .funded(format!("w5a-claimer-{day}").as_bytes(), 1)
                .pubkey();
            let k = w.a.defence_claim(&b, day);
            craft_short(
                &mut c,
                k,
                DCL::MAGIC,
                DCL::SIZE,
                &[
                    (DCL::BENEFICIARY, b.as_ref()),
                    (DCL::DAY, &day.to_le_bytes()),
                ],
            );
            (k, b)
        })
        .collect();
    let pd = c.programdata_len();
    let l_kind = permutation_frontier_svm_tests::chain::loaded_limit(Ix::CloseSeason, pd);
    for (part, pairs) in [(8u8, &rings), (9, &archives), (10, &claims)] {
        let n = close_float_pairs_max(part);
        // One pair more is refused before any work.
        assert_code(
            send(
                &mut c,
                close_float(&w.a, au, part, &pairs[..n + 1]),
                &w.authority,
            ),
            E::TooManyAccounts,
        );
        let ix = close_float(&w.a, au, part, &pairs[..n]);
        let label = format!("CloseSeason part {part}, {n} pairs (the cap)");
        let need = c
            .measure(std::slice::from_ref(&ix), &[&w.authority])
            .unwrap_or_else(|f| panic!("{label}: {f:?}"));
        assert_within(&label, &need, &ceilings(Ix::CloseSeason, 0, pd));
        assert!(
            need.cu <= ab::budget(Ix::CloseSeason).cu_limit as u64,
            "{label}: within the table's CU limit"
        );
        // The unrounded worst-set need `L(CloseSeason)` is computed from.
        assert!(
            need.loaded <= ab::loaded_need(Ix::CloseSeason, pd),
            "{label}: loaded {} B > the worst set's {} B",
            need.loaded,
            ab::loaded_need(Ix::CloseSeason, pd)
        );
        assert!(need.loaded <= l_kind as u64);
        // Sent with the client profile (table CU limit, L(kind)): lands.
        let mut f = c.fork();
        let l = expect_lands(
            f.send_client(std::slice::from_ref(&ix), &[&w.authority]),
            &label,
        );
        assert_eq!(records::of_kind(&l.logs, Kind::CLOSE).len(), n, "{label}");
        assert!(pairs[..n].iter().all(|(k, _)| f.is_absent(k)));
    }
    // Why 10 and 2: `frontier_abi::budgets::tests::close_float_caps`.
}
