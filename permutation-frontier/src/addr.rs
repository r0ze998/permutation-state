//! Addresses (M1 contract §4.1, I-01, I-02).
//!
//! `addr(kind, key) = sha256(season_pda ‖ seed ‖ program_id)` with `seed =
//! tag(2) ‖ lowercase hex of the little-endian key fields`: the grammar is
//! the kernel's (`permutation_rules::frontier::addr`) through
//! `frontier_abi::addr`, byte for byte SP-V2's. The Season is the only PDA,
//! `["season", le64(id)]`, its bump found once by AnnounceSeason and stored;
//! nobody ever passes a bump.
//!
//! **Presence** is authenticated by owner = program, magic, `season_id` and
//! the stored key fields; readers that need *the* instance also recompute
//! the address. **Absence** is the canonical address, owner System, no
//! data; lamports are ignored (pre-funded = absent).
//!
//! On chain the address hash is the `sol_sha256` syscall (≈ 0.5k CU per
//! address with the seed formatting).

pub use frontier_abi::addr::{
    citizen_tag, citizen_tag15, day_of, host_id, join_shard_of, keeper_tag8, split_host_id,
    with_seed, AddrCtx, HostParts, Seed,
};
pub use frontier_abi::prologue::ids;

/// Seed prefix of the Season PDA.
pub const SEASON_PREFIX: &[u8] = frontier_abi::layout::world::season::PDA_PREFIX;

/// The Season PDA's seeds without the bump: `("season", le64(id))`.
pub fn season_seed_id(id: u64) -> [u8; 8] {
    id.to_le_bytes()
}

/// Absent: owned by the System program and holding no data (§4.1).
pub fn absent(owner: &[u8; 32], data_len: usize) -> bool {
    *owner == ids::SYSTEM_PROGRAM && data_len == 0
}

/// Address context of a season: the canonical address of every with-seed
/// account, from the Season PDA and the program id.
pub fn ctx(season: &[u8; 32], program: &[u8; 32]) -> AddrCtx {
    AddrCtx {
        season: *season,
        program: *program,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_seed_is_sha256_of_base_seed_owner() {
        let season = [7u8; 32];
        let program = [9u8; 32];
        let c = ctx(&season, &program);
        let s = frontier_abi::addr::bell_anchor_seed(0x0102_0304, 5);
        assert_eq!(s.as_bytes(), b"an0403020105");
        let want = permutation_rules::hash::sha256(&[&season, s.as_bytes(), &program]);
        assert_eq!(c.bell_anchor(0x0102_0304, 5), want);
        assert_eq!(with_seed(&season, &s, &program), want);
    }

    #[test]
    fn absence_ignores_lamports_but_not_owner_or_data() {
        assert!(absent(&[0; 32], 0));
        assert!(!absent(&[1; 32], 0));
        assert!(!absent(&[0; 32], 1));
    }

    #[test]
    fn season_seed_is_little_endian() {
        assert_eq!(season_seed_id(0x0102), [2, 1, 0, 0, 0, 0, 0, 0]);
        assert_eq!(SEASON_PREFIX, b"season");
    }
}
