//! Map, terrain and deterministic generation (§2).

use crate::hex::{hexes_within, Hex};
use crate::params::Ruleset;
use crate::rng::{rand_id, Seed};
use crate::RulesError;
use alloc::vec::Vec;
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Terrain {
    Grassland,
    Plains,
    Forest,
    Hills,
    Mountain,
    Water,
}

#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize)]
pub struct TerrainInfo {
    pub terrain: Terrain,
    /// Percentage share of generated tiles.
    pub share_pct: u8,
    pub food: u32,
    pub prod: u32,
    pub gold: u32,
    /// 0 = impassable.
    pub move_cost: u8,
    /// Multiplier on damage dealt *to* a defender standing here (§8.2 #4).
    pub defense_bps: u32,
    pub vision_bonus: u8,
}

use Terrain::*;

/// Indexed by `Terrain as usize` (§2.1).
#[rustfmt::skip]
pub const TERRAIN_TABLE: [TerrainInfo; 6] = [
    TerrainInfo { terrain: Grassland, share_pct: 30, food: 2, prod: 0, gold: 0, move_cost: 1, defense_bps: 10_000, vision_bonus: 0 },
    TerrainInfo { terrain: Plains,    share_pct: 25, food: 1, prod: 1, gold: 0, move_cost: 1, defense_bps: 10_000, vision_bonus: 0 },
    TerrainInfo { terrain: Forest,    share_pct: 18, food: 1, prod: 2, gold: 0, move_cost: 2, defense_bps:  8_000, vision_bonus: 0 },
    TerrainInfo { terrain: Hills,     share_pct: 12, food: 0, prod: 2, gold: 0, move_cost: 2, defense_bps:  8_000, vision_bonus: 1 },
    TerrainInfo { terrain: Mountain,  share_pct:  7, food: 0, prod: 0, gold: 0, move_cost: 0, defense_bps: 10_000, vision_bonus: 0 },
    TerrainInfo { terrain: Water,     share_pct:  8, food: 1, prod: 0, gold: 1, move_cost: 0, defense_bps: 10_000, vision_bonus: 0 },
];

impl Terrain {
    pub const fn info(self) -> &'static TerrainInfo {
        &TERRAIN_TABLE[self as usize]
    }
    pub const fn is_land(self) -> bool {
        !matches!(self, Water)
    }
    pub const fn is_passable(self) -> bool {
        !matches!(self, Water | Mountain)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum TileResource {
    Wheat,
    Iron,
    Horses,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Tile {
    pub hex: Hex,
    pub terrain: Terrain,
    pub river: bool,
    pub resource: Option<TileResource>,
    /// Remaining strategic reserve (Iron/Horses); persists across seasons.
    pub reserve: u16,
    /// City whose territory contains this tile.
    pub owner_city: Option<u32>,
    /// Peak population of a ruin left here by a previous season (§3.4).
    pub ruin_peak_pop: Option<u16>,
    /// A city has already taken this ruin's heritage bonus this season (§3.4).
    pub heritage_claimed: bool,
}

impl Tile {
    /// Base yields including river and resource bonuses (§2.1, §2.2).
    /// Returned as whole units: (food, prod, gold).
    pub fn yields(&self) -> (u32, u32, u32) {
        let t = self.terrain.info();
        let (mut f, mut p, mut g) = (t.food, t.prod, t.gold);
        if self.river {
            g += 1;
        }
        match self.resource {
            Some(TileResource::Wheat) => f += 2,
            Some(TileResource::Iron) => p += 1,
            Some(TileResource::Horses) => f += 1,
            None => {}
        }
        (f, p, g)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Map {
    pub radius: u8,
    /// Sorted by (q, r); index = tile index.
    pub tiles: Vec<Tile>,
}

impl Map {
    /// Tile index of `hex`. Maps are always the full hexagon `hexes_within(radius)`
    /// in (q, r) order, so the index is arithmetic: the tiles of every column
    /// `q' < q`, plus the offset of `r` inside column `q`. (A binary search
    /// here dominated the engine's cost on chain.)
    pub fn index_of(&self, hex: Hex) -> Option<usize> {
        let big_r = self.radius as i64;
        if hex.radius() > self.radius as u32 {
            return None;
        }
        let (q, r) = (hex.q as i64, hex.r as i64);
        let width = 2 * big_r + 1;
        let offset = if q <= 0 {
            let n = q + big_r; // columns -R ..= q-1, lengths width + q'
            n * width + n * (q - 1 - big_r) / 2
        } else {
            let at0 = big_r * width - big_r * (big_r + 1) / 2;
            at0 + q * width - q * (q - 1) / 2
        };
        let r_min = (-big_r).max(-q - big_r);
        let i = (offset + r - r_min) as usize;
        match self.tiles.get(i) {
            Some(t) if t.hex == hex => Some(i),
            // Only a hand-built, non-hexagonal map gets here.
            _ => self.tiles.binary_search_by(|t| t.hex.cmp(&hex)).ok(),
        }
    }
    pub fn tile(&self, hex: Hex) -> Option<&Tile> {
        self.index_of(hex).map(|i| &self.tiles[i])
    }
    pub fn tile_mut(&mut self, hex: Hex) -> Option<&mut Tile> {
        self.index_of(hex).map(move |i| &mut self.tiles[i])
    }

    /// Indices of the on-map tiles within `radius` of `center`, ascending.
    /// Visits only the hexes in range (≤ 37 for radius 3), which matters on
    /// chain where scanning every tile per city is the dominant cost. On a
    /// full hexagonal map the tiles of one column are consecutive, so each
    /// column costs one `index_of`, not one per hex.
    pub fn indices_within(&self, center: Hex, radius: u32) -> Vec<usize> {
        let r = radius as i32;
        let mut out = Vec::with_capacity((3 * radius * (radius + 1) + 1) as usize);
        let big_r = self.radius as i32;
        let full = self.tiles.len() == (3 * big_r * (big_r + 1) + 1) as usize;
        for dq in -r..=r {
            let q = center.q + dq;
            let (lo, hi) = (center.r + (-r).max(-dq - r), center.r + r.min(-dq + r));
            if !full {
                out.extend((lo..=hi).filter_map(|rr| self.index_of(Hex::new(q, rr))));
                continue;
            }
            if q.abs() > big_r {
                continue;
            }
            // The rows of column `q` on the map, intersected with the range.
            let (lo, hi) = (
                lo.max((-big_r).max(-q - big_r)),
                hi.min(big_r.min(-q + big_r)),
            );
            if lo > hi {
                continue;
            }
            let Some(first) = self.index_of(Hex::new(q, lo)) else {
                continue;
            };
            out.extend(first..=first + (hi - lo) as usize);
        }
        out // (q, r) iteration order is tile-index order
    }

    /// Claim unowned tiles within `radius` of `center` for `city` (§5.4):
    /// ascending distance, then tile index. Owned tiles are never taken.
    pub fn claim_territory(&mut self, city: u32, center: Hex, radius: u32) {
        let mut targets: Vec<(u32, usize)> = self
            .indices_within(center, radius)
            .into_iter()
            .filter(|&i| self.tiles[i].owner_city.is_none())
            .map(|i| (self.tiles[i].hex.distance(center), i))
            .collect();
        targets.sort();
        for (_, i) in targets {
            self.tiles[i].owner_city = Some(city);
        }
    }
}

/// The largest `territory_radius`: no city owns a tile farther away.
pub const MAX_TERRITORY_RADIUS: u32 = 3;

/// Territory radius for a city of this population (§5.4).
pub const fn territory_radius(pop: u32) -> u32 {
    if pop >= 6 {
        3
    } else if pop >= 3 {
        2
    } else {
        1
    }
}

/// Output of generation: the map plus special sites.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Generated {
    pub map: Map,
    pub starts: Vec<Hex>,
    pub city_states: Vec<Hex>,
    pub hubs: Vec<Hex>,
}

/// Deterministic generation (§2.4). Rerolls (new `attempt` in the domain)
/// until starts are within the fairness bound or attempts run out.
pub fn generate(rules: &Ruleset, world_seed: &Seed, civs: usize) -> Result<Generated, RulesError> {
    let mut job = MapJob::new(rules, civs)?;
    loop {
        match job.step(rules, world_seed, u32::MAX)? {
            MapStep::Working(j) => job = j,
            MapStep::Done(g) => return Ok(g),
        }
    }
}

/// Map generation in bounded steps, for callers with a compute budget (the
/// on-chain program runs one step per transaction). Driving it to the end
/// gives exactly `generate`'s result; `generate` is implemented with it.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MapJob {
    pub civs: u16,
    pub attempt: u64,
    /// Terrain of the current attempt, once generated.
    pub map: Option<Map>,
    /// Start validity per tile index, filled up to `scanned`.
    pub valid: Vec<bool>,
    pub scanned: u32,
    /// Starts of this attempt, once placed.
    pub starts: Vec<Hex>,
    /// Balancing rounds that changed a tile so far (at most `BALANCE_ROUNDS`).
    pub rounds: u16,
    pub balanced: bool,
}

/// Cap on start-balancing rounds (§2.4).
pub const BALANCE_ROUNDS: u16 = 256;
/// Work units per balancing round when stepping (one unit = one start
/// candidate checked); a round re-values every start.
pub const BALANCE_ROUND_WORK: u32 = 25;

pub enum MapStep {
    Working(MapJob),
    Done(Generated),
}

impl MapJob {
    pub fn new(rules: &Ruleset, civs: usize) -> Result<MapJob, RulesError> {
        if civs == 0 || civs > rules.max_civs as usize {
            return Err(RulesError::MapGeneration("civilization count out of range"));
        }
        Ok(MapJob {
            civs: civs as u16,
            attempt: 0,
            map: None,
            valid: Vec::new(),
            scanned: 0,
            starts: Vec::new(),
            rounds: 0,
            balanced: false,
        })
    }

    /// One step: generate the attempt's terrain; or check up to `work` start
    /// candidates; or place starts; or run up to `work / BALANCE_ROUND_WORK`
    /// balancing rounds; or place sites. A failed stage moves to the next attempt.
    pub fn step(
        mut self,
        rules: &Ruleset,
        world_seed: &Seed,
        work: u32,
    ) -> Result<MapStep, RulesError> {
        if self.attempt >= rules.start_generation_attempts as u64 {
            return Err(RulesError::MapGeneration(
                "no fair start layout within attempt limit",
            ));
        }
        let Some(mut map) = self.map.take() else {
            let map = generate_terrain(rules, world_seed, self.attempt);
            self.valid = alloc::vec![false; map.tiles.len()];
            self.map = Some(map);
            return Ok(MapStep::Working(self));
        };
        let n = map.tiles.len() as u32;
        if self.scanned < n {
            let end = self.scanned.saturating_add(work).min(n);
            for i in self.scanned..end {
                let hex = map.tiles[i as usize].hex;
                self.valid[i as usize] = hex.radius() < map.radius as u32 && valid_start(&map, hex);
            }
            self.scanned = end;
        } else if self.starts.is_empty() {
            match place_starts(
                rules,
                &map,
                world_seed,
                self.attempt,
                self.civs as usize,
                &self.valid,
            ) {
                Some(starts) => self.starts = starts,
                None => return Ok(MapStep::Working(self.next_attempt())),
            }
        } else if !self.balanced {
            for _ in 0..(work / BALANCE_ROUND_WORK).max(1) {
                if self.rounds >= BALANCE_ROUNDS {
                    return Ok(MapStep::Working(self.next_attempt()));
                }
                match balance_round(rules, &mut map, &self.starts) {
                    Balance::Fair => {
                        self.balanced = true;
                        break;
                    }
                    Balance::Changed => self.rounds += 1,
                    Balance::Stuck => return Ok(MapStep::Working(self.next_attempt())),
                }
            }
        } else {
            let civs = self.civs as usize;
            let city_states = place_sites(
                &map,
                world_seed,
                self.attempt,
                b"citystate",
                (civs / 3).max(2),
                &self.starts,
                6,
                true,
            );
            let hubs = place_sites(
                &map,
                world_seed,
                self.attempt,
                b"hub",
                (civs / 4).max(2),
                &self.starts,
                5,
                false,
            );
            return Ok(match (city_states, hubs) {
                (Some(city_states), Some(hubs)) => MapStep::Done(Generated {
                    map,
                    starts: self.starts,
                    city_states,
                    hubs,
                }),
                _ => MapStep::Working(self.next_attempt()),
            });
        }
        self.map = Some(map);
        Ok(MapStep::Working(self))
    }

    fn next_attempt(self) -> MapJob {
        MapJob {
            civs: self.civs,
            attempt: self.attempt + 1,
            map: None,
            valid: Vec::new(),
            scanned: 0,
            starts: Vec::new(),
            rounds: 0,
            balanced: false,
        }
    }
}

fn generate_terrain(rules: &Ruleset, seed: &Seed, attempt: u64) -> Map {
    let domain_seed = attempt_seed(seed, attempt);
    let mut tiles = Vec::new();
    for (i, hex) in hexes_within(rules.map_radius as u32)
        .into_iter()
        .enumerate()
    {
        let roll = (rand_id(&domain_seed, b"terrain", i as u64) % 100) as u8;
        let mut acc = 0u8;
        let mut terrain = Water;
        for info in TERRAIN_TABLE {
            acc += info.share_pct;
            if roll < acc {
                terrain = info.terrain;
                break;
            }
        }
        let river = terrain.is_passable() && rand_id(&domain_seed, b"river", i as u64) % 10 == 0;
        let res_roll = rand_id(&domain_seed, b"resource", i as u64) % 120;
        // Densities (§2.2): wheat 1/30, iron 1/40, horses 1/40 of eligible land.
        let resource = match terrain {
            Grassland | Plains if res_roll < 4 => Some(TileResource::Wheat),
            Hills | Plains if (4..7).contains(&res_roll) => Some(TileResource::Iron),
            Grassland | Plains if (7..10).contains(&res_roll) => Some(TileResource::Horses),
            _ => None,
        };
        let reserve = match resource {
            Some(TileResource::Iron | TileResource::Horses) => rules.resource_reserve,
            _ => 0,
        };
        tiles.push(Tile {
            hex,
            terrain,
            river,
            resource,
            reserve,
            owner_city: None,
            ruin_peak_pop: None,
            heritage_claimed: false,
        });
    }
    Map {
        radius: rules.map_radius,
        tiles,
    }
}

fn attempt_seed(seed: &Seed, attempt: u64) -> Seed {
    let mut s = *seed;
    let r = rand_id(seed, b"attempt", attempt).to_le_bytes();
    for (i, b) in r.iter().enumerate() {
        s[i] ^= *b;
    }
    s
}

/// Start constraints (§2.4).
fn valid_start(map: &Map, hex: Hex) -> bool {
    let Some(tile) = map.tile(hex) else {
        return false;
    };
    if !tile.terrain.is_passable() || tile.resource.is_some() {
        return false;
    }
    let near = |r: u32| {
        map.indices_within(hex, r)
            .into_iter()
            .map(|i| &map.tiles[i])
            .filter(move |t| t.hex != hex)
    };
    let food_tiles = near(2).filter(|t| t.yields().0 >= 2).count();
    let prod_tiles = near(2)
        .filter(|t| matches!(t.terrain, Forest | Hills))
        .count();
    let strategic = near(5)
        .filter(|t| matches!(t.resource, Some(TileResource::Iron | TileResource::Horses)))
        .count();
    food_tiles >= 2 && prod_tiles >= 1 && strategic >= 1
}

/// Farthest-point start placement (§2.4): the first start is the valid
/// candidate with the lowest random key; each next start is the valid
/// candidate farthest from all chosen starts (ties by random key). Fails if
/// the best remaining spacing drops below `start_min_distance`.
fn place_starts(
    rules: &Ruleset,
    map: &Map,
    seed: &Seed,
    attempt: u64,
    civs: usize,
    valid: &[bool],
) -> Option<Vec<Hex>> {
    let s = attempt_seed(seed, attempt);
    let candidates: Vec<(u64, Hex)> = map
        .tiles
        .iter()
        .enumerate()
        .filter(|(i, _)| valid[*i])
        .map(|(i, t)| (rand_id(&s, b"start", i as u64), t.hex))
        .collect();
    let first = *candidates.iter().min()?;
    let mut starts = alloc::vec![first.1];
    while starts.len() < civs {
        let (spacing, _, hex) = candidates
            .iter()
            .map(|(key, h)| {
                let d = starts
                    .iter()
                    .map(|st| st.distance(*h))
                    .min()
                    .unwrap_or(u32::MAX);
                (d, core::cmp::Reverse(*key), *h)
            })
            .max()?;
        if spacing < rules.start_min_distance as u32 {
            return None;
        }
        starts.push(hex);
    }
    Some(starts)
}

/// Start balancing (§2.4): raise weak starts until every start value is within
/// the fairness bound of the best one. Only tiles within radius 3 that are
/// strictly closer to that start than to any other are changed, so balancing
/// one start never raises another. Upgrades, in order of preference:
/// Mountain→Hills, then Wheat on a plain Grassland/Plains tile, then
/// Water→Grassland. The attempt fails if no upgrade is left or after
/// `BALANCE_ROUNDS` rounds.
enum Balance {
    /// Every start is within the fairness bound.
    Fair,
    /// Some start was raised; value again.
    Changed,
    /// A weak start has no upgrade left.
    Stuck,
}

/// One balancing round (see above): re-value every start and upgrade one tile
/// near each start below the floor. `MapJob` runs at most `BALANCE_ROUNDS`.
fn balance_round(rules: &Ruleset, map: &mut Map, starts: &[Hex]) -> Balance {
    let values: Vec<u64> = starts.iter().map(|s| start_value(map, *s)).collect();
    let max = values.iter().copied().max().unwrap_or(0);
    let floor = (max * 10_000).div_ceil(rules.start_fairness_max_bps as u64);
    let mut all_ok = true;
    let mut changed = false;
    for (i, st) in starts.iter().enumerate() {
        if values[i] >= floor {
            continue;
        }
        all_ok = false;
        changed |= upgrade_one(map, *st, starts);
    }
    if all_ok {
        Balance::Fair
    } else if changed {
        Balance::Changed
    } else {
        Balance::Stuck
    }
}

fn upgrade_one(map: &mut Map, start: Hex, starts: &[Hex]) -> bool {
    let own = |h: Hex| {
        let d = h.distance(start);
        h != start && d <= 3 && starts.iter().all(|o| *o == start || o.distance(h) > d)
    };
    let mut region: Vec<(u32, usize)> = map
        .indices_within(start, 3)
        .into_iter()
        .filter(|&i| own(map.tiles[i].hex))
        .map(|i| (map.tiles[i].hex.distance(start), i))
        .collect();
    region.sort();
    for pass in 0..3 {
        for &(_, i) in &region {
            let t = &mut map.tiles[i];
            let done = match (pass, t.terrain, t.resource) {
                (0, Mountain, _) => {
                    t.terrain = Hills;
                    true
                }
                (1, Grassland | Plains, None) => {
                    t.resource = Some(TileResource::Wheat);
                    true
                }
                (2, Water, _) => {
                    t.terrain = Grassland;
                    t.river = false;
                    true
                }
                _ => false,
            };
            if done {
                return true;
            }
        }
    }
    false
}

/// `V = Σ radius-3 (2·food + 2·prod + gold) + 6 × strategic within 5` (§2.4).
pub fn start_value(map: &Map, hex: Hex) -> u64 {
    let mut v = 0u64;
    for i in map.indices_within(hex, 5) {
        let t = &map.tiles[i];
        let d = t.hex.distance(hex);
        if d <= 3 {
            let (f, p, g) = t.yields();
            v += (2 * f + 2 * p + g) as u64;
        }
        if d <= 5 && matches!(t.resource, Some(TileResource::Iron | TileResource::Horses)) {
            v += 6;
        }
    }
    v
}

#[allow(clippy::too_many_arguments)]
fn place_sites(
    map: &Map,
    seed: &Seed,
    attempt: u64,
    domain: &[u8],
    count: usize,
    starts: &[Hex],
    min_distance: u32,
    spaced: bool,
) -> Option<Vec<Hex>> {
    let s = attempt_seed(seed, attempt);
    let mut candidates: Vec<(u64, Hex)> = map
        .tiles
        .iter()
        .filter(|t| t.terrain.is_passable())
        .enumerate()
        .map(|(i, t)| (rand_id(&s, domain, i as u64), t.hex))
        .collect();
    candidates.sort();
    let mut out: Vec<Hex> = Vec::new();
    for (_, hex) in candidates {
        if out.len() == count {
            break;
        }
        let far_from_starts = starts.iter().all(|st| st.distance(hex) >= min_distance);
        let far_from_sites = !spaced || out.iter().all(|o| o.distance(hex) >= min_distance);
        if far_from_starts && far_from_sites {
            out.push(hex);
        }
    }
    (out.len() == count).then_some(out)
}

#[cfg(test)]
mod tests {
    /// The column-wise `indices_within` finds exactly the tiles a per-hex
    /// scan finds, in the same order, everywhere on the map and off its edge.
    #[test]
    fn indices_within_matches_a_per_hex_scan() {
        let rules = crate::params::Ruleset::new(crate::params::Preset::Blitz);
        let g = super::generate(&rules, &[5; 32], 6).unwrap();
        let big = g.map.radius as i32 + 2;
        for q in -big..=big {
            for rr in -big..=big {
                let c = Hex::new(q, rr);
                for radius in 0..=5u32 {
                    let r = radius as i32;
                    let mut want = Vec::new();
                    for dq in -r..=r {
                        for dr in (-r).max(-dq - r)..=r.min(-dq + r) {
                            if let Some(i) = g.map.index_of(Hex::new(q + dq, rr + dr)) {
                                want.push(i);
                            }
                        }
                    }
                    assert_eq!(
                        g.map.indices_within(c, radius),
                        want,
                        "center {c:?} radius {radius}"
                    );
                }
            }
        }
    }

    use super::*;
    use crate::params::Preset;

    #[test]
    fn terrain_shares_sum_to_100() {
        assert_eq!(
            TERRAIN_TABLE
                .iter()
                .map(|t| t.share_pct as u32)
                .sum::<u32>(),
            100
        );
        for (i, t) in TERRAIN_TABLE.iter().enumerate() {
            assert_eq!(t.terrain as usize, i);
        }
    }

    #[test]
    fn stepped_generation_equals_generate() {
        for (preset, civs) in [(Preset::Blitz, 6), (Preset::Season, 16)] {
            let rules = Ruleset::new(preset);
            let seed = [9u8; 32];
            let whole = generate(&rules, &seed, civs).expect("map");
            let mut job = MapJob::new(&rules, civs).unwrap();
            let mut steps = 0;
            let stepped = loop {
                steps += 1;
                match job.step(&rules, &seed, 37).unwrap() {
                    MapStep::Working(j) => {
                        job = borsh::from_slice(&borsh::to_vec(&j).unwrap()).unwrap()
                    } // survives storage
                    MapStep::Done(g) => break g,
                }
            };
            assert_eq!(stepped, whole);
            assert!(steps > 3, "work was actually split ({steps} steps)");
        }
    }

    #[test]
    fn arithmetic_index_matches_every_tile() {
        for (preset, civs) in [(Preset::Blitz, 6), (Preset::Season, 16)] {
            let rules = Ruleset::new(preset);
            let g = generate(&rules, &[5u8; 32], civs).expect("map");
            for (i, t) in g.map.tiles.iter().enumerate() {
                assert_eq!(g.map.index_of(t.hex), Some(i), "{:?}", t.hex);
            }
            let r = g.map.radius as i32;
            for h in [
                Hex::new(r + 1, 0),
                Hex::new(0, -r - 1),
                Hex::new(r, 1),
                Hex::new(-r, -1),
            ] {
                assert_eq!(g.map.index_of(h), None, "{h:?} is off the map");
            }
            let c = g.starts[0];
            let near = g.map.indices_within(c, 3);
            let brute: Vec<usize> = (0..g.map.tiles.len())
                .filter(|&i| g.map.tiles[i].hex.distance(c) <= 3)
                .collect();
            assert_eq!(near, brute);
        }
    }

    #[test]
    fn blitz_map_generates_fair_starts() {
        let rules = Ruleset::new(Preset::Blitz);
        let g = generate(&rules, &[3u8; 32], 6).expect("6-civ Blitz map");
        assert_eq!(g.map.tiles.len(), 547);
        assert_eq!(g.starts.len(), 6);
        for (i, a) in g.starts.iter().enumerate() {
            for b in &g.starts[i + 1..] {
                assert!(a.distance(*b) >= rules.start_min_distance as u32);
            }
        }
        let v: Vec<u64> = g.starts.iter().map(|s| start_value(&g.map, *s)).collect();
        let (mn, mx) = (*v.iter().min().unwrap(), *v.iter().max().unwrap());
        assert!(mx * 10_000 / mn <= 11_000);
        assert_eq!(g.city_states.len(), 2);
        assert_eq!(g.hubs.len(), 2);
    }

    #[test]
    fn every_supported_civ_count_generates_fair_spaced_starts() {
        let cases = [
            (Preset::Blitz, [2usize, 4, 6, 8]),
            (Preset::Season, [9, 12, 14, 16]),
        ];
        for (preset, counts) in cases {
            let rules = Ruleset::new(preset);
            for civs in counts {
                for seed in 0..3u8 {
                    let g = generate(&rules, &[seed; 32], civs)
                        .unwrap_or_else(|e| panic!("{preset:?} {civs} civs seed {seed}: {e:?}"));
                    assert_eq!(g.starts.len(), civs);
                    for (i, a) in g.starts.iter().enumerate() {
                        assert!(valid_start(&g.map, *a));
                        for b in &g.starts[i + 1..] {
                            assert!(a.distance(*b) >= rules.start_min_distance as u32);
                        }
                    }
                    let v: Vec<u64> = g.starts.iter().map(|s| start_value(&g.map, *s)).collect();
                    let (mn, mx) = (*v.iter().min().unwrap(), *v.iter().max().unwrap());
                    assert!(mx * 10_000 / mn <= rules.start_fairness_max_bps as u64);
                }
            }
        }
    }

    #[test]
    fn generation_is_deterministic() {
        let rules = Ruleset::new(Preset::Blitz);
        let a = generate(&rules, &[9u8; 32], 4).unwrap();
        let b = generate(&rules, &[9u8; 32], 4).unwrap();
        assert_eq!(a.map, b.map);
        assert_eq!(a.starts, b.starts);
    }
}
