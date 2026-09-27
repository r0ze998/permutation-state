//! Province (4,096) and its nested records (§5.3).

#![allow(clippy::module_inception)]

/// Province (4,096): `pv‖P,Q`.
pub mod province {
    crate::layout::chained!(b"PSF1PROV", 4_096;
        P @ 64 : "i16" = 2;
        Q @ 66 : "i16" = 2;
        RING @ 68 : "u16" = 2;
        WEDGE @ 70 : "u8" = 1;
        REGION @ 71 : "u8" = 1;
        RESOLVED_NEXT @ 72 : "u32" = 4;
        OPENED_BELL @ 76 : "u32" = 4;
        RELATIONS @ 80 : "u64" = 8;
        LAST_DIGEST @ 88 : "[u8;32]" = 32;
        N_ENTRIES @ 120 : "u8" = 1;
        N_SITES_USED @ 121 : "u8" = 1;
        QUIET_OK @ 122 : "u8" = 1;
        RSV_123 @ 123 : "rsv" = 1;
        ROSTER_EPOCH @ 124 : "u32" = 4;
        TERRAIN @ 128 : "[u8;61]" = 61;
        RESOURCE @ 189 : "[u8;61]" = 61;
        SITES @ 250 : "[u8;12]" = 12;
        SITE_COUNT @ 262 : "u8" = 1;
        RSV_263 @ 263 : "rsv" = 1;
        PASSABLE_MASK @ 264 : "u64" = 8;
        ROUGH_MASK @ 272 : "u64" = 8;
        ROAD_MASK @ 280 : "u64" = 8;
        EXPLORED_MASK @ 288 : "u64" = 8;
        SITE_MIRROR @ 296 : "rec:SiteMirror x12" = 768;
        ENTRIES @ 1064 : "rec:Entry x56" = 2_688;
        RESOLVE_SUMMARY @ 3752 : "rec:ResolveSummary x1" = 32;
        CAMP @ 3784 : "rec:Camp x1" = 16;
        TICKET_COHORTS @ 3800 : "rec:Cohort x8" = 64;
        RSV @ 3864 : "rsv" = 232;
    );
    pub const TILES: usize = 61;
    pub const SITES_N: usize = 12;
    /// Entry storage (I-43): ≤ 48 in the roster, ≤ 8 per faction; the rest
    /// muster-pending or departed.
    /// Taken from the kernel (§3.3: no hand-copied rules constants);
    /// storage_free in `clash::Occupancy` is computed against it.
    pub const ENTRIES_N: usize = permutation_rules::frontier::clash::Occupancy::STORAGE as usize;
    pub const ROSTER_CAP: usize = permutation_rules::frontier::host::PROVINCE_HOST_CAP;
    pub const FACTION_CAP: usize = permutation_rules::frontier::host::FACTION_RESIDENT_CAP;
    pub const COHORTS_N: usize = 8;
    // The byte layout above is literal (x12, x56): tie it to the kernel.
    const _: () = assert!(ENTRIES_N * super::entry::SIZE == 2_688);
    const _: () = assert!(SITES_N == permutation_rules::frontier::clash::MAX_GARRISONS);
    const _: () = assert!(SITES_N * super::site::SIZE == 768);
    /// Bells after which a ticket cohort expires (I-47).
    pub const COHORT_BELLS: u32 = 24;

    /// Offset of `site_mirror[i]`.
    pub const fn site(i: usize) -> usize {
        SITE_MIRROR + i * super::site::SIZE
    }
    /// Offset of `entries[i]`.
    pub const fn entry(i: usize) -> usize {
        ENTRIES + i * super::entry::SIZE
    }
    /// Offset of `ticket_cohorts[i]`.
    pub const fn cohort(i: usize) -> usize {
        TICKET_COHORTS + i * super::cohort::SIZE
    }
}

/// Site mirror (64) in `Province.site_mirror[12]`.
pub mod site {
    crate::layout::fields!(size = 64;
        STATE @ 0 : "u8" = 1;
        FACTION @ 1 : "u8" = 1;
        ORDER @ 2 : "u8" = 1;
        TIER @ 3 : "u8" = 1;
        GEN @ 4 : "u8" = 1;
        RSV_5 @ 5 : "rsv" = 3;
        GARRISON @ 8 : "u32" = 4;
        PEND0_BELL @ 12 : "u32" = 4;
        PEND0_DELTA @ 16 : "i64" = 8;
        PEND1_BELL @ 24 : "u32" = 4;
        RSV_28 @ 28 : "rsv" = 4;
        PEND1_DELTA @ 32 : "i64" = 8;
        WALLS_COMMITTED @ 40 : "u32" = 4;
        WALL_ITEM0_BELL @ 44 : "u32" = 4;
        WALL_ITEM0_DELTA @ 48 : "u32" = 4;
        WALL_ITEM1_BELL @ 52 : "u32" = 4;
        WALL_ITEM1_DELTA @ 56 : "u32" = 4;
        SHIELD_UNTIL_BELL @ 60 : "u32" = 4;
    );
    pub const STATE_FREE: u8 = 0;
    pub const STATE_HOLDING: u8 = 1;
    /// Unused since v1.1 (camps sit on non-site tiles, I-56).
    pub const STATE_UNUSED_CAMP: u8 = 2;
    pub const STATE_RELEASED_FREE: u8 = 3;
    /// Rings 0–1 (I-30).
    pub const STATE_RESERVED: u8 = 4;
    /// Pending garrison slot bell when empty.
    pub const NO_BELL: u32 = u32::MAX;
}

/// Province entry (48) in `Province.entries[56]`; codec in [`crate::entry`].
pub mod entry {
    crate::layout::fields!(size = 48;
        ID @ 0 : "u64" = 8;
        FACTION @ 8 : "u8" = 1;
        UNIT @ 9 : "u8" = 1;
        TILE @ 10 : "u8" = 1;
        STATE @ 11 : "u8" = 1;
        TROOPS @ 12 : "u32" = 4;
        STAMINA_VALUE @ 16 : "u16" = 2;
        DEALT_BPS @ 18 : "u16" = 2;
        STAMINA_BELL @ 20 : "u32" = 4;
        READY_BELL @ 24 : "u32" = 4;
        FROM_BELL @ 28 : "u32" = 4;
        PEND_BELL @ 32 : "u32" = 4;
        PEND_OP @ 36 : "u8" = 1;
        OP_A @ 37 : "u8" = 1;
        OP_B @ 38 : "u16" = 2;
        OP_TROOPS @ 40 : "u32" = 4;
        OP_REF @ 44 : "u32" = 4;
    );
    pub const STATE_FREE: u8 = 0;
    pub const STATE_ROSTER: u8 = 1;
    pub const STATE_MUSTER_PENDING: u8 = 2;
    pub const STATE_DEPARTED: u8 = 3;
    pub const OP_NONE: u8 = 0;
    pub const OP_SPEND: u8 = 1;
    pub const OP_SPLIT: u8 = 2;
    pub const OP_ABSORB: u8 = 3;
    pub const OP_ABSORBED_INTO: u8 = 4;
    pub const OP_LEAVE: u8 = 5;
    /// Bad seal (I-44): troops lost at the settle.
    pub const OP_FORFEIT: u8 = 6;
}

/// Last resolve summary (32).
pub mod summary {
    crate::layout::fields!(size = 32;
        BELL @ 0 : "u32" = 4;
        ENGAGEMENTS @ 4 : "u32" = 4;
        ARRIVALS @ 8 : "u8" = 1;
        DESTROYED @ 9 : "u8" = 1;
        BOUNCED @ 10 : "u8" = 1;
        RSV_11 @ 11 : "rsv" = 1;
        RESOLVER @ 12 : "[u8;8]" = 8;
        RSV_20 @ 20 : "rsv" = 12;
    );
}

/// Barbarian camp (16), I-56.
pub mod camp {
    crate::layout::fields!(size = 16;
        TILE @ 0 : "u8" = 1;
        STATE @ 1 : "u8" = 1;
        RSV_2 @ 2 : "rsv" = 2;
        TROOPS @ 4 : "u32" = 4;
        NEXT_CHECK_DAY @ 8 : "u32" = 4;
        GEN @ 12 : "u32" = 4;
    );
    pub const STATE_NONE: u8 = 0;
    pub const STATE_PRESENT: u8 = 1;
}

/// Ticket cohort (8), I-47.
pub mod cohort {
    crate::layout::fields!(size = 8;
        BELL @ 0 : "u32" = 4;
        FILED @ 4 : "u16" = 2;
        SETTLED @ 6 : "u16" = 2;
    );

    /// A cohort record is free (reusable) when every ticket in it settled or
    /// it expired (`now_bell ≥ bell + 24`). A zeroed record (`filed == 0`) is
    /// free.
    pub const fn is_free(bell: u32, filed: u16, settled: u16, now_bell: u32) -> bool {
        settled >= filed || now_bell >= bell.saturating_add(super::province::COHORT_BELLS)
    }
}
