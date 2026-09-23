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
    pub fn index_of(&self, hex: Hex) -> Option<usize> {
        if hex.radius() > self.radius as u32 {
            return None;
        }
        self.tiles.binary_search_by(|t| t.hex.cmp(&hex)).ok()
    }
    pub fn tile(&self, hex: Hex) -> Option<&Tile> {
        self.index_of(hex).map(|i| &self.tiles[i])
    }
    pub fn tile_mut(&mut self, hex: Hex) -> Option<&mut Tile> {
        self.index_of(hex).map(move |i| &mut self.tiles[i])
    }

    /// Claim unowned tiles within `radius` of `center` for `city` (§5.4):
    /// ascending distance, then tile index. Owned tiles are never taken.
    pub fn claim_territory(&mut self, city: u32, center: Hex, radius: u32) {
        let mut targets: Vec<(u32, usize)> = self
            .tiles
            .iter()
            .enumerate()
            .filter(|(_, t)| t.owner_city.is_none() && t.hex.distance(center) <= radius)
            .map(|(i, t)| (t.hex.distance(center), i))
            .collect();
        targets.sort();
        for (_, i) in targets {
            self.tiles[i].owner_city = Some(city);
        }
    }
}

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
pub struct Generated {
    pub map: Map,
    pub starts: Vec<Hex>,
    pub city_states: Vec<Hex>,
    pub hubs: Vec<Hex>,
}

/// Deterministic generation (§2.4). Rerolls (new `attempt` in the domain)
/// until starts are within the fairness bound or attempts run out.
pub fn generate(rules: &Ruleset, world_seed: &Seed, civs: usize) -> Result<Generated, RulesError> {
    if civs == 0 || civs > rules.max_civs as usize {
        return Err(RulesError::MapGeneration("civilization count out of range"));
    }
    for attempt in 0..rules.start_generation_attempts as u64 {
        let map = generate_terrain(rules, world_seed, attempt);
        let Some(starts) = place_starts(rules, &map, world_seed, attempt, civs) else {
            continue;
        };
        let values: Vec<u64> = starts.iter().map(|s| start_value(&map, *s)).collect();
        let (min, max) = (
            *values.iter().min().unwrap_or(&0),
            *values.iter().max().unwrap_or(&0),
        );
        if min == 0 || max * 10_000 / min > rules.start_fairness_max_bps as u64 {
            continue;
        }
        let city_states = place_sites(
            &map,
            world_seed,
            attempt,
            b"citystate",
            (civs / 3).max(2),
            &starts,
            6,
            true,
        );
        let hubs = place_sites(
            &map,
            world_seed,
            attempt,
            b"hub",
            (civs / 4).max(2),
            &starts,
            5,
            false,
        );
        if let (Some(city_states), Some(hubs)) = (city_states, hubs) {
            return Ok(Generated {
                map,
                starts,
                city_states,
                hubs,
            });
        }
    }
    Err(RulesError::MapGeneration(
        "no fair start layout within attempt limit",
    ))
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
        map.tiles
            .iter()
            .filter(move |t| t.hex.distance(hex) <= r && t.hex != hex)
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

fn place_starts(
    rules: &Ruleset,
    map: &Map,
    seed: &Seed,
    attempt: u64,
    civs: usize,
) -> Option<Vec<Hex>> {
    let s = attempt_seed(seed, attempt);
    let mut candidates: Vec<(u64, Hex)> = map
        .tiles
        .iter()
        .enumerate()
        .filter(|(_, t)| t.hex.radius() + 2 <= map.radius as u32)
        .map(|(i, t)| (rand_id(&s, b"start", i as u64), t.hex))
        .collect();
    candidates.sort();
    let mut starts: Vec<Hex> = Vec::new();
    for (_, hex) in candidates {
        if starts.len() == civs {
            break;
        }
        if starts
            .iter()
            .all(|s| s.distance(hex) >= rules.start_min_distance as u32)
            && valid_start(map, hex)
        {
            starts.push(hex);
        }
    }
    (starts.len() == civs).then_some(starts)
}

/// `V = Σ radius-3 (2·food + 2·prod + gold) + 6 × strategic within 5` (§2.4).
pub fn start_value(map: &Map, hex: Hex) -> u64 {
    let mut v = 0u64;
    for t in &map.tiles {
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
    fn generation_is_deterministic() {
        let rules = Ruleset::new(Preset::Blitz);
        let a = generate(&rules, &[9u8; 32], 4).unwrap();
        let b = generate(&rules, &[9u8; 32], 4).unwrap();
        assert_eq!(a.map, b.map);
        assert_eq!(a.starts, b.starts);
    }
}
