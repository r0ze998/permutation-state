//! Barbarian camps (contract §7, I-56; specified from `frontier-sim`
//! `sim.rs` l. 87–103 and 1119–1147).
//!
//! - **One camp per province**, on a passable tile that is not a site
//!   (tiles 1..61; the centre tile 0 never holds one), with `100 +
//!   rand(0..=300)` troops.
//! - **Daily respawn:** a province with a holding gets a camp with chance
//!   ½ each game day, evaluated lazily at the province's first resolve or
//!   skip of that day (the program's job; the kernel takes the day).
//! - **Initial camp** (`initial`): one at OpenProvince for rings ≥ 2, so a
//!   new player has something to raid (onboarding; a recorded deviation
//!   from the sim, which only respawns). Rings 0–1 never have camps
//!   (their sites are reserved, I-30).
//! - Camps do not reduce `open_sites`. Defeating one gives
//!   [`loot`] = `WORKS_CAMP` (10 Works) and no goods (the sim has none).
//!
//! **Draws** (pinned): `x(d) = rng::rand(ring_seed, d, le32(P) ‖ le32(Q)
//! ‖ le32(day) ‖ [initial])` with the domains `frontier/camp/spawn`,
//! `frontier/camp/tile` and `frontier/camp/troops`; `below(x, n) = ⌊x · n
//! / 2⁶⁴⌋`. The respawn happens iff `below(x_spawn, 2) == 0`; the tile is
//! `candidates[below(x_tile, len)]` over the ascending list of passable
//! non-site tiles 1..61; troops are `100 + below(x_troops, 301)`.

use crate::rng::{rand, Seed};

use super::catalog::WORKS_CAMP;
use super::geometry::{ProvinceCoord, PROVINCE_TILES};
use super::terrain::ProvinceTerrain;

/// Kernel version of this module (part of the ruleset hash).
pub const CAMP_VERSION: u16 = 1;

/// Troops of a camp: `CAMP_TROOPS_MIN + rand(0..=CAMP_TROOPS_SPREAD)`.
pub const CAMP_TROOPS_MIN: u32 = 100;
pub const CAMP_TROOPS_SPREAD: u32 = 300;
/// First ring whose provinces get an initial camp.
pub const CAMP_FIRST_RING: u32 = 2;

/// A camp: its tile and its troops (whole troops; `× MILLI` for the
/// clash kernel's `MilliTroops`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Camp {
    pub tile: u8,
    pub troops: u32,
}

/// `⌊x · n / 2⁶⁴⌋`: a draw in `0..n` from a uniform `u64`.
pub const fn below(x: u64, n: u64) -> u64 {
    ((x as u128 * n as u128) >> 64) as u64
}

fn draw(seed: &Seed, domain: &[u8], p: ProvinceCoord, day: u32, initial: bool) -> u64 {
    let mut id = [0u8; 13];
    id[..4].copy_from_slice(&p.p.to_le_bytes());
    id[4..8].copy_from_slice(&p.q.to_le_bytes());
    id[8..12].copy_from_slice(&day.to_le_bytes());
    id[12] = initial as u8;
    rand(seed, domain, &id)
}

/// Where a camp may stand: passable, not a site, not the centre tile.
pub fn camp_tile_ok(terrain: &ProvinceTerrain, tile: u8) -> bool {
    tile >= 1
        && (tile as usize) < PROVINCE_TILES
        && terrain.passable(tile)
        && !terrain.is_site(tile)
}

/// The camp province `p` gets on game day `day`, if any:
///
/// - `initial` (OpenProvince): one camp in rings ≥ 2, none in rings 0–1;
/// - otherwise (daily respawn): none without a holding; with one, a camp
///   with chance ½.
///
/// `None` also when the province has no allowed tile.
pub fn place(
    ring_seed: &[u8; 32],
    p: ProvinceCoord,
    terrain: &ProvinceTerrain,
    day: u32,
    has_holding: bool,
    initial: bool,
) -> Option<Camp> {
    if initial {
        if p.ring() < CAMP_FIRST_RING {
            return None;
        }
    } else {
        if !has_holding {
            return None;
        }
        if below(draw(ring_seed, b"frontier/camp/spawn", p, day, false), 2) != 0 {
            return None;
        }
    }
    let mut cands = [0u8; PROVINCE_TILES];
    let mut n = 0usize;
    for t in 1..PROVINCE_TILES as u8 {
        if camp_tile_ok(terrain, t) {
            cands[n] = t;
            n += 1;
        }
    }
    if n == 0 {
        return None;
    }
    let k = below(
        draw(ring_seed, b"frontier/camp/tile", p, day, initial),
        n as u64,
    ) as usize;
    let troops = CAMP_TROOPS_MIN
        + below(
            draw(ring_seed, b"frontier/camp/troops", p, day, initial),
            CAMP_TROOPS_SPREAD as u64 + 1,
        ) as u32;
    Some(Camp {
        tile: cands[k],
        troops,
    })
}

/// Works for defeating a camp (10; no goods in M1).
pub const fn loot() -> u32 {
    WORKS_CAMP as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontier::terrain::generate_province;

    #[test]
    fn camps_stand_on_allowed_tiles() {
        let seed = [9u8; 32];
        let mut spawned = 0u32;
        let mut tries = 0u32;
        for (p, q) in [(2, 0), (0, 3), (-4, 1), (5, -5)] {
            let c = ProvinceCoord::new(p, q);
            let t = generate_province(&seed, c);
            let init = place(&seed, c, &t, 0, false, true).unwrap();
            assert!(camp_tile_ok(&t, init.tile));
            assert!((100..=400).contains(&init.troops));
            for day in 0..200 {
                assert_eq!(place(&seed, c, &t, day, false, false), None);
                tries += 1;
                if let Some(x) = place(&seed, c, &t, day, true, false) {
                    assert!(camp_tile_ok(&t, x.tile));
                    assert!((100..=400).contains(&x.troops));
                    spawned += 1;
                }
            }
        }
        // chance ½ (800 draws): well inside 5 σ.
        assert!((330..=470).contains(&spawned), "{spawned} of {tries}");
    }

    #[test]
    fn inner_rings_get_no_initial_camp() {
        let seed = [1u8; 32];
        for c in [ProvinceCoord::CONCORD, ProvinceCoord::new(1, 0)] {
            let t = generate_province(&seed, c);
            assert_eq!(place(&seed, c, &t, 0, true, true), None);
        }
        assert_eq!(loot(), 10);
    }
}
