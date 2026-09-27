//! WP17: the session key signs its own Register (account 9), so nobody can
//! register a key they do not hold, and the fee source is the wallet's own
//! token account.

use permutation_chain_svm_tests::error::ChainError as E;
use permutation_chain_svm_tests::ix::RegisterArgs;
use permutation_chain_svm_tests::seat::substitute_key;
use permutation_chain_svm_tests::state::*;
use permutation_chain_svm_tests::*;
use solana_keypair::Keypair;
use solana_signer::Signer;

/// The facilitator F relays for A (session S) and for a front-runner B that
/// names S in its own Register: B can produce no signature of S, either by
/// passing S unsigned or by signing with its own key. A's Register lands,
/// and A is seated with S itself.
#[test]
fn facilitator_cannot_front_run_a_session_key() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create(&mut c, Params::default());
    let f = c.funded();
    let a = s.new_member(&mut c, 0, s.p.fee);
    let b = s.new_member(&mut c, 1, s.p.fee);
    let (aw, asess, bw) = (
        a.wallet.insecure_clone(),
        a.session.insecure_clone(),
        b.wallet.insecure_clone(),
    );
    let stolen = RegisterArgs {
        session: asess.pubkey().to_bytes(),
        ..s.register_args(&b, 0)
    };
    // S passed, not signing.
    let mut ix = s.register_ix_from(&b, &f.pubkey(), &stolen);
    ix.accounts[9].is_signer = false;
    assert_err(c.send(vec![ix], &[&f, &bw]), E::MissingSignature);
    // B's own key signs in S's place.
    let mut ix = s.register_ix_from(&b, &f.pubkey(), &stolen);
    ix.accounts[9].pubkey = b.session.pubkey();
    let bs = b.session.insecure_clone();
    assert_err(c.send(vec![ix], &[&f, &bw, &bs]), E::MissingSignature);
    assert_eq!(c.balance(&b.token), s.p.fee, "B paid nothing");
    assert_eq!(c.owner(&b.member), None);
    // A's own Register, relayed by F.
    c.send(vec![s.register_ix(&a, &f.pubkey(), 0)], &[&f, &aw, &asess])
        .expect("A registers");
    s.members.push(a);
    assert_eq!(s.season(&c).member_count, 1);
    s.genesis(&mut c);
    s.seat_all(&mut c);
    assert_eq!(s.world(&mut c).members[0].key, asess.pubkey().to_bytes());
}

/// A session key read from an earlier season's Member account cannot be
/// registered by anyone but its holder.
#[test]
fn a_key_from_an_earlier_season_cannot_be_reused_by_others() {
    let mut c = Chain::new();
    let mut old = SeasonFx::create(&mut c, Params::default());
    let i = old.register(&mut c, 0, 0);
    let published = old.member(&c, i).session;
    let s = SeasonFx::create(
        &mut c,
        Params {
            id: 8,
            ..Params::default()
        },
    );
    let m = s.new_member(&mut c, 0, s.p.fee);
    let w = m.wallet.insecure_clone();
    let a = RegisterArgs {
        session: published,
        ..s.register_args(&m, 0)
    };
    let mut ix = s.register_ix_from(&m, &w.pubkey(), &a);
    ix.accounts[9].is_signer = false;
    assert_err(c.send(vec![ix], &[&w]), E::MissingSignature);
    // The holder may use it again, with its signature.
    let holder = old.members[i].session.insecure_clone();
    c.send(
        vec![s.register_ix_from(&m, &w.pubkey(), &a)],
        &[&w, &holder],
    )
    .expect("the key's holder registers it again");
}

/// Liveness kept: one key holder registering two of its own wallets (the
/// key signs both). Seating completes; the second wallet is seated with its
/// substitute key, and the government opens.
#[test]
fn the_key_holder_registering_two_wallets_still_seats() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create(&mut c, Params::default());
    let key = Keypair::new();
    s.register_keyed(&mut c, 0, &key, 0x0f, [u32::MAX; 4], 0, [0; 32]);
    s.register_keyed(&mut c, 1, &key, 0x0f, [u32::MAX; 4], 0, [0; 32]);
    s.genesis(&mut c);
    let crank = s.crank.insecure_clone();
    let l = c
        .send(vec![s.seat_ix(&crank.pubkey(), &[0, 1])], &[&crank])
        .expect("SeatMembers");
    assert!(l
        .logs
        .iter()
        .any(|x| x.contains("PS member 1 seated with a substitute key")));
    let world = s.world(&mut c);
    assert_eq!(world.members[0].key, key.pubkey().to_bytes());
    assert_eq!(
        world.members[1].key,
        substitute_key(7, &s.members[1].wallet.pubkey().to_bytes())
    );
    s.open(&mut c);
    assert_eq!(s.season(&c).status, SeasonStatus::Running);
}

/// A fee payer other than the wallet is never the session key; a wallet
/// paying for itself may be its own session key.
#[test]
fn the_fee_payer_is_never_a_session_key() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create(&mut c, Params::default());
    let f = c.funded();
    let m = s.new_member(&mut c, 0, s.p.fee);
    let w = m.wallet.insecure_clone();
    let a = RegisterArgs {
        session: f.pubkey().to_bytes(),
        ..s.register_args(&m, 0)
    };
    assert_err(
        c.send(vec![s.register_ix_from(&m, &f.pubkey(), &a)], &[&f, &w]),
        E::Unauthorized,
    );
    let a = RegisterArgs {
        session: w.pubkey().to_bytes(),
        ..s.register_args(&m, 0)
    };
    c.send(vec![s.register_ix_from(&m, &w.pubkey(), &a)], &[&w])
        .expect("wallet = fee payer = session");
    s.members.push(m);
    assert_eq!(s.member(&c, 0).session, w.pubkey().to_bytes());
}

/// B, an SPL delegate of A's USDC account, cannot pay its registration from
/// it: the fee source must be owned by the registering wallet.
#[test]
fn a_delegate_cannot_fund_its_registration_from_anothers_account() {
    let mut c = Chain::new();
    let s = SeasonFx::create(&mut c, Params::default());
    let a = s.new_member(&mut c, 0, s.p.fee);
    let b = s.new_member(&mut c, 1, s.p.fee);
    // A approves B for the fee (SPL Approve: delegate and delegated amount).
    let mut d = c.data(&a.token);
    d[72..76].copy_from_slice(&1u32.to_le_bytes());
    d[76..108].copy_from_slice(b.wallet.pubkey().as_ref());
    d[121..129].copy_from_slice(&s.p.fee.to_le_bytes());
    c.set_data(&a.token, d.clone());
    let (bw, bs) = (b.wallet.insecure_clone(), b.session.insecure_clone());
    let mut ix = s.register_ix(&b, &bw.pubkey(), 0);
    ix.accounts[4].pubkey = a.token;
    assert_err(c.send(vec![ix], &[&bw, &bs]), E::WrongTokenAccount);
    assert_eq!(c.data(&a.token), d, "A's balance and allowance unchanged");
    assert_eq!(c.owner(&b.member), None);
    // From its own account, B registers.
    c.send(vec![s.register_ix(&b, &bw.pubkey(), 0)], &[&bw, &bs])
        .expect("B pays from its own account");
}

/// An honest Register with a deposit stays a Light instruction.
#[test]
fn register_cu() {
    let mut c = Chain::new();
    let d = 2_000_000;
    let s = SeasonFx::create(
        &mut c,
        Params {
            deposit: d,
            ..Params::default()
        },
    );
    let m = s.new_member(&mut c, 0, s.p.fee + d);
    let (w, ss) = (m.wallet.insecure_clone(), m.session.insecure_clone());
    let l = c
        .send(vec![s.register_ix(&m, &w.pubkey(), d)], &[&w, &ss])
        .expect("Register");
    println!("Register with a deposit: {} CU", l.cu);
    assert!(l.cu < 60_000, "{} CU", l.cu);
}
