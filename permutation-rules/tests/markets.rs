//! Markets (§11): gold AMM with hub fees, and the USDC Exchange.

use permutation_rules::fixed::MILLI;
use permutation_rules::genesis::{new_season, Entry};
use permutation_rules::invariants;
use permutation_rules::markets::GoodKind;
use permutation_rules::orders::{validate_batch, Good, Order, OrderBatch, Side};
use permutation_rules::rng::Seed;
use permutation_rules::state::{DeclaredKind, WorldState};
use permutation_rules::tech::Tech;
use permutation_rules::tick::{resolve_tick, TickInput};
use permutation_rules::{Preset, RulesError, Ruleset};

const USDC: u64 = 1_000_000;

fn setup() -> (Ruleset, WorldState) {
    let rules = Ruleset::new(Preset::Blitz);
    let entries: Vec<Entry> = (0..4)
        .map(|i| Entry {
            name: format!("civ-{i}"),
            declared_kind: DeclaredKind::Undeclared,
            payout_wallet: [i as u8; 32],
            exchange_deposit: 20 * USDC,
        })
        .collect();
    let mut s = new_season(&rules, &[11; 32], &[22; 32], &entries).unwrap();
    for c in &mut s.civs {
        c.techs.insert(Tech::BronzeWorking);
        c.techs.insert(Tech::Currency);
        c.gold = 500 * MILLI;
    }
    (rules, s)
}

fn vrf(tick: u16) -> Seed {
    let mut s = [0u8; 32];
    s[..2].copy_from_slice(&tick.to_le_bytes());
    s
}

fn step(s: &mut WorldState, rules: &Ruleset, orders: Vec<(u16, Vec<Order>)>) {
    let batches = orders
        .into_iter()
        .map(|(civ, orders)| OrderBatch {
            civ,
            tick: s.tick,
            decision_digest: [0; 32],
            orders,
        })
        .collect();
    resolve_tick(
        s,
        rules,
        &TickInput {
            vrf: vrf(s.tick),
            batches,
        },
    )
    .unwrap();
    let v = invariants::check(s, rules);
    assert!(v.is_empty(), "{v:?}");
}

/// Gold before phase-6 income: current gold minus last tick's income.
fn gold_ex_income(s: &WorldState, civ: u16) -> i64 {
    s.civs[civ as usize].gold - s.civs[civ as usize].last.gold as i64 * MILLI
}

// ------------------------------------------------------------------ AMM

#[test]
fn amm_buy_moves_the_pool_and_charges_the_fee() {
    let (rules, mut s) = setup();
    let g0 = s.civs[0].gold;
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![Order::MarketTrade {
                good: Good::Iron,
                side: Side::Buy,
                amount: 10,
                limit_gold: 200,
            }],
        )],
    );
    assert_eq!(s.civs[0].iron, 10 * MILLI);
    assert_eq!(s.pools[0].goods, 190 * MILLI);
    // Same numbers as the unit test on a fresh 200/2000 pool: 105.270 gold + 3% fee.
    let paid = g0 - gold_ex_income(&s, 0);
    assert_eq!(paid, 105_270 + 105_270 * 300 / 10_000);
}

#[test]
fn amm_requires_currency() {
    let (rules, mut s) = setup();
    s.civs[0].techs = Default::default();
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![Order::MarketTrade {
                good: Good::Iron,
                side: Side::Buy,
                amount: 5,
                limit_gold: 200,
            }],
        )],
    );
    assert_eq!(s.civs[0].iron, 0);
    assert_eq!(s.pools[0].goods, 200 * MILLI);
}

#[test]
fn amm_orders_in_one_tick_share_one_price_regardless_of_civ_order() {
    let (rules, mut s) = setup();
    let buy = |n| Order::MarketTrade {
        good: Good::Horses,
        side: Side::Buy,
        amount: n,
        limit_gold: 500,
    };
    let (g1, g3) = (s.civs[1].gold, s.civs[3].gold);
    step(&mut s, &rules, vec![(3, vec![buy(6)]), (1, vec![buy(6)])]);
    assert_eq!(g1 - gold_ex_income(&s, 1), g3 - gold_ex_income(&s, 3));
}

#[test]
fn amm_hub_holders_earn_one_percent() {
    let (rules, mut s) = setup();
    // Give civ 2 a hub: move its capital next to the hub and re-claim its
    // territory there, so the state stays consistent (invariant 11).
    let cap = s.civs[2].capital.unwrap();
    let hub = s.hubs[0];
    for t in &mut s.map.tiles {
        if t.owner_city == Some(cap) || t.hex == hub {
            t.owner_city = None;
        }
    }
    let site = hub
        .neighbors()
        .into_iter()
        .find(|h| s.map.tile(*h).is_some_and(|t| t.terrain.is_passable()) && !s.cities.iter().any(|c| c.hex == *h))
        .expect("land next to the hub");
    s.cities[cap as usize].hex = site;
    for u in s.units.iter_mut().filter(|u| u.owner == permutation_rules::state::Owner::Civ(2)) {
        u.hex = site;
    }
    s.map.tile_mut(site).unwrap().owner_city = Some(cap);
    s.map.claim_territory(cap, site, 1);
    assert_eq!(s.map.tile(hub).unwrap().owner_city, Some(cap));
    let g2 = s.civs[2].gold;
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![Order::MarketTrade {
                good: Good::Iron,
                side: Side::Buy,
                amount: 10,
                limit_gold: 200,
            }],
        )],
    );
    let notional = 105_270i64;
    let hub_fee = notional * 100 / 10_000 / s.hubs.len() as i64;
    assert_eq!(gold_ex_income(&s, 2) - g2, hub_fee);
}

// ------------------------------------------------------------------ Exchange

fn sell_iron(qty: u32, price: u64) -> Order {
    Order::ExchangeOrder {
        good: Good::Iron,
        side: Side::Sell,
        amount: qty,
        price,
    }
}
fn buy_iron(qty: u32, price: u64) -> Order {
    Order::ExchangeOrder {
        good: Good::Iron,
        side: Side::Buy,
        amount: qty,
        price,
    }
}

#[test]
fn exchange_matches_at_one_price_and_splits_the_fee() {
    let (rules, mut s) = setup();
    s.civs[1].iron = 10 * MILLI;
    step(&mut s, &rules, vec![(1, vec![sell_iron(3, USDC)])]); // establishes last-tick income
    assert_eq!(s.civs[1].iron, 10 * MILLI, "no buyer, no trade");
    step(
        &mut s,
        &rules,
        vec![
            (0, vec![buy_iron(1, 2 * USDC)]),
            (1, vec![sell_iron(3, USDC)]),
        ],
    );
    // Buyer cap for iron is max(1, income) = 1. At 1 and 2 USDC the volume (1)
    // and imbalance (2) tie, so the auction clears at the lower price.
    let price = s.civs[1].usdc - 20 * USDC;
    assert_eq!(price, USDC);
    let fee = price * 500 / 10_000;
    assert_eq!(s.civs[0].usdc, 20 * USDC - price - fee);
    assert_eq!(s.exchange_vault, fee * 8_000 / 10_000);
    assert_eq!(s.exchange_ops, fee - fee * 8_000 / 10_000);
    assert_eq!(s.civs[0].iron, MILLI);
    assert_eq!(s.civs[1].iron, 9 * MILLI);
    assert_eq!(s.civs[0].exchange_bought[GoodKind::Iron as usize], 1);
}

#[test]
fn exchange_buys_are_capped_by_income_and_the_season_cap() {
    let (rules, mut s) = setup();
    s.civs[1].gold = 5_000 * MILLI;
    step(&mut s, &rules, vec![]);
    let cap = s.civs[0].last.gold;
    let sell = Order::ExchangeOrder {
        good: Good::Gold,
        side: Side::Sell,
        amount: 1_000,
        price: 10_000,
    };
    let buy = Order::ExchangeOrder {
        good: Good::Gold,
        side: Side::Buy,
        amount: 1_000,
        price: 10_000,
    };
    let g0 = s.civs[0].gold;
    step(&mut s, &rules, vec![(0, vec![buy]), (1, vec![sell])]);
    let bought = (gold_ex_income(&s, 0) - g0) / MILLI;
    assert_eq!(bought as u32, cap, "per-tick cap = last tick's gold income");

    // Season cap: once spend reaches 1 × entry fee, further buys are refused.
    s.civs[0].exchange_spent = rules.entry_fee_usdc;
    let usdc = s.civs[0].usdc;
    step(
        &mut s,
        &rules,
        vec![
            (
                0,
                vec![Order::ExchangeOrder {
                    good: Good::Gold,
                    side: Side::Buy,
                    amount: 5,
                    price: 10_000,
                }],
            ),
            (
                1,
                vec![Order::ExchangeOrder {
                    good: Good::Gold,
                    side: Side::Sell,
                    amount: 5,
                    price: 10_000,
                }],
            ),
        ],
    );
    assert_eq!(s.civs[0].usdc, usdc);
}

#[test]
fn exchange_goods_cannot_be_resold_the_same_season() {
    let (rules, mut s) = setup();
    s.civs[1].iron = 10 * MILLI;
    step(&mut s, &rules, vec![]);
    step(
        &mut s,
        &rules,
        vec![(0, vec![buy_iron(1, USDC)]), (1, vec![sell_iron(1, USDC)])],
    );
    assert_eq!(s.civs[0].iron, MILLI);
    // Civ 0 now tries to sell that iron to civ 2: nothing to sell.
    step(
        &mut s,
        &rules,
        vec![(0, vec![sell_iron(1, USDC)]), (2, vec![buy_iron(1, USDC)])],
    );
    assert_eq!(s.civs[0].iron, MILLI);
    assert_eq!(s.civs[2].iron, 0);
}

#[test]
fn exchange_delivers_food_to_the_named_city() {
    let (rules, mut s) = setup();
    let seller_city = s.civs[1].capital.unwrap();
    let buyer_city = s.civs[0].capital.unwrap();
    s.cities[seller_city as usize].food = 20 * MILLI;
    step(&mut s, &rules, vec![]);
    let cap = s.civs[0].last.max_city_food_surplus;
    assert!(cap >= 1);
    // Control: the same tick without the trade, to separate growth from the transfer.
    let mut control = s.clone();
    step(&mut control, &rules, vec![]);
    step(
        &mut s,
        &rules,
        vec![
            (
                0,
                vec![Order::ExchangeOrder {
                    good: Good::Food(buyer_city),
                    side: Side::Buy,
                    amount: 1,
                    price: USDC,
                }],
            ),
            (
                1,
                vec![Order::ExchangeOrder {
                    good: Good::Food(seller_city),
                    side: Side::Sell,
                    amount: 1,
                    price: USDC,
                }],
            ),
        ],
    );
    assert_eq!(s.civs[1].usdc, 21 * USDC);
    assert_eq!(s.civs[0].exchange_bought[GoodKind::Food as usize], 1);
    let (b, cb) = (
        &s.cities[buyer_city as usize],
        &control.cities[buyer_city as usize],
    );
    let (se, cs) = (
        &s.cities[seller_city as usize],
        &control.cities[seller_city as usize],
    );
    assert_eq!(
        (b.pop, se.pop),
        (cb.pop, cs.pop),
        "no growth threshold crossed by the trade"
    );
    assert_eq!(b.food - cb.food, MILLI, "buyer's city received 1 food");
    assert_eq!(cs.food - se.food, MILLI, "seller's city gave 1 food");
}

#[test]
fn exchange_is_frozen_from_tick_120() {
    let (rules, mut s) = setup();
    s.tick = rules.exchange_freeze_tick;
    let b = OrderBatch {
        civ: 0,
        tick: s.tick,
        decision_digest: [0; 32],
        orders: vec![buy_iron(1, USDC)],
    };
    assert_eq!(validate_batch(&s, &rules, &b), Err(RulesError::Frozen));
}
