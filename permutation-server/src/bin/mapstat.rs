//! Map measurement: generate many Blitz maps without playing them and report
//! shape, connectivity and per-start-slot geography.
//!
//!     cargo run --release --bin mapstat -- [maps=200] [civs=6]
//!
//! Uses the same seeds as `sim` (the map seed mixes the world and season
//! seeds, §2.4), so slot `c` here is generation slot `c` there. Generation
//! is stepped with `work = 50` as the chain's genesis does, to count its
//! transactions.

use permutation_rules::genesis::map_seed;
use permutation_rules::hex::Hex;
use permutation_rules::map::{start_value, Generated, Map, MapJob, MapStep};
use permutation_rules::rng::Seed;
use permutation_rules::{Preset, Ruleset};
use std::collections::VecDeque;

fn seed_bytes(tag: &str, i: u32) -> Seed {
    let mut s = [0u8; 32];
    let t = format!("permutation-state/{tag}/sim-{i:04}");
    s[..t.len().min(32)].copy_from_slice(&t.as_bytes()[..t.len().min(32)]);
    s
}

/// Stepped generation: the result, the attempt it succeeded on, and steps.
fn generate(rules: &Ruleset, seed: &Seed, civs: usize) -> (Generated, u64, u32) {
    let mut job = MapJob::new(rules, civs).expect("civs");
    let mut steps = 0;
    loop {
        steps += 1;
        let attempt = job.attempt;
        match job.step(rules, seed, 50).expect("generation") {
            MapStep::Working(j) => job = j,
            MapStep::Done(g) => return (g, attempt, steps),
        }
    }
}

fn passable(map: &Map, h: Hex) -> bool {
    map.tile(h).is_some_and(|t| t.terrain.is_passable())
}

/// BFS distances over passable tiles from `from` (u32::MAX = unreachable).
fn bfs(map: &Map, from: Hex) -> Vec<u32> {
    let mut d = vec![u32::MAX; map.tiles.len()];
    let Some(s) = map.index_of(from) else {
        return d;
    };
    d[s] = 0;
    let mut q = VecDeque::from([from]);
    while let Some(h) = q.pop_front() {
        let dh = d[map.index_of(h).unwrap()];
        for n in h.neighbors() {
            if let Some(i) = map.index_of(n) {
                if d[i] == u32::MAX && passable(map, n) {
                    d[i] = dh + 1;
                    q.push_back(n);
                }
            }
        }
    }
    d
}

#[derive(Default, Clone)]
struct Slot {
    radius: f64,
    voronoi: f64,
    value: f64,
    nearest_start: f64,
    nearest_site: f64,
    n: u32,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let maps: u32 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(200);
    let civs: usize = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(6);
    let rules = Ruleset::new(Preset::Blitz);
    let mut slots = vec![Slot::default(); civs];
    let (mut same, mut pairs) = (0u64, 0u64);
    let (mut all_connected, mut attempts, mut steps) = (0u32, 0u64, 0u64);
    let mut max_steps = 0u32;
    let mut detours: Vec<f64> = Vec::new();
    let mut value_spread: Vec<f64> = Vec::new();
    let (mut river_chains, mut river_tiles) = (0u64, 0u64);
    for i in 0..maps {
        let seed = map_seed(&seed_bytes("world", i), &seed_bytes("season", i));
        let (g, attempt, st) = generate(&rules, &seed, civs);
        attempts += attempt;
        steps += st as u64;
        max_steps = max_steps.max(st);
        let map = &g.map;
        // Terrain coherence: share of adjacent tile pairs with equal terrain.
        for t in &map.tiles {
            for n in t.hex.neighbors() {
                if let Some(o) = map.tile(n) {
                    if (o.hex.q, o.hex.r) > (t.hex.q, t.hex.r) {
                        pairs += 1;
                        same += (o.terrain == t.terrain) as u64;
                    }
                }
            }
        }
        // Rivers: connected groups of river tiles.
        let mut seen = vec![false; map.tiles.len()];
        for (k, t) in map.tiles.iter().enumerate() {
            if !t.river || seen[k] {
                continue;
            }
            river_chains += 1;
            let mut q = VecDeque::from([t.hex]);
            seen[k] = true;
            while let Some(h) = q.pop_front() {
                river_tiles += 1;
                for n in h.neighbors() {
                    if let Some(j) = map.index_of(n) {
                        if !seen[j] && map.tiles[j].river {
                            seen[j] = true;
                            q.push_back(n);
                        }
                    }
                }
            }
        }
        // Connectivity and detours between starts.
        let dist: Vec<Vec<u32>> = g.starts.iter().map(|s| bfs(map, *s)).collect();
        let connected = g.starts.iter().all(|s| {
            let k = map.index_of(*s).unwrap();
            dist.iter().all(|d| d[k] != u32::MAX)
        }) && g
            .city_states
            .iter()
            .chain(&g.hubs)
            .all(|h| dist[0][map.index_of(*h).unwrap()] != u32::MAX);
        all_connected += connected as u32;
        for (a, from) in dist.iter().enumerate().take(civs) {
            for b in a + 1..civs {
                let path = from[map.index_of(g.starts[b]).unwrap()];
                if path != u32::MAX {
                    detours.push(path as f64 / g.starts[a].distance(g.starts[b]) as f64);
                }
            }
        }
        // Per-slot geography.
        let values: Vec<u64> = g.starts.iter().map(|s| start_value(map, *s)).collect();
        let (lo, hi) = (*values.iter().min().unwrap(), *values.iter().max().unwrap());
        value_spread.push(hi as f64 / lo.max(1) as f64);
        let mut voronoi = vec![0u32; civs];
        for t in map.tiles.iter().filter(|t| t.terrain.is_passable()) {
            let d: Vec<u32> = g.starts.iter().map(|s| s.distance(t.hex)).collect();
            let m = *d.iter().min().unwrap();
            let owners: Vec<usize> = (0..civs).filter(|c| d[*c] == m).collect();
            if owners.len() == 1 {
                voronoi[owners[0]] += 1;
            }
        }
        for (c, sl) in slots.iter_mut().enumerate() {
            let s = g.starts[c];
            sl.radius += s.radius() as f64;
            sl.voronoi += voronoi[c] as f64;
            sl.value += values[c] as f64;
            sl.nearest_start += (0..civs)
                .filter(|o| *o != c)
                .map(|o| s.distance(g.starts[o]))
                .min()
                .unwrap_or(0) as f64;
            sl.nearest_site += g
                .city_states
                .iter()
                .chain(&g.hubs)
                .map(|h| s.distance(*h))
                .min()
                .unwrap_or(0) as f64;
            sl.n += 1;
        }
    }
    let m = maps.max(1) as f64;
    detours.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |p: f64| {
        detours
            .get(((detours.len() as f64 - 1.0) * p) as usize)
            .copied()
            .unwrap_or(0.0)
    };
    println!("== {maps} Blitz maps, {civs} civs ==");
    println!(
        "same-terrain neighbour share: {:.3} (independent per-tile roll ≈ 0.21; target ≥ 0.45)",
        same as f64 / pairs.max(1) as f64
    );
    println!("starts, city-states and hubs all mutually reachable: {all_connected}/{maps}");
    println!(
        "start-to-start detour (path ÷ hex distance): median {:.2}, p90 {:.2}, max {:.2}",
        q(0.5),
        q(0.9),
        q(1.0)
    );
    println!(
        "generation: mean failed attempts {:.2}, mean steps {:.1} (work=50), max steps {max_steps}",
        attempts as f64 / m,
        steps as f64 / m
    );
    value_spread.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!(
        "start value max/min: median {:.3}, max {:.3} (bound 1.100)",
        value_spread[value_spread.len() / 2],
        value_spread.last().unwrap()
    );
    println!(
        "rivers: {:.1} connected groups per map, mean group size {:.2} tiles",
        river_chains as f64 / m,
        river_tiles as f64 / river_chains.max(1) as f64
    );
    println!("slot | ring (0 = centre) | own tiles (Voronoi) | start value | nearest start | nearest city-state/hub");
    for (c, s) in slots.iter().enumerate() {
        let n = s.n.max(1) as f64;
        println!(
            "{c:4} | {:17.1} | {:19.1} | {:11.1} | {:13.1} | {:.1}",
            s.radius / n,
            s.voronoi / n,
            s.value / n,
            s.nearest_start / n,
            s.nearest_site / n
        );
    }
}
