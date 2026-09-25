//! Season genesis: map, capitals, starting units, neutral actors (§2, §3).

use crate::economy::order_budget;
use crate::fixed::milli;
use crate::gov::{Credit, Nation};
use crate::map::{generate, territory_radius};
use crate::params::Ruleset;
use crate::rng::{rand_id, Seed};
use crate::state::{
    pair_count, Achievements, City, CityState, Civ, Owner, Pool, Relation, Scores, Specialty,
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
    NATIONS
        .iter()
        .take(n)
        .map(|name| Entry {
            name: String::from(*name),
            treasury: 0,
        })
        .collect()
}

/// Build tick-0 state. The map comes from `map_seed(world_seed,
/// season_seed)`; `season_seed` also drives everything random inside the
/// season.
pub fn new_season(
    rules: &Ruleset,
    world_seed: &Seed,
    season_seed: &Seed,
    entries: &[Entry],
) -> Result<WorldState, RulesError> {
    check_entries(rules, entries)?;
    let generated = generate(rules, &map_seed(world_seed, season_seed), entries.len())?;
    season_from_map(rules, world_seed, season_seed, entries, generated)
}

/// The seed the map is generated from (§2.4): the operator's public
/// `world_seed` mixed with the season seed, which only exists once
/// registration has closed. So nobody can compute the map before the nations
/// are chosen, and a world seed cannot be picked for one. Limit: the season
/// seed mixes a recent slot hash when `StartSeason` runs, so whoever sends
/// it could try slots (on a symmetric map every start is alike; this only
/// moves which nation neighbours which). A VRF would close it.
pub fn map_seed(world_seed: &Seed, season_seed: &Seed) -> Seed {
    crate::hash::sha256(&[b"permutation-rules/map-seed", world_seed, season_seed])
}

/// Which generated start each nation gets (§2.4): nation `i` starts at
/// `starts[start_order(..)[i]]`. The permutation is drawn from the season
/// seed, so the order in which generation placed the starts (the first one
/// at random, later ones pushed outwards) is not tied to any nation.
pub fn start_order(season_seed: &Seed, n: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|i| (rand_id(season_seed, b"start-order", *i as u64), *i));
    order
}

/// Entry rules checked before any generation work (§3.1).
pub fn check_entries(rules: &Ruleset, entries: &[Entry]) -> Result<(), RulesError> {
    if entries.len() < 2 || entries.len() > rules.max_civs as usize {
        return Err(RulesError::MapGeneration(
            "a season needs 2..=max_civs nations",
        ));
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
        return Err(RulesError::MapGeneration(
            "map was generated for a different number of civilizations",
        ));
    }
    let mut map = generated.map;

    let mut civs = Vec::with_capacity(n);
    let mut cities = Vec::with_capacity(n);
    let mut units = Vec::with_capacity(2 * n);

    let order = start_order(season_seed, n);
    for (i, entry) in entries.iter().enumerate() {
        let start = &generated.starts[order[i]];
        let civ_id = i as u16;
        let city_id = cities.len() as u32;
        map.claim_territory(city_id, *start, territory_radius(1));
        cities.push(City::founded(city_id, civ_id, *start, 0, rules));
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
            contract_income: 0,
            scores: Scores::default(),
            achievements: Achievements {
                trade: vec![0; n + 1],
                ..Achievements::default()
            },
        });
    }

    let city_states = generated
        .city_states
        .iter()
        .enumerate()
        .map(|(i, hex)| {
            // On a symmetric map the specialties repeat around the borders
            // (one rotating offset per season), so every nation's two
            // neighbouring city-states differ and all nations get the same
            // mix up to rotation; otherwise each is drawn on its own.
            let draw = if generated.symmetric {
                rand_id(season_seed, b"specialty", 0) + i as u64
            } else {
                rand_id(season_seed, b"specialty", i as u64)
            };
            let specialty = match draw % 3 {
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
        relations: vec![Relation::Peace; pair_count(n)],
        grievance: vec![0; n * n],
        proposals: Vec::new(),
        truce_until: vec![0; pair_count(n)],
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
        home_snapshot: Vec::new(),
        pact_last: vec![None; pair_count(n)],
        contracts: Vec::new(),
        next_contract: 0,
        merit_log: Vec::new(),
    })
}

#[cfg(test)]
mod seeds {
    use super::*;
    use crate::params::Preset;

    #[test]
    fn start_order_is_a_permutation_drawn_from_the_season_seed() {
        for n in 2..=8 {
            let mut o = start_order(&[7; 32], n);
            o.sort();
            assert_eq!(o, (0..n).collect::<Vec<_>>());
        }
        let orders: alloc::collections::BTreeSet<Vec<usize>> =
            (0..20u8).map(|k| start_order(&[k; 32], 6)).collect();
        assert!(orders.len() > 10, "the order depends on the season seed");
    }

    /// The same public world seed gives a different map for another season
    /// seed: the map is unknown until registration closes (§2.4).
    #[test]
    fn the_map_depends_on_the_season_seed() {
        let rules = Ruleset::new(Preset::Blitz);
        let a = new_season(&rules, &[1; 32], &[2; 32], &nation_entries(6)).unwrap();
        let b = new_season(&rules, &[1; 32], &[3; 32], &nation_entries(6)).unwrap();
        assert_ne!(a.map.tiles, b.map.tiles);
        // Nation i's capital is generated start order[i].
        let g = generate(&rules, &map_seed(&[1; 32], &[2; 32]), 6).unwrap();
        let order = start_order(&[2; 32], 6);
        for (i, civ) in a.civs.iter().enumerate() {
            let cap = a.cities[civ.capital.unwrap() as usize].hex;
            assert_eq!(cap, g.starts[order[i]]);
        }
    }
}
