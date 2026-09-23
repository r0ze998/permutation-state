//! Markets (§11): trade hubs, the gold AMM and the USDC Exchange.
//!
//! Both markets clear **once per tick at one price**, so the order in which
//! orders arrived never matters (V4 §5.2):
//! - the AMM nets all orders per pool and executes only the net against the
//!   constant-product curve; every trader pays or receives the same price;
//! - the Exchange runs a uniform-price call auction per good.
//!
//! The clearing functions are pure (`clear_amm`, `call_auction`) so clients
//! and agents can forecast with exactly the engine's arithmetic.

use crate::fixed::{BPS_ONE, MILLI};
use crate::orders::{Good, Order, Side};
use crate::params::Ruleset;
use crate::rng::tie_key;
use crate::state::{CivId, WorldState};
use crate::tech::Tech;
use crate::tick::{accepted, TickInput};
use alloc::vec;
use alloc::vec::Vec;

// ================================================================== AMM

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
    let mut live: Vec<bool> = orders.iter().map(|o| o.qty > 0).collect();
    let violates = |o: &AmmOrder, p: i64| {
        let notional = o.qty as i64 * p;
        match o.side {
            Side::Buy => {
                let cost = notional + fee(notional);
                cost > o.limit_gold as i64 * MILLI || cost > o.budget_milli
            }
            Side::Sell => notional - fee(notional) < o.limit_gold as i64 * MILLI,
        }
    };
    let mut priced = None;
    for round in 0..=rules.amm_limit_iterations {
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
            continue;
        };
        let mut dropped = false;
        for (i, o) in orders.iter().enumerate() {
            if live[i] && violates(o, p.0) {
                live[i] = false;
                dropped = true;
            }
        }
        if !dropped || round == rules.amm_limit_iterations {
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
            } else {
                priced = Some(p);
            }
            break;
        }
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
        .filter_map(|h| {
            state
                .map
                .tile(*h)?
                .owner_city
                .and_then(|c| state.cities.get(c as usize))
                .filter(|c| c.alive)?
                .owner
        })
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
pub fn apply_amm(state: &mut WorldState, rules: &Ruleset, input: &TickInput) {
    let mut per_pool: [Vec<(CivId, AmmOrder)>; 2] = [Vec::new(), Vec::new()];
    for (civ, _, orders) in accepted(state, rules, input) {
        let c = &state.civs[civ as usize];
        if !c.techs.has(Tech::Currency) {
            continue;
        }
        for order in orders {
            let Order::MarketTrade {
                good,
                side,
                amount,
                limit_gold,
            } = order
            else {
                continue;
            };
            let Some(pi) = pool_index(good) else { continue };
            let stock = if pi == 0 { c.iron } else { c.horses };
            if side == Side::Sell && stock < amount as i64 * MILLI {
                continue; // sellers must own the goods
            }
            per_pool[pi].push((
                civ,
                AmmOrder {
                    side,
                    qty: amount,
                    limit_gold,
                    budget_milli: c.gold,
                },
            ));
        }
    }
    let holders = hub_holders(state);
    for (pi, list) in per_pool.iter().enumerate() {
        if list.is_empty() {
            continue;
        }
        let orders: Vec<AmmOrder> = list.iter().map(|(_, o)| *o).collect();
        let pool = state.pools[pi];
        let result = clear_amm(rules, pool.goods, pool.gold, &orders);
        for (i, fill) in result.fills.iter().enumerate() {
            let Some((dgold, dgoods)) = fill else {
                continue;
            };
            let c = &mut state.civs[list[i].0 as usize];
            c.gold += dgold;
            if pi == 0 {
                c.iron += dgoods * MILLI;
            } else {
                c.horses += dgoods * MILLI;
            }
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

// ================================================================== Exchange

/// Goods on the Exchange, grouped for one auction each (§11.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GoodKind {
    Gold,
    Iron,
    Horses,
    Food,
    Production,
}

impl GoodKind {
    pub const ALL: [GoodKind; 5] = [
        GoodKind::Gold,
        GoodKind::Iron,
        GoodKind::Horses,
        GoodKind::Food,
        GoodKind::Production,
    ];
    pub fn of(good: Good) -> GoodKind {
        match good {
            Good::Gold => GoodKind::Gold,
            Good::Iron => GoodKind::Iron,
            Good::Horses => GoodKind::Horses,
            Good::Food(_) => GoodKind::Food,
            Good::Production(_) => GoodKind::Production,
        }
    }
}

/// One side of a call auction. `key` breaks ties (lower first).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bid {
    pub qty: u32,
    /// USDC base units per whole unit.
    pub price: u64,
    pub key: u64,
}

/// Uniform-price call auction (§11.3).
///
/// The clearing price maximises matched volume; ties go to the smallest
/// imbalance, then the lowest price. Buyers are filled by price (high first),
/// sellers by price (low first), ties by `key`. Returns the price and the
/// filled quantity per bid.
pub fn call_auction(buys: &[Bid], sells: &[Bid]) -> Option<(u64, Vec<u32>, Vec<u32>)> {
    let mut prices: Vec<u64> = buys.iter().chain(sells).map(|b| b.price).collect();
    prices.sort_unstable();
    prices.dedup();
    let mut best: Option<(u64, u64, u64)> = None; // (volume, imbalance, price)
    for &p in &prices {
        let demand: u64 = buys
            .iter()
            .filter(|b| b.price >= p)
            .map(|b| b.qty as u64)
            .sum();
        let supply: u64 = sells
            .iter()
            .filter(|s| s.price <= p)
            .map(|s| s.qty as u64)
            .sum();
        let volume = demand.min(supply);
        if volume == 0 {
            continue;
        }
        let imbalance = demand.abs_diff(supply);
        let better = match best {
            None => true,
            Some((v, im, _)) => volume > v || (volume == v && imbalance < im),
        };
        if better {
            best = Some((volume, imbalance, p));
        }
    }
    let (volume, _, price) = best?;
    let fill = |bids: &[Bid], eligible: &dyn Fn(&Bid) -> bool, high_first: bool| {
        let mut order: Vec<usize> = (0..bids.len()).filter(|i| eligible(&bids[*i])).collect();
        order.sort_by(|a, b| {
            let (pa, pb) = (bids[*a].price, bids[*b].price);
            let by_price = if high_first { pb.cmp(&pa) } else { pa.cmp(&pb) };
            by_price.then(bids[*a].key.cmp(&bids[*b].key))
        });
        let mut left = volume;
        let mut out = vec![0u32; bids.len()];
        for i in order {
            let q = (bids[i].qty as u64).min(left);
            out[i] = q as u32;
            left -= q;
        }
        out
    };
    let b = fill(buys, &|b| b.price >= price, true);
    let s = fill(sells, &|s| s.price <= price, false);
    Some((price, b, s))
}

struct ExOrder {
    civ: CivId,
    good: Good,
    bid: Bid,
}

/// Per-tick buy cap in whole units (§11.3), from the previous tick's yields.
fn buy_cap(state: &WorldState, civ: CivId, kind: GoodKind) -> u32 {
    let last = state.civs[civ as usize].last;
    match kind {
        GoodKind::Gold => last.gold,
        GoodKind::Iron => last.iron.max(1),
        GoodKind::Horses => last.horses.max(1),
        GoodKind::Food => last.max_city_food_surplus,
        GoodKind::Production => last.max_city_prod,
    }
}

/// Whole units a civ may sell of this good right now (§11.3 re-sale rule).
fn sellable(state: &WorldState, civ: CivId, good: Good) -> u32 {
    let c = &state.civs[civ as usize];
    let bought = c.exchange_bought[GoodKind::of(good) as usize] as i64;
    let stock_milli = match good {
        Good::Gold => c.gold,
        Good::Iron => c.iron,
        Good::Horses => c.horses,
        Good::Food(city) | Good::Production(city) => match state.cities.get(city as usize) {
            Some(x) if x.alive && x.owner == Some(civ) => {
                if matches!(good, Good::Food(_)) {
                    x.food
                } else {
                    x.prod
                }
            }
            _ => 0,
        },
    };
    (stock_milli / MILLI - bought).max(0) as u32
}

fn fee_of(rules: &Ruleset, notional: u64) -> u64 {
    notional * rules.exchange_fee_bps as u64 / BPS_ONE as u64
}

/// Phase 2: run one call auction per good and settle in USDC (§11.3).
/// Submission already rejects Exchange orders from tick 120 on.
pub fn apply_exchange(state: &mut WorldState, rules: &Ruleset, input: &TickInput) {
    let cap_total = rules.entry_fee_usdc * rules.exchange_spend_cap_bps as u64 / BPS_ONE as u64;
    let mut orders: Vec<(bool, ExOrder)> = Vec::new(); // (is_buy, order)
    let mut serial = 0u64;
    for (civ, _, list) in accepted(state, rules, input) {
        for order in list {
            let Order::ExchangeOrder {
                good,
                side,
                amount,
                price,
            } = order
            else {
                continue;
            };
            serial += 1;
            if amount == 0 || price == 0 {
                continue;
            }
            let key = tie_key(&state.tick_seed, ((civ as u64) << 32) | serial);
            let kind = GoodKind::of(good);
            let qty = match side {
                Side::Buy => {
                    // Delivery city must be the buyer's own.
                    if let Good::Food(c) | Good::Production(c) = good {
                        if !state
                            .cities
                            .get(c as usize)
                            .is_some_and(|x| x.alive && x.owner == Some(civ))
                        {
                            continue;
                        }
                    }
                    let already: u32 = orders
                        .iter()
                        .filter(|(b, o)| *b && o.civ == civ && GoodKind::of(o.good) == kind)
                        .map(|(_, o)| o.bid.qty)
                        .sum();
                    let cap = buy_cap(state, civ, kind).saturating_sub(already);
                    // Affordable at the limit price, within balance and the season cap.
                    let c = &state.civs[civ as usize];
                    let room = c.usdc.min(cap_total.saturating_sub(c.exchange_spent));
                    let unit = price + fee_of(rules, price);
                    let affordable = (room / unit.max(1)).min(u32::MAX as u64) as u32;
                    amount.min(cap).min(affordable)
                }
                Side::Sell => {
                    let already: u32 = orders
                        .iter()
                        .filter(|(b, o)| !*b && o.civ == civ && o.good == good)
                        .map(|(_, o)| o.bid.qty)
                        .sum();
                    amount.min(sellable(state, civ, good).saturating_sub(already))
                }
            };
            if qty > 0 {
                orders.push((
                    side == Side::Buy,
                    ExOrder {
                        civ,
                        good,
                        bid: Bid { qty, price, key },
                    },
                ));
            }
        }
    }

    for kind in GoodKind::ALL {
        let buys: Vec<&ExOrder> = orders
            .iter()
            .filter(|(b, o)| *b && GoodKind::of(o.good) == kind)
            .map(|(_, o)| o)
            .collect();
        let sells: Vec<&ExOrder> = orders
            .iter()
            .filter(|(b, o)| !*b && GoodKind::of(o.good) == kind)
            .map(|(_, o)| o)
            .collect();
        if buys.is_empty() || sells.is_empty() {
            continue;
        }
        let bb: Vec<Bid> = buys.iter().map(|o| o.bid).collect();
        let sb: Vec<Bid> = sells.iter().map(|o| o.bid).collect();
        let Some((price, bfill, sfill)) = call_auction(&bb, &sb) else {
            continue;
        };

        // Pair fills in order (buyers and sellers each sorted as the auction filled them).
        let mut sell_queue: Vec<(usize, u32)> = sfill
            .iter()
            .enumerate()
            .filter(|(_, q)| **q > 0)
            .map(|(i, q)| (i, *q))
            .collect();
        sell_queue.sort_by_key(|(i, _)| (sb[*i].price, sb[*i].key));
        let mut buy_queue: Vec<(usize, u32)> = bfill
            .iter()
            .enumerate()
            .filter(|(_, q)| **q > 0)
            .map(|(i, q)| (i, *q))
            .collect();
        buy_queue.sort_by_key(|(i, _)| (core::cmp::Reverse(bb[*i].price), bb[*i].key));
        let mut si = 0;
        for (bi, mut bq) in buy_queue {
            while bq > 0 && si < sell_queue.len() {
                let (s_idx, ref mut sq) = sell_queue[si];
                let q = bq.min(*sq);
                settle(state, rules, buys[bi], sells[s_idx], q, price);
                bq -= q;
                *sq -= q;
                if *sq == 0 {
                    si += 1;
                }
            }
        }
    }
}

fn settle(
    state: &mut WorldState,
    rules: &Ruleset,
    buy: &ExOrder,
    sell: &ExOrder,
    qty: u32,
    price: u64,
) {
    let notional = qty as u64 * price;
    let fee = fee_of(rules, notional);
    let vault = fee * rules.vault_share_bps as u64 / BPS_ONE as u64;
    // USDC
    {
        let b = &mut state.civs[buy.civ as usize];
        b.usdc -= notional + fee;
        b.exchange_spent += notional + fee;
        b.exchange_bought[GoodKind::of(buy.good) as usize] += qty;
    }
    state.civs[sell.civ as usize].usdc += notional;
    state.exchange_vault += vault;
    state.exchange_ops += fee - vault;
    // Goods
    let q = qty as i64 * MILLI;
    match (sell.good, buy.good) {
        (Good::Gold, _) => {
            state.civs[sell.civ as usize].gold -= q;
            state.civs[buy.civ as usize].gold += q;
        }
        (Good::Iron, _) => {
            state.civs[sell.civ as usize].iron -= q;
            state.civs[buy.civ as usize].iron += q;
        }
        (Good::Horses, _) => {
            state.civs[sell.civ as usize].horses -= q;
            state.civs[buy.civ as usize].horses += q;
        }
        (Good::Food(from), Good::Food(to)) => {
            state.cities[from as usize].food -= q;
            state.cities[to as usize].food += q;
        }
        (Good::Production(from), Good::Production(to)) => {
            state.cities[from as usize].prod -= q;
            state.cities[to as usize].prod += q;
        }
        _ => {}
    }
    let mut payload = [0u8; 12];
    payload[..2].copy_from_slice(&buy.civ.to_le_bytes());
    payload[2..4].copy_from_slice(&sell.civ.to_le_bytes());
    payload[4..8].copy_from_slice(&qty.to_le_bytes());
    payload[8..].copy_from_slice(&(price as u32).to_le_bytes());
    state.push_event(b"exchange", &payload);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::Preset;

    fn rules() -> Ruleset {
        Ruleset::new(Preset::Blitz)
    }

    fn buy(qty: u32, limit: u32) -> AmmOrder {
        AmmOrder {
            side: Side::Buy,
            qty,
            limit_gold: limit,
            budget_milli: i64::MAX,
        }
    }
    fn sell(qty: u32, limit: u32) -> AmmOrder {
        AmmOrder {
            side: Side::Sell,
            qty,
            limit_gold: limit,
            budget_milli: 0,
        }
    }

    #[test]
    fn amm_single_buy_follows_the_curve_and_keeps_k() {
        let r = rules();
        let (x, y) = (200_000i64, 2_000_000i64); // 200 iron, 2000 gold
        let c = clear_amm(&r, x, y, &[buy(10, 1_000)]);
        // Exact curve: new_y = ceil(k / 190_000) = 2_105_264 → 105_264 in, price ceil(105_264/10).
        assert_eq!(c.price_milli, 10_527);
        assert_eq!(c.pool_goods_delta, -10_000);
        assert_eq!(c.pool_gold_delta, 105_270);
        let k0 = x as i128 * y as i128;
        let k1 = (x + c.pool_goods_delta) as i128 * (y + c.pool_gold_delta) as i128;
        assert!(k1 >= k0, "the curve invariant never decreases");
        let (gold, goods) = c.fills[0].unwrap();
        assert_eq!(goods, 10);
        assert_eq!(gold, -(105_270 + 105_270 * 300 / 10_000));
    }

    #[test]
    fn amm_batch_gives_everyone_the_same_price() {
        let r = rules();
        let c = clear_amm(&r, 200_000, 2_000_000, &[buy(5, 1_000), buy(5, 1_000)]);
        let (g1, _) = c.fills[0].unwrap();
        let (g2, _) = c.fills[1].unwrap();
        assert_eq!(g1, g2);
        let solo = clear_amm(&r, 200_000, 2_000_000, &[buy(10, 1_000)]);
        assert_eq!(c.price_milli, solo.price_milli, "two 5s clear like one 10");
    }

    #[test]
    fn amm_nets_opposite_orders_at_spot() {
        let r = rules();
        let c = clear_amm(&r, 200_000, 2_000_000, &[buy(7, 1_000), sell(7, 0)]);
        assert_eq!(c.price_milli, 10_000); // spot: 2000 gold / 200 iron
        assert_eq!((c.pool_goods_delta, c.pool_gold_delta), (0, 0));
    }

    #[test]
    fn amm_conserves_gold_exactly() {
        let r = rules();
        let orders = [buy(12, 1_000), sell(3, 0), buy(4, 1_000), sell(20, 0)];
        let c = clear_amm(&r, 200_000, 2_000_000, &orders);
        let traders: i64 = c.fills.iter().flatten().map(|(g, _)| *g).sum();
        // What traders lose (net) = what the pool gains + fees.
        assert_eq!(
            -traders,
            c.pool_gold_delta + c.hub_fee_milli + c.burned_milli
        );
        let goods: i64 = c.fills.iter().flatten().map(|(_, q)| *q * 1000).sum();
        assert_eq!(goods, -c.pool_goods_delta);
    }

    #[test]
    fn amm_drops_limit_violators_and_reprices() {
        let r = rules();
        // A buyer willing to pay only 50 gold for 10 iron (~108 needed) is dropped.
        let c = clear_amm(&r, 200_000, 2_000_000, &[buy(10, 50), buy(3, 1_000)]);
        assert!(c.fills[0].is_none());
        assert!(c.fills[1].is_some());
        assert_eq!(
            c.price_milli,
            clear_amm(&r, 200_000, 2_000_000, &[buy(3, 1_000)]).price_milli
        );
    }

    #[test]
    fn amm_rejects_draining_the_pool() {
        let r = rules();
        let c = clear_amm(&r, 200_000, 2_000_000, &[buy(250, u32::MAX)]);
        assert!(c.fills[0].is_none());
    }

    fn bid(qty: u32, price: u64, key: u64) -> Bid {
        Bid { qty, price, key }
    }

    #[test]
    fn call_auction_maximises_volume_at_one_price() {
        // Buyers: 10 @ 5, 10 @ 3. Sellers: 8 @ 2, 8 @ 4.
        let (p, b, s) = call_auction(
            &[bid(10, 5, 1), bid(10, 3, 2)],
            &[bid(8, 2, 3), bid(8, 4, 4)],
        )
        .unwrap();
        // At 3: demand 20, supply 8 → 8. At 4: demand 10, supply 16 → 10. At 5: 10/16 → 10.
        assert_eq!(
            p, 4,
            "max volume 10; tie between 4 and 5 broken by imbalance (6 vs 6) then lowest price"
        );
        assert_eq!(b, vec![10, 0]);
        assert_eq!(s, vec![8, 2]);
    }

    #[test]
    fn call_auction_breaks_ties_by_key() {
        let (_, b, _) = call_auction(&[bid(5, 5, 9), bid(5, 5, 1)], &[bid(5, 5, 3)]).unwrap();
        assert_eq!(b, vec![0, 5]);
    }

    #[test]
    fn call_auction_needs_crossing_orders() {
        assert!(call_auction(&[bid(5, 2, 1)], &[bid(5, 3, 2)]).is_none());
    }
}
