//! Keeper play helpers (W4-C; M1 contract §5.11, §8.2): opening a march's
//! seal, the reveal order and slot index, the path provinces a Reveal
//! names, the gather parts of a province-bell and the rules a SettleTransit
//! write is planned by. Pure functions over the kernel's rules
//! (`permutation_rules::frontier::clash::{admit_arrival, quota_set,
//! slot_key}`, `geometry::locate`), so the keeper predicts exactly what the
//! program will decide and the program still decides.

use core::cmp::Reverse;

use permutation_rules::frontier::clash::{
    admit_arrival, quota_set, slot_key, FactionSlots, SlotDecision, SlotEntry, SlotRefusal,
};
use permutation_rules::frontier::geometry::{locate, tile_offset, ProvinceCoord};
use permutation_rules::hex::{Hex, DIRECTIONS};
use solana_address::Address;

use crate::abi::seal_code;
use crate::seal::{self, Plain, PLAIN_LEN, SEAL_LEN};

// ------------------------------------------------------------------ seals

/// What the keeper learns by opening a march's seal at T(arrive) (§8.2
/// Decrypt): the seal code SettleTransit will judge (0 valid, 1, 2, 4, 5
/// bad) and, for a seal that opens, the plaintext and the salt a Reveal
/// carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Opened {
    pub code: u8,
    pub plain: Option<[u8; PLAIN_LEN]>,
    pub salt: Option<[u8; 32]>,
}

impl Opened {
    pub fn valid(&self) -> bool {
        self.code == seal_code::VALID && self.plain.is_some()
    }
    pub fn plain(&self) -> Option<Plain> {
        self.plain.map(|p| seal::unpack(&p))
    }
}

/// Opens `seal` with the round-T(arrive) signature and judges it as
/// SettleTransit does (`seal::judge`), keeping the salt of the key.
pub fn open_march(
    seal_bytes: &[u8; SEAL_LEN],
    commit_logged: &[u8; 32],
    sig48: &[u8; 48],
    host_id: u64,
    arrive: u32,
) -> Opened {
    let (code, _) = seal::judge(seal_bytes, commit_logged, sig48, host_id, arrive);
    if code != seal_code::VALID {
        return Opened {
            code,
            plain: None,
            salt: None,
        };
    }
    match seal::open(seal_bytes, sig48) {
        Ok((k, pt)) => Opened {
            code,
            plain: Some(pt),
            salt: Some(seal::salt_of(&k)),
        },
        // judge opened it; open cannot fail afterwards.
        Err(_) => Opened {
            code: seal_code::FO_FAILED,
            plain: None,
            salt: None,
        },
    }
}

// ------------------------------------------------------------------ quota

/// The kernel's quota rank of an arrival (more troops first, then the
/// lower slot key, then the lower host id): the order the keeper sends a
/// group's Reveals in, so the first four to land are the final four and
/// no Reveal displaces another.
/// Resident actions at bell `b` need the province resolved through `b − 2`
/// (§5.1): `resolved_next + 1 ≥ b`, the program's rule
/// (`frontier_abi::prologue::resident_ok`; pinned by a test).
pub fn resident_ok(resolved_next: u32, b: u32) -> bool {
    resolved_next as u64 + 1 >= b as u64
}

#[cfg(test)]
mod resident_tests {
    #[test]
    fn resident_ok_is_the_programs() {
        for rn in [0u32, 1, 7, 8, 9, 143, 144, u32::MAX] {
            for b in [0u32, 1, 8, 9, 10, 145, u32::MAX] {
                assert_eq!(
                    super::resident_ok(rn, b),
                    frontier_abi::prologue::resident_ok(rn, b),
                    "{rn} {b}"
                );
            }
        }
    }
}

pub fn rank(p: i32, q: i32, bell: u32, e: &SlotEntry) -> (Reverse<u32>, u64, u64) {
    (
        Reverse(e.troops),
        slot_key(ProvinceCoord::new(p, q), bell, e.host_id),
        e.host_id,
    )
}

/// Sorts arrivals of one `(P, Q, bell, faction)` group in reveal order.
pub fn reveal_order(p: i32, q: i32, bell: u32, v: &mut [SlotEntry]) {
    v.sort_by_key(|e| rank(p, q, bell, e));
}

/// Where a Reveal of `x` goes against the slots as read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Into slot `i` (free).
    Fill(u8),
    /// Over slot `i` (the lowest-ranked, or the citizen's own weaker one).
    Displace(u8, u64),
    /// The host already holds a slot (the Reveal would be `AlreadyDone`).
    Already,
    /// Refused by the quota (`QuotaRefused`): settle it as bounced.
    Refused(SlotRefusal),
}

impl Target {
    pub fn index(&self) -> Option<u8> {
        match *self {
            Target::Fill(i) | Target::Displace(i, _) => Some(i),
            _ => None,
        }
    }
}

/// `admit_arrival` as the program's Reveal step 7 computes it.
pub fn target(p: i32, q: i32, bell: u32, slots: &FactionSlots, x: SlotEntry) -> Target {
    match admit_arrival(ProvinceCoord::new(p, q), bell, slots, x) {
        SlotDecision::Fill { slot } => Target::Fill(slot),
        SlotDecision::Displace { slot, displaced } => Target::Displace(slot, displaced.host_id),
        SlotDecision::Refuse(SlotRefusal::AlreadyIn) => Target::Already,
        SlotDecision::Refuse(r) => Target::Refused(r),
    }
}

/// Whether `x` is in the order-free final set of its group
/// (`quota_set` over the arrivals already in the slots and every known
/// candidate). Arrivals outside it are never revealed by the keeper: they
/// go to the SettleTransit queue (bounced without loss, D5).
pub fn in_final_set(p: i32, q: i32, bell: u32, all: &[SlotEntry], x: &SlotEntry) -> bool {
    quota_set(ProvinceCoord::new(p, q), bell, all)
        .iter()
        .any(|e| e.host_id == x.host_id)
}

// ------------------------------------------------------------------ paths

/// The provinces a Reveal names besides the destination (§5.11 step 6):
/// every province a step of the path enters, in first-entered order, the
/// destination left out (the origin included when a step lands in it).
/// `None` when the path leaves the map's grammar (a direction ≥ 6), ends
/// elsewhere than the plaintext's destination, or enters more than 3
/// provinces besides it (the program answers `Path` for all of them).
pub fn path_provinces(origin: (i32, i32, u8), plain: &Plain) -> Option<Vec<(i32, i32)>> {
    let c = ProvinceCoord::new(origin.0, origin.1).centre();
    let off = tile_offset(origin.2)?;
    let mut h = Hex::new(c.q + off.q, c.r + off.r);
    let dest = (plain.dest_p as i32, plain.dest_q as i32);
    let mut out: Vec<(i32, i32)> = vec![];
    for i in 0..plain.path_len {
        let d = seal::step(&plain.path, i) as usize;
        let (dq, dr) = *DIRECTIONS.get(d)?;
        h = Hex::new(h.q + dq, h.r + dr);
        let (pc, _) = locate(h);
        let k = (pc.p, pc.q);
        if k != dest && !out.contains(&k) {
            out.push(k);
        }
    }
    let (pc, tile) = locate(h);
    if (pc.p, pc.q) != dest || tile != plain.dest_tile || out.len() > 3 {
        return None;
    }
    Some(out)
}

// ------------------------------------------------------------------ gathers

/// Position of slot `(faction, i)` in ClashInputs (6 factions × 4 slots).
pub const fn position(faction: u8, i: u8) -> usize {
    faction as usize * 4 + i as usize
}

/// Positions of a province-bell.
pub const POSITIONS: usize = 24;
/// Holdings per GatherClash at most (§5.11).
pub const GATHER_MAX_HOLDINGS: usize = 10;
/// Slot keys plus distinct Holdings one GatherClash carries within the
/// 1,232-byte packet (10 fixed keys: fee payer, season, province, anchor,
/// ArrivalDay, inputs, the instructions sysvar, System, ComputeBudget and
/// the program; ≈ 33 B per further key).
pub const GATHER_MAX_KEYS: usize = 21;

/// One GatherClash part: positions `start..start + slots.len()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatherPart {
    pub start: u8,
    /// `(faction, i)` of every position of the part, in order.
    pub slots: Vec<(u8, u8)>,
    /// The owner Holding of every present position, in position order.
    pub holdings: Vec<Address>,
    /// Bit `k` set ⇔ **position** `k` (absolute, `start ≤ k < start + n`)
    /// is expected present (its Holding is in `holdings`): contract §5.11
    /// "which positions are expected present", as the program reads it
    /// (integ-W4: bit `j` relative to `start` made every part with
    /// `start > 0` fail `TooManyAccounts` or `BadData` on the program).
    pub bitmap: u32,
}

/// Splits the positions not yet in `mask` into parts: consecutive
/// positions, ≤ 10 Holdings and ≤ [`GATHER_MAX_KEYS`] slot keys plus
/// distinct Holdings each. `present[k]` is the owner Holding of position
/// `k` when its slot is present.
pub fn gather_parts(mask: u32, present: &[Option<Address>; POSITIONS]) -> Vec<GatherPart> {
    let mut parts: Vec<GatherPart> = vec![];
    let mut cur: Option<GatherPart> = None;
    for (k, pres) in present.iter().enumerate() {
        if mask & (1 << k) != 0 {
            if let Some(c) = cur.take() {
                parts.push(c);
            }
            continue;
        }
        let fits = |c: &GatherPart| {
            let mut hs = c.holdings.clone();
            if let Some(h) = pres {
                hs.push(*h);
            }
            let n_h = hs.len();
            hs.sort();
            hs.dedup();
            n_h <= GATHER_MAX_HOLDINGS && c.slots.len() + 1 + hs.len() <= GATHER_MAX_KEYS
        };
        if cur.as_ref().is_some_and(|c| !fits(c)) {
            parts.push(cur.take().expect("some"));
        }
        let c = cur.get_or_insert_with(|| GatherPart {
            start: k as u8,
            slots: vec![],
            holdings: vec![],
            bitmap: 0,
        });
        c.slots.push(((k / 4) as u8, (k % 4) as u8));
        if let Some(h) = pres {
            c.holdings.push(*h);
            c.bitmap |= 1 << k;
        }
    }
    if let Some(c) = cur {
        parts.push(c);
    }
    parts
}

// ------------------------------------------------------------------ settlement

/// The final set a SettleTransit compares a host with (D5): does `x`
/// (not in the resolved records) bounce without loss (outranked by the
/// faction's recorded set, or its citizen holds a higher-ranked arrival)?
pub fn bounces_unranked(p: i32, q: i32, bell: u32, recorded: &[SlotEntry], x: &SlotEntry) -> bool {
    let mut all = recorded.to_vec();
    all.push(*x);
    !in_final_set(p, q, bell, &all, x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(host: u64, citizen: u64, troops: u32) -> SlotEntry {
        SlotEntry {
            host_id: host,
            citizen,
            troops,
        }
    }

    #[test]
    fn reveal_order_fills_without_displacement() {
        // Five arrivals of one faction in random order: revealed in rank
        // order, the first four fill 0..3 and the fifth is refused Full.
        let (p, q, b) = (3, -1, 40);
        let mut v = vec![
            e(11, 1, 5_000),
            e(12, 2, 9_000),
            e(13, 3, 1_000),
            e(14, 4, 7_000),
            e(15, 5, 3_000),
        ];
        reveal_order(p, q, b, &mut v);
        assert_eq!(
            v.iter().map(|x| x.host_id).collect::<Vec<_>>(),
            vec![12, 14, 11, 15, 13]
        );
        let mut slots: FactionSlots = [None; 4];
        let mut fills = 0;
        for x in &v {
            match target(p, q, b, &slots, *x) {
                Target::Fill(i) => {
                    fills += 1;
                    slots[i as usize] = Some(*x);
                }
                Target::Refused(SlotRefusal::Full) => assert_eq!(x.host_id, 13),
                t => panic!("{t:?}"),
            }
        }
        assert_eq!(fills, 4);
        assert!(!in_final_set(p, q, b, &v, &v[4]));
        assert!(in_final_set(p, q, b, &v, &v[0]));
        // A second arrival of citizen 1 (smaller): refused, and bounces.
        let second = e(16, 1, 4_000);
        assert_eq!(
            target(p, q, b, &slots, second),
            Target::Refused(SlotRefusal::CitizenHasLarger)
        );
        assert!(bounces_unranked(p, q, b, &v[..4], &second));
        assert_eq!(target(p, q, b, &slots, v[0]), Target::Already);
    }

    #[test]
    fn gather_parts_respect_the_packet() {
        let h = |i: u8| Address::new_from_array([i; 32]);
        let mut present: [Option<Address>; POSITIONS] = [None; POSITIONS];
        // 12 present positions over 12 holdings.
        for k in 0..12 {
            present[k * 2] = Some(h(k as u8 + 1));
        }
        let parts = gather_parts(0, &present);
        let n: usize = parts.iter().map(|p| p.slots.len()).sum();
        assert_eq!(n, POSITIONS);
        for p in &parts {
            assert!(p.holdings.len() <= GATHER_MAX_HOLDINGS);
            assert!(p.slots.len() + p.holdings.len() <= GATHER_MAX_KEYS);
            assert_eq!(p.bitmap.count_ones() as usize, p.holdings.len());
            // absolute positions, inside the part's range (§5.11)
            let n = p.slots.len() as u32;
            let range = ((1u32 << n) - 1) << p.start;
            assert_eq!(p.bitmap & !range, 0, "part at {}", p.start);
        }
        assert!(parts.iter().any(|p| p.start > 0 && p.bitmap != 0));
        assert!(parts.len() <= 3, "a full province-bell needs ≤ 3 gathers");
        // Positions already gathered are skipped.
        let parts = gather_parts(0x00FF_FFF0, &present);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].start, 0);
        assert_eq!(parts[0].slots.len(), 4);
        // Nothing left.
        assert!(gather_parts(0x00FF_FFFF, &present).is_empty());
    }

    #[test]
    fn path_provinces_follow_locate() {
        // A straight walk east from tile 30 (the centre) of (2, 0).
        let origin = (2, 0, 30u8);
        let c = ProvinceCoord::new(2, 0).centre();
        let off = tile_offset(30).unwrap();
        let mut h = Hex::new(c.q + off.q, c.r + off.r);
        let mut seen = vec![];
        for _ in 0..12 {
            h = Hex::new(h.q + 1, h.r);
            let (pc, _) = locate(h);
            if !seen.contains(&(pc.p, pc.q)) {
                seen.push((pc.p, pc.q));
            }
        }
        let dirs = [0u8; 12];
        let (dest, tile) = locate(h);
        let plain = Plain {
            version: 1,
            host_id: 1,
            arrive_bell: 9,
            dest_p: dest.p as i16,
            dest_q: dest.q as i16,
            dest_tile: tile,
            stance: 0,
            retreat_bps: 0,
            path_len: dirs.len() as u8,
            path: seal::path_of(&dirs),
            ..Default::default()
        };
        let got = path_provinces(origin, &plain).unwrap();
        let want: Vec<_> = seen
            .into_iter()
            .filter(|&k| k != (dest.p, dest.q))
            .collect();
        assert_eq!(got, want);
        // The wrong destination tile.
        let mut bad = plain;
        bad.dest_tile = (tile + 1) % 61;
        assert_eq!(path_provinces(origin, &bad), None);
    }
}
