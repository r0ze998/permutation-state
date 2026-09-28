//! The clash input of one province-bell, rebuilt from the chain's own
//! bytes (V7's replay; contract §5.11 ResolveFromInputs).
//!
//! ResolveFromInputs builds the kernel's `ClashInput` from the Province as
//! it stood before the resolve and the gathered ClashInputs. Integ-W4
//! review: [`ContractBuilder`] is now the program's builder as
//! `fclient::clash_model` transcribes it (shared with the herald). The
//! wave-4 unit's reading of the §5.11 text was: residents in state 1 with
//! `from_bell ≤ bell` at `Host::values_at(bell)` fighting at Hold;
//! garrisons from the site mirror at `GarrisonState::at(bell)` with walls
//! effective at the bell; the present arrival records; the terrain from the
//! compact arrays; relations from the Province; the storage room from the
//! entry states (`pending[f]` = entries in state 2 of faction f,
//! `storage_free = 56 − entries in states 1–3`, I-43); the camp, if present,
//! as a NEUTRAL garrison. The lazy camp respawn of a day's first resolve
//! (I-56) is not modelled (W4-A's builder is normative; the integrator
//! swaps it in through [`ClashBuilder`] when it lands, as the herald does).

use frontier_abi::layout::clash::clash_inputs as CI;
use permutation_rules::frontier::clash::{
    self, ClashInput, ClashOutcome, Fighter, Garrison, Occupancy, Relations,
};
use permutation_rules::frontier::geometry::{ProvinceCoord, PROVINCE_TILES};
use permutation_rules::frontier::stance::Stance;
use permutation_rules::frontier::terrain::ProvinceTerrain;
use permutation_rules::hash::sha256;
use permutation_rules::map::{Terrain, TileResource};

use fclient::decode::Province;

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

/// The program's builder (`fclient::clash_model`, one transcription of
/// W4-A's `proc::clash::model::build` shared with the herald; integ-W4
/// review: the verifier's own §5.11 reading failed an honest program
/// season — camp troops, the 12-garrison cap, the day's camp check,
/// `dealt_bps` 0, id order).
pub struct ContractBuilder;

impl ClashBuilder for ContractBuilder {
    fn name(&self) -> &'static str {
        "program-model-fclient"
    }
    fn build(
        &self,
        province_before: &[u8],
        inputs: &[u8],
        bell: u32,
        seed: &[u8; 32],
    ) -> Result<Built, String> {
        let b = fclient::clash_model::build(province_before, inputs, bell, seed)?;
        Ok(Built {
            province: b.province,
            bell: b.bell,
            seed: b.seed,
            terrain: b.terrain,
            residents: b.residents,
            garrisons: b.garrisons,
            arrivals: b.arrivals,
            arrival_pos: b.arrival_pos,
            relations: b.relations,
            occupancy: b.occupancy,
        })
    }
}

/// CLASH's `input_digest` (contract v1.6 §22, W4-A §1): `sha256(
/// "PSF-CLASH-INPUT-v1" ‖ le32(b) ‖ seed ‖ province[SITE_MIRROR ..
/// TICKET_COHORTS] before ‖ inputs[ARRIVALS .. POSTURES])`.
pub fn input_digest(province_before: &[u8], inputs: &[u8], bell: u32, seed: &[u8; 32]) -> [u8; 32] {
    use frontier_abi::layout::province::province as PV;
    sha256(&[
        b"PSF-CLASH-INPUT-v1",
        &bell.to_le_bytes(),
        seed,
        province_before
            .get(PV::SITE_MIRROR..PV::TICKET_COHORTS)
            .unwrap_or(&[]),
        inputs.get(CI::ARRIVALS..CI::POSTURES).unwrap_or(&[]),
    ])
}

/// SKIP's `quiet_digest` (contract v1.6 §22): `sha256("PSF-QUIET-v1" ‖
/// le32(b0) ‖ n ‖ province[SITE_MIRROR .. TICKET_COHORTS] after)`.
pub fn quiet_digest(province_after: &[u8], b0: u32, n: u8) -> [u8; 32] {
    use frontier_abi::layout::province::province as PV;
    sha256(&[
        b"PSF-QUIET-v1",
        &b0.to_le_bytes(),
        &[n],
        province_after
            .get(PV::SITE_MIRROR..PV::TICKET_COHORTS)
            .unwrap_or(&[]),
    ])
}
