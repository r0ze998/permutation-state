//! Phase 2 (§4.2, §5): economy orders — found city, queues, focus,
//! research, purchase — plus the transfers, envoys and markets that
//! `phase_economy_orders` hands to their modules.

use crate::buildings::{info as building_info, Building};
use crate::checks::Blocked;
use crate::economy::stalemate_multiplier;
use crate::fixed::{apply_bps, milli, MILLI};
use crate::gov::{Credit, Path};
use crate::map::territory_radius;
use crate::merit;
use crate::orders::Order;
use crate::params::Ruleset;
use crate::state::{CivId, QueueItem, WorldState};
use crate::tech::Tech;
use crate::tick::accepted;
use crate::units::{stats, UnitType};
use alloc::vec::Vec;

pub(crate) fn phase_economy_orders(state: &mut WorldState, rules: &Ruleset) {
    crate::markets::deliver(state);
    let mut purchased: Vec<u32> = Vec::new();
    let economy = |o: &Order| {
        matches!(
            o,
            Order::FoundCity { .. }
                | Order::SetQueue { .. }
                | Order::SetFocus { .. }
                | Order::SetResearch { .. }
                | Order::Purchase { .. }
                | Order::SetStanding { .. }
                | Order::RevealRationale { .. }
        )
    };
    for (civ, order, credit, origin) in accepted(state, economy) {
        let result = match order {
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
            Order::RevealRationale {
                tick,
                policy,
                salt,
                text,
            } => {
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
    crate::trade::apply_transfers(state, rules);
    crate::envoys::apply_envoys(state, rules);
    crate::markets::apply_amm(state, rules);
    let escrowed = crate::contracts::apply_contract_orders(state, rules);
    crate::markets::apply_exchange(state, rules, &escrowed);
}

/// `FoundCity` (§4.2, §5.7): the settler founds a city where it stands.
fn found_city(
    state: &mut WorldState,
    rules: &Ruleset,
    civ: CivId,
    settler: u32,
    credit: Credit,
) -> Result<(), Blocked> {
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
        .min_by_key(|(_, t)| (t.hex.distance(hex), t.hex.turned(hex.sextant())))
        .map(|(i, t)| (i, t.ruin_peak_pop.unwrap_or(0) as u32 / 2));
    let (heritage_until, heritage_bonus) = match heritage {
        Some((i, bonus)) => {
            state.map.tiles[i].heritage_claimed = true;
            (Some(state.tick + rules.heritage_ticks), bonus)
        }
        None => (None, 0),
    };
    state.cities.push(crate::state::City {
        queue_credit: credit,
        steward_credit: credit,
        heritage_until,
        heritage_bonus,
        ..crate::state::City::founded(id, civ, hex, state.tick, rules)
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
    let tiles = state
        .map
        .tiles
        .iter()
        .filter(|t| t.owner_city == Some(id))
        .count() as u64;
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

fn set_queue(
    state: &mut WorldState,
    civ: CivId,
    city: u32,
    items: Vec<QueueItem>,
    credit: Credit,
) -> Result<(), Blocked> {
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

fn set_research(
    state: &mut WorldState,
    civ: CivId,
    techs: Vec<Tech>,
    credit: Credit,
) -> Result<(), Blocked> {
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

/// Production cost (milli) of a queue item in `city` (§5.6): Star Gate
/// stages grow with the stalemate multiplier, Barracks discount troops.
pub(crate) fn item_cost(
    state: &WorldState,
    rules: &Ruleset,
    city: &crate::state::City,
    item: &QueueItem,
) -> i64 {
    let prod = match *item {
        QueueItem::Building(b) => {
            let base = building_info(b).prod_cost as i64;
            if b.is_star_gate() {
                apply_bps(base, stalemate_multiplier(rules, state.tick))
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
pub(crate) fn purchase(
    state: &mut WorldState,
    rules: &Ruleset,
    civ: CivId,
    city: u32,
    gold: u32,
) -> Result<(), Blocked> {
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
    let prod = wanted
        .min(cap)
        .min(have / rules.purchase_gold_per_prod as i64);
    if prod <= 0 {
        return Err(Blocked::NotEnoughGold {
            need: rules.purchase_gold_per_prod,
            have: (have / MILLI).max(0) as u32,
        });
    }
    state.civs[civ as usize].gold -= prod * rules.purchase_gold_per_prod as i64;
    state.cities[city as usize].prod += prod;
    Ok(())
}
