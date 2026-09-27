//! WP15: a season is bound at creation to the rules and settlement logic
//! of the build that created it, and every instruction that applies the
//! rules refuses another build's (`RulesMismatch`); the money already owed
//! is still paid.

use permutation_chain::rules::{pinned_ruleset_hash, CHAIN_LOGIC_VERSION, PINNED_RULES_VERSION};
use permutation_chain_svm_tests::error::ChainError as E;
use permutation_chain_svm_tests::state::*;
use permutation_chain_svm_tests::*;
use solana_signer::Signer;

#[test]
fn create_season_binds_rules_and_logic() {
    let mut c = Chain::new();
    c.advance(1);
    let slot = c.svm.get_sysvar::<solana_clock::Clock>().slot;
    let s = SeasonFx::create(&mut c, Params::default());
    let season = s.season(&c);
    assert_eq!(
        (
            season.rules_version,
            season.rules_hash,
            season.logic_version
        ),
        (
            PINNED_RULES_VERSION,
            pinned_ruleset_hash(PRESET_BLITZ, true).unwrap(),
            CHAIN_LOGIC_VERSION
        )
    );
    assert_eq!(season.created_slot, slot);
    // Without the market, the other hash.
    let b = SeasonFx::create(
        &mut c,
        Params {
            id: 8,
            market: false,
            ..Params::default()
        },
    );
    assert_eq!(
        b.season(&c).rules_hash,
        pinned_ruleset_hash(PRESET_BLITZ, false).unwrap()
    );
}

/// Genesis runs the rules in SBF: the world it writes carries the pinned
/// ruleset hash (its first 32 body bytes), the host-computed one.
#[test]
fn genesis_in_sbf_matches_the_pinned_hash() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create(&mut c, Params::default());
    s.register(&mut c, 0, 0);
    s.register(&mut c, 1, 0);
    s.genesis(&mut c);
    assert_eq!(s.season(&c).status, SeasonStatus::Seating);
    let chunk0 = c.data(&s.chunks[0]);
    assert_eq!(
        chunk0[WORLD_HEADER..WORLD_HEADER + 32],
        pinned_ruleset_hash(PRESET_BLITZ, true).unwrap()
    );
}

/// Each season at the status where the instruction runs, created under
/// other rules (or other settlement logic): Register, StartSeason,
/// GenesisStep, SeatMembers and OpenGovernment refuse it. A finalized
/// season is still claimed.
#[test]
fn another_rules_hash_or_logic_halts_the_season_but_not_the_claims() {
    let other_rules = |x: &mut Season| x.rules_hash[0] ^= 1;
    let other_logic = |x: &mut Season| x.logic_version += 1;
    for (label, break_it) in [
        ("rules", &other_rules as &dyn Fn(&mut Season)),
        ("logic", &other_logic),
    ] {
        let mut c = Chain::new();
        // Registering: Register and StartSeason.
        let mut s = SeasonFx::create(&mut c, Params::default());
        s.register(&mut c, 0, 0);
        c.edit::<Season>(&s.season, |x| break_it(x));
        let m = s.new_member(&mut c, 1, s.p.fee);
        let (w, ss) = (m.wallet.insecure_clone(), m.session.insecure_clone());
        assert_err(
            c.send(vec![s.register_ix(&m, &w.pubkey(), 0)], &[&w, &ss]),
            E::RulesMismatch,
        );
        let crank = s.crank.insecure_clone();
        assert_err(
            c.send(vec![s.start_ix(&crank.pubkey())], &[&crank]),
            E::RulesMismatch,
        );
        // Genesis: GenesisStep.
        let mut g = SeasonFx::create(
            &mut c,
            Params {
                id: 8,
                ..Params::default()
            },
        );
        g.register(&mut c, 0, 0);
        let gc = g.crank.insecure_clone();
        c.send(vec![g.start_ix(&gc.pubkey())], &[&gc]).unwrap();
        g.seed(&mut c);
        c.edit::<Season>(&g.season, |x| break_it(x));
        assert_err(
            c.send(vec![g.genesis_step_ix(50)], &[&gc]),
            E::RulesMismatch,
        );
        // Seating: SeatMembers and OpenGovernment.
        let mut t = SeasonFx::create(
            &mut c,
            Params {
                id: 9,
                ..Params::default()
            },
        );
        t.register(&mut c, 0, 0);
        t.genesis(&mut c);
        let tc = t.crank.insecure_clone();
        let good = t.season(&c);
        c.edit::<Season>(&t.season, |x| break_it(x));
        assert_err(
            c.send(vec![t.seat_ix(&tc.pubkey(), &[0])], &[&tc]),
            E::RulesMismatch,
        );
        c.edit::<Season>(&t.season, |x| *x = good.clone());
        t.seat_all(&mut c);
        c.edit::<Season>(&t.season, |x| break_it(x));
        assert_err(
            c.send(vec![t.open_ix(&tc.pubkey())], &[&tc]),
            E::RulesMismatch,
        );
        // Finalized under this build, then read by another: Claim pays.
        let f = SeasonFx::running(
            &mut c,
            Params {
                id: 10,
                ..Params::default()
            },
            2,
        );
        f.fast_forward_to_end(&mut c);
        c.send(vec![f.finish_ix(false)], &[&crank])
            .expect("FinishSeason");
        c.edit::<Season>(&f.season, |x| break_it(x));
        let w = f.members[0].wallet.insecure_clone();
        let owed = permutation_chain::payout::claim_amount(&f.season(&c), &f.member(&c, 0));
        c.send(vec![f.claim_ix(0, &w.pubkey(), &f.members[0].token)], &[&w])
            .unwrap_or_else(|e| panic!("{label}: Claim still pays: {e:#?}"));
        assert_eq!(c.balance(&f.members[0].token), owed);
    }
}
