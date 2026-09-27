//! Season lifecycle (M1 contract §5.7).
//!
//! Implemented in wave 2 (W2-A): AnnounceSeason (0x08, upgrade authority
//! only, I-51), CreateSeason (0x01: the Season filled, Frontier, 6 wedge
//! ProvinceFunds, DefencePool; `join_gate`, `reveal_loaded_limit`),
//! InitBeaconLogs (0x09), InitShards (0x02), ConsumeGenesisSeed (0x03) and
//! SetWindowSchedule (0x07). EndSeason, AbortSeason and CloseSeason are
//! stubs until W4-B.
//!
//! Refusal codes the contract leaves open are pinned here (and listed in
//! `docs/frontier/m1/W2-A-NOTES.md`): a present target of an init
//! instruction is `AlreadyDone` (52); CreateSeason before `t_create_min` is
//! `TooEarly`, after the 7-day window `Announce`; parameters that fail
//! `SeasonParams::validate`, name another beacon key than the binary's,
//! name another `program_version` than [`crate::PROGRAM_VERSION`] (integ-W2
//! review), or carry invalid `PayoutParams` are `BadData`. A second
//! SetWindowSchedule is `AlreadyDone` (one change per season, v1.3).

use borsh::BorshDeserialize;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use frontier_abi::ix as aix;
use frontier_abi::layout::AccountKind;
use frontier_abi::log::{EntityKind, Kind, NO_BELL};
use frontier_abi::presets::{self, SeasonParams, SEASON_PARAMS_LEN};
use frontier_abi::prologue::ids::LOADER_V3;
use frontier_abi::tags::Ix;
use permutation_rules::frontier::beacon;
use permutation_rules::frontier::payout::PayoutParams;
use permutation_rules::hash::sha256;

use crate::addr::{self, Seed};
use crate::clock::{self, SeasonClock};
use crate::crypto::quick;
use crate::error::{BAD_ACCOUNT, OVERFLOW};
use crate::events::{self, Buf, Chained};
use crate::init::{self, SeasonSigner};
use crate::layout::world::SeasonCore;
use crate::layout::{
    defence_pool as DP, frontier as FR, init_header, join_shard as JS, province_fund as PF,
    season as S, Ro, Rw,
};
use crate::prologue::{self, check_accounts, expect_key, key};
use crate::{FrontierError, R};

super::stubs!(end_season, abort_season, close_season);

/// The authority stored in the Season must have signed (the flags were
/// checked by `check_accounts`); `Auth` otherwise.
fn check_authority(season: &AccountInfo, authority: &AccountInfo) -> R<SeasonCore> {
    let d = season.try_borrow_data()?;
    let core = SeasonCore::read(&d)?;
    if core.authority != key(authority) {
        return Err(FrontierError::Auth.into());
    }
    Ok(core)
}

/// The upgrade authority of `program` (I-51): its LoaderV3 Program account
/// names the ProgramData, which stores `Option<authority>` at offset 12.
/// `Auth` when the program is immutable (no authority).
fn upgrade_authority(
    program: &Pubkey,
    program_ai: &AccountInfo,
    pd_ai: &AccountInfo,
) -> R<[u8; 32]> {
    let loader = Pubkey::new_from_array(LOADER_V3);
    if program_ai.key != program || *program_ai.owner != loader || *pd_ai.owner != loader {
        return Err(BAD_ACCOUNT);
    }
    {
        let d = program_ai.try_borrow_data()?;
        let r = Ro(&d);
        // UpgradeableLoaderState::Program { programdata_address }
        if r.u32(0)? != 2 || r.arr::<32>(4)? != key(pd_ai) {
            return Err(FrontierError::BadAddress.into());
        }
    }
    let d = pd_ai.try_borrow_data()?;
    let r = Ro(&d);
    // UpgradeableLoaderState::ProgramData { slot, upgrade_authority_address }
    if r.u32(0)? != 3 {
        return Err(BAD_ACCOUNT);
    }
    if r.u8(12)? != 1 {
        return Err(FrontierError::Auth.into());
    }
    r.arr(13)
}

/// 0x08 AnnounceSeason: `[authority s,w] [season w (PDA)] [program r]
/// [programdata r] [system]`, data `id, params_hash, t_create_min, bond`.
pub fn announce_season(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::AnnounceSeason, a, None)?;
    let x = aix::AnnounceSeason::decode(d)?;
    let [authority, season_ai, program_ai, pd_ai, _system] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let now = prologue::now()?;
    // The program's upgrade authority, read from its ProgramData (I-51).
    if upgrade_authority(p, program_ai, pd_ai)? != key(authority) {
        return Err(FrontierError::Auth.into());
    }
    // The canonical Season PDA, found once here; its bump is stored.
    let id = addr::season_seed_id(x.id);
    let (pda, bump) = Pubkey::find_program_address(&[addr::SEASON_PREFIX, &id], p);
    if *season_ai.key != pda {
        return Err(FrontierError::BadAddress.into());
    }
    // A Season is never closed (CloseSeason leaves a tombstone), so a
    // present one means the id was used.
    if !init::is_absent(season_ai) {
        return Err(FrontierError::Announce.into());
    }
    let min = now
        .ts
        .checked_add(presets::MIN_ANNOUNCE_LEAD_SECS)
        .ok_or(OVERFLOW)?;
    if x.t_create_min < min || x.bond < presets::MIN_CREATION_BOND {
        return Err(FrontierError::Announce.into());
    }
    let total = init::rent(S::SIZE)?.checked_add(x.bond).ok_or(OVERFLOW)?;
    init::init_pda(
        authority,
        season_ai,
        &SeasonSigner::new(x.id, bump),
        S::SIZE,
        total,
        p,
    )?;
    let mut sd = season_ai.try_borrow_mut_data()?;
    init_header(&mut sd, AccountKind::Season, x.id)?;
    {
        let mut w = Rw(&mut sd);
        w.set_u8(S::STATUS, S::STATUS_ANNOUNCED)?;
        w.set_u8(S::BUMP, bump)?;
        w.set_arr(S::AUTHORITY, authority.key.as_ref())?;
        w.set_arr(S::PARAMS_HASH, &x.params_hash)?;
        w.set_i64(S::T_CREATE_MIN, x.t_create_min)?;
        w.set_i64(S::ANNOUNCED_TS, now.ts)?;
        w.set_u64(S::CREATION_BOND, x.bond)?;
        w.set_u32(S::WINDOW_FROM_BELL, S::WINDOW_NONE)?;
    }
    let key8 = x.id.to_le_bytes();
    let payload = Buf::<48>::new()
        .bytes(&x.params_hash)
        .i64(x.t_create_min)
        .u64(x.bond);
    events::emit(
        Kind::ANNOUNCE,
        NO_BELL,
        &key8,
        payload.get()?,
        &mut [Chained {
            entity: EntityKind::Season,
            data: &mut sd,
        }],
    )
}

/// Writes the Season fields CreateSeason fills.
fn write_params(
    w: &mut Rw,
    p: &SeasonParams,
    genesis_round: u64,
    genesis_ts: i64,
    now: i64,
    payout_hash: &[u8; 32],
) -> R<()> {
    w.set_u8(S::STATUS, S::STATUS_CREATED)?;
    w.set_u8(S::REGIONS, p.regions)?;
    w.set_u8(S::GENESIS_RING, p.genesis_ring)?;
    w.set_u16(S::R_MAX, p.r_max)?;
    w.set_u8(S::OFFICE_TERMS_PER_WALLET, p.office_terms_per_wallet)?;
    w.set_u8(S::POSTURES_ENABLED, p.postures_enabled)?;
    w.set_arr(S::RULESET_HASH, &crate::RULESET_HASH)?;
    w.set_u16(S::RULES_VERSION, presets::RULES_VERSION)?;
    w.set_u16(S::PROGRAM_VERSION, p.program_version)?;
    w.set_u32(S::BELL_SECS, p.bell_secs)?;
    w.set_i64(S::GENESIS_TS, genesis_ts)?;
    w.set_i64(S::CREATED_TS, now)?;
    w.set_u32(S::JOIN_CLOSE_BELL, p.join_close_bell)?;
    w.set_u32(S::END_BELL, p.end_bell)?;
    w.set_i64(S::DRAND_GENESIS, p.drand_genesis)?;
    w.set_u32(S::DRAND_PERIOD, p.drand_period)?;
    w.set_u8(S::NETWORK, p.network)?;
    w.set_arr(S::QUICKNET_PK_HASH, &p.quicknet_pk_hash)?;
    w.set_u32(S::REVEAL_WINDOW, p.reveal_window)?;
    w.set_u32(S::SEED_MARGIN, p.seed_margin)?;
    w.set_u32(S::WINDOW_NEXT, p.reveal_window)?;
    w.set_u32(S::WINDOW_FROM_BELL, S::WINDOW_NONE)?;
    w.set_u64(S::GENESIS_ROUND, genesis_round)?;
    w.set_arr(S::GENESIS_SEED, &[0; 32])?;
    w.set_u32(S::ARCHIVE_AFTER, p.archive_after)?;
    w.set_u8(S::MIN_LEAD, p.min_lead)?;
    w.set_u8(S::MAX_LEAD, p.max_lead)?;
    w.set_u8(S::TRANSIT_SLOTS, p.transit_slots)?;
    w.set_u64(S::MARCH_FEE, p.march_fee)?;
    w.set_u64(S::SEAL_BOND, p.seal_bond)?;
    w.set_u32(S::MIN_REVEAL_PRIORITY_MILLI, p.min_reveal_priority_milli)?;
    w.set_u32(S::REVEAL_CU_LIMIT, p.reveal_cu_limit)?;
    w.set_u16(S::BUCKET_RATE_PER_H, p.bucket_rate_per_h)?;
    w.set_u16(S::BUCKET_BURST, p.bucket_burst)?;
    w.set_u32(S::DEFENCE_CAP_MILLI, p.defence_cap_milli)?;
    w.set_u8(S::LATENESS_SLOTS, p.lateness_slots)?;
    w.set_u16(S::THETA_EARLY_BPS, p.theta_early_bps)?;
    w.set_u16(S::THETA_LATE_BPS, p.theta_late_bps)?;
    w.set_u32(S::THETA_SWITCH_SECS, p.theta_switch_secs)?;
    w.set_u16(S::RESERVE_BPS, p.reserve_bps)?;
    w.set_u16(S::EXTRA_FREE_BPS, p.extra_free_bps)?;
    w.set_u32(S::CLASH_CLOSE_GRACE, p.clash_close_grace)?;
    w.set_u32(S::CAMP_REGROW_BELLS, p.camp_regrow_bells)?;
    w.set_arr(S::PAYOUT_PARAMS_HASH, payout_hash)?;
    w.set_arr(S::SHADE_AUDITOR, &[0; 32])?;
    w.set_u32(S::DORMANT_AFTER_SECS, p.dormant_after_secs)?;
    w.set_u32(S::RELEASE_AFTER_SECS, p.release_after_secs)?;
    w.set_u64(S::PFUND_INITIAL, p.pfund_initial)?;
    w.set_u64(S::DPOOL_INITIAL, p.dpool_initial)?;
    w.set_u32(S::REVEAL_LOADED_LIMIT, p.reveal_loaded_limit)?;
    w.set_arr(S::JOIN_GATE, &p.join_gate)
}

/// `payout_params_hash = sha256(PayoutParams borsh)` (pinned by W2-A; the
/// verifier recomputes it from CreateSeason's data).
pub fn payout_params_hash(payout_borsh: &[u8]) -> [u8; 32] {
    sha256(&[payout_borsh])
}

/// Each wedge fund's share of `pfund_initial` (the remainder of the
/// division by 6 is not funded).
pub const fn wedge_share(pfund_initial: u64) -> u64 {
    pfund_initial / PF::WEDGES as u64
}

/// 0x01 CreateSeason: `[authority s,w] [season w] [frontier w] [pfund × 6
/// w] [dpool w] [system]`, data `SeasonParams ‖ PayoutParams (borsh)`.
pub fn create_season(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::CreateSeason, a, None)?;
    let x = aix::CreateSeason::decode(d)?;
    let [authority, season_ai, frontier_ai, pf0, pf1, pf2, pf3, pf4, pf5, dpool_ai, _system] = a
    else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let funds = [pf0, pf1, pf2, pf3, pf4, pf5];
    let now = prologue::now()?;
    let hdr = prologue::season(season_ai, p, None, &[S::STATUS_ANNOUNCED], now.ts)?;
    let core = check_authority(season_ai, authority)?;
    let (t_create_min, params_hash) = {
        let sd = season_ai.try_borrow_data()?;
        let r = Ro(&sd);
        (r.i64(S::T_CREATE_MIN)?, r.arr::<32>(S::PARAMS_HASH)?)
    };
    if now.ts < t_create_min {
        return Err(FrontierError::TooEarly.into());
    }
    let window_end = t_create_min
        .checked_add(presets::CREATE_WINDOW_SECS)
        .ok_or(OVERFLOW)?;
    if now.ts >= window_end {
        return Err(FrontierError::Announce.into());
    }
    let raw: &[u8; SEASON_PARAMS_LEN] = d
        .get(1..1 + SEASON_PARAMS_LEN)
        .and_then(|s| s.try_into().ok())
        .ok_or(FrontierError::BadData)?;
    if presets::params_hash(raw, x.payout) != params_hash {
        return Err(FrontierError::Announce.into());
    }
    let prm = x.params;
    if prm.validate().is_err()
        || prm.quicknet_pk_hash != crate::QUICKNET_PK_HASH
        || prm.program_version != crate::PROGRAM_VERSION
    {
        return Err(FrontierError::BadData.into());
    }
    let payout = PayoutParams::try_from_slice(x.payout).map_err(|_| FrontierError::BadData)?;
    if payout.validate_for_season().is_err() {
        return Err(FrontierError::BadData.into());
    }
    // Targets at their canonical addresses (absence is checked by init).
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    expect_key(frontier_ai, &ctx.frontier())?;
    for (w, f) in funds.iter().enumerate() {
        expect_key(f, &ctx.province_fund(w as u8))?;
    }
    expect_key(dpool_ai, &ctx.defence_pool())?;

    let genesis_round = clock::genesis_round(
        prm.drand_genesis,
        prm.drand_period,
        t_create_min,
        prm.seed_margin,
    );
    let genesis_ts = clock::genesis_ts(prm.drand_genesis, prm.drand_period, genesis_round);
    let signer = SeasonSigner::new(hdr.id, hdr.bump);
    let share = wedge_share(prm.pfund_initial);

    // Accounts (every CPI before any Season write: the Season signs them).
    init::init_with_seed(
        authority,
        frontier_ai,
        season_ai,
        &signer,
        &frontier_abi::addr::frontier_seed(),
        FR::SIZE,
        init::rent(FR::SIZE)?,
        p,
    )?;
    let fund_total = init::rent(PF::SIZE)?.checked_add(share).ok_or(OVERFLOW)?;
    for (w, f) in funds.iter().enumerate() {
        init::init_with_seed(
            authority,
            f,
            season_ai,
            &signer,
            &frontier_abi::addr::province_fund_seed(w as u8),
            PF::SIZE,
            fund_total,
            p,
        )?;
    }
    let pool_total = init::rent(DP::SIZE)?
        .checked_add(prm.dpool_initial)
        .ok_or(OVERFLOW)?;
    init::init_with_seed(
        authority,
        dpool_ai,
        season_ai,
        &signer,
        &frontier_abi::addr::defence_pool_seed(),
        DP::SIZE,
        pool_total,
        p,
    )?;

    {
        let mut fd = frontier_ai.try_borrow_mut_data()?;
        init_header(&mut fd, AccountKind::Frontier, hdr.id)?;
    }
    for (w, f) in funds.iter().enumerate() {
        let mut fd = f.try_borrow_mut_data()?;
        init_header(&mut fd, AccountKind::ProvinceFund, hdr.id)?;
        let mut r = Rw(&mut fd);
        r.set_u8(PF::WEDGE, w as u8)?;
        r.set_u64(PF::FUNDED_TOTAL, share)?;
    }
    {
        let mut pd = dpool_ai.try_borrow_mut_data()?;
        init_header(&mut pd, AccountKind::DefencePool, hdr.id)?;
        let mut r = Rw(&mut pd);
        r.set_u64(DP::PER_BELL_REGION_CAP, prm.per_bell_region_cap)?;
        r.set_u64(DP::PER_KEEPER_DAY_CAP, prm.per_keeper_day_cap)?;
    }

    let payout_hash = payout_params_hash(x.payout);
    let mut sd = season_ai.try_borrow_mut_data()?;
    write_params(
        &mut Rw(&mut sd),
        &prm,
        genesis_round,
        genesis_ts,
        now.ts,
        &payout_hash,
    )?;
    let key8 = core.id.to_le_bytes();
    let payload = Buf::<124>::new()
        .bytes(&params_hash)
        .u64(genesis_round)
        .i64(genesis_ts)
        .bytes(&crate::RULESET_HASH)
        .bytes(&prm.quicknet_pk_hash)
        .u32(prm.reveal_window)
        .u32(prm.seed_margin)
        .u16(prm.r_max)
        .u16(prm.program_version);
    events::emit(
        Kind::SEASON_CREATED,
        NO_BELL,
        &key8,
        payload.get()?,
        &mut [Chained {
            entity: EntityKind::Season,
            data: &mut sd,
        }],
    )
}

/// Present program account at a target address of an init instruction:
/// `AlreadyDone` (a repeat), anything else not absent: `BadAccount`.
fn must_be_absent(ai: &AccountInfo, p: &Pubkey, kind: AccountKind, season_id: u64) -> R<()> {
    if prologue::presence(ai, p, kind, season_id)? {
        return Err(FrontierError::AlreadyDone.into());
    }
    Ok(())
}

const EARLY: [u8; 3] = [S::STATUS_CREATED, S::STATUS_SEEDED, S::STATUS_RUNNING];

/// 0x09 InitBeaconLogs: `[authority s,w] [season] [blog × 16 w] [system]`.
pub fn init_beacon_logs(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::InitBeaconLogs, a, None)?;
    aix::InitBeaconLogs::decode(d)?;
    let (authority, season_ai, logs) = match a {
        [au, s, rest @ ..] if rest.len() == 17 => (au, s, &rest[..16]),
        _ => return Err(FrontierError::TooManyAccounts.into()),
    };
    let now = prologue::now()?;
    let hdr = prologue::season(season_ai, p, Some(&crate::RULESET_HASH), &EARLY, now.ts)?;
    check_authority(season_ai, authority)?;
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    let signer = SeasonSigner::new(hdr.id, hdr.bump);
    let total = init::rent(crate::layout::beacon_log::SIZE)?;
    for (r, log) in logs.iter().enumerate() {
        let region = r as u8;
        expect_key(log, &ctx.beacon_log(region))?;
        must_be_absent(log, p, AccountKind::BeaconLog, hdr.id)?;
        init::init_with_seed(
            authority,
            log,
            season_ai,
            &signer,
            &frontier_abi::addr::beacon_log_seed(region),
            crate::layout::beacon_log::SIZE,
            total,
            p,
        )?;
        let mut ld = log.try_borrow_mut_data()?;
        init_header(&mut ld, AccountKind::BeaconLog, hdr.id)?;
        Rw(&mut ld).set_u8(crate::layout::beacon_log::REGION, region)?;
    }
    Ok(())
}

/// 0x02 InitShards(f): `[authority s,w] [season] [js × 8 w] [system]`.
pub fn init_shards(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::InitShards, a, None)?;
    let x = aix::InitShards::decode(d)?;
    if x.faction >= JS::FACTIONS {
        return Err(FrontierError::BadData.into());
    }
    let (authority, season_ai, shards) = match a {
        [au, s, rest @ ..] if rest.len() == 9 => (au, s, &rest[..8]),
        _ => return Err(FrontierError::TooManyAccounts.into()),
    };
    let now = prologue::now()?;
    let hdr = prologue::season(season_ai, p, Some(&crate::RULESET_HASH), &EARLY, now.ts)?;
    check_authority(season_ai, authority)?;
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    let signer = SeasonSigner::new(hdr.id, hdr.bump);
    let total = init::rent(JS::SIZE)?;
    for (s, shard) in shards.iter().enumerate() {
        let s = s as u8;
        expect_key(shard, &ctx.join_shard(x.faction, s))?;
        must_be_absent(shard, p, AccountKind::JoinShard, hdr.id)?;
        let seed: Seed = frontier_abi::addr::join_shard_seed(x.faction, s);
        init::init_with_seed(
            authority,
            shard,
            season_ai,
            &signer,
            &seed,
            JS::SIZE,
            total,
            p,
        )?;
        let mut sd = shard.try_borrow_mut_data()?;
        init_header(&mut sd, AccountKind::JoinShard, hdr.id)?;
        let mut w = Rw(&mut sd);
        w.set_u8(JS::FACTION, x.faction)?;
        w.set_u8(JS::SHARD, s)?;
    }
    Ok(())
}

/// In a `test-beacon` build the key is public, so anyone could sign a
/// future round: refuse a round the chain's Clock has not reached
/// (`TooEarly`). Real quicknet rounds cannot exist before their time.
pub(crate) fn round_is_due(_c: &SeasonClock, _round: u64, _now: i64) -> R<()> {
    #[cfg(feature = "test-beacon")]
    if _now < _c.round_time(_round) {
        return Err(FrontierError::TooEarly.into());
    }
    Ok(())
}

/// 0x03 ConsumeGenesisSeed: K with the season writable; data `round,
/// sig48, hints`. Status Created; `round == genesis_round`; hinted
/// quicknet verification; `genesis_seed = seed_of(round, sig)`, status
/// Seeded.
pub fn consume_genesis_seed(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::ConsumeGenesisSeed, a, None)?;
    prologue::top_level(Ix::ConsumeGenesisSeed)?;
    let x = aix::ConsumeGenesisSeed::decode(d)?;
    let now = prologue::now()?;
    let hdr = prologue::keeper(a, p, &[S::STATUS_CREATED], now.ts)?;
    let [_fee_payer, season_ai] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let (genesis_round, c) = {
        let sd = season_ai.try_borrow_data()?;
        (Ro(&sd).u64(S::GENESIS_ROUND)?, SeasonClock::read(&sd)?)
    };
    if x.round != genesis_round {
        return Err(FrontierError::WrongRound.into());
    }
    round_is_due(&c, x.round, now.ts)?;
    let sig96 = quick::verify(x.round, &x.sig48, &x.hints)?;
    let seed = quick::seed_of(x.round, &sig96);
    let mut sd = season_ai.try_borrow_mut_data()?;
    {
        let mut w = Rw(&mut sd);
        w.set_arr(S::GENESIS_SEED, &seed)?;
        w.set_u8(S::STATUS, S::STATUS_SEEDED)?;
    }
    let key8 = hdr.id.to_le_bytes();
    let payload = Buf::<40>::new().u64(x.round).bytes(&seed);
    events::emit(
        Kind::GENESIS_SEED,
        hdr.bell(now.ts).unwrap_or(NO_BELL),
        &key8,
        payload.get()?,
        &mut [Chained {
            entity: EntityKind::Season,
            data: &mut sd,
        }],
    )
}

/// 0x07 SetWindowSchedule: `[authority s] [season w]`, data `window,
/// from_bell`. `600 ≤ window ≤ 1,800` and `from_bell < end_bell` (which
/// also refuses the `u32::MAX` "no change" sentinel) else `BadData`;
/// `from_bell ≥ now_bell + 144` (`TooEarly`); **one change per season**
/// (contract v1.3 §5.7): once `window_from_bell` is set, any further call
/// is `AlreadyDone`. `reveal_window` is never rewritten, so `W(b)` of a
/// bell never changes after the bell exists (a fold would re-read `W` for
/// bells whose seed caches, gathers or settlements may still be pending,
/// integ-W2 review).
pub fn set_window_schedule(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::SetWindowSchedule, a, None)?;
    let x = aix::SetWindowSchedule::decode(d)?;
    let [authority, season_ai] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let now = prologue::now()?;
    let hdr = prologue::season(season_ai, p, Some(&crate::RULESET_HASH), &EARLY, now.ts)?;
    check_authority(season_ai, authority)?;
    if !beacon::window_valid(x.window) || x.from_bell >= hdr.end_bell {
        return Err(FrontierError::BadData.into());
    }
    let now_bell = hdr.bell(now.ts).unwrap_or(0);
    if !beacon::window_change_allowed(now_bell, x.from_bell) {
        return Err(FrontierError::TooEarly.into());
    }
    let mut sd = season_ai.try_borrow_mut_data()?;
    {
        let mut w = Rw(&mut sd);
        if w.u32(S::WINDOW_FROM_BELL)? != S::WINDOW_NONE {
            return Err(FrontierError::AlreadyDone.into());
        }
        w.set_u32(S::WINDOW_NEXT, x.window)?;
        w.set_u32(S::WINDOW_FROM_BELL, x.from_bell)?;
    }
    let key8 = hdr.id.to_le_bytes();
    let payload = Buf::<8>::new().u32(x.window).u32(x.from_bell);
    events::emit(
        Kind::WINDOW,
        hdr.bell(now.ts).unwrap_or(NO_BELL),
        &key8,
        payload.get()?,
        &mut [Chained {
            entity: EntityKind::Season,
            data: &mut sd,
        }],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wedge_shares_and_payout_hash() {
        assert_eq!(wedge_share(18_000_000_000), 3_000_000_000);
        assert_eq!(wedge_share(7), 1);
        let b = PayoutParams::REV3.to_borsh();
        // Fixed vectors (integ-W2 review: not the function body): the
        // REV3 borsh is `frontier-abi/vectors/presets.json`'s
        // `payout_params_rev3_borsh`, and its hash is pinned here for the
        // verifier (`sha256` of those 28 bytes, computed independently).
        let hex = |v: &[u8]| v.iter().map(|x| format!("{x:02x}")).collect::<String>();
        assert_eq!(
            hex(&b),
            "28230000f401000005000000000000001c2500006900000000000000"
        );
        assert_eq!(
            hex(&payout_params_hash(&b)),
            "91dc5b2b7701b836c77dfbda55029e74af0152c38ead9370982b44797a77bb87"
        );
        let back = PayoutParams::try_from_slice(&b).unwrap();
        assert_eq!(back, PayoutParams::REV3);
        assert!(back.validate_for_season().is_ok());
        assert!(PayoutParams::REV2.validate_for_season().is_err());
    }

    /// The M1 presets name the key this binary verifies against (quicknet
    /// in a normal build; a test-beacon season must set the test key's
    /// hash).
    #[test]
    fn presets_name_the_binarys_key() {
        let p = presets::M1_LOCAL_7D;
        assert!(p.validate().is_ok());
        if cfg!(feature = "test-beacon") {
            assert_ne!(p.quicknet_pk_hash, crate::QUICKNET_PK_HASH);
        } else {
            assert_eq!(p.quicknet_pk_hash, crate::QUICKNET_PK_HASH);
        }
    }
}
