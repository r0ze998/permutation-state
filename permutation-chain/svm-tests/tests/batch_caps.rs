//! WP04 on the SBF build: the caps RevealOrders applies to a batch (order
//! count, free orders, adopted proposals, `MAX_REVEAL_BYTES`), and the
//! degraded ResolveTick step (`DEGRADED`) that finishes a tick no normal
//! part can. The joint capacity gate at the member cap is `budget.rs` (T).

use borsh::BorshDeserialize;
use permutation_chain_svm_tests::error::ChainError as E;
use permutation_chain_svm_tests::state::*;
use permutation_chain_svm_tests::*;
use permutation_rules::gov::{Role, NOBODY};
use permutation_rules::hex::Hex;
use permutation_rules::orders::{order_commitment, Order, OrderBatch};
use permutation_rules::tick::{run_phase, run_phase_degraded, TickInput};
use solana_signer::Signer;

/// The general of nation 0 and the member holding it.
fn general(c: &Chain, s: &SeasonFx) -> (u32, usize) {
    let n = s.nation(c, 0);
    let m = n.officers[Role::General.index()];
    assert_ne!(m, NOBODY);
    let who = s
        .members
        .iter()
        .position(|x| x.session.pubkey().to_bytes() == n.keys[Role::General.index()])
        .unwrap();
    (m, who)
}

fn moves(n: usize, hexes: usize) -> Vec<Order> {
    (0..n)
        .map(|u| Order::MoveUnit {
            unit: 1000 + u as u32,
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

/// Commits `orders` and `adopt` as nation 0's general batch in a fork at
/// tick 0, closes, and sends its reveal.
fn reveal_in_fork(
    c: &Chain,
    s: &SeasonFx,
    orders: Vec<Order>,
    adopt: Vec<u32>,
) -> (Chain, Result<Landed, Fail>, usize) {
    let mut f = c.fork();
    let (member, who) = general(&f, s);
    let b = OrderBatch {
        civ: 0,
        tick: 0,
        role: Role::General,
        member,
        decision_digest: [1; 32],
        orders,
        adopt,
    };
    let crank = s.crank.insecure_clone();
    let sess = s.members[who].session.insecure_clone();
    let salt = [0x44; 32];
    f.send(
        vec![s.commit_orders_ix(
            &sess.pubkey(),
            0,
            Role::General,
            0,
            order_commitment(&b, &salt),
        )],
        &[&crank, &sess],
    )
    .expect("CommitOrders");
    s.close(&mut f);
    let ix = s.reveal_orders_ix(&crank.pubkey(), &b, salt);
    let size = tx_size(std::slice::from_ref(&ix), &crank.pubkey());
    let r = f.send(vec![ix], &[&crank]);
    (f, r, size)
}

#[test]
fn reveals_are_capped() {
    let mut c = Chain::new();
    let s = SeasonFx::running(&mut c, Params::default(), 4);
    // A budget large enough that only the caps can refuse.
    c.edit::<NationAccount>(&s.nations[0], |n| n.spendable[Role::General.index()] = 100);
    // A zero-cost flood (HEAD accepted it and ran phase 0 out of memory).
    let (_, r, _) = reveal_in_fork(&c, &s, consents(272), vec![]);
    assert_err(r, E::OverBudget);
    // Nine free orders.
    let (_, r, _) = reveal_in_fork(&c, &s, consents(9), vec![]);
    assert_err(r, E::OverBudget);
    // One order and 24 adopted proposals (25 > MAX_BATCH_ORDERS).
    let (_, r, _) = reveal_in_fork(&c, &s, consents(1), (0..24).collect());
    assert_err(r, E::OverBudget);
    // Bytes: 950 is the cap.
    let mut exact = moves(12, 6);
    exact.extend(moves(4, 7).into_iter().map(|o| match o {
        Order::MoveUnit { unit, path } => Order::MoveUnit {
            unit: unit + 100,
            path,
        },
        o => o,
    }));
    let mut over = exact.clone();
    exact.extend(consents(2));
    assert_eq!(bytes(&exact), MAX_REVEAL_BYTES);
    over.pop();
    over.push(Order::MoveUnit {
        unit: 1200,
        path: vec![Hex { q: 1, r: -1 }; 6],
    });
    over.extend(consents(5));
    assert_eq!(bytes(&over), MAX_REVEAL_BYTES + 1);
    let (_, r, _) = reveal_in_fork(&c, &s, over, vec![]);
    assert_err(r, E::OverBudget);
    let (_, r, _) = reveal_in_fork(&c, &s, exact, vec![]);
    assert!(
        r.as_ref()
            .map_or_else(|f| f.code != Some(E::OverBudget as u32), |_| true),
        "950 B passes the byte cap: {r:?}"
    );
    // The largest batch a transaction carries: accepted.
    let mut sendable = moves(15, 6);
    sendable.extend(consents(8));
    assert_eq!(bytes(&sendable), 879);
    let l = reveal_in_fork(&c, &s, sendable.clone(), vec![])
        .1
        .expect("the sendable maximum lands");
    let (f, _, size) = reveal_in_fork(&c, &s, sendable.clone(), vec![]);
    println!(
        "sendable maximum: {} B of orders, tx {size} B, {} CU",
        bytes(&sendable),
        l.cu
    );
    assert!(size <= 1232);
    let n = s.nation(&f, 0);
    assert_eq!(
        n.batches[Role::General.index()]
            .as_ref()
            .map(|b| b.orders.len()),
        Some(23)
    );
    // One hex more per move: 999 B, over the byte cap.
    let mut unsendable = moves(15, 7);
    unsendable.extend(consents(8));
    assert_eq!(bytes(&unsendable), 999);
    let (_, r, _) = reveal_in_fork(&c, &s, unsendable, vec![]);
    assert_err(r, E::OverBudget);
}

/// The degraded step: only once `degrade_after(tick_seconds)` passed the
/// (reveal) deadline, exactly the next phase, without the tick's orders.
#[test]
fn degraded_step_needs_the_timer() {
    let mut c = Chain::new();
    let s = SeasonFx::running(&mut c, Params::default(), 4);
    let crank = s.crank.insecure_clone();
    let rules = s.rules();
    let (member, who) = general(&c, &s);
    let b = OrderBatch {
        civ: 0,
        tick: 0,
        role: Role::General,
        member,
        decision_digest: [1; 32],
        orders: vec![],
        adopt: vec![],
    };
    let sess = s.members[who].session.insecure_clone();
    c.send(
        vec![s.commit_orders_ix(
            &sess.pubkey(),
            0,
            Role::General,
            0,
            order_commitment(&b, &[5; 32]),
        )],
        &[&crank, &sess],
    )
    .expect("CommitOrders");
    s.close(&mut c);
    c.send(
        vec![s.reveal_orders_ix(&crank.pubkey(), &b, [5; 32])],
        &[&crank],
    )
    .expect("RevealOrders");
    let (input, _) = s.log_input(&mut c);
    let input = TickInput::try_from_slice(&input).unwrap();
    assert_eq!(input.batches.len(), 1);
    let start = s.world(&mut c);
    let deadline = s.meta(&c).deadline;
    c.set_time(deadline + degrade_after(30) - 1);
    assert_err(c.send(vec![s.resolve_ix(DEGRADED)], &[&crank]), E::TooEarly);
    c.set_time(deadline + degrade_after(30));
    // Phase 0 degraded: the input's batch is not applied.
    let l = c
        .send(vec![s.resolve_ix(DEGRADED | 12)], &[&crank])
        .expect("degraded step");
    let t = record(&l.logs, b"PS_TICK");
    assert_eq!(t[2], vec![DEGRADED | 1], "exactly the next phase");
    assert_eq!(
        t[5],
        s.meta(&c).input_hash.to_vec(),
        "the published input's hash"
    );
    let mut native = start.clone();
    run_phase_degraded(&mut native, &rules, input.vrf, 0).unwrap();
    assert_eq!(s.world(&mut c), native);
    // Normal parts again, then a degraded one mid-tick.
    c.send(vec![s.resolve_ix(3)], &[&crank])
        .expect("normal part");
    for p in 1..3 {
        run_phase(&mut native, &rules, &input, p).unwrap();
    }
    let l = c
        .send(vec![s.resolve_ix(DEGRADED)], &[&crank])
        .expect("degraded step");
    assert_eq!(record(&l.logs, b"PS_TICK")[2], vec![DEGRADED | 4]);
    run_phase_degraded(&mut native, &rules, input.vrf, 3).unwrap();
    assert_eq!(s.world(&mut c), native);
    c.send(vec![s.resolve_ix(12)], &[&crank])
        .expect("the rest of the tick");
    for p in 4..12 {
        run_phase(&mut native, &rules, &input, p).unwrap();
    }
    assert_eq!(s.world(&mut c), native);
    assert_eq!(native.tick, 1);
}
