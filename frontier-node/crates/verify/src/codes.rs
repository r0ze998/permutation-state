//! Finding codes (contract §8.5 table). A code names what is wrong, not the
//! check: several checks may raise the same code (the table's "FAIL codes"
//! column), and a tamper test asserts the code it is named for.

// V1 entity chains
pub const CHAIN_GAP: &str = "ChainGap";
pub const HEAD_MISMATCH: &str = "HeadMismatch";
pub const DUPLICATE_EVENT: &str = "DuplicateEvent";
pub const UNKNOWN_ENTITY: &str = "UnknownEntity";
/// A transaction or record that cannot be read (fail closed, K2).
pub const UNDECODABLE: &str = "Undecodable";
// V2 program and rules
pub const PROGRAM_MISMATCH: &str = "ProgramMismatch";
pub const RULESET_MISMATCH: &str = "RulesetMismatch";
pub const ANNOUNCE_MISMATCH: &str = "AnnounceMismatch";
// V3 randomness
pub const BEACON_SIG_INVALID: &str = "BeaconSigInvalid";
pub const DUPLICATE_ANCHOR: &str = "DuplicateAnchor";
pub const NON_CANONICAL_ADDRESS: &str = "NonCanonicalAddress";
pub const SEED_ROUND_RULE: &str = "SeedRoundRule";
pub const GENESIS_SEED_RULE: &str = "GenesisSeedRule";
pub const RING_SEED_RULE: &str = "RingSeedRule";
// V4 windows
pub const REVEAL_AFTER_CLOSE: &str = "RevealAfterClose";
pub const REVEAL_AFTER_LATCH: &str = "RevealAfterLatch";
// V5 seals
pub const REVEAL_COMMIT_MISMATCH: &str = "RevealCommitMismatch";
pub const VERDICT_DISAGREES_WITH_TLOCK: &str = "VerdictDisagreesWithTlock";
pub const BAD_SEAL_SURVIVED: &str = "BadSealSurvived";
pub const VALID_SEAL_UNREVEALED: &str = "ValidSealUnrevealed";
pub const REVEAL_NEAR_CLOSE: &str = "RevealNearClose";
pub const BAD_SEAL_UNSETTLED: &str = "BadSealUnsettled";
// V6 quotas
pub const QUOTA_SET_MISMATCH: &str = "QuotaSetMismatch";
pub const TRANSIT_MASS_MISMATCH: &str = "TransitMassMismatch";
// V7 replay
pub const CLASH_REPLAY_MISMATCH: &str = "ClashReplayMismatch";
pub const SKIP_NOT_QUIET: &str = "SkipNotQuiet";
pub const SKIP_OVER_ARRIVAL: &str = "SkipOverArrival";
pub const HOLDING_REPLAY_MISMATCH: &str = "HoldingReplayMismatch";
pub const TRANSIT_OUTCOME_MISMATCH: &str = "TransitOutcomeMismatch";
// V8 lag witness
pub const ORIGIN_VALUE_MISMATCH: &str = "OriginValueMismatch";
// V11 land
pub const TICKET_SCORE_MISMATCH: &str = "TicketScoreMismatch";
pub const DISPLACEMENT_RULE: &str = "DisplacementRule";
pub const COHORT_MISMATCH: &str = "CohortMismatch";
pub const TERRAIN_MISMATCH: &str = "TerrainMismatch";
pub const CAMP_MISMATCH: &str = "CampMismatch";
// V12 explore
pub const EXPLORE_ROLL_MISMATCH: &str = "ExploreRollMismatch";
// V13 payments
pub const PAYMENT_MISMATCH: &str = "PaymentMismatch";
pub const DEFENCE_REFUND_MISMATCH: &str = "DefenceRefundMismatch";
pub const CONSERVATION_GAP: &str = "ConservationGap";
/// Data a check needs is absent (UNVERIFIABLE, K2): e.g. no post-states
/// for a clash replay from a public RPC without an archive.
pub const MISSING_DATA: &str = "MissingData";
/// Informational: pre-funded addresses seen, never a failure (V9).
pub const PREFUNDED: &str = "PrefundedAddress";

/// Every FAIL code of §8.5 (the report's legend and a completeness test).
pub const FAIL_CODES: &[&str] = &[
    CHAIN_GAP,
    HEAD_MISMATCH,
    DUPLICATE_EVENT,
    UNKNOWN_ENTITY,
    PROGRAM_MISMATCH,
    RULESET_MISMATCH,
    ANNOUNCE_MISMATCH,
    BEACON_SIG_INVALID,
    DUPLICATE_ANCHOR,
    NON_CANONICAL_ADDRESS,
    SEED_ROUND_RULE,
    GENESIS_SEED_RULE,
    RING_SEED_RULE,
    REVEAL_AFTER_CLOSE,
    REVEAL_AFTER_LATCH,
    REVEAL_COMMIT_MISMATCH,
    VERDICT_DISAGREES_WITH_TLOCK,
    BAD_SEAL_SURVIVED,
    QUOTA_SET_MISMATCH,
    TRANSIT_MASS_MISMATCH,
    CLASH_REPLAY_MISMATCH,
    SKIP_NOT_QUIET,
    SKIP_OVER_ARRIVAL,
    HOLDING_REPLAY_MISMATCH,
    TRANSIT_OUTCOME_MISMATCH,
    ORIGIN_VALUE_MISMATCH,
    TICKET_SCORE_MISMATCH,
    DISPLACEMENT_RULE,
    COHORT_MISMATCH,
    TERRAIN_MISMATCH,
    CAMP_MISMATCH,
    EXPLORE_ROLL_MISMATCH,
    PAYMENT_MISMATCH,
    DEFENCE_REFUND_MISMATCH,
];

/// Warning codes (PASS with a warning).
pub const WARN_CODES: &[&str] = &[
    VALID_SEAL_UNREVEALED,
    REVEAL_NEAR_CLOSE,
    BAD_SEAL_UNSETTLED,
    CONSERVATION_GAP,
    PREFUNDED,
];
