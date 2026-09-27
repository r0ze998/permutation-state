//! Season-wide accounts: Season, Frontier, RingSeed, ProvinceFund,
//! JoinShard, BeaconLog, DefencePool (§5.3).

/// Season (2,048), the only PDA: `["season", le64(id)]`, bump stored.
pub mod season {
    crate::layout::chained!(b"PSF1SEAS", 2_048;
        STATUS @ 64 : "u8" = 1;
        BUMP @ 65 : "u8" = 1;
        REGIONS @ 66 : "u8" = 1;
        GENESIS_RING @ 67 : "u8" = 1;
        R_MAX @ 68 : "u16" = 2;
        OFFICE_TERMS_PER_WALLET @ 70 : "u8" = 1;
        POSTURES_ENABLED @ 71 : "u8" = 1;
        AUTHORITY @ 72 : "[u8;32]" = 32;
        RULESET_HASH @ 104 : "[u8;32]" = 32;
        RULES_VERSION @ 136 : "u16" = 2;
        PROGRAM_VERSION @ 138 : "u16" = 2;
        BELL_SECS @ 140 : "u32" = 4;
        GENESIS_TS @ 144 : "i64" = 8;
        CREATED_TS @ 152 : "i64" = 8;
        JOIN_CLOSE_BELL @ 160 : "u32" = 4;
        END_BELL @ 164 : "u32" = 4;
        DRAND_GENESIS @ 168 : "i64" = 8;
        DRAND_PERIOD @ 176 : "u32" = 4;
        NETWORK @ 180 : "u8" = 1;
        RSV_181 @ 181 : "rsv" = 3;
        QUICKNET_PK_HASH @ 184 : "[u8;32]" = 32;
        REVEAL_WINDOW @ 216 : "u32" = 4;
        SEED_MARGIN @ 220 : "u32" = 4;
        WINDOW_NEXT @ 224 : "u32" = 4;
        WINDOW_FROM_BELL @ 228 : "u32" = 4;
        GENESIS_ROUND @ 232 : "u64" = 8;
        GENESIS_SEED @ 240 : "[u8;32]" = 32;
        ARCHIVE_AFTER @ 272 : "u32" = 4;
        MIN_LEAD @ 276 : "u8" = 1;
        MAX_LEAD @ 277 : "u8" = 1;
        TRANSIT_SLOTS @ 278 : "u8" = 1;
        RSV_279 @ 279 : "rsv" = 1;
        MARCH_FEE @ 280 : "u64" = 8;
        SEAL_BOND @ 288 : "u64" = 8;
        MIN_REVEAL_PRIORITY_MILLI @ 296 : "u32" = 4;
        REVEAL_CU_LIMIT @ 300 : "u32" = 4;
        BUCKET_RATE_PER_H @ 304 : "u16" = 2;
        BUCKET_BURST @ 306 : "u16" = 2;
        DEFENCE_CAP_MILLI @ 308 : "u32" = 4;
        LATENESS_SLOTS @ 312 : "u8" = 1;
        RSV_313 @ 313 : "rsv" = 3;
        THETA_EARLY_BPS @ 316 : "u16" = 2;
        THETA_LATE_BPS @ 318 : "u16" = 2;
        THETA_SWITCH_SECS @ 320 : "u32" = 4;
        RESERVE_BPS @ 324 : "u16" = 2;
        EXTRA_FREE_BPS @ 326 : "u16" = 2;
        CLASH_CLOSE_GRACE @ 328 : "u32" = 4;
        CAMP_REGROW_BELLS @ 332 : "u32" = 4;
        PARAMS_HASH @ 336 : "[u8;32]" = 32;
        T_CREATE_MIN @ 368 : "i64" = 8;
        ANNOUNCED_TS @ 376 : "i64" = 8;
        CREATION_BOND @ 384 : "u64" = 8;
        PAYOUT_PARAMS_HASH @ 392 : "[u8;32]" = 32;
        SHADE_AUDITOR @ 424 : "[u8;32]" = 32;
        DORMANT_AFTER_SECS @ 456 : "u32" = 4;
        RELEASE_AFTER_SECS @ 460 : "u32" = 4;
        PFUND_INITIAL @ 464 : "u64" = 8;
        DPOOL_INITIAL @ 472 : "u64" = 8;
        REVEAL_LOADED_LIMIT @ 480 : "u32" = 4;
        JOIN_GATE @ 484 : "[u8;32]" = 32;
        RSV_M2M3 @ 516 : "rsv" = 508;
        RSV_TAIL @ 1024 : "rsv" = 1024;
    );

    /// Size after CloseSeason shrinks it to a tombstone (status Closed).
    pub const TOMBSTONE_SIZE: usize = 128;

    pub const STATUS_ANNOUNCED: u8 = 0;
    pub const STATUS_CREATED: u8 = 1;
    pub const STATUS_SEEDED: u8 = 2;
    /// Never stored: Seeded with `now ≥ genesis_ts` is Running (§5.3).
    pub const STATUS_RUNNING: u8 = 3;
    pub const STATUS_ENDED: u8 = 4;
    pub const STATUS_CLOSED: u8 = 5;
    pub const STATUS_ABORTED: u8 = 6;

    /// `window_from_bell` when no window change is scheduled.
    pub const WINDOW_NONE: u32 = u32::MAX;
    /// drand network id of quicknet.
    pub const NETWORK_QUICKNET: u8 = 2;
    /// PDA seed prefix.
    pub const PDA_PREFIX: &[u8] = b"season";

    /// The effective status (§5.3 note): Seeded with `now ≥ genesis_ts` is
    /// Running; every other stored status is itself.
    pub const fn effective_status(stored: u8, genesis_ts: i64, now: i64) -> u8 {
        if stored == STATUS_SEEDED && now >= genesis_ts {
            STATUS_RUNNING
        } else {
            stored
        }
    }
}

/// Frontier (512): ring openings and the occupancy fold.
pub mod frontier {
    crate::layout::chained!(b"PSF1FRNT", 512;
        RINGS_OPENED @ 64 : "u16" = 2;
        RSV_66 @ 66 : "rsv" = 2;
        LAST_RING_OPEN_BELL @ 68 : "u32" = 4;
        LAST_RING_OPEN_TS @ 72 : "i64" = 8;
        FOLD_BELL @ 80 : "u32" = 4;
        FOLD_PART @ 84 : "u8" = 1;
        RSV_85 @ 85 : "rsv" = 3;
        OPEN_SITES @ 88 : "u32" = 4;
        OCCUPIED_SITES @ 92 : "u32" = 4;
        PROVINCES_OPENED @ 96 : "u32" = 4;
        WEDGE_OPEN @ 100 : "[u32;6]" = 24;
        WEDGE_OCCUPIED @ 124 : "[u32;6]" = 24;
        ACC_OCCUPIED @ 148 : "u32" = 4;
        ACC_WEDGE @ 152 : "[u32;6]" = 24;
        RSV @ 176 : "rsv" = 336;
    );
}

/// RingSeed (128): `rs‖d`.
pub mod ring_seed {
    crate::layout::short!(b"PSF1RING", 128;
        D @ 16 : "u16" = 2;
        STATUS @ 18 : "u8" = 1;
        RSV_19 @ 19 : "rsv" = 1;
        OPENED_BELL @ 20 : "u32" = 4;
        T_OPEN @ 24 : "i64" = 8;
        ROUND @ 32 : "u64" = 8;
        SEED @ 40 : "[u8;32]" = 32;
        PROVINCES_CREATED @ 72 : "u16" = 2;
        RSV_74 @ 74 : "rsv" = 6;
        PAYER @ 80 : "[u8;32]" = 32;
        RSV @ 112 : "rsv" = 16;
    );
    pub const STATUS_REQUESTED: u8 = 1;
    pub const STATUS_SEEDED: u8 = 2;
    /// Domain of genesis ring seeds: `sha256("PSF-RING" ‖ genesis_seed ‖ le16(d))` (I-30).
    pub const GENESIS_RING_DOMAIN: &[u8] = b"PSF-RING";
}

/// ProvinceFund (128), one per wedge: `pf‖w` (I-48). Lamports above rent
/// are the fund.
pub mod province_fund {
    crate::layout::short!(b"PSF1PFND", 128;
        WEDGE @ 16 : "u8" = 1;
        RSV_17 @ 17 : "rsv" = 3;
        PROVINCES_OPENED @ 20 : "u32" = 4;
        FUNDED_TOTAL @ 24 : "u64" = 8;
        SPENT_TOTAL @ 32 : "u64" = 8;
        OPEN_SITES @ 40 : "u32" = 4;
        PROVINCES_FUNDED @ 44 : "u32" = 4;
        RSV @ 48 : "rsv" = 80;
    );
    pub const WEDGES: usize = 6;
}

/// JoinShard (256): `js‖faction,shard`, 8 per faction.
pub mod join_shard {
    crate::layout::chained!(b"PSF1JSHD", 256;
        FACTION @ 64 : "u8" = 1;
        SHARD @ 65 : "u8" = 1;
        RSV_66 @ 66 : "rsv" = 2;
        MEMBERS @ 68 : "u32" = 4;
        HOLDINGS @ 72 : "u32" = 4;
        FINAL_HOLDINGS @ 76 : "u32" = 4;
        HOLDINGS_BY_WEDGE @ 80 : "[u32;6]" = 24;
        RELEASED @ 104 : "u32" = 4;
        RSV @ 108 : "rsv" = 148;
    );
    pub const SHARDS_PER_FACTION: u8 = 8;
    pub const FACTIONS: u8 = 6;
}

/// BeaconLog (128): `bl‖region`, 16 of them.
pub mod beacon_log {
    crate::layout::short!(b"PSF1BLOG", 128;
        REGION @ 16 : "u8" = 1;
        RSV_17 @ 17 : "rsv" = 7;
        LATEST_ROUND @ 24 : "u64" = 8;
        POSTED_TS @ 32 : "i64" = 8;
        POSTED_SLOT @ 40 : "u64" = 8;
        SIG48 @ 48 : "[u8;48]" = 48;
        BENEFICIARY @ 96 : "[u8;32]" = 32;
    );
}

/// DefencePool (256): `dp`. Lamports above rent are the escrow.
pub mod defence_pool {
    crate::layout::short!(b"PSF1DPOL", 256;
        PAID_TOTAL @ 16 : "u64" = 8;
        DIVERTED_TOTAL @ 24 : "u64" = 8;
        PER_BELL_REGION_CAP @ 32 : "u64" = 8;
        PER_KEEPER_DAY_CAP @ 40 : "u64" = 8;
        CLAIMS @ 48 : "u64" = 8;
        RSV @ 56 : "rsv" = 200;
    );
}
