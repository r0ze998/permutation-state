//! The coverage tables `tests/coverage.rs` checks: for every instruction,
//! the tests that exercise it and the outcomes each asserts.
//!
//! `covered_by` is an exhaustive `match` over `ChainInstruction` with no
//! `_` arm, so a new instruction does not compile until it names its tests
//! (or maps to `Cover::Pending("WPxx")`, accepted unless `RELEASE_CHECK=1`).
//! The per-instruction lists live in one file per area, next to that area's
//! `PENDING` ledger (the ignored tests waiting for a fix, and the WP that
//! un-ignores them).
//!
//! A test is named `file::function` (`tests/<file>.rs`). The guard checks
//! that each named test exists, is not ignored, and that its body (comments
//! stripped) contains each listed outcome:
//!
//! | `Code` | Must appear in the test body |
//! |---|---|
//! | `Chain(X)` | `E::X` (the tests import `ChainError as E` and use `assert_err`) |
//! | `Token` | `assert_token_err(` |
//! | `Program(name)` | `name` (e.g. `"IncorrectProgramId"` with `assert_program_err`) |
//! | `Lands(needle)` | `needle` (a builder or driver call) and `.expect(` or `.unwrap()` in the same statement |

use crate::error::ChainError as E;
use crate::instruction::ChainInstruction as I;

pub mod delegation;
pub mod escape;
pub mod genesis;
pub mod play;
pub mod registration;
pub mod roster;
pub mod settlement;

/// An outcome a test asserts.
#[derive(Clone, Copy, Debug)]
pub enum Code {
    /// The program refuses with this error.
    Chain(E),
    /// The SPL Token program refuses a transfer the program asked for.
    Token,
    /// A non-custom `InstructionError` of the program, by name.
    Program(&'static str),
    /// The positive path: the instruction lands (`needle` is its builder or
    /// driver call) and the test asserts its effects.
    Lands(&'static str),
}

/// How an instruction is covered.
#[derive(Clone, Copy, Debug)]
pub enum Cover {
    /// `file::function` asserts these outcomes.
    Test(&'static str, &'static [Code]),
    /// Not covered yet: the WP that adds the instruction's tests. Accepted
    /// unless `RELEASE_CHECK=1`.
    Pending(&'static str),
}

pub use Code::{Chain, Lands, Program, Token};

/// The tests covering `ix` (every variant, no `_` arm).
pub fn covered_by(ix: &I) -> &'static [Cover] {
    match ix {
        I::CreateSeason { .. } => registration::CREATE_SEASON,
        I::AllocWorld { .. } => registration::ALLOC_WORLD,
        I::Register { .. } => registration::REGISTER,
        I::StartSeason => genesis::START_SEASON,
        I::GenesisStep { .. } => genesis::GENESIS_STEP,
        I::Delegate { .. } => delegation::DELEGATE,
        I::SubmitOrders { .. } => play::SUBMIT_ORDERS,
        I::ResolveTick { .. } => play::RESOLVE_TICK,
        I::Commit => delegation::COMMIT,
        I::CommitAndUndelegate => delegation::COMMIT_AND_UNDELEGATE,
        I::FinishSeason => settlement::FINISH_SEASON,
        I::Claim => settlement::CLAIM,
        I::UndelegatePart { .. } => delegation::UNDELEGATE_PART,
        I::UpdateMember { .. } => registration::UPDATE_MEMBER,
        I::AllocNation { .. } => registration::ALLOC_NATION,
        I::SeatMembers => genesis::SEAT_MEMBERS,
        I::OpenGovernment => genesis::OPEN_GOVERNMENT,
        I::SubmitGov { .. } => play::SUBMIT_GOV,
        I::WithdrawOps => settlement::WITHDRAW_OPS,
        I::LogTickInput { .. } => play::LOG_TICK_INPUT,
        I::CommitPart { .. } => delegation::COMMIT_PART,
        I::CloseCommits => play::CLOSE_COMMITS,
        I::CommitOrders { .. } => play::COMMIT_ORDERS,
        I::RevealOrders { .. } => play::REVEAL_ORDERS,
        I::RevealRoster { .. } => roster::REVEAL_ROSTER,
        I::AnchorTalk { .. } => play::ANCHOR_TALK,
        // The v9 instructions (tags 26–36): the wire contract is in; each
        // handler and its tests land with the unit named (wave 3), which
        // replaces its arm with its area's table.
        I::StartClock => play::START_CLOCK,
        I::PostBond { .. } => registration::POST_BOND,
        I::FreezeTick => play::FREEZE_TICK,
        I::ConsumeTickRandomness { .. } => play::CONSUME_TICK_RANDOMNESS,
        I::RetryTickRandomness => play::RETRY_TICK_RANDOMNESS,
        I::ConsumeSeasonSeed { .. } => genesis::CONSUME_SEASON_SEED,
        I::RetrySeasonSeed => genesis::RETRY_SEASON_SEED,
        I::Abort => escape::ABORT,
        I::RequestUndelegation { .. } => escape::REQUEST_UNDELEGATION,
        I::RollbackUndelegation { .. } => escape::ROLLBACK_UNDELEGATION,
        I::CloseSeasonAccounts { .. } => escape::CLOSE_SEASON_ACCOUNTS,
    }
}

/// The DLP's undelegation callback (a raw discriminator, not a variant).
pub const CALLBACK: &[Cover] = delegation::CALLBACK;

/// Data that is no instruction at all.
pub const NOT_AN_INSTRUCTION: &[Cover] = &[Cover::Test(
    "registration::garbage_instruction_data_is_refused",
    &[Chain(E::InvalidInstruction)],
)];

/// Errors no test has to assert, with the reason. Every code is reachable:
/// the v9 codes (34–44) are listed only until the unit named adds the
/// program behaviour and a covering test, and removes its line.
pub const EXEMPT: &[(E, &str)] = &[
    (E::UndelegationOrder, "PENDING WP02 (unit P4)"),
    (E::AlreadyDelegated, "PENDING WP02/WP14 (unit P4)"),
    (E::DelegationOrder, "PENDING WP14 (unit P4)"),
    (E::WrongValidator, "PENDING WP14/WP15 (unit P4)"),
];

/// Ignored tests that run only when an environment variable provides what
/// they need: (test, variable).
pub const OPT_IN: &[(&str, &str)] = &[
    ("budget::finish_after_played_season_at_the_cap", "SVM_HEAVY"),
    (
        "escape_hatches::rollback_restores_last_finalized_bytes",
        "DLP_SO",
    ),
    (
        "escape_hatches::an_owner_request_blocks_the_plain_undelegate",
        "DLP_SO",
    ),
];

/// Every area's ledger of ignored tests waiting for a fix: (test, "WPxx").
pub fn pending() -> Vec<(&'static str, &'static str)> {
    [
        registration::PENDING,
        genesis::PENDING,
        play::PENDING,
        delegation::PENDING,
        roster::PENDING,
        settlement::PENDING,
        escape::PENDING,
    ]
    .concat()
}
