//! Audit WP08: market arithmetic and per-batch caps. Each batch passes the
//! on-chain RevealOrders gate (role_allows_static + check_structure + cost
//! <= spendable) and resolves through resolve_tick as on the ER. Run in
//! debug (overflow checks on) and with --release (what the SBF build does).

mod common;
use common::{staff, vrf};
use permutation_rules::fixed::MILLI;
use permutation_rules::genesis::{new_season, Entry};
use permutation_rules::gov::Role;
use permutation_rules::hex::Hex;
use permutation_rules::invariants;
use permutation_rules::orders::{
    check_structure, office_batches, role_allows_static, spendable, Good, Order, Side,
    StandingOrder, StandingTarget,
};
use permutation_rules::state::{Owner, WorldState};
use permutation_rules::tech::Tech;
use permutation_rules::tick::{resolve_tick, TickInput};
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
    }
    staff(&mut s);
    (rules, s)
}

fn gate(
    s: &WorldState,
    rules: &Ruleset,
    civ: u16,
    role: Role,
    orders: &[Order],
) -> Result<(), RulesError> {
    assert!(orders.iter().all(|o| role_allows_static(role, o)));
    let cost = check_structure(rules, s.tick, orders)?;
    assert!(cost <= spendable(s, rules, civ, role), "budget");
    Ok(())
}

fn tick(s: &mut WorldState, rules: &Ruleset, orders: Vec<(u16, Vec<Order>)>) {
    staff(s);
    let batches = orders
        .into_iter()
        .flat_map(|(civ, o)| office_batches(s, civ, [9; 32], o))
        .collect();
    resolve_tick(
        s,
        rules,
        &TickInput {
            vrf: vrf(s.tick),
            batches,
            ..Default::default()
        },
    )
    .unwrap();
}

fn iron(side: Side, amount: u32, limit_gold: u32) -> Order {
    Order::MarketTrade {
        good: Good::Iron,
        side,
        amount,
        limit_gold,
    }
}

#[test]
fn amm_sells_are_capped_by_the_stock_across_the_batch() {
    let (rules, mut s) = setup();
    s.civs[0].iron = 10 * MILLI;
    let orders: Vec<Order> = (0..6).map(|_| iron(Side::Sell, 10, 0)).collect();
    s.nations[0].role_bank[Role::Diplomat.index()] = 16;
    gate(&s, &rules, 0, Role::Diplomat, &orders).unwrap();
    let pool0 = s.pools[0];
    tick(&mut s, &rules, vec![(0, orders)]);
    assert_eq!(s.civs[0].iron, 0, "sold its 10 iron once");
    assert_eq!(s.pools[0].goods - pool0.goods, 10 * MILLI);
    assert!(invariants::check(&s, &rules)
        .iter()
        .all(|v| v.invariant != 5));
}

#[test]
fn amm_buys_are_capped_by_the_gold_across_the_batch_and_both_pools() {
    let (rules, mut s) = setup();
    for u in s.units.iter_mut() {
        if u.owner == Owner::Civ(0) && !u.unit_type.is_civilian() {
            u.alive = false;
        }
    }
    s.civs[0].gold = 60 * MILLI;
    let mut orders: Vec<Order> = (0..4).map(|_| iron(Side::Buy, 5, 100)).collect();
    orders.push(Order::MarketTrade {
        good: Good::Horses,
        side: Side::Buy,
        amount: 5,
        limit_gold: 100,
    });
    s.nations[0].role_bank[Role::Diplomat.index()] = 16;
    gate(&s, &rules, 0, Role::Diplomat, &orders).unwrap();
    // Phases 0-2 only (the markets): the gold before upkeep could forgive a debt.
    let mut after2 = s.clone();
    staff(&mut after2);
    let batches = office_batches(&after2, 0, [9; 32], orders.clone());
    let input = TickInput {
        vrf: vrf(after2.tick),
        batches,
        ..Default::default()
    };
    for phase in 0..3 {
        permutation_rules::tick::run_phase(&mut after2, &rules, &input, phase).unwrap();
    }
    assert!(
        after2.civs[0].gold >= 0,
        "gold after the markets: {}",
        after2.civs[0].gold
    );
    tick(&mut s, &rules, vec![(0, orders)]);
    assert!(
        s.civs[0].gold >= 0 && !s.civs[0].deficit,
        "no debt for upkeep to forgive"
    );
    assert!(s.civs[0].iron + s.civs[0].horses <= 10 * MILLI);
}

#[test]
fn oversized_buys_no_longer_freeze_the_pool() {
    let (rules, mut base) = setup();
    base.civs[1].gold = 500 * MILLI;
    base.civs[0].gold = 500 * MILLI;
    let honest = vec![(1u16, vec![iron(Side::Buy, 10, 200)])];
    let max = rules.max_trade_amount;
    let spam: Vec<Order> = (0..4).map(|_| iron(Side::Buy, max, 1)).collect();
    base.nations[0].role_bank[Role::Diplomat.index()] = 16;
    gate(&base, &rules, 0, Role::Diplomat, &spam).unwrap();
    let too_big = vec![iron(Side::Buy, max + 1, 1)];
    assert_eq!(
        gate(&base, &rules, 0, Role::Diplomat, &too_big),
        Err(RulesError::OutOfRange)
    );
    let mut s = base.clone();
    let mut orders = honest;
    orders.push((0, spam));
    tick(&mut s, &rules, orders);
    assert_eq!(s.civs[1].iron, 10 * MILLI, "the honest buy fills");
}

#[test]
fn transfer_cap_sum_does_not_wrap() {
    let (rules, mut s) = setup();
    s.civs[0].gold = 1_000 * MILLI;
    s.civs[0].last.gold = 10;
    s.nations[0].role_bank[Role::Diplomat.index()] = 16;
    let huge = vec![Order::Transfer {
        civ: 1,
        good: Good::Gold,
        amount: u32::MAX,
    }];
    assert_eq!(
        gate(&s, &rules, 0, Role::Diplomat, &huge),
        Err(RulesError::OutOfRange)
    );
    let orders = vec![
        Order::Transfer {
            civ: 1,
            good: Good::Gold,
            amount: 1,
        },
        Order::Transfer {
            civ: 1,
            good: Good::Gold,
            amount: rules.max_trade_amount,
        },
    ];
    gate(&s, &rules, 0, Role::Diplomat, &orders).unwrap();
    let g1 = s.civs[1].gold;
    tick(&mut s, &rules, vec![(0, orders)]);
    assert_eq!(
        s.civs[1].gold - g1 - s.civs[1].last.gold as i64 * MILLI,
        MILLI,
        "only the first transfer"
    );
}

#[test]
fn extreme_hexes_are_refused_at_the_gate() {
    let (rules, s) = setup();
    let unit = s
        .units
        .iter()
        .find(|u| u.owner == Owner::Civ(0) && !u.unit_type.is_civilian())
        .unwrap()
        .id;
    let far = Hex::new(i32::MAX, 0);
    let mv = vec![Order::MoveUnit {
        unit,
        path: vec![far],
    }];
    assert_eq!(
        check_structure(&rules, s.tick, &mv),
        Err(RulesError::OutOfRange)
    );
    let patrol = vec![Order::SetStanding {
        target: StandingTarget::Unit(unit),
        rule: StandingOrder::Patrol {
            route: vec![Hex::new(0, i32::MIN)],
        },
    }];
    assert_eq!(
        check_structure(&rules, s.tick, &patrol),
        Err(RulesError::OutOfRange)
    );
    let edge = vec![Order::MoveUnit {
        unit,
        path: vec![Hex::new(255, -255)],
    }];
    assert!(check_structure(&rules, s.tick, &edge).is_ok());
}

#[test]
fn exchange_price_and_amount_bounds() {
    let (rules, s) = setup();
    let ex = |amount, price| {
        vec![Order::ExchangeOrder {
            good: Good::Gold,
            side: Side::Buy,
            amount,
            price,
        }]
    };
    assert!(check_structure(
        &rules,
        s.tick,
        &ex(rules.max_trade_amount, rules.exchange_max_price)
    )
    .is_ok());
    assert_eq!(
        check_structure(&rules, s.tick, &ex(1, rules.exchange_max_price + 1)),
        Err(RulesError::OutOfRange)
    );
    assert_eq!(
        check_structure(&rules, s.tick, &ex(rules.max_trade_amount + 1, 1)),
        Err(RulesError::OutOfRange)
    );
}

#[test]
fn trade_cost_covers_every_fill_at_or_below_the_limit() {
    use permutation_rules::markets::trade_cost;
    let rules = Ruleset::new(Preset::Blitz);
    for tariff in [500u32, 976, 5_391, 10_000] {
        for limit in [1u64, 19, 20, 999, 1_000_003, rules.exchange_max_price] {
            for q in [1u32, 2, 7, 1_000, rules.max_trade_amount] {
                let (n, i) = trade_cost(&rules, q, limit, tariff).unwrap();
                for p in [1, limit / 2 + 1, limit.saturating_sub(1).max(1), limit] {
                    let (n2, i2) = trade_cost(&rules, q, p, tariff).unwrap();
                    assert!(
                        n2 + i2 <= n + i,
                        "fill at {p} costs more than reserved at {limit}"
                    );
                }
                // Never cheaper than the whole-trade rounding it replaces.
                let old =
                    n + n * rules.exchange_fee_bps as u64 / 10_000 + n * tariff as u64 / 10_000;
                assert!(n + i >= old);
            }
        }
    }
}

#[test]
fn returned_escrow_leaves_the_tariff_position() {
    use permutation_rules::contracts::ContractTerm;
    let (rules, mut s) = setup();
    s.civs[0].usdc = 200 * USDC;
    s.usdc_deposited += 180 * USDC;
    let city = s.civs[2].capital.unwrap();
    let t = s.tick;
    let orders = vec![
        Order::OfferContract {
            to: None,
            term: ContractTerm::Capture { city },
            usdc: 200 * USDC,
            deadline: t + 1,
        },
        Order::ConsentSpend { usdc: 200 * USDC },
    ];
    tick(&mut s, &rules, vec![(0, orders)]);
    assert_eq!(s.civs[0].usdc, 0, "escrowed");
    assert_eq!(
        rules.tariff_bps(s.civs[0].market_spent),
        10_000,
        "counted while escrowed"
    );
    for _ in 0..3 {
        tick(&mut s, &rules, vec![]);
    }
    assert_eq!(s.civs[0].usdc, 200 * USDC, "returned");
    assert_eq!(s.civs[0].market_spent, 0);
    assert_eq!(rules.tariff_bps(s.civs[0].market_spent), 500);
    assert!(invariants::check(&s, &rules)
        .iter()
        .all(|v| v.invariant != 1));
}
