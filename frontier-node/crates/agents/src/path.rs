//! March paths over the provinces a bot has observed (M1 contract §5.11
//! Reveal check 6): ≤ 32 steps of 3-bit directions (`hex::DIRECTIONS`
//! order) from the host's tile, every step passable, ≤ 4 provinces, the
//! last hex the destination tile. Costs are the kernel's
//! (`travel::hex_secs` constants: flat 120 s, rough 180 s, roads and
//! cavalry halve), read from the Province accounts' masks, so a bot's
//! earliest arrival bell is never earlier than the program's (doctrines
//! only make marches faster: every `travel_bps ≤ 1.0`).

use std::collections::{BTreeMap, BinaryHeap};

use fclient::decode::Province;
use permutation_rules::frontier::geometry::{locate, ProvinceCoord};
use permutation_rules::frontier::travel::{
    self, CAVALRY_BPS, FLAT_HEX_SECS, MAX_PATH_PROVINCES, MAX_PATH_STEPS, ROAD_BPS, ROUGH_HEX_SECS,
};
use permutation_rules::hex::{Hex, DIRECTIONS};

/// A planned march path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Path {
    /// Directions 0–5, one per step.
    pub dirs: Vec<u8>,
    pub secs: u32,
    /// Distinct provinces the steps enter, first-entered order.
    pub provinces: Vec<(i32, i32)>,
}

impl Path {
    /// The 12-byte packed path of the plaintext.
    pub fn packed(&self) -> [u8; 12] {
        fclient::seal::path_of(&self.dirs)
    }

    /// Path provinces other than the destination (Reveal's accounts 13..).
    pub fn others(&self, dest: (i32, i32)) -> Vec<(i32, i32)> {
        self.provinces
            .iter()
            .copied()
            .filter(|&p| p != dest)
            .collect()
    }
}

fn bit(mask: u64, i: u8) -> bool {
    i < 64 && mask >> i & 1 == 1
}

/// Seconds to enter tile `idx` of `pv` for a (non-)mounted unit, or None
/// when impassable (the program's `hex_secs`, from the masks).
pub fn tile_secs(pv: &Province, idx: u8, mounted: bool) -> Option<u32> {
    if !bit(pv.passable_mask, idx) {
        return None;
    }
    let mut s = if bit(pv.rough_mask, idx) {
        ROUGH_HEX_SECS
    } else {
        FLAT_HEX_SECS
    } as u64;
    if bit(pv.road_mask, idx) {
        s = s * ROAD_BPS as u64 / 10_000;
    }
    if mounted {
        s = s * CAVALRY_BPS as u64 / 10_000;
    }
    Some(s as u32)
}

/// Whether unit `unit` (kernel id) marches at cavalry pace.
pub fn mounted(unit: u8) -> bool {
    permutation_rules::frontier::catalog::unit_of(unit).is_some_and(travel::is_cavalry)
}

/// The hex of tile `idx` of province `(p, q)`.
pub fn hex_of(p: i32, q: i32, idx: u8) -> Option<Hex> {
    ProvinceCoord::new(p, q).tile(idx)
}

/// Cheapest path (Dijkstra on seconds, ties by fewer steps then by
/// direction order, so it is deterministic) from tile `from` of `origin`
/// to tile `to` of `dest`, through observed provinces only.
pub fn plan(
    provinces: &BTreeMap<(i16, i16), &Province>,
    origin: (i32, i32),
    from: u8,
    dest: (i32, i32),
    to: u8,
    unit: u8,
) -> Option<Path> {
    let start = hex_of(origin.0, origin.1, from)?;
    let goal = hex_of(dest.0, dest.1, to)?;
    if start == goal {
        return None;
    }
    let cav = mounted(unit);
    // (Reverse(secs), Reverse(steps), hex) with predecessor (hex, dir).
    let mut best: BTreeMap<Hex, (u32, u32)> = BTreeMap::new();
    let mut prev: BTreeMap<Hex, (Hex, u8)> = BTreeMap::new();
    let mut heap = BinaryHeap::new();
    best.insert(start, (0, 0));
    heap.push(std::cmp::Reverse((0u32, 0u32, start)));
    while let Some(std::cmp::Reverse((secs, steps, h))) = heap.pop() {
        if h == goal {
            break;
        }
        if best.get(&h).is_some_and(|&b| b < (secs, steps)) {
            continue;
        }
        if steps as usize >= MAX_PATH_STEPS {
            continue;
        }
        for (d, (dq, dr)) in DIRECTIONS.iter().enumerate() {
            let n = Hex::new(h.q + dq, h.r + dr);
            let (pc, idx) = locate(n);
            let (Ok(pp), Ok(qq)) = (i16::try_from(pc.p), i16::try_from(pc.q)) else {
                continue;
            };
            let Some(pv) = provinces.get(&(pp, qq)) else {
                continue;
            };
            let Some(s) = tile_secs(pv, idx, cav) else {
                continue;
            };
            let cand = (secs + s, steps + 1);
            if best.get(&n).is_none_or(|&b| cand < b) {
                best.insert(n, cand);
                prev.insert(n, (h, d as u8));
                heap.push(std::cmp::Reverse((cand.0, cand.1, n)));
            }
        }
    }
    let &(secs, _) = best.get(&goal)?;
    let mut dirs = vec![];
    let mut at = goal;
    while at != start {
        let (p, d) = *prev.get(&at)?;
        dirs.push(d);
        at = p;
    }
    dirs.reverse();
    let mut provinces_in: Vec<(i32, i32)> = vec![];
    let mut h = start;
    for &d in &dirs {
        let (dq, dr) = DIRECTIONS[d as usize];
        h = Hex::new(h.q + dq, h.r + dr);
        let (pc, _) = locate(h);
        if !provinces_in.contains(&(pc.p, pc.q)) {
            provinces_in.push((pc.p, pc.q));
        }
    }
    if dirs.len() > MAX_PATH_STEPS || provinces_in.len() > MAX_PATH_PROVINCES {
        return None;
    }
    Some(Path {
        dirs,
        secs,
        provinces: provinces_in,
    })
}

/// The earliest arrival bell a march leaving at `depart_ts` over `secs`
/// can name (kernel `travel::earliest_arrival_bell`).
pub fn earliest_arrival(genesis_ts: i64, depart_ts: i64, secs: u32) -> u32 {
    travel::earliest_arrival_bell(genesis_ts, depart_ts, secs)
}

/// Tiles within one hex of `tile` in the same province (Explore's rule).
pub fn adjacent_tiles(tile: u8) -> Vec<u8> {
    use permutation_rules::frontier::geometry::{tile_index, tile_offset};
    let Some(o) = tile_offset(tile) else {
        return vec![];
    };
    DIRECTIONS
        .iter()
        .filter_map(|(dq, dr)| tile_index(Hex::new(o.q + dq, o.r + dr)))
        .collect()
}
