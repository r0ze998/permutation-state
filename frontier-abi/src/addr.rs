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
//! **One grammar (integ-W1).** The kernel module
//! `permutation_rules::frontier::addr` (W1-C, CL-21) owns the seed strings,
//! the tags and the host-id codec (§4.1); every builder here delegates to
//! it and only adds the ABI's `Seed` type, the tag table by account kind
//! and [`AddrCtx`]. `tests/addresses.rs` still checks the result against a
//! verbatim copy of SP-V2's seed builder, and `tags_are_the_kernels` pins
//! the tag table to the kernel's.

use crate::layout::AccountKind;
use permutation_rules::frontier::addr as ka;
use permutation_rules::frontier::geometry::ProvinceCoord;
use permutation_rules::hash::sha256;

pub use permutation_rules::frontier::addr::{citizen_tag, citizen_tag15, keeper_tag8};

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

impl Seed {
    /// The kernel's seed string as the ABI type.
    fn of_kernel(k: ka::SeedStr) -> Seed {
        let (buf, len) = k.into_parts();
        Seed {
            buf,
            len: len as u8,
        }
    }
    /// `kind`'s seed of a raw key of exactly `kind.raw_len()` bytes (the
    /// empty seed otherwise, which no address check accepts).
    fn of_raw(kind: ka::SeedKind, raw: &[u8]) -> Seed {
        match ka::try_seed(kind, raw) {
            Some(k) => Seed::of_kernel(k),
            None => Seed {
                buf: [0; MAX_SEED],
                len: 0,
            },
        }
    }
}

pub fn frontier_seed() -> Seed {
    Seed::of_kernel(ka::frontier())
}
pub fn ring_seed_seed(d: u16) -> Seed {
    Seed::of_kernel(ka::ring_seed(d))
}
pub fn province_fund_seed(wedge: u8) -> Seed {
    Seed::of_kernel(ka::province_fund(wedge))
}
pub fn join_shard_seed(faction: u8, shard: u8) -> Seed {
    Seed::of_kernel(ka::join_shard(faction, shard))
}
pub fn beacon_log_seed(region: u8) -> Seed {
    Seed::of_kernel(ka::beacon_log(region))
}
pub fn defence_pool_seed() -> Seed {
    Seed::of_kernel(ka::defence_pool())
}
/// Citizen seed from its 15-byte tag ([`citizen_tag15`]).
pub fn citizen_seed(tag15: &[u8; 15]) -> Seed {
    Seed::of_raw(ka::SeedKind::Citizen, tag15)
}
pub fn holding_seed(p: i32, q: i32, site: u8) -> Seed {
    Seed::of_kernel(ka::holding(p, q, site))
}
pub fn province_seed(p: i32, q: i32) -> Seed {
    Seed::of_kernel(ka::province(p, q))
}
pub fn arrival_slot_seed(p: i32, q: i32, bell: u32, faction: u8, i: u8) -> Seed {
    Seed::of_kernel(ka::slot(p, q, bell, faction, i))
}
pub fn arrival_day_seed(p: i32, q: i32, day: u32) -> Seed {
    Seed::of_kernel(ka::arrival_day(p, q, day))
}
pub fn clash_inputs_seed(p: i32, q: i32, bell: u32) -> Seed {
    Seed::of_kernel(ka::clash_inputs(p, q, bell))
}
/// Reserved (M3). SP-V2 grammar: 13 raw bytes `P, Q, bell, pos` (28-byte
/// seed; contract v1.2 §4.1).
pub fn posture_seed(p: i32, q: i32, bell: u32, pos: u8) -> Seed {
    Seed::of_kernel(ka::posture(p, q, bell, pos))
}
/// Reserved: SealVerdict removed in v1.1 (I-44).
pub fn seal_verdict_seed(host_id: u64, arrive_bell: u32) -> Seed {
    Seed::of_kernel(ka::seal_verdict_reserved(host_id, arrive_bell))
}
pub fn bell_anchor_seed(bell: u32, region: u8) -> Seed {
    Seed::of_kernel(ka::anchor(bell, region))
}
pub fn seed_cache_seed(bell: u32, region: u8, nonce: u8) -> Seed {
    Seed::of_kernel(ka::seed_cache(bell, region, nonce))
}
pub fn anchor_archive_seed(region: u8, day: u32) -> Seed {
    Seed::of_kernel(ka::anchor_archive(region, day))
}
/// DefenceClaim seed from the keeper tag ([`keeper_tag8`]).
pub fn defence_claim_seed(keeper_tag: &[u8; 8], day: u32) -> Seed {
    let mut r = [0u8; 12];
    r[..8].copy_from_slice(keeper_tag);
    r[8..].copy_from_slice(&day.to_le_bytes());
    Seed::of_raw(ka::SeedKind::DefenceClaim, &r)
}

// ------------------------------------------------------------ tags

/// JoinShard of a wallet: `sha256(wallet)[0] mod 8` (§5.9 Join).
pub fn join_shard_of(wallet: &[u8; 32]) -> u8 {
    sha256(&[wallet])[0] % crate::layout::world::join_shard::SHARDS_PER_FACTION
}

/// `day(b) = b / 144`.
pub const fn day_of(bell: u32) -> u32 {
    bell / crate::layout::clash::BELLS_PER_DAY
}

// ------------------------------------------------------------ host ids

/// Host id (§4.1, pinned; the kernel's `addr::host_id`):
/// `province_index(P,Q) << 44 | site << 40 | gen << 32 | seq`.
/// `None` if the province is outside ring 128 or `site ≥ 12`.
pub fn host_id(p: i32, q: i32, site: u8, gen: u8, seq: u32) -> Option<u64> {
    let id = ka::host_id(p, q, site, gen, seq);
    (id != ka::HOST_ID_INVALID).then_some(id)
}

/// The parts of a host id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostParts {
    pub province: ProvinceCoord,
    pub site: u8,
    pub gen: u8,
    pub seq: u32,
}

/// Inverse of [`host_id`] (the kernel's `addr::host_parts`); `None` for an
/// id no valid holding can issue.
pub fn split_host_id(id: u64) -> Option<HostParts> {
    let h = ka::host_parts(id)?;
    Some(HostParts {
        province: ProvinceCoord::new(h.p, h.q),
        site: h.site,
        gen: h.gen,
        seq: h.seq,
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
    ProvinceCoord::new(p, q).checked_index()
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

    /// The ABI's tag table is the kernel's (one grammar, §4.1).
    #[test]
    fn tags_are_the_kernels() {
        use ka::SeedKind as K;
        let pairs = [
            (tag::FRONTIER, K::Frontier),
            (tag::RING_SEED, K::RingSeed),
            (tag::PROVINCE_FUND, K::ProvinceFund),
            (tag::JOIN_SHARD, K::JoinShard),
            (tag::BEACON_LOG, K::BeaconLog),
            (tag::DEFENCE_POOL, K::DefencePool),
            (tag::CITIZEN, K::Citizen),
            (tag::HOLDING, K::Holding),
            (tag::PROVINCE, K::Province),
            (tag::ARRIVAL_SLOT, K::ArrivalSlot),
            (tag::ARRIVAL_DAY, K::ArrivalDay),
            (tag::CLASH_INPUTS, K::ClashInputs),
            (tag::SEAL_VERDICT, K::SealVerdict),
            (tag::BELL_ANCHOR, K::BellAnchor),
            (tag::SEED_CACHE, K::SeedCache),
            (tag::ANCHOR_ARCHIVE, K::AnchorArchive),
            (tag::DEFENCE_CLAIM, K::DefenceClaim),
            (tag::POSTURE, K::Posture),
        ];
        assert_eq!(pairs.len(), K::ALL.len());
        for (t, k) in pairs {
            assert_eq!(&t, k.tag(), "{k:?}");
        }
        assert_eq!(
            crate::layout::player::citizen::TAG_DOMAIN,
            b"PSF-CIT",
            "citizen_tag15 domain"
        );
        // A wrong-length raw key builds the empty seed, never a short one.
        assert!(citizen_seed(&[1; 15]).len() == 32);
        assert_eq!(Seed::of_raw(K::Citizen, &[1; 14]).len(), 0);
    }

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
        assert!(permutation_rules::frontier::geometry::provinces_within(128) < (1 << 20));
    }
}
