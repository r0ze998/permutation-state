//! Fixtures: beacon rounds (the 32 SP-V2 quicknet rounds and test-beacon
//! rounds signed on demand, I-53) and march seals (the S-TLOCK vectors and
//! seals built with the stock `tlock` crate for every seal code).

pub mod beacons;
pub mod tlock;

pub use beacons::Beacons;
