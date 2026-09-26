//! Exchange rounding (markets/usdc.rs): the reservation rounds the fee and
//! the tariff down PER UNIT (`unit_cost`), settlement rounds them down once
//! over the WHOLE trade (`settle`). floor(q*x) >= q*floor(x), so a buy that
//! the treasury limits can cost more than was reserved, and
//! `b.usdc -= notional + income` (usdc.rs:434) goes below zero.
//!
//! Debug build: panics "attempt to subtract with overflow" at usdc.rs:434.
//! Release build (what `cargo build-sbf` ships, no overflow-checks): the
//! treasury wraps to ~2^64 and `invariants::check` (run by `step`) still
//! passes, because its u64 sum wraps too.

mod common;
use common::step;
use permutation_rules::fixed::MILLI;
use permutation_rules::genesis::{new_season, Entry};
use permutation_rules::orders::{Good, Order, Side};
use permutation_rules::tech::Tech;
use permutation_rules::{Preset, Ruleset};

fn gold(side: Side, amount: u32, price: u64) -> Order {
    Order::ExchangeOrder {
        good: Good::Gold,
        side,
        amount,
        price,
    }
}

#[test]
fn a_treasury_bound_buy_never_costs_more_than_the_treasury() {
    let rules = Ruleset::new(Preset::Blitz);
    // Nation 0's treasury is exactly 38 base units; nation 1 has 1 USDC.
    let entries = [
        Entry {
            name: "a".into(),
            treasury: 38,
        },
        Entry {
            name: "b".into(),
            treasury: 1_000_000,
        },
    ];
    let mut s = new_season(&rules, &[11; 32], &[22; 32], &entries).unwrap();
    for c in &mut s.civs {
        c.techs.insert(Tech::BronzeWorking);
        c.techs.insert(Tech::Currency);
        c.gold = 500 * MILLI;
    }
    step(&mut s, &rules, vec![]); // last tick's gold income sets the buy cap
    assert!(s.civs[0].last.gold >= 2, "buy cap must allow 2 gold");
    let before = s.civs[0].usdc;
    assert_eq!(before, 38);

    // Reservation: unit_cost(19) = 19 + floor(0.95) + floor(0.95) = 19, so
    // room 38 affords q = 2. Settlement: 38 + floor(1.9) + floor(1.9) = 40.
    step(
        &mut s,
        &rules,
        vec![
            (0, vec![gold(Side::Buy, 2, 19)]),
            (1, vec![gold(Side::Sell, 2, 19)]),
        ],
    );
    let bought: u32 = s
        .deliveries
        .iter()
        .filter(|d| d.civ == 0)
        .map(|d| d.qty)
        .sum();
    println!(
        "bought={bought} usdc before={before} after={} market_spent={} vault={} ops={}",
        s.civs[0].usdc, s.civs[0].market_spent, s.exchange_vault, s.exchange_ops
    );
    assert!(
        s.civs[0].usdc <= before,
        "buyer's treasury grew from {before} to {} (u64 underflow at usdc.rs:434)",
        s.civs[0].usdc
    );
}
