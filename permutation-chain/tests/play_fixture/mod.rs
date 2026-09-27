//! Host fixtures for the processor's play tests: accounts held in memory
//! and handed to the real dispatcher (`processor::process`) as
//! `AccountInfo`s, the Instructions sysvar of a transaction, and the clock
//! (`set_host_clock`, the library is built with `host-clock` here).

#![allow(dead_code)]

use permutation_chain::instruction::ChainInstruction;
use permutation_chain::processor::process;
use permutation_chain::state::*;
use permutation_rules::gov::{Role, NOBODY};
use permutation_rules::state::WorldState;
use permutation_rules::{Preset, Ruleset};
use solana_program::account_info::AccountInfo;
use solana_program::entrypoint::ProgramResult;
use solana_program::instruction::{AccountMeta, Instruction};
use solana_program::pubkey::Pubkey;

pub const ID: u64 = 7;

/// The Blitz ruleset with the market, as `rules_for(PRESET_BLITZ, true)`.
pub fn rules() -> Ruleset {
    let mut r = Ruleset::new(Preset::Blitz);
    r.market_enabled = true;
    r
}

/// Accounts in memory, in the order they are pushed.
#[derive(Default)]
pub struct Accounts {
    pub keys: Vec<Pubkey>,
    pub owners: Vec<Pubkey>,
    pub signers: Vec<bool>,
    pub lamports: Vec<u64>,
    pub data: Vec<Vec<u8>>,
}

impl Accounts {
    pub fn push(&mut self, key: Pubkey, owner: Pubkey, signer: bool, data: Vec<u8>) -> usize {
        self.keys.push(key);
        self.owners.push(owner);
        self.signers.push(signer);
        self.lamports.push(1_000_000_000);
        self.data.push(data);
        self.keys.len() - 1
    }

    /// Every account, writable, as the runtime hands them to the program.
    pub fn infos(&mut self) -> Vec<AccountInfo<'_>> {
        let owners = &self.owners;
        self.keys
            .iter()
            .zip(self.lamports.iter_mut())
            .zip(self.data.iter_mut())
            .enumerate()
            .map(|(i, ((k, l), d))| {
                AccountInfo::new(
                    k,
                    self.signers[i],
                    true,
                    l,
                    d.as_mut_slice(),
                    &owners[i],
                    false,
                )
            })
            .collect()
    }

    pub fn load<T: borsh::BorshDeserialize>(&self, i: usize) -> T {
        load(&self.data[i]).unwrap()
    }

    pub fn store<T: borsh::BorshSerialize>(&mut self, i: usize, v: &T) {
        store(&mut self.data[i], v).unwrap();
    }
}

/// Runs `ix` through the program's dispatcher on the accounts at `at`.
pub fn run(
    program: &Pubkey,
    a: &mut Accounts,
    at: &[usize],
    ix: &ChainInstruction,
) -> ProgramResult {
    let infos = a.infos();
    let list: Vec<AccountInfo> = at.iter().map(|i| infos[*i].clone()).collect();
    process(program, &list, &borsh::to_vec(ix).unwrap())
}

/// The Instructions sysvar's data for a transaction of `ixs`, executing `current`.
#[allow(deprecated)]
pub fn sysvar_data(ixs: &[Instruction], current: u16) -> Vec<u8> {
    use solana_program::sysvar::instructions::{
        construct_instructions_data, BorrowedAccountMeta, BorrowedInstruction,
    };
    let borrowed: Vec<BorrowedInstruction> = ixs
        .iter()
        .map(|ix| BorrowedInstruction {
            program_id: &ix.program_id,
            accounts: ix
                .accounts
                .iter()
                .map(|m| BorrowedAccountMeta {
                    pubkey: &m.pubkey,
                    is_signer: m.is_signer,
                    is_writable: m.is_writable,
                })
                .collect(),
            data: &ix.data,
        })
        .collect();
    let mut d = construct_instructions_data(&borrowed);
    let n = d.len();
    d[n - 2..].copy_from_slice(&current.to_le_bytes());
    d
}

/// The sysvar of a transaction holding `ix` of `program` alone.
pub fn alone(program: &Pubkey, ix: &ChainInstruction) -> Vec<u8> {
    let only = Instruction {
        program_id: *program,
        accounts: vec![AccountMeta::new(Pubkey::new_unique(), false)],
        data: borsh::to_vec(ix).unwrap(),
    };
    sysvar_data(&[only], 0)
}

/// A nation account `["nation", ID, civ]` of `program`, open on `tick` with
/// `state`'s offices, keys and roll, closing at `deadline`.
pub fn open_nation(
    program: &Pubkey,
    state: &WorldState,
    civ: u16,
    tick: u16,
    deadline: i64,
) -> (Pubkey, NationAccount) {
    let (key, bump) = Pubkey::find_program_address(
        &[NATION_SEED, &ID.to_le_bytes(), &civ.to_le_bytes()],
        program,
    );
    let mut n = NationAccount::new(ID, civ, bump, PRESET_BLITZ, true, [9; 32]);
    let nation = &state.nations[civ as usize];
    n.open_tick = tick;
    for role in Role::ALL {
        let i = role.index();
        let m = nation.offices[i];
        n.officers[i] = m;
        n.keys[i] = state.members.get(m as usize).map_or([0; 32], |x| x.key);
        n.spendable[i] = if m == NOBODY { 0 } else { 100 };
    }
    n.deadline = deadline;
    n.seat_roll(state);
    (key, n)
}

/// `data` as a `NATION_SPACE` account.
pub fn nation_data(n: &NationAccount) -> Vec<u8> {
    let mut d = vec![0u8; NATION_SPACE];
    store(&mut d, n).unwrap();
    d
}

/// A tick-0 Blitz world of two nations with members holding `keys` (member
/// `i` in nation `i % 2`, standing for every office), after the first
/// election.
pub fn world(keys: &[[u8; 32]]) -> WorldState {
    use permutation_rules::genesis::{nation_entries, new_season};
    use permutation_rules::gov::{self, GovAction, GovEntry};
    let rules = rules();
    let mut s = new_season(&rules, &[1; 32], &[2; 32], &nation_entries(2)).unwrap();
    for (i, key) in keys.iter().enumerate() {
        let id = gov::join(&mut s, &rules, (i % 2) as u16, *key).unwrap();
        gov::apply_pre_season(
            &mut s,
            &rules,
            &[GovEntry {
                member: id,
                signer: *key,
                action: GovAction::Stand { roles: 0x0f },
            }],
        )
        .unwrap();
    }
    gov::first_election(&mut s, &rules).unwrap();
    s
}
