//! The overview binary (contract §9.3): `/h/overview/{ring}/{bell}.bin`.
//!
//! Header 32 B: `magic "PSFOV1\0\0"` · season u64 · ring u16 · n u16 ·
//! bell u32 · slot u64. Then `n` records × 24 B sorted by (P, Q): `P i16 ·
//! Q i16 · owners 40 bits (12 × 3-bit faction, site i at bits 3i..3i+2: 0–5,
//! 6 neutral/camp, 7 none) · site_state 24 bits (12 × 2 bits, site i at 2i:
//! 0 free, 1 holding, 2 camp, 3 reserved/released) · hosts_by_faction [7]
//! u8 (saturating) · flags u8 (1 clash this bell, 2 any dormant holding, 4
//! province opened this bell) · resolved_next u32`; every integer and bit
//! field little-endian (the JS decoder `client/src/frontier/herald.mjs`
//! reads the same bit order).
//!
//! Site mapping from the Province site mirror (§5.3): state 1 (holding) →
//! owner = the mirror's faction, site state 1; state 2 (the unused camp
//! state) → owner 6, site state 2; state 3 (released) and 4 (reserved,
//! rings 0–1) → owner 7, site state 3; state 0 → owner 7, free. Sites at
//! or beyond the province's `site_count` do not exist: owner 7, state 3
//! (never shown as free). `hosts_by_faction[f]` counts roster entries
//! (state 1) of faction f; a present camp counts as one host of the
//! neutral faction (6).

use fclient::decode::Province;
use frontier_abi::layout::province::{entry as E, site as S};

pub use crate::{overview_header, OVERVIEW_MAGIC, OVERVIEW_RECORD};

pub const OVERVIEW_HEADER: usize = 32;
pub const FLAG_CLASH: u8 = 1;
pub const FLAG_DORMANT: u8 = 2;
pub const FLAG_OPENED: u8 = 4;
pub const OWNER_NEUTRAL: u8 = 6;
pub const OWNER_NONE: u8 = 7;

/// One province's 24-byte record.
pub fn record(pv: &Province, flags: u8) -> [u8; OVERVIEW_RECORD] {
    let mut r = [0u8; OVERVIEW_RECORD];
    r[0..2].copy_from_slice(&pv.p.to_le_bytes());
    r[2..4].copy_from_slice(&pv.q.to_le_bytes());
    let mut owners: u64 = 0;
    let mut sites: u32 = 0;
    for i in 0..12usize {
        let (owner, state) = if i >= pv.site_count as usize {
            (OWNER_NONE, 3u32)
        } else {
            let m = &pv.site_mirror[i];
            match m.state {
                S::STATE_HOLDING => (m.faction.min(OWNER_NEUTRAL), 1),
                S::STATE_UNUSED_CAMP => (OWNER_NEUTRAL, 2),
                S::STATE_RELEASED_FREE | S::STATE_RESERVED => (OWNER_NONE, 3),
                _ => (OWNER_NONE, 0),
            }
        };
        owners |= (owner as u64 & 7) << (3 * i);
        sites |= (state & 3) << (2 * i);
    }
    r[4..9].copy_from_slice(&owners.to_le_bytes()[..5]);
    r[9..12].copy_from_slice(&sites.to_le_bytes()[..3]);
    let mut hosts = [0u8; 7];
    for e in pv.entries.iter().filter(|e| e.state == E::STATE_ROSTER) {
        if let Some(h) = hosts.get_mut(e.faction as usize) {
            *h = h.saturating_add(1);
        }
    }
    if pv.camp.state == 1 {
        hosts[6] = hosts[6].saturating_add(1);
    }
    r[12..19].copy_from_slice(&hosts);
    r[19] = flags;
    r[20..24].copy_from_slice(&pv.resolved_next.to_le_bytes());
    r
}

/// A whole file: header + records (the caller passes them sorted by (P, Q)).
pub fn file(
    season: u64,
    ring: u16,
    bell: u32,
    slot: u64,
    recs: &[[u8; OVERVIEW_RECORD]],
) -> Vec<u8> {
    let n = recs.len().min(u16::MAX as usize);
    let mut out = Vec::with_capacity(OVERVIEW_HEADER + n * OVERVIEW_RECORD);
    out.extend_from_slice(&overview_header(season, ring, n as u16, bell, slot));
    for r in &recs[..n] {
        out.extend_from_slice(r);
    }
    out
}
