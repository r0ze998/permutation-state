//! Per-bell clash accounts: ArrivalSlot, ArrivalDay, ClashInputs (§5.3).

/// ArrivalSlot (160): `ar‖P,Q,bell,faction,i`.
pub mod arrival_slot {
    crate::layout::short!(b"PSF1ASLT", 160;
        P @ 16 : "i16" = 2;
        Q @ 18 : "i16" = 2;
        BELL @ 20 : "u32" = 4;
        FACTION @ 24 : "u8" = 1;
        I @ 25 : "u8" = 1;
        UNIT @ 26 : "u8" = 1;
        STANCE @ 27 : "u8" = 1;
        TILE @ 28 : "u8" = 1;
        FLAGS @ 29 : "u8" = 1;
        RETREAT_BPS @ 30 : "u16" = 2;
        HOST_ID @ 32 : "u64" = 8;
        CITIZEN_TAG @ 40 : "u64" = 8;
        DEP_MASS @ 48 : "u32" = 4;
        DEALT_BPS @ 52 : "u16" = 2;
        RSV_54 @ 54 : "rsv" = 2;
        BENEFICIARY @ 56 : "[u8;32]" = 32;
        RENT_TO @ 88 : "[u8;32]" = 32;
        EV_SLOT @ 120 : "u64" = 8;
        EV_PRICE @ 128 : "u64" = 8;
        EV_LIMIT @ 136 : "u32" = 4;
        EV_LOADED @ 140 : "u32" = 4;
        CLAIMED @ 144 : "u8" = 1;
        RSV @ 145 : "rsv" = 15;
    );
    /// Settled by SettleTransit but kept open for a defence claim (I-52).
    pub const FLAG_SETTLED: u8 = 1;
    /// Slots per `(province, bell, faction)` (the quota).
    pub const SLOTS_PER_FACTION: u8 = 4;
    /// `retreat_bps` meaning "never retreat" (I-27).
    pub const RETREAT_NEVER: u16 = 0;
}

/// ArrivalDay (96): `ad‖P,Q,day`.
pub mod arrival_day {
    crate::layout::short!(b"PSF1ADAY", 96;
        P @ 16 : "i16" = 2;
        Q @ 18 : "i16" = 2;
        DAY @ 20 : "u32" = 4;
        BITS @ 24 : "[u8;18]" = 18;
        RSV_42 @ 42 : "rsv" = 22;
        RENT_TO @ 64 : "[u8;32]" = 32;
    );

    /// Byte offset and mask of bell `b`'s bit (`b mod 144`).
    pub const fn bit(b: u32) -> (usize, u8) {
        let i = (b % crate::layout::clash::BELLS_PER_DAY) as usize;
        (BITS + i / 8, 1u8 << (i % 8))
    }
}

/// Bells per game day (600-s bells).
pub const BELLS_PER_DAY: u32 = 144;

/// ClashInputs (1,280): `ci‖P,Q,bell`.
pub mod clash_inputs {
    crate::layout::chained!(b"PSF1CLIN", 1_280;
        P @ 64 : "i16" = 2;
        Q @ 66 : "i16" = 2;
        BELL @ 68 : "u32" = 4;
        ARRIVALS_MASK @ 72 : "u32" = 4;
        RSV_76 @ 76 : "rsv" = 4;
        POSTURE_MASK @ 80 : "u64" = 8;
        FLAGS @ 88 : "u8" = 1;
        N_PRESENT @ 89 : "u8" = 1;
        RSV_90 @ 90 : "rsv" = 2;
        SETTLED_MASK @ 92 : "u32" = 4;
        ARRIVALS @ 96 : "rec:ArrivalRecord x24" = 960;
        POSTURES @ 1056 : "[u16;60]" = 120;
        RESOLVER @ 1176 : "[u8;32]" = 32;
        EV_SLOT @ 1208 : "u64" = 8;
        EV_PRICE @ 1216 : "u64" = 8;
        EV_LIMIT @ 1224 : "u32" = 4;
        RESOLVED_TS @ 1228 : "u32" = 4;
        RENT_TO @ 1232 : "[u8;32]" = 32;
        RSV @ 1264 : "rsv" = 16;
    );
    /// No arrivals, proven by the ArrivalDay bit.
    pub const FLAG_NO_ARRIVALS: u8 = 1;
    pub const FLAG_RESOLVED: u8 = 2;
    /// 24 positions: 6 factions × 4 slots.
    pub const POSITIONS: usize = 24;
    pub const ALL_GATHERED: u32 = 0x00FF_FFFF;

    /// Offset of `arrivals[k]`.
    pub const fn arrival(k: usize) -> usize {
        ARRIVALS + k * super::arrival::SIZE
    }
    /// Position of slot `(faction, i)`.
    pub const fn position(faction: u8, i: u8) -> usize {
        faction as usize * 4 + i as usize
    }
}

/// Arrival record (40) in `ClashInputs.arrivals[24]`.
pub mod arrival {
    crate::layout::fields!(size = 40;
        HOST_ID @ 0 : "u64" = 8;
        CITIZEN_TAG @ 8 : "u64" = 8;
        DEP_MASS @ 16 : "u32" = 4;
        TROOPS @ 20 : "u32" = 4;
        STAMINA @ 24 : "u16" = 2;
        RETREAT @ 26 : "u16" = 2;
        DEALT @ 28 : "u16" = 2;
        FACTION @ 30 : "u8" = 1;
        UNIT @ 31 : "u8" = 1;
        TILE @ 32 : "u8" = 1;
        STANCE @ 33 : "u8" = 1;
        PRESENT @ 34 : "u8" = 1;
        FATE @ 35 : "u8" = 1;
        TROOPS_AFTER @ 36 : "u32" = 4;
    );
    pub const FATE_NONE: u8 = 0;
    pub const FATE_STAYS: u8 = 1;
    pub const FATE_WITHDREW: u8 = 2;
    pub const FATE_BOUNCED: u8 = 3;
    pub const FATE_RETREATED: u8 = 4;
    pub const FATE_DESTROYED: u8 = 5;
}
