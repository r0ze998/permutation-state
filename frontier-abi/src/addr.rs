//! Addresses (M1 contract §4.1, I-01, I-02).
//!
//! `addr(kind, key) = create_with_seed(season_pda, seed, program_id)
//!                  = sha256(season_pda ‖ seed ‖ program_id)`,
//! `seed = tag (2 ASCII) ‖ lowercase-hex(raw)`, `raw` = the key fields,
//! little-endian, fixed width, at most 15 bytes (so a seed is ≤ 32 bytes,
//! the System program's limit). Coordinates are **i32** in seeds (accounts
//! store P, Q as i16), days are **u32**. The Season is the only PDA,
//! `["season", le64(id)]`; its bump is found once by AnnounceSeason and
//! stored, so this crate never needs the curve check: callers pass the
//! Season PDA in.
//!
//! This is SP-V2 `acct.rs`'s grammar byte for byte (checked against a
//! verbatim copy of its seed builder in `tests/addresses.rs`).
//!
//! **Interim home (wave 1).** The contract names the kernel module
//! `permutation_rules::frontier::addr` (W1-C, CL-21) as the owner of the
//! seed strings. W1-E and W1-C are built in parallel from the same base, so
//! this module implements the pinned grammar itself; after W1-C merges, the
//! integrator either re-exports the kernel's builders here or keeps both
//! behind the equality test requested in `docs/frontier/m1/W1-E-NOTES.md`.

use crate::layout::AccountKind;
use permutation_rules::frontier::geometry::{provinces_within, ProvinceCoord, R_MAX_HARD};
use permutation_rules::hash::sha256;

/// Two-letter seed tags (§4.1).
pub mod tag {
    pub const FRONTIER: [u8; 2] = *b"fr";
    pub const RING_SEED: [u8; 2] = *b"rs";
    pub const PROVINCE_FUND: [u8; 2] = *b"pf";
    pub const JOIN_SHARD: [u8; 2] = *b"js";
    pub const BEACON_LOG: [u8; 2] = *b"bl";
    pub const DEFENCE_POOL: [u8; 2] = *b"dp";
    pub const CITIZEN: [u8; 2] = *b"ct";
    pub const HOLDING: [u8; 2] = *b"ho";
    pub const PROVINCE: [u8; 2] = *b"pv";
    pub const ARRIVAL_SLOT: [u8; 2] = *b"ar";
    pub const ARRIVAL_DAY: [u8; 2] = *b"ad";
    pub const CLASH_INPUTS: [u8; 2] = *b"ci";
    /// Reserved: SealVerdict was removed in v1.1 (I-44).
    pub const SEAL_VERDICT: [u8; 2] = *b"sv";
    pub const BELL_ANCHOR: [u8; 2] = *b"an";
    pub const SEED_CACHE: [u8; 2] = *b"sd";
    pub const ANCHOR_ARCHIVE: [u8; 2] = *b"aa";
    pub const DEFENCE_CLAIM: [u8; 2] = *b"dc";
    /// Reserved: PosturePDA (M3).
    pub const POSTURE: [u8; 2] = *b"po";
}

/// Most raw key bytes in a seed.
pub const MAX_RAW: usize = 15;
/// Most seed bytes (System `create_with_seed` limit).
pub const MAX_SEED: usize = 32;

/// A with-seed seed string `tag ‖ hex(raw)`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Seed {
    buf: [u8; MAX_SEED],
    len: u8,
}

impl core::fmt::Debug for Seed {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match core::str::from_utf8(self.as_bytes()) {
            Ok(s) => write!(f, "Seed({s})"),
            Err(_) => write!(f, "Seed({:?})", self.as_bytes()),
        }
    }
}

impl Seed {
    /// `tag ‖ lowercase-hex(raw)`; `raw` longer than 15 bytes is truncated to
    /// 15 (as SP-V2; every constructor below passes ≤ 15).
    pub fn new(tag: [u8; 2], raw: &[u8]) -> Seed {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut buf = [0u8; MAX_SEED];
        buf[0] = tag[0];
        buf[1] = tag[1];
        let mut n = 2;
        for &b in raw.iter().take(MAX_RAW) {
            buf[n] = HEX[(b >> 4) as usize];
            buf[n + 1] = HEX[(b & 15) as usize];
            n += 2;
        }
        Seed { buf, len: n as u8 }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len as usize]
    }

    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Parses a seed back into its tag and raw bytes (vectors, verifier).
    pub fn parse(s: &[u8]) -> Option<([u8; 2], [u8; MAX_RAW], usize)> {
        if s.len() < 2 || s.len() > MAX_SEED || s.len() % 2 != 0 {
            return None;
        }
        let tag = [s[0], s[1]];
        let mut raw = [0u8; MAX_RAW];
        let n = (s.len() - 2) / 2;
        for (i, pair) in s[2..].chunks(2).enumerate() {
            let hi = hexval(pair[0])?;
            let lo = hexval(pair[1])?;
            raw[i] = (hi << 4) | lo;
        }
        Some((tag, raw, n))
    }
}

fn hexval(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

/// `create_with_seed(base, seed, owner) = sha256(base ‖ seed ‖ owner)`.
pub fn with_seed(base: &[u8; 32], seed: &Seed, owner: &[u8; 32]) -> [u8; 32] {
    sha256(&[base, seed.as_bytes(), owner])
}

// ------------------------------------------------------------ seeds

fn raw_pq(p: i32, q: i32) -> [u8; 8] {
    let mut r = [0u8; 8];
    r[..4].copy_from_slice(&p.to_le_bytes());
    r[4..].copy_from_slice(&q.to_le_bytes());
    r
}

pub fn frontier_seed() -> Seed {
    Seed::new(tag::FRONTIER, &[])
}
pub fn ring_seed_seed(d: u16) -> Seed {
    Seed::new(tag::RING_SEED, &d.to_le_bytes())
}
pub fn province_fund_seed(wedge: u8) -> Seed {
    Seed::new(tag::PROVINCE_FUND, &[wedge])
}
pub fn join_shard_seed(faction: u8, shard: u8) -> Seed {
    Seed::new(tag::JOIN_SHARD, &[faction, shard])
}
pub fn beacon_log_seed(region: u8) -> Seed {
    Seed::new(tag::BEACON_LOG, &[region])
}
pub fn defence_pool_seed() -> Seed {
    Seed::new(tag::DEFENCE_POOL, &[])
}
/// Citizen seed from its 15-byte tag ([`citizen_tag15`]).
pub fn citizen_seed(tag15: &[u8; 15]) -> Seed {
    Seed::new(tag::CITIZEN, tag15)
}
pub fn holding_seed(p: i32, q: i32, site: u8) -> Seed {
    let mut r = [0u8; 9];
    r[..8].copy_from_slice(&raw_pq(p, q));
    r[8] = site;
    Seed::new(tag::HOLDING, &r)
}
pub fn province_seed(p: i32, q: i32) -> Seed {
    Seed::new(tag::PROVINCE, &raw_pq(p, q))
}
/// SP-V2 `slot_seed`: `P i32 ‖ Q i32 ‖ bell u32 ‖ x ‖ y`, first `n` bytes.
fn slot_raw(t: [u8; 2], p: i32, q: i32, bell: u32, x: u8, y: u8, n: usize) -> Seed {
    let mut r = [0u8; 14];
    r[..8].copy_from_slice(&raw_pq(p, q));
    r[8..12].copy_from_slice(&bell.to_le_bytes());
    r[12] = x;
    r[13] = y;
    Seed::new(t, &r[..n])
}
pub fn arrival_slot_seed(p: i32, q: i32, bell: u32, faction: u8, i: u8) -> Seed {
    slot_raw(tag::ARRIVAL_SLOT, p, q, bell, faction, i, 14)
}
pub fn arrival_day_seed(p: i32, q: i32, day: u32) -> Seed {
    slot_raw(tag::ARRIVAL_DAY, p, q, day, 0, 0, 12)
}
pub fn clash_inputs_seed(p: i32, q: i32, bell: u32) -> Seed {
    slot_raw(tag::CLASH_INPUTS, p, q, bell, 0, 0, 12)
}
/// Reserved (M3). SP-V2 grammar: 13 raw bytes `P, Q, bell, pos` (see the
/// W1-E notes: the contract table's "pos u8, 0 u8 → 14 B" disagrees with
/// SP-V2 `acct.rs`, which §4.1 says the vectors must match byte for byte).
pub fn posture_seed(p: i32, q: i32, bell: u32, pos: u8) -> Seed {
    slot_raw(tag::POSTURE, p, q, bell, pos, 0, 13)
}
/// Reserved: SealVerdict removed in v1.1 (I-44).
pub fn seal_verdict_seed(host_id: u64, arrive_bell: u32) -> Seed {
    let mut r = [0u8; 12];
    r[..8].copy_from_slice(&host_id.to_le_bytes());
    r[8..].copy_from_slice(&arrive_bell.to_le_bytes());
    Seed::new(tag::SEAL_VERDICT, &r)
}
pub fn bell_anchor_seed(bell: u32, region: u8) -> Seed {
    let mut r = [0u8; 5];
    r[..4].copy_from_slice(&bell.to_le_bytes());
    r[4] = region;
    Seed::new(tag::BELL_ANCHOR, &r)
}
pub fn seed_cache_seed(bell: u32, region: u8, nonce: u8) -> Seed {
    let mut r = [0u8; 6];
    r[..4].copy_from_slice(&bell.to_le_bytes());
    r[4] = region;
    r[5] = nonce;
    Seed::new(tag::SEED_CACHE, &r)
}
pub fn anchor_archive_seed(region: u8, day: u32) -> Seed {
    let mut r = [0u8; 5];
    r[0] = region;
    r[1..].copy_from_slice(&day.to_le_bytes());
    Seed::new(tag::ANCHOR_ARCHIVE, &r)
}
/// DefenceClaim seed from the keeper tag ([`keeper_tag8`]).
pub fn defence_claim_seed(keeper_tag: &[u8; 8], day: u32) -> Seed {
    let mut r = [0u8; 12];
    r[..8].copy_from_slice(keeper_tag);
    r[8..].copy_from_slice(&day.to_le_bytes());
    Seed::new(tag::DEFENCE_CLAIM, &r)
}

// ------------------------------------------------------------ tags

/// `sha256("PSF-CIT" ‖ wallet)[0..15]`: the Citizen seed key.
pub fn citizen_tag15(wallet: &[u8; 32]) -> [u8; 15] {
    let h = sha256(&[crate::layout::player::citizen::TAG_DOMAIN, wallet]);
    let mut t = [0u8; 15];
    t.copy_from_slice(&h[..15]);
    t
}

/// `sha256("PSF-KPR" ‖ beneficiary)[0..8]`: the DefenceClaim seed key.
pub fn keeper_tag8(beneficiary: &[u8; 32]) -> [u8; 8] {
    let h = sha256(&[b"PSF-KPR", beneficiary]);
    let mut t = [0u8; 8];
    t.copy_from_slice(&h[..8]);
    t
}

/// The quota's citizen id: the first 8 bytes of the Citizen **address**,
/// little-endian (§4.1).
pub fn citizen_tag(citizen_address: &[u8; 32]) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&citizen_address[..8]);
    u64::from_le_bytes(b)
}

/// JoinShard of a wallet: `sha256(wallet)[0] mod 8` (§5.9 Join).
pub fn join_shard_of(wallet: &[u8; 32]) -> u8 {
    sha256(&[wallet])[0] % crate::layout::world::join_shard::SHARDS_PER_FACTION
}

/// `day(b) = b / 144`.
pub const fn day_of(bell: u32) -> u32 {
    bell / crate::layout::clash::BELLS_PER_DAY
}

// ------------------------------------------------------------ host ids

/// Host id (§4.1, pinned):
/// `province_index(P,Q) << 44 | site << 40 | gen << 32 | seq`.
/// `None` if the province is outside ring 128 or `site ≥ 12`.
pub fn host_id(p: i32, q: i32, site: u8, gen: u8, seq: u32) -> Option<u64> {
    let idx = province_index(p, q)?;
    if site as usize >= crate::layout::province::province::SITES_N {
        return None;
    }
    Some(((idx as u64) << 44) | ((site as u64) << 40) | ((gen as u64) << 32) | seq as u64)
}

/// The parts of a host id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostParts {
    pub province: ProvinceCoord,
    pub site: u8,
    pub gen: u8,
    pub seq: u32,
}

/// Inverse of [`host_id`]; `None` for an id no valid holding can issue.
pub fn split_host_id(id: u64) -> Option<HostParts> {
    let idx = (id >> 44) as u32;
    let site = ((id >> 40) & 0xF) as u8;
    let gen = ((id >> 32) & 0xFF) as u8;
    let seq = id as u32;
    if idx >= provinces_within(R_MAX_HARD as u32) || site as usize >= 12 {
        return None;
    }
    Some(HostParts {
        province: ProvinceCoord::from_index(idx),
        site,
        gen,
        seq,
    })
}

/// The id of the holding (and the generation of its site) that issued a
/// host: the host id with the sequence number cleared. Used as the kernel
/// `Host.owner`.
pub const fn holding_key_of_host(id: u64) -> u64 {
    id & !0xFFFF_FFFF
}

/// Dense province index of (P, Q) within ring 128, computed without
/// overflow for any i32 pair.
pub fn province_index(p: i32, q: i32) -> Option<u32> {
    // ring = hex distance from the origin, in i64 (CL-04 style).
    let (p64, q64) = (p as i64, q as i64);
    let ring = (p64.abs() + q64.abs() + (p64 + q64).abs()) / 2;
    if ring > R_MAX_HARD as i64 {
        return None;
    }
    Some(ProvinceCoord::new(p, q).index())
}

/// Ring of (P, Q) in i64 (no overflow).
pub fn ring_of(p: i32, q: i32) -> u64 {
    let (p64, q64) = (p as i64, q as i64);
    ((p64.abs() + q64.abs() + (p64 + q64).abs()) / 2) as u64
}

// ------------------------------------------------------------ full addresses

/// The two constants every Frontier address depends on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddrCtx {
    /// The Season PDA `["season", le64(id)]`.
    pub season: [u8; 32],
    pub program: [u8; 32],
}

impl AddrCtx {
    pub fn of(&self, seed: &Seed) -> [u8; 32] {
        with_seed(&self.season, seed, &self.program)
    }
    pub fn frontier(&self) -> [u8; 32] {
        self.of(&frontier_seed())
    }
    pub fn ring_seed(&self, d: u16) -> [u8; 32] {
        self.of(&ring_seed_seed(d))
    }
    pub fn province_fund(&self, wedge: u8) -> [u8; 32] {
        self.of(&province_fund_seed(wedge))
    }
    pub fn join_shard(&self, faction: u8, shard: u8) -> [u8; 32] {
        self.of(&join_shard_seed(faction, shard))
    }
    pub fn beacon_log(&self, region: u8) -> [u8; 32] {
        self.of(&beacon_log_seed(region))
    }
    pub fn defence_pool(&self) -> [u8; 32] {
        self.of(&defence_pool_seed())
    }
    pub fn citizen(&self, wallet: &[u8; 32]) -> [u8; 32] {
        self.of(&citizen_seed(&citizen_tag15(wallet)))
    }
    pub fn citizen_by_tag15(&self, tag15: &[u8; 15]) -> [u8; 32] {
        self.of(&citizen_seed(tag15))
    }
    pub fn holding(&self, p: i32, q: i32, site: u8) -> [u8; 32] {
        self.of(&holding_seed(p, q, site))
    }
    /// The Holding that issued `host_id` (`None` for an invalid id).
    pub fn holding_of_host(&self, host_id: u64) -> Option<[u8; 32]> {
        let h = split_host_id(host_id)?;
        Some(self.holding(h.province.p, h.province.q, h.site))
    }
    pub fn province(&self, p: i32, q: i32) -> [u8; 32] {
        self.of(&province_seed(p, q))
    }
    pub fn arrival_slot(&self, p: i32, q: i32, bell: u32, faction: u8, i: u8) -> [u8; 32] {
        self.of(&arrival_slot_seed(p, q, bell, faction, i))
    }
    pub fn arrival_day(&self, p: i32, q: i32, day: u32) -> [u8; 32] {
        self.of(&arrival_day_seed(p, q, day))
    }
    pub fn clash_inputs(&self, p: i32, q: i32, bell: u32) -> [u8; 32] {
        self.of(&clash_inputs_seed(p, q, bell))
    }
    pub fn bell_anchor(&self, bell: u32, region: u8) -> [u8; 32] {
        self.of(&bell_anchor_seed(bell, region))
    }
    pub fn seed_cache(&self, bell: u32, region: u8, nonce: u8) -> [u8; 32] {
        self.of(&seed_cache_seed(bell, region, nonce))
    }
    pub fn anchor_archive(&self, region: u8, day: u32) -> [u8; 32] {
        self.of(&anchor_archive_seed(region, day))
    }
    pub fn defence_claim(&self, beneficiary: &[u8; 32], day: u32) -> [u8; 32] {
        self.of(&defence_claim_seed(&keeper_tag8(beneficiary), day))
    }
}

/// The seed tag of each account kind (`None` for the Season PDA).
pub const fn tag_of(kind: AccountKind) -> Option<[u8; 2]> {
    use AccountKind::*;
    Some(match kind {
        Season => return None,
        Frontier => tag::FRONTIER,
        RingSeed => tag::RING_SEED,
        ProvinceFund => tag::PROVINCE_FUND,
        JoinShard => tag::JOIN_SHARD,
        BeaconLog => tag::BEACON_LOG,
        DefencePool => tag::DEFENCE_POOL,
        Citizen => tag::CITIZEN,
        Holding => tag::HOLDING,
        Province => tag::PROVINCE,
        ArrivalSlot => tag::ARRIVAL_SLOT,
        ArrivalDay => tag::ARRIVAL_DAY,
        ClashInputs => tag::CLASH_INPUTS,
        BellAnchor => tag::BELL_ANCHOR,
        SeedCache => tag::SEED_CACHE,
        AnchorArchive => tag::ANCHOR_ARCHIVE,
        DefenceClaim => tag::DEFENCE_CLAIM,
    })
}

/// Raw key length of each kind (§4.1 table; the Season PDA has none).
pub const fn raw_len(kind: AccountKind) -> usize {
    use AccountKind::*;
    match kind {
        Season | Frontier | DefencePool => 0,
        RingSeed | JoinShard => 2,
        ProvinceFund | BeaconLog => 1,
        Citizen => 15,
        Holding => 9,
        Province => 8,
        ArrivalSlot => 14,
        ArrivalDay | ClashInputs | DefenceClaim => 12,
        BellAnchor | AnchorArchive => 5,
        SeedCache => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_lengths_are_the_contract_table() {
        assert_eq!(frontier_seed().len(), 2);
        assert_eq!(ring_seed_seed(7).len(), 6);
        assert_eq!(province_fund_seed(5).len(), 4);
        assert_eq!(join_shard_seed(5, 7).len(), 6);
        assert_eq!(beacon_log_seed(15).len(), 4);
        assert_eq!(defence_pool_seed().len(), 2);
        assert_eq!(citizen_seed(&[0xAB; 15]).len(), 32);
        assert_eq!(holding_seed(i32::MIN, i32::MAX, 11).len(), 20);
        assert_eq!(province_seed(-1, 1).len(), 18);
        assert_eq!(
            arrival_slot_seed(i32::MIN, i32::MAX, u32::MAX, 5, 3).len(),
            30
        );
        assert_eq!(arrival_day_seed(1, 2, u32::MAX).len(), 26);
        assert_eq!(clash_inputs_seed(1, 2, 3).len(), 26);
        assert_eq!(seal_verdict_seed(u64::MAX, 7).len(), 26);
        assert_eq!(bell_anchor_seed(9, 15).len(), 12);
        assert_eq!(seed_cache_seed(9, 15, 255).len(), 14);
        assert_eq!(anchor_archive_seed(15, u32::MAX).len(), 12);
        assert_eq!(defence_claim_seed(&[1; 8], 3).len(), 26);
        assert_eq!(posture_seed(1, 2, 3, 59).len(), 28);
        assert_eq!(seed_cache_seed(9, 15, 255).as_bytes(), b"sd090000000fff");
        for k in AccountKind::ALL {
            if let Some(t) = tag_of(k) {
                assert!(t[0].is_ascii_lowercase() && t[1].is_ascii_lowercase());
                assert!(2 + 2 * raw_len(k) <= MAX_SEED);
            }
        }
    }

    #[test]
    fn seeds_parse_back() {
        let s = arrival_slot_seed(-3, 4, 1_000, 5, 2);
        let (t, raw, n) = Seed::parse(s.as_bytes()).unwrap();
        assert_eq!(t, *b"ar");
        assert_eq!(n, 14);
        assert_eq!(Seed::new(t, &raw[..n]), s);
        assert!(Seed::parse(b"arXY").is_none());
    }

    #[test]
    fn host_ids_round_trip_at_the_extremes() {
        for (p, q) in [
            (0, 0),
            (128, 0),
            (-128, 0),
            (0, -128),
            (64, -128),
            (-64, -64),
            (1, -1),
        ] {
            for (site, gen, seq) in [(0u8, 0u8, 0u32), (11, 255, u32::MAX), (5, 7, 12_345)] {
                let id = host_id(p, q, site, gen, seq).unwrap();
                let h = split_host_id(id).unwrap();
                assert_eq!(
                    (h.province.p, h.province.q, h.site, h.gen, h.seq),
                    (p, q, site, gen, seq)
                );
                assert_eq!(
                    holding_key_of_host(id),
                    host_id(p, q, site, gen, 0).unwrap()
                );
            }
        }
        assert_eq!(host_id(129, 0, 0, 0, 0), None);
        assert_eq!(host_id(i32::MIN, i32::MAX, 0, 0, 0), None);
        assert_eq!(host_id(0, 0, 12, 0, 0), None);
        assert_eq!(split_host_id(u64::MAX), None);
        // index < 2^20 for R ≤ 128
        assert!(provinces_within(128) < (1 << 20));
    }
}
