//! Beacons (M1 contract §5.8, §5.1; SP-V2 `beacon.rs`).
//!
//! Implemented in wave 2 (W2-A): PostAnchor (0x10), PostAnchorMulti (0x11),
//! PostSeed (0x12) and PostBeacon (0x13). ArchiveAnchors and CloseSeedCache
//! are stubs until W4-B.
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

use frontier_abi::addr::{bell_anchor_seed, day_of, seed_cache_seed};
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
use crate::layout::beacon::{archive_tombstoned, Anchor};
use crate::layout::{
    beacon_log as BL, bell_anchor as BA, init_header, season as S, seed_cache as SC, Ro, Rw,
};
use crate::prologue::{self, check_accounts, expect_key, key, Now};
use crate::{FrontierError, R};

use super::season::round_is_due;

super::stubs!(archive_anchors, close_seed_cache);

const ANCHOR_STATUS: [u8; 2] = [S::STATUS_RUNNING, S::STATUS_ENDED];
const BEACON_STATUS: [u8; 3] = [S::STATUS_SEEDED, S::STATUS_RUNNING, S::STATUS_ENDED];

/// Regions of the M1 map (Season `regions`, validated to 16).
const REGIONS: u8 = 16;

fn season_clock(season: &AccountInfo) -> R<SeasonClock> {
    let d = season.try_borrow_data()?;
    SeasonClock::read(&d)
}

/// The archive `aa‖(region, day(bell))` at its canonical address: `true`
/// if it tombstones `bell` (absent: not archived).
fn archived(
    p: &Pubkey,
    ctx: &AddrCtx,
    season_id: u64,
    archive: &AccountInfo,
    bell: u32,
    region: u8,
) -> R<bool> {
    let day = day_of(bell);
    expect_key(archive, &ctx.anchor_archive(region, day))?;
    if !prologue::presence(archive, p, AccountKind::AnchorArchive, season_id)? {
        return Ok(false);
    }
    let d = archive.try_borrow_data()?;
    if crate::layout::beacon::archive_key(&d)? != (region, day) {
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
    if anchor.owner == p {
        return Ok(AnchorState::Present);
    }
    if !init::is_absent(anchor) {
        return Err(BAD_ACCOUNT);
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
    // The cache at its canonical address; present → no-op.
    expect_key(cache, &ctx.seed_cache(x.bell, x.region, x.nonce))?;
    if cache.owner == p {
        return Ok(());
    }
    if !init::is_absent(cache) {
        return Err(BAD_ACCOUNT);
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
