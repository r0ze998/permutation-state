//! WP04: one office's batch runs at most `MAX_BATCH_ORDERS` orders (its
//! adopted proposals' included) and `MAX_FREE_ORDERS` zero-cost ones, and a
//! tick nobody completed can take degraded steps without its orders.
mod common;
use common::nations::*;
use common::vrf;
use permutation_rules::gov::{self, GovAction, Role};
use permutation_rules::orders::{
    accept_batch, check_batch_size, check_structure, expand_batch, Good, Order, Side,
    MAX_BATCH_ORDERS, MAX_FREE_ORDERS,
};
use permutation_rules::tick::{resolve_tick, run_phase, run_phase_degraded, TickInput};
use permutation_rules::{Preset, RulesError, Ruleset};

fn moves(from: u32, n: usize) -> Vec<Order> {
    (0..n as u32)
        .map(|k| Order::MoveUnit {
            unit: from + k,
            path: vec![],
        })
        .collect()
}

/// The four zero-cost kinds, `n` of them in turn.
fn free(n: usize) -> Vec<Order> {
    (0..n)
        .map(|k| match k % 4 {
            0 => Order::ConsentWar { civ: 1 },
            1 => Order::ConsentSpend { usdc: k as u64 },
            2 => Order::ExchangeOrder {
                good: Good::Iron,
                side: Side::Buy,
                amount: 1,
                price: 1,
            },
            _ => Order::RevealRationale {
                tick: 1,
                policy: vec![],
                salt: [0; 16],
                text: vec![],
            },
        })
        .collect()
}

#[test]
fn free_orders_capped_at_eight() {
    let rules = Ruleset::new(Preset::Blitz);
    assert_eq!(MAX_FREE_ORDERS, 8);
    assert_eq!(check_structure(&rules, 5, &free(8)), Ok(0));
    assert_eq!(
        check_structure(&rules, 5, &free(9)),
        Err(RulesError::TooManyOrders)
    );
    // A ninth of each kind on top of the mixed eight.
    for extra in free(4) {
        let mut b = free(8);
        b.push(extra);
        assert_eq!(
            check_structure(&rules, 5, &b),
            Err(RulesError::TooManyOrders)
        );
    }
}

#[test]
fn batch_order_count_capped() {
    let rules = Ruleset::new(Preset::Blitz);
    assert_eq!(MAX_BATCH_ORDERS, 24);
    assert_eq!(check_structure(&rules, 0, &moves(0, 24)), Ok(24));
    assert_eq!(
        check_structure(&rules, 0, &moves(0, 25)),
        Err(RulesError::TooManyOrders)
    );
}

#[test]
fn check_batch_size_counts_adopted_ids() {
    let ids = |n: u32| (0..n).collect::<Vec<u32>>();
    assert_eq!(check_batch_size(&moves(0, 20), &ids(4)), Ok(()));
    assert_eq!(
        check_batch_size(&moves(0, 20), &ids(5)),
        Err(RulesError::TooManyOrders)
    );
}

#[test]
fn adoption_count_skip() {
    let (rules, mut s) = world(&[3, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::General]);
    gov::open_government(&mut s, &rules, &e).unwrap();
    // Three 4-order proposals (two proposers: two open proposals each at most).
    step(
        &mut s,
        &rules,
        vec![],
        vec![
            act(
                1,
                GovAction::Propose {
                    role: Role::General,
                    orders: moves(100, 4),
                },
            ),
            act(
                1,
                GovAction::Propose {
                    role: Role::General,
                    orders: moves(110, 4),
                },
            ),
            act(
                2,
                GovAction::Propose {
                    role: Role::General,
                    orders: moves(120, 4),
                },
            ),
        ],
    );
    let ids: Vec<u32> = s.nations[0].proposals.iter().map(|p| p.id).collect();
    assert_eq!(ids.len(), 3);
    let mut b = batch(&s, 0, Role::General, 0, moves(0, 20));
    b.adopt = ids.clone();
    // 20 + 3 ids fit `check_batch_size`; 20 + 4 = 24 adopts the first, the
    // next two would pass 24 and are skipped.
    let (orders, adopted) = expand_batch(&s, &b).unwrap();
    assert_eq!(adopted, vec![ids[0]]);
    assert_eq!(orders.len(), 24);
}

#[test]
fn max_legal_batch_is_accepted() {
    let (rules, mut s) = world(&[1, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::General]);
    gov::open_government(&mut s, &rules, &e).unwrap();
    s.nations[0].role_bank[Role::General.index()] = 15;
    s.civs[0].tick_budget = 0;
    let mut orders = moves(0, 15);
    orders.extend((1..=8).map(|c| Order::ConsentWar { civ: 1 + c % 5 }));
    let b = batch(&s, 0, Role::General, 0, orders);
    let (cost, run, adopted) = accept_batch(&s, &rules, &b).unwrap();
    assert_eq!((cost, run.len(), adopted.len()), (15, 23, 0));
}

/// A world with a seated general, a tick input with its batch and a vote.
fn loud_tick() -> (Ruleset, permutation_rules::state::WorldState, TickInput) {
    let (rules, mut s) = world(&[2, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::General, Role::Science]);
    gov::open_government(&mut s, &rules, &e).unwrap();
    let science = batch(
        &s,
        0,
        Role::Science,
        0,
        vec![Order::SetResearch {
            techs: vec![permutation_rules::tech::Tech::Agriculture],
        }],
    );
    let input = TickInput {
        vrf: vrf(s.tick),
        batches: vec![science],
        gov: vec![act(1, GovAction::Stand { roles: 0x0f })],
        deposits: vec![],
    };
    (rules, s, input)
}

#[test]
fn degraded_phase0_drops_orders_and_gov() {
    let (rules, s, input) = loud_tick();
    let mut degraded = s.clone();
    run_phase_degraded(&mut degraded, &rules, input.vrf, 0).unwrap();
    let mut empty = s.clone();
    let quiet = TickInput {
        vrf: input.vrf,
        ..Default::default()
    };
    run_phase(&mut empty, &rules, &quiet, 0).unwrap();
    assert_eq!(
        degraded, empty,
        "a degraded phase 0 is phase 0 on an empty input"
    );
    let mut normal = s.clone();
    run_phase(&mut normal, &rules, &input, 0).unwrap();
    assert_eq!(
        degraded.tick_seed, normal.tick_seed,
        "the randomness is unchanged"
    );
    assert_ne!(degraded.event_head, normal.event_head, "no decision event");
    assert_eq!(degraded.members[1].standing_for, s.members[1].standing_for);
    assert_ne!(normal.members[1].standing_for, s.members[1].standing_for);
}

#[test]
fn degraded_later_phase_clears_tick_orders_keeps_spent() {
    let (rules, mut s, input) = loud_tick();
    run_phase(&mut s, &rules, &input, 0).unwrap();
    run_phase(&mut s, &rules, &input, 1).unwrap();
    let spent: Vec<[u16; 4]> = s.tick_orders.iter().map(|c| c.spent).collect();
    assert!(s.tick_orders.iter().any(|c| !c.orders.is_empty()));
    run_phase_degraded(&mut s, &rules, input.vrf, 2).unwrap();
    assert_eq!(s.phase_cursor, 3);
    for c in &s.tick_orders {
        assert!(c.orders.is_empty() && c.credits.is_empty() && c.origin.is_empty());
    }
    assert_eq!(
        s.tick_orders.iter().map(|c| c.spent).collect::<Vec<_>>(),
        spent
    );
    let tick = s.tick;
    resolve_tick(&mut s, &rules, &TickInput::default()).unwrap();
    assert_eq!(s.tick, tick + 1, "the tick completes");
}
