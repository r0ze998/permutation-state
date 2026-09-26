//! PERMUTATION STATE on Solana + MagicBlock Ephemeral Rollups.
//!
//! The World PDA holds the borsh-encoded `permutation_rules::WorldState`; the
//! same crate the server, replay verifier and agents use runs here, so a
//! tick resolved on chain is byte-identical to one resolved anywhere else.
//! See `DESIGN.md` for the account model and the season lifecycle.
//!
//! Two layers, split by the `program` feature (on by default):
//!
//! * **Always compiled**: the account layouts and the pure modules below
//!   (`error`, `finalize`, `heap`, `instruction`, `payout`, `seat`, `state`,
//!   `token`). They must never `use` the MagicBlock SDK,
//!   `solana_system_interface` or `crate::processor` (test-only uses are
//!   gated `#[cfg(all(test, feature = "program"))]`). The play server, the
//!   replay verifier and the LiteSVM suite (`svm-tests`) build this crate
//!   without `program`, so code they call must live here, never in
//!   `processor`. Their builds fail if the rule is broken.
//! * **`program`**: the instruction handlers (`processor`) and the
//!   entrypoint, with the MagicBlock SDK.

#![allow(unexpected_cfgs)]

pub mod error;
pub mod finalize;
pub mod heap;
pub mod instruction;
pub mod payout;
#[cfg(feature = "program")]
pub mod processor;
pub mod seat;
pub mod state;
pub mod token;

#[cfg(all(feature = "program", not(feature = "no-entrypoint")))]
mod entrypoint {
    use solana_program::{account_info::AccountInfo, entrypoint::ProgramResult, pubkey::Pubkey};
    solana_program::entrypoint!(process_instruction);
    fn process_instruction(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        data: &[u8],
    ) -> ProgramResult {
        crate::processor::process(program_id, accounts, data)
    }
}
