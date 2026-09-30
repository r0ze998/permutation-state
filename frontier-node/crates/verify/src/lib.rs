//! `verify-core`: the Frontier verifier v2 (M1 contract §8.5, offchain
//! design §10, W4-D).
//!
//! Input: the program id, the season id, the pinned drand key, the ruleset
//! hash expected (and optionally the `.so` hash), the season's
//! transactions in feed order (failed ones included) and the **final
//! account states at a pinned slot** ([`input::Input`]). Every replayed
//! chain must reach the final on-chain head (K4); a transaction or record
//! that cannot be read is a FAIL (K2). Exit codes: 0 PASS, 1 FAIL, 2
//! cannot verify.
//!
//! | module | check |
//! |---|---|
//! | [`checks::v1_chains`] | V1 entity chains, final heads, duplicates, K4 state |
//! | [`checks::v2_program`] | V2 program id, `.so` hash, ruleset hash, announce → create |
//! | [`checks::v3_random`] | V3 anchors, caches, beacons, genesis and ring rounds (blstrs) |
//! | [`checks::v4_windows`] | V4 reveal window and latch |
//! | [`checks::v5_seals`] | V5 seals opened with the stock `tlock` crate; verdicts |
//! | [`checks::v6_quotas`] | V6 arrival quotas, departure masses |
//! | [`checks::v7_replay`] | V7 clashes, quiet skips (every bell, [`skip`]), transit outcomes, holdings (owner actions replayed, [`holding`]) |
//! | [`checks::v8_lag`] | V8 origin values (lag witness) |
//! | [`checks::v9_addresses`] | V9 canonical addresses, pre-funded inits |
//! | [`checks::v11_land`] | V11 tickets, cohorts, terrain, camps |
//! | [`checks::v12_explore`] | V12 explore rolls |
//! | [`checks::v13_payments`] | V13 payments and defence refunds |
//! | [`report`] | V10 `report.json` + `report.md` |
//!
//! Cargo features `mutate-v<n>` disable one check each (the checks of the
//! checks: the tamper of that check must then PASS; `mutate.sh`).
//! [`fixture`] builds the synthetic march season; [`tamper`] holds T1–T22,
//! the extra checks of the checks and the suite over any one run
//! ([`tamper::run_suite`], `frontier-verify tamper`, W5-D).

pub mod checks;
pub mod clash_input;
pub mod codes;
pub mod facts;
pub mod fixture;
pub mod holding;
pub mod input;
pub mod report;
pub mod skip;
pub mod tamper;
pub mod world;

use serde_json::Value;

pub use input::{Config, Input};

/// The `mutate-*` feature this build was compiled with, if any (wave-5
/// review of W5-D): such a build has a check disabled and is **not a
/// verifier**. Its report says so, and `frontier-verify` refuses to run
/// (exit 2) unless [`ALLOW_MUTATED_ENV`] is `1` (the checks of the checks
/// set it).
pub fn mutated() -> Option<&'static str> {
    macro_rules! m {
        ($($f:literal),*) => {{ $( if cfg!(feature = $f) { return Some($f); } )* None }};
    }
    m!(
        "mutate-v1",
        "mutate-v2",
        "mutate-v3",
        "mutate-v4",
        "mutate-v5",
        "mutate-v6",
        "mutate-v7",
        "mutate-v8",
        "mutate-v9",
        "mutate-v11",
        "mutate-v12",
        "mutate-v13"
    )
}

/// Set to `1` to let a `mutate-*` build of `frontier-verify` run.
pub const ALLOW_MUTATED_ENV: &str = "FRONTIER_VERIFY_ALLOW_MUTATED";

/// Exit codes: 0 PASS, 1 FAIL, 2 cannot verify.
pub const EXIT_PASS: i32 = 0;
pub const EXIT_FAIL: i32 = 1;
pub const EXIT_UNVERIFIABLE: i32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    Unverifiable,
}

impl Verdict {
    pub fn name(self) -> &'static str {
        match self {
            Verdict::Pass => "PASS",
            Verdict::Fail => "FAIL",
            Verdict::Unverifiable => "UNVERIFIABLE",
        }
    }
}

/// Severity of a finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Holds, with a liveness or informational remark (PASS with warning).
    Warn,
    /// The data needed to check could not be read (UNVERIFIABLE unless a
    /// FAIL exists).
    Unverifiable,
    Fail,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub code: String,
    pub severity: Severity,
    /// The check that raised it (`V1` … `V13`).
    pub check: &'static str,
    pub entity: String,
    pub bell: u32,
    pub signature: String,
    pub detail: String,
}

impl Finding {
    pub fn fail(&self) -> bool {
        self.severity == Severity::Fail
    }
}

/// Counters of `report.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub tx: u64,
    pub failed_tx: u64,
    pub seals: u64,
    pub reveals: u64,
    pub bad_seals: u64,
    pub clashes: u64,
    pub skips: u64,
    pub tickets: u64,
    pub explores: u64,
}

/// The liveness part of the report (warnings, never a FAIL).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Liveness {
    /// Valid seals never revealed: `(host_id, arrive_bell, signatures of
    /// the failed Reveal attempts)`.
    pub valid_unrevealed: Vec<(u64, u32, Vec<String>)>,
    /// Valid seals never revealed that the rules explain (W6-C, W6T-3):
    /// settled with an outcome other than `ROUTED` (reason `bounced`: no
    /// loss by rule, outranked, quota-refused, the citizen's second
    /// arrival), or settled `ROUTED` because §5.11 step 6 refuses their
    /// Reveal (`shielded-own`, `shielded-dest`, `path`, `arrival-bell`).
    pub unrevealed_by_rule: Vec<ByRule>,
    pub reveals_near_close: u64,
    pub max_anchor_delay_s: f64,
    /// Province-bells with more than one faction's arrivals or residents.
    pub contested_bells: Vec<(i32, i32, u32)>,
}

/// One valid seal never revealed that the rules explain (`liveness.
/// unrevealed_by_rule`, W6T-3): the march, its settlement outcome, the
/// reason (`checks::v5_seals::reason`) and its failed Reveal attempts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ByRule {
    pub host: u64,
    pub arrive: u32,
    pub outcome: u8,
    pub reason: &'static str,
    pub failed_attempts: usize,
}

/// The result of one verification.
#[derive(Clone, Debug)]
pub struct Report {
    pub verdict: Verdict,
    pub season: u64,
    pub program: String,
    pub slot_range: (u64, u64),
    pub entities: u64,
    pub bells: u64,
    pub counts: Counts,
    pub findings: Vec<Finding>,
    pub liveness: Liveness,
    /// The checks that ran (a `mutate-*` build lists fewer).
    pub checks_run: Vec<&'static str>,
    pub provenance: String,
}

impl Report {
    pub fn codes(&self) -> Vec<&str> {
        self.findings.iter().map(|f| f.code.as_str()).collect()
    }
    /// Whether a FAIL finding carries `code`.
    pub fn fails_with(&self, code: &str) -> bool {
        self.findings.iter().any(|f| f.fail() && f.code == code)
    }
    pub fn json(&self) -> Value {
        report::json(self)
    }
    pub fn markdown(&self) -> String {
        report::markdown(self)
    }
}

pub fn exit_code(v: Verdict) -> i32 {
    match v {
        Verdict::Pass => EXIT_PASS,
        Verdict::Fail => EXIT_FAIL,
        Verdict::Unverifiable => EXIT_UNVERIFIABLE,
    }
}

/// Runs every check over `input` and builds the report.
pub fn verify(input: &Input) -> Report {
    checks::run(input)
}
