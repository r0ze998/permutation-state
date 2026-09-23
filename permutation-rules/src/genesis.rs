//! Season genesis: map, capitals, starting units, neutral actors (§2, §3).

use crate::buildings::BuildingSet;
use crate::economy::order_budget;
use crate::fixed::milli;
use crate::map::{generate, territory_radius};
use crate::params::Ruleset;
use crate::rng::{rand_id, Seed};
use crate::state::{
    City, CityState, Civ, DeclaredKind, Focus, Owner, Relation, Scores, Specialty, StandingRule,
    Unit, WorldState,
};
use crate::tech::TechSet;
use crate::units::UnitType;
use crate::RulesError;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use sha2::{Digest, Sha256};

/// One paid entry (§3.1).
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub declared_kind: DeclaredKind,
    pub payout_wallet: [u8; 32],
}

/// Build tick-0 state. `world_seed` drives terrain (first season only);
/// `season_seed` drives everything random inside the season.
pub fn new_season(
    rules: &Ruleset,
    world_seed: &Seed,
    season_seed: &Seed,
    entries: &[Entry],
) -> Result<WorldState, RulesError> {
    let n = entries.len();
    for e in entries {
        let same_wallet = entries
            .iter()
            .filter(|o| o.payout_wallet == e.payout_wallet)
            .count();
        if same_wallet > rules.max_civs_per_wallet as usize {
            return Err(RulesError::WalletCap);
        }
    }
    let generated = generate(rules, world_seed, n)?;
    let mut map = generated.map;

    let mut civs = Vec::with_capacity(n);
    let mut cities = Vec::with_capacity(n);
    let mut units = Vec::with_capacity(2 * n);

    for (i, (entry, start)) in entries.iter().zip(generated.starts.iter()).enumerate() {
        let civ_id = i as u16;
        let city_id = cities.len() as u32;
        map.claim_territory(city_id, *start, territory_radius(1));
        cities.push(City {
            id: city_id,
            owner: Some(civ_id),
            founder: civ_id,
            founded_tick: 0,
            hex: *start,
            pop: 1,
            food: 0,
            prod: 0,
            buildings: BuildingSet::default(),
            loyalty: 100,
            defense: (rules.city_defense_base + 1) * 1000,
            attacked_this_tick: false,
            focus: Focus::Balanced,
            queue: Vec::new(),
            captured_tick: None,
            captured_from: None,
            scored_by: Vec::new(),
            razing: None,
            heritage_until: None,
            heritage_bonus: 0,
            alive: true,
        });
        let new_unit = |id: usize, unit_type: UnitType, troops: u32| Unit {
            id: id as u32,
            owner: Owner::Civ(civ_id),
            unit_type,
            troops,
            hex: *start,
            path: Vec::new(),
            last_moved: None,
            used_full_mp: false,
            standing: StandingRule::None,
            alive: true,
        };
        units.push(new_unit(units.len(), UnitType::Scout, 1000));
        units.push(new_unit(
            units.len(),
            UnitType::Spearman,
            rules.start_spearmen * 1000,
        ));
        civs.push(Civ {
            id: civ_id,
            name: entry.name.clone(),
            declared_kind: entry.declared_kind,
            payout_wallet: entry.payout_wallet,
            joined_tick: 0,
            gold: milli(rules.start_gold as i64),
            science_store: 0,
            influence: 0,
            iron: 0,
            horses: 0,
            techs: TechSet::default(),
            research_queue: Vec::new(),
            order_bank: 0,
            tick_budget: order_budget(rules, 1),
            deficit: false,
            war_weariness: 0,
            last_aggression: None,
            ever_allied: false,
            capital: Some(city_id),
            last_city_lost: None,
            protection_lost: false,
            active_ticks: 0,
            exchange_spent: 0,
            scores: Scores {
                max_pop: 1,
                ..Scores::default()
            },
        });
    }

    let city_states = generated
        .city_states
        .iter()
        .enumerate()
        .map(|(i, hex)| {
            let specialty = match rand_id(season_seed, b"specialty", i as u64) % 3 {
                0 => Specialty::Scientific,
                1 => Specialty::Mercantile,
                _ => Specialty::Agrarian,
            };
            CityState {
                id: i as u16,
                hex: *hex,
                pop: 3,
                defense: (8 + 2 * 3) * 1000,
                specialty,
                influence: vec![0; n],
                suzerain: None,
                captured_by: None,
            }
        })
        .collect();

    let mut genesis = Sha256::new();
    genesis.update(b"permutation-rules/genesis");
    genesis.update(world_seed);
    genesis.update(season_seed);

    Ok(WorldState {
        ruleset_hash: rules.hash(),
        season_seed: *season_seed,
        tick: 0,
        phase_cursor: 0,
        tick_seed: [0; 32],
        map,
        civs,
        cities,
        units,
        city_states,
        hubs: generated.hubs,
        relations: vec![Relation::Peace; n * n.saturating_sub(1) / 2],
        grievance: vec![0; n * n],
        event_head: genesis.finalize().into(),
    })
}
