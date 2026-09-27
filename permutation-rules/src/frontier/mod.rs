//! Rules v10: the Sixfold Frontier (open-world design, revision 2).
//!
//! A separate module tree next to the v9 (Skirmish) rules, which it does
//! not change (design §11.1). Kernels here are pure functions of their
//! inputs, integer only, so the Frontier program, the verifier and the
//! host simulator call the same code.

/// Rules version of the Frontier. v9 (`params::RULES_VERSION`) stays the
/// Skirmish rules; golden tests pin both.
pub const RULES_VERSION_FRONTIER: u16 = 10;

pub mod clash;
pub mod doctrine;
pub mod geometry;
pub mod holding;
pub mod host;
pub mod index;
pub mod laurel;
pub mod mandate;
pub mod payout;
pub mod pools;
pub mod siege;
pub mod stance;
pub mod terrain;
pub mod travel;
