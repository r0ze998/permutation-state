//! Deterministic rules engine for PERMUTATION STATE.
//!
//! Implements `PERMUTATION_STATE_RULES_SPEC_v0.1.md`. The same crate is meant to
//! run inside the MagicBlock ER program, the replay verifier, and (via WASM) the
//! browser client and agents, so it is `no_std`, allocation-only and free of
//! floating point. Section references (`§x.y`) point into the spec.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod battle;
pub mod buildings;
pub mod combat;
pub mod diplomacy;
pub mod economy;
pub mod error;
pub mod fixed;
pub mod genesis;
pub mod hex;
pub mod invariants;
pub mod map;
pub mod orders;
pub mod params;
pub mod rng;
pub mod scoring;
pub mod state;
pub mod tech;
pub mod tick;
pub mod units;

pub use error::RulesError;
pub use params::{Preset, Ruleset};
pub use state::WorldState;
