//! The clash input of one province-bell, rebuilt from the chain's own
//! bytes (V7's replay; contract §5.11 ResolveFromInputs).
//!
//! ResolveFromInputs builds the kernel's `ClashInput` from the Province as
//! it stood before the resolve and the gathered ClashInputs. The program's
//! builder is W4-A's (a stub on the wave-3 base this unit was cut from);
//! [`ContractBuilder`] follows the §5.11 text: residents in state 1 with
//! `from_bell ≤ bell` at `Host::values_at(bell)` fighting at Hold;
//! garrisons from the site mirror at `GarrisonState::at(bell)` with walls
//! effective at the bell; the present arrival records; the terrain from the
//! compact arrays; relations from the Province; the storage room from the
//! entry states (`pending[f]` = entries in state 2 of faction f,
//! `storage_free = 56 − entries in states 1–3`, I-43); the camp, if present,
//! as a NEUTRAL garrison. The lazy camp respawn of a day's first resolve
//! (I-56) is not modelled (W4-A's builder is normative; the integrator
//! swaps it in through [`ClashBuilder`] when it lands, as the herald does).

use frontier_abi::addr::holding_key_of_host;
use frontier_abi::entry::{read_entry, unit_from_u8};
use frontier_abi::layout::clash::clash_inputs as CI;
use frontier_abi::layout::province::{entry as E, province as PV, site as S};
use permutation_rules::frontier::clash::{
    self, ClashInput, ClashOutcome, Fighter, Garrison, Occupancy, Relations, NEUTRAL,
};
use permutation_rules::frontier::geometry::{ProvinceCoord, PROVINCE_TILES};
use permutation_rules::frontier::host::GarrisonState;
use permutation_rules::frontier::stance::{Posture, Stance};
use permutation_rules::frontier::terrain::ProvinceTerrain;
use permutation_rules::hash::sha256;
use permutation_rules::map::{Terrain, TileResource};

use fclient::decode::{ClashInputs, Province};

/// Builds and resolves the clash of a province-bell.
pub trait ClashBuilder {
    fn name(&self) -> &'static str;
    fn build(
        &self,
        province_before: &[u8],
        inputs: &[u8],
        bell: u32,
        seed: &[u8; 32],
    ) -> Result<Built, String>;
}

/// Terrain classes in declaration order (the Province's `terrain[i]`).
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
/// Stances in plaintext byte order (0 Hold, 1 Assault, 2 Flank, 3 Brace).
pub const STANCES: [Stance; 4] = [Stance::Hold, Stance::Assault, Stance::Flank, Stance::Brace];

/// The kernel terrain of a Province.
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
                    .ok_or(format!("resource byte {r}"))?,
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

/// The owned parts of a `ClashInput`.
#[derive(Clone, Debug)]
pub struct Built {
    pub province: ProvinceCoord,
    pub bell: u32,
    pub seed: [u8; 32],
    pub terrain: ProvinceTerrain,
    pub residents: Vec<Fighter>,
    pub garrisons: Vec<Garrison>,
    pub arrivals: Vec<Fighter>,
    /// ClashInputs position of each arrival (`faction × 4 + i`).
    pub arrival_pos: Vec<usize>,
    pub relations: Relations,
    pub occupancy: Occupancy,
}

impl Built {
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

/// The builder that follows the contract text (see the module note).
pub struct ContractBuilder;

impl ClashBuilder for ContractBuilder {
    fn name(&self) -> &'static str {
        "contract-5.11-w4d"
    }
    fn build(
        &self,
        province_before: &[u8],
        inputs: &[u8],
        bell: u32,
        seed: &[u8; 32],
    ) -> Result<Built, String> {
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
        let mut arrival_pos = vec![];
        if let Some(ci) = &ci {
            for (k, a) in ci
                .arrivals
                .iter()
                .enumerate()
                .filter(|(_, a)| a.present == 1)
            {
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
                arrival_pos.push(k);
            }
        }
        Ok(Built {
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
        })
    }
}

/// CLASH's input digest as the synthetic fixture writes it: `sha256(
/// "PSF-CLASH-INPUTS-v1" ‖ the 24 arrival records ‖ le32(bell))`. **Not
/// pinned by the contract** (§6 names the field only); V7 does not judge
/// it until W4-A pins the program's (W4-D notes, request to the
/// integrator).
pub fn provisional_input_digest(inputs: &[u8], bell: u32) -> [u8; 32] {
    let rec = inputs.get(CI::ARRIVALS..CI::ARRIVALS + 960).unwrap_or(&[]);
    sha256(&[b"PSF-CLASH-INPUTS-v1", rec, &bell.to_le_bytes()])
}

/// SKIP's quiet digest as the synthetic fixture writes it (not pinned;
/// not judged).
pub fn provisional_quiet_digest(p: i32, q: i32, b0: u32, n: u8) -> [u8; 32] {
    sha256(&[
        b"PSF-QUIET-v1",
        &p.to_le_bytes(),
        &q.to_le_bytes(),
        &b0.to_le_bytes(),
        &[n],
    ])
}
