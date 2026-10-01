//! `herald-fold` (M1 contract §8.4, §9.2, §9.3): the Frontier herald.
//!
//! | module | what |
//! |---|---|
//! | [`fold`] | archived transactions → captures, per-bell files, WS diffs (deterministic) |
//! | [`clash`] | clash reports recomputed natively (`ClashBuilder`, the provisional §5.11 builder) |
//! | [`overview`] | the §9.3 overview binary |
//! | [`records`] | PS2 records decoded, account and record keys |
//! | [`views`] | live answers: `/h/season`, `latest` provinces, `/h/me` |
//! | [`files`] | atomic writes with deterministic `.gz` siblings |
//! | [`checkpoint`] | the fold's state saved and restored |
//! | [`runner`] | findex → fold → files and diffs → checkpoints |
//! | [`server`] | `/h/*`, `WS /h/ws`, `/frontier/*`, `/gw/*`, security headers and CSP |
//! | [`ws`] | RFC 6455 frames, subscriptions, per-socket sequence and resync |
//! | [`viewers`] | the viewer load generator (`frontier-viewers`) |
//! | [`fixture`] | a synthetic mini-season archive (tests, load runs) |
//!
//! The herald is never a trust root: every answer carries raw account bytes
//! with their slot and event head, and the clash report says which builder
//! recomputed it.

pub mod checkpoint;
pub mod clash;
pub mod files;
pub mod fixture;
pub mod fold;
pub mod overview;
pub mod records;
pub mod roster;
pub mod runner;
pub mod server;
pub mod viewers;
pub mod views;
pub mod ws;

/// `magic "PSFOV1\0\0"`.
pub const OVERVIEW_MAGIC: &[u8; 8] = b"PSFOV1\0\0";
pub const OVERVIEW_RECORD: usize = 24;

/// Header 32 B: magic · season u64 · ring u16 · n u16 · bell u32 · slot u64.
pub fn overview_header(season: u64, ring: u16, n: u16, bell: u32, slot: u64) -> [u8; 32] {
    let mut h = [0u8; 32];
    h[..8].copy_from_slice(OVERVIEW_MAGIC);
    h[8..16].copy_from_slice(&season.to_le_bytes());
    h[16..18].copy_from_slice(&ring.to_le_bytes());
    h[18..20].copy_from_slice(&n.to_le_bytes());
    h[20..24].copy_from_slice(&bell.to_le_bytes());
    h[24..32].copy_from_slice(&slot.to_le_bytes());
    h
}

/// Ports no M1 process binds (contract §10.3).
pub const RESERVED_PORTS: [u16; 11] = [
    4185, 4190, 4191, 4194, 18899, 17799, 28899, 27799, 26699, 5185, 5191,
];

/// Whether a configured port is allowed: 0 (tests), or 41000–41999 and
/// not reserved.
pub fn port_allowed(p: u16) -> bool {
    p == 0 || ((41_000..=41_999).contains(&p) && !RESERVED_PORTS.contains(&p))
}

#[cfg(test)]
mod tests {
    #[test]
    fn header_layout() {
        let h = super::overview_header(1, 2, 3, 4, 5);
        assert_eq!(&h[..8], super::OVERVIEW_MAGIC);
        assert_eq!(h[16], 2);
        assert_eq!(h[24], 5);
    }

    #[test]
    fn ports() {
        assert!(super::port_allowed(0));
        assert!(super::port_allowed(41_040));
        assert!(!super::port_allowed(4_190));
        assert!(!super::port_allowed(8_080));
    }
}
