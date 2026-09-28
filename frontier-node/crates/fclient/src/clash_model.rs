//! The kernel input of one province-bell rebuilt from account bytes, and a
//! SkipQuiet replayed bell by bell, for the off-chain readers: the
//! herald's clash check, the verifier's V7 replay and itest's G14.
//!
//! **One copy (W4-A D8, integ-W5):** every rule here runs the program's own
//! model, `frontier_abi::clash_model` (moved there from the program's
//! `proc::clash::model`); this module only decodes, checks that the
//! Province and the ClashInputs are the same province-bell, and reshapes
//! the result for its callers. Until integ-W5 it was a transcription
//! (integ-W4 review: the herald's and the verifier's own copies disagreed
//! with the program; W5-C added the skip replay beside it).
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

use frontier_abi::clash_model as m;
use frontier_abi::layout::province::province as PV;
use permutation_rules::fixed::{Bps, BPS_ONE};
use permutation_rules::frontier::clash::{
    self, ClashInput, ClashOutcome, Fighter, Garrison, Occupancy, Relations,
};
use permutation_rules::frontier::geometry::{ProvinceCoord, PROVINCE_TILES};
use permutation_rules::frontier::stance::Stance;
use permutation_rules::frontier::terrain::ProvinceTerrain;
use permutation_rules::map::Terrain;

use crate::decode::{ClashInputs, Province};

pub use frontier_abi::clash_model::{CAMP_DOMAIN, DAY_BELLS, RESOURCES, TERRAINS};

fn err(e: m::ModelError) -> String {
    format!("clash model: {e}")
}

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

/// The camp after the day's check of `bell` (`pd`: the Province bytes;
/// the program's `camp_check`).
pub fn camp_at(
    pv: &Province,
    pd: &[u8],
    terrain: &ProvinceTerrain,
    bell: u32,
) -> Result<CampAt, String> {
    let _ = pv;
    Ok(match m::camp_check(pd, terrain, bell).map_err(err)? {
        Some((c, spawned)) => camp_of(&c, true, spawned),
        None => camp_of(&m::Camp::read(pd).map_err(err)?, false, false),
    })
}

fn camp_of(c: &m::Camp, checked: bool, spawned: bool) -> CampAt {
    CampAt {
        tile: c.tile,
        present: c.present(),
        troops: c.troops,
        gen: c.gen,
        checked,
        spawned,
    }
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
/// quiet test): the program's `model::build`.
pub fn build(
    province_before: &[u8],
    inputs: &[u8],
    bell: u32,
    seed: &[u8; 32],
) -> Result<BuiltInput, String> {
    let pv = Province::decode(province_before).map_err(|e| format!("province: {e:?}"))?;
    if !inputs.is_empty() {
        let ci = ClashInputs::decode(inputs).map_err(|e| format!("inputs: {e:?}"))?;
        if (ci.p, ci.q, ci.bell) != (pv.p, pv.q, bell) {
            return Err("inputs and province are different province-bells".into());
        }
    }
    let b = m::build(
        province_before,
        (!inputs.is_empty()).then_some(inputs),
        bell,
    )
    .map_err(err)?;
    let camp = camp_of(
        &b.camp,
        b.camp_checked.is_some(),
        b.camp_checked == Some(true),
    );
    Ok(BuiltInput {
        province: b.coord,
        bell,
        seed: *seed,
        terrain: b.terrain,
        residents: b.residents,
        garrisons: b.garrisons,
        arrivals: b.arrivals,
        arrival_pos: b.arr_pos.iter().map(|&k| k as usize).collect(),
        relations: b.relations,
        occupancy: b.occupancy,
        camp,
    })
}

// ------------------------------------------------------------------ skips

/// SKIP's `quiet_digest` (§22): `sha256("PSF-QUIET-v1" ‖ le32(b0) ‖ n ‖
/// province[SITE_MIRROR .. TICKET_COHORTS] after)` (the program's).
pub fn quiet_digest(province_after: &[u8], b0: u32, n: u8) -> Result<[u8; 32], String> {
    m::quiet_digest(province_after, b0, n).map_err(err)
}

/// CLASH's `input_digest` (§22): `sha256("PSF-CLASH-INPUT-v1" ‖ le32(b) ‖
/// seed ‖ province[SITE_MIRROR .. TICKET_COHORTS] before ‖
/// inputs[ARRIVALS .. POSTURES])` (the program's).
pub fn input_digest(
    province_before: &[u8],
    inputs: &[u8],
    bell: u32,
    seed: &[u8; 32],
) -> Result<[u8; 32], String> {
    m::input_digest(province_before, inputs, bell, seed).map_err(err)
}

/// One bell of a replayed quiet skip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkipBell {
    pub bell: u32,
    /// `clash::is_quiet` on the roster the bell froze (after the day's camp
    /// check), asked of the kernel at **every** bell (the program asks it
    /// at most once per transaction and otherwise uses its trivial test).
    pub quiet: bool,
    /// The bell's settle or camp spawn changed the Province.
    pub changed: bool,
    /// The camp the bell saw.
    pub camp: CampAt,
}

/// A SkipQuiet run replayed natively (G14; the verifier's V7 may take it
/// for its per-bell SKIP check): the Province the committed bells leave.
#[derive(Clone, Debug)]
pub struct SkipReplay {
    pub after: Vec<u8>,
    pub bells: Vec<SkipBell>,
    /// Camps spawned by the day's checks: `(tile, troops, day)`, one CAMP
    /// record each.
    pub spawns: Vec<(u8, u32, u32)>,
}

impl SkipReplay {
    /// The bells the kernel did not find quiet.
    pub fn loud(&self) -> Vec<u32> {
        self.bells
            .iter()
            .filter(|b| !b.quiet)
            .map(|b| b.bell)
            .collect()
    }
}

/// The program's `model::next_due`: the first bell whose settle changes
/// something (`u32::MAX` if none).
pub fn next_due(pd: &[u8]) -> Result<u32, String> {
    m::next_due(pd).map_err(err)
}

/// The program's `model::settle_bell`: settles the pending changes of bell
/// `b`; `true` when anything changed.
pub fn settle_bell(pd: &mut [u8], b: u32) -> Result<bool, String> {
    m::settle_bell(pd, b).map_err(err)
}

/// The program's `model::finish_bell`: `resolved_next = b + 1`; when
/// anything changed, `roster_epoch += 1` and `n_entries` recounted.
pub fn finish_bell(pd: &mut [u8], b: u32, changed: bool) -> Result<(), String> {
    m::finish_bell(pd, b, changed).map_err(err)
}

/// Replays the `n` bells a SKIP committed from `b0` on the Province bytes
/// before it, the program's way (the day's camp check, the quiet test, the
/// settle from the first due bell, `resolved_next`), asking
/// `clash::is_quiet` at every bell. The caller compares `after` with the
/// Province the SKIP left and `spawns` with its CAMP records.
pub fn skip_replay(province_before: &[u8], b0: u32, n: u8) -> Result<SkipReplay, String> {
    let mut pd = province_before.to_vec();
    let rn = frontier_abi::bytes::rd_u32(&pd, PV::RESOLVED_NEXT).ok_or("short province")?;
    if rn != b0 {
        return Err(format!(
            "the Province is resolved to {rn}, the skip starts at {b0}"
        ));
    }
    let mut due = next_due(&pd)?;
    let mut bells = Vec::with_capacity(n as usize);
    let mut spawns = vec![];
    for k in 0..n as u32 {
        let b = b0.checked_add(k).ok_or("bell overflow")?;
        let mut changed = false;
        let terrain = m::terrain_of(&pd).map_err(err)?;
        let camp = match m::camp_check(&pd, &terrain, b).map_err(err)? {
            Some((c, spawned)) => {
                c.write(&mut pd).map_err(err)?;
                if spawned {
                    spawns.push((c.tile, c.troops, b / DAY_BELLS));
                    changed = true;
                }
                camp_of(&c, true, spawned)
            }
            None => camp_of(&m::Camp::read(&pd).map_err(err)?, false, false),
        };
        let quiet = build(&pd, &[], b, &[0; 32])?.quiet()?;
        if b >= due {
            changed |= settle_bell(&mut pd, b)?;
            due = next_due(&pd)?;
        }
        finish_bell(&mut pd, b, changed)?;
        bells.push(SkipBell {
            bell: b,
            quiet,
            changed,
            camp,
        });
    }
    Ok(SkipReplay {
        after: pd,
        bells,
        spawns,
    })
}
