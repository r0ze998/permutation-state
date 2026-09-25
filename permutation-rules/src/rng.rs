//! Deterministic randomness (§0.2).
//!
//! `seed_t` mixes the season seed with the tick's VRF output, so nobody can
//! pre-simulate variance while the order window is open. Every draw names a
//! domain; the domain is length-prefixed so `("ab", "c")` and `("a", "bc")`
//! can never collide.

use crate::hash::sha256;

pub type Seed = [u8; 32];

/// `seed_t = sha256(season_seed ‖ vrf_t ‖ t)`.
pub fn tick_seed(season_seed: &Seed, vrf: &Seed, tick: u16) -> Seed {
    sha256(&[season_seed, vrf, &tick.to_le_bytes()])
}

/// One revealed salt of a tick's sealed orders: `(civ, role index, salt)`.
pub type Salt = (u16, u8, [u8; 32]);

/// The tick's randomness on chain (§0.2, revised 2026-09-25): the world
/// root before the tick and every salt revealed with the sealed orders, in
/// (civ, role) order. The salts were fixed (inside commitments) before the
/// tick froze and are unknown to everyone but their officer until the reveal,
/// so no one — the crank that picks the transaction's slot included — can
/// choose the outcome. Withholding a reveal (to steer the result) forfeits
/// that office's orders for the tick.
pub fn tick_vrf(pre_root: &[u8; 32], salts: &[Salt]) -> Seed {
    let mut parts: alloc::vec::Vec<[u8; 35]> = alloc::vec::Vec::with_capacity(salts.len());
    for (civ, role, salt) in salts {
        let mut p = [0u8; 35];
        p[..2].copy_from_slice(&civ.to_le_bytes());
        p[2] = *role;
        p[3..].copy_from_slice(salt);
        parts.push(p);
    }
    let mut all: alloc::vec::Vec<&[u8]> =
        alloc::vec![b"permutation-rules/tick-vrf".as_slice(), pre_root];
    all.extend(parts.iter().map(|p| p.as_slice()));
    sha256(&all)
}

/// `rand(seed, domain, id)` = first 8 bytes (LE) of
/// `sha256(seed ‖ len(domain) ‖ domain ‖ id)`.
pub fn rand(seed: &Seed, domain: &[u8], id: &[u8]) -> u64 {
    debug_assert!(domain.len() <= u8::MAX as usize);
    let out = sha256(&[seed, &[domain.len() as u8], domain, id]);
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
