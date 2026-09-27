//! WP03/WP07: Register admits at most `SEASON_MEMBER_CAP` members, and at
//! most `NATION_MEMBER_CAP` in one nation. A refused registration creates
//! no Member account and moves no USDC.

use permutation_chain_svm_tests::error::ChainError as E;
use permutation_chain_svm_tests::state::*;
use permutation_chain_svm_tests::*;
use solana_signer::Signer;

/// Registers one more member of `civ`; returns the result and the member.
fn try_register(c: &mut Chain, s: &SeasonFx, civ: u16) -> (Result<Landed, Fail>, MemberFx) {
    let m = s.new_member(c, civ, s.p.fee);
    let (w, ss) = (m.wallet.insecure_clone(), m.session.insecure_clone());
    let r = c.send(vec![s.register_ix(&m, &w.pubkey(), 0)], &[&w, &ss]);
    (r, m)
}

#[test]
fn register_refuses_past_the_season_cap() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create(
        &mut c,
        Params {
            nations: 6,
            ..Params::default()
        },
    );
    for i in 0..SEASON_MEMBER_CAP as usize {
        s.register(&mut c, (i % 6) as u16, 0);
    }
    let season = s.season(&c);
    assert_eq!(season.member_count, SEASON_MEMBER_CAP);
    let vault = c.balance(&s.vault);
    let (r, m) = try_register(&mut c, &s, 0);
    assert_err(r, E::SeasonFull);
    assert_eq!(c.owner(&m.member), None, "no Member account");
    assert_eq!((c.balance(&m.token), c.balance(&s.vault)), (s.p.fee, vault));
    assert_eq!(s.season(&c).member_count, SEASON_MEMBER_CAP);
}

#[test]
fn register_refuses_past_the_nation_cap() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create(&mut c, Params::default());
    for _ in 0..NATION_MEMBER_CAP {
        s.register(&mut c, 0, 0);
    }
    assert_eq!(s.season(&c).nation_members, vec![NATION_MEMBER_CAP, 0]);
    let (r, m) = try_register(&mut c, &s, 0);
    assert_err(r, E::SeasonFull);
    assert_eq!(c.owner(&m.member), None);
    // The other nation still has room.
    let (r, _) = try_register(&mut c, &s, 1);
    r.expect("a member of the other nation");
    assert_eq!(s.season(&c).nation_members, vec![NATION_MEMBER_CAP, 1]);
}
