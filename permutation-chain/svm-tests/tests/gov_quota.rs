//! WP03 on the SBF build: SubmitGov takes only the nation's members, each
//! with its seated key and within its quota, so the governance inboxes are
//! bounded and every tick keeps resolving at full quota. Flips the audit's
//! governance-flood repros (fresh keys filled every inbox; after the last
//! tick anyone could still write).

use permutation_chain_svm_tests::error::ChainError as E;
use permutation_chain_svm_tests::state::*;
use permutation_chain_svm_tests::*;
use permutation_rules::gov::GovAction;
use solana_keypair::Keypair;
use solana_signer::Signer;

/// Every member fills its quota with one-slot entries (a vote-sized action)
/// in one transaction per member, as the web client packs them; one more
/// is InboxFull; an outsider's relay-shaped entry (its own key, naming a
/// member) is Unauthorized. Then the tick resolves. Returns the entries.
fn full_quota_tick(c: &mut Chain, s: &SeasonFx) -> usize {
    let crank = s.crank.insecure_clone();
    let mut entries = 0;
    for civ in 0..s.p.nations as u16 {
        let n = s.nation(c, civ as usize);
        let each = (n.gov_quota as usize).min(MAX_GOV_PER_SIGNER);
        for seat in &n.roll {
            let outsider = Keypair::new();
            assert_err(
                c.send(
                    vec![s.submit_gov_ix(
                        &outsider.pubkey(),
                        civ,
                        seat.member,
                        GovAction::Stand { roles: 1 },
                    )],
                    &[&crank, &outsider],
                ),
                E::Unauthorized,
            );
            let sess = s.members[seat.member as usize].session.insecure_clone();
            let ixs = (0..each)
                .map(|_| {
                    s.submit_gov_ix(
                        &sess.pubkey(),
                        civ,
                        seat.member,
                        GovAction::Stand { roles: 1 },
                    )
                })
                .collect();
            c.send(ixs, &[&crank, &sess])
                .expect("a member's whole quota in one transaction");
            assert_err(
                c.send(
                    vec![s.submit_gov_ix(
                        &sess.pubkey(),
                        civ,
                        seat.member,
                        GovAction::Stand { roles: 1 },
                    )],
                    &[&crank, &sess],
                ),
                E::InboxFull,
            );
            entries += each;
        }
        assert_eq!(s.nation(c, civ as usize).inbox.len(), n.roll.len() * each);
    }
    let t = s.play_tick(c);
    println!("  {entries} entries: worst {} CU", t.worst_cu);
    assert!(t.worst_cu <= CU_CEILING);
    entries
}

#[test]
fn full_quota_every_tick_keeps_resolving() {
    for members in [12, 18] {
        let mut c = Chain::new();
        let s = SeasonFx::running(
            &mut c,
            Params {
                nations: 6,
                ..Params::default()
            },
            members,
        );
        s.ensure_rolls(&mut c);
        println!("{members} members:");
        for tick in 0..3u16 {
            let before = s.world(&mut c).tick;
            assert_eq!(before, tick);
            full_quota_tick(&mut c, &s);
            assert_eq!(s.world(&mut c).tick, tick + 1, "the tick resolved");
        }
    }
}

/// Outsiders fill nothing, whatever key or member they name; after the
/// last tick nobody can write any more, and the nations' bytes stay at
/// their base size.
#[test]
fn outsiders_and_the_finished_season_write_nothing() {
    let mut c = Chain::new();
    let s = SeasonFx::running(&mut c, Params::default(), 4);
    s.ensure_rolls(&mut c);
    let crank = s.crank.insecure_clone();
    for member in [0u32, 1, 2, 3, 99, u32::MAX] {
        for civ in 0..2u16 {
            let k = Keypair::new();
            assert_err(
                c.send(
                    vec![s.submit_gov_ix(&k.pubkey(), civ, member, GovAction::Stand { roles: 1 })],
                    &[&crank, &k],
                ),
                E::Unauthorized,
            );
        }
    }
    for civ in 0..2 {
        assert!(s.nation(&c, civ).inbox.is_empty());
    }
    s.set_tick(&mut c, 179);
    s.play_tick(&mut c);
    assert!(s.meta(&c).finished);
    for civ in 0..2u16 {
        let n = s.nation(&c, civ as usize);
        for seat in n.roll.clone() {
            let sess = s.members[seat.member as usize].session.insecure_clone();
            assert_err(
                c.send(
                    vec![s.submit_gov_ix(
                        &sess.pubkey(),
                        civ,
                        seat.member,
                        GovAction::Stand { roles: 1 },
                    )],
                    &[&crank, &sess],
                ),
                E::WrongStatus,
            );
        }
        let k = Keypair::new();
        assert_err(
            c.send(
                vec![s.submit_gov_ix(&k.pubkey(), civ, 0, GovAction::Stand { roles: 1 })],
                &[&crank, &k],
            ),
            E::WrongStatus,
        );
        let data = c.data(&s.nations[civ as usize]);
        let used = data.iter().rposition(|b| *b != 0).map_or(0, |i| i + 1);
        assert!(
            used <= NationAccount::base_len(n.roll.len()),
            "nation {civ}: {used} B used"
        );
    }
}
