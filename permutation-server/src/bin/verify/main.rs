//! Replay verifier (V4 §11 Must 8, V5 §11): recompute a season from public
//! data and check every root the chain claims, the governance included.
//!
//!     cargo run --release --bin verify -- --gateway http://127.0.0.1:4191 \
//!         --base http://127.0.0.1:18899 --er http://127.0.0.1:17799 \
//!         [--season ID] [--program ID] [--scan] [--max-scan N] [--threads N] [--json]
//!
//! The gateway is only a hint about where to look: every root, record and
//! input is read from the chain, a missing index entry is recovered from
//! the chain, and a wrong one is reported. `--er` is required (tick
//! records, inputs and commitments are read from the ER's transaction logs).
//!
//! 1. Identity: the program is `--program` (default: the canonical
//!    deployment) and must be the one the gateway indexes; the season is
//!    `--season` or, unpinned (a `–` line), the gateway's. The season
//!    address is derived from both, never taken from the gateway.
//! 2. The Season account, read from the base layer.
//! 3. A season not finalized: the live world (world chunks 0..19, from the
//!    layer where all 20 are the program's), read before the index.
//! 4. Genesis (`PS_GENESIS` selected by the season's seed), the seating of
//!    members (from their Member accounts, `PS_SEAT` records matched by
//!    root) and the first election (`PS_OPEN` at the recomputed root), from
//!    the base-layer transactions the gateway names, or the Season
//!    account's base history when they are not there (or `--scan`).
//! 5. Ticks: from the root after the first election, the chain of state
//!    roots through every `PS_TICK` record whose `pre_root` is the current
//!    root, wherever it sits (any transaction, any position), replayed with
//!    the record's own tick and stop, to `Season.final_root` or the live
//!    world. Records of other seasons are ignored. When a record, an input
//!    or a commitment is missing, the ER history of world chunk 0 is
//!    scanned in the slot window where it must lie (`--scan`: all of it;
//!    `--max-scan`: at most that many transactions, default 20000).
//! 6. Sealed orders, per tick: this season's `PS_COMMITS` (made by the
//!    office holders), `PS_SALTS` on the tick's key, every revealed batch
//!    against its commitment and salt, and the randomness from the salts.
//! 7. The gateway's index against the chain.
//! 8. A finalized season: the final world root, the operator AI roster, the
//!    settlement and the history root; and the season it follows.
//!
//! The members' talk is not checked here (see the gateway's
//! `scripts/verify-talk.mjs`).
//!
//! Lines: ✓ holds, ✗ contradicts the chain, ? could not be read, · a fact,
//! – not checked, ⚠ a warning. Exit code: 0 VERIFIED (or VERIFIED SO FAR,
//! NOT STARTED), 1 FAILED, 2 usage, 3 INCOMPLETE (chain data unavailable,
//! nothing contradicts it).

mod chain;
mod report;
mod settlement;
mod setup;
mod ticks;

use chain::{fetch_many, live_world, signatures_in, world_chunk_address, Budget, PrefixData};
use permutation_chain::state::{Season, SeasonStatus};
use permutation_rules::{Preset, Ruleset};
use permutation_server::chainlink::ChainLink;
use report::{Report, Stage};
use serde_json::Value;
use ticks::Target;

/// The published deployment, the verifier's default `--program` (until
/// the chain crate exports `CANONICAL_PROGRAM_ID`).
pub const CANONICAL_PROGRAM_ID: &str = "J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n";

/// What the stages share: the links, the gateway's season index, the season
/// and its rules, and the report.
pub struct Ctx {
    /// The base-layer RPC, keeping only the season program's log records.
    pub base: ChainLink,
    /// The ER RPC, keeping only the season program's log records.
    pub er: ChainLink,
    /// The season program's id (`--program`, or the canonical one).
    pub program_id: String,
    /// The gateway's `/season` index (a hint; `Null` if it did not answer).
    pub info: Value,
    pub season: Season,
    /// The season's address and its world chunk 0 (the anchor of its tick records).
    pub season_addr: String,
    pub anchor: String,
    pub rules: Ruleset,
    pub report: Report,
    /// `--scan`: scan the whole ER history instead of the index's gaps.
    pub scan: bool,
    pub threads: usize,
    /// `getTransaction` calls the scans may make (`--max-scan`).
    pub budget: Budget,
}

fn arg(args: &[String], k: &str) -> Option<String> {
    args.iter()
        .position(|a| a == k)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn usage(why: &str) -> ! {
    eprintln!("{why}");
    std::process::exit(2);
}

/// A numeric argument, or exit 2.
fn number<T: std::str::FromStr>(args: &[String], k: &str, default: T) -> T {
    match arg(args, k) {
        None => default,
        Some(v) => v
            .parse()
            .unwrap_or_else(|_| usage(&format!("{k}: not a number: {v}"))),
    }
}

/// The season's ruleset, as `permutation_chain::processor::rules_for` builds
/// it: preset 1 is Season, 0 Blitz (the program accepts no other preset, so
/// any other value falls back to Blitz here), with the market as the season
/// set it.
fn ruleset_for(preset: u8, market: bool) -> Ruleset {
    let mut rules = if preset == 1 {
        Ruleset::new(Preset::Season)
    } else {
        Ruleset::new(Preset::Blitz)
    };
    rules.market_enabled = market;
    rules
}

/// A u64 the gateway wrote as a string or a number.
fn u64_of(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str()?.parse().ok())
}

/// 1. The season's identity: the program and season id, pinned or taken
///    from the gateway, and the address derived from them. `None` after a ✗
///    line (the run stops).
fn identity(
    r: &mut Report,
    info: &Value,
    program: &str,
    pinned_program: bool,
    pinned_season: Option<u64>,
) -> Option<(u64, String)> {
    let what = "season identity";
    match info["programId"].as_str() {
        Some(p) if p != program => {
            r.check(
                what,
                false,
                format!("the gateway indexes program {p}, not {program}"),
            );
            return None;
        }
        None if !info.is_null() => {
            r.check(what, false, "the gateway's /season names no program");
            return None;
        }
        _ => {}
    }
    if pinned_program && program != CANONICAL_PROGRAM_ID {
        r.warn(
            "non-canonical program",
            format!("{program} is not the published deployment {CANONICAL_PROGRAM_ID}"),
        );
    }
    let offered = u64_of(&info["season"]["seasonId"]);
    let season_id = match (pinned_season, offered) {
        (Some(pin), Some(g)) if pin != g => {
            r.check(
                what,
                false,
                format!("the gateway serves season {g}, not the pinned season {pin}"),
            );
            return None;
        }
        (Some(pin), _) => pin,
        (None, Some(g)) => g,
        (None, None) => {
            r.incomplete(
                what,
                "the gateway's /season names no season id (pin one with --season)",
            );
            return None;
        }
    };
    let Some(address) = permutation_chain::state::season_address(program, season_id) else {
        r.check(what, false, format!("{program} is not a program id"));
        return None;
    };
    if let Some(a) = info["accounts"]["season"]
        .as_str()
        .filter(|a| *a != address)
    {
        r.check(
            what,
            false,
            format!(
                "the gateway's season account {a} is not season {season_id}'s address {address}"
            ),
        );
        return None;
    }
    if pinned_season.is_some() {
        r.check(
            "season identity pinned",
            true,
            format!("program {program}, season {season_id}"),
        );
    } else {
        r.not_checked(
            "season identity pin",
            format!("season id {season_id} taken from the gateway's /season (pin it with --season); program {program}"),
        );
    }
    Some((season_id, address))
}

/// 4. The base layer's pre-tick records: from the transactions the
///    gateway names, or from the Season account's history when this
///    season's genesis or open record is not among them.
fn prefix(ctx: &mut Ctx, need_open: bool) -> PrefixData {
    let mut hints: Vec<String> = Vec::new();
    for v in [&ctx.info["genesis"], &ctx.info["open"]]
        .into_iter()
        .chain(ctx.info["seating"].as_array().into_iter().flatten())
    {
        if let Some(s) = v["signature"].as_str() {
            hints.push(s.to_string());
        }
    }
    let mut p = PrefixData::default();
    let got = fetch_many(
        &ctx.base,
        &ctx.program_id,
        &hints,
        ctx.threads,
        None,
        "the index",
    );
    p.add_all(&got, &ctx.season_addr);
    let complete = p.has_genesis(&ctx.season.season_seed) && (!need_open || p.has_bound_open());
    if complete && !ctx.scan {
        return p;
    }
    match signatures_in(&ctx.base, &ctx.season_addr, 0, None, None, &ctx.budget) {
        Ok(sigs) => {
            let new: Vec<String> = sigs
                .into_iter()
                .map(|(s, _)| s)
                .filter(|s| !p.fetched.contains(s))
                .collect();
            let got = fetch_many(
                &ctx.base,
                &ctx.program_id,
                &new,
                ctx.threads,
                Some(&ctx.budget),
                "the Season account",
            );
            p.add_all(&got, &ctx.season_addr);
            ctx.report.info(
                "base-layer records located",
                format!(
                    "the Season account's base history was scanned ({} transactions{})",
                    got.len(),
                    if p.unreadable > 0 {
                        format!(", {} could not be fetched", p.unreadable)
                    } else {
                        String::new()
                    }
                ),
            );
        }
        Err(e) => ctx.report.info(
            "base-layer records located",
            format!("the Season account's base history could not be scanned: {e:?}"),
        ),
    }
    p
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let json = args.iter().any(|a| a == "--json");
    let Some(er_url) = arg(&args, "--er") else {
        usage("pass --er: tick records, inputs and commitments are read from the ER's transaction logs, never from the gateway");
    };
    let Some(base_url) = arg(&args, "--base") else {
        usage("pass --base: the season is read from the base layer, not from the gateway");
    };
    let gateway =
        ChainLink::new(&arg(&args, "--gateway").unwrap_or_else(|| "http://127.0.0.1:4191".into()))
            .unwrap_or_else(|e| usage(&format!("--gateway: {e}")));
    let pinned_program = arg(&args, "--program");
    let program_id = pinned_program
        .clone()
        .unwrap_or_else(|| CANONICAL_PROGRAM_ID.to_string());
    let pinned_season = arg(&args, "--season").map(|s| {
        s.parse::<u64>()
            .unwrap_or_else(|_| usage(&format!("--season: not a season id: {s}")))
    });
    let link = |url: &str, flag: &str| {
        ChainLink::new(url)
            .unwrap_or_else(|e| usage(&format!("{flag}: {e}")))
            .with_program(&program_id)
    };
    let (base, er) = (link(&base_url, "--base"), link(&er_url, "--er"));
    let budget = Budget::new(number(&args, "--max-scan", 20_000usize));
    let threads = number(&args, "--threads", 8usize).max(1);
    let mut report = Report::new(json);

    // 1. Identity (the gateway's /season is a hint; without it, a pinned
    //    season still verifies).
    let info = match gateway.get_json("/season") {
        Ok(v) => v,
        Err(e) => {
            report.info("the gateway's /season", format!("unavailable: {e}"));
            Value::Null
        }
    };
    let Some((season_id, season_addr)) = identity(
        &mut report,
        &info,
        &program_id,
        pinned_program.is_some(),
        pinned_season,
    ) else {
        report.finish(pinned_season.unwrap_or(0), Stage::NotStarted, 0);
    };

    // 2. Season account.
    let Some(season) = setup::read_season(&base, &season_addr, season_id, &mut report) else {
        report.finish(season_id, Stage::NotStarted, 0);
    };
    let anchor = world_chunk_address(&program_id, season_id, 0).expect("a valid program id");
    let mut ctx = Ctx {
        rules: ruleset_for(season.preset, season.market),
        base,
        er,
        program_id,
        info,
        season,
        season_addr,
        anchor,
        report,
        scan: args.iter().any(|a| a == "--scan"),
        threads,
        budget,
    };
    let status = ctx.season.status;
    if matches!(status, SeasonStatus::Registering | SeasonStatus::Genesis) {
        for what in [
            "genesis",
            "seating",
            "first election",
            "ticks",
            "settlement",
        ] {
            ctx.report
                .not_checked(what, format!("the season is {status:?}"));
        }
        ctx.report.finish(season_id, Stage::NotStarted, 0);
    }
    let finalized = status == SeasonStatus::Finalized;

    // 3. The live world, read before the index.
    let target = if finalized {
        Target::Final(ctx.season.final_root)
    } else {
        match live_world(&ctx.base, &ctx.er, &ctx.program_id, season_id) {
            Ok(w) => Target::Live(w),
            Err(e) => Target::Unread(format!("could not read the world accounts: {e}")),
        }
    };

    // 4. Genesis, seating and the first election.
    let seated = status != SeasonStatus::Seating;
    let prefix = prefix(&mut ctx, seated);
    let mut state = setup::genesis(&mut ctx, &prefix);
    setup::seed_not_checked(&mut ctx);
    if !seated {
        for what in ["seating", "first election", "ticks", "settlement"] {
            ctx.report.not_checked(what, "the members are being seated");
        }
        ctx.report.finish(season_id, Stage::Seating, 0);
    }
    let Some(members) = setup::seating(&mut ctx, &mut state, &prefix) else {
        // Without the members the world cannot be seated: every later line
        // would compare against a wrong world, so none is drawn.
        for what in ["first election", "ticks", "settlement"] {
            ctx.report
                .incomplete(what, "the member accounts could not be read");
        }
        ctx.report.finish(season_id, Stage::Seating, 0);
    };
    setup::first_election(&mut ctx, &mut state, &prefix);

    // 5–7. Ticks (the index is a hint: a failed /ticks is an empty one).
    let lines = gateway
        .get_json("/ticks?from=0")
        .ok()
        .and_then(|v| v["records"].as_array().cloned())
        .unwrap_or_default();
    let replayed = ticks::replay(&mut ctx, &mut state, &lines, &prefix, &target);

    // 8. Settlement, the roster and the history root; the season before.
    if finalized {
        settlement::finalized(&mut ctx, &state, &members);
    } else {
        ctx.report
            .not_checked("settlement", "the season is not finalized yet");
    }
    settlement::previous_season(&mut ctx);

    let stage = match &target {
        Target::Final(_) => Stage::Finalized,
        Target::Live(w) => Stage::Running { tick: Some(w.tick) },
        Target::Unread(_) => Stage::Running { tick: None },
    };
    ctx.report.finish(season_id, stage, replayed);
}
