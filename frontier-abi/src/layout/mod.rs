//! Account layouts (M1 contract §4.3, §5.2, §5.3), **frozen after wave 1**
//! (changes only by contract amendment, §11/§16).
//!
//! Every account module holds `SIZE`, `MAGIC` (8 ASCII bytes), `CHAINED`
//! (64-B header H with an event chain, or the 16-B short header SH), one
//! `pub const` offset per field and `FIELDS`, the table of every byte range
//! (reserved ranges included). A const assertion checks each field ends at
//! or before `SIZE`; `tests::layouts_tile_exactly` checks the table tiles
//! `[0, SIZE)` with no gap and no overlap; the vector writer prints every
//! offset (`vectors/layouts.json`), which `web-frontier-codec.test.mjs` and
//! `fclient` check.
//!
//! All integers little-endian; "rsv" is reserved and zero. Troop counts in
//! account state are the kernel's `MilliTroops` (1 troop = 1,000) except
//! `Holding.reserve`, which counts whole trained troops.

pub mod beacon;
pub mod clash;
pub mod player;
pub mod province;
pub mod world;

/// One byte range of a layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: &'static str,
    pub off: usize,
    pub len: usize,
    /// Wire type: `u8`…`i64`, `[u8;N]`, `[u32;N]`, `rec:<Name>xN`, `rsv`.
    pub ty: &'static str,
}

/// Declares the offset constants, the `FIELDS` table and the range
/// assertions of one layout.
macro_rules! fields {
    (size = $size:expr; $( $name:ident @ $off:literal : $ty:literal = $len:expr ;)* ) => {
        pub const SIZE: usize = $size;
        $( pub const $name: usize = $off; )*
        pub const FIELDS: &[$crate::layout::Field] = &[
            $( $crate::layout::Field { name: stringify!($name), off: $off, len: $len, ty: $ty }, )*
        ];
        #[allow(clippy::int_plus_one)]
        const _: () = { $( assert!($off + $len <= $size); )* };
    };
}
pub(crate) use fields;

/// A program account with the chained header H (64 B).
macro_rules! chained {
    ($magic:literal, $size:expr; $($rest:tt)*) => {
        pub const MAGIC: [u8; 8] = *$magic;
        pub const CHAINED: bool = true;
        $crate::layout::fields!(size = $size;
            HDR_MAGIC @ 0 : "[u8;8]" = 8;
            SEASON_ID @ 8 : "u64" = 8;
            LAYOUT_VERSION @ 16 : "u16" = 2;
            RSV_HEADER @ 18 : "rsv" = 6;
            EVENT_SEQ @ 24 : "u64" = 8;
            EVENT_HEAD @ 32 : "[u8;32]" = 32;
            $($rest)*);
    };
}
pub(crate) use chained;

/// A program account with the short header SH (16 B).
macro_rules! short {
    ($magic:literal, $size:expr; $($rest:tt)*) => {
        pub const MAGIC: [u8; 8] = *$magic;
        pub const CHAINED: bool = false;
        $crate::layout::fields!(size = $size;
            HDR_MAGIC @ 0 : "[u8;8]" = 8;
            SEASON_ID @ 8 : "u64" = 8;
            $($rest)*);
    };
}
pub(crate) use short;

/// Chained header H (§4.3).
pub mod header {
    pub const H_SIZE: usize = 64;
    pub const SH_SIZE: usize = 16;
    pub const MAGIC: usize = 0;
    pub const SEASON_ID: usize = 8;
    pub const LAYOUT_VERSION: usize = 16;
    pub const EVENT_SEQ: usize = 24;
    pub const EVENT_HEAD: usize = 32;
}

/// Rent-exempt minimum of an account with `space` data bytes (§4.2):
/// `(128 + space) × 5,080` lamports.
pub const fn rent(space: usize) -> u64 {
    (128 + space as u64) * RENT_PER_BYTE
}

/// Lamports per byte of the rent-exempt minimum (§4.2).
pub const RENT_PER_BYTE: u64 = 5_080;

/// A rent refund of a whole account (≥ this many lamports) never diverts
/// (§4.2, `pay_or_divert`): `rent(0)`, the rent-exempt minimum of a
/// data-less wallet, so the refund alone makes any recipient wallet
/// rent-exempt (contract v1.2 erratum: v1.1's 890,880 was 6,960 × 128,
/// the pre-SIMD-0194 rate, inconsistent with `RENT_PER_BYTE`). The
/// program reads the rent from the Rent sysvar; these constants are for
/// off-chain estimates and tests (`rent_is_the_sysvar_formula`).
pub const WHOLE_ACCOUNT_REFUND_FLOOR: u64 = rent(0);

/// The 17 program account kinds (§0, §5.2), numbered for CLOSE records and
/// tables. `SealVerdict` (removed, I-44) and `PosturePDA` (M3) are not kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum AccountKind {
    Season = 1,
    Frontier = 2,
    RingSeed = 3,
    ProvinceFund = 4,
    JoinShard = 5,
    BeaconLog = 6,
    DefencePool = 7,
    Citizen = 8,
    Holding = 9,
    Province = 10,
    ArrivalSlot = 11,
    ArrivalDay = 12,
    ClashInputs = 13,
    BellAnchor = 14,
    SeedCache = 15,
    AnchorArchive = 16,
    DefenceClaim = 17,
}

impl AccountKind {
    pub const ALL: [AccountKind; 17] = {
        use AccountKind::*;
        [
            Season,
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
            BellAnchor,
            SeedCache,
            AnchorArchive,
            DefenceClaim,
        ]
    };

    pub fn from_u8(v: u8) -> Option<AccountKind> {
        Self::ALL.iter().copied().find(|k| *k as u8 == v)
    }

    pub const fn size(self) -> usize {
        use AccountKind::*;
        match self {
            Season => world::season::SIZE,
            Frontier => world::frontier::SIZE,
            RingSeed => world::ring_seed::SIZE,
            ProvinceFund => world::province_fund::SIZE,
            JoinShard => world::join_shard::SIZE,
            BeaconLog => world::beacon_log::SIZE,
            DefencePool => world::defence_pool::SIZE,
            Citizen => player::citizen::SIZE,
            Holding => player::holding::SIZE,
            Province => province::province::SIZE,
            ArrivalSlot => clash::arrival_slot::SIZE,
            ArrivalDay => clash::arrival_day::SIZE,
            ClashInputs => clash::clash_inputs::SIZE,
            BellAnchor => beacon::bell_anchor::SIZE,
            SeedCache => beacon::seed_cache::SIZE,
            AnchorArchive => beacon::anchor_archive::SIZE,
            DefenceClaim => beacon::defence_claim::SIZE,
        }
    }

    pub const fn magic(self) -> [u8; 8] {
        use AccountKind::*;
        match self {
            Season => world::season::MAGIC,
            Frontier => world::frontier::MAGIC,
            RingSeed => world::ring_seed::MAGIC,
            ProvinceFund => world::province_fund::MAGIC,
            JoinShard => world::join_shard::MAGIC,
            BeaconLog => world::beacon_log::MAGIC,
            DefencePool => world::defence_pool::MAGIC,
            Citizen => player::citizen::MAGIC,
            Holding => player::holding::MAGIC,
            Province => province::province::MAGIC,
            ArrivalSlot => clash::arrival_slot::MAGIC,
            ArrivalDay => clash::arrival_day::MAGIC,
            ClashInputs => clash::clash_inputs::MAGIC,
            BellAnchor => beacon::bell_anchor::MAGIC,
            SeedCache => beacon::seed_cache::MAGIC,
            AnchorArchive => beacon::anchor_archive::MAGIC,
            DefenceClaim => beacon::defence_claim::MAGIC,
        }
    }

    pub const fn chained(self) -> bool {
        use AccountKind::*;
        matches!(
            self,
            Season | Frontier | JoinShard | Citizen | Holding | Province | ClashInputs
        )
    }

    pub const fn fields(self) -> &'static [Field] {
        use AccountKind::*;
        match self {
            Season => world::season::FIELDS,
            Frontier => world::frontier::FIELDS,
            RingSeed => world::ring_seed::FIELDS,
            ProvinceFund => world::province_fund::FIELDS,
            JoinShard => world::join_shard::FIELDS,
            BeaconLog => world::beacon_log::FIELDS,
            DefencePool => world::defence_pool::FIELDS,
            Citizen => player::citizen::FIELDS,
            Holding => player::holding::FIELDS,
            Province => province::province::FIELDS,
            ArrivalSlot => clash::arrival_slot::FIELDS,
            ArrivalDay => clash::arrival_day::FIELDS,
            ClashInputs => clash::clash_inputs::FIELDS,
            BellAnchor => beacon::bell_anchor::FIELDS,
            SeedCache => beacon::seed_cache::FIELDS,
            AnchorArchive => beacon::anchor_archive::FIELDS,
            DefenceClaim => beacon::defence_claim::FIELDS,
        }
    }

    pub const fn rent(self) -> u64 {
        rent(self.size())
    }

    pub const fn name(self) -> &'static str {
        use AccountKind::*;
        match self {
            Season => "Season",
            Frontier => "Frontier",
            RingSeed => "RingSeed",
            ProvinceFund => "ProvinceFund",
            JoinShard => "JoinShard",
            BeaconLog => "BeaconLog",
            DefencePool => "DefencePool",
            Citizen => "Citizen",
            Holding => "Holding",
            Province => "Province",
            ArrivalSlot => "ArrivalSlot",
            ArrivalDay => "ArrivalDay",
            ClashInputs => "ClashInputs",
            BellAnchor => "BellAnchor",
            SeedCache => "SeedCache",
            AnchorArchive => "AnchorArchive",
            DefenceClaim => "DefenceClaim",
        }
    }

    /// Which kind the 8-byte magic names (`PSF1SVRD`, reserved, is none).
    pub fn from_magic(m: &[u8; 8]) -> Option<AccountKind> {
        Self::ALL.iter().copied().find(|k| &k.magic() == m)
    }
}

/// Reserved magic of the removed SealVerdict (I-44). Never allocated.
pub const MAGIC_SEAL_VERDICT_RESERVED: [u8; 8] = *b"PSF1SVRD";

/// The sub-records nested in account layouts, for the vector writer.
pub const RECORDS: &[(&str, usize, &[Field])] = &[
    (
        "HoldingRef",
        player::holding_ref::SIZE,
        player::holding_ref::FIELDS,
    ),
    (
        "TicketSite",
        player::ticket_site::SIZE,
        player::ticket_site::FIELDS,
    ),
    ("Accrual", player::accrual::SIZE, player::accrual::FIELDS),
    (
        "QueueItem",
        player::queue_item::SIZE,
        player::queue_item::FIELDS,
    ),
    ("Transit", player::transit::SIZE, player::transit::FIELDS),
    (
        "ExploreRecord",
        player::explore::SIZE,
        player::explore::FIELDS,
    ),
    ("SiteMirror", province::site::SIZE, province::site::FIELDS),
    ("Entry", province::entry::SIZE, province::entry::FIELDS),
    (
        "ResolveSummary",
        province::summary::SIZE,
        province::summary::FIELDS,
    ),
    ("Camp", province::camp::SIZE, province::camp::FIELDS),
    ("Cohort", province::cohort::SIZE, province::cohort::FIELDS),
    (
        "ArrivalRecord",
        clash::arrival::SIZE,
        clash::arrival::FIELDS,
    ),
    (
        "ArchiveEntry",
        beacon::archive_entry::SIZE,
        beacon::archive_entry::FIELDS,
    ),
];

/// Reads the 8-byte magic and season id of any program account.
pub fn read_short_header(d: &[u8]) -> Option<([u8; 8], u64)> {
    Some((crate::bytes::rd_arr(d, 0)?, crate::bytes::rd_u64(d, 8)?))
}

/// Writes a fresh chained header H (seq 0, zero head) or short header SH.
pub fn write_header(d: &mut [u8], kind: AccountKind, season_id: u64) -> bool {
    let mut ok = crate::bytes::wr_arr(d, 0, &kind.magic());
    ok &= crate::bytes::wr_u64(d, 8, season_id);
    if kind.chained() {
        ok &= crate::bytes::wr_u16(d, header::LAYOUT_VERSION, crate::ABI_VERSION);
        ok &= crate::bytes::wr_arr(d, 18, &[0u8; 6]);
        ok &= crate::bytes::wr_u64(d, header::EVENT_SEQ, 0);
        ok &= crate::bytes::wr_arr(d, header::EVENT_HEAD, &[0u8; 32]);
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rent_is_the_sysvar_formula() {
        assert_eq!(rent(0), 650_240);
        assert_eq!(WHOLE_ACCOUNT_REFUND_FLOOR, rent(0));
        // Every program account's refund clears the floor.
        for k in AccountKind::ALL {
            assert!(k.rent() >= WHOLE_ACCOUNT_REFUND_FLOOR, "{k:?}");
        }
    }

    fn tiles(name: &str, size: usize, fields: &[Field]) {
        let mut at = 0;
        for f in fields {
            assert_eq!(f.off, at, "{name}.{} starts at {} not {at}", f.name, f.off);
            assert!(f.len > 0, "{name}.{} is empty", f.name);
            at += f.len;
        }
        assert_eq!(at, size, "{name} fields end at {at}, size {size}");
    }

    #[test]
    fn layouts_tile_exactly() {
        for k in AccountKind::ALL {
            tiles(k.name(), k.size(), k.fields());
            let hdr = if k.chained() {
                header::H_SIZE
            } else {
                header::SH_SIZE
            };
            assert!(
                k.fields().iter().any(|f| f.off == hdr),
                "{} body starts after its header",
                k.name()
            );
        }
        for (name, size, fields) in RECORDS {
            tiles(name, *size, fields);
        }
    }

    #[test]
    fn sizes_and_rent_are_the_contract_table() {
        use AccountKind::*;
        let table: [(AccountKind, usize, u64); 17] = [
            (Season, 2_048, 11_054_080),
            (Frontier, 512, 3_251_200),
            (RingSeed, 128, 1_300_480),
            (ProvinceFund, 128, 1_300_480),
            (JoinShard, 256, 1_950_720),
            (BeaconLog, 128, 1_300_480),
            (DefencePool, 256, 1_950_720),
            (Citizen, 384, 2_600_960),
            (Holding, 1_280, 7_152_640),
            (Province, 4_096, 21_457_920),
            (ArrivalSlot, 160, 1_463_040),
            (ArrivalDay, 96, 1_137_920),
            (ClashInputs, 1_280, 7_152_640),
            (BellAnchor, 144, 1_381_760),
            (SeedCache, 144, 1_381_760),
            (AnchorArchive, 6_144, 31_861_760),
            (DefenceClaim, 128, 1_300_480),
        ];
        for (k, size, r) in table {
            assert_eq!(k.size(), size, "{}", k.name());
            assert_eq!(k.rent(), r, "{}", k.name());
        }
        // Per-player refundable rent (I-31, I-47).
        assert_eq!(Citizen.rent() + Holding.rent(), 9_753_600);
    }

    #[test]
    fn magics_are_distinct_and_named() {
        for a in AccountKind::ALL {
            assert_eq!(&a.magic()[..4], b"PSF1");
            assert_eq!(AccountKind::from_magic(&a.magic()), Some(a));
            assert_eq!(AccountKind::from_u8(a as u8), Some(a));
            assert_ne!(a.magic(), MAGIC_SEAL_VERDICT_RESERVED);
        }
        assert_eq!(AccountKind::from_magic(&MAGIC_SEAL_VERDICT_RESERVED), None);
    }
}
