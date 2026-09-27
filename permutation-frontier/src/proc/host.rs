//! Hosts and departures (M1 contract §5.10, §5.11): Muster (0x43),
//! Dissolve (0x44), Garrison (0x45), DisbandStranded (0x48), Depart (0x50),
//! SettleDeparture (0x52). Implemented by W3-B.
//!
//! Entries are read and written through `frontier_abi::entry` (the one
//! codec of the 48-B entry); hosts obey the kernel `host::Host` rules:
//! every change issued during bell b is pending until b's clash (roster
//! freeze), one change per host.
//!
//! ## Pinned here (recorded in `W3-B-NOTES.md`)
//!
//! - **Units:** `Holding.reserve` counts whole troops, entries and the site
//!   mirror hold `MilliTroops` (§5.3 note); Muster and Garrison data are
//!   whole troops.
//! - **Muster** needs a passable tile of the holding's province
//!   (`BadData`); the caps (48 over states 1–2, 8 per faction) and a free
//!   entry are `ProvinceFull`; kernel bounds (100–30,000) are `Kernel`
//!   with a sub-code. `dealt_bps` = the faction doctrine at Hold, not
//!   arriving; `n_entries` counts non-free entries.
//! - **Garrison** moves whole troops of the Spearman reserve (the unit the
//!   simulator's garrison purchase prices) into the site mirror's pending
//!   change. **Only positive deltas in M1** (`BadData` otherwise): the
//!   program has no settle step that returns a withdrawal's post-clash
//!   troops to the Holding's reserve, and crediting them at issue would
//!   let a withdrawal issued during bell b keep troops that die in b's
//!   clash (W3-B notes, open item for the contract).
//! - **Depart** charges `march_stamina(32)` (I-32); the transit's
//!   `dealt_bps` is the faction doctrine at Hold arriving (the sealed
//!   stance is applied by Reveal's slot).
//! - **SettleDeparture** of a transit already settled is `AlreadyDone`
//!   (keeper idempotency); `ready_bell_off = ready_bell − depart_bell`.
//! - **DisbandStranded** of a host whose Holding is live with the same
//!   generation is `NotDormant` (46, "not stranded").

use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use frontier_abi::addr::split_host_id;
use frontier_abi::entry::{find_entry, read_entry, write_entry, Entry, EntryOp};
use frontier_abi::ix as aix;
use frontier_abi::layout::AccountKind;
use frontier_abi::log::{EntityKind, Kind, NO_BELL};
use frontier_abi::prologue::{self as ap, HoldingHdr};
use frontier_abi::tags::Ix;
use permutation_rules::fixed::{MilliTroops, MILLI};
use permutation_rules::frontier::catalog;
use permutation_rules::frontier::doctrine::{of_faction, Doctrine};
use permutation_rules::frontier::fees;
use permutation_rules::frontier::host::{GarrisonState, Host, HostError, DESTROYED_BELOW};
use permutation_rules::frontier::stance::{Posture, Stance};
use permutation_rules::frontier::travel::{march_stamina, MAX_PATH_STEPS};

use super::holding::{
    bps16, finality, live, load_touched, own_province, owned_holding, player, site_of, sub,
    write_holding, Player,
};
use crate::addr;
use crate::error::{kernel, BAD_ACCOUNT, OVERFLOW};
use crate::events::{self, Buf, Chained};
use crate::init;
use crate::layout::{
    citizen as C, entry as E, holding as H, province as P, season as S, site as SM, transit as T,
    Ro, Rw,
};
use crate::prologue::{self, check_accounts, expect_key, key};
use crate::{FrontierError, R, RULESET_HASH};

/// A kernel host refusal as a program code. `Unresolved` is the caller's
/// (`NotResident` for resident actions, `TooEarly` for settlements).
pub(crate) fn host_err(e: HostError, unresolved: FrontierError) -> crate::Error {
    match e {
        HostError::NoStamina | HostError::Cooldown => FrontierError::Cooldown.into(),
        HostError::Busy | HostError::Unsettled => FrontierError::HostBusy.into(),
        HostError::Unresolved => unresolved.into(),
        HostError::TooSmall => kernel(sub::HOST_TOO_SMALL),
        HostError::TooLarge => kernel(sub::HOST_TOO_LARGE),
        HostError::NotAHostUnit => kernel(sub::HOST_NOT_A_HOST_UNIT),
        HostError::Mismatch => kernel(sub::HOST_MISMATCH),
        HostError::TimeReversed => kernel(sub::HOST_TIME_REVERSED),
    }
}

/// The doctrine of `faction` (a stored faction outside 0..6 is `BadAccount`).
fn doctrine(faction: u8) -> R<&'static Doctrine> {
    of_faction(faction).ok_or(BAD_ACCOUNT)
}

/// A resident host of the caller, found in the Province it stands in.
pub(crate) struct ResidentHost {
    pub entry: Entry,
    pub index: usize,
    /// The province the host stands in.
    pub p: i16,
    pub q: i16,
}

/// The Province an account claims to be, at its canonical address
/// (`BadAddress`) and present (`BadAccount`): `(P, Q, resolved_next)`.
pub(crate) fn province_at(
    program: &Pubkey,
    pl: &Player,
    province: &AccountInfo,
) -> R<(i16, i16, u32)> {
    prologue::present(province, program, AccountKind::Province, pl.pc.season.id)?;
    let (pp, pq, rn) = {
        let pd = province.try_borrow_data()?;
        let r = Ro(&pd);
        (r.i16(P::P)?, r.i16(P::Q)?, r.u32(P::RESOLVED_NEXT)?)
    };
    expect_key(province, &pl.ctx.province(pp as i32, pq as i32))?;
    Ok((pp, pq, rn))
}

/// The resident checks of Dissolve and Explore in the order of §5.6 and
/// §5.10: the host id names this holding and generation (`NotOwner`); the
/// host is in no transit record in state 1–3 (`HostInTransit`, step 6);
/// the province it stands in (canonical, present; the lazy finality flip
/// when it is the holding's own); the holding final (`NotFinal`); the
/// province resolved through b − 2 and the host in its roster, state 1
/// (`NotResident`).
pub(crate) fn resident_host(
    program: &Pubkey,
    pl: &Player,
    hh: &HoldingHdr,
    holding: &AccountInfo,
    citizen: &AccountInfo,
    province: &AccountInfo,
    host_id: u64,
) -> R<ResidentHost> {
    caller_host(hh, host_id)?;
    {
        let hd = holding.try_borrow_data()?;
        if ap::host_in_transit(&hd, host_id) {
            return Err(FrontierError::HostInTransit.into());
        }
    }
    let (pp, pq, rn) = province_at(program, pl, province)?;
    let state = if (pp, pq) == (hh.p, hh.q) {
        finality(pl, hh, holding, citizen, province)?
    } else {
        hh.state
    };
    if state != H::STATE_FINAL {
        return Err(FrontierError::NotFinal.into());
    }
    if !ap::resident_ok(rn, pl.pc.now_bell) {
        return Err(FrontierError::NotResident.into());
    }
    let pd = province.try_borrow_data()?;
    let index = find_entry(&pd, host_id).ok_or(FrontierError::NotResident)?;
    let entry = read_entry(&pd, index).map_err(|_| BAD_ACCOUNT)?;
    if entry.state != E::STATE_ROSTER {
        return Err(FrontierError::NotResident.into());
    }
    Ok(ResidentHost {
        entry,
        index,
        p: pp,
        q: pq,
    })
}

/// The host id names the holding `hh` and its current generation
/// (`NotOwner`).
fn caller_host(hh: &HoldingHdr, host_id: u64) -> R<()> {
    let parts = split_host_id(host_id).ok_or(FrontierError::NotOwner)?;
    if parts.province.p != hh.p as i32
        || parts.province.q != hh.q as i32
        || parts.site != hh.site
        || parts.gen != hh.gen
    {
        return Err(FrontierError::NotOwner.into());
    }
    Ok(())
}

/// Writes entry `i` of a Province.
fn put_entry(province: &AccountInfo, i: usize, e: &Entry) -> R<()> {
    let mut pd = province.try_borrow_mut_data()?;
    write_entry(&mut pd, i, e).map_err(|_| BAD_ACCOUNT)
}

/// `roster_epoch += 1`.
fn bump_epoch(w: &mut Rw) -> R<()> {
    let e = w.u32(P::ROSTER_EPOCH)?.wrapping_add(1);
    w.set_u32(P::ROSTER_EPOCH, e)
}

// ------------------------------------------------------------ Muster

/// 0x43 Muster(unit, troops, tile): P + `[holding w] [province w]` (the
/// holding's). Troops come from `reserve[unit]` (whole troops) and form a
/// muster-pending entry (state 2) joining the roster at `now_bell + 1`.
pub fn muster(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::Muster, a, None)?;
    let x = aix::Muster::decode(d)?;
    let pl = player(p, a)?;
    let [_, _, _, citizen, holding, province] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let hh = owned_holding(p, &pl, holding, citizen)?;
    live(&hh)?;
    own_province(p, &pl, &hh, province)?;
    let state = finality(&pl, &hh, holding, citizen, province)?;
    let now = pl.now.ts;
    let h = load_touched(holding, now)?;
    if state != H::STATE_FINAL {
        return Err(FrontierError::NotFinal.into());
    }
    let b = pl.pc.now_bell;
    let (rn, passable) = {
        let pd = province.try_borrow_data()?;
        let r = Ro(&pd);
        (r.u32(P::RESOLVED_NEXT)?, r.u64(P::PASSABLE_MASK)?)
    };
    if !ap::resident_ok(rn, b) {
        return Err(FrontierError::NotResident.into());
    }
    let unit = catalog::unit_of(x.unit).ok_or(FrontierError::BadData)?;
    let (faction, gen, seq, have) = {
        let hd = holding.try_borrow_data()?;
        let r = Ro(&hd);
        (
            r.u8(H::FACTION)?,
            r.u8(H::GEN)?,
            r.u32(H::HOST_SEQ)?,
            r.u32(H::reserve(x.unit as usize))?,
        )
    };
    if have < x.troops {
        return Err(FrontierError::Insufficient.into());
    }
    let milli: MilliTroops = x
        .troops
        .checked_mul(MILLI as u32)
        .ok_or_else(|| kernel(sub::HOST_TOO_LARGE))?;
    let id = addr::host_id(hh.p as i32, hh.q as i32, hh.site, gen, seq).ok_or(BAD_ACCOUNT)?;
    let host = Host::muster(
        id,
        frontier_abi::addr::holding_key_of_host(id),
        faction,
        unit,
        milli,
        b,
    )
    .map_err(|e| host_err(e, FrontierError::NotResident))?;
    if x.tile as usize >= P::TILES || passable & (1u64 << x.tile) == 0 {
        return Err(FrontierError::BadData.into());
    }
    let dealt = bps16(doctrine(faction)?.dealt_bps(Posture::Stance(Stance::Hold), false))?;
    // Caps over states 1–2 and a free entry.
    let slot = {
        let pd = province.try_borrow_data()?;
        let (mut total, mut own, mut free) = (0usize, 0usize, None);
        for i in 0..P::ENTRIES_N {
            let o = P::entry(i);
            let st = Ro(&pd).u8(o + E::STATE)?;
            match st {
                E::STATE_FREE => {
                    if free.is_none() {
                        free = Some(i);
                    }
                }
                E::STATE_ROSTER | E::STATE_MUSTER_PENDING => {
                    total += 1;
                    if Ro(&pd).u8(o + E::FACTION)? == faction {
                        own += 1;
                    }
                }
                _ => {}
            }
        }
        if total >= P::ROSTER_CAP || own >= P::FACTION_CAP {
            return Err(FrontierError::ProvinceFull.into());
        }
        free.ok_or(FrontierError::ProvinceFull)?
    };
    let e = Entry::from_host(&host, x.tile, E::STATE_MUSTER_PENDING, dealt, b + 1);
    put_entry(province, slot, &e)?;
    {
        let mut pd = province.try_borrow_mut_data()?;
        let mut w = Rw(&mut pd);
        bump_epoch(&mut w)?;
        let n = w.u8(P::N_ENTRIES)?.saturating_add(1);
        w.set_u8(P::N_ENTRIES, n)?;
    }
    {
        let mut hd = holding.try_borrow_mut_data()?;
        write_holding(&mut hd, &h, now)?;
        let mut w = Rw(&mut hd);
        w.set_u32(H::reserve(x.unit as usize), have - x.troops)?;
        w.set_u32(H::HOST_SEQ, seq.checked_add(1).ok_or(OVERFLOW)?)?;
    }
    let payload = Buf::<7>::new()
        .u8(x.unit)
        .u32(x.troops)
        .u8(x.tile)
        .u8(slot as u8);
    super::holding::emit3(
        Kind::MUSTER,
        b,
        &id.to_le_bytes(),
        payload.get()?,
        citizen,
        holding,
        Some(province),
    )
}

// ------------------------------------------------------------ Dissolve

/// 0x44 Dissolve(host): P + `[holding w] [province w]` (where the host
/// stands). The host (state 1, no pending change) leaves after the bell's
/// clash: pending op `Leave`, its troops back to the reserve at the settle
/// (the resolver's, W4-A).
pub fn dissolve(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::Dissolve, a, None)?;
    let x = aix::Dissolve::decode(d)?;
    let pl = player(p, a)?;
    let [_, _, _, citizen, holding, province] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let hh = owned_holding(p, &pl, holding, citizen)?;
    live(&hh)?;
    let rh = resident_host(p, &pl, &hh, holding, citizen, province, x.host_id)?;
    let now = pl.now.ts;
    let h = load_touched(holding, now)?;
    if rh.entry.busy() {
        return Err(FrontierError::HostBusy.into());
    }
    let b = pl.pc.now_bell;
    let mut e = rh.entry;
    e.op = EntryOp::Leave;
    e.pend_bell = b;
    put_entry(province, rh.index, &e)?;
    {
        let mut hd = holding.try_borrow_mut_data()?;
        write_holding(&mut hd, &h, now)?;
    }
    let payload = Buf::<12>::new().u32(b).i64(e.troops as i64);
    super::holding::emit3(
        Kind::DISSOLVE,
        b,
        &x.host_id.to_le_bytes(),
        payload.get()?,
        citizen,
        holding,
        Some(province),
    )
}

// ------------------------------------------------------------ Garrison

/// The site mirror's garrison as the kernel's `GarrisonState` (an empty
/// pending slot is bell `NO_BELL` or a zero delta).
fn garrison_of(pd: &[u8], site: usize) -> R<GarrisonState> {
    let r = Ro(pd);
    let o = P::site(site);
    let slot = |b: usize, dl: usize| -> R<Option<(u32, i64)>> {
        let (bell, delta) = (r.u32(o + b)?, r.i64(o + dl)?);
        Ok((bell != SM::NO_BELL && delta != 0).then_some((bell, delta)))
    };
    Ok(GarrisonState {
        troops: r.u32(o + SM::GARRISON)?,
        pending: [
            slot(SM::PEND0_BELL, SM::PEND0_DELTA)?,
            slot(SM::PEND1_BELL, SM::PEND1_DELTA)?,
        ],
    })
}

fn put_garrison(pd: &mut [u8], site: usize, g: &GarrisonState) -> R<()> {
    let o = P::site(site);
    let mut w = Rw(pd);
    w.set_u32(o + SM::GARRISON, g.troops)?;
    for (k, (b, dl)) in [
        (SM::PEND0_BELL, SM::PEND0_DELTA),
        (SM::PEND1_BELL, SM::PEND1_DELTA),
    ]
    .into_iter()
    .enumerate()
    {
        let (bell, delta) = g.pending[k].unwrap_or((SM::NO_BELL, 0));
        w.set_u32(o + b, bell)?;
        w.set_i64(o + dl, delta)?;
    }
    Ok(())
}

/// Reserve unit a garrison draws on (module note).
pub const GARRISON_UNIT: usize = 0;

/// 0x45 Garrison(delta): P + `[holding w] [province w]` (the holding's):
/// `delta > 0` whole troops of the Spearman reserve join the garrison
/// after the bell's clash (`GarrisonState::change`).
pub fn garrison(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::Garrison, a, None)?;
    let x = aix::Garrison::decode(d)?;
    let pl = player(p, a)?;
    let [_, _, _, citizen, holding, province] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let hh = owned_holding(p, &pl, holding, citizen)?;
    live(&hh)?;
    own_province(p, &pl, &hh, province)?;
    let state = finality(&pl, &hh, holding, citizen, province)?;
    let now = pl.now.ts;
    let h = load_touched(holding, now)?;
    if state != H::STATE_FINAL {
        return Err(FrontierError::NotFinal.into());
    }
    let b = pl.pc.now_bell;
    let rn = {
        let pd = province.try_borrow_data()?;
        Ro(&pd).u32(P::RESOLVED_NEXT)?
    };
    if !ap::resident_ok(rn, b) {
        return Err(FrontierError::NotResident.into());
    }
    if x.delta <= 0 || x.delta > u32::MAX as i64 {
        return Err(FrontierError::BadData.into());
    }
    let n = x.delta as u32;
    let have = {
        let hd = holding.try_borrow_data()?;
        Ro(&hd).u32(H::reserve(GARRISON_UNIT))?
    };
    if have < n {
        return Err(FrontierError::Insufficient.into());
    }
    let delta_milli = x.delta.checked_mul(MILLI).ok_or(OVERFLOW)?;
    {
        let mut pd = province.try_borrow_mut_data()?;
        let site = site_of(&pd, &hh)?;
        let mut g = garrison_of(&pd, site)?;
        g.change(b, delta_milli, rn)
            .map_err(|e| host_err(e, FrontierError::NotResident))?;
        put_garrison(&mut pd, site, &g)?;
    }
    {
        let mut hd = holding.try_borrow_mut_data()?;
        write_holding(&mut hd, &h, now)?;
        Rw(&mut hd).set_u32(H::reserve(GARRISON_UNIT), have - n)?;
    }
    let key = super::holding::pqs_key(hh.p, hh.q, hh.site)?;
    let payload = Buf::<12>::new().u32(b).i64(delta_milli);
    super::holding::emit3(
        Kind::GARRISON,
        b,
        &key,
        payload.get()?,
        citizen,
        holding,
        Some(province),
    )
}

// ------------------------------------------------------------ DisbandStranded

/// 0x48 DisbandStranded(entry): `[any s] [season] [province w] [holding r
/// (canonical, may be absent)]`, class N. The entry's host id names a
/// Holding that is absent or re-founded (another generation); the entry
/// is freed and its troops are lost.
pub fn disband_stranded(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::DisbandStranded, a, None)?;
    let x = aix::DisbandStranded::decode(d)?;
    let now = prologue::now()?;
    let [_any, season_ai, province, holding] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let hdr = prologue::season(
        season_ai,
        p,
        Some(&RULESET_HASH),
        &[S::STATUS_RUNNING, S::STATUS_ENDED],
        now.ts,
    )?;
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    prologue::present(province, p, AccountKind::Province, hdr.id)?;
    let (pp, pq, e) = {
        let pd = province.try_borrow_data()?;
        let r = Ro(&pd);
        let e = read_entry(&pd, x.entry as usize).map_err(|_| FrontierError::BadData)?;
        (r.i16(P::P)?, r.i16(P::Q)?, e)
    };
    expect_key(province, &ctx.province(pp as i32, pq as i32))?;
    if e.state == E::STATE_FREE {
        return Err(FrontierError::BadData.into());
    }
    let parts = split_host_id(e.id).ok_or(BAD_ACCOUNT)?;
    expect_key(
        holding,
        &ctx.holding(parts.province.p, parts.province.q, parts.site),
    )?;
    if prologue::presence(holding, p, AccountKind::Holding, hdr.id)? {
        let hd = holding.try_borrow_data()?;
        let r = Ro(&hd);
        let live = matches!(r.u8(H::STATE)?, H::STATE_PROVISIONAL | H::STATE_FINAL);
        if live && r.u8(H::GEN)? == parts.gen {
            return Err(FrontierError::NotDormant.into());
        }
    }
    let lost = e.troops;
    {
        let mut pd = province.try_borrow_mut_data()?;
        write_entry(&mut pd, x.entry as usize, &Entry::FREE).map_err(|_| BAD_ACCOUNT)?;
        let mut w = Rw(&mut pd);
        bump_epoch(&mut w)?;
        let n = w.u8(P::N_ENTRIES)?.saturating_sub(1);
        w.set_u8(P::N_ENTRIES, n)?;
    }
    let mut pd = province.try_borrow_mut_data()?;
    events::emit(
        Kind::STRANDED,
        hdr.bell(now.ts).unwrap_or(NO_BELL),
        &e.id.to_le_bytes(),
        &lost.to_le_bytes(),
        &mut [Chained {
            entity: EntityKind::Province,
            data: &mut pd,
        }],
    )
}

// ------------------------------------------------------------ Depart

/// A compressed G2 point's first byte: compression flag set, infinity flag
/// clear (Depart's syntax check; validity is SettleTransit's, I-44).
pub const fn seal_flag_ok(b0: u8) -> bool {
    b0 & 0x80 != 0 && b0 & 0x40 == 0
}

/// The stamina a march is charged at Depart (I-32): the longest path's.
pub const DEPART_STAMINA: u16 = march_stamina(MAX_PATH_STEPS as u32);

/// 0x50 Depart: P + `[holding w] [province w] [system]`; data `host_id,
/// commit, seal, arrive_bell, tip, transit_slot`. Checks in the order of
/// §5.11.
pub fn depart(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::Depart, a, None)?;
    let x = aix::Depart::decode(d)?;
    let pl = player(p, a)?;
    let [_, payer, season_ai, citizen, holding, province, _system] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let hh = owned_holding(p, &pl, holding, citizen)?;
    live(&hh)?;
    let (pp, pq, rn) = province_at(p, &pl, province)?;
    let state = if (pp, pq) == (hh.p, hh.q) {
        finality(&pl, &hh, holding, citizen, province)?
    } else {
        hh.state
    };
    let now = pl.now.ts;
    let b = pl.pc.now_bell;
    let h = load_touched(holding, now)?;
    // 1.
    if state != H::STATE_FINAL {
        return Err(FrontierError::NotFinal.into());
    }
    if !ap::resident_ok(rn, b) {
        return Err(FrontierError::NotResident.into());
    }
    // 2.
    if !seal_flag_ok(x.seal[0]) {
        return Err(FrontierError::BadData.into());
    }
    // 3.
    caller_host(&hh, x.host_id)?;
    let (index, mut e) = {
        let pd = province.try_borrow_data()?;
        let i = find_entry(&pd, x.host_id).ok_or(FrontierError::NotResident)?;
        (i, read_entry(&pd, i).map_err(|_| BAD_ACCOUNT)?)
    };
    if e.state != E::STATE_ROSTER {
        return Err(FrontierError::NotResident.into());
    }
    if e.busy() {
        return Err(FrontierError::HostBusy.into());
    }
    {
        let hd = holding.try_borrow_data()?;
        if ap::host_in_transit(&hd, x.host_id) {
            return Err(FrontierError::HostInTransit.into());
        }
    }
    let mut host = e.to_host().map_err(|_| BAD_ACCOUNT)?;
    host.depart(b, DEPART_STAMINA, rn)
        .map_err(|er| host_err(er, FrontierError::NotResident))?;
    // 4.
    let lo = b.checked_add(2).ok_or(OVERFLOW)?;
    let hi = b.checked_add(72).ok_or(OVERFLOW)?;
    if x.arrive_bell < lo || x.arrive_bell > hi {
        return Err(FrontierError::ArrivalBell.into());
    }
    // 5.
    let (priority, limit, loaded, fee, bond) = {
        let sd = season_ai.try_borrow_data()?;
        let r = Ro(&sd);
        (
            r.u32(S::MIN_REVEAL_PRIORITY_MILLI)?,
            r.u32(S::REVEAL_CU_LIMIT)?,
            r.u32(S::REVEAL_LOADED_LIMIT)?,
            r.u64(S::MARCH_FEE)?,
            r.u64(S::SEAL_BOND)?,
        )
    };
    if x.tip < fees::min_tip_lamports(priority, limit, loaded) {
        return Err(FrontierError::TipTooLow.into());
    }
    if x.transit_slot as usize >= H::TRANSIT_N {
        return Err(FrontierError::BadData.into());
    }
    let t0 = H::transit(x.transit_slot as usize);
    {
        let hd = holding.try_borrow_data()?;
        if Ro(&hd).u8(t0 + T::STATE)? != T::STATE_FREE {
            return Err(FrontierError::TransitState.into());
        }
    }
    let total = x
        .tip
        .checked_add(fee)
        .and_then(|v| v.checked_add(bond))
        .ok_or(OVERFLOW)?;
    if payer.lamports() < total {
        return Err(FrontierError::Insufficient.into());
    }
    // Effects.
    let ct_hash = permutation_rules::hash::sha256(&[&x.seal]);
    let seal_root = permutation_rules::hash::sha256(&[&x.commit, &ct_hash]);
    let dealt = bps16(doctrine(e.faction)?.dealt_bps(Posture::Stance(Stance::Hold), true))?;
    init::transfer(payer, holding, total)?;
    e.set_host(&host);
    put_entry(province, index, &e)?;
    {
        let mut hd = holding.try_borrow_mut_data()?;
        write_holding(&mut hd, &h, now)?;
        let mut w = Rw(&mut hd);
        w.set_u8(t0 + T::STATE, T::STATE_DEPARTED)?;
        w.set_u8(t0 + T::UNIT, e.unit)?;
        w.set_u8(t0 + T::FACTION, e.faction)?;
        w.set_u8(t0 + T::ORIGIN_TILE, e.tile)?;
        w.set_i16(t0 + T::ORIGIN_P, pp)?;
        w.set_i16(t0 + T::ORIGIN_Q, pq)?;
        w.set_u64(t0 + T::HOST_ID, x.host_id)?;
        w.set_u32(t0 + T::DEPART_BELL, b)?;
        w.set_u32(t0 + T::ARRIVE_BELL, x.arrive_bell)?;
        w.set_i64(t0 + T::DEPART_TS, now)?;
        w.set_u32(t0 + T::DEP_MASS, e.troops)?;
        w.set_u16(t0 + T::MARCH_STAMINA, DEPART_STAMINA)?;
        w.set_u16(t0 + T::DEALT_BPS, dealt)?;
        w.set_u32(t0 + T::TROOPS_AFTER, 0)?;
        w.set_u16(t0 + T::STAMINA_AFTER, 0)?;
        w.set_u16(t0 + T::READY_BELL_OFF, 0)?;
        w.set_arr(t0 + T::SEAL_ROOT, &seal_root)?;
        w.set_u64(t0 + T::TIP, x.tip)?;
        w.set_u8(t0 + T::FLAGS, T::FLAG_FEE_ESCROWED | T::FLAG_BOND_ESCROWED)?;
        w.add_u64(H::ESCROW, total)?;
    }
    {
        let mut cd = citizen.try_borrow_mut_data()?;
        Rw(&mut cd).add_u32(C::ARRIVALS, 1)?;
    }
    let payload = Buf::<260>::new()
        .i32(pp as i32)
        .i32(pq as i32)
        .u8(e.tile)
        .u32(b)
        .u32(x.arrive_bell)
        .u32(e.troops)
        .u16(DEPART_STAMINA)
        .u64(x.tip)
        .bytes(&seal_root)
        .bytes(&x.commit)
        .bytes(&x.seal);
    super::holding::emit3(
        Kind::DEPART,
        b,
        &x.host_id.to_le_bytes(),
        payload.get()?,
        citizen,
        holding,
        Some(province),
    )
}

// ------------------------------------------------------------ SettleDeparture

/// 0x52 SettleDeparture(transit_slot): `[payer s] [season] [origin
/// province w] [holding w]`, class D. Once the origin resolved the
/// departure bell, the departed entry's post-clash values move into the
/// transit record (state 2, or 3 when destroyed at the origin) and the
/// entry is freed (I-11, D4).
pub fn settle_departure(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::SettleDeparture, a, None)?;
    let x = aix::SettleDeparture::decode(d)?;
    let now = prologue::now()?;
    let [_payer, season_ai, province, holding] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let hdr = prologue::season(
        season_ai,
        p,
        Some(&RULESET_HASH),
        &[S::STATUS_RUNNING, S::STATUS_ENDED],
        now.ts,
    )?;
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    prologue::present(holding, p, AccountKind::Holding, hdr.id)?;
    if x.transit_slot as usize >= H::TRANSIT_N {
        return Err(FrontierError::BadData.into());
    }
    let t0 = H::transit(x.transit_slot as usize);
    let (state, host_id, op, oq, depart_bell) = {
        let hd = holding.try_borrow_data()?;
        let r = Ro(&hd);
        let (hp, hq, site) = (r.i16(H::P)?, r.i16(H::Q)?, r.u8(H::SITE)?);
        expect_key(holding, &ctx.holding(hp as i32, hq as i32, site))?;
        (
            r.u8(t0 + T::STATE)?,
            r.u64(t0 + T::HOST_ID)?,
            r.i16(t0 + T::ORIGIN_P)?,
            r.i16(t0 + T::ORIGIN_Q)?,
            r.u32(t0 + T::DEPART_BELL)?,
        )
    };
    match state {
        T::STATE_DEPARTED => {}
        T::STATE_SETTLED | T::STATE_DESTROYED_AT_ORIGIN => {
            return Err(FrontierError::AlreadyDone.into())
        }
        _ => return Err(FrontierError::TransitState.into()),
    }
    expect_key(province, &ctx.province(op as i32, oq as i32))?;
    prologue::present(province, p, AccountKind::Province, hdr.id)?;
    let (rn, index, e) = {
        let pd = province.try_borrow_data()?;
        let rn = Ro(&pd).u32(P::RESOLVED_NEXT)?;
        if rn <= depart_bell {
            return Err(FrontierError::TooEarly.into());
        }
        let i = find_entry(&pd, host_id).ok_or(FrontierError::NotResident)?;
        (rn, i, read_entry(&pd, i).map_err(|_| BAD_ACCOUNT)?)
    };
    if e.state != E::STATE_DEPARTED {
        return Err(FrontierError::NotResident.into());
    }
    let host = e.to_host().map_err(|_| BAD_ACCOUNT)?;
    let (troops, stamina) = host
        .march_values(depart_bell, rn)
        .map_err(|er| host_err(er, FrontierError::TooEarly))?;
    let destroyed = troops < DESTROYED_BELOW;
    let (troops, stamina) = if destroyed { (0, 0) } else { (troops, stamina) };
    let ready_off = e
        .ready_bell
        .saturating_sub(depart_bell)
        .min(u16::MAX as u32) as u16;
    {
        let mut pd = province.try_borrow_mut_data()?;
        write_entry(&mut pd, index, &Entry::FREE).map_err(|_| BAD_ACCOUNT)?;
        let mut w = Rw(&mut pd);
        let n = w.u8(P::N_ENTRIES)?.saturating_sub(1);
        w.set_u8(P::N_ENTRIES, n)?;
    }
    {
        let mut hd = holding.try_borrow_mut_data()?;
        let mut w = Rw(&mut hd);
        w.set_u8(
            t0 + T::STATE,
            if destroyed {
                T::STATE_DESTROYED_AT_ORIGIN
            } else {
                T::STATE_SETTLED
            },
        )?;
        w.set_u32(t0 + T::TROOPS_AFTER, troops)?;
        w.set_u16(t0 + T::STAMINA_AFTER, stamina)?;
        w.set_u16(t0 + T::READY_BELL_OFF, ready_off)?;
    }
    let payload = Buf::<7>::new().u32(troops).u16(stamina).u8(destroyed as u8);
    let mut hd = holding.try_borrow_mut_data()?;
    let mut pd = province.try_borrow_mut_data()?;
    events::emit(
        Kind::DEPARTURE_SETTLED,
        hdr.bell(now.ts).unwrap_or(NO_BELL),
        &host_id.to_le_bytes(),
        payload.get()?,
        &mut [
            Chained {
                entity: EntityKind::Holding,
                data: &mut hd,
            },
            Chained {
                entity: EntityKind::Province,
                data: &mut pd,
            },
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_syntax_and_depart_stamina() {
        assert!(seal_flag_ok(0x80));
        assert!(seal_flag_ok(0xA5));
        assert!(!seal_flag_ok(0x00));
        assert!(!seal_flag_ok(0xC0), "infinity");
        assert_eq!(DEPART_STAMINA, 74);
    }

    #[test]
    fn garrison_mirror_round_trips() {
        let mut pd = alloc::vec![0u8; P::SIZE];
        let g = GarrisonState {
            troops: 5_000_000,
            pending: [Some((7, 100_000)), None],
        };
        put_garrison(&mut pd, 3, &g).unwrap();
        assert_eq!(garrison_of(&pd, 3).unwrap(), g);
        // a zeroed mirror reads as empty pending slots
        assert_eq!(
            garrison_of(&alloc::vec![0u8; P::SIZE], 0).unwrap(),
            GarrisonState::new(0)
        );
    }
}
