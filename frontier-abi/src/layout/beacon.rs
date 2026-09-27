//! Beacon and defence accounts: BellAnchor, SeedCache, AnchorArchive,
//! DefenceClaim (§5.3).

/// BellAnchor (144): `an‖bell,region`.
pub mod bell_anchor {
    crate::layout::short!(b"PSF1ANCH", 144;
        BELL @ 16 : "u32" = 4;
        REGION @ 20 : "u8" = 1;
        NET @ 21 : "u8" = 1;
        RSV_22 @ 22 : "rsv" = 2;
        ROUND @ 24 : "u64" = 8;
        A @ 32 : "i64" = 8;
        SLOT @ 40 : "u64" = 8;
        SIG48 @ 48 : "[u8;48]" = 48;
        RENT_TO @ 96 : "[u8;32]" = 32;
        EV_PRICE @ 128 : "u64" = 8;
        EV_LIMIT @ 136 : "u32" = 4;
        RSV @ 140 : "rsv" = 4;
    );
}

/// SeedCache (144): `sd‖bell,region,nonce`.
pub mod seed_cache {
    crate::layout::short!(b"PSF1SEED", 144;
        BELL @ 16 : "u32" = 4;
        REGION @ 20 : "u8" = 1;
        NONCE @ 21 : "u8" = 1;
        RSV_22 @ 22 : "rsv" = 2;
        ROUND @ 24 : "u64" = 8;
        SEED @ 32 : "[u8;32]" = 32;
        ANCHOR_KEY @ 64 : "[u8;32]" = 32;
        A @ 96 : "i64" = 8;
        SLOT @ 104 : "u64" = 8;
        RENT_TO @ 112 : "[u8;32]" = 32;
    );
    /// Domain of bell seeds: `sha256("PSF-SEED-v1" ‖ net ‖ be64(round) ‖ sig96)` (SP-V2).
    pub const SEED_DOMAIN: &[u8] = b"PSF-SEED-v1";
}

/// AnchorArchive (12,192, v1.1): `aa‖region,day`.
pub mod anchor_archive {
    crate::layout::short!(b"PSF1ARCH", 12_192;
        REGION @ 16 : "u8" = 1;
        RSV_17 @ 17 : "rsv" = 3;
        DAY @ 20 : "u32" = 4;
        TOMBSTONE @ 24 : "[u8;18]" = 18;
        ARCHIVED @ 42 : "[u8;18]" = 18;
        RSV_60 @ 60 : "rsv" = 4;
        ENTRIES @ 64 : "rec:ArchiveEntry x144" = 12_096;
        RENT_TO @ 12160 : "[u8;32]" = 32;
    );
    pub const ENTRIES_N: usize = 144;
    /// Seconds after `A` an anchor may be archived (default `archive_after`).
    pub const ARCHIVE_AFTER_DEFAULT: u32 = 172_800;

    /// Offset of `entries[b mod 144]`.
    pub const fn entry(b: u32) -> usize {
        ENTRIES + (b % 144) as usize * super::archive_entry::SIZE
    }
    /// Byte offset and mask of bell `b` in a 144-bit bitmap starting at `base`.
    pub const fn bit(base: usize, b: u32) -> (usize, u8) {
        let i = (b % 144) as usize;
        (base + i / 8, 1u8 << (i % 8))
    }
}

/// Archive entry (84): `{a_off u32, seed [32], sig [48]}` (I-44).
pub mod archive_entry {
    crate::layout::fields!(size = 84;
        A_OFF @ 0 : "u32" = 4;
        SEED @ 4 : "[u8;32]" = 32;
        SIG @ 36 : "[u8;48]" = 48;
    );
}

/// DefenceClaim (128): `dc‖keeper_tag8,day`.
pub mod defence_claim {
    crate::layout::short!(b"PSF1DCLM", 128;
        BENEFICIARY @ 16 : "[u8;32]" = 32;
        DAY @ 48 : "u32" = 4;
        RSV_52 @ 52 : "rsv" = 4;
        CLAIMED @ 56 : "u64" = 8;
        COUNT @ 64 : "u32" = 4;
        RSV @ 68 : "rsv" = 60;
    );
    /// Claim grace after the reveal close (I-52), in bells.
    pub const CLAIM_GRACE_BELLS: u32 = 6;
    /// Most slots per ClaimDefence.
    pub const MAX_SLOTS: usize = 6;
}
