//! StartSeason, GenesisStep, SeatMembers, OpenGovernment: effects and
//! every refusal. (The season seed itself: `season_seed_vrf.rs`.)

use borsh::BorshDeserialize;
use permutation_chain::randomness::SEED_PENDING;
use permutation_chain_svm_tests::error::ChainError as E;
use permutation_chain_svm_tests::seat::{substitute_key, Seat};
use permutation_chain_svm_tests::state::*;
use permutation_chain_svm_tests::*;
use permutation_rules::gov::{Role, NOBODY};
use solana_signer::Signer;

#[test]
fn start_season_checks() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create(&mut c, Params::default());
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    // No members yet.
    assert_err(c.send(vec![s.start_ix(&cp)], &[&crank]), E::WrongStatus);
    for i in 0..4 {
        s.register(&mut c, (i % 2) as u16, 0);
    }
    let outsider = c.funded();
    assert_err(
        c.send(vec![s.start_ix(&outsider.pubkey())], &[&outsider]),
        E::Unauthorized,
    );
    // The VRF accounts: the base queue only (A20), the VRF program, our
    // identity, system, SlotHashes.
    let clock = addr("SysvarC1ock11111111111111111111111111111111");
    for (at, key) in [
        (2, c.funded().pubkey()),
        (3, vrf::queue_er()),
        (4, addr(SYSTEM)),
        (5, clock),
        (6, clock),
    ] {
        let mut ix = s.start_ix(&cp);
        ix.accounts[at].pubkey = key;
        assert_err(c.send(vec![ix], &[&crank]), E::WrongOracle);
    }
    let mut ix = s.start_ix(&cp);
    ix.accounts[3].is_writable = false;
    assert_err(c.send(vec![ix], &[&crank]), E::WrongOracle);
    assert_eq!(s.season(&c).status, SeasonStatus::Registering);
    // The crank (or the admin) closes registration: Seeding, with no
    // request yet (RetrySeasonSeed makes the first, A19) and no world.
    vrf::requests();
    c.advance(5);
    c.send(vec![s.start_ix(&cp)], &[&crank])
        .expect("StartSeason");
    let season = s.season(&c);
    assert_eq!(season.status, SeasonStatus::Seeding);
    assert_eq!(
        (
            season.seed_state,
            season.seed_requests,
            season.season_seed,
            season.stage_at
        ),
        (SEED_PENDING, 0, [0; 32], c.now)
    );
    assert!(vrf::requests().is_empty(), "StartSeason requests nothing");
    assert_eq!(c.data(&s.chunks[0]), vec![0; CHUNK], "no world yet");
    assert_err(c.send(vec![s.start_ix(&cp)], &[&crank]), E::WrongStatus);
}

/// WP09: StartSeason needs the bond at `bond_floor`; PostBond gets it there.
#[test]
fn start_season_needs_the_bond_floor() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create_ai(&mut c, Params::default(), &[0, 1], 1_000_000, 1);
    s.register_ai(&mut c, 0);
    s.register_ai(&mut c, 1);
    s.register(&mut c, 0, 0);
    let crank = s.crank.insecure_clone();
    let floor = bond_floor(&s.season(&c));
    assert!(floor > 1, "a person plays: the floor is positive");
    assert_err(
        c.send(vec![s.start_ix(&crank.pubkey())], &[&crank]),
        E::BondTooSmall,
    );
    assert_eq!(s.top_up_bond(&mut c), floor - 1);
    assert_eq!(s.season(&c).bond, floor);
    c.send(vec![s.start_ix(&crank.pubkey())], &[&crank])
        .expect("StartSeason at the floor");
    assert_eq!(s.season(&c).status, SeasonStatus::Seeding);
}

#[test]
fn genesis_step_reaches_seating() {
    for nations in [2u8, 6] {
        let mut c = Chain::new();
        let mut s = SeasonFx::create(
            &mut c,
            Params {
                nations,
                ..Params::default()
            },
        );
        for i in 0..nations as usize {
            s.register(&mut c, i as u16, 0);
        }
        let crank = s.crank.insecure_clone();
        // Before StartSeason, and while the seed is pending.
        assert_err(
            c.send(vec![s.genesis_step_ix(50)], &[&crank]),
            E::WrongStatus,
        );
        c.send(vec![s.start_ix(&crank.pubkey())], &[&crank])
            .unwrap();
        assert_err(
            c.send(vec![s.genesis_step_ix(50)], &[&crank]),
            E::WrongStatus,
        );
        s.seed(&mut c);
        // Chunks out of order, 19 chunks, a chunk the program does not own.
        let mut ix = s.genesis_step_ix(50);
        ix.accounts.swap(1, 2);
        assert_err(c.send(vec![ix], &[&crank]), E::WrongPda);
        let mut ix = s.genesis_step_ix(50);
        ix.accounts.pop();
        assert_err(c.send(vec![ix], &[&crank]), E::WorldTooSmall);
        c.set_owner(&s.chunks[5], addr(SYSTEM));
        assert_err(
            c.send(vec![s.genesis_step_ix(50)], &[&crank]),
            E::WrongWorld,
        );
        c.set_owner(&s.chunks[5], c.program);
        // Anyone may step it. The first step writes the genesis job and the
        // world meta (WP11: StartSeason no longer touches the world).
        let anyone = c.funded();
        let l = c
            .send(vec![s.genesis_step_ix(50)], &[&anyone])
            .expect("GenesisStep");
        assert!(l.logs.iter().any(|x| x.contains("PS genesis job written")));
        assert_eq!(c.data(&s.chunks[0])[..8], GENESIS_MAGIC);
        let m = s.meta(&c);
        assert_eq!(
            (
                m.season_id,
                m.preset,
                m.civs,
                m.tick_seconds,
                m.deadline,
                m.finished,
                m.market
            ),
            (7, 0, nations, 30, 0, false, true)
        );
        // The last step writes tick 0.
        let mut steps = 1;
        c.advance(3);
        let last = loop {
            let l = c
                .send(vec![s.genesis_step_ix(50)], &[&anyone])
                .expect("GenesisStep");
            steps += 1;
            assert!(steps < 100, "genesis finishes");
            if s.season(&c).status == SeasonStatus::Seating {
                break l;
            }
            assert!(records(&l.logs, b"PS_GENESIS").is_empty());
        };
        println!("{nations} nations: genesis in {steps} steps");
        let g = record(&last.logs, b"PS_GENESIS");
        assert_eq!(
            g[1],
            s.root(&mut c).to_vec(),
            "the logged root is the world's"
        );
        let season = s.season(&c);
        assert_eq!(g[2], season.season_seed.to_vec());
        assert_eq!(season.stage_at, c.now, "Seating's stage begins");
        let world = s.world(&mut c);
        assert_eq!(
            (world.tick, world.nations.len(), world.members.len()),
            (0, nations as usize, 0)
        );
        assert_err(
            c.send(vec![s.genesis_step_ix(50)], &[&anyone]),
            E::WrongStatus,
        );
    }
}

#[test]
fn seat_members_checks() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create(&mut c, Params::default());
    // Members 1 and 2 registered the same session key: one key holder with
    // two wallets (the key signs both registrations, WP17).
    let shared = solana_keypair::Keypair::new();
    for i in 0..4 {
        let key = if i == 1 || i == 2 {
            shared.insecure_clone()
        } else {
            solana_keypair::Keypair::new()
        };
        s.register_keyed(
            &mut c,
            (i % 2) as u16,
            &key,
            0x0f,
            [u32::MAX; 4],
            0,
            [0; 32],
        );
    }
    let shared = shared.pubkey().to_bytes();
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    c.send(vec![s.start_ix(&cp)], &[&crank]).unwrap();
    // Not Seating yet (the seed, then genesis).
    assert_err(
        c.send(vec![s.seat_ix(&cp, &[0])], &[&crank]),
        E::WrongStatus,
    );
    s.genesis_steps(&mut c);
    let outsider = c.funded();
    assert_err(
        c.send(vec![s.seat_ix(&outsider.pubkey(), &[0])], &[&outsider]),
        E::Unauthorized,
    );
    // Out of registration order.
    assert_err(
        c.send(vec![s.seat_ix(&cp, &[1])], &[&crank]),
        E::InvalidParams,
    );
    // A member of another season.
    let mut b = SeasonFx::create(
        &mut c,
        Params {
            id: 8,
            ..Params::default()
        },
    );
    b.register(&mut c, 0, 0);
    let mut ix = s.seat_ix(&cp, &[0]);
    ix.accounts[2 + WORLD_CHUNKS].pubkey = b.members[0].member;
    assert_err(c.send(vec![ix], &[&crank]), E::NotInitialized);
    // In order: `seated` advances and PS_SEAT lists who was seated with which key.
    let l = c
        .send(vec![s.seat_ix(&cp, &[0, 1])], &[&crank])
        .expect("SeatMembers");
    assert_eq!(s.season(&c).seated, 2);
    let r = record(&l.logs, b"PS_SEAT");
    assert_eq!(r[1], s.root(&mut c).to_vec());
    let seats: Vec<Seat> = Vec::try_from_slice(&r[2]).unwrap();
    assert_eq!(seats.len(), 2);
    assert_eq!((seats[0].0, seats[1].0, seats[1].1), (0, 1, shared));
    // The duplicate key is seated with a substitute, so seating goes on.
    let l = c
        .send(vec![s.seat_ix(&cp, &[2, 3])], &[&crank])
        .expect("SeatMembers");
    assert!(l
        .logs
        .iter()
        .any(|x| x.contains("PS member 2 seated with a substitute key")));
    let seats: Vec<Seat> = Vec::try_from_slice(&record(&l.logs, b"PS_SEAT")[2]).unwrap();
    assert_eq!(
        seats[0].1,
        substitute_key(7, &s.members[2].wallet.pubkey().to_bytes()),
        "member 2 got its wallet's substitute key"
    );
    assert_eq!(s.season(&c).seated, 4);
    assert_eq!(s.world(&mut c).members.len(), 4);
    // Seated in full, the government opens.
    s.open(&mut c);
    assert_eq!(s.season(&c).status, SeasonStatus::Running);
}

#[test]
fn open_government_checks() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create(&mut c, Params::default());
    for i in 0..4 {
        s.register(&mut c, (i % 2) as u16, 0);
    }
    s.genesis(&mut c);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    // Not every member seated.
    c.send(vec![s.seat_ix(&cp, &[0, 1])], &[&crank]).unwrap();
    assert_err(c.send(vec![s.open_ix(&cp)], &[&crank]), E::WrongStatus);
    c.send(vec![s.seat_ix(&cp, &[2, 3])], &[&crank]).unwrap();
    let outsider = c.funded();
    assert_err(
        c.send(vec![s.open_ix(&outsider.pubkey())], &[&outsider]),
        E::Unauthorized,
    );
    // Nations out of order.
    let mut ix = s.open_ix(&cp);
    ix.accounts.swap(2 + WORLD_CHUNKS, 3 + WORLD_CHUNKS);
    assert_err(c.send(vec![ix], &[&crank]), E::MissingNation);
    // The admin may open it too.
    let admin = s.admin.insecure_clone();
    c.advance(7);
    let l = c
        .send(vec![s.open_ix(&admin.pubkey())], &[&admin])
        .expect("OpenGovernment");
    let season = s.season(&c);
    assert_eq!(
        (season.status, season.stage_at),
        (SeasonStatus::Running, c.now)
    );
    assert_eq!(record(&l.logs, b"PS_OPEN")[1], s.root(&mut c).to_vec());
    // Tick 0 closes a tick plus the grace for delegation from now (WP01).
    let deadline = c.now + 30 + TICK0_GRACE_SECONDS;
    let meta = s.meta(&c);
    assert_eq!(meta.deadline, deadline);
    assert!(!meta.frozen && !meta.revealing && !meta.finished);
    assert_eq!((meta.rand_state, meta.rand_tick), (0, NO_TICK));
    let world = s.world(&mut c);
    for civ in 0..2 {
        let n = s.nation(&c, civ);
        assert_eq!((n.open_tick, n.deadline), (0, deadline));
        for role in Role::ALL {
            let i = role.index();
            let holder = world.nations[civ].offices[i];
            assert_eq!(n.officers[i], holder);
            if holder != NOBODY {
                assert_eq!(
                    n.keys[i], world.members[holder as usize].key,
                    "the holder's session key"
                );
            }
        }
        assert!(
            n.officers.iter().any(|o| *o != NOBODY),
            "offices are held after the first election"
        );
        // The roll (WP03): the nation's members with their seated keys, and
        // the governance quota.
        let roll: Vec<RollSeat> = world
            .members
            .iter()
            .enumerate()
            .filter(|(_, m)| m.civ == civ as u16)
            .map(|(i, m)| RollSeat {
                member: i as u32,
                key8: key8(&m.key),
            })
            .collect();
        assert_eq!(n.roll, roll);
        assert_eq!(n.gov_quota, gov_quota(4, 2));
    }
    assert_err(c.send(vec![s.open_ix(&cp)], &[&crank]), E::WrongStatus);
}

/// WP14: seating and opening never wait on the operator. Anyone may seat
/// the members `TAKEOVER_SECONDS` after genesis completed, and open the
/// government: it takes no input but the member accounts.
#[test]
fn an_outsider_takes_over_seating() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create(&mut c, Params::default());
    for i in 0..4 {
        s.register(&mut c, (i % 2) as u16, 0);
    }
    s.genesis(&mut c);
    let stage = s.season(&c).stage_at;
    let outsider = c.funded();
    let op = outsider.pubkey();
    c.set_time(stage + TAKEOVER_SECONDS - 1);
    assert_err(
        c.send(vec![s.seat_ix(&op, &[0, 1, 2, 3])], &[&outsider]),
        E::Unauthorized,
    );
    c.set_time(stage + TAKEOVER_SECONDS);
    c.send(vec![s.seat_ix(&op, &[0, 1, 2, 3])], &[&outsider])
        .expect("the outsider seats");
    c.send(vec![s.open_ix(&op)], &[&outsider])
        .expect("the outsider opens");
    assert_eq!(s.season(&c).status, SeasonStatus::Running);
    assert_eq!(s.meta(&c).deadline, c.now + 30 + TICK0_GRACE_SECONDS);
}
