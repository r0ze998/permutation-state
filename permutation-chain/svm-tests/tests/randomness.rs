//! The sealed-randomness grind (audit repro `sealed_randomness_grind`,
//! seal-lens A/A6) no longer works. On HEAD the tick randomness was a hash
//! of the pre-state root and the revealed salts, so an operator holding
//! every salt (AI members, deposited seals) could grind a new salt after
//! the deadline and send `[CommitOrders, CloseCommits]` in one transaction.
//! Now:
//!
//! * a commitment at or after the deadline is refused (WP01), and
//!   CloseCommits is alone in its transaction;
//! * a salt ground *before* the deadline steers nothing: the tick's
//!   randomness mixes the frozen salts with the VRF output E, which nobody
//!   knows until after the freeze (WP11), so the same salts give different
//!   ticks for different E and the operator's candidate (the v8 value) is
//!   not the tick's.

use permutation_chain::randomness::{salts_hash, tick_vrf, RAND_PENDING, RAND_VRF};
use permutation_chain_svm_tests::error::ChainError as E;
use permutation_chain_svm_tests::*;
use permutation_rules::gov::{Role, NOBODY};
use permutation_rules::orders::{order_commitment, OrderBatch};
use permutation_rules::rng::tick_seed;
use permutation_server::replay::tick_vrf_v8;
use solana_signer::Signer;

/// The first office of nation `civ`: (role, member id, member index).
fn office(c: &Chain, s: &SeasonFx, civ: usize) -> (Role, u32, usize) {
    let n = s.nation(c, civ);
    let role = *Role::ALL
        .iter()
        .find(|r| n.officers[r.index()] != NOBODY)
        .unwrap();
    let who = s
        .members
        .iter()
        .position(|m| m.session.pubkey().to_bytes() == n.keys[role.index()])
        .unwrap();
    (role, n.officers[role.index()], who)
}

fn empty(civ: u16, role: Role, member: u32) -> OrderBatch {
    OrderBatch {
        civ,
        tick: 0,
        role,
        member,
        decision_digest: [1; 32],
        orders: vec![],
        adopt: vec![],
    }
}

#[test]
fn the_operator_cannot_choose_the_tick_randomness() {
    let mut c = Chain::new();
    let s = SeasonFx::running(&mut c, Params::default(), 2);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    let (hr, hm, hw) = office(&c, &s, 0); // a person on the web client
    let (ar, am, aw) = office(&c, &s, 1); // an operator AI
    let human = empty(0, hr, hm);
    let ai = empty(1, ar, am);
    let (salt_human, salt_ai) = ([0x11; 32], [0x22; 32]);
    let hs = s.members[hw].session.insecure_clone();
    let ais = s.members[aw].session.insecure_clone();
    let commit = |b: &OrderBatch, salt: &[u8; 32], who: &solana_address::Address| {
        s.commit_orders_ix(who, b.civ, b.role, 0, order_commitment(b, salt))
    };
    c.send(
        vec![commit(&human, &salt_human, &hs.pubkey())],
        &[&crank, &hs],
    )
    .expect("human commit");
    c.send(vec![commit(&ai, &salt_ai, &ais.pubkey())], &[&crank, &ais])
        .expect("AI commit");
    let pre_root = s.root(&mut c);
    let season_seed = s.world(&mut c).season_seed;
    let deadline = s.meta(&c).deadline;

    // After the deadline: the operator's ground salt, committed and closed
    // in one transaction, is refused at the commitment.
    let ground = [0x33; 32];
    let mut late = c.fork();
    late.set_time(deadline + 1);
    let r = late.send(
        vec![commit(&ai, &ground, &ais.pubkey()), s.close_ix()],
        &[&crank, &ais],
    );
    assert_err(r, E::WrongPhase);
    assert_err(
        late.send(vec![commit(&ai, &ground, &ais.pubkey())], &[&crank, &ais]),
        E::WrongPhase,
    );

    // Before the deadline the operator may still replace the AI's
    // commitment with a salt of its choice; it knows both salts.
    c.set_time(deadline - 1);
    c.send(vec![commit(&ai, &ground, &ais.pubkey())], &[&crank, &ais])
        .expect("AI commit, replaced before the deadline");
    let salts = vec![
        (0u16, hr.index() as u8, salt_human),
        (1u16, ar.index() as u8, ground),
    ];
    let ground_seed = tick_seed(&season_seed, &tick_vrf_v8(&pre_root, &salts), 0);
    s.close(&mut c);
    c.send(vec![s.reveal_orders_ix(&cp, &human, salt_human)], &[&crank])
        .expect("reveal human");
    c.send(vec![s.reveal_orders_ix(&cp, &ai, ground)], &[&crank])
        .expect("reveal AI");
    s.freeze_and_request(&mut c);
    // Frozen, the randomness pending: nothing publishes or resolves yet.
    assert_err(c.send(vec![s.log_ix(0)], &[&crank]), E::RandomnessPending);
    assert_err(
        c.send(vec![s.resolve_ix(12)], &[&crank]),
        E::InputNotPublished,
    );
    let m = s.meta(&c);
    assert_eq!(m.rand_state, RAND_PENDING);
    assert_eq!(m.rand_pre, salts_hash(7, 0, &salts));
    let request = s.tick_request(&c);

    // Two oracle outputs for the same frozen salts: two different ticks.
    let mut seeds = vec![];
    for e in [[0xa1; 32], [0xa2; 32]] {
        let mut f = c.fork();
        vrf::fulfil(&mut f, &request, e).expect("the oracle's callback");
        let m = s.meta(&f);
        assert_eq!(m.rand_state, RAND_VRF);
        assert_eq!(m.vrf, tick_vrf(&m.rand_pre, &e, RAND_VRF).unwrap());
        let vrf = m.vrf;
        let (input, _) = s.log_input(&mut f);
        let input: permutation_rules::tick::TickInput = borsh::from_slice(&input).unwrap();
        assert_eq!(input.vrf, vrf);
        resolve_parts(&mut f, &s, &crank, false);
        let world = s.world(&mut f);
        assert_eq!(world.tick, 1);
        assert_eq!(world.tick_seed, tick_seed(&season_seed, &vrf, 0));
        assert_ne!(
            world.tick_seed, ground_seed,
            "the tick seed is not the one the operator computed"
        );
        seeds.push(world.tick_seed);
    }
    assert_ne!(seeds[0], seeds[1], "E, not the salts, decides the tick");
}
