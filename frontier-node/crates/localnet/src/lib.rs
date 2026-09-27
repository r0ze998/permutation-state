//! `frontier-localnet` (M1 contract §8.7): the local chain node of Mode A.
//!
//! - [`chain`]: LiteSVM 0.16 (mainnet features incl. the SIMD-0388
//!   BLS12-381 syscalls), mainnet rent, the block builder (priority order,
//!   100M per block, 40M per writable account), the contention emulator
//!   (`frontier_hold`), the scaled Clock (real 400-ms slots, I-54),
//!   SIMD-0186 loaded-data enforcement with the fee charged (I-45), the
//!   ordered feed, snapshots and restore;
//! - [`persist`]: the ledger WAL and snapshot formats (crash safety);
//! - [`server`]: the JSON-RPC subset, `frontier_*` extensions, the ticker;
//! - [`ws`]: `slotSubscribe`, `signatureSubscribe`, `logsSubscribe`;
//! - [`inprocess`]: `ChainPort::InProcess` on virtual time.
pub mod chain;
pub mod inprocess;
pub mod persist;
pub mod server;
pub mod ws;

pub use chain::{BlockReport, Chain, Config};
pub use inprocess::InProcess;

/// Whether the node's feature set (LiteSVM's mainnet set, which
/// `LiteSVM::new()` installs and [`Chain::new`] uses) carries the
/// BLS12-381 syscalls (SIMD-0388) and SBPF v2 execution.
pub fn bls_and_sbpf_v2_active() -> (bool, bool) {
    let f = litesvm::LiteSVM::mainnet_feature_set();
    (
        f.is_active(&agave_feature_set::enable_bls12_381_syscall::id()),
        f.is_active(&agave_feature_set::enable_sbpf_v2_deployment_and_execution::id()),
    )
}
