//! Invariants on the SBF build (DESIGN.md): I1 conservation of USDC, chain
//! equals native (the verifier's premise), I4 flags move together, I6 a
//! split tick equals a whole one.

use borsh::BorshDeserialize;
use permutation_chain_svm_tests::*;
use permutation_rules::gov::{Role, NOBODY};
use permutation_rules::orders::{order_commitment, OrderBatch};
use permutation_rules::tick::{resolve_tick, TickInput};
use solana_signer::Signer;

/// Every tick, resolved natively on the published input, gives the root the
/// program stored.
#[test]
fn chain_ticks_match_native_ticks() {
    let mut c = Chain::new();
    let s = SeasonFx::running(
        &mut c,
        Params {
            nations: 6,
            ..Params::default()
        },
        12,
    );
    let rules = s.rules();
    let mut native = s.world(&mut c);
    for _ in 0..20 {
        let t = s.play_tick(&mut c);
        let input = TickInput::try_from_slice(&t.input).unwrap();
        let root = resolve_tick(&mut native, &rules, &input).unwrap();
        assert_eq!(s.root(&mut c), root, "tick {}", native.tick - 1);
        assert_eq!(s.world(&mut c), native);
    }
}

/// I6: a tick resolved in parts leaves every account byte-equal to the same
/// tick resolved whole.
#[test]
fn a_split_tick_equals_a_whole_tick() {
    let mut c = Chain::new();
    let s = SeasonFx::running(
        &mut c,
        Params {
            nations: 6,
            ..Params::default()
        },
        12,
    );
    let crank = s.crank.insecure_clone();
    for _ in 0..5 {
        s.play_tick(&mut c);
    }
    s.close(&mut c);
    s.log_input(&mut c);
    let mut split = c.fork();
    c.send(vec![s.resolve_ix(12)], &[&crank])
        .expect("ResolveTick whole");
    for to in [3u8, 4, 9, 12] {
        split
            .send(vec![s.resolve_ix(to)], &[&crank])
            .expect("ResolveTick part");
    }
    for k in s.chunks.iter().chain(&s.nations) {
        assert_eq!(c.data(k), split.data(k), "{k}");
    }
}

/// I4: after every play instruction the world's frozen/revealing flags
/// equal every nation's, and while revealing the world's deadline is the
/// nations' reveal deadline.
#[test]
fn flags_move_together() {
    let mut c = Chain::new();
    let s = SeasonFx::running(&mut c, Params::default(), 4);
    let crank = s.crank.insecure_clone();
    let check = |c: &Chain, step: &str| {
        let m = s.meta(c);
        for civ in 0..s.p.nations as usize {
            let n = s.nation(c, civ);
            assert_eq!(
                (n.frozen, n.revealing),
                (m.frozen, m.revealing),
                "{step}: nation {civ}"
            );
            if m.revealing {
                assert_eq!(n.deadline, m.deadline, "{step}: nation {civ}");
            }
        }
    };
    check(&c, "open");
    for _ in 0..3 {
        // Every held office commits and reveals.
        let mut sealed = vec![];
        for civ in 0..s.p.nations as u16 {
            let n = s.nation(&c, civ as usize);
            for role in Role::ALL
                .into_iter()
                .filter(|r| n.officers[r.index()] != NOBODY)
            {
                let who = s
                    .members
                    .iter()
                    .position(|m| m.session.pubkey().to_bytes() == n.keys[role.index()])
                    .unwrap();
                let b = OrderBatch {
                    civ,
                    tick: n.open_tick,
                    role,
                    member: n.officers[role.index()],
                    decision_digest: [1; 32],
                    orders: vec![],
                    adopt: vec![],
                };
                let sess = s.members[who].session.insecure_clone();
                let ix = s.commit_orders_ix(
                    &sess.pubkey(),
                    civ,
                    role,
                    b.tick,
                    order_commitment(&b, &[2; 32]),
                );
                c.send(vec![ix], &[&crank, &sess]).unwrap();
                check(&c, "commit");
                sealed.push(b);
            }
        }
        s.close(&mut c);
        check(&c, "close");
        for b in &sealed {
            c.send(
                vec![s.reveal_orders_ix(&crank.pubkey(), b, [2; 32])],
                &[&crank],
            )
            .unwrap();
            check(&c, "reveal");
        }
        s.log_input(&mut c);
        check(&c, "log");
        let mut from = 0;
        for to in crank_stops() {
            c.send(vec![s.resolve_ix(to)], &[&crank]).unwrap();
            check(&c, &format!("resolve {from}->{to}"));
            from = to;
        }
    }
}

/// I1 on a short season: USDC in = claims + operations + what is left, and
/// only rounding dust is left.
#[test]
fn usdc_is_conserved_with_deposits() {
    let mut c = Chain::new();
    // One deposit per season (WP12).
    let d = 4_000_000;
    let mut s = SeasonFx::create(
        &mut c,
        Params {
            deposit: d,
            ..Params::default()
        },
    );
    let deposits = [d; 4];
    for (i, d) in deposits.iter().enumerate() {
        s.register(&mut c, (i % 2) as u16, *d);
    }
    s.genesis(&mut c);
    s.seat_and_open(&mut c);
    let vault_in = 4 * s.p.fee + deposits.iter().sum::<u64>();
    assert_eq!(c.balance(&s.vault), vault_in);
    s.play_tick(&mut c);
    s.play_tick(&mut c);
    s.fast_forward_to_end(&mut c);
    let crank = s.crank.insecure_clone();
    c.send(vec![s.finish_ix(false)], &[&crank])
        .expect("FinishSeason");
    let out = s.settle(&mut c);
    println!(
        "vault in {vault_in} = claims {} + ops {} + left {}",
        out.claims, out.ops, out.left
    );
    assert_eq!(
        vault_in,
        out.claims + out.ops + out.left,
        "USDC is conserved"
    );
    assert_eq!(out.ops, s.season(&c).ops);
    assert!(
        out.left <= (s.members.len() + s.p.nations as usize) as u64,
        "only rounding dust: {}",
        out.left
    );
}
