//! The ER play instructions: sealed orders (CommitOrders, CloseCommits,
//! RevealOrders), SubmitGov, LogTickInput, ResolveTick, AnchorTalk and the
//! retired SubmitOrders.

use permutation_chain_svm_tests::error::ChainError as E;
use permutation_chain_svm_tests::state::*;
use permutation_chain_svm_tests::*;
use permutation_rules::gov::{GovAction, Role, NOBODY};
use permutation_rules::orders::{order_commitment, Order, OrderBatch};
use sha2::{Digest, Sha256};
use solana_keypair::Keypair;
use solana_signer::Signer;

/// Offices held after the first election: (civ, role, member index).
fn held(c: &Chain, s: &SeasonFx) -> Vec<(u16, Role, usize)> {
    let mut out = vec![];
    for civ in 0..s.p.nations as usize {
        let n = s.nation(c, civ);
        for role in Role::ALL {
            let i = role.index();
            if n.officers[i] != NOBODY {
                let who = s
                    .members
                    .iter()
                    .position(|m| m.session.pubkey().to_bytes() == n.keys[i])
                    .expect("seated with its own session key");
                out.push((civ as u16, role, who));
            }
        }
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

/// CommitOrders from the office holder's session key (the crank pays).
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

/// From an open reveal window: past its deadline, publish and resolve.
fn finish_tick(c: &mut Chain, s: &SeasonFx) {
    let crank = s.crank.insecure_clone();
    c.set_time(s.meta(c).deadline.max(c.now));
    s.log_input(c);
    resolve_parts(c, s, &crank, false);
}

#[test]
fn commit_orders_checks() {
    let mut c = Chain::new();
    let s = SeasonFx::running(&mut c, Params::default(), 4);
    let crank = s.crank.insecure_clone();
    let (civ, role, who) = held(&c, &s)[0];
    let sess = s.members[who].session.insecure_clone();
    let b = batch(&c, &s, civ, role);
    let salt = [0x5a; 32];
    let sealed = order_commitment(&b, &salt);
    let outsider = c.funded();
    assert_err(
        c.send(
            vec![s.commit_orders_ix(&outsider.pubkey(), civ, role, 0, sealed)],
            &[&outsider],
        ),
        E::Unauthorized,
    );
    // The holder's key on another nation's office.
    let other = 1 - civ;
    assert_err(
        c.send(
            vec![s.commit_orders_ix(&sess.pubkey(), other, role, 0, sealed)],
            &[&crank, &sess],
        ),
        E::Unauthorized,
    );
    assert_err(
        c.send(
            vec![s.commit_orders_ix(&sess.pubkey(), civ, role, 5, sealed)],
            &[&crank, &sess],
        ),
        E::WrongTick,
    );
    assert_err(
        c.send(
            vec![s.commit_orders_ix(&sess.pubkey(), civ, role, 0, [0; 32])],
            &[&crank, &sess],
        ),
        E::InvalidParams,
    );
    // A later commitment replaces an earlier one.
    c.send(
        vec![s.commit_orders_ix(&sess.pubkey(), civ, role, 0, [7; 32])],
        &[&crank, &sess],
    )
    .expect("CommitOrders");
    c.send(
        vec![s.commit_orders_ix(&sess.pubkey(), civ, role, 0, sealed)],
        &[&crank, &sess],
    )
    .expect("CommitOrders");
    let n = s.nation(&c, civ as usize);
    assert_eq!(
        (n.commits[role.index()], n.committed[role.index()]),
        (sealed, 0)
    );
    // Closed: the reveal window is open.
    s.close(&mut c);
    assert_err(
        c.send(
            vec![s.commit_orders_ix(&sess.pubkey(), civ, role, 0, sealed)],
            &[&crank, &sess],
        ),
        E::WrongPhase,
    );
    // Frozen: the input is being published.
    c.send(
        vec![s.reveal_orders_ix(&outsider.pubkey(), &b, salt)],
        &[&outsider],
    )
    .unwrap();
    c.send(vec![s.log_ix(0)], &[&crank]).unwrap();
    assert_err(
        c.send(
            vec![s.commit_orders_ix(&sess.pubkey(), civ, role, 0, sealed)],
            &[&crank, &sess],
        ),
        E::TickFrozen,
    );
}

#[test]
fn close_commits_checks() {
    let mut c = Chain::new();
    let s = SeasonFx::running(&mut c, Params::default(), 4);
    let crank = s.crank.insecure_clone();
    let (civ, role, who) = held(&c, &s)[0];
    let b = batch(&c, &s, civ, role);
    commit(&mut c, &s, who, &b, [3; 32]);
    assert_err(c.send(vec![s.close_ix()], &[&crank]), E::TooEarly);
    c.set_time(s.meta(&c).deadline);
    let mut ix = s.close_ix();
    ix.accounts.swap(WORLD_CHUNKS, WORLD_CHUNKS + 1);
    assert_err(c.send(vec![ix], &[&crank]), E::MissingNation);
    // Permissionless; every nation and the world enter the reveal window.
    let anyone = c.funded();
    let l = c
        .send(vec![s.close_ix()], &[&anyone])
        .expect("CloseCommits");
    let until = c.now + reveal_seconds(30);
    let m = s.meta(&c);
    assert_eq!((m.revealing, m.frozen, m.deadline), (true, false, until));
    for civ in 0..2 {
        let n = s.nation(&c, civ);
        assert_eq!((n.revealing, n.reveal_deadline), (true, until));
    }
    let r = record(&l.logs, b"PS_COMMITS");
    assert_eq!(r[1], 0u16.to_le_bytes().to_vec());
    let listed: Vec<(u16, u8, u32, [u8; 32])> = borsh::from_slice(&r[2]).unwrap();
    let officer = s.nation(&c, civ as usize).officers[role.index()];
    assert_eq!(
        listed,
        vec![(
            civ,
            role.index() as u8,
            officer,
            order_commitment(&b, &[3; 32])
        )]
    );
    assert_err(c.send(vec![s.close_ix()], &[&crank]), E::WrongPhase);
    finish_tick(&mut c, &s);
    // The last tick resolved: nothing closes any more.
    s.fast_forward_to_end(&mut c);
    assert_err(c.send(vec![s.close_ix()], &[&crank]), E::WrongPhase);
}

#[test]
fn reveal_orders_checks() {
    let mut c = Chain::new();
    let s = SeasonFx::running(&mut c, Params::default(), 4);
    let offices = held(&c, &s);
    assert!(offices.len() >= 4, "{offices:?}");
    let outsider = c.funded();
    let o = outsider.pubkey();
    let reveal = |c: &mut Chain, b: &OrderBatch, salt| {
        c.send(vec![s.reveal_orders_ix(&o, b, salt)], &[&outsider])
    };

    // Tick 0: the commit, the window, the salt; anyone may carry a reveal.
    let (civ, role, who) = offices[0];
    let b = batch(&c, &s, civ, role);
    let salt = [0x5a; 32];
    commit(&mut c, &s, who, &b, salt);
    assert_err(reveal(&mut c, &b, salt), E::WrongPhase);
    s.close(&mut c);
    assert_err(reveal(&mut c, &b, [9; 32]), E::CommitMismatch);
    let (civ2, role2, _) = offices[1];
    let uncommitted = batch(&c, &s, civ2, role2);
    assert_err(reveal(&mut c, &uncommitted, salt), E::WrongTick);
    reveal(&mut c, &b, salt).expect("RevealOrders");
    let n = s.nation(&c, civ as usize);
    let i = role.index();
    assert_eq!(
        (n.batches[i].clone(), n.salts[i], n.submitted[i]),
        (Some(b.clone()), salt, 0)
    );
    finish_tick(&mut c, &s);

    // Tick 1: batches that match their commitments but break the rules.
    let zero = OrderBatch {
        decision_digest: [0; 32],
        ..batch(&c, &s, offices[0].0, offices[0].1)
    };
    let first = (offices[0].0, offices[0].1);
    let (civ, role, who) = *offices[1..].iter().find(|x| x.1 != Role::Science).unwrap();
    let foreign = OrderBatch {
        orders: vec![Order::SetResearch { techs: vec![] }],
        ..batch(&c, &s, civ, role)
    };
    let adopt = s.rules().max_open_proposals as u32 + 1;
    let (civ3, role3, who3) = *offices
        .iter()
        .find(|x| (x.0, x.1) != (civ, role) && (x.0, x.1) != first)
        .unwrap();
    let many = OrderBatch {
        adopt: (0..adopt).collect(),
        ..batch(&c, &s, civ3, role3)
    };
    commit(&mut c, &s, offices[0].2, &zero, salt);
    commit(&mut c, &s, who, &foreign, salt);
    commit(&mut c, &s, who3, &many, salt);
    s.close(&mut c);
    assert_err(reveal(&mut c, &zero, salt), E::Rules);
    assert_err(reveal(&mut c, &foreign, salt), E::WrongOffice);
    assert_err(reveal(&mut c, &many, salt), E::OverBudget);
    finish_tick(&mut c, &s);

    // Tick 2: after the reveal window the input is frozen for reveals.
    let (civ, role, who) = offices[0];
    let late = batch(&c, &s, civ, role);
    assert_eq!(late.tick, 2);
    commit(&mut c, &s, who, &late, salt);
    s.close(&mut c);
    c.set_time(s.nation(&c, civ as usize).reveal_deadline + 1);
    assert_err(reveal(&mut c, &late, salt), E::TickFrozen);
    finish_tick(&mut c, &s);
}

#[test]
fn submit_gov_checks() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create(&mut c, Params::default());
    for i in 0..4 {
        s.register(&mut c, (i % 2) as u16, 0);
    }
    s.genesis(&mut c);
    s.seat_all(&mut c);
    let crank = s.crank.insecure_clone();
    let sess = s.members[0].session.insecure_clone();
    let stand = GovAction::Stand { roles: 1 };
    let gov = |c: &mut Chain, k: &Keypair| {
        c.send(
            vec![s.submit_gov_ix(&k.pubkey(), 0, 0, stand.clone())],
            &[&crank, k],
        )
    };
    // Before the government opens.
    assert_err(gov(&mut c, &sess), E::WrongStatus);
    s.open(&mut c);
    gov(&mut c, &sess).expect("SubmitGov");
    let n = s.nation(&c, 0);
    assert_eq!(n.inbox.len(), 1);
    assert_eq!(
        (n.inbox[0].member, n.inbox[0].signer),
        (0, sess.pubkey().to_bytes())
    );
    // At most MAX_GOV_PER_SIGNER per signer and tick.
    for _ in 1..MAX_GOV_PER_SIGNER {
        gov(&mut c, &sess).expect("SubmitGov");
    }
    assert_err(gov(&mut c, &sess), E::InboxFull);
    // Fresh keys fill the inbox up to the room kept for the reveals.
    let mut entries = MAX_GOV_PER_SIGNER;
    'keys: loop {
        let k = Keypair::new();
        for _ in 0..MAX_GOV_PER_SIGNER {
            match gov(&mut c, &k) {
                Ok(_) => entries += 1,
                Err(f) => {
                    assert_err(Err(f), E::InboxFull);
                    break 'keys;
                }
            }
        }
        assert!(entries < 2000);
    }
    println!("inbox full at {entries} entries");
    assert_eq!(s.nation(&c, 0).inbox.len(), entries);
    // Governance closes with the commitments.
    s.close(&mut c);
    assert_err(gov(&mut c, &Keypair::new()), E::TickFrozen);
}

#[test]
fn log_tick_input_checks() {
    let mut c = Chain::new();
    let s = SeasonFx::running(&mut c, Params::default(), 4);
    let crank = s.crank.insecure_clone();
    assert_err(c.send(vec![s.log_ix(1)], &[&crank]), E::InputNotPublished);
    assert_err(c.send(vec![s.log_ix(0)], &[&crank]), E::WrongPhase);
    let (civ, role, who) = held(&c, &s)[0];
    let b = batch(&c, &s, civ, role);
    commit(&mut c, &s, who, &b, [4; 32]);
    s.close(&mut c);
    // A commitment is not revealed and the window is open.
    assert_err(c.send(vec![s.log_ix(0)], &[&crank]), E::TooEarly);
    c.send(
        vec![s.reveal_orders_ix(&crank.pubkey(), &b, [4; 32])],
        &[&crank],
    )
    .unwrap();
    // Every commitment revealed: chunk 0 freezes the input at once (permissionless).
    let root = s.root(&mut c);
    let anyone = c.funded();
    let l = c.send(vec![s.log_ix(0)], &[&anyone]).expect("LogTickInput");
    let m = s.meta(&c);
    assert!(m.frozen && m.vrf != [0; 32]);
    assert_eq!((m.input_chunks, m.input_logged), (1, 1));
    for civ in 0..2 {
        assert!(s.nation(&c, civ).frozen);
    }
    let salts = record(&l.logs, b"PS_SALTS");
    assert_eq!(
        (salts[1].clone(), salts[2].clone()),
        (0u16.to_le_bytes().to_vec(), root.to_vec())
    );
    let input = record(&l.logs, b"PS_INPUT");
    assert_eq!(
        input[1..4],
        [
            0u16.to_le_bytes().to_vec(),
            0u16.to_le_bytes().to_vec(),
            1u16.to_le_bytes().to_vec()
        ]
    );
    assert_eq!(
        input[4],
        Sha256::digest(&input[5]).to_vec(),
        "the input's hash"
    );
    assert!(!log_truncated(&l.logs) && log_bytes(&l.logs) <= 10_000);
    assert_err(c.send(vec![s.log_ix(5)], &[&crank]), E::InvalidParams);
    resolve_parts(&mut c, &s, &crank, false);
    s.fast_forward_to_end(&mut c);
    assert_err(c.send(vec![s.log_ix(0)], &[&crank]), E::WrongStatus);
}

#[test]
fn resolve_tick_checks() {
    let mut c = Chain::new();
    let s = SeasonFx::running(&mut c, Params::default(), 4);
    let crank = s.crank.insecure_clone();
    assert_err(
        c.send(vec![s.resolve_ix(12)], &[&crank]),
        E::InputNotPublished,
    );
    s.close(&mut c);
    assert_err(
        c.send(vec![s.resolve_ix(12)], &[&crank]),
        E::InputNotPublished,
    );
    let (input, _) = s.log_input(&mut c);
    // A world chunk the program does not own.
    c.set_owner(&s.chunks[7], addr(SYSTEM));
    assert_err(c.send(vec![s.resolve_ix(12)], &[&crank]), E::WrongWorld);
    c.set_owner(&s.chunks[7], c.program);
    // In the crank's parts; each logs PS_TICK (pre root → post root, input hash).
    let mut root = s.root(&mut c);
    let anyone = c.funded();
    let mut from = 0;
    for to in crank_stops() {
        let l = c
            .send(vec![s.resolve_ix(to)], &[&anyone])
            .expect("ResolveTick");
        let t = record(&l.logs, b"PS_TICK");
        assert_eq!(
            (t[1].clone(), t[2].clone()),
            (0u16.to_le_bytes().to_vec(), vec![to])
        );
        assert_eq!(
            t[3],
            root.to_vec(),
            "part {from}->{to} starts at the stored root"
        );
        root = s.root(&mut c);
        assert_eq!(t[4], root.to_vec());
        assert_eq!(t[5], Sha256::digest(&input).to_vec());
        from = to;
    }
    // The tick completed: nations reopen for tick 1 with a new deadline.
    assert_eq!(s.world(&mut c).tick, 1);
    let m = s.meta(&c);
    assert_eq!(
        (
            m.frozen,
            m.revealing,
            m.finished,
            m.deadline,
            m.input_chunks
        ),
        (false, false, false, c.now + 30, 0)
    );
    for civ in 0..2 {
        let n = s.nation(&c, civ);
        assert_eq!(
            (n.open_tick, n.frozen, n.revealing, n.inbox.len()),
            (1, false, false, 0)
        );
    }
    // The last tick finishes the season.
    s.set_tick(&mut c, 179);
    s.play_tick(&mut c);
    assert!(s.meta(&c).finished);
    assert_eq!(s.world(&mut c).tick, 180);
    assert_err(c.send(vec![s.resolve_ix(12)], &[&crank]), E::WrongStatus);
}

#[test]
fn anchor_talk_checks() {
    let mut c = Chain::new();
    let s = SeasonFx::running(&mut c, Params::default(), 2);
    let crank = s.crank.insecure_clone();
    let l = c
        .send(
            vec![s.anchor_talk_ix(&crank.pubkey(), 3, 17, [8; 32])],
            &[&crank],
        )
        .expect("AnchorTalk");
    let t = record(&l.logs, b"PS_TALK");
    assert_eq!(
        t[1..],
        [
            7u64.to_le_bytes().to_vec(),
            3u16.to_le_bytes().to_vec(),
            17u32.to_le_bytes().to_vec(),
            vec![8; 32]
        ]
    );
    let outsider = c.funded();
    assert_err(
        c.send(
            vec![s.anchor_talk_ix(&outsider.pubkey(), 3, 17, [8; 32])],
            &[&outsider],
        ),
        E::Unauthorized,
    );
    let mut ix = s.anchor_talk_ix(&crank.pubkey(), 3, 17, [8; 32]);
    ix.accounts[0].is_signer = false;
    assert_err(c.send(vec![ix], &[&outsider]), E::MissingSignature);
}

#[test]
fn submit_orders_is_retired() {
    let mut c = Chain::new();
    let s = SeasonFx::running(&mut c, Params::default(), 2);
    let (civ, role, who) = held(&c, &s)[0];
    let crank = s.crank.insecure_clone();
    let sess = s.members[who].session.insecure_clone();
    assert_err(
        c.send(
            vec![s.submit_orders_ix(&sess.pubkey(), civ, role, 0, vec![])],
            &[&crank, &sess],
        ),
        E::Retired,
    );
}
