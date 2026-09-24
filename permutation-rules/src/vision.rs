//! Vision and fog of war (§7.3, §7.4).
//!
//! The rules resolve on the full state; every *player* — human or agent —
//! acts on a **belief state**: what its civilization (and its allies) can see
//! now, plus what it remembers from earlier ticks. One function builds that
//! view for everyone, so an agent never sees more than a human (V4 §5).
//!
//! * Terrain is public: it is derived from the published world seed.
//! * Units are seen only while inside vision.
//! * Cities, borders and ruins are remembered as last seen ("last known").
//! * Other civilizations' stockpiles, research and queues are never part of
//!   the belief state; clients must not display them.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use borsh::{BorshDeserialize, BorshSerialize};

use crate::hex::Hex;
use crate::map::Terrain;
use crate::state::{City, CityId, CivId, LastYields, Owner, Relation, WorldState};
use crate::tech::TechSet;
use crate::units::stats;

/// Vision radius of a city (§7.3).
pub const CITY_VISION: u32 = 2;

/// Hexes strictly between `a` and `b` on the hex line (§7.4). Integer-only:
/// cube lerp scaled by `1000·n`, nudged by (+1, +2, −3) so ties always round
/// the same way, then cube-rounded.
pub fn between(a: Hex, b: Hex) -> Vec<Hex> {
    let n = a.distance(b) as i64;
    if n < 2 {
        return Vec::new();
    }
    let d = n * 1000;
    let (aq, ar, as_) = (a.q as i64, a.r as i64, a.s() as i64);
    let (bq, br, bs) = (b.q as i64, b.r as i64, b.s() as i64);
    let round = |v: i64| (2 * v + d).div_euclid(2 * d);
    (1..n)
        .map(|i| {
            let vq = (aq * n + (bq - aq) * i) * 1000 + 1;
            let vr = (ar * n + (br - ar) * i) * 1000 + 2;
            let vs = (as_ * n + (bs - as_) * i) * 1000 - 3;
            let (mut q, mut r, s) = (round(vq), round(vr), round(vs));
            let (dq, dr, ds) = ((vq - q * d).abs(), (vr - r * d).abs(), (vs - s * d).abs());
            if dq > dr && dq > ds {
                q = -r - s;
            } else if dr > ds {
                r = -q - s;
            }
            Hex::new(q as i32, r as i32)
        })
        .collect()
}

/// Whether a viewer at `from` with `radius` sees `to`: in range, and no
/// Mountain strictly between them. A Mountain itself can be seen.
pub fn sees(state: &WorldState, from: Hex, radius: u32, to: Hex) -> bool {
    from.distance(to) <= radius
        && between(from, to)
            .into_iter()
            .all(|h| state.map.tile(h).is_none_or(|t| t.terrain != Terrain::Mountain))
}

/// Civilizations whose vision `civ` shares: itself and its allies (§10.4).
pub fn sharers(state: &WorldState, civ: CivId) -> Vec<CivId> {
    (0..state.civs.len() as CivId)
        .filter(|o| *o == civ || matches!(state.relation(civ, *o), Relation::Alliance { .. }))
        .collect()
}

/// Tiles `civ` sees this tick, indexed like `state.map.tiles`.
pub fn visible(state: &WorldState, civ: CivId) -> Vec<bool> {
    let team = sharers(state, civ);
    let mut sources: Vec<(Hex, u32)> = Vec::new();
    let bonus = |h: Hex| state.map.tile(h).map_or(0, |t| t.terrain.info().vision_bonus as u32);
    for u in state.units.iter().filter(|u| u.alive) {
        if matches!(u.owner, Owner::Civ(o) if team.contains(&o)) {
            sources.push((u.hex, stats(u.unit_type).vision as u32 + bonus(u.hex)));
        }
    }
    for c in state.cities.iter().filter(|c| c.alive) {
        if c.owner.is_some_and(|o| team.contains(&o)) {
            sources.push((c.hex, CITY_VISION + bonus(c.hex)));
        }
    }
    let mut seen = vec![false; state.map.tiles.len()];
    for (i, t) in state.map.tiles.iter().enumerate() {
        seen[i] = sources.iter().any(|(h, r)| sees(state, *h, *r, t.hex));
    }
    seen
}

/// What one civilization remembers (§7.4). Kept per civ by whoever serves
/// views; it is derived from past states, so it is not part of the world
/// state or its hash.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Memory {
    /// Tile index → ever seen.
    pub explored: Vec<bool>,
    /// Tile index → owning city when last seen.
    pub owner_city: Vec<Option<u32>>,
    /// Tile index → ruin peak population when last seen.
    pub ruin: Vec<Option<u16>>,
    /// Foreign city → (tick last seen, snapshot).
    pub cities: BTreeMap<CityId, (u16, City)>,
}

impl Memory {
    pub fn new(state: &WorldState) -> Memory {
        let n = state.map.tiles.len();
        Memory { explored: vec![false; n], owner_city: vec![None; n], ruin: vec![None; n], cities: BTreeMap::new() }
    }

    /// Record everything in `seen` (from [`visible`]) as of `state.tick`.
    pub fn update(&mut self, state: &WorldState, civ: CivId, seen: &[bool]) {
        let team = sharers(state, civ);
        for (i, t) in state.map.tiles.iter().enumerate() {
            if !seen[i] {
                continue;
            }
            self.explored[i] = true;
            self.owner_city[i] = t.owner_city;
            self.ruin[i] = t.ruin_peak_pop;
        }
        for c in &state.cities {
            let Some(i) = state.map.index_of(c.hex) else { continue };
            if !seen[i] {
                continue;
            }
            if c.alive && !c.owner.is_some_and(|o| team.contains(&o)) {
                self.cities.insert(c.id, (state.tick, c.clone()));
            } else {
                self.cities.remove(&c.id);
            }
        }
    }

    /// Tick at which a remembered foreign city was last seen.
    pub fn city_seen(&self, id: CityId) -> Option<u16> {
        self.cities.get(&id).map(|(t, _)| *t)
    }
}

/// The world as `civ` believes it to be: the input for every client and
/// agent decision (§7.4). Own and allied objects are live; foreign units
/// outside vision are removed; foreign cities and borders outside vision
/// are their last-seen snapshots, or absent if never seen.
pub fn belief(state: &WorldState, civ: CivId, seen: &[bool], memory: &Memory) -> WorldState {
    let team = sharers(state, civ);
    let mut b = state.clone();
    let hidden = |h: Hex| state.map.index_of(h).is_none_or(|i| !seen[i]);
    for (i, t) in b.map.tiles.iter_mut().enumerate() {
        if !seen[i] {
            t.owner_city = if memory.explored[i] { memory.owner_city[i] } else { None };
            t.ruin_peak_pop = if memory.explored[i] { memory.ruin[i] } else { None };
        }
    }
    for c in b.cities.iter_mut() {
        if c.owner.is_some_and(|o| team.contains(&o)) || !hidden(c.hex) {
            continue;
        }
        match memory.cities.get(&c.id) {
            Some((_, snap)) => *c = snap.clone(),
            None => c.alive = false,
        }
    }
    for u in b.units.iter_mut() {
        let ours = matches!(u.owner, Owner::Civ(o) if team.contains(&o));
        if !ours && hidden(u.hex) {
            u.alive = false;
        }
    }
    // Treasuries, income, research, order bank and foreign queues are private,
    // even to allies (§7.4).
    for o in b.civs.iter_mut().filter(|o| o.id != civ) {
        o.gold = 0;
        o.science_store = 0;
        o.influence = 0;
        o.iron = 0;
        o.horses = 0;
        o.techs = TechSet::default();
        o.research_queue.clear();
        o.usdc = 0;
        o.market_spent = 0;
        o.exchange_bought = [0; 5];
        o.last = LastYields::default();
        o.deficit = false;
        o.troops_lost = 0;
        o.war_weariness = 0;
    }
    // Other nations' order banks and proposals are theirs (V5).
    for (i, n) in b.nations.iter_mut().enumerate() {
        if i != civ as usize {
            n.role_bank = [0; 4];
            n.proposals.clear();
        }
    }
    for c in b.cities.iter_mut().filter(|c| c.owner != Some(civ)) {
        c.queue.clear();
        c.food = 0;
        c.prod = 0;
    }
    b
}
