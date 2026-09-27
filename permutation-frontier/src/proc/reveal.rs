//! Reveal (0x51, M1 contract §5.11): class W, top-level only, no player
//! signer. Implemented by W3-B.
//!
//! Accounts: `[0 fee_payer s,w] [1 season r] [2 holding r] [3 anchor r]
//! [4 archive r] [5 beaconlog r] [6 inputs r] [7 dest_province r]
//! [8 arrivalday r|w] [9..12 slot0..slot3 (exactly the target writable)]
//! [13.. path provinces other than the destination, 0–3, first-entered
//! order] [ix sysvar r] [system r]`. Data: `transit_slot, target_i,
//! plain [37], salt [32], ct_hash [32], beneficiary [32]`.
//!
//! Checks in the contract's order, cheap first: (1) season Running, or
//! Ended with `arrive < end_bell`, ruleset, top level; (2) holding and
//! transit, `Plain::validate` (`BadPlaintext`); (3) the commitment against
//! `seal_root` (`CommitMismatch`); (4) region and window (`WrongRegion`,
//! `WindowClosed`, `Archived`); (5) the latch (`LatchClosed`); (6) the
//! path (`Path`, `ArrivalBell`, `Shielded`); (7) the quota
//! (`QuotaRefused`, `AlreadyDone`, `SlotMoved`); (8) the ArrivalDay
//! (`NeedArrivalDay`); (9) the fee evidence. **Nothing but the slot and
//! the ArrivalDay is written** (DESIGN §6.2).
//!
//! ## Pinned here (recorded in `W3-B-NOTES.md`)
//!
//! - **Path walk:** from the transit's origin tile, each step moves the
//!   tile offset by `hex::DIRECTIONS[dir]`; the province is re-located
//!   only when the offset leaves the radius-4 hexagon. The supplied path
//!   provinces must be exactly the distinct non-destination provinces the
//!   steps enter, in first-entered order (an extra, missing or reordered
//!   province is `Path`); `path_len = 0` is `Path`. Passability and the
//!   step cost come from the Province's `passable_mask`, `rough_mask` and
//!   `road_mask` (bit = tile index): 120 s flat, 180 s rough, ×0.5 on a
//!   road, ×0.5 mounted (`travel::hex_secs`), then the doctrine's travel
//!   bias, then `travel::check_arrival_bell`.
//! - **Shield:** the destination tile is a site whose mirror shows another
//!   faction's holding with `shield_until_bell > arrive`; or the host's own
//!   holding is shielded at `bell_start(arrive)` (not dormant) and the
//!   destination tile is another faction's holding site.
//! - **Region mismatch:** an anchor, archive or BeaconLog account that is
//!   a present account of its kind at another address is `WrongRegion`;
//!   any other wrong key `BadAddress`.
//! - **Writability:** `target_i < 4` (`BadData`) and exactly slot
//!   `target_i` writable (`BadAccount` otherwise), checked with the
//!   account list.
//! - **Slot fields:** `dealt_bps` = the faction doctrine in the sealed
//!   stance, arriving; `flags` bit 2 (`created_day`) when this Reveal wrote
//!   the ArrivalDay (its bit was clear), cleared by a displacement.

use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use frontier_abi::addr::{archive_part_of, arrival_day_seed, arrival_slot_seed, day_of};
use frontier_abi::ix as aix;
use frontier_abi::layout::AccountKind;
use frontier_abi::log::Kind;
use frontier_abi::tags::Ix;
use permutation_rules::fixed::BPS_ONE;
use permutation_rules::frontier::clash::{FactionSlots, SlotDecision, SlotEntry, SlotRefusal};
use permutation_rules::frontier::doctrine::of_faction;
use permutation_rules::frontier::geometry::{region_of, ProvinceCoord};
use permutation_rules::frontier::seal::{self as ks, Plain};
use permutation_rules::frontier::stance::{Posture, Stance};
use permutation_rules::frontier::travel::{
    check_arrival_bell, is_cavalry, CAVALRY_BPS, FLAT_HEX_SECS, MAX_PATH_STEPS, ROAD_BPS,
    ROUGH_HEX_SECS,
};
use permutation_rules::hex::DIRECTIONS;

use crate::clock::SeasonClock;
use crate::error::{BAD_ACCOUNT, OVERFLOW};
use crate::evidence;
use crate::init::{self, SeasonSigner};
use crate::layout::beacon::{archive_key, archive_tombstoned};
use crate::layout::{
    arrival_day as AD, arrival_slot as AS, beacon_log as BL, bell_anchor as BA, holding as H,
    init_header, province as P, season as S, site as SM, transit as T, Ro, Rw,
};
use crate::prologue::{self, expect_key, key};
use crate::{FrontierError, R};

/// Fixed accounts before the path provinces.
const FIXED: usize = 13;
const HOLDING: usize = 2;
const ANCHOR: usize = 3;
const ARCHIVE: usize = 4;
const BEACONLOG: usize = 5;
const INPUTS: usize = 6;
const DEST: usize = 7;
const DAY: usize = 8;
const SLOT0: usize = 9;

/// The key must be `want`; a present account of `kind` elsewhere is
/// `WrongRegion` (another region's anchor, archive or log), anything else
/// `BadAddress`.
fn region_key(
    program: &Pubkey,
    ai: &AccountInfo,
    want: &[u8; 32],
    kind: AccountKind,
    season_id: u64,
) -> R<()> {
    if ai.key.as_array() == want {
        return Ok(());
    }
    match prologue::presence(ai, program, kind, season_id) {
        Ok(true) => Err(FrontierError::WrongRegion.into()),
        _ => Err(FrontierError::BadAddress.into()),
    }
}

// ------------------------------------------------------------ plaintext

/// Bits 0, 3, 6, …, 93: the first bit of every path step.
const STARTS_96: u128 = {
    let mut m = 0u128;
    let mut i = 0;
    while i < 32 {
        m |= 1u128 << (3 * i);
        i += 1;
    }
    m
};

/// `seal::validate(plain, host_id, arrive).is_ok()` with the path checks
/// done on the 96-bit word at once (the kernel loops over bytes and steps;
/// host test `plain_ok_is_validate`).
fn plain_ok(pt: &Plain, host_id: u64, arrive: u32) -> bool {
    let n = pt.path_len as u32;
    if pt.version != ks::PLAIN_VERSION
        || pt.reserved != [0; ks::RESERVED_BYTES]
        || pt.host_id != host_id
        || pt.arrive_bell != arrive
        || n as usize > MAX_PATH_STEPS
        || pt.dest_tile as usize >= permutation_rules::frontier::geometry::PROVINCE_TILES
        || pt.stance > ks::STANCE_MAX
        || pt.retreat_bps > ks::RETREAT_MAX_BPS
    {
        return false;
    }
    let mut b16 = [0u8; 16];
    b16[..ks::PATH_BYTES].copy_from_slice(&pt.path);
    let x = u128::from_le_bytes(b16);
    let used = 3 * n;
    let live = if used >= 96 {
        (1u128 << 96) - 1
    } else {
        (1u128 << used) - 1
    };
    if x & !live != 0 {
        return false;
    }
    // A step's direction is 6 or 7 iff its two high bits are set.
    (x >> 1) & (x >> 2) & (STARTS_96 & live) == 0
}

// ------------------------------------------------------------ addresses

const fn tag(kind: AccountKind) -> [u8; 2] {
    match frontier_abi::addr::tag_of(kind) {
        Some(t) => t,
        None => [0, 0],
    }
}

const fn nib(v: u8) -> u8 {
    if v < 10 {
        b'0' + v
    } else {
        b'a' - 10 + v
    }
}

/// The with-seed addresses Reveal recomputes (§4.1), each hashed from one
/// stack buffer `season ‖ tag ‖ hex(raw) ‖ program` — the grammar of
/// `frontier_abi::addr` (host test `addresses_are_the_abis`), without its
/// per-address formatting overhead (≈ 14 addresses per Reveal).
struct Addrs {
    season: [u8; 32],
    program: [u8; 32],
}

impl Addrs {
    fn buf(&self, tag: [u8; 2], raw: &[u8]) -> ([u8; 96], usize) {
        let mut b = [0u8; 96];
        b[..32].copy_from_slice(&self.season);
        b[32] = tag[0];
        b[33] = tag[1];
        let mut n = 34;
        for &x in raw.iter().take(frontier_abi::addr::MAX_RAW) {
            b[n] = nib(x >> 4);
            b[n + 1] = nib(x & 15);
            n += 2;
        }
        b[n..n + 32].copy_from_slice(&self.program);
        (b, n + 32)
    }
    fn of(&self, kind: AccountKind, raw: &[u8]) -> [u8; 32] {
        let (b, n) = self.buf(tag(kind), raw);
        permutation_rules::hash::sha256(&[&b[..n]])
    }
    fn pq(p: i32, q: i32) -> [u8; 8] {
        let mut r = [0u8; 8];
        r[..4].copy_from_slice(&p.to_le_bytes());
        r[4..].copy_from_slice(&q.to_le_bytes());
        r
    }
    fn holding(&self, p: i32, q: i32, site: u8) -> [u8; 32] {
        let mut r = [0u8; 9];
        r[..8].copy_from_slice(&Self::pq(p, q));
        r[8] = site;
        self.of(AccountKind::Holding, &r)
    }
    fn province(&self, p: i32, q: i32) -> [u8; 32] {
        self.of(AccountKind::Province, &Self::pq(p, q))
    }
    fn pqb(p: i32, q: i32, b: u32) -> [u8; 12] {
        let mut r = [0u8; 12];
        r[..8].copy_from_slice(&Self::pq(p, q));
        r[8..].copy_from_slice(&b.to_le_bytes());
        r
    }
    fn clash_inputs(&self, p: i32, q: i32, bell: u32) -> [u8; 32] {
        self.of(AccountKind::ClashInputs, &Self::pqb(p, q, bell))
    }
    fn arrival_day(&self, p: i32, q: i32, day: u32) -> [u8; 32] {
        self.of(AccountKind::ArrivalDay, &Self::pqb(p, q, day))
    }
    fn bell_anchor(&self, bell: u32, region: u8) -> [u8; 32] {
        let mut r = [0u8; 5];
        r[..4].copy_from_slice(&bell.to_le_bytes());
        r[4] = region;
        self.of(AccountKind::BellAnchor, &r)
    }
    fn anchor_archive(&self, region: u8, part: u32) -> [u8; 32] {
        let mut r = [0u8; 5];
        r[0] = region;
        r[1..].copy_from_slice(&part.to_le_bytes());
        self.of(AccountKind::AnchorArchive, &r)
    }
    fn beacon_log(&self, region: u8) -> [u8; 32] {
        self.of(AccountKind::BeaconLog, &[region])
    }
    /// The four ArrivalSlots of `(p, q, bell, faction)`: one buffer, the
    /// last hex pair (`i`) patched.
    fn slots(&self, p: i32, q: i32, bell: u32, faction: u8) -> [[u8; 32]; 4] {
        let mut r = [0u8; 14];
        r[..8].copy_from_slice(&Self::pq(p, q));
        r[8..12].copy_from_slice(&bell.to_le_bytes());
        r[12] = faction;
        let (mut b, n) = self.buf(tag(AccountKind::ArrivalSlot), &r);
        let at = n - 32 - 2;
        let mut out = [[0u8; 32]; 4];
        for (i, o) in out.iter_mut().enumerate() {
            b[at] = nib((i as u8) >> 4);
            b[at + 1] = nib(i as u8 & 15);
            *o = permutation_rules::hash::sha256(&[&b[..n]]);
        }
        out
    }
}

/// What the flag check sees of one account.
#[derive(Clone, Copy)]
struct Flags<'a> {
    signer: bool,
    writable: bool,
    key: &'a [u8; 32],
}

/// Reveal's account list checked as `frontier_abi::prologue::check_flags`
/// checks `accounts_of(Ix::Reveal)` — the same codes in the same position
/// order (`TooManyAccounts`, `Auth`, `BadAccount`) — without the generic
/// tables' allocations (≈ 2k CU of a 26k budget; host test
/// `flags_are_check_flags`).
fn base_flags(a: &[Flags]) -> Result<(), FrontierError> {
    let n = a.len();
    if !(FIXED + 2..=FIXED + 3 + 2).contains(&n) {
        return Err(FrontierError::TooManyAccounts);
    }
    for (i, f) in a.iter().enumerate() {
        if i == 0 {
            if !f.signer {
                return Err(FrontierError::Auth);
            }
            if !f.writable {
                return Err(FrontierError::BadAccount);
            }
        } else if i == n - 2 {
            if *f.key != frontier_abi::prologue::ids::INSTRUCTIONS_SYSVAR {
                return Err(FrontierError::BadAccount);
            }
        } else if i == n - 1 {
            if *f.key != frontier_abi::prologue::ids::SYSTEM_PROGRAM {
                return Err(FrontierError::BadAccount);
            }
        } else if f.writable && !(DAY..SLOT0 + 4).contains(&i) {
            // a program account listed read-only
            return Err(FrontierError::BadAccount);
        }
    }
    Ok(())
}

/// One province a path step may enter: masks read from its account.
#[derive(Clone, Copy)]
struct Land {
    p: i32,
    q: i32,
    passable: u64,
    rough: u64,
    road: u64,
}

/// Reads a supplied Province: canonical from its stored key, present.
fn land_of(program: &Pubkey, ad: &Addrs, sid: u64, ai: &AccountInfo) -> R<Land> {
    prologue::present(ai, program, AccountKind::Province, sid)?;
    let d = ai.try_borrow_data()?;
    let r = Ro(&d);
    let (p, q) = (r.i16(P::P)? as i32, r.i16(P::Q)? as i32);
    expect_key(ai, &ad.province(p, q))?;
    Ok(Land {
        p,
        q,
        passable: r.u64(P::PASSABLE_MASK)?,
        rough: r.u64(P::ROUGH_MASK)?,
        road: r.u64(P::ROAD_MASK)?,
    })
}

/// The step costs of `walk` are `travel::hex_secs` exactly.
const _: () = assert!(
    ROAD_BPS as u64 * 2 == BPS_ONE as u64
        && CAVALRY_BPS as u64 * 2 == BPS_ONE as u64
        && FLAT_HEX_SECS % 4 == 0
        && ROUGH_HEX_SECS % 4 == 0
);

// ------------------------------------------------------------ the step table

/// Province radius in tiles (4: 61 tiles).
const RADIUS: i32 = permutation_rules::frontier::geometry::PROVINCE_RADIUS;

/// `geometry::tile_offset` as a const fn (tiles in `hexes_within(4)` order,
/// sorted by q then r).
const fn tile_offset_c(idx: u8) -> (i32, i32) {
    let mut i = idx as i32;
    let mut q = -RADIUS;
    while q <= RADIUS {
        let r_min = if -RADIUS > -q - RADIUS {
            -RADIUS
        } else {
            -q - RADIUS
        };
        let r_max = if RADIUS < -q + RADIUS {
            RADIUS
        } else {
            -q + RADIUS
        };
        let n = r_max - r_min + 1;
        if i < n {
            return (q, r_min + i);
        }
        i -= n;
        q += 1;
    }
    (i32::MIN, i32::MIN)
}

/// `geometry::tile_index` as a const fn (−1 outside the hexagon).
const fn tile_index_c(q: i32, r: i32) -> i32 {
    let s = -q - r;
    let aq = if q < 0 { -q } else { q };
    let ar = if r < 0 { -r } else { r };
    let as_ = if s < 0 { -s } else { s };
    if (aq + ar + as_) / 2 > RADIUS {
        return -1;
    }
    let mut base = 0;
    let mut x = -RADIUS;
    while x < q {
        let r_min = if -RADIUS > -x - RADIUS {
            -RADIUS
        } else {
            -x - RADIUS
        };
        let r_max = if RADIUS < -x + RADIUS {
            RADIUS
        } else {
            -x + RADIUS
        };
        base += r_max - r_min + 1;
        x += 1;
    }
    let r_min = if -RADIUS > -q - RADIUS {
        -RADIUS
    } else {
        -q - RADIUS
    };
    base + r - r_min
}

/// `geometry::cell_of(h, 4)` as a const fn: the province lattice cell of a
/// hex (the nearest of the 3 × 3 lattice points around its real lattice
/// coordinates).
const fn cell_of_c(hq: i32, hr: i32) -> (i32, i32) {
    let (q, r, n) = (hq as i64, hr as i64, RADIUS as i64);
    let det = 3 * n * n + 3 * n + 1;
    let p0 = ((n + 1) * q - n * r).div_euclid(det);
    let q0 = (n * q + (2 * n + 1) * r).div_euclid(det);
    let mut best = (u64::MAX, 0i64, 0i64);
    let mut dp = -1;
    while dp <= 1 {
        let mut dq = -1;
        while dq <= 1 {
            let (pp, qq) = (p0 + dp, q0 + dq);
            let cq = (2 * n + 1) * pp + n * qq;
            let cr = -n * pp + (n + 1) * qq;
            let (a, b) = (q - cq, r - cr);
            let d = (a.unsigned_abs() + b.unsigned_abs() + (a + b).unsigned_abs()) / 2;
            if d < best.0 {
                best = (d, pp, qq);
            }
            dq += 1;
        }
        dp += 1;
    }
    (best.1 as i32, best.2 as i32)
}

/// A step that the table cannot place (never in a correct table; host
/// test `step_table_is_locate`).
const STEP_BAD: u16 = 0xFFFF;

/// `STEP[tile][dir]`: the tile a step in `hex::DIRECTIONS[dir]` enters
/// (low byte) and, in bits 8–10, `1 + k` when it leaves the province for
/// its neighbour `DIRECTIONS[k]` on the province lattice (0: same
/// province). The province grid is a lattice, so one table serves every
/// province; it equals `geometry::locate` (host test). Cut (a) of the
/// program design §10 (per-province neighbour gates) done as a program
/// constant: no account layout changes.
static STEP: [[u16; 6]; 61] = {
    let mut t = [[STEP_BAD; 6]; 61];
    let mut idx = 0;
    while idx < 61 {
        let (oq, or) = tile_offset_c(idx as u8);
        let mut d = 0;
        while d < 6 {
            let (dq, dr) = DIRECTIONS[d];
            let (nq, nr) = (oq + dq, or + dr);
            let inside = tile_index_c(nq, nr);
            if inside >= 0 {
                t[idx][d] = inside as u16;
            } else {
                let (cp, cq) = cell_of_c(nq, nr);
                // centre of lattice cell (cp, cq): cell_centre(Hex(cp, cq), 4)
                let (cx, cy) = (
                    (2 * RADIUS + 1) * cp + RADIUS * cq,
                    -RADIUS * cp + (RADIUS + 1) * cq,
                );
                let tile = tile_index_c(nq - cx, nr - cy);
                let mut k = 0;
                while k < 6 {
                    if DIRECTIONS[k].0 == cp && DIRECTIONS[k].1 == cq && tile >= 0 {
                        t[idx][d] = (((k as u16) + 1) << 8) | tile as u16;
                    }
                    k += 1;
                }
            }
            d += 1;
        }
        idx += 1;
    }
    t
};

/// The walk of §5.11 step 6 (module note): the travel seconds of the path
/// before the doctrine's bias. `lands[0]` is the destination, `lands[1..]`
/// the supplied path provinces in first-entered order. The loop keeps the
/// current province's masks in locals and re-reads them only when a step
/// leaves the province (≈ 30 CU a step).
#[inline(never)]
fn walk(plain: &Plain, origin: (i32, i32, u8), lands: &[Land], cavalry: bool) -> R<u32> {
    let n = plain.path_len as usize;
    let path = || crate::Error::from(FrontierError::Path);
    let dest = lands.first().ok_or_else(path)?;
    if n == 0 || n > MAX_PATH_STEPS || origin.2 as usize >= STEP.len() {
        return Err(path());
    }
    let (mut cp, mut cq) = (origin.0, origin.1);
    let mut tile = origin.2 as usize;
    let mut entered = 0usize;
    let mut secs = 0u32;
    // The current province's (passable, rough, road) masks; re-read after
    // a step leaves a province.
    let (mut pm, mut rm, mut dm) = (0u64, 0u64, 0u64);
    let mut have = false;
    // The 96-bit path as two words: step i is bits 3i..3i+3.
    let mut w = [0u8; 16];
    w[..ks::PATH_BYTES].copy_from_slice(&plain.path);
    let (lo8, hi8) = w.split_at(8);
    let mut lo = u64::from_le_bytes(lo8.try_into().map_err(|_| path())?);
    let mut hi = u64::from_le_bytes(hi8.try_into().map_err(|_| path())?);
    let half = if cavalry { 1 } else { 0 };
    for _ in 0..n {
        let dir = (lo & 7) as usize;
        lo = lo.wrapping_shr(3) | hi.wrapping_shl(61);
        hi = hi.wrapping_shr(3);
        let e = match STEP.get(tile).and_then(|row| row.get(dir)) {
            Some(&e) if e != STEP_BAD => e,
            _ => return Err(path()),
        };
        tile = (e & 0xFF) as usize;
        let k = (e >> 8) as usize;
        if k != 0 {
            // |P|, |Q| ≤ 2¹⁵ + 32: no wrap.
            if let Some((dq, dr)) = DIRECTIONS.get(k.wrapping_sub(1)) {
                cp = cp.wrapping_add(*dq);
                cq = cq.wrapping_add(*dr);
            }
            have = false;
        }
        if !have {
            let l = if (cp, cq) == (dest.p, dest.q) {
                dest
            } else if let Some(l) = lands[1..1 + entered]
                .iter()
                .find(|l| (l.p, l.q) == (cp, cq))
            {
                l
            } else {
                // a newly entered province: the next supplied one
                match lands.get(1 + entered) {
                    Some(l) if (l.p, l.q) == (cp, cq) => {
                        entered += 1;
                        l
                    }
                    _ => return Err(path()),
                }
            };
            pm = l.passable;
            rm = l.rough;
            dm = l.road;
            have = true;
        }
        // tile < 61 (from the table)
        let bit = 1u64.wrapping_shl(tile as u32);
        if pm & bit == 0 {
            return Err(path());
        }
        // `travel::hex_secs`: ×0.5 on a road and ×0.5 mounted are exact
        // halvings of 120 and 180 (const assertion below).
        let st = if rm & bit != 0 {
            ROUGH_HEX_SECS
        } else {
            FLAT_HEX_SECS
        };
        // ≤ 32 × 180 s: no wrap.
        secs = secs.wrapping_add(st.wrapping_shr(half + (dm & bit != 0) as u32));
    }
    if (cp, cq) != (dest.p, dest.q)
        || tile != plain.dest_tile as usize
        || entered + 1 != lands.len()
    {
        return Err(path());
    }
    Ok(secs)
}

// ------------------------------------------------------------ the quota

/// The kernel's `clash::admit_arrival`, with each arrival's slot key
/// computed once (the kernel recomputes `slot_key` — two hashes — at every
/// comparison, ≈ 12 hashes on a full faction). Same decision for every
/// input: host test `admit_is_the_kernels` over random fills; the kernel
/// stays the rule (the verifier and `quota_set` use it).
fn admit(p: ProvinceCoord, bell: u32, slots: &FactionSlots, x: SlotEntry) -> SlotDecision {
    use core::cmp::Reverse;
    if slots.iter().flatten().any(|e| e.host_id == x.host_id) {
        return SlotDecision::Refuse(SlotRefusal::AlreadyIn);
    }
    let seed = permutation_rules::hash::sha256(&[
        b"frontier/slot",
        &p.p.to_le_bytes(),
        &p.q.to_le_bytes(),
        &bell.to_le_bytes(),
    ]);
    let rank = |e: &SlotEntry| {
        (
            Reverse(e.troops),
            permutation_rules::rng::tie_key(&seed, e.host_id),
            e.host_id,
        )
    };
    if let Some((i, e)) = slots
        .iter()
        .enumerate()
        .find_map(|(i, e)| e.filter(|e| e.citizen == x.citizen).map(|e| (i, e)))
    {
        return if rank(&x) < rank(&e) {
            SlotDecision::Displace {
                slot: i as u8,
                displaced: e,
            }
        } else {
            SlotDecision::Refuse(SlotRefusal::CitizenHasLarger)
        };
    }
    if let Some(i) = slots.iter().position(Option::is_none) {
        return SlotDecision::Fill { slot: i as u8 };
    }
    type Low = (usize, SlotEntry, (Reverse<u32>, u64, u64));
    let mut low: Option<Low> = None;
    for (i, e) in slots.iter().enumerate() {
        if let Some(e) = e {
            let r = rank(e);
            if low.as_ref().is_none_or(|l| r >= l.2) {
                low = Some((i, *e, r));
            }
        }
    }
    match low {
        Some((i, e, r)) if rank(&x) < r => SlotDecision::Displace {
            slot: i as u8,
            displaced: e,
        },
        _ => SlotDecision::Refuse(SlotRefusal::Full),
    }
}

/// The site index of `tile` in a Province's site table, if it is a site.
fn site_at(r: &Ro, tile: u8) -> R<Option<usize>> {
    let n = r.u8(P::SITE_COUNT)? as usize;
    let sites: [u8; P::SITES_N] = r.arr(P::SITES)?;
    Ok(sites.iter().take(n).position(|&t| t == tile))
}

/// 0x51 Reveal.
pub fn reveal(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    {
        let mut f = [Flags {
            signer: false,
            writable: false,
            key: &[0u8; 32],
        }; FIXED + 5];
        let n = a.len().min(f.len());
        for (x, ai) in f.iter_mut().zip(a) {
            *x = Flags {
                signer: ai.is_signer,
                writable: ai.is_writable,
                key: ai.key.as_array(),
            };
        }
        if a.len() > f.len() {
            return Err(FrontierError::TooManyAccounts.into());
        }
        base_flags(&f[..n])?;
    }
    let x = aix::Reveal::decode(d)?;
    if x.target_i as usize >= AS::SLOTS_PER_FACTION as usize {
        return Err(FrontierError::BadData.into());
    }
    for i in 0..AS::SLOTS_PER_FACTION as usize {
        if a[SLOT0 + i].is_writable != (i == x.target_i as usize) {
            return Err(BAD_ACCOUNT);
        }
    }
    let n_path = a.len() - FIXED - 2;
    let ix_sysvar = &a[a.len() - 2];
    crate::heap::trace_checkpoint(0x5100);
    // 1. season, ruleset, top level
    prologue::top_level(Ix::Reveal)?;
    let now = prologue::now()?;
    let hdr = prologue::keeper(a, p, &[S::STATUS_RUNNING, S::STATUS_ENDED], now.ts)?;
    let fee_payer = &a[0];
    let season_ai = &a[1];
    let sid = hdr.id;
    let ad = Addrs {
        season: key(season_ai),
        program: p.to_bytes(),
    };
    crate::heap::trace_checkpoint(0x5101);
    // 2. holding, transit, plaintext
    let holding = &a[HOLDING];
    prologue::present(holding, p, AccountKind::Holding, sid)?;
    if x.transit_slot as usize >= H::TRANSIT_N {
        return Err(FrontierError::BadData.into());
    }
    let t0 = H::transit(x.transit_slot as usize);
    let tr = {
        let hd = holding.try_borrow_data()?;
        let r = Ro(&hd);
        let (hp, hq, site) = (r.i16(H::P)?, r.i16(H::Q)?, r.u8(H::SITE)?);
        expect_key(holding, &ad.holding(hp as i32, hq as i32, site))?;
        let t: &[u8; T::SIZE] = hd
            .get(t0..t0 + T::SIZE)
            .and_then(|x| x.try_into().ok())
            .ok_or(BAD_ACCOUNT)?;
        Transit {
            state: t[T::STATE],
            unit: t[T::UNIT],
            faction: t[T::FACTION],
            origin_tile: t[T::ORIGIN_TILE],
            origin_p: i16::from_le_bytes(le::<2>(t, T::ORIGIN_P)),
            origin_q: i16::from_le_bytes(le::<2>(t, T::ORIGIN_Q)),
            host_id: u64::from_le_bytes(le::<8>(t, T::HOST_ID)),
            arrive: u32::from_le_bytes(le::<4>(t, T::ARRIVE_BELL)),
            depart_ts: i64::from_le_bytes(le::<8>(t, T::DEPART_TS)),
            dep_mass: u32::from_le_bytes(le::<4>(t, T::DEP_MASS)),
            seal_root: le::<32>(t, T::SEAL_ROOT),
            citizen_tag: r.u64(H::OWNER_CITIZEN)?,
            shield_until: r.i64(H::SHIELD_UNTIL)?,
            dormant: r.u8(H::FLAGS)? & H::FLAG_DORMANT_CACHE != 0,
        }
    };
    if !T::in_transit(tr.state) {
        return Err(FrontierError::TransitState.into());
    }
    let plain = ks::unpack(&x.plain);
    if !plain_ok(&plain, tr.host_id, tr.arrive) {
        return Err(FrontierError::BadPlaintext.into());
    }
    if hdr.status == S::STATUS_ENDED && tr.arrive >= hdr.end_bell {
        return Err(FrontierError::WrongStatus.into());
    }
    crate::heap::trace_checkpoint(0x5102);
    // 3. commitment
    let commit = ks::commit(&x.plain, &x.salt);
    if ks::seal_root(&commit, &x.ct_hash) != tr.seal_root {
        return Err(FrontierError::CommitMismatch.into());
    }
    crate::heap::trace_checkpoint(0x5103);
    // 4. region and window
    let (dp, dq) = (plain.dest_p as i32, plain.dest_q as i32);
    let dest = ProvinceCoord::new(dp, dq);
    let region = region_of(dest);
    let arrive = tr.arrive;
    let clock = {
        let sd = season_ai.try_borrow_data()?;
        SeasonClock::read(&sd)?
    };
    let anchor = &a[ANCHOR];
    let archive = &a[ARCHIVE];
    let log = &a[BEACONLOG];
    region_key(
        p,
        anchor,
        &ad.bell_anchor(arrive, region),
        AccountKind::BellAnchor,
        sid,
    )?;
    let part = archive_part_of(arrive);
    region_key(
        p,
        archive,
        &ad.anchor_archive(region, part),
        AccountKind::AnchorArchive,
        sid,
    )?;
    region_key(p, log, &ad.beacon_log(region), AccountKind::BeaconLog, sid)?;
    if prologue::presence(anchor, p, AccountKind::BellAnchor, sid)? {
        let a_ts = {
            let d = anchor.try_borrow_data()?;
            let r = Ro(&d);
            if r.u32(BA::BELL)? != arrive || r.u8(BA::REGION)? != region {
                return Err(BAD_ACCOUNT);
            }
            r.i64(BA::A)?
        };
        if now.ts >= clock.reveal_close(arrive, a_ts) {
            return Err(FrontierError::WindowClosed.into());
        }
        prologue::present(log, p, AccountKind::BeaconLog, sid)?;
        let latest = {
            let ld = log.try_borrow_data()?;
            let r = Ro(&ld);
            if r.u8(BL::REGION)? != region {
                return Err(BAD_ACCOUNT);
            }
            r.u64(BL::LATEST_ROUND)?
        };
        if latest >= clock.seed_round(arrive, a_ts) {
            return Err(FrontierError::WindowClosed.into());
        }
    } else if prologue::presence(archive, p, AccountKind::AnchorArchive, sid)? {
        let ad = archive.try_borrow_data()?;
        if archive_key(&ad)? != (region, part) {
            return Err(BAD_ACCOUNT);
        }
        if archive_tombstoned(&ad, arrive)? {
            return Err(FrontierError::Archived.into());
        }
    }
    crate::heap::trace_checkpoint(0x5104);
    // 5. latch
    let inputs = &a[INPUTS];
    expect_key(inputs, &ad.clash_inputs(dp, dq, arrive))?;
    if !init::is_absent(inputs) {
        return Err(FrontierError::LatchClosed.into());
    }
    let dest_ai = &a[DEST];
    expect_key(dest_ai, &ad.province(dp, dq))?;
    prologue::present(dest_ai, p, AccountKind::Province, sid)?;
    crate::heap::trace_checkpoint(0x5105);
    // 6. path (and the destination's latch field)
    let mut lands = [Land {
        p: 0,
        q: 0,
        passable: 0,
        rough: 0,
        road: 0,
    }; 4];
    let (resolved_next, dest_site) = {
        let pd = dest_ai.try_borrow_data()?;
        let r = Ro(&pd);
        if r.i16(P::P)? as i32 != dp || r.i16(P::Q)? as i32 != dq {
            return Err(BAD_ACCOUNT);
        }
        lands[0] = Land {
            p: dp,
            q: dq,
            passable: r.u64(P::PASSABLE_MASK)?,
            rough: r.u64(P::ROUGH_MASK)?,
            road: r.u64(P::ROAD_MASK)?,
        };
        let site = match site_at(&r, plain.dest_tile)? {
            Some(k) => {
                let o = P::site(k);
                Some((
                    r.u8(o + SM::STATE)?,
                    r.u8(o + SM::FACTION)?,
                    r.u32(o + SM::SHIELD_UNTIL_BELL)?,
                ))
            }
            None => None,
        };
        (r.u32(P::RESOLVED_NEXT)?, site)
    };
    if resolved_next > arrive {
        return Err(FrontierError::LatchClosed.into());
    }
    for (k, ai) in a[FIXED..FIXED + n_path].iter().enumerate() {
        lands[1 + k] = land_of(p, &ad, sid, ai)?;
    }
    crate::heap::trace_checkpoint(0x5106);
    let unit = frontier_abi::entry::unit_from_u8(tr.unit).ok_or(BAD_ACCOUNT)?;
    let cav = is_cavalry(unit);
    crate::heap::trace_checkpoint(0x5112);
    let raw = walk(
        &plain,
        (tr.origin_p as i32, tr.origin_q as i32, tr.origin_tile),
        &lands[..1 + n_path],
        cav,
    )?;
    crate::heap::trace_checkpoint(0x5110);
    let doctrine = of_faction(tr.faction).ok_or(BAD_ACCOUNT)?;
    let secs = u32::try_from(doctrine.travel_secs(raw as u64)).map_err(|_| OVERFLOW)?;
    if check_arrival_bell(hdr.genesis_ts, tr.depart_ts, secs, arrive).is_err() {
        return Err(FrontierError::ArrivalBell.into());
    }
    crate::heap::trace_checkpoint(0x5111);
    if let Some((state, owner, shield_bell)) = dest_site {
        let other = state == SM::STATE_HOLDING && owner != tr.faction;
        if other && shield_bell > arrive {
            return Err(FrontierError::Shielded.into());
        }
        let own_shielded = !tr.dormant
            && tr.shield_until
                > permutation_rules::frontier::beacon::bell_start(hdr.genesis_ts, arrive);
        if other && own_shielded {
            return Err(FrontierError::Shielded.into());
        }
    }
    crate::heap::trace_checkpoint(0x5107);
    // 7. quota
    let mut slots: FactionSlots = [None; 4];
    let slot_keys = ad.slots(dp, dq, arrive, tr.faction);
    for (i, s) in slots.iter_mut().enumerate() {
        let ai = &a[SLOT0 + i];
        expect_key(ai, &slot_keys[i])?;
        // Absent (§4.1): System-owned and empty; the common case skips the
        // borrow.
        let absent = ai.data_is_empty()
            && ai.owner.as_array() == &frontier_abi::prologue::ids::SYSTEM_PROGRAM;
        if !absent && prologue::presence(ai, p, AccountKind::ArrivalSlot, sid)? {
            let sd = ai.try_borrow_data()?;
            let r = Ro(&sd);
            if r.i16(AS::P)? as i32 != dp
                || r.i16(AS::Q)? as i32 != dq
                || r.u32(AS::BELL)? != arrive
                || r.u8(AS::FACTION)? != tr.faction
                || r.u8(AS::I)? != i as u8
            {
                return Err(BAD_ACCOUNT);
            }
            *s = Some(SlotEntry {
                host_id: r.u64(AS::HOST_ID)?,
                citizen: r.u64(AS::CITIZEN_TAG)?,
                troops: r.u32(AS::DEP_MASS)?,
            });
        }
    }
    crate::heap::trace_checkpoint(0x5108);
    let me = SlotEntry {
        host_id: tr.host_id,
        citizen: tr.citizen_tag,
        troops: tr.dep_mass,
    };
    let (slot_i, displaced) = match admit(dest, arrive, &slots, me) {
        SlotDecision::Refuse(SlotRefusal::AlreadyIn) => {
            return Err(FrontierError::AlreadyDone.into())
        }
        SlotDecision::Refuse(_) => return Err(FrontierError::QuotaRefused.into()),
        SlotDecision::Fill { slot } => (slot, None),
        SlotDecision::Displace { slot, displaced } => (slot, Some(displaced)),
    };
    if slot_i != x.target_i {
        return Err(FrontierError::SlotMoved.into());
    }
    let slot_ai = &a[SLOT0 + slot_i as usize];
    crate::heap::trace_checkpoint(0x5109);
    // 8. ArrivalDay
    let day_ai = &a[DAY];
    let day = day_of(arrive);
    expect_key(day_ai, &ad.arrival_day(dp, dq, day))?;
    let (bit_at, mask) = AD::bit(arrive);
    let day_absent = day_ai.data_is_empty()
        && day_ai.owner.as_array() == &frontier_abi::prologue::ids::SYSTEM_PROGRAM;
    let day_present = !day_absent && prologue::presence(day_ai, p, AccountKind::ArrivalDay, sid)?;
    let bit_set = if day_present {
        let dd = day_ai.try_borrow_data()?;
        let r = Ro(&dd);
        if r.i16(AD::P)? as i32 != dp || r.i16(AD::Q)? as i32 != dq || r.u32(AD::DAY)? != day {
            return Err(BAD_ACCOUNT);
        }
        r.u8(bit_at)? & mask != 0
    } else {
        false
    };
    if !bit_set && !day_ai.is_writable {
        return Err(FrontierError::NeedArrivalDay.into());
    }
    crate::heap::trace_checkpoint(0x510a);
    // 9. evidence
    let ev = evidence::read(ix_sysvar, now.slot)?;
    crate::heap::trace_checkpoint(0x510b);
    // Effects.
    let signer = SeasonSigner::new(sid, hdr.bump);
    let created_day = !bit_set;
    let new_slot = displaced.is_none();
    // Rent (§4.2, I-49): the fee payer pays each new account's shortfall.
    // When the slot and the day are both new, one System transfer carries
    // both shortfalls to the slot; the day is allocated without a transfer
    // and the slot — the program's once allocated — then pays the day's
    // shortfall by lamport arithmetic: three CPIs instead of four, the
    // payer paying exactly the same. (The move comes after the day's
    // allocation: a program-side move before a CPI that does not list the
    // source would unbalance the CPI, W3-B notes.)
    let rent = {
        use solana_program::sysvar::Sysvar;
        solana_program::rent::Rent::get()?
    };
    let (rent_slot, rent_day) = (
        rent.minimum_balance(AS::SIZE),
        rent.minimum_balance(AD::SIZE),
    );
    let day_short = if day_present {
        0
    } else {
        rent_day.saturating_sub(day_ai.lamports())
    };
    if new_slot {
        if !day_present {
            let ns = rent_slot.saturating_sub(slot_ai.lamports());
            init::transfer(
                fee_payer,
                slot_ai,
                ns.checked_add(day_short).ok_or(OVERFLOW)?,
            )?;
        }
        init::init_with_seed(
            fee_payer,
            slot_ai,
            season_ai,
            &signer,
            &arrival_slot_seed(dp, dq, arrive, tr.faction, slot_i),
            AS::SIZE,
            rent_slot,
            p,
        )?;
        let mut sd = slot_ai.try_borrow_mut_data()?;
        init_header(&mut sd, AccountKind::ArrivalSlot, sid)?;
        let mut w = Rw(&mut sd);
        w.set_i16(AS::P, plain.dest_p)?;
        w.set_i16(AS::Q, plain.dest_q)?;
        w.set_u32(AS::BELL, arrive)?;
        w.set_u8(AS::FACTION, tr.faction)?;
        w.set_u8(AS::I, slot_i)?;
        w.set_arr(AS::RENT_TO, fee_payer.key.as_ref())?;
    }
    crate::heap::trace_checkpoint(0x510c);
    if !day_present {
        // With a new slot the day is allocated at its current lamports
        // (no transfer) and funded from the slot below.
        let total = if new_slot { 0 } else { rent_day };
        init::init_with_seed(
            fee_payer,
            day_ai,
            season_ai,
            &signer,
            &arrival_day_seed(dp, dq, day),
            AD::SIZE,
            total,
            p,
        )?;
        if new_slot {
            init::move_lamports(slot_ai, day_ai, day_short)?;
        }
        let mut dd = day_ai.try_borrow_mut_data()?;
        init_header(&mut dd, AccountKind::ArrivalDay, sid)?;
        let mut w = Rw(&mut dd);
        w.set_i16(AD::P, plain.dest_p)?;
        w.set_i16(AD::Q, plain.dest_q)?;
        w.set_u32(AD::DAY, day)?;
        w.set_arr(AD::RENT_TO, fee_payer.key.as_ref())?;
    }
    if !bit_set {
        let mut dd = day_ai.try_borrow_mut_data()?;
        let mut w = Rw(&mut dd);
        let v = w.u8(bit_at)?;
        w.set_u8(bit_at, v | mask)?;
    }
    crate::heap::trace_checkpoint(0x510d);
    let stance = Stance::from_u8(plain.stance).ok_or(FrontierError::BadPlaintext)?;
    let dealt = super::holding::bps16(doctrine.dealt_bps(Posture::Stance(stance), true))?;
    {
        let mut sd = slot_ai.try_borrow_mut_data()?;
        let mut w = Rw(&mut sd);
        w.set_u8(AS::UNIT, tr.unit)?;
        w.set_u8(AS::STANCE, plain.stance)?;
        w.set_u8(AS::TILE, plain.dest_tile)?;
        w.set_u8(
            AS::FLAGS,
            if created_day && new_slot {
                AS::FLAG_CREATED_DAY
            } else {
                0
            },
        )?;
        w.set_u16(AS::RETREAT_BPS, plain.retreat_bps)?;
        w.set_u64(AS::HOST_ID, tr.host_id)?;
        w.set_u64(AS::CITIZEN_TAG, tr.citizen_tag)?;
        w.set_u32(AS::DEP_MASS, tr.dep_mass)?;
        w.set_u16(AS::DEALT_BPS, dealt)?;
        w.set_arr(AS::BENEFICIARY, &x.beneficiary)?;
        w.set_u64(AS::EV_SLOT, ev.slot)?;
        w.set_u64(AS::EV_PRICE, ev.price)?;
        w.set_u32(AS::EV_LIMIT, ev.limit)?;
        w.set_u32(AS::EV_LOADED, ev.loaded)?;
        w.set_u8(AS::CLAIMED, 0)?;
    }
    crate::heap::trace_checkpoint(0x510e);
    let body = reveal_record(&RevealRec {
        bell: hdr.bell(now.ts).unwrap_or(frontier_abi::log::NO_BELL),
        dp,
        dq,
        arrive,
        faction: tr.faction,
        i: slot_i,
        host_id: tr.host_id,
        tile: plain.dest_tile,
        stance: plain.stance,
        retreat: plain.retreat_bps,
        displaced: displaced.map(|e| e.host_id),
        beneficiary: x.beneficiary,
        ev_slot: ev.slot,
        ev_price: ev.price,
        ev_limit: ev.limit,
        created_day,
    });
    solana_program::log::sol_log_data(&[frontier_abi::log::PREFIX, &body]);
    Ok(())
}

/// The REVEAL record's fields (§6 kind 31).
struct RevealRec {
    bell: u32,
    dp: i32,
    dq: i32,
    arrive: u32,
    faction: u8,
    i: u8,
    host_id: u64,
    tile: u8,
    stance: u8,
    retreat: u16,
    displaced: Option<u64>,
    beneficiary: [u8; 32],
    ev_slot: u64,
    ev_price: u64,
    ev_limit: u32,
    created_day: bool,
}

/// REVEAL body length: head 6, key 14, payload 74, an empty tail (REVEAL
/// chains nothing).
const REVEAL_BODY: usize = 6 + 14 + 74 + 1;

/// The REVEAL body, written directly (host test `reveal_record_is_the_abis`
/// against `events::record`; saves the generic 640-B record path).
fn reveal_record(r: &RevealRec) -> [u8; REVEAL_BODY] {
    let mut b = [0u8; REVEAL_BODY];
    b[0] = frontier_abi::log::VERSION;
    b[1] = Kind::REVEAL as u8;
    b[2..6].copy_from_slice(&r.bell.to_le_bytes());
    b[6..10].copy_from_slice(&r.dp.to_le_bytes());
    b[10..14].copy_from_slice(&r.dq.to_le_bytes());
    b[14..18].copy_from_slice(&r.arrive.to_le_bytes());
    b[18] = r.faction;
    b[19] = r.i;
    b[20..28].copy_from_slice(&r.host_id.to_le_bytes());
    b[28] = r.tile;
    b[29] = r.stance;
    b[30..32].copy_from_slice(&r.retreat.to_le_bytes());
    b[32] = r.displaced.is_some() as u8;
    b[33..41].copy_from_slice(&r.displaced.unwrap_or(0).to_le_bytes());
    b[41..73].copy_from_slice(&r.beneficiary);
    b[73..81].copy_from_slice(&r.ev_slot.to_le_bytes());
    b[81..89].copy_from_slice(&r.ev_price.to_le_bytes());
    b[89..93].copy_from_slice(&r.ev_limit.to_le_bytes());
    b[93] = r.created_day as u8;
    // b[94] = 0: no chain links
    b
}

/// `N` bytes at a fixed offset of a transit record (offsets from the
/// layout, all inside the 96 bytes).
#[inline(always)]
fn le<const N: usize>(t: &[u8; T::SIZE], o: usize) -> [u8; N] {
    let mut b = [0u8; N];
    b.copy_from_slice(&t[o..o + N]);
    b
}

/// The transit fields Reveal reads.
struct Transit {
    state: u8,
    unit: u8,
    faction: u8,
    origin_tile: u8,
    origin_p: i16,
    origin_q: i16,
    host_id: u64,
    arrive: u32,
    depart_ts: i64,
    dep_mass: u32,
    seal_root: [u8; 32],
    citizen_tag: u64,
    shield_until: i64,
    dormant: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use permutation_rules::frontier::geometry::{locate, tile_offset, PROVINCE_TILES};
    use permutation_rules::hex::Hex;

    #[test]
    fn plain_ok_is_validate() {
        let mut x = 0x9_1A7u64;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let mut agree = [0usize; 2];
        for _ in 0..50_000 {
            let n = (next() % 34) as u8;
            let dirs: Vec<u8> = (0..n.min(32)).map(|_| (next() % 6) as u8).collect();
            let (_, mut path) = ks::encode_path(&dirs).unwrap_or((0, [0; 12]));
            let mut pt = Plain {
                version: 1,
                host_id: 5,
                arrive_bell: 9,
                dest_p: 1,
                dest_q: 2,
                dest_tile: (next() % 62) as u8,
                stance: (next() % 5) as u8,
                retreat_bps: [0, 60_000, 60_001, 1][(next() % 4) as usize],
                path_len: n,
                path,
                reserved: [0; 3],
            };
            match next() % 8 {
                0 => {
                    let b = (next() % 12) as usize;
                    path[b] ^= 1 << (next() % 8);
                    pt.path = path;
                }
                1 => pt.version = 2,
                2 => pt.reserved[1] = 1,
                3 => pt.host_id = 6,
                4 => pt.arrive_bell = 8,
                _ => {}
            }
            let want = ks::validate(&pt, 5, 9).is_ok();
            assert_eq!(plain_ok(&pt, 5, 9), want, "{pt:?}");
            agree[want as usize] += 1;
        }
        assert!(agree[0] > 1_000 && agree[1] > 1_000, "{agree:?}");
    }

    #[test]
    fn reveal_record_is_the_abis() {
        let r = RevealRec {
            bell: 77,
            dp: -3,
            dq: 5,
            arrive: 80,
            faction: 4,
            i: 2,
            host_id: 0x1122_3344_5566_7788,
            tile: 30,
            stance: 3,
            retreat: 25_000,
            displaced: Some(99),
            beneficiary: [8; 32],
            ev_slot: 12_345,
            ev_price: 7,
            ev_limit: 26_000,
            created_day: true,
        };
        let key = crate::events::Buf::<14>::new()
            .i32(-3)
            .i32(5)
            .u32(80)
            .u8(4)
            .u8(2);
        let pay = crate::events::Buf::<74>::new()
            .u64(r.host_id)
            .u8(30)
            .u8(3)
            .u16(25_000)
            .u8(1)
            .u64(99)
            .bytes(&[8; 32])
            .u64(12_345)
            .u64(7)
            .u32(26_000)
            .u8(1);
        let mut out = [0u8; crate::events::MAX_RECORD];
        let n = crate::events::record(
            Kind::REVEAL,
            77,
            key.get().unwrap(),
            pay.get().unwrap(),
            &mut [],
            &mut out,
        )
        .unwrap();
        assert_eq!(&reveal_record(&r)[..], &out[..n]);
        let dec = frontier_abi::log::decode(&out[..n]).unwrap();
        assert_eq!(dec.kind, Kind::REVEAL);
        assert_eq!(dec.n_links, 0);
    }

    #[test]
    fn addresses_are_the_abis() {
        let ad = Addrs {
            season: [7; 32],
            program: [9; 32],
        };
        let ctx = crate::addr::ctx(&ad.season, &ad.program);
        for (p, q, b, f, r) in [
            (0, 0, 0u32, 0u8, 0u8),
            (-3, 5, 1_007, 5, 15),
            (i16::MAX as i32, i16::MIN as i32, u32::MAX, 7, 255),
        ] {
            assert_eq!(ad.holding(p, q, f), ctx.holding(p, q, f));
            assert_eq!(ad.province(p, q), ctx.province(p, q));
            assert_eq!(ad.clash_inputs(p, q, b), ctx.clash_inputs(p, q, b));
            assert_eq!(ad.arrival_day(p, q, b), ctx.arrival_day(p, q, b));
            assert_eq!(ad.bell_anchor(b, r), ctx.bell_anchor(b, r));
            assert_eq!(ad.anchor_archive(r, b), ctx.anchor_archive(r, b));
            assert_eq!(ad.beacon_log(r), ctx.beacon_log(r));
            let s = ad.slots(p, q, b, f);
            for (i, k) in s.iter().enumerate() {
                assert_eq!(*k, ctx.arrival_slot(p, q, b, f, i as u8));
            }
        }
    }

    #[test]
    fn admit_is_the_kernels() {
        use permutation_rules::frontier::clash::admit_arrival;
        let mut x = 0xAD_u64;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        for _ in 0..20_000 {
            let p = ProvinceCoord::new((next() % 21) as i32 - 10, (next() % 21) as i32 - 10);
            let bell = (next() % 5_000) as u32;
            let mut slots: FactionSlots = [None; 4];
            for s in slots.iter_mut() {
                if next() % 5 != 0 {
                    *s = Some(SlotEntry {
                        host_id: next() % 64,
                        citizen: next() % 6,
                        troops: [100_000, 200_000, 300_000][(next() % 3) as usize],
                    });
                }
            }
            // distinct hosts in the slots (as Reveal keeps them)
            let mut seen = vec![];
            for s in slots.iter_mut() {
                if let Some(e) = s {
                    if seen.contains(&e.host_id) {
                        *s = None;
                    } else {
                        seen.push(e.host_id);
                    }
                }
            }
            let me = SlotEntry {
                host_id: next() % 64,
                citizen: next() % 6,
                troops: [100_000, 200_000, 300_000][(next() % 3) as usize],
            };
            assert_eq!(
                admit(p, bell, &slots, me),
                admit_arrival(p, bell, &slots, me)
            );
        }
    }

    #[test]
    fn flags_are_check_flags() {
        use frontier_abi::prologue::{self as ap, ids, AccountView};
        let mut x = 0x5151_u64;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let owner = [0u8; 32];
        let other = [3u8; 32];
        for _ in 0..4_000 {
            let n = 14 + (next() % 6) as usize; // 14..19: two refused counts
            let bits = next();
            let keys: Vec<[u8; 32]> = (0..n)
                .map(|i| {
                    let good = next() % 8 != 0;
                    if i == n - 2 && good {
                        ids::INSTRUCTIONS_SYSVAR
                    } else if i == n - 1 && good {
                        ids::SYSTEM_PROGRAM
                    } else {
                        other
                    }
                })
                .collect();
            let fl: Vec<Flags> = (0..n)
                .map(|i| Flags {
                    signer: bits >> (2 * i) & 1 == 1 || (i == 0 && next() % 4 != 0),
                    writable: bits >> (2 * i + 1) & 1 == 1 && next() % 3 == 0
                        || (i == 0 && next() % 4 != 0),
                    key: &keys[i],
                })
                .collect();
            let views: Vec<AccountView> = fl
                .iter()
                .map(|f| AccountView {
                    key: f.key,
                    owner: &owner,
                    lamports: 0,
                    data: &[],
                    is_signer: f.signer,
                    is_writable: f.writable,
                })
                .collect();
            let mut c = [0u8; 8];
            let want = match ap::counts_from_len(Ix::Reveal, n, &mut c) {
                None => Err(FrontierError::TooManyAccounts),
                Some(ng) => {
                    let mut specs = vec![
                        ap::Spec {
                            name: "",
                            acc: ap::Acc::Any,
                            signer: false,
                            wr: ap::Wr::R
                        };
                        32
                    ];
                    let m = ap::resolve(Ix::Reveal, &c[..ng], &mut specs).unwrap();
                    ap::check_flags(&specs[..m], &views)
                }
            };
            assert_eq!(base_flags(&fl), want, "n {n}");
        }
    }

    #[test]
    fn step_table_is_locate() {
        for (pp, pq) in [(0, 0), (2, 0), (-3, 5), (7, -7), (-11, -2)] {
            let pc = ProvinceCoord::new(pp, pq);
            for t in 0..61u8 {
                let h = pc.tile(t).unwrap();
                assert_eq!(tile_offset(t).map(|o| (o.q, o.r)), Some(tile_offset_c(t)));
                for (d, (dq, dr)) in DIRECTIONS.iter().enumerate() {
                    let (np, nt) = locate(Hex::new(h.q + dq, h.r + dr));
                    let e = STEP[t as usize][d];
                    assert_ne!(e, STEP_BAD);
                    let k = (e >> 8) as usize;
                    let (xp, xq) = if k == 0 {
                        (pp, pq)
                    } else {
                        (pp + DIRECTIONS[k - 1].0, pq + DIRECTIONS[k - 1].1)
                    };
                    assert_eq!(
                        (np.p, np.q, nt),
                        (xp, xq, (e & 0xFF) as u8),
                        "{pp},{pq} t{t} d{d}"
                    );
                }
            }
        }
    }

    fn open(p: i32, q: i32) -> Land {
        Land {
            p,
            q,
            passable: (1u64 << PROVINCE_TILES) - 1,
            rough: 0,
            road: 0,
        }
    }

    fn plain_of(dirs: &[u8], dest: (i32, i32), tile: u8) -> Plain {
        let (n, path) = ks::encode_path(dirs).unwrap();
        Plain {
            version: 1,
            host_id: 1,
            arrive_bell: 10,
            dest_p: dest.0 as i16,
            dest_q: dest.1 as i16,
            dest_tile: tile,
            stance: 0,
            retreat_bps: 0,
            path_len: n,
            path,
            reserved: [0; 3],
        }
    }

    /// The walk agrees with `locate` over the global hexes of the path.
    fn native(origin: (i32, i32, u8), dirs: &[u8]) -> Vec<(i32, i32, u8)> {
        let mut h = ProvinceCoord::new(origin.0, origin.1)
            .tile(origin.2)
            .unwrap();
        let mut out = vec![];
        for &d in dirs {
            let (dq, dr) = DIRECTIONS[d as usize];
            h = Hex::new(h.q + dq, h.r + dr);
            let (p, t) = locate(h);
            out.push((p.p, p.q, t));
        }
        out
    }

    fn lands_for(steps: &[(i32, i32, u8)]) -> Vec<Land> {
        let last = steps.last().unwrap();
        let mut v = vec![open(last.0, last.1)];
        for s in steps {
            if (s.0, s.1) != (last.0, last.1) && !v.iter().any(|l| (l.p, l.q) == (s.0, s.1)) {
                v.push(open(s.0, s.1));
            }
        }
        v
    }

    #[test]
    fn a_straight_march_crosses_provinces_like_locate() {
        let origin = (2, 0, 30);
        let dirs = [0u8; 20];
        let steps = native(origin, &dirs);
        let lands = lands_for(&steps);
        assert!(lands.len() >= 3, "20 steps east cross provinces");
        let last = *steps.last().unwrap();
        let plain = plain_of(&dirs, (last.0, last.1), last.2);
        let secs = walk(&plain, origin, &lands, false).unwrap();
        assert_eq!(secs, 20 * FLAT_HEX_SECS);
        assert_eq!(
            walk(&plain, origin, &lands, true).unwrap(),
            20 * FLAT_HEX_SECS / 2
        );
        // a wrong end tile, a missing or extra province
        let mut wrong = plain;
        wrong.dest_tile = (last.2 + 1) % 61;
        assert!(walk(&wrong, origin, &lands, false).is_err());
        assert!(walk(&plain, origin, &lands[..lands.len() - 1], false).is_err());
        let mut extra = lands.clone();
        extra.push(open(40, 40));
        assert!(walk(&plain, origin, &extra, false).is_err());
        if lands.len() >= 3 {
            let mut swapped = lands.clone();
            swapped.swap(1, 2);
            assert!(walk(&plain, origin, &swapped, false).is_err());
        }
    }

    #[test]
    fn impassable_rough_and_empty_paths() {
        let origin = (2, 0, 30);
        let dirs = [1u8, 1, 2];
        let steps = native(origin, &dirs);
        let mut lands = lands_for(&steps);
        let last = *steps.last().unwrap();
        let plain = plain_of(&dirs, (last.0, last.1), last.2);
        for l in lands.iter_mut() {
            l.rough = u64::MAX;
        }
        assert_eq!(
            walk(&plain, origin, &lands, false).unwrap(),
            3 * ROUGH_HEX_SECS
        );
        lands[0].passable &= !(1u64 << last.2);
        assert!(walk(&plain, origin, &lands, false).is_err());
        let empty = plain_of(&[], (2, 0), 30);
        assert!(walk(&empty, origin, &lands_for(&[(2, 0, 30)]), false).is_err());
    }

    #[test]
    fn random_walks_match_locate() {
        let mut x = 0x9E37_79B9u64;
        for _ in 0..500 {
            let n = 1 + (x % 32) as usize;
            let mut dirs = vec![];
            for _ in 0..n {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                dirs.push((x % 6) as u8);
            }
            let origin = (3, -1, (x % 61) as u8);
            let steps = native(origin, &dirs);
            let lands = lands_for(&steps);
            let last = *steps.last().unwrap();
            let plain = plain_of(&dirs, (last.0, last.1), last.2);
            if lands.len() <= 4 {
                let r = walk(&plain, origin, &lands, false);
                assert_eq!(r.unwrap(), n as u32 * FLAT_HEX_SECS);
            }
        }
    }
}
