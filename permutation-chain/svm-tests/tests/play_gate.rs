//! The play gate (WP01): no tick instruction runs before the government
//! opens; tick 0's clock starts on the layer that plays it (StartClock);
//! commitments and governance close at the deadline; the tick instructions
//! that publish or resolve are alone in their transaction; the nations stay
//! frozen after the last tick; a world mixed from different writes is
//! refused.

use permutation_chain_svm_tests::error::ChainError as E;
use permutation_chain_svm_tests::instruction::ChainInstruction as I;
use permutation_chain_svm_tests::state::*;
use permutation_chain_svm_tests::vrf::{self, program_identity, queue_base, vrf_program};
use permutation_chain_svm_tests::*;
use permutation_rules::gov::{GovAction, Role, NOBODY};
use permutation_rules::hex::Hex;
use permutation_rules::orders::{order_commitment, Order, OrderBatch};
use solana_compute_budget_interface::ComputeBudgetInstruction;
use solana_signer::Signer;

/// StartSeason, and the first GenesisStep: the genesis job is in the world
/// and the season is in `Genesis`. With a StartSeason that asks the VRF
/// for the season seed (`Seeding`, unit P2), the seed is requested
/// (`RetrySeasonSeed`) and answered by the stand-in first.
fn to_genesis(c: &mut Chain, s: &SeasonFx) {
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    c.send(vec![s.start_ix(&cp)], &[&crank])
        .expect("StartSeason");
    if s.season(c).status == SeasonStatus::Seeding {
        let retry = s.ix(
            &I::RetrySeasonSeed,
            vec![
                ws(&cp),
                w(&s.season),
                r(&program_identity(&s.program)),
                w(&queue_base()),
                r(&vrf_program()),
                r(&addr(SYSTEM)),
                r(&addr(SLOT_HASHES)),
            ],
        );
        c.send(vec![retry], &[&crank]).expect("RetrySeasonSeed");
        let asked = vrf::requests().pop().expect("a season seed request");
        vrf::fulfil(c, &asked.request, [0x5e; 32]).expect("the season seed");
    }
    c.send(vec![s.genesis_step_ix(1)], &[&crank])
        .expect("GenesisStep");
    assert_eq!(s.season(c).status, SeasonStatus::Genesis);
}

/// Every tick instruction an outsider can send, refused before the
/// government opens (`genesis`: a genesis job in the world).
fn attack(c: &mut Chain, s: &SeasonFx, genesis: bool) {
    let attacker = c.funded();
    let ap = attacker.pubkey();
    let before: Vec<Vec<u8>> = s.chunks.iter().map(|k| c.data(k)).collect();
    assert_err(c.send(vec![s.close_ix()], &[&attacker]), E::WrongStatus);
    assert_err(c.send(vec![s.log_ix(0)], &[&attacker]), E::WrongPhase);
    assert_err(
        c.send(vec![s.resolve_ix(12)], &[&attacker]),
        E::InputNotPublished,
    );
    assert_err(
        c.send(vec![s.resolve_ix(DEGRADED)], &[&attacker]),
        E::InputNotPublished,
    );
    let freeze = c.send(vec![s.freeze_ix(&ap, &queue_base())], &[&attacker]);
    let clock = c.send(vec![s.start_clock_ix(&ap)], &[&attacker]);
    if genesis {
        assert_err(freeze, E::WrongWorld);
        assert_err(clock, E::WrongStatus);
    } else {
        assert_err(freeze, E::WrongPhase);
        assert_err(clock, E::Unauthorized);
    }
    let bundle = vec![s.close_ix(), s.log_ix(0), s.resolve_ix(12)];
    assert!(
        c.send(bundle, &[&attacker]).is_err(),
        "the bundle never lands"
    );
    let after: Vec<Vec<u8>> = s.chunks.iter().map(|k| c.data(k)).collect();
    assert!(before == after, "nothing was written");
}

/// Fixed behaviour (WP01): in Genesis and Seating, CloseCommits,
/// LogTickInput and ResolveTick are refused (6 / 29 / 28), alone or bundled
/// in one transaction, so nobody can freeze or resolve tick 0 before the
/// members are seated.
#[test]
fn seating_attacks_fail() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create(&mut c, Params::default());
    for i in 0..4 {
        s.register(&mut c, (i % 2) as u16, 0);
    }
    to_genesis(&mut c, &s);
    let mut genesis = c.fork();
    genesis.advance(3600);
    attack(&mut genesis, &s, true);
    s.genesis_steps(&mut c);
    assert_eq!(s.season(&c).status, SeasonStatus::Seating);
    c.advance(3600);
    attack(&mut c, &s, false);
    // Every member seated, the government not open yet: still refused.
    s.seat_all(&mut c);
    attack(&mut c, &s, false);
    assert_eq!(s.world(&mut c).phase_cursor, 0);
    // The government then opens as usual.
    s.open(&mut c);
    assert_eq!(s.season(&c).status, SeasonStatus::Running);
}

/// The deadline OpenGovernment gives tick 0 (`TICK0_GRACE_SECONDS` after a
/// full tick), written on the world and every nation where it is earlier:
/// an OpenGovernment without the grace (before unit P2) is driven as if it
/// had it. With the grace in place this changes nothing.
fn tick0_grace(c: &mut Chain, s: &SeasonFx, opened: i64) -> i64 {
    let d = opened + 30 + TICK0_GRACE_SECONDS;
    let chunks = s.chunks.clone();
    c.with_world(&chunks, |w| {
        let mut m = w.meta().unwrap();
        m.deadline = m.deadline.max(d);
        w.set_meta(&m).unwrap();
    });
    for k in &s.nations {
        c.edit::<NationAccount>(k, |n| n.deadline = n.deadline.max(d));
    }
    d
}

#[test]
fn tick0_clock() {
    let mut c = Chain::new();
    let s = SeasonFx::running(&mut c, Params::default(), 4);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    let opened = c.now;
    let grace = tick0_grace(&mut c, &s, opened);
    assert_err(c.send(vec![s.close_ix()], &[&crank]), E::TooEarly);
    let mut fallback = c.fork();
    let outsider = c.funded();
    assert_err(
        c.send(vec![s.start_clock_ix(&outsider.pubkey())], &[&outsider]),
        E::Unauthorized,
    );
    // The crank starts the clock once the accounts are where tick 0 plays.
    c.advance(40);
    c.send(vec![s.start_clock_ix(&cp)], &[&crank])
        .expect("StartClock");
    let d = c.now + 30;
    assert_eq!(s.meta(&c).deadline, d);
    for civ in 0..2 {
        assert_eq!(s.nation(&c, civ).deadline, d);
    }
    // A second StartClock never moves the deadline later.
    c.advance(5);
    c.send(vec![s.start_clock_ix(&cp)], &[&crank])
        .expect("StartClock again");
    assert_eq!(s.meta(&c).deadline, d);
    s.play_tick(&mut c);
    assert_err(c.send(vec![s.start_clock_ix(&cp)], &[&crank]), E::WrongTick);
    // Without StartClock, tick 0 closes after the grace.
    fallback.set_time(grace - 1);
    assert_err(fallback.send(vec![s.close_ix()], &[&crank]), E::TooEarly);
    fallback.set_time(grace);
    fallback
        .send(vec![s.close_ix()], &[&crank])
        .expect("CloseCommits after the grace");
}

/// Commitments and governance close at the deadline itself, before anyone
/// sent CloseCommits.
#[test]
fn commit_deadline() {
    let mut c = Chain::new();
    let s = SeasonFx::running(&mut c, Params::default(), 4);
    s.ensure_rolls(&mut c);
    let crank = s.crank.insecure_clone();
    let n = s.nation(&c, 0);
    let role = *Role::ALL
        .iter()
        .find(|r| n.officers[r.index()] != NOBODY)
        .unwrap();
    let member = n.officers[role.index()];
    let who = s
        .members
        .iter()
        .position(|m| m.session.pubkey().to_bytes() == n.keys[role.index()])
        .unwrap();
    let sess = s.members[who].session.insecure_clone();
    let b = OrderBatch {
        civ: 0,
        tick: 0,
        role,
        member,
        decision_digest: [1; 32],
        orders: vec![],
        adopt: vec![],
    };
    let commit = s.commit_orders_ix(&sess.pubkey(), 0, role, 0, order_commitment(&b, &[1; 32]));
    let gov = s.submit_gov_ix(&sess.pubkey(), 0, who as u32, GovAction::Stand { roles: 1 });
    let d = s.meta(&c).deadline;
    let mut early = c.fork();
    early.set_time(d - 1);
    early
        .send(vec![commit.clone()], &[&crank, &sess])
        .expect("CommitOrders 1 s before the deadline");
    early
        .send(vec![gov.clone()], &[&crank, &sess])
        .expect("SubmitGov 1 s before the deadline");
    c.set_time(d);
    assert_err(
        c.send(vec![commit.clone()], &[&crank, &sess]),
        E::WrongPhase,
    );
    assert_err(c.send(vec![gov], &[&crank, &sess]), E::TickFrozen);
    assert!(c
        .send(vec![commit, s.close_ix()], &[&crank, &sess])
        .is_err());
    assert_eq!(s.nation(&c, 0).committed[role.index()], NO_TICK);
}

/// CloseCommits, LogTickInput and ResolveTick are alone in their
/// transaction (compute-budget instructions aside), so no record is dropped
/// at the log cap and nothing reads their outcome in the same transaction.
/// A chunk-0 LogTickInput with 24 salts and a full 5000-byte chunk, behind
/// four compute-budget instructions, fits the log.
#[test]
fn alone_rule() {
    let mut c = Chain::new();
    let s = SeasonFx::running(
        &mut c,
        Params {
            nations: 6,
            ..Params::default()
        },
        12,
    );
    s.ensure_rolls(&mut c);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    assert_err(
        c.send(vec![s.close_ix(), s.close_ix()], &[&crank]),
        E::NotAlone,
    );
    // Every office seals and every member fills its governance quota.
    let mut sealed = vec![];
    for civ in 0..6u16 {
        let n = s.nation(&c, civ as usize);
        for role in Role::ALL {
            let i = role.index();
            if n.officers[i] == NOBODY {
                continue;
            }
            let who = s
                .members
                .iter()
                .position(|m| m.session.pubkey().to_bytes() == n.keys[i])
                .unwrap();
            let b = OrderBatch {
                civ,
                tick: 0,
                role,
                member: n.officers[i],
                decision_digest: [1; 32],
                orders: vec![],
                adopt: vec![],
            };
            let sess = s.members[who].session.insecure_clone();
            let salt = [civ as u8 * 4 + i as u8 + 1; 32];
            c.send(
                vec![s.commit_orders_ix(&sess.pubkey(), civ, role, 0, order_commitment(&b, &salt))],
                &[&crank, &sess],
            )
            .expect("CommitOrders");
            sealed.push((b, salt));
        }
        for seat in &n.roll {
            let sess = s.members[seat.member as usize].session.insecure_clone();
            let proposal = GovAction::Propose {
                role: Role::General,
                orders: (0..2)
                    .map(|u| Order::MoveUnit {
                        unit: u,
                        path: vec![Hex { q: 1, r: 0 }; 12],
                    })
                    .collect(),
            };
            c.send(
                vec![s.submit_gov_ix(&sess.pubkey(), civ, seat.member, proposal)],
                &[&crank, &sess],
            )
            .expect("SubmitGov");
            while c
                .send(
                    vec![s.submit_gov_ix(
                        &sess.pubkey(),
                        civ,
                        seat.member,
                        GovAction::Stand { roles: 1 },
                    )],
                    &[&crank, &sess],
                )
                .is_ok()
            {}
        }
    }
    assert_eq!(sealed.len(), 24, "every office is held");
    s.close(&mut c);
    for (b, salt) in &sealed {
        c.send(vec![s.reveal_orders_ix(&cp, b, *salt)], &[&crank])
            .expect("RevealOrders");
    }
    s.freeze(&mut c);
    assert_err(
        c.send(vec![s.log_ix(0), s.log_ix(0)], &[&crank]),
        E::NotAlone,
    );
    assert_err(
        c.send(vec![s.log_ix(0), s.resolve_ix(12)], &[&crank]),
        E::NotAlone,
    );
    let four = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(1_400_000),
        ComputeBudgetInstruction::request_heap_frame(256 * 1024),
        ComputeBudgetInstruction::set_compute_unit_price(1),
        ComputeBudgetInstruction::set_loaded_accounts_data_size_limit(64 * 1024 * 1024),
        s.log_ix(0),
    ];
    let l = c
        .send_with(Budget::Light, four, &[&crank])
        .expect("LogTickInput#0 behind four compute-budget instructions");
    let m = s.meta(&c);
    assert!(m.input_chunks >= 2, "a full first chunk");
    let salts: Vec<(u16, u8, [u8; 32])> =
        borsh::from_slice(&record(&l.logs, b"PS_SALTS")[3]).unwrap();
    assert_eq!(salts.len(), 24);
    assert_eq!(record(&l.logs, b"PS_INPUT")[5].len(), INPUT_CHUNK);
    println!(
        "LogTickInput#0 with 24 salts and a full chunk: {} B of logs",
        log_bytes(&l.logs)
    );
    assert!(!log_truncated(&l.logs) && log_bytes(&l.logs) < 9_500);
    s.log_input(&mut c);
    resolve_parts(&mut c, &s, &crank, false);
    assert_eq!(s.world(&mut c).tick, 1);
}

/// After the last tick the nations stay frozen: no orders, no governance.
#[test]
fn finished_season_freezes_nations() {
    let mut c = Chain::new();
    let s = SeasonFx::running(&mut c, Params::default(), 4);
    s.ensure_rolls(&mut c);
    s.set_tick(&mut c, 179);
    s.play_tick(&mut c);
    assert!(s.meta(&c).finished);
    let crank = s.crank.insecure_clone();
    for civ in 0..2 {
        let n = s.nation(&c, civ);
        assert!(n.frozen && n.open_tick == 180);
        let role = *Role::ALL
            .iter()
            .find(|r| n.officers[r.index()] != NOBODY)
            .unwrap();
        let who = s
            .members
            .iter()
            .position(|m| m.session.pubkey().to_bytes() == n.keys[role.index()])
            .unwrap();
        let sess = s.members[who].session.insecure_clone();
        assert_err(
            c.send(
                vec![s.commit_orders_ix(&sess.pubkey(), civ as u16, role, 180, [3; 32])],
                &[&crank, &sess],
            ),
            E::TickFrozen,
        );
        assert_err(
            c.send(
                vec![s.submit_gov_ix(
                    &sess.pubkey(),
                    civ as u16,
                    who as u32,
                    GovAction::Stand { roles: 1 },
                )],
                &[&crank, &sess],
            ),
            E::WrongStatus,
        );
    }
}

/// A world chunk from an earlier write (a stale commit, a rollback) makes
/// the world's body and its root trailer disagree: ResolveTick refuses it.
#[test]
fn mixed_world_is_refused() {
    let mut c = Chain::new();
    let s = SeasonFx::running(&mut c, Params::default(), 4);
    let crank = s.crank.insecure_clone();
    let old: Vec<Vec<u8>> = s.chunks.iter().map(|k| c.data(k)).collect();
    s.play_tick(&mut c);
    s.close(&mut c);
    s.log_input(&mut c);
    let k = (1..WORLD_CHUNKS)
        .find(|k| c.data(&s.chunks[*k]) != old[*k])
        .expect("a chunk after the first changed with the tick");
    let mut stale = c.fork();
    stale.set_data(&s.chunks[k], old[k].clone());
    assert_err(stale.send(vec![s.resolve_ix(12)], &[&crank]), E::WrongWorld);
    c.send(vec![s.resolve_ix(12)], &[&crank])
        .expect("the consistent world resolves");
}
