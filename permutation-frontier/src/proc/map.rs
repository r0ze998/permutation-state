//! Rings and provinces (M1 contract §5.9, I-30, I-46, I-48, I-56):
//! OpenRing (0x20), ConsumeRingSeed (0x21), OpenProvince (0x22),
//! FoldOccupancy (0x23) and CloseProvince (0x24). Implemented by W3-A.
//!
//! **Status sets (I-46).** OpenRing, ConsumeRingSeed and OpenProvince run
//! while the season's effective status is Seeded or Running (never Ended,
//! Closed or Aborted), so a closed Province or RingSeed can never be
//! created again; a ring beyond the genesis ring `g` also needs Running.
//! FoldOccupancy folds while Seeded or Running. CloseProvince needs Ended
//! with `now ≥ end + 72 h`, or Aborted.
//!
//! **Refusal codes the contract leaves open (pinned here, W3-A notes §4):**
//!
//! | case | code |
//! |---|---|
//! | OpenRing `d < rings_opened`, RingSeed present, ConsumeRingSeed of a seeded ring, OpenProvince of a present Province | `AlreadyDone` 52 |
//! | OpenRing `d > rings_opened` | `TooEarly` 13 |
//! | OpenRing `d > r_max`, OpenProvince outside `r_max` (CL-04), FoldOccupancy `part > 2` | `BadData` 1 |
//! | OpenRing `d > g` less than a bell after the last opening | `TooEarly` 13 |
//! | OpenRing `d > g` with no wedge at `θ` occupancy | `Capacity` 10 |
//! | OpenRing `d > g` with a wedge fund below `d × rent(4,096)`; OpenProvince with the fund short | `Insufficient` 21 |
//! | OpenProvince of a ring whose RingSeed is absent or not yet seeded | `SeedNotReady` 54 |
//! | CloseProvince before `end + 72 h` | `TooEarly` 13 |
//!
//! **Province encoding (pinned; the herald, the verifier and W4-A's clash
//! read it back):** `terrain[i] = Terrain as u8` (Grassland 0 … Water 5),
//! `resource[i] = 0` or `1 + TileResource as u8`, `sites` / `site_count`
//! as the kernel's, `passable_mask` bit i = `Terrain::is_passable`,
//! `rough_mask` bit i = `defense_bps < BPS_ONE` (Forest, Hills: the
//! clash kernel's rough test), `road_mask` and `explored_mask` 0. The
//! **terrain digest** of PROVINCE_OPEN is `sha256("PSF-TERRAIN-v1" ‖
//! province[128..296])` (those 168 bytes as written here). The Concord's
//! `wedge` is stored as 6 (none); it is funded from wedge 0 (DECISIONS
//! G10). The camp's `troops` are whole troops (kernel `camp::Camp`),
//! `next_check_day = day + 1`, `gen = 1` for the initial camp.

use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use frontier_abi::addr::{province_seed, ring_seed_seed};
use frontier_abi::ix as aix;
use frontier_abi::layout::AccountKind;
use frontier_abi::log::{close_key, EntityKind, Kind, NO_BELL};
use frontier_abi::prologue::SeasonHdr;
use frontier_abi::tags::Ix;
use permutation_rules::fixed::BPS_ONE;
use permutation_rules::frontier::beacon;
use permutation_rules::frontier::camp;
use permutation_rules::frontier::clash::NEUTRAL;
use permutation_rules::frontier::geometry::{region_of, ProvinceCoord, PROVINCE_TILES};
use permutation_rules::frontier::terrain::{generate_province, ProvinceTerrain};
use permutation_rules::hash::sha256;

use crate::addr::{self, AddrCtx};
use crate::clock::SeasonClock;
use crate::crypto::quick;
use crate::error::{BAD_ACCOUNT, OVERFLOW};
use crate::events::{self, Buf, Chained};
use crate::init::{self, SeasonSigner, Sink};
use crate::layout::{
    camp as CP, frontier as FR, init_header, join_shard as JS, province as PV, province_fund as PF,
    ring_seed as RS, season as S, site as SM, Ro, Rw,
};
use crate::prologue::{self, check_accounts, expect_key, key};
use crate::{FrontierError, R};

use super::season::round_is_due;

/// Seeded or Running (I-46).
pub(crate) const LAND_STATUS: [u8; 2] = [S::STATUS_SEEDED, S::STATUS_RUNNING];
/// Wedges (= factions).
const WEDGES: usize = PF::WEDGES;
/// Seconds after `end` before the season-end closes (§5.9, 72 h).
pub(crate) const END_GRACE_SECS: i64 = 72 * 3_600;
/// The Concord's stored wedge (it has none).
pub const NO_WEDGE: u8 = 6;
/// Domain of the PROVINCE_OPEN terrain digest.
pub const TERRAIN_DOMAIN: &[u8] = b"PSF-TERRAIN-v1";
/// Bytes of the Province the terrain digest covers: `TERRAIN ..
/// EXPLORED_MASK + 8` (terrain, resource, sites, site_count, rsv, masks).
pub const TERRAIN_BLOCK: core::ops::Range<usize> = PV::TERRAIN..PV::SITE_MIRROR;
const _: () = assert!(PV::SITE_MIRROR - PV::TERRAIN == 168);

// ------------------------------------------------------------ helpers

/// The current bell, or 0 before genesis (stored bells: `opened_bell`,
/// `resolved_next`, `last_ring_open_bell`, `fold_bell`).
pub(crate) fn bell_or_zero(hdr: &SeasonHdr, now: i64) -> u32 {
    hdr.bell(now).unwrap_or(0)
}

/// The bell a record carries (`NO_BELL` before genesis).
pub(crate) fn log_bell(hdr: &SeasonHdr, now: i64) -> u32 {
    hdr.bell(now).unwrap_or(NO_BELL)
}

/// The account is at `expected` (`BadAddress`) and present (`BadAccount`
/// otherwise, absence included).
pub(crate) fn present_at(
    ai: &AccountInfo,
    expected: &[u8; 32],
    p: &Pubkey,
    kind: AccountKind,
    season_id: u64,
) -> R<()> {
    expect_key(ai, expected)?;
    prologue::present(ai, p, kind, season_id)
}

/// The funding wedge of a province: its wedge, or 0 for the Concord
/// (DECISIONS G10).
pub fn fund_wedge(c: ProvinceCoord) -> u8 {
    c.wedge().unwrap_or(0)
}

/// `end` of the season: `bell_start(end_bell)`.
pub(crate) fn season_end_ts(hdr: &SeasonHdr) -> i64 {
    beacon::bell_start(hdr.genesis_ts, hdr.end_bell)
}

/// Season-end closes (§5.9 CloseProvince, CloseHolding, CloseCitizen):
/// Ended with `now ≥ end + 72 h` (`TooEarly` before), or Aborted.
pub(crate) fn season_end_close(season_ai: &AccountInfo, p: &Pubkey, now: i64) -> R<SeasonHdr> {
    let hdr = prologue::season(
        season_ai,
        p,
        Some(&crate::RULESET_HASH),
        &[S::STATUS_ENDED, S::STATUS_ABORTED],
        now,
    )?;
    if hdr.status == S::STATUS_ENDED {
        let at = season_end_ts(&hdr)
            .checked_add(END_GRACE_SECS)
            .ok_or(OVERFLOW)?;
        if now < at {
            return Err(FrontierError::TooEarly.into());
        }
    }
    Ok(hdr)
}

/// Raw key of a Province (§4.1): `le32(P) ‖ le32(Q)`.
pub fn raw_province(p: i32, q: i32) -> [u8; 8] {
    let mut r = [0u8; 8];
    r[..4].copy_from_slice(&p.to_le_bytes());
    r[4..].copy_from_slice(&q.to_le_bytes());
    r
}

/// Emits `CLOSE` for a chained account before it closes (v1.3 §4.2): key
/// `account kind ‖ raw key`, payload the chain as it stands before this
/// record (`final_seq`, `final_head`), the recipient and the lamports; the
/// record itself advances the chain one last time (its tail link).
pub(crate) fn emit_close(
    kind: AccountKind,
    entity: EntityKind,
    raw: &[u8],
    data: &mut [u8],
    recipient: &[u8; 32],
    lamports: u64,
    bell: u32,
) -> R<()> {
    let k = close_key(kind, raw).ok_or(BAD_ACCOUNT)?;
    let (seq, head) = crate::layout::chain_of(data)?;
    let payload = Buf::<80>::new()
        .u64(seq)
        .bytes(&head)
        .bytes(recipient)
        .u64(lamports);
    events::emit(
        Kind::CLOSE,
        bell,
        &k,
        payload.get()?,
        &mut [Chained { entity, data }],
    )
}

/// `init_funded` (§4.2) in the order the runtime accepts: **allocate first,
/// then move the lamports**. `init::init_funded` moves the shortfall out of
/// the fund by direct arithmetic *before* its AllocateWithSeed CPI; the
/// target's credit is synced into the CPI but the fund's debit is not (the
/// fund is not a CPI account), so the runtime refuses the caller as
/// `UnbalancedInstruction` [measured, W3-A notes finding F1]. Here the CPI
/// runs on the untouched accounts and the lamports move afterwards between
/// two program-owned accounts. Moves `max(amount, rent − pre-funded)` so the
/// target ends rent-exempt; returns the lamports moved.
#[allow(clippy::too_many_arguments)]
pub(crate) fn init_funded_after<'a>(
    fund: &AccountInfo<'a>,
    target: &AccountInfo<'a>,
    season: &AccountInfo<'a>,
    signer: &SeasonSigner,
    seed: &addr::Seed,
    space: usize,
    program: &Pubkey,
    amount: u64,
) -> R<u64> {
    if fund.owner != program || !init::is_absent(target) {
        return Err(BAD_ACCOUNT);
    }
    let need = init::rent(space)?
        .saturating_sub(target.lamports())
        .max(amount);
    let floor = init::rent(fund.data_len())?;
    if fund.lamports().saturating_sub(need) < floor {
        return Err(FrontierError::Insufficient.into());
    }
    let s = seed.as_bytes();
    let mut d = alloc::vec::Vec::with_capacity(4 + 32 + 8 + s.len() + 8 + 32);
    d.extend_from_slice(&9u32.to_le_bytes()); // SystemInstruction::AllocateWithSeed
    d.extend_from_slice(season.key.as_ref());
    d.extend_from_slice(&(s.len() as u64).to_le_bytes());
    d.extend_from_slice(s);
    d.extend_from_slice(&(space as u64).to_le_bytes());
    d.extend_from_slice(program.as_ref());
    solana_program::program::invoke_signed(
        &solana_program::instruction::Instruction {
            program_id: init::SYSTEM,
            accounts: alloc::vec![
                solana_program::instruction::AccountMeta::new(*target.key, false),
                solana_program::instruction::AccountMeta::new_readonly(*season.key, true),
            ],
            data: d,
        },
        &[target.clone(), season.clone()],
        &[&signer.seeds()],
    )?;
    init::move_lamports(fund, target, need)?;
    Ok(need)
}

// ------------------------------------------------------------ terrain encoding

/// Writes the kernel's terrain into the Province's compact block (the
/// pinned encoding of the module doc).
pub fn encode_terrain(t: &ProvinceTerrain, d: &mut [u8]) -> R<()> {
    let mut w = Rw(d);
    let mut passable = 0u64;
    let mut rough = 0u64;
    for i in 0..PROVINCE_TILES {
        let tr = t.terrain[i];
        w.set_u8(PV::TERRAIN + i, tr as u8)?;
        let res = match t.resource[i] {
            None => 0,
            Some(r) => 1 + r as u8,
        };
        w.set_u8(PV::RESOURCE + i, res)?;
        if tr.is_passable() {
            passable |= 1 << i;
        }
        if tr.info().defense_bps < BPS_ONE {
            rough |= 1 << i;
        }
    }
    w.set_arr(PV::SITES, &t.sites)?;
    w.set_u8(PV::SITE_COUNT, t.site_count)?;
    w.set_u64(PV::PASSABLE_MASK, passable)?;
    w.set_u64(PV::ROUGH_MASK, rough)?;
    w.set_u64(PV::ROAD_MASK, 0)?;
    w.set_u64(PV::EXPLORED_MASK, 0)
}

/// The PROVINCE_OPEN terrain digest over the Province's terrain block.
pub fn terrain_digest(province: &[u8]) -> R<[u8; 32]> {
    let block = province.get(TERRAIN_BLOCK).ok_or(BAD_ACCOUNT)?;
    Ok(sha256(&[TERRAIN_DOMAIN, block]))
}

/// The seed of a genesis ring (I-30): `sha256("PSF-RING" ‖ genesis_seed ‖
/// le16(d))`.
pub fn genesis_ring_seed(genesis_seed: &[u8; 32], d: u16) -> [u8; 32] {
    sha256(&[RS::GENESIS_RING_DOMAIN, genesis_seed, &d.to_le_bytes()])
}

/// OpenRing's occupancy condition for rings beyond `g`: some wedge with
/// open sites has `wedge_occupied × 10,000 ≥ θ × wedge_open` on the folded
/// values. A wedge with no open site (or a Frontier never folded) does not
/// qualify: `0 ≥ θ × 0` would open rings with nobody in them (W3-A notes).
pub fn ring_occupancy_met(occupied: &[u32; 6], open: &[u32; 6], theta_bps: u16) -> bool {
    (0..WEDGES).any(|w| {
        open[w] > 0 && occupied[w] as u64 * BPS_ONE as u64 >= theta_bps as u64 * open[w] as u64
    })
}

fn read_u32x6(r: &Ro, off: usize) -> R<[u32; 6]> {
    let mut v = [0u32; 6];
    for (i, x) in v.iter_mut().enumerate() {
        *x = r.u32(off + 4 * i)?;
    }
    Ok(v)
}

fn write_u32x6(w: &mut Rw, off: usize, v: &[u32; 6]) -> R<()> {
    for (i, x) in v.iter().enumerate() {
        w.set_u32(off + 4 * i, *x)?;
    }
    Ok(())
}

// ------------------------------------------------------------ OpenRing

/// 0x20 OpenRing(d): `[payer s,w] [season] [frontier w] [ringseed w]
/// [pfund × 6 r] [system]`. Class D.
///
/// `d == frontier.rings_opened ≤ r_max`; RingSeed absent. `d ≤ g`: the
/// RingSeed is seeded at once with the genesis ring seed (I-30). `d > g`:
/// effective Running, a bell since the last opening, some wedge at `θ`
/// occupancy on the folded values (`θ` early/late by `now − genesis_ts`),
/// every wedge fund above rent by `d × rent(4,096)`; the RingSeed waits
/// for `ring_seed_round(now)` (ConsumeRingSeed). Frontier `rings_opened =
/// d + 1`. Log `RING_OPEN` (Frontier).
pub fn open_ring(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::OpenRing, a, None)?;
    let x = aix::OpenRing::decode(d)?;
    let [payer, season_ai, frontier, ringseed, funds @ .., _system] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let now = prologue::now()?;
    let hdr = prologue::season(
        season_ai,
        p,
        Some(&crate::RULESET_HASH),
        &LAND_STATUS,
        now.ts,
    )?;
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    present_at(frontier, &ctx.frontier(), p, AccountKind::Frontier, hdr.id)?;
    let (g, r_max, genesis_seed, theta_early, theta_late, theta_switch, c) = {
        let sd = season_ai.try_borrow_data()?;
        let r = Ro(&sd);
        (
            r.u8(S::GENESIS_RING)? as u16,
            r.u16(S::R_MAX)?,
            r.arr::<32>(S::GENESIS_SEED)?,
            r.u16(S::THETA_EARLY_BPS)?,
            r.u16(S::THETA_LATE_BPS)?,
            r.u32(S::THETA_SWITCH_SECS)?,
            SeasonClock::read(&sd)?,
        )
    };
    if x.d > r_max {
        return Err(FrontierError::BadData.into());
    }
    let (rings_opened, last_bell, occ, open) = {
        let fd = frontier.try_borrow_data()?;
        let r = Ro(&fd);
        (
            r.u16(FR::RINGS_OPENED)?,
            r.u32(FR::LAST_RING_OPEN_BELL)?,
            read_u32x6(&r, FR::WEDGE_OCCUPIED)?,
            read_u32x6(&r, FR::WEDGE_OPEN)?,
        )
    };
    if x.d < rings_opened {
        return Err(FrontierError::AlreadyDone.into());
    }
    if x.d > rings_opened {
        return Err(FrontierError::TooEarly.into());
    }
    expect_key(ringseed, &ctx.ring_seed(x.d))?;
    if prologue::presence(ringseed, p, AccountKind::RingSeed, hdr.id)? {
        return Err(FrontierError::AlreadyDone.into());
    }
    let now_bell = bell_or_zero(&hdr, now.ts);
    let genesis = x.d <= g;
    if !genesis {
        if hdr.status != S::STATUS_RUNNING {
            return Err(FrontierError::WrongStatus.into());
        }
        if now_bell < last_bell.saturating_add(1) {
            return Err(FrontierError::TooEarly.into());
        }
        let theta = if now.ts.saturating_sub(hdr.genesis_ts) < theta_switch as i64 {
            theta_early
        } else {
            theta_late
        };
        if !ring_occupancy_met(&occ, &open, theta) {
            return Err(FrontierError::Capacity.into());
        }
    }
    // The six wedge funds, canonical and present, in wedge order; a ring
    // beyond g needs each to cover its d provinces.
    let need = (x.d as u64)
        .checked_mul(init::rent(PV::SIZE)?)
        .ok_or(OVERFLOW)?;
    let fund_rent = init::rent(PF::SIZE)?;
    for (w, f) in funds.iter().enumerate() {
        present_at(
            f,
            &ctx.province_fund(w as u8),
            p,
            AccountKind::ProvinceFund,
            hdr.id,
        )?;
        if !genesis && f.lamports().saturating_sub(fund_rent) < need {
            return Err(FrontierError::Insufficient.into());
        }
    }
    let (status, round, seed) = if genesis {
        (
            RS::STATUS_SEEDED,
            0u64,
            genesis_ring_seed(&genesis_seed, x.d),
        )
    } else {
        let bc = c.beacon();
        (
            RS::STATUS_REQUESTED,
            beacon::ring_seed_round(&bc, now.ts, c.seed_margin),
            [0u8; 32],
        )
    };
    init::init_with_seed(
        payer,
        ringseed,
        season_ai,
        &SeasonSigner::new(hdr.id, hdr.bump),
        &ring_seed_seed(x.d),
        RS::SIZE,
        init::rent(RS::SIZE)?,
        p,
    )?;
    {
        let mut rd = ringseed.try_borrow_mut_data()?;
        init_header(&mut rd, AccountKind::RingSeed, hdr.id)?;
        let mut w = Rw(&mut rd);
        w.set_u16(RS::D, x.d)?;
        w.set_u8(RS::STATUS, status)?;
        w.set_u32(RS::OPENED_BELL, now_bell)?;
        w.set_i64(RS::T_OPEN, now.ts)?;
        w.set_u64(RS::ROUND, round)?;
        w.set_arr(RS::SEED, &seed)?;
        w.set_arr(RS::PAYER, payer.key.as_ref())?;
    }
    let mut fd = frontier.try_borrow_mut_data()?;
    {
        let mut w = Rw(&mut fd);
        w.set_u16(FR::RINGS_OPENED, x.d.checked_add(1).ok_or(OVERFLOW)?)?;
        w.set_u32(FR::LAST_RING_OPEN_BELL, now_bell)?;
        w.set_i64(FR::LAST_RING_OPEN_TS, now.ts)?;
    }
    let payload = Buf::<48>::new().i64(now.ts).u64(round).bytes(&seed);
    events::emit(
        Kind::RING_OPEN,
        log_bell(&hdr, now.ts),
        &x.d.to_le_bytes(),
        payload.get()?,
        &mut [Chained {
            entity: EntityKind::Frontier,
            data: &mut fd,
        }],
    )
}

// ------------------------------------------------------------ ConsumeRingSeed

/// 0x21 ConsumeRingSeed(d): K + `[ringseed w]`, data `d, round, sig48,
/// hints`. Class D, top-level only. Status Seeded/Running (I-46); the
/// RingSeed present with status 1 (a seeded one is `AlreadyDone`); `round
/// == ringseed.round` (`WrongRound`); hinted quicknet verification; `seed
/// = seed_of(round, sig)`, status 2. Log `RING_SEED` (chains nothing).
pub fn consume_ring_seed(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::ConsumeRingSeed, a, None)?;
    prologue::top_level(Ix::ConsumeRingSeed)?;
    let x = aix::ConsumeRingSeed::decode(d)?;
    let now = prologue::now()?;
    let hdr = prologue::keeper(a, p, &LAND_STATUS, now.ts)?;
    let [_fee_payer, season_ai, ringseed] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    present_at(
        ringseed,
        &ctx.ring_seed(x.d),
        p,
        AccountKind::RingSeed,
        hdr.id,
    )?;
    let (status, round) = {
        let rd = ringseed.try_borrow_data()?;
        let r = Ro(&rd);
        if r.u16(RS::D)? != x.d {
            return Err(BAD_ACCOUNT);
        }
        (r.u8(RS::STATUS)?, r.u64(RS::ROUND)?)
    };
    if status == RS::STATUS_SEEDED {
        return Err(FrontierError::AlreadyDone.into());
    }
    if status != RS::STATUS_REQUESTED {
        return Err(BAD_ACCOUNT);
    }
    if x.round != round {
        return Err(FrontierError::WrongRound.into());
    }
    let c = {
        let sd = season_ai.try_borrow_data()?;
        SeasonClock::read(&sd)?
    };
    round_is_due(&c, x.round, now.ts)?;
    let sig96 = quick::verify(x.round, &x.sig48, &x.hints)?;
    let seed = quick::seed_of(x.round, &sig96);
    {
        let mut rd = ringseed.try_borrow_mut_data()?;
        let mut w = Rw(&mut rd);
        w.set_arr(RS::SEED, &seed)?;
        w.set_u8(RS::STATUS, RS::STATUS_SEEDED)?;
    }
    let payload = Buf::<40>::new().u64(x.round).bytes(&seed);
    events::emit(
        Kind::RING_SEED,
        log_bell(&hdr, now.ts),
        &x.d.to_le_bytes(),
        payload.get()?,
        &mut [],
    )
}

// ------------------------------------------------------------ OpenProvince

/// 0x22 OpenProvince(P, Q): `[payer s,w] [season] [ringseed w] [pfund(w)
/// w] [province w] [system]`. Class D, top-level only.
///
/// Status Seeded/Running (I-46); `ProvinceCoord::checked(P, Q, r_max)`
/// (CL-04, `BadData`); THE RingSeed of the province's ring, seeded
/// (`SeedNotReady`); the fund of the province's wedge (wedge 0 for the
/// Concord); the Province absent (present: `AlreadyDone`). Effects: the
/// Province is created from the fund (`init_funded`, pre-funding-safe);
/// terrain from `terrain::generate_province(ring_seed, p)`; rings 0–1:
/// every site `reserved` (I-30); rings ≥ 2: the initial camp
/// `camp::place(…, initial = true)` (I-56); `resolved_next =
/// opened_bell = now_bell` (0 before genesis); the fund's `open_sites +=
/// site_count − reserved`, `provinces_opened += 1`, `provinces_funded +=
/// 1`, `spent_total +=`; the RingSeed's `provinces_created += 1`. **No
/// Frontier write** (I-48). Log `PROVINCE_OPEN` (Province).
pub fn open_province(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::OpenProvince, a, None)?;
    prologue::top_level(Ix::OpenProvince)?;
    let x = aix::OpenProvince::decode(d)?;
    let [_payer, season_ai, ringseed, fund, province, _system] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let now = prologue::now()?;
    let hdr = prologue::season(
        season_ai,
        p,
        Some(&crate::RULESET_HASH),
        &LAND_STATUS,
        now.ts,
    )?;
    let r_max = {
        let sd = season_ai.try_borrow_data()?;
        Ro(&sd).u16(S::R_MAX)?
    };
    let (pi, qi) = (x.p as i32, x.q as i32);
    let coord = ProvinceCoord::checked(pi, qi, r_max).map_err(|_| FrontierError::BadData)?;
    let ring = coord.ring() as u16;
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    // THE RingSeed of the ring, seeded.
    expect_key(ringseed, &ctx.ring_seed(ring))?;
    if !prologue::presence(ringseed, p, AccountKind::RingSeed, hdr.id)? {
        return Err(FrontierError::SeedNotReady.into());
    }
    let ring_seed = {
        let rd = ringseed.try_borrow_data()?;
        let r = Ro(&rd);
        if r.u16(RS::D)? != ring {
            return Err(BAD_ACCOUNT);
        }
        if r.u8(RS::STATUS)? != RS::STATUS_SEEDED {
            return Err(FrontierError::SeedNotReady.into());
        }
        r.arr::<32>(RS::SEED)?
    };
    let wedge = fund_wedge(coord);
    present_at(
        fund,
        &ctx.province_fund(wedge),
        p,
        AccountKind::ProvinceFund,
        hdr.id,
    )?;
    expect_key(province, &ctx.province(pi, qi))?;
    if prologue::presence(province, p, AccountKind::Province, hdr.id)? {
        return Err(FrontierError::AlreadyDone.into());
    }
    // Everything the Province holds, computed before any lamport moves.
    crate::heap::trace_checkpoint(200);
    let terrain = generate_province(&ring_seed, coord);
    crate::heap::trace_checkpoint(201);
    let now_bell = bell_or_zero(&hdr, now.ts);
    let day = addr::day_of(now_bell);
    let reserved_ring = ring < camp::CAMP_FIRST_RING as u16;
    let camp_now = camp::place(&ring_seed, coord, &terrain, day, false, true);
    let region = region_of(coord);
    let spent = init_funded_after(
        fund,
        province,
        season_ai,
        &SeasonSigner::new(hdr.id, hdr.bump),
        &province_seed(pi, qi),
        PV::SIZE,
        p,
        0,
    )?;
    crate::heap::trace_checkpoint(202);
    let site_count = terrain.site_count;
    let mut pd = province.try_borrow_mut_data()?;
    init_header(&mut pd, AccountKind::Province, hdr.id)?;
    encode_terrain(&terrain, &mut pd)?;
    let stored_wedge = coord.wedge().unwrap_or(NO_WEDGE);
    {
        let mut w = Rw(&mut pd);
        w.set_i16(PV::P, x.p)?;
        w.set_i16(PV::Q, x.q)?;
        w.set_u16(PV::RING, ring)?;
        w.set_u8(PV::WEDGE, stored_wedge)?;
        w.set_u8(PV::REGION, region)?;
        w.set_u32(PV::RESOLVED_NEXT, now_bell)?;
        w.set_u32(PV::OPENED_BELL, now_bell)?;
        for i in 0..PV::SITES_N {
            let o = PV::site(i);
            let state = if (i as u8) < site_count && reserved_ring {
                SM::STATE_RESERVED
            } else {
                SM::STATE_FREE
            };
            w.set_u8(o + SM::STATE, state)?;
            w.set_u8(o + SM::FACTION, NEUTRAL)?;
            w.set_u32(o + SM::PEND0_BELL, SM::NO_BELL)?;
            w.set_u32(o + SM::PEND1_BELL, SM::NO_BELL)?;
        }
        if let Some(cmp) = camp_now {
            w.set_u8(PV::CAMP + CP::TILE, cmp.tile)?;
            w.set_u8(PV::CAMP + CP::STATE, CP::STATE_PRESENT)?;
            w.set_u32(PV::CAMP + CP::TROOPS, cmp.troops)?;
            w.set_u32(PV::CAMP + CP::GEN, 1)?;
        }
        w.set_u32(
            PV::CAMP + CP::NEXT_CHECK_DAY,
            day.checked_add(1).ok_or(OVERFLOW)?,
        )?;
    }
    let digest = terrain_digest(&pd)?;
    // The wedge fund's live counters (I-48) and the RingSeed's count.
    let open_sites = if reserved_ring { 0 } else { site_count as u32 };
    {
        let mut fdat = fund.try_borrow_mut_data()?;
        let mut w = Rw(&mut fdat);
        w.add_u32(PF::OPEN_SITES, open_sites)?;
        w.add_u32(PF::PROVINCES_OPENED, 1)?;
        w.add_u32(PF::PROVINCES_FUNDED, 1)?;
        w.add_u64(PF::SPENT_TOTAL, spent)?;
    }
    {
        let mut rd = ringseed.try_borrow_mut_data()?;
        let mut w = Rw(&mut rd);
        let n = w.u16(RS::PROVINCES_CREATED)?;
        w.set_u16(RS::PROVINCES_CREATED, n.checked_add(1).ok_or(OVERFLOW)?)?;
    }
    let (camp_tile, camp_troops) = camp_now.map_or((0xFF, 0), |c| (c.tile, c.troops));
    let payload = Buf::<43>::new()
        .u16(ring)
        .u8(stored_wedge)
        .u8(region)
        .bytes(&digest)
        .u8(site_count)
        .u8(camp_tile)
        .u32(camp_troops)
        .u8(reserved_ring as u8);
    events::emit(
        Kind::PROVINCE_OPEN,
        log_bell(&hdr, now.ts),
        &raw_province(pi, qi),
        payload.get()?,
        &mut [Chained {
            entity: EntityKind::Province,
            data: &mut pd,
        }],
    )
}

// ------------------------------------------------------------ FoldOccupancy

/// 0x23 FoldOccupancy(part): `[payer s] [season] [frontier w]` + part 0:
/// the 24 JoinShards of factions 0–2, part 1: of factions 3–5 (read-only,
/// faction then shard order), part 2: the 6 ProvinceFunds (v1.2). Class D.
///
/// Part 0 sums its shards' `holdings` and `holdings_by_wedge` into the
/// accumulators, `fold_bell = now_bell`, `fold_part = 1`. Part 1 needs
/// `fold_part == 1` and the same bell (`FoldStale`), adds its shards and
/// writes `occupied_sites`, `wedge_occupied`, `fold_part = 2`. Part 2
/// needs `fold_part == 2` and the same bell (`FoldStale`) and folds
/// `open_sites`, `wedge_open`, `provinces_opened` from the funds, `fold_part
/// = 0`. Log `FOLD` (Frontier): the occupied fields carry the accumulators
/// after part 0 and the folded values after parts 1 and 2; the open fields
/// the folded values.
pub fn fold_occupancy(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    let x = aix::FoldOccupancy::decode(d)?;
    if x.part >= aix::FoldOccupancy::PARTS {
        return Err(FrontierError::BadData.into());
    }
    let counts = [
        1,
        aix::FoldOccupancy::shards_in(x.part),
        aix::FoldOccupancy::funds_in(x.part),
    ];
    check_accounts(Ix::FoldOccupancy, a, Some(&counts))?;
    let [_payer, season_ai, frontier, rest @ ..] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let now = prologue::now()?;
    let hdr = prologue::season(
        season_ai,
        p,
        Some(&crate::RULESET_HASH),
        &LAND_STATUS,
        now.ts,
    )?;
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    present_at(frontier, &ctx.frontier(), p, AccountKind::Frontier, hdr.id)?;
    let now_bell = bell_or_zero(&hdr, now.ts);
    // Read the part's accounts first (no Frontier borrow held).
    let mut occ = 0u32;
    let mut occ_w = [0u32; 6];
    let mut open = 0u32;
    let mut open_w = [0u32; 6];
    let mut provinces = 0u32;
    match aix::FoldOccupancy::factions_of(x.part) {
        Some((f0, f1)) => {
            let mut i = 0usize;
            for f in f0..f1 {
                for s in 0..JS::SHARDS_PER_FACTION {
                    let ai = rest.get(i).ok_or(FrontierError::TooManyAccounts)?;
                    i += 1;
                    present_at(ai, &ctx.join_shard(f, s), p, AccountKind::JoinShard, hdr.id)?;
                    let jd = ai.try_borrow_data()?;
                    let r = Ro(&jd);
                    occ = occ.checked_add(r.u32(JS::HOLDINGS)?).ok_or(OVERFLOW)?;
                    let by = read_u32x6(&r, JS::HOLDINGS_BY_WEDGE)?;
                    for w in 0..WEDGES {
                        occ_w[w] = occ_w[w].checked_add(by[w]).ok_or(OVERFLOW)?;
                    }
                }
            }
        }
        None => {
            for (w, ai) in rest.iter().enumerate() {
                present_at(
                    ai,
                    &ctx.province_fund(w as u8),
                    p,
                    AccountKind::ProvinceFund,
                    hdr.id,
                )?;
                let fdat = ai.try_borrow_data()?;
                let r = Ro(&fdat);
                let o = r.u32(PF::OPEN_SITES)?;
                open_w[w] = o;
                open = open.checked_add(o).ok_or(OVERFLOW)?;
                provinces = provinces
                    .checked_add(r.u32(PF::PROVINCES_OPENED)?)
                    .ok_or(OVERFLOW)?;
            }
        }
    }
    let mut fd = frontier.try_borrow_mut_data()?;
    let (log_occ, log_occ_w, log_open, log_open_w, log_prov) = {
        let mut w = Rw(&mut fd);
        match x.part {
            0 => {
                w.set_u32(FR::ACC_OCCUPIED, occ)?;
                write_u32x6(&mut w, FR::ACC_WEDGE, &occ_w)?;
                w.set_u32(FR::FOLD_BELL, now_bell)?;
                w.set_u8(FR::FOLD_PART, 1)?;
            }
            1 => {
                if w.u8(FR::FOLD_PART)? != 1 || w.u32(FR::FOLD_BELL)? != now_bell {
                    return Err(FrontierError::FoldStale.into());
                }
                let acc = w.u32(FR::ACC_OCCUPIED)?;
                let acc_w = read_u32x6(&Ro(&*w.0), FR::ACC_WEDGE)?;
                let total = acc.checked_add(occ).ok_or(OVERFLOW)?;
                let mut by = [0u32; 6];
                for i in 0..WEDGES {
                    by[i] = acc_w[i].checked_add(occ_w[i]).ok_or(OVERFLOW)?;
                }
                w.set_u32(FR::OCCUPIED_SITES, total)?;
                write_u32x6(&mut w, FR::WEDGE_OCCUPIED, &by)?;
                w.set_u8(FR::FOLD_PART, 2)?;
            }
            _ => {
                if w.u8(FR::FOLD_PART)? != 2 || w.u32(FR::FOLD_BELL)? != now_bell {
                    return Err(FrontierError::FoldStale.into());
                }
                w.set_u32(FR::OPEN_SITES, open)?;
                write_u32x6(&mut w, FR::WEDGE_OPEN, &open_w)?;
                w.set_u32(FR::PROVINCES_OPENED, provinces)?;
                w.set_u8(FR::FOLD_PART, 0)?;
            }
        }
        let r = Ro(w.0);
        let (o, ow) = if x.part == 0 {
            (r.u32(FR::ACC_OCCUPIED)?, read_u32x6(&r, FR::ACC_WEDGE)?)
        } else {
            (
                r.u32(FR::OCCUPIED_SITES)?,
                read_u32x6(&r, FR::WEDGE_OCCUPIED)?,
            )
        };
        (
            o,
            ow,
            r.u32(FR::OPEN_SITES)?,
            read_u32x6(&r, FR::WEDGE_OPEN)?,
            r.u32(FR::PROVINCES_OPENED)?,
        )
    };
    let mut payload = Buf::<60>::new().u32(log_occ);
    for v in log_occ_w {
        payload = payload.u32(v);
    }
    payload = payload.u32(log_open);
    for v in log_open_w {
        payload = payload.u32(v);
    }
    payload = payload.u32(log_prov);
    events::emit(
        Kind::FOLD,
        log_bell(&hdr, now.ts),
        &[x.part],
        payload.get()?,
        &mut [Chained {
            entity: EntityKind::Frontier,
            data: &mut fd,
        }],
    )
}

// ------------------------------------------------------------ CloseProvince

/// 0x24 CloseProvince(P, Q): `[any s] [season] [province w] [pfund(w) w]`.
/// Class N. Ended with `now ≥ end + 72 h` (`TooEarly` before), or Aborted
/// (at once). The Province (canonical, present) closes into its wedge's
/// fund (wedge 0 for the Concord), whose `provinces_opened −= 1`. Log
/// `CLOSE` (Province) first (v1.3). Re-opening is impossible: OpenProvince
/// refuses outside Seeded/Running (I-46).
pub fn close_province(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::CloseProvince, a, None)?;
    let x = aix::CloseProvince::decode(d)?;
    let [_any, season_ai, province, fund] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let now = prologue::now()?;
    let hdr = season_end_close(season_ai, p, now.ts)?;
    let (pi, qi) = (x.p as i32, x.q as i32);
    let coord = ProvinceCoord::new(pi, qi);
    let ctx: AddrCtx = addr::ctx(&key(season_ai), &p.to_bytes());
    present_at(
        province,
        &ctx.province(pi, qi),
        p,
        AccountKind::Province,
        hdr.id,
    )?;
    present_at(
        fund,
        &ctx.province_fund(fund_wedge(coord)),
        p,
        AccountKind::ProvinceFund,
        hdr.id,
    )?;
    {
        let pd = province.try_borrow_data()?;
        let r = Ro(&pd);
        if r.i16(PV::P)? != x.p || r.i16(PV::Q)? != x.q {
            return Err(BAD_ACCOUNT);
        }
    }
    let bell = log_bell(&hdr, now.ts);
    {
        let lamports = province.lamports();
        let mut pd = province.try_borrow_mut_data()?;
        emit_close(
            AccountKind::Province,
            EntityKind::Province,
            &raw_province(pi, qi),
            &mut pd,
            &key(fund),
            lamports,
            bell,
        )?;
    }
    init::close_to(p, province, fund, &Sink::PoolOwed(fund), bell)?;
    let mut fdat = fund.try_borrow_mut_data()?;
    let mut w = Rw(&mut fdat);
    let n = w.u32(PF::PROVINCES_OPENED)?;
    w.set_u32(PF::PROVINCES_OPENED, n.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_block_round_trips_the_kernel() {
        let seed = [7u8; 32];
        for (pp, qq) in [(0, 0), (1, 0), (2, -1), (-3, 5), (7, 2)] {
            let c = ProvinceCoord::new(pp, qq);
            let t = generate_province(&seed, c);
            let mut d = alloc::vec![0u8; PV::SIZE];
            encode_terrain(&t, &mut d).unwrap();
            let r = Ro(&d);
            for i in 0..PROVINCE_TILES {
                assert_eq!(r.u8(PV::TERRAIN + i).unwrap(), t.terrain[i] as u8);
                let pm = r.u64(PV::PASSABLE_MASK).unwrap() >> i & 1 == 1;
                assert_eq!(pm, t.terrain[i].is_passable());
            }
            assert_eq!(r.u8(PV::SITE_COUNT).unwrap(), t.site_count);
            let d1 = terrain_digest(&d).unwrap();
            d[PV::TERRAIN] ^= 1;
            assert_ne!(terrain_digest(&d).unwrap(), d1);
        }
    }

    #[test]
    fn ring_occupancy_uses_bps() {
        let open = [100, 0, 0, 0, 0, 0];
        assert!(!ring_occupancy_met(&[54, 0, 0, 0, 0, 0], &open, 5_500));
        assert!(ring_occupancy_met(&[55, 0, 0, 0, 0, 0], &open, 5_500));
        // a wedge with no open site never qualifies (an unfolded Frontier)
        assert!(!ring_occupancy_met(&[0; 6], &[0; 6], 6_500));
        let open = [100; 6];
        assert!(!ring_occupancy_met(&[54; 6], &open, 5_500));
        assert!(ring_occupancy_met(&[54, 55, 0, 0, 0, 0], &open, 5_500));
    }

    #[test]
    fn genesis_ring_seed_is_domain_separated() {
        let g = [3u8; 32];
        assert_ne!(genesis_ring_seed(&g, 0), genesis_ring_seed(&g, 1));
        assert_eq!(
            genesis_ring_seed(&g, 2),
            sha256(&[b"PSF-RING", &g, &[2, 0]])
        );
    }
}
