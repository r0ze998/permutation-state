//! `SeasonParams` (CreateSeason's fixed-layout parameters, §5.7), their
//! validation, the announced params hash, and the M1 presets
//! `M1_LOCAL_7D` and `M1_PLAYTEST`.
//!
//! Values marked [placeholder] are working defaults until the measurement
//! or decision named next to them lands (the integrator amends them through
//! the owning wave's notes).

use crate::bytes::{rd_arr, rd_i64, rd_u16, rd_u32, rd_u64, rd_u8};
use crate::bytes::{wr_arr, wr_i64, wr_u16, wr_u32, wr_u64, wr_u8};
use permutation_rules::hash::sha256;

/// Byte layout of `SeasonParams` (224 B, ≤ 256 B by contract).
pub mod layout {
    crate::layout::fields!(size = 224;
        REGIONS @ 0 : "u8" = 1;
        GENESIS_RING @ 1 : "u8" = 1;
        R_MAX @ 2 : "u16" = 2;
        OFFICE_TERMS_PER_WALLET @ 4 : "u8" = 1;
        POSTURES_ENABLED @ 5 : "u8" = 1;
        PROGRAM_VERSION @ 6 : "u16" = 2;
        BELL_SECS @ 8 : "u32" = 4;
        JOIN_CLOSE_BELL @ 12 : "u32" = 4;
        END_BELL @ 16 : "u32" = 4;
        DRAND_PERIOD @ 20 : "u32" = 4;
        DRAND_GENESIS @ 24 : "i64" = 8;
        NETWORK @ 32 : "u8" = 1;
        MIN_LEAD @ 33 : "u8" = 1;
        MAX_LEAD @ 34 : "u8" = 1;
        TRANSIT_SLOTS @ 35 : "u8" = 1;
        QUICKNET_PK_HASH @ 36 : "[u8;32]" = 32;
        REVEAL_WINDOW @ 68 : "u32" = 4;
        SEED_MARGIN @ 72 : "u32" = 4;
        ARCHIVE_AFTER @ 76 : "u32" = 4;
        LATENESS_SLOTS @ 80 : "u8" = 1;
        RSV_81 @ 81 : "rsv" = 3;
        MARCH_FEE @ 84 : "u64" = 8;
        SEAL_BOND @ 92 : "u64" = 8;
        MIN_REVEAL_PRIORITY_MILLI @ 100 : "u32" = 4;
        REVEAL_CU_LIMIT @ 104 : "u32" = 4;
        REVEAL_LOADED_LIMIT @ 108 : "u32" = 4;
        BUCKET_RATE_PER_H @ 112 : "u16" = 2;
        BUCKET_BURST @ 114 : "u16" = 2;
        DEFENCE_CAP_MILLI @ 116 : "u32" = 4;
        THETA_EARLY_BPS @ 120 : "u16" = 2;
        THETA_LATE_BPS @ 122 : "u16" = 2;
        THETA_SWITCH_SECS @ 124 : "u32" = 4;
        RESERVE_BPS @ 128 : "u16" = 2;
        EXTRA_FREE_BPS @ 130 : "u16" = 2;
        CLASH_CLOSE_GRACE @ 132 : "u32" = 4;
        CAMP_REGROW_BELLS @ 136 : "u32" = 4;
        DORMANT_AFTER_SECS @ 140 : "u32" = 4;
        RELEASE_AFTER_SECS @ 144 : "u32" = 4;
        PFUND_INITIAL @ 148 : "u64" = 8;
        DPOOL_INITIAL @ 156 : "u64" = 8;
        PER_BELL_REGION_CAP @ 164 : "u64" = 8;
        PER_KEEPER_DAY_CAP @ 172 : "u64" = 8;
        JOIN_GATE @ 180 : "[u8;32]" = 32;
        RSV @ 212 : "rsv" = 12;
    );
}

pub const SEASON_PARAMS_LEN: usize = layout::SIZE;

/// Domain of the announced params hash (§5.7 CreateSeason).
pub const PARAMS_DOMAIN: &[u8] = b"PSF-PARAMS-v1";

/// drand quicknet (`bls-unchained-g1-rfc9380`), from
/// `https://api.drand.sh/v2/beacons/quicknet/info` (SP-V2 `quicknet-info.json`).
pub const QUICKNET_GENESIS: i64 = 1_692_803_367;
pub const QUICKNET_PERIOD: u32 = 3;
/// The quicknet group public key, 96-B compressed G2 as published.
pub const QUICKNET_PUBLIC_KEY: [u8; 96] = [
    0x83, 0xcf, 0x0f, 0x28, 0x96, 0xad, 0xee, 0x7e, 0xb8, 0xb5, 0xf0, 0x1f, 0xca, 0xd3, 0x91, 0x22,
    0x12, 0xc4, 0x37, 0xe0, 0x07, 0x3e, 0x91, 0x1f, 0xb9, 0x00, 0x22, 0xd3, 0xe7, 0x60, 0x18, 0x3c,
    0x8c, 0x4b, 0x45, 0x0b, 0x6a, 0x0a, 0x6c, 0x3a, 0xc6, 0xa5, 0x77, 0x6a, 0x2d, 0x10, 0x64, 0x51,
    0x0d, 0x1f, 0xec, 0x75, 0x8c, 0x92, 0x1c, 0xc2, 0x2b, 0x0e, 0x17, 0xe6, 0x3a, 0xaf, 0x4b, 0xcb,
    0x5e, 0xd6, 0x63, 0x04, 0xde, 0x9c, 0xf8, 0x09, 0xbd, 0x27, 0x4c, 0xa7, 0x3b, 0xab, 0x4a, 0xf5,
    0xa6, 0xe9, 0xc7, 0x6a, 0x4b, 0xc0, 0x9e, 0x76, 0xea, 0xe8, 0x99, 0x1e, 0xf5, 0xec, 0xe4, 0x5a,
];
/// `QUICKNET_PK_HASH = sha256(QUICKNET_PUBLIC_KEY)` (the value the release
/// program embeds and the Season stores; a `test-beacon` build pins its own,
/// I-53).
pub const QUICKNET_PK_HASH: [u8; 32] = [
    0x96, 0xe7, 0x4f, 0xcd, 0xd3, 0xa1, 0x18, 0x40, 0x6d, 0x38, 0x00, 0xa4, 0xe4, 0x93, 0x5e, 0x67,
    0x45, 0x0a, 0x6b, 0xef, 0xde, 0x91, 0x5d, 0x47, 0xa0, 0xd6, 0xa1, 0x35, 0x19, 0xce, 0xe1, 0x34,
];

/// `RULESET_HASH = permutation_rules::frontier::ruleset_hash()` (contract
/// §3.2): the value the program embeds and CreateSeason writes. A const so
/// the no_std program can embed it; `ruleset_hash_is_the_kernels` fails the
/// moment a kernel version, table or bound constant changes, and
/// `abi-vectors` publishes it in `presets.json`.
pub const RULESET_HASH: [u8; 32] = [
    0x1a, 0xc1, 0x1f, 0x85, 0xfd, 0xe3, 0xb8, 0x98, 0xeb, 0xcd, 0x8c, 0x24, 0x69, 0x64, 0xd9, 0xbe,
    0x2a, 0x29, 0xb4, 0x14, 0x4a, 0x7a, 0x7d, 0xdf, 0xa8, 0x10, 0x06, 0x99, 0x9a, 0x6d, 0xd0, 0x3f,
];

/// Smallest AnnounceSeason creation bond (1 SOL; test SOL in M1, CL-24).
pub const MIN_CREATION_BOND: u64 = 1_000_000_000;
/// AnnounceSeason lead: `t_create_min ≥ now + 24 h`.
pub const MIN_ANNOUNCE_LEAD_SECS: i64 = 86_400;
/// CreateSeason must land within 7 days of `t_create_min`.
pub const CREATE_WINDOW_SECS: i64 = 7 * 86_400;
/// Closes after the end wait 72 h.
pub const END_GRACE_SECS: i64 = 72 * 3_600;
/// Incinerator (bond burn on a post-round pre-join abort, CL-24).
pub const INCINERATOR: &str = "1nc1nerator11111111111111111111111111111111";
/// Rules version of the Frontier kernels stored in the Season.
pub const RULES_VERSION: u16 = permutation_rules::frontier::RULES_VERSION_FRONTIER;

/// CreateSeason's parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeasonParams {
    pub regions: u8,
    pub genesis_ring: u8,
    pub r_max: u16,
    pub office_terms_per_wallet: u8,
    pub postures_enabled: u8,
    pub program_version: u16,
    pub bell_secs: u32,
    pub join_close_bell: u32,
    pub end_bell: u32,
    pub drand_period: u32,
    pub drand_genesis: i64,
    pub network: u8,
    pub min_lead: u8,
    pub max_lead: u8,
    pub transit_slots: u8,
    pub quicknet_pk_hash: [u8; 32],
    pub reveal_window: u32,
    pub seed_margin: u32,
    pub archive_after: u32,
    pub lateness_slots: u8,
    pub march_fee: u64,
    pub seal_bond: u64,
    pub min_reveal_priority_milli: u32,
    pub reveal_cu_limit: u32,
    pub reveal_loaded_limit: u32,
    pub bucket_rate_per_h: u16,
    pub bucket_burst: u16,
    pub defence_cap_milli: u32,
    pub theta_early_bps: u16,
    pub theta_late_bps: u16,
    pub theta_switch_secs: u32,
    pub reserve_bps: u16,
    pub extra_free_bps: u16,
    pub clash_close_grace: u32,
    pub camp_regrow_bells: u32,
    pub dormant_after_secs: u32,
    pub release_after_secs: u32,
    pub pfund_initial: u64,
    pub dpool_initial: u64,
    pub per_bell_region_cap: u64,
    pub per_keeper_day_cap: u64,
    pub join_gate: [u8; 32],
}

impl SeasonParams {
    pub fn to_bytes(&self) -> [u8; SEASON_PARAMS_LEN] {
        use layout as L;
        let mut d = [0u8; SEASON_PARAMS_LEN];
        // Every offset below is a constant of the 224-B layout, so no write
        // can fall outside `d`.
        let _ = wr_u8(&mut d, L::REGIONS, self.regions)
            & wr_u8(&mut d, L::GENESIS_RING, self.genesis_ring)
            & wr_u16(&mut d, L::R_MAX, self.r_max)
            & wr_u8(
                &mut d,
                L::OFFICE_TERMS_PER_WALLET,
                self.office_terms_per_wallet,
            )
            & wr_u8(&mut d, L::POSTURES_ENABLED, self.postures_enabled)
            & wr_u16(&mut d, L::PROGRAM_VERSION, self.program_version)
            & wr_u32(&mut d, L::BELL_SECS, self.bell_secs)
            & wr_u32(&mut d, L::JOIN_CLOSE_BELL, self.join_close_bell)
            & wr_u32(&mut d, L::END_BELL, self.end_bell)
            & wr_u32(&mut d, L::DRAND_PERIOD, self.drand_period)
            & wr_i64(&mut d, L::DRAND_GENESIS, self.drand_genesis)
            & wr_u8(&mut d, L::NETWORK, self.network)
            & wr_u8(&mut d, L::MIN_LEAD, self.min_lead)
            & wr_u8(&mut d, L::MAX_LEAD, self.max_lead)
            & wr_u8(&mut d, L::TRANSIT_SLOTS, self.transit_slots)
            & wr_arr(&mut d, L::QUICKNET_PK_HASH, &self.quicknet_pk_hash)
            & wr_u32(&mut d, L::REVEAL_WINDOW, self.reveal_window)
            & wr_u32(&mut d, L::SEED_MARGIN, self.seed_margin)
            & wr_u32(&mut d, L::ARCHIVE_AFTER, self.archive_after)
            & wr_u8(&mut d, L::LATENESS_SLOTS, self.lateness_slots)
            & wr_u64(&mut d, L::MARCH_FEE, self.march_fee)
            & wr_u64(&mut d, L::SEAL_BOND, self.seal_bond)
            & wr_u32(
                &mut d,
                L::MIN_REVEAL_PRIORITY_MILLI,
                self.min_reveal_priority_milli,
            )
            & wr_u32(&mut d, L::REVEAL_CU_LIMIT, self.reveal_cu_limit)
            & wr_u32(&mut d, L::REVEAL_LOADED_LIMIT, self.reveal_loaded_limit)
            & wr_u16(&mut d, L::BUCKET_RATE_PER_H, self.bucket_rate_per_h)
            & wr_u16(&mut d, L::BUCKET_BURST, self.bucket_burst)
            & wr_u32(&mut d, L::DEFENCE_CAP_MILLI, self.defence_cap_milli)
            & wr_u16(&mut d, L::THETA_EARLY_BPS, self.theta_early_bps)
            & wr_u16(&mut d, L::THETA_LATE_BPS, self.theta_late_bps)
            & wr_u32(&mut d, L::THETA_SWITCH_SECS, self.theta_switch_secs)
            & wr_u16(&mut d, L::RESERVE_BPS, self.reserve_bps)
            & wr_u16(&mut d, L::EXTRA_FREE_BPS, self.extra_free_bps)
            & wr_u32(&mut d, L::CLASH_CLOSE_GRACE, self.clash_close_grace)
            & wr_u32(&mut d, L::CAMP_REGROW_BELLS, self.camp_regrow_bells)
            & wr_u32(&mut d, L::DORMANT_AFTER_SECS, self.dormant_after_secs)
            & wr_u32(&mut d, L::RELEASE_AFTER_SECS, self.release_after_secs)
            & wr_u64(&mut d, L::PFUND_INITIAL, self.pfund_initial)
            & wr_u64(&mut d, L::DPOOL_INITIAL, self.dpool_initial)
            & wr_u64(&mut d, L::PER_BELL_REGION_CAP, self.per_bell_region_cap)
            & wr_u64(&mut d, L::PER_KEEPER_DAY_CAP, self.per_keeper_day_cap)
            & wr_arr(&mut d, L::JOIN_GATE, &self.join_gate);
        d
    }

    /// Decodes the 224-B layout; `None` if short or a reserved byte is set.
    pub fn from_bytes(d: &[u8]) -> Option<SeasonParams> {
        use layout as L;
        if d.len() != SEASON_PARAMS_LEN
            || d[L::RSV_81..L::RSV_81 + 3].iter().any(|b| *b != 0)
            || d[L::RSV..].iter().any(|b| *b != 0)
        {
            return None;
        }
        Some(SeasonParams {
            regions: rd_u8(d, L::REGIONS)?,
            genesis_ring: rd_u8(d, L::GENESIS_RING)?,
            r_max: rd_u16(d, L::R_MAX)?,
            office_terms_per_wallet: rd_u8(d, L::OFFICE_TERMS_PER_WALLET)?,
            postures_enabled: rd_u8(d, L::POSTURES_ENABLED)?,
            program_version: rd_u16(d, L::PROGRAM_VERSION)?,
            bell_secs: rd_u32(d, L::BELL_SECS)?,
            join_close_bell: rd_u32(d, L::JOIN_CLOSE_BELL)?,
            end_bell: rd_u32(d, L::END_BELL)?,
            drand_period: rd_u32(d, L::DRAND_PERIOD)?,
            drand_genesis: rd_i64(d, L::DRAND_GENESIS)?,
            network: rd_u8(d, L::NETWORK)?,
            min_lead: rd_u8(d, L::MIN_LEAD)?,
            max_lead: rd_u8(d, L::MAX_LEAD)?,
            transit_slots: rd_u8(d, L::TRANSIT_SLOTS)?,
            quicknet_pk_hash: rd_arr(d, L::QUICKNET_PK_HASH)?,
            reveal_window: rd_u32(d, L::REVEAL_WINDOW)?,
            seed_margin: rd_u32(d, L::SEED_MARGIN)?,
            archive_after: rd_u32(d, L::ARCHIVE_AFTER)?,
            lateness_slots: rd_u8(d, L::LATENESS_SLOTS)?,
            march_fee: rd_u64(d, L::MARCH_FEE)?,
            seal_bond: rd_u64(d, L::SEAL_BOND)?,
            min_reveal_priority_milli: rd_u32(d, L::MIN_REVEAL_PRIORITY_MILLI)?,
            reveal_cu_limit: rd_u32(d, L::REVEAL_CU_LIMIT)?,
            reveal_loaded_limit: rd_u32(d, L::REVEAL_LOADED_LIMIT)?,
            bucket_rate_per_h: rd_u16(d, L::BUCKET_RATE_PER_H)?,
            bucket_burst: rd_u16(d, L::BUCKET_BURST)?,
            defence_cap_milli: rd_u32(d, L::DEFENCE_CAP_MILLI)?,
            theta_early_bps: rd_u16(d, L::THETA_EARLY_BPS)?,
            theta_late_bps: rd_u16(d, L::THETA_LATE_BPS)?,
            theta_switch_secs: rd_u32(d, L::THETA_SWITCH_SECS)?,
            reserve_bps: rd_u16(d, L::RESERVE_BPS)?,
            extra_free_bps: rd_u16(d, L::EXTRA_FREE_BPS)?,
            clash_close_grace: rd_u32(d, L::CLASH_CLOSE_GRACE)?,
            camp_regrow_bells: rd_u32(d, L::CAMP_REGROW_BELLS)?,
            dormant_after_secs: rd_u32(d, L::DORMANT_AFTER_SECS)?,
            release_after_secs: rd_u32(d, L::RELEASE_AFTER_SECS)?,
            pfund_initial: rd_u64(d, L::PFUND_INITIAL)?,
            dpool_initial: rd_u64(d, L::DPOOL_INITIAL)?,
            per_bell_region_cap: rd_u64(d, L::PER_BELL_REGION_CAP)?,
            per_keeper_day_cap: rd_u64(d, L::PER_KEEPER_DAY_CAP)?,
            join_gate: rd_arr(d, L::JOIN_GATE)?,
        })
    }

    /// The §5.7 ranges, plus the M1 constants the bell model and the
    /// network assume (600-s bells, quicknet, no postures). `Err` names the
    /// first failing field; the program maps it to `BadData`.
    pub fn validate(&self) -> Result<(), &'static str> {
        let p = self;
        let checks: [(bool, &'static str); 26] = [
            (p.regions == 16, "regions"),
            (p.genesis_ring <= 4, "genesis_ring"),
            (p.r_max > p.genesis_ring as u16 && p.r_max <= 128, "r_max"),
            (p.office_terms_per_wallet == 1, "office_terms_per_wallet"),
            (p.postures_enabled == 0, "postures_enabled"),
            (p.bell_secs == 600, "bell_secs"),
            (
                p.join_close_bell < p.end_bell && p.end_bell <= 4_032,
                "join_close_bell/end_bell",
            ),
            (p.drand_period >= 1, "drand_period"),
            (
                p.network == crate::layout::world::season::NETWORK_QUICKNET,
                "network",
            ),
            (p.min_lead == 2, "min_lead"),
            (p.max_lead == 72, "max_lead"),
            (p.transit_slots == 4, "transit_slots"),
            ((600..=1_800).contains(&p.reveal_window), "reveal_window"),
            (p.seed_margin >= 60, "seed_margin"),
            (p.archive_after >= 172_800, "archive_after"),
            (
                p.min_reveal_priority_milli >= 100,
                "min_reveal_priority_milli",
            ),
            (
                (16_000..=40_000).contains(&p.reveal_cu_limit),
                "reveal_cu_limit",
            ),
            (
                p.reveal_loaded_limit % 32_768 == 0
                    && (65_536..=4_194_304).contains(&p.reveal_loaded_limit),
                "reveal_loaded_limit",
            ),
            (p.bucket_rate_per_h >= 1, "bucket_rate_per_h"),
            (p.bucket_burst >= p.bucket_rate_per_h, "bucket_burst"),
            (p.defence_cap_milli <= 4_000, "defence_cap_milli"),
            (p.camp_regrow_bells >= 6, "camp_regrow_bells"),
            (
                p.theta_early_bps <= 10_000 && p.theta_late_bps <= 10_000,
                "theta_bps",
            ),
            (p.reserve_bps <= 10_000, "reserve_bps"),
            (
                p.release_after_secs >= p.dormant_after_secs,
                "release_after_secs",
            ),
            (p.quicknet_pk_hash != [0u8; 32], "quicknet_pk_hash"),
        ];
        match checks.iter().find(|(ok, _)| !ok) {
            Some((_, name)) => Err(name),
            None => Ok(()),
        }
    }
}

/// `params_hash = sha256("PSF-PARAMS-v1" ‖ params)` where `params` is
/// CreateSeason's data after the tag: `SeasonParams` bytes ‖ the borsh
/// `PayoutParams` bytes (AnnounceSeason commits to both).
pub fn params_hash(season_params: &[u8; SEASON_PARAMS_LEN], payout: &[u8]) -> [u8; 32] {
    sha256(&[PARAMS_DOMAIN, season_params, payout])
}

/// `M1_PLAYTEST`'s join gate until the operator sets the relay's gate key:
/// not a valid ed25519 point, so it can never sign and Join stays closed
/// (fails closed).
pub const PLAYTEST_GATE_UNSET: [u8; 32] = [0xFF; 32];

/// The 7-day local season (Mode A exit run, §13.4).
pub const M1_LOCAL_7D: SeasonParams = SeasonParams {
    regions: 16,
    genesis_ring: 3,
    r_max: 16,
    office_terms_per_wallet: 1,
    postures_enabled: 0,
    program_version: 1,
    bell_secs: 600,
    join_close_bell: 756,
    end_bell: 1_008,
    drand_period: QUICKNET_PERIOD,
    drand_genesis: QUICKNET_GENESIS,
    network: 2,
    min_lead: 2,
    max_lead: 72,
    transit_slots: 4,
    quicknet_pk_hash: QUICKNET_PK_HASH,
    reveal_window: 600,
    seed_margin: 60,
    archive_after: 172_800,
    lateness_slots: 4,
    march_fee: 10_000,
    seal_bond: 20_000,
    min_reveal_priority_milli: 433,
    // [placeholder] until W3-B measures the worst Reveal (I-08, CL-22).
    reveal_cu_limit: 26_000,
    // [placeholder] 1 MiB until the release .so is measured (I-45).
    reveal_loaded_limit: 1_048_576,
    bucket_rate_per_h: 30,
    bucket_burst: 60,
    defence_cap_milli: 2_000,
    theta_early_bps: 5_500,
    theta_late_bps: 6_500,
    theta_switch_secs: 259_200,
    reserve_bps: 200,
    extra_free_bps: 2_000,
    clash_close_grace: 1_008,
    camp_regrow_bells: 144,
    dormant_after_secs: 432_000,
    release_after_secs: 864_000,
    // Rent of every province within ring 16 (817 × 21,457,920 ≈ 17.53 SOL)
    // over 6 wedge funds, test SOL.
    pfund_initial: 18_000_000_000,
    // 20 SOL default (D18), test SOL in M1.
    dpool_initial: 20_000_000_000,
    // [placeholder] 20 SOL / 100 p99 attacked bells; CL-30 sizes it.
    per_bell_region_cap: 200_000_000,
    // [placeholder] until CL-30.
    per_keeper_day_cap: 2_000_000_000,
    join_gate: [0u8; 32],
};

/// The playtest preset: `M1_LOCAL_7D` with the relay's invite gate (I-51).
pub const fn m1_playtest(join_gate: [u8; 32]) -> SeasonParams {
    let mut p = M1_LOCAL_7D;
    p.join_gate = join_gate;
    p
}

/// `M1_PLAYTEST` with the gate unset (fails closed until the operator
/// supplies the relay's gate key through [`m1_playtest`]).
pub const M1_PLAYTEST: SeasonParams = m1_playtest(PLAYTEST_GATE_UNSET);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruleset_hash_is_the_kernels() {
        assert_eq!(
            RULESET_HASH,
            permutation_rules::frontier::ruleset_hash(),
            "a kernel changed: update RULESET_HASH and regenerate the vectors"
        );
    }

    #[test]
    fn quicknet_pk_hash_is_the_hash_of_the_key() {
        assert_eq!(sha256(&[&QUICKNET_PUBLIC_KEY]), QUICKNET_PK_HASH);
    }

    #[test]
    fn presets_validate_and_round_trip() {
        for p in [M1_LOCAL_7D, M1_PLAYTEST, m1_playtest([7; 32])] {
            assert_eq!(p.validate(), Ok(()));
            assert_eq!(SeasonParams::from_bytes(&p.to_bytes()), Some(p));
        }
        assert_eq!(M1_LOCAL_7D.join_gate, [0; 32]);
        assert_ne!(M1_PLAYTEST.join_gate, [0; 32]);
        assert_eq!(M1_LOCAL_7D.end_bell, 1_008);
        assert_eq!(M1_LOCAL_7D.join_close_bell, 756);
        // provinces within ring 16 are funded
        let need = permutation_rules::frontier::geometry::provinces_within(16) as u64
            * crate::layout::AccountKind::Province.rent();
        assert!(M1_LOCAL_7D.pfund_initial >= need);
    }

    #[test]
    fn validation_refuses_each_range() {
        let base = M1_LOCAL_7D;
        let mut bad = base;
        bad.reveal_loaded_limit = 65_536 + 1;
        assert_eq!(bad.validate(), Err("reveal_loaded_limit"));
        let mut bad = base;
        bad.reveal_window = 599;
        assert_eq!(bad.validate(), Err("reveal_window"));
        let mut bad = base;
        bad.r_max = 129;
        assert_eq!(bad.validate(), Err("r_max"));
        let mut bad = base;
        bad.end_bell = 4_033;
        assert_eq!(bad.validate(), Err("join_close_bell/end_bell"));
        let mut bad = base;
        bad.reveal_cu_limit = 15_999;
        assert_eq!(bad.validate(), Err("reveal_cu_limit"));
        let mut d = base.to_bytes();
        d[layout::RSV] = 1;
        assert_eq!(SeasonParams::from_bytes(&d), None);
    }

    #[test]
    fn params_hash_commits_to_both_parts() {
        let a = params_hash(&M1_LOCAL_7D.to_bytes(), &[1, 2, 3]);
        let b = params_hash(&M1_LOCAL_7D.to_bytes(), &[1, 2, 4]);
        let c = params_hash(&M1_PLAYTEST.to_bytes(), &[1, 2, 3]);
        assert!(a != b && a != c);
    }
}
