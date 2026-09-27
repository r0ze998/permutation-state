//! ABI glue: instruction tags, classes, budgets, magics, sizes, error codes,
//! log kinds and the byte layouts of M1 contract v1.1 §5.3–§5.5 and §6.
//!
//! **Integration note (W2-F).** The contract makes `frontier-abi` (W1-E,
//! built in parallel in wave 1) the single home of these tables, and this
//! module becomes `pub use frontier_abi::*` once W1-E is merged. Until then
//! the tables below are transcribed from the contract's normative text so
//! the rest of `fclient` can be written and tested; `abi_tables_match_the
//! contract` pins them, and W2-F's switch must keep every test in this crate
//! green (any disagreement is a contract bug to raise, not to paper over).

/// Instruction tags (§5.5).
pub mod tag {
    pub const ANNOUNCE_SEASON: u8 = 0x08;
    pub const CREATE_SEASON: u8 = 0x01;
    pub const INIT_BEACON_LOGS: u8 = 0x09;
    pub const INIT_SHARDS: u8 = 0x02;
    pub const CONSUME_GENESIS_SEED: u8 = 0x03;
    pub const END_SEASON: u8 = 0x04;
    pub const CLOSE_SEASON: u8 = 0x05;
    pub const ABORT_SEASON: u8 = 0x06;
    pub const SET_WINDOW_SCHEDULE: u8 = 0x07;
    pub const POST_ANCHOR: u8 = 0x10;
    pub const POST_ANCHOR_MULTI: u8 = 0x11;
    pub const POST_SEED: u8 = 0x12;
    pub const POST_BEACON: u8 = 0x13;
    pub const ARCHIVE_ANCHORS: u8 = 0x14;
    pub const CLOSE_SEED_CACHE: u8 = 0x15;
    pub const OPEN_RING: u8 = 0x20;
    pub const CONSUME_RING_SEED: u8 = 0x21;
    pub const OPEN_PROVINCE: u8 = 0x22;
    pub const FOLD_OCCUPANCY: u8 = 0x23;
    pub const CLOSE_PROVINCE: u8 = 0x24;
    pub const JOIN: u8 = 0x30;
    pub const SET_SESSION: u8 = 0x31;
    pub const SET_VIGIL: u8 = 0x32;
    pub const FILE_TICKET: u8 = 0x33;
    pub const SETTLE_TICKET: u8 = 0x34;
    pub const RELEASE_DORMANT: u8 = 0x35;
    pub const CLOSE_HOLDING: u8 = 0x36;
    pub const CLOSE_CITIZEN: u8 = 0x37;
    pub const HARVEST: u8 = 0x40;
    pub const BUILD: u8 = 0x41;
    pub const TRAIN: u8 = 0x42;
    pub const MUSTER: u8 = 0x43;
    pub const DISSOLVE: u8 = 0x44;
    pub const GARRISON: u8 = 0x45;
    pub const EXPLORE: u8 = 0x46;
    pub const SETTLE_EXPLORE: u8 = 0x47;
    pub const DISBAND_STRANDED: u8 = 0x48;
    pub const DEPART: u8 = 0x50;
    pub const REVEAL: u8 = 0x51;
    pub const SETTLE_DEPARTURE: u8 = 0x52;
    /// Reserved: ProveBadSeal was removed in v1.1 (I-44).
    pub const RESERVED_PROVE_BAD_SEAL: u8 = 0x53;
    pub const SETTLE_TRANSIT: u8 = 0x54;
    pub const SWEEP_POOL_OWED: u8 = 0x55;
    pub const GATHER_CLASH: u8 = 0x60;
    pub const RESOLVE_FROM_INPUTS: u8 = 0x61;
    /// Only in `oracle` builds of the program (tests).
    pub const RESOLVE_CLASH: u8 = 0x62;
    pub const SKIP_QUIET: u8 = 0x63;
    pub const CLOSE_CLASH_INPUTS: u8 = 0x64;
    pub const CLOSE_ARRIVAL_DAY: u8 = 0x65;
    pub const CLOSE_ARRIVAL_SLOT: u8 = 0x66;
    pub const CLAIM_DEFENCE: u8 = 0x70;
}

/// Keeper class of an instruction (§5.5, I-21).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    /// Window-closing: escalate to P_def 2.0, pool-eligible (Reveal).
    W,
    /// Delay-only: escalate to P_delay 0.5 from the keeper's own budget.
    D,
    /// Non-critical: fixed low bid.
    N,
    /// Player instruction through the relay at priority 0.
    P,
    /// Operator (season authority).
    O,
    /// Test-only (`oracle` ResolveClash).
    Test,
}

impl Class {
    pub fn letter(self) -> &'static str {
        match self {
            Class::W => "W",
            Class::D => "D",
            Class::N => "N",
            Class::P => "P",
            Class::O => "O",
            Class::Test => "T",
        }
    }
}

/// One row of the §5.5 table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IxInfo {
    pub tag: u8,
    pub name: &'static str,
    pub class: Class,
    /// CU ceiling asserted by the M1 gate (§5.5); for SkipQuiet the base
    /// (60k + 30k per recomputed bell).
    pub cu_budget: u32,
    /// Largest legal transaction, bytes.
    pub tx_max: u16,
    /// Refuses CPI (`NotTopLevel`).
    pub top_level: bool,
}

const fn ix(
    tag: u8,
    name: &'static str,
    class: Class,
    cu_budget: u32,
    tx_max: u16,
    top_level: bool,
) -> IxInfo {
    IxInfo {
        tag,
        name,
        class,
        cu_budget,
        tx_max,
        top_level,
    }
}

/// Every instruction of M1 (§5.5), in tag order. 49 + the test-only
/// ResolveClash = 50 rows.
pub const INSTRUCTIONS: [IxInfo; 50] = [
    ix(0x01, "CreateSeason", Class::O, 70_000, 1_100, false),
    ix(0x02, "InitShards", Class::O, 45_000, 600, false),
    ix(0x03, "ConsumeGenesisSeed", Class::D, 345_000, 760, true),
    ix(0x04, "EndSeason", Class::N, 10_000, 300, false),
    ix(0x05, "CloseSeason", Class::O, 60_000, 1_232, false),
    ix(0x06, "AbortSeason", Class::N, 20_000, 300, false),
    ix(0x07, "SetWindowSchedule", Class::O, 5_000, 200, false),
    ix(0x08, "AnnounceSeason", Class::O, 25_000, 480, false),
    ix(0x09, "InitBeaconLogs", Class::O, 80_000, 900, false),
    ix(0x10, "PostAnchor", Class::D, 345_000, 800, true),
    ix(0x11, "PostAnchorMulti", Class::D, 400_000, 1_232, true),
    ix(0x12, "PostSeed", Class::D, 345_000, 800, true),
    ix(0x13, "PostBeacon", Class::N, 340_000, 760, true),
    ix(0x14, "ArchiveAnchors", Class::D, 60_000, 1_232, false),
    ix(0x15, "CloseSeedCache", Class::N, 6_000, 300, false),
    ix(0x20, "OpenRing", Class::D, 30_000, 600, false),
    ix(0x21, "ConsumeRingSeed", Class::D, 345_000, 760, true),
    ix(0x22, "OpenProvince", Class::D, 220_000, 400, true),
    ix(0x23, "FoldOccupancy", Class::D, 30_000, 1_200, false),
    ix(0x24, "CloseProvince", Class::N, 10_000, 300, false),
    ix(0x30, "Join", Class::P, 25_000, 700, false),
    ix(0x31, "SetSession", Class::P, 6_000, 300, false),
    ix(0x32, "SetVigil", Class::P, 6_000, 250, false),
    ix(0x33, "FileTicket", Class::P, 14_000, 560, false),
    ix(0x34, "SettleTicket", Class::D, 40_000, 900, false),
    ix(0x35, "ReleaseDormant", Class::N, 25_000, 480, false),
    ix(0x36, "CloseHolding", Class::N, 15_000, 330, false),
    ix(0x37, "CloseCitizen", Class::N, 10_000, 300, false),
    ix(0x40, "Harvest", Class::P, 12_000, 320, false),
    ix(0x41, "Build", Class::P, 22_000, 360, false),
    ix(0x42, "Train", Class::P, 15_000, 330, false),
    ix(0x43, "Muster", Class::P, 25_000, 380, false),
    ix(0x44, "Dissolve", Class::P, 25_000, 380, false),
    ix(0x45, "Garrison", Class::P, 25_000, 380, false),
    ix(0x46, "Explore", Class::P, 18_000, 380, false),
    ix(0x47, "SettleExplore", Class::N, 15_000, 400, false),
    ix(0x48, "DisbandStranded", Class::N, 12_000, 300, false),
    ix(0x50, "Depart", Class::P, 15_000, 800, false),
    ix(0x51, "Reveal", Class::W, 26_000, 1_100, true),
    ix(0x52, "SettleDeparture", Class::D, 15_000, 400, false),
    ix(0x54, "SettleTransit", Class::D, 85_000, 1_100, false),
    ix(0x55, "SweepPoolOwed", Class::N, 8_000, 300, false),
    ix(0x60, "GatherClash", Class::D, 40_000, 1_232, true),
    ix(0x61, "ResolveFromInputs", Class::D, 340_000, 460, true),
    ix(0x62, "ResolveClash", Class::Test, 1_400_000, 1_232, true),
    ix(0x63, "SkipQuiet", Class::D, 60_000, 1_232, true),
    ix(0x64, "CloseClashInputs", Class::N, 8_000, 300, false),
    ix(0x65, "CloseArrivalDay", Class::N, 8_000, 300, false),
    ix(0x66, "CloseArrivalSlot", Class::N, 8_000, 300, false),
    ix(0x70, "ClaimDefence", Class::D, 25_000, 1_000, false),
];

/// The §5.5 row of `tag`.
pub fn ix_info(tag: u8) -> Option<&'static IxInfo> {
    INSTRUCTIONS.iter().find(|i| i.tag == tag)
}

/// Largest transaction (wire bytes) Solana accepts.
pub const PACKET: usize = 1_232;
/// Account locks per transaction (mainnet).
pub const LOCK_LIMIT: usize = 64;
/// Heap ceiling asserted at every gated fill (§5.5, I-50).
pub const HEAP_GATE: u32 = 28_672;
/// Heap frame the keeper's retry ladder requests (I-50).
pub const HEAP_FRAME_RETRY: u32 = 262_144;
/// CU ceiling of the keeper's retry ladder (I-50).
pub const CU_MAX: u32 = 1_400_000;

/// Account magics (§4.3).
pub mod magic {
    pub const SEASON: &[u8; 8] = b"PSF1SEAS";
    pub const FRONTIER: &[u8; 8] = b"PSF1FRNT";
    pub const RING_SEED: &[u8; 8] = b"PSF1RING";
    pub const PROVINCE_FUND: &[u8; 8] = b"PSF1PFND";
    pub const JOIN_SHARD: &[u8; 8] = b"PSF1JSHD";
    pub const BEACON_LOG: &[u8; 8] = b"PSF1BLOG";
    pub const DEFENCE_POOL: &[u8; 8] = b"PSF1DPOL";
    pub const CITIZEN: &[u8; 8] = b"PSF1CITZ";
    pub const HOLDING: &[u8; 8] = b"PSF1HOLD";
    pub const PROVINCE: &[u8; 8] = b"PSF1PROV";
    pub const ARRIVAL_SLOT: &[u8; 8] = b"PSF1ASLT";
    pub const ARRIVAL_DAY: &[u8; 8] = b"PSF1ADAY";
    pub const CLASH_INPUTS: &[u8; 8] = b"PSF1CLIN";
    /// Reserved (SealVerdict removed in v1.1).
    pub const SEAL_VERDICT: &[u8; 8] = b"PSF1SVRD";
    pub const BELL_ANCHOR: &[u8; 8] = b"PSF1ANCH";
    pub const SEED_CACHE: &[u8; 8] = b"PSF1SEED";
    pub const ANCHOR_ARCHIVE: &[u8; 8] = b"PSF1ARCH";
    pub const DEFENCE_CLAIM: &[u8; 8] = b"PSF1DCLM";
}

/// Account sizes in bytes (§5.2).
pub mod size {
    pub const SEASON: usize = 2_048;
    pub const SEASON_TOMBSTONE: usize = 128;
    pub const FRONTIER: usize = 512;
    pub const RING_SEED: usize = 128;
    pub const PROVINCE_FUND: usize = 128;
    pub const JOIN_SHARD: usize = 256;
    pub const BEACON_LOG: usize = 128;
    pub const DEFENCE_POOL: usize = 256;
    pub const CITIZEN: usize = 384;
    pub const HOLDING: usize = 1_280;
    pub const PROVINCE: usize = 4_096;
    pub const ARRIVAL_SLOT: usize = 160;
    pub const ARRIVAL_DAY: usize = 96;
    pub const CLASH_INPUTS: usize = 1_280;
    pub const BELL_ANCHOR: usize = 144;
    pub const SEED_CACHE: usize = 144;
    pub const ANCHOR_ARCHIVE: usize = 6_144;
    pub const DEFENCE_CLAIM: usize = 128;
}

/// Rent of an account of `space` bytes at mainnet rates (§4.2):
/// `(128 + space) × 5,080` lamports.
pub const fn rent(space: usize) -> u64 {
    (128 + space as u64) * 5_080
}

/// Counts fixed by the contract.
pub const REGIONS: u8 = 16;
pub const FACTIONS: u8 = 6;
pub const SHARDS_PER_FACTION: u8 = 8;
pub const WEDGES: u8 = 6;
pub const TRANSIT_SLOTS: usize = 4;
pub const ENTRIES: usize = 56;
pub const BELLS_PER_DAY: u32 = 144;
pub const BELL_SECS: i64 = 600;
pub const NEUTRAL: u8 = 6;
/// `PostAnchorMulti` region count, pinned by W2-A's tx-size test (8 or 16);
/// the builder accepts any k up to 16 and the transaction-size check refuses
/// an oversize one.
pub const MULTI_MAX_REGIONS_CEILING: usize = 16;
/// Burned creation bonds go here (CL-24).
pub const INCINERATOR: &str = "1nc1nerator11111111111111111111111111111111";
pub const SYSTEM_PROGRAM: &str = "11111111111111111111111111111111";
pub const COMPUTE_BUDGET_PROGRAM: &str = "ComputeBudget111111111111111111111111111111";
pub const INSTRUCTIONS_SYSVAR: &str = "Sysvar1nstructions1111111111111111111111111";
pub const CLOCK_SYSVAR: &str = "SysvarC1ock11111111111111111111111111111111";
pub const LOADER_V3: &str = "BPFLoaderUpgradeab1e11111111111111111111111";

/// Program error codes (§5.4). Stable forever.
pub const ERRORS: [(u32, &str); 62] = [
    (1, "BadData"),
    (2, "BadAccount"),
    (3, "BadAddress"),
    (4, "Auth"),
    (5, "WrongStatus"),
    (6, "RulesetMismatch"),
    (7, "WrongRound"),
    (8, "NoAnchor"),
    (9, "Crypto"),
    (10, "Capacity"),
    (11, "SiteTaken"),
    (12, "WindowClosed"),
    (13, "TooEarly"),
    (14, "Reserved14"),
    (15, "Kernel"),
    (16, "Archived"),
    (17, "Bucket"),
    (18, "NotTopLevel"),
    (19, "Overflow"),
    (20, "NotOwner"),
    (21, "Insufficient"),
    (22, "QueueFull"),
    (23, "NoTicket"),
    (24, "NotFinal"),
    (25, "TooManyAccounts"),
    (26, "NotResident"),
    (27, "ProvinceFull"),
    (28, "HostBusy"),
    (29, "Cooldown"),
    (30, "TransitState"),
    (31, "ArrivalBell"),
    (32, "Path"),
    (33, "CommitMismatch"),
    (34, "QuotaRefused"),
    (35, "SlotMoved"),
    (36, "NeedArrivalDay"),
    (37, "Shielded"),
    (38, "DepartureUnsettled"),
    (39, "NotGathered"),
    (40, "OutOfOrder"),
    (41, "NotQuiet"),
    (42, "InputsOpen"),
    (43, "NotEligible"),
    (44, "FoldStale"),
    (45, "TicketState"),
    (46, "NotDormant"),
    (47, "Explored"),
    (48, "SessionExpired"),
    (49, "WrongRegion"),
    (50, "Aborted"),
    (51, "TipTooLow"),
    (52, "AlreadyDone"),
    (53, "LatchClosed"),
    (54, "SeedNotReady"),
    (55, "BadPlaintext"),
    (56, "Announce"),
    (57, "ReservedSite"),
    (58, "HostInTransit"),
    (59, "JoinGate"),
    (60, "CohortFull"),
    (61, "TipNotPreset"),
    (99, "NotImplemented"),
];

pub fn error_name(code: u32) -> Option<&'static str> {
    ERRORS.iter().find(|e| e.0 == code).map(|e| e.1)
}

/// Keeper mapping of the codes it acts on (§5.4, offchain P8).
pub mod err {
    pub const SLOT_MOVED: u32 = 35;
    pub const WINDOW_CLOSED: u32 = 12;
    pub const ARCHIVED: u32 = 16;
    pub const LATCH_CLOSED: u32 = 53;
    pub const NO_ANCHOR: u32 = 8;
    pub const SEED_NOT_READY: u32 = 54;
    pub const NOT_GATHERED: u32 = 39;
    pub const ALREADY_DONE: u32 = 52;
    pub const QUOTA_REFUSED: u32 = 34;
    pub const TIP_TOO_LOW: u32 = 51;
    pub const HOST_IN_TRANSIT: u32 = 58;
    /// Land (W3-C): a fold part of an older bell; a ticket whose `k` or
    /// state moved; a holding not dormant (any more).
    pub const FOLD_STALE: u32 = 44;
    pub const TICKET_STATE: u32 = 45;
    pub const NOT_DORMANT: u32 = 46;
    pub const NOT_IMPLEMENTED: u32 = 99;

    /// The window of this write is gone for good: stop retrying.
    pub fn stops_window(code: u32) -> bool {
        matches!(code, WINDOW_CLOSED | ARCHIVED | LATCH_CLOSED)
    }
}

/// PS2 log record kinds (§6).
pub mod kind {
    pub const ANNOUNCE: u8 = 1;
    pub const SEASON_CREATED: u8 = 2;
    pub const GENESIS_SEED: u8 = 3;
    pub const RING_OPEN: u8 = 4;
    pub const RING_SEED: u8 = 5;
    pub const PROVINCE_OPEN: u8 = 6;
    pub const FOLD: u8 = 7;
    pub const SEASON_STATUS: u8 = 8;
    pub const WINDOW: u8 = 9;
    pub const JOIN: u8 = 10;
    pub const SESSION: u8 = 11;
    pub const VIGIL: u8 = 12;
    pub const TICKET: u8 = 13;
    pub const SETTLE: u8 = 14;
    pub const RELEASE: u8 = 15;
    pub const HOLDING_FINAL: u8 = 16;
    pub const HARVEST: u8 = 20;
    pub const BUILD: u8 = 21;
    pub const TRAIN: u8 = 22;
    pub const MUSTER: u8 = 23;
    pub const DISSOLVE: u8 = 24;
    pub const GARRISON: u8 = 25;
    pub const EXPLORE: u8 = 26;
    pub const EXPLORE_RESULT: u8 = 27;
    pub const STRANDED: u8 = 28;
    pub const DEPART: u8 = 30;
    pub const REVEAL: u8 = 31;
    pub const DEPARTURE_SETTLED: u8 = 32;
    /// Reserved (BAD_SEAL removed in v1.1).
    pub const RESERVED_BAD_SEAL: u8 = 33;
    pub const TRANSIT_SETTLED: u8 = 34;
    pub const GATHER: u8 = 40;
    pub const CLASH: u8 = 41;
    pub const SKIP: u8 = 42;
    pub const CAMP: u8 = 43;
    pub const ANCHOR: u8 = 50;
    pub const SEED: u8 = 51;
    pub const BEACON: u8 = 52;
    pub const ARCHIVE: u8 = 53;
    pub const DIVERT: u8 = 60;
    pub const DEFENCE_CLAIM: u8 = 61;
    pub const POOL_SWEEP: u8 = 62;
    pub const CLOSE: u8 = 70;

    /// (kind, name) for every defined record kind.
    pub const ALL: [(u8, &str); 41] = [
        (ANNOUNCE, "ANNOUNCE"),
        (SEASON_CREATED, "SEASON_CREATED"),
        (GENESIS_SEED, "GENESIS_SEED"),
        (RING_OPEN, "RING_OPEN"),
        (RING_SEED, "RING_SEED"),
        (PROVINCE_OPEN, "PROVINCE_OPEN"),
        (FOLD, "FOLD"),
        (SEASON_STATUS, "SEASON_STATUS"),
        (WINDOW, "WINDOW"),
        (JOIN, "JOIN"),
        (SESSION, "SESSION"),
        (VIGIL, "VIGIL"),
        (TICKET, "TICKET"),
        (SETTLE, "SETTLE"),
        (RELEASE, "RELEASE"),
        (HOLDING_FINAL, "HOLDING_FINAL"),
        (HARVEST, "HARVEST"),
        (BUILD, "BUILD"),
        (TRAIN, "TRAIN"),
        (MUSTER, "MUSTER"),
        (DISSOLVE, "DISSOLVE"),
        (GARRISON, "GARRISON"),
        (EXPLORE, "EXPLORE"),
        (EXPLORE_RESULT, "EXPLORE_RESULT"),
        (STRANDED, "STRANDED"),
        (DEPART, "DEPART"),
        (REVEAL, "REVEAL"),
        (DEPARTURE_SETTLED, "DEPARTURE_SETTLED"),
        (TRANSIT_SETTLED, "TRANSIT_SETTLED"),
        (GATHER, "GATHER"),
        (CLASH, "CLASH"),
        (SKIP, "SKIP"),
        (CAMP, "CAMP"),
        (ANCHOR, "ANCHOR"),
        (SEED, "SEED"),
        (BEACON, "BEACON"),
        (ARCHIVE, "ARCHIVE"),
        (DIVERT, "DIVERT"),
        (DEFENCE_CLAIM, "DEFENCE_CLAIM"),
        (POOL_SWEEP, "POOL_SWEEP"),
        (CLOSE, "CLOSE"),
    ];

    pub fn name(k: u8) -> Option<&'static str> {
        ALL.iter().find(|e| e.0 == k).map(|e| e.1)
    }
}

/// Chained entity kinds in a PS2 tail (§6).
pub mod entity {
    pub const SEASON: u8 = 1;
    pub const FRONTIER: u8 = 2;
    pub const JOIN_SHARD: u8 = 3;
    pub const CITIZEN: u8 = 4;
    pub const HOLDING: u8 = 5;
    pub const PROVINCE: u8 = 6;
    pub const CLASH_INPUTS: u8 = 7;
    pub const MAX: u8 = 7;
}

/// Seal verdict codes judged by SettleTransit (§5.3, I-44).
pub mod seal_code {
    pub const VALID: u8 = 0;
    pub const FO_FAILED: u8 = 1;
    pub const BAD_POINT: u8 = 2;
    pub const WRONG_ROUND: u8 = 3;
    pub const COMMIT_MISMATCH: u8 = 4;
    pub const PLAINTEXT_INVALID: u8 = 5;
}

/// Byte offsets of every account layout (§4.3, §5.3). Field names follow
/// the contract; `*_N` constants are record strides and counts.
pub mod layout {
    /// Chained header H (64 B).
    pub mod h {
        pub const MAGIC: usize = 0;
        pub const SEASON_ID: usize = 8;
        pub const LAYOUT_VERSION: usize = 16;
        pub const EVENT_SEQ: usize = 24;
        pub const EVENT_HEAD: usize = 32;
        pub const LEN: usize = 64;
    }
    /// Short header SH (16 B).
    pub mod sh {
        pub const MAGIC: usize = 0;
        pub const SEASON_ID: usize = 8;
        pub const LEN: usize = 16;
    }
    pub mod season {
        pub const STATUS: usize = 64;
        pub const BUMP: usize = 65;
        pub const REGIONS: usize = 66;
        pub const GENESIS_RING: usize = 67;
        pub const R_MAX: usize = 68;
        pub const OFFICE_TERMS_PER_WALLET: usize = 70;
        pub const POSTURES_ENABLED: usize = 71;
        pub const AUTHORITY: usize = 72;
        pub const RULESET_HASH: usize = 104;
        pub const RULES_VERSION: usize = 136;
        pub const PROGRAM_VERSION: usize = 138;
        pub const BELL_SECS: usize = 140;
        pub const GENESIS_TS: usize = 144;
        pub const CREATED_TS: usize = 152;
        pub const JOIN_CLOSE_BELL: usize = 160;
        pub const END_BELL: usize = 164;
        pub const DRAND_GENESIS: usize = 168;
        pub const DRAND_PERIOD: usize = 176;
        pub const NETWORK: usize = 180;
        pub const QUICKNET_PK_HASH: usize = 184;
        pub const REVEAL_WINDOW: usize = 216;
        pub const SEED_MARGIN: usize = 220;
        pub const WINDOW_NEXT: usize = 224;
        pub const WINDOW_FROM_BELL: usize = 228;
        pub const GENESIS_ROUND: usize = 232;
        pub const GENESIS_SEED: usize = 240;
        pub const ARCHIVE_AFTER: usize = 272;
        pub const MIN_LEAD: usize = 276;
        pub const MAX_LEAD: usize = 277;
        pub const TRANSIT_SLOTS: usize = 278;
        pub const MARCH_FEE: usize = 280;
        pub const SEAL_BOND: usize = 288;
        pub const MIN_REVEAL_PRIORITY_MILLI: usize = 296;
        pub const REVEAL_CU_LIMIT: usize = 300;
        pub const BUCKET_RATE_PER_H: usize = 304;
        pub const BUCKET_BURST: usize = 306;
        pub const DEFENCE_CAP_MILLI: usize = 308;
        pub const LATENESS_SLOTS: usize = 312;
        pub const THETA_EARLY_BPS: usize = 316;
        pub const THETA_LATE_BPS: usize = 318;
        pub const THETA_SWITCH_SECS: usize = 320;
        pub const RESERVE_BPS: usize = 324;
        pub const EXTRA_FREE_BPS: usize = 326;
        pub const CLASH_CLOSE_GRACE: usize = 328;
        pub const CAMP_REGROW_BELLS: usize = 332;
        pub const PARAMS_HASH: usize = 336;
        pub const T_CREATE_MIN: usize = 368;
        pub const ANNOUNCED_TS: usize = 376;
        pub const CREATION_BOND: usize = 384;
        pub const PAYOUT_PARAMS_HASH: usize = 392;
        pub const SHADE_AUDITOR: usize = 424;
        pub const DORMANT_AFTER_SECS: usize = 456;
        pub const RELEASE_AFTER_SECS: usize = 460;
        pub const PFUND_INITIAL: usize = 464;
        pub const DPOOL_INITIAL: usize = 472;
        pub const REVEAL_LOADED_LIMIT: usize = 480;
        pub const JOIN_GATE: usize = 484;
        pub const END: usize = 516;
    }
    pub mod frontier {
        pub const RINGS_OPENED: usize = 64;
        pub const LAST_RING_OPEN_BELL: usize = 68;
        pub const LAST_RING_OPEN_TS: usize = 72;
        pub const FOLD_BELL: usize = 80;
        pub const FOLD_PART: usize = 84;
        pub const OPEN_SITES: usize = 88;
        pub const OCCUPIED_SITES: usize = 92;
        pub const PROVINCES_OPENED: usize = 96;
        pub const WEDGE_OPEN: usize = 100;
        pub const WEDGE_OCCUPIED: usize = 124;
        pub const ACC_OCCUPIED: usize = 148;
        pub const ACC_WEDGE: usize = 152;
        pub const END: usize = 176;
    }
    pub mod ring_seed {
        pub const D: usize = 16;
        pub const STATUS: usize = 18;
        pub const OPENED_BELL: usize = 20;
        pub const T_OPEN: usize = 24;
        pub const ROUND: usize = 32;
        pub const SEED: usize = 40;
        pub const PROVINCES_CREATED: usize = 72;
        pub const PAYER: usize = 80;
        pub const END: usize = 112;
    }
    pub mod province_fund {
        pub const WEDGE: usize = 16;
        pub const PROVINCES_OPENED: usize = 20;
        pub const FUNDED_TOTAL: usize = 24;
        pub const SPENT_TOTAL: usize = 32;
        pub const OPEN_SITES: usize = 40;
        pub const PROVINCES_FUNDED: usize = 44;
        pub const END: usize = 48;
    }
    pub mod join_shard {
        pub const FACTION: usize = 64;
        pub const SHARD: usize = 65;
        pub const MEMBERS: usize = 68;
        pub const HOLDINGS: usize = 72;
        pub const FINAL_HOLDINGS: usize = 76;
        pub const HOLDINGS_BY_WEDGE: usize = 80;
        pub const RELEASED: usize = 104;
        pub const END: usize = 108;
    }
    pub mod beacon_log {
        pub const REGION: usize = 16;
        pub const LATEST_ROUND: usize = 24;
        pub const POSTED_TS: usize = 32;
        pub const POSTED_SLOT: usize = 40;
        pub const SIG48: usize = 48;
        pub const BENEFICIARY: usize = 96;
        pub const END: usize = 128;
    }
    pub mod defence_pool {
        pub const PAID_TOTAL: usize = 16;
        pub const DIVERTED_TOTAL: usize = 24;
        pub const PER_BELL_REGION_CAP: usize = 32;
        pub const PER_KEEPER_DAY_CAP: usize = 40;
        pub const CLAIMS: usize = 48;
        pub const END: usize = 56;
    }
    pub mod citizen {
        pub const WALLET: usize = 64;
        pub const SESSION: usize = 96;
        pub const SESSION_EXPIRY: usize = 128;
        pub const FACTION: usize = 136;
        pub const FLAGS: usize = 137;
        pub const HOLDINGS_N: usize = 138;
        pub const EXPLORES_FLOOR_LEFT: usize = 139;
        pub const JOIN_BELL: usize = 140;
        pub const JOIN_SHARD: usize = 144;
        pub const VIGIL_START_MIN: usize = 146;
        pub const VIGIL_NEXT_MIN: usize = 148;
        pub const VIGIL_FROM_TS: usize = 152;
        pub const BUCKET_MILLI: usize = 160;
        pub const BUCKET_T: usize = 164;
        /// 3 × {P i16, Q i16, site u8, gen u8}.
        pub const HOLDING: usize = 168;
        pub const HOLDING_STRIDE: usize = 6;
        pub const OFFICE_TERMS_USED: usize = 186;
        pub const TICKET_BELL: usize = 188;
        /// 3 × {P i16, Q i16, site u8}.
        pub const TICKET_SITES: usize = 192;
        pub const TICKET_SITE_STRIDE: usize = 5;
        pub const TICKET_NEXT: usize = 207;
        pub const CITIZEN_TAG: usize = 208;
        pub const LAST_ACTION_TS: usize = 216;
        pub const WORKS: usize = 224;
        pub const EXPLORES: usize = 232;
        pub const ARRIVALS: usize = 236;
        pub const RENT_PAYER: usize = 240;
        pub const TICKET_ESCROW: usize = 272;
        pub const TICKET_FUNDER: usize = 280;
        pub const END: usize = 312;
        pub const FLAG_JOINED: u8 = 1;
        pub const FLAG_FIRST_FINAL: u8 = 4;
        pub const FLAG_PROVISIONAL: u8 = 8;
        pub const FLAG_REFUGEE: u8 = 16;
    }
    pub mod holding {
        pub const P: usize = 64;
        pub const Q: usize = 66;
        pub const SITE: usize = 68;
        pub const GEN: usize = 69;
        pub const TILE: usize = 70;
        pub const STATE: usize = 71;
        pub const OWNER_CITIZEN: usize = 72;
        pub const TICKET_SCORE: usize = 104;
        pub const FACTION: usize = 112;
        pub const ORDER: usize = 113;
        pub const TIER: usize = 114;
        pub const FLAGS: usize = 115;
        pub const TICKET_BELL: usize = 116;
        pub const FOUNDED_TS: usize = 120;
        pub const FOUNDED_DAY: usize = 128;
        pub const HOST_SEQ: usize = 132;
        pub const LAST_OWNER_ACTION: usize = 136;
        pub const SHIELD_UNTIL: usize = 144;
        /// 8 × Accrual {value, rate, cap, t0, frac} (i64 each).
        pub const STORES: usize = 152;
        pub const ACCRUAL_STRIDE: usize = 40;
        pub const PRODUCTION: usize = 472;
        pub const UPKEEP: usize = 536;
        /// 4 × {done_at i64, kind u8, arg u8, rsv[6], delta i64}.
        pub const QUEUE: usize = 600;
        pub const QUEUE_STRIDE: usize = 24;
        pub const WALLS: usize = 696;
        pub const WALLS_COMMITTED_BEFORE: usize = 704;
        pub const FOOD_SHORTFALL: usize = 712;
        pub const RESERVE: usize = 720;
        pub const DELEGATE: usize = 752;
        pub const TRANSIT: usize = 784;
        pub const TRANSIT_STRIDE: usize = 96;
        pub const EXPLORE: usize = 1_168;
        pub const ESCROW: usize = 1_192;
        pub const RENT_PAYER: usize = 1_200;
        pub const FINAL_TS: usize = 1_232;
        pub const POOL_OWED: usize = 1_240;
        pub const END: usize = 1_248;
        pub const STATE_NONE: u8 = 0;
        pub const STATE_PROVISIONAL: u8 = 1;
        pub const STATE_FINAL: u8 = 2;
        pub const STATE_RELEASED: u8 = 3;
    }
    /// Transit record (96 B) inside the Holding.
    pub mod transit {
        pub const STATE: usize = 0;
        pub const UNIT: usize = 1;
        pub const FACTION: usize = 2;
        pub const ORIGIN_TILE: usize = 3;
        pub const ORIGIN_P: usize = 4;
        pub const ORIGIN_Q: usize = 6;
        pub const HOST_ID: usize = 8;
        pub const DEPART_BELL: usize = 16;
        pub const ARRIVE_BELL: usize = 20;
        pub const DEPART_TS: usize = 24;
        pub const DEP_MASS: usize = 32;
        pub const MARCH_STAMINA: usize = 36;
        pub const DEALT_BPS: usize = 38;
        pub const TROOPS_AFTER: usize = 40;
        pub const STAMINA_AFTER: usize = 44;
        pub const READY_BELL_OFF: usize = 46;
        pub const SEAL_ROOT: usize = 48;
        pub const TIP: usize = 80;
        pub const FLAGS: usize = 88;
        pub const END: usize = 89;
    }
    /// Explore record (24 B) inside the Holding.
    pub mod explore {
        pub const BELL: usize = 0;
        pub const P: usize = 4;
        pub const Q: usize = 6;
        pub const TILES: usize = 8;
        pub const HOST: usize = 10;
        pub const STATE: usize = 18;
        pub const LEN: usize = 24;
    }
    pub mod province {
        pub const P: usize = 64;
        pub const Q: usize = 66;
        pub const RING: usize = 68;
        pub const WEDGE: usize = 70;
        pub const REGION: usize = 71;
        pub const RESOLVED_NEXT: usize = 72;
        pub const OPENED_BELL: usize = 76;
        pub const RELATIONS: usize = 80;
        pub const LAST_OUTCOME_DIGEST: usize = 88;
        pub const N_ENTRIES: usize = 120;
        pub const N_SITES_USED: usize = 121;
        pub const QUIET_OK: usize = 122;
        pub const ROSTER_EPOCH: usize = 124;
        pub const TERRAIN: usize = 128;
        pub const RESOURCE: usize = 189;
        pub const TILES: usize = 61;
        pub const SITES: usize = 250;
        pub const SITE_COUNT: usize = 262;
        pub const PASSABLE_MASK: usize = 264;
        pub const ROUGH_MASK: usize = 272;
        pub const ROAD_MASK: usize = 280;
        pub const EXPLORED_MASK: usize = 288;
        pub const SITE_MIRROR: usize = 296;
        pub const SITE_MIRROR_STRIDE: usize = 64;
        pub const ENTRIES: usize = 1_064;
        pub const ENTRY_STRIDE: usize = 48;
        pub const LAST_RESOLVE: usize = 3_752;
        pub const CAMP: usize = 3_784;
        pub const TICKET_COHORTS: usize = 3_800;
        pub const COHORT_STRIDE: usize = 8;
        pub const COHORTS: usize = 8;
        pub const END: usize = 3_864;
    }
    /// Site mirror (64 B) inside the Province.
    pub mod site {
        pub const STATE: usize = 0;
        pub const FACTION: usize = 1;
        pub const ORDER: usize = 2;
        pub const TIER: usize = 3;
        pub const GEN: usize = 4;
        pub const GARRISON: usize = 8;
        pub const PEND0_BELL: usize = 12;
        pub const PEND0_DELTA: usize = 16;
        pub const PEND1_BELL: usize = 24;
        pub const PEND1_DELTA: usize = 32;
        pub const WALLS_COMMITTED: usize = 40;
        pub const WALL_ITEM0: usize = 44;
        pub const WALL_ITEM1: usize = 52;
        pub const SHIELD_UNTIL_BELL: usize = 60;
        pub const STATE_FREE: u8 = 0;
        pub const STATE_HOLDING: u8 = 1;
        pub const STATE_RELEASED_FREE: u8 = 3;
        pub const STATE_RESERVED: u8 = 4;
    }
    /// Province entry (48 B).
    pub mod entry {
        pub const ID: usize = 0;
        pub const FACTION: usize = 8;
        pub const UNIT: usize = 9;
        pub const TILE: usize = 10;
        pub const STATE: usize = 11;
        pub const TROOPS: usize = 12;
        pub const STAMINA_VALUE: usize = 16;
        pub const DEALT_BPS: usize = 18;
        pub const STAMINA_BELL: usize = 20;
        pub const READY_BELL: usize = 24;
        pub const FROM_BELL: usize = 28;
        pub const PEND_BELL: usize = 32;
        pub const PEND_OP: usize = 36;
        pub const OP_A: usize = 37;
        pub const OP_B: usize = 38;
        pub const OP_TROOPS: usize = 40;
        pub const OP_REF: usize = 44;
        pub const STATE_FREE: u8 = 0;
        pub const STATE_ROSTER: u8 = 1;
        pub const STATE_MUSTER_PENDING: u8 = 2;
        pub const STATE_DEPARTED: u8 = 3;
    }
    pub mod arrival_slot {
        pub const P: usize = 16;
        pub const Q: usize = 18;
        pub const BELL: usize = 20;
        pub const FACTION: usize = 24;
        pub const I: usize = 25;
        pub const UNIT: usize = 26;
        pub const STANCE: usize = 27;
        pub const TILE: usize = 28;
        pub const FLAGS: usize = 29;
        pub const RETREAT_BPS: usize = 30;
        pub const HOST_ID: usize = 32;
        pub const CITIZEN_TAG: usize = 40;
        pub const DEP_MASS: usize = 48;
        pub const DEALT_BPS: usize = 52;
        pub const BENEFICIARY: usize = 56;
        pub const RENT_TO: usize = 88;
        pub const EV_SLOT: usize = 120;
        pub const EV_PRICE: usize = 128;
        pub const EV_LIMIT: usize = 136;
        pub const EV_LOADED: usize = 140;
        pub const CLAIMED: usize = 144;
        pub const END: usize = 145;
    }
    pub mod arrival_day {
        pub const P: usize = 16;
        pub const Q: usize = 18;
        pub const DAY: usize = 20;
        pub const BITS: usize = 24;
        pub const RENT_TO: usize = 64;
        pub const END: usize = 96;
    }
    pub mod clash_inputs {
        pub const P: usize = 64;
        pub const Q: usize = 66;
        pub const BELL: usize = 68;
        pub const ARRIVALS_MASK: usize = 72;
        pub const POSTURE_MASK: usize = 80;
        pub const FLAGS: usize = 88;
        pub const N_PRESENT: usize = 89;
        pub const SETTLED_MASK: usize = 92;
        pub const ARRIVALS: usize = 96;
        pub const ARRIVAL_STRIDE: usize = 40;
        pub const POSTURES: usize = 1_056;
        pub const RESOLVER: usize = 1_176;
        pub const EV_SLOT: usize = 1_208;
        pub const EV_PRICE: usize = 1_216;
        pub const EV_LIMIT: usize = 1_224;
        pub const RESOLVED_TS: usize = 1_228;
        pub const RENT_TO: usize = 1_232;
        pub const END: usize = 1_264;
    }
    /// Arrival record (40 B) inside ClashInputs.
    pub mod arrival {
        pub const HOST_ID: usize = 0;
        pub const CITIZEN_TAG: usize = 8;
        pub const DEP_MASS: usize = 16;
        pub const TROOPS: usize = 20;
        pub const STAMINA: usize = 24;
        pub const RETREAT: usize = 26;
        pub const DEALT: usize = 28;
        pub const FACTION: usize = 30;
        pub const UNIT: usize = 31;
        pub const TILE: usize = 32;
        pub const STANCE: usize = 33;
        pub const PRESENT: usize = 34;
        pub const FATE: usize = 35;
        pub const TROOPS_AFTER: usize = 36;
    }
    pub mod bell_anchor {
        pub const BELL: usize = 16;
        pub const REGION: usize = 20;
        pub const NET: usize = 21;
        pub const ROUND: usize = 24;
        pub const A: usize = 32;
        pub const SLOT: usize = 40;
        pub const SIG48: usize = 48;
        pub const RENT_TO: usize = 96;
        pub const EV_PRICE: usize = 128;
        pub const EV_LIMIT: usize = 136;
        pub const END: usize = 140;
    }
    pub mod seed_cache {
        pub const BELL: usize = 16;
        pub const REGION: usize = 20;
        pub const NONCE: usize = 21;
        pub const ROUND: usize = 24;
        pub const SEED: usize = 32;
        pub const ANCHOR_KEY: usize = 64;
        pub const A: usize = 96;
        pub const SLOT: usize = 104;
        pub const RENT_TO: usize = 112;
        pub const END: usize = 144;
    }
    /// v1.3: one archive per region and half day (`part = bell / 72`).
    pub mod anchor_archive {
        pub const REGION: usize = 16;
        pub const PART: usize = 20;
        pub const TOMBSTONE: usize = 24;
        pub const ARCHIVED: usize = 33;
        pub const ENTRIES: usize = 64;
        /// Bells per archive.
        pub const ENTRIES_N: usize = 72;
        /// {a_off u32, seed [32], sig [48]}.
        pub const ENTRY_STRIDE: usize = 84;
        pub const RENT_TO: usize = 6_112;
        pub const END: usize = 6_144;
    }
    pub mod defence_claim {
        pub const BENEFICIARY: usize = 16;
        pub const DAY: usize = 48;
        pub const CLAIMED: usize = 56;
        pub const COUNT: usize = 64;
        pub const END: usize = 68;
    }
}

/// Season status values (§5.3).
pub mod status {
    pub const ANNOUNCED: u8 = 0;
    pub const CREATED: u8 = 1;
    pub const SEEDED: u8 = 2;
    /// Never stored: Seeded with `now ≥ genesis_ts` is Running.
    pub const RUNNING: u8 = 3;
    pub const ENDED: u8 = 4;
    pub const CLOSED: u8 = 5;
    pub const ABORTED: u8 = 6;
}

// Every layout's last field ends at or before its size (the contract's
// const assertion, §5.3), and the record arrays tile exactly.
const _: () = {
    assert!(layout::season::END <= 1_024);
    assert!(layout::frontier::END <= size::FRONTIER);
    assert!(layout::ring_seed::END <= size::RING_SEED);
    assert!(layout::province_fund::END <= size::PROVINCE_FUND);
    assert!(layout::join_shard::END <= size::JOIN_SHARD);
    assert!(layout::beacon_log::END <= size::BEACON_LOG);
    assert!(layout::defence_pool::END <= size::DEFENCE_POOL);
    assert!(layout::citizen::END <= size::CITIZEN);
    assert!(layout::holding::END <= size::HOLDING);
    assert!(
        layout::holding::TRANSIT + 4 * layout::holding::TRANSIT_STRIDE == layout::holding::EXPLORE
    );
    assert!(layout::transit::END <= layout::holding::TRANSIT_STRIDE);
    assert!(layout::province::END <= size::PROVINCE);
    assert!(
        layout::province::ENTRIES + 56 * layout::province::ENTRY_STRIDE
            == layout::province::LAST_RESOLVE
    );
    assert!(
        layout::province::SITE_MIRROR + 12 * layout::province::SITE_MIRROR_STRIDE
            == layout::province::ENTRIES
    );
    assert!(layout::arrival_slot::END <= size::ARRIVAL_SLOT);
    assert!(layout::arrival_day::END <= size::ARRIVAL_DAY);
    assert!(
        layout::clash_inputs::ARRIVALS + 24 * layout::clash_inputs::ARRIVAL_STRIDE
            == layout::clash_inputs::POSTURES
    );
    assert!(layout::clash_inputs::END <= size::CLASH_INPUTS);
    assert!(layout::bell_anchor::END <= size::BELL_ANCHOR);
    assert!(layout::seed_cache::END <= size::SEED_CACHE);
    assert!(
        layout::anchor_archive::ENTRIES
            + layout::anchor_archive::ENTRIES_N * layout::anchor_archive::ENTRY_STRIDE
            == layout::anchor_archive::RENT_TO
    );
    assert!(layout::defence_claim::END <= size::DEFENCE_CLAIM);
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_tables_match_the_contract() {
        // 49 instructions + the oracle ResolveClash; tags unique; 0x53 reserved.
        let mut tags: Vec<u8> = INSTRUCTIONS.iter().map(|i| i.tag).collect();
        tags.sort();
        tags.dedup();
        assert_eq!(tags.len(), 50);
        assert!(ix_info(tag::RESERVED_PROVE_BAD_SEAL).is_none());
        assert_eq!(ix_info(tag::REVEAL).unwrap().class, Class::W);
        assert_eq!(ix_info(tag::SETTLE_TICKET).unwrap().class, Class::D);
        for t in [
            tag::OPEN_RING,
            tag::CONSUME_RING_SEED,
            tag::OPEN_PROVINCE,
            tag::FOLD_OCCUPANCY,
            tag::CLAIM_DEFENCE,
        ] {
            assert_eq!(
                ix_info(t).unwrap().class,
                Class::D,
                "I-47/48/52 class D: {t:#x}"
            );
        }
        // Error codes unique and 14 kept reserved.
        let mut codes: Vec<u32> = ERRORS.iter().map(|e| e.0).collect();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), ERRORS.len());
        assert_eq!(error_name(52), Some("AlreadyDone"));
        // Rent figures quoted in §5.2.
        assert_eq!(rent(size::SEASON), 11_054_080);
        assert_eq!(rent(size::CITIZEN) + rent(size::HOLDING), 9_753_600);
        assert_eq!(rent(size::ANCHOR_ARCHIVE), 31_861_760);
        assert_eq!(rent(size::PROVINCE), 21_457_920);
    }
}

/// The transcription equals the canonical tables in `frontier-abi`
/// (contract §3.3: hand-copied tables are a review failure unless a test
/// ties them; integ-W1). A dev-dependency only: fclient's runtime keeps its
/// Solana-typed glue.
#[cfg(test)]
mod twin_tests {
    use super::*;
    use frontier_abi::error::FrontierError;
    use frontier_abi::layout::AccountKind;
    use frontier_abi::tags::{Class as AbiClass, Ix};

    #[test]
    fn errors_are_frontier_abis() {
        let abi: Vec<(u32, &str)> = FrontierError::ALL
            .iter()
            .map(|e| (e.code(), e.name()))
            .collect();
        assert_eq!(ERRORS.to_vec(), abi);
    }

    #[test]
    fn instructions_are_frontier_abis() {
        assert_eq!(INSTRUCTIONS.len(), Ix::ALL.len());
        for ix in Ix::ALL {
            let row = ix_info(ix.tag()).unwrap_or_else(|| panic!("{}", ix.name()));
            assert_eq!(row.name, ix.name());
            let class = match ix.class() {
                AbiClass::W => Class::W,
                AbiClass::D => Class::D,
                AbiClass::N => Class::N,
                AbiClass::P => Class::P,
                AbiClass::O => Class::O,
                AbiClass::Test => Class::Test,
            };
            assert_eq!(row.class, class, "{}", ix.name());
            assert_eq!(row.top_level, ix.top_level_only(), "{}", ix.name());
            let b = frontier_abi::budgets::budget(*ix);
            if b.cu_budget == 0 {
                // Recorded, not gated (the oracle ResolveClash): fclient
                // requests the transaction maximum.
                assert_eq!(row.cu_budget, CU_MAX, "{}", ix.name());
            } else {
                assert_eq!(row.cu_budget, b.cu_budget, "{}", ix.name());
            }
            assert_eq!(row.tx_max as u32, b.tx_contract, "{}", ix.name());
        }
    }

    #[test]
    fn magics_and_rent_are_frontier_abis() {
        let ours = [
            magic::SEASON,
            magic::FRONTIER,
            magic::RING_SEED,
            magic::PROVINCE_FUND,
            magic::JOIN_SHARD,
            magic::BEACON_LOG,
            magic::DEFENCE_POOL,
            magic::CITIZEN,
            magic::HOLDING,
            magic::PROVINCE,
            magic::ARRIVAL_SLOT,
            magic::ARRIVAL_DAY,
            magic::CLASH_INPUTS,
            magic::BELL_ANCHOR,
            magic::SEED_CACHE,
            magic::ANCHOR_ARCHIVE,
            magic::DEFENCE_CLAIM,
        ];
        for k in AccountKind::ALL {
            assert!(ours.contains(&&k.magic()), "{k:?}");
            assert_eq!(
                rent(k.size()),
                frontier_abi::layout::rent(k.size()),
                "{k:?}"
            );
        }
    }
}
