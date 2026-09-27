//! The composer's path planner and the direction-list pricer. Terrain comes
//! from `terrain::generate_province` with the ring seeds the caller knows;
//! a hex of a ring without a seed is never entered. The price and every
//! limit are `travel::path_cost`'s, so a planned path is one the program
//! accepts (roads are M3: none in M1).

use crate::api::{travel_refusal, PathCostArgs, PathCostOut, PathPlan, PlanArgs, RingSeed};
use permutation_rules::frontier::geometry;
use permutation_rules::frontier::seal;
use permutation_rules::frontier::terrain::{self, ProvinceTerrain};
use permutation_rules::frontier::travel::{self, Step};
use permutation_rules::hex::{Hex, DIRECTIONS};
use permutation_rules::map::Terrain;
use permutation_rules::units::UnitType;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

/// Terrain of hexes, one generated province at a time.
struct Land<'a> {
    seeds: &'a [RingSeed],
    provinces: BTreeMap<(i32, i32), Option<ProvinceTerrain>>,
}

impl<'a> Land<'a> {
    fn new(seeds: &'a [RingSeed]) -> Self {
        Land {
            seeds,
            provinces: BTreeMap::new(),
        }
    }

    /// The terrain of `h`, or `None` for unknown land (no ring seed, or
    /// outside the coordinate bound).
    fn terrain(&mut self, h: Hex) -> Option<Terrain> {
        geometry::check_hex(h).ok()?;
        let (p, idx) = geometry::locate(h);
        let seeds = self.seeds;
        let t = self.provinces.entry((p.p, p.q)).or_insert_with(|| {
            let ring = p.ring();
            if ring > u32::from(geometry::R_MAX_HARD) {
                return None;
            }
            let s = seeds.iter().find(|s| s.ring == ring)?;
            Some(terrain::generate_province(&s.seed, p))
        });
        t.as_ref().map(|t| t.terrain[idx as usize])
    }
}

fn dir_between(a: Hex, b: Hex) -> Option<u8> {
    DIRECTIONS
        .iter()
        .position(|&(dq, dr)| a.q + dq == b.q && a.r + dr == b.r)
        .map(|i| i as u8)
}

/// Steps of a direction list from `start`, with their terrain; the index
/// of the first bad direction or unknown hex on failure.
fn steps_of(land: &mut Land, start: Hex, dirs: &[u8]) -> Result<Vec<Step>, (u8, u32)> {
    let mut at = start;
    let mut out = Vec::with_capacity(dirs.len());
    for (i, &d) in dirs.iter().enumerate() {
        let (dq, dr) = *DIRECTIONS.get(d as usize).ok_or((9u8, i as u32))?;
        at = Hex::new(at.q.saturating_add(dq), at.r.saturating_add(dr));
        // Unknown land is priced as impassable (the program would refuse
        // the province as missing).
        let terrain = land.terrain(at).unwrap_or(Terrain::Water);
        out.push(Step {
            hex: at,
            terrain,
            road: false,
        });
    }
    Ok(out)
}

pub fn cost(a: &PathCostArgs) -> Result<PathCostOut, (u8, u32)> {
    let mut land = Land::new(&a.seeds);
    let steps = steps_of(&mut land, a.start, &a.dirs)?;
    let c = travel::path_cost(a.start, &steps, a.unit).map_err(travel_refusal)?;
    Ok(PathCostOut {
        secs: c.secs,
        hexes: c.hexes,
        provinces: c.provinces,
        end: steps.last().map(|s| s.hex).unwrap_or(a.start),
    })
}

fn step_secs(land: &mut Land, h: Hex, unit: UnitType) -> Option<u32> {
    travel::hex_secs(land.terrain(h)?, false, unit)
}

/// Dijkstra on `(secs, steps)` over the hexes within reach of both ends
/// (a hex farther than 32 steps from `start`, or that cannot still reach
/// `dest` in the steps left, is never expanded). The result is re-priced
/// by `travel::path_cost`, so the ≤ 4-provinces rule is the kernel's.
pub fn plan(a: &PlanArgs) -> Option<PathPlan> {
    let max = travel::MAX_PATH_STEPS as u32;
    if a.start == a.dest || a.start.distance(a.dest) > max {
        return None;
    }
    let blocked: BTreeSet<Hex> = a.blocked.iter().copied().collect();
    if blocked.contains(&a.dest) {
        return None;
    }
    let mut land = Land::new(&a.seeds);
    let mut best: BTreeMap<Hex, (u32, u32)> = BTreeMap::new();
    let mut prev: BTreeMap<Hex, Hex> = BTreeMap::new();
    let mut heap = BinaryHeap::new();
    best.insert(a.start, (0, 0));
    heap.push(Reverse((0u32, 0u32, a.start)));
    while let Some(Reverse((secs, steps, h))) = heap.pop() {
        if best.get(&h).is_some_and(|&b| b < (secs, steps)) {
            continue;
        }
        if h == a.dest {
            break;
        }
        for n in h.neighbors() {
            let nsteps = steps + 1;
            if nsteps + n.distance(a.dest) > max || blocked.contains(&n) {
                continue;
            }
            let Some(s) = step_secs(&mut land, n, a.unit) else {
                continue;
            };
            let cand = (secs.saturating_add(s), nsteps);
            if best.get(&n).is_none_or(|&b| cand < b) {
                best.insert(n, cand);
                prev.insert(n, h);
                heap.push(Reverse((cand.0, cand.1, n)));
            }
        }
    }
    best.get(&a.dest)?;
    let mut hexes = vec![a.dest];
    let mut at = a.dest;
    while at != a.start {
        at = *prev.get(&at)?;
        hexes.push(at);
    }
    hexes.reverse();
    let dirs: Vec<u8> = hexes
        .windows(2)
        .map(|w| dir_between(w[0], w[1]))
        .collect::<Option<_>>()?;
    let (path_len, path) = seal::encode_path(&dirs)?;
    let c = cost(&PathCostArgs {
        start: a.start,
        dirs: dirs.clone(),
        unit: a.unit,
        seeds: a.seeds.clone(),
    })
    .ok()?;
    Some(PathPlan {
        dirs,
        path_len,
        path,
        secs: c.secs,
        hexes: c.hexes,
        provinces: c.provinces,
    })
}
