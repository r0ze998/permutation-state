//! Operator AI members (V5 §18.2–§18.4): who they are is hidden while the
//! season is played and proven after it.
//!
//! Every member registers with a 32-byte `tag`. People put random bytes
//! there; an operator AI puts `roster_tag(season, wallet, salt)`. Before
//! registration opens the operator commits the number of its AI members and
//! `roster_chain`, the tags linked in order. After the season it reveals each
//! AI's salt: the tag recomputes, and it sits in a registration the AI's own
//! wallet signed, so nobody can be named an AI who is not one (a person's
//! random tag has no salt).
//!
//! An AI's home city is drawn from its nation's cities at the end of
//! `ai_home_tick` with its salt and the season seed, so nobody (the operator
//! included) knows it before; the first conquest of that city afterwards
//! earns the bounty.

use crate::hash::sha256;
use crate::state::{CityId, CivId, WorldState};
use alloc::vec;
use alloc::vec::Vec;

/// The tag an operator AI registers with.
pub fn roster_tag(season_id: u64, wallet: &[u8; 32], salt: &[u8; 32]) -> [u8; 32] {
    sha256(&[
        b"permutation-rules/ai",
        &season_id.to_le_bytes(),
        wallet,
        salt,
    ])
}

/// One link of the roster chain: `roster_chain` is `link(… link(link(0, t₁), t₂) …, tₙ)`.
pub fn roster_link(prev: &[u8; 32], tag: &[u8; 32]) -> [u8; 32] {
    sha256(&[b"permutation-rules/roster", prev, tag])
}

/// The chain over tags in order, from the zero root.
pub fn roster_chain(tags: &[[u8; 32]]) -> [u8; 32] {
    tags.iter().fold([0; 32], |acc, t| roster_link(&acc, t))
}

/// An AI's home city: one of its nation's cities recorded at the end of
/// `ai_home_tick`, chosen by its salt and the season seed. `None` before
/// that tick, or if the nation had no city then.
pub fn home_city(state: &WorldState, civ: CivId, salt: &[u8; 32]) -> Option<CityId> {
    let cities = state.home_snapshot.get(civ as usize)?;
    if cities.is_empty() {
        return None;
    }
    let h = sha256(&[b"permutation-rules/home", salt, &state.season_seed]);
    let mut x = [0u8; 8];
    x.copy_from_slice(&h[..8]);
    Some(cities[(u64::from_le_bytes(x) % cities.len() as u64) as usize])
}

/// Where the bounties go (V5 §18.4).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bounties {
    /// USDC added to each nation's prize share.
    pub by_civ: Vec<u64>,
    /// USDC of AIs whose home was never conquered (or conquered by a pact
    /// partner): it joins the pool.
    pub unpaid: u64,
    /// Each AI's home city, in roster order.
    pub homes: Vec<Option<CityId>>,
}

/// Bounties of the revealed AIs, given as (nation, salt) in roster order,
/// `each` USDC per AI.
pub fn bounties(state: &WorldState, ais: &[(CivId, [u8; 32])], each: u64) -> Bounties {
    let mut out = Bounties {
        by_civ: vec![0; state.civs.len()],
        unpaid: 0,
        homes: Vec::with_capacity(ais.len()),
    };
    for (civ, salt) in ais {
        let home = home_city(state, *civ, salt);
        out.homes.push(home);
        let conquest = home
            .and_then(|c| state.cities.get(c as usize))
            .and_then(|c| c.first_conquest)
            .filter(|q| q.bounty);
        match conquest {
            Some(q) if (q.by as usize) < out.by_civ.len() => out.by_civ[q.by as usize] += each,
            _ => out.unpaid += each,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_bind_the_wallet_and_the_chain_binds_the_order() {
        let (w1, w2, s) = ([1; 32], [2; 32], [9; 32]);
        assert_ne!(roster_tag(1, &w1, &s), roster_tag(1, &w2, &s));
        assert_ne!(roster_tag(1, &w1, &s), roster_tag(2, &w1, &s));
        let (a, b) = (roster_tag(1, &w1, &s), roster_tag(1, &w2, &s));
        assert_ne!(roster_chain(&[a, b]), roster_chain(&[b, a]));
        assert_eq!(roster_chain(&[]), [0; 32]);
        assert_eq!(roster_chain(&[a]), roster_link(&[0; 32], &a));
    }
}
