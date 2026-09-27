//! The march seal's plaintext, commitment and body (DESIGN §6.2 rev 3.1;
//! contract I-06, I-27, I-28, I-44).
//!
//! **Seal (165 B, compact-16):** `U (96, compressed G2) ‖ V (16) ‖ W (16) ‖
//! body (37)`. `U, V, W` are tlock's IBE block over a 16-byte key `k` to
//! the quicknet round `T(arrive_bell)` (the pairing part lives in the
//! program's `crypto::seal` and in `fclient`, never here); the body is the
//! 37-byte plaintext XOR a SHA-256 counter keystream of `k`
//! ([`body_xor`]).
//!
//! **Plaintext (37 B, pinned):**
//!
//! | Off | Size | Field |
//! |---|---|---|
//! | 0 | 1 | version u8 (= 1) |
//! | 1 | 8 | host_id u64 |
//! | 9 | 4 | arrive_bell u32 |
//! | 13 | 2 | dest P i16 |
//! | 15 | 2 | dest Q i16 |
//! | 17 | 1 | dest tile u8 (0..61) |
//! | 18 | 1 | stance u8 (0 Hold, 1 Assault, 2 Flank, 3 Brace) |
//! | 19 | 2 | retreat_bps u16 (0 = never; 1..=60,000) |
//! | 21 | 1 | path_len u8 (≤ 32) |
//! | 22 | 12 | path: 32 × 3-bit directions, little-endian bits |
//! | 34 | 3 | reserved (0) |
//!
//! Path step `i` occupies bits `3i .. 3i + 3` of the 96-bit little-endian
//! integer `path` (bit 0 = the low bit of byte 0). Directions are
//! `hex::DIRECTIONS` indices: 0 = E, then counter-clockwise.
//!
//! **Commitment:** `salt = sha256("PS-SALT" ‖ k)`, `commit =
//! sha256("PS-FRONTIER-MARCH-v1" ‖ plain37 ‖ salt)`, `seal_root =
//! sha256(commit ‖ sha256(seal))` (computed by Depart). Reveal and
//! SettleTransit take the salt, never `k` (I-06).

use crate::hash::sha256;
use crate::hex::DIRECTIONS;

use super::geometry::PROVINCE_TILES;
use super::travel::MAX_PATH_STEPS;

/// Kernel version of this module (part of the ruleset hash).
pub const SEAL_VERSION: u16 = 1;

/// "Never retreat" is 0; a ratio above this (bps) is an invalid plaintext
/// (I-27). Defined once, in `clash` (W1-A), and re-exported here.
pub use super::clash::RETREAT_MAX_BPS;

pub const DOMAIN_MARCH: &[u8] = b"PS-FRONTIER-MARCH-v1";
/// M3 postures (reserved).
pub const DOMAIN_POSTURE: &[u8] = b"PS-FRONTIER-POSTURE-v1";
pub const DOMAIN_SALT: &[u8] = b"PS-SALT";
pub const DOMAIN_KS: &[u8] = b"PS-KS";

/// Plaintext version of a march.
pub const PLAIN_VERSION: u8 = 1;
pub const PLAIN_LEN: usize = 37;
pub const PATH_BYTES: usize = 12;
pub const RESERVED_BYTES: usize = 3;
/// The seal key size (compact-16).
pub const K_LEN: usize = 16;
pub const SEAL_LEN: usize = 165;
/// Offsets of the compact-16 seal.
pub const SEAL_U: usize = 0;
pub const SEAL_V: usize = 96;
pub const SEAL_W: usize = 112;
pub const SEAL_BODY: usize = 128;

/// Stance byte values (`stance::Stance as u8`).
pub const STANCE_MAX: u8 = 3;

/// A march plaintext, field for field (`Default` is all zero, which is
/// not a valid plaintext: `version` must be set).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Plain {
    pub version: u8,
    pub host_id: u64,
    pub arrive_bell: u32,
    pub dest_p: i16,
    pub dest_q: i16,
    pub dest_tile: u8,
    pub stance: u8,
    pub retreat_bps: u16,
    pub path_len: u8,
    pub path: [u8; PATH_BYTES],
    /// Must be zero ([`validate`]); kept so `pack(unpack(b)) == b` for
    /// every 37-byte input.
    pub reserved: [u8; RESERVED_BYTES],
}

/// Why a plaintext is invalid (Reveal refuses `BadPlaintext`;
/// SettleTransit judges the seal bad with code 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlainError {
    Version,
    Reserved,
    HostMismatch,
    ArriveMismatch,
    PathTooLong,
    /// A bit above the last step is set.
    PathBits,
    /// A step direction is 6 or 7.
    Direction,
    Tile,
    Stance,
    Retreat,
}

/// Packs a plaintext into its 37 bytes.
pub fn pack(pt: &Plain) -> [u8; PLAIN_LEN] {
    let mut b = [0u8; PLAIN_LEN];
    b[0] = pt.version;
    b[1..9].copy_from_slice(&pt.host_id.to_le_bytes());
    b[9..13].copy_from_slice(&pt.arrive_bell.to_le_bytes());
    b[13..15].copy_from_slice(&pt.dest_p.to_le_bytes());
    b[15..17].copy_from_slice(&pt.dest_q.to_le_bytes());
    b[17] = pt.dest_tile;
    b[18] = pt.stance;
    b[19..21].copy_from_slice(&pt.retreat_bps.to_le_bytes());
    b[21] = pt.path_len;
    b[22..34].copy_from_slice(&pt.path);
    b[34..37].copy_from_slice(&pt.reserved);
    b
}

/// Unpacks 37 bytes (total: every input has a `Plain`; [`validate`]
/// decides whether it is a valid march).
pub fn unpack(b: &[u8; PLAIN_LEN]) -> Plain {
    let mut path = [0u8; PATH_BYTES];
    path.copy_from_slice(&b[22..34]);
    let mut reserved = [0u8; RESERVED_BYTES];
    reserved.copy_from_slice(&b[34..37]);
    Plain {
        version: b[0],
        host_id: u64::from_le_bytes([b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8]]),
        arrive_bell: u32::from_le_bytes([b[9], b[10], b[11], b[12]]),
        dest_p: i16::from_le_bytes([b[13], b[14]]),
        dest_q: i16::from_le_bytes([b[15], b[16]]),
        dest_tile: b[17],
        stance: b[18],
        retreat_bps: u16::from_le_bytes([b[19], b[20]]),
        path_len: b[21],
        path,
        reserved,
    }
}

/// Direction of path step `i` (0..32), 0..8 (only 0..6 are valid).
pub fn path_step(path: &[u8; PATH_BYTES], i: usize) -> u8 {
    let bit = 3 * i;
    let lo = path.get(bit / 8).copied().unwrap_or(0) as u16;
    let hi = path.get(bit / 8 + 1).copied().unwrap_or(0) as u16;
    (((hi << 8 | lo) >> (bit % 8)) & 7) as u8
}

/// Sets path step `i` (0..32) to `dir & 7`.
pub fn set_path_step(path: &mut [u8; PATH_BYTES], i: usize, dir: u8) {
    if i >= MAX_PATH_STEPS {
        return;
    }
    let bit = 3 * i;
    let (byte, shift) = (bit / 8, bit % 8);
    let v = ((dir & 7) as u16) << shift;
    let m = 7u16 << shift;
    path[byte] = (path[byte] & !(m as u8)) | v as u8;
    if byte + 1 < PATH_BYTES {
        path[byte + 1] = (path[byte + 1] & !((m >> 8) as u8)) | (v >> 8) as u8;
    }
}

/// Packs a list of directions (≤ 32) into the path field; `None` if too
/// long or a direction is ≥ 6.
pub fn encode_path(dirs: &[u8]) -> Option<(u8, [u8; PATH_BYTES])> {
    if dirs.len() > MAX_PATH_STEPS || dirs.iter().any(|&d| d as usize >= DIRECTIONS.len()) {
        return None;
    }
    let mut path = [0u8; PATH_BYTES];
    for (i, &d) in dirs.iter().enumerate() {
        set_path_step(&mut path, i, d);
    }
    Some((dirs.len() as u8, path))
}

/// Checks a plaintext against its transit (I-28): version 1, reserved
/// bytes zero, host and arrival bell equal to the transit's, `path_len ≤
/// 32`, no bit set above the last step, every step a real direction
/// (0..6), the tile inside the province (0..61), a known stance and
/// `retreat_bps ≤ 60,000`. The web client's self-audit runs the same
/// checks before sending a Depart.
pub fn validate(pt: &Plain, host_id: u64, arrive_bell: u32) -> Result<(), PlainError> {
    if pt.version != PLAIN_VERSION {
        return Err(PlainError::Version);
    }
    if pt.reserved != [0; RESERVED_BYTES] {
        return Err(PlainError::Reserved);
    }
    if pt.host_id != host_id {
        return Err(PlainError::HostMismatch);
    }
    if pt.arrive_bell != arrive_bell {
        return Err(PlainError::ArriveMismatch);
    }
    let n = pt.path_len as usize;
    if n > MAX_PATH_STEPS {
        return Err(PlainError::PathTooLong);
    }
    // Bits 3n..96 must be zero.
    let used = 3 * n;
    for (i, &byte) in pt.path.iter().enumerate() {
        let lo = 8 * i;
        let keep = if used >= lo + 8 {
            0xffu8
        } else if used <= lo {
            0
        } else {
            (1u8 << (used - lo)) - 1
        };
        if byte & !keep != 0 {
            return Err(PlainError::PathBits);
        }
    }
    if (0..n).any(|i| path_step(&pt.path, i) as usize >= DIRECTIONS.len()) {
        return Err(PlainError::Direction);
    }
    if pt.dest_tile as usize >= PROVINCE_TILES {
        return Err(PlainError::Tile);
    }
    if pt.stance > STANCE_MAX {
        return Err(PlainError::Stance);
    }
    if pt.retreat_bps > RETREAT_MAX_BPS {
        return Err(PlainError::Retreat);
    }
    Ok(())
}

/// `salt = sha256("PS-SALT" ‖ k)`.
pub fn salt_of(k: &[u8; K_LEN]) -> [u8; 32] {
    sha256(&[DOMAIN_SALT, k])
}

/// `commit = sha256("PS-FRONTIER-MARCH-v1" ‖ plain37 ‖ salt)`.
pub fn commit(pt: &[u8; PLAIN_LEN], salt: &[u8; 32]) -> [u8; 32] {
    sha256(&[DOMAIN_MARCH, pt, salt])
}

/// The same commitment under another domain (M3 postures use
/// [`DOMAIN_POSTURE`]).
pub fn commit_with(domain: &[u8], pt: &[u8; PLAIN_LEN], salt: &[u8; 32]) -> [u8; 32] {
    sha256(&[domain, pt, salt])
}

/// `ct_hash = sha256(seal)` (Reveal passes it; Depart computes it).
pub fn ct_hash(seal: &[u8; SEAL_LEN]) -> [u8; 32] {
    sha256(&[seal])
}

/// `seal_root = sha256(commit ‖ ct_hash)`.
pub fn seal_root(commit: &[u8; 32], ct_hash: &[u8; 32]) -> [u8; 32] {
    sha256(&[commit, ct_hash])
}

/// The 37-byte body XOR the keystream `sha256("PS-KS" ‖ k ‖ [c])`, `c =
/// 0, 1` (an involution: it seals and opens).
pub fn body_xor(k: &[u8; K_LEN], pt: &[u8; PLAIN_LEN]) -> [u8; PLAIN_LEN] {
    let mut out = *pt;
    let mut i = 0;
    let mut c = 0u8;
    while i < PLAIN_LEN {
        let blk = sha256(&[DOMAIN_KS, k, &[c]]);
        let n = (PLAIN_LEN - i).min(32);
        for j in 0..n {
            out[i + j] ^= blk[j];
        }
        i += 32;
        c += 1;
    }
    out
}

/// The body of a seal.
pub fn seal_body(seal: &[u8; SEAL_LEN]) -> [u8; PLAIN_LEN] {
    let mut b = [0u8; PLAIN_LEN];
    b.copy_from_slice(&seal[SEAL_BODY..]);
    b
}

/// Opens the body with a key `k` recovered from the IBE block and checks
/// the commitment: `Some(plain)` iff `commit(plain, salt_of(k)) ==
/// commit`. (The pairing and the FO check are the caller's.)
pub fn open_body(
    k: &[u8; K_LEN],
    seal: &[u8; SEAL_LEN],
    commitment: &[u8; 32],
) -> Option<[u8; PLAIN_LEN]> {
    let pt = body_xor(k, &seal_body(seal));
    (commit(&pt, &salt_of(k)) == *commitment).then_some(pt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Plain {
        let (n, path) = encode_path(&[0, 1, 2, 3, 4, 5, 0, 5]).unwrap();
        Plain {
            version: 1,
            host_id: 0x0123_4567_89ab_cdef,
            arrive_bell: 500,
            dest_p: -3,
            dest_q: 7,
            dest_tile: 60,
            stance: 2,
            retreat_bps: 0,
            path_len: n,
            path,
            reserved: [0; 3],
        }
    }

    #[test]
    fn pack_round_trips_every_byte() {
        let pt = sample();
        assert_eq!(unpack(&pack(&pt)), pt);
        let mut x = 0x9e37_79b9_7f4a_7c15u64;
        for _ in 0..10_000 {
            let mut b = [0u8; PLAIN_LEN];
            for v in b.iter_mut() {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *v = x as u8;
            }
            assert_eq!(pack(&unpack(&b)), b);
        }
    }

    #[test]
    fn path_steps_round_trip() {
        let dirs: [u8; 32] = core::array::from_fn(|i| (i * 5 % 6) as u8);
        let (n, path) = encode_path(&dirs).unwrap();
        assert_eq!(n, 32);
        for (i, &d) in dirs.iter().enumerate() {
            assert_eq!(path_step(&path, i), d);
        }
        assert!(encode_path(&[6]).is_none());
        assert!(encode_path(&[0; 33]).is_none());
    }

    #[test]
    fn validate_refuses_each_field() {
        let ok = sample();
        assert_eq!(validate(&ok, ok.host_id, 500), Ok(()));
        let e = |f: &dyn Fn(&mut Plain)| {
            let mut p = ok;
            f(&mut p);
            validate(&p, ok.host_id, 500)
        };
        assert_eq!(e(&|p| p.version = 2), Err(PlainError::Version));
        assert_eq!(e(&|p| p.reserved[2] = 1), Err(PlainError::Reserved));
        assert_eq!(e(&|p| p.host_id += 1), Err(PlainError::HostMismatch));
        assert_eq!(e(&|p| p.arrive_bell = 501), Err(PlainError::ArriveMismatch));
        assert_eq!(e(&|p| p.path_len = 33), Err(PlainError::PathTooLong));
        assert_eq!(e(&|p| p.path_len = 7), Err(PlainError::PathBits));
        assert_eq!(e(&|p| p.path[11] = 0x80), Err(PlainError::PathBits));
        assert_eq!(
            e(&|p| set_path_step(&mut p.path, 1, 7)),
            Err(PlainError::Direction)
        );
        assert_eq!(e(&|p| p.dest_tile = 61), Err(PlainError::Tile));
        assert_eq!(e(&|p| p.stance = 4), Err(PlainError::Stance));
        assert_eq!(e(&|p| p.retreat_bps = 60_001), Err(PlainError::Retreat));
        assert_eq!(e(&|p| p.retreat_bps = 60_000), Ok(()));
        assert_eq!(
            e(&|p| {
                p.path_len = 0;
                p.path = [0; 12]
            }),
            Ok(())
        );
    }

    #[test]
    fn body_xor_is_an_involution_and_opens() {
        let k = [0x42u8; 16];
        let pt = pack(&sample());
        let body = body_xor(&k, &pt);
        assert_ne!(body, pt);
        assert_eq!(body_xor(&k, &body), pt);
        let mut seal = [0u8; SEAL_LEN];
        seal[SEAL_BODY..].copy_from_slice(&body);
        let c = commit(&pt, &salt_of(&k));
        assert_eq!(open_body(&k, &seal, &c), Some(pt));
        assert_eq!(open_body(&[0u8; 16], &seal, &c), None);
    }
}
