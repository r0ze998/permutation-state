//! The season seed from the MagicBlock VRF (WP11 §3.2, A19–A22):
//! StartSeason closes registration (Seeding) without a request,
//! RetrySeasonSeed files the requests on the base queue, and only the VRF's
//! callback (ConsumeSeasonSeed, signed as its scoped identity) sets the seed
//! and opens genesis. The VRF is the harness's stand-in (`vrf`).

use permutation_chain::randomness::{
    self, season_seed, season_seed_request, CONSUME_SEED_TAG, REQUEST_SCOPED_HIGH_PRIORITY,
    SEED_PENDING, SEED_VRF,
};
use permutation_chain_svm_tests::error::ChainError as E;
use permutation_chain_svm_tests::state::*;
use permutation_chain_svm_tests::vrf::{Meta, Request};
use permutation_chain_svm_tests::*;
use permutation_rules::genesis::{nation_entries, new_season};
use solana_signer::Signer;

/// Four members, registration closed (Seeding), no request yet.
fn seeding(c: &mut Chain) -> SeasonFx {
    let mut s = SeasonFx::create(
        c,
        Params {
            deposit: 1_000_000,
            ..Params::default()
        },
    );
    for i in 0..4 {
        s.register(c, (i % 2) as u16, 1_000_000);
    }
    let crank = s.crank.insecure_clone();
    c.send(vec![s.start_ix(&crank.pubkey())], &[&crank])
        .expect("StartSeason");
    s
}

/// The request the season's seed is asked with: the registration totals
/// bound as the caller seed, called back with tag 31 on the season account.
fn expected_request(c: &Chain, s: &SeasonFx) -> Request {
    let season = s.season(c);
    let treasury = borsh::to_vec(&season.treasury).unwrap();
    Request {
        caller_seed: season_seed_request(
            season.season_id,
            season.member_count,
            &treasury,
            &season.prev_history_root,
        ),
        callback_program_id: c.program.to_bytes(),
        callback_discriminator: vec![CONSUME_SEED_TAG],
        callback_accounts_metas: vec![Meta {
            pubkey: s.season.to_bytes(),
            is_signer: false,
            is_writable: true,
        }],
        callback_args: season.season_id.to_le_bytes().to_vec(),
    }
}

/// While Seeding, nothing reads or moves the season: the seed is zero (no
/// transaction can observe it before the oracle answers), registration is
/// closed and genesis waits.
#[test]
fn seeding_waits_for_the_oracle() {
    let mut c = Chain::new();
    let s = seeding(&mut c);
    let season = s.season(&c);
    assert_eq!(
        (season.status, season.seed_state, season.season_seed),
        (SeasonStatus::Seeding, SEED_PENDING, [0; 32])
    );
    let crank = s.crank.insecure_clone();
    assert_err(
        c.send(vec![s.start_ix(&crank.pubkey())], &[&crank]),
        E::WrongStatus,
    );
    assert_err(
        c.send(vec![s.genesis_step_ix(50)], &[&crank]),
        E::WrongStatus,
    );
    let m = s.new_member(&mut c, 0, s.p.fee + 1_000_000);
    let (w, ss) = (m.wallet.insecure_clone(), m.session.insecure_clone());
    assert_err(
        c.send(vec![s.register_ix(&m, &w.pubkey(), 1_000_000)], &[&w, &ss]),
        E::WrongStatus,
    );
    let w0 = s.members[0].wallet.insecure_clone();
    assert_err(
        c.send(
            vec![s.update_member_ix(&w0.pubkey(), 0, 1, [u32::MAX; 4])],
            &[&w0],
        ),
        E::WrongStatus,
    );
    // The seed arrives: genesis opens.
    s.seed(&mut c);
    assert_eq!(s.season(&c).status, SeasonStatus::Genesis);
}

/// RetrySeasonSeed files the first request at once (A19), from anyone, on
/// the base queue with a scoped high-priority request (A20, A22); again
/// only `SEED_RETRY_SECONDS` after the last one.
#[test]
fn retry_season_seed_requests_on_the_base_queue() {
    let mut c = Chain::new();
    let s = seeding(&mut c);
    let anyone = c.funded();
    let a = anyone.pubkey();
    // The ER queue, another VRF program: WrongOracle, nothing changes.
    let mut ix = s.retry_seed_ix(&a);
    ix.accounts[3].pubkey = vrf::queue_er();
    assert_err(c.send(vec![ix], &[&anyone]), E::WrongOracle);
    let mut ix = s.retry_seed_ix(&a);
    ix.accounts[4].pubkey = addr(SYSTEM);
    assert_err(c.send(vec![ix], &[&anyone]), E::WrongOracle);
    assert_eq!(s.season(&c).seed_requests, 0);
    vrf::requests();
    c.send(vec![s.retry_seed_ix(&a)], &[&anyone])
        .expect("RetrySeasonSeed");
    let got = vrf::requests();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].kind, REQUEST_SCOPED_HIGH_PRIORITY);
    assert_eq!(
        (got[0].payer, got[0].queue, got[0].identity),
        (a, vrf::queue_base(), vrf::program_identity(&c.program))
    );
    assert_eq!(got[0].request, expected_request(&c, &s));
    let season = s.season(&c);
    assert_eq!((season.seed_requests, season.seed_requested_at), (1, c.now));
    // Again only after SEED_RETRY_SECONDS.
    c.advance(SEED_RETRY_SECONDS - 1);
    assert_err(c.send(vec![s.retry_seed_ix(&a)], &[&anyone]), E::TooEarly);
    c.advance(1);
    c.send(vec![s.retry_seed_ix(&a)], &[&anyone])
        .expect("the second request");
    assert_eq!(vrf::requests().len(), 1);
    let season = s.season(&c);
    assert_eq!((season.seed_requests, season.seed_requested_at), (2, c.now));
    // Not while registering, nor once the seed arrived.
    let b = SeasonFx::create(
        &mut c,
        Params {
            id: 8,
            ..Params::default()
        },
    );
    assert_err(
        c.send(vec![b.retry_seed_ix(&a)], &[&anyone]),
        E::WrongStatus,
    );
    let req = expected_request(&c, &s);
    vrf::fulfil(&mut c, &req, [9; 32]).expect("the callback");
    c.advance(SEED_RETRY_SECONDS);
    assert_err(
        c.send(vec![s.retry_seed_ix(&a)], &[&anyone]),
        E::WrongStatus,
    );
}

/// Only the VRF, signing as its scoped identity for this program, sets the
/// seed: `season_seed(E, …)`, logged as PS_SEED, and genesis opens. A
/// second callback changes nothing (and succeeds); the first answer wins.
#[test]
fn consume_season_seed_is_the_oracles() {
    let mut c = Chain::new();
    let s = seeding(&mut c);
    let anyone = c.funded();
    // Sent directly: whoever signs, it is not the VRF's scoped identity.
    assert_err(
        c.send(
            vec![s.consume_seed_ix(&anyone.pubkey(), [7; 32], 7)],
            &[&anyone],
        ),
        E::WrongOracle,
    );
    // Two requests outstanding (the second after the retry time).
    let crank = s.crank.insecure_clone();
    c.send(vec![s.retry_seed_ix(&crank.pubkey())], &[&crank])
        .unwrap();
    c.advance(SEED_RETRY_SECONDS);
    c.send(vec![s.retry_seed_ix(&crank.pubkey())], &[&crank])
        .unwrap();
    let reqs = vrf::requests();
    assert_eq!(reqs.len(), 2);
    // A callback naming another season id: refused.
    let mut other = reqs[0].request.clone();
    other.callback_args = 8u64.to_le_bytes().to_vec();
    let f = vrf::fulfil(&mut c, &other, [5; 32]).unwrap_err();
    assert_eq!(f.code, Some(E::WrongPda as u32), "{f:#?}");
    assert_eq!(s.season(&c).status, SeasonStatus::Seeding);
    // The second request's answer comes first: it is the seed.
    c.advance(4);
    let (e1, e2) = ([0x11; 32], [0x22; 32]);
    let l = vrf::fulfil(&mut c, &reqs[1].request, e2).expect("the callback");
    let season = s.season(&c);
    let treasury = borsh::to_vec(&season.treasury).unwrap();
    let seed = season_seed(
        &e2,
        7,
        season.member_count,
        &treasury,
        &season.prev_history_root,
    );
    assert_eq!(
        (season.status, season.seed_state, season.seed_oracle),
        (SeasonStatus::Genesis, SEED_VRF, e2)
    );
    assert_eq!((season.season_seed, season.stage_at), (seed, c.now));
    let r = record(&l.logs, b"PS_SEED");
    assert_eq!(r[1], 7u64.to_le_bytes().to_vec());
    assert_eq!(
        (r[2].clone(), r[3].clone(), r[4].clone()),
        (vec![SEED_VRF], e2.to_vec(), seed.to_vec())
    );
    // The first request's late answer is ignored.
    let before = c.data(&s.season);
    let l = vrf::fulfil(&mut c, &reqs[0].request, e1).expect("a stale callback succeeds");
    assert!(records(&l.logs, b"PS_SEED").is_empty());
    assert_eq!(c.data(&s.season), before);
}

/// After the seed, the first GenesisStep writes the job and the steps reach
/// Seating; the world is the rules' `new_season` under the drawn seed.
#[test]
fn genesis_runs_on_the_drawn_seed() {
    let mut c = Chain::new();
    let s = seeding(&mut c);
    let l = s.seed(&mut c);
    assert_eq!(record(&l.logs, b"PS_SEED")[2], vec![SEED_VRF]);
    s.genesis_steps(&mut c);
    let season = s.season(&c);
    assert_eq!(season.status, SeasonStatus::Seating);
    let mut entries = nation_entries(2);
    for (e, t) in entries.iter_mut().zip(&season.treasury) {
        e.treasury = *t;
    }
    let want = new_season(&s.rules(), &[1; 32], &season.season_seed, &entries).unwrap();
    assert_eq!(s.root(&mut c), want.state_root().unwrap());
    assert_eq!(randomness::SEED_VRF, season.seed_state);
}
