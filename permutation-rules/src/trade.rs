//! Trade between nations (§10.5) and the trade accounting that feeds the
//! Concord milestones and merit (V5 §6.2, §7.3): transfers in phase 2, and
//! `record_trade` / `trade_value` for the gold market too.

use crate::checks::Blocked;
use crate::diplomacy::{event, valid_pair};
use crate::fixed::MILLI;
use crate::gov::{Credit, Path};
use crate::merit;
use crate::orders::{Good, Order};
use crate::params::Ruleset;
use crate::state::{CivId, WorldState};
use crate::tick::accepted;
use alloc::vec::Vec;

/// Transfers between civilizations (§10.5). Frozen from tick 162 at
/// submission. The caps apply to the sum of one tick's transfers of a good
/// from one civ to another (v0.2 C4).
pub fn apply_transfers(state: &mut WorldState, rules: &Ruleset) {
    let mut sent: Vec<(CivId, CivId, u8, u64)> = Vec::new(); // (from, to, good kind, amount)
    for (civ, order, credit, origin) in accepted(state, |o| matches!(o, Order::Transfer { .. })) {
        if let Order::Transfer {
            civ: to,
            good,
            amount,
        } = order
        {
            let kind = crate::markets::GoodKind::of(good) as u8;
            let before: u64 = sent
                .iter()
                .filter(|s| s.0 == civ && s.1 == to && s.2 == kind)
                .map(|s| s.3)
                .sum();
            match transfer(state, rules, civ, to, good, amount, before, credit) {
                Ok(()) => sent.push((civ, to, kind, amount as u64)),
                Err(why) => state.skip(civ, origin, why.code()),
            }
        }
    }
}

fn stock_mut(civ: &mut crate::state::Civ, good: Good) -> &mut i64 {
    match good {
        Good::Gold => &mut civ.gold,
        Good::Iron => &mut civ.iron,
        _ => &mut civ.horses,
    }
}

/// Value of goods in whole gold for trade volume (V5 §6.2): gold at face
/// value, iron and horses at the gold market's spot price, production at
/// the purchase rate, food at 1.
pub fn trade_value(state: &WorldState, rules: &Ruleset, good: Good, qty: u32) -> u64 {
    let spot = |i: usize| {
        let p = &state.pools[i];
        if p.goods > 0 {
            (p.gold / p.goods).max(1) as u64
        } else {
            1
        }
    };
    qty as u64
        * match good {
            Good::Gold | Good::Food(_) => 1,
            Good::Iron => spot(0),
            Good::Horses => spot(1),
            Good::Production(_) => rules.purchase_gold_per_prod as u64,
        }
}

/// Add `value` of trade between `civ` and counterparty slot `with` (a civ
/// id, or `civs.len()` for the gold market), and credit the issuer with the
/// growth of `civ`'s effective volume (V5 §7.3).
pub fn record_trade(
    state: &mut WorldState,
    rules: &Ruleset,
    civ: CivId,
    with: usize,
    value: u64,
    credit: Credit,
) {
    let before =
        crate::scoring::trade_effective(rules, &state.civs[civ as usize].achievements.trade);
    state.civs[civ as usize].achievements.trade[with] += value;
    let after =
        crate::scoring::trade_effective(rules, &state.civs[civ as usize].achievements.trade);
    let gained = after.saturating_sub(before) * MILLI as u64 / rules.merit_trade_div.max(1) as u64;
    merit::credit(state, credit, Path::Concord, gained, b"trade");
}

#[allow(clippy::too_many_arguments)]
fn transfer(
    state: &mut WorldState,
    rules: &Ruleset,
    from: CivId,
    to: CivId,
    good: Good,
    amount: u32,
    already: u64,
    credit: Credit,
) -> Result<(), Blocked> {
    if !valid_pair(state, from, to) {
        return Err(Blocked::UnknownCiv);
    }
    if amount == 0 {
        return Err(Blocked::NothingToSell);
    }
    if state.at_war(from, to) {
        return Err(Blocked::AlreadyAtWar);
    }
    let last = state.civs[from as usize].last;
    let qty = amount as i64 * MILLI;
    match good {
        Good::Gold | Good::Iron | Good::Horses => {
            let cap = match good {
                Good::Gold => last.gold.max(10),
                Good::Iron => last.iron.max(1),
                _ => last.horses.max(1),
            };
            if already + amount as u64 > cap as u64 {
                return Err(Blocked::OverCap { cap });
            }
            if *stock_mut(&mut state.civs[from as usize], good) < qty {
                return Err(Blocked::NothingToSell);
            }
            *stock_mut(&mut state.civs[from as usize], good) -= qty;
            *stock_mut(&mut state.civs[to as usize], good) += qty;
        }
        Good::Food(city) | Good::Production(city) => {
            let is_food = matches!(good, Good::Food(_));
            let cap = if is_food {
                last.max_city_food_surplus
            } else {
                last.max_city_prod
            };
            if already + amount as u64 > cap as u64 {
                return Err(Blocked::OverCap { cap });
            }
            let target_ok = state
                .cities
                .get(city as usize)
                .is_some_and(|c| c.alive && c.owner == Some(to));
            if !target_ok {
                return Err(Blocked::UnknownCity);
            }
            // Taken from the sender's city holding the most of that good (then lowest id).
            let store = |c: &crate::state::City| if is_food { c.food } else { c.prod };
            let src = state
                .living_cities_of(from)
                .filter(|c| store(c) >= qty)
                .max_by_key(|c| (store(c), core::cmp::Reverse(c.id)))
                .map(|c| c.id as usize)
                .ok_or(Blocked::NothingToSell)?;
            if is_food {
                state.cities[src].food -= qty;
                state.cities[city as usize].food += qty;
            } else {
                state.cities[src].prod -= qty;
                state.cities[city as usize].prod += qty;
            }
        }
    }
    event(state, b"transfer", from, to);
    let value = trade_value(state, rules, good, amount);
    record_trade(state, rules, from, to as usize, value, credit);
    record_trade(state, rules, to, from as usize, value, Credit::NONE);
    Ok(())
}
