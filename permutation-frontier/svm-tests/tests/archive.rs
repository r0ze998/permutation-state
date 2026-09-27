//! ArchiveAnchors (0x14) and CloseSeedCache (0x15), W4-B, on the
//! test-beacon build (any round, I-53): the archive entry keeps each bell's
//! `a_off`, seed and signature (I-44), the tombstone and archived bits are
//! set before THE anchor closes to its `rent_to` (I-49), caches close once
//! their bell is archived; G2 (pre-funded AnchorArchive; re-creation of an
//! archived anchor or cache refused), G3 forgeries, G1 budgets, G13 codes.
//! Nothing is crafted: every anchor, cache and archive here is written by
//! the program.

mod common;

use common::{assert_program_account, assert_shortfall_only, paid, prefunds};
use frontier_abi::layout::beacon::{
    anchor_archive as AA, archive_entry as AE, bell_anchor as BA, seed_cache as SC,
};
use frontier_abi::layout::AccountKind;
use frontier_abi::log::Kind;
use frontier_abi::tags::Ix;
use permutation_frontier_svm_tests::budget::{assert_within, ceilings};
use permutation_frontier_svm_tests::chain::{assert_code, expect_lands, with_account, Chain};
use permutation_frontier_svm_tests::ix::beacon::{
    archive_anchors, archive_at, close_cache_at, close_seed_cache, ArchiveItem,
};
use permutation_frontier_svm_tests::records::{self, le};
use permutation_frontier_svm_tests::world::{archive_part, World};
use permutation_frontier_svm_tests::{Address, FrontierError as E, Instruction, Keypair, Signer};

const REGION: u8 = 3;
/// Eight bells of one half-day part (part 0).
const BELLS: [u32; 8] = [10, 11, 12, 13, 14, 15, 16, 17];

/// A running season with THE anchors of `bells` (region 3) and a cache of
/// each (nonce 0), the Clock at `A(last) + archive_after`.
fn anchored(bells: &[u32]) -> (Chain, World) {
    let (mut c, w) = common::test_beacon();
    for b in bells {
        expect_lands(w.post_anchor(&mut c, *b, REGION), "PostAnchor");
    }
    for b in bells {
        expect_lands(w.post_seed(&mut c, *b, REGION, 0), "PostSeed");
    }
    let last = w.anchor_a(&c, *bells.last().unwrap(), REGION).unwrap();
    let at = last + w.params.season.archive_after as i64;
    if c.now < at {
        c.set_time(at);
    }
    (c, w)
}

fn items(w: &World, bells: &[u32]) -> Vec<ArchiveItem> {
    bells
        .iter()
        .map(|&bell| ArchiveItem {
            bell,
            cache_nonce: 0,
            anchor_rent_to: w.keeper.pubkey(),
        })
        .collect()
}

fn archive_ix(w: &World, payer: &Address, bells: &[u32]) -> Instruction {
    archive_anchors(
        &w.a,
        *payer,
        REGION,
        archive_part(bells[0]),
        &items(w, bells),
    )
}

fn send(
    c: &mut Chain,
    ix: Instruction,
    k: &Keypair,
) -> permutation_frontier_svm_tests::chain::SendResult {
    c.send(&[ix], &[k])
}

/// ArchiveAnchors of 8 bells into a fresh region-half-day archive: each
/// entry holds `A − bell_end(b)`, THE cache's seed and THE anchor's
/// round-T(b) signature; the tombstone and archived bits are set; each
/// anchor closes to its `rent_to` (`ARCHIVE`, then `CLOSE`); the archive's
/// `rent_to` is the payer. A repeat is a success that changes nothing.
#[test]
fn g05_archive_anchors_keeps_the_entries_and_closes_the_anchors() {
    let (mut c, w) = anchored(&BELLS);
    let keeper = w.keeper.pubkey();
    let payer = c.funded(b"w4b-archive-payer", 1);
    let before: Vec<([u8; 48], i64, [u8; 32], u64)> = BELLS
        .iter()
        .map(|&b| {
            let d = c.data(&w.a.anchor(b, REGION));
            (
                d[BA::SIG48..BA::SIG48 + 48].try_into().unwrap(),
                le(&d[BA::A..BA::A + 8]) as i64,
                w.cache_seed(&c, b, REGION, 0),
                c.lamports(&w.a.anchor(b, REGION)),
            )
        })
        .collect();
    let k0 = c.lamports(&keeper);
    let ix = archive_ix(&w, &payer.pubkey(), &BELLS);
    let l = expect_lands(send(&mut c, ix.clone(), &payer), "archive_ix(");
    let arc = w.a.archive(REGION, 0);
    common::assert_program_account(&c, &arc, AA::MAGIC, AA::SIZE, w.id);
    let d = c.data(&arc);
    assert_eq!(d[AA::RENT_TO..AA::RENT_TO + 32], *payer.pubkey().as_ref());
    for (b, (sig, a, seed, rent)) in BELLS.iter().zip(&before) {
        for base in [AA::TOMBSTONE, AA::ARCHIVED] {
            let (at, m) = AA::bit(base, *b);
            assert!(d[at] & m != 0, "bell {b} bit at {base}");
        }
        let o = AA::entry(*b);
        let a_off = le(&d[o + AE::A_OFF..o + AE::A_OFF + 4]) as i64;
        assert_eq!(a_off, a - w.bell_end(*b));
        assert_eq!(d[o + AE::SEED..o + AE::SEED + 32], *seed);
        assert_eq!(d[o + AE::SIG..o + AE::SIG + 48], *sig);
        assert!(c.is_absent(&w.a.anchor(*b, REGION)));
        let _ = rent;
    }
    let refunds: u64 = before.iter().map(|x| x.3).sum();
    assert_eq!(
        c.lamports(&keeper),
        k0 + refunds,
        "anchors refund their rent_to"
    );
    let recs = records::of_kind(&l.logs, Kind::ARCHIVE);
    assert_eq!(recs.len(), 8);
    assert_eq!(recs[0].key[0], REGION);
    assert_eq!(recs[0].u64("bell"), 10);
    let closes = records::of_kind(&l.logs, Kind::CLOSE);
    assert_eq!(closes.len(), 8);
    assert!(closes
        .iter()
        .all(|r| r.key[0] == AccountKind::BellAnchor as u8));
    // Repeat: every bell already archived, nothing changes.
    let d0 = c.data(&arc);
    let mut f = c.fork();
    f.next_slot();
    expect_lands(send(&mut f, ix, &payer), "archive_ix( repeat");
    assert_eq!(f.data(&arc), d0);
}

/// Re-creation after archive (§13.2, I-46): THE anchor of an archived bell
/// cannot be posted again (`Archived`), and no cache can be posted without
/// it (`NoAnchor`); CloseSeedCache closes the bell's caches only once it is
/// archived (`TooEarly` before), to their `rent_to` (`BadAddress`
/// otherwise), and a repeat is `AlreadyDone`.
#[test]
fn g02_recreate_after_archive_refused_and_caches_close() {
    let (mut c, w) = anchored(&BELLS[..1]);
    let b = BELLS[0];
    let keeper = w.keeper.pubkey();
    let any = c.funded(b"w4b-any", 1);
    let close = close_seed_cache(&w.a, any.pubkey(), b, REGION, 0, keeper);
    assert_code(send(&mut c, close.clone(), &any), E::TooEarly);
    expect_lands(
        send(&mut c, archive_ix(&w, &keeper, &[b]), &w.keeper),
        "archive_ix(",
    );
    assert_code(w.post_anchor(&mut c, b, REGION), E::Archived);
    let r = w.tlock_round(b) + 300;
    let seed = w.seed_ix(b, REGION, 1, r);
    assert_code(send(&mut c, seed, &w.keeper), E::NoAnchor);
    let stranger = c.funded(b"w4b-stranger", 1).pubkey();
    assert_code(
        send(
            &mut c,
            with_account(close.clone(), close_cache_at::RENT_TO, stranger),
            &any,
        ),
        E::BadAddress,
    );
    let k0 = c.lamports(&keeper);
    let rent = c.lamports(&w.a.seed_cache(b, REGION, 0));
    let l = expect_lands(send(&mut c, close.clone(), &any), "close_seed_cache(");
    assert!(c.is_absent(&w.a.seed_cache(b, REGION, 0)));
    assert_eq!(c.lamports(&keeper), k0 + rent);
    assert_eq!(
        records::one(&l.logs, Kind::CLOSE).key[0],
        AccountKind::SeedCache as u8
    );
    assert_code(send(&mut c, close, &any), E::AlreadyDone);
    let bad = close_seed_cache(&w.a, any.pubkey(), b, 16, 0, keeper);
    assert_code(send(&mut c, bad, &any), E::BadData);
}

/// G2 (§13.2): the AnchorArchive created on a pre-funded address (one
/// lamport, rent, 10× rent); the payer pays only the shortfall.
#[test]
fn g02_prefund_anchor_archive() {
    let (base, w) = anchored(&BELLS[..1]);
    let rent = base.rent(AA::SIZE);
    let arc = w.a.archive(REGION, 0);
    for pre in prefunds(rent) {
        let mut c = base.fork();
        c.prefund(&arc, pre);
        let payer = c.funded(b"w4b-prefund", 1);
        let before = c.lamports(&payer.pubkey());
        let l = expect_lands(
            send(&mut c, archive_ix(&w, &payer.pubkey(), &BELLS[..1]), &payer),
            "ArchiveAnchors on a pre-funded archive",
        );
        assert_shortfall_only("AnchorArchive", paid(before, &c, &payer, &l), pre, rent, 0);
        assert_program_account(&c, &arc, AA::MAGIC, AA::SIZE, w.id);
    }
}

/// ArchiveAnchors' refusals (G13) and forgeries (G3).
#[test]
fn g13_archive_anchors_refusals() {
    let (mut c, w) = common::test_beacon();
    let keeper = w.keeper.pubkey();
    for b in [10u32, 11] {
        expect_lands(w.post_anchor(&mut c, b, REGION), "PostAnchor");
    }
    expect_lands(w.post_seed(&mut c, 10, REGION, 0), "PostSeed");
    // Too early: A + archive_after not reached.
    assert_code(
        send(&mut c, archive_ix(&w, &keeper, &[10]), &w.keeper),
        E::TooEarly,
    );
    let a11 = w.anchor_a(&c, 11, REGION).unwrap();
    c.set_time(a11 + w.params.season.archive_after as i64);
    // Bell 11 has no cache: SeedNotReady; bell 12 no anchor: NoAnchor.
    assert_code(
        send(&mut c, archive_ix(&w, &keeper, &[11]), &w.keeper),
        E::SeedNotReady,
    );
    assert_code(
        send(&mut c, archive_ix(&w, &keeper, &[12]), &w.keeper),
        E::NoAnchor,
    );
    // A bell of another part, a region ≥ 16: BadData.
    let other_part = archive_anchors(&w.a, keeper, REGION, 0, &items(&w, &[72]));
    assert_code(send(&mut c, other_part, &w.keeper), E::BadData);
    let bad_region = archive_anchors(&w.a, keeper, 16, 0, &items(&w, &[10]));
    assert_code(send(&mut c, bad_region, &w.keeper), E::BadData);
    let ix = archive_ix(&w, &keeper, &[10]);
    // The anchor's rent_to, the archive and the anchor at their addresses.
    let stranger = c.funded(b"w4b-archive-stranger", 1).pubkey();
    let first = archive_at::FIRST_BELL;
    assert_code(
        send(
            &mut c,
            with_account(ix.clone(), first + 2, stranger),
            &w.keeper,
        ),
        E::BadAddress,
    );
    assert_code(
        send(
            &mut c,
            with_account(ix.clone(), archive_at::ARCHIVE, w.a.archive(REGION, 1)),
            &w.keeper,
        ),
        E::BadAddress,
    );
    assert_code(
        send(
            &mut c,
            with_account(ix.clone(), first, w.a.anchor(11, REGION)),
            &w.keeper,
        ),
        E::BadAddress,
    );
    // Another bell's cache (canonical for its own bell): BadAddress.
    expect_lands(w.post_seed(&mut c, 11, REGION, 0), "PostSeed");
    assert_code(
        send(
            &mut c,
            with_account(ix.clone(), first + 1, w.a.seed_cache(11, REGION, 0)),
            &w.keeper,
        ),
        E::BadAddress,
    );
    // A forged anchor at the canonical address (magic): BadAccount.
    let mut f = c.fork();
    f.edit(&w.a.anchor(10, REGION), |d| d[0] ^= 1);
    assert_code(send(&mut f, ix.clone(), &w.keeper), E::BadAccount);
    // An archive of another season (season id): BadAccount.
    expect_lands(send(&mut c, ix.clone(), &w.keeper), "archive_ix(");
    let mut f = c.fork();
    f.edit(&w.a.archive(REGION, 0), |d| d[8] ^= 1);
    assert_code(
        send(&mut f, archive_ix(&w, &keeper, &[11]), &w.keeper),
        E::BadAccount,
    );
}

/// G1 (§13.1): ArchiveAnchors with 8 bells, 8 anchor closes and a fresh
/// archive; CloseSeedCache.
#[test]
fn g01_budget_w4b_archive() {
    let (mut c, w) = anchored(&BELLS);
    let payer = c.funded(b"w4b-g1-payer", 1);
    let need = c
        .measure(&[archive_ix(&w, &payer.pubkey(), &BELLS)], &[&payer])
        .expect("measures");
    assert_within(
        "ArchiveAnchors 8 bells, fresh archive",
        &need,
        &ceilings(Ix::ArchiveAnchors, 0, c.programdata_len()),
    );
    expect_lands(
        send(&mut c, archive_ix(&w, &payer.pubkey(), &BELLS), &payer),
        "archive_ix(",
    );
    let any = c.funded(b"w4b-g1-any", 1);
    let close = close_seed_cache(&w.a, any.pubkey(), 10, REGION, 0, w.keeper.pubkey());
    let need = c.measure(&[close], &[&any]).expect("measures");
    assert_within(
        "CloseSeedCache",
        &need,
        &ceilings(Ix::CloseSeedCache, 0, c.programdata_len()),
    );
    let _ = SC::SIZE;
}

/// `g01_loaded_limit_*` (I-45) for ArchiveAnchors (8 bells) and
/// CloseSeedCache.
#[test]
fn g01_loaded_limit_w4b_archive() {
    use permutation_frontier_svm_tests::world::transit::loaded_check;
    let (mut c, w) = anchored(&BELLS);
    let payer = c.funded(b"w4b-loaded-payer", 1);
    loaded_check(
        &c,
        Ix::ArchiveAnchors,
        &[archive_ix(&w, &payer.pubkey(), &BELLS)],
        &[&payer],
    );
    expect_lands(
        send(&mut c, archive_ix(&w, &payer.pubkey(), &BELLS), &payer),
        "archive_ix(",
    );
    let any = c.funded(b"w4b-loaded-any", 1);
    let close = close_seed_cache(&w.a, any.pubkey(), 10, REGION, 0, w.keeper.pubkey());
    loaded_check(&c, Ix::CloseSeedCache, &[close], &[&any]);
}
