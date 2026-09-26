//! The program's instruction handlers, one module per stage of a season:
//!
//! | Module | Layer | Instructions |
//! |---|---|---|
//! | `registration` | base | CreateSeason, AllocWorld, AllocNation, Register, UpdateMember, PostBond |
//! | `genesis` | base | StartSeason, ConsumeSeasonSeed, RetrySeasonSeed, GenesisStep, SeatMembers, OpenGovernment |
//! | `delegation` | base / ER | Delegate, the undelegation callback, CommitPart, UndelegatePart |
//! | `play` | ER | StartClock, CommitOrders, CloseCommits, RevealOrders, SubmitGov, FreezeTick, ConsumeTickRandomness, RetryTickRandomness, LogTickInput, ResolveTick |
//! | `roster` | base / ER | RevealRoster, AnchorTalk |
//! | `settlement` | base | FinishSeason, Claim, WithdrawOps |
//! | `escape` | base | Abort, RequestUndelegation, RollbackUndelegation, CloseSeasonAccounts |
//!
//! `accounts` holds the checks and loaders they share, and the clock
//! (`now`). Retired and always refused (`Retired`): `SubmitOrders` (plain
//! orders, before commit–reveal), `Commit` and `CommitAndUndelegate` (one
//! intent this large exceeds the committor's limits).

use borsh::BorshDeserialize;
use solana_program::{account_info::AccountInfo, entrypoint::ProgramResult, pubkey::Pubkey};

use crate::error::ChainError;
use crate::instruction::ChainInstruction;

mod accounts;
mod delegation;
mod escape;
mod genesis;
mod play;
mod registration;
mod roster;
mod settlement;

use delegation::*;
use escape::*;
use genesis::*;
use play::*;
use registration::*;
use roster::*;
use settlement::*;

pub use accounts::now;
#[cfg(all(not(target_os = "solana"), any(test, feature = "host-clock")))]
pub use accounts::set_host_clock;

pub use crate::payout::claim_amount;
pub use crate::state::NATION_TARGET;

/// Fixed discriminator the delegation program calls on undelegation.
pub const UNDELEGATE_CALLBACK_DISCRIMINATOR: [u8; 8] = [196, 28, 41, 206, 48, 37, 51, 167];
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
            ai_count,
            roster_commit,
            bounty_each,
            bond,
            deposit,
            start_by,
            validator,
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
                ai_count,
                roster_commit,
                bounty_each,
                bond,
                deposit,
                start_by,
                validator,
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
            tag,
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
            tag,
        ),
        ChainInstruction::StartSeason => start_season(program_id, accounts),
        ChainInstruction::GenesisStep { work } => genesis_step(program_id, accounts, work),
        ChainInstruction::Delegate { target } => delegate(program_id, accounts, target),
        ChainInstruction::SubmitOrders { .. } => Err(ChainError::Retired.into()),
        ChainInstruction::ResolveTick { to } => resolve_tick(program_id, accounts, to),
        ChainInstruction::Commit | ChainInstruction::CommitAndUndelegate => {
            Err(ChainError::Retired.into())
        }
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
        ChainInstruction::RevealRoster { from, salts, blind } => {
            reveal_roster(program_id, accounts, from, salts, blind)
        }
        ChainInstruction::AnchorTalk { tick, count, root } => {
            anchor_talk(program_id, accounts, tick, count, root)
        }
        ChainInstruction::StartClock => start_clock(program_id, accounts),
        ChainInstruction::PostBond { amount } => post_bond(program_id, accounts, amount),
        ChainInstruction::FreezeTick => freeze_tick(program_id, accounts),
        ChainInstruction::ConsumeTickRandomness {
            randomness,
            season_id,
            tick,
        } => consume_tick_randomness(program_id, accounts, randomness, season_id, tick),
        ChainInstruction::RetryTickRandomness => retry_tick_randomness(program_id, accounts),
        ChainInstruction::ConsumeSeasonSeed {
            randomness,
            season_id,
        } => consume_season_seed(program_id, accounts, randomness, season_id),
        ChainInstruction::RetrySeasonSeed => retry_season_seed(program_id, accounts),
        ChainInstruction::Abort => abort(program_id, accounts),
        ChainInstruction::RequestUndelegation { target } => {
            request_undelegation(program_id, accounts, target)
        }
        ChainInstruction::RollbackUndelegation { target } => {
            rollback_undelegation(program_id, accounts, target)
        }
        ChainInstruction::CloseSeasonAccounts { targets } => {
            close_season_accounts(program_id, accounts, targets)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(ix: ChainInstruction) -> ProgramResult {
        process(&Pubkey::new_unique(), &[], &borsh::to_vec(&ix).unwrap())
    }

    /// Commit and CommitAndUndelegate are retired like SubmitOrders.
    #[test]
    fn whole_world_commits_are_retired() {
        let retired: solana_program::program_error::ProgramError = ChainError::Retired.into();
        assert_eq!(run(ChainInstruction::Commit).unwrap_err(), retired);
        assert_eq!(
            run(ChainInstruction::CommitAndUndelegate).unwrap_err(),
            retired
        );
    }

    /// Data that decodes as no instruction is refused before any account is read.
    #[test]
    fn unknown_tags_are_refused() {
        let err = process(&Pubkey::new_unique(), &[], &[37]).unwrap_err();
        assert_eq!(err, ChainError::InvalidInstruction.into());
    }
}
