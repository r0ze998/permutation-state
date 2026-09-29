//! `keeper-core` (M1 contract §8.2; offchain design §6).
//!
//! | module | what |
//! |---|---|
//! | [`config`] | `keeper.toml` |
//! | [`pools`] | reveal (≥ 150) and delay (≥ 32) payer pools, ≥ 4 funders, uniform draws, payer care |
//! | [`engine`] | the escalation engine: W/D/N bids ×2 per slot, a new payer per version, the CU and heap retry ladder, contested detection |
//! | [`journal`] | SQLite journal (WAL, `synchronous=FULL`), the process lock |
//! | [`rounds`] | verified drand rounds with their hints |
//! | [`beacon`] | anchors (combined + fallbacks), seed caches with the nonce switch, beacon logs |
//! | [`genesis`] | the genesis seed |
//! | [`archive`] | ArchiveAnchors 48 h after the anchor, CloseSeedCache |
//! | [`rings`] | every ring (genesis and crowding), ConsumeRingSeed, OpenProvince (W3-C) |
//! | [`fold`] | FoldOccupancy, three parts per bell while joins are open (W3-C) |
//! | [`tickets`] | SettleTicket in descending score per site, displacement, expiry (W3-C) |
//! | [`explore`] | SettleExplore (W3-C) |
//! | [`holdings`] | ReleaseDormant, DisbandStranded, SweepPoolOwed (W3-C) |
//! | [`landindex`] | the land objects the feed's PS2 records name (W3-C) |
//! | [`seeds`] | where a `(bell, region)` seed is read and its value (W3-C) |
//! | [`reveal_accept`] | the `/v1/reveal` accept path against chain (W3-C) |
//! | [`playindex`] | transits, reveals, gathers, clashes and closes the feed names (W4-C) |
//! | [`play`] | decrypt, the reveal pipeline, SettleDeparture and returns, the gather/resolve/skip scheduler, SettleTransit, closes, claims (W4-C) |
//! | [`api`] | the loopback API (`/v1/*`, `/metrics`) |
//!
//! [`Keeper::tick`] runs once per slot: it reads the Clock, plans every
//! duty from chain reads (the chain is the state), then lets the engine
//! poll, settle and send. Every rule time is the Clock's `unix_timestamp`,
//! never the wall clock, so the same code runs at 1× on a validator and at
//! 20× (or in virtual time) on `localnet`.
//!
//! W3-C added land (rings past genesis, folds, tickets, explore, dormancy,
//! stranded hosts, sweeps, the `/v1/reveal` accept path); W4-C adds the
//! reveal pipeline, gathers, resolves, skips, settlements, closes and claims.

pub mod api;
pub mod archive;
pub mod beacon;
pub mod config;
pub mod engine;
pub mod explore;
pub mod fold;
pub mod genesis;
pub mod holdings;
pub mod journal;
pub mod landindex;
pub mod play;
pub mod playindex;
pub mod pools;
pub mod reveal_accept;
pub mod rings;
pub mod rounds;
pub mod seeds;
#[cfg(test)]
pub(crate) mod testkit;
pub mod tickets;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use solana_address::Address;

use fclient::abi::{self, status, tag, Class};
use fclient::addr::Addresses;
use fclient::budgets::Budgets;
use fclient::clock::SeasonClock;
use fclient::decode::Season;
use fclient::fees;
use fclient::ports::{ChainPort, DrandPort};

use crate::archive::ArchiveDuty;
use crate::beacon::BeaconDuty;
use crate::config::KeeperConfig;
use crate::engine::{BuildCtx, Engine, EngineParams, Outcome, WriteSpec};
use crate::explore::ExploreDuty;
use crate::fold::FoldDuty;
use crate::genesis::GenesisDuty;
use crate::holdings::HoldingDuty;
use crate::journal::Journal;
use crate::landindex::LandIndex;
use crate::play::PlayDuty;
use crate::pools::Payers;
use crate::rings::RingDuty;
use crate::rounds::Rounds;
use crate::seeds::SeedFinder;
use crate::tickets::TicketDuty;

/// Land roles that need the feed index.
const LAND_ROLES: [&str; 5] = ["tickets", "explore", "dormancy", "sweep", "fold"];
/// Play roles (W4-C).
pub const PLAY_ROLES: [&str; 8] = [
    "reveal",
    "settle-departure",
    "gather",
    "resolve",
    "skip",
    "settle",
    "close",
    "claims",
];

/// The role a land write key belongs to (for "the program lacks it").
fn role_of_key(key: &str) -> Option<&'static str> {
    match key.split(':').next()? {
        "fold" => Some("fold"),
        "ticket" => Some("tickets"),
        "explore" => Some("explore"),
        "release" | "stranded" => Some("dormancy"),
        "sweep" => Some("sweep"),
        "reveal" => Some("reveal"),
        "sdep" | "return" => Some("settle-departure"),
        "gather" => Some("gather"),
        "resolve" => Some("resolve"),
        "skip" => Some("skip"),
        "settle" => Some("settle"),
        "close" => Some("close"),
        "claim" => Some("claims"),
        _ => None,
    }
}

/// The beacon duty's latency-critical write kinds: sent as soon as they
/// are planned, and on the idle ticks of a slot (W6T-2).
const BEACON_KINDS: [&str; 3] = ["anchor", "anchor-multi", "seed"];

/// P_def = 2.0, P_delay = 0.5 (milli-units, §8.2).
pub const P_DEF_MILLI: u64 = 2_000;
pub const P_DELAY_MILLI: u64 = 500;
/// The fixed low bid of class N writes and the start of D escalation.
pub const P_LOW_MILLI: u64 = 100;

/// The W1 skeleton's bid schedule, kept for its callers: the priority of
/// the version sent `slots` slots after the first (see [`EngineParams::bid`]).
pub fn bid_milli(class: Class, p_start: u64, slots: u32, season_cap_milli: u64) -> u64 {
    EngineParams {
        p_start_milli: p_start,
        d_start_milli: p_start,
        p_def_milli: P_DEF_MILLI.min(season_cap_milli),
        ..EngineParams::default()
    }
    .bid(class, slots as u64)
}

/// The canonical budgets table (`frontier-abi/vectors/budgets.json`, §10.2):
/// per-kind CU limit and `L(kind)`, regenerated by the gates from the
/// measured program; a `budgets_file` in `keeper.toml` overrides it.
pub const CANONICAL_BUDGETS: &str = include_str!("../../../../frontier-abi/vectors/budgets.json");

/// What every duty sees of one tick.
pub struct Tick<'a> {
    pub slot: u64,
    /// The Clock's `unix_timestamp` (game time).
    pub now: i64,
    pub season: &'a Season,
    pub clock: SeasonClock,
    pub addrs: &'a Addresses,
    pub cfg: &'a KeeperConfig,
}

/// State the loopback API reads and writes (updated every tick).
///
/// W6T-2: a tick never takes `status`, `metrics`, `tracks` or
/// `reveal_keys` out of it; the play duty takes only the queued `reveals`
/// and `nudges` and merges its track updates back. Before, the whole
/// struct was taken for the play duty's plan, and `/v1/status` answered
/// `null` for as long as the plan ran (322 of 1,035 bells in w6-s7), a
/// track answered 404 and the same material could be queued twice.
#[derive(Default)]
pub struct Shared {
    /// The status snapshot of the last tick (published after its sends).
    pub status: Value,
    pub metrics: String,
    /// Owner reveal material queued by `/v1/reveal` (W3-C accepts, W4-C sends).
    pub reveals: Vec<(String, Value)>,
    /// `holding:slot:commit` → track id (the same material answers the same track).
    pub reveal_keys: BTreeMap<String, String>,
    pub tracks: BTreeMap<String, Value>,
    pub nudges: Vec<Value>,
    pub next_track: u64,
    /// `"P,Q"` → what holds the province now (W4-C, `/v1/nudge`).
    pub blocking: BTreeMap<String, Vec<Value>>,
}

/// What one tick did.
#[derive(Debug, Default)]
pub struct TickReport {
    pub slot: u64,
    pub now: i64,
    pub idle: bool,
    pub sent: usize,
    pub outcomes: Vec<(String, Outcome)>,
    pub contested: Vec<(Option<u32>, Option<u8>, String)>,
    /// The chain's slot when the first version went out (`None`: nothing
    /// sent), and per phase `(name, wall ms, chain reads)` (W6T-2; logged
    /// with `FRONTIER_KEEPER_TICK_LOG=1`).
    pub chain_at_send: Option<u64>,
    pub phases: Vec<(&'static str, u64, u64)>,
}

impl TickReport {
    /// One line per tick: `TICK slot=… chain_at_send=… sent=… total=…ms
    /// <phase>=<ms>ms/<reads>r …`.
    pub fn line(&self, total_ms: u64) -> String {
        let mut l = format!(
            "TICK slot={} chain_at_send={} sent={} total={total_ms}ms",
            self.slot,
            self.chain_at_send.map_or(-1, |s| s as i64),
            self.sent
        );
        for (n, ms, r) in &self.phases {
            l.push_str(&format!(" {n}={ms}ms/{r}r"));
        }
        l
    }
}

/// `(p99, p50)`-style quantile of a sample (nearest rank).
pub fn quantile(v: &[u64], q: f64) -> Option<u64> {
    if v.is_empty() {
        return None;
    }
    let mut s = v.to_vec();
    s.sort_unstable();
    let i = ((q * s.len() as f64).ceil() as usize).clamp(1, s.len()) - 1;
    Some(s[i])
}

pub struct Keeper<P: ChainPort, D: DrandPort> {
    pub cfg: KeeperConfig,
    pub port: P,
    pub drand: D,
    pub addrs: Addresses,
    pub payers: Payers,
    pub engine: Engine,
    pub journal: Option<Journal>,
    pub rounds: Rounds,
    pub season: Option<Season>,
    pub beacon: BeaconDuty,
    pub genesis: GenesisDuty,
    pub archive: ArchiveDuty,
    pub rings: RingDuty,
    pub fold: FoldDuty,
    pub tickets: TicketDuty,
    pub explore: ExploreDuty,
    pub holdings: HoldingDuty,
    pub index: LandIndex,
    pub seeds: SeedFinder,
    /// Play (W4-C).
    pub play: PlayDuty,
    /// Land roles the program answered NotImplemented (99) for: off.
    pub unsupported_roles: BTreeSet<&'static str>,
    pub shared: Arc<Mutex<Shared>>,
    pub last_slot: Option<u64>,
    /// `(slot, kind, detail)`.
    pub alerts: Vec<(u64, String, String)>,
    /// Per bell: `(bell, effective N reveal, effective N delay)` (E5 criterion 4).
    pub effective_n: Vec<(u32, usize, usize)>,
    /// Fees sent per game day (the daily budget).
    pub spend_by_day: BTreeMap<u32, u64>,
    season_read_slot: Option<u64>,
    wrong_key_alerted: bool,
    /// The last payer care: `(slot, Clock now, transfers planned)`.
    care_last: Option<(u64, i64, usize)>,
}

/// Slots between two cares while a pool is below its minimum (the top-up
/// of the previous care has had a block to land).
pub const CARE_LOW_GAP_SLOTS: u64 = 2;

/// Whether payer care runs this tick (W6-C, W5-B F1). Care runs at rest,
/// never while care transfers are still pending (that would top the same
/// payer up twice): on the first tick; every `care_every_slots` slots or
/// `care_every_game_secs` game seconds, whichever comes first (a cadence in
/// slots alone ran every 10 bells at 100×); and, when a pool is below its
/// minimum effective N (`low`), `CARE_LOW_GAP_SLOTS` after a care that
/// planned transfers (§8.2: "tops up at once"; a care that found no
/// funder able to pay waits for the cadence).
pub fn care_due(
    cfg: &KeeperConfig,
    last: Option<(u64, i64, usize)>,
    slot: u64,
    now: i64,
    low: bool,
    care_pending: bool,
) -> bool {
    if care_pending {
        return false;
    }
    let Some((s0, n0, planned)) = last else {
        return true;
    };
    slot >= s0 + cfg.care_every_slots.max(1)
        || now >= n0 + cfg.care_every_game_secs.max(1) as i64
        || (low && planned > 0 && slot >= s0 + CARE_LOW_GAP_SLOTS)
}

impl<P: ChainPort, D: DrandPort> Keeper<P, D> {
    /// A keeper over `port` and `drand` with payers derived from `master`.
    pub fn new(
        cfg: KeeperConfig,
        port: P,
        drand: D,
        master: &[u8; 32],
        journal: Option<Journal>,
    ) -> Result<Keeper<P, D>, String> {
        let budgets = match &cfg.budgets_file {
            Some(p) => Budgets::from_json(
                &serde_json::from_slice(&std::fs::read(p).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?,
            )?,
            None => Budgets::from_json(
                &serde_json::from_str(CANONICAL_BUDGETS).map_err(|e| e.to_string())?,
            )?,
        };
        // The reveal floor is priced at the Reveal this table requests
        // (integ-W6 review, W6-E F1).
        let payers = Payers::with_budgets(master, &cfg, &budgets).map_err(|e| format!("{e:?}"))?;
        let params = EngineParams {
            d_start_milli: cfg.p_low_milli,
            p_low_milli: cfg.p_low_milli,
            p_def_milli: cfg.p_def_milli,
            p_delay_milli: cfg.p_delay_milli,
            contested_slots: cfg.contested_slots,
            per_write_cap: cfg.per_write_cap_lamports,
            d_resend_slots: cfg.d_resend_slots,
            cap_resend_slots: cfg.cap_resend_slots,
            ..EngineParams::default()
        };
        let pk96 = drand.info().public_key;
        let mut engine = Engine::new(budgets, params);
        // Payer care: plain System transfers (tag 0 is no Frontier instruction).
        engine.budgets.set(
            0,
            fclient::budgets::Budget {
                cu_limit: 2_000,
                loaded_limit: fees::PAGE,
            },
        );
        Ok(Keeper {
            addrs: Addresses::new(cfg.program, cfg.season_id),
            beacon: BeaconDuty::new(cfg.regions.clone()),
            engine,
            rounds: Rounds::new(pk96),
            payers,
            journal,
            season: None,
            genesis: GenesisDuty::default(),
            archive: ArchiveDuty::default(),
            rings: RingDuty::default(),
            fold: FoldDuty::default(),
            tickets: TicketDuty::default(),
            explore: ExploreDuty::default(),
            holdings: HoldingDuty::default(),
            index: LandIndex::default(),
            seeds: SeedFinder::default(),
            play: PlayDuty::default(),
            unsupported_roles: BTreeSet::new(),
            shared: Arc::new(Mutex::new(Shared::default())),
            last_slot: None,
            alerts: vec![],
            effective_n: vec![],
            spend_by_day: BTreeMap::new(),
            season_read_slot: None,
            wrong_key_alerted: false,
            care_last: None,
            cfg,
            port,
            drand,
        })
    }

    /// Sets the ClaimDefence signer: the beneficiary's key (§5.12: the
    /// keeper that claims is the beneficiary of the slots).
    pub fn set_claim_key(&mut self, kp: fclient::Keypair) -> Result<(), String> {
        use solana_signer::Signer as _;
        if kp.pubkey() != self.cfg.beneficiary {
            return Err(format!(
                "the claim key {} is not the beneficiary {}",
                kp.pubkey(),
                self.cfg.beneficiary
            ));
        }
        self.cfg.claim_key = Some(kp.pubkey());
        self.payers.extra.push(kp);
        Ok(())
    }

    fn alert(&mut self, slot: u64, kind: &str, detail: String) {
        if let Some(j) = &self.journal {
            let _ = j.alert(slot, kind, &detail);
        }
        self.alerts.push((slot, kind.into(), detail));
    }

    /// Start-up (§8.2 crash safety): reconcile every in-flight attempt of
    /// the journal with the chain, adopt the ones still undecided so their
    /// landing ends the write, and read the payers' balances.
    pub async fn start(&mut self) -> Result<usize, String> {
        let clock = self.port.clock().await.map_err(|e| e.to_string())?;
        let mut adopted = 0;
        if let Some(j) = &self.journal {
            let inflight = j.in_flight()?;
            let sigs: Vec<fclient::ports::Signature> =
                inflight.iter().filter_map(|a| a.sig.parse().ok()).collect();
            let sts = self.port.statuses(&sigs).await.map_err(|e| e.to_string())?;
            let mut keep = vec![];
            for (a, st) in inflight.iter().zip(sts) {
                match st {
                    Some(st) => {
                        let s = if st.err.is_none() { "landed" } else { "failed" };
                        j.set_status(&a.sig, s, Some(st.slot), st.code)?;
                    }
                    None if clock.slot > a.first_valid_slot + 151 => {
                        j.set_status(&a.sig, "expired", None, None)?;
                    }
                    None => keep.push(a.clone()),
                }
            }
            adopted = keep.len();
            self.engine.adopt(&keep);
        }
        self.payers
            .refresh(&self.port, clock.slot)
            .await
            .map_err(|e| e.to_string())?;
        Ok(adopted)
    }

    fn update_params(&mut self, s: &Season) {
        let cost_reveal = fees::cost(s.reveal_cu_limit, 1, 2, s.reveal_loaded_limit);
        let tip_min = fees::min_tip_lamports(
            s.min_reveal_priority_milli,
            s.reveal_cu_limit,
            s.reveal_loaded_limit,
        );
        let p_tip = fees::p_tip_milli(tip_min, cost_reveal).max(1);
        let p = &mut self.engine.params;
        p.p_tip_milli = p_tip;
        p.p_start_milli = match self.cfg.peace_start {
            Some(f) => ((p_tip as f64) * f).round().max(1.0) as u64,
            None => p_tip,
        };
        p.p_def_milli = self.cfg.p_def_milli.min(s.defence_cap_milli as u64).max(1);
    }

    /// One slot of work. Returns early (idle) when the slot has not moved.
    pub async fn tick(&mut self) -> Result<TickReport, String> {
        let clock = self.port.clock().await.map_err(|e| e.to_string())?;
        let (slot, now) = (clock.slot, clock.unix_timestamp);
        let mut rep = TickReport {
            slot,
            now,
            ..Default::default()
        };
        if self.last_slot == Some(slot) {
            // W6T-2 (cause B): a drand round this slot asked for and did
            // not get is asked again within the slot, and its anchor or
            // seed cache goes out at once (beacon duty and send only). The
            // round is public from its time; waiting for the next slot cost
            // one of criterion 3's two slots (W6-A F-A3).
            if let Some(season) = self.season.clone() {
                if self.cfg.has_role("beacon")
                    && season.status >= status::SEEDED
                    && self.rounds.pk_hash() == season.quicknet_pk_hash
                    && self.rounds.take_missed(slot)
                {
                    let t = Tick {
                        slot,
                        now,
                        season: &season,
                        clock: SeasonClock::from_season(&season),
                        addrs: &self.addrs,
                        cfg: &self.cfg,
                    };
                    self.beacon
                        .plan(
                            &t,
                            &self.port,
                            &self.drand,
                            &mut self.rounds,
                            &mut self.engine,
                            self.journal.as_ref(),
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                    let mut r = crate::engine::StepReport::default();
                    self.engine
                        .send_kinds(
                            &self.port,
                            &mut self.payers,
                            self.journal.as_ref(),
                            slot,
                            &mut r,
                            Some(&BEACON_KINDS),
                        )
                        .await;
                    rep.sent = r.sent;
                    rep.chain_at_send = self.engine.last_send_chain_slot.filter(|_| r.sent > 0);
                    self.note_step(slot, &r);
                    rep.outcomes = r.outcomes;
                    rep.contested = r.contested;
                }
            }
            rep.idle = true;
            return Ok(rep);
        }
        self.last_slot = Some(slot);
        let mut t_phase = std::time::Instant::now();
        let mut phase = |rep: &mut TickReport, name: &'static str, reads: u64| {
            rep.phases
                .push((name, t_phase.elapsed().as_millis() as u64, reads));
            t_phase = std::time::Instant::now();
        };
        // The season: every slot until it runs, then every 16 slots.
        let stale = self.season.as_ref().is_none_or(|s| {
            s.effective_status(now) != status::RUNNING
                || self.season_read_slot.is_none_or(|r| slot >= r + 16)
        });
        if stale {
            let got = self
                .port
                .accounts(&[self.addrs.season], 0)
                .await
                .map_err(|e| e.to_string())?;
            if let Some(s) = got[0]
                .as_ref()
                .filter(|a| a.owner == self.addrs.program)
                .and_then(|a| Season::decode(&a.data).ok())
            {
                self.update_params(&s);
                self.season = Some(s);
                self.season_read_slot = Some(slot);
            }
        }
        // Payer care and the per-bell effective N (at rest, never inside a
        // critical write).
        let bell = self
            .season
            .as_ref()
            .and_then(|s| SeasonClock::from_season(s).bell_at(now));
        let low = self.payers.effective_n(Class::W)
            < fclient::payers::MIN_REVEAL.min(self.payers.reveal.keys.len())
            || self.payers.effective_n(Class::D)
                < fclient::payers::MIN_DELAY.min(self.payers.delay.keys.len());
        let care_pending = self
            .engine
            .pending_keys()
            .iter()
            .any(|k| k.starts_with("care:"));
        if care_due(&self.cfg, self.care_last, slot, now, low, care_pending) {
            self.payers
                .refresh(&self.port, slot)
                .await
                .map_err(|e| e.to_string())?;
            let planned = self.plan_care(slot);
            self.care_last = Some((slot, now, planned));
        }
        if let Some(b) = bell {
            if self.effective_n.last().is_none_or(|x| x.0 != b) {
                let (w, d) = (
                    self.payers.effective_n(Class::W),
                    self.payers.effective_n(Class::D),
                );
                self.effective_n.push((b, w, d));
                let want = fclient::payers::MIN_REVEAL.min(self.payers.reveal.keys.len());
                if w < want {
                    self.alert(
                        slot,
                        "reveal-pool-low",
                        format!(
                            "bell {b}: effective N {w} < {want}; topping up, W writes continue"
                        ),
                    );
                }
            }
        }
        phase(&mut rep, "season+care", 0);
        // Poll first: a write that landed is an outcome before the duties
        // re-read the chain.
        let mut r = self
            .engine
            .poll(&self.port, self.journal.as_ref(), slot)
            .await;
        for (k, o) in &r.outcomes {
            if let Outcome::Landed { .. } = o {
                self.archive.on_landed(k, &mut self.beacon.anchors);
                self.fold.on_landed(k);
            }
            if k.starts_with("ticket:") {
                self.tickets.on_outcome(k, o);
            }
            let tracks = self.play.on_outcome(k, o);
            if !tracks.is_empty() {
                let mut s = self.shared.lock().unwrap_or_else(|p| p.into_inner());
                for (id, v) in tracks {
                    s.tracks.insert(id, v);
                }
            }
        }
        phase(&mut rep, "poll", 0);
        // Duties.
        if let Some(season) = self.season.clone() {
            let sclock = SeasonClock::from_season(&season);
            let key_ok = self.rounds.pk_hash() == season.quicknet_pk_hash;
            if !key_ok && !self.wrong_key_alerted {
                self.wrong_key_alerted = true;
                self.alert(
                    slot,
                    "beacon-key",
                    "the drand key does not hash to the season's quicknet_pk_hash; beacon duties off"
                        .into(),
                );
            }
            // Roles the program lacks (99) are off for the rest of the run.
            let cfg_eff;
            let cfg = if self.unsupported_roles.is_empty() {
                &self.cfg
            } else {
                let mut c = self.cfg.clone();
                c.roles
                    .retain(|r| !self.unsupported_roles.contains(r.as_str()));
                cfg_eff = c;
                &cfg_eff
            };
            let t = Tick {
                slot,
                now,
                season: &season,
                clock: sclock,
                addrs: &self.addrs,
                cfg,
            };
            if key_ok {
                self.genesis
                    .plan(
                        &t,
                        &self.port,
                        &self.drand,
                        &mut self.rounds,
                        &mut self.engine,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                self.rings
                    .plan(
                        &t,
                        &self.port,
                        &self.drand,
                        &mut self.rounds,
                        &mut self.engine,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                if self.cfg.has_role("beacon") && season.status >= status::SEEDED {
                    self.beacon
                        .plan(
                            &t,
                            &self.port,
                            &self.drand,
                            &mut self.rounds,
                            &mut self.engine,
                            self.journal.as_ref(),
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
            self.archive
                .plan(&t, &self.beacon.anchors, &mut self.engine);
            phase(&mut rep, "beacon", 0);
            // The anchors and seed caches go out at once (W6T-2, R2): the
            // land and play duties first pull the feed (≈ 0.4 s a tick while
            // a restarted keeper re-reads it), and an anchor planned in its
            // publication slot but sent after them landed a slot late.
            self.engine
                .send_kinds(
                    &self.port,
                    &mut self.payers,
                    self.journal.as_ref(),
                    slot,
                    &mut r,
                    Some(&BEACON_KINDS),
                )
                .await;
            if r.sent > 0 {
                rep.chain_at_send = self.engine.last_send_chain_slot;
            }
            phase(&mut rep, "send0", 0);
            // One feed read for both indexes (W6T-2): while their cursors
            // agree (always, from a start), a page is read once and ingested
            // by the land and the play index. A restarted keeper re-reads
            // the whole feed; two readers made its catch-up ticks ≈ 0.45 s,
            // longer than a 20× slot (R2).
            let play_pulls = PLAY_ROLES.iter().any(|r| t.cfg.has_role(r))
                && matches!(
                    season.effective_status(now),
                    status::RUNNING | status::ENDED
                );
            let shared_read = LAND_ROLES.iter().any(|r| t.cfg.has_role(r))
                && play_pulls
                && self.index.cursor == self.play.index.cursor;
            if shared_read {
                for _ in 0..crate::landindex::PAGES_PER_TICK {
                    let page = self
                        .port
                        .feed(self.index.cursor)
                        .await
                        .map_err(|e| e.to_string())?;
                    if page.is_empty() {
                        break;
                    }
                    self.index.ingest_page(&page, &self.addrs);
                    self.play.index.ingest_page(&page, &self.addrs);
                }
            }
            // The play duty's own read is skipped this tick (the cursors
            // stay together).
            self.play.feed_read = shared_read;
            // Land (W3-C).
            if LAND_ROLES.iter().any(|r| t.cfg.has_role(r)) {
                if !shared_read {
                    self.index
                        .pull(&self.port, &self.addrs)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                self.fold
                    .plan(&t, &self.port, &mut self.engine)
                    .await
                    .map_err(|e| e.to_string())?;
                self.tickets
                    .plan(
                        &t,
                        &self.port,
                        &mut self.engine,
                        &mut self.index,
                        &mut self.seeds,
                        &self.beacon.anchors,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                self.explore
                    .plan(
                        &t,
                        &self.port,
                        &mut self.engine,
                        &mut self.index,
                        &mut self.seeds,
                        &self.beacon.anchors,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                let mut provinces = self.rings.opened.clone();
                provinces.extend(
                    self.index
                        .provinces
                        .iter()
                        .map(|&(p, q)| (p as i32, q as i32)),
                );
                self.holdings
                    .plan(
                        &t,
                        &self.port,
                        &mut self.engine,
                        &mut self.index,
                        &provinces,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
            }
            phase(&mut rep, "land", 0);
        }
        // Play (W4-C).
        let play_on = PLAY_ROLES.iter().any(|r| self.cfg.has_role(r))
            && !PLAY_ROLES
                .iter()
                .all(|r| self.unsupported_roles.contains(r));
        // The CU-meter test of a failed version reads the feed's meta,
        // which the play index pulls (W6T-2).
        self.engine.expect_tx_meta = play_on;
        if let Some(season) = self.season.clone() {
            if play_on {
                let cfg_eff;
                let cfg = if self.unsupported_roles.is_empty() {
                    &self.cfg
                } else {
                    let mut c = self.cfg.clone();
                    c.roles
                        .retain(|r| !self.unsupported_roles.contains(r.as_str()));
                    cfg_eff = c;
                    &cfg_eff
                };
                let t = Tick {
                    slot,
                    now,
                    season: &season,
                    clock: SeasonClock::from_season(&season),
                    addrs: &self.addrs,
                    cfg,
                };
                let mut provinces = self.rings.opened.clone();
                provinces.extend(
                    self.index
                        .provinces
                        .iter()
                        .map(|&(p, q)| (p as i32, q as i32)),
                );
                // Only the queued material and nudges leave the API's state
                // (W6T-2): the status, tracks and dedupe keys stay served.
                let mut shared = {
                    let mut s = self.shared.lock().unwrap_or_else(|p| p.into_inner());
                    Shared {
                        reveals: std::mem::take(&mut s.reveals),
                        nudges: std::mem::take(&mut s.nudges),
                        ..Shared::default()
                    }
                };
                let r2 = self
                    .play
                    .plan_core(
                        &t,
                        &self.port,
                        &self.drand,
                        &mut self.rounds,
                        &mut self.engine,
                        &self.beacon.anchors,
                        &mut self.seeds,
                        &mut shared,
                        self.journal.as_ref(),
                        &provinces.into_iter().collect::<Vec<_>>(),
                    )
                    .await;
                self.play.guard_reveals(&t, &mut self.engine);
                let blocking = self.play.blocking(&t, &self.engine);
                {
                    // Merge back: material the play duty could not take
                    // yet stays queued ahead of what the API queued since.
                    let mut s = self.shared.lock().unwrap_or_else(|p| p.into_inner());
                    let newer_reveals = std::mem::take(&mut s.reveals);
                    let newer_nudges = std::mem::take(&mut s.nudges);
                    s.reveals = std::mem::take(&mut shared.reveals);
                    s.reveals.extend(newer_reveals);
                    s.nudges = std::mem::take(&mut shared.nudges);
                    s.nudges.extend(newer_nudges);
                    s.tracks.extend(std::mem::take(&mut shared.tracks));
                    s.blocking = blocking;
                }
                for (sig, units, meter) in self.play.index.failed_meta.drain(..) {
                    self.engine.note_tx_meta(sig, units, meter);
                }
                r2.map_err(|e| e.to_string())?;
            }
        }
        phase(&mut rep, "play", 0);
        // Send the next versions.
        self.engine
            .send(
                &self.port,
                &mut self.payers,
                self.journal.as_ref(),
                slot,
                &mut r,
            )
            .await;
        if rep.chain_at_send.is_none() && r.sent > 0 {
            rep.chain_at_send = self.engine.last_send_chain_slot;
        }
        phase(&mut rep, "send", 0);
        // Housekeeping after the send (W6T-2): closes (class N) and claims
        // plan once the latency-critical versions of this slot are out, and
        // go out in a second send.
        if play_on {
            if let Some(season) = self.season.clone() {
                let mut cfg_eff = self.cfg.clone();
                cfg_eff
                    .roles
                    .retain(|r| !self.unsupported_roles.contains(r.as_str()));
                let t = Tick {
                    slot,
                    now,
                    season: &season,
                    clock: SeasonClock::from_season(&season),
                    addrs: &self.addrs,
                    cfg: &cfg_eff,
                };
                let before = self.play.closes_reads;
                self.play
                    .housekeeping(&t, &self.port, &mut self.engine, &self.beacon.anchors)
                    .await
                    .map_err(|e| e.to_string())?;
                let reads = self.play.closes_reads - before;
                phase(&mut rep, "closes", reads);
                let sent = r.sent;
                self.engine
                    .send(
                        &self.port,
                        &mut self.payers,
                        self.journal.as_ref(),
                        slot,
                        &mut r,
                    )
                    .await;
                if rep.chain_at_send.is_none() && r.sent > sent {
                    rep.chain_at_send = self.engine.last_send_chain_slot;
                }
                phase(&mut rep, "send2", 0);
            }
        }
        rep.sent = r.sent;
        let day = bell.map_or(0, |b| b / abi::BELLS_PER_DAY);
        let fees_now: u64 = self.engine.stats.values().map(|s| s.fees_charged).sum();
        self.spend_by_day.insert(day, fees_now);
        self.note_step(slot, &r);
        rep.outcomes = r.outcomes;
        rep.contested = r.contested;
        self.publish(slot, now, bell);
        Ok(rep)
    }

    /// Alerts and duty reactions for what a step reported: dead and
    /// failed writes, contested bells, retry-ladder rungs.
    fn note_step(&mut self, slot: u64, r: &crate::engine::StepReport) {
        for (k, o) in &r.outcomes {
            match o {
                Outcome::Dead { code, reason } => {
                    let land_role =
                        role_of_key(k).filter(|_| *code == Some(abi::err::NOT_IMPLEMENTED));
                    if let Some(role) = land_role {
                        if self.unsupported_roles.insert(role) {
                            self.alert(
                                slot,
                                "not-implemented",
                                format!("{k}: {reason}; role `{role}` off (the program lacks it)"),
                            );
                        }
                    } else if self.rings.on_dead(k, *code) || self.archive.on_dead(k, *code) {
                        self.alert(
                            slot,
                            "not-implemented",
                            format!("{k}: {reason}; the program lacks this instruction"),
                        );
                    } else {
                        self.alert(slot, "dead", format!("{k}: {reason}"));
                    }
                }
                Outcome::Landed { .. } => {}
                Outcome::Failed { code, err, .. } => {
                    // Expected refusals of a re-planned write: an anchor or
                    // seed not there yet, a fold part of an older bell, a
                    // ticket whose preference moved (another settler won).
                    let expected = [
                        abi::err::NO_ANCHOR,
                        abi::err::SEED_NOT_READY,
                        abi::err::FOLD_STALE,
                        abi::err::TICKET_STATE,
                        // Play (W4-C): a skip that meets a contested bell
                        // (gathered and resolved instead), a slot index
                        // another Reveal took, a quota refusal (settled
                        // bounced), a settlement or gather a little early.
                        abi::err::NOT_QUIET,
                        abi::err::SLOT_MOVED,
                        abi::err::QUOTA_REFUSED,
                        abi::err::TOO_EARLY,
                        abi::err::DEPARTURE_UNSETTLED,
                    ];
                    if !code.is_some_and(|c| expected.contains(&c)) {
                        self.alert(slot, "failed", format!("{k}: {code:?} {err}"));
                    }
                }
            }
        }
        for (b, rg, k) in &r.contested {
            self.beacon.on_contested(*b);
            self.alert(
                slot,
                "contested",
                format!("{k} (bell {b:?}, region {rg:?})"),
            );
        }
        for (k, cu, heap) in &r.ladder {
            self.alert(slot, "retry-ladder", format!("{k}: cu {cu}, heap {heap:?}"));
        }
    }

    /// Top-ups and sweeps as class N transfers signed by their source.
    fn plan_care(&mut self, slot: u64) -> usize {
        let plan = self.payers.care_plan();
        let n = plan.len();
        for tr in plan {
            let ix = tr.instruction();
            self.engine.ensure(
                WriteSpec {
                    key: format!("care:{}:{}:{slot}", tr.from, tr.to),
                    kind: "payer-care",
                    tag: 0,
                    class: Class::N,
                    bell: None,
                    region: None,
                    build: Arc::new(move |_: &BuildCtx| vec![ix.clone()]),
                    deadline_slot: None,
                    not_before_slot: 0,
                    fixed_payer: Some(tr.from),
                },
                slot,
            );
        }
        n
    }

    /// The status JSON (`GET /v1/status`).
    pub fn status_json(&self, slot: u64, now: i64, bell: Option<u32>) -> Value {
        let kinds: BTreeMap<&str, Value> = self
            .engine
            .stats
            .iter()
            .map(|(k, s)| {
                (
                    *k,
                    json!({
                        "writes": s.writes, "versions": s.versions, "landed": s.landed, "failed": s.failed,
                        "dead": s.dead, "fees_charged": s.fees_charged,
                        "latency_slots_p50": quantile(&s.latency_slots, 0.5),
                        "latency_slots_p99": quantile(&s.latency_slots, 0.99),
                    }),
                )
            })
            .collect();
        let pool_sum = |ks: Vec<Address>| ks.iter().map(|k| self.payers.balance(k)).sum::<u64>();
        json!({
            "slot": slot, "now": now, "bell": bell,
            "season": self.season.as_ref().map(|s| json!({
                "id": s.h.season_id, "status": s.effective_status(now), "genesis_ts": s.genesis_ts,
                "genesis_round": s.genesis_round,
            })),
            "duties": {
                "next_bell": self.beacon.next_bell,
                "anchors_known": self.beacon.anchors.len(),
                "caches_known": self.beacon.anchors.values().filter(|a| a.cache.is_some()).count(),
                "contested_bells": self.beacon.contested_bells.iter().collect::<Vec<_>>(),
                "anchor_latency_slots_p99": quantile(&self.beacon.anchor_latency, 0.99),
                "seed_latency_slots_p99": quantile(&self.beacon.seed_latency, 0.99),
                "provinces_opened": self.rings.opened.len(),
                "rings_complete": self.rings.complete.iter().collect::<Vec<_>>(),
                "ring_waiting": self.rings.waiting.as_ref().map(|w| format!("{w:?}")),
                "rings_unsupported": self.rings.unsupported,
                "folded_bells": self.fold.folded.len(),
                "tickets_open": self.index.tickets.len(),
                "tickets_settled": self.tickets.settled.len(),
                "tickets_expired": self.tickets.expired,
                "ticket_winner_latency_slots_p99": quantile(&self.tickets.winner_latency(), 0.99),
                "explores_sent": self.explore.sent.len(),
                "releases_sent": self.holdings.releases.len(),
                "sweeps_sent": self.holdings.sweeps.len(),
                "disbands_sent": self.holdings.disbands.len(),
                "land_unsupported": self.unsupported_roles.iter().collect::<Vec<_>>(),
                "archive_unsupported": self.archive.unsupported,
                "archived_bells": self.archive.archived_bells,
            },
            "pending": self.engine.pending_len(),
            "kinds": kinds,
            "pools": {
                "reveal": {"n": self.payers.reveal.keys.len(), "effective_n": self.payers.effective_n(Class::W),
                           "floor": self.payers.reveal.floor, "lamports": pool_sum(self.payers.reveal.addresses())},
                "delay": {"n": self.payers.delay.keys.len(), "effective_n": self.payers.effective_n(Class::D),
                          "floor": self.payers.delay.floor, "lamports": pool_sum(self.payers.delay.addresses())},
                "funders": {"n": self.payers.funders.keys.len(),
                            "lamports": self.payers.funders.keys.iter().map(|k| self.payers.balance(&solana_signer::Signer::pubkey(k))).sum::<u64>()},
            },
            "params": {
                "p_tip_milli": self.engine.params.p_tip_milli, "p_start_milli": self.engine.params.p_start_milli,
                "p_def_milli": self.engine.params.p_def_milli, "p_delay_milli": self.engine.params.p_delay_milli,
            },
            "play": self.play.status(),
            "spend_by_day": self.spend_by_day,
            "alerts": self.alerts.len(),
        })
    }

    /// Prometheus text (`GET /metrics`).
    pub fn metrics_text(&self, slot: u64, bell: Option<u32>) -> String {
        let mut m = String::new();
        let mut line = |k: &str, v: String| {
            m.push_str(k);
            m.push(' ');
            m.push_str(&v);
            m.push('\n');
        };
        line("frontier_keeper_slot", slot.to_string());
        line(
            "frontier_keeper_bell",
            bell.map_or("-1".into(), |b| b.to_string()),
        );
        for (k, s) in &self.engine.stats {
            line(
                &format!("frontier_keeper_writes_total{{kind=\"{k}\"}}"),
                s.writes.to_string(),
            );
            line(
                &format!("frontier_keeper_versions_total{{kind=\"{k}\"}}"),
                s.versions.to_string(),
            );
            line(
                &format!("frontier_keeper_landed_total{{kind=\"{k}\"}}"),
                s.landed.to_string(),
            );
            line(
                &format!("frontier_keeper_failed_total{{kind=\"{k}\"}}"),
                s.failed.to_string(),
            );
            line(
                &format!("frontier_keeper_fees_lamports_total{{kind=\"{k}\"}}"),
                s.fees_charged.to_string(),
            );
            if let Some(p) = quantile(&s.latency_slots, 0.99) {
                line(
                    &format!("frontier_keeper_latency_slots{{kind=\"{k}\",q=\"0.99\"}}"),
                    p.to_string(),
                );
            }
        }
        for (pool, class) in [("reveal", Class::W), ("delay", Class::D)] {
            line(
                &format!("frontier_keeper_pool_effective_n{{pool=\"{pool}\"}}"),
                self.payers.effective_n(class).to_string(),
            );
        }
        line(
            "frontier_keeper_contested_bells",
            self.beacon.contested_bells.len().to_string(),
        );
        if let Some(p) = quantile(&self.beacon.anchor_latency, 0.99) {
            line(
                "frontier_keeper_anchor_latency_slots{q=\"0.99\"}",
                p.to_string(),
            );
        }
        if let Some(p) = quantile(&self.beacon.seed_latency, 0.99) {
            line(
                "frontier_keeper_seed_latency_slots{q=\"0.99\"}",
                p.to_string(),
            );
        }
        line(
            "frontier_keeper_alerts_total",
            self.alerts.len().to_string(),
        );
        line(
            "frontier_keeper_rounds_bad_total",
            self.rounds.bad.to_string(),
        );
        let ps = &self.play.stats;
        for (k, v) in [
            ("seals_opened", ps.opened),
            ("bad_seals", ps.bad_seals),
            ("reveals_sent", ps.reveals_sent),
            ("reveals_landed", ps.reveals_landed),
            ("reveals_refused", ps.reveals_refused),
            ("reveals_missed", ps.reveals_missed),
            ("slot_moved", ps.slot_moved),
            ("skipped_bells", ps.skipped_bells),
        ] {
            line(&format!("frontier_keeper_play_{k}_total"), v.to_string());
        }
        if let Some(p) = quantile(&ps.resolve_latency, 0.99) {
            line(
                "frontier_keeper_close_to_resolve_slots{q=\"0.99\"}",
                p.to_string(),
            );
        }
        m
    }

    fn publish(&self, slot: u64, now: i64, bell: Option<u32>) {
        let status = self.status_json(slot, now, bell);
        let metrics = self.metrics_text(slot, bell);
        let mut s = self.shared.lock().unwrap_or_else(|p| p.into_inner());
        s.status = status;
        s.metrics = metrics;
    }
}

/// The land instruction tags W3-C sends, with their §5.5 class.
pub const W3C_TAGS: [(u8, Class); 9] = [
    (tag::OPEN_RING, Class::D),
    (tag::CONSUME_RING_SEED, Class::D),
    (tag::OPEN_PROVINCE, Class::D),
    (tag::FOLD_OCCUPANCY, Class::D),
    (tag::SETTLE_TICKET, Class::D),
    (tag::SETTLE_EXPLORE, Class::N),
    (tag::RELEASE_DORMANT, Class::N),
    (tag::DISBAND_STRANDED, Class::N),
    (tag::SWEEP_POOL_OWED, Class::N),
];

/// The play instruction tags W4-C sends, with their §5.5 class (the §21
/// return settle is SettleDeparture with `transit_slot = 0xFF`).
pub const W4C_TAGS: [(u8, Class); 10] = [
    (tag::REVEAL, Class::W),
    (tag::SETTLE_DEPARTURE, Class::D),
    (tag::GATHER_CLASH, Class::D),
    (tag::RESOLVE_FROM_INPUTS, Class::D),
    (tag::SKIP_QUIET, Class::D),
    (tag::SETTLE_TRANSIT, Class::D),
    (tag::CLAIM_DEFENCE, Class::D),
    (tag::CLOSE_CLASH_INPUTS, Class::N),
    (tag::CLOSE_ARRIVAL_DAY, Class::N),
    (tag::CLOSE_ARRIVAL_SLOT, Class::N),
];

/// The keeper-write instruction tags this unit sends (budget coverage test).
pub const W2F_TAGS: [u8; 9] = [
    tag::CONSUME_GENESIS_SEED,
    tag::POST_ANCHOR,
    tag::POST_ANCHOR_MULTI,
    tag::POST_SEED,
    tag::POST_BEACON,
    tag::OPEN_RING,
    tag::OPEN_PROVINCE,
    tag::ARCHIVE_ANCHORS,
    tag::CLOSE_SEED_CACHE,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// W6-C (W5-B F1): payer care runs on a game-time cadence as well as a
    /// slot one, at once (after a block) when a pool is below its minimum
    /// and the last care planned top-ups, and never over pending transfers.
    #[test]
    fn payer_care_cadence_follows_game_time_and_low_pools() {
        let c = KeeperConfig::new(
            Address::new_from_array([1; 32]),
            1,
            Address::new_from_array([2; 32]),
        );
        assert_eq!((c.care_every_slots, c.care_every_game_secs), (150, 300));
        assert!(care_due(&c, None, 10, 1_000, false, false), "first tick");
        assert!(
            !care_due(&c, None, 10, 1_000, true, true),
            "never over pending care"
        );
        let last = Some((10, 1_000, 0));
        // At 100× a slot is 40 game s: 300 game s pass after 8 slots, long
        // before 150 slots (the old cadence: 10 bells).
        assert!(!care_due(&c, last, 17, 1_280, false, false));
        assert!(care_due(&c, last, 18, 1_320, false, false));
        // At 1× (0.4 game s a slot) the slot cadence comes first.
        assert!(!care_due(&c, last, 159, 1_060, false, false));
        assert!(care_due(&c, last, 160, 1_060, false, false));
        // A low pool: at once after a care that planned top-ups, but not
        // when the last care found nothing to fund.
        let topped = Some((10, 1_000, 150));
        assert!(!care_due(&c, topped, 11, 1_040, true, false));
        assert!(care_due(&c, topped, 12, 1_080, true, false));
        assert!(!care_due(&c, last, 12, 1_080, true, false));
        assert!(!care_due(&c, topped, 12, 1_080, true, true));
    }

    #[test]
    fn escalation_by_class() {
        assert_eq!(bid_milli(Class::W, 433, 0, 2_000), 433);
        assert_eq!(bid_milli(Class::W, 433, 1, 2_000), 866);
        assert_eq!(bid_milli(Class::W, 433, 3, 2_000), 2_000);
        assert_eq!(bid_milli(Class::W, 433, 9, 1_500), 1_500, "season cap");
        assert_eq!(bid_milli(Class::D, 433, 2, 2_000), 500);
        assert_eq!(bid_milli(Class::N, 433, 9, 2_000), P_LOW_MILLI);
    }

    #[test]
    fn quantiles_and_classes_of_the_w2f_writes() {
        assert_eq!(quantile(&[], 0.99), None);
        assert_eq!(quantile(&[3, 1, 2], 0.5), Some(2));
        assert_eq!(quantile(&(1..=100).collect::<Vec<u64>>(), 0.99), Some(99));
        // §5.5 / I-21: every keeper write of this unit is D except the
        // beacon log and the cache close (N).
        for t in W2F_TAGS {
            let c = abi::ix_info(t).unwrap().class;
            let want = if t == tag::POST_BEACON || t == tag::CLOSE_SEED_CACHE {
                Class::N
            } else {
                Class::D
            };
            assert_eq!(c, want, "tag {t:#x}");
        }
    }

    #[test]
    fn land_writes_have_their_contract_class_and_a_budget() {
        let b = Budgets::from_json(&serde_json::from_str(CANONICAL_BUDGETS).unwrap()).unwrap();
        for (t, class) in W3C_TAGS {
            assert_eq!(abi::ix_info(t).unwrap().class, class, "tag {t:#x}");
            let x = b.get(t);
            assert!(x.cu_limit > 0 && x.loaded_limit >= fees::PAGE, "tag {t:#x}");
        }
        assert_eq!(role_of_key("ticket:x:5:0"), Some("tickets"));
        assert_eq!(role_of_key("stranded:2,0:9"), Some("dormancy"));
        assert_eq!(role_of_key("anchor:1:2"), None);
    }

    #[test]
    fn play_writes_have_their_contract_class_and_a_budget() {
        let b = Budgets::from_json(&serde_json::from_str(CANONICAL_BUDGETS).unwrap()).unwrap();
        for (t, class) in W4C_TAGS {
            assert_eq!(abi::ix_info(t).unwrap().class, class, "tag {t:#x}");
            let x = b.get(t);
            assert!(x.cu_limit > 0 && x.loaded_limit >= fees::PAGE, "tag {t:#x}");
        }
        for (k, role) in [
            ("reveal:1:2", "reveal"),
            ("sdep:1:2:3,4", "settle-departure"),
            ("return:1,2,3:3,4", "settle-departure"),
            ("gather:3,4:9:0", "gather"),
            ("resolve:3,4:9", "resolve"),
            ("skip:3,4:9:24", "skip"),
            ("settle:1:2", "settle"),
            ("close:slot:3,4:9:0:1", "close"),
            ("claim:0:x", "claims"),
        ] {
            assert_eq!(role_of_key(k), Some(role), "{k}");
            assert!(PLAY_ROLES.contains(&role));
        }
    }
}
