//! The in-process system run (`inproc_day`, §12 Gate W4): the test-beacon
//! program on `localnet` at 20× (virtual time, real slot counts), the
//! keeper with every role and its loopback API, the herald (findex archive
//! → fold → files, `/h/*` served on `127.0.0.1:0`), the relay stand-in
//! (`crate::relay`) and a fleet of bots that read only the herald and write
//! only through the relay (six personas also through their own key,
//! `crate::direct`).
//!
//! One loop drives time: each slot the keeper ticks and one block is
//! produced; every `herald_every` slots the herald ingests the feed and the
//! bots that are due (their per-bell duties and profile sessions,
//! `Fleet::step_due`) observe and act at the new Clock. Play stops at
//! `play_bells`; the drain then runs `drain_bells` more bells with the
//! keeper only, so the last marches can settle. Nothing binds a port other
//! than `127.0.0.1:0`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use fclient::clock::SeasonClock;
use fclient::Signer;
use frontier_agents::profile::{roster, AgentSpec, Mix};
use frontier_agents::Persona;
use frontier_bots::bot::{ClockSource, Config as BotConfig, Shared as BotShared};
use frontier_bots::fleet::Fleet;
use frontier_bots::journal::Journal as MarchBook;
use frontier_bots::ports::{HttpHerald, HttpRelay};
use frontier_bots::report::{Report, Verdict};
use herald_fold::runner::{Ingest, IngestCfg};
use herald_fold::server::App;
use herald_fold::views::SeasonStatic;
use keeper_core::reveal_accept::ChainGate;
use keeper_core::Keeper;
use localnet::InProcess;
use serde_json::{json, Value};
use tokio::sync::broadcast;

use crate::checks::Facts;
use crate::direct::InProcDirect;
use crate::relay::{self, KeeperLink, RelayState, RelayStats};
use crate::world::{self, World, SEASON_ID};

/// The run's settings.
#[derive(Clone, Debug)]
pub struct DayCfg {
    pub bots: usize,
    pub seed: u64,
    /// Bells of play (144 = one game day).
    pub play_bells: u32,
    /// Keeper-only bells after play (settlement, closes).
    pub drain_bells: u32,
    /// The first run's stand-ins for the units not merged yet
    /// (`crate::standin`); never in the strict (gate) run.
    pub stubs: bool,
    pub dir: PathBuf,
    /// Herald ingest and bot wake-ups every this many slots.
    pub herald_every: u64,
    pub bot_concurrency: usize,
    /// Every bot joins on day 0 (a one-day run otherwise leaves 40% idle).
    pub all_join_day0: bool,
    /// Record the herald's answers into this directory (the agents'
    /// recorded fixtures) at the first bell from `record_bell` whose herald
    /// drives an unrested march (W6-C).
    pub record: Option<PathBuf>,
    /// The bell the recording is taken at (integ-W4 review, W4-F: two
    /// thirds into play, while the bots still hold hosts and march; at the
    /// end of play the recorded wallets' hosts were all spent and the
    /// fixtures drove no march).
    pub record_bell: u32,
    /// Write the whole run as a verifier fixture (`verify_core::Input`:
    /// the program's feed with the program accounts' post-states and the
    /// final states) to this file (`.gz`: compressed). Integ-W4 review, W4-D
    /// R5: the march season recorded from the program.
    pub verify_dump: Option<PathBuf>,
}

impl DayCfg {
    pub fn new(dir: impl Into<PathBuf>) -> DayCfg {
        DayCfg {
            bots: 100,
            seed: 0x1D_A7,
            play_bells: 144,
            // integ-W4: the keeper skips an idle Province in 24-bell
            // batches (keeper `play::SKIP_MAX`; §8.2, ≤ 6 skips a
            // province-day), so an idle Province may legitimately sit up to
            // 24 bells + a close behind. The drain covers one whole batch
            // and its close (24 + 2 bells); a Province still behind after it
            // is stuck, not batched. (8 bells counted 141 batched
            // province-bells over 21 idle Provinces as stuck.)
            drain_bells: 26,
            stubs: false,
            dir: dir.into(),
            herald_every: 5,
            bot_concurrency: 32,
            all_join_day0: true,
            record: None,
            record_bell: 96,
            verify_dump: None,
        }
    }
}

/// What a run produced.
pub struct DayOut {
    pub cfg: DayCfg,
    pub so_sha256: String,
    pub so_origin: String,
    pub slots: u64,
    pub wall_secs: f64,
    pub facts: Facts,
    pub bots: Report,
    pub roster: Vec<AgentSpec>,
    pub relay: RelayStats,
    pub keeper_alerts: BTreeMap<String, u64>,
    pub keeper_alert_samples: Vec<String>,
    pub keeper_unsupported: Vec<String>,
    pub keeper_writes: BTreeMap<String, (u64, u64, u64, u64)>,
    pub reveals_queued: usize,
    pub herald: Value,
    pub herald_mismatches: Vec<String>,
    pub standin_moves: u64,
    /// Marches the persona bots sent: (persona, host id, arrival bell).
    pub persona_marches: Vec<(Persona, u64, u32)>,
    pub bot_runs: u64,
    pub bot_actions: u64,
}

impl DayOut {
    pub fn persona_verdicts(&self) -> BTreeMap<&'static str, &'static str> {
        Persona::ALL
            .iter()
            .map(|&p| (p.name(), self.bots.verdict(p).name()))
            .collect()
    }

    pub fn violated(&self) -> Vec<&'static str> {
        Persona::ALL
            .iter()
            .filter(|&&p| self.bots.verdict(p) == Verdict::Violated)
            .map(|p| p.name())
            .collect()
    }

    pub fn summary(&self) -> Value {
        let relay: BTreeMap<String, u64> = self
            .relay
            .by
            .iter()
            .map(|((r, t, x), n)| (format!("{r} {t} {x}"), *n))
            .collect();
        let writes: BTreeMap<&String, Value> = self
            .keeper_writes
            .iter()
            .map(|(k, (w, l, f, d))| (k, json!({"writes": w, "landed": l, "failed": f, "dead": d})))
            .collect();
        json!({
            "program": {"sha256": self.so_sha256, "origin": self.so_origin},
            "bots": self.cfg.bots,
            "stubs": self.cfg.stubs,
            "slots": self.slots,
            "gameHours": self.slots as f64 * 8.0 / 3600.0,
            "wallSecs": self.wall_secs,
            "chain": self.facts.to_json(),
            "botReport": self.bots.to_json(),
            "botRuns": self.bot_runs,
            "botActions": self.bot_actions,
            "relay": relay,
            "keeper": {
                "alerts": self.keeper_alerts,
                "alertSamples": self.keeper_alert_samples,
                "unsupportedRoles": self.keeper_unsupported,
                "writes": writes,
                "revealsQueued": self.reveals_queued,
            },
            "herald": self.herald,
            "heraldMismatches": self.herald_mismatches,
            "standinMoves": self.standin_moves,
            "personaMarches": self
                .persona_marches
                .iter()
                .map(|(p, h, a)| {
                    json!({
                        "persona": p.name(),
                        "host": h.to_string(),
                        "arrive": a,
                        "revealed": self.facts.revealed.get(&(*h, *a)).copied().unwrap_or(0),
                        "settled": self.facts.settled.get(&(*h, *a)).map(|(o, c)| json!({"outcome": o, "sealCode": c})),
                        "stockCode": self.facts.departs.iter().find(|d| d.host_id == *h && d.arrive == *a).map(|d| d.stock_code),
                    })
                })
                .collect::<Vec<_>>(),
        })
    }
}

/// The herald's in-process pieces.
struct Herald {
    ing: Ingest,
    src: findex::LocalnetFeed<InProcess>,
    base: String,
    task: tokio::task::JoinHandle<std::io::Result<()>>,
}

async fn herald(w: &World, dir: &Path, relay_addr: &str) -> Result<Herald, String> {
    let (diffs, _) = broadcast::channel(65_536);
    let cfg = IngestCfg::new(dir, w.program, SEASON_ID);
    let ing = Ingest::open(cfg.clone(), diffs.clone())?;
    let mut app = App::new(
        ing.fold.clone(),
        herald_fold::files::Out::new(cfg.files_dir()),
        SeasonStatic {
            cluster: "localnet".into(),
            drand: w.drand.key.info(),
            quotas: json!({"perDay": relay::QUOTA_PER_DAY}),
        },
        diffs,
    );
    app.relay = Some(relay_addr.to_string());
    app.index = Some(cfg.index_path());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| e.to_string())?;
    let base = format!("http://{}", l.local_addr().map_err(|e| e.to_string())?);
    let task = tokio::spawn(herald_fold::server::serve(l, Arc::new(app)));
    let src = findex::LocalnetFeed::new(InProcess::from_chain(w.ip.chain.clone(), Some(w.program)));
    Ok(Herald {
        ing,
        src,
        base,
        task,
    })
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for e in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let e = e.map_err(|e| e.to_string())?;
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst).map_err(|e| format!("{}: {e}", src.display()))?;
        }
    }
    Ok(())
}

/// Paths the recorded fixture set holds.
async fn record(
    base: &str,
    out: &Path,
    fleet_wallets: &[(AgentSpec, String)],
    seed: u64,
    rings: u16,
    bell: u32,
) {
    let _ = std::fs::remove_dir_all(out);
    let herald = frontier_bots::ports::DirHerald::new(out);
    let mut paths = vec!["/h/season".to_string()];
    for d in 0..rings.min(3) {
        paths.push(format!("/h/overview/{d}/latest.bin"));
    }
    let mut provinces = vec![];
    for d in 0..rings.min(3) as u32 {
        for pc in permutation_rules::frontier::geometry::ring_provinces(d) {
            provinces.push((pc.p, pc.q));
        }
    }
    let mut regions = std::collections::BTreeSet::new();
    for (p, q) in &provinces {
        paths.push(format!("/h/province/{p},{q}/latest"));
        regions.insert(fclient::ix::region_of(*p, *q));
    }
    // The latest bells: anchored (b − 1) and seeded (b − 2, b − 3).
    for r in regions {
        for back in 1..=3 {
            paths.push(format!("/h/bell/{}/region/{r}", bell.saturating_sub(back)));
        }
    }
    for (_, w) in fleet_wallets {
        paths.push(format!("/h/me/{w}"));
    }
    for p in &paths {
        let url = format!("{base}{p}");
        let Ok(r) = fclient::http::get(&url).await else {
            continue;
        };
        if r.status != 200 {
            continue;
        }
        let f = herald.file_of(p);
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let body = if p.ends_with(".bin") {
            r.body
        } else {
            // Pretty and key-sorted, so a re-record diffs readably.
            serde_json::from_slice::<Value>(&r.body)
                .ok()
                .and_then(|v| serde_json::to_vec_pretty(&v).ok())
                .unwrap_or(r.body)
        };
        let _ = std::fs::write(f, body);
    }
    let idx = json!({
        "source": "itest::inproc_day (FRONTIER_RECORD_FIXTURES=1), the herald of the in-process day",
        "recordedBell": bell,
        "seed": seed,
        "wallets": fleet_wallets
            .iter()
            .map(|(a, w)| {
                json!({
                    "index": a.index,
                    "wallet": w,
                    "arch": a.arch.key(),
                    "faction": a.faction,
                    "joinBell": a.join_bell,
                    "persona": a.persona.map(|p| p.name()),
                })
            })
            .collect::<Vec<_>>(),
    });
    let _ = std::fs::write(
        out.join("index.json"),
        serde_json::to_vec_pretty(&idx).unwrap_or_default(),
    );
}

/// The run as a verifier fixture (the recorder of `verify`'s tests, over
/// the itest world).
async fn verify_dump(w: &World, path: &Path, cfg: &DayCfg) -> Result<(), String> {
    use fclient::ports::{ChainPort, Cursor, TxRecord};
    let mut txs: Vec<TxRecord> = vec![];
    let mut cur = Cursor(0);
    loop {
        let page = w.ip.feed(cur).await.map_err(|e| format!("{e:?}"))?;
        let Some(last) = page.last() else { break };
        cur = Cursor(last.seq);
        txs.extend(page);
    }
    let mut owned = std::collections::BTreeSet::new();
    for t in txs.iter_mut() {
        t.post
            .retain(|(_, a)| a.as_ref().is_some_and(|a| a.owner == w.program));
        for (k, _) in &t.post {
            owned.insert(*k);
        }
    }
    let so = std::fs::read(&w.so.path).map_err(|e| e.to_string())?;
    let so_hash = permutation_rules::hash::sha256(&[&so]);
    let inp = {
        let c = w.ip.lock();
        let finals = owned
            .into_iter()
            .map(|k| {
                (
                    k.to_bytes(),
                    c.account(&k)
                        .filter(|a| a.lamports > 0 || !a.data.is_empty()),
                )
            })
            .collect();
        verify_core::input::Input {
            cfg: verify_core::input::Config {
                program: w.program,
                season_id: SEASON_ID,
                quicknet_pk: w.drand.key.pk96,
                ruleset_hash: frontier_abi::presets::RULESET_HASH,
                program_hash: Some(so_hash),
            },
            txs,
            finals,
            final_slot: c.slot(),
            program_hashes: vec![(0, so_hash)],
            provenance: format!(
                "itest::inproc_day (VERIFY_DUMP): the test-beacon program (sha256 {}) over localnet in process at 20x, the keeper with every role, the herald, the relay stand-in and {} bots over {} bells + {} drain bells",
                w.so.sha256, cfg.bots, cfg.play_bells, cfg.drain_bells
            ),
            scenarios: vec![],
        }
    };
    inp.save(path)
}

/// Runs one in-process day.
pub async fn run(cfg: DayCfg) -> Result<DayOut, String> {
    let t0 = Instant::now();
    let _ = std::fs::remove_dir_all(&cfg.dir);
    std::fs::create_dir_all(&cfg.dir).map_err(|e| e.to_string())?;
    let so = crate::program::test_beacon_so()?;
    let w = world::world(&so, |_| {}).await;
    let season = w.season();
    let sc = SeasonClock::from_season(&season);
    let genesis_ts = season.genesis_ts;

    // ---- keeper: every role, the contract's pool minimums, its API.
    let mut kc = keeper_core::config::KeeperConfig::new(
        w.program,
        SEASON_ID,
        fclient::Address::new_from_array([0xBE; 32]),
    );
    kc.roles = keeper_core::config::ROLES
        .iter()
        .map(|r| r.to_string())
        .collect();
    let journal = keeper_core::journal::Journal::open(&cfg.dir.join("keeper.journal.sqlite"))?;
    let mut k = Keeper::new(
        kc,
        w.ip.clone(),
        w.drand.clone(),
        &[0x77; 32],
        Some(journal),
    )?;
    for a in k.payers.delay.addresses() {
        w.airdrop(&a, 2_000_000_000);
    }
    for f in &k.payers.funders.keys {
        w.airdrop(&f.pubkey(), 40_000_000_000);
    }
    k.start().await?;
    let token = "itest-keeper-token-0123456789".to_string();
    let api = keeper_core::api::serve_with(
        "127.0.0.1:0".parse().map_err(|_| "addr")?,
        k.shared.clone(),
        token.clone(),
        Some(Arc::new(ChainGate {
            port: w.ip.clone(),
            addrs: w.addrs.clone(),
        })),
    )
    .await?;

    // ---- relay stand-in and herald.
    let rs = RelayState::new(w.ip.clone(), w.program, SEASON_ID, &[0x52; 32]);
    *rs.keeper.lock().unwrap_or_else(|p| p.into_inner()) = Some(KeeperLink {
        addr: api.addr,
        token,
    });
    let relay = relay::serve(rs).await?;
    let mut h = herald(&w, &cfg.dir.join("herald"), &relay.addr.to_string()).await?;

    // ---- bots.
    let mut mix =
        Mix::for_season_days(season.end_bell / 144).with_join_close(season.join_close_bell);
    if cfg.all_join_day0 {
        mix.day0_share = 1.0;
    }
    let specs = roster(cfg.bots, cfg.seed, &mix);
    let journal = MarchBook::open(cfg.dir.join("marchbook.jsonl")).map_err(|e| e.to_string())?;
    let mut bcfg = BotConfig::new(cfg.seed);
    // Personas act every bell, so one game day reaches their tests (their
    // archetypes' sessions would leave most of them unmarched by bell 144).
    bcfg.eager_personas = true;
    let shared = BotShared::new(
        HttpHerald::new(&h.base),
        HttpRelay::new(relay.base()),
        Some(InProcDirect { ip: w.ip.clone() }),
        bcfg,
        ClockSource::fixed(w.now()),
    )
    .with_journal(journal);
    let mut fleet = Fleet::new(shared, &specs);

    // ---- the loop.
    let end_play = genesis_ts + cfg.play_bells as i64 * 600;
    let end = end_play + cfg.drain_bells as i64 * 600;
    let mut slots = 0u64;
    let mut standin_moves = 0u64;
    let (mut bot_runs, mut bot_actions) = (0u64, 0u64);
    // W6-C (DECISIONS O11): the recording is taken at the first bell from
    // `record_bell` whose herald drives an unrested march (a recorded
    // wallet plans a march with its host's recorded stamina,
    // `frontier_agents::recorded`), once per bell, **before** the bots act
    // at that step (the state they decide on: taken after, a host rested
    // this bell has already marched, and the integ-W5 re-recordings at
    // bells 60, 120 and 140 drove none). If no bell of play does, the try
    // with the most marches is kept (the recorded-herald test then fails).
    let mut recorded = false;
    let mut rec_tried: Option<u32> = None;
    let mut rec_best: Option<(usize, usize, u32)> = None;
    let rec_try = cfg.dir.join("record-try");
    let rec_keep = cfg.dir.join("record-best");
    while w.now() < end {
        k.tick().await?;
        w.step();
        slots += 1;
        if cfg.stubs && slots.is_multiple_of(cfg.herald_every) {
            standin_moves += crate::standin::pass(&w.ip, &w.program, &sc, w.now()) as u64;
        }
        if slots.is_multiple_of(cfg.herald_every) {
            h.ing.step(&mut h.src).await?;
            let (slot, now) = (w.slot(), w.now());
            h.ing.observe_clock(slot, now);
            let record_at = genesis_ts + cfg.record_bell.min(cfg.play_bells) as i64 * 600;
            let bell = sc.bell_at(now).unwrap_or(0);
            if cfg.record.is_some()
                && !recorded
                && now >= record_at
                && now < end_play
                && rec_tried != Some(bell)
            {
                rec_tried = Some(bell);
                let wallets: Vec<(AgentSpec, String)> = fleet
                    .bots
                    .iter()
                    .filter(|b| b.spec.index < 32 || b.spec.persona.is_some())
                    .map(|b| (b.spec, b.wallet.pubkey().to_string()))
                    .collect();
                let rings = frontier_bots::bot::Shared::season(&fleet.shared)
                    .await
                    .map(|s| s.rings_opened())
                    .unwrap_or(4);
                record(&h.base, &rec_try, &wallets, cfg.seed, rings, bell).await;
                let (unrested, rested) = match frontier_agents::recorded::load(&rec_try) {
                    Ok(r) => {
                        let p = frontier_agents::recorded::plans(
                            &r,
                            frontier_agents::fixture::REVEAL_LOADED_LIMIT,
                        );
                        (p.unrested(), p.rested())
                    }
                    Err(e) => {
                        eprintln!("itest: recording at bell {bell} does not load: {e}");
                        (0, 0)
                    }
                };
                if rec_best.is_none_or(|b| (unrested, rested) > (b.0, b.1)) {
                    let _ = std::fs::remove_dir_all(&rec_keep);
                    std::fs::rename(&rec_try, &rec_keep).map_err(|e| e.to_string())?;
                    rec_best = Some((unrested, rested, bell));
                }
                if unrested > 0 {
                    recorded = true;
                }
            }
            if now < end_play && now >= genesis_ts {
                fleet.shared.clock.set(now);
                let (r, a) = fleet.step_due(cfg.bot_concurrency).await;
                bot_runs += r as u64;
                bot_actions += a as u64;
            }
        }
    }
    if let (Some(out), Some((unrested, rested, bell))) = (&cfg.record, rec_best) {
        let _ = std::fs::remove_dir_all(out);
        copy_dir(&rec_keep, out)?;
        eprintln!(
            "itest: herald recorded at bell {bell}: {unrested} marches planned with the recorded stamina, {rested} with rested hosts"
        );
    }
    // Final ingest.
    while h.ing.step(&mut h.src).await? > 0 {}
    h.ing.checkpoint()?;

    let end_bell = cfg.play_bells;
    let facts = Facts::collect(&w, end_bell).await;
    if let Some(path) = &cfg.verify_dump {
        verify_dump(&w, path, &cfg).await?;
    }
    let bots = fleet.shared.report.lock().map_err(|_| "report")?.clone();
    let persona_marches: Vec<(Persona, u64, u32)> = fleet
        .bots
        .iter()
        .filter_map(|b| b.spec.persona.map(|p| (p, b)))
        .flat_map(|(p, b)| {
            b.mem
                .marches
                .iter()
                .filter(|m| m.sent)
                .map(move |m| (p, m.key.0, m.arrive_bell))
        })
        .collect();
    let relay_stats = relay.state.stats.lock().map_err(|_| "relay stats")?.clone();
    let mut alerts: BTreeMap<String, u64> = BTreeMap::new();
    let mut samples = vec![];
    for (slot, kind, msg) in &k.alerts {
        let n = alerts.entry(kind.clone()).or_default();
        *n += 1;
        if *n <= 2 {
            samples.push(format!("{slot} {kind}: {msg}"));
        }
    }
    let writes = k
        .engine
        .stats
        .iter()
        .map(|(kind, st)| (kind.to_string(), (st.writes, st.landed, st.failed, st.dead)))
        .collect();
    let reveals_queued = k.shared.lock().map_err(|_| "keeper shared")?.reveals.len();
    let (herald_json, mismatches) = {
        let f = h.ing.fold.read().map_err(|_| "fold")?;
        let mut mism = vec![];
        let clash_dir = h.ing.cfg.files_dir().join("h/clash");
        if let Ok(rd) = std::fs::read_dir(&clash_dir) {
            for pq in rd.flatten() {
                for bf in std::fs::read_dir(pq.path()).into_iter().flatten().flatten() {
                    // The `.gz` siblings (DECISIONS J2) are the same report
                    // compressed; only the JSON files are read (integ-W4:
                    // every sibling counted as a `null` mismatch).
                    if bf.path().extension().is_some_and(|x| x == "gz") {
                        continue;
                    }
                    let v: Value = std::fs::read(bf.path())
                        .ok()
                        .and_then(|b| serde_json::from_slice(&b).ok())
                        .unwrap_or(Value::Null);
                    if v["heraldCheck"] != "match" {
                        mism.push(format!("{}: {}", bf.path().display(), v["heraldCheck"]));
                    }
                }
            }
        }
        (
            json!({
                "events": f.st.events,
                "badRecords": f.st.alarms.bad_records,
                "rewrites": f.st.alarms.rewrites,
                "clashMismatch": f.st.alarms.clash_mismatch,
                "clashUnchecked": f.st.alarms.clash_unchecked,
                "writeErrors": f.st.alarms.write_errors,
            }),
            mism,
        )
    };
    h.task.abort();
    relay.stop();
    api.stop();
    Ok(DayOut {
        so_sha256: so.sha256.clone(),
        so_origin: so.origin.clone(),
        slots,
        wall_secs: t0.elapsed().as_secs_f64(),
        facts,
        bots,
        roster: specs,
        relay: relay_stats,
        keeper_alerts: alerts,
        keeper_alert_samples: samples,
        keeper_unsupported: k.unsupported_roles.iter().map(|s| s.to_string()).collect(),
        keeper_writes: writes,
        reveals_queued,
        herald: herald_json,
        herald_mismatches: mismatches,
        standin_moves,
        persona_marches,
        bot_runs,
        bot_actions,
        cfg,
    })
}
