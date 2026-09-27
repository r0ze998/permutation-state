//! On-chain cryptography, ported from SP-V2 (`m0b/spikes/SP-V2/program`)
//! unchanged except where noted (M1 contract §3.1, I-44, I-53):
//!
//! | module | content | change from SP-V2 |
//! |---|---|---|
//! | [`sys`] | raw SIMD-0388 BLS12-381 syscalls (G1 add/decompress, G2 decompress/mul, pairing) | none (host stubs fail closed) |
//! | [`field`] | canonical big-endian field codecs, out-of-line mul/square | none |
//! | [`xmd`] | RFC 9380 `expand_message_xmd` over SHA-256 | hashes through `permutation_rules::hash` (the same syscall on chain) |
//! | [`quick`] | hinted quicknet signature verification, bell seeds | signature and hints as separate arguments (ABI §5.8); feature `test-beacon` pins the local test key (I-53) |
//! | [`seal`] | the tlock opener with the Fujisaki–Okamoto check and the seal codes SettleTransit logs | returns the contract's seal codes 0–5 and the opened plaintext (I-44), salt-based commitment (I-06) |
//!
//! The arkworks field operations stay `#[inline(never)]` (§3.3): inlined,
//! ark's unrolled Montgomery code gives the calling function an SBF stack
//! frame far above the 4 KiB limit (measured in S-BEACON).

pub mod field;
pub mod quick;
pub mod seal;
pub mod sys;
pub mod xmd;
