//! Markets (§11): gold AMM with hub fees, and the USDC Exchange.

mod common;
use common::step;
use permutation_rules::fixed::MILLI;
use permutation_rules::genesis::{new_season, Entry};
use permutation_rules::markets::GoodKind;
use permutation_rules::orders::{validate_batch, Good, Order, OrderBatch, Side};
use permutation_rules::state::WorldState;
use permutation_rules::tech::Tech;
use permutation_rules::{Preset, RulesError, Ruleset};

const USDC: u64 = 1_000_000;

fn setup() -> (Ruleset, WorldState) {
    let rules = Ruleset::new(Preset::Blitz);
    let entries: Vec<Entry> = (0..4)
        .map(|i| Entry {
            name: format!("civ-{i}"),
            treasury: 20 * USDC,
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
        .find(|h| {
            s.map.tile(*h).is_some_and(|t| t.terrain.is_passable())
                && !s.cities.iter().any(|c| c.hex == *h)
        })
        .expect("land next to the hub");
    s.cities[cap as usize].hex = site;
    for u in s
        .units
        .iter_mut()
        .filter(|u| u.owner == permutation_rules::state::Owner::Civ(2))
    {
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

/// Buyer's cost of `qty` at `price` for a nation that has spent nothing yet:
/// notional + 5% fee + 5% tariff (V5 §7.5).
fn first_cost(rules: &Ruleset, qty: u64, price: u64) -> (u64, u64) {
    let notional = qty * price;
    let income = notional * rules.exchange_fee_bps as u64 / 10_000
        + notional * rules.tariff_bps(0) as u64 / 10_000;
    (notional, income)
}

#[test]
fn exchange_matches_at_one_price_and_splits_the_income() {
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
    let (notional, income) = first_cost(&rules, 1, price);
    assert_eq!(s.civs[0].usdc, 20 * USDC - notional - income);
    assert_eq!(s.civs[0].market_spent, notional + income);
    // Fee and tariff are income: 80% prize pool, 20% operations (V5 D11).
    assert_eq!(s.exchange_vault, income * 8_000 / 10_000);
    assert_eq!(s.exchange_ops, income - income * 8_000 / 10_000);
    // The seller's iron left now; the buyer's arrives after the delivery delay.
    assert_eq!(s.civs[1].iron, 9 * MILLI);
    assert_eq!(s.civs[0].iron, 0);
    assert_eq!(s.deliveries.len(), 1);
    for _ in 0..rules.delivery_ticks - 1 {
        step(&mut s, &rules, vec![]);
        assert_eq!(s.civs[0].iron, 0, "still in transit");
    }
    step(&mut s, &rules, vec![]);
    assert_eq!(s.civs[0].iron, MILLI);
    assert!(s.deliveries.is_empty());
    assert_eq!(s.civs[0].exchange_bought[GoodKind::Iron as usize], 1);
}

#[test]
fn exchange_buys_are_capped_by_income_and_the_tariff_rises() {
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
    step(&mut s, &rules, vec![(0, vec![buy]), (1, vec![sell])]);
    let bought: u32 = s
        .deliveries
        .iter()
        .filter(|d| d.civ == 0)
        .map(|d| d.qty)
        .sum();
    assert_eq!(bought, cap, "per-tick cap = last tick's gold income");

    // The tariff rises with cumulative spend: at 100 USDC it is 100%, so
    // the buyer pays the price twice plus the fee (V5 §7.5).
    s.civs[0].market_spent = rules.tariff_full_usdc;
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
                    amount: 1,
                    price: 10_000,
                }],
            ),
            (
                1,
                vec![Order::ExchangeOrder {
                    good: Good::Gold,
                    side: Side::Sell,
                    amount: 1,
                    price: 10_000,
                }],
            ),
        ],
    );
    let notional = 10_000;
    let fee = notional * rules.exchange_fee_bps as u64 / 10_000;
    assert_eq!(usdc - s.civs[0].usdc, notional + fee + notional);
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
    for _ in 0..rules.delivery_ticks {
        step(&mut s, &rules, vec![]);
    }
    assert_eq!(s.civs[0].iron, MILLI);
    // Civ 0 now tries to sell that iron to civ 2: nothing to sell.
    step(
        &mut s,
        &rules,
        vec![(0, vec![sell_iron(1, USDC)]), (2, vec![buy_iron(1, USDC)])],
    );
    assert_eq!(s.civs[0].iron, MILLI);
    assert!(s.deliveries.is_empty());
}

#[test]
fn exchange_delivers_food_to_the_named_city() {
    let (rules, mut s) = setup();
    let seller_city = s.civs[1].capital.unwrap();
    let buyer_city = s.civs[0].capital.unwrap();
    s.cities[seller_city as usize].food = 20 * MILLI;
    step(&mut s, &rules, vec![]);
    assert!(s.civs[0].last.max_city_food_surplus >= 1);
    // Control: the same ticks without the trade, to separate growth from the delivery.
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
    let se = &s.cities[seller_city as usize];
    let cs = &control.cities[seller_city as usize];
    assert_eq!(se.pop, cs.pop);
    assert_eq!(cs.food - se.food, MILLI, "seller's city gave 1 food");
    for _ in 0..rules.delivery_ticks {
        step(&mut s, &rules, vec![]);
        step(&mut control, &rules, vec![]);
    }
    let (b, cb) = (
        &s.cities[buyer_city as usize],
        &control.cities[buyer_city as usize],
    );
    assert_eq!(b.pop, cb.pop, "no growth threshold crossed by the delivery");
    assert_eq!(b.food - cb.food, MILLI, "buyer's city received 1 food");
}

#[test]
fn no_trades_with_oneself_or_an_enemy() {
    let (rules, mut s) = setup();
    s.civs[0].iron = 10 * MILLI;
    s.civs[1].iron = 10 * MILLI;
    step(&mut s, &rules, vec![]);
    // Civ 0 on both sides: no self-match (v0.2 C8).
    step(
        &mut s,
        &rules,
        vec![(0, vec![buy_iron(1, USDC), sell_iron(1, USDC)])],
    );
    assert!(s.deliveries.is_empty());
    assert_eq!(s.civs[0].usdc, 20 * USDC);
    // At war: no match either.
    step(
        &mut s,
        &rules,
        vec![(0, vec![Order::DeclareWar { civ: 1 }])],
    );
    step(
        &mut s,
        &rules,
        vec![(0, vec![buy_iron(1, USDC)]), (1, vec![sell_iron(1, USDC)])],
    );
    assert!(s.at_war(0, 1));
    assert!(s.deliveries.is_empty());
    assert_eq!(s.civs[1].iron, 10 * MILLI);
}

#[test]
fn bought_production_never_reaches_a_star_gate_city() {
    use permutation_rules::buildings::Building;
    use permutation_rules::state::QueueItem;
    let (rules, mut s) = setup();
    let buyer_city = s.civs[0].capital.unwrap();
    let seller_city = s.civs[1].capital.unwrap();
    s.cities[seller_city as usize].prod = 5 * MILLI;
    s.cities[seller_city as usize].queue = vec![QueueItem::Building(Building::Granary)];
    s.cities[buyer_city as usize].queue = vec![QueueItem::Building(Building::StarGate1)];
    step(&mut s, &rules, vec![]);
    let skipped_before = s.last_skipped.len();
    step(
        &mut s,
        &rules,
        vec![
            (
                0,
                vec![Order::ExchangeOrder {
                    good: Good::Production(buyer_city),
                    side: Side::Buy,
                    amount: 1,
                    price: USDC,
                }],
            ),
            (
                1,
                vec![Order::ExchangeOrder {
                    good: Good::Production(seller_city),
                    side: Side::Sell,
                    amount: 1,
                    price: USDC,
                }],
            ),
        ],
    );
    assert!(s.deliveries.is_empty());
    assert!(s.last_skipped.len() > skipped_before || !s.last_skipped.is_empty());
    assert_eq!(s.civs[0].usdc, 20 * USDC);
}

#[test]
fn spending_above_the_threshold_needs_a_second_officer() {
    let (rules, mut s) = setup();
    s.civs[1].gold = 5_000 * MILLI;
    s.civs[0].usdc = 100 * USDC;
    s.usdc_deposited += 80 * USDC;
    step(&mut s, &rules, vec![]);
    let cap = s.civs[0].last.gold as u64;
    // A capital earns at least 2 gold a tick, so cap × 3 USDC exceeds the
    // 5 USDC threshold on any map.
    let price = 3 * USDC;
    assert!(
        cap >= 2 && cap * price > rules.spend_consent_usdc,
        "cap {cap}"
    );
    let trade = |consent: Option<u64>| {
        let mut orders = vec![Order::ExchangeOrder {
            good: Good::Gold,
            side: Side::Buy,
            amount: 1_000,
            price,
        }];
        if let Some(usdc) = consent {
            orders.push(Order::ConsentSpend { usdc });
        }
        vec![
            (0, orders),
            (
                1,
                vec![Order::ExchangeOrder {
                    good: Good::Gold,
                    side: Side::Sell,
                    amount: 1_000,
                    price,
                }],
            ),
        ]
    };
    let mut alone = s.clone();
    step(&mut alone, &rules, trade(None));
    let spent = 100 * USDC - alone.civs[0].usdc;
    assert!(
        spent <= rules.spend_consent_usdc,
        "the diplomat alone stays under the threshold"
    );
    let mut agreed = s.clone();
    step(&mut agreed, &rules, trade(Some(50 * USDC)));
    let got: u32 = agreed
        .deliveries
        .iter()
        .filter(|d| d.civ == 0)
        .map(|d| d.qty)
        .sum();
    assert_eq!(got as u64, cap, "with consent the income cap binds instead");
}

#[test]
fn exchange_is_frozen_from_tick_120() {
    let (rules, mut s) = setup();
    s.tick = rules.exchange_freeze_tick;
    let b = OrderBatch {
        civ: 0,
        tick: s.tick,
        role: permutation_rules::gov::Role::Diplomat,
        member: permutation_rules::gov::NOBODY,
        adopt: vec![],
        decision_digest: [0; 32],
        orders: vec![buy_iron(1, USDC)],
    };
    assert_eq!(validate_batch(&s, &rules, &b), Err(RulesError::Frozen));
}
