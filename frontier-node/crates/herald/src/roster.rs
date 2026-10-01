//! The roster binary (design session, "people" request): who holds each
//! site, for the page's names and avatars — `/h/roster/{ring}/latest.bin`.
//! The chain keeps no names; the page derives a name and a face from the
//! owner's `citizen_tag` (the first 8 bytes of the Citizen address,
//! contract §4.1), the same for a human and a shade. Spectators read this
//! one small file per ring instead of the event log (5,000 viewers: the
//! answer is cached per fold version, like the overview's `latest.bin`).
//!
//! Header 32 B: `magic "PSFRS1\0\0"` · season u64 · ring u16 · n u16 ·
//! bell u32 · slot u64 (the overview header's layout). Then `n` records ×
//! 148 B sorted by (P, Q): `P i16 · Q i16 · 12 × {citizen_tag u64 ·
//! founded_bell u32}`; a site without a holding is all zero (a tag of 0
//! never names anyone). `founded_bell` is the bell containing the
//! Holding's `founded_ts` (a replay names a site only from that bell on).
//! Little-endian throughout; the JS decoder is `web/frontier/people/
//! roster.mjs`.

pub const ROSTER_MAGIC: &[u8; 8] = b"PSFRS1\0\0";
pub const ROSTER_HEADER: usize = 32;
pub const ROSTER_SITE: usize = 12;
pub const ROSTER_RECORD: usize = 4 + 12 * ROSTER_SITE;

/// One site's owner: `(citizen_tag, founded_bell)`.
pub type Owner = (u64, u32);

/// One province's record.
pub fn record(p: i16, q: i16, owners: &[Option<Owner>; 12]) -> [u8; ROSTER_RECORD] {
    let mut r = [0u8; ROSTER_RECORD];
    r[0..2].copy_from_slice(&p.to_le_bytes());
    r[2..4].copy_from_slice(&q.to_le_bytes());
    for (i, o) in owners.iter().enumerate() {
        if let Some((tag, bell)) = o {
            let at = 4 + i * ROSTER_SITE;
            r[at..at + 8].copy_from_slice(&tag.to_le_bytes());
            r[at + 8..at + 12].copy_from_slice(&bell.to_le_bytes());
        }
    }
    r
}

/// A whole file: header + records (the caller passes them sorted by (P, Q)).
pub fn file(season: u64, ring: u16, bell: u32, slot: u64, recs: &[[u8; ROSTER_RECORD]]) -> Vec<u8> {
    let n = recs.len().min(u16::MAX as usize);
    let mut out = Vec::with_capacity(ROSTER_HEADER + n * ROSTER_RECORD);
    let mut h = crate::overview_header(season, ring, n as u16, bell, slot);
    h[..8].copy_from_slice(ROSTER_MAGIC);
    out.extend_from_slice(&h);
    for r in &recs[..n] {
        out.extend_from_slice(r);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout() {
        let mut owners = [None; 12];
        owners[3] = Some((0x0102_0304_0506_0708, 42));
        let r = record(-2, 5, &owners);
        assert_eq!(&r[0..2], &(-2i16).to_le_bytes());
        assert_eq!(&r[2..4], &5i16.to_le_bytes());
        let at = 4 + 3 * ROSTER_SITE;
        assert_eq!(&r[at..at + 8], &0x0102_0304_0506_0708u64.to_le_bytes());
        assert_eq!(&r[at + 8..at + 12], &42u32.to_le_bytes());
        assert!(r[4..at].iter().all(|b| *b == 0));
        let f = file(7, 2, 100, 9, &[r]);
        assert_eq!(&f[..8], ROSTER_MAGIC);
        assert_eq!(f.len(), ROSTER_HEADER + ROSTER_RECORD);
        assert_eq!(u16::from_le_bytes([f[18], f[19]]), 1);
    }
}
