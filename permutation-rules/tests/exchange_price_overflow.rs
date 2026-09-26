//! Audit repro: `ExchangeOrder.price` is an unbounded u64 and the Exchange
//! arithmetic (`unit_cost`, `settle`) is plain u64. The chain program is a
//! release build (no `overflow-checks`), so run this with `--release` to see
//! what the program computes; a debug build panics on the first overflow.
//!
//!   cargo test --release --test exchange_price_overflow -- --nocapture

mod common;
use common::step;
use permutation_rules::fixed::MILLI;
use permutation_rules::genesis::{new_season, Entry};
use permutation_rules::orders::{check_structure, Good, Order, Side};
use permutation_rules::tech::Tech;
use permutation_rules::{Preset, Ruleset};

const USDC: u64 = 1_000_000;
/// With a 5% fee and the first 5% tariff, `unit_cost` = price + 2 * (price *
/// 500 mod 2^64) / 10^4 wraps to 999 base units (0.000999 USDC).
const HUGE: u64 = 18_443_390_120_241_605_333;

#[test]
fn a_huge_exchange_price_wraps_the_seller_and_inflates_the_pool() {
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
    step(&mut s, &rules, vec![]); // last tick's gold income sets the buy cap

    let order = |side| Order::ExchangeOrder {
        good: Good::Gold,
        side,
        amount: 1,
        price: HUGE,
    };
    // What RevealOrders checks on chain (play.rs): it refuses this price
    // (audit WP08), and phase 0's validate_batch runs the same check.
    assert_eq!(
        check_structure(&rules, s.tick, &[order(Side::Buy)]),
        Err(permutation_rules::RulesError::OutOfRange)
    );

    let deposited = s.usdc_deposited;
    let (a0, b0) = (s.civs[0].usdc, s.civs[1].usdc);
    // Civ 0's diplomat buys 1 gold, civ 1's diplomat sells it, both at HUGE.
    // `step` also runs invariants::check (USDC conservation): it passes.
    step(
        &mut s,
        &rules,
        vec![(0, vec![order(Side::Buy)]), (1, vec![order(Side::Sell)])],
    );
    println!("usdc_deposited      = {deposited}");
    println!("buyer  A paid       = {}", a0.wrapping_sub(s.civs[0].usdc));
    println!("seller B credited   = {}", s.civs[1].usdc.wrapping_sub(b0));
    println!("seller B usdc       = {}", s.civs[1].usdc);
    println!("exchange_vault      = {}", s.exchange_vault);
    println!("exchange_ops        = {}", s.exchange_ops);

    // Nobody's balance, and neither in-play income bucket, can exceed the
    // USDC actually deposited on the ER.
    assert!(
        s.civs[1].usdc <= deposited,
        "seller treasury {} > all deposits {deposited}",
        s.civs[1].usdc
    );
    assert!(
        s.exchange_vault <= deposited,
        "exchange_vault {} > all deposits {deposited}",
        s.exchange_vault
    );
    assert!(
        s.exchange_ops <= deposited,
        "exchange_ops {} > all deposits {deposited}",
        s.exchange_ops
    );
}
