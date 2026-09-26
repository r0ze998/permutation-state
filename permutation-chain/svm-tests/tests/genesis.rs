//! StartSeason, GenesisStep, SeatMembers, OpenGovernment: effects and
//! every refusal.

use borsh::BorshDeserialize;
use permutation_chain_svm_tests::error::ChainError as E;
use permutation_chain_svm_tests::seat::Seat;
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
    // Another sysvar than SlotHashes.
    let mut ix = s.start_ix(&cp);
    ix.accounts[2].pubkey = addr("SysvarC1ock11111111111111111111111111111111");
    assert_err(c.send(vec![ix], &[&crank]), E::InvalidParams);
    // 19 world chunks.
    let mut ix = s.start_ix(&cp);
    ix.accounts.pop();
    assert_err(c.send(vec![ix], &[&crank]), E::WorldTooSmall);
    // A chunk the program does not own.
    c.set_owner(&s.chunks[5], addr(SYSTEM));
    assert_err(c.send(vec![s.start_ix(&cp)], &[&crank]), E::WrongWorld);
    c.set_owner(&s.chunks[5], c.program);
    // The crank (or the admin) starts it: genesis begins in the world.
    c.send(vec![s.start_ix(&cp)], &[&crank])
        .expect("StartSeason");
    let season = s.season(&c);
    assert_eq!(season.status, SeasonStatus::Genesis);
    assert_ne!(season.season_seed, [0; 32]);
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
        (7, 0, 2, 30, 0, false, true)
    );
    assert_err(c.send(vec![s.start_ix(&cp)], &[&crank]), E::WrongStatus);
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
        // Before StartSeason.
        assert_err(
            c.send(vec![s.genesis_step_ix(50)], &[&crank]),
            E::WrongStatus,
        );
        c.send(vec![s.start_ix(&crank.pubkey())], &[&crank])
            .unwrap();
        // Chunks out of order.
        let mut ix = s.genesis_step_ix(50);
        ix.accounts.swap(1, 2);
        assert_err(c.send(vec![ix], &[&crank]), E::WrongPda);
        // Anyone may step it; the last step writes tick 0.
        let anyone = c.funded();
        let mut steps = 0;
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
        assert_eq!(g[2], s.season(&c).season_seed.to_vec());
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
    let shared = solana_keypair::Keypair::new().pubkey().to_bytes();
    // Members 1 and 2 registered the same session key.
    for i in 0..4 {
        let key = if i == 1 || i == 2 {
            shared
        } else {
            solana_keypair::Keypair::new().pubkey().to_bytes()
        };
        s.register_raw(&mut c, (i % 2) as u16, key, 0x0f, [u32::MAX; 4], 0, [0; 32]);
    }
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    c.send(vec![s.start_ix(&cp)], &[&crank]).unwrap();
    // Not Seating yet (genesis runs).
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
    let seats: Vec<Seat> = Vec::try_from_slice(&record(&l.logs, b"PS_SEAT")[2]).unwrap();
    assert_ne!(seats[0].1, shared, "member 2 got a substitute key");
    assert_eq!(s.season(&c).seated, 4);
    assert_eq!(s.world(&mut c).members.len(), 4);
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
    let l = c
        .send(vec![s.open_ix(&admin.pubkey())], &[&admin])
        .expect("OpenGovernment");
    assert_eq!(s.season(&c).status, SeasonStatus::Running);
    assert_eq!(record(&l.logs, b"PS_OPEN")[1], s.root(&mut c).to_vec());
    assert_eq!(
        s.meta(&c).deadline,
        c.now + 30,
        "tick 0 closes a tick from now"
    );
    let world = s.world(&mut c);
    for civ in 0..2 {
        let n = s.nation(&c, civ);
        assert_eq!(n.open_tick, 0);
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
    }
    assert_err(c.send(vec![s.open_ix(&cp)], &[&crank]), E::WrongStatus);
}
