//! Clash reports (contract §8.4 `/h/clash/{P},{Q}/{bell}`): the inputs, the
//! seed and its anchor, the digests the program logged, the decoded
//! fighters and fates, and `heraldCheck` — the clash **recomputed natively
//! with `clash::resolve_clash`** and compared with the outcome digest the
//! CLASH record carries.
//!
//! Recomputing needs the `ClashInput` that ResolveFromInputs builds from the
//! Province (before the resolve) and the ClashInputs account. That builder is
//! the program's (W4-A, `permutation-frontier/src/proc/clash.rs`, a stub in
//! wave 3), so the herald takes it through the [`ClashBuilder`] trait:
//! [`Provisional`] follows the §5.11 ResolveFromInputs text over the
//! `frontier-abi` entry codec until W4-A exports the program's builder, and
//! the herald then plugs that in (handover note in `W3-D-NOTES.md`). The
//! report names the builder it used, and a builder that cannot rebuild the
//! input gives `heraldCheck: "unchecked"` with the reason instead of a
//! false `MISMATCH` alarm.

use frontier_abi::addr::holding_key_of_host;
use frontier_abi::entry::{read_entry, unit_from_u8};
use frontier_abi::layout::province::{entry as E, province as PV, site as S};
use permutation_rules::frontier::clash::{
    self, ClashInput, ClashOutcome, Fate, Fighter, Garrison, Occupancy, Relations, NEUTRAL,
};
use permutation_rules::frontier::geometry::{ProvinceCoord, PROVINCE_TILES};
use permutation_rules::frontier::host::GarrisonState;
use permutation_rules::frontier::stance::{Posture, Stance};
use permutation_rules::frontier::terrain::ProvinceTerrain;
use permutation_rules::map::{Terrain, TileResource};

use fclient::decode::{ClashInputs, Province};

/// Builds the kernel input of one province-bell and resolves it.
pub trait ClashBuilder: Send + Sync {
    /// Named in every report (`builder`).
    fn name(&self) -> &'static str;
    /// The outcome of the clash of `bell` from the Province bytes before
    /// ResolveFromInputs, the ClashInputs bytes after it and the seed.
    fn recompute(
        &self,
        province_before: &[u8],
        inputs: &[u8],
        bell: u32,
        seed: &[u8; 32],
    ) -> Result<ClashOutcome, String>;
}

/// The §5.11 ResolveFromInputs input, as the contract text states it:
/// residents in state 1 with `from_bell ≤ bell` at `Host::values_at(bell)`
/// fighting at Hold; garrisons from the site mirror at `GarrisonState::at`
/// with walls effective at the bell; arrivals = the present records;
/// terrain from the compact arrays; relations from the Province (0 in M1);
/// the storage room from the entry states; the camp, if present, as a
/// NEUTRAL garrison. It does **not** model the lazy camp respawn of the
/// day's first resolve (I-56): the program's builder (W4-A) is normative.
pub struct Provisional;

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
const STANCES: [Stance; 4] = [Stance::Hold, Stance::Assault, Stance::Flank, Stance::Brace];

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

/// The owned parts of a `ClashInput` (it borrows them).
pub struct BuiltInput {
    pub province: ProvinceCoord,
    pub bell: u32,
    pub seed: [u8; 32],
    pub terrain: ProvinceTerrain,
    pub residents: Vec<Fighter>,
    pub garrisons: Vec<Garrison>,
    pub arrivals: Vec<Fighter>,
    pub relations: Relations,
    pub occupancy: Occupancy,
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
}

impl Provisional {
    /// The input the provisional builder hands to the kernel.
    pub fn build(
        &self,
        province_before: &[u8],
        inputs: &[u8],
        bell: u32,
        seed: &[u8; 32],
    ) -> Result<BuiltInput, String> {
        let pv = Province::decode(province_before).map_err(|e| format!("province: {e:?}"))?;
        let ci = ClashInputs::decode(inputs).map_err(|e| format!("inputs: {e:?}"))?;
        if (ci.p, ci.q, ci.bell) != (pv.p, pv.q, bell) {
            return Err("inputs and province are different province-bells".into());
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
                        dealt_bps: e.dealt_bps as u32,
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
            let pend = |b: u32, d: i64| (b != S::NO_BELL).then_some((b, d));
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
                troops,
                walls,
                posture: Posture::Stance(Stance::Hold),
            });
        }
        if pv.camp.state == 1 {
            garrisons.push(Garrison {
                id: u64::MAX - pv.camp.gen as u64,
                faction: NEUTRAL,
                tile: pv.camp.tile,
                troops: pv.camp.troops,
                walls: false,
                posture: Posture::Stance(Stance::Hold),
            });
        }
        let mut arrivals = vec![];
        for a in ci.arrivals.iter().filter(|a| a.present == 1) {
            arrivals.push(Fighter {
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
                dealt_bps: a.dealt as u32,
            });
        }
        Ok(BuiltInput {
            province: ProvinceCoord::new(pv.p as i32, pv.q as i32),
            bell,
            seed: *seed,
            terrain,
            residents,
            garrisons,
            arrivals,
            relations: Relations {
                peaceful: pv.relations,
            },
            occupancy: Occupancy {
                pending,
                storage_free: Occupancy::STORAGE.saturating_sub(used),
            },
        })
    }
}

impl ClashBuilder for Provisional {
    fn name(&self) -> &'static str {
        "provisional-w3d"
    }
    fn recompute(
        &self,
        province_before: &[u8],
        inputs: &[u8],
        bell: u32,
        seed: &[u8; 32],
    ) -> Result<ClashOutcome, String> {
        let b = self.build(province_before, inputs, bell, seed)?;
        clash::resolve_clash(&clash::frontier_ruleset(), &b.input()).map_err(|e| format!("{e:?}"))
    }
}

/// A fate's name in reports.
pub fn fate_name(f: &Fate) -> &'static str {
    match f {
        Fate::Stays { .. } => "Stays",
        Fate::Withdrew { .. } => "Withdrew",
        Fate::Bounced => "Bounced",
        Fate::Retreated => "Retreated",
        Fate::Destroyed => "Destroyed",
    }
}

/// The fighters of an outcome as report JSON.
pub fn fighters_json(o: &ClashOutcome) -> serde_json::Value {
    serde_json::Value::Array(
        o.fighters
            .iter()
            .map(|f| {
                let tile = match f.fate {
                    Fate::Stays { tile } | Fate::Withdrew { tile } => Some(tile),
                    _ => None,
                };
                serde_json::json!({
                    "id": f.id.to_string(), "arrival": f.arrival, "troops": f.troops,
                    "stamina": f.stamina, "fate": fate_name(&f.fate), "tile": tile, "engaged": f.engaged,
                })
            })
            .collect(),
    )
}
