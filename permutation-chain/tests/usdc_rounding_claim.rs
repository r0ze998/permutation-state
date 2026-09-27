//! Audit repro, fixed: the Exchange rounding underflow (permutation-rules
//! markets/usdc.rs) and its impact on the money the program pays out.
//! Rules v9 round the fee and tariff up per unit and check every transfer
//! (WP08), and the invariant check sums in u128 (WP12), so the tick below
//! neither wraps A's treasury nor mints USDC for B: FinishSeason is not
//! voided and owes exactly what the vault holds.
//!
//! The attack as it was:
//! Nation A (deposit 38 base units) buys 2 gold at 19 from nation B:
//! reserved 2*19 = 38, charged 38 + floor(1.9) + floor(1.9) = 40, so A's
//! treasury wraps to ~2^64 (release, no overflow-checks, as `cargo
//! build-sbf` builds this crate). Next tick A "buys" gold from B at a price
//! the attacker picks, with a second A officer's ConsentSpend, which credits
//! B unbacked USDC. `finalize` copies it into `treasury_final`, and
//! `claim_amount` pays B's single depositor from the shared vault.
//!

#[path = "../../permutation-rules/tests/common/mod.rs"]
mod common;
use common::step;
use permutation_chain::finalize::{self, finalize, Roster};
use permutation_chain::payout::claim_amount;
use permutation_chain::state::*;
use permutation_rules::fixed::MILLI;
use permutation_rules::genesis::{new_season, Entry};
use permutation_rules::orders::{Good, Order, Side};
use permutation_rules::tech::Tech;
use permutation_rules::{Preset, Ruleset};

const USDC: u64 = 1_000_000;
const FEE: u64 = 10 * USDC;

fn gold(side: Side, amount: u32, price: u64) -> Order {
    Order::ExchangeOrder {
        good: Good::Gold,
        side,
        amount,
        price,
    }
}

fn member(index: u32, civ: u16, shares: u64) -> MemberAccount {
    MemberAccount {
        magic: MEMBER_MAGIC,
        season_id: 1,
        bump: 0,
        index,
        civ,
        wallet: [0; 32],
        session: [0; 32],
        kind: 0,
        name: String::new(),
        attestation: [0; 32],
        stand: 0,
        votes: [0; 4],
        shares,
        claimed: false,
        tag: [0; 32],
    }
}

#[test]
fn claims_never_exceed_the_vault_after_a_treasury_bound_exchange_buy() {
    let rules = Ruleset::new(Preset::Blitz);
    // A (attacker, 38 base units), B (attacker, 1 USDC), C (honest, 20 USDC).
    let deposits = [38u64, USDC, 20 * USDC];
    let entries: Vec<Entry> = deposits
        .iter()
        .enumerate()
        .map(|(i, t)| Entry {
            name: format!("civ-{i}"),
            treasury: *t,
        })
        .collect();
    let mut s = new_season(&rules, &[11; 32], &[22; 32], &entries).unwrap();
    for c in &mut s.civs {
        c.techs.insert(Tech::BronzeWorking);
        c.techs.insert(Tech::Currency);
        c.gold = 500 * MILLI;
    }
    step(&mut s, &rules, vec![]); // seats 2 officers per nation; sets the buy cap

    // Tick 2: the rounding underflow.
    step(
        &mut s,
        &rules,
        vec![
            (0, vec![gold(Side::Buy, 2, 19)]),
            (1, vec![gold(Side::Sell, 2, 19)]),
        ],
    );
    println!("A.usdc after buying 2 gold @19 with 38: {}", s.civs[0].usdc);

    // The vault holds every entry fee and every deposit (no AI members).
    let members = s.members.len() as u64;
    let vault = members * FEE + deposits.iter().sum::<u64>();

    // Tick 3: A buys from B at a chosen price; a second A officer consents.
    let cap = s.civs[0].last.gold;
    let price = vault / 2 / cap as u64;
    let b_before = s.civs[1].usdc;
    step(
        &mut s,
        &rules,
        vec![
            (
                0,
                vec![
                    gold(Side::Buy, cap, price),
                    Order::ConsentSpend { usdc: u64::MAX },
                ],
            ),
            (1, vec![gold(Side::Sell, cap, price)]),
        ],
    );
    println!(
        "B.usdc {} -> {} (B deposited {}), A.usdc {}",
        b_before, s.civs[1].usdc, USDC, s.civs[0].usdc
    );

    // FinishSeason: the same updates `finish_season` makes.
    s.tick = rules.ticks_per_season;
    let ops = FEE * rules.ops_share_bps as u64 / 10_000;
    let mut season = Season {
        magic: SEASON_MAGIC,
        season_id: 1,
        bump: 0,
        vault_bump: 0,
        admin: [0; 32],
        crank: [0; 32],
        usdc_mint: [0; 32],
        usdc_decimals: 6,
        preset: 0,
        nations: 3,
        entry_fee: FEE,
        tick_seconds: 30,
        market: true,
        status: SeasonStatus::Running,
        world_seed: [11; 32],
        season_seed: [22; 32],
        member_count: members as u32,
        nation_members: vec![2; 3],
        seated: members as u32,
        pool: members * (FEE - ops),
        ops: members * ops,
        ops_withdrawn: false,
        treasury: deposits.to_vec(),
        treasury_final: Vec::new(),
        payouts: Vec::new(),
        final_root: [0; 32],
        prev_season_id: 0,
        prev_history_root: [0; 32],
        history_root: [0; 32],
        ai_count: 0,
        roster_commit: [0; 32],
        bounty_each: 0,
        bond: 0,
        roster_acc: [0; 32],
        roster_revealed: 0,
        roster_outcome: 0,
        bounty_paid: Vec::new(),
        delegated: 0,
        roster_blind: [0; 32],
        refund_base: Vec::new(),
        refund_in_payout: Vec::new(),
        seed_state: 0,
        seed_oracle: [0; 32],
        seed_requested_at: 0,
        seed_requests: 0,
        deposit: 0,
        outstanding: 0,
        voided: false,
        start_by: 0,
        stage_at: 0,
        rolled_back: 0,
        aborted_from: 0,
        validator: [0; 32],
        rules_version: 0,
        rules_hash: [0; 32],
        logic_version: 0,
        created_slot: 0,
    };
    let f = finalize(&season, &s, &rules, Roster::None, false);
    assert!(!f.voided, "no wrapped treasury: the world conserves USDC");
    assert_eq!(f.owed as u128, finalize::paid_in(&season));
    season.pool = f.pool;
    season.ops = f.ops;
    season.payouts = f.payouts.clone();
    season.treasury_final = f.treasury_final.clone();
    season.refund_base = f.refund_base.clone();
    season.refund_in_payout = f.refund_in_payout.clone();
    season.outstanding = f.owed;
    season.status = SeasonStatus::Finalized;

    // One depositor per nation holds all of that nation's shares.
    let first_of = |civ: u16| s.members.iter().position(|m| m.civ == civ).unwrap() as u32;
    let b_claim = claim_amount(&season, &member(first_of(1), 1, USDC));
    let c_claim = claim_amount(&season, &member(first_of(2), 2, 20 * USDC));
    let owed: u128 = season.payouts.iter().map(|x| *x as u128).sum::<u128>()
        + season
            .treasury_final
            .iter()
            .map(|x| *x as u128)
            .sum::<u128>()
        + season.ops as u128;
    println!("vault {vault}; treasury_final {:?}", season.treasury_final);
    println!(
        "B's depositor claims {b_claim} ({:.1}% of the vault; B's members paid in {}); \
         vault left after B: {}; C's depositor is owed {c_claim}",
        b_claim as f64 * 100.0 / vault as f64,
        2 * FEE + USDC,
        vault.saturating_sub(b_claim)
    );
    // Even without A's own wrapped refund (the attacker never claims it),
    // what the program owes everyone else exceeds the vault.
    let owed_but_a = owed - season.treasury_final[0] as u128;
    println!("total owed {owed}; owed excluding A's refund {owed_but_a}; vault {vault}");
    assert_eq!(owed, vault as u128, "owed == vault");
    assert!(
        owed_but_a <= vault as u128,
        "the program owes {owed_but_a} (A's refund aside) out of a vault of {vault}"
    );
}
