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
    /// The Reveal that wrote this slot's evidence also created the
    /// ArrivalDay (`fees::Evidence::created_day`: one more write lock in
    /// the refund's cost). Set by Reveal, cleared by a displacement with
    /// the other `ev_*` fields (contract v1.2 §5.3, integ-W1 review).
    pub const FLAG_CREATED_DAY: u8 = 2;
    /// Slots per `(province, bell, faction)` (the quota).
    pub const SLOTS_PER_FACTION: u8 = 4;
    /// `retreat_bps` meaning "never retreat" (I-27).
    pub const RETREAT_NEVER: u16 = 0;

    /// The defence-refund evidence of a slot's bytes, as
    /// `fees::defence_refund` takes it (ClaimDefence and the verifier read
    /// it the same way; `created_day` from [`FLAG_CREATED_DAY`]).
    pub fn evidence(d: &[u8]) -> Option<permutation_rules::frontier::fees::Evidence> {
        use crate::bytes::{rd_u32, rd_u64, rd_u8};
        Some(permutation_rules::frontier::fees::Evidence {
            price_micro: rd_u64(d, EV_PRICE)?,
            limit: rd_u32(d, EV_LIMIT)?,
            loaded: rd_u32(d, EV_LOADED)?,
            created_day: rd_u8(d, FLAGS)? & FLAG_CREATED_DAY != 0,
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn evidence_reads_the_created_day_flag() {
            let mut d = [0u8; SIZE];
            d[EV_PRICE..EV_PRICE + 8].copy_from_slice(&7u64.to_le_bytes());
            d[EV_LIMIT..EV_LIMIT + 4].copy_from_slice(&26_000u32.to_le_bytes());
            d[EV_LOADED..EV_LOADED + 4].copy_from_slice(&(1u32 << 20).to_le_bytes());
            d[FLAGS] = FLAG_SETTLED;
            let e = evidence(&d).unwrap();
            assert_eq!((e.price_micro, e.limit, e.loaded), (7, 26_000, 1 << 20));
            assert!(!e.created_day);
            d[FLAGS] = FLAG_SETTLED | FLAG_CREATED_DAY;
            assert!(evidence(&d).unwrap().created_day);
            assert!(evidence(&d[..100]).is_none());
            assert_eq!(FLAG_SETTLED & FLAG_CREATED_DAY, 0, "distinct bits");
        }
    }
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
