//! The (instruction, error code) coverage registry of G13 (§3.5, §13.3):
//! for every instruction, the tests that exercise it and the outcomes each
//! asserts. `tests/coverage.rs` (`g13_coverage_*`) checks it.
//!
//! [`covered_by`] is an exhaustive `match` over `Ix` with no `_` arm, so a
//! new instruction does not compile until it names its tests or maps to
//! `Cover::Pending("<unit>")` (accepted unless `RELEASE_CHECK=1`). The
//! per-instruction lists live in one file per area, owned as §11 assigns:
//!
//! | file | owner |
//! |---|---|
//! | `season`, `beacon` | W2-B (W2-A's instructions), then W4-B |
//! | `map`, `citizen` | W3-A |
//! | `holding`, `host`, `reveal` | W3-B |
//! | `clash` | W4-A |
//! | `transit`, `defence` | W4-B |
//!
//! A test is named `file::function` (`tests/<file>.rs`). The guard checks
//! that each named test exists, is not `#[ignore]`d, and that its body
//! (comments stripped) contains each outcome's needle:
//!
//! | `Code` | Must appear in the test body |
//! |---|---|
//! | `Err(X)` | `E::X` (the tests import `FrontierError as E` and use `assert_code`) |
//! | `Lands(needle)` | `needle` and `expect_lands(` |
//! | `Refused(needle)` | `needle` and `assert_refused(` (a refusal the contract pins without a code) |
//! | `Loaded` | `check(` (the `g01_loaded_limit` helper: lands at `L(kind)`, fails charged one page below) |

pub use frontier_abi::error::FrontierError as E;
use frontier_abi::tags::Ix as I;

pub mod beacon;
pub mod citizen;
pub mod clash;
pub mod defence;
pub mod holding;
pub mod host;
pub mod map;
pub mod reveal;
pub mod season;
pub mod transit;

/// An outcome a test asserts.
#[derive(Clone, Copy, Debug)]
pub enum Code {
    /// The program refuses with this error.
    Err(E),
    /// The instruction lands (`needle` is its builder or driver call) and
    /// the test asserts its effects.
    Lands(&'static str),
    /// Refused with a code the contract does not pin (`needle` names the call).
    Refused(&'static str),
    /// `L(kind)` loads and lands, one page below fails charged
    /// (`g01_loaded_limit_*`).
    Loaded,
}

/// How an instruction is covered.
#[derive(Clone, Copy, Debug)]
pub enum Cover {
    /// `file::function` asserts these outcomes.
    Test(&'static str, &'static [Code]),
    /// Not covered yet: the unit that adds the instruction and its tests.
    /// Accepted unless `RELEASE_CHECK=1`.
    Pending(&'static str),
}

pub use Code::{Err, Lands, Loaded, Refused};

/// The tests covering `ix` (every variant, no `_` arm).
pub fn covered_by(ix: I) -> &'static [Cover] {
    match ix {
        I::AnnounceSeason => season::ANNOUNCE_SEASON,
        I::CreateSeason => season::CREATE_SEASON,
        I::InitBeaconLogs => season::INIT_BEACON_LOGS,
        I::InitShards => season::INIT_SHARDS,
        I::ConsumeGenesisSeed => season::CONSUME_GENESIS_SEED,
        I::EndSeason => season::END_SEASON,
        I::CloseSeason => season::CLOSE_SEASON,
        I::AbortSeason => season::ABORT_SEASON,
        I::SetWindowSchedule => season::SET_WINDOW_SCHEDULE,
        I::PostAnchor => beacon::POST_ANCHOR,
        I::PostAnchorMulti => beacon::POST_ANCHOR_MULTI,
        I::PostSeed => beacon::POST_SEED,
        I::PostBeacon => beacon::POST_BEACON,
        I::ArchiveAnchors => beacon::ARCHIVE_ANCHORS,
        I::CloseSeedCache => beacon::CLOSE_SEED_CACHE,
        I::OpenRing => map::OPEN_RING,
        I::ConsumeRingSeed => map::CONSUME_RING_SEED,
        I::OpenProvince => map::OPEN_PROVINCE,
        I::FoldOccupancy => map::FOLD_OCCUPANCY,
        I::CloseProvince => map::CLOSE_PROVINCE,
        I::Join => citizen::JOIN,
        I::SetSession => citizen::SET_SESSION,
        I::SetVigil => citizen::SET_VIGIL,
        I::FileTicket => citizen::FILE_TICKET,
        I::SettleTicket => citizen::SETTLE_TICKET,
        I::ReleaseDormant => citizen::RELEASE_DORMANT,
        I::CloseHolding => citizen::CLOSE_HOLDING,
        I::CloseCitizen => citizen::CLOSE_CITIZEN,
        I::Harvest => holding::HARVEST,
        I::Build => holding::BUILD,
        I::Train => holding::TRAIN,
        I::SettleExplore => holding::SETTLE_EXPLORE,
        I::Muster => host::MUSTER,
        I::Dissolve => host::DISSOLVE,
        I::Garrison => host::GARRISON,
        I::Explore => host::EXPLORE,
        I::DisbandStranded => host::DISBAND_STRANDED,
        I::Depart => host::DEPART,
        I::SettleDeparture => host::SETTLE_DEPARTURE,
        I::Reveal => reveal::REVEAL,
        I::SettleTransit => transit::SETTLE_TRANSIT,
        I::SweepPoolOwed => transit::SWEEP_POOL_OWED,
        I::GatherClash => clash::GATHER_CLASH,
        I::ResolveFromInputs => clash::RESOLVE_FROM_INPUTS,
        I::ResolveClash => clash::RESOLVE_CLASH,
        I::SkipQuiet => clash::SKIP_QUIET,
        I::CloseClashInputs => clash::CLOSE_CLASH_INPUTS,
        I::CloseArrivalDay => clash::CLOSE_ARRIVAL_DAY,
        I::CloseArrivalSlot => clash::CLOSE_ARRIVAL_SLOT,
        I::ClaimDefence => defence::CLAIM_DEFENCE,
    }
}

/// Codes no instruction test has to assert, with the reason.
pub const EXEMPT: &[(E, &str)] = &[
    (
        E::Reserved14,
        "reserved: was SealValid, ProveBadSeal removed (I-44)",
    ),
    (E::TipNotPreset, "relay only, never a program code (§5.4)"),
    (
        E::NotImplemented,
        "dev stubs only; RELEASE_CHECK=1 fails if any path returns it",
    ),
    // W5-A (G13 completion, amendment request §5.4): two codes of the
    // table no M1 path emits. A ticket for a taken site ends as the SETTLE
    // outcome `taken` (I-47), not a refusal; an instruction on an Aborted
    // season is `WrongStatus` (§5.6 status sets). Kept reserved (codes
    // are stable forever).
    (
        E::SiteTaken,
        "reserved in M1: a taken site is the SETTLE outcome `taken` (I-47)",
    ),
    (
        E::Aborted,
        "reserved in M1: an Aborted season is `WrongStatus` (§5.6)",
    ),
];

/// Ignored tests that run only when an environment variable provides what
/// they need: (test, variable).
pub const OPT_IN: &[(&str, &str)] = &[("drill::drill_validator_loaded_data", "PSF_DRILL_RPC")];

/// Every area's ledger of ignored tests waiting for a fix: (test, unit).
pub fn pending() -> Vec<(&'static str, &'static str)> {
    [
        season::PENDING,
        beacon::PENDING,
        map::PENDING,
        citizen::PENDING,
        holding::PENDING,
        host::PENDING,
        reveal::PENDING,
        clash::PENDING,
        transit::PENDING,
        defence::PENDING,
    ]
    .concat()
}
