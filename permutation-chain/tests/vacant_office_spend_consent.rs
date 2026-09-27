//! The two-officer rule for treasury spending (V5 §7.5 ④: "spending more
//! than the threshold per tick needs, besides the diplomat, the consent of
//! one other officer — one person cannot empty the treasury") is bypassed
//! through a vacant office.
//!
//! The caretaker of a vacant office adopts a lone member's proposal and runs
//! it with `credit.officer == NOBODY`; `markets::usdc::spend_limit` accepts a
//! `ConsentSpend` whenever `credit.officer != diplomat || diplomat == NOBODY`,
//! so the consent the diplomat (or anyone) proposes for itself is counted.
//!
//! Audit repro (WP10 R1/R2): fixed by rules v9. A treasury order cannot be
//! proposed, and a consent counts only from a seated officer who is not the
//! diplomat, so both attacks leave the treasury intact; the control is
//! unchanged. Claims go through the program's own `finalize` and
//! `claim_amount`.

use permutation_chain::finalize::{finalize, Roster};
use permutation_chain::payout::claim_amount;
use permutation_chain::state::*;
use permutation_rules::genesis::{new_season, Entry};
use permutation_rules::gov::{self, GovAction, GovEntry, MemberId, Role, NOBODY};
use permutation_rules::orders::{Good, Order, OrderBatch, Side};
use permutation_rules::state::WorldState;
use permutation_rules::tick::{resolve_tick, TickInput};
use permutation_rules::{Preset, Ruleset};

const USDC: u64 = 1_000_000;
const FEE: u64 = 10 * USDC;
const NEEDS_SPEND_CONSENT: u8 = 50;

fn key(m: u32) -> [u8; 32] {
    let mut k = [7u8; 32];
    k[..4].copy_from_slice(&(m + 1).to_le_bytes());
    k
}
fn act(m: MemberId, action: GovAction) -> GovEntry {
    GovEntry {
        member: m,
        signer: key(m),
        action,
    }
}
fn buy_or_sell_gold(side: Side, price: u64) -> Order {
    Order::ExchangeOrder {
        good: Good::Gold,
        side,
        amount: 1,
        price,
    }
}
fn officer_batch(s: &WorldState, civ: u16, role: Role, orders: Vec<Order>) -> OrderBatch {
    OrderBatch {
        civ,
        tick: s.tick,
        role,
        member: s.nations[civ as usize].holder(role),
        decision_digest: [9; 32],
        orders,
        adopt: vec![],
    }
}
fn tick(s: &mut WorldState, rules: &Ruleset, batches: Vec<OrderBatch>, gov: Vec<GovEntry>) {
    let mut vrf = [0u8; 32];
    vrf[..2].copy_from_slice(&s.tick.to_le_bytes());
    resolve_tick(
        s,
        rules,
        &TickInput {
            vrf,
            batches,
            gov,
            deposits: vec![],
        },
    )
    .unwrap();
    let v = permutation_rules::invariants::check(s, rules);
    assert!(v.is_empty(), "{v:?}");
}

/// Six nations. Members: 0 = D (nation 0, deposit 0), 1 = H (nation 0,
/// honest depositor of 100 USDC, stands for nothing), 2 = Y (nation 1,
/// deposit 1 base unit, its diplomat). `d_stands`: D is nation 0's diplomat.
/// Every other office of nation 0 is vacant.
fn world(d_stands: bool) -> (Ruleset, WorldState, Vec<u64>) {
    let rules = Ruleset::new(Preset::Blitz);
    let shares = vec![0u64, 100 * USDC, 1];
    let entries: Vec<Entry> = [100 * USDC, 1, 0, 0, 0, 0]
        .iter()
        .enumerate()
        .map(|(i, t)| Entry {
            name: format!("n{i}"),
            treasury: *t,
        })
        .collect();
    let mut s = new_season(&rules, &[11; 32], &[22; 32], &entries).unwrap();
    for (i, civ) in [0u16, 0, 1].into_iter().enumerate() {
        assert_eq!(
            gov::join(&mut s, &rules, civ, key(i as u32)).unwrap(),
            i as u32
        );
    }
    let mut pre = vec![];
    if d_stands {
        pre.push(act(
            0,
            GovAction::Stand {
                roles: Role::Diplomat.bit(),
            },
        ));
        pre.push(act(
            0,
            GovAction::Vote {
                role: Role::Diplomat,
                candidate: 0,
            },
        ));
    }
    pre.push(act(
        2,
        GovAction::Stand {
            roles: Role::Diplomat.bit(),
        },
    ));
    pre.push(act(
        2,
        GovAction::Vote {
            role: Role::Diplomat,
            candidate: 2,
        },
    ));
    gov::open_government(&mut s, &rules, &pre).unwrap();
    (rules, s, shares)
}

/// What each member could claim if the season ended in this state, through
/// the program's own `finalize` and `claim_amount`.
fn claims(s: &WorldState, rules: &Ruleset, shares: &[u64]) -> Vec<u64> {
    let mut end = s.clone();
    end.tick = rules.ticks_per_season;
    let members = end.members.len() as u64;
    let ops = FEE * rules.ops_share_bps as u64 / 10_000;
    let mut treasury = vec![0u64; end.civs.len()];
    let accts: Vec<MemberAccount> = end
        .members
        .iter()
        .enumerate()
        .map(|(i, m)| {
            treasury[m.civ as usize] += shares[i];
            MemberAccount {
                magic: MEMBER_MAGIC,
                season_id: 1,
                bump: 0,
                index: i as u32,
                civ: m.civ,
                wallet: [i as u8; 32],
                session: [0; 32],
                kind: 0,
                name: String::new(),
                attestation: [0; 32],
                stand: 0,
                votes: [0; 4],
                shares: shares[i],
                claimed: false,
                tag: [0; 32],
            }
        })
        .collect();
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
        nations: end.civs.len() as u8,
        entry_fee: FEE,
        tick_seconds: 30,
        market: true,
        status: SeasonStatus::Running,
        world_seed: [0; 32],
        season_seed: [0; 32],
        member_count: members as u32,
        nation_members: vec![],
        seated: members as u32,
        pool: members * (FEE - ops),
        ops: members * ops,
        ops_withdrawn: false,
        treasury,
        treasury_final: vec![],
        payouts: vec![],
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
        bounty_paid: vec![],
        delegated: 0,
        roster_blind: [0; 32],
        refund_base: vec![],
        refund_in_payout: vec![],
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
    let f = finalize(&season, &end, rules, Roster::None, false);
    assert!(!f.voided);
    season.payouts = f.payouts.clone();
    season.treasury_final = f.treasury_final.clone();
    season.refund_base = f.refund_base.clone();
    season.refund_in_payout = f.refund_in_payout.clone();
    season.pool = f.pool;
    season.ops = f.ops;
    season.status = SeasonStatus::Finalized;
    accts.iter().map(|m| claim_amount(&season, m)).collect()
}

/// 1 gold at a price whose unit cost (price + 5% fee + 5% base tariff) is
/// the whole 100 USDC treasury.
const PRICE: u64 = 100 * USDC * 10 / 11;

fn consent_max() -> Vec<Order> {
    vec![Order::ConsentSpend { usdc: u64::MAX }]
}

/// Control: without a consent, D's buy exceeds the 5 USDC threshold and is
/// skipped with NeedsSpendConsent.
#[test]
fn control_the_limit_holds_without_consent() {
    let (rules, mut s, _) = world(true);
    tick(&mut s, &rules, vec![], vec![]);
    let b = vec![
        officer_batch(
            &s,
            0,
            Role::Diplomat,
            vec![buy_or_sell_gold(Side::Buy, PRICE)],
        ),
        officer_batch(
            &s,
            1,
            Role::Diplomat,
            vec![buy_or_sell_gold(Side::Sell, PRICE)],
        ),
    ];
    tick(&mut s, &rules, b, vec![]);
    eprintln!(
        "control: nation0 usdc {} skips {:?}",
        s.civs[0].usdc, s.last_skipped
    );
    assert_eq!(s.civs[0].usdc, 100 * USDC);
    assert!(s
        .last_skipped
        .iter()
        .any(|k| k.civ == 0 && k.reason == NEEDS_SPEND_CONSENT));
}

/// D is the diplomat and the only officer. D proposes ConsentSpend(max) to
/// the vacant Science office; the caretaker adopts it the next tick and it
/// counts as "another officer's consent". D empties the treasury alone.
#[test]
fn a_lone_diplomat_cannot_consent_to_its_own_spending() {
    let (rules, mut s, shares) = world(true);
    assert_eq!(
        s.nations[0].holder(Role::Diplomat),
        0,
        "D holds the diplomat office"
    );
    assert!(
        [Role::General, Role::Steward, Role::Science]
            .iter()
            .all(|r| s.nations[0].holder(*r) == NOBODY),
        "every other office of nation 0 is vacant"
    );
    let before = claims(&s, &rules, &shares);

    // Tick 0: D (the diplomat itself) proposes the consent to the vacant Science office.
    tick(
        &mut s,
        &rules,
        vec![],
        vec![act(
            0,
            GovAction::Propose {
                role: Role::Science,
                orders: consent_max(),
            },
        )],
    );
    // Tick 1: the caretaker adopts it; D buys 1 gold from Y at ~the whole treasury.
    let b = vec![
        officer_batch(
            &s,
            0,
            Role::Diplomat,
            vec![buy_or_sell_gold(Side::Buy, PRICE)],
        ),
        officer_batch(
            &s,
            1,
            Role::Diplomat,
            vec![buy_or_sell_gold(Side::Sell, PRICE)],
        ),
    ];
    let (vault0, ops0) = (s.exchange_vault, s.exchange_ops);
    tick(&mut s, &rules, b, vec![]);
    let after = claims(&s, &rules, &shares);
    eprintln!(
        "lone diplomat: proposals adopted {}; nation0 usdc {} (was {}), nation1 usdc {} (was 1); \
         pool +{} ops +{}; skips {:?}\n  claims [D, H, Y] before {:?}\n  claims [D, H, Y] after  {:?}",
        s.nations[0].adopted,
        s.civs[0].usdc,
        100 * USDC,
        s.civs[1].usdc,
        s.exchange_vault - vault0,
        s.exchange_ops - ops0,
        s.last_skipped,
        before,
        after
    );
    assert!(
        s.last_skipped.iter().any(|k| k.civ == 0 && k.reason == NEEDS_SPEND_CONSENT),
        "the diplomat's own consent must not lift its spend limit: expected skip 50 (NeedsSpendConsent)"
    );
    assert_eq!(
        s.civs[0].usdc,
        100 * USDC,
        "one person emptied nation 0's treasury (V5 §7.5 ④)"
    );
    assert_eq!(after, before, "the claims are unchanged");
}

/// Nobody holds an office in nation 0. D, a member with no office at all,
/// proposes the buy (to the vacant Diplomat office) and the consent (to the
/// vacant Science office); the caretaker adopts both the next tick.
#[test]
fn a_member_without_office_cannot_drain_a_nation_with_a_vacant_diplomat() {
    let (rules, mut s, shares) = world(false);
    assert!(
        s.nations[0].offices.iter().all(|o| *o == NOBODY),
        "nation 0 has no officers"
    );
    let before = claims(&s, &rules, &shares);
    tick(
        &mut s,
        &rules,
        vec![],
        vec![
            act(
                0,
                GovAction::Propose {
                    role: Role::Diplomat,
                    orders: vec![buy_or_sell_gold(Side::Buy, PRICE)],
                },
            ),
            act(
                0,
                GovAction::Propose {
                    role: Role::Science,
                    orders: consent_max(),
                },
            ),
        ],
    );
    let b = vec![officer_batch(
        &s,
        1,
        Role::Diplomat,
        vec![buy_or_sell_gold(Side::Sell, PRICE)],
    )];
    tick(&mut s, &rules, b, vec![]);
    let after = claims(&s, &rules, &shares);
    eprintln!(
        "no-office member: proposals adopted {}; nation0 usdc {} (was {}), nation1 usdc {} (was 1); \
         skips {:?}\n  claims [D, H, Y] before {:?}\n  claims [D, H, Y] after  {:?}",
        s.nations[0].adopted,
        s.civs[0].usdc,
        100 * USDC,
        s.civs[1].usdc,
        s.last_skipped,
        before,
        after
    );
    assert_eq!(
        s.civs[0].usdc,
        100 * USDC,
        "a member with no office emptied nation 0's treasury on its own proposals"
    );
    assert_eq!(after, before, "the claims are unchanged");
}
