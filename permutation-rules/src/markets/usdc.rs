//! The USDC market (V5 §7.5, spec v0.2 §11.3): a uniform-price call
//! auction per good between nation treasuries, a rising tariff on the
//! buyer's cumulative spend, delivery after `delivery_ticks`, no trades with
//! oneself or an enemy, nothing bought into a Star Gate city.

use crate::checks::Blocked;
use crate::fixed::{BPS_ONE, MILLI};
use crate::gov::{Role, NOBODY};
use crate::orders::{Good, Order, Side};
use crate::params::Ruleset;
use crate::rng::tie_key;
use crate::state::{CivId, Delivery, QueueItem, WorldState};
use crate::tick::accepted;
use alloc::vec;
use alloc::vec::Vec;

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
    /// Office and position of the order, for `Skip`.
    origin: (u8, u16),
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

/// A city whose current item is a Star Gate stage: bought production may
/// not reach it (v0.2 C8, V5 §7.5).
fn building_star_gate(state: &WorldState, city: u32) -> bool {
    state
        .cities
        .get(city as usize)
        .and_then(|c| c.queue.first())
        .is_some_and(|q| matches!(q, QueueItem::Building(b) if b.is_star_gate()))
}

/// Where bought goods may be delivered to `civ` now.
fn deliverable(state: &WorldState, civ: CivId, good: Good) -> bool {
    match good {
        Good::Food(c) | Good::Production(c) => {
            let own = state
                .cities
                .get(c as usize)
                .is_some_and(|x| x.alive && x.owner == Some(civ));
            own && !(matches!(good, Good::Production(_)) && building_star_gate(state, c))
        }
        _ => true,
    }
}

/// What a buyer pays for `qty` units at `price` with `tariff_bps`:
/// `(notional, income)`, the fee and the tariff rounded **up, per unit**
/// (V5 §7.5). The reservation (at the buyer's limit) and the settlement (at
/// the clearing price, never above the limit) use this one function, and
/// the cost never falls as the price rises, so a fill never costs more than
/// was reserved. `None` if the total does not fit a u64.
pub fn trade_cost(rules: &Ruleset, qty: u32, price: u64, tariff_bps: u32) -> Option<(u64, u64)> {
    let p = price as u128;
    let b = BPS_ONE as u128;
    let per_unit_income =
        (p * rules.exchange_fee_bps as u128).div_ceil(b) + (p * tariff_bps as u128).div_ceil(b);
    let notional = u64::try_from(qty as u128 * p).ok()?;
    let income = u64::try_from(qty as u128 * per_unit_income).ok()?;
    notional.checked_add(income)?;
    Some((notional, income))
}

/// Price per unit a buyer pays at `price`: price + fee + tariff.
fn unit_cost(rules: &Ruleset, price: u64, tariff_bps: u32) -> Option<u64> {
    trade_cost(rules, 1, price, tariff_bps).map(|(n, i)| n + i)
}

/// Most the diplomat may spend from the treasury this tick: what is left of
/// the term's allowance without consent, or more with the `ConsentSpend` of
/// a seated officer other than the diplomat, in their own batch (V5 §7.5 ④).
pub fn spend_limit(state: &WorldState, rules: &Ruleset, civ: CivId) -> u64 {
    let free = rules
        .spend_consent_usdc
        .saturating_sub(state.civs[civ as usize].free_spent);
    free.max(spend_consent(state, civ))
}

/// The largest valid `ConsentSpend` for `civ` this tick (0 if none).
pub fn spend_consent(state: &WorldState, civ: CivId) -> u64 {
    let diplomat = state
        .nations
        .get(civ as usize)
        .map_or(NOBODY, |n| n.holder(Role::Diplomat));
    state
        .tick_orders
        .iter()
        .filter(|a| a.civ == civ)
        .flat_map(|a| a.iter())
        // A consent counts only from a seated officer of another office who
        // is not the diplomat: the caretaker (officer NOBODY) never consents,
        // and nobody consents to their own spending (V5 §7.5 ④).
        .filter(|(_, credit, origin)| {
            origin.0 != Role::Diplomat as u8
                && credit.officer != NOBODY
                && credit.officer != diplomat
        })
        .filter_map(|(o, _, _)| match o {
            Order::ConsentSpend { usdc } => Some(*usdc),
            _ => None,
        })
        .max()
        .unwrap_or(0)
}

/// USDC `civ` may still commit this tick (market purchases and contract
/// escrow share it): the spend limit less what it already committed, and
/// never more than its free treasury.
pub fn treasury_room(state: &WorldState, rules: &Ruleset, civ: CivId, committed: u64) -> u64 {
    spend_limit(state, rules, civ)
        .saturating_sub(committed)
        .min(state.civs[civ as usize].free_usdc())
}

/// The reason a treasury payment of more than `room` is refused: the spend
/// limit (a second officer's consent would allow it) or the treasury.
pub fn short_of_usdc(limit_binds: bool) -> Blocked {
    if limit_binds {
        Blocked::NeedsSpendConsent
    } else {
        Blocked::NotEnoughUsdc
    }
}

/// Phase 2 (start): hand over goods bought `delivery_ticks` ago (V5 §7.5).
/// Food and production go to the named city if it is still the buyer's and
/// (for production) not building a Star Gate stage, else to the capital
/// under the same rule, else they are lost.
pub fn deliver(state: &mut WorldState) {
    let tick = state.tick;
    let due: Vec<Delivery> = state
        .deliveries
        .iter()
        .copied()
        .filter(|d| d.due <= tick)
        .collect();
    state.deliveries.retain(|d| d.due > tick);
    for d in due {
        let q = d.qty as i64 * MILLI;
        let civ = d.civ as usize;
        match d.good {
            Good::Gold => state.civs[civ].gold += q,
            Good::Iron => state.civs[civ].iron += q,
            Good::Horses => state.civs[civ].horses += q,
            Good::Food(city) | Good::Production(city) => {
                let is_food = matches!(d.good, Good::Food(_));
                let capital = state.civs[civ].capital;
                let target = [Some(city), capital].into_iter().flatten().find(|c| {
                    let g = if is_food {
                        Good::Food(*c)
                    } else {
                        Good::Production(*c)
                    };
                    deliverable(state, d.civ, g)
                });
                if let Some(c) = target {
                    if is_food {
                        state.cities[c as usize].food += q;
                    } else {
                        state.cities[c as usize].prod += q;
                    }
                }
            }
        }
    }
}

/// Phase 2: run one call auction per good between treasuries and settle in
/// USDC (V5 §7.5). Submission rejects market orders from `exchange_freeze_tick` on and in
/// seasons without a market.
pub fn apply_exchange(state: &mut WorldState, rules: &Ruleset, escrowed: &[u64]) {
    let n = state.civs.len();
    let tariff: Vec<u32> = state
        .civs
        .iter()
        .map(|c| rules.tariff_bps(c.market_spent))
        .collect();
    // Contract escrow placed this tick shares the limit (V5 §18.6).
    let limit: Vec<u64> = (0..n as CivId)
        .map(|c| {
            treasury_room(
                state,
                rules,
                c,
                escrowed.get(c as usize).copied().unwrap_or(0),
            )
        })
        .collect();
    let mut reserved = vec![0u64; n];
    let mut orders: Vec<(bool, ExOrder)> = Vec::new(); // (is_buy, order)
    let mut serial = 0u64;
    for (civ, order, _, origin) in accepted(state, |o| matches!(o, Order::ExchangeOrder { .. })) {
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
        // `check_structure` (phase 0) already refused larger values.
        if amount == 0
            || price == 0
            || amount > rules.max_trade_amount
            || price > rules.exchange_max_price
        {
            state.skip(civ, origin, Blocked::NothingToSell.code());
            continue;
        }
        let key = tie_key(&state.tick_seed, ((civ as u64) << 32) | serial);
        let kind = GoodKind::of(good);
        let qty = match side {
            Side::Buy => {
                if !deliverable(state, civ, good) {
                    state.skip(civ, origin, Blocked::CannotBuyStarGate.code());
                    continue;
                }
                let already: u32 = orders
                    .iter()
                    .filter(|(b, o)| *b && o.civ == civ && GoodKind::of(o.good) == kind)
                    .map(|(_, o)| o.bid.qty)
                    .sum();
                let cap = buy_cap(state, civ, kind).saturating_sub(already);
                // Affordable at the limit price, within the treasury and the tick's spend limit.
                let room = limit[civ as usize].saturating_sub(reserved[civ as usize]);
                let affordable = unit_cost(rules, price, tariff[civ as usize])
                    .map_or(0, |unit| (room / unit.max(1)).min(u32::MAX as u64) as u32);
                let q = amount.min(cap).min(affordable);
                // q × unit ≤ room, so this always fits.
                let cost = trade_cost(rules, q, price, tariff[civ as usize]).map(|(n, i)| n + i);
                match cost {
                    Some(c) if q > 0 => reserved[civ as usize] += c,
                    _ => {
                        let why = short_of_usdc(
                            limit[civ as usize] < state.civs[civ as usize].free_usdc(),
                        );
                        state.skip(civ, origin, why.code());
                        continue;
                    }
                }
                q
            }
            Side::Sell => {
                let already: u32 = orders
                    .iter()
                    .filter(|(b, o)| !*b && o.civ == civ && o.good == good)
                    .map(|(_, o)| o.bid.qty)
                    .sum();
                let q = amount.min(sellable(state, civ, good).saturating_sub(already));
                if q == 0 {
                    state.skip(civ, origin, Blocked::NothingToSell.code());
                }
                q
            }
        };
        if qty > 0 {
            orders.push((
                side == Side::Buy,
                ExOrder {
                    civ,
                    good,
                    bid: Bid { qty, price, key },
                    origin,
                },
            ));
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

        // Pair fills: buyers by priority take from sellers by priority,
        // skipping themselves and enemies; what cannot be paired stays
        // unfilled at the same price (v0.2 C8).
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
        for (bi, mut bq) in buy_queue {
            for (s_idx, sq) in sell_queue.iter_mut() {
                if bq == 0 {
                    break;
                }
                let (b, sl) = (buys[bi], sells[*s_idx]);
                if *sq == 0 || b.civ == sl.civ || state.at_war(b.civ, sl.civ) {
                    continue;
                }
                let q = bq.min(*sq);
                if !settle(state, rules, b, sl, q, price, tariff[b.civ as usize]) {
                    // Cannot happen (a fill costs no more than was reserved):
                    // refuse the rest of this buy rather than wrap.
                    state.skip(b.civ, b.origin, Blocked::NotEnoughUsdc.code());
                    break;
                }
                bq -= q;
                *sq -= q;
            }
        }
    }
}

/// Settle one fill. Every amount is computed and checked before anything
/// changes; returns false (and changes nothing) if a balance would go below
/// zero or overflow.
fn settle(
    state: &mut WorldState,
    rules: &Ruleset,
    buy: &ExOrder,
    sell: &ExOrder,
    qty: u32,
    price: u64,
    tariff_bps: u32,
) -> bool {
    let Some((notional, income)) = trade_cost(rules, qty, price, tariff_bps) else {
        return false;
    };
    // Fits: checked in trade_cost.
    let cost = notional + income;
    // Income from play is split like entry fees: 80% prize pool, 20% operations (V5 D11).
    let pool = (income as u128 * rules.vault_share_bps as u128 / BPS_ONE as u128) as u64;
    let (b, s) = (buy.civ as usize, sell.civ as usize);
    let (Some(b_usdc), Some(s_usdc), Some(vault), Some(ops)) = (
        state.civs[b].usdc.checked_sub(cost),
        state.civs[s].usdc.checked_add(notional),
        state.exchange_vault.checked_add(pool),
        state.exchange_ops.checked_add(income - pool),
    ) else {
        return false;
    };
    {
        let c = &mut state.civs[b];
        c.usdc = b_usdc;
        c.market_spent = c.market_spent.saturating_add(cost);
        let k = GoodKind::of(buy.good) as usize;
        c.exchange_bought[k] = c.exchange_bought[k].saturating_add(qty);
    }
    state.civs[s].usdc = s_usdc;
    state.exchange_vault = vault;
    state.exchange_ops = ops;
    // The seller's goods leave now; the buyer's arrive after the delivery delay.
    let q = qty as i64 * MILLI;
    match sell.good {
        Good::Gold => state.civs[s].gold -= q,
        Good::Iron => state.civs[s].iron -= q,
        Good::Horses => state.civs[s].horses -= q,
        Good::Food(from) => state.cities[from as usize].food -= q,
        Good::Production(from) => state.cities[from as usize].prod -= q,
    }
    let due = state.tick + rules.delivery_ticks;
    state.deliveries.push(Delivery {
        civ: buy.civ,
        good: buy.good,
        qty,
        due,
    });
    // buyer, seller, qty, price (u64), notional, income: nothing truncated.
    let mut payload = [0u8; 32];
    payload[..2].copy_from_slice(&buy.civ.to_le_bytes());
    payload[2..4].copy_from_slice(&sell.civ.to_le_bytes());
    payload[4..8].copy_from_slice(&qty.to_le_bytes());
    payload[8..16].copy_from_slice(&price.to_le_bytes());
    payload[16..24].copy_from_slice(&notional.to_le_bytes());
    payload[24..32].copy_from_slice(&income.to_le_bytes());
    state.push_event(b"exchange", &payload);
    true
}
