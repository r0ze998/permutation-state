//! The tick randomness (WP11, amendments A18–A20, A22): FreezeTick,
//! RetryTickRandomness (the VRF requests), ConsumeTickRandomness (the
//! oracle's callback, through the VRF stand-in) and the fallback.

use permutation_chain::randomness::{
    salts_hash, scoped_vrf_identity, tick_callback_args, tick_vrf, CONSUME_TICK_TAG, RAND_FALLBACK,
    RAND_NONE, RAND_PENDING, RAND_VRF,
};
use permutation_chain_svm_tests::error::ChainError as E;
use permutation_chain_svm_tests::state::*;
use permutation_chain_svm_tests::*;
use permutation_rules::gov::{GovAction, Role, NOBODY};
use permutation_rules::orders::{order_commitment, OrderBatch};
use solana_address::Address;
use solana_signer::Signer;

use permutation_chain_svm_tests::vrf::{self, queue_base, queue_er, Request};

fn pk(a: &Address) -> solana_program::pubkey::Pubkey {
    solana_program::pubkey::Pubkey::new_from_array(a.to_bytes())
}

/// The first office each nation holds: (civ, role, member index).
fn offices(c: &Chain, s: &SeasonFx) -> Vec<(u16, Role, usize)> {
    let mut out = vec![];
    for civ in 0..s.p.nations as usize {
        let n = s.nation(c, civ);
        let role = *Role::ALL
            .iter()
            .find(|r| n.officers[r.index()] != NOBODY)
            .expect("an office is held");
        let who = s
            .members
            .iter()
            .position(|m| m.session.pubkey().to_bytes() == n.keys[role.index()])
            .unwrap();
        out.push((civ as u16, role, who));
    }
    out
}

fn batch(c: &Chain, s: &SeasonFx, civ: u16, role: Role) -> OrderBatch {
    let n = s.nation(c, civ as usize);
    OrderBatch {
        civ,
        tick: n.open_tick,
        role,
        member: n.officers[role.index()],
        decision_digest: [1; 32],
        orders: vec![],
        adopt: vec![],
    }
}

fn commit(c: &mut Chain, s: &SeasonFx, who: usize, b: &OrderBatch, salt: [u8; 32]) {
    let crank = s.crank.insecure_clone();
    let sess = s.members[who].session.insecure_clone();
    let ix = s.commit_orders_ix(
        &sess.pubkey(),
        b.civ,
        b.role,
        b.tick,
        order_commitment(b, &salt),
    );
    c.send(vec![ix], &[&crank, &sess]).expect("CommitOrders");
}

fn reveal(c: &mut Chain, s: &SeasonFx, b: &OrderBatch, salt: [u8; 32]) {
    let crank = s.crank.insecure_clone();
    c.send(
        vec![s.reveal_orders_ix(&crank.pubkey(), b, salt)],
        &[&crank],
    )
    .expect("RevealOrders");
}

/// A running season whose tick 0 has one revealed batch per nation and the
/// commitments closed; returns the batches' salts in (civ, role) order.
fn revealed(c: &mut Chain, n: usize) -> (SeasonFx, Vec<(u16, u8, [u8; 32])>) {
    let s = SeasonFx::running(c, Params::default(), n);
    let mut salts = vec![];
    let mut sealed = vec![];
    for (k, (civ, role, who)) in offices(c, &s).into_iter().enumerate() {
        let b = batch(c, &s, civ, role);
        let salt = [0x40 + k as u8; 32];
        commit(c, &s, who, &b, salt);
        salts.push((civ, role.index() as u8, salt));
        sealed.push((b, salt));
    }
    s.close(c);
    for (b, salt) in &sealed {
        reveal(c, &s, b, *salt);
    }
    (s, salts)
}

/// Marks the season as played on the ER: world chunk 0 delegated
/// (`Season::delegated`, as `Delegate` records it).
fn delegated(c: &mut Chain, s: &SeasonFx) {
    c.edit::<Season>(&s.season, |x| x.delegated = all_targets(x.nations));
}

#[test]
fn freeze_tick_checks() {
    let mut c = Chain::new();
    let s = SeasonFx::running(&mut c, Params::default(), 4);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    let q = queue_base();
    // Only in the reveal window.
    assert_err(c.send(vec![s.freeze_ix(&cp, &q)], &[&crank]), E::WrongPhase);
    let (civ, role, who) = offices(&c, &s)[0];
    let b = batch(&c, &s, civ, role);
    commit(&mut c, &s, who, &b, [9; 32]);
    s.close(&mut c);
    // A commitment still unrevealed inside the window.
    assert_err(c.send(vec![s.freeze_ix(&cp, &q)], &[&crank]), E::TooEarly);
    reveal(&mut c, &s, &b, [9; 32]);
    let mut unsigned = s.freeze_ix(&cp, &q);
    unsigned.accounts[0].is_signer = false;
    let anyone = c.funded();
    assert_err(c.send(vec![unsigned], &[&anyone]), E::MissingSignature);
    // Nations of different ticks.
    let mut apart = c.fork();
    apart.edit::<NationAccount>(&s.nations[1], |n| n.open_tick = 4);
    assert_err(
        apart.send(vec![s.freeze_ix(&cp, &q)], &[&crank]),
        E::WrongTick,
    );
    // Anyone, once every commitment was revealed: frozen, pending, no request.
    let l = c
        .send(vec![s.freeze_ix(&anyone.pubkey(), &q)], &[&anyone])
        .expect("FreezeTick");
    assert!(
        vrf::requests().is_empty(),
        "FreezeTick makes no request (A19)"
    );
    let m = s.meta(&c);
    let salts = vec![(civ, role.index() as u8, [9; 32])];
    assert_eq!(
        (m.frozen, m.revealing, m.rand_state, m.rand_tick),
        (true, true, RAND_PENDING, 0)
    );
    assert_eq!(m.rand_pre, salts_hash(7, 0, &salts));
    assert_eq!(
        (m.frozen_at, m.rand_requested_at, m.rand_requests, m.vrf),
        (c.now, 0, 0, [0; 32])
    );
    for civ in 0..2 {
        assert!(s.nation(&c, civ).frozen);
    }
    let r = record(&l.logs, b"PS_FREEZE");
    assert_eq!(r[1], 0u16.to_le_bytes().to_vec());
    assert_eq!(r[2], m.rand_pre.to_vec());
    let lapsed: Vec<(u16, u8, u32)> = borsh::from_slice(&r[3]).unwrap();
    assert!(lapsed.is_empty());
    // Once per tick.
    assert_err(c.send(vec![s.freeze_ix(&cp, &q)], &[&crank]), E::WrongPhase);
    println!("FreezeTick (2 nations): {} CU", l.cu);
    assert!(l.cu < 400_000);
}

/// Wrong oracle accounts are refused before anything is written.
#[test]
fn oracle_accounts_are_checked() {
    let mut c = Chain::new();
    let (s, _) = revealed(&mut c, 4);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    let good = s.freeze_ix(&cp, &queue_base());
    let before = c.data(&s.chunks[0]);
    for (k, what) in [
        (2, "identity"),
        (3, "queue"),
        (4, "VRF program"),
        (5, "system program"),
        (6, "SlotHashes"),
    ] {
        let mut ix = good.clone();
        ix.accounts[k].pubkey = Address::new_unique();
        assert_err(c.send(vec![ix], &[&crank]), E::WrongOracle);
        assert_eq!(c.data(&s.chunks[0]), before, "{what}: nothing written");
    }
    let mut readonly_queue = good.clone();
    readonly_queue.accounts[3].is_writable = false;
    assert_err(c.send(vec![readonly_queue], &[&crank]), E::WrongOracle);
    // The season account must be this season's.
    let mut other = good.clone();
    let last = other.accounts.len() - 1;
    other.accounts[last].pubkey = s.chunks[3];
    assert!(c.send(vec![other], &[&crank]).is_err());
    assert!(!s.meta(&c).frozen && s.meta(&c).revealing);
    c.send(vec![good], &[&crank]).expect("FreezeTick");
}

/// A20 and A10: the queue is the one of the play mode the chain recorded. A
/// season played on the base layer (the production build, nothing
/// delegated) freezes and draws from `VRF_QUEUE_BASE`; the ER queue is
/// refused there, and the other way round once chunk 0 is delegated.
#[test]
fn queue_bound_to_the_play_mode() {
    // Base play: FreezeTick and the request on the base queue.
    let mut c = Chain::new();
    let (s, _) = revealed(&mut c, 4);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    assert_eq!(s.vrf_queue(&c), queue_base());
    assert_err(
        c.send(vec![s.freeze_ix(&cp, &queue_er())], &[&crank]),
        E::WrongOracle,
    );
    let mut er = c.fork();
    c.send(
        vec![
            s.freeze_ix(&cp, &queue_base()),
            s.retry_ix(&cp, &queue_base()),
        ],
        &[&crank],
    )
    .expect("FreezeTick + RetryTickRandomness on base");
    let got = vrf::requests();
    assert_eq!(got.len(), 1);
    let r = &got[0];
    assert_eq!(r.queue, queue_base());
    assert_eq!(r.kind, 11, "scoped, high priority (A22)");
    assert_eq!(r.payer, cp);
    assert_eq!(r.identity, vrf::program_identity(&s.program));
    let m = s.meta(&c);
    assert_eq!(r.request.caller_seed, m.rand_pre);
    assert_eq!(r.request.callback_program_id, s.program.to_bytes());
    assert_eq!(r.request.callback_discriminator, vec![CONSUME_TICK_TAG]);
    assert_eq!(
        r.request.callback_accounts_metas,
        vec![vrf::Meta {
            pubkey: s.chunks[0].to_bytes(),
            is_signer: false,
            is_writable: true
        }]
    );
    assert_eq!(r.request.callback_args, tick_callback_args(7, 0).to_vec());
    assert_eq!((m.rand_requests, m.rand_requested_at), (1, c.now));
    // The base queue's answer draws the tick, and the tick plays on.
    let e = [0x77; 32];
    vrf::fulfil(&mut c, &r.request, e).expect("the base oracle's callback");
    assert_eq!(s.meta(&c).vrf, tick_vrf(&m.rand_pre, &e, RAND_VRF).unwrap());
    s.log_input(&mut c);
    resolve_parts(&mut c, &s, &crank, false);
    assert_eq!(s.world(&mut c).tick, 1);
    // ER play: the base queue is refused, the ER queue is used; a retry
    // through the wrong queue at the give-up time reaches no fallback.
    delegated(&mut er, &s);
    assert_eq!(s.vrf_queue(&er), queue_er());
    assert_err(
        er.send(vec![s.freeze_ix(&cp, &queue_base())], &[&crank]),
        E::WrongOracle,
    );
    er.send(vec![s.freeze_ix(&cp, &queue_er())], &[&crank])
        .expect("FreezeTick on the ER");
    er.advance(VRF_GIVEUP_SECONDS);
    assert_err(
        er.send(vec![s.retry_ix(&cp, &queue_base())], &[&crank]),
        E::WrongOracle,
    );
    assert_eq!(s.meta(&er).rand_state, RAND_PENDING);
    er.send(vec![s.retry_ix(&cp, &queue_er())], &[&crank])
        .expect("RetryTickRandomness on the ER");
    assert_eq!(vrf::requests()[0].queue, queue_er());
}

#[test]
fn retry_tick_randomness_checks() {
    let mut c = Chain::new();
    let (s, _) = revealed(&mut c, 4);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    let q = queue_base();
    // Nothing frozen: nothing to request.
    assert_err(c.send(vec![s.retry_ix(&cp, &q)], &[&crank]), E::WrongPhase);
    c.send(vec![s.freeze_ix(&cp, &q)], &[&crank])
        .expect("FreezeTick");
    assert_err(
        c.send(vec![s.retry_ix(&cp, &queue_er())], &[&crank]),
        E::WrongOracle,
    );
    // The first request at once (FreezeTick made none), permissionless.
    let anyone = c.funded();
    c.send(vec![s.retry_ix(&anyone.pubkey(), &q)], &[&anyone])
        .expect("RetryTickRandomness");
    assert_eq!(vrf::requests()[0].payer, anyone.pubkey());
    // The next one only VRF_RETRY_SECONDS later.
    c.advance(VRF_RETRY_SECONDS - 1);
    assert_err(c.send(vec![s.retry_ix(&cp, &q)], &[&crank]), E::TooEarly);
    c.advance(1);
    c.send(vec![s.retry_ix(&cp, &q)], &[&crank])
        .expect("RetryTickRandomness");
    let m = s.meta(&c);
    assert_eq!((m.rand_requests, m.rand_requested_at), (2, c.now));
    // Drawn: no more requests.
    s.fulfil_tick(&mut c, [5; 32]);
    assert_err(c.send(vec![s.retry_ix(&cp, &q)], &[&crank]), E::WrongPhase);
}

/// A18: the callback is authenticated by the scoped identity and the
/// account alone; a stale or duplicate one changes nothing.
#[test]
fn callback_authenticated() {
    let mut c = Chain::new();
    let (s, _) = revealed(&mut c, 4);
    s.freeze_and_request(&mut c);
    let m = s.meta(&c);
    let request = s.tick_request(&c);
    // A key that is not the VRF's scoped identity for this program.
    let k = c.funded();
    assert_err(
        c.send(vec![s.consume_ix(&k.pubkey(), [1; 32], 7, 0)], &[&k]),
        E::WrongOracle,
    );
    // The scoped identity is a PDA of the VRF program: nobody else signs as it.
    let identity = Address::new_from_array(scoped_vrf_identity(&pk(&s.program)).to_bytes());
    let mut unsigned = s.consume_ix(&identity, [1; 32], 7, 0);
    unsigned.accounts[0].is_signer = false;
    assert_err(c.send(vec![unsigned], &[&k]), E::WrongOracle);
    // The oracle's callback naming another season, or another account.
    let wrong_season = Request {
        callback_args: tick_callback_args(8, 0).to_vec(),
        ..request.clone()
    };
    let r = vrf::fulfil(&mut c, &wrong_season, [1; 32]);
    assert!(
        r.as_ref()
            .is_err_and(|f| f.code == Some(E::WrongWorld as u32)),
        "{r:?}"
    );
    let mut wrong_account = request.clone();
    wrong_account.callback_accounts_metas[0].pubkey = s.chunks[1].to_bytes();
    let r = vrf::fulfil(&mut c, &wrong_account, [1; 32]);
    assert!(
        r.as_ref()
            .is_err_and(|f| f.code == Some(E::WrongWorld as u32)),
        "{r:?}"
    );
    assert_eq!(s.meta(&c), m, "nothing written");
    // Another tick's answer: ignored, and it succeeds (the oracle stops).
    let stale = Request {
        callback_args: tick_callback_args(7, 1).to_vec(),
        ..request.clone()
    };
    let l = vrf::fulfil(&mut c, &stale, [1; 32]).expect("stale callback lands");
    assert!(l
        .logs
        .iter()
        .any(|x| x.contains("PS stale randomness ignored")));
    assert_eq!(s.meta(&c), m);
    // The answer.
    let e = [0x3c; 32];
    let l = vrf::fulfil(&mut c, &request, e).expect("the oracle's callback");
    let ours = format!("Program {} invoke [2]", s.program);
    assert!(l.logs.iter().any(|x| x == &ours), "a CPI at stack height 2");
    let vrf_out = tick_vrf(&m.rand_pre, &e, RAND_VRF).unwrap();
    let m = s.meta(&c);
    assert_eq!((m.rand_state, m.rand_out, m.vrf), (RAND_VRF, e, vrf_out));
    let r = record(&l.logs, b"PS_RAND");
    assert_eq!(
        r[1..],
        [
            0u16.to_le_bytes().to_vec(),
            vec![RAND_VRF],
            e.to_vec(),
            vrf_out.to_vec()
        ]
    );
    println!(
        "ConsumeTickRandomness (with the stand-in's CPI): {} CU",
        l.cu
    );
    assert!(l.cu < 50_000);
    // A duplicate: ignored.
    vrf::fulfil(&mut c, &request, [0x3d; 32]).expect("duplicate callback lands");
    assert_eq!(s.meta(&c).rand_out, e);
}

/// Two requests outstanding: the first answer wins, the second is a no-op.
#[test]
fn first_callback_wins() {
    let mut c = Chain::new();
    let (s, _) = revealed(&mut c, 4);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    let q = s.vrf_queue(&c);
    s.freeze_and_request(&mut c);
    let first = s.tick_request(&c);
    c.advance(VRF_RETRY_SECONDS);
    c.send(vec![s.retry_ix(&cp, &q)], &[&crank])
        .expect("RetryTickRandomness");
    let second = s.tick_request(&c);
    assert_eq!(first, second, "the same seed and callback");
    vrf::fulfil(&mut c, &second, [2; 32]).expect("second answer");
    vrf::fulfil(&mut c, &first, [1; 32]).expect("first answer, late");
    let m = s.meta(&c);
    assert_eq!((m.rand_out, m.rand_requests), ([2; 32], 2));
}

/// The fallback needs an unanswered request and `VRF_GIVEUP_SECONDS` since
/// the freeze; it is logged, a late answer changes nothing, and PS_TICK
/// carries its state.
#[test]
fn fallback_only_after_giveup() {
    let mut c = Chain::new();
    let (s, _) = revealed(&mut c, 4);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    let q = s.vrf_queue(&c);
    c.send(vec![s.freeze_ix(&cp, &q)], &[&crank])
        .expect("FreezeTick");
    let frozen_at = s.meta(&c).frozen_at;
    // Nobody asked yet: at the give-up time the first request is made.
    let mut unasked = c.fork();
    unasked.set_time(frozen_at + VRF_GIVEUP_SECONDS);
    unasked
        .send(vec![s.retry_ix(&cp, &q)], &[&crank])
        .expect("first request");
    let m = s.meta(&unasked);
    assert_eq!((m.rand_state, m.rand_requests), (RAND_PENDING, 1));
    vrf::requests();
    // Asked at once, and again before the give-up time: still pending.
    c.send(vec![s.retry_ix(&cp, &q)], &[&crank])
        .expect("first request");
    c.set_time(frozen_at + VRF_GIVEUP_SECONDS - VRF_RETRY_SECONDS);
    c.send(vec![s.retry_ix(&cp, &q)], &[&crank])
        .expect("second request");
    assert_eq!(s.meta(&c).rand_state, RAND_PENDING);
    c.set_time(frozen_at + VRF_GIVEUP_SECONDS - 1);
    assert_err(c.send(vec![s.retry_ix(&cp, &q)], &[&crank]), E::TooEarly);
    c.set_time(frozen_at + VRF_GIVEUP_SECONDS);
    let l = c
        .send(vec![s.retry_ix(&cp, &q)], &[&crank])
        .expect("RetryTickRandomness: the fallback");
    let m = s.meta(&c);
    let fallback = tick_vrf(&m.rand_pre, &[0; 32], RAND_FALLBACK).unwrap();
    assert_eq!(
        (m.rand_state, m.rand_out, m.vrf, m.rand_requests),
        (RAND_FALLBACK, [0; 32], fallback, 2)
    );
    let asked = vrf::requests();
    assert_eq!(asked.len(), 2, "the fallback makes no request");
    let r = record(&l.logs, b"PS_RAND");
    assert_eq!(
        r[1..],
        [
            0u16.to_le_bytes().to_vec(),
            vec![RAND_FALLBACK],
            vec![0; 32],
            fallback.to_vec()
        ]
    );
    assert!(l.logs.iter().any(|x| x.contains("fallback")));
    // A late answer changes nothing; the tick plays on the fallback.
    vrf::fulfil(&mut c, &asked[1].request, [9; 32]).expect("late answer");
    assert_eq!(s.meta(&c).vrf, fallback);
    s.log_input(&mut c);
    let l = c
        .send(vec![s.resolve_ix(12)], &[&crank])
        .expect("ResolveTick");
    let t = record(&l.logs, b"PS_TICK");
    assert_eq!(
        (t[6].clone(), t[7].clone(), t[8].clone()),
        (vec![RAND_FALLBACK], m.rand_pre.to_vec(), vec![0; 32])
    );
    assert_eq!(s.world(&mut c).tick, 1);
}

/// After FreezeTick the input cannot change.
#[test]
fn frozen_input_immutable() {
    let mut c = Chain::new();
    let s = SeasonFx::running(&mut c, Params::default(), 4);
    s.ensure_rolls(&mut c);
    let crank = s.crank.insecure_clone();
    let (civ, role, who) = offices(&c, &s)[0];
    let b = batch(&c, &s, civ, role);
    commit(&mut c, &s, who, &b, [1; 32]);
    s.close(&mut c);
    c.set_time(s.meta(&c).deadline);
    s.freeze_and_request(&mut c);
    let sess = s.members[who].session.insecure_clone();
    assert_err(
        c.send(
            vec![s.reveal_orders_ix(&crank.pubkey(), &b, [1; 32])],
            &[&crank],
        ),
        E::TickFrozen,
    );
    assert_err(
        c.send(
            vec![s.commit_orders_ix(&sess.pubkey(), civ, role, 0, [2; 32])],
            &[&crank, &sess],
        ),
        E::TickFrozen,
    );
    assert_err(
        c.send(
            vec![s.submit_gov_ix(
                &sess.pubkey(),
                civ,
                who as u32,
                GovAction::Stand { roles: 1 },
            )],
            &[&crank, &sess],
        ),
        E::TickFrozen,
    );
}

/// Auditor scenario B: an office that commits and withholds its reveal
/// after reading the other salts cannot steer the tick. The values it could
/// compute (the v8 derivation, with or without its salt) are not the tick's
/// randomness; the lapse is logged; the freeze changes only `frozen` in the
/// nation account; its next commitment is accepted.
#[test]
fn withholding_no_longer_steers() {
    let mut c = Chain::new();
    let s = SeasonFx::running(&mut c, Params::default(), 4);
    let office = offices(&c, &s);
    let (hc, hr, hw) = office[0];
    let (ac, ar, aw) = office[1];
    let honest = batch(&c, &s, hc, hr);
    let attacker = batch(&c, &s, ac, ar);
    let (hs, ats) = ([0x11; 32], [0x22; 32]);
    commit(&mut c, &s, hw, &honest, hs);
    commit(&mut c, &s, aw, &attacker, ats);
    let pre_root = s.root(&mut c);
    s.close(&mut c);
    reveal(&mut c, &s, &honest, hs);
    let before: Vec<Vec<u8>> = s.nations.iter().map(|k| c.data(k)).collect();
    c.set_time(s.meta(&c).deadline);
    let l = s.freeze_and_request(&mut c);
    let r = record(&l.logs, b"PS_FREEZE");
    let lapsed: Vec<(u16, u8, u32)> = borsh::from_slice(&r[3]).unwrap();
    assert_eq!(lapsed, vec![(ac, ar.index() as u8, attacker.member)]);
    for (k, key) in s.nations.iter().enumerate() {
        let (a, b) = (&before[k], &c.data(key));
        let differ: Vec<usize> = (0..a.len()).filter(|i| a[*i] != b[*i]).collect();
        let n: NationAccount = load(b).unwrap();
        assert!(n.frozen);
        assert_eq!(
            differ.len(),
            1,
            "nation {k}: only `frozen` changed: {differ:?}"
        );
    }
    s.fulfil_tick(&mut c, [0x5e; 32]);
    let vrf_now = s.meta(&c).vrf;
    let with = permutation_server::replay::tick_vrf_v8(
        &pre_root,
        &[(hc, hr.index() as u8, hs), (ac, ar.index() as u8, ats)],
    );
    let without = permutation_server::replay::tick_vrf_v8(&pre_root, &[(hc, hr.index() as u8, hs)]);
    assert!(vrf_now != with && vrf_now != without);
    s.log_input(&mut c);
    resolve_parts(&mut c, &s, &s.crank, false);
    // No lapse rule: the attacker's office commits again next tick.
    let next = batch(&c, &s, ac, ar);
    commit(&mut c, &s, aw, &next, [3; 32]);
}

/// After a full tick the randomness is reset and the next tick freezes.
#[test]
fn resolve_resets_randomness() {
    let mut c = Chain::new();
    let s = SeasonFx::running(&mut c, Params::default(), 4);
    s.play_tick(&mut c);
    let m = s.meta(&c);
    assert_eq!(
        (
            m.rand_state,
            m.rand_tick,
            m.rand_pre,
            m.rand_out,
            m.rand_requests,
            m.frozen_at
        ),
        (RAND_NONE, NO_TICK, [0; 32], [0; 32], 0, 0)
    );
    s.close(&mut c);
    s.freeze(&mut c);
    assert_eq!(s.meta(&c).rand_tick, 1);
}

/// FreezeTick and its request fit the client's light profile with six
/// nations (six nation accounts, the season).
#[test]
fn freeze_fits_with_six_nations() {
    let mut c = Chain::new();
    let s = SeasonFx::running(
        &mut c,
        Params {
            nations: 6,
            ..Params::default()
        },
        12,
    );
    s.close(&mut c);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    let q = s.vrf_queue(&c);
    let need = c
        .need(&[s.freeze_ix(&cp, &q), s.retry_ix(&cp, &q)], &[&crank])
        .expect("FreezeTick + RetryTickRandomness lands");
    vrf::requests();
    assert_fits(
        "6 nations: FreezeTick + RetryTickRandomness",
        need,
        Budget::Light,
    );
    let size = tx_size(&[s.freeze_ix(&cp, &q), s.retry_ix(&cp, &q)], &cp);
    println!("[FreezeTick, RetryTickRandomness] with 6 nations: {size} B");
    assert!(size <= 1232);
}
