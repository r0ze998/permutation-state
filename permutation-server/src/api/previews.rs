//! Previews of a nation's options (legal actions with costs, forecasts and
//! blocked reasons) and the preflight of an order list.

use permutation_rules::battle::AttackForecast;
use permutation_rules::checks::Blocked;
use permutation_rules::orders::{AttackTarget, Order, StandingOrder, StandingTarget};
use permutation_rules::preview;
use permutation_rules::state::{CivId, Owner, StandingRule, WorldState};
use permutation_rules::units::UnitType;
use permutation_rules::Ruleset;
use serde_json::{json, Value};

use super::blocked::{blocked, result};
use super::dto::{item_dto, OrderDto};
use super::name;

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
    let can_found = s
        .units
        .get(unit as usize)
        .filter(|u| u.unit_type == UnitType::Settler)
        .map(|_| found.as_ref().is_some_and(Value::is_null));
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

/// Resolution-time checks for a batch, as far as `civ` can know them from its
/// belief state `s`. `validate_batch` only enforces what the chain checks at
/// submit time (structure, tick, budget); an order that is illegal when the
/// tick resolves is skipped. Outside agents use this to catch those early.
/// Returns one `{index, type, blocked}` per order that would be skipped now.
pub fn preflight(s: &WorldState, rules: &Ruleset, civ: CivId, orders: &[Order]) -> Vec<Value> {
    use permutation_rules::checks as c;
    let unit = |id: u32| -> Result<&permutation_rules::state::Unit, Blocked> {
        let u = s
            .units
            .get(id as usize)
            .filter(|u| u.alive)
            .ok_or(Blocked::UnknownUnit)?;
        if u.owner != Owner::Civ(civ) {
            return Err(Blocked::NotYours);
        }
        Ok(u)
    };
    let city = |id: u32| -> Result<(), Blocked> {
        let x = s
            .cities
            .get(id as usize)
            .filter(|x| x.alive)
            .ok_or(Blocked::UnknownCity)?;
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
            Order::SetQueue { city: id, items } => items
                .iter()
                .try_for_each(|it| c::queue_item(s, civ, *id, it)),
            Order::SetFocus { city: id, .. }
            | Order::Purchase { city: id, .. }
            | Order::Raze { city: id } => city(*id),
            Order::SetResearch { techs } => techs.iter().try_for_each(|t| {
                c::research(planned, *t)?;
                planned.insert(*t);
                Ok(())
            }),
            Order::DeclareWar { civ: t } => c::declare_war(s, civ, *t),
            Order::ProposePeace { civ: t } => c::propose_peace(s, civ, *t),
            Order::ProposeNap { civ: t, bond } => c::propose_nap(s, rules, civ, *t, *bond),
            Order::ProposeAlliance { civ: t } => c::propose_alliance(s, rules, civ, *t),
            // A new unit rule past the nation's cap is stored but waits
            // unused (only `ruled_units_cap` ruled units per nation run).
            Order::SetStanding { target, rule } => {
                c::standing(s, civ, *target, rule).and_then(|()| match target {
                    StandingTarget::Unit(id)
                        if !matches!(rule, StandingOrder::Clear)
                            && s.units
                                .get(*id as usize)
                                .is_some_and(|u| u.standing == StandingRule::None) =>
                    {
                        c::ruled_units_below_cap(s, civ)
                    }
                    _ => Ok(()),
                })
            }
            _ => Ok(()),
        };
        if let Err(b) = r {
            let dto = OrderDto::from_order(o);
            let kind = serde_json::to_value(&dto)
                .ok()
                .and_then(|v| v["type"].as_str().map(String::from))
                .unwrap_or_default();
            out.push(json!({"index": i, "type": kind, "blocked": blocked(b)}));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use permutation_rules::genesis::{nation_entries, new_season};
    use permutation_rules::orders::ruled_units_cap;
    use permutation_rules::state::Unit;
    use permutation_rules::Preset;

    /// A new unit rule for a nation already at its cap of ruled units is
    /// flagged `OverCap` (it would wait unused); changing or clearing an
    /// already ruled unit's rule is not.
    #[test]
    fn preflight_flags_a_unit_rule_past_the_cap() {
        let rules = Ruleset::new(Preset::Blitz);
        let mut s = new_season(&rules, &[5; 32], &[6; 32], &nation_entries(6)).unwrap();
        let civ: CivId = 0;
        let cap = ruled_units_cap(s.civs.len());
        assert_eq!(cap, 8);
        let home = s
            .units
            .iter()
            .find(|u| u.owner == Owner::Civ(civ))
            .unwrap()
            .hex;
        for _ in 0..=cap {
            let id = s.units.len() as u32;
            s.units.push(Unit {
                id,
                owner: Owner::Civ(civ),
                unit_type: UnitType::Spearman,
                troops: 1_000,
                hex: home,
                path: Vec::new(),
                last_moved: None,
                used_full_mp: false,
                standing: StandingRule::Retreat { ratio_bps: 15_000 },
                alive: true,
            });
        }
        // `cap` ruled armies and one more without a rule.
        let free = s.units.len() as u32 - 1;
        s.units[free as usize].standing = StandingRule::None;
        let ruled = free - 1;
        let set = |unit: u32, rule: StandingOrder| Order::SetStanding {
            target: StandingTarget::Unit(unit),
            rule,
        };
        let w = preflight(
            &s,
            &rules,
            civ,
            &[set(free, StandingOrder::Retreat { ratio_bps: 20_000 })],
        );
        assert_eq!(w.len(), 1, "{w:?}");
        assert_eq!(w[0]["type"], "SetStanding");
        assert_eq!(w[0]["blocked"]["code"], "OverCap");
        assert_eq!(w[0]["blocked"]["cap"], 8);
        let fine = [
            set(ruled, StandingOrder::AutoDefend { radius: 2 }),
            set(ruled, StandingOrder::Clear),
            set(free, StandingOrder::Clear),
        ];
        assert!(preflight(&s, &rules, civ, &fine).is_empty());
        // The rule itself is still checked.
        let w = preflight(
            &s,
            &rules,
            civ,
            &[set(ruled, StandingOrder::AutoDefend { radius: 9 })],
        );
        assert_eq!(w[0]["blocked"]["code"], "OutOfBounds");
        // One ruled army fewer: room for the new rule.
        s.units[ruled as usize].alive = false;
        let w = preflight(
            &s,
            &rules,
            civ,
            &[set(free, StandingOrder::Retreat { ratio_bps: 20_000 })],
        );
        assert!(w.is_empty(), "{w:?}");
    }
}
