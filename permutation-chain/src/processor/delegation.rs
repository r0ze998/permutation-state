//! The Ephemeral Rollup lifecycle: delegating the world and nation
//! accounts, committing them back to the base layer and undelegating them.

use ephemeral_rollups_sdk::{
    consts::{DELEGATION_PROGRAM_ID, MAGIC_CONTEXT_ID, MAGIC_PROGRAM_ID},
    cpi::{delegate_account, undelegate_account, DelegateAccounts, DelegateConfig},
    ephem::{FoldableIntentBuilder, MagicIntentBundleBuilder},
};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
};
use solana_system_interface::program as system_program;

use super::accounts::*;
use super::{ER_COMMIT_FREQUENCY_MS, NATION_TARGET};
use crate::error::ChainError;
use crate::state::*;

pub(super) fn delegate(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    target: u16,
) -> ProgramResult {
    let it = &mut accounts.iter();
    let authority = next_account_info(it)?;
    let system = next_account_info(it)?;
    let season_ai = next_account_info(it)?;
    let pda = next_account_info(it)?;
    let owner_program = next_account_info(it)?;
    let buffer = next_account_info(it)?;
    let record = next_account_info(it)?;
    let metadata = next_account_info(it)?;
    let delegation_program = next_account_info(it)?;
    let validator = it.next();
    signer(authority)?;
    let season = load_season(program_id, season_ai)?;
    require_operator(&season, authority)?;
    if season.status != SeasonStatus::Running {
        return Err(ChainError::WrongStatus.into());
    }
    if owner_program.key != program_id || system.key != &system_program::id() {
        return Err(ProgramError::IncorrectProgramId);
    }
    if delegation_program.key != &DELEGATION_PROGRAM_ID {
        return Err(ChainError::WrongDelegationProgram.into());
    }
    let id = season.season_id.to_le_bytes();
    let civ_bytes;
    let chunk;
    let seeds: Vec<&[u8]> = if (target as usize) < WORLD_CHUNKS {
        chunk = [target as u8];
        vec![WORLD_SEED, &id, &chunk]
    } else if target >= NATION_TARGET && target - NATION_TARGET < season.nations as u16 {
        civ_bytes = (target - NATION_TARGET).to_le_bytes();
        vec![NATION_SEED, &id, &civ_bytes]
    } else {
        return Err(ChainError::InvalidParams.into());
    };
    expect_pda(program_id, pda, &seeds)?;
    delegate_account(
        DelegateAccounts {
            payer: authority,
            pda,
            owner_program,
            buffer,
            delegation_record: record,
            delegation_metadata: metadata,
            delegation_program,
            system_program: system,
        },
        &seeds,
        DelegateConfig {
            commit_frequency_ms: ER_COMMIT_FREQUENCY_MS,
            validator: validator.map(|v| *v.key),
        },
    )?;
    msg!("PS delegated {} target {}", pda.key, target);
    Ok(())
}

pub(super) fn undelegate_callback(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    seeds: Vec<Vec<u8>>,
) -> ProgramResult {
    let it = &mut accounts.iter();
    let pda = next_account_info(it)?;
    let buffer = next_account_info(it)?;
    let payer = next_account_info(it)?;
    let system = next_account_info(it)?;
    if buffer.owner != &DELEGATION_PROGRAM_ID {
        return Err(ChainError::WrongDelegationProgram.into());
    }
    // Only our own PDAs, with the content they are supposed to hold.
    let refs: Vec<&[u8]> = seeds.iter().map(Vec::as_slice).collect();
    expect_pda(program_id, pda, &refs)?;
    let data = buffer.try_borrow_data()?;
    let ok = match refs.first().copied() {
        // Chunk 0 carries the header; the others are raw continuation bytes.
        Some(s) if s == WORLD_SEED => {
            data.len() == CHUNK
                && (refs.get(2) != Some(&&[0u8][..])
                    || data[..8] == WORLD_MAGIC
                    || data[..8] == GENESIS_MAGIC)
        }
        Some(s) if s == NATION_SEED => data.len() >= 8 && data[..8] == NATION_MAGIC,
        _ => false,
    };
    drop(data);
    if !ok {
        return Err(ChainError::WrongPda.into());
    }
    undelegate_account(pda, program_id, buffer, payer, system, seeds)?;
    msg!("PS undelegated {}", pda.key);
    Ok(())
}

pub(super) fn commit(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    undelegate: bool,
) -> ProgramResult {
    let it = &mut accounts.iter();
    let payer = next_account_info(it)?;
    let magic_program = next_account_info(it)?;
    let magic_context = next_account_info(it)?;
    signer(payer)?;
    if magic_program.key != &MAGIC_PROGRAM_ID || magic_context.key != &MAGIC_CONTEXT_ID {
        return Err(ChainError::WrongMagicProgram.into());
    }
    let chunk0 = accounts.get(3).ok_or(ProgramError::NotEnoughAccountKeys)?;
    let world = world_chunks(program_id, &accounts[3..], world_season_id(chunk0)?)?;
    let meta = world.meta()?;
    if undelegate && !meta.finished {
        return Err(ChainError::SeasonNotOver.into());
    }
    let it = &mut accounts[3 + WORLD_CHUNKS..].iter();
    let mut list: Vec<AccountInfo> = world.accounts.to_vec();
    for civ in 0..meta.civs as u16 {
        let ai = next_account_info(it)?;
        load_nation(program_id, ai, meta.season_id, civ)?;
        list.push(ai.clone());
    }
    let builder =
        MagicIntentBundleBuilder::new(payer.clone(), magic_context.clone(), magic_program.clone());
    if undelegate {
        builder.commit_and_undelegate(&list).build_and_invoke()?;
    } else {
        builder.commit(&list).build_and_invoke()?;
    }
    msg!(
        "PS commit{} season {}",
        if undelegate { "+undelegate" } else { "" },
        meta.season_id
    );
    Ok(())
}

/// Commit (and with `undelegate`, undelegate) the `targets` in one small intent.
pub(super) fn commit_part(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    targets: Vec<u16>,
    undelegate: bool,
) -> ProgramResult {
    let it = &mut accounts.iter();
    let payer = next_account_info(it)?;
    let magic_program = next_account_info(it)?;
    let magic_context = next_account_info(it)?;
    let chunk0 = next_account_info(it)?;
    signer(payer)?;
    if magic_program.key != &MAGIC_PROGRAM_ID || magic_context.key != &MAGIC_CONTEXT_ID {
        return Err(ChainError::WrongMagicProgram.into());
    }
    if targets.is_empty() || targets.len() > WORLD_CHUNKS + MAX_NATIONS {
        return Err(ChainError::InvalidParams.into());
    }
    // Undelegation needs the season to be over: read from chunk 0's header.
    let season_id = world_season_id(chunk0)?;
    let id = season_id.to_le_bytes();
    if chunk0.owner != program_id || chunk0.data_len() != CHUNK {
        return Err(ChainError::WrongWorld.into());
    }
    expect_pda(program_id, chunk0, &[WORLD_SEED, &id, &[0]])?;
    let meta = WorldMeta::from_chunk0(&chunk0.try_borrow_data()?)?;
    if undelegate && !meta.finished {
        return Err(ChainError::SeasonNotOver.into());
    }
    let mut list: Vec<AccountInfo> = Vec::with_capacity(targets.len());
    for (i, &t) in targets.iter().enumerate() {
        if targets[..i].contains(&t) {
            return Err(ChainError::InvalidParams.into());
        }
        if t == 0 {
            list.push(chunk0.clone());
            continue;
        }
        let ai = next_account_info(it)?;
        if (t as usize) < WORLD_CHUNKS {
            if ai.owner != program_id {
                return Err(ChainError::WrongWorld.into());
            }
            expect_pda(program_id, ai, &[WORLD_SEED, &id, &[t as u8]])?;
        } else {
            let civ = t
                .checked_sub(NATION_TARGET)
                .filter(|c| *c < meta.civs as u16)
                .ok_or(ChainError::InvalidParams)?;
            load_nation(program_id, ai, season_id, civ)?;
        }
        list.push(ai.clone());
    }
    let builder =
        MagicIntentBundleBuilder::new(payer.clone(), magic_context.clone(), magic_program.clone());
    if undelegate {
        builder.commit_and_undelegate(&list).build_and_invoke()?;
    } else {
        builder.commit(&list).build_and_invoke()?;
    }
    msg!(
        "PS {} {:?} season {}",
        if undelegate { "undelegate" } else { "commit" },
        targets,
        season_id
    );
    Ok(())
}
