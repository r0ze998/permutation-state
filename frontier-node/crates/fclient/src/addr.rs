//! Canonical addresses (M1 contract §4.1, I-01, I-02).
//!
//! `addr(kind, key) = sha256(season_pda ‖ seed ‖ program_id)` (System
//! `create_with_seed`), `seed = tag(2 ASCII) ‖ lowercase-hex(raw)`, `raw`
//! the key fields little-endian at fixed width (coordinates i32, days and
//! bells u32), at most 15 bytes. The Season is the only PDA:
//! `["season", le64(id)]`.
//!
//! **Integration note.** The grammar is the kernel's
//! (`permutation_rules::frontier::addr`, W1-C, built in parallel); this is a
//! byte-for-byte twin of SP-V2 `acct.rs` so `fclient` is testable now. W2-F
//! makes `seed`/`host_id` call the kernel once W1-C is merged; the tests
//! below (SP-V2 strings and extremes) must stay green across that switch.

use sha2::{Digest, Sha256};
use solana_address::Address;

use crate::abi;

/// Seed tags (§4.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
    /// M3, reserved.
    Posture,
}

impl SeedKind {
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

    /// Raw key length in bytes (§4.1 table).
    pub const fn raw_len(self) -> usize {
        match self {
            SeedKind::Frontier | SeedKind::DefencePool => 0,
            SeedKind::RingSeed | SeedKind::JoinShard => 2,
            SeedKind::ProvinceFund | SeedKind::BeaconLog => 1,
            SeedKind::Citizen => 15,
            SeedKind::Holding => 9,
            SeedKind::Province => 8,
            SeedKind::ArrivalSlot | SeedKind::Posture => 14,
            SeedKind::ArrivalDay
            | SeedKind::ClashInputs
            | SeedKind::SealVerdict
            | SeedKind::DefenceClaim => 12,
            SeedKind::BellAnchor | SeedKind::AnchorArchive => 5,
            SeedKind::SeedCache => 6,
        }
    }
}

/// `tag ‖ lowercase-hex(raw)`; returns the buffer and its length (≤ 32).
pub fn seed(kind: SeedKind, raw: &[u8]) -> ([u8; 32], usize) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    assert!(raw.len() <= 15, "raw key ≤ 15 bytes");
    let mut buf = [0u8; 32];
    buf[..2].copy_from_slice(kind.tag());
    let mut n = 2;
    for &b in raw {
        buf[n] = HEX[(b >> 4) as usize];
        buf[n + 1] = HEX[(b & 15) as usize];
        n += 2;
    }
    (buf, n)
}

/// The seed as a string (ASCII by construction).
pub fn seed_str(kind: SeedKind, raw: &[u8]) -> String {
    let (b, n) = seed(kind, raw);
    String::from_utf8(b[..n].to_vec()).expect("ascii")
}

/// System `create_with_seed`.
pub fn with_seed(base: &Address, seed: &[u8], owner: &Address) -> Address {
    let h: [u8; 32] = Sha256::new()
        .chain_update(base.as_ref())
        .chain_update(seed)
        .chain_update(owner.as_ref())
        .finalize()
        .into();
    Address::new_from_array(h)
}

/// `["season", le64(id)]` under the program.
pub fn season_pda(program: &Address, id: u64) -> (Address, u8) {
    Address::find_program_address(&[b"season", &id.to_le_bytes()], program)
}

/// The LoaderV3 ProgramData address of `program`.
pub fn programdata(program: &Address) -> Address {
    Address::find_program_address(&[program.as_ref()], &address(abi::LOADER_V3)).0
}

/// Parses a base58 address constant.
pub fn address(s: &str) -> Address {
    s.parse().expect("valid base58 address")
}

pub fn system_program() -> Address {
    address(abi::SYSTEM_PROGRAM)
}
pub fn compute_budget_program() -> Address {
    address(abi::COMPUTE_BUDGET_PROGRAM)
}
pub fn instructions_sysvar() -> Address {
    address(abi::INSTRUCTIONS_SYSVAR)
}
pub fn clock_sysvar() -> Address {
    address(abi::CLOCK_SYSVAR)
}
pub fn incinerator() -> Address {
    address(abi::INCINERATOR)
}

// ---------------------------------------------------------------- raw keys

pub fn raw_ring_seed(d: u16) -> [u8; 2] {
    d.to_le_bytes()
}
pub fn raw_join_shard(faction: u8, shard: u8) -> [u8; 2] {
    [faction, shard]
}
pub fn raw_holding(p: i32, q: i32, site: u8) -> [u8; 9] {
    let mut r = [0u8; 9];
    r[..4].copy_from_slice(&p.to_le_bytes());
    r[4..8].copy_from_slice(&q.to_le_bytes());
    r[8] = site;
    r
}
pub fn raw_province(p: i32, q: i32) -> [u8; 8] {
    let mut r = [0u8; 8];
    r[..4].copy_from_slice(&p.to_le_bytes());
    r[4..].copy_from_slice(&q.to_le_bytes());
    r
}
/// `P, Q, bell, x, y` truncated to `n` bytes (SP-V2 `slot_seed`).
fn raw_pqb(p: i32, q: i32, bell: u32, x: u8, y: u8) -> [u8; 14] {
    let mut r = [0u8; 14];
    r[..4].copy_from_slice(&p.to_le_bytes());
    r[4..8].copy_from_slice(&q.to_le_bytes());
    r[8..12].copy_from_slice(&bell.to_le_bytes());
    r[12] = x;
    r[13] = y;
    r
}
pub fn raw_arrival_slot(p: i32, q: i32, bell: u32, faction: u8, i: u8) -> [u8; 14] {
    raw_pqb(p, q, bell, faction, i)
}
pub fn raw_arrival_day(p: i32, q: i32, day: u32) -> [u8; 12] {
    raw_pqb(p, q, day, 0, 0)[..12].try_into().expect("12")
}
pub fn raw_clash_inputs(p: i32, q: i32, bell: u32) -> [u8; 12] {
    raw_pqb(p, q, bell, 0, 0)[..12].try_into().expect("12")
}
pub fn raw_posture(p: i32, q: i32, bell: u32, pos: u8) -> [u8; 14] {
    raw_pqb(p, q, bell, pos, 0)
}
pub fn raw_seal_verdict(host: u64, bell: u32) -> [u8; 12] {
    let mut r = [0u8; 12];
    r[..8].copy_from_slice(&host.to_le_bytes());
    r[8..].copy_from_slice(&bell.to_le_bytes());
    r
}
pub fn raw_anchor(bell: u32, region: u8) -> [u8; 5] {
    let mut r = [0u8; 5];
    r[..4].copy_from_slice(&bell.to_le_bytes());
    r[4] = region;
    r
}
pub fn raw_seed_cache(bell: u32, region: u8, nonce: u8) -> [u8; 6] {
    let mut r = [0u8; 6];
    r[..4].copy_from_slice(&bell.to_le_bytes());
    r[4] = region;
    r[5] = nonce;
    r
}
pub fn raw_archive(region: u8, day: u32) -> [u8; 5] {
    let mut r = [0u8; 5];
    r[0] = region;
    r[1..].copy_from_slice(&day.to_le_bytes());
    r
}
pub fn raw_defence_claim(beneficiary: &[u8; 32], day: u32) -> [u8; 12] {
    let mut r = [0u8; 12];
    r[..8].copy_from_slice(&keeper_tag8(beneficiary));
    r[8..].copy_from_slice(&day.to_le_bytes());
    r
}

/// `sha256("PSF-CIT" ‖ wallet)[0..15]`.
pub fn citizen_tag15(wallet: &[u8; 32]) -> [u8; 15] {
    let h = Sha256::new()
        .chain_update(b"PSF-CIT")
        .chain_update(wallet)
        .finalize();
    h[..15].try_into().expect("15")
}

/// `sha256("PSF-KPR" ‖ beneficiary)[0..8]`.
pub fn keeper_tag8(b: &[u8; 32]) -> [u8; 8] {
    let h = Sha256::new()
        .chain_update(b"PSF-KPR")
        .chain_update(b)
        .finalize();
    h[..8].try_into().expect("8")
}

/// The JoinShard of a wallet: `sha256(wallet)[0] mod 8` (§5.9 Join).
pub fn join_shard_of(wallet: &[u8; 32]) -> u8 {
    Sha256::digest(wallet)[0] % abi::SHARDS_PER_FACTION
}

/// The quota's citizen id: the Citizen address's first 8 bytes, LE u64.
pub fn citizen_tag_u64(citizen: &Address) -> u64 {
    u64::from_le_bytes(citizen.as_ref()[..8].try_into().expect("8"))
}

/// `day(b) = b / 144`.
pub const fn day_of(bell: u32) -> u32 {
    bell / abi::BELLS_PER_DAY
}

// ---------------------------------------------------------------- host id

/// Why a host id cannot be built or read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostIdError {
    /// The province index does not fit 20 bits (R ≤ 128 guarantees it does).
    ProvinceIndex,
    /// Site ≥ 12 (4 bits hold 0..=15; the rule is < 12).
    Site,
}

/// Province index as the kernel numbers it (`ProvinceCoord::index`).
pub fn province_index(p: i32, q: i32) -> u32 {
    permutation_rules::frontier::geometry::ProvinceCoord::new(p, q).index()
}

/// `(province_index << 44) | (site << 40) | (gen << 32) | seq` (§4.1): the
/// kernel's `addr::host_id` (ring ≤ 128; the first twin accepted ring 129).
pub fn host_id(p: i32, q: i32, site: u8, gen: u8, seq: u32) -> Result<u64, HostIdError> {
    if site >= 12 {
        return Err(HostIdError::Site);
    }
    let id = permutation_rules::frontier::addr::host_id(p, q, site, gen, seq);
    if id == permutation_rules::frontier::addr::HOST_ID_INVALID {
        return Err(HostIdError::ProvinceIndex);
    }
    Ok(id)
}

/// Inverse of [`host_id`]: `(P, Q, site, gen, seq)` (the kernel's
/// `addr::host_parts`: an index beyond ring 128 is refused).
pub fn host_parts(id: u64) -> Result<(i32, i32, u8, u8, u32), HostIdError> {
    let site = ((id >> 40) & 0xF) as u8;
    if site >= 12 {
        return Err(HostIdError::Site);
    }
    let h = permutation_rules::frontier::addr::host_parts(id).ok_or(HostIdError::ProvinceIndex)?;
    Ok((h.p, h.q, h.site, h.gen, h.seq))
}

// ---------------------------------------------------------------- per season

/// Every canonical address of one season of one program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Addresses {
    pub program: Address,
    pub season_id: u64,
    pub season: Address,
    pub bump: u8,
}

impl Addresses {
    pub fn new(program: Address, season_id: u64) -> Addresses {
        let (season, bump) = season_pda(&program, season_id);
        Addresses {
            program,
            season_id,
            season,
            bump,
        }
    }

    /// The with-seed address of `kind` at `raw`.
    pub fn of(&self, kind: SeedKind, raw: &[u8]) -> Address {
        debug_assert_eq!(raw.len(), kind.raw_len(), "{kind:?}");
        let (s, n) = seed(kind, raw);
        with_seed(&self.season, &s[..n], &self.program)
    }

    pub fn frontier(&self) -> Address {
        self.of(SeedKind::Frontier, &[])
    }
    pub fn ring_seed(&self, d: u16) -> Address {
        self.of(SeedKind::RingSeed, &raw_ring_seed(d))
    }
    pub fn province_fund(&self, wedge: u8) -> Address {
        self.of(SeedKind::ProvinceFund, &[wedge])
    }
    pub fn province_funds(&self) -> [Address; 6] {
        core::array::from_fn(|w| self.province_fund(w as u8))
    }
    pub fn join_shard(&self, faction: u8, shard: u8) -> Address {
        self.of(SeedKind::JoinShard, &raw_join_shard(faction, shard))
    }
    pub fn beacon_log(&self, region: u8) -> Address {
        self.of(SeedKind::BeaconLog, &[region])
    }
    pub fn defence_pool(&self) -> Address {
        self.of(SeedKind::DefencePool, &[])
    }
    pub fn citizen(&self, wallet: &Address) -> Address {
        self.of(SeedKind::Citizen, &citizen_tag15(&wallet.to_bytes()))
    }
    pub fn holding(&self, p: i32, q: i32, site: u8) -> Address {
        self.of(SeedKind::Holding, &raw_holding(p, q, site))
    }
    pub fn province(&self, p: i32, q: i32) -> Address {
        self.of(SeedKind::Province, &raw_province(p, q))
    }
    pub fn arrival_slot(&self, p: i32, q: i32, bell: u32, faction: u8, i: u8) -> Address {
        self.of(
            SeedKind::ArrivalSlot,
            &raw_arrival_slot(p, q, bell, faction, i),
        )
    }
    pub fn arrival_day(&self, p: i32, q: i32, day: u32) -> Address {
        self.of(SeedKind::ArrivalDay, &raw_arrival_day(p, q, day))
    }
    pub fn clash_inputs(&self, p: i32, q: i32, bell: u32) -> Address {
        self.of(SeedKind::ClashInputs, &raw_clash_inputs(p, q, bell))
    }
    pub fn anchor(&self, bell: u32, region: u8) -> Address {
        self.of(SeedKind::BellAnchor, &raw_anchor(bell, region))
    }
    pub fn seed_cache(&self, bell: u32, region: u8, nonce: u8) -> Address {
        self.of(SeedKind::SeedCache, &raw_seed_cache(bell, region, nonce))
    }
    pub fn archive(&self, region: u8, day: u32) -> Address {
        self.of(SeedKind::AnchorArchive, &raw_archive(region, day))
    }
    pub fn defence_claim(&self, beneficiary: &Address, day: u32) -> Address {
        self.of(
            SeedKind::DefenceClaim,
            &raw_defence_claim(&beneficiary.to_bytes(), day),
        )
    }
    pub fn programdata(&self) -> Address {
        programdata(&self.program)
    }
    /// The Holding that owns host `id`.
    pub fn holding_of_host(&self, id: u64) -> Result<Address, HostIdError> {
        let (p, q, site, _, _) = host_parts(id)?;
        Ok(self.holding(p, q, site))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_strings_match_sp_v2_byte_for_byte() {
        // SP-V2 acct.rs: anchor_seed(bell, region) = "an" ‖ hex(le32 bell, region).
        assert_eq!(
            seed_str(SeedKind::BellAnchor, &raw_anchor(0x0102_0304, 7)),
            "an0403020107"
        );
        assert_eq!(
            seed_str(SeedKind::SeedCache, &raw_seed_cache(1, 15, 255)),
            "sd010000000fff"
        );
        assert_eq!(
            seed_str(SeedKind::ArrivalSlot, &raw_arrival_slot(-1, 2, 3, 4, 5)),
            "arffffffff02000000030000000405"
        );
        assert_eq!(
            seed_str(SeedKind::ClashInputs, &raw_clash_inputs(-1, 2, 3)),
            "ciffffffff0200000003000000"
        );
        assert_eq!(
            seed_str(SeedKind::Posture, &raw_posture(1, 1, 1, 9)),
            "po0100000001000000010000000900"
        );
        assert_eq!(
            seed_str(SeedKind::AnchorArchive, &raw_archive(3, 7)),
            "aa0307000000"
        );
        assert_eq!(
            seed_str(SeedKind::SealVerdict, &raw_seal_verdict(u64::MAX, 1)),
            "svffffffffffffffff01000000"
        );
    }

    #[test]
    fn seed_lengths_match_the_table() {
        let expect = [
            (SeedKind::Frontier, 2),
            (SeedKind::RingSeed, 6),
            (SeedKind::ProvinceFund, 4),
            (SeedKind::JoinShard, 6),
            (SeedKind::BeaconLog, 4),
            (SeedKind::DefencePool, 2),
            (SeedKind::Citizen, 32),
            (SeedKind::Holding, 20),
            (SeedKind::Province, 18),
            (SeedKind::ArrivalSlot, 30),
            (SeedKind::ArrivalDay, 26),
            (SeedKind::ClashInputs, 26),
            (SeedKind::SealVerdict, 26),
            (SeedKind::BellAnchor, 12),
            (SeedKind::SeedCache, 14),
            (SeedKind::AnchorArchive, 12),
            (SeedKind::DefenceClaim, 26),
            (SeedKind::Posture, 30),
        ];
        for (k, n) in expect {
            let raw = vec![0xABu8; k.raw_len()];
            assert_eq!(seed(k, &raw).1, n, "{k:?}");
        }
    }

    #[test]
    fn extremes_encode() {
        assert_eq!(
            seed_str(
                SeedKind::ArrivalSlot,
                &raw_arrival_slot(i32::MIN, i32::MAX, u32::MAX, 5, 3)
            ),
            "ar00000080ffffff7fffffffff0503"
        );
        assert_eq!(
            seed_str(SeedKind::Province, &raw_province(-3, 4)),
            "pvfdffffff04000000"
        );
    }

    #[test]
    fn with_seed_is_create_with_seed() {
        // sha256(base ‖ seed ‖ owner), as SP-V2 `with_seed`.
        let base = Address::new_from_array([1; 32]);
        let owner = Address::new_from_array([2; 32]);
        let a = with_seed(&base, b"fr", &owner);
        let mut h = Sha256::new();
        h.update([1u8; 32]);
        h.update(b"fr");
        h.update([2u8; 32]);
        assert_eq!(a.to_bytes(), <[u8; 32]>::from(h.finalize()));
    }

    #[test]
    fn host_id_round_trips() {
        for (p, q) in [(0, 0), (3, -2), (-17, 40), (128, -128), (-64, 0)] {
            let id = host_id(p, q, 11, 255, u32::MAX).unwrap();
            assert_eq!(host_parts(id).unwrap(), (p, q, 11, 255, u32::MAX));
        }
        assert_eq!(host_id(0, 0, 12, 0, 0), Err(HostIdError::Site));
    }

    /// The kernel's address vectors (W1-C): every host id row, incl. ring
    /// 129 and site 12 (refused), through fclient's codec.
    #[test]
    fn host_id_vectors_match_the_kernels() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../permutation-rules/vectors/addr-vectors-v1.json");
        let j: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).expect("vectors")).expect("json");
        for r in j["host_ids"].as_array().unwrap() {
            let (p, q) = (
                r["p"].as_i64().unwrap() as i32,
                r["q"].as_i64().unwrap() as i32,
            );
            let (site, gen) = (
                r["site"].as_u64().unwrap() as u8,
                r["gen"].as_u64().unwrap() as u8,
            );
            let seq = r["seq"].as_u64().unwrap() as u32;
            let want: u64 = r["host_id"].as_str().unwrap().parse().unwrap();
            match host_id(p, q, site, gen, seq) {
                Ok(id) => {
                    assert_eq!(id, want, "({p},{q})");
                    assert_eq!(host_parts(id), Ok((p, q, site, gen, seq)));
                }
                Err(_) => assert_eq!(want, u64::MAX, "({p},{q}) site {site} refused"),
            }
        }
        assert!(host_id(129, 0, 0, 0, 0).is_err());
        assert!(host_parts(u64::MAX).is_err());
    }
}
