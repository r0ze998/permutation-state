//! A stand-in for the delegation program's owner-side escape (dlp-api 3.1.0,
//! WP14 §3.6), registered at the DLP's id in place of `magicblock`'s
//! recording stand-in. Every call is recorded there too
//! (`magicblock::mocks::take`), so `Delegate` works as before; besides:
//!
//! * **26 `RequestUndelegation`**: the rent payer and the PDA must sign (the
//!   PDA by our program's CPI). Records the request; the real program writes
//!   the request PDA and blocks the validator's plain undelegation.
//! * **27 `UndelegateWithRollbackAfterTimeout`**: the PDA must sign; it is
//!   handed back to the owner program (account 1) **emptied**, as the real
//!   program does before the owner restores its last finalized state.
//!
//! What the real program checks beyond that (request expiry, rent payer,
//! pending commit) is covered only with `DLP_SO` (`Chain::with_real_dlp`).

use crate::chain::Chain;
use crate::{addr, pda, DLP};
use solana_address::Address;

/// `DlpDiscriminator::RequestUndelegation` (u64 LE).
pub const DLP_REQUEST_UNDELEGATION: u64 = 26;
/// `DlpDiscriminator::UndelegateWithRollbackAfterTimeout` (u64 LE).
pub const DLP_ROLLBACK_AFTER_TIMEOUT: u64 = 27;

pub mod mock {
    use solana_instruction::error::InstructionError;
    use solana_program_runtime::declare_process_instruction;
    use solana_program_runtime::invoke_context::InvokeContext;

    fn escape(ic: &mut InvokeContext) -> Result<(), InstructionError> {
        let ctx = ic.transaction_context.get_current_instruction_context()?;
        let mut keys = vec![];
        for i in 0..ctx.get_number_of_instruction_accounts() {
            keys.push(*ctx.get_key_of_instruction_account(i)?);
        }
        let data = ctx.get_instruction_data().to_vec();
        crate::magicblock::mocks::CALLS.with(|c| c.borrow_mut().push(("dlp", keys, data.clone())));
        let disc = data
            .get(..8)
            .map(|d| u64::from_le_bytes(d.try_into().unwrap()));
        match disc {
            Some(super::DLP_REQUEST_UNDELEGATION) => {
                if !ctx.is_instruction_account_signer(0)?
                    || !ctx.is_instruction_account_signer(1)?
                {
                    return Err(InstructionError::MissingRequiredSignature);
                }
                Ok(())
            }
            Some(super::DLP_ROLLBACK_AFTER_TIMEOUT) => {
                if !ctx.is_instruction_account_signer(0)? {
                    return Err(InstructionError::MissingRequiredSignature);
                }
                let owner = *ctx.get_key_of_instruction_account(1)?;
                let mut pda = ctx.try_borrow_instruction_account(0)?;
                pda.set_data_length(0)?;
                pda.set_owner(owner.as_ref())?;
                Ok(())
            }
            // Delegate and the rest: recorded only (as `MockDlp`).
            _ => Ok(()),
        }
    }

    declare_process_instruction!(MockDlpEscape, 150, |invoke_context| {
        escape(invoke_context)
    });
}

impl Chain {
    /// `with_magicblock` with the escape-aware delegation stand-in.
    pub fn with_dlp_escape(self) -> Self {
        use solana_program_runtime::solana_sbpf::program::BuiltinFunctionDefinition;
        let mut c = self.with_magicblock();
        c.svm.add_builtin(addr(DLP), mock::MockDlpEscape::register);
        c
    }
}

/// The DLP's undelegation request PDA of `target`.
pub fn undelegation_request(target: &Address) -> Address {
    pda(&addr(DLP), &[b"undelegation-request", target.as_ref()])
}

/// The DLP's delegation record of `target`.
pub fn delegation_record(target: &Address) -> Address {
    pda(&addr(DLP), &[b"delegation", target.as_ref()])
}

/// The DLP's delegation metadata of `target`.
pub fn delegation_metadata(target: &Address) -> Address {
    pda(&addr(DLP), &[b"delegation-metadata", target.as_ref()])
}

/// The DLP's commit state of `target` (`state-diff`).
pub fn commit_state(target: &Address) -> Address {
    pda(&addr(DLP), &[b"state-diff", target.as_ref()])
}

/// The DLP's commit record of `target`.
pub fn commit_record(target: &Address) -> Address {
    pda(&addr(DLP), &[b"commit-state-record", target.as_ref()])
}
