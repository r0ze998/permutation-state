//! `resolve_tick` (§15). Phases run in a fixed order; the next phase index is
//! stored in state (`phase_cursor`) so resolution can be split across
//! transactions and resumed by anyone (§15.1, §15.2).
//!
//! Implementation status of each phase is listed on `run_phase`.

use crate::buildings::{info as building_info, Building};
use crate::economy::{
    amenities, apply_amenities, city_upkeep, city_yield, growth_threshold, order_budget,
    stalemate_multiplier, tech_cost, unit_upkeep,
};
use crate::fixed::{apply_bps, milli, MILLI};
use crate::hex::Hex;
use crate::map::territory_radius;
use crate::orders::{next_bank, validate_batch, Order, OrderBatch};
use crate::params::Ruleset;
use crate::rng::{tick_seed, tie_key, Seed};
use crate::scoring::{concord_tick, dominion_tick};
use crate::state::{CivId, Owner, QueueItem, Relation, StandingRule, Unit, WorldState};
use crate::tech::Tech;
use crate::units::{stats, UnitType};
use crate::RulesError;
use alloc::vec::Vec;

pub const PHASE_COUNT: u8 = 12;

/// Everything outside the state that a tick consumes. Replaying the same
/// inputs over the same state must give the same root (§17 invariant 7).
#[derive(Clone, Debug, Default)]
pub struct TickInput {
    /// MagicBlock VRF output for this tick (§0.2).
    pub vrf: Seed,
    /// At most one batch per civ is used; extra or invalid batches are ignored.
    pub batches: Vec<OrderBatch>,
}

/// Run all remaining phases of the current tick. Returns the new state root.
pub fn resolve_tick(
    state: &mut WorldState,
    rules: &Ruleset,
    input: &TickInput,
) -> Result<[u8; 32], RulesError> {
    for phase in state.phase_cursor..PHASE_COUNT {
        run_phase(state, rules, input, phase)?;
    }
    state.state_root()
}

/// Run exactly one phase.
///
/// | # | Phase | Status |
/// |---|---|---|
/// | 0 | Seed | done |
/// | 1 | Diplomacy | done (see `diplomacy`): war, peace, NAPs with bonds, alliances, proposals |
/// | 2 | Economy orders | queue, focus, research, purchase, found city, transfers, envoys. TODO: AMM, Exchange |
/// | 3 | Standing rules | TODO |
/// | 4 | Movement | done (paths, MP, occupancy, territory, protection, tie-break) |
/// | 5 | Combat | done (see `battle`); barbarian and standing-rule attacks TODO |
/// | 6 | Production and growth | done |
/// | 7 | Upkeep | done |
/// | 8 | Society | done: war weariness (incl. casualties), loyalty, grievance decay, city regen |
/// | 9 | Neutral actors | city-state growth/regen, suzerainty cycle reset. TODO: Crisis |
/// | 10 | Scoring | done (coalitions TODO) |
/// | 11 | Commit | done |
pub fn run_phase(
    state: &mut WorldState,
    rules: &Ruleset,
    input: &TickInput,
    phase: u8,
) -> Result<(), RulesError> {
    if state.tick >= rules.ticks_per_season {
        return Err(RulesError::SeasonOver);
    }
    if phase != state.phase_cursor {
        return Err(RulesError::PhaseOutOfOrder {
            expected: state.phase_cursor,
            got: phase,
        });
    }
    match phase {
        0 => phase_seed(state, input),
        1 => crate::diplomacy::phase_diplomacy(state, rules, input),
        2 => phase_economy_orders(state, rules, input),
        3 => {} // TODO(§13): compile standing rules into implicit orders.
        4 => phase_movement(state, rules, input),
        5 => crate::battle::phase_combat(state, rules, input),
        6 => phase_production(state, rules),
        7 => phase_upkeep(state),
        8 => phase_society(state, rules),
        9 => phase_neutral(state, rules),
        10 => phase_scoring(state, rules, input),
        11 => phase_commit(state, rules),
        _ => {
            return Err(RulesError::PhaseOutOfOrder {
                expected: state.phase_cursor,
                got: phase,
            })
        }
    }
    if phase + 1 == PHASE_COUNT {
        state.phase_cursor = 0;
        state.tick += 1;
    } else {
        state.phase_cursor = phase + 1;
    }
    Ok(())
}

/// The batch used for each civ, in civ order, with its cost. First valid batch wins.
pub(crate) fn accepted(
    state: &WorldState,
    rules: &Ruleset,
    input: &TickInput,
) -> Vec<(CivId, u32, Vec<Order>)> {
    let mut out = Vec::new();
    for civ in 0..state.civs.len() as u16 {
        if let Some((batch, cost)) = input
            .batches
            .iter()
            .filter(|b| b.civ == civ)
            .find_map(|b| validate_batch(state, rules, b).ok().map(|c| (b, c)))
        {
            out.push((civ, cost, batch.orders.clone()));
        }
    }
    out
}

// ---------------------------------------------------------------- phase 0

fn phase_seed(state: &mut WorldState, input: &TickInput) {
    state.tick_seed = tick_seed(&state.season_seed, &input.vrf, state.tick);
    for c in &mut state.cities {
        c.attacked_this_tick = false;
    }
    for civ in &mut state.civs {
        civ.deficit = false;
        civ.troops_lost = 0;
    }
}

// ---------------------------------------------------------------- phase 1

// ---------------------------------------------------------------- phase 2

fn phase_economy_orders(state: &mut WorldState, rules: &Ruleset, input: &TickInput) {
    for (civ, _, orders) in accepted(state, rules, input) {
        for order in orders {
            match order {
                Order::FoundCity { settler } => found_city(state, rules, civ, settler),
                Order::SetQueue { city, items } => set_queue(state, civ, city, items),
                Order::SetFocus { city, focus } => {
                    if let Some(c) = owned_city_mut(state, civ, city) {
                        c.focus = focus;
                    }
                }
                Order::SetResearch { techs } => set_research(state, civ, techs),
                Order::Purchase { city, gold } => purchase(state, rules, civ, city, gold),
                _ => {}
            }
        }
    }
    crate::diplomacy::apply_transfers(state, rules, input);
    crate::diplomacy::apply_envoys(state, rules, input);
    crate::markets::apply_amm(state, rules, input);
    crate::markets::apply_exchange(state, rules, input);
}

/// `FoundCity` (§4.2, §5.7): the settler founds a city where it stands.
fn found_city(state: &mut WorldState, rules: &Ruleset, civ: CivId, settler: u32) {
    let Some(u) = state
        .units
        .get(settler as usize)
        .filter(|u| u.alive && u.owner == Owner::Civ(civ) && u.unit_type == UnitType::Settler)
    else {
        return;
    };
    let hex = u.hex;
    if crate::checks::found_city(state, rules, civ, settler).is_err() {
        return;
    }
    let id = state.cities.len() as u32;
    // Heritage (§3.4): the first city near an unclaimed ruin this season.
    let heritage = state
        .map
        .tiles
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            !t.heritage_claimed
                && t.ruin_peak_pop.is_some()
                && t.hex.distance(hex) <= rules.heritage_radius as u32
        })
        .min_by_key(|(i, t)| (t.hex.distance(hex), *i))
        .map(|(i, t)| (i, t.ruin_peak_pop.unwrap_or(0) as u32 / 2));
    let (heritage_until, heritage_bonus) = match heritage {
        Some((i, bonus)) => {
            state.map.tiles[i].heritage_claimed = true;
            (Some(state.tick + rules.heritage_ticks), bonus)
        }
        None => (None, 0),
    };
    state.cities.push(crate::state::City {
        id,
        owner: Some(civ),
        founder: civ,
        founded_tick: state.tick,
        hex,
        pop: 1,
        food: 0,
        prod: 0,
        buildings: Default::default(),
        loyalty: 100,
        defense: (rules.city_defense_base + 1) * 1000,
        attacked_this_tick: false,
        focus: crate::state::Focus::Balanced,
        queue: Vec::new(),
        captured_tick: None,
        captured_from: None,
        scored_by: Vec::new(),
        capture_scores: false,
        razing: None,
        heritage_until,
        heritage_bonus,
        alive: true,
    });
    if let Some(t) = state.map.tile_mut(hex) {
        t.owner_city = Some(id); // the centre always belongs to the new city
    }
    state.map.claim_territory(id, hex, territory_radius(1));
    state.units[settler as usize].alive = false;
    if state.civs[civ as usize].capital.is_none() {
        state.civs[civ as usize].capital = Some(id);
    }
    state.push_event(b"found_city", &id.to_le_bytes());
}

fn owned_city_mut(
    state: &mut WorldState,
    civ: CivId,
    city: u32,
) -> Option<&mut crate::state::City> {
    state
        .cities
        .get_mut(city as usize)
        .filter(|c| c.alive && c.owner == Some(civ))
}

fn item_valid(state: &WorldState, civ: CivId, city: u32, item: &QueueItem) -> bool {
    crate::checks::queue_item(state, civ, city, item).is_ok()
}

fn set_queue(state: &mut WorldState, civ: CivId, city: u32, items: Vec<QueueItem>) {
    if owned_city_mut(state, civ, city).is_none() {
        return;
    }
    if !items.iter().all(|i| item_valid(state, civ, city, i)) {
        return; // dropped without refund (§4.1)
    }
    state.cities[city as usize].queue = items;
}

fn set_research(state: &mut WorldState, civ: CivId, techs: Vec<Tech>) {
    let c = &mut state.civs[civ as usize];
    let mut planned = c.techs;
    for t in &techs {
        if crate::checks::research(planned, *t).is_err() {
            return;
        }
        planned.insert(*t);
    }
    c.research_queue = techs;
}

fn item_cost(
    rules: &WorldState,
    ruleset: &Ruleset,
    city: &crate::state::City,
    item: &QueueItem,
) -> i64 {
    let prod = match *item {
        QueueItem::Building(b) => {
            let base = building_info(b).prod_cost as i64;
            if b.is_star_gate() {
                apply_bps(base, stalemate_multiplier(ruleset, rules.tick))
            } else {
                base
            }
        }
        QueueItem::Troops { unit, n } => {
            let base = stats(unit).prod_cost as i64 * n as i64;
            if city.buildings.has(Building::Barracks) {
                apply_bps(base, 7_500)
            } else {
                base
            }
        }
        QueueItem::Scout => stats(UnitType::Scout).prod_cost as i64,
        QueueItem::Settler => stats(UnitType::Settler).prod_cost as i64,
    };
    milli(prod)
}

fn purchase(state: &mut WorldState, rules: &Ruleset, civ: CivId, city: u32, gold: u32) {
    let Some(c) = state
        .cities
        .get(city as usize)
        .filter(|c| c.alive && c.owner == Some(civ))
    else {
        return;
    };
    let Some(item) = c.queue.first() else { return };
    let remaining = (item_cost(state, rules, c, item) - c.prod).max(0);
    let cap = apply_bps(remaining, rules.purchase_max_bps);
    let wanted = milli(gold as i64) / rules.purchase_gold_per_prod as i64;
    let prod = wanted
        .min(cap)
        .min(state.civs[civ as usize].gold / rules.purchase_gold_per_prod as i64);
    if prod <= 0 {
        return;
    }
    state.civs[civ as usize].gold -= prod * rules.purchase_gold_per_prod as i64;
    state.cities[city as usize].prod += prod;
}

// ---------------------------------------------------------------- phase 4

pub(crate) fn allied(state: &WorldState, a: CivId, b: CivId) -> bool {
    a != b && matches!(state.relation(a, b), Relation::Alliance { .. })
}

pub(crate) fn unit_civ(u: &Unit) -> Option<CivId> {
    match u.owner {
        Owner::Civ(c) => Some(c),
        Owner::Barbarian => None,
    }
}

/// Whether `mover` may enter `hex` (terrain, protected zones, borders). Public
/// so clients and agents can list legal moves with the engine's own rule.
pub fn may_enter(state: &WorldState, rules: &Ruleset, mover: Option<CivId>, hex: Hex) -> bool {
    crate::checks::enter(state, rules, mover, hex).is_ok()
}

/// Occupancy: one army or one civilian per tile; inside an own city, one of each (§7.2).
pub(crate) fn tile_free_for(state: &WorldState, unit: &Unit, hex: Hex) -> bool {
    let is_civ = unit.unit_type.is_civilian();
    let mut occupants = state
        .units
        .iter()
        .filter(|u| u.alive && u.hex == hex && u.id != unit.id);
    let own_city = state
        .cities
        .iter()
        .any(|c| c.alive && c.hex == hex && c.owner.is_some() && c.owner == unit_civ(unit));
    occupants.all(|o| o.owner == unit.owner && own_city && o.unit_type.is_civilian() != is_civ)
}

fn phase_movement(state: &mut WorldState, rules: &Ruleset, input: &TickInput) {
    // 1. Accept MoveUnit orders (paths continue on later ticks without orders).
    for (civ, _, orders) in accepted(state, rules, input) {
        for order in orders {
            if let Order::MoveUnit { unit, path } = order {
                let Some(u) = state
                    .units
                    .get(unit as usize)
                    .filter(|u| u.alive && u.owner == Owner::Civ(civ))
                else {
                    continue;
                };
                let mut prev = u.hex;
                let valid = !path.is_empty()
                    && path.iter().all(|h| {
                        let ok = prev.distance(*h) == 1
                            && state.map.tile(*h).is_some_and(|t| t.terrain.is_passable());
                        prev = *h;
                        ok
                    });
                if valid {
                    state.units[unit as usize].path = path;
                }
            } else if let Order::Attack { army, .. } = order {
                // An attacking army holds its position this tick (§8, v0.1 clarification).
                if let Some(u) = state
                    .units
                    .get_mut(army as usize)
                    .filter(|u| u.alive && u.owner == Owner::Civ(civ))
                {
                    u.path.clear();
                }
            }
        }
    }

    // 2. Sub-steps (§15 phase 4).
    let full_mp: Vec<u8> = state
        .units
        .iter()
        .map(|u| stats(u.unit_type).movement)
        .collect();
    let mut mp_left = full_mp.clone();
    let mut moved = alloc::vec![false; state.units.len()];
    let max_mp = full_mp.iter().copied().max().unwrap_or(0);
    for _ in 0..max_mp {
        let mut intents: Vec<(u64, usize, Hex, u8)> = Vec::new();
        for (i, u) in state.units.iter().enumerate() {
            if !u.alive || mp_left[i] == 0 || u.path.is_empty() {
                continue;
            }
            let next = u.path[0];
            let terrain_cost = state
                .map
                .tile(next)
                .map_or(0, |t| t.terrain.info().move_cost);
            let cost = if u.unit_type == UnitType::Scout {
                1
            } else {
                terrain_cost
            };
            // A unit with full MP may always move one tile (§7.3).
            if cost == 0 || (mp_left[i] < cost && mp_left[i] != full_mp[i]) {
                mp_left[i] = 0;
                continue;
            }
            intents.push((tie_key(&state.tick_seed, u.id as u64), i, next, cost));
        }
        if intents.is_empty() {
            break;
        }
        intents.sort();
        for (_, i, next, cost) in intents {
            let unit = state.units[i].clone();
            if !may_enter(state, rules, unit_civ(&unit), next) {
                state.units[i].path.clear();
                mp_left[i] = 0;
                continue;
            }
            if !tile_free_for(state, &unit, next) {
                continue; // wait this sub-step; never swap (§15 phase 4)
            }
            let u = &mut state.units[i];
            u.hex = next;
            u.path.remove(0);
            mp_left[i] = mp_left[i].saturating_sub(cost);
            moved[i] = true;
        }
    }
    let tick = state.tick;
    for (i, u) in state.units.iter_mut().enumerate() {
        if moved[i] {
            u.last_moved = Some(tick);
        }
        u.used_full_mp = moved[i] && mp_left[i] == 0;
    }
}

// ---------------------------------------------------------------- phase 6

fn phase_production(state: &mut WorldState, rules: &Ruleset) {
    let mut last = alloc::vec![crate::state::LastYields::default(); state.civs.len()];
    for i in 0..state.cities.len() {
        let (owner, alive) = (state.cities[i].owner, state.cities[i].alive);
        let razing = state.cities[i].razing.is_some();
        let (Some(civ), true, false) = (owner, alive, razing) else {
            continue;
        };
        let c_ref = &state.civs[civ as usize];
        let is_capital = c_ref.capital == Some(i as u32);
        let has_philosophy = c_ref.techs.has(Tech::Philosophy);
        let ww = c_ref.war_weariness;
        let civ_cities = state.city_count(civ);

        let (y, worked) = city_yield(&state.map, &state.cities[i], is_capital, has_philosophy);
        let amen = amenities(&state.cities[i], civ_cities, ww);
        let (surplus, prod_bps) = apply_amenities(rules, y.food, state.cities[i].pop, amen);

        // Food and growth (§5.3).
        {
            let city = &mut state.cities[i];
            city.food += surplus * MILLI;
            let threshold = growth_threshold(city.pop) as i64 * MILLI;
            if city.food >= threshold {
                city.food -= threshold;
                city.pop += 1;
            } else if city.food < 0 {
                city.pop = city.pop.saturating_sub(1).max(1);
                city.food = 0;
            }
            city.prod += apply_bps(y.prod as i64 * MILLI, prod_bps);
            if city.heritage_until.is_some_and(|t| state.tick >= t) {
                city.heritage_until = None;
                city.heritage_bonus = 0;
            }
        }
        let (hex, pop, id) = (state.cities[i].hex, state.cities[i].pop, i as u32);
        state.map.claim_territory(id, hex, territory_radius(pop));

        // Civilization-level yields.
        {
            let l = &mut last[civ as usize];
            l.gold += y.gold;
            l.max_city_food_surplus = l.max_city_food_surplus.max(surplus.max(0) as u32);
            l.max_city_prod = l.max_city_prod.max(y.prod);
            let c = &mut state.civs[civ as usize];
            c.gold += y.gold as i64 * MILLI;
            c.science_store += y.science as i64 * MILLI;
            c.influence += y.influence as i64 * MILLI;
            c.scores.science_total += y.science as u64;
        }
        for t in worked {
            let tile = &mut state.map.tiles[t];
            if tile.reserve == 0 {
                continue;
            }
            match tile.resource {
                Some(crate::map::TileResource::Iron) => {
                    state.civs[civ as usize].iron += MILLI;
                    last[civ as usize].iron += 1;
                }
                Some(crate::map::TileResource::Horses) => {
                    state.civs[civ as usize].horses += MILLI;
                    last[civ as usize].horses += 1;
                }
                _ => continue,
            }
            tile.reserve -= 1;
        }

        complete_queue(state, rules, civ, i);
    }

    // City-state suzerain bonuses (§12.1).
    for cs in 0..state.city_states.len() {
        let (Some(civ), None) = (
            state.city_states[cs].suzerain,
            state.city_states[cs].captured_by,
        ) else {
            continue;
        };
        let c = &mut state.civs[civ as usize];
        match state.city_states[cs].specialty {
            crate::state::Specialty::Scientific => {
                c.science_store += 3 * MILLI;
                c.scores.science_total += 3;
            }
            crate::state::Specialty::Mercantile => {
                c.gold += 4 * MILLI;
                last[civ as usize].gold += 4;
            }
            crate::state::Specialty::Agrarian => {
                if let Some(cap) = c.capital {
                    state.cities[cap as usize].food += 2 * MILLI;
                }
            }
        }
    }
    for (c, l) in state.civs.iter_mut().zip(last) {
        c.last = l;
    }

    // Research (§6.2).
    for civ in 0..state.civs.len() {
        let cities = state.city_count(civ as u16);
        loop {
            let c = &mut state.civs[civ];
            let Some(&tech) = c.research_queue.first() else {
                break;
            };
            if c.techs.has(tech) || !c.techs.prereqs_met(tech) {
                c.research_queue.remove(0);
                continue;
            }
            let cost = tech_cost(rules, tech, cities) as i64 * MILLI;
            if c.science_store < cost {
                break;
            }
            c.science_store -= cost;
            c.techs.insert(tech);
            c.research_queue.remove(0);
        }
    }
}

fn complete_queue(state: &mut WorldState, rules: &Ruleset, civ: CivId, i: usize) {
    while let Some(item) = state.cities[i].queue.first().copied() {
        let cost = item_cost(state, rules, &state.cities[i], &item);
        if state.cities[i].prod < cost {
            return;
        }
        match item {
            QueueItem::Building(b) => {
                let city = &mut state.cities[i];
                city.buildings.insert(b);
                if b.is_star_gate() {
                    let stages = city.buildings.star_gate_stages();
                    let s = &mut state.civs[civ as usize].scores;
                    s.star_gate_stages = s.star_gate_stages.max(stages);
                    s.star_gate_tick = Some(state.tick);
                }
            }
            QueueItem::Troops { unit, n } => {
                let st = stats(unit);
                let (iron, horses) = (
                    milli((st.iron * n as u32) as i64),
                    milli((st.horses * n as u32) as i64),
                );
                let c = &state.civs[civ as usize];
                if c.iron < iron || c.horses < horses {
                    return; // wait for strategic resources
                }
                if !spawn(state, civ, i, unit, n as u32 * 1000) {
                    return; // no free tile: delayed (§5.6)
                }
                let c = &mut state.civs[civ as usize];
                c.iron -= iron;
                c.horses -= horses;
            }
            QueueItem::Scout => {
                if !spawn(state, civ, i, UnitType::Scout, 1000) {
                    return;
                }
            }
            QueueItem::Settler => {
                if state.cities[i].pop < rules.settler_min_pop {
                    return;
                }
                if !spawn(state, civ, i, UnitType::Settler, 1000) {
                    return;
                }
                state.cities[i].pop -= 1;
            }
        }
        let city = &mut state.cities[i];
        city.prod -= cost;
        city.queue.remove(0);
        // CityQueueRepeat (default on, §13): repeat the last unit item.
        if city.queue.is_empty() && !matches!(item, QueueItem::Building(_)) {
            city.queue.push(item);
        }
    }
}

/// Spawn on the city tile or the first free neighbour (§5.6). False = delayed.
fn spawn(
    state: &mut WorldState,
    civ: CivId,
    city: usize,
    unit_type: UnitType,
    troops: u32,
) -> bool {
    let center = state.cities[city].hex;
    let probe = Unit {
        id: state.units.len() as u32,
        owner: Owner::Civ(civ),
        unit_type,
        troops,
        hex: center,
        path: Vec::new(),
        last_moved: None,
        used_full_mp: false,
        standing: StandingRule::None,
        alive: true,
    };
    let candidates = core::iter::once(center).chain(center.neighbors());
    for hex in candidates {
        let passable = state.map.tile(hex).is_some_and(|t| t.terrain.is_passable());
        if passable && tile_free_for(state, &probe, hex) {
            state.units.push(Unit { hex, ..probe });
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------- phase 7

fn phase_upkeep(state: &mut WorldState) {
    for civ in 0..state.civs.len() as u16 {
        let cities = state.city_count(civ);
        let owned = |u: &&Unit| u.alive && u.owner == Owner::Civ(civ);
        let upkeep = |s: &WorldState| {
            (unit_upkeep(s.units.iter().filter(owned)) + city_upkeep(cities)) as i64 * MILLI
        };
        let mut balance = state.civs[civ as usize].gold - upkeep(state);
        if balance >= 0 {
            state.civs[civ as usize].gold = balance;
            continue;
        }
        // Deficit (§6.1): disband whole troops, T2 first, then largest army, then highest id.
        while balance < 0 {
            let victim = state
                .units
                .iter()
                .filter(owned)
                .filter(|u| !u.unit_type.is_civilian())
                .max_by_key(|u| (stats(u.unit_type).tier, u.troops, u.id))
                .map(|u| u.id as usize);
            let Some(v) = victim else { break };
            let u = &mut state.units[v];
            u.troops = u.troops.saturating_sub(1000);
            if u.troops < 500 {
                u.alive = false;
            }
            balance = state.civs[civ as usize].gold - upkeep(state);
        }
        let c = &mut state.civs[civ as usize];
        c.gold = balance.max(0);
        c.deficit = true;
    }
}

// ---------------------------------------------------------------- phase 8

fn phase_society(state: &mut WorldState, rules: &Ruleset) {
    let n = state.civs.len() as u16;
    // War weariness (§9.3), including +1 per 2 whole troops lost this tick.
    for a in 0..n {
        let mut gain = 0u32;
        let mut at_war = false;
        for b in 0..n {
            if a == b || !state.at_war(a, b) {
                continue;
            }
            at_war = true;
            gain += match state.relation(a, b) {
                Relation::War {
                    declared_by,
                    casus_belli: false,
                    ..
                } if declared_by == a => 2,
                _ => 1,
            };
        }
        let c = &mut state.civs[a as usize];
        gain += c.troops_lost / 2000;
        c.war_weariness = (c.war_weariness + gain).saturating_sub(if at_war { 1 } else { 3 });
    }

    // Loyalty (§9.4) and city defence regeneration (§8.3).
    for i in 0..state.cities.len() {
        let city = &state.cities[i];
        let (Some(owner), true) = (city.owner, city.alive) else {
            continue;
        };
        let civ = &state.civs[owner as usize];
        let capital_hex = civ
            .capital
            .and_then(|id| state.cities.get(id as usize))
            .filter(|c| c.alive)
            .map(|c| c.hex);
        let mut delta = 2;
        if capital_hex.is_some_and(|h| h.distance(city.hex) <= 6) {
            delta += 1;
        }
        let garrisoned = state.units.iter().any(|u| {
            u.alive
                && u.hex == city.hex
                && u.owner == Owner::Civ(owner)
                && !u.unit_type.is_civilian()
        });
        if garrisoned {
            delta += 3;
        }
        let pressure: u32 = state
            .cities
            .iter()
            .filter(|o| o.alive && o.hex.distance(city.hex) <= rules.loyalty_pressure_radius as u32)
            .filter(|o| matches!(o.owner, Some(x) if x != owner && !allied(state, x, owner)))
            .map(|o| o.pop)
            .sum();
        delta -= (pressure / 3) as i32;
        let mut amen = amenities(city, state.city_count(owner), civ.war_weariness);
        if civ.deficit {
            amen -= 2;
        }
        if amen <= -3 {
            delta -= 2;
        }
        let max_def = (rules.city_defense_base + city.pop) * 1000;
        let attacked = city.attacked_this_tick;
        let city = &mut state.cities[i];
        city.loyalty = (city.loyalty + delta).clamp(0, 100);
        if !attacked {
            city.defense = (city.defense + rules.city_regen_milli).min(max_def);
        }
        if city.loyalty == 0 {
            city.owner = None; // Free City
            let (id, hex) = (city.id, city.hex);
            state.push_event(b"free_city", &id.to_le_bytes());
            crate::battle::displace_civilians(state, hex);
        }
    }

    for g in &mut state.grievance {
        *g = g.saturating_sub(1);
    }
}

// ---------------------------------------------------------------- phase 9

fn phase_neutral(state: &mut WorldState, rules: &Ruleset) {
    let tick = state.tick;
    for cs in &mut state.city_states {
        if cs.captured_by.is_some() {
            continue;
        }
        if tick > 0 && tick % 30 == 0 && cs.pop < 6 {
            cs.pop += 1;
        }
        cs.defense = (cs.defense + rules.city_regen_milli).min((8 + 2 * cs.pop) * 1000);
        // Suzerainty contest cycles restart every `suzerain_lock_ticks` (§12.1).
        if tick > 0 && tick % rules.suzerain_lock_ticks == 0 {
            cs.suzerain = None;
            for v in &mut cs.influence {
                *v /= 2;
            }
        }
    }
    // TODO(§12.2): Crisis waves at ticks 120, 126, …, 156.
}

// ---------------------------------------------------------------- phase 10

fn phase_scoring(state: &mut WorldState, rules: &Ruleset, input: &TickInput) {
    // A capture scores at most once per captor per season (§14.1): decide on
    // the first tick the holding becomes eligible.
    let tick = state.tick;
    for c in &mut state.cities {
        let (Some(owner), Some(t)) = (c.owner, c.captured_tick) else {
            continue;
        };
        if c.capture_scores && tick.saturating_sub(t) + 1 == rules.capture_hold_ticks {
            if c.scored_by.contains(&owner) {
                c.capture_scores = false;
            } else {
                c.scored_by.push(owner);
            }
        }
    }
    let spent: Vec<(CivId, u32, bool)> = accepted(state, rules, input)
        .into_iter()
        .map(|(civ, cost, orders)| (civ, cost, !orders.is_empty()))
        .collect();
    for civ in 0..state.civs.len() as u16 {
        let d = dominion_tick(state, rules, civ);
        let total_pop: u32 = state.living_cities_of(civ).map(|c| c.pop).sum();
        let before = state.civs[civ as usize].scores.max_pop;
        let cd = concord_tick(state, rules, &state.civs[civ as usize], total_pop, before);
        let in_alliance = (0..state.civs.len() as u16).any(|o| allied(state, civ, o));
        let (cost, active) = spent
            .iter()
            .find(|s| s.0 == civ)
            .map_or((0, false), |s| (s.1, s.2));
        let c = &mut state.civs[civ as usize];
        c.scores.dominion += d;
        c.scores.concord_raw += cd;
        c.scores.max_pop = before.max(total_pop);
        c.ever_allied |= in_alliance;
        if active {
            c.active_ticks += 1;
        }
        c.order_bank = next_bank(rules, c.order_bank, c.tick_budget, cost);
    }
}

// ---------------------------------------------------------------- phase 11

fn phase_commit(state: &mut WorldState, rules: &Ruleset) {
    for civ in 0..state.civs.len() as u16 {
        let cities = state.city_count(civ);
        state.civs[civ as usize].tick_budget = order_budget(rules, cities);
    }
    let tick = state.tick;
    state.push_event(b"tick", &tick.to_le_bytes());
}
