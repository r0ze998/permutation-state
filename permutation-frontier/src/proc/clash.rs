//! Clashes (M1 contract §5.11): GatherClash (0x60), ResolveFromInputs
//! (0x61), ResolveClash (0x62, feature `oracle`, tests only), SkipQuiet
//! (0x63), CloseClashInputs (0x64), CloseArrivalDay (0x65),
//! CloseArrivalSlot (0x66), and the **return settle** of hosts that leave
//! a province (Dissolve's `Leave`, a bounced resident), contract v1.5 §21.
//! Implemented by W4-A.
//!
//! [`model`] is the clash of one province-bell as a pure function of
//! account bytes (the `ClashInput` builder, the write-back, the settle of
//! pending changes, the camp's daily check): since integ-W5 (W4-A D8) it
//! lives in `frontier_abi::clash_model`, so the program, the herald, the
//! verifier, itest and the WASM client run one copy (W3-D D1).
//!
//! ## Pinned here (recorded in `W4-A-NOTES.md`; v1.6 amendment requests)
//!
//! - **Residents** are the entries in state 1 with `from_bell ≤ b` at
//!   `Host::values_at(b)`, at Hold, `dealt_bps` as stored. **Garrisons**
//!   are the site mirrors in state 1 (holding) among `site_count`, at
//!   `GarrisonState::at(b)`, walls when `walls_committed > 0` or a wall
//!   item with `delta > 0` is effective at or before b, id = the holding
//!   key (`host_id(P, Q, site, gen, 0)`), at Hold. **The camp** (present)
//!   is a NEUTRAL garrison with id `u64::MAX − gen`, troops `× 1,000`, no
//!   walls, at Hold, when fewer than 12 garrisons stand (12 holdings and a
//!   camp: the camp sits the bell out; `MAX_GARRISONS` = 12). **Arrivals**
//!   are the records with `present = 1`.
//! - **The camp's daily check** (I-56) runs at the first resolve or skip of
//!   a day (`day(b) ≥ next_check_day`): `camp::place(camp_seed, P, terrain,
//!   day(b), has_holding, false)` with `camp_seed = sha256("PSF-CAMP-v1" ‖
//!   province[TERRAIN..=SITE_COUNT])` (the ring seed is in neither
//!   instruction's accounts; this block is fixed at OpenProvince and derived
//!   from the ring seed, so it is as public as the ring seed); a spawn
//!   replaces a present camp (the simulator's respawn), `gen += 1`, a
//!   `CAMP` record; `next_check_day = day(b) + 1` either way.
//! - **Write-back** of a resident: its post-clash troops, stamina and
//!   cooldown (`Host::apply_clash`) only when something changed (a quiet
//!   bell leaves the entry's lazy stamina clock untouched, so a skip and a
//!   resolve leave the same bytes, G11); `Withdrew` moves its tile;
//!   `Destroyed` frees the entry unless a departure (`Spend`) or a leave is
//!   pending (those keep their post-clash values for SettleDeparture and
//!   the return settle); `Bounced` / `Retreated` send it home: a pending
//!   `Leave` issued at b (unless it is departing or forfeit).
//!   **Arrivals** that stay or withdraw become roster entries (state 1,
//!   `from_bell = b + 1`, `ready_bell = b + 2` if engaged else `b + 1`,
//!   `dealt_bps` = the faction doctrine at Hold, not arriving). **Garrisons**
//!   take their post-clash troops; **the camp** is cleared (`state 0`) when
//!   its troops fall below one whole troop or hostile hosts hold its hex,
//!   and `WORKS_CAMP` goes to the arrivals of the holding factions that
//!   stay on the camp's tile: their positions are the `camp_mask` stored in
//!   ClashInputs' reserved word at offset 76 (layout request) and logged in
//!   the `CAMP` record of the clear (`troops = 0`).
//! - **Settle of bell b** (resolve and skip alike): musters with
//!   `from_bell ≤ b + 1` join the roster; `Spend` → state 3 (departed,
//!   post-clash values, SettleDeparture next); `Leave` → state 3 with op
//!   `Leave` kept until the return settle; `Forfeit` → freed (troops
//!   lost); splits and merges by the kernel; garrison changes of bells ≤ b
//!   (`GarrisonState::settle`). `roster_epoch` moves (and `n_entries` is
//!   recounted) only when an entry, a garrison or the camp changed, so a
//!   skip and a resolve of the same quiet bell leave the same Province.
//! - **The return settle** is SettleDeparture with `transit_slot = 0xFF`
//!   ([`RETURN_SLOT`], same accounts `[payer s] [season] [province w]
//!   [holding w]`): the state-3 `Leave` entries of that Holding in that
//!   Province are freed, at most [`RETURN_MAX`] per transaction (v1.7),
//!   and `reserve[unit] += troops / 1,000` (whole troops, rounded down)
//!   credited to the Holding when it is live with the host's
//!   generation (`DEPARTURE_SETTLED` with `destroyed = 2`), else the troops
//!   are lost (`STRANDED`). Nothing to return is `AlreadyDone`.
//! - **Gather records:** a slot whose Holding is absent, released or
//!   re-founded, or whose transit record is gone, gathers as not present;
//!   a transit still departed (state 1) is `DepartureUnsettled`; one
//!   destroyed at the origin (state 3) is recorded with `present = 0`,
//!   `fate = Destroyed`. The arrival's stamina is the transit's
//!   `stamina_after` refilled from `depart_bell + 1` to the arrival bell
//!   (the kernel's `Stamina::at`). A present slot listed without a Holding,
//!   or an absent one with a Holding, is `BadData` (G6: a gather that
//!   omits a present slot cannot complete).
//! - **Bells at or after `end_bell`** are never gathered, resolved or
//!   skipped (`WrongStatus`): no arrival can exist there.
//! - **SkipQuiet** re-tests quietness at its first bell and after every
//!   change of the roster, garrisons or camp (an in-transaction cache; the
//!   Province's `quiet_ok` byte is left 0: nothing records the epoch it
//!   would be valid for). The test is [`model::trivially_quiet`] (every
//!   occupied hex one faction's, within the caps: no engagement, bounce or
//!   withdrawal can happen) and, only at the transaction's first bell, the
//!   kernel's `is_quiet` (heap scoped: the bump allocator frees only its
//!   top block). A later bell that is not trivially quiet ends the
//!   transaction, which commits the bells before it ([`SKIP_KERNEL_TESTS`]:
//!   the CU-aware stop of I-50 as a work bound, since
//!   `sol_remaining_compute_units` is not active on mainnet). The day's
//!   camp check of the bell where the transaction stops is undone (v1.7),
//!   so the committed bells leave exactly what resolving them would. Settles
//!   run only from the first bell something is due ([`model::next_due`]).
//! - **Digests:** CLASH's `input_digest = sha256("PSF-CLASH-INPUT-v1" ‖
//!   le32(b) ‖ seed ‖ province[SITE_MIRROR..TICKET_COHORTS] before ‖
//!   inputs[ARRIVALS..POSTURES])`; SKIP's `quiet_digest = sha256(
//!   "PSF-QUIET-v1" ‖ le32(b0) ‖ n ‖ province[SITE_MIRROR..TICKET_COHORTS]
//!   after)`.

use alloc::vec::Vec;

use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use frontier_abi::addr::{archive_part_of, clash_inputs_seed, day_of, split_host_id, AddrCtx};
use frontier_abi::entry::{read_entry, write_entry, Entry};
use frontier_abi::ix as aix;
use frontier_abi::layout::AccountKind;
use frontier_abi::log::{close_key, pack_fates, EntityKind, Kind, NO_BELL};
use frontier_abi::prologue::SeasonHdr;
use frontier_abi::tags::Ix;
use permutation_rules::frontier::clash::{self as kc, ClashOutcome};
use permutation_rules::frontier::geometry::region_of;

use crate::clock::SeasonClock;
use crate::error::{kernel, BAD_ACCOUNT, OVERFLOW};
use crate::events::{self, Buf, Chained};
use crate::evidence;
use crate::init::{self, SeasonSigner, Sink};
use crate::layout::beacon::{archive_archived, archive_entry_of, archive_key, Anchor};
use crate::layout::{
    arrival as AR, arrival_day as AD, arrival_slot as AS, clash_inputs as CI, holding as H,
    init_header, province as P, season as S, transit as T, Ro, Rw,
};
use crate::prologue::{self, check_accounts, expect_key, key};
use crate::{FrontierError, R};

pub use model::{Applied, Built, Camp};

/// `transit_slot` of SettleDeparture that asks for the return settle (§21).
pub const RETURN_SLOT: u8 = 0xFF;

/// DEPARTURE_SETTLED's `destroyed` byte of a return settle (troops back in
/// the reserve).
pub const RETURNED: u8 = 2;

/// Most `Leave` entries one return settle frees (v1.7, wave-4 review: the
/// work bound that keeps it within SettleDeparture's 15k CU; ≈ 8.7k for
/// one entry, ≈ 3.7k per further entry). The keeper sends it again while
/// entries remain (`AlreadyDone` once none do).
pub const RETURN_MAX: usize = 3;

/// SkipQuiet's CU-aware stop (I-50) is a work bound, not a read of the
/// remaining units (`sol_remaining_compute_units` is not active on mainnet:
/// LiteSVM 0.16's mainnet feature list of 2026-08-24): the kernel's quiet
/// test (≈ 75k CU at 48 residents, `W4-A-NOTES.md`) runs at most once per
/// transaction, at its first bell; a later bell that is not
/// [`model::trivially_quiet`] ends the transaction, which commits the bells
/// before it. Every other bell costs a few thousand CU.
pub const SKIP_KERNEL_TESTS: u32 = 1;

/// SkipQuiet stops before a bell once the heap has handed out this much
/// (the runtime maps 32 KiB without a heap frame).
pub const SKIP_HEAP_STOP: u64 = 20 * 1024;

/// ClashInputs' `camp_mask` at offset 76: bit k set ⇔ the arrival at
/// position k took the camp (`WORKS_CAMP`). Named in the ABI since v1.8
/// (was `RSV_76`).
pub const CAMP_MASK: usize = CI::CAMP_MASK;

/// `Kernel` (15) sub-codes of the clash area (defined with the model in
/// `frontier_abi::clash_model`).
pub use frontier_abi::clash_model::sub;

// ================================================================ model

/// The clash of one province-bell over account bytes (module note):
/// `frontier_abi::clash_model` (W4-A D8, integ-W5), moved to the ABI crate
/// so the herald, the verifier, itest and the WASM share the program's own
/// code. Here it runs with the heap trace's checkpoints; its errors become
/// program codes through `From<ModelError> for Error` (`Kernel` logs its
/// sub-code there).
pub mod model {
    pub use frontier_abi::clash_model::*;

    use frontier_abi::clash_model as m;
    use permutation_rules::frontier::clash::ClashOutcome;

    /// The heap trace build's checkpoints (a no-op in other builds).
    pub struct HeapProbe;

    impl Probe for HeapProbe {
        #[inline(always)]
        fn checkpoint(tag: u64) {
            crate::heap::trace_checkpoint(tag);
        }
    }

    /// [`m::build`] with the program's checkpoints.
    #[inline]
    pub fn build(pd: &[u8], inputs: Option<&[u8]>, b: u32) -> crate::R<Built> {
        Ok(m::build_probed::<HeapProbe>(pd, inputs, b)?)
    }

    /// [`m::apply`] with the program's checkpoints.
    #[inline]
    pub fn apply(pd: &mut [u8], b: &Built, out: &ClashOutcome) -> crate::R<Applied> {
        Ok(m::apply_probed::<HeapProbe>(pd, b, out)?)
    }
}

// ================================================================ program

/// The heap the bump allocator has handed out (0 off chain).
fn heap_used() -> u64 {
    crate::heap::peak()
}

/// A kernel clash refusal as a program code.
fn clash_err(_e: kc::ClashError) -> crate::Error {
    kernel(sub::CLASH_INPUT)
}

/// The Province at its canonical address from its stored `(P, Q)`
/// (`BadAddress`), present (`BadAccount`): `(P, Q, resolved_next)`.
fn province_of(
    program: &Pubkey,
    ctx: &AddrCtx,
    sid: u64,
    province: &AccountInfo,
) -> R<(i16, i16, u32)> {
    prologue::present(province, program, AccountKind::Province, sid)?;
    let (pp, pq, rn) = {
        let d = province.try_borrow_data()?;
        let r = Ro(&d);
        (r.i16(P::P)?, r.i16(P::Q)?, r.u32(P::RESOLVED_NEXT)?)
    };
    expect_key(province, &ctx.province(pp as i32, pq as i32))?;
    Ok((pp, pq, rn))
}

/// `A` of THE anchor of `(bell, region)` from the account given for it:
/// the anchor itself (present at its canonical address), or the
/// region-half-day archive once the bell is archived. A missing anchor is
/// `NoAnchor`; any other key `BadAddress`.
fn anchor_time(
    program: &Pubkey,
    ctx: &AddrCtx,
    sid: u64,
    genesis_ts: i64,
    bell: u32,
    region: u8,
    ai: &AccountInfo,
) -> R<i64> {
    let part = archive_part_of(bell);
    if ai.key.as_array() == &ctx.anchor_archive(region, part) {
        if !prologue::presence(ai, program, AccountKind::AnchorArchive, sid)? {
            return Err(FrontierError::NoAnchor.into());
        }
        let d = ai.try_borrow_data()?;
        if archive_key(&d)? != (region, part) {
            return Err(BAD_ACCOUNT);
        }
        if !archive_archived(&d, bell)? {
            return Err(FrontierError::NoAnchor.into());
        }
        let (a_off, _, _) = archive_entry_of(&d, bell)?;
        return permutation_rules::frontier::beacon::bell_end(genesis_ts, bell)
            .checked_add(a_off as i64)
            .ok_or(OVERFLOW);
    }
    expect_key(ai, &ctx.bell_anchor(bell, region))?;
    if !prologue::presence(ai, program, AccountKind::BellAnchor, sid)? {
        return Err(FrontierError::NoAnchor.into());
    }
    let an = {
        let d = ai.try_borrow_data()?;
        Anchor::read(&d)?
    };
    if an.bell != bell || an.region != region {
        return Err(BAD_ACCOUNT);
    }
    Ok(an.a)
}

/// The reveal window of `bell` has closed (`TooEarly` otherwise).
#[allow(clippy::too_many_arguments)]
fn window_closed(
    program: &Pubkey,
    ctx: &AddrCtx,
    sid: u64,
    clock: &SeasonClock,
    now: i64,
    bell: u32,
    region: u8,
    ai: &AccountInfo,
) -> R<()> {
    let a = anchor_time(program, ctx, sid, clock.genesis_ts, bell, region, ai)?;
    if now < clock.reveal_close(bell, a) {
        return Err(FrontierError::TooEarly.into());
    }
    Ok(())
}

/// Whether the ArrivalDay of `(P, Q, day(bell))` has `bell`'s bit (absent
/// = clear). The account must be at its canonical address.
fn day_bit(
    program: &Pubkey,
    ctx: &AddrCtx,
    sid: u64,
    (pp, pq): (i16, i16),
    bell: u32,
    ai: &AccountInfo,
) -> R<bool> {
    let day = day_of(bell);
    expect_key(ai, &ctx.arrival_day(pp as i32, pq as i32, day))?;
    if !prologue::presence(ai, program, AccountKind::ArrivalDay, sid)? {
        return Ok(false);
    }
    let d = ai.try_borrow_data()?;
    let r = Ro(&d);
    if r.i16(AD::P)? != pp || r.i16(AD::Q)? != pq || r.u32(AD::DAY)? != day {
        return Err(BAD_ACCOUNT);
    }
    let (at, mask) = AD::bit(bell);
    Ok(r.u8(at)? & mask != 0)
}

/// [`day_bit`] for an account whose address the caller has checked.
fn day_bit_at(
    program: &Pubkey,
    sid: u64,
    (pp, pq): (i16, i16),
    bell: u32,
    ai: &AccountInfo,
) -> R<bool> {
    if !prologue::presence(ai, program, AccountKind::ArrivalDay, sid)? {
        return Ok(false);
    }
    let d = ai.try_borrow_data()?;
    let r = Ro(&d);
    if r.i16(AD::P)? != pp || r.i16(AD::Q)? != pq || r.u32(AD::DAY)? != day_of(bell) {
        return Err(BAD_ACCOUNT);
    }
    let (at, mask) = AD::bit(bell);
    Ok(r.u8(at)? & mask != 0)
}

/// Season statuses a clash write accepts.
const CLASH_STATUS: [u8; 2] = [S::STATUS_RUNNING, S::STATUS_ENDED];

/// Bells at or after `end_bell` are never gathered, resolved or skipped.
fn in_season(hdr: &SeasonHdr, bell: u32) -> R<()> {
    if bell >= hdr.end_bell {
        return Err(FrontierError::WrongStatus.into());
    }
    Ok(())
}

// ------------------------------------------------------------ addresses

/// `with_seed(season, tag ‖ lowercase-hex(raw), program)` from one stack
/// buffer: the §4.1 grammar of `frontier_abi::addr` without its seed
/// validation (≈ 0.3k CU instead of ≈ 0.7k; host test
/// `fast_addresses_are_the_abis`). Gathers recompute up to 22 of them.
fn seed_addr(season: &[u8; 32], program: &[u8; 32], tag: &[u8; 2], raw: &[u8]) -> [u8; 32] {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut b = [0u8; 96];
    b[..32].copy_from_slice(season);
    b[32] = tag[0];
    b[33] = tag[1];
    let mut n = 34;
    for &x in raw.iter().take(frontier_abi::addr::MAX_RAW) {
        b[n] = HEX[(x >> 4) as usize];
        b[n + 1] = HEX[(x & 15) as usize];
        n += 2;
    }
    b[n..n + 32].copy_from_slice(program);
    permutation_rules::hash::sha256(&[&b[..n + 32]])
}

/// The ArrivalSlot `(P, Q, bell, f, i)`.
fn slot_addr(ctx: &AddrCtx, p: i16, q: i16, bell: u32, f: u8, i: u8) -> [u8; 32] {
    let mut raw = [0u8; 14];
    raw[..4].copy_from_slice(&(p as i32).to_le_bytes());
    raw[4..8].copy_from_slice(&(q as i32).to_le_bytes());
    raw[8..12].copy_from_slice(&bell.to_le_bytes());
    raw[12] = f;
    raw[13] = i;
    seed_addr(
        &ctx.season,
        &ctx.program,
        &frontier_abi::addr::tag::ARRIVAL_SLOT,
        &raw,
    )
}

/// The Holding `(P, Q, site)`.
fn holding_addr(ctx: &AddrCtx, p: i32, q: i32, site: u8) -> [u8; 32] {
    let mut raw = [0u8; 9];
    raw[..4].copy_from_slice(&p.to_le_bytes());
    raw[4..8].copy_from_slice(&q.to_le_bytes());
    raw[8] = site;
    seed_addr(
        &ctx.season,
        &ctx.program,
        &frontier_abi::addr::tag::HOLDING,
        &raw,
    )
}

// ------------------------------------------------------------ gathering

/// The ClashInputs record of position `k` from its slot (and the slot's
/// Holding): `Ok(true)` when a host is recorded (module note). A matching
/// transit (state 2 or 3) is stamped
/// with this destination (`FLAG_GATHERED`, `DEST_P`, `DEST_Q`), so
/// SettleTransit can settle it only here (v1.7, W4-B F1). Fixed-offset
/// reads and writes (the per-position cost bounds a gather, §13.1).
#[allow(clippy::too_many_arguments)]
fn gather_position(
    program: &Pubkey,
    ctx: &AddrCtx,
    sid: u64,
    (pp, pq): (i16, i16),
    bell: u32,
    k: usize,
    slot: &AccountInfo,
    holding: Option<&AccountInfo>,
    rec: &mut [u8],
) -> R<bool> {
    let (f, i) = ((k / 4) as u8, (k % 4) as u8);
    expect_key(slot, &slot_addr(ctx, pp, pq, bell, f, i))?;
    let present = prologue::presence(slot, program, AccountKind::ArrivalSlot, sid)?;
    if present != holding.is_some() {
        return Err(FrontierError::BadData.into());
    }
    let rec: &mut [u8; AR::SIZE] = rec.try_into().map_err(|_| BAD_ACCOUNT)?;
    rec.fill(0);
    let Some(holding) = holding else {
        return Ok(false);
    };
    let sd = slot.try_borrow_data()?;
    let s: &[u8; AS::SIZE] = sd
        .get(..AS::SIZE)
        .and_then(|x| x.try_into().ok())
        .ok_or(BAD_ACCOUNT)?;
    let le16 = |o: usize| [s[o], s[o + 1]];
    let le32 = |o: usize| u32::from_le_bytes([s[o], s[o + 1], s[o + 2], s[o + 3]]);
    if i16::from_le_bytes(le16(AS::P)) != pp
        || i16::from_le_bytes(le16(AS::Q)) != pq
        || le32(AS::BELL) != bell
        || s[AS::FACTION] != f
        || s[AS::I] != i
    {
        return Err(BAD_ACCOUNT);
    }
    let mut id8 = [0u8; 8];
    id8.copy_from_slice(&s[AS::HOST_ID..AS::HOST_ID + 8]);
    let host_id = u64::from_le_bytes(id8);
    let parts = split_host_id(host_id).ok_or(BAD_ACCOUNT)?;
    expect_key(
        holding,
        &holding_addr(ctx, parts.province.p, parts.province.q, parts.site),
    )?;
    rec[AR::HOST_ID..AR::HOST_ID + 8].copy_from_slice(&id8);
    rec[AR::CITIZEN_TAG..AR::CITIZEN_TAG + 8]
        .copy_from_slice(&s[AS::CITIZEN_TAG..AS::CITIZEN_TAG + 8]);
    rec[AR::DEP_MASS..AR::DEP_MASS + 4].copy_from_slice(&s[AS::DEP_MASS..AS::DEP_MASS + 4]);
    rec[AR::RETREAT..AR::RETREAT + 2].copy_from_slice(&s[AS::RETREAT_BPS..AS::RETREAT_BPS + 2]);
    rec[AR::DEALT..AR::DEALT + 2].copy_from_slice(&s[AS::DEALT_BPS..AS::DEALT_BPS + 2]);
    rec[AR::FACTION] = f;
    rec[AR::UNIT] = s[AS::UNIT];
    rec[AR::TILE] = s[AS::TILE];
    rec[AR::STANCE] = s[AS::STANCE];
    drop(sd);
    if !prologue::presence(holding, program, AccountKind::Holding, sid)? {
        return Ok(true);
    }
    let mut hd = holding.try_borrow_mut_data()?;
    let live = matches!(
        hd.get(H::STATE).copied(),
        Some(H::STATE_PROVISIONAL | H::STATE_FINAL)
    );
    if !live || hd.get(H::GEN).copied() != Some(parts.gen) {
        return Ok(true);
    }
    for t in 0..H::TRANSIT_N {
        let o = H::transit(t);
        let tr: &[u8; T::SIZE] = hd
            .get(o..o + T::SIZE)
            .and_then(|x| x.try_into().ok())
            .ok_or(BAD_ACCOUNT)?;
        let st = tr[T::STATE];
        let t32 = |o: usize| u32::from_le_bytes([tr[o], tr[o + 1], tr[o + 2], tr[o + 3]]);
        if st == T::STATE_FREE
            || tr[T::HOST_ID..T::HOST_ID + 8] != id8
            || t32(T::ARRIVE_BELL) != bell
        {
            continue;
        }
        let stamp = match st {
            T::STATE_DEPARTED => return Err(FrontierError::DepartureUnsettled.into()),
            T::STATE_DESTROYED_AT_ORIGIN => {
                rec[AR::FATE] = AR::FATE_DESTROYED;
                true
            }
            _ => {
                let troops = t32(T::TROOPS_AFTER);
                let stamina = model::arrival_stamina(
                    u16::from_le_bytes([tr[T::STAMINA_AFTER], tr[T::STAMINA_AFTER + 1]]),
                    t32(T::DEPART_BELL),
                    bell,
                );
                rec[AR::TROOPS..AR::TROOPS + 4].copy_from_slice(&troops.to_le_bytes());
                rec[AR::STAMINA..AR::STAMINA + 2].copy_from_slice(&stamina.to_le_bytes());
                if troops >= permutation_rules::frontier::host::DESTROYED_BELOW {
                    rec[AR::PRESENT] = 1;
                } else {
                    rec[AR::FATE] = AR::FATE_DESTROYED;
                }
                true
            }
        };
        if !stamp {
            return Ok(true);
        }
        let tr = hd.get_mut(o..o + T::SIZE).ok_or(BAD_ACCOUNT)?;
        let same = tr[T::FLAGS] & T::FLAG_GATHERED != 0
            && tr[T::DEST_P..T::DEST_P + 2] == pp.to_le_bytes()
            && tr[T::DEST_Q..T::DEST_Q + 2] == pq.to_le_bytes();
        if same {
            return Ok(true);
        }
        tr[T::FLAGS] |= T::FLAG_GATHERED;
        tr[T::DEST_P..T::DEST_P + 2].copy_from_slice(&pp.to_le_bytes());
        tr[T::DEST_Q..T::DEST_Q + 2].copy_from_slice(&pq.to_le_bytes());
        return Ok(true);
    }
    Ok(true)
}

/// Positions of a gather range: `start..start + n` within the 24.
fn range_mask(start: u8, n: u8) -> R<u32> {
    let end = start as usize + n as usize;
    if n == 0 || end > CI::POSITIONS {
        return Err(FrontierError::BadData.into());
    }
    Ok(((1u64 << end) - (1u64 << start)) as u32)
}

/// 0x60 GatherClash(bell, start, n, holdings_bitmap, beneficiary): K +
/// `[province (dest) r] [anchor|archive r] [arrivalday r] [inputs w] [ix
/// sysvar] [system] [slot_k r …] [holding_k w …]`, class D, top level
/// (v1.7: the Holdings are writable; a matching transit is stamped with
/// this destination and the GATHER record chains each stamped Holding).
pub fn gather_clash(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    let x = aix::GatherClash::decode(d)?;
    let range = range_mask(x.start, x.n)?;
    let n_hold = (x.holdings_bitmap & range).count_ones() as u8;
    check_accounts(Ix::GatherClash, a, Some(&[1, x.n, n_hold]))?;
    prologue::top_level(Ix::GatherClash)?;
    let now = prologue::now()?;
    let hdr = prologue::keeper(a, p, &CLASH_STATUS, now.ts)?;
    crate::heap::trace_checkpoint(0x6001);
    let [fee_payer, season_ai, province, anchor, day_ai, inputs, ix_sysvar, _system, rest @ ..] = a
    else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let sid = hdr.id;
    let ctx = crate::addr::ctx(&key(season_ai), &p.to_bytes());
    let (pp, pq, rn) = province_of(p, &ctx, sid, province)?;
    if x.bell < rn {
        return Err(FrontierError::LatchClosed.into());
    }
    in_season(&hdr, x.bell)?;
    let clock = {
        let sd = season_ai.try_borrow_data()?;
        SeasonClock::read(&sd)?
    };
    let region = region_of(permutation_rules::frontier::geometry::ProvinceCoord::new(
        pp as i32, pq as i32,
    ));
    window_closed(p, &ctx, sid, &clock, now.ts, x.bell, region, anchor)?;
    crate::heap::trace_checkpoint(0x6002);
    expect_key(inputs, &ctx.clash_inputs(pp as i32, pq as i32, x.bell))?;
    let created = !prologue::presence(inputs, p, AccountKind::ClashInputs, sid)?;
    if created {
        let ev = evidence::read(ix_sysvar, now.slot)?;
        init::init_with_seed(
            fee_payer,
            inputs,
            season_ai,
            &SeasonSigner::new(sid, hdr.bump),
            &clash_inputs_seed(pp as i32, pq as i32, x.bell),
            CI::SIZE,
            init::rent(CI::SIZE)?,
            p,
        )?;
        let mut cd = inputs.try_borrow_mut_data()?;
        init_header(&mut cd, AccountKind::ClashInputs, sid)?;
        let mut w = Rw(&mut cd);
        w.set_i16(CI::P, pp)?;
        w.set_i16(CI::Q, pq)?;
        w.set_u32(CI::BELL, x.bell)?;
        w.set_arr(CI::RENT_TO, fee_payer.key.as_ref())?;
        w.set_u64(CI::EV_SLOT, ev.slot)?;
        w.set_u64(CI::EV_PRICE, ev.price)?;
        w.set_u32(CI::EV_LIMIT, ev.limit)?;
    } else {
        let cd = inputs.try_borrow_data()?;
        let r = Ro(&cd);
        if r.i16(CI::P)? != pp || r.i16(CI::Q)? != pq || r.u32(CI::BELL)? != x.bell {
            return Err(BAD_ACCOUNT);
        }
    }
    crate::heap::trace_checkpoint(0x6003);
    let (slots, holdings) = rest.split_at(x.n as usize);
    let mut no_arrivals = false;
    let mask = {
        let mut cd = inputs.try_borrow_mut_data()?;
        let mut mask = Ro(&cd).u32(CI::ARRIVALS_MASK)?;
        if !day_bit(p, &ctx, sid, (pp, pq), x.bell, day_ai)? {
            no_arrivals = true;
            let mut w = Rw(&mut cd);
            let f = w.u8(CI::FLAGS)?;
            w.set_u8(CI::FLAGS, f | CI::FLAG_NO_ARRIVALS)?;
            mask = CI::ALL_GATHERED;
        } else {
            let mut hi = 0usize;
            for (j, slot) in slots.iter().enumerate() {
                let k = x.start as usize + j;
                let has = x.holdings_bitmap & (1 << k) != 0;
                let holding = if has {
                    let h = holdings.get(hi).ok_or(FrontierError::TooManyAccounts)?;
                    hi += 1;
                    Some(h)
                } else {
                    None
                };
                if mask & (1 << k) != 0 {
                    continue;
                }
                let o = CI::arrival(k);
                let rec = cd.get_mut(o..o + AR::SIZE).ok_or(BAD_ACCOUNT)?;
                gather_position(p, &ctx, sid, (pp, pq), x.bell, k, slot, holding, rec)?;
                crate::heap::trace_checkpoint(0x6010 + k as u64);
                mask |= 1 << k;
            }
            let mut n_present = 0u8;
            for k in 0..CI::POSITIONS {
                if cd[CI::arrival(k) + AR::PRESENT] == 1 {
                    n_present += 1;
                }
            }
            Rw(&mut cd).set_u8(CI::N_PRESENT, n_present)?;
        }
        Rw(&mut cd).set_u32(CI::ARRIVALS_MASK, mask)?;
        mask
    };
    crate::heap::trace_checkpoint(0x6004);
    let key12 = pqb_key(pp, pq, x.bell);
    let payload = Buf::<7>::new()
        .u8(x.start)
        .u8(x.n)
        .u32(mask)
        .u8(no_arrivals as u8);
    // Every present Holding the part lists (each key once, account order)
    // is chained (v1.7: the gather may stamp its transit record; chaining
    // every listed one, stamped or not, keeps the record's `InTx` links in
    // the transaction's account order for any reader).
    let mut chain: Vec<usize> = Vec::with_capacity(holdings.len());
    for (j, h) in holdings.iter().enumerate() {
        if chain.iter().any(|&c| holdings[c].key == h.key) {
            continue;
        }
        if prologue::presence(h, p, AccountKind::Holding, sid)? {
            chain.push(j);
        }
    }
    let mut cd = inputs.try_borrow_mut_data()?;
    let mut hds = Vec::with_capacity(chain.len());
    for &j in &chain {
        hds.push(holdings[j].try_borrow_mut_data()?);
    }
    let mut list: Vec<Chained> = Vec::with_capacity(1 + hds.len());
    list.push(Chained {
        entity: EntityKind::ClashInputs,
        data: &mut cd,
    });
    for hd in hds.iter_mut() {
        list.push(Chained {
            entity: EntityKind::Holding,
            data: hd,
        });
    }
    events::emit(
        Kind::GATHER,
        hdr.bell(now.ts).unwrap_or(NO_BELL),
        &key12,
        payload.get()?,
        &mut list,
    )
}

/// `le32(P) ‖ le32(Q) ‖ le32(bell)` (records' key).
fn pqb_key(p: i16, q: i16, bell: u32) -> [u8; 12] {
    let mut k = [0u8; 12];
    k[..4].copy_from_slice(&(p as i32).to_le_bytes());
    k[4..8].copy_from_slice(&(q as i32).to_le_bytes());
    k[8..].copy_from_slice(&bell.to_le_bytes());
    k
}

/// `le32(P) ‖ le32(Q)`.
fn pq_key(p: i16, q: i16) -> [u8; 8] {
    let mut k = [0u8; 8];
    k[..4].copy_from_slice(&(p as i32).to_le_bytes());
    k[4..].copy_from_slice(&(q as i32).to_le_bytes());
    k
}

// ------------------------------------------------------------ resolving

/// The resolve of bell `b` over the Province and gathered inputs bytes:
/// build, kernel, write-back, settle, fate table. Returns the outcome
/// digest, the input digest, the engagements and the fates.
struct Resolved {
    outcome: [u8; 32],
    input: [u8; 32],
    engagements: u32,
    applied: Applied,
    camp: Option<Camp>,
}

fn resolve_core(pd: &mut [u8], ci: &mut [u8], b: u32, seed: &[u8; 32]) -> R<Resolved> {
    let input = model::input_digest(pd, ci, b, seed)?;
    crate::heap::trace_checkpoint(0x6100);
    let built = model::build(pd, Some(ci), b)?;
    crate::heap::trace_checkpoint(0x6101);
    let out: ClashOutcome =
        kc::resolve_clash(&kc::frontier_ruleset(), &built.input(seed)).map_err(clash_err)?;
    crate::heap::trace_checkpoint(0x6102);
    let applied = model::apply(pd, &built, &out)?;
    crate::heap::trace_checkpoint(0x6122);
    let settled = model::settle_bell(pd, b)?;
    crate::heap::trace_checkpoint(0x6123);
    model::finish_bell(pd, b, applied.changed || settled)?;
    crate::heap::trace_checkpoint(0x6103);
    let digest = model::outcome_digest(&out)?;
    crate::heap::trace_checkpoint(0x6124);
    {
        let mut w = Rw(pd);
        w.set_arr(P::LAST_DIGEST, &digest)?;
    }
    {
        let mut w = Rw(ci);
        for k in 0..CI::POSITIONS {
            let o = CI::arrival(k);
            if w.u8(o + AR::PRESENT)? == 1 {
                w.set_u8(o + AR::FATE, applied.fates[k])?;
                w.set_u32(o + AR::TROOPS_AFTER, applied.troops_after[k])?;
            }
        }
        let f = w.u8(CI::FLAGS)?;
        w.set_u8(CI::FLAGS, f | CI::FLAG_RESOLVED)?;
        w.set_u32(CAMP_MASK, applied.camp_mask)?;
    }
    crate::heap::trace_checkpoint(0x6125);
    let camp = (built.camp_checked == Some(true)).then_some(built.camp);
    Ok(Resolved {
        outcome: digest,
        input,
        engagements: out.engagements,
        applied,
        camp,
    })
}

/// The summary of the last resolve (§5.3).
fn write_summary(pd: &mut [u8], b: u32, r: &Resolved, n_arr: u8, beneficiary: &[u8; 32]) -> R<()> {
    use crate::layout::summary as SU;
    let o = P::RESOLVE_SUMMARY;
    let mut w = Rw(pd);
    w.set_u32(o + SU::BELL, b)?;
    w.set_u32(o + SU::ENGAGEMENTS, r.engagements)?;
    w.set_u8(o + SU::ARRIVALS, n_arr)?;
    w.set_u8(o + SU::DESTROYED, r.applied.destroyed)?;
    w.set_u8(o + SU::BOUNCED, r.applied.bounced)?;
    w.set_arr(o + SU::RESOLVER, &beneficiary[..8])
}

/// Emits CAMP for a camp that spawned at this bell (and for the clear).
fn emit_camp(pd: &mut [u8], pp: i16, pq: i16, c: &Camp, bell_log: u32, day: u32) -> R<()> {
    let payload = Buf::<9>::new().u8(c.tile).u32(c.troops).u32(day);
    events::emit(
        Kind::CAMP,
        bell_log,
        &pq_key(pp, pq),
        payload.get()?,
        &mut [Chained {
            entity: EntityKind::Province,
            data: pd,
        }],
    )
}

/// Emits CLASH (Province and ClashInputs chained).
#[allow(clippy::too_many_arguments)]
fn emit_clash(
    pd: &mut [u8],
    ci: &mut [u8],
    pp: i16,
    pq: i16,
    b: u32,
    r: &Resolved,
    bell_log: u32,
) -> R<()> {
    let payload = Buf::<77>::new()
        .bytes(&r.outcome)
        .bytes(&r.input)
        .u32(r.engagements)
        .bytes(&pack_fates(&r.applied.fates));
    events::emit(
        Kind::CLASH,
        bell_log,
        &pqb_key(pp, pq, b),
        payload.get()?,
        &mut [
            Chained {
                entity: EntityKind::Province,
                data: pd,
            },
            Chained {
                entity: EntityKind::ClashInputs,
                data: ci,
            },
        ],
    )
}

/// The camp records of a resolve: a spawn (before the clash) and a clear.
fn emit_camps(pd: &mut [u8], pp: i16, pq: i16, b: u32, r: &Resolved, bell_log: u32) -> R<()> {
    if let Some(c) = &r.camp {
        emit_camp(pd, pp, pq, c, bell_log, b / model::DAY_BELLS)?;
    }
    if r.applied.camp_cleared {
        let c = Camp::read(pd)?;
        emit_camp(pd, pp, pq, &c, bell_log, b / model::DAY_BELLS)?;
    }
    Ok(())
}

/// 0x61 ResolveFromInputs(bell, beneficiary): K + `[province w] [inputs w]
/// [seedcache|archive r] [anchor|archive r] [ix sysvar]`, class D, top
/// level.
pub fn resolve_from_inputs(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::ResolveFromInputs, a, None)?;
    let x = aix::ResolveFromInputs::decode(d)?;
    prologue::top_level(Ix::ResolveFromInputs)?;
    let now = prologue::now()?;
    let hdr = prologue::keeper(a, p, &CLASH_STATUS, now.ts)?;
    let [_fee_payer, season_ai, province, inputs, seed_ai, anchor_ai, ix_sysvar] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    crate::heap::trace_checkpoint(0x6110);
    let sid = hdr.id;
    let ctx = crate::addr::ctx(&key(season_ai), &p.to_bytes());
    let (pp, pq, rn) = province_of(p, &ctx, sid, province)?;
    if x.bell != rn {
        return Err(FrontierError::OutOfOrder.into());
    }
    in_season(&hdr, x.bell)?;
    expect_key(inputs, &ctx.clash_inputs(pp as i32, pq as i32, x.bell))?;
    if !prologue::presence(inputs, p, AccountKind::ClashInputs, sid)? {
        return Err(FrontierError::NotGathered.into());
    }
    {
        let cd = inputs.try_borrow_data()?;
        let r = Ro(&cd);
        if r.i16(CI::P)? != pp || r.i16(CI::Q)? != pq || r.u32(CI::BELL)? != x.bell {
            return Err(BAD_ACCOUNT);
        }
        if r.u32(CI::ARRIVALS_MASK)? != CI::ALL_GATHERED {
            return Err(FrontierError::NotGathered.into());
        }
    }
    let clock = {
        let sd = season_ai.try_borrow_data()?;
        SeasonClock::read(&sd)?
    };
    let region = region_of(permutation_rules::frontier::geometry::ProvinceCoord::new(
        pp as i32, pq as i32,
    ));
    let seed = super::holding::bell_seed(p, &ctx, sid, &clock, x.bell, region, seed_ai, anchor_ai)?;
    let ev = evidence::read(ix_sysvar, now.slot)?;
    crate::heap::trace_checkpoint(0x6111);
    let bell_log = hdr.bell(now.ts).unwrap_or(NO_BELL);
    let mut pd = province.try_borrow_mut_data()?;
    let mut cd = inputs.try_borrow_mut_data()?;
    let r = resolve_core(&mut pd, &mut cd, x.bell, &seed)?;
    let n_arr = Ro(&cd).u8(CI::N_PRESENT)?;
    write_summary(&mut pd, x.bell, &r, n_arr, &x.beneficiary)?;
    {
        let mut w = Rw(&mut cd);
        w.set_arr(CI::RESOLVER, &x.beneficiary)?;
        w.set_u64(CI::EV_SLOT, ev.slot)?;
        w.set_u64(CI::EV_PRICE, ev.price)?;
        w.set_u32(CI::EV_LIMIT, ev.limit)?;
        let t = now
            .ts
            .saturating_sub(clock.genesis_ts)
            .clamp(0, u32::MAX as i64);
        w.set_u32(CI::RESOLVED_TS, t as u32)?;
    }
    emit_camps(&mut pd, pp, pq, x.bell, &r, bell_log)?;
    crate::heap::trace_checkpoint(0x6126);
    emit_clash(&mut pd, &mut cd, pp, pq, x.bell, &r, bell_log)?;
    crate::heap::trace_checkpoint(0x6112);
    Ok(())
}

/// 0x62 ResolveClash(bell, beneficiary), feature `oracle` (tests): K +
/// `[province w]` + `[seedcache|archive] [anchor|archive] [arrivalday]
/// [slot × 24] [holding × m]` (the Holdings of the present slots in
/// position order). Gathers into a ClashInputs image in memory and runs
/// the resolve ResolveFromInputs runs; writes the Province only.
#[cfg(feature = "oracle")]
pub fn resolve_clash(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::ResolveClash, a, None)?;
    let x = aix::ResolveClash::decode(d)?;
    prologue::top_level(Ix::ResolveClash)?;
    let now = prologue::now()?;
    let hdr = prologue::keeper(a, p, &CLASH_STATUS, now.ts)?;
    let [_fee_payer, season_ai, province, seed_ai, anchor_ai, day_ai, rest @ ..] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    if rest.len() < CI::POSITIONS {
        return Err(FrontierError::TooManyAccounts.into());
    }
    let (slots, holdings) = rest.split_at(CI::POSITIONS);
    let sid = hdr.id;
    let ctx = crate::addr::ctx(&key(season_ai), &p.to_bytes());
    let (pp, pq, rn) = province_of(p, &ctx, sid, province)?;
    if x.bell != rn {
        return Err(FrontierError::OutOfOrder.into());
    }
    in_season(&hdr, x.bell)?;
    let clock = {
        let sd = season_ai.try_borrow_data()?;
        SeasonClock::read(&sd)?
    };
    let region = region_of(permutation_rules::frontier::geometry::ProvinceCoord::new(
        pp as i32, pq as i32,
    ));
    window_closed(p, &ctx, sid, &clock, now.ts, x.bell, region, anchor_ai)?;
    let seed = super::holding::bell_seed(p, &ctx, sid, &clock, x.bell, region, seed_ai, anchor_ai)?;
    let mut ci = alloc::vec![0u8; CI::SIZE];
    init_header(&mut ci, AccountKind::ClashInputs, sid)?;
    if day_bit(p, &ctx, sid, (pp, pq), x.bell, day_ai)? {
        let mut hi = 0usize;
        for (k, slot) in slots.iter().enumerate() {
            let present = prologue::presence(slot, p, AccountKind::ArrivalSlot, sid)?;
            let holding = if present {
                let h = holdings.get(hi).ok_or(FrontierError::TooManyAccounts)?;
                hi += 1;
                Some(h)
            } else {
                None
            };
            let o = CI::arrival(k);
            gather_position(
                p,
                &ctx,
                sid,
                (pp, pq),
                x.bell,
                k,
                slot,
                holding,
                &mut ci[o..o + AR::SIZE],
            )?;
        }
    }
    let bell_log = hdr.bell(now.ts).unwrap_or(NO_BELL);
    let mut pd = province.try_borrow_mut_data()?;
    let r = resolve_core(&mut pd, &mut ci, x.bell, &seed)?;
    let n_arr = ci
        .iter()
        .skip(CI::ARRIVALS + AR::PRESENT)
        .step_by(AR::SIZE)
        .take(CI::POSITIONS)
        .filter(|v| **v == 1)
        .count() as u8;
    write_summary(&mut pd, x.bell, &r, n_arr, &x.beneficiary)?;
    emit_camps(&mut pd, pp, pq, x.bell, &r, bell_log)?;
    emit_clash(&mut pd, &mut ci, pp, pq, x.bell, &r, bell_log)
}

// ------------------------------------------------------------ skipping

/// 0x63 SkipQuiet(b0, n): `[payer s] [season] [province w] [arrivalday_0
/// r] [arrivalday_1 r] [anchor_or_archive × n r]`, class D, top level.
/// `arrivalday_0` is `ad‖(P, Q, day(b0))`, `arrivalday_1` `ad‖(P, Q,
/// day(b0) + 1)`; anchor k is THE anchor of `b0 + k` or its archive.
pub fn skip_quiet(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::SkipQuiet, a, None)?;
    let x = aix::SkipQuiet::decode(d)?;
    let [_payer, season_ai, province, day0, day1, anchors @ ..] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    if x.n == 0 || x.n as usize > 24 || anchors.len() != x.n as usize {
        return Err(FrontierError::BadData.into());
    }
    prologue::top_level(Ix::SkipQuiet)?;
    let now = prologue::now()?;
    let hdr = prologue::season(
        season_ai,
        p,
        Some(&crate::RULESET_HASH),
        &CLASH_STATUS,
        now.ts,
    )?;
    let sid = hdr.id;
    let ctx = crate::addr::ctx(&key(season_ai), &p.to_bytes());
    let (pp, pq, rn) = province_of(p, &ctx, sid, province)?;
    if x.b0 != rn {
        return Err(FrontierError::OutOfOrder.into());
    }
    let clock = {
        let sd = season_ai.try_borrow_data()?;
        SeasonClock::read(&sd)?
    };
    let coord = permutation_rules::frontier::geometry::ProvinceCoord::new(pp as i32, pq as i32);
    let region = region_of(coord);
    let d0 = day_of(x.b0);
    expect_key(day0, &ctx.arrival_day(pp as i32, pq as i32, d0))?;
    expect_key(
        day1,
        &ctx.arrival_day(pp as i32, pq as i32, d0.checked_add(1).ok_or(OVERFLOW)?),
    )?;
    let bell_log = hdr.bell(now.ts).unwrap_or(NO_BELL);
    let mut done = 0u8;
    let mut quiet_known = false;
    let mut tests = 0u32;
    let mut spawns: Vec<(Camp, u32)> = Vec::new();
    let mut pd = province.try_borrow_mut_data()?;
    let mut due = model::next_due(&pd)?;
    for (k, anchor) in anchors.iter().enumerate() {
        let b =
            x.b0.checked_add(u32::try_from(k).map_err(|_| OVERFLOW)?)
                .ok_or(OVERFLOW)?;
        let first = k == 0;
        let camp_due = day_of(b) >= Camp::read(&pd)?.next_check_day;
        if !first && heap_used() > SKIP_HEAP_STOP {
            break;
        }
        // (1) the window; bells past the season end are never skipped
        let step = in_season(&hdr, b)
            .and_then(|_| window_closed(p, &ctx, sid, &clock, now.ts, b, region, anchor));
        if let Err(e) = step {
            if first {
                return Err(e);
            }
            break;
        }
        // (2) no arrival at b (the two days' addresses were checked above)
        let day_ai = if day_of(b) == d0 { day0 } else { day1 };
        if day_bit_at(p, sid, (pp, pq), b, day_ai)? {
            if first {
                return Err(FrontierError::NotQuiet.into());
            }
            break;
        }
        // (3) the camp's daily check lands first (the roster frozen at b
        // includes the camp it leaves). It is provisional until bell b
        // passes the quiet test: a stop at b undoes it, so the committed
        // bells b0..b0 + n − 1 leave exactly what resolving them would
        // (wave-4 review; the next transaction runs the check again).
        let mut changed = false;
        let mut camp_undo: Option<Camp> = None;
        if camp_due {
            let t = model::terrain_of(&pd)?;
            if let Some((c, spawned)) = model::camp_check(&pd, &t, b)? {
                camp_undo = Some(Camp::read(&pd)?);
                c.write(&mut pd)?;
                if spawned {
                    spawns.push((c, day_of(b)));
                    changed = true;
                    quiet_known = false;
                }
            }
        }
        crate::heap::trace_checkpoint(0x6301);
        // Stops before bell b: the camp check of b is undone.
        let stop_at_b = |pd: &mut [u8], spawns: &mut Vec<(Camp, u32)>| -> R<()> {
            if let Some(c) = camp_undo {
                c.write(pd)?;
                if changed {
                    spawns.pop();
                }
            }
            Ok(())
        };
        // (4) quiet: the trivial test, else (first bell only) the kernel's,
        // heap scoped; recomputed after any change
        if !quiet_known {
            let q = if model::trivially_quiet(&pd, b)? {
                true
            } else if tests < SKIP_KERNEL_TESTS && first {
                tests += 1;
                let q = crate::heap::scoped(|| {
                    let built = model::build(&pd, None, b).map_err(|_| ())?;
                    kc::is_quiet(&kc::frontier_ruleset(), &built.input(&[0; 32])).map_err(|_| ())
                });
                match q {
                    Ok(q) => q,
                    Err(()) => {
                        // Re-run outside the scope for the error itself.
                        let built = model::build(&pd, None, b)?;
                        kc::is_quiet(&kc::frontier_ruleset(), &built.input(&[0; 32]))
                            .map_err(clash_err)?
                    }
                }
            } else {
                // Left to the next transaction's first bell.
                stop_at_b(&mut pd, &mut spawns)?;
                break;
            };
            if !q {
                if first {
                    return Err(FrontierError::NotQuiet.into());
                }
                stop_at_b(&mut pd, &mut spawns)?;
                break;
            }
            quiet_known = true;
        }
        crate::heap::trace_checkpoint(0x6302);
        // (5) the bell's settle (only from the first bell something is due)
        if b >= due {
            changed |= model::settle_bell(&mut pd, b)?;
            due = model::next_due(&pd)?;
        }
        model::finish_bell(&mut pd, b, changed)?;
        if changed {
            quiet_known = false;
        }
        done += 1;
        crate::heap::trace_checkpoint(0x6303);
    }
    for (c, day) in &spawns {
        emit_camp(&mut pd, pp, pq, c, bell_log, *day)?;
    }
    let qd = model::quiet_digest(&pd, x.b0, done)?;
    let payload = Buf::<37>::new().u32(x.b0).u8(done).bytes(&qd);
    events::emit(
        Kind::SKIP,
        bell_log,
        &pq_key(pp, pq),
        payload.get()?,
        &mut [Chained {
            entity: EntityKind::Province,
            data: &mut pd,
        }],
    )
}

// ------------------------------------------------------------ closes

/// Closes allowed while the season runs, after its end, or once aborted.
const CLOSE_STATUS: [u8; 3] = [S::STATUS_RUNNING, S::STATUS_ENDED, S::STATUS_ABORTED];

/// Emits CLOSE for a short-header account (no chain): final seq 0, zero
/// head.
fn emit_close_short(
    kind: AccountKind,
    raw: &[u8],
    recipient: &[u8; 32],
    lamports: u64,
    bell: u32,
) -> R<()> {
    let k = close_key(kind, raw).ok_or(BAD_ACCOUNT)?;
    let payload = Buf::<80>::new()
        .u64(0)
        .bytes(&[0u8; 32])
        .bytes(recipient)
        .u64(lamports);
    events::emit(Kind::CLOSE, bell, &k, payload.get()?, &mut [])
}

/// `rent_to` of the account must be `recipient` (`BadAccount`).
fn rent_to_is(d: &[u8], off: usize, recipient: &AccountInfo) -> R<()> {
    if Ro(d).arr::<32>(off)? != recipient.key.to_bytes() {
        return Err(BAD_ACCOUNT);
    }
    Ok(())
}

/// Whether the Season Ended at least 72 h ago (the season-end fallback of
/// the closes: CloseArrivalSlot case (b), CloseClashInputs,
/// CloseArrivalDay; v1.7).
fn ended_long_ago(hdr: &SeasonHdr, now: i64) -> bool {
    hdr.status == S::STATUS_ENDED && {
        let end = super::map::season_end_ts(hdr);
        now >= end.saturating_add(super::map::END_GRACE_SECS)
    }
}

/// The Season of a keeper-float close (W6-B, W5-A O4): live (Running,
/// Ended or Aborted: [`CLOSE_STATUS`], the close's own rules) or the
/// Closed tombstone (every reader of the account is gone: it closes at
/// once).
pub(crate) enum FloatSeason {
    Live(SeasonHdr),
    Tomb(u64),
}

impl FloatSeason {
    pub(crate) fn id(&self) -> u64 {
        match self {
            FloatSeason::Live(h) => h.id,
            FloatSeason::Tomb(id) => *id,
        }
    }

    /// Whether the close's own rules apply: a live Season that has not
    /// been Ended for 72 h (the season-end fallback and the tombstone
    /// close without them).
    fn rules_apply(&self, now: i64) -> bool {
        matches!(self, FloatSeason::Live(h) if !ended_long_ago(h, now))
    }

    /// The bell of the CLOSE record (`NO_BELL` on the tombstone, as
    /// CloseSeason logs there).
    pub(crate) fn log_bell(&self, now: i64) -> u32 {
        match self {
            FloatSeason::Live(h) => h.bell(now).unwrap_or(NO_BELL),
            FloatSeason::Tomb(_) => NO_BELL,
        }
    }
}

/// [`FloatSeason`] of `season_ai`: the tombstone, else the prologue with
/// `statuses` (`CLOSE_STATUS` for the clash closes).
pub(crate) fn float_season_with(
    season_ai: &AccountInfo,
    p: &Pubkey,
    statuses: &[u8],
    now: i64,
) -> R<FloatSeason> {
    if let Some(id) = super::season::float_tombstone(season_ai, p)? {
        return Ok(FloatSeason::Tomb(id));
    }
    Ok(FloatSeason::Live(prologue::season(
        season_ai,
        p,
        Some(&crate::RULESET_HASH),
        statuses,
        now,
    )?))
}

fn float_season(season_ai: &AccountInfo, p: &Pubkey, now: i64) -> R<FloatSeason> {
    float_season_with(season_ai, p, &CLOSE_STATUS, now)
}

/// 0x64 CloseClashInputs(P, Q, bell): `[any s] [season] [province r]
/// [inputs w] [rent_to w]`, class N. (a) The inputs are resolved, every
/// **present** record settled (`settled_mask`; v1.7: a record with a host
/// id and `present = 0` — absent, released or re-founded Holding, no
/// matching transit, destroyed at the origin — has no SettleTransit that
/// could set its bit), and the close grace passed; or (b) no-arrival
/// inputs (`FLAG_NO_ARRIVALS`) whose bell the Province has passed by a
/// skip (`resolved_next > bell`, v1.7: a gather of a bell that another
/// keeper then skipped), after the grace; or (c) the Season Ended at
/// least 72 h ago; or (d) (W6-B, W5-A O4) the Season is the Closed
/// tombstone. `InputsOpen` otherwise. In cases (c) and (d) the Province is
/// not read: only its canonical address is checked, and it may be absent
/// (CloseProvince may have run first; before W6-B that locked the inputs).
pub fn close_clash_inputs(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::CloseClashInputs, a, None)?;
    let x = aix::CloseClashInputs::decode(d)?;
    let [_any, season_ai, province, inputs, rent_to] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let now = prologue::now()?;
    let live = float_season(season_ai, p, now.ts)?;
    let sid = live.id();
    let ctx = crate::addr::ctx(&key(season_ai), &p.to_bytes());
    let (pi, qi) = (x.p as i32, x.q as i32);
    let rules_apply = live.rules_apply(now.ts);
    if rules_apply {
        super::map::present_at(
            province,
            &ctx.province(pi, qi),
            p,
            AccountKind::Province,
            sid,
        )?;
    } else {
        expect_key(province, &ctx.province(pi, qi))?;
    }
    super::map::present_at(
        inputs,
        &ctx.clash_inputs(pi, qi, x.bell),
        p,
        AccountKind::ClashInputs,
        sid,
    )?;
    {
        let cd = inputs.try_borrow_data()?;
        let r = Ro(&cd);
        if r.i16(CI::P)? != x.p || r.i16(CI::Q)? != x.q || r.u32(CI::BELL)? != x.bell {
            return Err(BAD_ACCOUNT);
        }
        rent_to_is(&cd, CI::RENT_TO, rent_to)?;
        if let (true, FloatSeason::Live(hdr)) = (rules_apply, &live) {
            let grace = {
                let sd = season_ai.try_borrow_data()?;
                Ro(&sd).u32(S::CLASH_CLOSE_GRACE)?
            };
            let rn = {
                let pd = province.try_borrow_data()?;
                Ro(&pd).u32(P::RESOLVED_NEXT)?
            };
            let flags = r.u8(CI::FLAGS)?;
            let grace_secs = (grace as i64).checked_mul(600).ok_or(OVERFLOW)?;
            let since_genesis = now.ts.saturating_sub(hdr.genesis_ts);
            if flags & CI::FLAG_RESOLVED != 0 {
                let settled = r.u32(CI::SETTLED_MASK)?;
                for k in 0..CI::POSITIONS {
                    let o = CI::arrival(k);
                    if r.u8(o + AR::PRESENT)? != 0 && settled & (1 << k) == 0 {
                        return Err(FrontierError::InputsOpen.into());
                    }
                }
                let open_until = (r.u32(CI::RESOLVED_TS)? as i64)
                    .checked_add(grace_secs)
                    .ok_or(OVERFLOW)?;
                if open_until > since_genesis {
                    return Err(FrontierError::InputsOpen.into());
                }
            } else if flags & CI::FLAG_NO_ARRIVALS != 0 && rn > x.bell {
                // Skipped past after this gather: never resolved, nothing
                // recorded. The grace runs from the bell's end.
                let bell_end = (x.bell as i64)
                    .checked_add(1)
                    .and_then(|b| b.checked_mul(600))
                    .ok_or(OVERFLOW)?;
                if bell_end.checked_add(grace_secs).ok_or(OVERFLOW)? > since_genesis {
                    return Err(FrontierError::InputsOpen.into());
                }
            } else {
                return Err(FrontierError::InputsOpen.into());
            }
        }
    }
    let bell = live.log_bell(now.ts);
    {
        let lamports = inputs.lamports();
        let mut cd = inputs.try_borrow_mut_data()?;
        super::map::emit_close(
            AccountKind::ClashInputs,
            EntityKind::ClashInputs,
            &pqb_key(x.p, x.q, x.bell),
            &mut cd,
            &key(rent_to),
            lamports,
            bell,
        )?;
    }
    init::close_to(p, inputs, rent_to, &Sink::Never, bell)?;
    Ok(())
}

/// 0x65 CloseArrivalDay(P, Q, day): `[any s] [season] [province r] [day
/// w] [rent_to w]`, class N, once the province resolved every bell of the
/// day, or the Season Ended at least 72 h ago (v1.7: bells at or past
/// `end_bell` are never resolved), or (W6-B, W5-A O4) the Season is the
/// Closed tombstone (`TooEarly`). In the last two cases the Province is
/// not read (canonical address only; it may be absent).
pub fn close_arrival_day(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::CloseArrivalDay, a, None)?;
    let x = aix::CloseArrivalDay::decode(d)?;
    let [_any, season_ai, province, day_ai, rent_to] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let now = prologue::now()?;
    let live = float_season(season_ai, p, now.ts)?;
    let sid = live.id();
    let ctx = crate::addr::ctx(&key(season_ai), &p.to_bytes());
    let (pi, qi) = (x.p as i32, x.q as i32);
    let rules_apply = live.rules_apply(now.ts);
    if rules_apply {
        super::map::present_at(
            province,
            &ctx.province(pi, qi),
            p,
            AccountKind::Province,
            sid,
        )?;
    } else {
        expect_key(province, &ctx.province(pi, qi))?;
    }
    super::map::present_at(
        day_ai,
        &ctx.arrival_day(pi, qi, x.day),
        p,
        AccountKind::ArrivalDay,
        sid,
    )?;
    {
        let dd = day_ai.try_borrow_data()?;
        let r = Ro(&dd);
        if r.i16(AD::P)? != x.p || r.i16(AD::Q)? != x.q || r.u32(AD::DAY)? != x.day {
            return Err(BAD_ACCOUNT);
        }
        rent_to_is(&dd, AD::RENT_TO, rent_to)?;
    }
    if rules_apply {
        let rn = {
            let pd = province.try_borrow_data()?;
            Ro(&pd).u32(P::RESOLVED_NEXT)?
        };
        let need = (x.day as u64 + 1) * model::DAY_BELLS as u64;
        if (rn as u64) < need {
            return Err(FrontierError::TooEarly.into());
        }
    }
    let bell = live.log_bell(now.ts);
    let mut raw = [0u8; 12];
    raw[..4].copy_from_slice(&pi.to_le_bytes());
    raw[4..8].copy_from_slice(&qi.to_le_bytes());
    raw[8..].copy_from_slice(&x.day.to_le_bytes());
    emit_close_short(
        AccountKind::ArrivalDay,
        &raw,
        &key(rent_to),
        day_ai.lamports(),
        bell,
    )?;
    init::close_to(p, day_ai, rent_to, &Sink::Never, bell)?;
    Ok(())
}

/// 0x66 CloseArrivalSlot(P, Q, bell, f, i): `[any s] [season] [slot w]
/// [rent_to w] ([anchor r])`, class N. (a) SettleTransit set the slot's
/// `settled` flag and the slot is claimed or its claim grace passed
/// (`close + 6 bells`; THE anchor gives `close`, an archived anchor —
/// absent at its canonical address — means the grace passed long ago); or
/// (b) the season Ended at least 72 h ago; or (W6-B, W5-A O4) the Season
/// is the Closed tombstone. In (b) and on the tombstone the anchor, if
/// passed, is not read. `TooEarly` otherwise.
pub fn close_arrival_slot(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::CloseArrivalSlot, a, None)?;
    let x = aix::CloseArrivalSlot::decode(d)?;
    let (slot, rent_to, anchor) = match a {
        [_any, _season, slot, rent_to] => (slot, rent_to, None),
        [_any, _season, slot, rent_to, anchor] => (slot, rent_to, Some(anchor)),
        _ => return Err(FrontierError::TooManyAccounts.into()),
    };
    let season_ai = &a[1];
    let now = prologue::now()?;
    let live = float_season(season_ai, p, now.ts)?;
    let sid = live.id();
    let ctx = crate::addr::ctx(&key(season_ai), &p.to_bytes());
    let (pi, qi) = (x.p as i32, x.q as i32);
    super::map::present_at(
        slot,
        &ctx.arrival_slot(pi, qi, x.bell, x.faction, x.i),
        p,
        AccountKind::ArrivalSlot,
        sid,
    )?;
    let (flags, claimed) = {
        let sd = slot.try_borrow_data()?;
        let r = Ro(&sd);
        if r.i16(AS::P)? != x.p
            || r.i16(AS::Q)? != x.q
            || r.u32(AS::BELL)? != x.bell
            || r.u8(AS::FACTION)? != x.faction
            || r.u8(AS::I)? != x.i
        {
            return Err(BAD_ACCOUNT);
        }
        rent_to_is(&sd, AS::RENT_TO, rent_to)?;
        (r.u8(AS::FLAGS)?, r.u8(AS::CLAIMED)? != 0)
    };
    // (b) and (W6-B) the tombstone: the anchor is not read.
    let rules_apply = live.rules_apply(now.ts);
    let case_a = !rules_apply
        || flags & AS::FLAG_SETTLED != 0
            && (claimed
                || match anchor {
                    None => false,
                    Some(an) => {
                        let region = region_of(
                            permutation_rules::frontier::geometry::ProvinceCoord::new(pi, qi),
                        );
                        expect_key(an, &ctx.bell_anchor(x.bell, region))?;
                        if prologue::presence(an, p, AccountKind::BellAnchor, sid)? {
                            let a_ts = {
                                let ad = an.try_borrow_data()?;
                                Anchor::read(&ad)?.a
                            };
                            let clock = {
                                let sd = season_ai.try_borrow_data()?;
                                SeasonClock::read(&sd)?
                            };
                            let grace =
                                crate::layout::defence_claim::CLAIM_GRACE_BELLS as i64 * 600;
                            now.ts >= clock.reveal_close(x.bell, a_ts).saturating_add(grace)
                        } else {
                            true
                        }
                    }
                });
    if !case_a {
        return Err(FrontierError::TooEarly.into());
    }
    let bell = live.log_bell(now.ts);
    let mut raw = [0u8; 14];
    raw[..4].copy_from_slice(&pi.to_le_bytes());
    raw[4..8].copy_from_slice(&qi.to_le_bytes());
    raw[8..12].copy_from_slice(&x.bell.to_le_bytes());
    raw[12] = x.faction;
    raw[13] = x.i;
    emit_close_short(
        AccountKind::ArrivalSlot,
        &raw,
        &key(rent_to),
        slot.lamports(),
        bell,
    )?;
    init::close_to(p, slot, rent_to, &Sink::Never, bell)?;
    Ok(())
}

// ------------------------------------------------------------ the return settle

/// SettleDeparture(`transit_slot = 0xFF`), the return settle (§21):
/// `[payer s] [season] [province w] [holding w]` (the Holding that issued
/// the hosts, canonical, possibly absent). Every state-3 `Leave` entry of
/// that Holding in the Province is freed (at most [`RETURN_MAX`] per
/// transaction, in entry order); its whole troops return to
/// `reserve[unit]` when the Holding is live with the host's generation,
/// else they are lost. `AlreadyDone` when there is nothing to return.
pub fn settle_return(p: &Pubkey, a: &[AccountInfo]) -> R<()> {
    let [_payer, season_ai, province, holding] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let now = prologue::now()?;
    let hdr = prologue::season(
        season_ai,
        p,
        Some(&crate::RULESET_HASH),
        &CLASH_STATUS,
        now.ts,
    )?;
    let sid = hdr.id;
    let ctx = crate::addr::ctx(&key(season_ai), &p.to_bytes());
    province_of(p, &ctx, sid, province)?;
    // The holding's (P, Q, site) and live generation, if present.
    let (live, hpqs) = if prologue::presence(holding, p, AccountKind::Holding, sid)? {
        let hd = holding.try_borrow_data()?;
        let r = Ro(&hd);
        let (hp, hq, site) = (r.i16(H::P)?, r.i16(H::Q)?, r.u8(H::SITE)?);
        expect_key(holding, &ctx.holding(hp as i32, hq as i32, site))?;
        let st = r.u8(H::STATE)?;
        let gen = r.u8(H::GEN)?;
        let live = matches!(st, H::STATE_PROVISIONAL | H::STATE_FINAL).then_some(gen);
        (live, Some((hp as i32, hq as i32, site)))
    } else {
        (None, None)
    };
    let bell_log = hdr.bell(now.ts).unwrap_or(NO_BELL);
    let hkey = key(holding);
    // Selection (wave-5 review, G1): one read-only pass over the raw entry
    // bytes (state, pending op, host id) picks the first RETURN_MAX Leave
    // entries of this Holding in entry order; only those are decoded. A
    // host id's issuing Holding is its (province index, site) bits, so a
    // live Holding matches by comparing them; with the Holding absent the
    // owner is matched by address, derived once per run of one foreign
    // (P, Q, site) and never again once this Holding's own bits are known.
    use permutation_rules::frontier::addr::{HOST_GEN_SHIFT, HOST_SITE_SHIFT};
    let mut picked = [(0usize, 0u8); RETURN_MAX];
    let mut n_picked = 0usize;
    {
        use crate::layout::entry as EL;
        let pd = province.try_borrow_data()?;
        let r = Ro(&pd);
        let mut own = match hpqs {
            Some((hp, hq, site)) => Some(
                frontier_abi::addr::host_id(hp, hq, site, 0, 0).ok_or(BAD_ACCOUNT)?
                    >> HOST_SITE_SHIFT,
            ),
            None => None,
        };
        let mut foreign: Option<u64> = None;
        for i in 0..P::ENTRIES_N {
            let o = P::entry(i);
            if r.u8(o + EL::STATE)? != EL::STATE_DEPARTED || r.u8(o + EL::PEND_OP)? != EL::OP_LEAVE
            {
                continue;
            }
            let id = r.u64(o + EL::ID)?;
            let home = id >> HOST_SITE_SHIFT;
            let mine = match own {
                Some(h) => h == home,
                None if foreign == Some(home) => false,
                None => {
                    let t = split_host_id(id).ok_or(BAD_ACCOUNT)?;
                    if ctx.holding(t.province.p, t.province.q, t.site) == hkey {
                        own = Some(home);
                        true
                    } else {
                        foreign = Some(home);
                        false
                    }
                }
            };
            if mine {
                picked[n_picked] = (i, (id >> HOST_GEN_SHIFT) as u8);
                n_picked += 1;
                if n_picked == RETURN_MAX {
                    break;
                }
            }
        }
    }
    let mut returned = 0usize;
    for &(i, gen) in &picked[..n_picked] {
        let e = {
            let pd = province.try_borrow_data()?;
            read_entry(&pd, i).map_err(|_| BAD_ACCOUNT)?
        };
        let credit = live == Some(gen);
        {
            let mut pd = province.try_borrow_mut_data()?;
            write_entry(&mut pd, i, &Entry::FREE).map_err(|_| BAD_ACCOUNT)?;
            let mut w = Rw(&mut pd);
            let n = w.u8(P::N_ENTRIES)?.saturating_sub(1);
            w.set_u8(P::N_ENTRIES, n)?;
        }
        let key8 = e.id.to_le_bytes();
        if credit {
            let whole = e.troops / permutation_rules::fixed::MILLI as u32;
            {
                let mut hd = holding.try_borrow_mut_data()?;
                let mut w = Rw(&mut hd);
                let o = H::reserve(e.unit as usize);
                let v = w.u32(o)?.checked_add(whole).ok_or(OVERFLOW)?;
                w.set_u32(o, v)?;
            }
            let payload = Buf::<7>::new()
                .u32(whole.saturating_mul(permutation_rules::fixed::MILLI as u32))
                .u16(0)
                .u8(RETURNED);
            let mut hd = holding.try_borrow_mut_data()?;
            let mut pd = province.try_borrow_mut_data()?;
            events::emit(
                Kind::DEPARTURE_SETTLED,
                bell_log,
                &key8,
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
            )?;
        } else {
            let mut pd = province.try_borrow_mut_data()?;
            events::emit(
                Kind::STRANDED,
                bell_log,
                &key8,
                &e.troops.to_le_bytes(),
                &mut [Chained {
                    entity: EntityKind::Province,
                    data: &mut pd,
                }],
            )?;
        }
        returned += 1;
        if returned >= RETURN_MAX {
            break;
        }
    }
    if returned == 0 {
        return Err(FrontierError::AlreadyDone.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::model::*;
    use super::*;
    use crate::layout::{camp as CP, entry as E, site as SM};
    use frontier_abi::addr::{holding_key_of_host, host_id};
    use frontier_abi::entry::EntryOp;
    use permutation_rules::frontier::clash::{is_quiet, resolve_clash, Occupancy};
    use permutation_rules::frontier::host::Host;
    use permutation_rules::units::UnitType;

    /// A Province with generated terrain at (P, Q) = (3, -1).
    fn province() -> Vec<u8> {
        let seed = [7u8; 32];
        let c = permutation_rules::frontier::geometry::ProvinceCoord::new(3, -1);
        let t = permutation_rules::frontier::terrain::generate_province(&seed, c);
        let mut d = alloc::vec![0u8; P::SIZE];
        super::super::map::encode_terrain(&t, &mut d).unwrap();
        let mut w = Rw(&mut d);
        w.set_i16(P::P, 3).unwrap();
        w.set_i16(P::Q, -1).unwrap();
        for s in 0..P::SITES_N {
            let o = P::site(s);
            w.set_u8(o + SM::FACTION, 6).unwrap();
            w.set_u32(o + SM::PEND0_BELL, SM::NO_BELL).unwrap();
            w.set_u32(o + SM::PEND1_BELL, SM::NO_BELL).unwrap();
        }
        w.set_u32(P::CAMP + CP::NEXT_CHECK_DAY, 1_000).unwrap();
        d
    }

    fn passable(d: &[u8]) -> Vec<u8> {
        let m = Ro(d).u64(P::PASSABLE_MASK).unwrap();
        (0..61u8).filter(|t| m >> t & 1 == 1).collect()
    }

    fn host(
        pd: &mut [u8],
        i: usize,
        faction: u8,
        seq: u32,
        troops: u32,
        tile: u8,
        state: u8,
    ) -> u64 {
        let id = host_id(3, -1, faction, 1, seq).unwrap();
        let h = Host::muster(
            id,
            holding_key_of_host(id),
            faction,
            UnitType::Spearman,
            troops,
            0,
        )
        .unwrap();
        let e = Entry::from_host(&h, tile, state, 10_000, 1);
        write_entry(pd, i, &e).unwrap();
        id
    }

    #[test]
    fn terrain_round_trips_the_province_encoding() {
        let d = province();
        let t = terrain_of(&d).unwrap();
        let c = permutation_rules::frontier::geometry::ProvinceCoord::new(3, -1);
        assert_eq!(
            t,
            permutation_rules::frontier::terrain::generate_province(&[7u8; 32], c)
        );
    }

    #[test]
    fn occupancy_counts_pending_and_departed_entries() {
        let mut d = province();
        let t = passable(&d)[3];
        host(&mut d, 0, 0, 1, 500_000, t, E::STATE_ROSTER);
        host(&mut d, 1, 2, 2, 500_000, t, E::STATE_MUSTER_PENDING);
        host(&mut d, 2, 2, 3, 500_000, t, E::STATE_MUSTER_PENDING);
        host(&mut d, 7, 1, 4, 500_000, t, E::STATE_DEPARTED);
        let o = occupancy_of(&d).unwrap();
        assert_eq!(o.pending[2], 2);
        assert_eq!(o.storage_free, 52);
        assert_eq!(occupancy_of(&province()).unwrap(), Occupancy::EMPTY);
    }

    #[test]
    fn a_quiet_bell_resolves_to_the_same_bytes_as_a_skip() {
        let mut d = province();
        let tiles = passable(&d);
        host(&mut d, 0, 0, 1, 500_000, tiles[2], E::STATE_ROSTER);
        host(&mut d, 1, 1, 2, 700_000, tiles[9], E::STATE_ROSTER);
        let b = 5;
        let built = build(&d, None, b).unwrap();
        assert!(is_quiet(&kc::frontier_ruleset(), &built.input(&[0; 32])).unwrap());
        let out = resolve_clash(&kc::frontier_ruleset(), &built.input(&[3; 32])).unwrap();
        let mut resolved = d.clone();
        let ap = apply(&mut resolved, &built, &out).unwrap();
        let ch = settle_bell(&mut resolved, b).unwrap();
        finish_bell(&mut resolved, b, ap.changed || ch).unwrap();
        let mut skipped = d.clone();
        let ch = settle_bell(&mut skipped, b).unwrap();
        finish_bell(&mut skipped, b, ch).unwrap();
        assert!(!ap.changed);
        assert_eq!(resolved, skipped);
    }

    #[test]
    fn settle_moves_departures_leaves_and_forfeits() {
        let mut d = province();
        let t = passable(&d)[4];
        let b = 9;
        let dep = host(&mut d, 0, 0, 1, 500_000, t, E::STATE_ROSTER);
        let lv = host(&mut d, 1, 0, 2, 800_000, t, E::STATE_ROSTER);
        let ff = host(&mut d, 2, 0, 3, 800_000, t, E::STATE_ROSTER);
        host(&mut d, 3, 0, 4, 800_000, t, E::STATE_MUSTER_PENDING);
        let mut e = read_entry(&d, 0).unwrap();
        let mut h = e.to_host().unwrap();
        h.depart(b, 10, b).unwrap();
        e.set_host(&h);
        write_entry(&mut d, 0, &e).unwrap();
        for (i, op) in [(1, EntryOp::Leave), (2, EntryOp::Forfeit)] {
            let mut e = read_entry(&d, i).unwrap();
            e.op = op;
            e.pend_bell = b;
            write_entry(&mut d, i, &e).unwrap();
        }
        let mut e3 = read_entry(&d, 3).unwrap();
        e3.from_bell = b + 1;
        write_entry(&mut d, 3, &e3).unwrap();
        assert!(settle_bell(&mut d, b).unwrap());
        assert_eq!(read_entry(&d, 0).unwrap().state, E::STATE_DEPARTED);
        assert_eq!(read_entry(&d, 0).unwrap().id, dep);
        assert_eq!(read_entry(&d, 0).unwrap().op, EntryOp::None);
        let l = read_entry(&d, 1).unwrap();
        assert_eq!(
            (l.id, l.state, l.op),
            (lv, E::STATE_DEPARTED, EntryOp::Leave)
        );
        assert_eq!(
            read_entry(&d, 2).unwrap(),
            Entry::FREE,
            "forfeit {ff} freed"
        );
        assert_eq!(read_entry(&d, 3).unwrap().state, E::STATE_ROSTER);
        // nothing left for the next bell
        assert!(!settle_bell(&mut d, b + 1).unwrap());
    }

    #[test]
    fn the_camp_check_runs_once_a_day_and_spawns_only_with_a_holding() {
        let mut d = province();
        Rw(&mut d).set_u32(P::CAMP + CP::NEXT_CHECK_DAY, 2).unwrap();
        let t = terrain_of(&d).unwrap();
        assert_eq!(camp_check(&d, &t, 2 * 144 - 1).unwrap(), None);
        let (c, spawned) = camp_check(&d, &t, 2 * 144).unwrap().unwrap();
        assert!(!spawned, "no holding: no respawn");
        assert_eq!(c.next_check_day, 3);
        // with a holding the draw spawns about every other day
        Rw(&mut d)
            .set_u8(P::site(0) + SM::STATE, SM::STATE_HOLDING)
            .unwrap();
        let mut n = 0;
        for day in 2..202u32 {
            Rw(&mut d)
                .set_u32(P::CAMP + CP::NEXT_CHECK_DAY, day)
                .unwrap();
            let (c, s) = camp_check(&d, &t, day * 144 + 3).unwrap().unwrap();
            if s {
                n += 1;
                assert!(permutation_rules::frontier::camp::camp_tile_ok(&t, c.tile));
                assert_eq!(c.state, CP::STATE_PRESENT);
            }
        }
        assert!((60..=140).contains(&n), "{n}");
    }

    #[test]
    fn garrison_ids_are_the_holding_keys() {
        let mut d = province();
        for s in [0usize, 3, 11] {
            let o = P::site(s);
            let mut w = Rw(&mut d);
            w.set_u8(o + SM::STATE, SM::STATE_HOLDING).unwrap();
            w.set_u8(o + SM::GEN, 200 + s as u8).unwrap();
            w.set_u8(P::SITE_COUNT, 12).unwrap();
        }
        let b = build(&d, None, 1).unwrap();
        assert_eq!(b.garrisons.len(), 3);
        for (g, &s) in b.garrisons.iter().zip(&b.gar_site) {
            let id = host_id(3, -1, s, 200 + s, 0).unwrap();
            assert_eq!(g.id, holding_key_of_host(id));
            assert_eq!(
                g.id,
                holding_key_of_host(host_id(3, -1, s, 200 + s, 77).unwrap())
            );
        }
    }

    #[test]
    fn outcome_digest_is_the_kernels() {
        let mut d = province();
        let tiles = passable(&d);
        for n in 0..12u32 {
            host(
                &mut d,
                n as usize,
                (n % 6) as u8,
                n + 1,
                500_000 + n * 7_000,
                tiles[(n % 3) as usize],
                E::STATE_ROSTER,
            );
        }
        Rw(&mut d)
            .set_u8(P::site(1) + SM::STATE, SM::STATE_HOLDING)
            .unwrap();
        Rw(&mut d).set_u8(P::site(1) + SM::FACTION, 2).unwrap();
        Rw(&mut d)
            .set_u32(P::site(1) + SM::GARRISON, 3_000_000)
            .unwrap();
        let built = build(&d, None, 4).unwrap();
        let out = resolve_clash(&kc::frontier_ruleset(), &built.input(&[9; 32])).unwrap();
        assert!(out.engagements > 0);
        assert_eq!(outcome_digest(&out).unwrap(), out.digest());
        // residents come sorted by id, the entry order kept aside
        assert!(built.residents.windows(2).all(|w| w[0].id < w[1].id));
    }

    /// Random single-faction-per-hex rosters (with garrisons and a camp):
    /// whenever `trivially_quiet` says so, the kernel's `is_quiet` agrees.
    #[test]
    fn trivially_quiet_rosters_are_quiet() {
        let mut rng = 0x5eed_u64;
        let mut next = |n: u64| {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng % n
        };
        let (mut yes, mut no) = (0, 0);
        for round in 0..300 {
            let mut d = province();
            let tiles = passable(&d);
            let n = 1 + next(48) as usize;
            for i in 0..n {
                let f = next(6) as u8;
                let t = tiles[(f as usize * 3 + next(if round % 3 == 0 { 3 } else { 9 }) as usize)
                    % tiles.len()];
                host(
                    &mut d,
                    i,
                    f,
                    i as u32 + 1,
                    100_000 + next(29_000) as u32 * 1_000,
                    t,
                    E::STATE_ROSTER,
                );
            }
            if round % 2 == 0 {
                let mut w = Rw(&mut d);
                w.set_u8(P::site(0) + SM::STATE, SM::STATE_HOLDING).unwrap();
                w.set_u8(P::site(0) + SM::FACTION, next(6) as u8).unwrap();
                w.set_u32(P::site(0) + SM::GARRISON, 1_000_000).unwrap();
            }
            if round % 5 == 0 {
                let mut w = Rw(&mut d);
                w.set_u8(P::CAMP + CP::STATE, CP::STATE_PRESENT).unwrap();
                w.set_u8(P::CAMP + CP::TILE, tiles[next(tiles.len() as u64) as usize])
                    .unwrap();
                w.set_u32(P::CAMP + CP::TROOPS, 200).unwrap();
            }
            let built = build(&d, None, 3).unwrap();
            let kq = is_quiet(&kc::frontier_ruleset(), &built.input(&[0; 32]));
            if trivially_quiet(&d, 3).unwrap() {
                yes += 1;
                assert_eq!(kq, Ok(true), "round {round}");
            } else {
                no += 1;
            }
        }
        assert!(yes > 30 && no > 30, "{yes} trivially quiet, {no} not");
    }

    #[test]
    fn fast_addresses_are_the_abis() {
        let ctx = AddrCtx {
            season: [3; 32],
            program: [9; 32],
        };
        for (p, q, bell, f, i) in [
            (0i16, 0i16, 0u32, 0u8, 0u8),
            (-3, 7, 1_007, 5, 3),
            (i16::MIN, i16::MAX, u32::MAX, 5, 3),
        ] {
            assert_eq!(
                slot_addr(&ctx, p, q, bell, f, i),
                ctx.arrival_slot(p as i32, q as i32, bell, f, i)
            );
            assert_eq!(
                holding_addr(&ctx, p as i32, q as i32, f + 6),
                ctx.holding(p as i32, q as i32, f + 6)
            );
        }
    }

    #[test]
    fn arrival_stamina_refills_during_the_march() {
        assert_eq!(arrival_stamina(40, 10, 12), 41);
        assert_eq!(arrival_stamina(119, 10, 30), 120);
    }

    #[test]
    fn gather_ranges() {
        assert_eq!(range_mask(0, 24).unwrap(), 0x00FF_FFFF);
        assert_eq!(range_mask(8, 8).unwrap(), 0x0000_FF00);
        assert!(range_mask(20, 5).is_err());
        assert!(range_mask(0, 0).is_err());
    }
}
