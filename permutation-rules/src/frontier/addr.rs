//! With-seed address grammar (contract §4.1; closeout CL-21; conflict
//! I-02: SP-V2's `acct.rs` grammar byte for byte).
//!
//! Every Frontier program account except the Season is
//! `create_with_seed(season_pda, seed, program_id) = sha256(season_pda ‖
//! seed ‖ program_id)`. The seed is
//!
//! ```text
//! seed = tag (2 ASCII bytes) ‖ lowercase-hex(raw)
//! raw  = the key fields, little-endian, fixed width, ≤ 15 bytes
//! ```
//!
//! so every seed is at most 32 bytes (the System program's limit), each
//! kind has one fixed seed length, and two different keys (of the same or
//! of different kinds) never share a seed: the tags are distinct and the
//! hex of a fixed-width field is injective. Coordinates are **i32** in
//! seeds (accounts store them as i16), days and bells **u32**.
//!
//! This module is pure (no Solana types): the program, `frontier-abi`,
//! `fclient`, the verifier and the web client build seeds here and hash
//! them with [`with_seed_address`].
//!
//! | Kind | Tag | Raw key fields (LE) | Raw B | Seed B |
//! |---|---|---|---|---|
//! | Frontier | `fr` | — | 0 | 2 |
//! | RingSeed | `rs` | d u16 | 2 | 6 |
//! | ProvinceFund | `pf` | w u8 | 1 | 4 |
//! | JoinShard | `js` | faction u8, shard u8 | 2 | 6 |
//! | BeaconLog | `bl` | region u8 | 1 | 4 |
//! | DefencePool | `dp` | — | 0 | 2 |
//! | Citizen | `ct` | `sha256("PSF-CIT" ‖ wallet)[0..15]` | 15 | 32 |
//! | Holding | `ho` | P i32, Q i32, site u8 | 9 | 20 |
//! | Province | `pv` | P i32, Q i32 | 8 | 18 |
//! | ArrivalSlot | `ar` | P i32, Q i32, bell u32, faction u8, i u8 | 14 | 30 |
//! | ArrivalDay | `ad` | P i32, Q i32, day u32 | 12 | 26 |
//! | ClashInputs | `ci` | P i32, Q i32, bell u32 | 12 | 26 |
//! | SealVerdict (reserved, I-44) | `sv` | host_id u64, arrive_bell u32 | 12 | 26 |
//! | BellAnchor | `an` | bell u32, region u8 | 5 | 12 |
//! | SeedCache | `sd` | bell u32, region u8, nonce u8 | 6 | 14 |
//! | AnchorArchive | `aa` | region u8, day u32 | 5 | 12 |
//! | DefenceClaim | `dc` | `sha256("PSF-KPR" ‖ beneficiary)[0..8]`, day u32 | 12 | 26 |
//! | PosturePDA (M3, reserved) | `po` | P i32, Q i32, bell u32, pos u8 | 13 | 28 |
//!
//! **PosturePDA:** SP-V2's `posture_seed` has 13 raw bytes (28-byte seed);
//! the contract's §4.1 table lists 14 (a trailing zero byte, 30 B). I-02
//! pins SP-V2 byte for byte, so this module follows SP-V2 (recorded in the
//! W1-C notes as a contract erratum).
//!
//! **Host id (pinned):** `host_id = province_index(P, Q) << 44 | site << 40
//! | gen << 32 | seq` ([`host_id`], inverse [`host_parts`]).

use super::geometry::{ProvinceCoord, SITES_PER_PROVINCE};
use crate::hash::sha256;

/// Kernel version of this module (part of the ruleset hash).
pub const ADDR_VERSION: u16 = 1;

/// Longest seed the System program accepts.
pub const MAX_SEED_LEN: usize = 32;
/// Longest raw key.
pub const MAX_RAW_LEN: usize = 15;

/// Every with-seed account kind (and the reserved tags).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SeedKind {
    Frontier,
    RingSeed,
    ProvinceFund,
    JoinShard,
    BeaconLog,
    DefencePool,
    Citizen,
    Holding,
    Province,
    ArrivalSlot,
    ArrivalDay,
    ClashInputs,
    /// Removed in v1.1 (I-44); the tag stays reserved.
    SealVerdict,
    BellAnchor,
    SeedCache,
    AnchorArchive,
    DefenceClaim,
    /// M3; reserved.
    Posture,
}

impl SeedKind {
    pub const ALL: [SeedKind; 18] = [
        SeedKind::Frontier,
        SeedKind::RingSeed,
        SeedKind::ProvinceFund,
        SeedKind::JoinShard,
        SeedKind::BeaconLog,
        SeedKind::DefencePool,
        SeedKind::Citizen,
        SeedKind::Holding,
        SeedKind::Province,
        SeedKind::ArrivalSlot,
        SeedKind::ArrivalDay,
        SeedKind::ClashInputs,
        SeedKind::SealVerdict,
        SeedKind::BellAnchor,
        SeedKind::SeedCache,
        SeedKind::AnchorArchive,
        SeedKind::DefenceClaim,
        SeedKind::Posture,
    ];

    /// The two-letter tag.
    pub const fn tag(self) -> &'static [u8; 2] {
        match self {
            SeedKind::Frontier => b"fr",
            SeedKind::RingSeed => b"rs",
            SeedKind::ProvinceFund => b"pf",
            SeedKind::JoinShard => b"js",
            SeedKind::BeaconLog => b"bl",
            SeedKind::DefencePool => b"dp",
            SeedKind::Citizen => b"ct",
            SeedKind::Holding => b"ho",
            SeedKind::Province => b"pv",
            SeedKind::ArrivalSlot => b"ar",
            SeedKind::ArrivalDay => b"ad",
            SeedKind::ClashInputs => b"ci",
            SeedKind::SealVerdict => b"sv",
            SeedKind::BellAnchor => b"an",
            SeedKind::SeedCache => b"sd",
            SeedKind::AnchorArchive => b"aa",
            SeedKind::DefenceClaim => b"dc",
            SeedKind::Posture => b"po",
        }
    }

    /// The fixed raw key width of the kind, in bytes.
    pub const fn raw_len(self) -> usize {
        match self {
            SeedKind::Frontier | SeedKind::DefencePool => 0,
            SeedKind::ProvinceFund | SeedKind::BeaconLog => 1,
            SeedKind::RingSeed | SeedKind::JoinShard => 2,
            SeedKind::BellAnchor | SeedKind::AnchorArchive => 5,
            SeedKind::SeedCache => 6,
            SeedKind::Province => 8,
            SeedKind::Holding => 9,
            SeedKind::ArrivalDay
            | SeedKind::ClashInputs
            | SeedKind::SealVerdict
            | SeedKind::DefenceClaim => 12,
            SeedKind::Posture => 13,
            SeedKind::ArrivalSlot => 14,
            SeedKind::Citizen => 15,
        }
    }

    /// The fixed seed length of the kind: `2 + 2 × raw_len`.
    pub const fn seed_len(self) -> usize {
        2 + 2 * self.raw_len()
    }

    /// The kind a tag names, if any.
    pub fn from_tag(tag: &[u8]) -> Option<SeedKind> {
        SeedKind::ALL.into_iter().find(|k| k.tag() == tag)
    }
}

/// A seed string (`tag ‖ hex(raw)`), at most 32 bytes, stored inline.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SeedStr {
    buf: [u8; MAX_SEED_LEN],
    len: u8,
}

impl core::fmt::Debug for SeedStr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Seeds are ASCII by construction.
        match core::str::from_utf8(self.as_bytes()) {
            Ok(s) => write!(f, "SeedStr({s:?})"),
            Err(_) => write!(f, "SeedStr({:?})", self.as_bytes()),
        }
    }
}

impl SeedStr {
    /// `tag ‖ lowercase-hex(raw)`; only the first 15 raw bytes are used
    /// (the typed constructors never pass more).
    pub fn new(tag: &[u8; 2], raw: &[u8]) -> SeedStr {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut buf = [0u8; MAX_SEED_LEN];
        buf[..2].copy_from_slice(tag);
        let mut n = 2;
        for &b in raw.iter().take(MAX_RAW_LEN) {
            buf[n] = HEX[(b >> 4) as usize];
            buf[n + 1] = HEX[(b & 15) as usize];
            n += 2;
        }
        SeedStr { buf, len: n as u8 }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len as usize]
    }

    pub const fn len(&self) -> usize {
        self.len as usize
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// `(buffer, length)`, the pinned return shape of [`seed`].
    pub const fn into_parts(self) -> ([u8; MAX_SEED_LEN], usize) {
        (self.buf, self.len as usize)
    }
}

/// The seed of `kind` for the raw key `raw` (pinned signature). A raw key
/// longer than 15 bytes is truncated; callers use the typed constructors.
pub fn seed(kind: SeedKind, raw: &[u8]) -> ([u8; 32], usize) {
    SeedStr::new(kind.tag(), raw).into_parts()
}

/// Parses a seed back into its kind and raw key; `None` unless it is a
/// well-formed seed of a known kind at that kind's fixed length.
pub fn parse_seed(s: &[u8]) -> Option<(SeedKind, [u8; MAX_RAW_LEN], usize)> {
    if s.len() < 2 {
        return None;
    }
    let kind = SeedKind::from_tag(&s[..2])?;
    if s.len() != kind.seed_len() {
        return None;
    }
    let nib = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    };
    let mut raw = [0u8; MAX_RAW_LEN];
    for i in 0..kind.raw_len() {
        raw[i] = nib(s[2 + 2 * i])? << 4 | nib(s[3 + 2 * i])?;
    }
    Some((kind, raw, kind.raw_len()))
}

/// `create_with_seed(base, seed, owner) = sha256(base ‖ seed ‖ owner)`
/// (the System program's derivation; no curve check).
pub fn with_seed_address(base: &[u8; 32], seed: &[u8], owner: &[u8; 32]) -> [u8; 32] {
    sha256(&[base, seed, owner])
}

// ------------------------------------------------------------ tags

/// The Citizen's raw key: `sha256("PSF-CIT" ‖ wallet)[0..15]`.
pub fn citizen_tag15(wallet: &[u8; 32]) -> [u8; 15] {
    let h = sha256(&[b"PSF-CIT", wallet]);
    let mut o = [0u8; 15];
    o.copy_from_slice(&h[..15]);
    o
}

/// The DefenceClaim keeper key: `sha256("PSF-KPR" ‖ beneficiary)[0..8]`.
pub fn keeper_tag8(b: &[u8; 32]) -> [u8; 8] {
    let h = sha256(&[b"PSF-KPR", b]);
    let mut o = [0u8; 8];
    o.copy_from_slice(&h[..8]);
    o
}

/// The quota's citizen id: the first 8 bytes of the Citizen *address*,
/// little-endian.
pub fn citizen_tag(citizen_address: &[u8; 32]) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&citizen_address[..8]);
    u64::from_le_bytes(b)
}

// ------------------------------------------------------------ constructors

pub fn frontier() -> SeedStr {
    SeedStr::new(SeedKind::Frontier.tag(), &[])
}

pub fn defence_pool() -> SeedStr {
    SeedStr::new(SeedKind::DefencePool.tag(), &[])
}

pub fn ring_seed(d: u16) -> SeedStr {
    SeedStr::new(SeedKind::RingSeed.tag(), &d.to_le_bytes())
}

pub fn province_fund(wedge: u8) -> SeedStr {
    SeedStr::new(SeedKind::ProvinceFund.tag(), &[wedge])
}

pub fn join_shard(faction: u8, shard: u8) -> SeedStr {
    SeedStr::new(SeedKind::JoinShard.tag(), &[faction, shard])
}

pub fn beacon_log(region: u8) -> SeedStr {
    SeedStr::new(SeedKind::BeaconLog.tag(), &[region])
}

/// The Citizen seed of a wallet.
pub fn citizen(wallet: &[u8; 32]) -> SeedStr {
    SeedStr::new(SeedKind::Citizen.tag(), &citizen_tag15(wallet))
}

pub fn holding(p: i32, q: i32, site: u8) -> SeedStr {
    let mut raw = [0u8; 9];
    raw[..4].copy_from_slice(&p.to_le_bytes());
    raw[4..8].copy_from_slice(&q.to_le_bytes());
    raw[8] = site;
    SeedStr::new(SeedKind::Holding.tag(), &raw)
}

pub fn province(p: i32, q: i32) -> SeedStr {
    let mut raw = [0u8; 8];
    raw[..4].copy_from_slice(&p.to_le_bytes());
    raw[4..8].copy_from_slice(&q.to_le_bytes());
    SeedStr::new(SeedKind::Province.tag(), &raw)
}

/// `P, Q, bell` then up to two single bytes, `n` raw bytes in all (SP-V2
/// `slot_seed`).
fn pqb(kind: SeedKind, p: i32, q: i32, b: u32, x: u8, y: u8) -> SeedStr {
    let mut raw = [0u8; 14];
    raw[..4].copy_from_slice(&p.to_le_bytes());
    raw[4..8].copy_from_slice(&q.to_le_bytes());
    raw[8..12].copy_from_slice(&b.to_le_bytes());
    raw[12] = x;
    raw[13] = y;
    SeedStr::new(kind.tag(), &raw[..kind.raw_len()])
}

/// ArrivalSlot `(P, Q, bell, faction, i)`.
pub fn slot(p: i32, q: i32, bell: u32, faction: u8, i: u8) -> SeedStr {
    pqb(SeedKind::ArrivalSlot, p, q, bell, faction, i)
}

/// ArrivalDay `(P, Q, day)`.
pub fn arrival_day(p: i32, q: i32, day: u32) -> SeedStr {
    pqb(SeedKind::ArrivalDay, p, q, day, 0, 0)
}

/// ClashInputs `(P, Q, bell)`.
pub fn clash_inputs(p: i32, q: i32, bell: u32) -> SeedStr {
    pqb(SeedKind::ClashInputs, p, q, bell, 0, 0)
}

/// PosturePDA `(P, Q, bell, pos)` (M3, reserved; SP-V2's 13-byte form).
pub fn posture(p: i32, q: i32, bell: u32, pos: u8) -> SeedStr {
    pqb(SeedKind::Posture, p, q, bell, pos, 0)
}

/// SealVerdict `(host_id, arrive_bell)` — removed in v1.1 (I-44); kept so
/// the reserved seed is pinned and never reused.
pub fn seal_verdict_reserved(host: u64, bell: u32) -> SeedStr {
    let mut raw = [0u8; 12];
    raw[..8].copy_from_slice(&host.to_le_bytes());
    raw[8..].copy_from_slice(&bell.to_le_bytes());
    SeedStr::new(SeedKind::SealVerdict.tag(), &raw)
}

/// BellAnchor `(bell, region)`.
pub fn anchor(bell: u32, region: u8) -> SeedStr {
    let mut raw = [0u8; 5];
    raw[..4].copy_from_slice(&bell.to_le_bytes());
    raw[4] = region;
    SeedStr::new(SeedKind::BellAnchor.tag(), &raw)
}

/// SeedCache `(bell, region, nonce)`.
pub fn seed_cache(bell: u32, region: u8, nonce: u8) -> SeedStr {
    let mut raw = [0u8; 6];
    raw[..4].copy_from_slice(&bell.to_le_bytes());
    raw[4] = region;
    raw[5] = nonce;
    SeedStr::new(SeedKind::SeedCache.tag(), &raw)
}

/// AnchorArchive `(region, day)` (region first, as SP-V2).
pub fn anchor_archive(region: u8, day: u32) -> SeedStr {
    let mut raw = [0u8; 5];
    raw[0] = region;
    raw[1..].copy_from_slice(&day.to_le_bytes());
    SeedStr::new(SeedKind::AnchorArchive.tag(), &raw)
}

/// DefenceClaim `(keeper_tag8(beneficiary), day)`.
pub fn defence_claim(beneficiary: &[u8; 32], day: u32) -> SeedStr {
    let mut raw = [0u8; 12];
    raw[..8].copy_from_slice(&keeper_tag8(beneficiary));
    raw[8..].copy_from_slice(&day.to_le_bytes());
    SeedStr::new(SeedKind::DefenceClaim.tag(), &raw)
}

// ------------------------------------------------------------ host ids

/// Bits of a host id: province index (20) | site (4) | gen (8) | seq (32).
pub const HOST_SEQ_BITS: u32 = 32;
pub const HOST_GEN_SHIFT: u32 = 32;
pub const HOST_SITE_SHIFT: u32 = 40;
pub const HOST_PROVINCE_SHIFT: u32 = 44;
/// The id [`host_id`] returns for coordinates or a site out of range; no
/// real host has it ([`host_parts`] refuses it).
pub const HOST_ID_INVALID: u64 = u64::MAX;

/// The fields a host id packs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostParts {
    pub province_index: u32,
    pub p: i32,
    pub q: i32,
    pub site: u8,
    pub gen: u8,
    pub seq: u32,
}

/// `host_id = province_index(P, Q) << 44 | site << 40 | gen << 32 | seq`
/// (pinned). Coordinates beyond ring `R_MAX_HARD` (128) or a site ≥ 12
/// give [`HOST_ID_INVALID`].
pub fn host_id(p: i32, q: i32, site: u8, gen: u8, seq: u32) -> u64 {
    if site as usize >= SITES_PER_PROVINCE {
        return HOST_ID_INVALID;
    }
    let Some(idx) = ProvinceCoord::new(p, q).checked_index() else {
        return HOST_ID_INVALID;
    };
    let idx = idx as u64;
    idx << HOST_PROVINCE_SHIFT
        | (site as u64) << HOST_SITE_SHIFT
        | (gen as u64) << HOST_GEN_SHIFT
        | seq as u64
}

/// Inverse of [`host_id`]; `None` for an index beyond ring 128 or a site
/// ≥ 12 (so [`HOST_ID_INVALID`] never decodes).
pub fn host_parts(id: u64) -> Option<HostParts> {
    let province_index = (id >> HOST_PROVINCE_SHIFT) as u32;
    let site = ((id >> HOST_SITE_SHIFT) & 0xf) as u8;
    let gen = ((id >> HOST_GEN_SHIFT) & 0xff) as u8;
    let seq = id as u32;
    if site as usize >= SITES_PER_PROVINCE {
        return None;
    }
    let c = ProvinceCoord::checked_from_index(province_index)?;
    Some(HostParts {
        province_index,
        p: c.p,
        q: c.q,
        site,
        gen,
        seq,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_its_fixed_length() {
        let w = [7u8; 32];
        let cases: [(SeedKind, SeedStr); 18] = [
            (SeedKind::Frontier, frontier()),
            (SeedKind::RingSeed, ring_seed(3)),
            (SeedKind::ProvinceFund, province_fund(5)),
            (SeedKind::JoinShard, join_shard(5, 7)),
            (SeedKind::BeaconLog, beacon_log(15)),
            (SeedKind::DefencePool, defence_pool()),
            (SeedKind::Citizen, citizen(&w)),
            (SeedKind::Holding, holding(-1, 2, 11)),
            (SeedKind::Province, province(-1, 2)),
            (SeedKind::ArrivalSlot, slot(-1, 2, 3, 4, 5)),
            (SeedKind::ArrivalDay, arrival_day(-1, 2, 3)),
            (SeedKind::ClashInputs, clash_inputs(-1, 2, 3)),
            (SeedKind::SealVerdict, seal_verdict_reserved(u64::MAX, 7)),
            (SeedKind::BellAnchor, anchor(9, 15)),
            (SeedKind::SeedCache, seed_cache(9, 15, 255)),
            (SeedKind::AnchorArchive, anchor_archive(15, 9)),
            (SeedKind::DefenceClaim, defence_claim(&w, 9)),
            (SeedKind::Posture, posture(1, 2, 3, 59)),
        ];
        for (k, s) in cases {
            assert_eq!(s.len(), k.seed_len(), "{k:?}");
            assert!(s.len() <= MAX_SEED_LEN);
            assert_eq!(&s.as_bytes()[..2], k.tag());
            let (kk, _, n) = parse_seed(s.as_bytes()).unwrap();
            assert_eq!((kk, n), (k, k.raw_len()));
        }
        assert_eq!(seed_cache(9, 15, 255).as_bytes(), b"sd090000000fff");
    }

    #[test]
    fn tags_are_distinct() {
        for (i, a) in SeedKind::ALL.iter().enumerate() {
            for b in &SeedKind::ALL[i + 1..] {
                assert_ne!(a.tag(), b.tag());
            }
        }
    }

    #[test]
    fn host_id_round_trips() {
        for (p, q) in [
            (0, 0),
            (1, 0),
            (-128, 64),
            (128, -128),
            (0, 128),
            (-64, -64),
        ] {
            for site in [0u8, 5, 11] {
                for (gen, seq) in [(0u8, 0u32), (255, u32::MAX), (7, 12_345)] {
                    let id = host_id(p, q, site, gen, seq);
                    assert_ne!(id, HOST_ID_INVALID);
                    let h = host_parts(id).unwrap();
                    assert_eq!((h.p, h.q, h.site, h.gen, h.seq), (p, q, site, gen, seq));
                }
            }
        }
        assert_eq!(host_id(129, 0, 0, 0, 0), HOST_ID_INVALID);
        assert_eq!(host_id(i32::MIN, i32::MAX, 0, 0, 0), HOST_ID_INVALID);
        assert_eq!(host_id(0, 0, 12, 0, 0), HOST_ID_INVALID);
        assert_eq!(host_parts(HOST_ID_INVALID), None);
        // The pinned layout: index < 2^20 for R ≤ 128.
        use crate::frontier::geometry::{provinces_within, R_MAX_HARD};
        assert!(provinces_within(R_MAX_HARD as u32) < 1 << 20);
    }
}
