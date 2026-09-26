//! Harness for running the SBF build of permutation-chain in LiteSVM.
//!
//! The program's own layout modules are compiled in (no hand mirrors): a
//! layout change in `permutation-chain/src/state.rs` changes these tests with
//! it. The program runs from `target/deploy/permutation_chain.so` (or
//! `PERMUTATION_CHAIN_SO`) with real SPL Token transfers, a real clock, the
//! real compute meter and the real heap frame; the MagicBlock programs are
//! recording stand-ins (`magicblock`).
//!
//! | Module | What |
//! |---|---|
//! | `chain` | the SVM, sending with the client's budget profiles, accounts, the clock |
//! | `spl` | hand-packed SPL Token v3 mints and token accounts |
//! | `season` | `SeasonFx`: a season built through the program itself |
//! | `ix` | instruction builders, one file per area (account order = `chain.mjs`) |
//! | `drive` | genesis, seating and ticks as the crank drives them |
//! | `bots` | the play server's bots playing a season through the program |
//! | `magicblock` | stand-ins for the delegation and Magic programs |
//! | `vrf` | a stand-in for the MagicBlock VRF program (requests, `fulfil`) |
//! | `records` | `sol_log_data` records (`PS_*`) |
//! | `budget` | CU/heap needs (`need`), the ceilings, the crank's split points |
//! | `cover` | the coverage tables the guard (`tests/coverage.rs`) checks |
//!
//! Run it with `run.sh` (it builds the program first); see README.md.

#![allow(clippy::too_many_arguments)]

pub use permutation_chain::{error, finalize, instruction, payout, seat, state};

pub mod bots;
pub mod budget;
pub mod chain;
pub mod cover;
pub mod drive;
pub mod ix;
pub mod magicblock;
pub mod records;
pub mod season;
pub mod spl;
pub mod vrf;

pub use bots::{BotSeason, TickReport};
pub use budget::*;
pub use chain::*;
pub use magicblock::*;
pub use records::*;
pub use season::*;

use solana_address::Address;
use solana_instruction::AccountMeta;

/// The program id the devnet deploy uses (`J4aZxe3…niU6n`).
pub const PROGRAM_ID: &str = "J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n";
pub const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const SYSTEM: &str = "11111111111111111111111111111111";
pub const SLOT_HASHES: &str = "SysvarS1otHashes111111111111111111111111111";
/// The MagicBlock delegation program.
pub const DLP: &str = "DELeGGvXpWV2fqJUhqcF5ZSYMS4JTLjteaAMARRSaeSh";
/// Unix time every chain starts at.
pub const T0: i64 = 1_800_000_000;

pub fn addr(s: &str) -> Address {
    s.parse().unwrap()
}

pub fn pda(program: &Address, seeds: &[&[u8]]) -> Address {
    Address::find_program_address(seeds, program).0
}

/// Writable, not a signer.
pub fn w(k: &Address) -> AccountMeta {
    AccountMeta::new(*k, false)
}
/// Read-only, not a signer.
pub fn r(k: &Address) -> AccountMeta {
    AccountMeta::new_readonly(*k, false)
}
/// Writable signer.
pub fn ws(k: &Address) -> AccountMeta {
    AccountMeta::new(*k, true)
}
/// Read-only signer.
pub fn rs(k: &Address) -> AccountMeta {
    AccountMeta::new_readonly(*k, true)
}

/// The season's ruleset, as the program's `rules_for` builds it.
pub fn rules(preset: u8, market: bool) -> permutation_rules::Ruleset {
    let mut r = permutation_rules::Ruleset::new(if preset == 0 {
        permutation_rules::Preset::Blitz
    } else {
        permutation_rules::Preset::Season
    });
    r.market_enabled = market;
    r
}
