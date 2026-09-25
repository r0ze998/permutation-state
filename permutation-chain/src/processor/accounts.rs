//! Account checks and loaders shared by every instruction.

use permutation_rules::{Preset, Ruleset};
use solana_program::{
    account_info::AccountInfo,
    entrypoint::ProgramResult,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    sysvar::Sysvar,
};
use solana_system_interface::{instruction as system_instruction, program as system_program};

use crate::error::ChainError;
use crate::state::*;
use crate::token;

/// The season's ruleset: the preset, with the market switched on or off (V5 §7.5).
pub(super) fn rules_for(preset: u8, market: bool) -> Result<Ruleset, ProgramError> {
    let mut rules = match preset {
        0 => Ruleset::new(Preset::Blitz),
        1 => Ruleset::new(Preset::Season),
        _ => return Err(ChainError::InvalidParams.into()),
    };
    rules.market_enabled = market;
    Ok(rules)
}

pub(super) fn signer(a: &AccountInfo) -> ProgramResult {
    if a.is_signer {
        Ok(())
    } else {
        Err(ChainError::MissingSignature.into())
    }
}

pub(super) fn expect_pda(
    program_id: &Pubkey,
    account: &AccountInfo,
    seeds: &[&[u8]],
) -> Result<u8, ProgramError> {
    let (pda, bump) = Pubkey::find_program_address(seeds, program_id);
    if pda != *account.key {
        return Err(ChainError::WrongPda.into());
    }
    Ok(bump)
}

/// Claim a system-owned PDA even if a third party pre-funded it (defeats the
/// one-lamport squatting attack), then assign it to `owner`.
pub(super) fn init_pda<'a>(
    payer: &AccountInfo<'a>,
    pda: &AccountInfo<'a>,
    space: usize,
    owner: &Pubkey,
    seeds: &[&[u8]],
) -> ProgramResult {
    if pda.owner != &system_program::id() || !pda.data_is_empty() {
        return Err(ChainError::AlreadyInitialized.into());
    }
    let need = Rent::get()?
        .minimum_balance(space)
        .saturating_sub(pda.lamports());
    if need > 0 {
        invoke(
            &system_instruction::transfer(payer.key, pda.key, need),
            &[payer.clone(), pda.clone()],
        )?;
    }
    invoke_signed(
        &system_instruction::allocate(pda.key, space as u64),
        std::slice::from_ref(pda),
        &[seeds],
    )?;
    invoke_signed(
        &system_instruction::assign(pda.key, owner),
        std::slice::from_ref(pda),
        &[seeds],
    )
}

pub(super) fn season_seeds(id: &[u8; 8]) -> [&[u8]; 2] {
    [SEASON_SEED, id]
}

pub(super) fn load_season(
    program_id: &Pubkey,
    account: &AccountInfo,
) -> Result<Season, ProgramError> {
    if account.owner != program_id {
        return Err(ChainError::NotInitialized.into());
    }
    let s: Season = load(&account.try_borrow_data()?)?;
    if s.magic != SEASON_MAGIC {
        return Err(ChainError::NotInitialized.into());
    }
    expect_pda(
        program_id,
        account,
        &season_seeds(&s.season_id.to_le_bytes()),
    )?;
    Ok(s)
}

/// A nation account, checked to be the PDA `["nation", season, civ]` it
/// claims to be. The owner and magic alone are not enough: world chunks after
/// the first are program-owned raw bytes, partly shaped by player input.
/// Uses the stored bump, so it costs one hash rather than a bump search.
pub(super) fn load_nation_at(
    program_id: &Pubkey,
    account: &AccountInfo,
) -> Result<NationAccount, ProgramError> {
    if account.owner != program_id {
        return Err(ChainError::MissingNation.into());
    }
    let n: NationAccount = load(&account.try_borrow_data()?)?;
    if n.magic != NATION_MAGIC {
        return Err(ChainError::MissingNation.into());
    }
    let seeds: [&[u8]; 4] = [
        NATION_SEED,
        &n.season_id.to_le_bytes(),
        &n.civ.to_le_bytes(),
        &[n.bump],
    ];
    if Pubkey::create_program_address(&seeds, program_id)
        .ok()
        .as_ref()
        != Some(account.key)
    {
        return Err(ChainError::WrongPda.into());
    }
    Ok(n)
}

/// The nation account of `civ` in `season_id`.
pub(super) fn load_nation(
    program_id: &Pubkey,
    account: &AccountInfo,
    season_id: u64,
    civ: u16,
) -> Result<NationAccount, ProgramError> {
    let n = load_nation_at(program_id, account)?;
    if n.season_id != season_id || n.civ != civ {
        return Err(ChainError::MissingNation.into());
    }
    Ok(n)
}

pub(super) fn load_member(
    program_id: &Pubkey,
    account: &AccountInfo,
    season_id: u64,
) -> Result<MemberAccount, ProgramError> {
    if account.owner != program_id {
        return Err(ChainError::NotInitialized.into());
    }
    let m: MemberAccount = load(&account.try_borrow_data()?)?;
    if m.magic != MEMBER_MAGIC || m.season_id != season_id {
        return Err(ChainError::NotInitialized.into());
    }
    expect_pda(
        program_id,
        account,
        &[MEMBER_SEED, &season_id.to_le_bytes(), &m.wallet],
    )?;
    Ok(m)
}

/// The roster account `["roster", season]` (V5 §18.2).
pub(super) fn load_roster(
    program_id: &Pubkey,
    account: &AccountInfo,
    season_id: u64,
) -> Result<RosterAccount, ProgramError> {
    if account.owner != program_id {
        return Err(ChainError::NotInitialized.into());
    }
    let r: RosterAccount = load(&account.try_borrow_data()?)?;
    if r.magic != ROSTER_MAGIC || r.season_id != season_id {
        return Err(ChainError::NotInitialized.into());
    }
    expect_pda(
        program_id,
        account,
        &[ROSTER_SEED, &season_id.to_le_bytes()],
    )?;
    Ok(r)
}

/// The `WORLD_CHUNKS` world accounts at the front of `accounts`, checked.
pub(super) fn world_chunks<'a, 'info>(
    program_id: &Pubkey,
    accounts: &'a [AccountInfo<'info>],
    season_id: u64,
) -> Result<Chunks<'a, 'info>, ProgramError> {
    let list = accounts
        .get(..WORLD_CHUNKS)
        .ok_or(ChainError::WorldTooSmall)?;
    let id = season_id.to_le_bytes();
    for (k, a) in list.iter().enumerate() {
        if a.owner != program_id {
            return Err(ChainError::WrongWorld.into());
        }
        expect_pda(program_id, a, &[WORLD_SEED, &id, &[k as u8]])?;
    }
    Chunks::new(list)
}

/// Season id recorded in world chunk 0 (to find the other accounts).
pub(super) fn world_season_id(chunk0: &AccountInfo) -> Result<u64, ProgramError> {
    let d = chunk0.try_borrow_data()?;
    Ok(u64::from_le_bytes(
        d.get(12..20)
            .ok_or(ChainError::WorldTooSmall)?
            .try_into()
            .unwrap(),
    ))
}

/// Only the season's crank may commit during play: MagicBlock sponsors a
/// limited number of commits per delegated account, and the last one is
/// kept for the final undelegation. Every nation account of the season
/// records the crank's key, and they stay delegated for the whole of play.
pub(super) fn require_crank(
    program_id: &Pubkey,
    nation_ai: &AccountInfo,
    season_id: u64,
    payer: &AccountInfo,
) -> ProgramResult {
    let n = load_nation_at(program_id, nation_ai)?;
    if n.season_id != season_id {
        return Err(ChainError::MissingNation.into());
    }
    if payer.key.as_ref() != n.crank {
        return Err(ChainError::Unauthorized.into());
    }
    Ok(())
}

/// The season's admin or crank: they run genesis, seating and delegation.
pub(super) fn require_operator(season: &Season, authority: &AccountInfo) -> ProgramResult {
    let key = authority.key.as_ref();
    if key == season.admin || key == season.crank {
        Ok(())
    } else {
        Err(ChainError::Unauthorized.into())
    }
}

/// The season's USDC mint, the token program and the season's vault.
pub(super) fn check_usdc(
    program_id: &Pubkey,
    season: &Season,
    mint: &AccountInfo,
    token_program: &AccountInfo,
    vault: &AccountInfo,
) -> ProgramResult {
    if token_program.key != &token::TOKEN_PROGRAM_ID || mint.key.as_ref() != season.usdc_mint {
        return Err(ChainError::WrongMint.into());
    }
    expect_pda(
        program_id,
        vault,
        &[VAULT_SEED, &season.season_id.to_le_bytes()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use permutation_rules::gov::NOBODY;

    fn nation(season_id: u64, civ: u16, bump: u8) -> Vec<u8> {
        let n = NationAccount {
            magic: NATION_MAGIC,
            season_id,
            civ,
            bump,
            preset: 0,
            market: false,
            crank: [0; 32],
            open_tick: 0,
            officers: [NOBODY; 4],
            keys: [[0; 32]; 4],
            spendable: [0; 4],
            submitted: [u16::MAX; 4],
            frozen: false,
            revealing: false,
            reveal_deadline: 0,
            committed: [u16::MAX; 4],
            commits: [[0; 32]; 4],
            salts: [[0; 32]; 4],
            batches: [None, None, None, None],
            inbox: Vec::new(),
        };
        let mut data = vec![0u8; NATION_SPACE];
        store(&mut data, &n).unwrap();
        data
    }

    #[test]
    fn only_the_crank_commits() {
        let program = Pubkey::new_unique();
        let (pda, bump) = Pubkey::find_program_address(
            &[NATION_SEED, &7u64.to_le_bytes(), &0u16.to_le_bytes()],
            &program,
        );
        let crank = Pubkey::new_unique();
        let mut n: NationAccount = load(&nation(7, 0, bump)).unwrap();
        n.crank = crank.to_bytes();
        let mut data = vec![0u8; NATION_SPACE];
        store(&mut data, &n).unwrap();
        let (mut l, mut l1, mut l2) = (0u64, 0u64, 0u64);
        let nation_ai = AccountInfo::new(&pda, false, false, &mut l, &mut data, &program, false);
        let mut none: [u8; 0] = [];
        let mut none2: [u8; 0] = [];
        let owner = Pubkey::default();
        let stranger_key = Pubkey::new_unique();
        let crank_ai = AccountInfo::new(&crank, true, true, &mut l1, &mut none, &owner, false);
        let stranger = AccountInfo::new(
            &stranger_key,
            true,
            true,
            &mut l2,
            &mut none2,
            &owner,
            false,
        );
        assert!(require_crank(&program, &nation_ai, 7, &crank_ai).is_ok());
        assert_eq!(
            require_crank(&program, &nation_ai, 7, &stranger).unwrap_err(),
            ChainError::Unauthorized.into()
        );
        assert_eq!(
            require_crank(&program, &nation_ai, 8, &crank_ai).unwrap_err(),
            ChainError::MissingNation.into()
        );
    }

    /// A program-owned account holding a well-formed nation is not a nation
    /// unless it is that nation's PDA (world chunks are program-owned too).
    #[test]
    fn nations_are_checked_by_address() {
        let program = Pubkey::new_unique();
        let (pda, bump) = Pubkey::find_program_address(
            &[NATION_SEED, &7u64.to_le_bytes(), &2u16.to_le_bytes()],
            &program,
        );
        let impostor = Pubkey::new_unique();
        let (mut l1, mut l2) = (0u64, 0u64);
        let (mut d1, mut d2) = (nation(7, 2, bump), nation(7, 2, bump));
        let real = AccountInfo::new(&pda, false, true, &mut l1, &mut d1, &program, false);
        let fake = AccountInfo::new(&impostor, false, true, &mut l2, &mut d2, &program, false);
        assert!(load_nation(&program, &real, 7, 2).is_ok());
        assert_eq!(
            load_nation(&program, &fake, 7, 2).unwrap_err(),
            ChainError::WrongPda.into()
        );
        assert_eq!(
            load_nation(&program, &real, 7, 3).unwrap_err(),
            ChainError::MissingNation.into()
        );
        let other = Pubkey::new_unique();
        let (mut l3, mut d3) = (0u64, nation(7, 2, bump));
        let foreign = AccountInfo::new(&pda, false, true, &mut l3, &mut d3, &other, false);
        assert_eq!(
            load_nation_at(&program, &foreign).unwrap_err(),
            ChainError::MissingNation.into()
        );
    }
}
