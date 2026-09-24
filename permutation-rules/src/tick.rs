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
use crate::checks::Blocked;
use crate::fixed::{apply_bps, milli, MILLI};
use crate::gov::{active_officer, Credit, GovEntry, Path, Role, NOBODY};
use crate::hex::Hex;
use crate::map::territory_radius;
use crate::merit;
use crate::orders::{batch_orders, next_bank, role_allows, validate_batch, CivOrders, Order, OrderBatch};
use crate::params::Ruleset;
use crate::rng::{tick_seed, tie_key, Seed};
use crate::state::{CivId, Owner, QueueItem, Relation, StandingRule, Unit, WorldState};
use crate::tech::Tech;
use crate::units::{stats, UnitType};
use crate::RulesError;
use alloc::vec::Vec;

pub const PHASE_COUNT: u8 = 12;

/// Everything outside the state that a tick consumes. Replaying the same
/// inputs over the same state must give the same root (§17 invariant 7).
#[derive(Clone, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct TickInput {
    /// MagicBlock VRF output for this tick (§0.2).
    pub vrf: Seed,
    /// At most one batch per office per civ is used (the first valid one);
    /// extra or invalid batches are ignored.
    pub batches: Vec<OrderBatch>,
    /// Governance actions, applied in this order in phase 0 (V5 §5.7).
    pub gov: Vec<GovEntry>,
    /// USDC deposited into nation treasuries since the last tick (V5 §7.5).
    pub deposits: Vec<(CivId, u64)>,
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
/// | 3 | Standing rules | done (see `standing`): AutoDefend, Retreat, Patrol, AutoPurchase |
/// | 4 | Movement | done (paths, MP, occupancy, territory, protection, tie-break) |
/// | 5 | Combat | done (see `battle`), incl. standing-rule attacks; barbarian AI TODO |
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
        0 => phase_seed(state, rules, input),
        1 => crate::diplomacy::phase_diplomacy(state, rules, input),
        2 => phase_economy_orders(state, rules, input),
        3 => crate::standing::phase_standing(state, rules, input),
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

/// The orders accepted for this tick, per civ in civ order (merged in phase 0).
pub(crate) fn accepted(state: &WorldState) -> Vec<CivOrders> {
    state.tick_orders.clone()
}

// ---------------------------------------------------------------- phase 0

fn phase_seed(state: &mut WorldState, rules: &Ruleset, input: &TickInput) {
    state.tick_seed = tick_seed(&state.season_seed, &input.vrf, state.tick);
    state.last_skipped.clear();
    state.merit_log.clear();
    for c in &mut state.cities {
        c.attacked_this_tick = false;
    }
    for civ in &mut state.civs {
        civ.deficit = false;
        civ.troops_lost = 0;
    }
    // Governance first: supports and proposals of this tick count for adoption.
    crate::gov::apply_actions(state, rules, &input.gov);
    for &(civ, amount) in &input.deposits {
        if state.tick < rules.exchange_freeze_tick && rules.market_enabled && (civ as usize) < state.civs.len() {
            state.civs[civ as usize].usdc += amount;
            state.usdc_deposited += amount;
        }
    }
    state.tick_orders = (0..state.civs.len() as CivId).map(|civ| merge_orders(state, rules, input, civ)).collect();
}

/// An office's accepted batch, its orders with credits, and its cost.
type Chosen = (OrderBatch, Vec<(Order, Credit)>, u32);

/// Pick each office's batch (the first valid one), commit its decision,
/// and merge the offices' orders in `Role::ALL` order (V5 §5.1–§5.6).
fn merge_orders(state: &mut WorldState, rules: &Ruleset, input: &TickInput, civ: CivId) -> CivOrders {
    let tick = state.tick;
    let mut chosen: [Option<Chosen>; 4] = [None, None, None, None];
    for role in Role::ALL {
        let mut rejected = false;
        for b in input.batches.iter().filter(|b| b.civ == civ && b.role == role) {
            match validate_batch(state, rules, b) {
                Ok(cost) => {
                    let orders = batch_orders(state, b).unwrap_or_default();
                    chosen[role.index()] = Some((b.clone(), orders, cost));
                    break;
                }
                Err(_) => rejected = true,
            }
        }
        if rejected && chosen[role.index()].is_none() {
            state.skip(civ, (role as u8, u16::MAX), Blocked::BatchRejected.code());
        }
    }

    // Decision commitments enter the event chain before anything resolves (§4.3).
    for (b, _, _) in chosen.iter().flatten() {
        let mut payload = [0u8; 39];
        payload[..2].copy_from_slice(&civ.to_le_bytes());
        payload[2] = b.role as u8;
        payload[3..7].copy_from_slice(&b.member.to_le_bytes());
        payload[7..].copy_from_slice(&b.decision_digest);
        state.push_event(b"decision", &payload);
    }

    // War needs a second officer (V5 §5.6): the general's or the steward's
    // consent, from a different person than the diplomat.
    let diplomat = chosen[Role::Diplomat.index()].as_ref().map(|c| c.0.member);
    let consents: Vec<CivId> = [Role::General, Role::Steward]
        .iter()
        .filter_map(|r| chosen[r.index()].as_ref())
        .filter(|(b, _, _)| diplomat.is_none_or(|d| b.member != d || d == NOBODY))
        .flat_map(|(_, orders, _)| orders.iter().filter_map(|(o, _)| match o {
            Order::ConsentWar { civ } => Some(*civ),
            _ => None,
        }))
        .collect();

    let mut out = CivOrders { civ, orders: Vec::new(), credits: Vec::new(), origin: Vec::new(), spent: [0; 4] };
    for role in Role::ALL {
        let Some((batch, orders, cost)) = chosen[role.index()].take() else { continue };
        out.spent[role.index()] = cost as u16;
        if batch.member != NOBODY {
            state.nations[civ as usize].office_seen[role.index()] = tick;
        }
        for id in &batch.adopt {
            let n = &mut state.nations[civ as usize];
            if let Some(p) = n.proposals.iter_mut().find(|p| p.id == *id && !p.adopted) {
                p.adopted = true;
                n.adopted += 1;
            }
        }
        let mut acted = false;
        for (i, (order, mut credit)) in orders.into_iter().enumerate() {
            let origin = (role as u8, i as u16);
            if !role_allows(state, role, &order) {
                state.skip(civ, origin, Blocked::WrongOffice.code());
                continue;
            }
            let war = matches!(order, Order::DeclareWar { civ: t } | Order::BreakNap { civ: t } if !consents.contains(&t));
            if war {
                state.skip(civ, origin, Blocked::NeedsConsent.code());
                continue;
            }
            if credit.proposer == NOBODY && credit.officer != NOBODY {
                credit.proposer = auto_match(state, civ, role, &order);
            }
            acted |= order.is_action();
            out.orders.push(order);
            out.credits.push(credit);
            out.origin.push(origin);
        }
        // An office acted this tick (v0.2 C9: reveals alone do not count).
        if acted && batch.member != NOBODY {
            crate::gov::mark_active(state, rules, batch.member);
            state.nations[civ as usize].office_last_act[role.index()] = tick;
            merit::credit(state, Credit::officer(batch.member), Path::Common, rules.merit_office_tick as u64, b"office");
        }
    }
    out
}

/// An officer's own order identical to one in a supported proposal made on
/// an earlier tick counts as adopting it (V5 §5.4, against claim-jumping).
fn auto_match(state: &mut WorldState, civ: CivId, role: Role, order: &Order) -> u32 {
    let tick = state.tick;
    let n = &mut state.nations[civ as usize];
    match n
        .proposals
        .iter_mut()
        .find(|p| p.role == role && !p.adopted && p.tick < tick && !p.supporters.is_empty() && p.orders.contains(order))
    {
        Some(p) => {
            p.adopted = true;
            let proposer = p.proposer;
            n.adopted += 1;
            proposer
        }
        None => NOBODY,
    }
}

// ---------------------------------------------------------------- phase 1

// ---------------------------------------------------------------- phase 2

fn phase_economy_orders(state: &mut WorldState, rules: &Ruleset, _input: &TickInput) {
    crate::markets::deliver(state);
    let mut purchased: Vec<u32> = Vec::new();
    for a in accepted(state) {
        let civ = a.civ;
        for (order, credit, origin) in a.iter() {
            let result = match order.clone() {
                Order::FoundCity { settler } => found_city(state, rules, civ, settler, credit),
                Order::SetQueue { city, items } => set_queue(state, civ, city, items, credit),
                Order::SetFocus { city, focus } => match owned_city_mut(state, civ, city) {
                    Some(c) => {
                        c.focus = focus;
                        c.steward_credit = credit;
                        Ok(())
                    }
                    None => Err(Blocked::NotYours),
                },
                Order::SetResearch { techs } => set_research(state, civ, techs, credit),
                // One purchase per city per tick (v0.2 C3).
                Order::Purchase { city, gold } => {
                    if purchased.contains(&city) {
                        Err(Blocked::AlreadyPurchased)
                    } else {
                        purchased.push(city);
                        purchase(state, rules, civ, city, gold)
                    }
                }
                Order::SetStanding { target, rule } => crate::standing::apply(state, civ, target, rule),
                Order::RevealRationale { tick, policy, salt, text } => {
                    // Recorded, never interpreted: verifiers check it against the
                    // `decision` event of (`civ`, office, `tick`) (§4.3).
                    let mut payload = Vec::with_capacity(69);
                    payload.extend_from_slice(&civ.to_le_bytes());
                    payload.push(origin.0);
                    payload.extend_from_slice(&tick.to_le_bytes());
                    payload.extend_from_slice(&crate::decision::policy_id(&policy));
                    payload.extend_from_slice(&crate::decision::rationale_hash(&salt, &text));
                    state.push_event(b"reveal", &payload);
                    Ok(())
                }
                _ => Ok(()),
            };
            if let Err(why) = result {
                state.skip(civ, origin, why.code());
            }
        }
    }
    crate::diplomacy::apply_transfers(state, rules);
    crate::diplomacy::apply_envoys(state, rules);
    crate::markets::apply_amm(state, rules);
    crate::markets::apply_exchange(state, rules);
}

/// `FoundCity` (§4.2, §5.7): the settler founds a city where it stands.
fn found_city(state: &mut WorldState, rules: &Ruleset, civ: CivId, settler: u32, credit: Credit) -> Result<(), Blocked> {
    let hex = crate::checks::found_city(state, rules, civ, settler)?;
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
        queue_credit: credit,
        steward_credit: credit,
        captured_tick: None,
        captured_from: None,
        capture_scores: false,
        razing: None,
        heritage_until,
        heritage_bonus,
        standing: crate::state::CityStanding::DEFAULT,
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
    let tiles = state.map.tiles.iter().filter(|t| t.owner_city == Some(id)).count() as u64;
    let m = rules.merit_found_city as u64 + tiles * rules.merit_found_per_tile as u64;
    merit::credit(state, credit, Path::Prosperity, m, b"found_city");
    Ok(())
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

fn set_queue(state: &mut WorldState, civ: CivId, city: u32, items: Vec<QueueItem>, credit: Credit) -> Result<(), Blocked> {
    if owned_city_mut(state, civ, city).is_none() {
        return Err(Blocked::NotYours);
    }
    for i in &items {
        crate::checks::queue_item(state, civ, city, i)?; // dropped without refund (§4.1)
    }
    let c = &mut state.cities[city as usize];
    c.queue = items;
    c.queue_credit = credit;
    c.steward_credit = credit;
    Ok(())
}

fn set_research(state: &mut WorldState, civ: CivId, techs: Vec<Tech>, credit: Credit) -> Result<(), Blocked> {
    let c = &mut state.civs[civ as usize];
    let mut planned = c.techs;
    for t in &techs {
        crate::checks::research(planned, *t)?;
        planned.insert(*t);
    }
    c.research_queue = techs;
    c.research_credit = credit;
    Ok(())
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

/// `Purchase` (§5.6): gold buys up to `purchase_max_bps` of the current
/// item's remaining production. Never a Star Gate stage (v0.2 C3).
pub(crate) fn purchase(state: &mut WorldState, rules: &Ruleset, civ: CivId, city: u32, gold: u32) -> Result<(), Blocked> {
    let c = state
        .cities
        .get(city as usize)
        .filter(|c| c.alive && c.owner == Some(civ))
        .ok_or(Blocked::NotYours)?;
    let item = c.queue.first().ok_or(Blocked::NothingQueued)?;
    if matches!(item, QueueItem::Building(b) if b.is_star_gate()) {
        return Err(Blocked::CannotBuyStarGate);
    }
    let remaining = (item_cost(state, rules, c, item) - c.prod).max(0);
    let cap = apply_bps(remaining, rules.purchase_max_bps);
    let wanted = milli(gold as i64) / rules.purchase_gold_per_prod as i64;
    let have = state.civs[civ as usize].gold;
    let prod = wanted.min(cap).min(have / rules.purchase_gold_per_prod as i64);
    if prod <= 0 {
        return Err(Blocked::NotEnoughGold { need: rules.purchase_gold_per_prod, have: (have / MILLI).max(0) as u32 });
    }
    state.civs[civ as usize].gold -= prod * rules.purchase_gold_per_prod as i64;
    state.cities[city as usize].prod += prod;
    Ok(())
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

/// Office and position of an order (see `CivOrders::origin`).
type Origin = (u8, u16);

fn phase_movement(state: &mut WorldState, rules: &Ruleset, _input: &TickInput) {
    // 1. Accept MoveUnit orders (paths continue on later ticks without orders),
    //    manual first, then those compiled from standing rules (§13).
    let mut all: Vec<(CivId, Order, Option<Origin>)> = Vec::new();
    for a in accepted(state) {
        all.extend(a.orders.into_iter().zip(a.origin).map(|(o, g)| (a.civ, o, Some(g))));
    }
    all.extend(state.implicit.iter().cloned().map(|(c, o)| (c, o, None)));
    {
        for (civ, order, origin) in all {
            if let Order::MoveUnit { unit, path } = order {
                let Some(u) = state
                    .units
                    .get(unit as usize)
                    .filter(|u| u.alive && u.owner == Owner::Civ(civ))
                else {
                    if let Some(g) = origin {
                        state.skip(civ, g, Blocked::UnknownUnit.code());
                    }
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
                } else if let Some(g) = origin {
                    state.skip(civ, g, Blocked::Impassable.code());
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
    let mut grown: Vec<Credit> = Vec::new();
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
                grown.push(city.steward_credit);
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
            // The Science focus multiplies the city's science in milli units
            // (v0.2 C6: applied before rounding, so it always has an effect).
            let science = if state.cities[i].focus == crate::state::Focus::Science {
                y.science as i64 * 12_500
            } else {
                y.science as i64 * MILLI
            };
            let c = &mut state.civs[civ as usize];
            c.gold += y.gold as i64 * MILLI;
            c.achievements.wealth += y.gold as u64;
            c.science_store += science;
            c.influence += y.influence as i64 * MILLI;
            c.scores.science_total += (science / MILLI) as u64;
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
                c.achievements.wealth += 4;
                last[civ as usize].gold += 4;
            }
            crate::state::Specialty::Agrarian => {
                if let Some(cap) = c.capital {
                    state.cities[cap as usize].food += 2 * MILLI;
                }
            }
        }
    }
    for c in grown {
        merit::credit(state, c, Path::Prosperity, rules.merit_pop as u64, b"growth");
    }
    // City gold earns the steward merit (V5 §7.3).
    for (civ, l) in last.iter().enumerate() {
        if let Some(m) = active_officer(state, rules, civ as CivId, Role::Steward) {
            let amount = l.gold as u64 * MILLI as u64 / rules.merit_gold_div.max(1) as u64;
            merit::credit(state, Credit::officer(m), Path::Prosperity, amount, b"gold");
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
            let credit = c.research_credit;
            let amount = (cost / rules.merit_tech_div.max(1) as i64) as u64;
            merit::credit(state, credit, Path::Science, amount, b"tech");
        }
    }
}

/// Complete at most one queue item (v0.2 C1), then cap the production
/// store at the next item's cost (nothing is banked without an item).
fn complete_queue(state: &mut WorldState, rules: &Ruleset, civ: CivId, i: usize) {
    // Overflow from a completion carries to the next item, up to its cost.
    try_complete(state, rules, civ, i);
    let cap = match state.cities[i].queue.first().copied() {
        Some(next) => item_cost(state, rules, &state.cities[i], &next),
        None => 0,
    };
    let city = &mut state.cities[i];
    city.prod = city.prod.min(cap);
}

fn try_complete(state: &mut WorldState, rules: &Ruleset, civ: CivId, i: usize) -> bool {
    {
        let Some(item) = state.cities[i].queue.first().copied() else {
            return false;
        };
        let cost = item_cost(state, rules, &state.cities[i], &item);
        if state.cities[i].prod < cost {
            return false;
        }
        match item {
            QueueItem::Building(b) => {
                if b.is_star_gate() {
                    // Stages of one civ are at least `star_gate_spacing` ticks apart (v0.2 C2).
                    let last = state.civs[civ as usize].last_star_gate;
                    if last.is_some_and(|t| state.tick < t + rules.star_gate_spacing) {
                        return false;
                    }
                }
                let city = &mut state.cities[i];
                city.buildings.insert(b);
                let credit = city.queue_credit;
                if b.is_star_gate() {
                    let stages = city.buildings.star_gate_stages();
                    let tick = state.tick;
                    let c = &mut state.civs[civ as usize];
                    c.scores.star_gate_stages = c.scores.star_gate_stages.max(stages);
                    c.scores.star_gate_tick = Some(tick);
                    c.last_star_gate = Some(tick);
                    c.achievements.star_gate_max = c.achievements.star_gate_max.max(stages);
                    let mut payload = [0u8; 3];
                    payload[..2].copy_from_slice(&civ.to_le_bytes());
                    payload[2] = stages;
                    state.push_event(b"star_gate", &payload);
                    // Half to the steward who queued it, half to the science officer.
                    let half = rules.merit_star_gate as u64 / 2;
                    merit::credit(state, credit, Path::Science, rules.merit_star_gate as u64 - half, b"star_gate");
                    if let Some(m) = active_officer(state, rules, civ, Role::Science) {
                        merit::credit(state, Credit::officer(m), Path::Science, half, b"star_gate");
                    }
                } else {
                    let amount = (cost / rules.merit_building_div.max(1) as i64) as u64;
                    merit::credit(state, credit, Path::Prosperity, amount, b"building");
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
                    return false; // wait for strategic resources
                }
                if !spawn(state, civ, i, unit, n as u32 * 1000) {
                    return false; // no free tile: delayed (§5.6)
                }
                let c = &mut state.civs[civ as usize];
                c.iron -= iron;
                c.horses -= horses;
            }
            QueueItem::Scout => {
                if !spawn(state, civ, i, UnitType::Scout, 1000) {
                    return false;
                }
            }
            QueueItem::Settler => {
                if state.cities[i].pop < rules.settler_min_pop {
                    return false;
                }
                if !spawn(state, civ, i, UnitType::Settler, 1000) {
                    return false;
                }
                state.cities[i].pop -= 1;
            }
        }
        let city = &mut state.cities[i];
        city.prod -= cost;
        city.queue.remove(0);
        // CityQueueRepeat (default on, §13): repeat the last unit item.
        if city.queue.is_empty() && city.standing.repeat_queue && !matches!(item, QueueItem::Building(_)) {
            city.queue.push(item);
        }
        true
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
            city.standing = crate::state::CityStanding::DEFAULT;
            let (id, hex) = (city.id, city.hex);
            state.push_event(b"free_city", &id.to_le_bytes());
            crate::battle::displace_civilians(state, hex);
        }
    }

    // Grievance decays by 1, but what was added this tick does not (v0.2 C7).
    for (g, fresh) in state.grievance.iter_mut().zip(state.grievance_fresh.iter_mut()) {
        if *g > *fresh {
            *g -= 1;
        }
        *fresh = 0;
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

fn phase_scoring(state: &mut WorldState, rules: &Ruleset, _input: &TickInput) {
    let n = state.civs.len();
    // Suzerainty: the milestone record and merit for the envoys (V5 §7.3).
    for cs in 0..state.city_states.len() {
        let (Some(civ), None) = (state.city_states[cs].suzerain, state.city_states[cs].captured_by) else {
            continue;
        };
        state.civs[civ as usize].achievements.ever_suzerain = true;
        let shares: Vec<(Credit, u64)> = state.city_states[cs]
            .envoys
            .iter()
            .filter(|e| e.civ == civ)
            .map(|e| (e.credit, e.influence))
            .collect();
        merit::credit_shared(state, &shares, Path::Concord, rules.merit_suzerain as u64, b"suzerain");
    }
    // Office banks (§4.1 per office, V5 §5.2).
    let spent: Vec<[u16; 4]> = (0..n)
        .map(|c| state.tick_orders.iter().find(|a| a.civ as usize == c).map_or([0; 4], |a| a.spent))
        .collect();
    for (civ, used) in spent.iter().enumerate() {
        let b = state.civs[civ].tick_budget;
        let in_alliance = (0..n as u16).any(|o| allied(state, civ as u16, o));
        state.civs[civ].ever_allied |= in_alliance;
        let nation = &mut state.nations[civ];
        for (role, u) in used.iter().enumerate() {
            nation.role_bank[role] = next_bank(rules, nation.role_bank[role], rules.role_budget(b, role), *u as u32);
        }
    }
    // Milestones and eras, announced when they change (V5 §6.3).
    let facts = crate::scoring::facts_all(state, rules);
    for (civ, f) in facts.iter().enumerate() {
        let score = crate::scoring::score_of(rules, f);
        let a = &state.civs[civ].achievements;
        let (old_tiers, old_era) = (a.tiers, a.era);
        for (p, (new, old)) in score.tiers.iter().zip(old_tiers).enumerate() {
            if *new != old {
                state.push_event(b"milestone", &[civ as u8, p as u8, *new]);
            }
        }
        if score.era != old_era {
            state.push_event(b"era", &[civ as u8, score.era]);
        }
        let a = &mut state.civs[civ].achievements;
        a.tiers = score.tiers;
        a.era = score.era;
    }
}

// ---------------------------------------------------------------- phase 11

fn phase_commit(state: &mut WorldState, rules: &Ruleset) {
    for civ in 0..state.civs.len() as u16 {
        let cities = state.city_count(civ);
        state.civs[civ as usize].tick_budget = order_budget(rules, cities);
    }
    // Recalls, idle recalls and elections take effect next tick (V5 §5.3–§5.5).
    crate::gov::end_of_tick(state, rules);
    state.implicit.clear();
    state.tick_orders.clear();
    let tick = state.tick;
    state.push_event(b"tick", &tick.to_le_bytes());
}
