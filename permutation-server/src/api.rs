//! JSON API: order DTOs, the per-civ world view, and preview payloads.
//!
//! Names on the wire are the engine's own enum names (`"Granary"`,
//! `"Pikeman"`, `"Astronomy"`), so clients and agents never need a second
//! table of ids. Blocked reasons are `{ "code": "...", ...data }`; clients
//! localise by code.

use permutation_rules::battle::AttackForecast;
use permutation_rules::buildings::{Building, BUILDINGS};
use permutation_rules::checks::Blocked;
use permutation_rules::hex::Hex;
use permutation_rules::orders::{AttackTarget, Good, Order, Side};
use permutation_rules::preview;
use permutation_rules::state::{
    CivId, Focus, Owner, ProposalKind, QueueItem, Relation, StandingRule, WorldState,
};
use permutation_rules::tech::{Tech, TECHS};
use permutation_rules::units::{UnitType, UNIT_STATS};
use permutation_rules::Ruleset;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ------------------------------------------------------------------ names

fn name<T: core::fmt::Debug>(v: T) -> String {
    format!("{v:?}")
}

pub fn parse_tech(s: &str) -> Option<Tech> {
    TECHS.iter().map(|t| t.tech).find(|t| name(t) == s)
}
pub fn parse_building(s: &str) -> Option<Building> {
    BUILDINGS.iter().map(|b| b.building).find(|b| name(b) == s)
}
pub fn parse_unit(s: &str) -> Option<UnitType> {
    UNIT_STATS.iter().map(|u| u.unit).find(|u| name(u) == s)
}
fn parse_focus(s: &str) -> Option<Focus> {
    [Focus::Balanced, Focus::Food, Focus::Production, Focus::Gold, Focus::Science]
        .into_iter()
        .find(|f| name(f) == s)
}
fn parse_side(s: &str) -> Option<Side> {
    match s {
        "Buy" => Some(Side::Buy),
        "Sell" => Some(Side::Sell),
        _ => None,
    }
}

// ------------------------------------------------------------------ orders in

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ItemDto {
    Building { building: String },
    Troops { unit: String, n: u8 },
    Scout,
    Settler,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum TargetDto {
    Unit { id: u32 },
    City { id: u32 },
    CityState { id: u16 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum GoodDto {
    Gold,
    Iron,
    Horses,
    Food { city: u32 },
    Production { city: u32 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum OrderDto {
    MoveUnit { unit: u32, path: Vec<[i32; 2]> },
    Attack { army: u32, target: TargetDto },
    FoundCity { settler: u32 },
    SetQueue { city: u32, items: Vec<ItemDto> },
    SetFocus { city: u32, focus: String },
    Purchase { city: u32, gold: u32 },
    SetResearch { techs: Vec<String> },
    DeclareWar { civ: CivId },
    ProposePeace { civ: CivId },
    AcceptPeace { civ: CivId },
    ProposeNap { civ: CivId, bond: u32 },
    AcceptNap { civ: CivId, bond: u32 },
    BreakNap { civ: CivId },
    ProposeAlliance { civ: CivId },
    AcceptAlliance { civ: CivId },
    LeaveAlliance,
    SendEnvoy { city_state: u16, influence: u32 },
    Transfer { civ: CivId, good: GoodDto, amount: u32 },
    MarketTrade { good: GoodDto, side: String, amount: u32, limit_gold: u32 },
    ExchangeOrder { good: GoodDto, side: String, amount: u32, price: u64 },
    Raze { city: u32 },
    AutoDefend { unit: u32, radius: u8 },
}

fn good(g: &GoodDto) -> Good {
    match *g {
        GoodDto::Gold => Good::Gold,
        GoodDto::Iron => Good::Iron,
        GoodDto::Horses => Good::Horses,
        GoodDto::Food { city } => Good::Food(city),
        GoodDto::Production { city } => Good::Production(city),
    }
}

fn item(i: &ItemDto) -> Result<QueueItem, String> {
    Ok(match i {
        ItemDto::Building { building } => QueueItem::Building(
            parse_building(building).ok_or_else(|| format!("unknown building {building}"))?,
        ),
        ItemDto::Troops { unit, n } => QueueItem::Troops {
            unit: parse_unit(unit).ok_or_else(|| format!("unknown unit {unit}"))?,
            n: *n,
        },
        ItemDto::Scout => QueueItem::Scout,
        ItemDto::Settler => QueueItem::Settler,
    })
}

pub fn item_dto(i: QueueItem) -> ItemDto {
    match i {
        QueueItem::Building(b) => ItemDto::Building { building: name(b) },
        QueueItem::Troops { unit, n } => ItemDto::Troops { unit: name(unit), n },
        QueueItem::Scout => ItemDto::Scout,
        QueueItem::Settler => ItemDto::Settler,
    }
}

impl OrderDto {
    pub fn to_order(&self) -> Result<Order, String> {
        use OrderDto as D;
        Ok(match self {
            D::MoveUnit { unit, path } => Order::MoveUnit {
                unit: *unit,
                path: path.iter().map(|[q, r]| Hex::new(*q, *r)).collect(),
            },
            D::Attack { army, target } => Order::Attack {
                army: *army,
                target: match *target {
                    TargetDto::Unit { id } => AttackTarget::Unit(id),
                    TargetDto::City { id } => AttackTarget::City(id),
                    TargetDto::CityState { id } => AttackTarget::CityState(id),
                },
            },
            D::FoundCity { settler } => Order::FoundCity { settler: *settler },
            D::SetQueue { city, items } => Order::SetQueue {
                city: *city,
                items: items.iter().map(item).collect::<Result<_, _>>()?,
            },
            D::SetFocus { city, focus } => Order::SetFocus {
                city: *city,
                focus: parse_focus(focus).ok_or("unknown focus")?,
            },
            D::Purchase { city, gold } => Order::Purchase { city: *city, gold: *gold },
            D::SetResearch { techs } => Order::SetResearch {
                techs: techs
                    .iter()
                    .map(|t| parse_tech(t).ok_or_else(|| format!("unknown tech {t}")))
                    .collect::<Result<_, _>>()?,
            },
            D::DeclareWar { civ } => Order::DeclareWar { civ: *civ },
            D::ProposePeace { civ } => Order::ProposePeace { civ: *civ },
            D::AcceptPeace { civ } => Order::AcceptPeace { civ: *civ },
            D::ProposeNap { civ, bond } => Order::ProposeNap { civ: *civ, bond: *bond },
            D::AcceptNap { civ, bond } => Order::AcceptNap { civ: *civ, bond: *bond },
            D::BreakNap { civ } => Order::BreakNap { civ: *civ },
            D::ProposeAlliance { civ } => Order::ProposeAlliance { civ: *civ },
            D::AcceptAlliance { civ } => Order::AcceptAlliance { civ: *civ },
            D::LeaveAlliance => Order::LeaveAlliance,
            D::SendEnvoy { city_state, influence } => Order::SendEnvoy {
                city_state: *city_state,
                influence: *influence,
            },
            D::Transfer { civ, good: g, amount } => Order::Transfer {
                civ: *civ,
                good: good(g),
                amount: *amount,
            },
            D::MarketTrade { good: g, side, amount, limit_gold } => Order::MarketTrade {
                good: good(g),
                side: parse_side(side).ok_or("unknown side")?,
                amount: *amount,
                limit_gold: *limit_gold,
            },
            D::ExchangeOrder { good: g, side, amount, price } => Order::ExchangeOrder {
                good: good(g),
                side: parse_side(side).ok_or("unknown side")?,
                amount: *amount,
                price: *price,
            },
            D::Raze { city } => Order::Raze { city: *city },
            D::AutoDefend { unit, radius } => Order::SetStanding {
                unit: *unit,
                rule: StandingRule::AutoDefend { radius: *radius },
            },
        })
    }
}

// ------------------------------------------------------------------ blocked out

pub fn blocked(b: Blocked) -> Value {
    use Blocked as B;
    let code = match b {
        B::NeedsTech(_) => "NeedsTech",
        B::TooCloseToCity { .. } => "TooCloseToCity",
        B::TooCloseToCityState { .. } => "TooCloseToCityState",
        B::NeedsPop { .. } => "NeedsPop",
        B::InTruce { .. } => "InTruce",
        B::BondTooSmall { .. } => "BondTooSmall",
        B::NotEnoughGold { .. } => "NotEnoughGold",
        B::AllianceFull { .. } => "AllianceFull",
        B::OutOfRange { .. } => "OutOfRange",
        B::OverCap { .. } => "OverCap",
        B::ProtectedCapital { .. } => "ProtectedCapital",
        other => return json!({ "code": name(other) }),
    };
    let mut v = json!({ "code": code });
    let o = v.as_object_mut().unwrap();
    match b {
        B::NeedsTech(t) => {
            o.insert("tech".into(), json!(name(t)));
        }
        B::TooCloseToCity { distance, min } | B::TooCloseToCityState { distance, min } => {
            o.insert("distance".into(), json!(distance));
            o.insert("min".into(), json!(min));
        }
        B::NeedsPop { need, have } | B::NotEnoughGold { need, have } => {
            o.insert("need".into(), json!(need));
            o.insert("have".into(), json!(have));
        }
        B::InTruce { until } => {
            o.insert("until".into(), json!(until));
        }
        B::BondTooSmall { min } => {
            o.insert("min".into(), json!(min));
        }
        B::AllianceFull { cap } | B::OverCap { cap } => {
            o.insert("cap".into(), json!(cap));
        }
        B::ProtectedCapital { civ, until } => {
            o.insert("civ".into(), json!(civ));
            o.insert("until".into(), if until == u16::MAX { Value::Null } else { json!(until) });
        }
        B::OutOfRange { distance, range } => {
            o.insert("distance".into(), json!(distance));
            o.insert("range".into(), json!(range));
        }
        _ => {}
    }
    v
}

fn result<T>(r: Result<T, Blocked>) -> Value {
    match r {
        Ok(_) => Value::Null,
        Err(b) => blocked(b),
    }
}

// ------------------------------------------------------------------ previews out

pub fn unit_preview(s: &WorldState, rules: &Ruleset, me: CivId, unit: u32) -> Value {
    let reach: Vec<Value> = preview::reachable(s, rules, unit)
        .into_iter()
        .map(|r| json!([r.hex.q, r.hex.r, r.ticks, r.cost]))
        .collect();
    let attacks: Vec<Value> = preview::attack_options(s, rules, me, unit)
        .into_iter()
        .map(|a| {
            let target = match a.target {
                AttackTarget::Unit(id) => json!({"kind":"Unit","id":id}),
                AttackTarget::City(id) => json!({"kind":"City","id":id}),
                AttackTarget::CityState(id) => json!({"kind":"CityState","id":id}),
            };
            let (forecast, blocked_by) = match a.result {
                Ok(AttackForecast::Engagement {
                    to_defender,
                    to_attacker,
                    defender_troops,
                    neutral,
                    city,
                }) => (
                    json!({"toDefender": to_defender, "toAttacker": to_attacker,
                           "defenderTroops": defender_troops, "neutral": neutral, "city": city}),
                    Value::Null,
                ),
                Ok(AttackForecast::CaptureCivilian) => (json!({"captureCivilian": true}), Value::Null),
                Err(b) => (Value::Null, blocked(b)),
            };
            json!({"target": target, "q": a.hex.q, "r": a.hex.r, "forecast": forecast, "blocked": blocked_by})
        })
        .collect();
    let found = s
        .units
        .get(unit as usize)
        .filter(|u| u.unit_type == UnitType::Settler)
        .map(|_| result(permutation_rules::checks::found_city(s, rules, me, unit)));
    json!({ "reach": reach, "attacks": attacks, "found": found })
}

pub fn city_preview(s: &WorldState, rules: &Ruleset, me: CivId, city: u32) -> Value {
    let outlook = preview::city_outlook(s, rules, city).map(|o| {
        json!({
            "food": o.yields.food, "prod": o.yields.prod, "gold": o.yields.gold,
            "science": o.yields.science, "influence": o.yields.influence,
            "amenities": o.amenities, "foodSurplus": o.food_surplus,
            "production": o.production, "ticksToGrow": o.ticks_to_grow,
            "growthThreshold": o.growth_threshold,
        })
    });
    let options: Vec<Value> = preview::queue_options(s, rules, me, city)
        .into_iter()
        .map(|o| json!({"item": item_dto(o.item), "cost": o.cost, "ticks": o.ticks, "blocked": result(o.result)}))
        .collect();
    json!({ "outlook": outlook, "options": options })
}

pub fn research_preview(s: &WorldState, rules: &Ruleset, me: CivId) -> Value {
    let v: Vec<Value> = preview::research_options(s, rules, me)
        .into_iter()
        .map(|o| {
            let info = permutation_rules::tech::info(o.tech);
            json!({
                "tech": name(o.tech), "era": info.era, "cost": o.cost,
                "prereqs": info.prereqs.iter().flatten().map(|p| name(*p)).collect::<Vec<_>>(),
                "held": s.civs[me as usize].techs.has(o.tech),
                "blocked": result(o.result),
            })
        })
        .collect();
    json!(v)
}

pub fn diplomacy_preview(s: &WorldState, rules: &Ruleset, me: CivId, other: CivId) -> Value {
    let v: Vec<Value> = preview::diplomacy_options(s, rules, me, other)
        .into_iter()
        .map(|o| json!({"action": name(o.action), "blocked": result(o.result)}))
        .collect();
    json!(v)
}

// ------------------------------------------------------------------ world view

pub fn relation_code(r: Relation) -> &'static str {
    match r {
        Relation::Peace => "peace",
        Relation::War { .. } => "war",
        Relation::Nap { .. } => "nap",
        Relation::Alliance { .. } => "alliance",
    }
}

/// Static map payload: sent once.
pub fn map_view(s: &WorldState) -> Value {
    let tiles: Vec<Value> = s
        .map
        .tiles
        .iter()
        .map(|t| {
            json!([t.hex.q, t.hex.r, name(t.terrain), t.river, t.resource.map(name)])
        })
        .collect();
    let hubs: Vec<Value> = s.hubs.iter().map(|h| json!([h.q, h.r])).collect();
    json!({ "radius": s.map.radius, "tiles": tiles, "hubs": hubs })
}

/// Everything the UI needs each poll, from `me`'s point of view. There is no
/// fog-of-war yet, so all positions are visible (labelled in the UI).
pub fn world_view(s: &WorldState, rules: &Ruleset, me: CivId) -> Value {
    let n = s.civs.len() as CivId;
    let owners: String = s
        .map
        .tiles
        .iter()
        .map(|t| {
            t.owner_city
                .and_then(|c| s.cities.get(c as usize))
                .filter(|c| c.alive)
                .and_then(|c| c.owner)
                .map_or('.', |o| char::from_digit(o as u32, 36).unwrap())
        })
        .collect();
    let ruins: Vec<Value> = s
        .map
        .tiles
        .iter()
        .filter_map(|t| t.ruin_peak_pop.map(|p| json!([t.hex.q, t.hex.r, p])))
        .collect();
    let civs: Vec<Value> = s
        .civs
        .iter()
        .map(|c| {
            let pop: u32 = s.living_cities_of(c.id).map(|x| x.pop).sum();
            let troops: u32 = s
                .units
                .iter()
                .filter(|u| u.alive && u.owner == Owner::Civ(c.id) && !u.unit_type.is_civilian())
                .map(|u| u.troops / 1000)
                .sum();
            let (rel, truce) = if c.id == me {
                ("self", 0)
            } else {
                (relation_code(s.relation(me, c.id)), s.truce_until[s.pair_index(me, c.id)])
            };
            json!({
                "id": c.id, "name": c.name, "kind": name(c.declared_kind),
                "cities": s.city_count(c.id), "pop": pop, "troops": troops,
                "techs": c.techs.count(),
                "dominion": c.scores.dominion,
                "concord": permutation_rules::scoring::concord_final(rules, c),
                "science": c.scores.science_total,
                "stages": c.scores.star_gate_stages,
                "stageTick": c.scores.star_gate_tick,
                "aggressor": c.is_aggressor(s.tick.saturating_sub(1), rules.aggressor_window),
                "everAllied": c.ever_allied,
                "relation": rel, "truceUntil": truce,
                "grievanceAgainstMe": if c.id == me { 0 } else { s.grievance(me, c.id) },
                "myGrievanceAgainst": if c.id == me { 0 } else { s.grievance(c.id, me) },
                "capital": c.capital,
                "protectionLost": c.protection_lost,
            })
        })
        .collect();
    let m = &s.civs[me as usize];
    let current_research = m.research_queue.first().map(|t| {
        json!({"tech": name(*t), "cost": permutation_rules::economy::tech_cost(rules, *t, s.city_count(me)),
               "store": m.science_store / 1000})
    });
    let upkeep_units = permutation_rules::economy::unit_upkeep(
        s.units.iter().filter(|u| u.alive && u.owner == Owner::Civ(me)),
    );
    let upkeep_cities = permutation_rules::economy::city_upkeep(s.city_count(me));
    let cities: Vec<Value> = s
        .cities
        .iter()
        .filter(|c| c.alive)
        .map(|c| {
            let mine = c.owner == Some(me);
            json!({
                "id": c.id, "q": c.hex.q, "r": c.hex.r, "owner": c.owner, "pop": c.pop,
                "defense": c.defense / 100, "defenseMax": (rules.city_defense_base + c.pop) * 10,
                "walls": c.buildings.has(Building::Walls),
                "stages": c.buildings.star_gate_stages(),
                "capital": c.owner.is_some_and(|o| s.civs[o as usize].capital == Some(c.id)),
                "razing": c.razing, "founder": c.founder,
                "buildings": BUILDINGS.iter().filter(|b| c.buildings.has(b.building)).map(|b| name(b.building)).collect::<Vec<_>>(),
                "queue": if mine { json!(c.queue.iter().map(|i| item_dto(*i)).collect::<Vec<_>>()) } else { Value::Null },
                "focus": if mine { json!(name(c.focus)) } else { Value::Null },
                "prod": if mine { json!(c.prod / 1000) } else { Value::Null },
                "food": if mine { json!(c.food / 1000) } else { Value::Null },
                "loyalty": if mine { json!(c.loyalty) } else { Value::Null },
            })
        })
        .collect();
    let units: Vec<Value> = s
        .units
        .iter()
        .filter(|u| u.alive)
        .map(|u| {
            let owner = match u.owner {
                Owner::Civ(c) => json!(c),
                Owner::Barbarian => json!("barbarian"),
            };
            let mine = u.owner == Owner::Civ(me);
            json!({
                "id": u.id, "q": u.hex.q, "r": u.hex.r, "owner": owner, "type": name(u.unit_type),
                "troops": u.troops / 100, "civilian": u.unit_type.is_civilian(),
                "path": if mine { json!(u.path.iter().map(|h| [h.q, h.r]).collect::<Vec<_>>()) } else { Value::Null },
                "moved": u.last_moved == Some(s.tick.saturating_sub(1)),
            })
        })
        .collect();
    let city_states: Vec<Value> = s
        .city_states
        .iter()
        .map(|cs| {
            let top = cs.influence.iter().copied().max().unwrap_or(0);
            json!({
                "id": cs.id, "q": cs.hex.q, "r": cs.hex.r, "specialty": name(cs.specialty),
                "pop": cs.pop, "defense": cs.defense / 100, "suzerain": cs.suzerain,
                "capturedBy": cs.captured_by,
                "myInfluence": cs.influence[me as usize] / 1000, "topInfluence": top / 1000,
            })
        })
        .collect();
    let proposals: Vec<Value> = s
        .proposals
        .iter()
        .filter(|p| p.to == me || p.from == me)
        .map(|p| {
            let (kind, bond) = match p.kind {
                ProposalKind::Peace => ("Peace", 0),
                ProposalKind::Nap { bond } => ("Nap", bond),
                ProposalKind::Alliance => ("Alliance", 0),
            };
            json!({"kind": kind, "bond": bond, "from": p.from, "to": p.to, "tick": p.tick,
                   "expires": p.tick + rules.proposal_ttl})
        })
        .collect();
    let pools: Vec<Value> = s
        .pools
        .iter()
        .enumerate()
        .map(|(i, p)| {
            json!({"good": if i == 0 { "Iron" } else { "Horses" },
                   "goods": p.goods / 1000, "gold": p.gold / 1000,
                   "spot": if p.goods > 0 { p.gold as f64 / p.goods as f64 } else { 0.0 }})
        })
        .collect();
    let _ = n;
    json!({
        "tick": s.tick, "ticks": rules.ticks_per_season,
        "protectionRadius": rules.protection_radius(s.tick),
        "me": me, "civs": civs, "owners": owners, "ruins": ruins,
        "cities": cities, "units": units, "cityStates": city_states,
        "proposals": proposals, "pools": pools,
        "economy": {
            "gold": m.gold / 1000, "goldIncome": m.last.gold, "iron": m.iron / 1000, "horses": m.horses / 1000,
            "ironIncome": m.last.iron, "horseIncome": m.last.horses,
            "influence": m.influence / 1000, "scienceStore": m.science_store / 1000,
            "research": current_research,
            "researchQueue": m.research_queue.iter().map(|t| name(*t)).collect::<Vec<_>>(),
            "techs": TECHS.iter().filter(|t| m.techs.has(t.tech)).map(|t| name(t.tech)).collect::<Vec<_>>(),
            "upkeepUnits": upkeep_units, "upkeepCities": upkeep_cities,
            "warWeariness": m.war_weariness,
            "budget": m.tick_budget, "bank": m.order_bank,
            "usdc": m.usdc, "exchangeSpent": m.exchange_spent,
            "exchangeCap": rules.entry_fee_usdc * rules.exchange_spend_cap_bps as u64 / 10_000,
            "protectionLost": m.protection_lost,
        },
        "vault": s.exchange_vault,
    })
}
