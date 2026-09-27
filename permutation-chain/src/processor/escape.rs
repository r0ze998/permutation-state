//! Base layer, when a season cannot move on (WP14): aborting it, taking
//! delegated accounts back without the validator, and closing a done
//! season's accounts. When each is allowed is decided by the pure
//! functions of `crate::lifecycle`.

use ephemeral_rollups_sdk::consts::DELEGATION_PROGRAM_ID;
use solana_program::{
    account_info::AccountInfo,
    entrypoint::ProgramResult,
    instruction::{AccountMeta, Instruction},
    msg,
    program::invoke_signed,
    program_error::ProgramError,
    pubkey::Pubkey,
};
use solana_system_interface::program as system_program;

use super::accounts::*;
use super::settlement::precheck_world;
use crate::error::ChainError;
use crate::lifecycle::{check_abort, running_deadline, WorldView};
use crate::state::*;

/// `RequestUndelegation` and `UndelegateWithRollbackAfterTimeout` of the
/// delegation program (dlp-api 3.1.0 `DlpDiscriminator`, u64 LE).
const DLP_REQUEST_UNDELEGATION: u64 = 26;
const DLP_ROLLBACK_AFTER_TIMEOUT: u64 = 27;

/// A delegation target of the season: its PDA seeds (without the bump) and
/// its bit in `Season::delegated`. `InvalidParams` for anything else.
fn target_seeds(season: &Season, target: u16) -> Result<(Vec<Vec<u8>>, u32), ProgramError> {
    let bit = target_bit(target)
        .filter(|b| b & all_targets(season.nations) != 0)
        .ok_or(ChainError::InvalidParams)?;
    let id = season.season_id.to_le_bytes().to_vec();
    let seeds = if (target as usize) < WORLD_CHUNKS {
        vec![WORLD_SEED.to_vec(), id, vec![target as u8]]
    } else {
        vec![
            NATION_SEED.to_vec(),
            id,
            (target - NATION_TARGET).to_le_bytes().to_vec(),
        ]
    };
    Ok((seeds, bit))
}

/// The world as `Abort` sees it on base: the 20 chunk PDAs checked, any
/// owner. Chunk 0's meta is read only when it is home, `CHUNK` long and a
/// world; `precheck_ok` is FinishSeason's precheck when every chunk is home.
fn world_view(
    program_id: &Pubkey,
    season: &Season,
    chunks: &[AccountInfo],
) -> Result<WorldView, ProgramError> {
    let list = chunks
        .get(..WORLD_CHUNKS)
        .ok_or(ChainError::WorldTooSmall)?;
    let id = season.season_id.to_le_bytes();
    for (k, a) in list.iter().enumerate() {
        expect_pda(program_id, a, &[WORLD_SEED, &id, &[k as u8]])?;
    }
    let mut v = WorldView {
        all_home: list.iter().all(|a| a.owner == program_id),
        chunk0_home: list[0].owner == program_id,
        ..Default::default()
    };
    if v.chunk0_home && list[0].data_len() == CHUNK {
        let d = list[0].try_borrow_data()?;
        if d[..8] == WORLD_MAGIC {
            let meta = WorldMeta::from_chunk0(&d)?;
            (v.is_world, v.finished, v.deadline) = (true, meta.finished, meta.deadline);
        }
    }
    if v.all_home {
        v.precheck_ok = Chunks::new(list)
            .map(|w| precheck_world(season, &w).is_ok())
            .unwrap_or(false);
    }
    Ok(v)
}

/// `Abort`: accounts 0 caller (s) · 1 season (w) · 2..=21 world chunks 0..19
/// (read; needed while Running). Who may abort, and when, is
/// `lifecycle::check_abort`; members then claim refunds
/// (`lifecycle::refund_amount`).
pub(super) fn abort(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let [caller, season_info, chunks @ ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer(caller)?;
    let mut season = load_season(program_id, season_info)?;
    let operator = require_operator(&season, caller).is_ok();
    let rules = rules_for(season.preset, season.market)?;
    let view = if season.status == SeasonStatus::Running {
        Some(world_view(program_id, &season, chunks)?)
    } else {
        None
    };
    let now = now()?;
    check_abort(
        &season,
        rules.ticks_per_season,
        view.as_ref(),
        operator,
        now,
    )?;
    season.aborted_from = season.status as u8;
    season.status = SeasonStatus::Aborted;
    season.stage_at = now;
    // An aborted season adds nothing to the history layer: a season that
    // follows it chains onto the same root.
    season.history_root = season.prev_history_root;
    // Everything paid in is owed back (WP12's decrement in Claim and
    // WithdrawOps then covers the refunds exactly).
    season.outstanding = u64::try_from(crate::finalize::paid_in(&season)).unwrap_or(u64::MAX);
    season.voided = false;
    store(&mut season_info.try_borrow_mut_data()?, &season)?;
    solana_program::log::sol_log_data(&[
        b"PS_ABORT",
        &season.season_id.to_le_bytes(),
        &[season.aborted_from],
        &now.to_le_bytes(),
    ]);
    msg!(
        "PS season {} aborted from status {}",
        season.season_id,
        season.aborted_from
    );
    Ok(())
}

/// `RequestUndelegation { target }`: the operator (the delegation's rent
/// payer, whose signature the delegation program requires) asks the
/// delegation program to take `target` back without the validator. Only
/// once the season is done, or Running past its running deadline: a
/// request stops the validator's own undelegation of that account, so
/// during play or an honest wind-up it would let the operator halt a
/// season it is losing (WP14 §3.6, C13). A dropped undelegation intent
/// therefore waits for the running deadline, after which the season can
/// only be aborted and refunded.
pub(super) fn request_undelegation(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    target: u16,
) -> ProgramResult {
    let [operator, season_info, pda, owner_program, request, record, metadata, system, dlp, ..] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer(operator)?;
    let season = load_season(program_id, season_info)?;
    require_operator(&season, operator)?;
    if owner_program.key != program_id || system.key != &system_program::id() {
        return Err(ProgramError::IncorrectProgramId);
    }
    if dlp.key != &DELEGATION_PROGRAM_ID {
        return Err(ChainError::WrongDelegationProgram.into());
    }
    let (seeds, bit) = target_seeds(&season, target)?;
    let refs: Vec<&[u8]> = seeds.iter().map(Vec::as_slice).collect();
    let bump = expect_pda(program_id, pda, &refs)?;
    match season.status {
        SeasonStatus::Finalized | SeasonStatus::Aborted => {}
        SeasonStatus::Running => {
            let rules = rules_for(season.preset, season.market)?;
            if now()? < running_deadline(&season, rules.ticks_per_season) {
                return Err(ChainError::TooEarly.into());
            }
        }
        _ => return Err(ChainError::WrongStatus.into()),
    }
    if season.delegated & bit == 0 {
        return Err(ChainError::InvalidParams.into());
    }
    let ix = Instruction {
        program_id: DELEGATION_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(*operator.key, true),
            AccountMeta::new_readonly(*pda.key, true),
            AccountMeta::new_readonly(*program_id, false),
            AccountMeta::new(*request.key, false),
            AccountMeta::new_readonly(*record.key, false),
            AccountMeta::new(*metadata.key, false),
            AccountMeta::new_readonly(*system.key, false),
        ],
        data: DLP_REQUEST_UNDELEGATION.to_le_bytes().to_vec(),
    };
    let b = [bump];
    let mut signer_seeds = refs.clone();
    signer_seeds.push(&b);
    invoke_signed(
        &ix,
        &[
            operator.clone(),
            pda.clone(),
            owner_program.clone(),
            request.clone(),
            record.clone(),
            metadata.clone(),
            system.clone(),
            dlp.clone(),
        ],
        &[&signer_seeds],
    )?;
    msg!(
        "PS season {} undelegation requested for target {}",
        season.season_id,
        target
    );
    Ok(())
}

/// `RollbackUndelegation { target }` (permissionless): once a request has
/// expired, the delegation program hands `target` back emptied; its last
/// finalized state (the base copy) is saved first and written back. While
/// Running the target is marked in `rolled_back`, so FinishSeason refuses
/// the world (`WorldRolledBack`) and the season ends in refunds.
pub(super) fn rollback_undelegation(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    target: u16,
) -> ProgramResult {
    let [season_info, pda, owner_program, request, record, metadata, rent_payer, commit_state, commit_record, reimbursement, dlp, ..] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    let mut season = load_season(program_id, season_info)?;
    if owner_program.key != program_id {
        return Err(ProgramError::IncorrectProgramId);
    }
    if dlp.key != &DELEGATION_PROGRAM_ID || pda.owner != &DELEGATION_PROGRAM_ID {
        return Err(ChainError::WrongDelegationProgram.into());
    }
    let (seeds, bit) = target_seeds(&season, target)?;
    let refs: Vec<&[u8]> = seeds.iter().map(Vec::as_slice).collect();
    let bump = expect_pda(program_id, pda, &refs)?;
    let saved = pda.try_borrow_data()?.to_vec();
    let ix = Instruction {
        program_id: DELEGATION_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(*pda.key, true),
            AccountMeta::new_readonly(*program_id, false),
            AccountMeta::new(*request.key, false),
            AccountMeta::new(*record.key, false),
            AccountMeta::new(*metadata.key, false),
            AccountMeta::new(*rent_payer.key, false),
            AccountMeta::new(*commit_state.key, false),
            AccountMeta::new(*commit_record.key, false),
            AccountMeta::new(*reimbursement.key, false),
        ],
        data: DLP_ROLLBACK_AFTER_TIMEOUT.to_le_bytes().to_vec(),
    };
    let b = [bump];
    let mut signer_seeds = refs.clone();
    signer_seeds.push(&b);
    invoke_signed(
        &ix,
        &[
            pda.clone(),
            owner_program.clone(),
            request.clone(),
            record.clone(),
            metadata.clone(),
            rent_payer.clone(),
            commit_state.clone(),
            commit_record.clone(),
            reimbursement.clone(),
            dlp.clone(),
        ],
        &[&signer_seeds],
    )?;
    if pda.owner != program_id {
        return Err(ChainError::WrongDelegationProgram.into());
    }
    pda.resize(saved.len())?;
    pda.try_borrow_mut_data()?.copy_from_slice(&saved);
    if season.status == SeasonStatus::Running {
        season.rolled_back |= bit;
        store(&mut season_info.try_borrow_mut_data()?, &season)?;
    }
    solana_program::log::sol_log_data(&[
        b"PS_ROLLBACK",
        &season.season_id.to_le_bytes(),
        &target.to_le_bytes(),
        &solana_program::hash::hashv(&[&saved]).to_bytes(),
    ]);
    msg!(
        "PS season {} target {} rolled back to its last finalized state",
        season.season_id,
        target
    );
    Ok(())
}

/// `CloseSeasonAccounts { targets }`: the operator takes back the rent of a
/// done season's world chunks and nations (`AllocWorld` / `AllocNation`
/// refuse after Registering, so a closed account is never re-created). A
/// still-delegated target is `WrongWorld`: roll it back first. Clients send
/// at most 13 targets per transaction.
pub(super) fn close_season_accounts(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    targets: Vec<u16>,
) -> ProgramResult {
    let [operator, season_info, list @ ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer(operator)?;
    let season = load_season(program_id, season_info)?;
    require_operator(&season, operator)?;
    if !matches!(
        season.status,
        SeasonStatus::Finalized | SeasonStatus::Aborted
    ) {
        return Err(ChainError::WrongStatus.into());
    }
    if targets.is_empty() || targets.len() != list.len() {
        return Err(ChainError::InvalidParams.into());
    }
    for (i, (&t, info)) in targets.iter().zip(list).enumerate() {
        if targets[..i].contains(&t) {
            return Err(ChainError::InvalidParams.into());
        }
        let (seeds, _) = target_seeds(&season, t)?;
        let refs: Vec<&[u8]> = seeds.iter().map(Vec::as_slice).collect();
        expect_pda(program_id, info, &refs)?;
        if info.owner != program_id {
            return Err(ChainError::WrongWorld.into());
        }
        let lamports = info.lamports();
        **info.try_borrow_mut_lamports()? = 0;
        let to = operator
            .lamports()
            .checked_add(lamports)
            .ok_or(ChainError::InvalidParams)?;
        **operator.try_borrow_mut_lamports()? = to;
        info.resize(0)?;
        info.assign(&system_program::id());
    }
    msg!(
        "PS closed {} accounts of season {}",
        targets.len(),
        season.season_id
    );
    Ok(())
}
