//! The clash at the bell (design §6.3, §6.4).
//!
//! Everything that arrives at a province in bell b resolves together
//! against the province's **roster frozen at the start of b** (hosts and
//! garrisons whose `host::Presence` covers b). [`resolve_clash`] is a pure
//! function of that roster, the bell's revealed arrivals and postures, the
//! diplomatic relations and the bell seed `S(b, r)`. Nothing in it depends
//! on when the reveals or the resolve landed, or on the order of any list
//! it is given, so lag only waits and never changes an outcome (§8.4).
//!
//! Steps:
//! 1. **Retreat orders.** An arrival with a `retreat_ratio` r withdraws
//!    without fighting if the frozen hostile strength on its target hex
//!    exceeds r × its own. (Evaluated against the frozen roster only, so it
//!    is done first and frees its slot.)
//! 2. **Merge** by the province caps (≤ 48 hosts, ≤ 8 per faction) and the
//!    hex fair-share rule, allocated by **side**, not by faction (allied
//!    factions cannot pool slots against a third, §6.1): the factions on a
//!    hex fall into sides, the connected groups of factions that are not
//!    hostile to each other. Each side is guaranteed
//!    `min(its hosts, ⌊6 / sides⌋)` slots and the rest go by mass. On a
//!    holding's hex the sides hostile to the owner together get at most 3
//!    (shared by the same rule) and the owner's side the rest, the owner's
//!    own hosts first. Ties go by `tie_key(seed, host_id)`. Hosts that find
//!    no room bounce home with no loss.
//! 3. **Engagements.** On every hex each hostile pair of combatants fights
//!    one `combat::resolve_engagement`, reused unchanged **with both of its
//!    halves** (the attack and the defender's retaliation, with v9's
//!    retaliation modifiers: a garrison fighting as `Combatant::City`
//!    retaliates at ×0.5 and never attacks, a ranged defender retaliates at
//!    ×0.5), **from pre-clash counts**. The arrival attacks a resident; a
//!    garrison is always the defender; two arrivals or two residents
//!    engage both ways at half weight each. Every clash engagement is on
//!    one hex, so none is a ranged attack (`RANGED_ATTACK` applies to v9's
//!    attacks from range only). What a combatant deals in each of its
//!    engagements is divided by its number of engagements on the hex, then
//!    scaled by the stance table (`stance`). The variance dice of every
//!    engagement are drawn from the bell seed.
//! 4. **All damage applies at once.** Hosts below 0.5 troops are destroyed.
//! 5. **One side holds the field;** hosts hostile to it withdraw to an
//!    adjacent friendly hex of the province, or bounce home.
//! 6. **Damage-ratio refund:** a faction whose damage ratio on the hex is
//!    ≥ 10 pays no engagement stamina.
//!
//! **Inputs as of the bell's start.** Everything the clash of bell b reads
//! is its value at the start of b, whenever the clash is resolved: the
//! roster (`host::Presence`), the hosts' and garrisons' troops and stamina
//! (`host::Host::values_at`, `host::GarrisonState::at`), walls
//! (`holding::Holding::walls_at`) and relations ([`RelationsLog::at`]).
//!
//! **Quiet bells.** A bell with no arrivals in which resolving would change
//! nothing ([`is_quiet`]: no hostile combatants share a hex and no hex is
//! over its fair share) needs no resolution: skipping it gives the same
//! state, and sieges count a run of quiet bells in closed form
//! (`siege::Siege::advance_quiet`). Hostile residents sharing a hex (for
//! example besiegers and a garrison that still has troops) fight every
//! bell, so such a bell is never quiet.

use super::geometry::{tile_index, tile_offset, ProvinceCoord, PROVINCE_TILES};
use super::host::{
    city_strength, strength, BATTLE_COOLDOWN_BELLS, DESTROYED_BELOW, ENGAGE_STAMINA,
    FACTION_RESIDENT_CAP, HEX_HOST_CAP, OWNER_HEX_SLOTS, PROVINCE_HOST_CAP,
};
use super::stance::{damage_bps, Posture};
use super::terrain::ProvinceTerrain;
use crate::combat::{resolve_engagement, variance, Combatant, Situation};
use crate::fixed::{Bps, MilliTroops, BPS_ONE};
use crate::hash::{sha256, Digest32};
use crate::hex::Hex;
use crate::params::{Preset, Ruleset};
use crate::rng::{rand, rand_id, tie_key, Seed};
use crate::units::UnitType;
use alloc::vec;
use alloc::vec::Vec;
use borsh::{BorshDeserialize, BorshSerialize};

/// Arrival slots per (province, bell): 4 per faction.
pub const MAX_ARRIVALS: usize = 24;
/// Holdings (sites) per province.
pub const MAX_GARRISONS: usize = 12;
/// Barbarians and Free Cities: hostile to everyone, never at peace.
pub const NEUTRAL: u8 = 6;
/// Faction ids are below this.
pub const FACTION_LIMIT: u8 = 8;
/// A side whose damage ratio reaches this pays no stamina.
pub const REFUND_RATIO: u64 = 10;

/// The combat constants the clash reuses from `combat` (identical in
/// every preset).
pub fn frontier_ruleset() -> Ruleset {
    Ruleset::new(Preset::Season)
}

/// Who is hostile to whom this bell: a symmetric matrix over factions
/// 0..8; a set bit means "not hostile" (Peace, NAP, Alliance). Different
/// factions are hostile by default (Rivalry, War); `NEUTRAL` always is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Relations {
    pub peaceful: u64,
}

impl Relations {
    pub const ALL_HOSTILE: Relations = Relations { peaceful: 0 };

    const fn bit(a: u8, b: u8) -> u64 {
        1 << ((a as u32 % 8) * 8 + b as u32 % 8)
    }

    pub const fn hostile(self, a: u8, b: u8) -> bool {
        if a == b {
            return false;
        }
        if a == NEUTRAL || b == NEUTRAL {
            return true;
        }
        self.peaceful & Self::bit(a, b) == 0
    }

    /// Mark a pair peaceful (or hostile again). `NEUTRAL` cannot be.
    pub fn set_peaceful(&mut self, a: u8, b: u8, peaceful: bool) {
        if a == b || a == NEUTRAL || b == NEUTRAL || a >= FACTION_LIMIT || b >= FACTION_LIMIT {
            return;
        }
        let m = Self::bit(a, b) | Self::bit(b, a);
        if peaceful {
            self.peaceful |= m;
        } else {
            self.peaceful &= !m;
        }
    }
}

/// A Peace, NAP or Alliance decree issued during bell c takes effect at the
/// start of c + 1 …
pub const PEACE_LEAD_BELLS: u32 = 1;
/// … and a declaration of War (or breaking a treaty) after its 36-bell horn.
pub const WAR_HORN_BELLS: u32 = 36;

/// One diplomatic decree between two factions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Decree {
    /// First bell whose clash sees it.
    pub from_bell: u32,
    /// Issue order (ties at one `from_bell`: the later decree wins).
    pub seq: u64,
    pub a: u8,
    pub b: u8,
    pub peaceful: bool,
}

/// Relations by bell: the clash of bell b reads `at(b)`, the matrix in
/// force at the start of b, however late it is resolved (design §6.3,
/// §8.4). Decrees are kept until every province has resolved past them
/// (`prune`); the program stores them per change, like the bell anchors.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RelationsLog {
    /// Relations in force before every kept decree.
    pub base: Relations,
    pub decrees: Vec<Decree>,
    next_seq: u64,
}

impl RelationsLog {
    pub fn new(base: Relations) -> RelationsLog {
        RelationsLog {
            base,
            decrees: Vec::new(),
            next_seq: 0,
        }
    }

    /// A decree issued during bell `issued`; returns the bell it takes
    /// effect at (`issued + 1` for peace, `issued + 36` for hostility).
    pub fn decree(&mut self, issued: u32, a: u8, b: u8, peaceful: bool) -> u32 {
        let lead = if peaceful {
            PEACE_LEAD_BELLS
        } else {
            WAR_HORN_BELLS
        };
        let from_bell = issued.saturating_add(lead);
        self.decrees.push(Decree {
            from_bell,
            seq: self.next_seq,
            a,
            b,
            peaceful,
        });
        self.next_seq += 1;
        from_bell
    }

    /// Relations in force at the start of bell `bell`.
    pub fn at(&self, bell: u32) -> Relations {
        let mut ds: Vec<&Decree> = self
            .decrees
            .iter()
            .filter(|d| d.from_bell <= bell)
            .collect();
        ds.sort_by_key(|d| (d.from_bell, d.seq));
        let mut r = self.base;
        for d in ds {
            r.set_peaceful(d.a, d.b, d.peaceful);
        }
        r
    }

    /// Fold every decree in force by bell `bell` into the base, once every
    /// province has resolved through `bell − 1` (no clash reads older).
    pub fn prune(&mut self, bell: u32) {
        self.base = self.at(bell);
        self.decrees.retain(|d| d.from_bell > bell);
    }
}

/// A host in the clash: a resident of the frozen roster, or an arrival.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Fighter {
    pub id: u64,
    pub faction: u8,
    pub unit: UnitType,
    pub troops: MilliTroops,
    /// Stamina at this bell (residents: refilled to it; arrivals: after
    /// the march).
    pub stamina: u16,
    /// Tile index in the province (arrivals: the revealed target hex).
    pub tile: u8,
    pub posture: Posture,
    /// Arrivals only: withdraw without fighting if the frozen hostile
    /// strength on the target hex exceeds this share (bps) of its own.
    pub retreat_bps: Option<Bps>,
    /// Doctrine hook on damage dealt (`BPS_ONE` = none).
    pub dealt_bps: Bps,
}

/// A holding's garrison and walls, fighting as a virtual host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Garrison {
    /// The holding's id.
    pub id: u64,
    pub faction: u8,
    pub tile: u8,
    pub troops: MilliTroops,
    pub walls: bool,
    pub posture: Posture,
}

/// Everything one clash reads.
#[derive(Clone, Copy, Debug)]
pub struct ClashInput<'a> {
    pub province: ProvinceCoord,
    pub bell: u32,
    /// The bell seed `S(b, r)` of the province's region (drand round after
    /// the region's reveal close, §8.5).
    pub seed: Seed,
    pub terrain: &'a ProvinceTerrain,
    /// The roster frozen at the start of `bell`.
    pub residents: &'a [Fighter],
    pub garrisons: &'a [Garrison],
    /// Revealed arrivals of `bell` (unrevealed ones were routed and are
    /// not here).
    pub arrivals: &'a [Fighter],
    pub relations: Relations,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClashError {
    TooManyResidents,
    TooManyArrivals,
    TooManyGarrisons,
    BadTile(u64),
    BadFaction(u64),
    DuplicateId(u64),
    /// Two garrisons on one tile.
    SharedTile(u8),
    /// An arrival's stance comes from its revealed commitment.
    ArrivalWithoutStance(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Fate {
    /// Holds (or shares) the field on `tile`; arrivals join the roster
    /// from the next bell.
    Stays {
        tile: u8,
    },
    /// Lost the field and fell back to an adjacent friendly hex.
    Withdrew {
        tile: u8,
    },
    /// No room, or lost the field with nowhere to fall back: home, no loss.
    Bounced,
    /// Its `retreat_ratio` fired: home without fighting, no loss.
    Retreated,
    Destroyed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FighterResult {
    pub id: u64,
    pub arrival: bool,
    pub troops: MilliTroops,
    pub stamina: u16,
    pub fate: Fate,
    /// Fought this bell: may not depart before [`ready_bell_after`].
    pub engaged: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct GarrisonResult {
    pub id: u64,
    pub troops: MilliTroops,
    /// Hosts hostile to the owner hold the holding's hex (siege input).
    pub attackers_hold: bool,
    /// Bit f set: faction f, hostile to the owner, holds the hex with a
    /// host of its own (a siege advances only while its declarer does).
    pub holders: u8,
    /// A host of the owner or a non-hostile faction stands on the hex.
    pub defender_present: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ClashOutcome {
    pub province: ProvinceCoord,
    pub bell: u32,
    /// Sorted by id.
    pub fighters: Vec<FighterResult>,
    /// Sorted by id.
    pub garrisons: Vec<GarrisonResult>,
    pub engagements: u32,
}

impl ClashOutcome {
    /// Hash of the whole outcome (event chains, property tests).
    pub fn digest(&self) -> Digest32 {
        let bytes = borsh::to_vec(self).unwrap_or_default();
        sha256(&[b"frontier/clash-outcome", &bytes])
    }

    pub fn fighter(&self, id: u64) -> Option<&FighterResult> {
        self.fighters
            .binary_search_by_key(&id, |f| f.id)
            .ok()
            .map(|i| &self.fighters[i])
    }
}

/// First bell a host that fought at `bell` may depart.
pub const fn ready_bell_after(bell: u32) -> u32 {
    bell + 1 + BATTLE_COOLDOWN_BELLS
}

/// The clash's own seed: `sha256("frontier/clash" ‖ S ‖ P ‖ Q ‖ b)`.
pub fn clash_seed(seed: &Seed, p: ProvinceCoord, bell: u32) -> Seed {
    sha256(&[
        b"frontier/clash",
        seed,
        &p.p.to_le_bytes(),
        &p.q.to_le_bytes(),
        &bell.to_le_bytes(),
    ])
}

// ------------------------------------------------------------ internals

#[derive(Clone, Copy, PartialEq, Eq)]
enum St {
    In,
    Out(Fate),
}

struct Unit {
    id: u64,
    city: bool,
    faction: u8,
    unit: Option<UnitType>,
    troops: MilliTroops,
    stamina: u16,
    tile: u8,
    posture: Posture,
    arrival: bool,
    retreat_bps: Option<Bps>,
    dealt_bps: Bps,
    walls: bool,
    tie: u64,
    st: St,
}

impl Unit {
    fn strength(&self) -> u64 {
        match self.unit {
            Some(u) => strength(u, self.troops),
            None => city_strength(self.troops),
        }
    }
    fn combatant(&self) -> Combatant {
        match self.unit {
            Some(unit) => Combatant::Army {
                unit,
                troops: self.troops,
            },
            None => Combatant::City {
                defense: self.troops,
            },
        }
    }
    fn fights(&self) -> bool {
        self.troops > 0 && self.unit.is_none_or(|u| !u.is_civilian())
    }
    fn key(&self) -> [u8; 9] {
        let mut k = [0u8; 9];
        k[0] = self.city as u8;
        k[1..].copy_from_slice(&self.id.to_le_bytes());
        k
    }
    /// Mass order: more troops first, then the lower tie key, then id.
    fn order(&self) -> (core::cmp::Reverse<MilliTroops>, u64, u64) {
        (core::cmp::Reverse(self.troops), self.tie, self.id)
    }
}

fn validate(inp: &ClashInput) -> Result<(), ClashError> {
    if inp.residents.len() > PROVINCE_HOST_CAP {
        return Err(ClashError::TooManyResidents);
    }
    if inp.arrivals.len() > MAX_ARRIVALS {
        return Err(ClashError::TooManyArrivals);
    }
    if inp.garrisons.len() > MAX_GARRISONS {
        return Err(ClashError::TooManyGarrisons);
    }
    let mut ids: Vec<u64> = Vec::with_capacity(inp.residents.len() + inp.arrivals.len());
    for f in inp.residents.iter().chain(inp.arrivals.iter()) {
        if f.tile as usize >= PROVINCE_TILES {
            return Err(ClashError::BadTile(f.id));
        }
        if f.faction >= FACTION_LIMIT {
            return Err(ClashError::BadFaction(f.id));
        }
        ids.push(f.id);
    }
    for a in inp.arrivals {
        if a.posture == Posture::Disarray {
            return Err(ClashError::ArrivalWithoutStance(a.id));
        }
    }
    ids.sort_unstable();
    if let Some(w) = ids.windows(2).find(|w| w[0] == w[1]) {
        return Err(ClashError::DuplicateId(w[0]));
    }
    let mut tiles: Vec<u8> = Vec::new();
    let mut gids: Vec<u64> = Vec::new();
    for g in inp.garrisons {
        if g.tile as usize >= PROVINCE_TILES {
            return Err(ClashError::BadTile(g.id));
        }
        if g.faction >= FACTION_LIMIT {
            return Err(ClashError::BadFaction(g.id));
        }
        if tiles.contains(&g.tile) {
            return Err(ClashError::SharedTile(g.tile));
        }
        if gids.contains(&g.id) {
            return Err(ClashError::DuplicateId(g.id));
        }
        tiles.push(g.tile);
        gids.push(g.id);
    }
    Ok(())
}

/// Sides among `factions`: the connected groups of factions that are not
/// hostile to each other. Returns the side (its smallest faction) of each.
fn side_of(factions: &[u8], rel: Relations) -> [u8; FACTION_LIMIT as usize] {
    let mut side: [u8; FACTION_LIMIT as usize] = core::array::from_fn(|f| f as u8);
    // Union by smallest member, repeated to a fixed point (≤ 8 factions).
    loop {
        let mut changed = false;
        for &a in factions {
            for &b in factions {
                if a != b && !rel.hostile(a, b) {
                    let m = side[a as usize].min(side[b as usize]);
                    for s in [a, b] {
                        if side[s as usize] != m {
                            side[s as usize] = m;
                            changed = true;
                        }
                    }
                }
            }
        }
        if !changed {
            return side;
        }
    }
}

/// Hex fair share: indices of `cands` (sorted by mass order) admitted
/// into `cap` slots. Each side is guaranteed `min(its hosts, ⌊cap /
/// sides⌋)`, its heaviest first; the rest go by mass.
fn fair_share(units: &[Unit], cands: &[usize], cap: usize, rel: Relations) -> Vec<usize> {
    if cands.len() <= cap {
        return cands.to_vec();
    }
    let mut factions: Vec<u8> = cands.iter().map(|&i| units[i].faction).collect();
    factions.sort_unstable();
    factions.dedup();
    let side = side_of(&factions, rel);
    let mut sides: Vec<u8> = factions.iter().map(|&f| side[f as usize]).collect();
    sides.sort_unstable();
    sides.dedup();
    let g = cap / sides.len();
    let mut taken = vec![false; cands.len()];
    let mut n = 0;
    for s in &sides {
        let mut k = 0;
        for (j, &i) in cands.iter().enumerate() {
            if k == g {
                break;
            }
            if side[units[i].faction as usize] == *s {
                taken[j] = true;
                k += 1;
                n += 1;
            }
        }
    }
    for t in taken.iter_mut() {
        if n == cap {
            break;
        }
        if !*t {
            *t = true;
            n += 1;
        }
    }
    cands
        .iter()
        .zip(taken)
        .filter(|(_, t)| *t)
        .map(|(i, _)| *i)
        .collect()
}

/// Whether the clash of `inp.bell` is quiet: no arrivals, and resolving
/// it would change nothing (no hostile combatants share a hex, no hex is
/// over its fair share). Independent of the seed, so a program can skip a
/// quiet bell before its seed exists; skipping it gives exactly the state
/// resolving it would.
pub fn is_quiet(rules: &Ruleset, inp: &ClashInput) -> Result<bool, ClashError> {
    if !inp.arrivals.is_empty() {
        return Ok(false);
    }
    let probe = ClashInput {
        seed: [0u8; 32],
        ..*inp
    };
    let o = resolve_clash(rules, &probe)?;
    let same = o.engagements == 0
        && o.fighters.iter().all(|f| {
            inp.residents.iter().any(|r| {
                r.id == f.id
                    && f.fate == Fate::Stays { tile: r.tile }
                    && f.troops == r.troops
                    && f.stamina == r.stamina
                    && !f.engaged
            })
        })
        && o.garrisons.iter().all(|g| {
            inp.garrisons
                .iter()
                .any(|x| x.id == g.id && x.troops == g.troops)
        });
    Ok(same)
}

/// Resolve the clash of `inp.province` at `inp.bell`.
pub fn resolve_clash(rules: &Ruleset, inp: &ClashInput) -> Result<ClashOutcome, ClashError> {
    validate(inp)?;
    let cs = clash_seed(&inp.seed, inp.province, inp.bell);
    let rel = inp.relations;

    // Hosts in id order, then garrisons in id order.
    let mut units: Vec<Unit> =
        Vec::with_capacity(inp.residents.len() + inp.arrivals.len() + inp.garrisons.len());
    let mut hosts: Vec<(&Fighter, bool)> = inp
        .residents
        .iter()
        .map(|f| (f, false))
        .chain(inp.arrivals.iter().map(|f| (f, true)))
        .collect();
    hosts.sort_by_key(|(f, _)| f.id);
    for (f, arrival) in hosts {
        units.push(Unit {
            id: f.id,
            city: false,
            faction: f.faction,
            unit: Some(f.unit),
            troops: f.troops,
            stamina: f.stamina,
            tile: f.tile,
            posture: f.posture,
            arrival,
            retreat_bps: if arrival { f.retreat_bps } else { None },
            dealt_bps: f.dealt_bps,
            walls: false,
            tie: tie_key(&cs, f.id),
            st: St::In,
        });
    }
    let n_hosts = units.len();
    let mut gs: Vec<&Garrison> = inp.garrisons.iter().collect();
    gs.sort_by_key(|g| g.id);
    for g in gs {
        units.push(Unit {
            id: g.id,
            city: true,
            faction: g.faction,
            unit: None,
            troops: g.troops,
            stamina: 0,
            tile: g.tile,
            posture: g.posture,
            arrival: false,
            retreat_bps: None,
            dealt_bps: BPS_ONE,
            walls: g.walls,
            tie: 0,
            st: St::In,
        });
    }
    let garrison_on = |units: &[Unit], tile: u8| -> Option<usize> {
        (n_hosts..units.len()).find(|&g| units[g].tile == tile)
    };

    // 1. Retreat orders against the frozen roster.
    for a in 0..n_hosts {
        let Some(r) = units[a].retreat_bps else {
            continue;
        };
        let (tile, fa) = (units[a].tile, units[a].faction);
        let defending: u64 = units
            .iter()
            .filter(|u| !u.arrival && u.tile == tile && rel.hostile(u.faction, fa))
            .map(|u| u.strength())
            .sum();
        let mine = units[a].strength();
        if defending as u128 * BPS_ONE as u128 > r as u128 * mine as u128 {
            units[a].st = St::Out(Fate::Retreated);
        }
    }

    // 2a. Province caps: arrivals by mass order.
    let mut per_faction = [0usize; FACTION_LIMIT as usize];
    let mut total = 0;
    for u in units[..n_hosts].iter().filter(|u| !u.arrival) {
        per_faction[u.faction as usize] += 1;
        total += 1;
    }
    let mut arrivals: Vec<usize> = (0..n_hosts)
        .filter(|&i| units[i].arrival && units[i].st == St::In)
        .collect();
    arrivals.sort_by_key(|&i| units[i].order());
    for i in arrivals {
        let f = units[i].faction as usize;
        if total >= PROVINCE_HOST_CAP || per_faction[f] >= FACTION_RESIDENT_CAP {
            units[i].st = St::Out(Fate::Bounced);
        } else {
            per_faction[f] += 1;
            total += 1;
        }
    }

    // 2b. Hex fair share.
    let mut tiles: Vec<u8> = units[..n_hosts]
        .iter()
        .filter(|u| u.st == St::In)
        .map(|u| u.tile)
        .collect();
    tiles.sort_unstable();
    tiles.dedup();
    for &t in &tiles {
        let mut cands: Vec<usize> = (0..n_hosts)
            .filter(|&i| units[i].st == St::In && units[i].tile == t)
            .collect();
        if cands.len() <= OWNER_HEX_SLOTS {
            continue;
        }
        cands.sort_by_key(|&i| units[i].order());
        let admitted = match garrison_on(&units, t) {
            Some(g) => {
                let owner = units[g].faction;
                let others: Vec<usize> = cands
                    .iter()
                    .copied()
                    .filter(|&i| rel.hostile(units[i].faction, owner))
                    .collect();
                let mut a = fair_share(&units, &others, OWNER_HEX_SLOTS, rel);
                let room = HEX_HOST_CAP - a.len();
                // The owner's side: its own hosts first, then its friends,
                // each by mass.
                let mut own: Vec<usize> = cands
                    .iter()
                    .copied()
                    .filter(|&i| !rel.hostile(units[i].faction, owner))
                    .collect();
                own.sort_by_key(|&i| (units[i].faction != owner, units[i].order()));
                a.extend(own.into_iter().take(room));
                a
            }
            None => fair_share(&units, &cands, HEX_HOST_CAP, rel),
        };
        for i in cands {
            if !admitted.contains(&i) {
                units[i].st = St::Out(Fate::Bounced);
            }
        }
    }

    // 3. Engagements from pre-clash counts.
    let n = units.len();
    let mut dmg = vec![0u64; n];
    let mut engaged = vec![false; n];
    // per (tile, faction): damage dealt and taken
    let mut dealt: Vec<(u8, u8, u64)> = Vec::new();
    let mut taken: Vec<(u8, u8, u64)> = Vec::new();
    let bump = |v: &mut Vec<(u8, u8, u64)>, t: u8, f: u8, d: u64| match v
        .iter_mut()
        .find(|x| x.0 == t && x.1 == f)
    {
        Some(x) => x.2 += d,
        None => v.push((t, f, d)),
    };
    let mut engagements = 0u32;
    for &t in &tiles {
        let part: Vec<usize> = (0..n)
            .filter(|&i| units[i].st == St::In && units[i].tile == t && units[i].fights())
            .collect();
        let rough = inp
            .terrain
            .terrain
            .get(t as usize)
            .is_some_and(|x| x.info().defense_bps < BPS_ONE);
        // Engagements each combatant is in on this hex.
        let count = |i: usize| -> u64 {
            part.iter()
                .filter(|&&j| rel.hostile(units[i].faction, units[j].faction))
                .count() as u64
        };
        for (x, &i) in part.iter().enumerate() {
            for &j in &part[x + 1..] {
                if !rel.hostile(units[i].faction, units[j].faction) {
                    continue;
                }
                // Roles: a garrison always defends; an arrival attacks a
                // resident; otherwise both ways at half weight.
                let (ui, uj) = (&units[i], &units[j]);
                let pairs: &[(usize, usize, u64)] = if uj.city {
                    &[(i, j, 1)]
                } else if ui.city {
                    &[(j, i, 1)]
                } else if ui.arrival != uj.arrival {
                    if ui.arrival {
                        &[(i, j, 1)]
                    } else {
                        &[(j, i, 1)]
                    }
                } else {
                    &[(i, j, 2), (j, i, 2)]
                };
                for &(ai, di, half) in pairs {
                    let (a, d) = (&units[ai], &units[di]);
                    let sit = Situation {
                        defender_on_rough_terrain: rough && !d.arrival,
                        city_walls: d.city && d.walls,
                        attacker_exhausted: a.stamina < ENGAGE_STAMINA,
                        ..Situation::default()
                    };
                    let mut id = [0u8; 18];
                    id[..9].copy_from_slice(&a.key());
                    id[9..].copy_from_slice(&d.key());
                    let eid = rand(&cs, b"eng", &id) as u32;
                    let va = variance(rules, &cs, eid, 0);
                    let vd = variance(rules, &cs, eid, 1);
                    let (to_def, to_att) =
                        resolve_engagement(rules, a.combatant(), d.combatant(), sit, va, vd);
                    let scale = |raw: MilliTroops, from: &Unit, to: &Unit, n: u64| -> u64 {
                        raw as u64 * damage_bps(from.posture, to.posture) as u64 / BPS_ONE as u64
                            * from.dealt_bps as u64
                            / BPS_ONE as u64
                            / n.max(1)
                            / half
                    };
                    let x_def = scale(to_def, a, d, count(ai));
                    let x_att = scale(to_att, d, a, count(di));
                    let (tile, fa, fd) = (t, a.faction, d.faction);
                    dmg[di] += x_def;
                    dmg[ai] += x_att;
                    engaged[ai] = true;
                    engaged[di] = true;
                    bump(&mut dealt, tile, fa, x_def);
                    bump(&mut taken, tile, fd, x_def);
                    bump(&mut dealt, tile, fd, x_att);
                    bump(&mut taken, tile, fa, x_att);
                    engagements += 1;
                }
            }
        }
    }

    // 4. Apply all damage at once; 6. stamina with the damage-ratio refund.
    for i in 0..n {
        if units[i].st != St::In {
            continue;
        }
        let u = &mut units[i];
        u.troops = (u.troops as u64).saturating_sub(dmg[i]) as MilliTroops;
        if !u.city {
            if engaged[i] {
                let get = |v: &Vec<(u8, u8, u64)>| {
                    v.iter()
                        .find(|x| x.0 == u.tile && x.1 == u.faction)
                        .map_or(0, |x| x.2)
                };
                let (d, tk) = (get(&dealt), get(&taken));
                let refund = d > 0 && d >= REFUND_RATIO * tk;
                if !refund {
                    u.stamina = u.stamina.saturating_sub(ENGAGE_STAMINA);
                }
            }
            if u.troops < DESTROYED_BELOW {
                u.troops = 0;
                u.st = St::Out(Fate::Destroyed);
            }
        }
    }

    // 5. One side holds the field.
    let mut withdrawing: Vec<usize> = Vec::new();
    for &t in &tiles {
        let here: Vec<usize> = (0..n)
            .filter(|&i| {
                units[i].tile == t
                    && units[i].st == St::In
                    && (!units[i].city || units[i].troops > 0)
            })
            .collect();
        let mut factions: Vec<u8> = here.iter().map(|&i| units[i].faction).collect();
        factions.sort_unstable();
        factions.dedup();
        let contested = factions
            .iter()
            .any(|&a| factions.iter().any(|&b| rel.hostile(a, b)));
        if !contested {
            continue;
        }
        // (strength desc, defender first, seeded faction tie)
        let mut ranked: Vec<(core::cmp::Reverse<u64>, bool, u64, u8)> = factions
            .iter()
            .map(|&f| {
                let s: u64 = here
                    .iter()
                    .filter(|&&i| units[i].faction == f)
                    .map(|&i| units[i].strength())
                    .sum();
                let defender = here
                    .iter()
                    .any(|&i| units[i].faction == f && !units[i].arrival);
                (
                    core::cmp::Reverse(s),
                    !defender,
                    rand_id(&cs, b"field", f as u64),
                    f,
                )
            })
            .collect();
        ranked.sort();
        let mut kept: Vec<u8> = Vec::new();
        for (_, _, _, f) in ranked {
            if kept.iter().all(|&k| !rel.hostile(k, f)) {
                kept.push(f);
            }
        }
        for &i in &here {
            if !units[i].city && !kept.contains(&units[i].faction) {
                withdrawing.push(i);
            }
        }
    }
    withdrawing.sort_by_key(|&i| units[i].order());
    // Mark all losers out first so they do not count as occupants.
    for &i in &withdrawing {
        units[i].st = St::Out(Fate::Bounced);
    }
    let k = inp.province.wedge().unwrap_or(0);
    for &i in &withdrawing {
        if units[i].arrival {
            continue;
        }
        let (f, from) = (units[i].faction, units[i].tile);
        let Some(o) = tile_offset(from) else { continue };
        for nb in Hex::ORIGIN.neighbors_in(k) {
            let Some(nt) = tile_index(Hex::new(o.q + nb.q, o.r + nb.r)) else {
                continue;
            };
            if !inp.terrain.passable(nt) {
                continue;
            }
            let occ: Vec<usize> = (0..n)
                .filter(|&j| {
                    units[j].tile == nt
                        && (matches!(units[j].st, St::In | St::Out(Fate::Withdrew { .. })))
                        && (!units[j].city || units[j].troops > 0)
                })
                .collect();
            let hosts_there = occ.iter().filter(|&&j| !units[j].city).count();
            let friendly = occ.iter().any(|&j| units[j].faction == f);
            let hostile = occ.iter().any(|&j| rel.hostile(units[j].faction, f));
            if friendly && !hostile && hosts_there < HEX_HOST_CAP {
                units[i].tile = nt;
                units[i].st = St::Out(Fate::Withdrew { tile: nt });
                break;
            }
        }
    }

    // Results.
    let mut fighters = Vec::with_capacity(n_hosts);
    for (i, u) in units[..n_hosts].iter().enumerate() {
        let fate = match u.st {
            St::In => Fate::Stays { tile: u.tile },
            St::Out(f) => f,
        };
        fighters.push(FighterResult {
            id: u.id,
            arrival: u.arrival,
            troops: u.troops,
            stamina: u.stamina,
            fate,
            engaged: engaged[i],
        });
    }
    let mut garrisons = Vec::with_capacity(n - n_hosts);
    for g in &units[n_hosts..] {
        let stay = |u: &Unit| {
            !u.city && u.tile == g.tile && matches!(u.st, St::In | St::Out(Fate::Withdrew { .. }))
        };
        let attackers_hold = units
            .iter()
            .any(|u| stay(u) && rel.hostile(u.faction, g.faction));
        let holders = units
            .iter()
            .filter(|u| stay(u) && rel.hostile(u.faction, g.faction) && u.faction < 8)
            .fold(0u8, |m, u| m | (1 << u.faction));
        let defender_present = units
            .iter()
            .any(|u| stay(u) && !rel.hostile(u.faction, g.faction));
        garrisons.push(GarrisonResult {
            id: g.id,
            troops: g.troops,
            attackers_hold,
            holders,
            defender_present,
        });
    }
    Ok(ClashOutcome {
        province: inp.province,
        bell: inp.bell,
        fighters,
        garrisons,
        engagements,
    })
}
