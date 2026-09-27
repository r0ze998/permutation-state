//! WP09: RevealRoster is the operator's, only after the season is over with
//! its final world on base, restartable from 0 until complete, and the
//! roster commitment is blinded; a forfeited roster costs the operator at
//! least `forfeit_penalty` with the bond at its floor. Every call goes
//! through `processor::process`, as the deployed entrypoint dispatches it.
//! (Audit repros `roster_outsider_reveal.rs`, `roster_reveal_grief.rs`: the
//! outsider is refused, the operator's in-order reveal finishes Revealed.)
#![cfg(feature = "program")]

use borsh::BorshSerialize;
use permutation_chain::error::ChainError;
use permutation_chain::finalize::{finalize, Roster};
use permutation_chain::instruction::ChainInstruction;
use permutation_chain::processor::process;
use permutation_chain::state::*;
use permutation_chain::token::TOKEN_PROGRAM_ID;
use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::gov::{join, Path};
use permutation_rules::roster::{roster_chain, roster_commit, roster_tag};
use permutation_rules::state::WorldState;
use permutation_rules::{Preset, Ruleset};
use solana_program::{account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey};

const ID: u64 = 7;
const FEE: u64 = 10_000_000;
const BOUNTY: u64 = 4_000_000;
const AI_COUNT: u16 = 3;
const BLIND: [u8; 32] = [0xB1; 32];
const SALTS: [[u8; 32]; 3] = [[0xA0; 32], [0xA1; 32], [0xA2; 32]];

// Accounts: 0 crank · 1 outsider · 2 season · 3 roster · 4 vault · 5..25
// chunks · 25, 26, 27 the AIs (roster positions 0, 1, 2) · 28 a griefer
// (a self-made tag) · 29 a person.
const CRANK: usize = 0;
const OUTSIDER: usize = 1;
const SEASON: usize = 2;
const ROSTER: usize = 3;
const VAULT: usize = 4;
const WORLD: usize = 5;
const AI0: usize = WORLD + WORLD_CHUNKS;
const GRIEFER: usize = AI0 + 3;
const MEMBERS: usize = 5;

fn blob(space: usize, v: &impl BorshSerialize) -> Vec<u8> {
    let mut d = vec![0u8; space];
    store(&mut d, v).unwrap();
    d
}

fn finished_world(rules: &Ruleset) -> WorldState {
    let mut s = new_season(rules, &[1; 32], &[2; 32], &nation_entries(2)).unwrap();
    for (k, civ) in [0u16, 1, 0, 1, 0].into_iter().enumerate() {
        join(&mut s, rules, civ, [k as u8 + 1; 32]).unwrap();
    }
    for m in &mut s.members {
        m.windows = (1 << 18) - 1;
        m.merit[Path::Science as usize] = 1_000;
    }
    s.tick = rules.ticks_per_season;
    s
}

fn season(status: SeasonStatus, crank: [u8; 32], bump: u8, commit: [u8; 32]) -> Season {
    let n = MEMBERS as u64;
    let ops = n * FEE / 5;
    let mut s = Season {
        magic: SEASON_MAGIC,
        season_id: ID,
        bump,
        vault_bump: 0,
        admin: [9; 32],
        crank,
        usdc_mint: [0; 32],
        usdc_decimals: 6,
        preset: 0,
        nations: 2,
        entry_fee: FEE,
        tick_seconds: 30,
        market: true,
        status,
        world_seed: [1; 32],
        season_seed: [2; 32],
        member_count: MEMBERS as u32,
        nation_members: vec![3, 2],
        seated: MEMBERS as u32,
        pool: n * FEE - ops,
        ops,
        ops_withdrawn: false,
        treasury: vec![0, 0],
        treasury_final: vec![],
        payouts: vec![],
        final_root: [0; 32],
        prev_season_id: 0,
        prev_history_root: [0; 32],
        history_root: [0; 32],
        ai_count: AI_COUNT,
        roster_commit: commit,
        bounty_each: BOUNTY,
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
        stage_at: 0,
        rolled_back: 0,
        aborted_from: 0,
        validator: [4; 32],
        rules_version: permutation_chain::rules::PINNED_RULES_VERSION,
        rules_hash: permutation_chain::rules::pinned_ruleset_hash(0, true).unwrap(),
        logic_version: permutation_chain::rules::CHAIN_LOGIC_VERSION,
        created_slot: 0,
    };
    s.bond = bond_floor(&s);
    s
}

struct Fx {
    program: Pubkey,
    keys: Vec<Pubkey>,
    data: Vec<Vec<u8>>,
    lamports: Vec<u64>,
    owners: Vec<Pubkey>,
    signers: Vec<bool>,
    rules: Ruleset,
}

fn fixture(status: SeasonStatus, finished: bool) -> Fx {
    let program = Pubkey::new_unique();
    let rules = Ruleset::new(Preset::Blitz);
    let id = ID.to_le_bytes();
    let crank = Pubkey::new_unique();
    let wallets: Vec<[u8; 32]> = (0..MEMBERS)
        .map(|_| Pubkey::new_unique().to_bytes())
        .collect();
    let mut tags: Vec<[u8; 32]> = (0..3)
        .map(|i| roster_tag(ID, &wallets[i], &SALTS[i]))
        .collect();
    tags.push(roster_tag(ID, &wallets[3], &[0x66; 32])); // the griefer's own tag
    tags.push([0x77; 32]); // a person
    let (season_key, bump) = Pubkey::find_program_address(&[SEASON_SEED, &id], &program);
    let s = season(
        status,
        crank.to_bytes(),
        bump,
        roster_commit(&BLIND, &roster_chain(&tags[..3])),
    );
    let mut keys = vec![crank, Pubkey::new_unique(), season_key];
    let mut data = vec![vec![], vec![], blob(SEASON_SPACE, &s)];
    let mut owners = vec![Pubkey::default(), Pubkey::default(), program];
    let (roster_key, rb) = Pubkey::find_program_address(&[ROSTER_SEED, &id], &program);
    keys.push(roster_key);
    data.push(blob(
        roster_space(AI_COUNT),
        &RosterAccount {
            magic: ROSTER_MAGIC,
            season_id: ID,
            bump: rb,
            entries: vec![],
        },
    ));
    owners.push(program);
    // The vault: a token account of the season's mint, owned by the season,
    // holding everything paid in.
    keys.push(Pubkey::find_program_address(&[VAULT_SEED, &id], &program).0);
    let mut vault = vec![0u8; 165];
    vault[32..64].copy_from_slice(season_key.as_ref());
    let paid = permutation_chain::finalize::paid_in(&s) as u64;
    vault[64..72].copy_from_slice(&paid.to_le_bytes());
    vault[108] = 1;
    data.push(vault);
    owners.push(TOKEN_PROGRAM_ID);
    for k in 0..WORLD_CHUNKS {
        keys.push(Pubkey::find_program_address(&[WORLD_SEED, &id, &[k as u8]], &program).0);
        data.push(vec![0u8; CHUNK]);
        owners.push(program);
    }
    for (i, wallet) in wallets.iter().enumerate() {
        let (key, bump) = Pubkey::find_program_address(&[MEMBER_SEED, &id, wallet], &program);
        keys.push(key);
        data.push(blob(
            MEMBER_SPACE,
            &MemberAccount {
                magic: MEMBER_MAGIC,
                season_id: ID,
                bump,
                index: i as u32,
                civ: [0u16, 1, 0, 1, 0][i],
                wallet: *wallet,
                session: [i as u8 + 1; 32],
                kind: 2,
                name: format!("m{i}"),
                attestation: [0; 32],
                stand: 0,
                votes: [u32::MAX; 4],
                shares: 0,
                claimed: false,
                tag: tags[i],
            },
        ));
        owners.push(program);
    }
    let lamports = vec![1u64; keys.len()];
    let signers = vec![false; keys.len()];
    let mut fx = Fx {
        program,
        keys,
        data,
        lamports,
        owners,
        signers,
        rules,
    };
    // The final world on base.
    {
        let a = infos(&mut fx);
        let c = Chunks::new(&a[WORLD..WORLD + WORLD_CHUNKS]).unwrap();
        c.write_world(&finished_world(&Ruleset::new(Preset::Blitz)))
            .unwrap();
        c.set_meta(&WorldMeta {
            season_id: ID,
            civs: 2,
            tick_seconds: 30,
            deadline: 1_000,
            finished,
            market: true,
            ..Default::default()
        })
        .unwrap();
    }
    fx
}

fn infos(f: &mut Fx) -> Vec<AccountInfo<'_>> {
    f.keys
        .iter()
        .zip(f.lamports.iter_mut())
        .zip(f.data.iter_mut())
        .zip(f.owners.iter())
        .zip(f.signers.iter())
        .map(|((((k, l), d), o), s)| AccountInfo::new(k, *s, true, l, d, o, false))
        .collect()
}

fn reveal(
    f: &mut Fx,
    signer: usize,
    from: u16,
    members: &[usize],
    salts: &[[u8; 32]],
    blind: [u8; 32],
) -> Result<(), ProgramError> {
    f.signers = vec![false; f.keys.len()];
    f.signers[signer] = true;
    let program = f.program;
    let a = infos(f);
    let mut accts = vec![
        a[signer].clone(),
        a[SEASON].clone(),
        a[ROSTER].clone(),
        a[WORLD].clone(),
    ];
    accts.extend(members.iter().map(|&i| a[i].clone()));
    let ix = borsh::to_vec(&ChainInstruction::RevealRoster {
        from,
        salts: salts.to_vec(),
        blind,
    })
    .unwrap();
    process(&program, &accts, &ix)
}

fn finish(f: &mut Fx) -> Result<(), ProgramError> {
    f.signers = vec![false; f.keys.len()];
    let program = f.program;
    let a = infos(f);
    let mut accts = vec![a[SEASON].clone()];
    accts.extend(a[WORLD..WORLD + WORLD_CHUNKS].iter().cloned());
    accts.push(a[VAULT].clone());
    accts.push(a[ROSTER].clone());
    process(
        &program,
        &accts,
        &borsh::to_vec(&ChainInstruction::FinishSeason).unwrap(),
    )
}

fn season_of(f: &Fx) -> Season {
    load(&f.data[SEASON]).unwrap()
}

fn err(e: ChainError) -> Result<(), ProgramError> {
    Err(e.into())
}

#[test]
fn only_the_operator_reveals() {
    let mut f = fixture(SeasonStatus::Running, true);
    assert_eq!(
        reveal(&mut f, OUTSIDER, 0, &[AI0], &[SALTS[0]], BLIND),
        err(ChainError::Unauthorized)
    );
    // The griefer's self-made tag: its own reveal is refused (not the operator).
    assert_eq!(
        reveal(&mut f, OUTSIDER, 0, &[GRIEFER], &[[0x66; 32]], BLIND),
        err(ChainError::Unauthorized)
    );
    assert_eq!(season_of(&f).roster_revealed, 0);
}

#[test]
fn only_after_the_season_with_the_final_world_on_base() {
    for status in [
        SeasonStatus::Registering,
        SeasonStatus::Seeding,
        SeasonStatus::Genesis,
        SeasonStatus::Seating,
        SeasonStatus::Finalized,
        SeasonStatus::Aborted,
    ] {
        let mut f = fixture(status, true);
        assert_eq!(
            reveal(&mut f, CRANK, 0, &[AI0], &[SALTS[0]], BLIND),
            err(ChainError::WrongStatus),
            "{status:?}"
        );
    }
    let mut f = fixture(SeasonStatus::Running, false);
    assert_eq!(
        reveal(&mut f, CRANK, 0, &[AI0], &[SALTS[0]], BLIND),
        err(ChainError::SeasonNotOver)
    );
    let mut f = fixture(SeasonStatus::Running, true);
    f.owners[WORLD] = Pubkey::new_unique(); // chunk 0 still delegated
    assert_eq!(
        reveal(&mut f, CRANK, 0, &[AI0], &[SALTS[0]], BLIND),
        err(ChainError::WrongWorld)
    );
}

#[test]
fn an_honest_reveal_in_batches_finishes_with_the_roster() {
    let mut f = fixture(SeasonStatus::Running, true);
    reveal(&mut f, CRANK, 0, &[AI0, AI0 + 1], &SALTS[..2], [0; 32]).unwrap();
    // A replayed first batch, or a wrong position, is refused harmlessly.
    assert_eq!(
        reveal(&mut f, CRANK, 1, &[AI0 + 1], &[SALTS[1]], BLIND),
        err(ChainError::InvalidParams)
    );
    reveal(&mut f, CRANK, 2, &[AI0 + 2], &[SALTS[2]], BLIND).unwrap();
    let s = season_of(&f);
    assert_eq!((s.roster_revealed, s.roster_blind), (3, BLIND));
    // Complete is final: no restart, no more reveals.
    assert_eq!(
        reveal(&mut f, CRANK, 0, &[AI0], &[SALTS[0]], BLIND),
        err(ChainError::WrongStatus)
    );
    finish(&mut f).unwrap();
    let s = season_of(&f);
    assert_eq!(s.roster_outcome, ROSTER_REVEALED);
    assert!(!s.voided);
}

#[test]
fn a_wrong_order_or_blind_is_redone_from_zero() {
    let mut f = fixture(SeasonStatus::Running, true);
    // The operator's own mistake: position 2 first.
    reveal(&mut f, CRANK, 0, &[AI0 + 2], &[SALTS[2]], [0; 32]).unwrap();
    assert_eq!(
        reveal(&mut f, CRANK, 1, &[AI0, AI0 + 1], &SALTS[..2], BLIND),
        err(ChainError::RosterMismatch)
    );
    // A wrong blind fails the completing batch (and nothing of it is kept).
    assert_eq!(
        reveal(&mut f, CRANK, 0, &[AI0, AI0 + 1, AI0 + 2], &SALTS, [0; 32]),
        err(ChainError::RosterMismatch)
    );
    assert_eq!(season_of(&f).roster_revealed, 1);
    reveal(&mut f, CRANK, 0, &[AI0, AI0 + 1, AI0 + 2], &SALTS, BLIND).unwrap();
    finish(&mut f).unwrap();
    assert_eq!(season_of(&f).roster_outcome, ROSTER_REVEALED);
}

#[test]
fn the_commitment_hides_the_set_without_the_blind() {
    let f = fixture(SeasonStatus::Running, true);
    let s = season_of(&f);
    let tags: Vec<[u8; 32]> = (AI0..AI0 + 3)
        .map(|i| load::<MemberAccount>(&f.data[i]).unwrap().tag)
        .collect();
    assert_ne!(
        roster_chain(&tags),
        s.roster_commit,
        "the plain chain is not what is stored"
    );
    assert_eq!(roster_commit(&BLIND, &roster_chain(&tags)), s.roster_commit);
}

/// With the bond at its floor, a forfeited roster (equal split) never pays
/// the operator's AIs more than the bond less the penalty, whatever the
/// merit.
#[test]
fn forfeiting_never_pays_the_operator_at_the_floor() {
    let f = fixture(SeasonStatus::Running, true);
    let rules = f.rules.clone();
    let mut world = finished_world(&rules);
    for (i, m) in world.members.iter_mut().enumerate() {
        m.merit[Path::Science as usize] = if i < 3 { 50_000 } else { 1 }; // AIs dominate merit
    }
    let season = season_of(&f);
    let forf = finalize(&season, &world, &rules, Roster::Forfeited, false);
    assert!(!forf.voided);
    let ai_take: u64 = forf.payouts[..3].iter().sum();
    let each = forf.payouts[0];
    assert!(forf.payouts.iter().all(|x| *x == each), "equal split");
    assert!(
        ai_take <= season.bond,
        "AIs {ai_take} > bond {}",
        season.bond
    );
    // Withholding costs the operator at least the penalty (its AIs' fees).
    let penalty = forfeit_penalty(&season);
    assert!(
        season.bond - ai_take >= penalty,
        "cost {} < penalty {penalty}",
        season.bond - ai_take
    );
    // People as a whole receive at least the penalty more than a reveal
    // could pay them (pool + bounties), up to rounding.
    let people: u64 = forf.payouts[3..].iter().sum();
    let paid = season.pool + season.bounty_each * AI_COUNT as u64;
    assert!(
        people + MEMBERS as u64 >= paid + penalty,
        "people {people} < {}",
        paid + penalty
    );
}
