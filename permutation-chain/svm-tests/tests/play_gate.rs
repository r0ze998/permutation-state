//! The play gate: no tick instruction runs before the government opens.

use permutation_chain_svm_tests::error::ChainError as E;
use permutation_chain_svm_tests::state::SeasonStatus;
use permutation_chain_svm_tests::*;
use solana_signer::Signer;

/// Fixed behaviour (WP01): in Genesis and Seating, CloseCommits,
/// LogTickInput and ResolveTick are refused (6 / 29 / 28), alone or bundled
/// in one transaction, so nobody can freeze or resolve tick 0 before the
/// members are seated.
#[test]
#[ignore = "until WP01: tick instructions are refused before OpenGovernment"]
fn seating_attacks_fail() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create(&mut c, Params::default());
    for i in 0..4 {
        s.register(&mut c, (i % 2) as u16, 0);
    }
    let crank = s.crank.insecure_clone();
    c.send(vec![s.start_ix(&crank.pubkey())], &[&crank])
        .unwrap();
    let attacker = c.funded();
    let mut genesis = c.fork();
    s.genesis_steps(&mut c);
    assert_eq!(s.season(&c).status, SeasonStatus::Seating);
    for chain in [&mut genesis, &mut c] {
        chain.advance(3600);
        assert_err(chain.send(vec![s.close_ix()], &[&attacker]), E::WrongStatus);
        assert_err(chain.send(vec![s.log_ix(0)], &[&attacker]), E::WrongPhase);
        assert_err(
            chain.send(vec![s.resolve_ix(12)], &[&attacker]),
            E::InputNotPublished,
        );
        let bundle = vec![s.close_ix(), s.log_ix(0), s.resolve_ix(12)];
        assert!(
            chain.send(bundle, &[&attacker]).is_err(),
            "the bundle never lands"
        );
    }
    // Seating then goes on as usual.
    s.seat_and_open(&mut c);
}
