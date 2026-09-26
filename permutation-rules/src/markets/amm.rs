//! The gold AMM (§11.2): constant-product pools at trade hubs. All orders
//! against a pool net out and clear once per tick at one price.

use crate::checks::Blocked;
use crate::fixed::{BPS_ONE, MILLI};
use crate::gov::Credit;
use crate::orders::{Good, Order, Side};
use crate::params::Ruleset;
use crate::state::{CivId, WorldState};
use crate::tech::Tech;
use crate::tick::accepted;
use alloc::vec;
use alloc::vec::Vec;

/// One order against a pool, in whole units of the pool's good.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AmmOrder {
    pub side: Side,
    pub qty: u32,
    /// Buy: most gold (whole units) the trader will pay incl. fee.
    /// Sell: least gold (whole units) the trader will receive after fee.
    pub limit_gold: u32,
    /// Gold (milli) the trader can spend; only checked for buys.
    pub budget_milli: i64,
}

/// Result of clearing one pool for one tick.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AmmClearing {
    /// Price per whole unit, in milli-gold, before fees. 0 if nothing traded.
    pub price_milli: i64,
    /// Per input order: `Some((gold_delta_milli, goods_delta_units))` if filled.
    /// Buyers have negative gold and positive goods; sellers the reverse.
    pub fills: Vec<Option<(i64, i64)>>,
    /// Signed changes to the pool reserves (milli).
    pub pool_goods_delta: i64,
    pub pool_gold_delta: i64,
    /// Total fees (milli-gold): the hub share and the burned rest.
    pub hub_fee_milli: i64,
    pub burned_milli: i64,
}

/// Price the net flow of a set of orders against the pool (§11.2).
/// Returns `(price_milli_per_unit, pool_goods_delta_milli, pool_gold_delta_milli)`,
/// or `None` if the pool cannot supply the net demand.
fn price_net(goods: i64, gold: i64, buys: u64, sells: u64) -> Option<(i64, i64, i64)> {
    let (x, y) = (goods as u128, gold as u128);
    let k = x * y;
    if sells >= buys {
        let net = sells - buys;
        if net == 0 {
            // Fully internal: trade at the spot price, the pool is untouched.
            return Some(((y * 1000 / x) as i64, 0, 0));
        }
        let new_x = x + net as u128 * 1000;
        let new_y = k.div_ceil(new_x); // round in the pool's favour
        let out = y - new_y;
        let p = out / net as u128; // floor: the pool keeps the rounding
        Some((p as i64, net as i64 * 1000, -((p * net as u128) as i64)))
    } else {
        let net = buys - sells;
        let take = net as u128 * 1000;
        if take >= x {
            return None;
        }
        let new_y = k.div_ceil(x - take);
        let inn = new_y - y;
        let p = inn.div_ceil(net as u128); // ceil: the pool keeps the rounding
        Some((p as i64, -(net as i64 * 1000), (p * net as u128) as i64))
    }
}

/// Batch-clear one pool (§11.2). Orders violating their limit (or a buyer's
/// budget) at the batch price are dropped and the batch is re-priced, up to
/// `rules.amm_limit_iterations` times; then any remaining violators are
/// dropped once more and the final price is used as is.
pub fn clear_amm(rules: &Ruleset, goods: i64, gold: i64, orders: &[AmmOrder]) -> AmmClearing {
    let fee = |notional: i64| notional * rules.amm_fee_bps as i64 / BPS_ONE as i64;
    let fee128 = |notional: i128| notional * rules.amm_fee_bps as i128 / BPS_ONE as i128;
    let mut live: Vec<bool> = orders.iter().map(|o| o.qty > 0).collect();
    // In i128: a price near an emptied pool times a large quantity must not wrap.
    let violates = |o: &AmmOrder, p: i64| {
        let notional = o.qty as i128 * p as i128;
        match o.side {
            Side::Buy => {
                let cost = notional + fee128(notional);
                cost > o.limit_gold as i128 * MILLI as i128 || cost > o.budget_milli as i128
            }
            Side::Sell => notional - fee128(notional) < o.limit_gold as i128 * MILLI as i128,
        }
    };
    let over_budget = |o: &AmmOrder, p: i64| {
        let notional = o.qty as i128 * p as i128;
        o.side == Side::Buy && notional + fee128(notional) > o.budget_milli as i128
    };
    let mut priced;
    let mut round = 0u8;
    loop {
        let buys: u64 = orders
            .iter()
            .zip(&live)
            .filter(|(o, l)| **l && o.side == Side::Buy)
            .map(|(o, _)| o.qty as u64)
            .sum();
        let sells: u64 = orders
            .iter()
            .zip(&live)
            .filter(|(o, l)| **l && o.side == Side::Sell)
            .map(|(o, _)| o.qty as u64)
            .sum();
        if buys == 0 && sells == 0 {
            return AmmClearing {
                fills: vec![None; orders.len()],
                ..Default::default()
            };
        }
        let Some(p) = price_net(goods, gold, buys, sells) else {
            // Net demand exceeds the pool: drop the largest buy (then the lowest index) and retry.
            let worst = (0..orders.len())
                .filter(|i| live[*i] && orders[*i].side == Side::Buy)
                .max_by_key(|i| (orders[*i].qty, core::cmp::Reverse(*i)));
            match worst {
                Some(i) => live[i] = false,
                None => {
                    return AmmClearing {
                        fills: vec![None; orders.len()],
                        ..Default::default()
                    }
                }
            }
            // Dropping an order the pool cannot supply is not a re-pricing
            // round: oversized buys cannot use the rounds up (audit WP08).
            continue;
        };
        let mut dropped = false;
        for (i, o) in orders.iter().enumerate() {
            if live[i] && violates(o, p.0) {
                live[i] = false;
                dropped = true;
            }
        }
        if !dropped || round >= rules.amm_limit_iterations {
            if dropped {
                // Final pass: price what is left and accept it.
                let buys: u64 = orders
                    .iter()
                    .zip(&live)
                    .filter(|(o, l)| **l && o.side == Side::Buy)
                    .map(|(o, _)| o.qty as u64)
                    .sum();
                let sells: u64 = orders
                    .iter()
                    .zip(&live)
                    .filter(|(o, l)| **l && o.side == Side::Sell)
                    .map(|(o, _)| o.qty as u64)
                    .sum();
                priced = if buys + sells == 0 {
                    None
                } else {
                    price_net(goods, gold, buys, sells)
                };
                // A buyer never pays more than its reserved gold: if the
                // final price breaks a budget, nothing clears this tick.
                if let Some((p, _, _)) = priced {
                    if orders
                        .iter()
                        .zip(&live)
                        .any(|(o, l)| *l && over_budget(o, p))
                    {
                        priced = None;
                    }
                }
            } else {
                priced = Some(p);
            }
            break;
        }
        round += 1;
    }
    let Some((p, dgoods, dgold)) = priced else {
        return AmmClearing {
            fills: vec![None; orders.len()],
            ..Default::default()
        };
    };
    let mut out = AmmClearing {
        price_milli: p,
        fills: vec![None; orders.len()],
        pool_goods_delta: dgoods,
        pool_gold_delta: dgold,
        ..Default::default()
    };
    // Every live buy costs at most its budget (an i64); a sell receives at
    // most what the buys and the pool pay. Checked anyway: nothing clears
    // rather than wrap.
    let fits = orders
        .iter()
        .zip(&live)
        .all(|(o, l)| !*l || i64::try_from(o.qty as i128 * p as i128 * 2).is_ok());
    if !fits {
        return AmmClearing {
            fills: vec![None; orders.len()],
            ..Default::default()
        };
    }
    for (i, o) in orders.iter().enumerate() {
        if !live[i] {
            continue;
        }
        let notional = o.qty as i64 * p;
        let f = fee(notional);
        let hub = notional * rules.hub_fee_bps as i64 / BPS_ONE as i64;
        out.hub_fee_milli += hub;
        out.burned_milli += f - hub;
        out.fills[i] = Some(match o.side {
            Side::Buy => (-(notional + f), o.qty as i64),
            Side::Sell => (notional - f, -(o.qty as i64)),
        });
    }
    out
}

/// Civs holding a trade hub this tick (§11.1), one entry per held hub.
pub fn hub_holders(state: &WorldState) -> Vec<CivId> {
    state
        .hubs
        .iter()
        .filter_map(|h| state.territory_owner(*h))
        .collect()
}

fn pool_index(good: Good) -> Option<usize> {
    match good {
        Good::Iron => Some(0),
        Good::Horses => Some(1),
        _ => None,
    }
}

/// Phase 2: clear both AMM pools and settle (§11.2).
pub fn apply_amm(state: &mut WorldState, rules: &Ruleset) {
    let mut per_pool: [Vec<(CivId, AmmOrder, Credit)>; 2] = [Vec::new(), Vec::new()];
    // Per civ, across both pools and every order of the tick (audit WP08):
    // gold set aside for its buys, goods set aside for its sells (milli).
    let n = state.civs.len();
    let mut gold_reserved = vec![0i64; n];
    let mut goods_reserved = vec![[0i64; 2]; n];
    for (civ, order, credit, origin) in accepted(state, |o| matches!(o, Order::MarketTrade { .. }))
    {
        let Order::MarketTrade {
            good,
            side,
            amount,
            limit_gold,
        } = order
        else {
            continue;
        };
        let c = &state.civs[civ as usize];
        let why = if !c.techs.has(Tech::Currency) {
            Some(Blocked::NeedsTech(Tech::Currency))
        } else if pool_index(good).is_none() {
            Some(Blocked::NothingToSell)
        } else {
            let pi = pool_index(good).unwrap_or(0);
            let stock = if pi == 0 { c.iron } else { c.horses };
            let free = stock - goods_reserved[civ as usize][pi];
            // Sellers must own the goods, net of their earlier sells this tick.
            (side == Side::Sell && free < amount as i64 * MILLI).then_some(Blocked::NothingToSell)
        };
        if let Some(why) = why {
            state.skip(civ, origin, why.code());
            continue;
        }
        let pi = pool_index(good).unwrap_or(0);
        let budget_milli = match side {
            Side::Sell => {
                goods_reserved[civ as usize][pi] += amount as i64 * MILLI;
                0
            }
            Side::Buy => {
                // At most the limit, out of the gold not yet set aside.
                let left = c.gold - gold_reserved[civ as usize];
                let r = (limit_gold as i64 * MILLI).min(left);
                if r <= 0 {
                    state.skip(
                        civ,
                        origin,
                        Blocked::NotEnoughGold {
                            need: limit_gold,
                            have: (left / MILLI).max(0) as u32,
                        }
                        .code(),
                    );
                    continue;
                }
                gold_reserved[civ as usize] += r;
                r
            }
        };
        per_pool[pi].push((
            civ,
            AmmOrder {
                side,
                qty: amount,
                limit_gold,
                budget_milli,
            },
            credit,
        ));
    }
    let holders = hub_holders(state);
    for (pi, list) in per_pool.iter().enumerate() {
        if list.is_empty() {
            continue;
        }
        let orders: Vec<AmmOrder> = list.iter().map(|(_, o, _)| *o).collect();
        let pool = state.pools[pi];
        let result = clear_amm(rules, pool.goods, pool.gold, &orders);
        let market = state.civs.len();
        for (i, fill) in result.fills.iter().enumerate() {
            let Some((dgold, dgoods)) = *fill else {
                continue;
            };
            let civ = list[i].0;
            let c = &mut state.civs[civ as usize];
            c.gold += dgold;
            if pi == 0 {
                c.iron += dgoods * MILLI;
            } else {
                c.horses += dgoods * MILLI;
            }
            // Gold market trades count as trade volume (V5 §6.2).
            let value = (dgold.unsigned_abs()) / MILLI as u64;
            crate::trade::record_trade(state, rules, civ, market, value, list[i].2);
        }
        state.pools[pi].goods += result.pool_goods_delta;
        state.pools[pi].gold += result.pool_gold_delta;
        // Hub fee split equally among held hubs; an unheld hub's share is burned.
        if !state.hubs.is_empty() {
            let share = result.hub_fee_milli / state.hubs.len() as i64;
            for civ in &holders {
                state.civs[*civ as usize].gold += share;
            }
        }
        if result.price_milli > 0 {
            state.push_event(b"amm", &(result.price_milli as u64).to_le_bytes());
        }
    }
}
