//! Program-level tests of Wylls (M1 contract §3.5, §11 W2-B,
//! §13): the SBPF v2 build of `permutation-frontier` run in LiteSVM 0.16.
//!
//! | module | what |
//! |---|---|
//! | [`chain`] | the SVM, the program deployed under LoaderV3 at its `--max-len`, sending with the client's compute-budget profile, **SIMD-0186 loaded-data enforcement** (I-45) with its control, accounts, the clock |
//! | [`fixtures`] | the 32 SP-V2 quicknet rounds, the S-TLOCK seal vectors and seal builders, **test-beacon rounds** signed on demand (I-53) |
//! | [`records`] | the program's PS2 records and the event-chain check of chained accounts |
//! | [`budget`] | what a transaction needs: CU, heap (trace build), tx bytes, locks, loaded data; the §5.5 ceilings |
//! | [`wallets`] | seeded wallets (CL-28) |
//! | [`probe`] | the stand-alone probe program: SIMD-0186 control, pre-funding regression |
//! | [`world`] | seasons and area builders (`land`, `holding`, `clash`, `transit`: handed over, §11) |
//! | [`ix`] | builders per area (the client's account order), plus forgery helpers |
//! | [`cover`] | the (instruction, error code) coverage registry (G13) with one area file per owner |
//!
//! Test names start with their gate (`g01_…` … `g14_…`) so a gate runs by
//! prefix: `./run.sh --release -- g01_loaded_ g02_ g03_ g04_ g05_`.

pub mod budget;
pub mod chain;
pub mod cover;
pub mod fixtures;
pub mod ix;
pub mod probe;
pub mod records;
pub mod wallets;
pub mod world;

pub use fclient::addr::Addresses;
pub use frontier_abi::error::FrontierError;
pub use frontier_abi::tags::Ix;
pub use solana_address::Address;
pub use solana_instruction::{AccountMeta, Instruction};
pub use solana_keypair::Keypair;
pub use solana_signer::Signer;

/// sha256 of the concatenated parts.
pub fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

/// A deterministic keypair from a label (`sha256(label)` as the secret).
pub fn keypair(label: &[u8]) -> Keypair {
    Keypair::new_from_array(sha256(&[b"PSF-SVM-TESTS-KEY", label]))
}

/// Seeded xorshift64* (§3.5: seeded, no proptest).
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in `[lo, hi]` (inclusive; `hi ≥ lo`).
    pub fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next_u64() % (hi - lo + 1)
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n.max(1)
    }
    pub fn coin(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }
}
