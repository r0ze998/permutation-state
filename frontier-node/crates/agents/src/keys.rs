//! Bot keys from the fleet seed (offchain design §9, "each bot has a wallet
//! and a session key derived from `--seed`"):
//! `ed25519(sha256("PS-FRONTIER-BOT-v1" ‖ le64(seed) ‖ le32(index) ‖ role))`
//! with role `wallet`, `session` or `direct` (the funded key a persona
//! pays its own transactions with). Test and local-season keys only: the
//! seed is not a secret.

use fclient::Keypair;
use sha2::{Digest, Sha256};

pub const DOMAIN: &[u8] = b"PS-FRONTIER-BOT-v1";

fn derive(seed: u64, index: u32, role: &str) -> Keypair {
    let sk: [u8; 32] = Sha256::new()
        .chain_update(DOMAIN)
        .chain_update(seed.to_le_bytes())
        .chain_update(index.to_le_bytes())
        .chain_update(role.as_bytes())
        .finalize()
        .into();
    Keypair::new_from_array(sk)
}

/// The bot's wallet (signs Join and SetSession).
pub fn wallet(seed: u64, index: u32) -> Keypair {
    derive(seed, index, "wallet")
}

/// The bot's session key (signs every other player instruction).
pub fn session(seed: u64, index: u32) -> Keypair {
    derive(seed, index, "session")
}

/// The persona's own fee payer for direct transactions.
pub fn direct(seed: u64, index: u32) -> Keypair {
    derive(seed, index, "direct")
}

#[cfg(test)]
mod tests {
    use super::*;
    use fclient::Signer;

    #[test]
    fn keys_are_distinct_and_stable() {
        let a = wallet(1, 0).pubkey();
        assert_eq!(a, wallet(1, 0).pubkey());
        assert_ne!(a, wallet(1, 1).pubkey());
        assert_ne!(a, wallet(2, 0).pubkey());
        assert_ne!(a, session(1, 0).pubkey());
        assert_ne!(a, direct(1, 0).pubkey());
    }
}
