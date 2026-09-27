//! Citizens and land (M1 contract §5.6, §5.9, I-29, I-40, I-47, I-49,
//! I-51): Join (0x30), SetSession (0x31), SetVigil (0x32), FileTicket
//! (0x33), SettleTicket (0x34), ReleaseDormant (0x35), CloseHolding
//! (0x36) and CloseCitizen (0x37). Implemented by W3-A, with the on-chain
//! **player prologue** ([`player`]) every player instruction starts with.
//!
//! **Ticket cohorts (I-47).** FileTicket counts the ticket in the cohort
//! for `now_bell` of each distinct Province of its sites (`filed += 1`,
//! reusing a free record, else `CohortFull`). The SettleTicket that ends
//! the ticket (fresh, displace, exhausted or expired) marks it settled in
//! every one of them. A provisional holding becomes final only at
//! `final_ts = round_time(S) + 600` **and** once its cohort is closed (the
//! lazy flip of the resident prologue, `frontier_abi::prologue::
//! finality_due`), so inside the cohort a higher score displaces with no
//! time condition.
//!
//! **Pinned rules the contract leaves to the implementer (W3-A notes §4):**
//!
//! - **Ticket score** `= rng::rand(S, "site", le32(P) ‖ le32(Q) ‖ site ‖
//!   le64(citizen_tag))` ([`ticket_score`]); a tie goes to the lower
//!   `citizen_tag`.
//! - **Site generations:** every founding bumps the site's `gen` (the
//!   first holding of a site has `gen = 1`), so hosts of a released or
//!   displaced holding are stranded.
//! - **A founded holding** is `Holding::found(now, day, 1)` with
//!   `production = catalog::base_production(Hamlet)` (the simulator's
//!   `found`; doctrines have no production multipliers, O5), food upkeep
//!   0, the starter kit credited, walls 0; the queue is written as zeros.
//!   The site mirror's `shield_until_bell` is the first bell starting at
//!   or after `shield_until`; its garrison starts at 0.
//! - **The ticket's site count** is the number of leading non-zero
//!   `ticket_sites` entries (an all-zero entry is the Concord's site 0,
//!   never a ticket site).
//! - **Vigil weekly rule** (the Citizen stores no request time): a change
//!   is refused (`Cooldown`) until `vigil_from_ts + 6 days`, which is never
//!   earlier than the kernel's `last_request + 7 days`.
//! - **Refusal codes:** a present Citizen at Join, a repeated SettleTicket
//!   `k < ticket_next` are `AlreadyDone`; a ticket or holding state that
//!   forbids the call is `TicketState`; a SettleTicket without the
//!   displaced accounts it needs, or with ones it does not use, is
//!   `TooManyAccounts`; FileTicket outside the wedge rule, duplicate
//!   sites, a site index past the Province's `site_count` or a ring not
//!   yet opened is `BadData`; ReleaseDormant of a second or third holding
//!   or before `release_after` is `NotDormant`, with transits
//!   `HasTransits` (both 46); SetVigil within the weekly limit is
//!   `Cooldown` (29); the season-end closes before `end + 72 h` are
//!   `TooEarly`.

use alloc::vec::Vec;

use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use frontier_abi::addr::{citizen_seed, holding_seed};
use frontier_abi::ix as aix;
use frontier_abi::layout::player::{holding_ref as HR, ticket_site as TS, transit as T};
use frontier_abi::layout::AccountKind;
use frontier_abi::log::{divert_reason, settle_outcome, EntityKind, Kind};
use frontier_abi::prologue::{self as ap, PlayerCtx};
use frontier_abi::tags::Ix;
use permutation_rules::frontier::beacon;
use permutation_rules::frontier::catalog;
use permutation_rules::frontier::clash::NEUTRAL;
use permutation_rules::frontier::geometry::ProvinceCoord;
use permutation_rules::frontier::holding::{Holding, Resource, Tier, RESOURCES};
use permutation_rules::frontier::siege;
use permutation_rules::rng;

use crate::addr::{self, AddrCtx};
use crate::clock::SeasonClock;
use crate::error::{BAD_ACCOUNT, OVERFLOW};
use crate::events::{self, Buf, Chained};
use crate::init::{self, SeasonSigner, Sink};
use crate::layout::beacon::{archive_archived, archive_entry_of, archive_key, Anchor, Cache};
use crate::layout::player::accrual as AC;
use crate::layout::{
    citizen as C, cohort as CO, frontier as FR, holding as H, init_header, join_shard as JS,
    province as PV, season as S, site as SM, Ro, Rw,
};
use crate::prologue::{self, check_accounts, expect_key, key, view};
use crate::{FrontierError, R};

use super::map::{emit_close, log_bell, present_at, raw_province, season_end_close};

/// Seconds a session may run from its setting (30 days).
const MAX_SESSION_SECS: i64 = C::MAX_SESSION_SECS;
/// Minutes in a day (SetVigil `start_min < 1,440`).
const DAY_MINUTES: u16 = 1_440;
/// The Citizen's weekly vigil rule, measured from `vigil_from_ts`.
pub const VIGIL_AFTER_FROM_SECS: i64 = 6 * 86_400;
/// Bells after `ticket_bell` at which a ticket expires (I-47).
const COHORT_BELLS: u32 = PV::COHORT_BELLS;

// ------------------------------------------------------------ player prologue

/// The player prologue (§5.6) over `[actor, payer, season, citizen]`:
/// the Season structurally and at its recomputed PDA (`BadAddress`, H2),
/// its effective status Running, the ruleset; the Citizen present at its
/// canonical address with `actor == wallet` (or the unexpired session when
/// `session_ok`); the action bucket; `bell < end_bell`. The account flags
/// were checked by `check_accounts`. Writes the Citizen's bucket.
pub(crate) fn player(p: &Pubkey, a: &[AccountInfo], now: i64, session_ok: bool) -> R<PlayerCtx> {
    let [actor, payer, season_ai, citizen, ..] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let start = {
        let sd = season_ai.try_borrow_data()?;
        let sv = view(season_ai, &sd);
        let s = ap::read_season(&sv, p.as_array(), now)?;
        if addr::season_pda(s.id, s.bump, p.as_array()) != *season_ai.key.as_array() {
            return Err(FrontierError::BadAddress.into());
        }
        let cd = citizen.try_borrow_data()?;
        let views = [view(actor, &[]), view(payer, &[]), sv, view(citizen, &cd)];
        ap::player_prologue(&views, p.as_array(), &crate::RULESET_HASH, now, session_ok)?
    };
    let mut cd = citizen.try_borrow_mut_data()?;
    let ctx = start.finish(&mut cd, now)?;
    Rw(&mut cd).set_i64(C::LAST_ACTION_TS, now)?;
    Ok(ctx)
}

/// The Citizen's seed tag (key of its records).
fn tag15_of(citizen: &[u8]) -> R<[u8; 15]> {
    let wallet: [u8; 32] = Ro(citizen).arr(C::WALLET)?;
    Ok(addr::citizen_tag15(&wallet))
}

// ------------------------------------------------------------ pure rules

/// The ticket score (pinned, module doc): `rng::rand(S, "site", le32(P) ‖
/// le32(Q) ‖ site ‖ le64(citizen_tag))`.
pub fn ticket_score(seed: &[u8; 32], p: i32, q: i32, site: u8, citizen_tag: u64) -> u64 {
    let mut id = [0u8; 17];
    id[..4].copy_from_slice(&p.to_le_bytes());
    id[4..8].copy_from_slice(&q.to_le_bytes());
    id[8] = site;
    id[9..].copy_from_slice(&citizen_tag.to_le_bytes());
    rng::rand(seed, b"site", &id)
}

/// Whether a challenger with `(score, tag)` displaces a provisional
/// holding of the same cohort held with `(held_score, held_tag)`: the
/// higher score wins, a tie goes to the lower tag.
pub const fn displaces(score: u64, tag: u64, held_score: u64, held_tag: u64) -> bool {
    score > held_score || (score == held_score && tag < held_tag)
}

/// The FileTicket wedge rule: the citizen's own wedge, or — when the
/// folded Frontier shows no free site left in it — the outermost open ring
/// of an adjacent wedge.
pub fn wedge_allowed(
    faction: u8,
    site_wedge: u8,
    site_ring: u32,
    own_open: u32,
    own_occupied: u32,
    rings_opened: u16,
) -> bool {
    if site_wedge == faction {
        return true;
    }
    let adjacent = site_wedge == (faction + 1) % 6 || site_wedge == (faction + 5) % 6;
    own_open.saturating_sub(own_occupied) == 0
        && adjacent
        && rings_opened > 0
        && site_ring == rings_opened as u32 - 1
}

/// A ticket site `(P, Q, site)`.
type SiteKey = (i16, i16, u8);

/// The ticket's sites: `(P, Q, site)` of the leading non-zero entries.
fn ticket_sites(citizen: &[u8]) -> R<([SiteKey; 3], usize)> {
    let r = Ro(citizen);
    let mut out = [(0i16, 0i16, 0u8); 3];
    let mut n = 0;
    for (i, s) in out.iter_mut().enumerate() {
        let o = C::TICKET_SITES + i * TS::SIZE;
        let e = (r.i16(o + TS::P)?, r.i16(o + TS::Q)?, r.u8(o + TS::SITE)?);
        if e == (0, 0, 0) {
            break;
        }
        *s = e;
        n = i + 1;
    }
    Ok((out, n))
}

/// Distinct provinces of sites, first-seen order.
fn distinct_provinces(sites: &[(i16, i16, u8)]) -> Vec<(i16, i16)> {
    let mut v: Vec<(i16, i16)> = Vec::with_capacity(3);
    for s in sites {
        if !v.contains(&(s.0, s.1)) {
            v.push((s.0, s.1));
        }
    }
    v
}

/// The cohort table (8 × 8 B at `TICKET_COHORTS`) as one slice, read in a
/// single pass (the per-field bounds-checked reads cost ≈ 1k CU a
/// Province, W3-A notes §2).
fn cohort_table(province: &mut [u8]) -> R<&mut [u8]> {
    province
        .get_mut(PV::TICKET_COHORTS..PV::TICKET_COHORTS + PV::COHORTS_N * CO::SIZE)
        .ok_or(BAD_ACCOUNT)
}

fn cohort_rec(r: &[u8]) -> (u32, u16, u16) {
    (
        u32::from_le_bytes([r[0], r[1], r[2], r[3]]),
        u16::from_le_bytes([r[4], r[5]]),
        u16::from_le_bytes([r[6], r[7]]),
    )
}

/// FileTicket's cohort step (I-47): the record of `now_bell` gets
/// `filed += 1`, else the first free record is reused, else `CohortFull`.
pub fn cohort_file(province: &mut [u8], now_bell: u32) -> R<()> {
    let t = cohort_table(province)?;
    let mut free: Option<usize> = None;
    for (i, r) in t.chunks_exact(CO::SIZE).enumerate() {
        let (b, f, s) = cohort_rec(r);
        if b == now_bell && f > 0 {
            let nf = f.checked_add(1).ok_or(OVERFLOW)?;
            let o = i * CO::SIZE + CO::FILED;
            t[o..o + 2].copy_from_slice(&nf.to_le_bytes());
            return Ok(());
        }
        if free.is_none() && CO::is_free(b, f, s, now_bell) {
            free = Some(i);
        }
    }
    let i = free.ok_or(FrontierError::CohortFull)?;
    let o = i * CO::SIZE;
    t[o..o + CO::SIZE].copy_from_slice(&[0; 8]);
    t[o..o + 4].copy_from_slice(&now_bell.to_le_bytes());
    t[o + CO::FILED..o + CO::FILED + 2].copy_from_slice(&1u16.to_le_bytes());
    Ok(())
}

/// A ticket of `ticket_bell` ended: its cohort record (if still there,
/// not expired and reused) gets `settled += 1`.
pub fn cohort_settle(province: &mut [u8], ticket_bell: u32) -> R<()> {
    let t = cohort_table(province)?;
    for (i, r) in t.chunks_exact(CO::SIZE).enumerate() {
        let (b, f, s) = cohort_rec(r);
        if b == ticket_bell && f > 0 && s < f {
            let o = i * CO::SIZE + CO::SETTLED;
            t[o..o + 2].copy_from_slice(&(s + 1).to_le_bytes());
            return Ok(());
        }
    }
    Ok(())
}

// ------------------------------------------------------------ Join

/// 0x30 Join: `[wallet s] [payer s,w] [season] [frontier r] [citizen w]
/// [joinshard w] [system] [join_gate s?]`, data `faction, session,
/// session_expiry` (the session is data, I-40). Class P.
///
/// Effective Running and `bell < join_close_bell` (`WrongStatus`); when
/// `season.join_gate ≠ 0` account 7 is the gate key and signs
/// (`JoinGate`, I-51), with no gate there is no account 7
/// (`TooManyAccounts`); `faction ≤ 5` and `session_expiry ≤ now + 30 d`
/// (`BadData`); the Citizen absent at `ct‖tag(wallet)` (present:
/// `AlreadyDone`); the JoinShard `(faction, sha256(wallet)[0] mod 8)`;
/// **capacity** on the folded Frontier: `(open − occupied) × 10,000 ≥
/// reserve_bps × open`, or a ring can still open (`rings_opened ≤ r_max`;
/// the funds are not in Join's account list), else `Capacity`. Effects:
/// the Citizen (payer funds it, `rent_payer = payer`, bucket full, three
/// floor explorations, no ticket), JoinShard `members += 1`. Log `JOIN`.
pub fn join(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::Join, a, None)?;
    let x = aix::Join::decode(d)?;
    let (wallet, payer, season_ai, frontier, citizen, shard, gate) = match a {
        [w, pa, s, f, c, j, _sys] => (w, pa, s, f, c, j, None),
        [w, pa, s, f, c, j, _sys, g] => (w, pa, s, f, c, j, Some(g)),
        _ => return Err(FrontierError::TooManyAccounts.into()),
    };
    let now = prologue::now()?;
    let hdr = prologue::season(
        season_ai,
        p,
        Some(&crate::RULESET_HASH),
        &[S::STATUS_RUNNING],
        now.ts,
    )?;
    let now_bell = hdr.bell(now.ts).ok_or(FrontierError::WrongStatus)?;
    if now_bell >= hdr.join_close_bell {
        return Err(FrontierError::WrongStatus.into());
    }
    let (join_gate, reserve_bps, r_max) = {
        let sd = season_ai.try_borrow_data()?;
        let r = Ro(&sd);
        (
            r.arr::<32>(S::JOIN_GATE)?,
            r.u16(S::RESERVE_BPS)?,
            r.u16(S::R_MAX)?,
        )
    };
    match gate {
        None if join_gate != [0u8; 32] => return Err(FrontierError::JoinGate.into()),
        Some(g) if join_gate == [0u8; 32] => {
            let _ = g;
            return Err(FrontierError::TooManyAccounts.into());
        }
        Some(g) if key(g) != join_gate || !g.is_signer => {
            return Err(FrontierError::JoinGate.into())
        }
        _ => {}
    }
    if x.faction >= JS::FACTIONS || x.session_expiry > now.ts.saturating_add(MAX_SESSION_SECS) {
        return Err(FrontierError::BadData.into());
    }
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    let w_bytes = key(wallet);
    let tag15 = addr::citizen_tag15(&w_bytes);
    expect_key(citizen, &ctx.citizen_by_tag15(&tag15))?;
    if prologue::presence(citizen, p, AccountKind::Citizen, hdr.id)? {
        return Err(FrontierError::AlreadyDone.into());
    }
    let shard_i = addr::join_shard_of(&w_bytes);
    present_at(
        shard,
        &ctx.join_shard(x.faction, shard_i),
        p,
        AccountKind::JoinShard,
        hdr.id,
    )?;
    present_at(frontier, &ctx.frontier(), p, AccountKind::Frontier, hdr.id)?;
    {
        let fd = frontier.try_borrow_data()?;
        let r = Ro(&fd);
        let open = r.u32(FR::OPEN_SITES)? as u64;
        let occupied = r.u32(FR::OCCUPIED_SITES)? as u64;
        let rings = r.u16(FR::RINGS_OPENED)?;
        let free_ok = open.saturating_sub(occupied) * 10_000 >= reserve_bps as u64 * open;
        if !free_ok && rings > r_max {
            return Err(FrontierError::Capacity.into());
        }
    }
    init::init_with_seed(
        payer,
        citizen,
        season_ai,
        &SeasonSigner::new(hdr.id, hdr.bump),
        &citizen_seed(&tag15),
        C::SIZE,
        init::rent(C::SIZE)?,
        p,
    )?;
    let bucket_t = u32::try_from(now.ts.saturating_sub(hdr.genesis_ts).max(0))
        .map_err(|_| FrontierError::Overflow)?;
    let mut cd = citizen.try_borrow_mut_data()?;
    init_header(&mut cd, AccountKind::Citizen, hdr.id)?;
    {
        let mut w = Rw(&mut cd);
        w.set_arr(C::WALLET, &w_bytes)?;
        w.set_arr(C::SESSION, &x.session)?;
        w.set_i64(C::SESSION_EXPIRY, x.session_expiry)?;
        w.set_u8(C::FACTION, x.faction)?;
        w.set_u8(C::FLAGS, C::FLAG_JOINED)?;
        w.set_u8(C::EXPLORES_FLOOR_LEFT, C::EXPLORES_FLOOR)?;
        w.set_u32(C::JOIN_BELL, now_bell)?;
        w.set_u8(C::JOIN_SHARD, shard_i)?;
        w.set_u32(C::BUCKET_MILLI, hdr.bucket_burst as u32 * 1_000)?;
        w.set_u32(C::BUCKET_T, bucket_t)?;
        w.set_u32(C::TICKET_BELL, C::NO_TICKET)?;
        w.set_u64(C::CITIZEN_TAG, addr::citizen_tag(citizen.key.as_array()))?;
        w.set_i64(C::LAST_ACTION_TS, now.ts)?;
        w.set_arr(C::RENT_PAYER, payer.key.as_ref())?;
    }
    let mut jd = shard.try_borrow_mut_data()?;
    Rw(&mut jd).add_u32(JS::MEMBERS, 1)?;
    let payload = Buf::<74>::new()
        .bytes(&w_bytes)
        .u8(x.faction)
        .u8(shard_i)
        .bytes(&x.session)
        .i64(x.session_expiry);
    events::emit(
        Kind::JOIN,
        now_bell,
        &tag15,
        payload.get()?,
        &mut [
            Chained {
                entity: EntityKind::Citizen,
                data: &mut cd,
            },
            Chained {
                entity: EntityKind::JoinShard,
                data: &mut jd,
            },
        ],
    )
}

// ------------------------------------------------------------ SetSession, SetVigil

/// 0x31 SetSession: P with the wallet as actor (a session key cannot
/// replace itself: `Auth`); data `session, expiry`, `expiry ≤ now + 30 d`
/// (`BadData`). Log `SESSION`.
pub fn set_session(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::SetSession, a, None)?;
    let x = aix::SetSession::decode(d)?;
    let now = prologue::now()?;
    let pc = player(p, a, now.ts, false)?;
    if x.expiry > now.ts.saturating_add(MAX_SESSION_SECS) {
        return Err(FrontierError::BadData.into());
    }
    let citizen = &a[3];
    let mut cd = citizen.try_borrow_mut_data()?;
    {
        let mut w = Rw(&mut cd);
        w.set_arr(C::SESSION, &x.session)?;
        w.set_i64(C::SESSION_EXPIRY, x.expiry)?;
    }
    let tag15 = tag15_of(&cd)?;
    let payload = Buf::<40>::new().bytes(&x.session).i64(x.expiry);
    events::emit(
        Kind::SESSION,
        pc.now_bell,
        &tag15,
        payload.get()?,
        &mut [Chained {
            entity: EntityKind::Citizen,
            data: &mut cd,
        }],
    )
}

/// 0x32 SetVigil(start_min): P. `start_min < 1,440` (`BadData`); a change
/// already in force is folded in (`vigil_start_min = vigil_next_min` once
/// `now ≥ vigil_from_ts`); the weekly rule (`Cooldown`, module doc); the
/// new window takes effect at `siege::change_effective_at(now)` = the
/// first UTC midnight at or after `now + 24 h` (CL-09). Log `VIGIL`.
pub fn set_vigil(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::SetVigil, a, None)?;
    let x = aix::SetVigil::decode(d)?;
    let now = prologue::now()?;
    let pc = player(p, a, now.ts, true)?;
    if x.start_min >= DAY_MINUTES {
        return Err(FrontierError::BadData.into());
    }
    let citizen = &a[3];
    let mut cd = citizen.try_borrow_mut_data()?;
    let from = {
        let mut w = Rw(&mut cd);
        let from_ts = w.i64(C::VIGIL_FROM_TS)?;
        if from_ts != 0 {
            if now.ts < from_ts.saturating_add(VIGIL_AFTER_FROM_SECS) {
                return Err(FrontierError::Cooldown.into());
            }
            let next = w.u16(C::VIGIL_NEXT_MIN)?;
            w.set_u16(C::VIGIL_START_MIN, next)?;
        }
        let from = siege::change_effective_at(now.ts);
        w.set_u16(C::VIGIL_NEXT_MIN, x.start_min)?;
        w.set_i64(C::VIGIL_FROM_TS, from)?;
        from
    };
    let tag15 = tag15_of(&cd)?;
    let payload = Buf::<10>::new().u16(x.start_min).i64(from);
    events::emit(
        Kind::VIGIL,
        pc.now_bell,
        &tag15,
        payload.get()?,
        &mut [Chained {
            entity: EntityKind::Citizen,
            data: &mut cd,
        }],
    )
}

// ------------------------------------------------------------ FileTicket

/// 0x33 FileTicket(n ≤ 3 sites): P + `[frontier r] [province × m w]
/// [system]`, the m distinct Provinces of the sites in first-seen order.
///
/// No provisional or final first holding and no open ticket
/// (`TicketState`); each site checked (`ProvinceCoord::checked`, ring ≥ 2
/// else `ReservedSite`, ring < `rings_opened`, `site < site_count`, no
/// duplicate: `BadData`), its Province present (`BadAccount`, I-48), the
/// wedge rule (`BadData`); each Province's cohort for `now_bell`
/// (`CohortFull`); the payer tops the Citizen's Holding-rent escrow up to
/// `rent(1,280)` (I-47); the payer becomes `ticket_funder` only when it
/// tops up (v1.5), else the stored funder is kept. (`ticket_bell ≠ now_bell` holds by construction:
/// a ticket settles only after its bell's seed, so no ticket can end in
/// the bell it was filed.) Log `TICKET` (Citizen, the m Provinces).
pub fn file_ticket(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::FileTicket, a, None)?;
    let x = aix::FileTicket::decode(d)?;
    let now = prologue::now()?;
    let pc = player(p, a, now.ts, true)?;
    crate::heap::trace_checkpoint(300);
    let [_actor, payer, season_ai, citizen, frontier, rest @ ..] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let provinces = rest
        .split_last()
        .map(|(_sys, v)| v)
        .ok_or(FrontierError::TooManyAccounts)?;
    let hdr = pc.season;
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    let (flags, open_ticket, faction, escrow) = {
        let cd = citizen.try_borrow_data()?;
        let r = Ro(&cd);
        (
            r.u8(C::FLAGS)?,
            r.u32(C::TICKET_BELL)?,
            r.u8(C::FACTION)?,
            r.u64(C::TICKET_ESCROW)?,
        )
    };
    if flags & (C::FLAG_PROVISIONAL | C::FLAG_FIRST_HOLDING_FINAL) != 0
        || open_ticket != C::NO_TICKET
    {
        return Err(FrontierError::TicketState.into());
    }
    present_at(frontier, &ctx.frontier(), p, AccountKind::Frontier, hdr.id)?;
    crate::heap::trace_checkpoint(301);
    let (rings_opened, own_open, own_occ) = {
        let fd = frontier.try_borrow_data()?;
        let r = Ro(&fd);
        (
            r.u16(FR::RINGS_OPENED)?,
            r.u32(FR::WEDGE_OPEN + 4 * faction as usize)?,
            r.u32(FR::WEDGE_OCCUPIED + 4 * faction as usize)?,
        )
    };
    let r_max = {
        let sd = season_ai.try_borrow_data()?;
        Ro(&sd).u16(S::R_MAX)?
    };
    let n = x.n as usize;
    let mut sites = [(0i16, 0i16, 0u8); 3];
    for (i, s) in x.sites.iter().take(n).enumerate() {
        let (pi, qi) = (s.p as i32, s.q as i32);
        let c = ProvinceCoord::checked(pi, qi, r_max).map_err(|_| FrontierError::BadData)?;
        let ring = c.ring();
        if ring < 2 {
            return Err(FrontierError::ReservedSite.into());
        }
        if ring >= rings_opened as u32 || s.site as usize >= PV::SITES_N {
            return Err(FrontierError::BadData.into());
        }
        let w = c.wedge().ok_or(FrontierError::BadData)?;
        if !wedge_allowed(faction, w, ring, own_open, own_occ, rings_opened) {
            return Err(FrontierError::BadData.into());
        }
        let e = (s.p, s.q, s.site);
        if sites[..i].contains(&e) {
            return Err(FrontierError::BadData.into());
        }
        sites[i] = e;
    }
    let distinct = distinct_provinces(&sites[..n]);
    crate::heap::trace_checkpoint(302);
    if distinct.len() != provinces.len() {
        return Err(FrontierError::TooManyAccounts.into());
    }
    let now_bell = pc.now_bell;
    for (ai, (pp, qq)) in provinces.iter().zip(&distinct) {
        present_at(
            ai,
            &ctx.province(*pp as i32, *qq as i32),
            p,
            AccountKind::Province,
            hdr.id,
        )?;
        let mut pd = ai.try_borrow_mut_data()?;
        let site_count = Ro(&pd).u8(PV::SITE_COUNT)?;
        if sites[..n]
            .iter()
            .any(|s| (s.0, s.1) == (*pp, *qq) && s.2 >= site_count)
        {
            return Err(FrontierError::BadData.into());
        }
        cohort_file(&mut pd, now_bell)?;
    }
    crate::heap::trace_checkpoint(303);
    // Escrow: the payer tops the Holding rent up (I-47). v1.5 (§5.9): the
    // payer becomes the funder only when it tops up; an escrow left by an
    // expired or exhausted ticket keeps the funder that paid it, so a
    // refile never redirects someone else's escrow refund.
    let rent_h = init::rent(H::SIZE)?;
    let top = rent_h.saturating_sub(escrow);
    if top > payer.lamports() {
        return Err(FrontierError::Insufficient.into());
    }
    init::transfer(payer, citizen, top)?;
    crate::heap::trace_checkpoint(304);
    let mut cd = citizen.try_borrow_mut_data()?;
    let funder: [u8; 32] = if top > 0 {
        payer.key.to_bytes()
    } else {
        Ro(&cd).arr(C::TICKET_FUNDER)?
    };
    {
        let mut w = Rw(&mut cd);
        for (i, s) in sites.iter().enumerate() {
            let o = C::TICKET_SITES + i * TS::SIZE;
            w.set_i16(o + TS::P, s.0)?;
            w.set_i16(o + TS::Q, s.1)?;
            w.set_u8(o + TS::SITE, s.2)?;
        }
        w.set_u32(C::TICKET_BELL, now_bell)?;
        w.set_u8(C::TICKET_NEXT, 0)?;
        w.set_u64(C::TICKET_ESCROW, escrow.max(rent_h))?;
        w.set_arr(C::TICKET_FUNDER, &funder)?;
    }
    let tag15 = tag15_of(&cd)?;
    let sites_raw: [u8; 15] = Ro(&cd).arr(C::TICKET_SITES)?;
    let payload = Buf::<60>::new()
        .u32(now_bell)
        .u8(x.n)
        .bytes(&sites_raw)
        .u64(escrow.max(rent_h))
        .bytes(&funder);
    let mut borrows = Vec::with_capacity(provinces.len());
    for ai in provinces {
        borrows.push(ai.try_borrow_mut_data()?);
    }
    let mut chained: Vec<Chained> = Vec::with_capacity(4);
    chained.push(Chained {
        entity: EntityKind::Citizen,
        data: &mut cd,
    });
    for b in borrows.iter_mut() {
        chained.push(Chained {
            entity: EntityKind::Province,
            data: b,
        });
    }
    events::emit(Kind::TICKET, now_bell, &tag15, payload.get()?, &mut chained)
}

// ------------------------------------------------------------ SettleTicket

/// The seed of `S(bell, region)` for a SettleTicket: THE anchor's
/// SeedCache (`[cache] [anchor]`, any nonce: the cache names THE anchor
/// and its `A`, and its round is `S(A)`), or the archive entry once
/// archived (both positions the archive `aa‖(region, part(bell))`).
/// Returns `(seed, S)`. A missing cache or archive entry is
/// `SeedNotReady`; a missing anchor `NoAnchor`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn seed_of_bell(
    p: &Pubkey,
    ctx: &AddrCtx,
    season_id: u64,
    c: &SeasonClock,
    genesis_ts: i64,
    seed_ai: &AccountInfo,
    anchor_ai: &AccountInfo,
    bell: u32,
    region: u8,
) -> R<([u8; 32], u64)> {
    let part = addr::archive_part_of(bell);
    let archive = ctx.anchor_archive(region, part);
    if *seed_ai.key.as_array() == archive {
        expect_key(anchor_ai, &archive)?;
        if !prologue::presence(seed_ai, p, AccountKind::AnchorArchive, season_id)? {
            return Err(FrontierError::SeedNotReady.into());
        }
        let d = seed_ai.try_borrow_data()?;
        if archive_key(&d)? != (region, part) {
            return Err(BAD_ACCOUNT);
        }
        if !archive_archived(&d, bell)? {
            return Err(FrontierError::SeedNotReady.into());
        }
        let (a_off, seed, _sig) = archive_entry_of(&d, bell)?;
        let a = beacon::bell_end(genesis_ts, bell)
            .checked_add(a_off as i64)
            .ok_or(OVERFLOW)?;
        return Ok((seed, c.seed_round(bell, a)));
    }
    expect_key(anchor_ai, &ctx.bell_anchor(bell, region))?;
    if !prologue::presence(anchor_ai, p, AccountKind::BellAnchor, season_id)? {
        return Err(FrontierError::NoAnchor.into());
    }
    let an = {
        let d = anchor_ai.try_borrow_data()?;
        Anchor::read(&d)?
    };
    if an.bell != bell || an.region != region {
        return Err(BAD_ACCOUNT);
    }
    if !prologue::presence(seed_ai, p, AccountKind::SeedCache, season_id)? {
        return Err(FrontierError::SeedNotReady.into());
    }
    let cache = {
        let d = seed_ai.try_borrow_data()?;
        Cache::read(&d)?
    };
    expect_key(seed_ai, &ctx.seed_cache(bell, region, cache.nonce))?;
    if cache.bell != bell
        || cache.region != region
        || cache.anchor_key != *anchor_ai.key.as_array()
        || cache.a != an.a
        || cache.round != c.seed_round(bell, an.a)
    {
        return Err(BAD_ACCOUNT);
    }
    Ok((cache.seed, cache.round))
}

/// The first bell starting at or after `t` (0 before genesis).
fn bell_from(genesis_ts: i64, t: i64) -> u32 {
    let dt = t.saturating_sub(genesis_ts).max(0);
    u32::try_from((dt + 599) / 600).unwrap_or(u32::MAX)
}

/// A founded (or re-founded) holding (module doc): `Holding::found(now,
/// day, 1)`, Hamlet base production, food upkeep 0, the starter kit.
pub fn founded_holding(now: i64, day: u32) -> R<Holding> {
    let mut h = Holding::found(now, day, 1);
    h.production = catalog::base_production(Tier::Hamlet);
    h.set_upkeep(now, Resource::Food, 0)
        .map_err(|_| FrontierError::Kernel)?;
    let kit = catalog::starter_kit();
    for (r, amount) in kit.iter().enumerate() {
        if *amount > 0 {
            h.credit(now, Resource::ALL[r], *amount)
                .map_err(|_| FrontierError::Kernel)?;
        }
    }
    Ok(h)
}

/// The fields SettleTicket writes into a founded Holding.
struct Founding {
    p: i16,
    q: i16,
    site: u8,
    gen: u8,
    tile: u8,
    owner: [u8; 32],
    score: u64,
    faction: u8,
    ticket_bell: u32,
    rent_payer: [u8; 32],
    final_ts: i64,
    pool_owed: u64,
}

fn write_holding(d: &mut [u8], f: &Founding, h: &Holding) -> R<()> {
    // Everything after the header is rewritten (a displacement keeps only
    // the chain and the `pool_owed` the payment may have diverted).
    d.get_mut(64..H::SIZE).ok_or(BAD_ACCOUNT)?.fill(0);
    let mut w = Rw(d);
    w.set_i16(H::P, f.p)?;
    w.set_i16(H::Q, f.q)?;
    w.set_u8(H::SITE, f.site)?;
    w.set_u8(H::GEN, f.gen)?;
    w.set_u8(H::TILE, f.tile)?;
    w.set_u8(H::STATE, H::STATE_PROVISIONAL)?;
    w.set_arr(H::OWNER_CITIZEN, &f.owner)?;
    w.set_u64(H::TICKET_SCORE, f.score)?;
    w.set_u8(H::FACTION, f.faction)?;
    w.set_u8(H::ORDER, h.order)?;
    w.set_u8(H::TIER, h.tier as u8)?;
    w.set_u32(H::TICKET_BELL, f.ticket_bell)?;
    w.set_i64(H::FOUNDED_TS, h.founded_ts)?;
    w.set_u32(H::FOUNDED_DAY, h.founded_day)?;
    w.set_i64(H::LAST_OWNER_ACTION, h.last_owner_action)?;
    w.set_i64(H::SHIELD_UNTIL, h.shield_until())?;
    for r in 0..RESOURCES {
        let s = &h.stores[r];
        let o = H::store(r);
        w.set_i64(o + AC::VALUE, s.value)?;
        w.set_i64(o + AC::RATE, s.rate)?;
        w.set_i64(o + AC::CAP, s.cap)?;
        w.set_i64(o + AC::T0, s.t0)?;
        w.set_i64(o + AC::FRAC, s.frac)?;
        w.set_i64(H::PRODUCTION + 8 * r, h.production[r])?;
        w.set_i64(H::UPKEEP + 8 * r, h.upkeep[r])?;
    }
    w.set_u32(H::WALLS, h.walls)?;
    w.set_i64(H::WALLS_COMMITTED_BEFORE, h.walls_committed_before)?;
    w.set_i64(H::FOOD_SHORTFALL, h.food_shortfall)?;
    w.set_arr(H::RENT_PAYER, &f.rent_payer)?;
    w.set_i64(H::FINAL_TS, f.final_ts)?;
    w.set_u64(H::POOL_OWED, f.pool_owed)
}

fn write_holding_ref(citizen: &mut [u8], p: i16, q: i16, site: u8, gen: u8) -> R<()> {
    let mut w = Rw(citizen);
    w.set_i16(C::HOLDING + HR::P, p)?;
    w.set_i16(C::HOLDING + HR::Q, q)?;
    w.set_u8(C::HOLDING + HR::SITE, site)?;
    w.set_u8(C::HOLDING + HR::GEN, gen)
}

fn shard_holdings(js: &mut [u8], wedge: u8, delta: i32) -> R<()> {
    let mut w = Rw(js);
    let at = JS::HOLDINGS_BY_WEDGE + 4 * wedge as usize;
    for off in [JS::HOLDINGS, at] {
        let v = w.u32(off)?;
        let nv = if delta >= 0 {
            v.checked_add(delta as u32).ok_or(OVERFLOW)?
        } else {
            v.saturating_sub(delta.unsigned_abs())
        };
        w.set_u32(off, nv)?;
    }
    Ok(())
}

/// 0x34 SettleTicket(k): `[payer s,w] [season] [citizen w] [holding w]
/// [province w] [joinshard w] [seedcache|archive r] [anchor|archive r]
/// [other ticket provinces w × 0–2] [displaced_rent_payer w,
/// displaced_citizen w, displaced_joinshard w]? [system]`. Class D.
///
/// Status Running. The ticket present (`NoTicket`), `k == ticket_next`
/// (`AlreadyDone` below, `TicketState` otherwise); the holding and
/// Province of site k, the ticket's other distinct Provinces in
/// first-seen order, the citizen's JoinShard. **Expired** (`now_bell ≥
/// ticket_bell + 24`) ends the ticket. Otherwise the seed `S(ticket_bell,
/// r_site)` ([`seed_of_bell`]), `score` ([`ticket_score`]); site free or
/// released-free → **fresh**; a provisional holding of the same
/// `ticket_bell` with a lower score (tie: lower tag) → **displace** (no
/// time condition, I-47); else **taken** (`ticket_next += 1`; the ticket is
/// exhausted at `n`). Fresh: the Holding is funded from the ticket escrow
/// (program-to-program), `rent_payer = ticket_funder`. Displace: the
/// Holding is rewritten in place (`gen + 1`), the new escrow pays the
/// displaced Holding's `rent_payer` (`pay_or_divert`, sink `pool_owed`),
/// the displaced Citizen reverts and its JoinShard decrements. Whenever
/// the ticket ends every ticket Province's cohort gets `settled += 1`.
/// Log `SETTLE`.
pub fn settle_ticket(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    let x = aix::SettleTicket::decode(d)?;
    if a.len() < 9 || a.len() > 14 {
        return Err(FrontierError::TooManyAccounts.into());
    }
    let extra = a.len() - 9;
    let (n_other, n_disp) = if extra >= 3 {
        (extra - 3, 1)
    } else {
        (extra, 0)
    };
    check_accounts(
        Ix::SettleTicket,
        a,
        Some(&[1, n_other as u8, n_disp as u8, 1]),
    )?;
    let [payer, season_ai, citizen, holding, province, shard, seed_ai, anchor_ai, rest @ ..] = a
    else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let others = &rest[..n_other];
    let disp = if n_disp == 1 {
        Some((&rest[n_other], &rest[n_other + 1], &rest[n_other + 2]))
    } else {
        None
    };
    let now = prologue::now()?;
    let hdr = prologue::season(
        season_ai,
        p,
        Some(&crate::RULESET_HASH),
        &[S::STATUS_RUNNING],
        now.ts,
    )?;
    let now_bell = hdr.bell(now.ts).ok_or(FrontierError::WrongStatus)?;
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    // The citizen and its ticket.
    prologue::present(citizen, p, AccountKind::Citizen, hdr.id)?;
    let (tag15, faction, shard_i, ticket_bell, ticket_next, my_tag, escrow, funder, sites, n) = {
        let cd = citizen.try_borrow_data()?;
        let r = Ro(&cd);
        let (sites, n) = ticket_sites(&cd)?;
        (
            tag15_of(&cd)?,
            r.u8(C::FACTION)?,
            r.u8(C::JOIN_SHARD)?,
            r.u32(C::TICKET_BELL)?,
            r.u8(C::TICKET_NEXT)?,
            r.u64(C::CITIZEN_TAG)?,
            r.u64(C::TICKET_ESCROW)?,
            r.arr::<32>(C::TICKET_FUNDER)?,
            sites,
            n,
        )
    };
    expect_key(citizen, &ctx.citizen_by_tag15(&tag15))?;
    if ticket_bell == C::NO_TICKET {
        return Err(FrontierError::NoTicket.into());
    }
    if x.k < ticket_next {
        return Err(FrontierError::AlreadyDone.into());
    }
    if x.k != ticket_next || x.k as usize >= n {
        return Err(FrontierError::TicketState.into());
    }
    let (sp, sq, site) = sites[x.k as usize];
    let (pi, qi) = (sp as i32, sq as i32);
    let coord = ProvinceCoord::new(pi, qi);
    expect_key(holding, &ctx.holding(pi, qi, site))?;
    present_at(
        province,
        &ctx.province(pi, qi),
        p,
        AccountKind::Province,
        hdr.id,
    )?;
    let distinct = distinct_provinces(&sites[..n]);
    let other_keys: Vec<(i16, i16)> = distinct.into_iter().filter(|pq| *pq != (sp, sq)).collect();
    if other_keys.len() != others.len() {
        return Err(FrontierError::TooManyAccounts.into());
    }
    for (ai, (pp, qq)) in others.iter().zip(&other_keys) {
        present_at(
            ai,
            &ctx.province(*pp as i32, *qq as i32),
            p,
            AccountKind::Province,
            hdr.id,
        )?;
    }
    present_at(
        shard,
        &ctx.join_shard(faction, shard_i),
        p,
        AccountKind::JoinShard,
        hdr.id,
    )?;
    let expired = now_bell >= ticket_bell.saturating_add(COHORT_BELLS);
    let (outcome, score, final_ts) = if expired {
        (settle_outcome::EXPIRED, 0u64, 0i64)
    } else {
        let c = {
            let sd = season_ai.try_borrow_data()?;
            SeasonClock::read(&sd)?
        };
        let region = permutation_rules::frontier::geometry::region_of(coord);
        let (seed, s_round) = seed_of_bell(
            p,
            &ctx,
            hdr.id,
            &c,
            hdr.genesis_ts,
            seed_ai,
            anchor_ai,
            ticket_bell,
            region,
        )?;
        let score = ticket_score(&seed, pi, qi, site, my_tag);
        let final_ts = c.round_time(s_round).checked_add(600).ok_or(OVERFLOW)?;
        // The site as the Province mirrors it.
        let state = {
            let pd = province.try_borrow_data()?;
            let r = Ro(&pd);
            if site >= r.u8(PV::SITE_COUNT)? {
                return Err(BAD_ACCOUNT);
            }
            r.u8(PV::site(site as usize) + SM::STATE)?
        };
        let outcome = match state {
            SM::STATE_FREE | SM::STATE_RELEASED_FREE => {
                if !init::is_absent(holding) {
                    return Err(BAD_ACCOUNT);
                }
                settle_outcome::FRESH
            }
            SM::STATE_HOLDING => {
                prologue::present(holding, p, AccountKind::Holding, hdr.id)?;
                let hd = holding.try_borrow_data()?;
                let r = Ro(&hd);
                if (r.i16(H::P)?, r.i16(H::Q)?, r.u8(H::SITE)?) != (sp, sq, site) {
                    return Err(BAD_ACCOUNT);
                }
                let owner: [u8; 32] = r.arr(H::OWNER_CITIZEN)?;
                if r.u8(H::STATE)? == H::STATE_PROVISIONAL
                    && r.u32(H::TICKET_BELL)? == ticket_bell
                    && displaces(
                        score,
                        my_tag,
                        r.u64(H::TICKET_SCORE)?,
                        addr::citizen_tag(&owner),
                    )
                {
                    settle_outcome::DISPLACE
                } else {
                    settle_outcome::TAKEN
                }
            }
            _ => settle_outcome::TAKEN,
        };
        (outcome, score, final_ts)
    };
    if (outcome == settle_outcome::DISPLACE) != disp.is_some() {
        return Err(FrontierError::TooManyAccounts.into());
    }
    let won = matches!(outcome, settle_outcome::FRESH | settle_outcome::DISPLACE);
    let exhausted = outcome == settle_outcome::TAKEN && x.k as usize + 1 >= n;
    let ends = outcome != settle_outcome::TAKEN || exhausted;
    let bell = now_bell;
    let mut gen = 0u8;
    let mut displaced_tag = 0u64;
    let wedge = coord.wedge().unwrap_or(0);
    if won {
        let (tile, old_gen) = {
            let pd = province.try_borrow_data()?;
            let r = Ro(&pd);
            (
                r.u8(PV::SITES + site as usize)?,
                r.u8(PV::site(site as usize) + SM::GEN)?,
            )
        };
        gen = old_gen.checked_add(1).ok_or(OVERFLOW)?;
        let mut pool_owed = 0u64;
        if outcome == settle_outcome::FRESH {
            // The whole escrow moves (a pre-funded Holding keeps its extra).
            let moved = init::init_funded(
                citizen,
                holding,
                season_ai,
                &SeasonSigner::new(hdr.id, hdr.bump),
                &holding_seed(pi, qi, site),
                H::SIZE,
                p,
                escrow,
            )?;
            if moved != escrow {
                return Err(FrontierError::Insufficient.into());
            }
            let mut hd = holding.try_borrow_mut_data()?;
            init_header(&mut hd, AccountKind::Holding, hdr.id)?;
        } else if let Some((d_payer, d_citizen, d_shard)) = disp {
            let (old_owner, old_payer) = {
                let hd = holding.try_borrow_data()?;
                let r = Ro(&hd);
                (r.arr::<32>(H::OWNER_CITIZEN)?, r.arr::<32>(H::RENT_PAYER)?)
            };
            expect_key(d_payer, &old_payer)?;
            expect_key(d_citizen, &old_owner)?;
            prologue::present(d_citizen, p, AccountKind::Citizen, hdr.id)?;
            let (d_faction, d_shard_i) = {
                let dd = d_citizen.try_borrow_data()?;
                let r = Ro(&dd);
                displaced_tag = r.u64(C::CITIZEN_TAG)?;
                (r.u8(C::FACTION)?, r.u8(C::JOIN_SHARD)?)
            };
            present_at(
                d_shard,
                &ctx.join_shard(d_faction, d_shard_i),
                p,
                AccountKind::JoinShard,
                hdr.id,
            )?;
            // The new escrow pays the displaced Holding's rent payer.
            init::pay_or_divert(
                p,
                citizen,
                d_payer,
                escrow,
                &Sink::PoolOwed(holding),
                divert_reason::ESCROW_REFUND,
                bell,
            )?;
            pool_owed = {
                let hd = holding.try_borrow_data()?;
                Ro(&hd).u64(H::POOL_OWED)?
            };
            {
                let mut dd = d_citizen.try_borrow_mut_data()?;
                let f = Ro(&dd).u8(C::FLAGS)?;
                let mut w = Rw(&mut dd);
                w.set_u8(
                    C::FLAGS,
                    f & !(C::FLAG_PROVISIONAL | C::FLAG_FIRST_HOLDING_FINAL),
                )?;
                w.set_u8(C::HOLDINGS_N, 0)?;
                write_holding_ref(&mut dd, 0, 0, 0, 0)?;
            }
            let mut sd = d_shard.try_borrow_mut_data()?;
            shard_holdings(&mut sd, wedge, -1)?;
        }
        let h = founded_holding(now.ts, addr::day_of(now_bell))?;
        let f = Founding {
            p: sp,
            q: sq,
            site,
            gen,
            tile,
            owner: key(citizen),
            score,
            faction,
            ticket_bell,
            rent_payer: funder,
            final_ts,
            pool_owed,
        };
        {
            let mut hd = holding.try_borrow_mut_data()?;
            write_holding(&mut hd, &f, &h)?;
        }
        let shield_bell = bell_from(hdr.genesis_ts, h.shield_until());
        {
            let mut pd = province.try_borrow_mut_data()?;
            let mut w = Rw(&mut pd);
            let o = PV::site(site as usize);
            let was_free = w.u8(o + SM::STATE)? != SM::STATE_HOLDING;
            d_zero(&mut w, o)?;
            w.set_u8(o + SM::STATE, SM::STATE_HOLDING)?;
            w.set_u8(o + SM::FACTION, faction)?;
            w.set_u8(o + SM::ORDER, 1)?;
            w.set_u8(o + SM::TIER, Tier::Hamlet as u8)?;
            w.set_u8(o + SM::GEN, gen)?;
            w.set_u32(o + SM::PEND0_BELL, SM::NO_BELL)?;
            w.set_u32(o + SM::PEND1_BELL, SM::NO_BELL)?;
            w.set_u32(o + SM::SHIELD_UNTIL_BELL, shield_bell)?;
            if was_free {
                let u = w.u8(PV::N_SITES_USED)?;
                w.set_u8(PV::N_SITES_USED, u.checked_add(1).ok_or(OVERFLOW)?)?;
            }
            let e = w.u32(PV::ROSTER_EPOCH)?;
            w.set_u32(PV::ROSTER_EPOCH, e.wrapping_add(1))?;
        }
        {
            let mut jd = shard.try_borrow_mut_data()?;
            shard_holdings(&mut jd, wedge, 1)?;
        }
    }
    // The citizen's side.
    {
        let mut cd = citizen.try_borrow_mut_data()?;
        if won {
            write_holding_ref(&mut cd, sp, sq, site, gen)?;
        }
        let f = Ro(&cd).u8(C::FLAGS)?;
        let mut w = Rw(&mut cd);
        if won {
            w.set_u8(C::FLAGS, f | C::FLAG_PROVISIONAL)?;
            w.set_u8(C::HOLDINGS_N, 1)?;
            w.set_u64(C::TICKET_ESCROW, 0)?;
        } else if outcome == settle_outcome::TAKEN {
            w.set_u8(C::TICKET_NEXT, x.k + 1)?;
        }
        if ends {
            w.set_u32(C::TICKET_BELL, C::NO_TICKET)?;
        }
    }
    if ends {
        let mut pd = province.try_borrow_mut_data()?;
        cohort_settle(&mut pd, ticket_bell)?;
        for ai in others {
            let mut od = ai.try_borrow_mut_data()?;
            cohort_settle(&mut od, ticket_bell)?;
        }
    }
    // SETTLE: chains the Citizen always; won: the JoinShard(s), the
    // displaced Citizen, the Holding; the site's Province unless taken; the
    // other Provinces (and, exhausted, the site's) when the ticket ends.
    let mut key9 = [0u8; 9];
    key9[..8].copy_from_slice(&raw_province(pi, qi));
    key9[8] = site;
    let payload = Buf::<38>::new()
        .u8(outcome)
        .u64(my_tag)
        .u64(score)
        .u64(displaced_tag)
        .u8(gen)
        .i64(final_ts)
        .u32(ticket_bell);
    let same_shard = disp.is_some_and(|(_, _, ds)| ds.key == shard.key);
    let mut cd = citizen.try_borrow_mut_data()?;
    let mut jd = if won {
        Some(shard.try_borrow_mut_data()?)
    } else {
        None
    };
    let mut dcd = match disp {
        Some((_, dc, _)) => Some(dc.try_borrow_mut_data()?),
        None => None,
    };
    let mut djd = match disp {
        Some((_, _, ds)) if !same_shard => Some(ds.try_borrow_mut_data()?),
        _ => None,
    };
    let mut hd = if won {
        Some(holding.try_borrow_mut_data()?)
    } else {
        None
    };
    let mut pd = if outcome != settle_outcome::TAKEN || exhausted {
        Some(province.try_borrow_mut_data()?)
    } else {
        None
    };
    let mut od = Vec::with_capacity(others.len());
    if ends {
        for ai in others {
            od.push(ai.try_borrow_mut_data()?);
        }
    }
    let mut chained: Vec<Chained> = Vec::with_capacity(8);
    if let Some(j) = jd.as_mut() {
        chained.push(Chained {
            entity: EntityKind::JoinShard,
            data: j,
        });
    }
    if let Some(j) = djd.as_mut() {
        chained.push(Chained {
            entity: EntityKind::JoinShard,
            data: j,
        });
    }
    chained.push(Chained {
        entity: EntityKind::Citizen,
        data: &mut cd,
    });
    if let Some(c) = dcd.as_mut() {
        chained.push(Chained {
            entity: EntityKind::Citizen,
            data: c,
        });
    }
    if let Some(h) = hd.as_mut() {
        chained.push(Chained {
            entity: EntityKind::Holding,
            data: h,
        });
    }
    if let Some(pv) = pd.as_mut() {
        chained.push(Chained {
            entity: EntityKind::Province,
            data: pv,
        });
    }
    for o in od.iter_mut() {
        chained.push(Chained {
            entity: EntityKind::Province,
            data: o,
        });
    }
    let _ = payer;
    events::emit(Kind::SETTLE, bell, &key9, payload.get()?, &mut chained)
}

/// Clears a site mirror record (64 B at `o`).
fn d_zero(w: &mut Rw, o: usize) -> R<()> {
    w.set_arr(o, &[0u8; SM::SIZE])
}

// ------------------------------------------------------------ ReleaseDormant

fn raw_holding(p: i32, q: i32, site: u8) -> [u8; 9] {
    let mut r = [0u8; 9];
    r[..8].copy_from_slice(&raw_province(p, q));
    r[8] = site;
    r
}

/// The Holding's stored key `(P, Q, site)`; the account must be at its
/// canonical address (`BadAddress`) and present (`BadAccount`).
fn holding_key(p: &Pubkey, ctx: &AddrCtx, id: u64, holding: &AccountInfo) -> R<(i16, i16, u8)> {
    prologue::present(holding, p, AccountKind::Holding, id)?;
    let k = {
        let hd = holding.try_borrow_data()?;
        let r = Ro(&hd);
        (r.i16(H::P)?, r.i16(H::Q)?, r.u8(H::SITE)?)
    };
    expect_key(holding, &ctx.holding(k.0 as i32, k.1 as i32, k.2))?;
    Ok(k)
}

/// Moves `pool_owed` from the Holding to the DefencePool (N-class closes)
/// and logs `POOL_SWEEP` when there is any.
fn sweep_pool_owed(holding: &AccountInfo, dpool: &AccountInfo, raw: &[u8; 9], bell: u32) -> R<u64> {
    let owed = {
        let hd = holding.try_borrow_data()?;
        Ro(&hd).u64(H::POOL_OWED)?
    };
    if owed == 0 {
        return Ok(0);
    }
    init::move_lamports(holding, dpool, owed)?;
    let mut hd = holding.try_borrow_mut_data()?;
    Rw(&mut hd).set_u64(H::POOL_OWED, 0)?;
    let payload = Buf::<8>::new().u64(owed);
    events::emit(
        Kind::POOL_SWEEP,
        bell,
        raw,
        payload.get()?,
        &mut [Chained {
            entity: EntityKind::Holding,
            data: &mut hd,
        }],
    )?;
    Ok(owed)
}

/// 0x35 ReleaseDormant: `[any s] [season] [holding w] [province w]
/// [citizen w] [joinshard w] [rent_payer w] [dpool w]`. Class N.
///
/// Status Running. The Holding (canonical from its stored key) is a first
/// holding (`order == 1`) whose owner has not acted for `release_after`
/// (`NotDormant`) and has no transit in state 1–3 (`HasTransits`); its
/// Province, owner Citizen, the owner's JoinShard, `rent_payer` and the
/// DefencePool. Effects: the site becomes released-free (`gen` kept, so
/// the next founding bumps it and the old hosts are stranded), the
/// Province's `n_sites_used −= 1`, `roster_epoch += 1`; the Citizen gets the
/// refugee flag, loses its holding (flags 4 and 8 cleared: it may file a
/// new ticket); JoinShard `holdings −= 1`, `released += 1`; `pool_owed` →
/// DefencePool (`POOL_SWEEP`); the Holding closes to its rent payer. Logs
/// `RELEASE`, then `CLOSE` (Holding).
pub fn release_dormant(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::ReleaseDormant, a, None)?;
    aix::ReleaseDormant::decode(d)?;
    let [_any, season_ai, holding, province, citizen, shard, rent_payer, dpool] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let now = prologue::now()?;
    let hdr = prologue::season(
        season_ai,
        p,
        Some(&crate::RULESET_HASH),
        &[S::STATUS_RUNNING],
        now.ts,
    )?;
    let release_after = {
        let sd = season_ai.try_borrow_data()?;
        Ro(&sd).u32(S::RELEASE_AFTER_SECS)? as i64
    };
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    let (hp, hq, site) = holding_key(p, &ctx, hdr.id, holding)?;
    let (pi, qi) = (hp as i32, hq as i32);
    let (order, last, owner, h_payer, gen, has_transit) = {
        let hd = holding.try_borrow_data()?;
        let r = Ro(&hd);
        let mut transit = false;
        for i in 0..H::TRANSIT_N {
            if T::in_transit(r.u8(H::transit(i) + T::STATE)?) {
                transit = true;
            }
        }
        (
            r.u8(H::ORDER)?,
            r.i64(H::LAST_OWNER_ACTION)?,
            r.arr::<32>(H::OWNER_CITIZEN)?,
            r.arr::<32>(H::RENT_PAYER)?,
            r.u8(H::GEN)?,
            transit,
        )
    };
    if order != 1 || now.ts < last.saturating_add(release_after) {
        return Err(FrontierError::NotDormant.into());
    }
    if has_transit {
        return Err(FrontierError::NotDormant.into());
    }
    present_at(
        province,
        &ctx.province(pi, qi),
        p,
        AccountKind::Province,
        hdr.id,
    )?;
    expect_key(citizen, &owner)?;
    prologue::present(citizen, p, AccountKind::Citizen, hdr.id)?;
    let (faction, shard_i, my_tag) = {
        let cd = citizen.try_borrow_data()?;
        let r = Ro(&cd);
        (
            r.u8(C::FACTION)?,
            r.u8(C::JOIN_SHARD)?,
            r.u64(C::CITIZEN_TAG)?,
        )
    };
    present_at(
        shard,
        &ctx.join_shard(faction, shard_i),
        p,
        AccountKind::JoinShard,
        hdr.id,
    )?;
    expect_key(rent_payer, &h_payer)?;
    present_at(
        dpool,
        &ctx.defence_pool(),
        p,
        AccountKind::DefencePool,
        hdr.id,
    )?;
    let bell = log_bell(&hdr, now.ts);
    let wedge = ProvinceCoord::new(pi, qi).wedge().unwrap_or(0);
    {
        let mut pd = province.try_borrow_mut_data()?;
        let mut w = Rw(&mut pd);
        let o = PV::site(site as usize);
        if w.u8(o + SM::STATE)? != SM::STATE_HOLDING || w.u8(o + SM::GEN)? != gen {
            return Err(BAD_ACCOUNT);
        }
        d_zero(&mut w, o)?;
        w.set_u8(o + SM::STATE, SM::STATE_RELEASED_FREE)?;
        w.set_u8(o + SM::FACTION, NEUTRAL)?;
        w.set_u8(o + SM::GEN, gen)?;
        w.set_u32(o + SM::PEND0_BELL, SM::NO_BELL)?;
        w.set_u32(o + SM::PEND1_BELL, SM::NO_BELL)?;
        let u = w.u8(PV::N_SITES_USED)?;
        w.set_u8(PV::N_SITES_USED, u.saturating_sub(1))?;
        let e = w.u32(PV::ROSTER_EPOCH)?;
        w.set_u32(PV::ROSTER_EPOCH, e.wrapping_add(1))?;
    }
    {
        let mut cd = citizen.try_borrow_mut_data()?;
        let f = Ro(&cd).u8(C::FLAGS)?;
        let mut w = Rw(&mut cd);
        w.set_u8(
            C::FLAGS,
            (f | C::FLAG_REFUGEE) & !(C::FLAG_PROVISIONAL | C::FLAG_FIRST_HOLDING_FINAL),
        )?;
        w.set_u8(C::HOLDINGS_N, 0)?;
        write_holding_ref(&mut cd, 0, 0, 0, 0)?;
    }
    {
        let mut jd = shard.try_borrow_mut_data()?;
        shard_holdings(&mut jd, wedge, -1)?;
        Rw(&mut jd).add_u32(JS::RELEASED, 1)?;
    }
    let raw = raw_holding(pi, qi, site);
    {
        let mut jd = shard.try_borrow_mut_data()?;
        let mut cd = citizen.try_borrow_mut_data()?;
        let mut hd = holding.try_borrow_mut_data()?;
        let mut pd = province.try_borrow_mut_data()?;
        let payload = Buf::<8>::new().u64(my_tag);
        events::emit(
            Kind::RELEASE,
            bell,
            &raw,
            payload.get()?,
            &mut [
                Chained {
                    entity: EntityKind::JoinShard,
                    data: &mut jd,
                },
                Chained {
                    entity: EntityKind::Citizen,
                    data: &mut cd,
                },
                Chained {
                    entity: EntityKind::Holding,
                    data: &mut hd,
                },
                Chained {
                    entity: EntityKind::Province,
                    data: &mut pd,
                },
            ],
        )?;
    }
    close_holding_to(p, holding, rent_payer, dpool, &raw, bell)
}

/// Sweeps `pool_owed`, logs `CLOSE` and closes the Holding to `rent_payer`
/// (its escrow and rent).
fn close_holding_to<'a>(
    p: &Pubkey,
    holding: &AccountInfo<'a>,
    rent_payer: &AccountInfo<'a>,
    dpool: &AccountInfo<'a>,
    raw: &[u8; 9],
    bell: u32,
) -> R<()> {
    sweep_pool_owed(holding, dpool, raw, bell)?;
    {
        let lamports = holding.lamports();
        let mut hd = holding.try_borrow_mut_data()?;
        emit_close(
            AccountKind::Holding,
            EntityKind::Holding,
            raw,
            &mut hd,
            &key(rent_payer),
            lamports,
            bell,
        )?;
    }
    init::close_to(p, holding, rent_payer, &Sink::DefencePool(dpool), bell)?;
    Ok(())
}

// ------------------------------------------------------------ season-end closes

/// 0x36 CloseHolding: `[any s] [season] [holding w] [rent_payer w] [dpool
/// w]`. Class N. Ended with `now ≥ end + 72 h`, or Aborted. `pool_owed` →
/// DefencePool (`POOL_SWEEP`); the Holding (escrow of unsettled transits
/// included) closes to its `rent_payer`. Log `CLOSE`.
pub fn close_holding(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::CloseHolding, a, None)?;
    aix::CloseHolding::decode(d)?;
    let [_any, season_ai, holding, rent_payer, dpool] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let now = prologue::now()?;
    let hdr = season_end_close(season_ai, p, now.ts)?;
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    let (hp, hq, site) = holding_key(p, &ctx, hdr.id, holding)?;
    let h_payer = {
        let hd = holding.try_borrow_data()?;
        Ro(&hd).arr::<32>(H::RENT_PAYER)?
    };
    expect_key(rent_payer, &h_payer)?;
    present_at(
        dpool,
        &ctx.defence_pool(),
        p,
        AccountKind::DefencePool,
        hdr.id,
    )?;
    let raw = raw_holding(hp as i32, hq as i32, site);
    close_holding_to(p, holding, rent_payer, dpool, &raw, log_bell(&hdr, now.ts))
}

/// 0x37 CloseCitizen: `[any s] [season] [citizen w] [rent_payer w]
/// [ticket_funder w]?`. Class N. Ended with `now ≥ end + 72 h`, or
/// Aborted. The escrow of an open or exhausted ticket goes back to
/// `ticket_funder` (listed exactly when there is escrow:
/// `TooManyAccounts`), the rest to `rent_payer`. Log `CLOSE`.
pub fn close_citizen(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::CloseCitizen, a, None)?;
    aix::CloseCitizen::decode(d)?;
    let (season_ai, citizen, rent_payer, funder) = match a {
        [_any, s, c, r] => (s, c, r, None),
        [_any, s, c, r, f] => (s, c, r, Some(f)),
        _ => return Err(FrontierError::TooManyAccounts.into()),
    };
    let now = prologue::now()?;
    let hdr = season_end_close(season_ai, p, now.ts)?;
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    prologue::present(citizen, p, AccountKind::Citizen, hdr.id)?;
    let (tag15, c_payer, escrow, c_funder) = {
        let cd = citizen.try_borrow_data()?;
        let r = Ro(&cd);
        (
            tag15_of(&cd)?,
            r.arr::<32>(C::RENT_PAYER)?,
            r.u64(C::TICKET_ESCROW)?,
            r.arr::<32>(C::TICKET_FUNDER)?,
        )
    };
    expect_key(citizen, &ctx.citizen_by_tag15(&tag15))?;
    expect_key(rent_payer, &c_payer)?;
    match (escrow > 0, funder) {
        (true, Some(f)) => {
            expect_key(f, &c_funder)?;
            init::move_lamports(citizen, f, escrow)?;
            let mut cd = citizen.try_borrow_mut_data()?;
            Rw(&mut cd).set_u64(C::TICKET_ESCROW, 0)?;
        }
        (false, None) => {}
        _ => return Err(FrontierError::TooManyAccounts.into()),
    }
    let bell = log_bell(&hdr, now.ts);
    {
        let lamports = citizen.lamports();
        let mut cd = citizen.try_borrow_mut_data()?;
        emit_close(
            AccountKind::Citizen,
            EntityKind::Citizen,
            &tag15,
            &mut cd,
            &key(rent_payer),
            lamports,
            bell,
        )?;
    }
    // A whole-account refund (≥ rent(0)) never diverts (§4.2); the sink is
    // never used.
    init::close_to(p, citizen, rent_payer, &Sink::Never, bell)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displacement_order_is_score_then_lower_tag() {
        assert!(displaces(10, 5, 9, 1));
        assert!(!displaces(9, 5, 10, 1));
        assert!(displaces(10, 1, 10, 2));
        assert!(!displaces(10, 2, 10, 1));
        assert!(!displaces(10, 2, 10, 2));
    }

    #[test]
    fn wedge_rule_allows_adjacent_outermost_only_when_full() {
        assert!(wedge_allowed(2, 2, 3, 10, 0, 4));
        assert!(!wedge_allowed(2, 3, 3, 10, 9, 4));
        assert!(wedge_allowed(2, 3, 3, 10, 10, 4));
        assert!(wedge_allowed(2, 1, 3, 10, 10, 4));
        assert!(!wedge_allowed(2, 4, 3, 10, 10, 4));
        assert!(!wedge_allowed(2, 3, 2, 10, 10, 4));
        assert!(wedge_allowed(0, 5, 3, 0, 0, 4));
    }

    #[test]
    fn cohorts_file_reuse_and_settle() {
        let mut d = alloc::vec![0u8; PV::SIZE];
        cohort_file(&mut d, 100).unwrap();
        cohort_file(&mut d, 100).unwrap();
        let o = PV::cohort(0);
        assert_eq!(Ro(&d).u16(o + CO::FILED).unwrap(), 2);
        for b in 101..108 {
            cohort_file(&mut d, b).unwrap();
        }
        // eight open cohorts: full
        assert_eq!(
            cohort_file(&mut d, 108),
            Err(FrontierError::CohortFull.into())
        );
        // settling bell 101's only ticket frees its record
        cohort_settle(&mut d, 101).unwrap();
        cohort_file(&mut d, 108).unwrap();
        assert_eq!(Ro(&d).u32(PV::cohort(1) + CO::BELL).unwrap(), 108);
        // expiry frees a record after 24 bells
        cohort_file(&mut d, 124).unwrap();
        assert_eq!(Ro(&d).u32(PV::cohort(0) + CO::BELL).unwrap(), 124);
        // a settle of a vanished cohort is a no-op
        cohort_settle(&mut d, 100).unwrap();
        assert!(ap::cohort_closed(&d, 100, 124));
    }

    #[test]
    fn ticket_score_binds_every_field() {
        let s = [1u8; 32];
        let a = ticket_score(&s, 2, -1, 3, 7);
        assert_ne!(a, ticket_score(&s, 2, -1, 3, 8));
        assert_ne!(a, ticket_score(&s, 2, -1, 4, 7));
        assert_ne!(a, ticket_score(&s, -1, 2, 3, 7));
        assert_ne!(a, ticket_score(&[2u8; 32], 2, -1, 3, 7));
    }

    #[test]
    fn founded_holding_has_the_kit_and_base_production() {
        let h = founded_holding(1_000_000, 3).unwrap();
        assert_eq!(h.order, 1);
        assert_eq!(h.production, catalog::base_production(Tier::Hamlet));
        let stock = h.stock_at(1_000_000);
        assert_eq!(stock, catalog::starter_kit());
    }
}
