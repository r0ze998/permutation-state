//! The seating freeze (audit repro `seating_freeze`, flipped): during
//! Seating the world's tick-0 accounts are program-owned PDAs on the base
//! layer, and HEAD's permissionless ResolveTick could advance
//! `phase_cursor` there, after which SeatMembers and OpenGovernment failed
//! forever (the vault locked). Now ResolveTick needs every nation's open
//! tick to be the world's (`WrongTick` while the government is not open),
//! and it must be alone in its transaction (`NotAlone` without the
//! Instructions sysvar), so the world stays at phase 0 and seating goes on.
//!
//! Driven through the program's dispatcher on a Seating season built with
//! the program's own layouts, the frozen input already published and the
//! randomness drawn: the strongest position an attacker could reach.

mod play_fixture;

use permutation_chain::error::ChainError;
use permutation_chain::instruction::ChainInstruction;
use permutation_chain::processor::set_host_clock;
use permutation_chain::randomness::RAND_VRF;
use permutation_chain::state::*;
use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::gov::{Role, NOBODY};
use play_fixture::*;
use solana_program::program_error::ProgramError;
use solana_program::pubkey::Pubkey;

const NATIONS: u8 = 2;
const MEMBERS: usize = 3;

fn season(program: &Pubkey, operator: &Pubkey) -> Season {
    let (_, bump) = Pubkey::find_program_address(&[SEASON_SEED, &ID.to_le_bytes()], program);
    Season {
        magic: SEASON_MAGIC,
        season_id: ID,
        bump,
        vault_bump: 0,
        admin: operator.to_bytes(),
        crank: operator.to_bytes(),
        usdc_mint: [3; 32],
        usdc_decimals: 6,
        preset: PRESET_BLITZ,
        nations: NATIONS,
        entry_fee: 1,
        tick_seconds: 30,
        market: true,
        status: SeasonStatus::Seating,
        world_seed: [1; 32],
        season_seed: [2; 32],
        member_count: MEMBERS as u32,
        nation_members: vec![2, 1],
        seated: 0,
        pool: 0,
        ops: 0,
        ops_withdrawn: false,
        treasury: vec![0, 0],
        treasury_final: vec![],
        payouts: vec![],
        final_root: [0; 32],
        prev_season_id: 0,
        prev_history_root: [0; 32],
        history_root: [0; 32],
        ai_count: 0,
        roster_commit: [0; 32],
        bounty_each: 0,
        bond: 0,
        roster_acc: [0; 32],
        roster_revealed: 0,
        roster_outcome: 0,
        bounty_paid: vec![],
        delegated: 0,
        roster_blind: [0; 32],
        refund_base: vec![],
        refund_in_payout: vec![],
        seed_state: 0,
        seed_oracle: [0; 32],
        seed_requested_at: 0,
        seed_requests: 0,
        deposit: 0,
        outstanding: 0,
        voided: false,
        start_by: 0,
        stage_at: 1_000,
        rolled_back: 0,
        aborted_from: 0,
        validator: [5; 32],
        rules_version: permutation_chain::rules::PINNED_RULES_VERSION,
        rules_hash: permutation_chain::rules::pinned_ruleset_hash(PRESET_BLITZ, true).unwrap(),
        logic_version: permutation_chain::rules::CHAIN_LOGIC_VERSION,
        created_slot: 0,
    }
}

fn member(program: &Pubkey, i: usize) -> MemberAccount {
    let wallet = [0xc0 + i as u8; 32];
    let (_, bump) =
        Pubkey::find_program_address(&[MEMBER_SEED, &ID.to_le_bytes(), &wallet], program);
    MemberAccount {
        magic: MEMBER_MAGIC,
        season_id: ID,
        bump,
        index: i as u32,
        civ: [0, 0, 1][i],
        wallet,
        session: [0xa0 + i as u8; 32],
        kind: 2,
        name: format!("member {i}"),
        attestation: [0; 32],
        stand: 0x0f,
        votes: [NOBODY; 4],
        shares: 0,
        claimed: false,
        tag: [0; 32],
    }
}

/// The accounts, in this order: operator, season, the 20 world chunks, the
/// nations, the members, the Instructions sysvar.
struct Seating {
    program: Pubkey,
    a: Accounts,
}

const OP: usize = 0;
const SEASON: usize = 1;
const WORLD0: usize = 2;
const NATION0: usize = WORLD0 + WORLD_CHUNKS;
const MEMBER0: usize = NATION0 + NATIONS as usize;
const SYSVAR: usize = MEMBER0 + MEMBERS;

impl Seating {
    /// A Seating season whose world (genesis done, nobody seated) carries
    /// a frozen, published tick-0 input with its randomness drawn.
    fn new() -> Seating {
        set_host_clock(2_000);
        let program = Pubkey::new_unique();
        let operator = Pubkey::new_unique();
        let id = ID.to_le_bytes();
        let mut a = Accounts::default();
        a.push(operator, Pubkey::default(), true, vec![]);
        let key = Pubkey::find_program_address(&[SEASON_SEED, &id], &program).0;
        let mut d = vec![0u8; SEASON_SPACE];
        store(&mut d, &season(&program, &operator)).unwrap();
        a.push(key, program, false, d);
        for k in 0..WORLD_CHUNKS as u8 {
            let key = Pubkey::find_program_address(&[WORLD_SEED, &id, &[k]], &program).0;
            a.push(key, program, false, vec![0u8; CHUNK]);
        }
        for civ in 0..NATIONS as u16 {
            let (key, bump) =
                Pubkey::find_program_address(&[NATION_SEED, &id, &civ.to_le_bytes()], &program);
            let n = NationAccount::new(ID, civ, bump, PRESET_BLITZ, true, operator.to_bytes());
            a.push(key, program, false, nation_data(&n));
        }
        for i in 0..MEMBERS {
            let m = member(&program, i);
            let key = Pubkey::find_program_address(&[MEMBER_SEED, &id, &m.wallet], &program).0;
            let mut d = vec![0u8; MEMBER_SPACE];
            store(&mut d, &m).unwrap();
            a.push(key, program, false, d);
        }
        let resolve = ChainInstruction::ResolveTick { to: 1 };
        a.push(
            solana_program::sysvar::instructions::ID,
            Pubkey::default(),
            false,
            alone(&program, &resolve),
        );
        let mut s = Seating { program, a };
        s.with_world(|w| {
            let rules = rules();
            let state = new_season(&rules, &[1; 32], &[2; 32], &nation_entries(2)).unwrap();
            w.write_world(&state).unwrap();
            w.set_meta(&WorldMeta {
                season_id: ID,
                preset: PRESET_BLITZ,
                civs: NATIONS,
                tick_seconds: 30,
                market: true,
                revealing: true,
                frozen: true,
                input_chunks: 1,
                input_logged: 1,
                rand_state: RAND_VRF,
                rand_tick: 0,
                vrf: [7; 32],
                ..Default::default()
            })
            .unwrap();
        });
        s
    }

    fn with_world<R>(&mut self, f: impl FnOnce(&Chunks) -> R) -> R {
        let infos = self.a.infos();
        f(&Chunks::new(&infos[WORLD0..NATION0]).unwrap())
    }

    fn run(&mut self, at: &[usize], ix: ChainInstruction) -> Result<(), ProgramError> {
        run(&self.program, &mut self.a, at, &ix)
    }

    /// ResolveTick's accounts: the chunks, the nations and (with `sysvar`)
    /// the Instructions sysvar.
    fn resolve(&mut self, sysvar: bool) -> Result<(), ProgramError> {
        let mut at: Vec<usize> = (WORLD0..MEMBER0).collect();
        if sysvar {
            at.push(SYSVAR);
        }
        self.run(&at, ChainInstruction::ResolveTick { to: 1 })
    }

    fn phase_cursor(&mut self) -> u8 {
        self.with_world(|w| w.read_world().unwrap().phase_cursor)
    }
}

#[test]
fn resolve_tick_cannot_freeze_seating() {
    let mut s = Seating::new();
    assert_eq!(s.phase_cursor(), 0);
    // Without the sysvar the alone rule refuses first; with it, the gate:
    // no nation has tick 0 open.
    assert_eq!(s.resolve(false), Err(ChainError::NotAlone.into()));
    assert_eq!(s.resolve(true), Err(ChainError::WrongTick.into()));
    assert_eq!(s.phase_cursor(), 0, "the world did not move");
    // The same call from inside another program's instruction (a CPI) is
    // not alone either.
    let other = solana_program::instruction::Instruction {
        program_id: Pubkey::new_unique(),
        accounts: vec![],
        data: vec![],
    };
    s.a.data[SYSVAR] = sysvar_data(&[other], 0);
    assert_eq!(s.resolve(true), Err(ChainError::NotAlone.into()));

    // Seating goes on: every member is seated, then the government opens.
    let mut at = vec![OP, SEASON];
    at.extend(WORLD0..NATION0);
    at.extend(MEMBER0..SYSVAR);
    s.run(&at, ChainInstruction::SeatMembers)
        .expect("SeatMembers");
    let seated: Season = s.a.load(SEASON);
    assert_eq!(seated.seated, MEMBERS as u32);
    assert_eq!(
        s.with_world(|w| w.read_world().unwrap().members.len()),
        MEMBERS
    );
    let mut at = vec![OP, SEASON];
    at.extend(WORLD0..MEMBER0);
    match s.run(&at, ChainInstruction::OpenGovernment) {
        Ok(()) => {
            let season: Season = s.a.load(SEASON);
            assert_eq!(season.status, SeasonStatus::Running);
            let n: NationAccount = s.a.load(NATION0);
            assert_eq!(n.open_tick, 0);
            assert!(Role::ALL.iter().any(|r| n.officers[r.index()] != NOBODY));
        }
        // An OpenGovernment that reads the Clock sysvar itself (no host
        // clock): the first election passed; only the clock stops it here.
        Err(ProgramError::UnsupportedSysvar) => {}
        Err(e) => panic!("OpenGovernment: {e:?}"),
    }
}
