//! Deterministic rules engine for PERMUTATION STATE.
//!
//! Implements `PERMUTATION_STATE_RULES_SPEC_v0.2.md` (which merges the v0.1
//! numbers with the v0.2 changes and Game Design V5: nations, offices, merit,
//! achievements, the USDC market). The same crate is meant to
//! run inside the MagicBlock ER program, the replay verifier, and (via WASM) the
//! browser client and agents, so it is `no_std`, allocation-only and free of
//! floating point. Section references (`§x.y`) point into the spec.

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(unsafe_code)] // one exception: the sol_sha256 syscall in `hash`

extern crate alloc;

pub mod battle;
pub mod buildings;
pub mod checks;
pub mod combat;
pub mod decision;
pub mod diplomacy;
pub mod economy;
pub mod envoys;
pub mod error;
pub mod fixed;
pub mod genesis;
pub mod gov;
pub mod hash;
pub mod hex;
pub mod history;
pub mod invariants;
pub mod map;
pub mod mapgen;
pub mod markets;
pub mod merit;
pub mod movement;
pub mod orders;
pub mod params;
pub mod payout;
pub mod preview;
pub mod probe;
pub mod rng;
pub mod scoring;
pub mod standing;
pub mod state;
pub mod tech;
pub mod tick;
pub mod trade;
pub mod units;
pub mod vision;

pub use error::RulesError;
pub use params::{Preset, Ruleset};
pub use state::WorldState;
