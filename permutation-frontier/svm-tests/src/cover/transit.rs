//! Coverage of the transit area: stub, **handed over to W4-B** (M1 contract §11).
//! Replace each `Pending` with the tests that cover the instruction.

use super::Cover;

/// Ignored tests of this area waiting for a fix: (test, unit).
pub const PENDING: &[(&str, &str)] = &[];

pub const SETTLE_TRANSIT: &[Cover] = &[Cover::Pending("W4-B")];

pub const SWEEP_POOL_OWED: &[Cover] = &[Cover::Pending("W4-B")];
