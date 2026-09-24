//! JSON API: order DTOs, the per-civ world view, and preview payloads.
//!
//! Names on the wire are the engine's own enum names (`"Granary"`,
//! `"Pikeman"`, `"Astronomy"`), so clients and agents never need a second
//! table of ids. Blocked reasons are `{ "code": "...", ...data }`; clients
//! localise by code.

use permutation_rules::battle::AttackForecast;
use permutation_rules::buildings::{Building, BUILDINGS};
use permutation_rules::checks::Blocked;
use permutation_rules::gov::{GovAction, MemberId, Role, NOBODY};
use permutation_rules::hex::Hex;
use permutation_rules::orders::{AttackTarget, Good, Order, Side, StandingOrder, StandingTarget};
use permutation_rules::preview;
use permutation_rules::state::{
    CivId, Focus, Owner, ProposalKind, QueueItem, Relation, StandingRule, WorldState,
};
use permutation_rules::tech::{Tech, TECHS};
use permutation_rules::units::{UnitType, UNIT_STATS};
use crate::fog::Fog;
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
    SetStanding { target: TargetDto, rule: StandingDto },
    /// Opens an earlier decision commitment (§7.5). `salt` is 32 hex chars.
    RevealRationale { tick: u16, policy: String, salt: String, text: String },
    /// The general or steward agrees to this tick's war on `civ` (V5 §5.6).
    ConsentWar { civ: CivId },
    /// Another officer allows treasury spending up to `usdc` this tick (V5 §7.5).
    ConsentSpend { usdc: u64 },
}

/// Standing rules (§13) as the client sends them.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all_fields = "camelCase")]
pub enum StandingDto {
    Clear,
    AutoDefend { radius: u8 },
    Retreat { ratio_bps: u32 },
    Patrol { route: Vec<[i32; 2]> },
    QueueRepeat { on: bool },
    AutoPurchase { max_gold: u32 },
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

fn good_dto(g: Good) -> GoodDto {
    match g {
        Good::Gold => GoodDto::Gold,
        Good::Iron => GoodDto::Iron,
        Good::Horses => GoodDto::Horses,
        Good::Food(city) => GoodDto::Food { city },
        Good::Production(city) => GoodDto::Production { city },
    }
}

fn side_name(s: Side) -> String {
    match s {
        Side::Buy => "Buy".into(),
        Side::Sell => "Sell".into(),
    }
}

impl OrderDto {
    /// The inverse of `to_order` (used to send engine-built orders, e.g. the
    /// bots', through the gateway). `from_order(o).to_order() == o`.
    pub fn from_order(o: &Order) -> OrderDto {
        use OrderDto as D;
        let target = |t: &AttackTarget| match *t {
            AttackTarget::Unit(id) => TargetDto::Unit { id },
            AttackTarget::City(id) => TargetDto::City { id },
            AttackTarget::CityState(id) => TargetDto::CityState { id },
        };
        match o {
            Order::MoveUnit { unit, path } => D::MoveUnit { unit: *unit, path: path.iter().map(|h| [h.q, h.r]).collect() },
            Order::Attack { army, target: t } => D::Attack { army: *army, target: target(t) },
            Order::FoundCity { settler } => D::FoundCity { settler: *settler },
            Order::SetQueue { city, items } => D::SetQueue { city: *city, items: items.iter().map(|i| item_dto(*i)).collect() },
            Order::SetFocus { city, focus } => D::SetFocus { city: *city, focus: name(focus) },
            Order::Purchase { city, gold } => D::Purchase { city: *city, gold: *gold },
            Order::SetResearch { techs } => D::SetResearch { techs: techs.iter().map(name).collect() },
            Order::DeclareWar { civ } => D::DeclareWar { civ: *civ },
            Order::ProposePeace { civ } => D::ProposePeace { civ: *civ },
            Order::AcceptPeace { civ } => D::AcceptPeace { civ: *civ },
            Order::ProposeNap { civ, bond } => D::ProposeNap { civ: *civ, bond: *bond },
            Order::AcceptNap { civ, bond } => D::AcceptNap { civ: *civ, bond: *bond },
            Order::BreakNap { civ } => D::BreakNap { civ: *civ },
            Order::ProposeAlliance { civ } => D::ProposeAlliance { civ: *civ },
            Order::AcceptAlliance { civ } => D::AcceptAlliance { civ: *civ },
            Order::LeaveAlliance => D::LeaveAlliance,
            Order::SendEnvoy { city_state, influence } => D::SendEnvoy { city_state: *city_state, influence: *influence },
            Order::Transfer { civ, good, amount } => D::Transfer { civ: *civ, good: good_dto(*good), amount: *amount },
            Order::MarketTrade { good, side, amount, limit_gold } => {
                D::MarketTrade { good: good_dto(*good), side: side_name(*side), amount: *amount, limit_gold: *limit_gold }
            }
            Order::ExchangeOrder { good, side, amount, price } => D::ExchangeOrder { good: good_dto(*good), side: side_name(*side), amount: *amount, price: *price },
            Order::Raze { city } => D::Raze { city: *city },
            Order::SetStanding { target: t, rule } => D::SetStanding {
                target: match *t {
                    StandingTarget::Unit(id) => TargetDto::Unit { id },
                    StandingTarget::City(id) => TargetDto::City { id },
                },
                rule: match rule {
                    StandingOrder::Clear => StandingDto::Clear,
                    StandingOrder::AutoDefend { radius } => StandingDto::AutoDefend { radius: *radius },
                    StandingOrder::Retreat { ratio_bps } => StandingDto::Retreat { ratio_bps: *ratio_bps },
                    StandingOrder::Patrol { route } => StandingDto::Patrol { route: route.iter().map(|h| [h.q, h.r]).collect() },
                    StandingOrder::QueueRepeat { on } => StandingDto::QueueRepeat { on: *on },
                    StandingOrder::AutoPurchase { max_gold } => StandingDto::AutoPurchase { max_gold: *max_gold },
                },
            },
            Order::RevealRationale { tick, policy, salt, text } => D::RevealRationale {
                tick: *tick,
                policy: String::from_utf8_lossy(policy).into_owned(),
                salt: salt.iter().map(|b| format!("{b:02x}")).collect(),
                text: String::from_utf8_lossy(text).into_owned(),
            },
            Order::ConsentWar { civ } => D::ConsentWar { civ: *civ },
            Order::ConsentSpend { usdc } => D::ConsentSpend { usdc: *usdc },
        }
    }

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
            D::ConsentWar { civ } => Order::ConsentWar { civ: *civ },
            D::ConsentSpend { usdc } => Order::ConsentSpend { usdc: *usdc },
            D::RevealRationale { tick, policy, salt, text } => {
                let raw: Vec<u8> = (0..salt.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(salt.get(i..i + 2).unwrap_or("zz"), 16))
                    .collect::<Result<_, _>>()
                    .map_err(|_| "salt must be hex".to_string())?;
                Order::RevealRationale {
                    tick: *tick,
                    policy: policy.as_bytes().to_vec(),
                    salt: raw.try_into().map_err(|_| "salt must be 16 bytes".to_string())?,
                    text: text.as_bytes().to_vec(),
                }
            }
            D::SetStanding { target, rule } => Order::SetStanding {
                target: match *target {
                    TargetDto::Unit { id } => StandingTarget::Unit(id),
                    TargetDto::City { id } => StandingTarget::City(id),
                    TargetDto::CityState { .. } => return Err("city-states take no standing rules".into()),
                },
                rule: match rule {
                    StandingDto::Clear => StandingOrder::Clear,
                    StandingDto::AutoDefend { radius } => StandingOrder::AutoDefend { radius: *radius },
                    StandingDto::Retreat { ratio_bps } => StandingOrder::Retreat { ratio_bps: *ratio_bps },
                    StandingDto::Patrol { route } => {
                        StandingOrder::Patrol { route: route.iter().map(|[q, r]| Hex::new(*q, *r)).collect() }
                    }
                    StandingDto::QueueRepeat { on } => StandingOrder::QueueRepeat { on: *on },
                    StandingDto::AutoPurchase { max_gold } => StandingOrder::AutoPurchase { max_gold: *max_gold },
                },
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
        B::OutOfBounds { .. } => "OutOfBounds",
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
        B::OutOfBounds { min, max } => {
            o.insert("min".into(), json!(min));
            o.insert("max".into(), json!(max));
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
    // `found` is null both for "not a settler" and "can found"; `canFound` says which.
    let can_found = s.units.get(unit as usize).filter(|u| u.unit_type == UnitType::Settler).map(|_| found.as_ref().is_some_and(Value::is_null));
    json!({ "reach": reach, "attacks": attacks, "found": found, "canFound": can_found })
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

/// Everything the UI needs each poll, from `me`'s point of view. `s` must be
/// `me`'s belief state (`Fog::belief`), so nothing outside its vision or
/// memory can leak; other civilizations' private totals are sent as `null`.
/// The world as `viewer` sees it (its belief state `s`), or with `None` the
/// omniscient spectator view of the full state.
pub fn world_view(s: &WorldState, rules: &Ruleset, viewer: Option<CivId>, fog: &Fog) -> Value {
    let omni = viewer.is_none();
    let me = viewer.unwrap_or(0);
    let seen = fog.seen(me);
    let memory = fog.memory(me);
    let in_sight = |h: Hex| omni || s.map.index_of(h).is_some_and(|i| seen[i]);
    let explored = |h: Hex| omni || s.map.index_of(h).is_some_and(|i| memory.explored[i]);
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
            let (rel, truce) = if omni {
                ("none", 0)
            } else if c.id == me {
                ("self", 0)
            } else {
                (relation_code(s.relation(me, c.id)), s.truce_until[s.pair_index(me, c.id)])
            };
            let own = omni || c.id == me;
            let a = &c.achievements;
            json!({
                "id": c.id, "name": c.name,
                // Foreign totals are what `me` knows: cities it has seen, armies in sight.
                "cities": s.city_count(c.id),
                "pop": if own { json!(pop) } else { Value::Null },
                "troops": if own { json!(troops) } else { Value::Null },
                "troopsSeen": troops,
                "techs": if own { json!(c.techs.count()) } else { Value::Null },
                // Milestone tiers and eras are announced publicly (V5 §6.3).
                "tiers": a.tiers, "era": a.era,
                "science": if own { json!(c.scores.science_total) } else { Value::Null },
                "stages": c.scores.star_gate_stages,
                "stageTick": c.scores.star_gate_tick,
                "aggressor": c.is_aggressor(s.tick.saturating_sub(1), rules.aggressor_window),
                "everAllied": c.ever_allied,
                "relation": rel, "truceUntil": truce,
                "grievanceAgainstMe": if omni || c.id == me { 0 } else { s.grievance(me, c.id) },
                "myGrievanceAgainst": if omni || c.id == me { 0 } else { s.grievance(c.id, me) },
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
            let mine = omni || c.owner == Some(me);
            let live = mine || in_sight(c.hex);
            json!({
                "id": c.id, "q": c.hex.q, "r": c.hex.r, "owner": c.owner, "pop": c.pop,
                "seenTick": if live { Value::Null } else { json!(memory.city_seen(c.id)) },
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
                "standing": if mine { json!({"repeatQueue": c.standing.repeat_queue, "autoPurchase": c.standing.auto_purchase}) } else { Value::Null },
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
            let mine = if omni { matches!(u.owner, Owner::Civ(_)) } else { u.owner == Owner::Civ(me) };
            json!({
                "id": u.id, "q": u.hex.q, "r": u.hex.r, "owner": owner, "type": name(u.unit_type),
                "troops": u.troops / 100, "civilian": u.unit_type.is_civilian(),
                "path": if mine { json!(u.path.iter().map(|h| [h.q, h.r]).collect::<Vec<_>>()) } else { Value::Null },
                "standing": if mine { standing_json(u.standing) } else { Value::Null },
                "moved": u.last_moved == Some(s.tick.saturating_sub(1)),
            })
        })
        .collect();
    let city_states: Vec<Value> = s
        .city_states
        .iter()
        .filter(|cs| explored(cs.hex))
        .map(|cs| {
            let top = cs.influence.iter().copied().max().unwrap_or(0);
            json!({
                "id": cs.id, "q": cs.hex.q, "r": cs.hex.r, "specialty": name(cs.specialty),
                "pop": cs.pop, "defense": cs.defense / 100, "suzerain": cs.suzerain,
                "capturedBy": cs.captured_by,
                "myInfluence": if omni { 0 } else { cs.influence[me as usize] / 1000 }, "topInfluence": top / 1000,
            })
        })
        .collect();
    let proposals: Vec<Value> = s
        .proposals
        .iter()
        .filter(|p| omni || p.to == me || p.from == me)
        .map(|p| {
            let (kind, bond) = match p.kind {
                ProposalKind::Peace => ("Peace", 0),
                ProposalKind::Nap { bond } => ("Nap", bond),
                ProposalKind::Alliance => ("Alliance", 0),
            };
            json!({"kind": kind, "bond": bond, "from": p.from, "to": p.to, "tick": p.tick,
                   "expires": p.tick + rules.proposal_ttl, "proposer": member_ref(p.credit.officer)})
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
    let economy = json!({
        "gold": m.gold / 1000, "goldIncome": m.last.gold, "iron": m.iron / 1000, "horses": m.horses / 1000,
        "ironIncome": m.last.iron, "horseIncome": m.last.horses,
        "influence": m.influence / 1000, "scienceStore": m.science_store / 1000,
        "research": current_research,
        "researchQueue": m.research_queue.iter().map(|t| name(*t)).collect::<Vec<_>>(),
        "techs": TECHS.iter().filter(|t| m.techs.has(t.tech)).map(|t| name(t.tech)).collect::<Vec<_>>(),
        "upkeepUnits": upkeep_units, "upkeepCities": upkeep_cities,
        "warWeariness": m.war_weariness,
        "budget": m.tick_budget,
        "offices": Role::ALL.iter().map(|r| json!({
            "role": name(r), "budget": rules.role_budget(m.tick_budget, r.index()),
            "bank": s.nations[me as usize].role_bank[r.index()],
            "spendable": permutation_rules::orders::spendable(s, rules, me, *r),
        })).collect::<Vec<_>>(),
        "treasury": m.usdc, "marketSpent": m.market_spent,
        "tariffBps": rules.tariff_bps(m.market_spent),
        "deliveries": s.deliveries.iter().filter(|d| d.civ == me).map(|d| json!({"good": good_dto(d.good), "qty": d.qty, "due": d.due})).collect::<Vec<_>>(),
        "protectionLost": m.protection_lost,
        "wealth": m.achievements.wealth,
    });
    json!({
        "tick": s.tick, "ticks": rules.ticks_per_season,
        "fog": if omni { "2".repeat(s.map.tiles.len()) } else { fog.code(me) },
        "protectionRadius": rules.protection_radius(s.tick),
        "me": viewer, "spectator": omni, "civs": civs, "owners": owners, "ruins": ruins,
        "cities": cities, "units": units, "cityStates": city_states,
        "proposals": proposals, "pools": pools,
        "economy": if omni { Value::Null } else { economy },
        "vault": s.exchange_vault,
    })
}

fn standing_json(r: StandingRule) -> Value {
    match r {
        StandingRule::None => Value::Null,
        StandingRule::AutoDefend { radius, anchor } => json!({"kind": "AutoDefend", "radius": radius, "anchor": [anchor.q, anchor.r]}),
        StandingRule::Retreat { ratio_bps } => json!({"kind": "Retreat", "ratioBps": ratio_bps}),
        StandingRule::Patrol { route, len, next } => json!({
            "kind": "Patrol", "next": next,
            "route": route[..len as usize].iter().map(|h| [h.q, h.r]).collect::<Vec<_>>(),
        }),
    }
}

/// Resolution-time checks for a batch, as far as `civ` can know them from its
/// belief state `s`. `validate_batch` only enforces what the chain checks at
/// submit time (structure, tick, budget); an order that is illegal when the
/// tick resolves is skipped. Outside agents use this to catch those early.
/// Returns one `{index, type, blocked}` per order that would be skipped now.
pub fn preflight(s: &WorldState, rules: &Ruleset, civ: CivId, orders: &[Order]) -> Vec<Value> {
    use permutation_rules::checks as c;
    let unit = |id: u32| -> Result<&permutation_rules::state::Unit, Blocked> {
        let u = s.units.get(id as usize).filter(|u| u.alive).ok_or(Blocked::UnknownUnit)?;
        if u.owner != Owner::Civ(civ) {
            return Err(Blocked::NotYours);
        }
        Ok(u)
    };
    let city = |id: u32| -> Result<(), Blocked> {
        let x = s.cities.get(id as usize).filter(|x| x.alive).ok_or(Blocked::UnknownCity)?;
        if x.owner != Some(civ) {
            return Err(Blocked::NotYours);
        }
        Ok(())
    };
    let mut planned = s.civs[civ as usize].techs;
    let mut out = Vec::new();
    for (i, o) in orders.iter().enumerate() {
        let r: Result<(), Blocked> = match o {
            Order::MoveUnit { unit: id, path } => match unit(*id) {
                Err(b) => Err(b),
                Ok(u) => {
                    // Each step must be a neighbour of the previous hex.
                    let mut at = u.hex;
                    let mut r = Ok(());
                    for h in path {
                        if at.distance(*h) != 1 {
                            out.push(json!({"index": i, "type": "MoveUnit", "blocked": {"code": "PathNotContiguous", "at": [h.q, h.r]}}));
                            break;
                        }
                        if let Err(b) = c::enter(s, rules, Some(civ), *h) {
                            r = Err(b);
                            break;
                        }
                        at = *h;
                    }
                    r
                }
            },
            Order::Attack { army, .. } => unit(*army).map(|_| ()),
            Order::FoundCity { settler } => c::found_city(s, rules, civ, *settler).map(|_| ()),
            Order::SetQueue { city: id, items } => items.iter().try_for_each(|it| c::queue_item(s, civ, *id, it)),
            Order::SetFocus { city: id, .. } | Order::Purchase { city: id, .. } | Order::Raze { city: id } => city(*id),
            Order::SetResearch { techs } => techs.iter().try_for_each(|t| {
                c::research(planned, *t)?;
                planned.insert(*t);
                Ok(())
            }),
            Order::DeclareWar { civ: t } => c::declare_war(s, civ, *t),
            Order::ProposePeace { civ: t } => c::propose_peace(s, civ, *t),
            Order::ProposeNap { civ: t, bond } => c::propose_nap(s, rules, civ, *t, *bond),
            Order::ProposeAlliance { civ: t } => c::propose_alliance(s, rules, civ, *t),
            _ => Ok(()),
        };
        if let Err(b) = r {
            let dto = OrderDto::from_order(o);
            let kind = serde_json::to_value(&dto).ok().and_then(|v| v["type"].as_str().map(String::from)).unwrap_or_default();
            out.push(json!({"index": i, "type": kind, "blocked": blocked(b)}));
        }
    }
    out
}

// ------------------------------------------------------------------ V5: nations and governance

/// A member id on the wire: `null` for nobody (the acting official).
pub fn member_ref(m: MemberId) -> Value {
    if m == NOBODY {
        Value::Null
    } else {
        json!(m)
    }
}

pub fn parse_role(s: &str) -> Option<Role> {
    Role::ALL.into_iter().find(|r| name(r) == s)
}

/// Display data the server keeps for each member (not part of the rules).
#[derive(Clone, Debug, Default, Serialize)]
pub struct MemberMeta {
    pub name: String,
    /// "human", "agent" or "undeclared" (self-declared, V5 D16).
    pub kind: String,
    /// Verified agent registration (e.g. ERC-8004), shown as a badge.
    pub attested: bool,
}

/// Governance actions as clients send them (`POST /api/gov`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum GovDto {
    /// Offices to stand for, by role name; empty withdraws.
    Stand { roles: Vec<String> },
    Vote { role: String, candidate: MemberId },
    Propose { role: String, orders: Vec<OrderDto> },
    Support { proposal: u32 },
    Recall { role: String },
}

impl GovDto {
    pub fn to_action(&self) -> Result<GovAction, String> {
        let role = |r: &str| parse_role(r).ok_or_else(|| format!("unknown office {r}"));
        Ok(match self {
            GovDto::Stand { roles } => GovAction::Stand { roles: roles.iter().map(|r| role(r).map(|x| x.bit())).sum::<Result<u8, _>>()? },
            GovDto::Vote { role: r, candidate } => GovAction::Vote { role: role(r)?, candidate: *candidate },
            GovDto::Propose { role: r, orders } => GovAction::Propose {
                role: role(r)?,
                orders: orders.iter().map(OrderDto::to_order).collect::<Result<_, _>>()?,
            },
            GovDto::Support { proposal } => GovAction::Support { proposal: *proposal },
            GovDto::Recall { role: r } => GovAction::Recall { role: role(r)? },
        })
    }

    pub fn from_action(a: &GovAction) -> GovDto {
        match a {
            GovAction::Stand { roles } => GovDto::Stand { roles: Role::ALL.iter().filter(|r| roles & r.bit() != 0).map(name).collect() },
            GovAction::Vote { role, candidate } => GovDto::Vote { role: name(role), candidate: *candidate },
            GovAction::Propose { role, orders } => GovDto::Propose { role: name(role), orders: orders.iter().map(OrderDto::from_order).collect() },
            GovAction::Support { proposal } => GovDto::Support { proposal: *proposal },
            GovAction::Recall { role } => GovDto::Recall { role: name(role) },
        }
    }
}

/// The government of `civ` (V5 §5): offices, the coming election, recalls
/// and proposals. Everything here is recorded on chain, so it is public to
/// the nation; other nations' proposals are hidden by the belief state.
pub fn gov_view(s: &WorldState, rules: &Ruleset, civ: CivId, meta: &[MemberMeta]) -> Value {
    let n = &s.nations[civ as usize];
    let who = |m: MemberId| {
        if m == NOBODY {
            return Value::Null;
        }
        let x = meta.get(m as usize).cloned().unwrap_or_default();
        json!({"id": m, "name": x.name, "kind": x.kind, "attested": x.attested})
    };
    let t = s.tick;
    let offices: Vec<Value> = Role::ALL
        .iter()
        .map(|r| {
            let i = r.index();
            let last = n.office_last_act[i];
            json!({
                "role": name(r), "holder": who(n.offices[i]), "since": n.office_since[i],
                "lastAct": if last == permutation_rules::gov::NEVER { Value::Null } else { json!(last) },
                "active": permutation_rules::gov::active_officer(s, rules, civ, *r).is_some(),
                "runnerUp": who(n.runner_up[i]),
            })
        })
        .collect();
    let candidates: Vec<Value> = Role::ALL
        .iter()
        .map(|r| {
            let list: Vec<Value> = s
                .members
                .iter()
                .enumerate()
                .filter(|(_, m)| m.civ == civ && m.standing_for & r.bit() != 0)
                .map(|(id, _)| {
                    let votes = n.votes.iter().filter(|v| v.role == *r && v.candidate == id as MemberId).count();
                    json!({"member": who(id as MemberId), "votes": votes})
                })
                .collect();
            json!({"role": name(r), "candidates": list})
        })
        .collect();
    let recalls: Vec<Value> = n
        .recalls
        .iter()
        .map(|rc| json!({"role": name(rc.role), "holder": who(rc.holder), "opened": rc.opened, "automatic": rc.automatic,
                        "yes": rc.yes.len(), "closes": rc.opened + rules.recall_ticks}))
        .collect();
    let proposals: Vec<Value> = n
        .proposals
        .iter()
        .map(|p| json!({
            "id": p.id, "role": name(p.role), "proposer": who(p.proposer), "tick": p.tick,
            "expires": p.tick + rules.proposal_ttl_ticks, "supporters": p.supporters.len(),
            "supportedBy": p.supporters, "orders": p.orders.iter().map(OrderDto::from_order).collect::<Vec<_>>(),
        }))
        .collect();
    let electorate = s
        .members
        .iter()
        .filter(|m| m.civ == civ && m.last_active != permutation_rules::gov::NEVER && t - m.last_active < rules.recall_electorate_ticks)
        .count();
    let next = permutation_rules::gov::next_term_start(rules, t);
    json!({
        "offices": offices, "candidates": candidates, "recalls": recalls, "proposals": proposals,
        "members": n.members, "electorate": electorate,
        "nextElection": next, "voteOpen": permutation_rules::gov::vote_open(rules, t),
        "voteFrom": next.map(|x| x.saturating_sub(rules.vote_window)),
        "idleRecallTicks": rules.idle_recall_ticks, "termTicks": rules.term_ticks,
    })
}

/// Milestone board of every nation (V5 §6): tiers, era and points now
/// ("if the season ended now"). Tiers and eras are public announcements.
pub fn achievements_view(s: &WorldState, rules: &Ruleset) -> Value {
    let facts = permutation_rules::scoring::facts_all(s, rules);
    let list: Vec<Value> = facts
        .iter()
        .enumerate()
        .map(|(civ, f)| {
            let sc = permutation_rules::scoring::score_of(rules, f);
            json!({
                "civ": civ, "tiers": sc.tiers, "era": sc.era, "pathPoints": sc.path_points,
                "eraPoints": sc.era_points, "points": sc.total(),
            })
        })
        .collect();
    json!({
        "nations": list,
        "tierPoints": rules.tier_points,
        "thresholds": {
            "hegemonyTiles": rules.hegemony_tiles, "hegemonyCities": rules.hegemony_cities,
            "prosperityPop": rules.prosperity_pop, "prosperityWealth": rules.prosperity_wealth,
            "scienceTechs": rules.science_techs, "concordTrade": rules.concord_trade,
        },
    })
}

/// What `civ` knows of its own milestone facts (V5 §6.2), for the board.
pub fn facts_view(s: &WorldState, rules: &Ruleset, civ: CivId) -> Value {
    let f = permutation_rules::scoring::facts_all(s, rules)[civ as usize];
    json!({
        "tiles": f.tiles, "capturedHeld": f.captured_held, "pop": f.pop, "wealth": f.wealth,
        "techs": f.techs, "starGateMax": f.star_gate_max, "partners": f.partners, "alliances": f.alliances,
        "suzerainties": f.suzerainties, "everSuzerain": f.ever_suzerain, "envoySent": f.envoy_sent, "trade": f.trade,
    })
}

/// The payout projection for the whole world (V5 §7): per nation, and per member.
pub fn settlement_view(p: &permutation_rules::payout::Settlement) -> Value {
    json!({
        "pool": p.pool, "refund": p.refund, "counted": p.counted,
        "nationShare": p.nation_share, "equalEach": p.equal_each, "perMember": p.per_member,
        "points": p.scores.iter().map(|x| x.total()).collect::<Vec<_>>(),
    })
}

/// One member as seen by itself: merit, activity, offices, projection.
pub fn member_view(s: &WorldState, rules: &Ruleset, m: MemberId, meta: &[MemberMeta], projection: Option<&permutation_rules::payout::Settlement>) -> Value {
    let Some(x) = s.members.get(m as usize) else { return Value::Null };
    let n = &s.nations[x.civ as usize];
    let offices: Vec<String> = Role::ALL.iter().filter(|r| n.holder(**r) == m).map(name).collect();
    let log: Vec<Value> = s
        .merit_log
        .iter()
        .filter(|e| e.member == m)
        .map(|e| json!({"path": name(e.path), "merit": e.milli as f64 / 1000.0, "what": String::from_utf8_lossy(e.what)}))
        .collect();
    let info = meta.get(m as usize).cloned().unwrap_or_default();
    json!({
        "id": m, "civ": x.civ, "name": info.name, "kind": info.kind, "attested": info.attested,
        "offices": offices,
        "standingFor": Role::ALL.iter().filter(|r| x.standing_for & r.bit() != 0).map(name).collect::<Vec<_>>(),
        "merit": {
            "hegemony": x.merit[0] as f64 / 1000.0, "prosperity": x.merit[1] as f64 / 1000.0,
            "science": x.merit[2] as f64 / 1000.0, "concord": x.merit[3] as f64 / 1000.0,
            "common": x.merit[4] as f64 / 1000.0, "total": x.merit_total() as f64 / 1000.0,
        },
        "activeWindows": x.active_windows(), "windowsNeeded": rules.active_windows_needed,
        "windows": rules.activity_windows(), "active": permutation_rules::gov::is_active_member(rules, x),
        "lastActive": if x.last_active == permutation_rules::gov::NEVER { Value::Null } else { json!(x.last_active) },
        "projectedPayout": projection.and_then(|p| p.per_member.get(m as usize)).copied(),
        "meritLog": log,
    })
}

/// Everyone in `civ`'s nation, for the plaza.
pub fn roster_view(s: &WorldState, rules: &Ruleset, civ: CivId, meta: &[MemberMeta]) -> Value {
    let list: Vec<Value> = s
        .members
        .iter()
        .enumerate()
        .filter(|(_, x)| x.civ == civ)
        .map(|(id, x)| {
            let info = meta.get(id).cloned().unwrap_or_default();
            json!({
                "id": id, "name": info.name, "kind": info.kind, "attested": info.attested,
                "merit": x.merit_total() as f64 / 1000.0, "active": permutation_rules::gov::is_active_member(rules, x),
                "activeWindows": x.active_windows(),
            })
        })
        .collect();
    json!(list)
}

/// This nation's orders that did not take effect last tick (v0.2 C12).
pub fn skipped_view(s: &WorldState, civ: CivId) -> Value {
    let list: Vec<Value> = s
        .last_skipped
        .iter()
        .filter(|k| k.civ == civ)
        .map(|k| json!({
            "role": Role::from_index(k.role as usize).map(name),
            "index": if k.index == u16::MAX { Value::Null } else { json!(k.index) },
            "reason": permutation_rules::checks::BLOCKED_NAMES.get(k.reason as usize).copied().unwrap_or("Unknown"),
        }))
        .collect();
    json!(list)
}

/// The office an order belongs to, by name (for clients' drafting).
pub fn office_of(s: &WorldState, o: &Order) -> Option<String> {
    Role::ALL.into_iter().find(|r| permutation_rules::orders::role_allows(s, *r, o)).map(name)
}
