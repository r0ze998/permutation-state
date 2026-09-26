//! Base layer, when a season cannot move on (WP14): aborting it, taking
//! delegated accounts back without the validator, and closing a done
//! season's accounts. The rules are the pure functions of
//! `crate::lifecycle`; the handlers land with unit P3 and refuse every call
//! until then.

use solana_program::{account_info::AccountInfo, entrypoint::ProgramResult, pubkey::Pubkey};

use crate::error::ChainError;

/// `Abort` (not implemented yet).
pub(super) fn abort(_program_id: &Pubkey, _accounts: &[AccountInfo]) -> ProgramResult {
    Err(ChainError::InvalidInstruction.into())
}

/// `RequestUndelegation { target }` (not implemented yet).
pub(super) fn request_undelegation(
    _program_id: &Pubkey,
    _accounts: &[AccountInfo],
    _target: u16,
) -> ProgramResult {
    Err(ChainError::InvalidInstruction.into())
}

/// `RollbackUndelegation { target }` (not implemented yet).
pub(super) fn rollback_undelegation(
    _program_id: &Pubkey,
    _accounts: &[AccountInfo],
    _target: u16,
) -> ProgramResult {
    Err(ChainError::InvalidInstruction.into())
}

/// `CloseSeasonAccounts { targets }` (not implemented yet).
pub(super) fn close_season_accounts(
    _program_id: &Pubkey,
    _accounts: &[AccountInfo],
    _targets: Vec<u16>,
) -> ProgramResult {
    Err(ChainError::InvalidInstruction.into())
}
