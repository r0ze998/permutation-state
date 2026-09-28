//! Player accounts: Citizen (384, v1.1) and Holding (1,280) with their
//! nested records (§5.3).

/// Citizen (384): `ct‖tag15(wallet)`.
pub mod citizen {
    crate::layout::chained!(b"PSF1CITZ", 384;
        WALLET @ 64 : "[u8;32]" = 32;
        SESSION @ 96 : "[u8;32]" = 32;
        SESSION_EXPIRY @ 128 : "i64" = 8;
        FACTION @ 136 : "u8" = 1;
        FLAGS @ 137 : "u8" = 1;
        HOLDINGS_N @ 138 : "u8" = 1;
        EXPLORES_FLOOR_LEFT @ 139 : "u8" = 1;
        JOIN_BELL @ 140 : "u32" = 4;
        JOIN_SHARD @ 144 : "u8" = 1;
        RSV_145 @ 145 : "rsv" = 1;
        VIGIL_START_MIN @ 146 : "u16" = 2;
        VIGIL_NEXT_MIN @ 148 : "u16" = 2;
        RSV_150 @ 150 : "rsv" = 2;
        VIGIL_FROM_TS @ 152 : "i64" = 8;
        BUCKET_MILLI @ 160 : "u32" = 4;
        BUCKET_T @ 164 : "u32" = 4;
        HOLDING @ 168 : "rec:HoldingRef x3" = 18;
        OFFICE_TERMS_USED @ 186 : "u8" = 1;
        RSV_187 @ 187 : "rsv" = 1;
        TICKET_BELL @ 188 : "u32" = 4;
        TICKET_SITES @ 192 : "rec:TicketSite x3" = 15;
        TICKET_NEXT @ 207 : "u8" = 1;
        CITIZEN_TAG @ 208 : "u64" = 8;
        LAST_ACTION_TS @ 216 : "i64" = 8;
        WORKS @ 224 : "u64" = 8;
        EXPLORES @ 232 : "u32" = 4;
        ARRIVALS @ 236 : "u32" = 4;
        RENT_PAYER @ 240 : "[u8;32]" = 32;
        TICKET_ESCROW @ 272 : "u64" = 8;
        TICKET_FUNDER @ 280 : "[u8;32]" = 32;
        RSV @ 312 : "rsv" = 72;
    );
    pub const FLAG_JOINED: u8 = 1;
    pub const FLAG_FIRST_HOLDING_FINAL: u8 = 4;
    pub const FLAG_PROVISIONAL: u8 = 8;
    pub const FLAG_REFUGEE: u8 = 16;
    /// `ticket_bell` with no open ticket.
    pub const NO_TICKET: u32 = u32::MAX;
    /// Floor explorations granted at Join.
    pub const EXPLORES_FLOOR: u8 = 3;
    /// Most holdings a citizen can own (M1 uses 1; 2–3 are M2/M3).
    pub const MAX_HOLDINGS: usize = 3;
    /// Most sites on one ticket.
    pub const MAX_TICKET_SITES: usize = 3;
    /// Longest session validity from its setting (30 days).
    pub const MAX_SESSION_SECS: i64 = 30 * 86_400;
    /// Domain of the Citizen seed tag: `sha256("PSF-CIT" ‖ wallet)[0..15]`.
    pub const TAG_DOMAIN: &[u8] = b"PSF-CIT";
}

/// `{P i16, Q i16, site u8, gen u8}` in `Citizen.holding[3]`.
pub mod holding_ref {
    crate::layout::fields!(size = 6;
        P @ 0 : "i16" = 2;
        Q @ 2 : "i16" = 2;
        SITE @ 4 : "u8" = 1;
        GEN @ 5 : "u8" = 1;
    );
}

/// `{P i16, Q i16, site u8}` in `Citizen.ticket_sites[3]`.
pub mod ticket_site {
    crate::layout::fields!(size = 5;
        P @ 0 : "i16" = 2;
        Q @ 2 : "i16" = 2;
        SITE @ 4 : "u8" = 1;
    );
}

/// Holding (1,280): `ho‖P,Q,site`.
pub mod holding {
    crate::layout::chained!(b"PSF1HOLD", 1_280;
        P @ 64 : "i16" = 2;
        Q @ 66 : "i16" = 2;
        SITE @ 68 : "u8" = 1;
        GEN @ 69 : "u8" = 1;
        TILE @ 70 : "u8" = 1;
        STATE @ 71 : "u8" = 1;
        OWNER_CITIZEN @ 72 : "[u8;32]" = 32;
        TICKET_SCORE @ 104 : "u64" = 8;
        FACTION @ 112 : "u8" = 1;
        ORDER @ 113 : "u8" = 1;
        TIER @ 114 : "u8" = 1;
        FLAGS @ 115 : "u8" = 1;
        TICKET_BELL @ 116 : "u32" = 4;
        FOUNDED_TS @ 120 : "i64" = 8;
        FOUNDED_DAY @ 128 : "u32" = 4;
        HOST_SEQ @ 132 : "u32" = 4;
        LAST_OWNER_ACTION @ 136 : "i64" = 8;
        SHIELD_UNTIL @ 144 : "i64" = 8;
        STORES @ 152 : "rec:Accrual x8" = 320;
        PRODUCTION @ 472 : "[i64;8]" = 64;
        UPKEEP @ 536 : "[i64;8]" = 64;
        QUEUE @ 600 : "rec:QueueItem x4" = 96;
        WALLS @ 696 : "u32" = 4;
        RSV_700 @ 700 : "rsv" = 4;
        WALLS_COMMITTED_BEFORE @ 704 : "i64" = 8;
        FOOD_SHORTFALL @ 712 : "i64" = 8;
        RESERVE @ 720 : "[u32;8]" = 32;
        DELEGATE @ 752 : "[u8;32]" = 32;
        TRANSIT @ 784 : "rec:Transit x4" = 384;
        EXPLORE @ 1168 : "rec:ExploreRecord x1" = 24;
        ESCROW @ 1192 : "u64" = 8;
        RENT_PAYER @ 1200 : "[u8;32]" = 32;
        FINAL_TS @ 1232 : "i64" = 8;
        POOL_OWED @ 1240 : "u64" = 8;
        RSV @ 1248 : "rsv" = 32;
    );
    pub const STATE_NONE: u8 = 0;
    pub const STATE_PROVISIONAL: u8 = 1;
    pub const STATE_FINAL: u8 = 2;
    pub const STATE_RELEASED: u8 = 3;
    pub const FLAG_DORMANT_CACHE: u8 = 1;
    pub const STORES_N: usize = 8;
    pub const QUEUE_N: usize = 4;
    pub const RESERVE_N: usize = 8;
    pub const TRANSIT_N: usize = 4;

    /// Offset of `stores[i]`.
    pub const fn store(i: usize) -> usize {
        STORES + i * super::accrual::SIZE
    }
    /// Offset of `queue[i]`.
    pub const fn queue(i: usize) -> usize {
        QUEUE + i * super::queue_item::SIZE
    }
    /// Offset of `reserve[unit]`.
    pub const fn reserve(unit: usize) -> usize {
        RESERVE + unit * 4
    }
    /// Offset of `transit[i]`.
    pub const fn transit(i: usize) -> usize {
        TRANSIT + i * super::transit::SIZE
    }
}

/// Kernel `Accrual {value, rate, cap, t0, frac}` (40).
pub mod accrual {
    crate::layout::fields!(size = 40;
        VALUE @ 0 : "i64" = 8;
        RATE @ 8 : "i64" = 8;
        CAP @ 16 : "i64" = 8;
        T0 @ 24 : "i64" = 8;
        FRAC @ 32 : "i64" = 8;
    );
}

/// Build queue item (24).
pub mod queue_item {
    crate::layout::fields!(size = 24;
        DONE_AT @ 0 : "i64" = 8;
        KIND @ 8 : "u8" = 1;
        ARG @ 9 : "u8" = 1;
        RSV_10 @ 10 : "rsv" = 6;
        DELTA @ 16 : "i64" = 8;
    );
}

/// Transit record (96) in `Holding.transit[4]`.
pub mod transit {
    crate::layout::fields!(size = 96;
        STATE @ 0 : "u8" = 1;
        UNIT @ 1 : "u8" = 1;
        FACTION @ 2 : "u8" = 1;
        ORIGIN_TILE @ 3 : "u8" = 1;
        ORIGIN_P @ 4 : "i16" = 2;
        ORIGIN_Q @ 6 : "i16" = 2;
        HOST_ID @ 8 : "u64" = 8;
        DEPART_BELL @ 16 : "u32" = 4;
        ARRIVE_BELL @ 20 : "u32" = 4;
        DEPART_TS @ 24 : "i64" = 8;
        DEP_MASS @ 32 : "u32" = 4;
        MARCH_STAMINA @ 36 : "u16" = 2;
        DEALT_BPS @ 38 : "u16" = 2;
        TROOPS_AFTER @ 40 : "u32" = 4;
        STAMINA_AFTER @ 44 : "u16" = 2;
        READY_BELL_OFF @ 46 : "u16" = 2;
        SEAL_ROOT @ 48 : "[u8;32]" = 32;
        TIP @ 80 : "u64" = 8;
        FLAGS @ 88 : "u8" = 1;
        DEST_P @ 89 : "i16" = 2;
        DEST_Q @ 91 : "i16" = 2;
        RSV_93 @ 93 : "rsv" = 3;
    );
    /// Free.
    pub const STATE_FREE: u8 = 0;
    /// Departed; values not settled (SettleDeparture pending).
    pub const STATE_DEPARTED: u8 = 1;
    /// Values settled.
    pub const STATE_SETTLED: u8 = 2;
    /// Values settled; destroyed at origin (troops 0).
    pub const STATE_DESTROYED_AT_ORIGIN: u8 = 3;
    pub const FLAG_FEE_ESCROWED: u8 = 1;
    pub const FLAG_BOND_ESCROWED: u8 = 2;
    /// v1.7 (W4-B F1): a GatherClash recorded this transit at `(DEST_P,
    /// DEST_Q)`; SettleTransit then settles it only against that Province.
    pub const FLAG_GATHERED: u8 = 4;

    /// States in which the host may not act (`HostInTransit`, I-44).
    pub const fn in_transit(state: u8) -> bool {
        matches!(state, 1..=3)
    }
}

/// Explore record (24) in `Holding.explore`.
pub mod explore {
    crate::layout::fields!(size = 24;
        BELL @ 0 : "u32" = 4;
        P @ 4 : "i16" = 2;
        Q @ 6 : "i16" = 2;
        TILES @ 8 : "[u8;2]" = 2;
        HOST @ 10 : "u64" = 8;
        STATE @ 18 : "u8" = 1;
        RSV_19 @ 19 : "rsv" = 5;
    );
    pub const STATE_FREE: u8 = 0;
    pub const STATE_PENDING: u8 = 1;
    /// `tiles[i]` when only one tile is explored.
    pub const NO_TILE: u8 = 0xFF;
}
