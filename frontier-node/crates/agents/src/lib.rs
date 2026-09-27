//! `frontier-agents` (M1 contract §8.6, unit W3-E): what a Frontier bot is
//! and how it decides, with no IO.
//!
//! | module | what |
//! |---|---|
//! | [`profile`] | `Arch`/`Profile` copied from `frontier-sim/src/model.rs` (equality test, I-36); the mix and roster |
//! | [`persona`] | the thirteen adversarial personas and their expected outcomes |
//! | [`rng`] | the simulator's SplitMix64 |
//! | [`keys`] | wallet, session and direct keys from the fleet seed |
//! | [`obs`] | herald files (§8.4, §9.2, §9.3) → an [`obs::Observation`] |
//! | [`path`] | march paths over observed provinces (Reveal check 6) |
//! | [`policy`] | [`policy::decide`]: observation → intents, deterministic in (seed, observation) |
//! | [`fixture`] | the fixed herald world the unit tests read |
//!
//! The runner, transports, sealing and journal are `frontier-bots`.

pub mod fixture;
pub mod keys;
pub mod obs;
pub mod path;
pub mod persona;
pub mod policy;
pub mod profile;
pub mod rng;

pub use persona::Persona;
pub use profile::{profile, AgentSpec, Arch, Mix, Profile, ARCHS};
