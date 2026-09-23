//! Deterministic randomness (§0.2).
//!
//! `seed_t` mixes the season seed with the tick's VRF output, so nobody can
//! pre-simulate variance while the order window is open. Every draw names a
//! domain; the domain is length-prefixed so `("ab", "c")` and `("a", "bc")`
//! can never collide.

use sha2::{Digest, Sha256};

pub type Seed = [u8; 32];

/// `seed_t = sha256(season_seed ‖ vrf_t ‖ t)`.
pub fn tick_seed(season_seed: &Seed, vrf: &Seed, tick: u16) -> Seed {
    let mut h = Sha256::new();
    h.update(season_seed);
    h.update(vrf);
    h.update(tick.to_le_bytes());
    h.finalize().into()
}

/// `rand(seed, domain, id)` = first 8 bytes (LE) of
/// `sha256(seed ‖ len(domain) ‖ domain ‖ id)`.
pub fn rand(seed: &Seed, domain: &[u8], id: &[u8]) -> u64 {
    debug_assert!(domain.len() <= u8::MAX as usize);
    let mut h = Sha256::new();
    h.update(seed);
    h.update([domain.len() as u8]);
    h.update(domain);
    h.update(id);
    let out = h.finalize();
    let mut b = [0u8; 8];
    b.copy_from_slice(&out[..8]);
    u64::from_le_bytes(b)
}

/// Convenience for numeric ids.
pub fn rand_id(seed: &Seed, domain: &[u8], id: u64) -> u64 {
    rand(seed, domain, &id.to_le_bytes())
}

/// Tie-break priority (§0.2): lower key wins.
pub fn tie_key(seed: &Seed, id: u64) -> u64 {
    rand_id(seed, b"tie", id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_domain_separated() {
        let s = [7u8; 32];
        assert_eq!(rand(&s, b"var", b"1"), rand(&s, b"var", b"1"));
        assert_ne!(rand(&s, b"var", b"1"), rand(&s, b"tie", b"1"));
        assert_ne!(rand(&s, b"ab", b"c"), rand(&s, b"a", b"bc"));
    }

    #[test]
    fn tick_seed_depends_on_every_input() {
        let a = tick_seed(&[1; 32], &[2; 32], 5);
        assert_ne!(a, tick_seed(&[9; 32], &[2; 32], 5));
        assert_ne!(a, tick_seed(&[1; 32], &[9; 32], 5));
        assert_ne!(a, tick_seed(&[1; 32], &[2; 32], 6));
    }
}
