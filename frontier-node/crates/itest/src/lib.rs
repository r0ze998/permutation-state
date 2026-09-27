//! `itest`: in-process integration tests (M1 contract §3.5, I-57).
//!
//! **W1 skeleton.** The wave-4 integration unit owns this crate
//! (`inproc_day`, the lag gate, crash injection, `g14_`). What exists now is
//! one smoke test (`tests/inproc_smoke.rs`) that wires the W1 pieces the way
//! those tests will: `localnet::InProcess` on virtual time at 20×,
//! `fclient`'s rule times and the `drand-replay` test key.
