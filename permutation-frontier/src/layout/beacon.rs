//! Beacon accounts (§5.3): BellAnchor, SeedCache, AnchorArchive (with its
//! entries), DefenceClaim. Offsets: `frontier_abi::layout::beacon`.
//!
//! The readers below are the typed records every instruction that consults
//! THE anchor, a cache or an archive uses (PostSeed here; GatherClash,
//! ResolveFromInputs, Reveal, SettleTransit and ArchiveAnchors later).

pub use frontier_abi::layout::beacon::{
    anchor_archive, archive_entry, bell_anchor, defence_claim, seed_cache,
};

use super::Ro;
use crate::R;

/// THE anchor of `(bell, region)` as stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anchor {
    pub bell: u32,
    pub region: u8,
    pub round: u64,
    /// `A`: the Clock at the anchor's creation.
    pub a: i64,
    pub slot: u64,
    pub sig48: [u8; 48],
    pub rent_to: [u8; 32],
}

impl Anchor {
    pub fn read(d: &[u8]) -> R<Anchor> {
        let r = Ro(d);
        Ok(Anchor {
            bell: r.u32(bell_anchor::BELL)?,
            region: r.u8(bell_anchor::REGION)?,
            round: r.u64(bell_anchor::ROUND)?,
            a: r.i64(bell_anchor::A)?,
            slot: r.u64(bell_anchor::SLOT)?,
            sig48: r.arr(bell_anchor::SIG48)?,
            rent_to: r.arr(bell_anchor::RENT_TO)?,
        })
    }
}

/// A SeedCache as stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cache {
    pub bell: u32,
    pub region: u8,
    pub nonce: u8,
    pub round: u64,
    pub seed: [u8; 32],
    pub anchor_key: [u8; 32],
    pub a: i64,
}

impl Cache {
    pub fn read(d: &[u8]) -> R<Cache> {
        let r = Ro(d);
        Ok(Cache {
            bell: r.u32(seed_cache::BELL)?,
            region: r.u8(seed_cache::REGION)?,
            nonce: r.u8(seed_cache::NONCE)?,
            round: r.u64(seed_cache::ROUND)?,
            seed: r.arr(seed_cache::SEED)?,
            anchor_key: r.arr(seed_cache::ANCHOR_KEY)?,
            a: r.i64(seed_cache::A)?,
        })
    }
}

/// The region and part (half day, v1.3) an AnchorArchive covers.
pub fn archive_key(d: &[u8]) -> R<(u8, u32)> {
    let r = Ro(d);
    Ok((r.u8(anchor_archive::REGION)?, r.u32(anchor_archive::PART)?))
}

/// Whether the archive tombstones bell `b` (its anchor was archived and
/// closed, or is being closed: the bit is set first, §4.2).
pub fn archive_tombstoned(d: &[u8], b: u32) -> R<bool> {
    let (at, mask) = anchor_archive::bit(anchor_archive::TOMBSTONE, b);
    Ok(Ro(d).u8(at)? & mask != 0)
}

/// Whether the archive holds bell `b`'s entry.
pub fn archive_archived(d: &[u8], b: u32) -> R<bool> {
    let (at, mask) = anchor_archive::bit(anchor_archive::ARCHIVED, b);
    Ok(Ro(d).u8(at)? & mask != 0)
}

/// Bell `b`'s archive entry: `(a_off, seed, sig48)`.
pub fn archive_entry_of(d: &[u8], b: u32) -> R<(u32, [u8; 32], [u8; 48])> {
    let o = anchor_archive::entry(b);
    let r = Ro(d);
    Ok((
        r.u32(o + archive_entry::A_OFF)?,
        r.arr(o + archive_entry::SEED)?,
        r.arr(o + archive_entry::SIG)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{init_header, AccountKind, Rw};

    #[test]
    fn archive_bits_and_entries() {
        let mut d = alloc::vec![0u8; anchor_archive::SIZE];
        init_header(&mut d, AccountKind::AnchorArchive, 1).unwrap();
        let mut w = Rw(&mut d);
        w.set_u8(anchor_archive::REGION, 7).unwrap();
        w.set_u32(anchor_archive::PART, 3).unwrap();
        let b = 3 * 72 + 71;
        let (at, m) = anchor_archive::bit(anchor_archive::TOMBSTONE, b);
        w.set_u8(at, m).unwrap();
        let o = anchor_archive::entry(b);
        w.set_u32(o + archive_entry::A_OFF, 9).unwrap();
        w.set_arr(o + archive_entry::SIG, &[5; 48]).unwrap();
        assert_eq!(archive_key(&d).unwrap(), (7, 3));
        assert!(archive_tombstoned(&d, b).unwrap());
        assert!(!archive_tombstoned(&d, b - 1).unwrap());
        assert!(!archive_archived(&d, b).unwrap());
        let (a_off, seed, sig) = archive_entry_of(&d, b).unwrap();
        assert_eq!((a_off, seed, sig), (9, [0; 32], [5; 48]));
    }
}
