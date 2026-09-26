//! The operator's AI roster: RevealRoster and FinishSeason's roster
//! branches (Revealed, pending, Forfeited after the grace period).

use permutation_chain_svm_tests::error::ChainError as E;
use permutation_chain_svm_tests::state::*;
use permutation_chain_svm_tests::*;
use solana_signer::Signer;

const BOUNTY: u64 = 1_000_000;
const BOND: u64 = 4_000_000;

/// Three AIs (nations 0, 1, 0) and two people, seated and open.
fn ai_season(c: &mut Chain) -> SeasonFx {
    let mut s = SeasonFx::create_ai(c, Params::default(), &[0, 1, 0], BOUNTY, BOND);
    for i in 0..3 {
        s.register_ai(c, i);
    }
    s.register(c, 0, 0);
    s.register(c, 1, 2_000_000);
    s.genesis(c);
    s.seat_and_open(c);
    s
}

#[test]
fn reveal_roster_checks() {
    let mut c = Chain::new();
    let s = ai_season(&mut c);
    s.fast_forward_to_end(&mut c);
    let anyone = c.funded();
    let m: Vec<usize> = (0..3).map(|i| s.ai[i].member.unwrap()).collect();
    let salts: Vec<[u8; 32]> = s.ai.iter().map(|a| a.salt).collect();
    assert_err(
        c.send(vec![s.reveal_roster_ix(&[], vec![])], &[&anyone]),
        E::InvalidParams,
    );
    assert_err(
        c.send(
            vec![s.reveal_roster_ix(&m[..2], vec![salts[0]])],
            &[&anyone],
        ),
        E::InvalidParams,
    );
    // More than ai_count.
    let four = [m[0], m[1], m[2], 3];
    assert_err(
        c.send(
            vec![s.reveal_roster_ix(&four, vec![salts[0]; 4])],
            &[&anyone],
        ),
        E::InvalidParams,
    );
    // A salt that does not turn the wallet into its tag.
    assert_err(
        c.send(vec![s.reveal_roster_ix(&m[..1], vec![[0; 32]])], &[&anyone]),
        E::RosterMismatch,
    );
    // The same member twice.
    assert_err(
        c.send(
            vec![s.reveal_roster_ix(&[m[0], m[0]], vec![salts[0]; 2])],
            &[&anyone],
        ),
        E::RosterMismatch,
    );
    // In roster order, in batches: complete when the tags chain to the commitment.
    c.send(vec![s.reveal_ai_ix(&[0])], &[&anyone])
        .expect("RevealRoster");
    assert_eq!(s.season(&c).roster_revealed, 1);
    c.send(vec![s.reveal_ai_ix(&[1, 2])], &[&anyone])
        .expect("RevealRoster");
    let season = s.season(&c);
    assert_eq!(
        (season.roster_revealed, season.roster_acc),
        (3, s.roster_chain())
    );
    let entries = s.roster(&c).entries;
    let want: Vec<(u32, u16, [u8; 32])> = (0..3)
        .map(|i| (m[i] as u32, s.ai[i].civ, salts[i]))
        .collect();
    let got: Vec<(u32, u16, [u8; 32])> =
        entries.iter().map(|e| (e.member, e.civ, e.salt)).collect();
    assert_eq!(got, want);
    // Finalized: nothing more to reveal.
    c.send(vec![s.finish_ix(true)], &[&anyone])
        .expect("FinishSeason");
    assert_eq!(s.season(&c).roster_outcome, ROSTER_REVEALED);
    assert_err(
        c.send(vec![s.reveal_ai_ix(&[0])], &[&anyone]),
        E::WrongStatus,
    );

    // A roster whose last reveal does not chain to the commitment.
    let mut t = SeasonFx::create_ai(
        &mut c,
        Params {
            id: 8,
            ..Params::default()
        },
        &[0, 1],
        BOUNTY,
        BOND,
    );
    t.register_ai(&mut c, 0);
    t.register_ai(&mut c, 1);
    c.edit::<Season>(&t.season, |x| x.roster_commit = [0x77; 32]);
    c.send(vec![t.reveal_ai_ix(&[0])], &[&anyone])
        .expect("RevealRoster");
    assert_err(
        c.send(vec![t.reveal_ai_ix(&[1])], &[&anyone]),
        E::RosterMismatch,
    );
}

#[test]
fn finish_season_roster_branches() {
    let mut c = Chain::new();
    let s = ai_season(&mut c);
    s.fast_forward_to_end(&mut c);
    let anyone = c.funded();
    // Revealed in full: the roster account goes with FinishSeason.
    let mut revealed = c.fork();
    revealed
        .send(vec![s.reveal_ai_ix(&[0, 1, 2])], &[&anyone])
        .unwrap();
    revealed
        .send(vec![s.finish_ix(true)], &[&anyone])
        .expect("FinishSeason, roster revealed");
    let season = s.season(&revealed);
    assert_eq!(
        (season.status, season.roster_outcome),
        (SeasonStatus::Finalized, ROSTER_REVEALED)
    );
    // Not revealed: pending through the grace period after the last tick.
    assert_err(
        c.send(vec![s.finish_ix(false)], &[&anyone]),
        E::RosterPending,
    );
    c.advance(ROSTER_GRACE_SECONDS - 1);
    assert_err(
        c.send(vec![s.finish_ix(false)], &[&anyone]),
        E::RosterPending,
    );
    // Then anyone finishes it without the roster: the bounties and the bond are forfeited.
    c.advance(1);
    c.send(vec![s.finish_ix(false)], &[&anyone])
        .expect("FinishSeason, roster forfeited");
    let season = s.season(&c);
    assert_eq!(
        (season.status, season.roster_outcome),
        (SeasonStatus::Finalized, ROSTER_FORFEITED)
    );
}

/// Fixed behaviour (WP09): only the operator reveals the roster, and only
/// once the season is over.
#[test]
#[ignore = "until WP09: RevealRoster is the operator's, after the last tick"]
fn reveal_roster_needs_the_operator_and_the_end() {
    let mut c = Chain::new();
    let s = ai_season(&mut c);
    let outsider = c.funded();
    // While Running, before the end: refused, whoever sends it. At HEAD
    // RevealRoster names no authority, so the builder cannot tell the two
    // senders apart: WP09 gives it the authority account (contract §1.3
    // tag 24) and pins the codes here with assert_err: the outsider
    // Unauthorized (before and after the end), the operator before the
    // end SeasonNotOver.
    assert!(c.send(vec![s.reveal_ai_ix(&[0])], &[&outsider]).is_err());
    let admin = s.admin.insecure_clone();
    assert!(c.send(vec![s.reveal_ai_ix(&[0])], &[&admin]).is_err());
    // After the end: the outsider is refused, the operator reveals in order.
    s.fast_forward_to_end(&mut c);
    assert!(c
        .send(vec![s.reveal_ai_ix(&[0, 1, 2])], &[&outsider])
        .is_err());
    c.send(vec![s.reveal_ai_ix(&[0, 1, 2])], &[&admin])
        .expect("the operator reveals");
    c.send(vec![s.finish_ix(true)], &[&outsider])
        .expect("FinishSeason");
    assert_eq!(s.season(&c).roster_outcome, ROSTER_REVEALED);
}

/// Fixed behaviour (WP09): a registrant whose tag is `roster_tag` of its
/// own wallet cannot reveal itself into the roster and so force Forfeited.
#[test]
#[ignore = "until WP09: a self-reveal cannot break the roster"]
fn self_reveal_cannot_break_the_roster() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create_ai(&mut c, Params::default(), &[0, 1], BOUNTY, BOND);
    s.register_ai(&mut c, 0);
    s.register_ai(&mut c, 1);
    let m = s.new_member(&mut c, 0, s.p.fee);
    let salt = [0x42; 32];
    let tag = permutation_rules::roster::roster_tag(s.p.id, &m.wallet.pubkey().to_bytes(), &salt);
    let wallet = m.wallet.insecure_clone();
    let ix = s.register_ix_with(
        &m,
        &wallet.pubkey(),
        m.session.pubkey().to_bytes(),
        0x0f,
        [u32::MAX; 4],
        0,
        tag,
    );
    c.send(vec![ix], &[&wallet]).unwrap();
    s.members.push(m);
    let griefer = s.members.len() - 1;
    s.genesis(&mut c);
    s.seat_and_open(&mut c);
    s.fast_forward_to_end(&mut c);
    assert!(c
        .send(vec![s.reveal_roster_ix(&[griefer], vec![salt])], &[&wallet])
        .is_err());
    let admin = s.admin.insecure_clone();
    c.send(vec![s.reveal_ai_ix(&[0, 1])], &[&admin])
        .expect("the operator reveals");
    c.send(vec![s.finish_ix(true)], &[&admin])
        .expect("FinishSeason");
    assert_eq!(s.season(&c).roster_outcome, ROSTER_REVEALED);
}
