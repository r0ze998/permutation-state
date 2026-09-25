//! Replay verifier (V4 §11 Must 8, V5 §11): recompute a season from public
//! data and check every root the chain claims, the governance included.
//!
//!     cargo run --release --bin verify -- --gateway http://127.0.0.1:4191 \
//!         --base http://127.0.0.1:18899 [--er http://127.0.0.1:17799] [--json]
//!
//! 1. Read the Season account straight from the base-layer RPC, and check
//!    that it is the season address of the program whose logs are read (the
//!    program id comes from the gateway, which is only an index).
//! 2. Rebuild genesis (the season's nations, seeds and treasuries) with the
//!    same `permutation-rules` crate and compare with `PS_GENESIS`.
//! 3. Replay the seating of members (from their Member accounts on the base
//!    layer: each member's nation, key, candidacy and first-election votes,
//!    checking each `PS_SEAT` root) and the first election (`PS_OPEN`, or
//!    tick 0's pre-state root).
//! 4. Replay every tick record and check that each `pre_root` matches the
//!    previous state and each `post_root` the recomputed one. `PS_TICK`
//!    carries the roots and the input's hash; the input itself (the tick
//!    seed, every office's batch and every governance action) was published
//!    as `PS_INPUT` chunks before the program would resolve the tick. With
//!    `--er`, both are re-read from the ER's transaction logs, so the gateway
//!    is only an index, never a source of truth, and the sealed orders are
//!    checked too: every revealed batch against its `PS_COMMITS` commitment,
//!    and each tick's randomness against the `PS_SALTS` salts.
//! 5. If the season is finalized: the final world root; the operator AI
//!    roster (V5 §18: the revealed members' tags chain to the committed
//!    roster, and their home cities' bounties; or the roster forfeited; or
//!    none); the settlement (V5 §7): every member's payout, the pool and
//!    bounties, the operations total and the treasuries; and the history
//!    root recomputed from the final world.
//! 6. If the season follows another, that season is finalized and its
//!    history root is the one this season took over.
//!
//! The members' talk is not checked here (see the gateway's
//! `scripts/verify-talk.mjs`).
//!
//! Exit code: 0 if every check held, 1 if one failed, 2 without `--base`.

mod chain;
mod report;
mod settlement;
mod setup;
mod ticks;

use permutation_chain::state::{Season, SeasonStatus};
use permutation_rules::{Preset, Ruleset};
use permutation_server::chainlink::ChainLink;
use report::Report;
use serde_json::Value;

/// What the stages share: the links, the gateway's season index, the season
/// and its rules, and the report.
pub struct Ctx {
    /// The base-layer RPC, keeping only the season program's log records.
    pub base: ChainLink,
    /// The ER RPC (`--er`), keeping only the season program's log records.
    pub er: Option<ChainLink>,
    /// The season program's id, as the gateway gave it (checked in
    /// `setup::read_season`).
    pub program_id: String,
    /// The gateway's `/season` index.
    pub info: Value,
    pub season: Season,
    pub rules: Ruleset,
    pub report: Report,
}

fn arg(args: &[String], k: &str) -> Option<String> {
    args.iter()
        .position(|a| a == k)
        .and_then(|i| args.get(i + 1))
        .cloned()
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let gateway =
        ChainLink::new(&arg(&args, "--gateway").unwrap_or_else(|| "http://127.0.0.1:4191".into()))
            .expect("--gateway URL");
    // The program id comes from the gateway, which is only an index: the
    // verifier checks below that the season account is its season PDA.
    let season_index = gateway.get_json("/season");
    let program_id = season_index
        .as_ref()
        .ok()
        .and_then(|v| v["programId"].as_str().map(String::from))
        .unwrap_or_default();
    let link = |flag: &str, what: &str| {
        arg(&args, flag).map(|u| ChainLink::new(&u).expect(what).with_program(&program_id))
    };
    let base = link("--base", "--base URL");
    let er = link("--er", "--er URL");
    let mut report = Report::new(args.iter().any(|a| a == "--json"));

    // 1. Season account.
    let info = season_index.expect("gateway /season");
    let season_addr = info["accounts"]["season"]
        .as_str()
        .expect("season address")
        .to_string();
    let Some(base) = base else {
        eprintln!("pass --base: the season is read from the base layer, not from the gateway");
        std::process::exit(2);
    };
    let season = setup::read_season(&base, &program_id, &season_addr, &mut report);
    let mut ctx = Ctx {
        rules: ruleset_for(season.preset, season.market),
        base,
        er,
        program_id,
        info,
        season,
        report,
    };

    // 2. Genesis.
    let mut state = setup::genesis(&mut ctx);

    // 3. Seating and the first election.
    let members = setup::seating(&mut ctx, &mut state);
    let ticks = gateway.get_json("/ticks?from=0");
    setup::first_election(&mut ctx, &mut state, ticks.as_ref().ok());

    // 4. Ticks.
    let records = ticks.expect("gateway /ticks")["records"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let replayed = ticks::replay(&mut ctx, &mut state, &records);

    // 5. Settlement, the roster and the history root.
    let finalized = ctx.season.status == SeasonStatus::Finalized;
    if finalized {
        settlement::finalized(&mut ctx, &state, &members);
    }
    // 6. The season before.
    settlement::previous_season(&mut ctx);
    if !finalized {
        ctx.report.check(
            "season not finalized yet: settlement not checked",
            true,
            format!("status {:?}", ctx.season.status),
        );
    }

    ctx.report.finish(ctx.season.season_id, replayed);
}
