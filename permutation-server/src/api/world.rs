//! The world as one viewer sees it (`/api/state`, `/api/map`).

use crate::fog::Fog;
use permutation_rules::buildings::{Building, BUILDINGS};
use permutation_rules::gov::Role;
use permutation_rules::state::{CivId, Owner, ProposalKind, Relation, StandingRule, WorldState};
use permutation_rules::tech::TECHS;
use permutation_rules::Ruleset;
use serde_json::{json, Value};

use super::dto::{good_dto, item_dto};
use super::{member_ref, name};

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
            json!([
                t.hex.q,
                t.hex.r,
                name(t.terrain),
                t.river,
                t.resource.map(name)
            ])
        })
        .collect();
    let hubs: Vec<Value> = s.hubs.iter().map(|h| json!([h.q, h.r])).collect();
    json!({ "radius": s.map.radius, "tiles": tiles, "hubs": hubs })
}

/// Everything the UI needs each poll, from `me`'s point of view (`None`: a
/// spectator). Perfect information (`fog`): every nation's cities, units,
/// totals, queues and paths are shown to everyone, as they are public on
/// chain; `me` only adds its relations, its economy panel and its sight.
pub fn world_view(s: &WorldState, rules: &Ruleset, viewer: Option<CivId>, fog: &Fog) -> Value {
    let omni = viewer.is_none();
    let me = viewer.unwrap_or(0);
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
                // One base-36 digit per tile (at most 8 nations; never panics).
                .and_then(|o| char::from_digit(o as u32, 36))
                .unwrap_or('.')
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
                (
                    relation_code(s.relation(me, c.id)),
                    s.truce_until[s.pair_index(me, c.id)],
                )
            };
            let a = &c.achievements;
            json!({
                "id": c.id, "name": c.name,
                "cities": s.city_count(c.id),
                "pop": pop,
                "troops": troops,
                "troopsSeen": troops,
                "techs": c.techs.count(),
                "tiers": a.tiers, "era": a.era,
                "science": c.scores.science_total,
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
        json!({"tech": name(*t), "cost": permutation_rules::economy::civ_tech_cost(s, rules, me, *t),
               "store": m.science_store / 1000})
    });
    let upkeep_units = permutation_rules::economy::unit_upkeep(
        s.units
            .iter()
            .filter(|u| u.alive && u.owner == Owner::Civ(me)),
    );
    let upkeep_cities = permutation_rules::economy::city_upkeep(s.city_count(me));
    let cities: Vec<Value> = s
        .cities
        .iter()
        .filter(|c| c.alive)
        .map(|c| {
            json!({
                "id": c.id, "q": c.hex.q, "r": c.hex.r, "owner": c.owner, "pop": c.pop,
                "seenTick": Value::Null,
                "defense": c.defense / 100, "defenseMax": (rules.city_defense_base + c.pop) * 10,
                "walls": c.buildings.has(Building::Walls),
                "stages": c.buildings.star_gate_stages(),
                "capital": c.owner.is_some_and(|o| s.civs[o as usize].capital == Some(c.id)),
                "razing": c.razing, "founder": c.founder,
                "buildings": BUILDINGS.iter().filter(|b| c.buildings.has(b.building)).map(|b| name(b.building)).collect::<Vec<_>>(),
                "queue": c.queue.iter().map(|i| item_dto(*i)).collect::<Vec<_>>(),
                "focus": name(c.focus),
                "prod": c.prod / 1000,
                "food": c.food / 1000,
                "loyalty": c.loyalty,
                "standing": {"repeatQueue": c.standing.repeat_queue, "autoPurchase": c.standing.auto_purchase},
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
            json!({
                "id": u.id, "q": u.hex.q, "r": u.hex.r, "owner": owner, "type": name(u.unit_type),
                "troops": u.troops / 100, "civilian": u.unit_type.is_civilian(),
                "path": u.path.iter().map(|h| [h.q, h.r]).collect::<Vec<_>>(),
                "standing": standing_json(u.standing),
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
                "myInfluence": if omni { 0 } else { cs.influence[me as usize] / 1000 }, "topInfluence": top / 1000,
            })
        })
        .collect();
    let proposals: Vec<Value> = s
        .proposals
        .iter()
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
        // Perfect information: nothing is fogged. `sight` (display only) is
        // what `me`'s units and cities overlook.
        "fog": "2".repeat(s.map.tiles.len()),
        "sight": if omni { Value::Null } else { json!(fog.sight_code(me)) },
        "protectionRadius": rules.protection_radius(s.tick),
        "me": viewer, "spectator": omni, "civs": civs, "owners": owners, "ruins": ruins,
        "cities": cities, "units": units, "cityStates": city_states,
        "proposals": proposals, "pools": pools,
        "economy": if omni { Value::Null } else { economy },
        "vault": s.exchange_vault,
    })
}

pub(super) fn standing_json(r: StandingRule) -> Value {
    match r {
        StandingRule::None => Value::Null,
        StandingRule::AutoDefend { radius, anchor } => {
            json!({"kind": "AutoDefend", "radius": radius, "anchor": [anchor.q, anchor.r]})
        }
        StandingRule::Retreat { ratio_bps } => json!({"kind": "Retreat", "ratioBps": ratio_bps}),
        StandingRule::Patrol { route, len, next } => json!({
            "kind": "Patrol", "next": next,
            "route": route[..len as usize].iter().map(|h| [h.q, h.r]).collect::<Vec<_>>(),
        }),
    }
}
