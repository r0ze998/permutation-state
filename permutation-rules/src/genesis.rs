//! Season genesis: map, capitals, starting units, neutral actors (§2, §3).

use crate::buildings::BuildingSet;
use crate::economy::order_budget;
use crate::fixed::milli;
use crate::map::{generate, territory_radius};
use crate::params::Ruleset;
use crate::rng::{rand_id, Seed};
use crate::gov::{Credit, Nation};
use crate::state::{
    Achievements, City, CityState, Civ, Focus, Owner, Pool, Relation, Scores, Specialty,
    StandingRule, Unit, WorldState,
};
use crate::tech::TechSet;
use crate::units::UnitType;
use crate::RulesError;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// One nation of the season (V5 §4). Members join nations; nations are
/// not players.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Entry {
    pub name: String,
    /// USDC (6 decimals) in the nation's treasury at the start (deposits
    /// made during registration, V5 §7.5).
    pub treasury: u64,
}

/// The six nations of a V5 season (V5 §4).
pub const NATIONS: [&str; 6] = ["Aster", "Borealis", "Cinder", "Dunmar", "Ember", "Fjordal"];

pub fn nation_entries(n: usize) -> Vec<Entry> {
    NATIONS.iter().take(n).map(|name| Entry { name: String::from(*name), treasury: 0 }).collect()
}

/// Build tick-0 state. `world_seed` drives terrain (first season only);
/// `season_seed` drives everything random inside the season.
pub fn new_season(
    rules: &Ruleset,
    world_seed: &Seed,
    season_seed: &Seed,
    entries: &[Entry],
) -> Result<WorldState, RulesError> {
    check_entries(rules, entries)?;
    let generated = generate(rules, world_seed, entries.len())?;
    season_from_map(rules, world_seed, season_seed, entries, generated)
}

/// Entry rules checked before any generation work (§3.1).
pub fn check_entries(rules: &Ruleset, entries: &[Entry]) -> Result<(), RulesError> {
    if entries.len() < 2 || entries.len() > rules.max_civs as usize {
        return Err(RulesError::MapGeneration("a season needs 2..=max_civs nations"));
    }
    Ok(())
}

/// Tick-0 state from a generated map (the second half of `new_season`, for
/// callers that ran `map::MapJob` step by step).
pub fn season_from_map(
    rules: &Ruleset,
    world_seed: &Seed,
    season_seed: &Seed,
    entries: &[Entry],
    generated: crate::map::Generated,
) -> Result<WorldState, RulesError> {
    check_entries(rules, entries)?;
    let n = entries.len();
    if generated.starts.len() != n {
        return Err(RulesError::MapGeneration("map was generated for a different number of civilizations"));
    }
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
            queue_credit: Credit::NONE,
            steward_credit: Credit::NONE,
            captured_tick: None,
            captured_from: None,
            capture_scores: false,
            razing: None,
            heritage_until: None,
            heritage_bonus: 0,
            standing: crate::state::CityStanding::DEFAULT,
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
            gold: milli(rules.start_gold as i64),
            science_store: 0,
            influence: 0,
            iron: 0,
            horses: 0,
            techs: TechSet::default(),
            research_queue: Vec::new(),
            research_credit: Credit::NONE,
            tick_budget: order_budget(rules, 1),
            deficit: false,
            troops_lost: 0,
            last: Default::default(),
            war_weariness: 0,
            last_aggression: None,
            ever_allied: false,
            capital: Some(city_id),
            last_city_lost: None,
            protection_lost: false,
            last_star_gate: None,
            usdc: entry.treasury,
            market_spent: 0,
            exchange_bought: [0; 5],
            scores: Scores::default(),
            achievements: Achievements { trade: vec![0; n + 1], ..Achievements::default() },
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
                envoys: Vec::new(),
            }
        })
        .collect();

    let genesis = crate::hash::sha256(&[b"permutation-rules/genesis", world_seed, season_seed]);

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
        proposals: Vec::new(),
        truce_until: vec![0; n * n.saturating_sub(1) / 2],
        pools: vec![
            Pool {
                goods: rules.amm_seed_goods as i64 * 1000,
                gold: rules.amm_seed_gold as i64 * 1000,
            };
            2
        ],
        usdc_deposited: entries.iter().map(|e| e.treasury).sum(),
        exchange_vault: 0,
        exchange_ops: 0,
        event_head: genesis,
        implicit: Vec::new(),
        grievance_fresh: vec![0; n * n],
        members: Vec::new(),
        nations: vec![Nation::new(); n],
        tick_orders: Vec::new(),
        last_skipped: Vec::new(),
        deliveries: Vec::new(),
        merit_log: Vec::new(),
    })
}
