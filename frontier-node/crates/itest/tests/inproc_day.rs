//! `itest::inproc_day` (M1 contract §12 Gate W4): 100 bots + keeper +
//! herald fold over `ChainPort::InProcess` with the test-beacon program and
//! the test key, one game day (144 bells at 20×, 10,800 slots of virtual
//! time) and a 26-bell keeper-only drain (one whole SkipQuiet batch of the
//! keeper's and its close, `DayCfg::drain_bells`).
//!
//! Pass (strict mode, the gate): **zero stuck province-bells**; every
//! transit settled or routed by rule; every bad seal destroyed at
//! settlement with the code the stock `tlock` opener gives (and at least
//! one bad seal marched: the three bad-seal personas are in the roster);
//! no instruction answering `NotImplemented`; no persona outcome violated;
//! no keeper write dead or over budget; the herald folded every record and
//! every clash report matches its native recomputation; honest traffic
//! never tripped the relay's drain guard.
//!
//! `ITEST_STUBS=1` is the brief's first run before W4-A/W4-B/W4-C merge: the
//! resolution stand-in (`itest::standin`) moves provinces so resident
//! actions and marches happen, and every condition a missing unit decides
//! is `PENDING` (printed, never a pass). Environment: `ITEST_BOTS` (100),
//! `ITEST_BELLS` (144), `ITEST_SUMMARY=<file>` (the run's JSON summary),
//! `FRONTIER_RECORD_FIXTURES=1` (re-record `crates/agents/fixtures/
//! herald-recorded` from this run's herald at `ITEST_RECORD_BELL`, default
//! 96), `PSF_FRONTIER_SO`,
//! `ITEST_NO_BUILD` (see `itest::program`).

use itest::day::{self, DayCfg};
use itest::gate::day_conditions;

fn env_num(k: &str, d: u64) -> u64 {
    std::env::var(k)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(d)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "one game day in process (minutes of wall time); Gate W4 runs it with --include-ignored inproc_"]
async fn inproc_day() {
    let stubs = std::env::var("ITEST_STUBS").is_ok_and(|v| v == "1");
    let dir = std::env::temp_dir().join(format!("itest-inproc-day-{}", std::process::id()));
    let mut cfg = DayCfg::new(&dir);
    cfg.stubs = stubs;
    cfg.bots = env_num("ITEST_BOTS", 100) as usize;
    cfg.play_bells = env_num("ITEST_BELLS", 144) as u32;
    // The gate is 100 bots over one game day (integ-W4 review, W4-F: the
    // environment could shrink it silently).
    let small = cfg.bots < 100 || cfg.play_bells < 144;
    let allow_small = std::env::var("ITEST_ALLOW_SMALL").is_ok_and(|v| v == "1");
    assert!(
        !small || allow_small,
        "ITEST_BOTS ≥ 100 and ITEST_BELLS ≥ 144 for the gate (ITEST_ALLOW_SMALL=1 for a smaller, non-gate run)"
    );
    if std::env::var("FRONTIER_RECORD_FIXTURES").is_ok_and(|v| v == "1") {
        // The recorded herald must come from the strict day (integ-W4
        // review: the stub world's provinces never move).
        assert!(!stubs, "FRONTIER_RECORD_FIXTURES=1 needs the strict run");
        // W5-C: the bell of the recording (default 96); the bots now wait
        // for the stamina Depart charges, so a bell where the recorded
        // wallets hold rested hosts drives marches.
        cfg.record_bell = env_num("ITEST_RECORD_BELL", cfg.record_bell as u64) as u32;
        cfg.record = Some(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../agents/fixtures/herald-recorded"),
        );
    }
    if let Ok(p) = std::env::var("VERIFY_DUMP") {
        cfg.verify_dump = Some(p.into());
    }
    let out = day::run(cfg).await.expect("the run");
    let summary = out.summary();
    let text = serde_json::to_string_pretty(&summary).unwrap();
    println!("{text}");
    if let Ok(p) = std::env::var("ITEST_SUMMARY") {
        std::fs::write(&p, &text).expect("ITEST_SUMMARY");
    }
    let conds = day_conditions(&out);
    println!(
        "\ninproc_day ({} mode{}, {} bots, {} bells, {} slots, {:.0} s wall, program {}):",
        if stubs { "STUB" } else { "strict" },
        if small {
            ", SMALL (not a gate run)"
        } else {
            ""
        },
        out.cfg.bots,
        out.cfg.play_bells,
        out.slots,
        out.wall_secs,
        &out.so_sha256[..16]
    );
    for (name, c) in &conds {
        let (l, d) = c.label();
        println!("  {l:8} {name}: {d}");
    }
    let fails: Vec<&str> = conds
        .iter()
        .filter(|(_, c)| c.is_fail())
        .map(|(n, _)| *n)
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(fails.is_empty(), "failed: {fails:?}");
    // A stub run is the brief's first run, never a gate pass (integ-W4
    // review, W4-F: ITEST_STUBS=1 left in the environment made every
    // missing-unit condition PENDING and the test exit 0).
    assert!(
        !stubs,
        "ITEST_STUBS=1: a stub run is not a gate run (conditions above)"
    );
}
