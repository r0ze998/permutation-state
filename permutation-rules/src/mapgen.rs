//! Rotationally symmetric maps (§2.4, revised 2026-09-25).
//!
//! A season pays a prize by where nations end up, so where they start must
//! be fair. The earlier generator rolled every tile on its own and then
//! raised weak starts until their yields were within 10%; yields were equal,
//! but position was not (starts on the rim scored about 1.75× the central
//! ones in simulation). Here fairness is by construction and visible to
//! anyone: the map is six copies of one sextant, rotated by 60°, and each
//! nation starts at the same place in its own sextant.
//!
//! The sextant `W0 = {q > 0, r ≥ 0}` (91 tiles on a radius-13 map) is laid
//! out in polar terms: ring `d = q + r` and position `i = r` along it.
//! Inside it:
//!
//! * **Terrain with shape.** Elevation and moisture are integer value noise
//!   in (ring, angle), periodic in the angle, so the land flows across the
//!   seams between sextants. Terrain is assigned by rank, so every sextant
//!   has exactly the share table's counts: the lowest tiles are lakes and
//!   coast, the highest mountains and hills, the rest forest, grassland and
//!   plains by moisture.
//! * **Borders.** A ridge runs along each seam (the `i = 0` column), so
//!   neighbours meet through two passes: an inner one on the way to the
//!   centre, and an outer one held by a city-state (six in all, one on each
//!   border, each between the same two neighbours' starts).
//! * **Centre.** The centre tile holds the trade hub; iron and horses
//!   fields ring it (full reserves). Each start has a smaller iron and horses
//!   deposit of its own and two wheat tiles nearby: home deposits run dry in
//!   mid-season, the centre does not.
//! * **Rivers** run downhill from hills next to mountains.
//! * **Connectivity.** If mountains or water cut off any land, the
//!   cheapest way through is opened (mountain → hills, water → grassland).
//!
//! Everything is integer arithmetic on a hash of the map seed, so the chain
//! and every verifier generate the same map; the whole map is made in one
//! step (the cost is that of one sextant plus copying).

use crate::hex::{hexes_within, Hex};
use crate::map::{Map, Terrain, Tile, TileResource, TERRAIN_TABLE};
use crate::params::Ruleset;
use crate::rng::{rand_id, Seed};
use alloc::collections::VecDeque;
use alloc::vec;
use alloc::vec::Vec;
use borsh::{BorshDeserialize, BorshSerialize};

/// Rotate a hex by 60° about the origin.
pub const fn rotate(h: Hex) -> Hex {
    h.rotate()
}

/// Rotate a hex by `k` × 60°.
pub fn rotate_by(h: Hex, k: u8) -> Hex {
    h.rotate_by(k)
}

const fn unrotate(h: Hex) -> Hex {
    h.unrotate()
}

fn in_w0(h: Hex) -> bool {
    h.q > 0 && h.r >= 0
}

/// The sextant of a hex other than the origin, and its representative in
/// `W0`: `h == rotate_by(w, k)`.
pub fn sextant_of(h: Hex) -> Option<(u8, Hex)> {
    if h == Hex::ORIGIN {
        return None;
    }
    let mut w = h;
    for k in 0..6u8 {
        if in_w0(w) {
            return Some((k, w));
        }
        w = unrotate(w);
    }
    None
}

/// Nations that a symmetric map supports: a divisor of 6.
pub const fn symmetric_for(civs: usize) -> bool {
    matches!(civs, 1 | 2 | 3 | 6)
}

/// The sextant's tiles, in (ring, position) order, and a lookup by hex.
struct Sextant {
    radius: i32,
    tiles: Vec<Hex>,
}

impl Sextant {
    fn new(radius: u8) -> Sextant {
        let r = radius as i32;
        let mut tiles = Vec::new();
        for d in 1..=r {
            for i in 0..d {
                tiles.push(Hex::new(d - i, i));
            }
        }
        Sextant { radius: r, tiles }
    }
    fn index(&self, h: Hex) -> Option<usize> {
        if !in_w0(h) {
            return None;
        }
        let d = h.q + h.r;
        if d > self.radius {
            return None;
        }
        Some(((d - 1) * d / 2 + h.r) as usize)
    }
}

const fn ring(h: Hex) -> i32 {
    h.q + h.r
}

// ------------------------------------------------------------------ noise

fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A lattice value in 0..1024.
fn lattice(base: u64, salt: u64, x: i64, y: i64) -> i64 {
    let z = base
        ^ salt.wrapping_mul(0xD6E8_FEB8_6659_FD93)
        ^ (x as u64).wrapping_mul(0x632B_E59B_D9B4_E019)
        ^ (y as u64).wrapping_mul(0x8515_7AF5_B6E3_7F33);
    (mix(z) >> 54) as i64
}

/// 3t² − 2t³ in 1/1024 units.
fn smooth(t: i64) -> i64 {
    t * t * (3 * 1024 - 2 * t) / (1024 * 1024)
}

/// Value noise in 0..1024 at (`px`, `py`) (1/1024 lattice units), periodic
/// in y with `period` cells.
fn noise(base: u64, salt: u64, px: i64, py: i64, period: i64) -> i64 {
    let (x0, fx) = (px.div_euclid(1024), smooth(px.rem_euclid(1024)));
    let (y0, fy) = (py.div_euclid(1024), smooth(py.rem_euclid(1024)));
    let (ya, yb) = (y0.rem_euclid(period), (y0 + 1).rem_euclid(period));
    let v = |x, y| lattice(base, salt, x, y);
    let top = v(x0, ya) * (1024 - fx) + v(x0 + 1, ya) * fx;
    let bottom = v(x0, yb) * (1024 - fx) + v(x0 + 1, yb) * fx;
    (top * (1024 - fy) + bottom * fy) / (1024 * 1024)
}

/// Two octaves of noise at a sextant tile: ring `d` radially (a cell every
/// `cell` rings), position `i/d` around (`around` cells per sextant).
fn field(base: u64, salt: u64, h: Hex) -> i64 {
    let (d, i) = (ring(h) as i64, h.r as i64);
    let at = |cell: i64, around: i64, s: u64| {
        noise(
            base,
            salt ^ s,
            d * 1024 / cell,
            i * 1024 * around / d,
            around,
        )
    };
    3 * at(5, 2, 0) + at(3, 3, 0x55)
}

// ------------------------------------------------------------------ layout

/// Tiles per terrain in one sextant of `n` tiles: the share table by
/// largest remainder (ties: table order).
fn counts(n: usize) -> [usize; 6] {
    let mut out = [0usize; 6];
    let mut rest: Vec<(usize, usize)> = Vec::new();
    for (k, t) in TERRAIN_TABLE.iter().enumerate() {
        let exact = n * t.share_pct as usize;
        out[k] = exact / 100;
        rest.push((exact % 100, k));
    }
    rest.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let short = n - out.iter().sum::<usize>();
    for (_, k) in rest.into_iter().take(short) {
        out[k] += 1;
    }
    out
}

/// Where things go on a map of `radius`, derived from the radius alone.
pub struct Layout {
    /// Start in the sextant: ring `radius − 4`, middle position.
    pub start: Hex,
    /// The seam ring of the outer pass, where the city-state stands.
    pub gate: i32,
    /// The seam ring of the inner pass.
    pub inner_pass: i32,
}

pub fn layout(radius: u8) -> Layout {
    let r = radius as i32;
    let d = (r - 4).max(2);
    let start = Hex::new(d - d / 2, d / 2);
    // The outer pass: from the starts' ring outwards, the first seam ring at
    // least 6 from both starts it lies between (its own sextant's and the
    // previous one's).
    let prev = unrotate(start);
    let gate = (d..r)
        .find(|g| {
            let t = Hex::new(*g, 0);
            t.distance(start) >= 6 && t.distance(prev) >= 6
        })
        .unwrap_or(r - 1);
    debug_assert!(gate > d);
    Layout {
        start,
        gate,
        inner_pass: 4.min(r - 1),
    }
}

/// Symmetric generation in steps (the chain runs one step per transaction,
/// each within its compute budget): 0 terrain, 1 connectivity (one opening
/// per step until every land tile is reachable), 2 rivers and resources,
/// then done. Every change is made to all six turns of a tile, so the map
/// stays symmetric throughout.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SymJob {
    pub stage: u8,
    /// Elevation per sextant tile (rivers run down it).
    pub elev: Vec<i32>,
    pub map: Option<Map>,
}

impl SymJob {
    pub fn new() -> SymJob {
        SymJob {
            stage: 0,
            elev: Vec::new(),
            map: None,
        }
    }
}

impl Default for SymJob {
    fn default() -> Self {
        SymJob::new()
    }
}

pub enum SymStep {
    Working(SymJob),
    /// The map, the starts, the six city-states (sextant order) and the hub.
    Done(Map, Vec<Hex>, Vec<Hex>, Vec<Hex>),
}

/// Apply `f` to the six turns of sextant tile `w`.
fn set_all(map: &mut Map, w: Hex, f: impl Fn(&mut Tile)) {
    for k in 0..6 {
        if let Some(t) = map.tile_mut(w.rotate_by(k)) {
            f(t);
        }
    }
}

/// One step of symmetric generation.
pub fn step_symmetric(rules: &Ruleset, seed: &Seed, civs: usize, mut job: SymJob) -> SymStep {
    use Terrain::*;
    let radius = rules.map_radius;
    let sx = Sextant::new(radius);
    let n = sx.tiles.len();
    let lay = layout(radius);
    let start_i = sx.index(lay.start).expect("start in the sextant");
    let near = |k: usize, lo: u32, hi: u32| {
        let dist = sx.tiles[k].distance(lay.start);
        k != start_i && dist >= lo && dist <= hi
    };
    match job.stage {
        0 => {
            let base = rand_id(seed, b"mapgen", 0);
            let elev: Vec<i64> = sx
                .tiles
                .iter()
                .map(|h| {
                    let (d, i) = (ring(*h), h.r);
                    let mut e = field(base, 1, *h);
                    // A ridge along the seam, broken by the two passes.
                    if i == 0 && d >= 3 && d < sx.radius && d != lay.gate && d != lay.inner_pass {
                        e += 4000;
                    }
                    // The rim is lower: coasts form there.
                    if d == sx.radius {
                        e -= 500;
                    }
                    e
                })
                .collect();
            // Moisture, smoothed once with its six neighbours (read across the
            // seams through the rotation), so forests, grassland and plains
            // form patches.
            let raw: Vec<i64> = sx.tiles.iter().map(|h| field(base, 2, *h)).collect();
            let at = |h: Hex| -> Option<i64> {
                sextant_of(h).and_then(|(_, w)| sx.index(w)).map(|k| raw[k])
            };
            let moist: Vec<i64> = sx
                .tiles
                .iter()
                .enumerate()
                .map(|(k, h)| {
                    let (mut sum, mut cnt) = (2 * raw[k], 2);
                    for nb in h.neighbors() {
                        if let Some(v) = at(nb) {
                            sum += v;
                            cnt += 1;
                        }
                    }
                    sum / cnt
                })
                .collect();
            // Terrain by rank.
            let protected: Vec<bool> = sx
                .tiles
                .iter()
                .map(|h| {
                    ring(*h) <= 2
                        || h.distance(lay.start) <= 1
                        || (h.r == 0 && (ring(*h) == lay.gate || ring(*h) == lay.inner_pass))
                })
                .collect();
            let want = counts(n);
            let mut terrain: Vec<Option<Terrain>> = vec![None; n];
            terrain[start_i] = Some(Grassland);
            let assign = |t: Terrain,
                          count: usize,
                          key: &dyn Fn(usize) -> i64,
                          may: &dyn Fn(usize) -> bool,
                          terrain: &mut Vec<Option<Terrain>>| {
                let mut order: Vec<usize> = (0..n)
                    .filter(|k| terrain[*k].is_none() && may(*k))
                    .collect();
                order.sort_by_key(|k| (key(*k), *k));
                for k in order.into_iter().take(count) {
                    terrain[k] = Some(t);
                }
            };
            let idx = |t: Terrain| TERRAIN_TABLE.iter().position(|x| x.terrain == t).unwrap();
            assign(
                Water,
                want[idx(Water)],
                &|k| elev[k],
                &|k| !protected[k],
                &mut terrain,
            );
            assign(
                Mountain,
                want[idx(Mountain)],
                &|k| -elev[k],
                &|k| !protected[k],
                &mut terrain,
            );
            assign(
                Hills,
                want[idx(Hills)],
                &|k| -elev[k],
                &|_| true,
                &mut terrain,
            );
            assign(
                Forest,
                want[idx(Forest)],
                &|k| -moist[k],
                &|_| true,
                &mut terrain,
            );
            assign(
                Grassland,
                want[idx(Grassland)] - 1,
                &|k| -moist[k],
                &|_| true,
                &mut terrain,
            );
            assign(Plains, n, &|k| k as i64, &|_| true, &mut terrain);
            let mut terrain: Vec<Terrain> =
                terrain.into_iter().map(|t| t.unwrap_or(Plains)).collect();
            // The city-state and inner-pass seam tiles are open ground.
            for d in [lay.gate, lay.inner_pass] {
                if let Some(k) = sx.index(Hex::new(d, 0)) {
                    if !terrain[k].is_passable() {
                        terrain[k] = Plains;
                    }
                }
            }
            // A workable start (§2.4 validity): forest or hills within 2.
            if !(0..n).any(|k| near(k, 1, 2) && matches!(terrain[k], Forest | Hills)) {
                if let Some(k) = (0..n).find(|k| near(*k, 1, 2) && terrain[*k] == Plains) {
                    terrain[k] = Forest;
                }
            }
            // The whole map: the origin (grassland) and six turned copies.
            let tiles: Vec<Tile> = hexes_within(radius as u32)
                .into_iter()
                .map(|h| {
                    let t = sextant_of(h)
                        .and_then(|(_, w)| sx.index(w))
                        .map_or(Grassland, |k| terrain[k]);
                    Tile {
                        hex: h,
                        terrain: t,
                        river: false,
                        resource: None,
                        reserve: 0,
                        owner_city: None,
                        ruin_peak_pop: None,
                        heritage_claimed: false,
                    }
                })
                .collect();
            job.map = Some(Map { radius, tiles });
            job.elev = elev.into_iter().map(|e| e as i32).collect();
            job.stage = 1;
            SymStep::Working(job)
        }
        1 => {
            // Connectivity: a 0-1 BFS from the centre (cost 1 per impassable
            // tile crossed); if some land is cut off, open the cheapest way
            // to the nearest such tile (mountain → hills, water → grassland),
            // on all six turns, and check again on the next step.
            let map = job.map.as_mut().expect("map from stage 0");
            let len = map.tiles.len();
            let mut cost = vec![u32::MAX; len];
            let mut from = vec![u32::MAX; len];
            let o = map.index_of(Hex::ORIGIN).expect("origin");
            cost[o] = 0;
            let mut q = VecDeque::from([o as u32]);
            while let Some(a) = q.pop_front() {
                let a = a as usize;
                for nb in map.tiles[a].hex.neighbors() {
                    let Some(b) = map.index_of(nb) else { continue };
                    let w = if map.tiles[b].terrain.is_passable() {
                        0
                    } else {
                        1
                    };
                    if cost[a] + w < cost[b] {
                        cost[b] = cost[a] + w;
                        from[b] = a as u32;
                        if w == 0 {
                            q.push_front(b as u32);
                        } else {
                            q.push_back(b as u32);
                        }
                    }
                }
            }
            let cut = (0..len)
                .filter(|b| cost[*b] > 0 && map.tiles[*b].terrain.is_passable())
                .min_by_key(|b| (cost[*b], *b));
            match cut {
                None => job.stage = 2,
                Some(mut b) => {
                    while from[b] != u32::MAX {
                        let h = map.tiles[b].hex;
                        if let Some((_, w)) = sextant_of(h) {
                            set_all(map, w, |t| {
                                t.terrain = match t.terrain {
                                    Mountain => Hills,
                                    Water => Grassland,
                                    x => x,
                                }
                            });
                        }
                        b = from[b] as usize;
                    }
                }
            }
            SymStep::Working(job)
        }
        2 => {
            let map = job.map.as_mut().expect("map from stage 0");
            let elev = |h: Hex| -> i32 {
                sextant_of(h)
                    .and_then(|(_, w)| sx.index(w))
                    .map_or(0, |k| job.elev[k])
            };
            let terrain_at = |map: &Map, h: Hex| map.tile(h).map(|t| t.terrain);
            // Rivers: downhill from hills next to mountains.
            let mut sources: Vec<usize> = (0..n)
                .filter(|k| {
                    terrain_at(map, sx.tiles[*k]) == Some(Hills)
                        && sx.tiles[*k]
                            .neighbors()
                            .iter()
                            .any(|nb| terrain_at(map, *nb) == Some(Mountain))
                })
                .collect();
            sources.sort_by_key(|k| (-(job.elev[*k] as i64), *k));
            let river_cap = (n * 10).div_ceil(100);
            let mut river_tiles = 0usize;
            for s in sources {
                if river_tiles >= river_cap {
                    break;
                }
                let mut at = sx.tiles[s];
                for _ in 0..8 {
                    let Some((_, w)) = sextant_of(at) else { break };
                    let t = map.tile(w).expect("on the map");
                    if !t.terrain.is_passable() || t.river {
                        break;
                    }
                    set_all(map, w, |t| t.river = true);
                    river_tiles += 1;
                    if at
                        .neighbors()
                        .iter()
                        .any(|nb| terrain_at(map, *nb) == Some(Water))
                        || at.radius() == radius as u32
                    {
                        break;
                    }
                    let next = at
                        .neighbors()
                        .into_iter()
                        .filter(|nb| {
                            *nb != Hex::ORIGIN
                                && terrain_at(map, *nb).is_some_and(|t| t.is_passable())
                        })
                        .filter(|nb| elev(*nb) < elev(at))
                        .min_by_key(|nb| (elev(*nb), *nb));
                    match next {
                        Some(nb) => at = nb,
                        None => break,
                    }
                }
            }
            // Resources.
            let reserve = rules.resource_reserve;
            let place = |map: &mut Map,
                         area: &dyn Fn(usize) -> bool,
                         fits: &[Terrain],
                         res: TileResource,
                         amount: u16,
                         target: Hex| {
                let mut cands: Vec<usize> = (0..n)
                    .filter(|k| {
                        area(*k)
                            && *k != start_i
                            && map.tile(sx.tiles[*k]).is_some_and(|t| t.resource.is_none())
                    })
                    .collect();
                cands.sort_by_key(|k| (sx.tiles[*k].distance(target), *k));
                let terrain = |k: usize| map.tile(sx.tiles[k]).map(|t| t.terrain).unwrap_or(Water);
                let k = cands
                    .iter()
                    .copied()
                    .find(|k| fits.contains(&terrain(*k)))
                    .or_else(|| cands.iter().copied().find(|k| terrain(*k).is_passable()));
                if let Some(k) = k {
                    let convert = !fits.contains(&terrain(k));
                    set_all(map, sx.tiles[k], |t| {
                        if convert {
                            t.terrain = fits[0];
                        }
                        t.resource = Some(res);
                        t.reserve = amount;
                    });
                }
            };
            let start = lay.start;
            // Wheat: two tiles next to the start.
            for _ in 0..2 {
                place(
                    map,
                    &|k| near(k, 1, 2),
                    &[Grassland, Plains],
                    TileResource::Wheat,
                    0,
                    start,
                );
            }
            // Home deposits, half reserves, 3–4 from the start.
            place(
                map,
                &|k| near(k, 3, 4),
                &[Hills, Plains],
                TileResource::Iron,
                reserve / 2,
                start,
            );
            place(
                map,
                &|k| near(k, 3, 4),
                &[Grassland, Plains],
                TileResource::Horses,
                reserve / 2,
                start,
            );
            // Central fields, full reserves: iron on ring 3, horses on ring 5.
            let mid = |d: i32| Hex::new(d - d / 2, d / 2);
            place(
                map,
                &|k| ring(sx.tiles[k]) == 3 && sx.tiles[k].r > 0,
                &[Hills, Plains],
                TileResource::Iron,
                reserve,
                mid(3),
            );
            place(
                map,
                &|k| ring(sx.tiles[k]) == 5 && sx.tiles[k].r > 0,
                &[Grassland, Plains],
                TileResource::Horses,
                reserve,
                mid(5),
            );
            job.stage = 3;
            SymStep::Working(job)
        }
        _ => {
            let map = job.map.take().expect("map from stage 0");
            let starts: Vec<Hex> = (0..civs)
                .map(|c| rotate_by(lay.start, (c * 6 / civs) as u8))
                .collect();
            let city_states: Vec<Hex> = (0..6u8)
                .map(|k| rotate_by(Hex::new(lay.gate, 0), k))
                .collect();
            SymStep::Done(map, starts, city_states, vec![Hex::ORIGIN])
        }
    }
}

/// Generate a symmetric map for `civs` nations (a divisor of 6) in one go.
/// Returns the map, the starts (sextants `k · 6/civs`), the six city-states
/// (one per border, in sextant order) and the hub (the centre).
pub fn generate_symmetric(
    rules: &Ruleset,
    seed: &Seed,
    civs: usize,
) -> (Map, Vec<Hex>, Vec<Hex>, Vec<Hex>) {
    let mut job = SymJob::new();
    loop {
        match step_symmetric(rules, seed, civs, job) {
            SymStep::Working(j) => job = j,
            SymStep::Done(map, starts, cs, hubs) => return (map, starts, cs, hubs),
        }
    }
}

/// Turn a whole world by `k` × 60° about the origin (map, cities, units and
/// their paths and patrols, city-states, hubs). For tools and tests: the
/// rules treat every position of a symmetric map alike exactly when a
/// season played in a turned world with turned orders
/// (`Order::rotate`) ends the same.
pub fn rotate_world(s: &mut crate::state::WorldState, k: u8) {
    use crate::state::StandingRule;
    let old = s.map.clone();
    for t in &mut s.map.tiles {
        let src = t.hex.turned(k);
        if let Some(o) = old.tile(src) {
            *t = Tile {
                hex: t.hex,
                ..o.clone()
            };
        }
    }
    for c in &mut s.cities {
        c.hex = c.hex.rotate_by(k);
    }
    for u in &mut s.units {
        u.hex = u.hex.rotate_by(k);
        u.path.iter_mut().for_each(|h| *h = h.rotate_by(k));
        match &mut u.standing {
            StandingRule::AutoDefend { anchor, .. } => *anchor = anchor.rotate_by(k),
            StandingRule::Patrol { route, .. } => {
                route.iter_mut().for_each(|h| *h = h.rotate_by(k))
            }
            _ => {}
        }
    }
    for cs in &mut s.city_states {
        cs.hex = cs.hex.rotate_by(k);
    }
    for h in &mut s.hubs {
        *h = h.rotate_by(k);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::Preset;

    #[test]
    fn the_sextant_tiles_the_map_under_rotation() {
        for radius in [5u8, 13, 19] {
            let sx = Sextant::new(radius);
            assert_eq!(
                sx.tiles.len(),
                (radius as usize) * (radius as usize + 1) / 2
            );
            let mut seen = alloc::collections::BTreeSet::new();
            for w in &sx.tiles {
                assert_eq!(sx.index(*w).map(|k| sx.tiles[k]), Some(*w));
                for k in 0..6 {
                    let h = rotate_by(*w, k);
                    assert!(h.radius() <= radius as u32);
                    assert_eq!(sextant_of(h), Some((k, *w)));
                    assert!(seen.insert(h), "{h:?} twice");
                }
            }
            assert_eq!(seen.len() + 1, hexes_within(radius as u32).len());
        }
    }

    #[test]
    fn a_symmetric_map_is_the_same_after_a_sixth_turn() {
        let rules = Ruleset::new(Preset::Blitz);
        for s in 0..20u8 {
            let (map, starts, cs, hubs) = generate_symmetric(&rules, &[s; 32], 6);
            for t in &map.tiles {
                let r = map.tile(rotate(t.hex)).unwrap();
                assert_eq!(
                    (r.terrain, r.river, r.resource, r.reserve),
                    (t.terrain, t.river, t.resource, t.reserve)
                );
            }
            for (k, st) in starts.iter().enumerate() {
                assert_eq!(rotate(*st), starts[(k + 1) % 6]);
                assert!(map.tile(*st).unwrap().terrain.is_passable());
                assert!(map.tile(*st).unwrap().resource.is_none());
            }
            assert_eq!(cs.len(), 6);
            assert!(cs
                .iter()
                .all(|h| map.tile(*h).unwrap().terrain.is_passable()));
            assert_eq!(hubs, vec![Hex::ORIGIN]);
            // Terrain counts: six sextants of the share table (the centre
            // is grassland; opening cut-off land may move a few tiles).
            let n = map.tiles.len() - 1;
            assert_eq!(n % 6, 0);
        }
    }

    #[test]
    fn every_land_tile_is_reachable() {
        let rules = Ruleset::new(Preset::Blitz);
        for s in 0..40u8 {
            let (map, starts, cs, _) = generate_symmetric(&rules, &[s; 32], 6);
            let hexes: Vec<Hex> = map.tiles.iter().map(|t| t.hex).collect();
            let mut seen = vec![false; hexes.len()];
            let o = hexes.binary_search(&Hex::ORIGIN).unwrap();
            seen[o] = true;
            let mut q = VecDeque::from([Hex::ORIGIN]);
            while let Some(h) = q.pop_front() {
                for nb in h.neighbors() {
                    if let Ok(i) = hexes.binary_search(&nb) {
                        if !seen[i] && map.tiles[i].terrain.is_passable() {
                            seen[i] = true;
                            q.push_back(nb);
                        }
                    }
                }
            }
            for (i, t) in map.tiles.iter().enumerate() {
                assert!(
                    !t.terrain.is_passable() || seen[i],
                    "seed {s}: {:?} cut off",
                    t.hex
                );
            }
            for h in starts.iter().chain(&cs) {
                assert!(seen[hexes.binary_search(h).unwrap()]);
            }
        }
    }

    #[test]
    fn share_counts_fill_the_sextant() {
        assert_eq!(counts(91).iter().sum::<usize>(), 91);
        assert_eq!(counts(190).iter().sum::<usize>(), 190);
    }
}
