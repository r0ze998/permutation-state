//! The kernel input of one province-bell rebuilt from account bytes: the
//! program's `proc::clash::model::build` (W4-A, normative), transcribed
//! once for the off-chain readers — the herald's clash check and the
//! verifier's V7 replay (integ-W4 review: each carried its own copy, and
//! both disagreed with the program; the herald's was corrected in
//! `3acc6b1`, the verifier's failed an honest program season). Moving the
//! program's model itself into `frontier-abi` (one builder for the
//! program, these readers and the WASM `resolve_from_inputs`) stays W4-A's
//! request D8 for W5-A.
//!
//! The rules (contract §5.11 v1.6 §22):
//!
//! - **residents**: entries in state 1 with `from_bell ≤ b` at
//!   `Host::values_at(b)`, at Hold, the stored `dealt_bps` (0 reads 1.0);
//! - **garrisons**: site mirrors in state 1 among `site_count` at
//!   `GarrisonState::at(b)` (capped at `MAX_HOST_TROOPS`), walls when
//!   committed or a wall item with `delta > 0` effective by b, id = the
//!   holding key;
//! - **the camp** after the day's check (I-56: at the first resolve or skip
//!   of a day, `camp::place(sha256("PSF-CAMP-v1" ‖ province[TERRAIN ..=
//!   SITE_COUNT]), …)`, a spawn replacing a present camp, `gen + 1`): a
//!   NEUTRAL garrison, id `u64::MAX − gen`, troops `× 1,000` capped, only
//!   while fewer than `MAX_GARRISONS` garrisons stand;
//! - **arrivals**: the present records;
//! - residents and arrivals to the kernel **in id order**; the storage room
//!   from the entry states (I-43).

use frontier_abi::addr::holding_key_of_host;
use frontier_abi::entry::{read_entry, unit_from_u8};
use frontier_abi::layout::province::{entry as E, province as PV, site as S};
use permutation_rules::fixed::{Bps, BPS_ONE, MILLI};
use permutation_rules::frontier::camp as kcamp;
use permutation_rules::frontier::clash::{
    self, ClashInput, ClashOutcome, Fighter, Garrison, Occupancy, Relations, MAX_GARRISONS, NEUTRAL,
};
use permutation_rules::frontier::geometry::{ProvinceCoord, PROVINCE_TILES};
use permutation_rules::frontier::host::{GarrisonState, MAX_HOST_TROOPS};
use permutation_rules::frontier::stance::{Posture, Stance};
use permutation_rules::frontier::terrain::ProvinceTerrain;
use permutation_rules::map::{Terrain, TileResource};

use crate::decode::{ClashInputs, Province};

/// Domain of the camp seed (the program's `model::CAMP_DOMAIN`).
pub const CAMP_DOMAIN: &[u8] = b"PSF-CAMP-v1";
/// Bells per game day.
pub const DAY_BELLS: u32 = 144;

/// Terrain classes in declaration (= borsh) order.
pub const TERRAINS: [Terrain; 6] = [
    Terrain::Grassland,
    Terrain::Plains,
    Terrain::Forest,
    Terrain::Hills,
    Terrain::Mountain,
    Terrain::Water,
];
const RESOURCES: [TileResource; 3] = [
    TileResource::Wheat,
    TileResource::Iron,
    TileResource::Horses,
];
/// Stances in `seal` byte order (0 Hold, 1 Assault, 2 Flank, 3 Brace).
pub const STANCES: [Stance; 4] = [Stance::Hold, Stance::Assault, Stance::Flank, Stance::Brace];

/// A doctrine multiplier as stored (0 reads as 1.0, the program's `bps`).
pub fn bps(v: u16) -> Bps {
    if v == 0 {
        BPS_ONE
    } else {
        v as Bps
    }
}

/// The kernel terrain of a Province account.
pub fn terrain_of(pv: &Province) -> Result<ProvinceTerrain, String> {
    let mut terrain = [Terrain::Grassland; PROVINCE_TILES];
    let mut resource = [None; PROVINCE_TILES];
    for i in 0..PROVINCE_TILES {
        terrain[i] = *TERRAINS
            .get(pv.terrain[i] as usize)
            .ok_or(format!("terrain byte {} at tile {i}", pv.terrain[i]))?;
        resource[i] = match pv.resource[i] {
            0 => None,
            r => Some(
                *RESOURCES
                    .get(r as usize - 1)
                    .ok_or(format!("resource byte {r} at tile {i}"))?,
            ),
        };
    }
    Ok(ProvinceTerrain {
        terrain,
        resource,
        sites: pv.sites,
        site_count: pv.site_count,
    })
}

/// The camp a clash (or skip) of `bell` sees after the day's check (I-56;
/// the program's `camp_check`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CampAt {
    pub tile: u8,
    pub present: bool,
    /// Whole troops.
    pub troops: u32,
    pub gen: u32,
    /// The day's check ran at this bell (`next_check_day` moves).
    pub checked: bool,
    /// The check spawned a camp (a `CAMP` record).
    pub spawned: bool,
}

/// The camp after the day's check of `bell` (`pd`: the Province bytes).
pub fn camp_at(
    pv: &Province,
    pd: &[u8],
    terrain: &ProvinceTerrain,
    bell: u32,
) -> Result<CampAt, String> {
    let c = &pv.camp;
    let day = bell / DAY_BELLS;
    let same = CampAt {
        tile: c.tile,
        present: c.state == 1,
        troops: c.troops,
        gen: c.gen,
        checked: false,
        spawned: false,
    };
    if day < c.next_check_day {
        return Ok(same);
    }
    let block = pd
        .get(PV::TERRAIN..PV::SITE_COUNT + 1)
        .ok_or("province: short terrain block")?;
    let seed = permutation_rules::hash::sha256(&[CAMP_DOMAIN, block]);
    let has_holding = pv.site_mirror[..(pv.site_count as usize).min(PV::SITES_N)]
        .iter()
        .any(|m| m.state == S::STATE_HOLDING);
    let coord = ProvinceCoord::new(pv.p as i32, pv.q as i32);
    Ok(
        match kcamp::place(&seed, coord, terrain, day, has_holding, false) {
            Some(k) => CampAt {
                tile: k.tile,
                present: true,
                troops: k.troops,
                gen: c.gen.wrapping_add(1),
                checked: true,
                spawned: true,
            },
            None => CampAt {
                checked: true,
                ..same
            },
        },
    )
}

/// The owned parts of a `ClashInput` (it borrows them).
#[derive(Clone, Debug)]
pub struct BuiltInput {
    pub province: ProvinceCoord,
    pub bell: u32,
    pub seed: [u8; 32],
    pub terrain: ProvinceTerrain,
    /// Residents in id order.
    pub residents: Vec<Fighter>,
    pub garrisons: Vec<Garrison>,
    /// Present arrivals in id order.
    pub arrivals: Vec<Fighter>,
    /// The ClashInputs position (`faction × 4 + i`) of each arrival.
    pub arrival_pos: Vec<usize>,
    pub relations: Relations,
    pub occupancy: Occupancy,
    /// The camp the clash saw (after the day's check).
    pub camp: CampAt,
}

impl BuiltInput {
    pub fn input(&self) -> ClashInput<'_> {
        ClashInput {
            province: self.province,
            bell: self.bell,
            seed: self.seed,
            terrain: &self.terrain,
            residents: &self.residents,
            garrisons: &self.garrisons,
            arrivals: &self.arrivals,
            relations: self.relations,
            occupancy: self.occupancy,
        }
    }
    pub fn resolve(&self) -> Result<ClashOutcome, String> {
        clash::resolve_clash(&clash::frontier_ruleset(), &self.input())
            .map_err(|e| format!("{e:?}"))
    }
    pub fn quiet(&self) -> Result<bool, String> {
        clash::is_quiet(&clash::frontier_ruleset(), &self.input()).map_err(|e| format!("{e:?}"))
    }
}

/// The input of the clash of `bell` from the Province bytes before the
/// resolve and the gathered ClashInputs bytes (empty: no arrivals, a skip's
/// quiet test).
pub fn build(
    province_before: &[u8],
    inputs: &[u8],
    bell: u32,
    seed: &[u8; 32],
) -> Result<BuiltInput, String> {
    let pv = Province::decode(province_before).map_err(|e| format!("province: {e:?}"))?;
    let ci = if inputs.is_empty() {
        None
    } else {
        Some(ClashInputs::decode(inputs).map_err(|e| format!("inputs: {e:?}"))?)
    };
    if let Some(ci) = &ci {
        if (ci.p, ci.q, ci.bell) != (pv.p, pv.q, bell) {
            return Err("inputs and province are different province-bells".into());
        }
    }
    let terrain = terrain_of(&pv)?;
    let mut residents = vec![];
    let mut pending = [0u8; clash::FACTION_LIMIT as usize];
    let mut used = 0u8;
    for i in 0..PV::ENTRIES_N {
        let e = read_entry(province_before, i).map_err(|x| format!("entry {i}: {x:?}"))?;
        match e.state {
            E::STATE_ROSTER => {
                used += 1;
                if e.from_bell > bell {
                    continue;
                }
                let h = e.to_host().map_err(|x| format!("entry {i}: {x:?}"))?;
                let (troops, stamina) = h
                    .values_at(bell)
                    .map_err(|x| format!("host {}: {x:?}", e.id))?;
                residents.push(Fighter {
                    id: e.id,
                    faction: e.faction,
                    unit: h.unit,
                    troops,
                    stamina,
                    tile: e.tile,
                    posture: Posture::Stance(Stance::Hold),
                    retreat_bps: None,
                    dealt_bps: bps(e.dealt_bps),
                });
            }
            E::STATE_MUSTER_PENDING => {
                used += 1;
                if let Some(c) = pending.get_mut(e.faction as usize) {
                    *c = c.saturating_add(1);
                }
            }
            E::STATE_DEPARTED => used += 1,
            _ => {}
        }
    }
    let mut garrisons = vec![];
    for i in 0..(pv.site_count as usize).min(PV::SITES_N) {
        let m = &pv.site_mirror[i];
        if m.state != S::STATE_HOLDING {
            continue;
        }
        let pend = |b: u32, d: i64| (b != S::NO_BELL && d != 0).then_some((b, d));
        let g = GarrisonState {
            troops: m.garrison,
            pending: [
                pend(m.pend0_bell, m.pend0_delta),
                pend(m.pend1_bell, m.pend1_delta),
            ],
        };
        let troops = g.at(bell).map_err(|x| format!("garrison {i}: {x:?}"))?;
        let walls = m.walls_committed > 0
            || [m.wall_item0, m.wall_item1]
                .iter()
                .any(|(eff, d)| *d > 0 && *eff != S::NO_BELL && *eff <= bell);
        let id = frontier_abi::addr::host_id(pv.p as i32, pv.q as i32, i as u8, m.gen, 0)
            .map(holding_key_of_host)
            .ok_or(format!("site {i}: no holding id"))?;
        garrisons.push(Garrison {
            id,
            faction: m.faction,
            tile: pv.sites[i],
            troops: troops.min(MAX_HOST_TROOPS),
            walls,
            posture: Posture::Stance(Stance::Hold),
        });
    }
    let camp = camp_at(&pv, province_before, &terrain, bell)?;
    if camp.present && garrisons.len() < MAX_GARRISONS {
        garrisons.push(Garrison {
            id: u64::MAX - camp.gen as u64,
            faction: NEUTRAL,
            tile: camp.tile,
            troops: (camp.troops as u64 * MILLI as u64).min(MAX_HOST_TROOPS as u64) as u32,
            walls: false,
            posture: Posture::Stance(Stance::Hold),
        });
    }
    let mut arr: Vec<(Fighter, usize)> = vec![];
    if let Some(ci) = &ci {
        for (k, a) in ci
            .arrivals
            .iter()
            .enumerate()
            .filter(|(_, a)| a.present == 1)
        {
            arr.push((
                Fighter {
                    id: a.host_id,
                    faction: a.faction,
                    unit: unit_from_u8(a.unit).ok_or(format!("arrival unit {}", a.unit))?,
                    troops: a.troops,
                    stamina: a.stamina,
                    tile: a.tile,
                    posture: Posture::Stance(
                        *STANCES
                            .get(a.stance as usize)
                            .ok_or(format!("arrival stance {}", a.stance))?,
                    ),
                    retreat_bps: (a.retreat != 0).then_some(a.retreat as u32),
                    dealt_bps: bps(a.dealt),
                },
                k,
            ));
        }
    }
    residents.sort_by_key(|f| f.id);
    arr.sort_by_key(|(f, _)| f.id);
    let (arrivals, arrival_pos) = arr.into_iter().unzip();
    Ok(BuiltInput {
        province: ProvinceCoord::new(pv.p as i32, pv.q as i32),
        bell,
        seed: *seed,
        terrain,
        residents,
        garrisons,
        arrivals,
        arrival_pos,
        relations: Relations {
            peaceful: pv.relations,
        },
        occupancy: Occupancy {
            pending,
            storage_free: Occupancy::STORAGE.saturating_sub(used),
        },
        camp,
    })
}
