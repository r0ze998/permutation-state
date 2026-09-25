//! The program's instruction handlers, one module per stage of a season:
//!
//! | Module | Layer | Instructions |
//! |---|---|---|
//! | `registration` | base | CreateSeason, AllocWorld, AllocNation, Register, UpdateMember |
//! | `genesis` | base | StartSeason, GenesisStep, SeatMembers, OpenGovernment |
//! | `delegation` | base / ER | Delegate, the undelegation callback, Commit, CommitAndUndelegate, CommitPart, UndelegatePart |
//! | `play` | ER | SubmitOrders, SubmitGov, LogTickInput, ResolveTick |
//! | `settlement` | base | FinishSeason, Claim, WithdrawOps |
//!
//! `accounts` holds the checks and loaders they share.

use borsh::BorshDeserialize;
use solana_program::{account_info::AccountInfo, entrypoint::ProgramResult, pubkey::Pubkey};

use crate::error::ChainError;
use crate::instruction::ChainInstruction;

mod accounts;
mod delegation;
mod genesis;
mod play;
mod registration;
mod settlement;

use delegation::*;
use genesis::*;
use play::*;
use registration::*;
use settlement::*;

pub use settlement::claim_amount;

/// Fixed discriminator the delegation program calls on undelegation.
pub const UNDELEGATE_CALLBACK_DISCRIMINATOR: [u8; 8] = [196, 28, 41, 206, 48, 37, 51, 167];
/// `Delegate { target }`: world chunks are 0..WORLD_CHUNKS, nations are NATION_TARGET + civ.
pub const NATION_TARGET: u16 = 1000;
/// How often the ER auto-commits delegated accounts to the base layer.
pub const ER_COMMIT_FREQUENCY_MS: u32 = 30_000;

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    if let Some(seeds) = data.strip_prefix(&UNDELEGATE_CALLBACK_DISCRIMINATOR) {
        let seeds =
            Vec::<Vec<u8>>::try_from_slice(seeds).map_err(|_| ChainError::InvalidInstruction)?;
        return undelegate_callback(program_id, accounts, seeds);
    }
    let ix = ChainInstruction::try_from_slice(data).map_err(|_| ChainError::InvalidInstruction)?;
    match ix {
        ChainInstruction::CreateSeason {
            season_id,
            preset,
            nations,
            entry_fee,
            tick_seconds,
            world_seed,
            crank,
            market,
            prev_season_id,
        } => create_season(
            program_id,
            accounts,
            season_id,
            SeasonParams {
                preset,
                nations,
                entry_fee,
                tick_seconds,
                world_seed,
                crank,
                market,
                prev_season_id,
            },
        ),
        ChainInstruction::AllocWorld { chunk } => alloc_world(program_id, accounts, chunk),
        ChainInstruction::Register {
            civ,
            name,
            kind,
            session,
            attestation,
            stand,
            votes,
            deposit,
        } => register(
            program_id,
            accounts,
            civ,
            name,
            kind,
            session,
            attestation,
            stand,
            votes,
            deposit,
        ),
        ChainInstruction::StartSeason => start_season(program_id, accounts),
        ChainInstruction::GenesisStep { work } => genesis_step(program_id, accounts, work),
        ChainInstruction::Delegate { target } => delegate(program_id, accounts, target),
        ChainInstruction::SubmitOrders { .. } => Err(ChainError::Retired.into()),
        ChainInstruction::ResolveTick { to } => resolve_tick(program_id, accounts, to),
        ChainInstruction::Commit => commit(program_id, accounts, false),
        ChainInstruction::CommitAndUndelegate => commit(program_id, accounts, true),
        ChainInstruction::FinishSeason => finish_season(program_id, accounts),
        ChainInstruction::Claim => claim(program_id, accounts),
        ChainInstruction::UndelegatePart { targets } => {
            commit_part(program_id, accounts, targets, true)
        }
        ChainInstruction::UpdateMember { stand, votes } => {
            update_member(program_id, accounts, stand, votes)
        }
        ChainInstruction::AllocNation { civ } => alloc_nation(program_id, accounts, civ),
        ChainInstruction::SeatMembers => seat_members(program_id, accounts),
        ChainInstruction::OpenGovernment => open_government(program_id, accounts),
        ChainInstruction::SubmitGov { member, action } => {
            submit_gov(program_id, accounts, member, action)
        }
        ChainInstruction::WithdrawOps => withdraw_ops(program_id, accounts),
        ChainInstruction::LogTickInput { chunk } => log_tick_input(program_id, accounts, chunk),
        ChainInstruction::CommitPart { targets } => {
            commit_part(program_id, accounts, targets, false)
        }
        ChainInstruction::CloseCommits => close_commits(program_id, accounts),
        ChainInstruction::CommitOrders {
            role,
            tick,
            commitment,
        } => commit_orders(program_id, accounts, role, tick, commitment),
        ChainInstruction::RevealOrders {
            role,
            tick,
            decision_digest,
            orders,
            adopt,
            salt,
        } => reveal_orders(
            program_id,
            accounts,
            role,
            tick,
            OrderBatchParts {
                decision_digest,
                orders,
                adopt,
            },
            salt,
        ),
    }
}
