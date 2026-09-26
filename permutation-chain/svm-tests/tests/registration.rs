//! CreateSeason, AllocWorld, AllocNation, Register, UpdateMember: effects
//! and every refusal, with real keypairs (sigverify on).

use permutation_chain_svm_tests::error::ChainError as E;
use permutation_chain_svm_tests::ix::{CreateArgs, RegisterArgs};
use permutation_chain_svm_tests::state::*;
use permutation_chain_svm_tests::*;
use solana_signer::Signer;

#[test]
fn create_season_lands_with_its_vault() {
    let mut c = Chain::new();
    let s = SeasonFx::bare(&mut c, Params::default());
    let admin = s.admin.insecure_clone();
    c.send(vec![s.create_ix(&admin.pubkey())], &[&admin])
        .expect("CreateSeason");
    let season = s.season(&c);
    assert_eq!(season.magic, SEASON_MAGIC);
    assert_eq!(season.status, SeasonStatus::Registering);
    assert_eq!(
        (
            season.season_id,
            season.admin,
            season.crank,
            season.usdc_mint
        ),
        (
            7,
            admin.pubkey().to_bytes(),
            s.crank.pubkey().to_bytes(),
            s.mint.to_bytes()
        )
    );
    assert_eq!(
        (
            season.usdc_decimals,
            season.preset,
            season.nations,
            season.entry_fee,
            season.tick_seconds,
            season.market
        ),
        (6, 0, 2, s.p.fee, 30, true)
    );
    assert_eq!(
        (season.nation_members.clone(), season.treasury.clone()),
        (vec![0, 0], vec![0, 0])
    );
    assert_eq!(
        (
            season.member_count,
            season.pool,
            season.ops,
            season.ai_count
        ),
        (0, 0, 0, 0)
    );
    // The vault: a token account of the season's mint owned by the Season PDA.
    assert_eq!(c.owner(&s.vault), Some(addr(TOKEN)));
    assert_eq!(c.token_mint_owner(&s.vault), (s.mint, s.season));
    assert_eq!(c.balance(&s.vault), 0);
}

#[test]
fn create_season_with_ais_escrows_into_the_vault() {
    let mut c = Chain::new();
    let (bounty, bond) = (1_000_000, 4_000_000);
    let s = SeasonFx::with_ai(&mut c, Params::default(), &[0, 1], bounty, bond);
    let admin = s.admin.insecure_clone();
    c.send(
        vec![s.create_ai_ix(&admin.pubkey(), s.roster_chain())],
        &[&admin],
    )
    .expect("CreateSeason with AIs");
    // `with_ai` funded the admin with exactly the escrow; it all moved.
    assert_eq!(
        (c.balance(&s.vault), c.balance(&s.admin_token)),
        (2 * bounty + bond, 0)
    );
    let season = s.season(&c);
    assert_eq!(
        (
            season.ai_count,
            season.bounty_each,
            season.bond,
            season.roster_commit
        ),
        (2, bounty, bond, s.roster_chain())
    );
    let roster = s.roster(&c);
    assert_eq!(
        (roster.magic, roster.season_id, roster.entries.len()),
        (ROSTER_MAGIC, 7, 0)
    );
    assert_eq!(c.owner(&s.roster), Some(c.program));
}

#[test]
fn create_season_checks() {
    let mut c = Chain::new();
    let s = SeasonFx::bare(&mut c, Params::default());
    let admin = s.admin.insecure_clone();
    let a = s.create_args();
    let payer = c.funded();
    let create = |c: &mut Chain, a: &CreateArgs, extra| {
        c.send(vec![s.create_ix_from(&admin.pubkey(), a, extra)], &[&admin])
    };

    // The admin must sign (someone else pays).
    let mut ix = s.create_ix(&admin.pubkey());
    ix.accounts[0].is_signer = false;
    assert_err(c.send(vec![ix], &[&payer]), E::MissingSignature);
    // Parameters.
    for bad in [
        CreateArgs {
            preset: 2,
            ..a.clone()
        },
        CreateArgs {
            nations: 1,
            ..a.clone()
        },
        CreateArgs {
            nations: 7,
            ..a.clone()
        },
        CreateArgs {
            tick_seconds: 0,
            ..a.clone()
        },
        CreateArgs {
            ai_count: MAX_AI + 1,
            ..a.clone()
        },
        CreateArgs {
            bounty_each: 1,
            ..a.clone()
        },
        CreateArgs {
            bond: 1,
            ..a.clone()
        },
        CreateArgs {
            roster_commit: [1; 32],
            ..a.clone()
        },
    ] {
        assert_err(create(&mut c, &bad, vec![]), E::InvalidParams);
    }
    // With AIs: an escrow that overflows u64.
    let ai = CreateArgs {
        ai_count: 2,
        bounty_each: u64::MAX,
        ..a.clone()
    };
    assert_err(
        create(&mut c, &ai, vec![w(&s.admin_token), w(&s.roster)]),
        E::InvalidParams,
    );
    // An admin short of the escrow: the token program refuses.
    let ai = CreateArgs {
        ai_count: 1,
        bounty_each: 1_000_000,
        ..a.clone()
    };
    assert_token_err(create(&mut c, &ai, vec![w(&s.admin_token), w(&s.roster)]));
    // A mint account the token program does not own.
    let mut ix = s.create_ix(&admin.pubkey());
    ix.accounts[3].pubkey = payer.pubkey();
    assert_err(c.send(vec![ix], &[&admin]), E::WrongMint);
    // Another token program.
    let mut ix = s.create_ix(&admin.pubkey());
    ix.accounts[4].pubkey = addr(SYSTEM);
    assert_program_err(c.send(vec![ix], &[&admin]), "IncorrectProgramId");
    // The season or the vault PDA of another id.
    let other = 8u64.to_le_bytes();
    let mut ix = s.create_ix(&admin.pubkey());
    ix.accounts[1].pubkey = pda(&c.program, &[SEASON_SEED, &other]);
    assert_err(c.send(vec![ix], &[&admin]), E::WrongPda);
    let mut ix = s.create_ix(&admin.pubkey());
    ix.accounts[2].pubkey = pda(&c.program, &[VAULT_SEED, &other]);
    assert_err(c.send(vec![ix], &[&admin]), E::WrongPda);
    // It lands once; a second time the season exists.
    create(&mut c, &a, vec![]).expect("CreateSeason");
    assert_err(create(&mut c, &a, vec![]), E::AlreadyInitialized);

    // The history layer: a predecessor must be Finalized, of the same
    // admin, and another season.
    let next = |id: u64, prev: u64| CreateArgs {
        season_id: id,
        prev_season_id: prev,
        ..a.clone()
    };
    let program = c.program;
    let at = |id: u64| {
        let id = id.to_le_bytes();
        (
            pda(&program, &[SEASON_SEED, &id]),
            pda(&program, &[VAULT_SEED, &id]),
        )
    };
    let with_prev = |id: u64, prev: u64| {
        let (season, vault) = at(id);
        let mut ix = s.create_ix_from(&admin.pubkey(), &next(id, prev), vec![r(&at(prev).0)]);
        ix.accounts[1].pubkey = season;
        ix.accounts[2].pubkey = vault;
        ix
    };
    assert_err(c.send(vec![with_prev(9, 7)], &[&admin]), E::InvalidParams);
    c.edit::<Season>(&s.season, |x| {
        x.status = SeasonStatus::Finalized;
        x.history_root = [0x33; 32];
    });
    // The same id as its predecessor.
    let mut ix = s.create_ix_from(&admin.pubkey(), &next(7, 7), vec![r(&s.season)]);
    ix.accounts[1].pubkey = s.season;
    assert_err(c.send(vec![ix], &[&admin]), E::InvalidParams);
    // Another admin's finalized season.
    let stranger = c.funded();
    let mut ix = s.create_ix_from(&stranger.pubkey(), &next(9, 7), vec![r(&s.season)]);
    (ix.accounts[1].pubkey, ix.accounts[2].pubkey) = at(9);
    assert_err(c.send(vec![ix], &[&stranger]), E::InvalidParams);
    // The admin's own finalized season: the history root is taken over.
    c.send(vec![with_prev(9, 7)], &[&admin])
        .expect("CreateSeason after a finalized season");
    let nine: Season = c.load(&at(9).0);
    assert_eq!(
        (nine.prev_season_id, nine.prev_history_root),
        (7, [0x33; 32])
    );
}

#[test]
fn alloc_world_checks() {
    let mut c = Chain::new();
    let s = SeasonFx::bare(&mut c, Params::default());
    let admin = s.admin.insecure_clone();
    c.send(vec![s.create_ix(&admin.pubkey())], &[&admin])
        .unwrap();
    let payer = c.funded();
    let p = payer.pubkey();
    assert_err(
        c.send(vec![s.alloc_world_ix(&p, WORLD_CHUNKS as u8)], &[&payer]),
        E::InvalidParams,
    );
    // Chunk 4's PDA passed as chunk 3.
    let mut ix = s.alloc_world_ix(&p, 3);
    ix.accounts[2].pubkey = s.chunks[4];
    assert_err(c.send(vec![ix], &[&payer]), E::WrongPda);
    // The payer must sign.
    let mut ix = s.alloc_world_ix(&p, 3);
    ix.accounts[0].is_signer = false;
    let other = c.funded();
    assert_err(c.send(vec![ix], &[&other]), E::MissingSignature);
    // A season account the program does not own.
    let fake = c.funded().pubkey();
    c.put(fake, addr(SYSTEM), c.data(&s.season));
    let mut ix = s.alloc_world_ix(&p, 3);
    ix.accounts[1].pubkey = fake;
    assert_err(c.send(vec![ix], &[&payer]), E::NotInitialized);
    // Anyone may pay: a program-owned CHUNK-byte account.
    c.send(vec![s.alloc_world_ix(&p, 3)], &[&payer])
        .expect("AllocWorld");
    assert_eq!(c.owner(&s.chunks[3]), Some(c.program));
    assert_eq!(c.data(&s.chunks[3]), vec![0; CHUNK]);
    assert_err(
        c.send(vec![s.alloc_world_ix(&p, 3)], &[&payer]),
        E::AlreadyInitialized,
    );
}

#[test]
fn alloc_nation_checks() {
    let mut c = Chain::new();
    let s = SeasonFx::bare(&mut c, Params::default());
    let admin = s.admin.insecure_clone();
    c.send(vec![s.create_ix(&admin.pubkey())], &[&admin])
        .unwrap();
    let payer = c.funded();
    let p = payer.pubkey();
    assert_err(
        c.send(vec![s.alloc_nation_ix(&p, 2)], &[&payer]),
        E::InvalidParams,
    );
    let mut ix = s.alloc_nation_ix(&p, 0);
    ix.accounts[2].pubkey = s.nations[1];
    assert_err(c.send(vec![ix], &[&payer]), E::WrongPda);
    c.send(vec![s.alloc_nation_ix(&p, 1)], &[&payer])
        .expect("AllocNation");
    let n = s.nation(&c, 1);
    assert_eq!(
        (n.magic, n.season_id, n.civ, n.preset, n.market),
        (NATION_MAGIC, 7, 1, 0, true)
    );
    assert_eq!(
        (n.crank, n.open_tick),
        (s.crank.pubkey().to_bytes(), NO_TICK)
    );
    assert_eq!(c.data(&s.nations[1]).len(), NATION_SPACE);
    assert_err(
        c.send(vec![s.alloc_nation_ix(&p, 1)], &[&payer]),
        E::AlreadyInitialized,
    );
}

#[test]
fn register_lands_and_splits_the_fee() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create(&mut c, Params::default());
    let fee = s.p.fee;
    let deposit = 3_000_000;
    for (i, civ) in [1u16, 1].into_iter().enumerate() {
        let m = s.new_member(&mut c, civ, fee + deposit);
        let wallet = m.wallet.insecure_clone();
        c.send(
            vec![s.register_ix(&m, &wallet.pubkey(), deposit)],
            &[&wallet],
        )
        .expect("Register");
        let mm: MemberAccount = c.load(&m.member);
        assert_eq!(
            (mm.magic, mm.season_id, mm.index, mm.civ),
            (MEMBER_MAGIC, 7, i as u32, civ)
        );
        assert_eq!(
            (mm.wallet, mm.session),
            (wallet.pubkey().to_bytes(), m.session.pubkey().to_bytes())
        );
        assert_eq!(
            (mm.shares, mm.claimed, mm.stand, mm.votes),
            (deposit, false, 0x0f, [u32::MAX; 4])
        );
        assert_eq!(c.balance(&m.token), 0, "fee and deposit left the wallet");
        s.members.push(m);
    }
    let season = s.season(&c);
    let ops = 2 * (fee / 5); // 20% operations, 80% prize pool (V5 D10)
    assert_eq!(
        (season.member_count, season.ops, season.pool),
        (2, ops, 2 * fee - ops)
    );
    assert_eq!(
        (season.treasury.clone(), season.nation_members.clone()),
        (vec![0, 2 * deposit], vec![0, 2])
    );
    assert_eq!(c.balance(&s.vault), 2 * (fee + deposit));
}

#[test]
fn register_checks() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create(&mut c, Params::default());
    let fee = s.p.fee;
    let m = s.new_member(&mut c, 0, fee + 1_000_000);
    let wl = m.wallet.insecure_clone();
    let reg = |c: &mut Chain, a: &RegisterArgs| {
        c.send(vec![s.register_ix_from(&m, &wl.pubkey(), a)], &[&wl])
    };
    let a = s.register_args(&m, 0);

    // The wallet must sign (someone else pays).
    let payer = c.funded();
    let mut ix = s.register_ix(&m, &payer.pubkey(), 0);
    ix.accounts[0].is_signer = false;
    assert_err(c.send(vec![ix], &[&payer]), E::MissingSignature);
    assert_err(
        reg(
            &mut c,
            &RegisterArgs {
                civ: 2,
                ..a.clone()
            },
        ),
        E::InvalidParams,
    );
    // Name, kind and candidacy.
    for bad in [
        RegisterArgs {
            name: String::new(),
            ..a.clone()
        },
        RegisterArgs {
            name: "x".repeat(MAX_NAME + 1),
            ..a.clone()
        },
        RegisterArgs {
            kind: MAX_KIND + 1,
            ..a.clone()
        },
        RegisterArgs {
            stand: STAND_MASK + 1,
            ..a.clone()
        },
    ] {
        assert_err(reg(&mut c, &bad), E::InvalidName);
    }
    // A full season.
    c.edit::<Season>(&s.season, |x| x.member_count = MAX_MEMBERS);
    assert_err(reg(&mut c, &a), E::SeasonFull);
    c.edit::<Season>(&s.season, |x| x.member_count = 0);
    // A deposit into a season without the market.
    c.edit::<Season>(&s.season, |x| x.market = false);
    assert_err(
        reg(
            &mut c,
            &RegisterArgs {
                deposit: 1,
                ..a.clone()
            },
        ),
        E::InvalidParams,
    );
    c.edit::<Season>(&s.season, |x| x.market = true);
    // Another mint account.
    let other_mint = c.mint(6);
    let mut ix = s.register_ix(&m, &wl.pubkey(), 0);
    ix.accounts[6].pubkey = other_mint;
    assert_err(c.send(vec![ix], &[&wl]), E::WrongMint);
    // A wallet token account of another mint.
    let mut ix = s.register_ix(&m, &wl.pubkey(), 0);
    ix.accounts[4].pubkey = c.token_account(&other_mint, &wl.pubkey(), fee);
    assert_err(c.send(vec![ix], &[&wl]), E::WrongTokenAccount);
    // A vault that is not the season's PDA.
    let mut ix = s.register_ix(&m, &wl.pubkey(), 0);
    ix.accounts[5].pubkey = c.token_account(&s.mint, &s.season, 0);
    assert_err(c.send(vec![ix], &[&wl]), E::WrongPda);
    // The member PDA of another wallet.
    let mut ix = s.register_ix(&m, &wl.pubkey(), 0);
    ix.accounts[3].pubkey = s.member_pda(&payer.pubkey());
    assert_err(c.send(vec![ix], &[&wl]), E::WrongPda);
    // Not enough USDC: the token program refuses and no member is written.
    let poor = s.new_member(&mut c, 0, fee - 1);
    let wp = poor.wallet.insecure_clone();
    assert_token_err(c.send(vec![s.register_ix(&poor, &wp.pubkey(), 0)], &[&wp]));
    assert_eq!(c.owner(&poor.member), None);
    assert_eq!(s.season(&c).member_count, 0);
    // One member per wallet.
    reg(&mut c, &a).expect("Register");
    assert_err(reg(&mut c, &a), E::AlreadyInitialized);
    s.members.push(m);
    // Registration closes with StartSeason.
    let crank = s.crank.insecure_clone();
    c.send(vec![s.start_ix(&crank.pubkey())], &[&crank])
        .unwrap();
    let late = s.new_member(&mut c, 1, fee);
    let wl2 = late.wallet.insecure_clone();
    assert_err(
        c.send(vec![s.register_ix(&late, &wl2.pubkey(), 0)], &[&wl2]),
        E::WrongStatus,
    );
    assert_eq!(c.balance(&s.vault), fee);
}

#[test]
fn update_member_checks() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create(&mut c, Params::default());
    let i = s.register(&mut c, 0, 0);
    let wallet = s.members[i].wallet.insecure_clone();
    let session = s.members[i].session.insecure_clone();
    let payer = c.funded();
    let stranger = c.funded();
    assert_err(
        c.send(
            vec![s.update_member_ix(&stranger.pubkey(), i, 1, [1; 4])],
            &[&stranger],
        ),
        E::Unauthorized,
    );
    // The wallet and the session key may both update.
    c.send(
        vec![s.update_member_ix(&wallet.pubkey(), i, 1, [2; 4])],
        &[&wallet],
    )
    .expect("the wallet updates");
    assert_eq!((s.member(&c, i).stand, s.member(&c, i).votes), (1, [2; 4]));
    c.send(
        vec![s.update_member_ix(&session.pubkey(), i, 3, [0; 4])],
        &[&payer, &session],
    )
    .expect("the session key updates");
    assert_eq!((s.member(&c, i).stand, s.member(&c, i).votes), (3, [0; 4]));
    assert_err(
        c.send(
            vec![s.update_member_ix(&session.pubkey(), i, STAND_MASK + 1, [0; 4])],
            &[&payer, &session],
        ),
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
    let j = b.register(&mut c, 0, 0);
    let bw = b.members[j].wallet.insecure_clone();
    let mut ix = s.update_member_ix(&bw.pubkey(), i, 1, [0; 4]);
    ix.accounts[2].pubkey = b.members[j].member;
    assert_err(c.send(vec![ix], &[&bw]), E::NotInitialized);
    // Only while registering.
    let crank = s.crank.insecure_clone();
    c.send(vec![s.start_ix(&crank.pubkey())], &[&crank])
        .unwrap();
    assert_err(
        c.send(
            vec![s.update_member_ix(&wallet.pubkey(), i, 1, [0; 4])],
            &[&wallet],
        ),
        E::WrongStatus,
    );
}

/// Fixed behaviour (WP17): the session key co-signs Register (account 9),
/// so nobody registers a session key they do not hold.
#[test]
#[ignore = "until WP17: Register takes the session key as a signer (account 9)"]
fn register_needs_the_session_signature() {
    let mut c = Chain::new();
    let s = SeasonFx::create(&mut c, Params::default());
    let m = s.new_member(&mut c, 0, s.p.fee);
    let wallet = m.wallet.insecure_clone();
    // The session key is passed, but does not sign.
    let mut ix = s.register_ix(&m, &wallet.pubkey(), 0);
    ix.accounts.push(r(&m.session.pubkey()));
    assert_err(c.send(vec![ix], &[&wallet]), E::MissingSignature);
    // Signed by both, it lands.
    let mut ix = s.register_ix(&m, &wallet.pubkey(), 0);
    ix.accounts.push(rs(&m.session.pubkey()));
    let session = m.session.insecure_clone();
    c.send(vec![ix], &[&wallet, &session])
        .expect("Register with the session's signature");
}

#[test]
fn garbage_instruction_data_is_refused() {
    let mut c = Chain::new();
    let k = c.funded();
    for data in [vec![255u8], vec![], vec![0, 1, 2]] {
        let ix = solana_instruction::Instruction::new_with_bytes(
            c.program,
            &data,
            vec![ws(&k.pubkey())],
        );
        assert_err(c.send(vec![ix], &[&k]), E::InvalidInstruction);
    }
}
