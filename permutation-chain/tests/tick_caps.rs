//! The batch caps RevealOrders applies (WP04), through the program's
//! dispatcher: at most `MAX_BATCH_ORDERS` orders and adopted proposals, at
//! most `MAX_FREE_ORDERS` zero-cost ones, at most `MAX_REVEAL_BYTES` of
//! orders; every refusal is `OverBudget`. And the degraded step's timer
//! (`state::tests::reveal_room_holds_the_largest_revealed_batch` pins
//! `REVEAL_ROOM` against the largest batch).

mod play_fixture;

use permutation_chain::error::ChainError;
use permutation_chain::instruction::ChainInstruction;
use permutation_chain::processor::set_host_clock;
use permutation_chain::state::*;
use permutation_rules::gov::Role;
use permutation_rules::hex::Hex;
use permutation_rules::orders::{
    order_commitment, Order, OrderBatch, MAX_BATCH_ORDERS, MAX_FREE_ORDERS,
};
use play_fixture::*;
use solana_program::program_error::ProgramError;
use solana_program::pubkey::Pubkey;

const NOW: i64 = 1_000;

fn moves(n: usize, hexes: usize, first: u32) -> Vec<Order> {
    (0..n)
        .map(|u| Order::MoveUnit {
            unit: first + u as u32,
            path: vec![Hex { q: 1, r: -1 }; hexes],
        })
        .collect()
}

fn consents(n: usize) -> Vec<Order> {
    (0..n).map(|_| Order::ConsentWar { civ: 1 }).collect()
}

fn bytes(orders: &[Order]) -> usize {
    orders
        .iter()
        .map(|o| borsh::object_length(o).unwrap())
        .sum()
}

/// Nation 0's general reveals `orders` and `adopt`, committed for tick 1,
/// in the reveal window (a spendable budget of 100).
fn reveal(orders: Vec<Order>, adopt: Vec<u32>) -> Result<(), ProgramError> {
    set_host_clock(NOW);
    let program = Pubkey::new_unique();
    let keys: Vec<[u8; 32]> = (0..2).map(|i| [0x20 + i as u8; 32]).collect();
    let state = world(&keys);
    let (key, mut n) = open_nation(&program, &state, 0, 1, NOW + 5);
    let g = Role::General.index();
    let b = OrderBatch {
        civ: 0,
        tick: 1,
        role: Role::General,
        member: n.officers[g],
        decision_digest: [1; 32],
        orders,
        adopt,
    };
    let salt = [0x33; 32];
    n.committed[g] = 1;
    n.commits[g] = order_commitment(&b, &salt);
    n.revealing = true;
    let mut a = Accounts::default();
    a.push(Pubkey::new_unique(), Pubkey::default(), true, vec![]);
    a.push(key, program, false, nation_data(&n));
    let r = run(
        &program,
        &mut a,
        &[0, 1],
        &ChainInstruction::RevealOrders {
            role: Role::General,
            tick: 1,
            decision_digest: b.decision_digest,
            orders: b.orders.clone(),
            adopt: b.adopt.clone(),
            salt,
        },
    );
    if r.is_ok() {
        let n: NationAccount = a.load(1);
        assert_eq!(n.batches[g], Some(b));
    }
    r
}

fn over() -> Result<(), ProgramError> {
    Err(ChainError::OverBudget.into())
}

#[test]
fn reveal_size_limits() {
    // Bytes: exactly MAX_REVEAL_BYTES lands, one more is refused.
    let mut exact = moves(12, 6, 0);
    exact.extend(moves(4, 7, 100));
    let mut one_more = exact.clone();
    exact.extend(consents(2));
    assert_eq!(bytes(&exact), MAX_REVEAL_BYTES);
    one_more.pop();
    one_more.extend(moves(1, 6, 200));
    one_more.extend(consents(5));
    assert_eq!(bytes(&one_more), MAX_REVEAL_BYTES + 1);
    assert_eq!(reveal(exact, vec![]), Ok(()));
    assert_eq!(reveal(one_more, vec![]), over());
    // Orders and adopted proposals together: MAX_BATCH_ORDERS.
    assert_eq!(MAX_BATCH_ORDERS, 24);
    assert_eq!(reveal(moves(24, 1, 0), vec![]), Ok(()));
    assert_eq!(reveal(moves(24, 1, 0), vec![0]), over());
    assert_eq!(reveal(consents(1), (0..24).collect()), over());
    // Zero-cost orders: MAX_FREE_ORDERS, and the audit's 272-order flood.
    assert_eq!(reveal(consents(MAX_FREE_ORDERS), vec![]), Ok(()));
    assert_eq!(reveal(consents(MAX_FREE_ORDERS + 1), vec![]), over());
    assert_eq!(reveal(consents(272), vec![]), over());
}

/// The degraded step waits `max(600, 2 × tick_seconds)` past the deadline:
/// ten minutes in Blitz, eight hours for a four-hour tick.
#[test]
fn degrade_after_values() {
    assert_eq!(degrade_after(30), 600);
    assert_eq!(degrade_after(300), 600);
    assert_eq!(degrade_after(301), 602);
    assert_eq!(degrade_after(MAX_TICK_SECONDS), 28_800);
    assert_eq!(DEGRADED, 0x80);
}
