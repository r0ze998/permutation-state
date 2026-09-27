//! `frontier-abi`: the one source of truth for the Sixfold Frontier M1
//! program interface (M1 contract §3.1, I-03, I-55).
//!
//! No Solana types and no dependency besides `permutation-rules`, so the
//! program (`permutation-frontier`), its LiteSVM suite, the off-chain
//! workspace (`frontier-node`) and the JS vectors all consume the same
//! definitions:
//!
//! | module | contract | content |
//! |---|---|---|
//! | [`bytes`] | §5.3 | little-endian readers and writers over byte slices (no panics) |
//! | [`layout`] | §4.3, §5.2, §5.3 | headers, magics, sizes, every field offset, rent |
//! | [`addr`] | §4.1 | with-seed seed strings, addresses, host ids, citizen and keeper tags |
//! | [`tags`] | §5.5 | instruction tags, keeper classes, top-level rule |
//! | [`ix`] | §5.5–§5.12 | instruction data encodings (fixed width, LE, no borsh) |
//! | [`error`] | §5.4 | program error codes (stable forever) |
//! | [`log`] | §6 | PS2 log records: encode, decode, chains, the event-head function |
//! | [`budgets`] | §5.5, §10.1, §10.2 | CU, tx, heap, lock and loaded-data (`L(kind)`) budgets |
//! | [`presets`] | §5.7 | `SeasonParams` layout, validation, `M1_LOCAL_7D`, `M1_PLAYTEST` |
//! | [`prologue`] | §5.6–§5.12 | account-list tables and the common checks as pure functions |
//! | [`entry`] | §5.3 (Province entry), §4.1 (host id) | Province entry ↔ kernel `Host` codec |
//!
//! The vector writer (`src/bin/abi-vectors.rs`) turns these tables into
//! `frontier-abi/vectors/*.json`; `--check` fails when a checked-in vector
//! is stale.

#![no_std]
#![deny(unsafe_code)]

#[cfg(test)]
extern crate std;

pub mod addr;
pub mod budgets;
pub mod bytes;
pub mod entry;
pub mod error;
pub mod ix;
pub mod layout;
pub mod log;
pub mod presets;
pub mod prologue;
pub mod tags;

pub use error::FrontierError;

/// ABI version carried in every chained header (`layout_version`) and in the
/// vector files. Bumped only by a contract amendment (§16).
pub const ABI_VERSION: u16 = 1;
