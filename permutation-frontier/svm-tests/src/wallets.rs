//! Seeded wallets (CL-28): Join's CU and every address a wallet derives are
//! reproducible across runs.
//!
//! `wallet(i) = Keypair::new_from_array(sha256("join" ‖ le32(i)))`, the
//! closeout's rule. v9's worry (a PDA bump search whose cost depends on the
//! wallet) does not exist in M1: the Citizen is a with-seed address of the
//! Season PDA (I-01), `ct‖hex(sha256("PSF-CIT" ‖ wallet)[0..15])`, so Join
//! hashes a fixed-length input for every wallet. The **adversarial wallet**
//! of §13.1's Join row is therefore chosen for contention and extremes,
//! not hash length: among the seeded wallets 1,000,000 … 1,004,095, the
//! one on JoinShard 0 (`sha256(wallet)[0] mod 8 == 0`) whose citizen tag
//! sorts last — it shares shard 0 with ≈ 1/8 of the bench (lock
//! contention on one JoinShard) and is the worst case for any scan by tag. `adversarial_is_stable` pins it.

use fclient::addr::{citizen_tag15, join_shard_of};
use solana_keypair::Keypair;
use solana_signer::Signer;

use crate::sha256;

/// Seeded wallets in the Join fill (§13.1).
pub const SEEDED: u32 = 1_000;
/// First index of the adversarial search.
pub const ADVERSARIAL_FROM: u32 = 1_000_000;
/// Candidates the adversarial search looks at.
pub const ADVERSARIAL_CANDIDATES: u32 = 4_096;

/// Seeded wallet `i`.
pub fn wallet(i: u32) -> Keypair {
    Keypair::new_from_array(sha256(&[b"join", &i.to_le_bytes()]))
}

/// The first `n` seeded wallets.
pub fn wallets(n: u32) -> Vec<Keypair> {
    (0..n).map(wallet).collect()
}

/// The adversarial wallet's seed index.
pub fn adversarial_index() -> u32 {
    let mut best: Option<(u32, [u8; 15])> = None;
    for i in ADVERSARIAL_FROM..ADVERSARIAL_FROM + ADVERSARIAL_CANDIDATES {
        let w = wallet(i).pubkey().to_bytes();
        if join_shard_of(&w) != 0 {
            continue;
        }
        let tag = citizen_tag15(&w);
        if best.as_ref().is_none_or(|(_, t)| tag > *t) {
            best = Some((i, tag));
        }
    }
    best.expect("shard 0 appears among 4,096 candidates").0
}

/// The adversarial wallet (see the module text).
pub fn adversarial() -> Keypair {
    wallet(adversarial_index())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_wallets_are_reproducible_and_distinct() {
        assert_eq!(wallet(7).pubkey(), wallet(7).pubkey());
        let w = wallets(64);
        for i in 0..w.len() {
            for j in i + 1..w.len() {
                assert_ne!(w[i].pubkey(), w[j].pubkey());
            }
        }
        // Every JoinShard of a faction receives some of the 1,000.
        let mut per = [0u32; 8];
        for i in 0..SEEDED {
            per[join_shard_of(&wallet(i).pubkey().to_bytes()) as usize] += 1;
        }
        assert!(per.iter().all(|n| *n > 60), "{per:?}");
    }

    #[test]
    fn adversarial_is_stable() {
        let i = adversarial_index();
        assert!((ADVERSARIAL_FROM..ADVERSARIAL_FROM + ADVERSARIAL_CANDIDATES).contains(&i));
        assert_eq!(join_shard_of(&adversarial().pubkey().to_bytes()), 0);
        assert_eq!(adversarial_index(), i);
    }
}
