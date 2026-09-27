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
    // The deadlines and the validator (WP14), the deposit (WP12).
    assert_eq!(
        (
            season.stage_at,
            season.start_by,
            season.deposit,
            season.validator
        ),
        (
            c.now,
            s.create_args().start_by,
            0,
            addr(ix::registration::ER_VALIDATOR).to_bytes()
        )
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
        // Only measured presets (WP13): not the Season preset.
        CreateArgs {
            preset: PRESET_SEASON,
            ..a.clone()
        },
        CreateArgs {
            ai_count: MAX_AI + 1,
            ..a.clone()
        },
        // At least one seat is left for people (WP07).
        CreateArgs {
            ai_count: SEASON_MEMBER_CAP as u16,
            ..a.clone()
        },
        // The caps (WP12).
        CreateArgs {
            entry_fee: MAX_ENTRY_FEE + 1,
            ..a.clone()
        },
        CreateArgs {
            deposit: MAX_DEPOSIT + 1,
            ..a.clone()
        },
        CreateArgs {
            deposit: 1,
            market: false,
            ..a.clone()
        },
        // A validator is named (WP14).
        CreateArgs {
            validator: [0; 32],
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
    // With AIs: a bounty or a bond over the caps.
    let ai = CreateArgs {
        ai_count: 2,
        bounty_each: MAX_BOUNTY + 1,
        ..a.clone()
    };
    assert_err(
        create(&mut c, &ai, vec![w(&s.admin_token), w(&s.roster)]),
        E::InvalidParams,
    );
    let ai = CreateArgs {
        ai_count: 2,
        bond: MAX_BOND + 1,
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
    // A mint account the token program does not own; a mint of 9
    // decimals; a mint with a freeze authority (WP12).
    let mut ix = s.create_ix(&admin.pubkey());
    ix.accounts[3].pubkey = payer.pubkey();
    assert_err(c.send(vec![ix], &[&admin]), E::WrongMint);
    let nine = c.mint(9);
    let mut ix = s.create_ix(&admin.pubkey());
    ix.accounts[3].pubkey = nine;
    assert_err(c.send(vec![ix], &[&admin]), E::WrongMint);
    let frozen = c.mint(6);
    let mut d = c.data(&frozen);
    d[46..50].copy_from_slice(&1u32.to_le_bytes());
    d[50..82].copy_from_slice(&[7; 32]);
    c.set_data(&frozen, d);
    let mut ix = s.create_ix(&admin.pubkey());
    ix.accounts[3].pubkey = frozen;
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

    // The history layer: a predecessor must be Finalized or Aborted, of the
    // same admin, and another season.
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
    // An aborted one too (WP14).
    c.edit::<Season>(&s.season, |x| x.status = SeasonStatus::Aborted);
    c.send(vec![with_prev(10, 7)], &[&admin])
        .expect("CreateSeason after an aborted season");
    let ten: Season = c.load(&at(10).0);
    assert_eq!(ten.prev_history_root, [0x33; 32]);
}

/// WP14: a tick is at most `MAX_TICK_SECONDS`, and StartSeason is due in
/// the future, at most `MAX_REGISTRATION_SECONDS` ahead.
#[test]
fn create_season_bounds() {
    let mut c = Chain::new();
    let s = SeasonFx::bare(&mut c, Params::default());
    let admin = s.admin.insecure_clone();
    let a = s.create_args();
    let now = c.now;
    for bad in [
        CreateArgs {
            tick_seconds: MAX_TICK_SECONDS + 1,
            ..a.clone()
        },
        CreateArgs {
            start_by: now,
            ..a.clone()
        },
        CreateArgs {
            start_by: now + MAX_REGISTRATION_SECONDS + 1,
            ..a.clone()
        },
    ] {
        assert_err(
            c.send(
                vec![s.create_ix_from(&admin.pubkey(), &bad, vec![])],
                &[&admin],
            ),
            E::InvalidParams,
        );
    }
    let edge = CreateArgs {
        tick_seconds: MAX_TICK_SECONDS,
        start_by: now + MAX_REGISTRATION_SECONDS,
        ..a.clone()
    };
    c.send(
        vec![s.create_ix_from(&admin.pubkey(), &edge, vec![])],
        &[&admin],
    )
    .expect("the longest tick and registration window");
    let season = s.season(&c);
    assert_eq!(
        (season.tick_seconds, season.start_by, season.stage_at),
        (MAX_TICK_SECONDS, now + MAX_REGISTRATION_SECONDS, now)
    );
}

/// WP12 (revision 2): the history continues across the redeploy. A season
/// written by the previous program (`PSSEASN7`: no v8 tail), Finalized,
/// is followed; one still Running is not; another magic is not a season.
#[test]
fn create_season_follows_a_v7_finalized_season() {
    let mut c = Chain::new();
    let s = SeasonFx::create(&mut c, Params::default());
    let admin = s.admin.insecure_clone();
    // Season 7 as the previous program left it: the fields up to
    // `bounty_paid`, the rest of the account zero.
    let write_v7 = |c: &mut Chain, status: SeasonStatus, magic: [u8; 8]| {
        let mut v7 = s.season(c);
        v7.magic = magic;
        v7.status = status;
        v7.history_root = [0x44; 32];
        let full = borsh::to_vec(&v7).unwrap();
        let tail = borsh::to_vec(&Tail::of(&v7)).unwrap().len();
        let mut data = vec![0u8; SEASON_SPACE];
        data[..full.len() - tail].copy_from_slice(&full[..full.len() - tail]);
        c.set_data(&s.season, data);
    };
    let program = c.program;
    let next = |c: &mut Chain| {
        let id = 9u64.to_le_bytes();
        let a = CreateArgs {
            season_id: 9,
            prev_season_id: 7,
            ..s.create_args()
        };
        let mut ix = s.create_ix_from(&admin.pubkey(), &a, vec![r(&s.season)]);
        ix.accounts[1].pubkey = pda(&program, &[SEASON_SEED, &id]);
        ix.accounts[2].pubkey = pda(&program, &[VAULT_SEED, &id]);
        c.send(vec![ix], &[&admin])
    };
    write_v7(&mut c, SeasonStatus::Running, LEGACY_SEASON_MAGIC);
    assert_err(next(&mut c), E::InvalidParams);
    write_v7(&mut c, SeasonStatus::Finalized, *b"PSSEASN6");
    assert_err(next(&mut c), E::NotInitialized);
    write_v7(&mut c, SeasonStatus::Finalized, LEGACY_SEASON_MAGIC);
    next(&mut c).expect("CreateSeason after a v7 finalized season");
    let nine: Season = c.load(&pda(&program, &[SEASON_SEED, &9u64.to_le_bytes()]));
    assert_eq!(
        (nine.prev_season_id, nine.prev_history_root),
        (7, [0x44; 32])
    );
}

/// The v8 tail of a Season (every field after `bounty_paid`), to cut it off.
#[derive(borsh::BorshSerialize)]
struct Tail {
    delegated: u32,
    roster_blind: [u8; 32],
    refund_base: Vec<u64>,
    refund_in_payout: Vec<u8>,
    seed: (u8, [u8; 32], i64, u8),
    solvency: (u64, u64, bool),
    escape: (i64, i64, u32, u8, [u8; 32]),
    code: (u16, [u8; 32], u16, u64),
}

impl Tail {
    fn of(s: &Season) -> Tail {
        Tail {
            delegated: s.delegated,
            roster_blind: s.roster_blind,
            refund_base: s.refund_base.clone(),
            refund_in_payout: s.refund_in_payout.clone(),
            seed: (
                s.seed_state,
                s.seed_oracle,
                s.seed_requested_at,
                s.seed_requests,
            ),
            solvency: (s.deposit, s.outstanding, s.voided),
            escape: (
                s.start_by,
                s.stage_at,
                s.rolled_back,
                s.aborted_from,
                s.validator,
            ),
            code: (
                s.rules_version,
                s.rules_hash,
                s.logic_version,
                s.created_slot,
            ),
        }
    }
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
    // Only while registering (WP14).
    c.edit::<Season>(&s.season, |x| x.status = SeasonStatus::Aborted);
    assert_err(
        c.send(vec![s.alloc_world_ix(&p, 3)], &[&payer]),
        E::WrongStatus,
    );
    c.edit::<Season>(&s.season, |x| x.status = SeasonStatus::Registering);
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
    // Only while registering (WP14).
    c.edit::<Season>(&s.season, |x| x.status = SeasonStatus::Genesis);
    assert_err(
        c.send(vec![s.alloc_nation_ix(&p, 1)], &[&payer]),
        E::WrongStatus,
    );
    c.edit::<Season>(&s.season, |x| x.status = SeasonStatus::Registering);
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
    let deposit = 3_000_000;
    let mut s = SeasonFx::create(
        &mut c,
        Params {
            deposit,
            ..Params::default()
        },
    );
    let fee = s.p.fee;
    // Every member pays the season's deposit, no other (WP12).
    let m = s.new_member(&mut c, 0, fee + deposit);
    let (w, ss) = (m.wallet.insecure_clone(), m.session.insecure_clone());
    assert_err(
        c.send(vec![s.register_ix(&m, &w.pubkey(), 0)], &[&w, &ss]),
        E::InvalidParams,
    );
    for (i, civ) in [1u16, 1].into_iter().enumerate() {
        let m = s.new_member(&mut c, civ, fee + deposit);
        let (wallet, session) = (m.wallet.insecure_clone(), m.session.insecure_clone());
        c.send(
            vec![s.register_ix(&m, &wallet.pubkey(), deposit)],
            &[&wallet, &session],
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
    let (wl, ss) = (m.wallet.insecure_clone(), m.session.insecure_clone());
    let reg = |c: &mut Chain, a: &RegisterArgs| {
        c.send(vec![s.register_ix_from(&m, &wl.pubkey(), a)], &[&wl, &ss])
    };
    let a = s.register_args(&m, 0);

    // The wallet must sign (someone else pays).
    let payer = c.funded();
    let mut ix = s.register_ix(&m, &payer.pubkey(), 0);
    ix.accounts[0].is_signer = false;
    assert_err(c.send(vec![ix], &[&payer, &ss]), E::MissingSignature);
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
    // A first-election vote for a member the Season cannot hold (WP06).
    let mut votes = [u32::MAX; 4];
    votes[1] = MAX_MEMBERS;
    assert_err(
        reg(&mut c, &RegisterArgs { votes, ..a.clone() }),
        E::InvalidParams,
    );
    // A full season (SEASON_MEMBER_CAP), a full nation (NATION_MEMBER_CAP).
    c.edit::<Season>(&s.season, |x| x.member_count = SEASON_MEMBER_CAP);
    assert_err(reg(&mut c, &a), E::SeasonFull);
    c.edit::<Season>(&s.season, |x| {
        x.member_count = NATION_MEMBER_CAP;
        x.nation_members = vec![NATION_MEMBER_CAP, 0];
    });
    assert_err(reg(&mut c, &a), E::SeasonFull);
    c.edit::<Season>(&s.season, |x| {
        x.member_count = 0;
        x.nation_members = vec![0, 0];
    });
    // A deposit other than the season's (0 here; WP12).
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
    // Another mint account.
    let other_mint = c.mint(6);
    let mut ix = s.register_ix(&m, &wl.pubkey(), 0);
    ix.accounts[6].pubkey = other_mint;
    assert_err(c.send(vec![ix], &[&wl, &ss]), E::WrongMint);
    // A wallet token account of another mint, or not the wallet's own
    // (WP17: never as someone's delegate).
    let mut ix = s.register_ix(&m, &wl.pubkey(), 0);
    ix.accounts[4].pubkey = c.token_account(&other_mint, &wl.pubkey(), fee);
    assert_err(c.send(vec![ix], &[&wl, &ss]), E::WrongTokenAccount);
    let mut ix = s.register_ix(&m, &wl.pubkey(), 0);
    ix.accounts[4].pubkey = c.token_account(&s.mint, &payer.pubkey(), fee);
    assert_err(c.send(vec![ix], &[&wl, &ss]), E::WrongTokenAccount);
    // A vault that is not the season's PDA.
    let mut ix = s.register_ix(&m, &wl.pubkey(), 0);
    ix.accounts[5].pubkey = c.token_account(&s.mint, &s.season, 0);
    assert_err(c.send(vec![ix], &[&wl, &ss]), E::WrongPda);
    // The member PDA of another wallet.
    let mut ix = s.register_ix(&m, &wl.pubkey(), 0);
    ix.accounts[3].pubkey = s.member_pda(&payer.pubkey());
    assert_err(c.send(vec![ix], &[&wl, &ss]), E::WrongPda);
    // Not enough USDC: the token program refuses and no member is written.
    let poor = s.new_member(&mut c, 0, fee - 1);
    let (wp, sp) = (poor.wallet.insecure_clone(), poor.session.insecure_clone());
    assert_token_err(c.send(vec![s.register_ix(&poor, &wp.pubkey(), 0)], &[&wp, &sp]));
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
    let (wl2, sl2) = (late.wallet.insecure_clone(), late.session.insecure_clone());
    assert_err(
        c.send(vec![s.register_ix(&late, &wl2.pubkey(), 0)], &[&wl2, &sl2]),
        E::WrongStatus,
    );
    assert_eq!(c.balance(&s.vault), fee);
}

/// WP07: in a season with operator AI members every registration is paid
/// by the operator (admin or crank); a self-paid one is refused before any
/// transfer. Without AIs, a wallet pays for itself.
#[test]
fn register_is_operator_paid_in_ai_seasons() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create_ai(&mut c, Params::default(), &[0, 1], 1_000_000, 0);
    let m = s.new_member(&mut c, 0, s.p.fee);
    let (w, ss) = (m.wallet.insecure_clone(), m.session.insecure_clone());
    assert_err(
        c.send(vec![s.register_ix(&m, &w.pubkey(), 0)], &[&w, &ss]),
        E::Unauthorized,
    );
    assert_eq!(c.balance(&m.token), s.p.fee);
    for payer in [s.crank.insecure_clone(), s.admin.insecure_clone()] {
        let m = s.new_member(&mut c, 1, s.p.fee);
        let (w, ss) = (m.wallet.insecure_clone(), m.session.insecure_clone());
        c.send(
            vec![s.register_ix(&m, &payer.pubkey(), 0)],
            &[&payer, &w, &ss],
        )
        .expect("an operator-paid registration");
        s.members.push(m);
    }
    assert_eq!(s.season(&c).member_count, 2);
    // The last seat, then a full season, whoever pays.
    c.edit::<Season>(&s.season, |x| x.member_count = SEASON_MEMBER_CAP - 1);
    s.register(&mut c, 0, 0);
    let m = s.new_member(&mut c, 1, s.p.fee);
    let (w, ss, crank) = (
        m.wallet.insecure_clone(),
        m.session.insecure_clone(),
        s.crank.insecure_clone(),
    );
    assert_err(
        c.send(
            vec![s.register_ix(&m, &crank.pubkey(), 0)],
            &[&crank, &w, &ss],
        ),
        E::SeasonFull,
    );
    // No AIs: self-paid.
    let mut b = SeasonFx::create(
        &mut c,
        Params {
            id: 8,
            ..Params::default()
        },
    );
    b.register(&mut c, 0, 0);
    assert_eq!(b.season(&c).member_count, 1);
}

/// WP10: a season with operator AI members takes no pre-season votes, in
/// Register or UpdateMember; without AIs both take them.
#[test]
fn register_rejects_votes_in_ai_seasons() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create_ai(&mut c, Params::default(), &[0], 0, 0);
    let mut votes = [u32::MAX; 4];
    votes[0] = 0;
    let m = s.new_member(&mut c, 0, s.p.fee);
    let crank = s.crank.insecure_clone();
    let (w, ss) = (m.wallet.insecure_clone(), m.session.insecure_clone());
    let a = RegisterArgs {
        votes,
        ..s.register_args(&m, 0)
    };
    assert_err(
        c.send(
            vec![s.register_ix_from(&m, &crank.pubkey(), &a)],
            &[&crank, &w, &ss],
        ),
        E::InvalidParams,
    );
    let i = s.register(&mut c, 0, 0);
    let wallet = s.members[i].wallet.insecure_clone();
    assert_err(
        c.send(
            vec![s.update_member_ix(&wallet.pubkey(), i, 0x0f, votes)],
            &[&wallet],
        ),
        E::InvalidParams,
    );
    c.send(
        vec![s.update_member_ix(&wallet.pubkey(), i, 0x01, [u32::MAX; 4])],
        &[&wallet],
    )
    .expect("a candidacy without votes");
    // Without AIs, votes are taken.
    let mut b = SeasonFx::create(
        &mut c,
        Params {
            id: 8,
            ..Params::default()
        },
    );
    let j = b.register_keyed(
        &mut c,
        0,
        &solana_keypair::Keypair::new(),
        0x0f,
        votes,
        0,
        [0; 32],
    );
    assert_eq!(b.member(&c, j).votes, votes);
    let bw = b.members[j].wallet.insecure_clone();
    c.send(
        vec![b.update_member_ix(&bw.pubkey(), j, 0x0f, [1; 4])],
        &[&bw],
    )
    .expect("UpdateMember with votes");
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
    // A vote for a member the Season cannot hold (WP06).
    assert_err(
        c.send(
            vec![s.update_member_ix(&wallet.pubkey(), i, 1, [300, 0, 0, 0])],
            &[&wallet],
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

/// WP17: the session key co-signs Register (account 9), so nobody
/// registers a session key they do not hold.
#[test]
fn register_needs_the_session_signature() {
    let mut c = Chain::new();
    let s = SeasonFx::create(&mut c, Params::default());
    let m = s.new_member(&mut c, 0, s.p.fee);
    let wallet = m.wallet.insecure_clone();
    // The session key is passed, but does not sign.
    let mut ix = s.register_ix(&m, &wallet.pubkey(), 0);
    ix.accounts[9].is_signer = false;
    assert_err(c.send(vec![ix], &[&wallet]), E::MissingSignature);
    // Signed by both, it lands.
    let session = m.session.insecure_clone();
    c.send(
        vec![s.register_ix(&m, &wallet.pubkey(), 0)],
        &[&wallet, &session],
    )
    .expect("Register with the session's signature");
}

/// WP09 PostBond: the operator's, while registering, in seasons with AIs,
/// from a USDC account of the season's mint, capped at `MAX_BOND`.
#[test]
fn post_bond_checks() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create_ai(&mut c, Params::default(), &[0, 1], 1_000_000, 4_000_000);
    let admin = s.admin.insecure_clone();
    let crank = s.crank.insecure_clone();
    let source = c.token_account(&s.mint, &crank.pubkey(), 3_000_000);
    let post = |c: &mut Chain, who: &solana_keypair::Keypair, src, amount| {
        c.send(vec![s.post_bond_ix(&who.pubkey(), src, amount)], &[who])
    };
    // Not the operator.
    let outsider = c.funded();
    let theirs = c.token_account(&s.mint, &outsider.pubkey(), 1_000_000);
    assert_err(post(&mut c, &outsider, &theirs, 1), E::Unauthorized);
    // Nothing to add.
    assert_err(post(&mut c, &crank, &source, 0), E::InvalidParams);
    // Another mint's account; another mint account.
    let other_mint = c.mint(6);
    let wrong = c.token_account(&other_mint, &crank.pubkey(), 1_000_000);
    assert_err(post(&mut c, &crank, &wrong, 1), E::WrongTokenAccount);
    let mut ix = s.post_bond_ix(&crank.pubkey(), &source, 1);
    ix.accounts[4].pubkey = other_mint;
    assert_err(c.send(vec![ix], &[&crank]), E::WrongMint);
    // Above MAX_BOND.
    assert_err(
        post(&mut c, &crank, &source, MAX_BOND - 4_000_000 + 1),
        E::InvalidParams,
    );
    // More than the account holds: the token program refuses.
    assert_token_err(post(&mut c, &crank, &source, 3_000_001));
    // The crank (or the admin) posts: exactly `amount` moves.
    let vault = c.balance(&s.vault);
    let l = post(&mut c, &crank, &source, 2_000_000).expect("PostBond");
    assert_eq!(s.season(&c).bond, 6_000_000);
    assert_eq!(c.balance(&s.vault), vault + 2_000_000);
    assert_eq!(c.balance(&source), 1_000_000);
    assert!(l
        .logs
        .iter()
        .any(|x| x.contains("PS season 7 bond 6000000")));
    c.set_balance(&s.admin_token, 1);
    post(&mut c, &admin, &s.admin_token, 1).expect("the admin posts");
    assert_eq!(s.season(&c).bond, 6_000_001);
    // A season without AIs has no bond.
    let b = SeasonFx::create(
        &mut c,
        Params {
            id: 8,
            ..Params::default()
        },
    );
    let bc = b.crank.insecure_clone();
    let bs = c.token_account(&b.mint, &bc.pubkey(), 10);
    assert_err(
        c.send(vec![b.post_bond_ix(&bc.pubkey(), &bs, 1)], &[&bc]),
        E::InvalidParams,
    );
    // Only while registering.
    s.register_ai(&mut c, 0);
    s.register_ai(&mut c, 1);
    c.send(vec![s.start_ix(&crank.pubkey())], &[&crank])
        .expect("StartSeason (AIs only: the floor is 0)");
    assert_err(
        c.send(vec![s.post_bond_ix(&crank.pubkey(), &source, 1)], &[&crank]),
        E::WrongStatus,
    );
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
