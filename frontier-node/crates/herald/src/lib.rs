//! `herald-fold` (M1 contract §8.4, §9.3).
//!
//! **W1 skeleton.** W3 builds the fold, the files and the WS feed. What
//! exists now is the overview binary header of §9.3.

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

#[cfg(test)]
mod tests {
    #[test]
    fn header_layout() {
        let h = super::overview_header(1, 2, 3, 4, 5);
        assert_eq!(&h[..8], super::OVERVIEW_MAGIC);
        assert_eq!(h[16], 2);
        assert_eq!(h[24], 5);
    }
}
