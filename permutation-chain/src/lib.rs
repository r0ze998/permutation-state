//! PERMUTATION STATE on Solana + MagicBlock Ephemeral Rollups.
//!
//! The World PDA holds the borsh-encoded `permutation_rules::WorldState`; the
//! same crate the server, replay verifier and agents use runs here, so a
//! tick resolved on chain is byte-identical to one resolved anywhere else.
//! See `DESIGN.md` for the account model and the season lifecycle.

#![allow(unexpected_cfgs)]

pub mod error;
pub mod heap;
pub mod instruction;
pub mod processor;
pub mod state;
pub mod token;

#[cfg(not(feature = "no-entrypoint"))]
mod entrypoint {
    use solana_program::{account_info::AccountInfo, entrypoint::ProgramResult, pubkey::Pubkey};
    solana_program::entrypoint!(process_instruction);
    fn process_instruction(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
        crate::processor::process(program_id, accounts, data)
    }
}
