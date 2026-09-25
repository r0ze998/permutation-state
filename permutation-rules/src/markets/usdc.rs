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

/// Price per unit a buyer pays at its limit: price + fee + tariff (V5 §7.5).
fn unit_cost(rules: &Ruleset, price: u64, tariff_bps: u32) -> u64 {
    price + fee_of(rules, price) + price * tariff_bps as u64 / BPS_ONE as u64
}

/// Most the diplomat may spend from the treasury this tick: the threshold,
/// or more with another officer's `ConsentSpend` (V5 §7.5).
pub fn spend_limit(state: &WorldState, rules: &Ruleset, civ: CivId) -> u64 {
    let diplomat = state
        .nations
        .get(civ as usize)
        .map_or(NOBODY, |n| n.holder(Role::Diplomat));
    let consent = state
        .tick_orders
        .iter()
        .filter(|a| a.civ == civ)
        .flat_map(|a| a.iter())
        .filter(|(_, credit, origin)| {
            origin.0 != Role::Diplomat as u8 && (credit.officer != diplomat || diplomat == NOBODY)
        })
        .filter_map(|(o, _, _)| match o {
            Order::ConsentSpend { usdc } => Some(*usdc),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    rules.spend_consent_usdc.max(consent)
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
        if amount == 0 || price == 0 {
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
                let unit = unit_cost(rules, price, tariff[civ as usize]);
                let room = limit[civ as usize].saturating_sub(reserved[civ as usize]);
                let affordable = (room / unit.max(1)).min(u32::MAX as u64) as u32;
                let q = amount.min(cap).min(affordable);
                if q == 0 {
                    let why =
                        short_of_usdc(limit[civ as usize] < state.civs[civ as usize].free_usdc());
                    state.skip(civ, origin, why.code());
                }
                reserved[civ as usize] += q as u64 * unit;
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
                settle(state, rules, b, sl, q, price, tariff[b.civ as usize]);
                bq -= q;
                *sq -= q;
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
    tariff_bps: u32,
) {
    let notional = qty as u64 * price;
    let fee = fee_of(rules, notional);
    let tariff = notional * tariff_bps as u64 / BPS_ONE as u64;
    let income = fee + tariff;
    // Income from play is split like entry fees: 80% prize pool, 20% operations (V5 D11).
    let pool = income * rules.vault_share_bps as u64 / BPS_ONE as u64;
    {
        let b = &mut state.civs[buy.civ as usize];
        b.usdc -= notional + income;
        b.market_spent += notional + income;
        b.exchange_bought[GoodKind::of(buy.good) as usize] += qty;
    }
    state.civs[sell.civ as usize].usdc += notional;
    state.exchange_vault += pool;
    state.exchange_ops += income - pool;
    // The seller's goods leave now; the buyer's arrive after the delivery delay.
    let q = qty as i64 * MILLI;
    match sell.good {
        Good::Gold => state.civs[sell.civ as usize].gold -= q,
        Good::Iron => state.civs[sell.civ as usize].iron -= q,
        Good::Horses => state.civs[sell.civ as usize].horses -= q,
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
    let mut payload = [0u8; 12];
    payload[..2].copy_from_slice(&buy.civ.to_le_bytes());
    payload[2..4].copy_from_slice(&sell.civ.to_le_bytes());
    payload[4..8].copy_from_slice(&qty.to_le_bytes());
    payload[8..].copy_from_slice(&(price as u32).to_le_bytes());
    state.push_event(b"exchange", &payload);
}
