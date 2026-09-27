//! Province terrain: symmetric by construction (design §3.2).
//!
//! A province in wedge k at ring d is generated from its canonical copy in
//! wedge 0 (`ProvinceCoord::turned(k)`) with the ring seed `R_d`, and the
//! result is turned back. All six wedges of a ring therefore get identical
//! terrain, resources and sites: *every faction's wedge offers the same land
//! at the same ring at the same moment.* The Concord (ring 0) is made from
//! its own wedge-0 sextant, so it has the six-fold symmetry by itself.
//!
//! * **Terrain** is `mapgen`'s integer value noise (elevation, moisture)
//!   at the canonical hex, classified by thresholds into the six terrains
//!   of `map::Terrain`.
//! * **Connectivity by rule:** a province border is 9 adjacent tile pairs,
//!   5 tiles on each side. The middle 3 tiles of every side ([`GATES`])
//!   are always passable (a blocked one becomes a pass: mountain → hills,
//!   water → grassland). Both provinces of a border force their own middle
//!   tiles, whatever ring seed each was made with, so every border has at
//!   least 5 passable crossings.
//! * **Connectivity across the province:** from every gate tile and every
//!   site a fixed path to the province centre ([`carve_path`]: each step
//!   one tile closer to the centre, preferring a passable tile, then the
//!   lower tile index) is made passable the same way. So every site and
//!   every gate lies in the centre's passable component, and any two sites
//!   on the map are connected by land. The Concord carves the union of its
//!   paths turned six ways, keeping its six-fold symmetry.
//! * **Sites:** up to 12 settlement sites by score, at least 2 apart, ties
//!   to the lower canonical tile index. The Concord has none (neutral).

use super::geometry::{
    tile_index, tile_offset, tile_turned, ProvinceCoord, PROVINCE_TILES, SITES_PER_PROVINCE,
};
use crate::hex::Hex;
use crate::map::{Terrain, TileResource};
use crate::mapgen::noise;
use crate::rng::{rand, rand_id, Seed};
use borsh::{BorshDeserialize, BorshSerialize};

/// The middle 3 tiles of each border side, by neighbour direction
/// (`hex::DIRECTIONS` order in the province grid). Derived from the tiling
/// in the tests; `GATES[j]` turned by 60° is `GATES[j − 1]` (`Hex::rotate`
/// takes direction j to j − 1).
pub const GATES: [[u8; 3]; 6] = [
    [57, 58, 59],
    [35, 43, 50],
    [5, 11, 18],
    [1, 2, 3],
    [10, 17, 25],
    [42, 49, 55],
];

/// Noise cell sizes in tiles (coarse shape, detail).
const CELL_COARSE: i64 = 7;
const CELL_FINE: i64 = 3;
/// Non-periodic noise (the period is far outside the coordinate bound).
const NO_PERIOD: i64 = 1 << 40;

/// Elevation / moisture thresholds (0..1024).
const WATER_BELOW: i64 = 300;
const MOUNTAIN_ABOVE: i64 = 725;
const HILLS_ABOVE: i64 = 655;
const FOREST_ABOVE: i64 = 600;
const GRASS_ABOVE: i64 = 470;

/// One province's generated land.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProvinceTerrain {
    /// By tile index (`geometry::tile_offset` order).
    pub terrain: [Terrain; PROVINCE_TILES],
    pub resource: [Option<TileResource>; PROVINCE_TILES],
    /// Tile indices of the settlement sites; the first `site_count` are used.
    pub sites: [u8; SITES_PER_PROVINCE],
    pub site_count: u8,
}

impl ProvinceTerrain {
    pub fn is_site(&self, idx: u8) -> bool {
        self.sites[..self.site_count as usize].contains(&idx)
    }
    pub fn passable(&self, idx: u8) -> bool {
        self.terrain
            .get(idx as usize)
            .is_some_and(|t| t.is_passable())
    }
}

/// Planar noise coordinates (1/1024 lattice units) of a hex for a cell of
/// `cell` tiles: x = q + r/2, y = r·√3/2 (√3/2 ≈ 887/1024).
fn plane(h: Hex, cell: i64) -> (i64, i64) {
    let (q, r) = (h.q as i64, h.r as i64);
    ((2 * q + r) * 512 / cell, r * 887 / cell)
}

fn classify(e: i64, m: i64) -> Terrain {
    if e < WATER_BELOW {
        Terrain::Water
    } else if e > MOUNTAIN_ABOVE {
        Terrain::Mountain
    } else if e > HILLS_ABOVE {
        Terrain::Hills
    } else if m > FOREST_ABOVE {
        Terrain::Forest
    } else if m > GRASS_ABOVE {
        Terrain::Grassland
    } else {
        Terrain::Plains
    }
}

fn hex_key(h: Hex) -> u64 {
    ((h.q as u32 as u64) << 32) | h.r as u32 as u64
}

/// Terrain and resource of the canonical hex `c` under ring seed `seed`.
pub fn tile_at(seed: &Seed, c: Hex) -> (Terrain, Option<TileResource>) {
    let base = rand(seed, b"frontier/terrain", &[]);
    let at = |salt: u64, cell: i64| {
        let (x, y) = plane(c, cell);
        noise(base, salt, x, y, NO_PERIOD)
    };
    let e = (3 * at(1, CELL_COARSE) + at(2, CELL_FINE)) / 4;
    let m = (3 * at(3, CELL_COARSE) + at(4, CELL_FINE)) / 4;
    let t = classify(e, m);
    let roll = rand_id(seed, b"frontier/feat", hex_key(c)) % 16;
    let res = match (roll, t) {
        (0, Terrain::Grassland | Terrain::Plains) => Some(TileResource::Wheat),
        (1, Terrain::Hills | Terrain::Forest) => Some(TileResource::Iron),
        (2, Terrain::Plains | Terrain::Grassland) => Some(TileResource::Horses),
        _ => None,
    };
    (t, res)
}

/// Site score of a tile: 0 if it cannot hold a settlement.
fn site_score(t: Terrain, r: Option<TileResource>) -> u32 {
    if !t.is_passable() {
        return 0;
    }
    let i = t.info();
    let bonus = if r.is_some() { 3 } else { 0 };
    1 + 3 * i.food + 2 * i.prod + 2 * i.gold + bonus
}

/// A blocked tile made passable: mountain → hills, water → grassland.
fn pass(t: Terrain) -> Terrain {
    match t {
        Terrain::Mountain => Terrain::Hills,
        Terrain::Water => Terrain::Grassland,
        other => other,
    }
}

/// The fixed path from tile `from` to the province centre: each step goes
/// to a neighbour one tile closer to the centre, preferring a passable one,
/// then the lower tile index. Marks every tile on it (both ends included).
fn carve_path(terrain: &[Terrain; PROVINCE_TILES], from: u8, mark: &mut [bool; PROVINCE_TILES]) {
    let Some(mut at) = tile_offset(from) else {
        return;
    };
    loop {
        if let Some(i) = tile_index(at) {
            mark[i as usize] = true;
        }
        let d = at.distance(Hex::ORIGIN);
        if d == 0 {
            return;
        }
        let mut best: Option<(bool, u8, Hex)> = None;
        for n in at.neighbors() {
            if n.distance(Hex::ORIGIN) + 1 != d {
                continue;
            }
            let Some(i) = tile_index(n) else { continue };
            let key = (!terrain[i as usize].is_passable(), i, n);
            if best.is_none_or(|b| (key.0, key.1) < (b.0, b.1)) {
                best = Some(key);
            }
        }
        match best {
            Some((_, _, n)) => at = n,
            None => return,
        }
    }
}

/// Make every marked tile passable (turned six ways for the Concord).
fn carve(out: &mut ProvinceTerrain, mark: &[bool; PROVINCE_TILES], concord: bool) {
    for i in 0..PROVINCE_TILES as u8 {
        if !mark[i as usize] {
            continue;
        }
        let turns = if concord { 6 } else { 1 };
        for k in 0..turns {
            let j = tile_turned(i, k) as usize;
            out.terrain[j] = pass(out.terrain[j]);
        }
    }
}

/// The canonical copy (wedge 0, or the Concord) of a province's land.
fn canonical(seed: &Seed, pc: ProvinceCoord) -> ProvinceTerrain {
    let concord = pc.is_concord();
    let mut out = ProvinceTerrain {
        terrain: [Terrain::Plains; PROVINCE_TILES],
        resource: [None; PROVINCE_TILES],
        sites: [0; SITES_PER_PROVINCE],
        site_count: 0,
    };
    for i in 0..PROVINCE_TILES as u8 {
        let mut g = pc.tile(i).unwrap_or(Hex::ORIGIN);
        if concord {
            g = g.turned(g.sextant()); // six-fold symmetric by itself
        }
        let (t, r) = tile_at(seed, g);
        out.terrain[i as usize] = t;
        out.resource[i as usize] = r;
    }
    for side in GATES {
        for i in side {
            let t = &mut out.terrain[i as usize];
            *t = pass(*t);
        }
    }
    // Every gate reaches the centre.
    let mut mark = [false; PROVINCE_TILES];
    for side in GATES {
        for i in side {
            carve_path(&out.terrain, i, &mut mark);
        }
    }
    carve(&mut out, &mark, concord);
    if concord {
        return out;
    }
    // Greedy sites: best score first, ties to the lower index, ≥ 2 apart.
    let mut score: [u32; PROVINCE_TILES] =
        core::array::from_fn(|i| site_score(out.terrain[i], out.resource[i]));
    while (out.site_count as usize) < SITES_PER_PROVINCE {
        let mut best: Option<usize> = None;
        for i in 0..PROVINCE_TILES {
            if score[i] > 0 && best.is_none_or(|b| score[i] > score[b]) {
                best = Some(i);
            }
        }
        let Some(b) = best else { break };
        out.sites[out.site_count as usize] = b as u8;
        out.site_count += 1;
        let ob = tile_offset(b as u8).unwrap_or(Hex::ORIGIN);
        for (i, s) in score.iter_mut().enumerate() {
            let oi = tile_offset(i as u8).unwrap_or(Hex::ORIGIN);
            if oi.distance(ob) < 2 {
                *s = 0;
            }
        }
    }
    // Every site reaches the centre.
    let mut mark = [false; PROVINCE_TILES];
    for s in 0..out.site_count as usize {
        carve_path(&out.terrain, out.sites[s], &mut mark);
    }
    carve(&mut out, &mark, false);
    out
}

/// The land of province `p`, from the seed of its ring (design §3.2). Pure:
/// the program runs it once in `OpenProvince`, every verifier re-runs it.
pub fn generate_province(ring_seed: &Seed, p: ProvinceCoord) -> ProvinceTerrain {
    let k = p.wedge().unwrap_or(0);
    let canon = canonical(ring_seed, p.turned(k));
    if k == 0 {
        return canon;
    }
    let mut out = canon.clone();
    for i in 0..PROVINCE_TILES as u8 {
        let c = tile_turned(i, k) as usize;
        out.terrain[i as usize] = canon.terrain[c];
        out.resource[i as usize] = canon.resource[c];
    }
    for s in 0..canon.site_count as usize {
        // the tile whose canonical copy is the canonical site
        let o = tile_offset(canon.sites[s]).unwrap_or(Hex::ORIGIN);
        out.sites[s] = tile_index(o.rotate_by(k)).unwrap_or(0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontier::geometry::province_of;

    /// `GATES[j]` are the middle 3 of the 5 tiles facing neighbour j,
    /// ordered along the border, and turn with the map.
    #[test]
    fn gates_are_the_middle_of_each_border() {
        let p0 = ProvinceCoord::CONCORD;
        for (j, pj) in p0.neighbors().iter().enumerate() {
            let c = pj.centre();
            let mut own: alloc::vec::Vec<(i64, u8)> = alloc::vec::Vec::new();
            for i in 0..PROVINCE_TILES as u8 {
                let t = p0.tile(i).unwrap();
                for n in t.neighbors() {
                    if province_of(n) == *pj {
                        // position along the border: cross product with the
                        // direction to the neighbour's centre
                        let key = t.r as i64 * c.q as i64 - t.q as i64 * c.r as i64;
                        if !own.iter().any(|x| x.1 == i) {
                            own.push((key, i));
                        }
                    }
                }
            }
            own.sort();
            assert_eq!(own.len(), 5, "dir {j}");
            let mut mid = [own[1].1, own[2].1, own[3].1];
            mid.sort();
            assert_eq!(mid, GATES[j], "dir {j}");
            let mut turned: [u8; 3] =
                GATES[j].map(|i| tile_index(tile_offset(i).unwrap().rotate()).unwrap());
            turned.sort();
            assert_eq!(turned, GATES[(j + 5) % 6]);
        }
    }
}
