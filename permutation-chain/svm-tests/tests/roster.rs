//! The operator's AI roster (WP09, WP10): RevealRoster (the operator's,
//! after the last tick, restartable, the blinded commitment) and
//! FinishSeason's roster branches (Revealed, pending, Forfeited after the
//! grace period with the equal split), with the AI members' refunds capped.

use permutation_chain_svm_tests::error::ChainError as E;
use permutation_chain_svm_tests::payout::claim_amount;
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
    s.register(c, 1, 0);
    s.genesis(c);
    s.seat_and_open(c);
    s
}

/// The member indices and salts of AIs `ai`.
fn members_and_salts(s: &SeasonFx, ai: &[usize]) -> (Vec<usize>, Vec<[u8; 32]>) {
    (
        ai.iter().map(|a| s.ai[*a].member.unwrap()).collect(),
        ai.iter().map(|a| s.ai[*a].salt).collect(),
    )
}

#[test]
fn reveal_roster_checks() {
    let mut c = Chain::new();
    let s = ai_season(&mut c);
    let admin = s.admin.insecure_clone();
    let ap = admin.pubkey();
    let blind = s.roster_blind();
    let (m, salts) = members_and_salts(&s, &[0, 1, 2]);
    // Not over yet: the world can still be played.
    assert_err(
        c.send(vec![s.reveal_ai_ix(&[0])], &[&admin]),
        E::SeasonNotOver,
    );
    s.fast_forward_to_end(&mut c);
    // The operator only, and it signs.
    let outsider = c.funded();
    assert_err(
        c.send(
            vec![s.reveal_roster_ix(&outsider.pubkey(), 0, &m[..1], salts[..1].to_vec(), blind)],
            &[&outsider],
        ),
        E::Unauthorized,
    );
    let mut ix = s.reveal_ai_ix(&[0]);
    ix.accounts[0].is_signer = false;
    assert_err(c.send(vec![ix], &[&outsider]), E::MissingSignature);
    // The final world on base: chunk 0 still delegated, or another chunk.
    let mut on_er = c.fork();
    on_er.set_owner(&s.chunks[0], addr(DLP));
    assert_err(
        on_er.send(vec![s.reveal_ai_ix(&[0])], &[&admin]),
        E::WrongWorld,
    );
    let mut ix = s.reveal_ai_ix(&[0]);
    ix.accounts[3].pubkey = s.chunks[1];
    assert_err(c.send(vec![ix], &[&admin]), E::WrongPda);
    // `from` is 0 (restart) or the count revealed so far.
    assert_err(
        c.send(
            vec![s.reveal_roster_ix(&ap, 1, &m[..1], salts[..1].to_vec(), blind)],
            &[&admin],
        ),
        E::InvalidParams,
    );
    assert_err(
        c.send(
            vec![s.reveal_roster_ix(&ap, 0, &[], vec![], blind)],
            &[&admin],
        ),
        E::InvalidParams,
    );
    assert_err(
        c.send(
            vec![s.reveal_roster_ix(&ap, 0, &m[..2], salts[..1].to_vec(), blind)],
            &[&admin],
        ),
        E::InvalidParams,
    );
    // More than ai_count.
    let four = [m[0], m[1], m[2], 3];
    assert_err(
        c.send(
            vec![s.reveal_roster_ix(&ap, 0, &four, vec![salts[0]; 4], blind)],
            &[&admin],
        ),
        E::InvalidParams,
    );
    // A salt that does not turn the wallet into its tag.
    assert_err(
        c.send(
            vec![s.reveal_roster_ix(&ap, 0, &m[..1], vec![[0; 32]], blind)],
            &[&admin],
        ),
        E::RosterMismatch,
    );
    // The same member twice.
    assert_err(
        c.send(
            vec![s.reveal_roster_ix(&ap, 0, &[m[0], m[0]], vec![salts[0]; 2], blind)],
            &[&admin],
        ),
        E::RosterMismatch,
    );
    // In roster order, in batches: complete when the tags chain to the
    // blinded commitment; the blind is stored.
    c.send(vec![s.reveal_ai_ix(&[0])], &[&admin])
        .expect("RevealRoster");
    assert_eq!(s.season(&c).roster_revealed, 1);
    assert_err(
        c.send(
            vec![s.reveal_roster_ix(&ap, 1, &m[..1], salts[..1].to_vec(), blind)],
            &[&admin],
        ),
        E::RosterMismatch,
    );
    c.send(vec![s.reveal_ai_ix(&[1, 2])], &[&admin])
        .expect("RevealRoster");
    let season = s.season(&c);
    assert_eq!(
        (
            season.roster_revealed,
            season.roster_acc,
            season.roster_blind
        ),
        (3, s.roster_chain(), blind)
    );
    let entries = s.roster(&c).entries;
    let want: Vec<(u32, u16, [u8; 32], u64)> = (0..3)
        .map(|i| (m[i] as u32, s.ai[i].civ, salts[i], 0))
        .collect();
    let got: Vec<(u32, u16, [u8; 32], u64)> = entries
        .iter()
        .map(|e| (e.member, e.civ, e.salt, e.shares))
        .collect();
    assert_eq!(got, want);
    // Complete is final, even a restart.
    assert_err(
        c.send(vec![s.reveal_ai_ix(&[0, 1, 2])], &[&admin]),
        E::WrongStatus,
    );
    c.send(vec![s.finish_ix(true)], &[&outsider])
        .expect("FinishSeason");
    assert_eq!(s.season(&c).roster_outcome, ROSTER_REVEALED);
    assert_err(
        c.send(vec![s.reveal_ai_ix(&[0])], &[&admin]),
        E::WrongStatus,
    );
    // A season still registering.
    let t = SeasonFx::create_ai(
        &mut c,
        Params {
            id: 8,
            ..Params::default()
        },
        &[0, 1],
        BOUNTY,
        BOND,
    );
    let tp = t.admin.insecure_clone();
    assert_err(
        c.send(
            vec![t.reveal_roster_ix(&tp.pubkey(), 0, &[], vec![[0; 32]], [0; 32])],
            &[&tp],
        ),
        E::WrongStatus,
    );
}

/// An operator mistake (a wrong order, a wrong blind) fails without
/// leaving partial state, and a restart from 0 recovers it.
#[test]
fn a_wrong_order_or_blind_is_redone_from_zero() {
    let mut c = Chain::new();
    let s = ai_season(&mut c);
    s.fast_forward_to_end(&mut c);
    let admin = s.admin.insecure_clone();
    let ap = admin.pubkey();
    let blind = s.roster_blind();
    // AIs 1 and 0 swapped: the batch lands, the completing one cannot.
    let (m, salts) = members_and_salts(&s, &[1, 0]);
    c.send(
        vec![s.reveal_roster_ix(&ap, 0, &m, salts, blind)],
        &[&admin],
    )
    .expect("RevealRoster, out of order");
    let (m2, s2) = members_and_salts(&s, &[2]);
    assert_err(
        c.send(
            vec![s.reveal_roster_ix(&ap, 2, &m2, s2.clone(), blind)],
            &[&admin],
        ),
        E::RosterMismatch,
    );
    assert_eq!(s.season(&c).roster_revealed, 2, "nothing half-written");
    // Right order, wrong blind.
    let (m, salts) = members_and_salts(&s, &[0, 1, 2]);
    assert_err(
        c.send(
            vec![s.reveal_roster_ix(&ap, 0, &m, salts.clone(), [0x99; 32])],
            &[&admin],
        ),
        E::RosterMismatch,
    );
    // Redone from zero.
    c.send(
        vec![s.reveal_roster_ix(&ap, 0, &m, salts, blind)],
        &[&admin],
    )
    .expect("RevealRoster from 0");
    assert_eq!(s.roster(&c).entries.len(), 3);
    c.send(vec![s.finish_ix(true)], &[&admin])
        .expect("FinishSeason");
    assert_eq!(s.season(&c).roster_outcome, ROSTER_REVEALED);
}

#[test]
fn finish_season_roster_branches() {
    let mut c = Chain::new();
    let s = ai_season(&mut c);
    s.fast_forward_to_end(&mut c);
    let anyone = c.funded();
    let admin = s.admin.insecure_clone();
    // Revealed in full: the roster account goes with FinishSeason.
    let mut revealed = c.fork();
    revealed
        .send(vec![s.reveal_ai_ix(&[0, 1, 2])], &[&admin])
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
    // Then anyone finishes it without the roster: the bounties and the bond
    // are forfeited and the whole pool is split equally (WP09).
    c.advance(1);
    c.send(vec![s.finish_ix(false)], &[&anyone])
        .expect("FinishSeason, roster forfeited");
    let season = s.season(&c);
    assert_eq!(
        (season.status, season.roster_outcome),
        (SeasonStatus::Finalized, ROSTER_FORFEITED)
    );
    assert!(season.payouts.iter().all(|p| *p == season.payouts[0]));
    assert!(season.payouts[0] > 0 && !season.voided);
}

/// Audit repro `roster_grief.rs` flip (WP09): an outsider cannot touch the
/// reveal; the operator's reveal a minute after the end settles Revealed.
#[test]
fn the_operators_reveal_after_the_end_settles_revealed() {
    let mut c = Chain::new();
    let s = ai_season(&mut c);
    s.fast_forward_to_end(&mut c);
    c.advance(60);
    let outsider = c.funded();
    let (m, salts) = members_and_salts(&s, &[0]);
    assert_err(
        c.send(
            vec![s.reveal_roster_ix(&outsider.pubkey(), 0, &m, salts, [0; 32])],
            &[&outsider],
        ),
        E::Unauthorized,
    );
    let crank = s.crank.insecure_clone();
    let mut ix = s.reveal_ai_ix(&[0, 1, 2]);
    ix.accounts[0].pubkey = crank.pubkey();
    c.send(vec![ix], &[&crank]).expect("the crank reveals");
    c.send(vec![s.finish_ix(true)], &[&outsider])
        .expect("FinishSeason");
    assert_eq!(s.season(&c).roster_outcome, ROSTER_REVEALED);
}

/// Audit repro `roster_grief.rs` flip (WP09): a reveal that never
/// completes stops nothing: FinishSeason forfeits at the end of the grace
/// period with the equal split.
#[test]
fn a_stuck_reveal_forfeits_after_the_grace_period() {
    let mut c = Chain::new();
    let s = ai_season(&mut c);
    s.fast_forward_to_end(&mut c);
    let admin = s.admin.insecure_clone();
    c.send(vec![s.reveal_ai_ix(&[0])], &[&admin])
        .expect("a partial reveal");
    let anyone = c.funded();
    let end = s.meta(&c).deadline;
    c.set_time(end + ROSTER_GRACE_SECONDS - 1);
    assert_err(
        c.send(vec![s.finish_ix(false)], &[&anyone]),
        E::RosterPending,
    );
    c.set_time(end + ROSTER_GRACE_SECONDS);
    c.send(vec![s.finish_ix(false)], &[&anyone])
        .expect("FinishSeason, forfeited");
    let season = s.season(&c);
    assert_eq!(season.roster_outcome, ROSTER_FORFEITED);
    assert!(season.payouts.iter().all(|p| *p == season.payouts[0]));
}

/// Fixed behaviour (WP09): only the operator reveals the roster, and only
/// once the season is over.
#[test]
fn reveal_roster_needs_the_operator_and_the_end() {
    let mut c = Chain::new();
    let s = ai_season(&mut c);
    let outsider = c.funded();
    let op = outsider.pubkey();
    let (m, salts) = members_and_salts(&s, &[0, 1, 2]);
    let blind = s.roster_blind();
    let by_outsider = |s: &SeasonFx| s.reveal_roster_ix(&op, 0, &m, salts.clone(), blind);
    // While Running, before the end: the outsider is Unauthorized, the
    // operator SeasonNotOver.
    assert_err(c.send(vec![by_outsider(&s)], &[&outsider]), E::Unauthorized);
    let admin = s.admin.insecure_clone();
    assert_err(
        c.send(vec![s.reveal_ai_ix(&[0])], &[&admin]),
        E::SeasonNotOver,
    );
    // After the end: the outsider is still refused, the operator reveals.
    s.fast_forward_to_end(&mut c);
    assert_err(c.send(vec![by_outsider(&s)], &[&outsider]), E::Unauthorized);
    c.send(vec![s.reveal_ai_ix(&[0, 1, 2])], &[&admin])
        .expect("the operator reveals");
    c.send(vec![s.finish_ix(true)], &[&outsider])
        .expect("FinishSeason");
    assert_eq!(s.season(&c).roster_outcome, ROSTER_REVEALED);
}

/// Fixed behaviour (WP09): a registrant whose tag is `roster_tag` of its
/// own wallet cannot reveal itself into the roster and so force Forfeited.
#[test]
fn self_reveal_cannot_break_the_roster() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create_ai(&mut c, Params::default(), &[0, 1], BOUNTY, BOND);
    s.register_ai(&mut c, 0);
    s.register_ai(&mut c, 1);
    let salt = [0x42; 32];
    let m = s.new_member(&mut c, 0, s.p.fee);
    let wallet = m.wallet.insecure_clone();
    let tag = permutation_rules::roster::roster_tag(s.p.id, &wallet.pubkey().to_bytes(), &salt);
    // Operator-paid, as every join of an AI season; the session key signs
    // too (WP17).
    let payer = s.registration_payer(&wallet);
    let ix = s.register_ix_with(
        &m,
        &payer.pubkey(),
        m.session.pubkey().to_bytes(),
        0x0f,
        [u32::MAX; 4],
        0,
        tag,
    );
    let session = m.session.insecure_clone();
    c.send(vec![ix], &[&payer, &wallet, &session]).unwrap();
    s.members.push(m);
    let griefer = s.members.len() - 1;
    s.genesis(&mut c);
    s.seat_and_open(&mut c);
    s.fast_forward_to_end(&mut c);
    assert_err(
        c.send(
            vec![s.reveal_roster_ix(&wallet.pubkey(), 0, &[griefer], vec![salt], [0; 32])],
            &[&wallet],
        ),
        E::Unauthorized,
    );
    let admin = s.admin.insecure_clone();
    c.send(vec![s.reveal_ai_ix(&[0, 1])], &[&admin])
        .expect("the operator reveals");
    c.send(vec![s.finish_ix(true)], &[&admin])
        .expect("FinishSeason");
    assert_eq!(s.season(&c).roster_outcome, ROSTER_REVEALED);
}

/// Two AIs of nation 0 and one person in nation 1, each depositing `dep`
/// (the season's deposit, WP12); nation 1's treasury then flows to nation
/// 0, so the AI nation ends with more than it deposited.
fn drained_into_the_ai_nation(c: &mut Chain, dep: u64) -> SeasonFx {
    let p = Params {
        deposit: dep,
        ..Params::default()
    };
    let mut s = SeasonFx::create_ai(c, p, &[0, 0], BOUNTY, BOND);
    for i in 0..2 {
        s.register_ai(c, i);
    }
    s.register(c, 1, dep);
    s.genesis(c);
    s.seat_and_open(c);
    s.fast_forward_to_end(c);
    let chunks = s.chunks.clone();
    c.with_world(&chunks, |w| {
        let mut st = w.read_world().unwrap();
        let moved = st.civs[1].usdc;
        st.civs[1].usdc = 0;
        st.civs[0].usdc += moved;
        w.write_world(&st).unwrap();
    });
    s
}

/// WP10 C2: a revealed AI member gets at most its deposit back (inside its
/// payout); the gain it would have shared goes to people.
#[test]
fn finish_and_claim_cap_revealed_ai_refunds() {
    let mut c = Chain::new();
    let dep = 3_000_000;
    let s = drained_into_the_ai_nation(&mut c, dep);
    let admin = s.admin.insecure_clone();
    c.send(vec![s.reveal_ai_ix(&[0, 1])], &[&admin])
        .expect("RevealRoster");
    c.send(vec![s.finish_ix(true)], &[&admin])
        .expect("FinishSeason");
    let season = s.season(&c);
    assert!(!season.voided);
    for a in 0..2 {
        let i = s.ai[a].member.unwrap();
        assert!(season.refund_in_payout[i / 8] & (1 << (i % 8)) != 0);
        let claim = claim_amount(&season, &s.member(&c, i));
        assert!(claim <= dep, "AI {a} claims {claim}, its deposit {dep}");
    }
    let out = s.settle(&mut c);
    assert!(out.left <= 8, "only dust stays: {}", out.left);
}

/// WP10: a forfeited roster caps every treasury refund at the deposit
/// (the AI wallets are unknown); the excess joins the equal split.
#[test]
fn forfeit_caps_refunds() {
    let mut c = Chain::new();
    let dep = 3_000_000;
    let s = drained_into_the_ai_nation(&mut c, dep);
    c.advance(ROSTER_GRACE_SECONDS);
    let anyone = c.funded();
    c.send(vec![s.finish_ix(false)], &[&anyone])
        .expect("FinishSeason, forfeited");
    let season = s.season(&c);
    assert_eq!(season.roster_outcome, ROSTER_FORFEITED);
    assert_eq!(season.treasury_final[0], season.treasury[0]);
    for i in 0..s.members.len() {
        let claim = claim_amount(&season, &s.member(&c, i));
        assert!(claim <= season.payouts[i] + dep);
    }
    let out = s.settle(&mut c);
    assert!(out.left <= 8, "only dust stays: {}", out.left);
}

/// WP09: twelve salts in one RevealRoster fit a packet and the Light
/// profile (CU < 150,000).
#[test]
fn reveal_roster_of_twelve_fits_a_transaction() {
    let mut c = Chain::new();
    let civs: Vec<u16> = (0..12).map(|i| i % 2).collect();
    let mut s = SeasonFx::create_ai(&mut c, Params::default(), &civs, BOUNTY, BOND);
    for i in 0..12 {
        s.register_ai(&mut c, i);
    }
    s.register(&mut c, 0, 0);
    s.genesis(&mut c);
    s.seat_and_open(&mut c);
    s.fast_forward_to_end(&mut c);
    let admin = s.admin.insecure_clone();
    let all: Vec<usize> = (0..12).collect();
    let ix = s.reveal_ai_ix(&all);
    let size = tx_size(std::slice::from_ref(&ix), &admin.pubkey());
    assert!(size <= PACKET_DATA_SIZE, "{size} B");
    let l = c.send(vec![ix], &[&admin]).expect("RevealRoster, 12 salts");
    println!("RevealRoster, 12 salts: {} CU, {size} B", l.cu);
    assert!(l.cu < 150_000, "{} CU", l.cu);
    assert_eq!(s.season(&c).roster_revealed, 12);
}
