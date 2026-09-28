//! Beacons (M1 contract §5.8, §5.1; SP-V2 `beacon.rs`).
//!
//! Implemented in wave 2 (W2-A): PostAnchor (0x10), PostAnchorMulti (0x11),
//! PostSeed (0x12) and PostBeacon (0x13). Wave 4 (W4-B): ArchiveAnchors
//! (0x14, class D) and CloseSeedCache (0x15, class N), and the reader of
//! THE anchor or its archive entry that SettleTransit uses
//! ([`the_anchor_or_archive`]).
//!
//! - **THE anchor** of `(bell, region)` lives at `an‖(bell, region)`; the
//!   first valid post wins and a later one is a success no-op (≈ 1.75k CU,
//!   SP-V2), so `A` is fixed once. A bell whose anchor was archived is
//!   refused (`Archived`): a second anchor would carry a new `A`, hence a
//!   new seed round.
//! - **Seed caches** `sd‖(bell, region, nonce)`: any keeper, any nonce; the
//!   round is fixed by THE anchor (`S(A) = first_round_from(A + W(bell) +
//!   Δ)`), so every cache of a bell holds the same seed.
//! - **BeaconLog(region)**: the latest round any keeper posted; a Reveal
//!   refuses once it reaches `S(A)` (§5.1 "reveal open").
//!
//! Status sets (the contract names none for §5.8): anchors and caches are
//! posted while the season is Running or Ended (bells before `end_bell`);
//! beacons while Seeded, Running or Ended. Pinned refusals: a region ≥ 16
//! or an anchor bell ≥ `end_bell` is `BadData`; a PostBeacon round at or
//! below the logged one is `AlreadyDone`. In a `test-beacon` build a round
//! the chain's Clock has not reached is `TooEarly` (the test key is public).

use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use frontier_abi::addr::{
    anchor_archive_seed, archive_part_of, bell_anchor_seed, day_of, seed_cache_seed,
};
use frontier_abi::ix as aix;
use frontier_abi::layout::AccountKind;
use frontier_abi::log::{Kind, NO_BELL};
use frontier_abi::tags::Ix;

use crate::addr::{self, AddrCtx};
use crate::clock::SeasonClock;
use crate::crypto::quick;
use crate::error::BAD_ACCOUNT;
use crate::events::{self, Buf};
use crate::evidence::{self, Evidence};
use crate::init::{self, SeasonSigner};
use crate::layout::beacon::{
    archive_archived, archive_entry_of, archive_key, archive_tombstoned, Anchor, Cache,
};
use crate::layout::{
    anchor_archive as AA, archive_entry as AE, beacon_log as BL, bell_anchor as BA, init_header,
    season as S, seed_cache as SC, Ro, Rw,
};
use crate::prologue::{self, check_accounts, expect_key, key, Now};
use crate::{FrontierError, R};

use super::season::round_is_due;

const ANCHOR_STATUS: [u8; 2] = [S::STATUS_RUNNING, S::STATUS_ENDED];
const BEACON_STATUS: [u8; 3] = [S::STATUS_SEEDED, S::STATUS_RUNNING, S::STATUS_ENDED];

/// Regions of the M1 map (Season `regions`, validated to 16).
const REGIONS: u8 = 16;

fn season_clock(season: &AccountInfo) -> R<SeasonClock> {
    let d = season.try_borrow_data()?;
    SeasonClock::read(&d)
}

/// The archive `aa‖(region, part(bell))` (v1.3: half-day archives, `part =
/// bell / 72`) at its canonical address: `true` if it tombstones `bell`
/// (absent: not archived).
fn archived(
    p: &Pubkey,
    ctx: &AddrCtx,
    season_id: u64,
    archive: &AccountInfo,
    bell: u32,
    region: u8,
) -> R<bool> {
    let part = archive_part_of(bell);
    expect_key(archive, &ctx.anchor_archive(region, part))?;
    if !prologue::presence(archive, p, AccountKind::AnchorArchive, season_id)? {
        return Ok(false);
    }
    let d = archive.try_borrow_data()?;
    if crate::layout::beacon::archive_key(&d)? != (region, part) {
        return Err(BAD_ACCOUNT);
    }
    archive_tombstoned(&d, bell)
}

/// Where THE anchor stands.
enum AnchorState {
    /// Already posted: the post is a no-op.
    Present,
    /// Absent and not archived: create it.
    Create,
}

/// The anchor checks of §5.8 in order: canonical address; present → no-op;
/// the region-day archive canonical and not tombstoning `bell`.
///
/// "Present" is authenticated (§4.1): owner = program, magic, season id and
/// the stored key fields `(bell, region)`; any other non-absent account at
/// the canonical address is `BadAccount` (§13.2 G3), never a no-op.
#[allow(clippy::too_many_arguments)]
fn anchor_state(
    p: &Pubkey,
    ctx: &AddrCtx,
    season_id: u64,
    anchor: &AccountInfo,
    archive: &AccountInfo,
    bell: u32,
    region: u8,
) -> R<AnchorState> {
    expect_key(anchor, &ctx.bell_anchor(bell, region))?;
    if prologue::presence(anchor, p, AccountKind::BellAnchor, season_id)? {
        let an = {
            let ad = anchor.try_borrow_data()?;
            Anchor::read(&ad)?
        };
        if an.bell != bell || an.region != region {
            return Err(BAD_ACCOUNT);
        }
        return Ok(AnchorState::Present);
    }
    if archived(p, ctx, season_id, archive, bell, region)? {
        return Err(FrontierError::Archived.into());
    }
    Ok(AnchorState::Create)
}

/// Creates THE anchor and logs `ANCHOR`.
#[allow(clippy::too_many_arguments)]
fn create_anchor<'a>(
    p: &Pubkey,
    fee_payer: &AccountInfo<'a>,
    season: &AccountInfo<'a>,
    anchor: &AccountInfo<'a>,
    signer: &SeasonSigner,
    season_id: u64,
    bell: u32,
    region: u8,
    round: u64,
    sig48: &[u8; 48],
    ev: &Evidence,
    now: &Now,
    log_bell: u32,
    beneficiary: &[u8; 32],
) -> R<()> {
    init::init_with_seed(
        fee_payer,
        anchor,
        season,
        signer,
        &bell_anchor_seed(bell, region),
        BA::SIZE,
        init::rent(BA::SIZE)?,
        p,
    )?;
    let mut ad = anchor.try_borrow_mut_data()?;
    init_header(&mut ad, AccountKind::BellAnchor, season_id)?;
    let mut w = Rw(&mut ad);
    w.set_u32(BA::BELL, bell)?;
    w.set_u8(BA::REGION, region)?;
    w.set_u8(BA::NET, quick::NET_QUICKNET)?;
    w.set_u64(BA::ROUND, round)?;
    w.set_i64(BA::A, now.ts)?;
    w.set_u64(BA::SLOT, now.slot)?;
    w.set_arr(BA::SIG48, sig48)?;
    w.set_arr(BA::RENT_TO, fee_payer.key.as_ref())?;
    w.set_u64(BA::EV_PRICE, ev.price)?;
    w.set_u32(BA::EV_LIMIT, ev.limit)?;
    let key = Buf::<5>::new().u32(bell).u8(region);
    let payload = Buf::<56>::new()
        .u64(round)
        .i64(now.ts)
        .u64(now.slot)
        .bytes(beneficiary);
    events::emit(Kind::ANCHOR, log_bell, key.get()?, payload.get()?, &mut [])
}

/// 0x10 PostAnchor: K + `[anchor w] [archive r] [ix sysvar] [system]`, data
/// `region, bell, round, sig48, hints, beneficiary`.
pub fn post_anchor(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::PostAnchor, a, None)?;
    prologue::top_level(Ix::PostAnchor)?;
    let x = aix::PostAnchor::decode(d)?;
    let now = prologue::now()?;
    let hdr = prologue::keeper(a, p, &ANCHOR_STATUS, now.ts)?;
    let [fee_payer, season_ai, anchor, archive, ix_sysvar, _system] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    if x.region >= REGIONS || x.bell >= hdr.end_bell {
        return Err(FrontierError::BadData.into());
    }
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    if let AnchorState::Present = anchor_state(p, &ctx, hdr.id, anchor, archive, x.bell, x.region)?
    {
        return Ok(());
    }
    let c = season_clock(season_ai)?;
    if x.round != c.tlock_round(x.bell) {
        return Err(FrontierError::WrongRound.into());
    }
    round_is_due(&c, x.round, now.ts)?;
    quick::verify(x.round, &x.sig48, &x.hints)?;
    let ev = evidence::read(ix_sysvar, now.slot)?;
    create_anchor(
        p,
        fee_payer,
        season_ai,
        anchor,
        &SeasonSigner::new(hdr.id, hdr.bump),
        hdr.id,
        x.bell,
        x.region,
        x.round,
        &x.sig48,
        &ev,
        &now,
        hdr.bell(now.ts).unwrap_or(NO_BELL),
        &x.beneficiary,
    )
}

/// 0x11 PostAnchorMulti: K + `[anchor × k w] [archive × k r] [ix sysvar]
/// [system]`, data `bell, round, sig48, hints, mask, beneficiary`; the
/// regions are the set bits of `mask`, ascending; `1 ≤ k ≤
/// MULTI_MAX_REGIONS` (7, pinned by the tx-size test). One verification
/// for every anchor created; present anchors are skipped.
pub fn post_anchor_multi(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    let x = aix::PostAnchorMulti::decode(d)?;
    let (regions, k) = crate::ix::mask_regions(x.mask);
    if k == 0 || k > frontier_abi::budgets::MULTI_MAX_REGIONS {
        return Err(FrontierError::BadData.into());
    }
    check_accounts(Ix::PostAnchorMulti, a, Some(&[1, k as u8, k as u8, 1]))?;
    prologue::top_level(Ix::PostAnchorMulti)?;
    let now = prologue::now()?;
    let hdr = prologue::keeper(a, p, &ANCHOR_STATUS, now.ts)?;
    if x.bell >= hdr.end_bell {
        return Err(FrontierError::BadData.into());
    }
    let fee_payer = &a[0];
    let season_ai = &a[1];
    let anchors = &a[2..2 + k];
    let archives = &a[2 + k..2 + 2 * k];
    let ix_sysvar = &a[2 + 2 * k];
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    let mut create = [false; 16];
    let mut any = false;
    for i in 0..k {
        if let AnchorState::Create = anchor_state(
            p,
            &ctx,
            hdr.id,
            &anchors[i],
            &archives[i],
            x.bell,
            regions[i],
        )? {
            create[i] = true;
            any = true;
        }
    }
    if !any {
        return Ok(());
    }
    let c = season_clock(season_ai)?;
    if x.round != c.tlock_round(x.bell) {
        return Err(FrontierError::WrongRound.into());
    }
    round_is_due(&c, x.round, now.ts)?;
    quick::verify(x.round, &x.sig48, &x.hints)?;
    let ev = evidence::read(ix_sysvar, now.slot)?;
    let signer = SeasonSigner::new(hdr.id, hdr.bump);
    let log_bell = hdr.bell(now.ts).unwrap_or(NO_BELL);
    for i in (0..k).filter(|i| create[*i]) {
        create_anchor(
            p,
            fee_payer,
            season_ai,
            &anchors[i],
            &signer,
            hdr.id,
            x.bell,
            regions[i],
            x.round,
            &x.sig48,
            &ev,
            &now,
            log_bell,
            &x.beneficiary,
        )?;
    }
    Ok(())
}

/// 0x12 PostSeed: K + `[anchor r] [cache w] [ix sysvar] [system]`, data
/// `region, bell, nonce, round, sig48, hints, beneficiary`.
pub fn post_seed(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::PostSeed, a, None)?;
    prologue::top_level(Ix::PostSeed)?;
    let x = aix::PostSeed::decode(d)?;
    let now = prologue::now()?;
    let hdr = prologue::keeper(a, p, &ANCHOR_STATUS, now.ts)?;
    let [fee_payer, season_ai, anchor, cache, _ix_sysvar, _system] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    if x.region >= REGIONS {
        return Err(FrontierError::BadData.into());
    }
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    // THE anchor, present at its canonical address.
    expect_key(anchor, &ctx.bell_anchor(x.bell, x.region))?;
    if !prologue::presence(anchor, p, AccountKind::BellAnchor, hdr.id)? {
        return Err(FrontierError::NoAnchor.into());
    }
    let an = {
        let ad = anchor.try_borrow_data()?;
        Anchor::read(&ad)?
    };
    if an.bell != x.bell || an.region != x.region {
        return Err(BAD_ACCOUNT);
    }
    // The cache at its canonical address; present (authenticated by owner,
    // magic, season and key fields, §4.1) → no-op; any other non-absent
    // account there → `BadAccount`.
    expect_key(cache, &ctx.seed_cache(x.bell, x.region, x.nonce))?;
    if prologue::presence(cache, p, AccountKind::SeedCache, hdr.id)? {
        let cd = cache.try_borrow_data()?;
        let r = Ro(&cd);
        if r.u32(SC::BELL)? != x.bell
            || r.u8(SC::REGION)? != x.region
            || r.u8(SC::NONCE)? != x.nonce
        {
            return Err(BAD_ACCOUNT);
        }
        return Ok(());
    }
    let c = season_clock(season_ai)?;
    if x.round != c.seed_round(x.bell, an.a) {
        return Err(FrontierError::WrongRound.into());
    }
    round_is_due(&c, x.round, now.ts)?;
    let sig96 = quick::verify(x.round, &x.sig48, &x.hints)?;
    let seed = quick::seed_of(x.round, &sig96);
    init::init_with_seed(
        fee_payer,
        cache,
        season_ai,
        &SeasonSigner::new(hdr.id, hdr.bump),
        &seed_cache_seed(x.bell, x.region, x.nonce),
        SC::SIZE,
        init::rent(SC::SIZE)?,
        p,
    )?;
    {
        let mut cd = cache.try_borrow_mut_data()?;
        init_header(&mut cd, AccountKind::SeedCache, hdr.id)?;
        let mut w = Rw(&mut cd);
        w.set_u32(SC::BELL, x.bell)?;
        w.set_u8(SC::REGION, x.region)?;
        w.set_u8(SC::NONCE, x.nonce)?;
        w.set_u64(SC::ROUND, x.round)?;
        w.set_arr(SC::SEED, &seed)?;
        w.set_arr(SC::ANCHOR_KEY, anchor.key.as_ref())?;
        w.set_i64(SC::A, an.a)?;
        w.set_u64(SC::SLOT, now.slot)?;
        w.set_arr(SC::RENT_TO, fee_payer.key.as_ref())?;
    }
    let key = Buf::<6>::new().u32(x.bell).u8(x.region).u8(x.nonce);
    let payload = Buf::<48>::new().u64(x.round).bytes(&seed).i64(an.a);
    events::emit(
        Kind::SEED,
        hdr.bell(now.ts).unwrap_or(NO_BELL),
        key.get()?,
        payload.get()?,
        &mut [],
    )
}

/// 0x13 PostBeacon: K + `[beaconlog w]`, data `region, round, sig48,
/// hints`: `round > latest_round` (`AlreadyDone` otherwise), then verify.
pub fn post_beacon(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::PostBeacon, a, None)?;
    prologue::top_level(Ix::PostBeacon)?;
    let x = aix::PostBeacon::decode(d)?;
    let now = prologue::now()?;
    let hdr = prologue::keeper(a, p, &BEACON_STATUS, now.ts)?;
    let [fee_payer, season_ai, log] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    if x.region >= REGIONS {
        return Err(FrontierError::BadData.into());
    }
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    expect_key(log, &ctx.beacon_log(x.region))?;
    prologue::present(log, p, AccountKind::BeaconLog, hdr.id)?;
    let latest = {
        let ld = log.try_borrow_data()?;
        let r = Ro(&ld);
        if r.u8(BL::REGION)? != x.region {
            return Err(BAD_ACCOUNT);
        }
        r.u64(BL::LATEST_ROUND)?
    };
    if x.round <= latest {
        return Err(FrontierError::AlreadyDone.into());
    }
    round_is_due(&season_clock(season_ai)?, x.round, now.ts)?;
    quick::verify(x.round, &x.sig48, &x.hints)?;
    {
        let mut ld = log.try_borrow_mut_data()?;
        let mut w = Rw(&mut ld);
        w.set_u64(BL::LATEST_ROUND, x.round)?;
        w.set_i64(BL::POSTED_TS, now.ts)?;
        w.set_u64(BL::POSTED_SLOT, now.slot)?;
        w.set_arr(BL::SIG48, &x.sig48)?;
        w.set_arr(BL::BENEFICIARY, fee_payer.key.as_ref())?;
    }
    let key = [x.region];
    let payload = x.round.to_le_bytes();
    events::emit(
        Kind::BEACON,
        hdr.bell(now.ts).unwrap_or(NO_BELL),
        &key,
        &payload,
        &mut [],
    )
}

// ------------------------------------------------------------ W4-B: archives

/// Where THE anchor of a bell was read from, and what SettleTransit needs
/// of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnchorSource {
    /// `A`: the Clock at the anchor's creation (archived: `bell_end(b) +
    /// a_off`).
    pub a: i64,
    /// THE anchor's landing slot (`None` once archived: the archive does
    /// not keep it, and every claim grace has passed by then).
    pub anchor_slot: Option<u64>,
    /// The round-T(b) signature (compressed G1).
    pub sig48: [u8; 48],
}

/// THE anchor `an‖(bell, region)` when `ai` is at its canonical address
/// (present, else `NoAnchor`), or the region-half-day archive `aa‖(region,
/// part(bell))` holding the bell's entry (present with the archived bit,
/// else `NoAnchor`); any other address is `BadAddress` (§5.8, I-44).
pub(crate) fn the_anchor_or_archive(
    p: &Pubkey,
    ctx: &AddrCtx,
    season_id: u64,
    ai: &AccountInfo,
    bell: u32,
    region: u8,
    clock: &SeasonClock,
) -> R<AnchorSource> {
    if ai.key.as_array() == &ctx.bell_anchor(bell, region) {
        if !prologue::presence(ai, p, AccountKind::BellAnchor, season_id)? {
            return Err(FrontierError::NoAnchor.into());
        }
        let an = {
            let d = ai.try_borrow_data()?;
            Anchor::read(&d)?
        };
        if an.bell != bell || an.region != region {
            return Err(BAD_ACCOUNT);
        }
        return Ok(AnchorSource {
            a: an.a,
            anchor_slot: Some(an.slot),
            sig48: an.sig48,
        });
    }
    let part = archive_part_of(bell);
    expect_key(ai, &ctx.anchor_archive(region, part))?;
    if !prologue::presence(ai, p, AccountKind::AnchorArchive, season_id)? {
        return Err(FrontierError::NoAnchor.into());
    }
    let d = ai.try_borrow_data()?;
    if archive_key(&d)? != (region, part) {
        return Err(BAD_ACCOUNT);
    }
    if !archive_archived(&d, bell)? {
        return Err(FrontierError::NoAnchor.into());
    }
    let (a_off, _seed, sig48) = archive_entry_of(&d, bell)?;
    let a = permutation_rules::frontier::beacon::bell_end(clock.genesis_ts, bell)
        .checked_add(a_off as i64)
        .ok_or(crate::error::OVERFLOW)?;
    Ok(AnchorSource {
        a,
        anchor_slot: None,
        sig48,
    })
}

/// THE anchor of `bell` or its archive, for whichever region the account
/// names (v1.7, SettleTransit of a valid seal whose destination Province
/// is absent: the region is known only once the seal is opened, and every
/// region's anchor of a bell carries the same round-T(bell) signature).
/// The account must be the canonical anchor or archive of its own stored
/// region (as [`the_anchor_or_archive`] checks). Returns the source and
/// the region; `NoAnchor` for anything else.
pub(crate) fn anchor_of_any_region(
    p: &Pubkey,
    ctx: &AddrCtx,
    season_id: u64,
    ai: &AccountInfo,
    bell: u32,
    clock: &SeasonClock,
) -> R<(AnchorSource, u8)> {
    let region = {
        let d = ai.try_borrow_data()?;
        let magic = d.get(..8);
        if ai.owner != p {
            None
        } else if magic == Some(&AccountKind::BellAnchor.magic()[..]) {
            Some(Anchor::read(&d)?.region)
        } else if magic == Some(&AccountKind::AnchorArchive.magic()[..]) {
            Some(archive_key(&d)?.0)
        } else {
            None
        }
    };
    let Some(region) = region.filter(|&r| r < REGIONS) else {
        return Err(FrontierError::NoAnchor.into());
    };
    let src = the_anchor_or_archive(p, ctx, season_id, ai, bell, region, clock)?;
    Ok((src, region))
}

/// Emits `CLOSE` for a short-header account (no chain: final seq 0 and a
/// zero head), before it closes (v1.3 §4.2).
pub(crate) fn emit_close_short(
    kind: AccountKind,
    raw: &[u8],
    recipient: &[u8; 32],
    lamports: u64,
    bell: u32,
) -> R<()> {
    let k = frontier_abi::log::close_key(kind, raw).ok_or(BAD_ACCOUNT)?;
    let payload = Buf::<80>::new()
        .u64(0)
        .bytes(&[0; 32])
        .bytes(recipient)
        .u64(lamports);
    events::emit(Kind::CLOSE, bell, &k, payload.get()?, &mut [])
}

/// Raw key of a BellAnchor (§4.1): `le32(bell) ‖ region`.
fn anchor_raw(bell: u32, region: u8) -> [u8; 5] {
    let mut r = [0u8; 5];
    r[..4].copy_from_slice(&bell.to_le_bytes());
    r[4] = region;
    r
}

/// Raw key of a SeedCache (§4.1): `le32(bell) ‖ region ‖ nonce`.
fn cache_raw(bell: u32, region: u8, nonce: u8) -> [u8; 6] {
    let mut r = [0u8; 6];
    r[..4].copy_from_slice(&bell.to_le_bytes());
    r[4] = region;
    r[5] = nonce;
    r
}

/// The archive entry's `a_off = A − bell_end(b)` (u32; an anchor is posted
/// at or after its round T(b), itself at or after `bell_end(b)`).
pub fn a_off_of(a: i64, genesis_ts: i64, bell: u32) -> Option<u32> {
    let end = permutation_rules::frontier::beacon::bell_end(genesis_ts, bell);
    u32::try_from(a.checked_sub(end)?).ok()
}

/// Seasons whose anchors may be archived: Running and Ended (anchors exist
/// only for bells of a running season).
const ARCHIVE_STATUS: [u8; 2] = [S::STATUS_RUNNING, S::STATUS_ENDED];

/// 0x14 ArchiveAnchors(region, part, n, bells): `[payer s,w] [season]
/// [archive w] [system] ([anchor w] [cache r] [anchor_rent_to w]) × n`
/// (1 ≤ n ≤ 8), class D (§5.8).
///
/// Checks in order: region < 16 and every bell of `part` (`bell / 72 ==
/// part`, `BadData`); the archive `aa‖(region, part)` canonical (created
/// when absent, 6,144 B, `rent_to` = the payer; pre-funding-safe). Per
/// bell: THE anchor `an‖(bell, region)` canonical; absent with the bell's
/// archived bit set → already archived, skipped (a repeat is a success);
/// absent otherwise → `NoAnchor`; `now ≥ A + archive_after` (`TooEarly`);
/// the cache a present SeedCache of THE anchor (canonical at its stored
/// nonce, `anchor_key` and `A` THE anchor's, round `S(A)`; absent →
/// `SeedNotReady`, another anchor's → `BadAccount`); `anchor_rent_to ==
/// anchor.rent_to` (`BadAddress`). Effects per bell: the tombstone and
/// archived bits set **first**, the entry `{a_off, seed, sig}` written
/// (I-44), `ARCHIVE {region, day; bell, a_off, seed}`, then `CLOSE` and
/// the anchor closed to its `rent_to` (I-49).
pub fn archive_anchors(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    let x = aix::ArchiveAnchors::decode(d)?;
    check_accounts(Ix::ArchiveAnchors, a, Some(&[1, x.n]))?;
    let now = prologue::now()?;
    let hdr = prologue::season(
        &a[1],
        p,
        Some(&crate::RULESET_HASH),
        &ARCHIVE_STATUS,
        now.ts,
    )?;
    let (payer, season_ai, archive, rest) = match a {
        [pa, s, ar, _system, rest @ ..] => (pa, s, ar, rest),
        _ => return Err(FrontierError::TooManyAccounts.into()),
    };
    let n = x.n as usize;
    let bells = &x.bells[..n];
    if x.region >= REGIONS || bells.iter().any(|b| archive_part_of(*b) != x.part) {
        return Err(FrontierError::BadData.into());
    }
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    let c = season_clock(season_ai)?;
    let archive_after = {
        let sd = season_ai.try_borrow_data()?;
        Ro(&sd).u32(S::ARCHIVE_AFTER)?
    };
    let log_bell = hdr.bell(now.ts).unwrap_or(NO_BELL);
    expect_key(archive, &ctx.anchor_archive(x.region, x.part))?;
    if prologue::presence(archive, p, AccountKind::AnchorArchive, hdr.id)? {
        let ad = archive.try_borrow_data()?;
        if archive_key(&ad)? != (x.region, x.part) {
            return Err(BAD_ACCOUNT);
        }
    } else {
        init::init_with_seed(
            payer,
            archive,
            season_ai,
            &SeasonSigner::new(hdr.id, hdr.bump),
            &anchor_archive_seed(x.region, x.part),
            AA::SIZE,
            init::rent(AA::SIZE)?,
            p,
        )?;
        let mut ad = archive.try_borrow_mut_data()?;
        init_header(&mut ad, AccountKind::AnchorArchive, hdr.id)?;
        let mut w = Rw(&mut ad);
        w.set_u8(AA::REGION, x.region)?;
        w.set_u32(AA::PART, x.part)?;
        w.set_arr(AA::RENT_TO, payer.key.as_ref())?;
    }
    for (i, &bell) in bells.iter().enumerate() {
        let (anchor, cache, rent_to) = (&rest[3 * i], &rest[3 * i + 1], &rest[3 * i + 2]);
        let anchor_key = ctx.bell_anchor(bell, x.region);
        expect_key(anchor, &anchor_key)?;
        if !prologue::presence(anchor, p, AccountKind::BellAnchor, hdr.id)? {
            let ad = archive.try_borrow_data()?;
            if archive_archived(&ad, bell)? {
                continue;
            }
            return Err(FrontierError::NoAnchor.into());
        }
        let an = {
            let dd = anchor.try_borrow_data()?;
            Anchor::read(&dd)?
        };
        if an.bell != bell || an.region != x.region {
            return Err(BAD_ACCOUNT);
        }
        let due =
            an.a.checked_add(archive_after as i64)
                .ok_or(crate::error::OVERFLOW)?;
        if now.ts < due {
            return Err(FrontierError::TooEarly.into());
        }
        if !prologue::presence(cache, p, AccountKind::SeedCache, hdr.id)? {
            return Err(FrontierError::SeedNotReady.into());
        }
        let ca = {
            let cd = cache.try_borrow_data()?;
            Cache::read(&cd)?
        };
        expect_key(cache, &ctx.seed_cache(bell, x.region, ca.nonce))?;
        if ca.bell != bell
            || ca.region != x.region
            || ca.anchor_key != anchor_key
            || ca.a != an.a
            || ca.round != c.seed_round(bell, an.a)
        {
            return Err(BAD_ACCOUNT);
        }
        expect_key(rent_to, &an.rent_to)?;
        let a_off = a_off_of(an.a, c.genesis_ts, bell).ok_or(crate::error::OVERFLOW)?;
        {
            let mut ad = archive.try_borrow_mut_data()?;
            let mut w = Rw(&mut ad);
            for base in [AA::TOMBSTONE, AA::ARCHIVED] {
                let (at, m) = AA::bit(base, bell);
                let v = w.u8(at)? | m;
                w.set_u8(at, v)?;
            }
            let o = AA::entry(bell);
            w.set_u32(o + AE::A_OFF, a_off)?;
            w.set_arr(o + AE::SEED, &ca.seed)?;
            w.set_arr(o + AE::SIG, &an.sig48)?;
        }
        let k = Buf::<5>::new().u8(x.region).u32(day_of(bell));
        let payload = Buf::<40>::new().u32(bell).u32(a_off).bytes(&ca.seed);
        events::emit(Kind::ARCHIVE, log_bell, k.get()?, payload.get()?, &mut [])?;
        emit_close_short(
            AccountKind::BellAnchor,
            &anchor_raw(bell, x.region),
            &an.rent_to,
            anchor.lamports(),
            log_bell,
        )?;
        init::close_to(p, anchor, rent_to, &init::Sink::Never, log_bell)?;
    }
    Ok(())
}

/// 0x15 CloseSeedCache(bell, region, nonce): `[any s] [season] [cache w]
/// [archive r] [rent_to w]`, class N (§5.8). The cache canonical and
/// present (absent: `AlreadyDone`, a repeat); the archive `aa‖(region,
/// part(bell))` holds the bell (archived bit, else `TooEarly`); `rent_to ==
/// cache.rent_to` (`BadAddress`). Log `CLOSE`; the rent returns to the
/// fee payer that posted the cache (I-49). W6-B (W5-A O4): on the Closed
/// tombstone the archive (canonical address only; CloseSeason part 9 may
/// have closed it) is not read and the cache closes at once — no reader
/// of a cache runs after the final part.
pub fn close_seed_cache(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::CloseSeedCache, a, None)?;
    let x = aix::CloseSeedCache::decode(d)?;
    let now = prologue::now()?;
    let [_any, season_ai, cache, archive, rent_to] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let live = super::clash::float_season_with(season_ai, p, &ARCHIVE_STATUS, now.ts)?;
    if x.region >= REGIONS {
        return Err(FrontierError::BadData.into());
    }
    let sid = live.id();
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    expect_key(cache, &ctx.seed_cache(x.bell, x.region, x.nonce))?;
    if !prologue::presence(cache, p, AccountKind::SeedCache, sid)? {
        return Err(FrontierError::AlreadyDone.into());
    }
    let ca_rent_to = {
        let cd = cache.try_borrow_data()?;
        let r = Ro(&cd);
        if r.u32(SC::BELL)? != x.bell
            || r.u8(SC::REGION)? != x.region
            || r.u8(SC::NONCE)? != x.nonce
        {
            return Err(BAD_ACCOUNT);
        }
        r.arr::<32>(SC::RENT_TO)?
    };
    let part = archive_part_of(x.bell);
    expect_key(archive, &ctx.anchor_archive(x.region, part))?;
    let tomb = matches!(live, super::clash::FloatSeason::Tomb(_));
    let archived = tomb
        || prologue::presence(archive, p, AccountKind::AnchorArchive, sid)? && {
            let ad = archive.try_borrow_data()?;
            if archive_key(&ad)? != (x.region, part) {
                return Err(BAD_ACCOUNT);
            }
            archive_archived(&ad, x.bell)?
        };
    if !archived {
        return Err(FrontierError::TooEarly.into());
    }
    expect_key(rent_to, &ca_rent_to)?;
    let bell = live.log_bell(now.ts);
    emit_close_short(
        AccountKind::SeedCache,
        &cache_raw(x.bell, x.region, x.nonce),
        &ca_rent_to,
        cache.lamports(),
        bell,
    )?;
    init::close_to(p, cache, rent_to, &init::Sink::Never, bell)?;
    Ok(())
}

#[cfg(test)]
mod archive_tests {
    use super::*;

    #[test]
    fn a_off_is_the_anchor_time_after_the_bell() {
        let g = 1_000_000;
        assert_eq!(a_off_of(g + 600 * 11 + 7, g, 10), Some(7));
        assert_eq!(a_off_of(g + 600 * 11, g, 10), Some(0));
        assert_eq!(
            a_off_of(g + 600 * 11 - 1, g, 10),
            None,
            "before the bell end"
        );
        assert_eq!(anchor_raw(0x0102_0304, 9), [4, 3, 2, 1, 9]);
        assert_eq!(cache_raw(1, 2, 3), [1, 0, 0, 0, 2, 3]);
    }
}
