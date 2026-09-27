//! Time, travel and supply (design §3.6, §8.4, owner decision D10).
//!
//! | Terrain or mode | Time per hex |
//! |---|---|
//! | Grassland, plains, on foot | 2 min |
//! | Hills, forest | 3 min |
//! | Road | ×0.5 |
//! | Cavalry (mounted units) | ×0.5 |
//! | Mountains, water | impassable (terrain generation leaves passes and fords) |
//!
//! A march's arrival is rounded up to the next bell; a player may pick a
//! later bell, up to 72 bells after departure. One march covers ≤ 32
//! hexes over ≤ 4 provinces. The chain never searches: the client submits
//! the path and [`path_cost`] checks it (WP05, B4).

use super::geometry::{check_hex, province_of, ProvinceCoord};
use crate::fixed::{Bps, BPS_ONE};
use crate::hex::Hex;
use crate::map::Terrain;
use crate::units::{stats, UnitClass, UnitType};
use alloc::vec::Vec;

/// A bell is 10 minutes (design §2.1).
pub const BELL_SECS: i64 = 600;
/// Bells per day.
pub const BELLS_PER_DAY: u32 = 144;
pub const FLAT_HEX_SECS: u32 = 120;
pub const ROUGH_HEX_SECS: u32 = 180;
pub const ROAD_BPS: Bps = 5_000;
pub const CAVALRY_BPS: Bps = 5_000;
pub const MAX_PATH_STEPS: usize = 32;
pub const MAX_PATH_PROVINCES: usize = 4;
/// Latest arrival a march may choose, in bells after its departure bell.
pub const MAX_ARRIVAL_LEAD_BELLS: u32 = 72;
/// Waystone to Waystone: 3 bells, destination public, only into a
/// friendly holding's hex (design §3.6).
pub const WAYSTONE_BELLS: u32 = 3;
/// Supply range: hosts more than this many provinces from any friendly
/// holding suffer attrition (`host::supply_attrition`).
pub const SUPPLY_RANGE_PROVINCES: u32 = 3;
/// Stamina a march costs: a base plus a share per hex [design].
pub const MARCH_STAMINA_BASE: u16 = 10;
pub const MARCH_STAMINA_PER_HEX: u16 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TravelError {
    EmptyPath,
    TooLong,
    /// A step is not adjacent to the previous one.
    NotAdjacent(u8),
    Impassable(u8),
    OutOfBounds(u8),
    TooManyProvinces,
    /// The chosen arrival bell is before the earliest possible one.
    TooEarly {
        earliest: u32,
    },
    /// The chosen arrival bell is more than 72 bells after departure.
    TooLate {
        latest: u32,
    },
}

/// Bell containing unix time `ts` (`(ts − genesis) / 600`, 0 before genesis).
pub const fn bell_at(genesis_ts: i64, ts: i64) -> u32 {
    if ts <= genesis_ts {
        0
    } else {
        ((ts - genesis_ts) / BELL_SECS) as u32
    }
}

/// Unix time bell `b` starts.
pub const fn bell_start(genesis_ts: i64, b: u32) -> i64 {
    genesis_ts + b as i64 * BELL_SECS
}

/// Mounted units ride at cavalry pace.
pub const fn is_cavalry(unit: UnitType) -> bool {
    matches!(stats(unit).class, UnitClass::Mounted)
}

/// Seconds to enter one hex of `terrain` (None: impassable).
pub fn hex_secs(terrain: Terrain, road: bool, unit: UnitType) -> Option<u32> {
    let base = match terrain {
        Terrain::Grassland | Terrain::Plains => FLAT_HEX_SECS,
        Terrain::Forest | Terrain::Hills => ROUGH_HEX_SECS,
        Terrain::Mountain | Terrain::Water => return None,
    };
    let mut s = base as u64;
    if road {
        s = s * ROAD_BPS as u64 / BPS_ONE as u64;
    }
    if is_cavalry(unit) {
        s = s * CAVALRY_BPS as u64 / BPS_ONE as u64;
    }
    Some(s as u32)
}

/// One step of a submitted path: the hex entered, its terrain and whether a
/// road runs there (both read from the province accounts).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub hex: Hex,
    pub terrain: Terrain,
    pub road: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathCost {
    pub secs: u32,
    pub hexes: u32,
    /// Distinct provinces the steps enter, in first-entered order.
    pub provinces: Vec<ProvinceCoord>,
}

/// Check a path from `start` and price it for `unit`: every step adjacent
/// to the last, passable, inside the coordinate bound; ≤ 32 steps over
/// ≤ 4 provinces.
pub fn path_cost(start: Hex, steps: &[Step], unit: UnitType) -> Result<PathCost, TravelError> {
    if steps.is_empty() {
        return Err(TravelError::EmptyPath);
    }
    if steps.len() > MAX_PATH_STEPS {
        return Err(TravelError::TooLong);
    }
    check_hex(start).map_err(|_| TravelError::OutOfBounds(0))?;
    let mut prev = start;
    let mut secs = 0u32;
    let mut provinces: Vec<ProvinceCoord> = Vec::new();
    for (i, s) in steps.iter().enumerate() {
        let i = i as u8;
        check_hex(s.hex).map_err(|_| TravelError::OutOfBounds(i))?;
        if prev.distance(s.hex) != 1 {
            return Err(TravelError::NotAdjacent(i));
        }
        secs += hex_secs(s.terrain, s.road, unit).ok_or(TravelError::Impassable(i))?;
        let p = province_of(s.hex);
        if !provinces.contains(&p) {
            if provinces.len() == MAX_PATH_PROVINCES {
                return Err(TravelError::TooManyProvinces);
            }
            provinces.push(p);
        }
        prev = s.hex;
    }
    Ok(PathCost {
        secs,
        hexes: steps.len() as u32,
        provinces,
    })
}

/// Earliest arrival bell of a march leaving at `depart_ts` that takes
/// `secs`: its arrival time rounded up to the next bell start, and never
/// the departure bell itself.
pub const fn earliest_arrival_bell(genesis_ts: i64, depart_ts: i64, secs: u32) -> u32 {
    let t = depart_ts + secs as i64 - genesis_ts;
    let up = if t <= 0 {
        0
    } else {
        ((t + BELL_SECS - 1) / BELL_SECS) as u32
    };
    let after = bell_at(genesis_ts, depart_ts) + 1;
    if up > after {
        up
    } else {
        after
    }
}

/// Check a chosen arrival bell (public at departure) against the path's
/// travel time: no earlier than possible, no later than 72 bells after
/// the departure bell.
pub fn check_arrival_bell(
    genesis_ts: i64,
    depart_ts: i64,
    secs: u32,
    chosen: u32,
) -> Result<(), TravelError> {
    let earliest = earliest_arrival_bell(genesis_ts, depart_ts, secs);
    let latest = bell_at(genesis_ts, depart_ts) + MAX_ARRIVAL_LEAD_BELLS;
    if chosen < earliest {
        return Err(TravelError::TooEarly { earliest });
    }
    if chosen > latest {
        return Err(TravelError::TooLate { latest });
    }
    Ok(())
}

/// Stamina a march of `hexes` costs.
pub const fn march_stamina(hexes: u32) -> u16 {
    let s = MARCH_STAMINA_BASE as u32 + MARCH_STAMINA_PER_HEX as u32 * hexes;
    if s > u16::MAX as u32 {
        u16::MAX
    } else {
        s as u16
    }
}

/// Whether a host in province `at` is out of supply: more than 3
/// provinces from every friendly holding.
pub fn out_of_supply(at: ProvinceCoord, friendly_holdings: &[ProvinceCoord]) -> bool {
    !friendly_holdings
        .iter()
        .any(|h| h.distance(at) <= SUPPLY_RANGE_PROVINCES)
}

/// Rim-to-Concord (or any) straight-line time on open ground: `hexes`
/// flat hexes, for planning tables.
pub fn open_ground_secs(hexes: u32, road: bool, unit: UnitType) -> u64 {
    hex_secs(Terrain::Plains, road, unit).unwrap_or(0) as u64 * hexes as u64
}
