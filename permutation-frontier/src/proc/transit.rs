//! Transits (M1 contract §5.11, §5.12): SettleTransit (0x54, class D, the
//! seal proof of I-44) and SweepPoolOwed (0x55, class N). Implemented by
//! W4-B.
//!
//! ## SettleTransit
//!
//! Accounts `[0 payer s,w] [1 season] [2 holding w] [3 dest province w] [4
//! inputs w] [5 slot w] [6 home province w] [7 anchor|archive r] [8
//! slot_beneficiary w] [9 resolver w] [10 holding_rent_payer w] [11
//! settle_beneficiary w] [12 system]`; data `transit_slot, commit, seal,
//! beneficiary`. Checks in the order of §5.11:
//!
//! 1. the Holding (present, canonical from its stored key) and its transit
//!    record in state 2 or 3 (`DepartureUnsettled` for 1, `TransitState`
//!    for 0);
//! 2. `now ≥ close(arrive, r_dest) + 600` and `dest.resolved_next > arrive`
//!    (`TooEarly`); `A` from THE anchor `an‖(arrive, r_dest)` or, once it
//!    is archived, from the region-half-day archive's entry (`NoAnchor`
//!    when neither holds the bell);
//! 3. `sha256(commit ‖ sha256(seal)) == seal_root` (`CommitMismatch`);
//! 4. **the proof**: the round-T(arrive) signature from THE anchor or the
//!    archive entry opens the seal ([`crate::crypto::seal::judge`]: FO
//!    check, commitment, `Plain::validate`); a valid seal's destination
//!    must be the supplied Province (`BadAddress`);
//! 5. `settle_beneficiary == beneficiary` (`BadAddress`); the ClashInputs
//!    `ci‖(dest, arrive)` count only when present with flag 2 (I-46).
//!
//! Outcomes (D5, order-free; `TRANSIT_SETTLED.outcome`):
//!
//! | branch | host | tip | march fee | bond |
//! |---|---|---|---|---|
//! | bad seal (codes 1, 2, 4, 5) | destroyed; a Stays/Withdrew entry at the destination gets `Forfeit` at `now_bell` | reward | reward | reward |
//! | in the final set, fate 1–5 | Stays/Withdrew stay; Bounced/Retreated return; Destroyed gone | its slot's beneficiary | resolver | rent payer |
//! | not in the set, outranked (bounce, 6) | returns, no loss | rent payer | resolver | rent payer |
//! | not in the set, would have been admitted (routed, 7) | returns with `rout_survivors`, stamina 0 | `pool_owed` | resolver | rent payer |
//! | no resolved inputs (routed, 7) | as above | `pool_owed` | `pool_owed` | rent payer |
//!
//! "reward" = tip + march fee + bond to `settle_beneficiary`. Every payment
//! goes through `pay_or_divert` with sink `Holding.pool_owed`; no
//! DefencePool account is listed (I-48).
//!
//! ## Pinned here (recorded in `W4-B-NOTES.md`)
//!
//! - **The slot account** (v1.5): the ArrivalSlot `(dest, arrive, faction,
//!   i)` with `i` = the host's position in the resolved final set mod 4,
//!   or `i = 0` when the host is not in a resolved set (a present slot of
//!   another host there is "no slot of this host" and is never paid).
//!   With resolved inputs a host has a slot at the destination iff it is
//!   in the final set (a present slot cannot be skipped by a gather, and
//!   the latch stops reveals once inputs exist), so the index is decided
//!   by the records, not by the settler.
//! - **A final-set host whose slot is gone** (closed at season end + 72 h,
//!   CloseArrivalSlot case b): its tip goes to `pool_owed`.
//! - **The slot's rent**: SettleTransit's account list has no `rent_to`
//!   position (§5.11). A slot whose `rent_to` is one of the listed wallets
//!   (payer, slot beneficiary, resolver, rent payer, settle beneficiary) is
//!   closed to it at once when it is not kept for a claim; otherwise its
//!   `settled` flag is set and CloseArrivalSlot (case a) closes it to its
//!   `rent_to` after the claim grace. Either way the rent returns to the
//!   Reveal's fee payer (I-49).
//! - **Claim-eligible** (I-52): THE anchor present (an archived bell is
//!   past any grace), `claimed == 0`, `ev_slot − anchor.slot ≥
//!   lateness_slots`, `fees::defence_refund > 0` (a price above the tip
//!   level) and `now < close + 6 × 600`.
//! - **Returning hosts** rejoin the home Province (the Holding's own) as a
//!   muster-pending entry (state 2, `from_bell = now_bell + 1`) when a
//!   free entry and the caps (48 over states 1–2, 8 per faction) allow,
//!   else `reserve[unit] += troops / 1,000`; a host below
//!   `DESTROYED_BELOW` returns nothing. Values: Bounced/Retreated from the
//!   final set carry the record's `troops_after` and its stamina at the
//!   arrival bell; a bounce by rank carries the transit's post-origin
//!   values (no loss); a routed host `rout_survivors(troops)` and stamina
//!   0 at `now_bell`. `ready_bell = max(depart_bell + ready_bell_off,
//!   now_bell + 1)`; `dealt_bps` = the doctrine at Hold (as Muster).
//! - **Payment recipients** are checked only when paid (`BadAddress`):
//!   `slot_beneficiary == slot.beneficiary`, `resolver ==
//!   inputs.resolver`; `holding_rent_payer == Holding.rent_payer` always.
//!   A recipient owned by this program is never credited directly: the
//!   amount stays in the Holding as `pool_owed` (`DIVERT`), so every
//!   lamport a transit escrowed is accounted.
//! - `TRANSIT_SETTLED` payments: `*_to` is the first 8 bytes of the
//!   intended recipient (the Holding's own address for `pool_owed`);
//!   the bad-seal branch logs `tip`, `fee`, `bond` as 0 and the whole
//!   escrow as `reward`; `pool_owed_delta` counts routed and diverted
//!   lamports.
//!
//! **Open finding (W4-B F1, notes):** for a seal judged bad with codes 1,
//! 2 or 4 the destination cannot be read from the seal, so the program
//! uses the settler's Province; a host revealed (by its owner, who knows
//! the salt) to another Province and staying there escapes the `Forfeit`
//! if its owner settles first and names a wrong Province. Nothing the
//! program has written ties the destination to the transit (Reveal writes
//! only the slot, GatherClash reads the Holding): the fix needs a contract
//! decision.

use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use frontier_abi::addr::split_host_id;
use frontier_abi::entry::{find_entry, read_entry, unit_from_u8, write_entry, Entry, EntryOp};
use frontier_abi::ix as aix;
use frontier_abi::layout::AccountKind;
use frontier_abi::log::{divert_reason, seal_code, transit_outcome as TO, EntityKind, Kind};
use frontier_abi::tags::Ix;
use permutation_rules::frontier::clash::{admit_arrival, SlotDecision, SlotEntry};
use permutation_rules::frontier::doctrine::of_faction;
use permutation_rules::frontier::fees::{self, DefenceParams};
use permutation_rules::frontier::geometry::{region_of, ProvinceCoord};
use permutation_rules::frontier::host::{
    effective_bell, rout_survivors, Host, Stamina, DESTROYED_BELOW,
};
use permutation_rules::frontier::stance::{Posture, Stance};

use super::beacon::{the_anchor_or_archive, AnchorSource};
use crate::addr::{self, AddrCtx};
use crate::clock::SeasonClock;
use crate::crypto::{seal, sys};
use crate::error::{crypto, BAD_ACCOUNT, OVERFLOW};
use crate::events::{self, Buf, Chained};
use crate::init::{self, Paid, Sink};
use crate::layout::{
    arrival as AR, arrival_slot as AS, clash_inputs as CI, defence_claim as DCL, entry as E,
    holding as H, province as P, season as S, transit as T, Ro, Rw,
};
use crate::prologue::{self, check_accounts, expect_key, key};
use crate::{FrontierError, R, RULESET_HASH};

/// Seconds after the reveal close before a transit may settle (§5.1).
pub const SETTLE_AFTER_CLOSE_SECS: i64 = 600;
/// Bells a claim-eligible slot stays open after its close (I-52).
pub const CLAIM_GRACE_BELLS: u32 = DCL::CLAIM_GRACE_BELLS;
/// Seconds per bell.
const BELL_SECS: i64 = 600;

/// The seasons SettleTransit and SweepPoolOwed run in.
const LIVE: [u8; 2] = [S::STATUS_RUNNING, S::STATUS_ENDED];

/// The transit record SettleTransit reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transit {
    pub state: u8,
    pub unit: u8,
    pub faction: u8,
    pub host_id: u64,
    pub depart_bell: u32,
    pub arrive_bell: u32,
    pub dep_mass: u32,
    pub troops_after: u32,
    pub stamina_after: u16,
    pub ready_bell_off: u16,
    pub seal_root: [u8; 32],
    pub tip: u64,
    pub flags: u8,
}

impl Transit {
    /// Transit record `i` of a Holding's data.
    pub fn read(hd: &[u8], i: usize) -> R<Transit> {
        let o = H::transit(i);
        let r = Ro(hd);
        Ok(Transit {
            state: r.u8(o + T::STATE)?,
            unit: r.u8(o + T::UNIT)?,
            faction: r.u8(o + T::FACTION)?,
            host_id: r.u64(o + T::HOST_ID)?,
            depart_bell: r.u32(o + T::DEPART_BELL)?,
            arrive_bell: r.u32(o + T::ARRIVE_BELL)?,
            dep_mass: r.u32(o + T::DEP_MASS)?,
            troops_after: r.u32(o + T::TROOPS_AFTER)?,
            stamina_after: r.u16(o + T::STAMINA_AFTER)?,
            ready_bell_off: r.u16(o + T::READY_BELL_OFF)?,
            seal_root: r.arr(o + T::SEAL_ROOT)?,
            tip: r.u64(o + T::TIP)?,
            flags: r.u8(o + T::FLAGS)?,
        })
    }
}

/// One record of a resolved final set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Arrival {
    pub host_id: u64,
    pub citizen_tag: u64,
    pub dep_mass: u32,
    pub stamina: u16,
    pub present: bool,
    pub fate: u8,
    pub troops_after: u32,
}

/// `arrivals[k]` of a ClashInputs' data.
pub fn arrival_at(cd: &[u8], k: usize) -> R<Arrival> {
    let o = CI::arrival(k);
    let r = Ro(cd);
    Ok(Arrival {
        host_id: r.u64(o + AR::HOST_ID)?,
        citizen_tag: r.u64(o + AR::CITIZEN_TAG)?,
        dep_mass: r.u32(o + AR::DEP_MASS)?,
        stamina: r.u16(o + AR::STAMINA)?,
        present: r.u8(o + AR::PRESENT)? != 0,
        fate: r.u8(o + AR::FATE)?,
        troops_after: r.u32(o + AR::TROOPS_AFTER)?,
    })
}

/// How a transit ends (§5.11 D5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Codes 1, 2, 4, 5: the host is destroyed; `fate` is its final-set
    /// fate when it was in one (a Stays/Withdrew host is forfeited).
    BadSeal { code: u8, fate: Option<u8> },
    /// In the resolved final set with this fate (1–5).
    Fate(u8),
    /// Not in the final set and outranked by it: bounce, no loss.
    BounceUnranked,
    /// Not in the final set but it would have been admitted (not revealed),
    /// or no resolved inputs: routed. `resolved` says whether a resolver
    /// earns the march fee.
    Routed { resolved: bool },
}

impl Outcome {
    /// The `TRANSIT_SETTLED.outcome` code.
    pub const fn code(self) -> u8 {
        match self {
            Outcome::BadSeal { .. } => TO::BAD_SEAL,
            Outcome::Fate(f) => f,
            Outcome::BounceUnranked => TO::BOUNCED_UNRANKED,
            Outcome::Routed { .. } => TO::ROUTED,
        }
    }
}

/// D5 for a valid seal, order-free: the host's final-set fate if it is in
/// `set` (the faction's four records, `(position, record)`); else, against
/// the present records, `admit_arrival` refusing it (the set is full of
/// higher-ranked arrivals, or its citizen has a higher-ranked one) is a
/// bounce by rank, and anything it would have been admitted to is a rout
/// (it was not revealed). `None` when no resolved final set exists.
pub fn valid_outcome(
    dest: ProvinceCoord,
    arrive: u32,
    me: SlotEntry,
    set: Option<&[Arrival; 4]>,
) -> Outcome {
    let Some(set) = set else {
        return Outcome::Routed { resolved: false };
    };
    if let Some(a) = set.iter().find(|a| a.present && a.host_id == me.host_id) {
        return Outcome::Fate(a.fate);
    }
    let mut slots = [None; 4];
    for (s, a) in slots.iter_mut().zip(set.iter()) {
        if a.present {
            *s = Some(SlotEntry {
                host_id: a.host_id,
                citizen: a.citizen_tag,
                troops: a.dep_mass,
            });
        }
    }
    match admit_arrival(dest, arrive, &slots, me) {
        SlotDecision::Refuse(_) => Outcome::BounceUnranked,
        SlotDecision::Fill { .. } | SlotDecision::Displace { .. } => {
            Outcome::Routed { resolved: true }
        }
    }
}

/// The values a returning host comes home with, or `None` (nothing comes
/// back). `rec` is its final-set record (fates 3–4).
pub fn return_values(o: Outcome, t: &Transit, rec: Option<&Arrival>) -> Option<(u32, u16, bool)> {
    match o {
        Outcome::Fate(AR::FATE_BOUNCED) | Outcome::Fate(AR::FATE_RETREATED) => {
            rec.map(|r| (r.troops_after, r.stamina, false))
        }
        Outcome::BounceUnranked => Some((t.troops_after, t.stamina_after, false)),
        Outcome::Routed { .. } => Some((rout_survivors(t.troops_after), 0, true)),
        _ => None,
    }
    .filter(|(troops, _, _)| *troops >= DESTROYED_BELOW)
}

/// The slot kept for a claim (I-52): its evidence is claim-eligible against
/// THE anchor's slot, unclaimed, and the claim grace is still running.
pub fn claim_eligible(
    slot: &[u8],
    anchor_slot: Option<u64>,
    lateness: u8,
    params: &DefenceParams,
    now: i64,
    close: i64,
) -> R<bool> {
    let Some(anchor_slot) = anchor_slot else {
        return Ok(false);
    };
    let r = Ro(slot);
    if r.u8(AS::CLAIMED)? != 0 {
        return Ok(false);
    }
    let grace_end = close
        .checked_add(CLAIM_GRACE_BELLS as i64 * BELL_SECS)
        .ok_or(OVERFLOW)?;
    if now >= grace_end {
        return Ok(false);
    }
    if r.u64(AS::EV_SLOT)?.saturating_sub(anchor_slot) < lateness as u64 {
        return Ok(false);
    }
    let ev = AS::evidence(slot).ok_or(BAD_ACCOUNT)?;
    Ok(super::defence::refund(&ev, params) > 0)
}

/// The Season values SettleTransit reads.
struct SeasonVals {
    clock: SeasonClock,
    march_fee: u64,
    seal_bond: u64,
    lateness: u8,
    defence: DefenceParams,
}

fn season_vals(season: &AccountInfo) -> R<SeasonVals> {
    let d = season.try_borrow_data()?;
    let r = Ro(&d);
    let tip_min = fees::min_tip_lamports(
        r.u32(S::MIN_REVEAL_PRIORITY_MILLI)?,
        r.u32(S::REVEAL_CU_LIMIT)?,
        r.u32(S::REVEAL_LOADED_LIMIT)?,
    );
    Ok(SeasonVals {
        clock: SeasonClock::read(&d)?,
        march_fee: r.u64(S::MARCH_FEE)?,
        seal_bond: r.u64(S::SEAL_BOND)?,
        lateness: r.u8(S::LATENESS_SLOTS)?,
        defence: DefenceParams {
            defence_cap_milli: r.u32(S::DEFENCE_CAP_MILLI)?,
            tip_min,
        },
    })
}

/// A present Province at its canonical address: `(P, Q, resolved_next)`.
fn province_at(
    program: &Pubkey,
    ctx: &AddrCtx,
    season_id: u64,
    ai: &AccountInfo,
) -> R<(i16, i16, u32)> {
    prologue::present(ai, program, AccountKind::Province, season_id)?;
    let (pp, pq, rn) = {
        let d = ai.try_borrow_data()?;
        let r = Ro(&d);
        (r.i16(P::P)?, r.i16(P::Q)?, r.u32(P::RESOLVED_NEXT)?)
    };
    expect_key(ai, &ctx.province(pp as i32, pq as i32))?;
    Ok((pp, pq, rn))
}

/// Pays `amount` of the Holding's lamports to `to` (`pay_or_divert`, sink
/// `pool_owed`); a recipient owned by this program is never credited
/// directly (module note). Returns the lamports added to `pool_owed`.
fn pay<'a>(
    program: &Pubkey,
    holding: &AccountInfo<'a>,
    to: &AccountInfo<'a>,
    amount: u64,
    reason: u8,
    bell: u32,
) -> R<u64> {
    if amount == 0 {
        return Ok(0);
    }
    if to.owner == program {
        return owe(holding, to.key.as_ref(), amount, reason, bell);
    }
    match init::pay_or_divert(
        program,
        holding,
        to,
        amount,
        &Sink::PoolOwed(holding),
        reason,
        bell,
    )? {
        Paid::Recipient => Ok(0),
        Paid::Diverted => Ok(amount),
    }
}

/// Keeps `amount` in the Holding as `pool_owed`; `DIVERT` names the
/// intended recipient when there was one.
fn owe(holding: &AccountInfo, recipient: &[u8], amount: u64, reason: u8, bell: u32) -> R<u64> {
    if amount == 0 {
        return Ok(0);
    }
    {
        let mut d = holding.try_borrow_mut_data()?;
        Rw(&mut d).add_u64(H::POOL_OWED, amount)?;
    }
    if recipient != holding.key.as_ref() {
        let payload = Buf::<9>::new().u64(amount).u8(reason);
        events::emit(Kind::DIVERT, bell, recipient, payload.get()?, &mut [])?;
    }
    Ok(amount)
}

/// The first 8 bytes of an address (`TRANSIT_SETTLED.*_to`).
fn to8(k: &[u8]) -> [u8; 8] {
    let mut o = [0u8; 8];
    o.copy_from_slice(&k[..8]);
    o
}

/// Adds a returning host to the home Province, or its troops to the
/// Holding's reserve (module note). `true` when the Province was written
/// (a muster-pending entry), `false` for the reserve.
#[allow(clippy::too_many_arguments)]
fn return_home(
    home: &AccountInfo,
    holding: &AccountInfo,
    t: &Transit,
    troops: u32,
    stamina: u16,
    stamina_bell: u32,
    tile: u8,
    now_bell: u32,
) -> R<bool> {
    let unit = unit_from_u8(t.unit).ok_or(BAD_ACCOUNT)?;
    let doctrine = of_faction(t.faction).ok_or(BAD_ACCOUNT)?;
    let dealt = super::holding::bps16(doctrine.dealt_bps(Posture::Stance(Stance::Hold), false))?;
    let from = now_bell.checked_add(1).ok_or(OVERFLOW)?;
    let ready = t
        .depart_bell
        .checked_add(t.ready_bell_off as u32)
        .ok_or(OVERFLOW)?
        .max(from);
    let slot = {
        let pd = home.try_borrow_data()?;
        let r = Ro(&pd);
        let (mut total, mut own, mut free) = (0usize, 0usize, None);
        for i in 0..P::ENTRIES_N {
            let o = P::entry(i);
            match r.u8(o + E::STATE)? {
                E::STATE_FREE => {
                    if free.is_none() {
                        free = Some(i);
                    }
                }
                E::STATE_ROSTER | E::STATE_MUSTER_PENDING => {
                    total += 1;
                    if r.u8(o + E::FACTION)? == t.faction {
                        own += 1;
                    }
                }
                _ => {}
            }
        }
        if total >= P::ROSTER_CAP || own >= P::FACTION_CAP {
            None
        } else {
            free
        }
    };
    match slot {
        Some(i) => {
            let host = Host {
                id: t.host_id,
                owner: frontier_abi::addr::holding_key_of_host(t.host_id),
                faction: t.faction,
                unit,
                troops,
                stamina: Stamina {
                    value: stamina,
                    bell: stamina_bell,
                },
                ready_bell: ready,
                pending: None,
            };
            let e = Entry::from_host(&host, tile, E::STATE_MUSTER_PENDING, dealt, from);
            let mut pd = home.try_borrow_mut_data()?;
            write_entry(&mut pd, i, &e).map_err(|_| BAD_ACCOUNT)?;
            let mut w = Rw(&mut pd);
            let n = w.u8(P::N_ENTRIES)?.saturating_add(1);
            w.set_u8(P::N_ENTRIES, n)?;
            let ep = w.u32(P::ROSTER_EPOCH)?.wrapping_add(1);
            w.set_u32(P::ROSTER_EPOCH, ep)?;
            Ok(true)
        }
        None => {
            let whole = troops / permutation_rules::fixed::MILLI as u32;
            let mut hd = holding.try_borrow_mut_data()?;
            let mut w = Rw(&mut hd);
            let at = H::reserve(t.unit as usize);
            let v = w.u32(at)?.checked_add(whole).ok_or(OVERFLOW)?;
            w.set_u32(at, v)?;
            Ok(false)
        }
    }
}

/// A bad-seal host still among the destination's residents (state 1) gets
/// the pending op `Forfeit` issued at `now_bell` (I-44; the resolve or skip
/// of that bell frees it, its troops lost). Returns whether the Province
/// was written.
fn forfeit(dest: &AccountInfo, host_id: u64, now_bell: u32) -> R<bool> {
    let mut pd = dest.try_borrow_mut_data()?;
    let Some(i) = find_entry(&pd, host_id) else {
        return Ok(false);
    };
    let mut e = read_entry(&pd, i).map_err(|_| BAD_ACCOUNT)?;
    if e.state != E::STATE_ROSTER || e.busy() {
        // Already pending a Forfeit (a stranded disband) or not a resident:
        // nothing to add.
        return Ok(false);
    }
    e.op = EntryOp::Forfeit;
    e.pend_bell = now_bell;
    write_entry(&mut pd, i, &e).map_err(|_| BAD_ACCOUNT)?;
    let mut w = Rw(&mut pd);
    let ep = w.u32(P::ROSTER_EPOCH)?.wrapping_add(1);
    w.set_u32(P::ROSTER_EPOCH, ep)?;
    Ok(true)
}

/// 0x54 SettleTransit (module doc).
pub fn settle_transit(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::SettleTransit, a, None)?;
    let x = aix::SettleTransit::decode(d)?;
    let now = prologue::now()?;
    let [payer, season_ai, holding, dest, inputs, slot, home, anchor, slot_ben, resolver, rent_payer, settle_ben, _system] =
        a
    else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let hdr = prologue::season(season_ai, p, Some(&RULESET_HASH), &LIVE, now.ts)?;
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    let now_bell = hdr.bell(now.ts).ok_or(FrontierError::WrongStatus)?;
    let sv = season_vals(season_ai)?;

    // 1. The Holding and its transit record.
    prologue::present(holding, p, AccountKind::Holding, hdr.id)?;
    if x.transit_slot as usize >= H::TRANSIT_N {
        return Err(FrontierError::BadData.into());
    }
    let (hp, hq, site, gen, tile, owner_citizen, holding_rent_payer, t) = {
        let hd = holding.try_borrow_data()?;
        let r = Ro(&hd);
        (
            r.i16(H::P)?,
            r.i16(H::Q)?,
            r.u8(H::SITE)?,
            r.u8(H::GEN)?,
            r.u8(H::TILE)?,
            r.arr::<32>(H::OWNER_CITIZEN)?,
            r.arr::<32>(H::RENT_PAYER)?,
            Transit::read(&hd, x.transit_slot as usize)?,
        )
    };
    expect_key(holding, &ctx.holding(hp as i32, hq as i32, site))?;
    match t.state {
        T::STATE_SETTLED | T::STATE_DESTROYED_AT_ORIGIN => {}
        T::STATE_DEPARTED => return Err(FrontierError::DepartureUnsettled.into()),
        _ => return Err(FrontierError::TransitState.into()),
    }
    let parts = split_host_id(t.host_id).ok_or(BAD_ACCOUNT)?;
    if parts.province.p != hp as i32
        || parts.province.q != hq as i32
        || parts.site != site
        || parts.gen != gen
    {
        return Err(BAD_ACCOUNT);
    }
    let arrive = t.arrive_bell;

    // 2. Timing: THE anchor (or its archive entry) of the destination's
    // region, and the destination resolved past the arrival.
    let (dp, dq, dest_rn) = province_at(p, &ctx, hdr.id, dest)?;
    let dest_c = ProvinceCoord::new(dp as i32, dq as i32);
    let region = region_of(dest_c);
    let src: AnchorSource =
        the_anchor_or_archive(p, &ctx, hdr.id, anchor, arrive, region, &sv.clock)?;
    let close = sv.clock.reveal_close(arrive, src.a);
    let settle_at = close.checked_add(SETTLE_AFTER_CLOSE_SECS).ok_or(OVERFLOW)?;
    if now.ts < settle_at || dest_rn <= arrive {
        return Err(FrontierError::TooEarly.into());
    }

    // 3. The logged pair is the committed one.
    if seal::seal_root_of(&x.commit, &x.seal) != t.seal_root {
        return Err(FrontierError::CommitMismatch.into());
    }

    // 4. The proof (I-44).
    let s_aff = sys::bls_g1_decompress(&src.sig48)
        .map_err(|_| crypto(crate::error::crypto_sub::BAD_POINT))?;
    let (code, plain) = seal::judge(&s_aff, &x.seal, &x.commit, t.host_id, arrive);
    if let Some(pl) = plain {
        if (pl.dest_p, pl.dest_q) != (dp, dq) {
            return Err(FrontierError::BadAddress.into());
        }
    }

    // 5. The beneficiary, the final set and the slot.
    expect_key(settle_ben, &x.beneficiary)?;
    expect_key(rent_payer, &holding_rent_payer)?;
    expect_key(inputs, &ctx.clash_inputs(dp as i32, dq as i32, arrive))?;
    let (set, resolver_key) = if prologue::presence(inputs, p, AccountKind::ClashInputs, hdr.id)? {
        let cd = inputs.try_borrow_data()?;
        let r = Ro(&cd);
        if r.i16(CI::P)? != dp || r.i16(CI::Q)? != dq || r.u32(CI::BELL)? != arrive {
            return Err(BAD_ACCOUNT);
        }
        if r.u8(CI::FLAGS)? & CI::FLAG_RESOLVED != 0 {
            let base = CI::position(t.faction, 0);
            let mut set = [arrival_at(&cd, base)?; 4];
            for (i, s) in set.iter_mut().enumerate().skip(1) {
                *s = arrival_at(&cd, base + i)?;
            }
            (Some(set), r.arr::<32>(CI::RESOLVER)?)
        } else {
            (None, [0; 32])
        }
    } else {
        (None, [0; 32])
    };
    let position = set
        .as_ref()
        .and_then(|s| s.iter().position(|a| a.present && a.host_id == t.host_id));
    let slot_i = position.unwrap_or(0) as u8;
    expect_key(
        slot,
        &ctx.arrival_slot(dp as i32, dq as i32, arrive, t.faction, slot_i),
    )?;
    let own_slot = if prologue::presence(slot, p, AccountKind::ArrivalSlot, hdr.id)? {
        let sd = slot.try_borrow_data()?;
        let r = Ro(&sd);
        if r.i16(AS::P)? != dp
            || r.i16(AS::Q)? != dq
            || r.u32(AS::BELL)? != arrive
            || r.u8(AS::FACTION)? != t.faction
            || r.u8(AS::I)? != slot_i
        {
            return Err(BAD_ACCOUNT);
        }
        if r.u64(AS::HOST_ID)? == t.host_id {
            Some((r.arr::<32>(AS::BENEFICIARY)?, r.arr::<32>(AS::RENT_TO)?))
        } else {
            None
        }
    } else {
        None
    };

    // The outcome.
    let rec = position.and_then(|k| set.as_ref().map(|s| s[k]));
    let outcome = if code != seal_code::VALID {
        Outcome::BadSeal {
            code,
            fate: rec.map(|r| r.fate),
        }
    } else {
        let me = SlotEntry {
            host_id: t.host_id,
            citizen: u64::from_le_bytes(to8(&owner_citizen)),
            troops: t.dep_mass,
        };
        valid_outcome(dest_c, arrive, me, set.as_ref())
    };
    if let Outcome::Fate(f) = outcome {
        if !(AR::FATE_STAYS..=AR::FATE_DESTROYED).contains(&f) {
            return Err(BAD_ACCOUNT);
        }
    }

    // Payments (out of the Holding's escrow).
    let fee = if t.flags & T::FLAG_FEE_ESCROWED != 0 {
        sv.march_fee
    } else {
        0
    };
    let bond = if t.flags & T::FLAG_BOND_ESCROWED != 0 {
        sv.seal_bond
    } else {
        0
    };
    let total = t
        .tip
        .checked_add(fee)
        .and_then(|v| v.checked_add(bond))
        .ok_or(OVERFLOW)?;
    {
        let mut hd = holding.try_borrow_mut_data()?;
        let mut w = Rw(&mut hd);
        let esc = w.u64(H::ESCROW)?.checked_sub(total).ok_or(BAD_ACCOUNT)?;
        w.set_u64(H::ESCROW, esc)?;
    }
    let hkey = key(holding);
    let mut pool = 0u64;
    let mut log = [0u64; 4]; // tip, fee, bond, reward
    let mut to = [[0u8; 8]; 4];
    let resolved = matches!(
        outcome,
        Outcome::Fate(_) | Outcome::BounceUnranked | Outcome::Routed { resolved: true }
    );
    let check_resolver = || expect_key(resolver, &resolver_key);
    match outcome {
        Outcome::BadSeal { .. } => {
            to[3] = to8(settle_ben.key.as_ref());
            log[3] = total;
            pool += pay(
                p,
                holding,
                settle_ben,
                total,
                divert_reason::REWARD,
                now_bell,
            )?;
        }
        _ => {
            // tip
            log[0] = t.tip;
            match outcome {
                Outcome::Fate(_) => match own_slot {
                    Some((ben, _)) => {
                        expect_key(slot_ben, &ben)?;
                        to[0] = to8(&ben);
                        pool += pay(p, holding, slot_ben, t.tip, divert_reason::TIP, now_bell)?;
                    }
                    None => {
                        to[0] = to8(&hkey);
                        pool += owe(holding, &hkey, t.tip, divert_reason::TIP, now_bell)?;
                    }
                },
                Outcome::BounceUnranked => {
                    to[0] = to8(&holding_rent_payer);
                    pool += pay(p, holding, rent_payer, t.tip, divert_reason::TIP, now_bell)?;
                }
                _ => {
                    to[0] = to8(&hkey);
                    pool += owe(holding, &hkey, t.tip, divert_reason::TIP, now_bell)?;
                }
            }
            // march fee
            log[1] = fee;
            if resolved {
                check_resolver()?;
                to[1] = to8(&resolver_key);
                pool += pay(
                    p,
                    holding,
                    resolver,
                    fee,
                    divert_reason::MARCH_FEE,
                    now_bell,
                )?;
            } else {
                to[1] = to8(&hkey);
                pool += owe(holding, &hkey, fee, divert_reason::MARCH_FEE, now_bell)?;
            }
            // bond
            log[2] = bond;
            to[2] = to8(&holding_rent_payer);
            pool += pay(p, holding, rent_payer, bond, divert_reason::BOND, now_bell)?;
        }
    }

    // The host.
    let mut dest_written = false;
    let mut home_written = false;
    let mut troops_logged = 0u32;
    let home_key = ctx.province(hp as i32, hq as i32);
    expect_key(home, &home_key)?;
    match outcome {
        Outcome::BadSeal { fate, .. } => {
            if matches!(fate, Some(AR::FATE_STAYS) | Some(AR::FATE_WITHDREW)) {
                dest_written = forfeit(dest, t.host_id, now_bell)?;
            }
        }
        Outcome::Fate(AR::FATE_STAYS) | Outcome::Fate(AR::FATE_WITHDREW) => {
            troops_logged = rec.map_or(0, |r| r.troops_after);
        }
        _ => {
            if let Some((troops, stamina, routed)) = return_values(outcome, &t, rec.as_ref()) {
                let stamina_bell = if routed {
                    now_bell
                } else if rec.is_some() {
                    arrive
                } else {
                    effective_bell(t.depart_bell)
                };
                troops_logged = troops;
                prologue::present(home, p, AccountKind::Province, hdr.id)?;
                home_written = return_home(
                    home,
                    holding,
                    &t,
                    troops,
                    stamina,
                    stamina_bell,
                    tile,
                    now_bell,
                )?;
            }
        }
    }
    let same_province = dest.key == home.key;

    // The final set's settled bit.
    let inputs_written = match position {
        Some(k) => {
            let mut cd = inputs.try_borrow_mut_data()?;
            let mut w = Rw(&mut cd);
            let bit = 1u32 << CI::position(t.faction, k as u8);
            let m = w.u32(CI::SETTLED_MASK)? | bit;
            w.set_u32(CI::SETTLED_MASK, m)?;
            true
        }
        None => false,
    };

    // The slot: kept for a claim, closed to a listed rent_to, or flagged.
    let mut slot_kept = false;
    if let Some((_, rent_to)) = own_slot {
        let keep = {
            let sd = slot.try_borrow_data()?;
            claim_eligible(
                &sd,
                src.anchor_slot,
                sv.lateness,
                &sv.defence,
                now.ts,
                close,
            )?
        };
        let recipient = if keep {
            None
        } else {
            [payer, slot_ben, resolver, rent_payer, settle_ben]
                .into_iter()
                .find(|ai| ai.key.as_ref() == rent_to)
        };
        match recipient {
            Some(to_ai) => {
                let lamports = slot.lamports();
                let raw = slot_raw(dp, dq, arrive, t.faction, slot_i);
                super::beacon::emit_close_short(
                    AccountKind::ArrivalSlot,
                    &raw,
                    &rent_to,
                    lamports,
                    now_bell,
                )?;
                init::close_to(p, slot, to_ai, &Sink::Never, now_bell)?;
            }
            None => {
                let mut sd = slot.try_borrow_mut_data()?;
                let mut w = Rw(&mut sd);
                let f = w.u8(AS::FLAGS)? | AS::FLAG_SETTLED;
                w.set_u8(AS::FLAGS, f)?;
                slot_kept = true;
            }
        }
    }

    // The transit record is free again.
    {
        let mut hd = holding.try_borrow_mut_data()?;
        let o = H::transit(x.transit_slot as usize);
        let rec = hd.get_mut(o..o + T::SIZE).ok_or(BAD_ACCOUNT)?;
        rec.fill(0);
    }

    // TRANSIT_SETTLED.
    let payload = Buf::<79>::new()
        .u8(outcome.code())
        .u8(code)
        .u32(troops_logged)
        .bytes(&to[0])
        .u64(log[0])
        .bytes(&to[1])
        .u64(log[1])
        .bytes(&to[2])
        .u64(log[2])
        .bytes(&to[3])
        .u64(log[3])
        .u64(pool)
        .u8(slot_kept as u8);
    let mut hd = holding.try_borrow_mut_data()?;
    let mut dd = if dest_written {
        Some(dest.try_borrow_mut_data()?)
    } else {
        None
    };
    let mut md = if home_written && !(same_province && dest_written) {
        Some(home.try_borrow_mut_data()?)
    } else {
        None
    };
    let mut cd = if inputs_written {
        Some(inputs.try_borrow_mut_data()?)
    } else {
        None
    };
    let mut list: alloc::vec::Vec<Chained> = alloc::vec::Vec::with_capacity(4);
    list.push(Chained {
        entity: EntityKind::Holding,
        data: &mut hd,
    });
    if let Some(d) = dd.as_mut() {
        list.push(Chained {
            entity: EntityKind::Province,
            data: d,
        });
    }
    if let Some(d) = md.as_mut() {
        list.push(Chained {
            entity: EntityKind::Province,
            data: d,
        });
    }
    if let Some(d) = cd.as_mut() {
        list.push(Chained {
            entity: EntityKind::ClashInputs,
            data: d,
        });
    }
    events::emit(
        Kind::TRANSIT_SETTLED,
        now_bell,
        &t.host_id.to_le_bytes(),
        payload.get()?,
        &mut list,
    )
}

/// Raw key of an ArrivalSlot (§4.1).
fn slot_raw(p: i16, q: i16, bell: u32, faction: u8, i: u8) -> [u8; 14] {
    let mut r = [0u8; 14];
    r[..4].copy_from_slice(&(p as i32).to_le_bytes());
    r[4..8].copy_from_slice(&(q as i32).to_le_bytes());
    r[8..12].copy_from_slice(&bell.to_le_bytes());
    r[12] = faction;
    r[13] = i;
    r
}

/// 0x55 SweepPoolOwed: `[any s] [season] [holding w] [dpool w]`, class N.
/// Moves `Holding.pool_owed` lamports to the DefencePool (counted in its
/// `diverted_total`) and zeroes it; `AlreadyDone` when nothing is owed.
/// Log `POOL_SWEEP` (Holding).
pub fn sweep_pool_owed(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::SweepPoolOwed, a, None)?;
    aix::SweepPoolOwed::decode(d)?;
    let now = prologue::now()?;
    let [_any, season_ai, holding, dpool] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let hdr = prologue::season(season_ai, p, Some(&RULESET_HASH), &LIVE, now.ts)?;
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    prologue::present(holding, p, AccountKind::Holding, hdr.id)?;
    let (hp, hq, site, owed) = {
        let hd = holding.try_borrow_data()?;
        let r = Ro(&hd);
        (
            r.i16(H::P)?,
            r.i16(H::Q)?,
            r.u8(H::SITE)?,
            r.u64(H::POOL_OWED)?,
        )
    };
    expect_key(holding, &ctx.holding(hp as i32, hq as i32, site))?;
    expect_key(dpool, &ctx.defence_pool())?;
    prologue::present(dpool, p, AccountKind::DefencePool, hdr.id)?;
    if owed == 0 {
        return Err(FrontierError::AlreadyDone.into());
    }
    init::move_lamports(holding, dpool, owed)?;
    {
        let mut pd = dpool.try_borrow_mut_data()?;
        Rw(&mut pd).add_u64(crate::layout::defence_pool::DIVERTED_TOTAL, owed)?;
    }
    let bell = hdr.bell(now.ts).unwrap_or(frontier_abi::log::NO_BELL);
    let mut hd = holding.try_borrow_mut_data()?;
    Rw(&mut hd).set_u64(H::POOL_OWED, 0)?;
    let k = super::holding::pqs_key(hp, hq, site)?;
    events::emit(
        Kind::POOL_SWEEP,
        bell,
        &k,
        &owed.to_le_bytes(),
        &mut [Chained {
            entity: EntityKind::Holding,
            data: &mut hd,
        }],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arr(host: u64, tag: u64, mass: u32, fate: u8) -> Arrival {
        Arrival {
            host_id: host,
            citizen_tag: tag,
            dep_mass: mass,
            stamina: 50,
            present: true,
            fate,
            troops_after: mass,
        }
    }

    const NONE: Arrival = Arrival {
        host_id: 0,
        citizen_tag: 0,
        dep_mass: 0,
        stamina: 0,
        present: false,
        fate: 0,
        troops_after: 0,
    };

    #[test]
    fn d5_outcomes_are_rank_based() {
        let p = ProvinceCoord::new(3, -1);
        let me = |mass| SlotEntry {
            host_id: 99,
            citizen: 7,
            troops: mass,
        };
        // No final set: routed, no resolver.
        assert_eq!(
            valid_outcome(p, 10, me(500_000), None),
            Outcome::Routed { resolved: false }
        );
        // In the set: its fate.
        let set = [arr(99, 7, 500_000, 3), NONE, NONE, NONE];
        assert_eq!(
            valid_outcome(p, 10, me(500_000), Some(&set)),
            Outcome::Fate(3)
        );
        // A full set of larger arrivals: bounce by rank.
        let big = [
            arr(1, 1, 900_000, 1),
            arr(2, 2, 900_000, 1),
            arr(3, 3, 900_000, 1),
            arr(4, 4, 900_000, 1),
        ];
        assert_eq!(
            valid_outcome(p, 10, me(500_000), Some(&big)),
            Outcome::BounceUnranked
        );
        // A full set with a smaller arrival: it would have displaced it, so
        // it was not revealed: routed.
        let mut small = big;
        small[2] = arr(3, 3, 100_000, 1);
        assert_eq!(
            valid_outcome(p, 10, me(500_000), Some(&small)),
            Outcome::Routed { resolved: true }
        );
        // Room left: routed.
        let room = [arr(1, 1, 900_000, 1), NONE, NONE, NONE];
        assert_eq!(
            valid_outcome(p, 10, me(100_000), Some(&room)),
            Outcome::Routed { resolved: true }
        );
        // Its citizen holds a larger arrival: bounce.
        let mine = [arr(5, 7, 900_000, 1), NONE, NONE, NONE];
        assert_eq!(
            valid_outcome(p, 10, me(500_000), Some(&mine)),
            Outcome::BounceUnranked
        );
    }

    #[test]
    fn returns_follow_the_outcome() {
        let t = Transit {
            state: 2,
            unit: 0,
            faction: 1,
            host_id: 5,
            depart_bell: 10,
            arrive_bell: 14,
            dep_mass: 800_000,
            troops_after: 700_000,
            stamina_after: 40,
            ready_bell_off: 1,
            seal_root: [0; 32],
            tip: 1,
            flags: 3,
        };
        let r = arr(5, 7, 800_000, AR::FATE_RETREATED);
        assert_eq!(
            return_values(Outcome::Fate(AR::FATE_RETREATED), &t, Some(&r)),
            Some((800_000, 50, false))
        );
        assert_eq!(
            return_values(Outcome::BounceUnranked, &t, None),
            Some((700_000, 40, false))
        );
        assert_eq!(
            return_values(Outcome::Routed { resolved: true }, &t, None),
            Some((350_000, 0, true))
        );
        assert_eq!(
            return_values(Outcome::Fate(AR::FATE_STAYS), &t, Some(&r)),
            None
        );
        assert_eq!(
            return_values(
                Outcome::BadSeal {
                    code: 1,
                    fate: None
                },
                &t,
                None
            ),
            None
        );
        let mut tiny = t;
        tiny.troops_after = 900;
        assert_eq!(
            return_values(Outcome::Routed { resolved: false }, &tiny, None),
            None,
            "450 milli-troops is below DESTROYED_BELOW"
        );
    }

    #[test]
    fn outcome_codes_are_the_logs() {
        assert_eq!(Outcome::Fate(1).code(), TO::STAYS);
        assert_eq!(Outcome::BounceUnranked.code(), 6);
        assert_eq!(Outcome::Routed { resolved: true }.code(), 7);
        assert_eq!(
            Outcome::BadSeal {
                code: 2,
                fate: None
            }
            .code(),
            8
        );
    }

    #[test]
    fn claim_eligibility() {
        let params = DefenceParams {
            defence_cap_milli: 2_000,
            tip_min: 14_441,
        };
        let mut s = alloc::vec![0u8; AS::SIZE];
        let set = |s: &mut [u8], price: u64, ev_slot: u64| {
            s[AS::EV_PRICE..AS::EV_PRICE + 8].copy_from_slice(&price.to_le_bytes());
            s[AS::EV_LIMIT..AS::EV_LIMIT + 4].copy_from_slice(&26_000u32.to_le_bytes());
            s[AS::EV_LOADED..AS::EV_LOADED + 4].copy_from_slice(&(1u32 << 20).to_le_bytes());
            s[AS::EV_SLOT..AS::EV_SLOT + 8].copy_from_slice(&ev_slot.to_le_bytes());
        };
        // Late and expensive: eligible inside the grace.
        set(&mut s, 2_000_000, 110);
        assert!(claim_eligible(&s, Some(100), 4, &params, 1_000, 900).unwrap());
        // After the grace.
        assert!(!claim_eligible(&s, Some(100), 4, &params, 900 + 3_600, 900).unwrap());
        // Archived (no anchor slot).
        assert!(!claim_eligible(&s, None, 4, &params, 1_000, 900).unwrap());
        // On time.
        set(&mut s, 2_000_000, 102);
        assert!(!claim_eligible(&s, Some(100), 4, &params, 1_000, 900).unwrap());
        // Cheap (at the tip level): no refund.
        set(&mut s, 0, 110);
        assert!(!claim_eligible(&s, Some(100), 4, &params, 1_000, 900).unwrap());
        // Claimed.
        set(&mut s, 2_000_000, 110);
        s[AS::CLAIMED] = 1;
        assert!(!claim_eligible(&s, Some(100), 4, &params, 1_000, 900).unwrap());
    }
}
