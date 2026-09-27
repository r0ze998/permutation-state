//! Audit repro (chain side), fixed: one 1-unit Exchange trade at a huge
//! `price` used to pass RevealOrders' `check_structure` and make
//! FinishSeason's `finalize` owe far more USDC than the vault holds. Rules
//! v9 bound the price (WP08: `OutOfRange`) and the engine refuses the trade
//! even if it reached it; FinishSeason is not voided, owes exactly what was
//! paid in (WP12), and no claim exceeds the vault.

use permutation_chain::finalize::{self, finalize, Roster};
use permutation_chain::payout::claim_amount;
use permutation_chain::state::{MemberAccount, Season, SeasonStatus, MEMBER_MAGIC, SEASON_MAGIC};
use permutation_rules::fixed::MILLI;
use permutation_rules::genesis::{new_season, Entry};
use permutation_rules::gov::{join, Path, Role};
use permutation_rules::orders::{check_structure, office_batches, Good, Order, OrderBatch, Side};
use permutation_rules::tech::Tech;
use permutation_rules::tick::{resolve_tick, TickInput};
use permutation_rules::{Preset, Ruleset};

const USDC: u64 = 1_000_000;
const FEE: u64 = 10 * USDC;
const HUGE: u64 = 18_443_390_120_241_605_333; // unit_cost wraps to 999

#[test]
fn a_huge_price_trade_never_makes_finish_season_owe_more_than_the_vault() {
    let rules = Ruleset::new(Preset::Blitz); // preset 0, market on
    let entries: Vec<Entry> = (0..2)
        .map(|i| Entry {
            name: format!("n{i}"),
            treasury: 20 * USDC,
        })
        .collect();
    let mut s = new_season(&rules, &[1; 32], &[2; 32], &entries).unwrap();
    // Two members per nation; the second holds Science + Diplomat.
    for (k, civ) in [0u16, 0, 1, 1].into_iter().enumerate() {
        join(&mut s, &rules, civ, [k as u8 + 1; 32]).unwrap();
    }
    for civ in 0..2usize {
        let n = &mut s.nations[civ];
        for (r, id) in [
            (Role::General, 2 * civ),
            (Role::Steward, 2 * civ),
            (Role::Science, 2 * civ + 1),
            (Role::Diplomat, 2 * civ + 1),
        ] {
            n.offices[r.index()] = id as permutation_rules::gov::MemberId;
        }
        s.civs[civ].techs.insert(Tech::BronzeWorking);
        s.civs[civ].techs.insert(Tech::Currency);
        s.civs[civ].gold = 500 * MILLI;
    }
    let tick = |s: &mut permutation_rules::state::WorldState, batches: Vec<OrderBatch>| {
        let mut vrf = [0u8; 32];
        vrf[..2].copy_from_slice(&s.tick.to_le_bytes());
        resolve_tick(
            s,
            &rules,
            &TickInput {
                vrf,
                batches,
                ..Default::default()
            },
        )
        .unwrap();
    };
    tick(&mut s, vec![]); // tick 0: last tick's gold income sets the buy cap

    // Tick 1: A's diplomat buys 1 gold, B's diplomat sells it, both at HUGE.
    let order = |side| Order::ExchangeOrder {
        good: Good::Gold,
        side,
        amount: 1,
        price: HUGE,
    };
    assert!(
        check_structure(&rules, s.tick, &[order(Side::Buy)]).is_err(),
        "RevealOrders refuses the price (WP08)"
    );
    let (a0, deposited) = (s.civs[0].usdc, s.usdc_deposited);
    let mut batches = office_batches(&s, 0, [9; 32], vec![order(Side::Buy)]);
    batches.extend(office_batches(&s, 1, [9; 32], vec![order(Side::Sell)]));
    tick(&mut s, batches);
    println!("buyer A paid {} base units", a0 - s.civs[0].usdc);

    // Jump to the end (as the finalize unit tests do) so settlement pays prizes.
    for m in &mut s.members {
        m.windows = (1 << 18) - 1;
        m.merit[Path::Science as usize] = 1_000;
    }
    s.tick = rules.ticks_per_season;

    // 4 members paid FEE (80/20 pool/ops); each nation deposited 20 USDC.
    let ops = 4 * FEE / 5;
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
        nations: 2,
        entry_fee: FEE,
        tick_seconds: 30,
        market: true,
        status: SeasonStatus::Running,
        world_seed: [1; 32],
        season_seed: [2; 32],
        member_count: 4,
        nation_members: vec![2, 2],
        seated: 4,
        pool: 4 * FEE - ops,
        ops,
        ops_withdrawn: false,
        treasury: vec![20 * USDC, 20 * USDC],
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
    // USDC the vault token account really holds: entry fees + deposits.
    let vault_balance = season.pool + season.ops + deposited;

    // Exactly what processor::settlement::finish_season stores.
    let f = finalize(&season, &s, &rules, Roster::None, false);
    assert!(
        !f.voided,
        "the rules refuse the trade: the world conserves USDC"
    );
    assert_eq!(f.owed as u128, finalize::paid_in(&season));
    season.pool = f.pool;
    season.ops = f.ops;
    season.payouts = f.payouts.clone();
    season.treasury_final = f.treasury_final.clone();
    season.refund_base = f.refund_base.clone();
    season.refund_in_payout = f.refund_in_payout.clone();
    season.outstanding = f.owed;
    season.status = SeasonStatus::Finalized;

    let claims: Vec<u64> = (0..4u32)
        .map(|i| {
            let civ = (i / 2) as u16;
            claim_amount(
                &season,
                &MemberAccount {
                    magic: MEMBER_MAGIC,
                    season_id: 1,
                    bump: 0,
                    index: i,
                    civ,
                    wallet: [0; 32],
                    session: [0; 32],
                    kind: 0,
                    name: String::new(),
                    attestation: [0; 32],
                    stand: 0,
                    votes: [0; 4],
                    // Each nation's 20 USDC deposit came from its first member.
                    shares: if i % 2 == 0 { 20 * USDC } else { 0 },
                    claimed: false,
                    tag: [0; 32],
                },
            )
        })
        .collect();
    println!("vault balance       = {vault_balance}");
    println!("season.pool         = {}", season.pool);
    println!("season.ops          = {}", season.ops);
    println!("treasury_final      = {:?}", season.treasury_final);
    println!("claim per member    = {claims:?}");

    assert_eq!(season.outstanding, vault_balance, "owed == vault");
    assert!(
        season.ops <= vault_balance,
        "WithdrawOps owes {} but the vault holds {vault_balance}",
        season.ops
    );
    for (i, c) in claims.iter().enumerate() {
        assert!(
            *c <= vault_balance,
            "member {i} is owed {c} but the vault holds {vault_balance}"
        );
    }
}
