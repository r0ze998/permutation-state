//! One bot: observe (herald) → decide (`frontier_agents::policy`) → act
//! (relay, keeper link, own key), and the environment 1,000 bots share.
//!
//! **Pacing.** Sponsored writes stay inside the relay's quota (§8.3 D4:
//! 40 a citizen a game day on days 0–6, then 20): a bot spends at most
//! what `/h/me`'s `quota.left` says minus a reserve of [`QUOTA_RESERVE`]
//! kept for its marches and settlements (the spammer ignores it, which is
//! the point). The on-chain bucket (30/h, burst 60) is wider than the
//! quota on every day, so the quota binds first.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use fclient::abi::tag;
use fclient::addr::Addresses;
use fclient::budgets::Budgets;
use fclient::clock::GameClock;
use fclient::ix::{self, DepartArgs, Player};
use fclient::ports::ClockSysvar;
use fclient::{Address, Instruction, Keypair, Signer};
use frontier_agents::obs::{
    b64_encode, BellView, MeView, Observation, Overview, ProvinceView, SeasonView,
};
use frontier_agents::policy::{
    self, Ctx, DepartPlan, Intent, MarchMemo, Memory, RevealRoute, Route,
};
use frontier_agents::profile::{AgentSpec, BELLS_PER_DAY};
use frontier_agents::{keys, Persona};
use permutation_rules::frontier::geometry::ProvinceCoord;
use serde_json::{json, Value};

use crate::journal::Journal;
use crate::ports::{Answer, DirectPort, HeraldPort, RelayPort};
use crate::report::{Outcome, Report};
use crate::seal::SealPool;
use crate::txb::{self, RelayInfo};

/// Sponsored transactions a bot keeps for marches and settlements.
pub const QUOTA_RESERVE: u32 = 6;
/// Provinces a bot reads per decision (the web's tile-LOD limit, §4.2).
pub const PROVINCE_LIMIT: usize = 12;
/// Lamports airdropped to a persona's own key on first use.
pub const DIRECT_FUNDS: u64 = 5_000_000_000;
/// Session keys live 29 days (Join allows ≤ 30).
pub const SESSION_SECS: i64 = 29 * 86_400;

/// Where game time comes from.
pub enum ClockSource {
    /// A fixed game time (unit tests; `set` moves it).
    Fixed(AtomicI64),
    /// The herald's `latestSlot`/`latestUnix`, extrapolated (`GameClock`).
    Game(Mutex<GameClock>),
}

impl ClockSource {
    pub fn fixed(t: i64) -> ClockSource {
        ClockSource::Fixed(AtomicI64::new(t))
    }
    pub fn set(&self, t: i64) {
        if let ClockSource::Fixed(a) = self {
            a.store(t, Ordering::SeqCst);
        }
    }
    pub fn observe(&self, slot: u64, unix: i64) {
        if let ClockSource::Game(g) = self {
            g.lock().expect("clock").observe(
                ClockSysvar {
                    slot,
                    unix_timestamp: unix,
                    ..ClockSysvar::default()
                },
                Instant::now(),
            );
        }
    }
    pub fn now(&self) -> Option<i64> {
        match self {
            ClockSource::Fixed(a) => Some(a.load(Ordering::SeqCst)),
            ClockSource::Game(g) => g.lock().expect("clock").now(),
        }
    }
    /// The last game time actually observed (the herald's `latestUnix`),
    /// not extrapolated.
    pub fn observed(&self) -> Option<i64> {
        match self {
            ClockSource::Fixed(a) => Some(a.load(Ordering::SeqCst)),
            ClockSource::Game(g) => g.lock().expect("clock").last().map(|c| c.unix_timestamp),
        }
    }
    /// Game seconds per wall second.
    pub fn scale(&self) -> f64 {
        match self {
            ClockSource::Fixed(_) => 1.0,
            ClockSource::Game(g) => g.lock().expect("clock").scale(),
        }
    }
    /// Wall time until game time `t` (zero on a fixed clock).
    pub fn wall_until(&self, t: i64) -> Duration {
        match self {
            ClockSource::Fixed(_) => Duration::ZERO,
            ClockSource::Game(g) => {
                let s = g
                    .lock()
                    .expect("clock")
                    .wall_secs_until(t, Instant::now())
                    .unwrap_or(1.0);
                Duration::from_secs_f64(s.clamp(0.0, 3_600.0))
            }
        }
    }
}

/// Fleet settings.
pub struct Config {
    pub seed: u64,
    pub budgets: Budgets,
    /// Invite tokens for a gated season, one per bot index (if any).
    pub invites: Vec<String>,
    /// Wall time a cached season or overview file stays fresh.
    pub cache_ttl: Duration,
    /// Game seconds per slot when the clock cannot tell (0.4 × scale).
    pub slot_game_secs: f64,
    /// Persona bots run a session every bell instead of their archetype's
    /// sessions (a one-game-day run, e.g. W4-F's `inproc_day`, then reaches
    /// every persona's test; off by default).
    pub eager_personas: bool,
}

impl Config {
    pub fn new(seed: u64) -> Config {
        Config {
            seed,
            budgets: txb::budgets(),
            invites: vec![],
            cache_ttl: Duration::from_millis(1_500),
            slot_game_secs: 8.0,
            eager_personas: false,
        }
    }
    pub fn reveal_loaded_limit(&self) -> u32 {
        self.budgets.get(tag::REVEAL).loaded_limit
    }
}

#[derive(Default)]
struct Cache {
    season: Option<(Instant, SeasonView)>,
    overviews: BTreeMap<u16, (Instant, Option<Overview>)>,
}

/// What every bot of a process shares.
pub struct Shared<H, R, D> {
    pub herald: H,
    pub relay: R,
    pub direct: Option<D>,
    pub journal: Option<Journal>,
    pub pool: SealPool,
    pub cfg: Config,
    pub clock: ClockSource,
    pub report: Mutex<Report>,
    cache: Mutex<Cache>,
}

impl<H: HeraldPort, R: RelayPort, D: DirectPort> Shared<H, R, D> {
    pub fn new(herald: H, relay: R, direct: Option<D>, cfg: Config, clock: ClockSource) -> Self {
        Shared {
            herald,
            relay,
            direct,
            journal: None,
            pool: SealPool::default_size(),
            cfg,
            clock,
            report: Mutex::new(Report::default()),
            cache: Mutex::new(Cache::default()),
        }
    }

    pub fn with_journal(mut self, j: Journal) -> Self {
        self.journal = Some(j);
        self
    }

    fn fresh(&self, at: Instant) -> bool {
        at.elapsed() < self.cfg.cache_ttl
    }

    /// `/h/season` (shared, cached for `cache_ttl`); feeds the game clock.
    pub async fn season(&self) -> Result<SeasonView, String> {
        if let Some((at, v)) = &self.cache.lock().expect("cache").season {
            if self.fresh(*at) {
                return Ok(v.clone());
            }
        }
        let b = self
            .herald
            .get("/h/season")
            .await
            .map_err(|e| format!("/h/season: {e}"))?
            .ok_or("/h/season: 404 (no season yet)")?;
        let v: Value = serde_json::from_slice(&b).map_err(|e| e.to_string())?;
        let s = SeasonView::from_json(&v).map_err(|e| e.to_string())?;
        self.clock.observe(s.latest_slot, s.latest_unix);
        self.cache.lock().expect("cache").season = Some((Instant::now(), s.clone()));
        Ok(s)
    }

    /// `/h/overview/{ring}/latest.bin` (shared, cached).
    pub async fn overview(&self, ring: u16) -> Option<Overview> {
        if let Some((at, v)) = self.cache.lock().expect("cache").overviews.get(&ring) {
            if self.fresh(*at) {
                return v.clone();
            }
        }
        let v = match self
            .herald
            .get(&format!("/h/overview/{ring}/latest.bin"))
            .await
        {
            Ok(Some(b)) => Overview::decode(&b).ok(),
            _ => None,
        };
        self.cache
            .lock()
            .expect("cache")
            .overviews
            .insert(ring, (Instant::now(), v.clone()));
        v
    }
}

impl<H, R, D> Shared<H, R, D> {
    pub fn record(&self, o: Outcome) {
        self.report.lock().expect("report").record(o);
    }

    pub fn error(&self, what: &str) {
        self.report.lock().expect("report").error(what);
    }

    pub fn now(&self) -> Option<i64> {
        self.clock.now()
    }
}

async fn json_of<H: HeraldPort>(h: &H, path: &str) -> Option<Value> {
    match h.get(path).await {
        Ok(Some(b)) => serde_json::from_slice(&b).ok(),
        _ => None,
    }
}

/// One bot.
pub struct Bot {
    pub spec: AgentSpec,
    pub wallet: Keypair,
    pub session: Keypair,
    pub direct: Keypair,
    pub mem: Memory,
    funded: bool,
    /// The Citizen's address once joined (the relay's quota view).
    citizen: Option<Address>,
    /// Sponsored transactions spent since the herald's last quota figure.
    spent: u32,
    /// This step held a resident action back and nudged its province
    /// (integ-W4 review): the session is tried again shortly (`Pace`).
    pub nudged: bool,
    /// Session retries after a nudge (at most [`NUDGE_RETRIES`]).
    pub nudge_retries: u8,
}

/// Session retries after a nudge before the bot gives the session up.
pub const NUDGE_RETRIES: u8 = 3;

/// The kinds of ways an action went.
fn outcome(
    bot: &Bot,
    action: &'static str,
    route: &'static str,
    a: &Answer,
    tag: Option<&'static str>,
) -> Outcome {
    Outcome {
        bot: bot.spec.index,
        arch: bot.spec.arch,
        persona: bot.spec.persona,
        action,
        route,
        ok: a.ok(),
        status: a.status,
        code: a.code(),
        signature: a.signature(),
        tag,
    }
}

impl Bot {
    pub fn new(spec: AgentSpec, seed: u64) -> Bot {
        Bot {
            spec,
            wallet: keys::wallet(seed, spec.index),
            session: keys::session(seed, spec.index),
            direct: keys::direct(seed, spec.index),
            mem: Memory::default(),
            funded: false,
            citizen: None,
            spent: 0,
            nudged: false,
            nudge_retries: 0,
        }
    }

    /// Reads what one decision needs (the herald only).
    pub async fn observe<H: HeraldPort, R: RelayPort, D: DirectPort>(
        &self,
        sh: &Shared<H, R, D>,
    ) -> Result<Observation, String> {
        let season = sh.season().await?;
        let now = sh.now().unwrap_or(season.latest_unix);
        let wallet = self.wallet.pubkey();
        let me = match json_of(&sh.herald, &format!("/h/me/{wallet}")).await {
            Some(v) => MeView::from_json(&v).map_err(|e| e.to_string())?,
            None => MeView::empty(wallet),
        };
        let mut overviews = vec![];
        for d in 1..season.rings_opened().max(3) {
            if let Some(o) = sh.overview(d).await {
                overviews.push(o);
            }
        }
        // Provinces: own holdings and their neighbours, the hosts' and the
        // marches' provinces, and (without a holding) ticket candidates.
        let opened: std::collections::BTreeSet<(i16, i16)> = overviews
            .iter()
            .flat_map(|o| o.provinces.iter().map(|r| (r.p, r.q)))
            .collect();
        let mut want: Vec<(i16, i16)> = vec![];
        let add = |k: (i16, i16), want: &mut Vec<(i16, i16)>| {
            if !want.contains(&k) && (opened.is_empty() || opened.contains(&k)) {
                want.push(k);
            }
        };
        for (_, h) in &me.holdings {
            add((h.p, h.q), &mut want);
        }
        for m in &self.mem.marches {
            add((m.dest.0 as i16, m.dest.1 as i16), &mut want);
        }
        for (_, h) in &me.holdings {
            for n in ProvinceCoord::new(h.p as i32, h.q as i32).neighbors() {
                add((n.p as i16, n.q as i16), &mut want);
            }
        }
        if me.holdings.is_empty() {
            if let Some((_, cz)) = &me.citizen {
                for o in &overviews {
                    for r in &o.provinces {
                        let pc = ProvinceCoord::new(r.p as i32, r.q as i32);
                        if pc.ring() >= 2
                            && pc.wedge() == Some(cz.faction % 6)
                            && r.free_sites(12).next().is_some()
                        {
                            add((r.p, r.q), &mut want);
                        }
                    }
                }
            }
        }
        want.truncate(PROVINCE_LIMIT);
        let mut provinces = BTreeMap::new();
        for (p, q) in want {
            if let Some(v) = json_of(&sh.herald, &format!("/h/province/{p},{q}/latest")).await {
                if let Ok(pv) = ProvinceView::from_json(&v) {
                    provinces.insert(pv.coord(), pv);
                }
            }
        }
        // The arrival bell's own envelope of every march whose destination
        // resolved past it (SettleTransit's slot and resolver).
        let mut province_bells = BTreeMap::new();
        for m in self.mem.marches.iter().filter(|m| !m.settled) {
            let pq = (m.dest.0 as i16, m.dest.1 as i16);
            let past = provinces
                .get(&pq)
                .is_some_and(|v: &ProvinceView| v.province.resolved_next > m.arrive_bell);
            if !past || province_bells.contains_key(&(pq, m.arrive_bell)) {
                continue;
            }
            let path = format!("/h/province/{},{}/{}", pq.0, pq.1, m.arrive_bell);
            if let Some(v) = json_of(&sh.herald, &path).await {
                if let Ok(pv) = ProvinceView::from_json(&v) {
                    province_bells.insert((pq, m.arrive_bell), pv);
                }
            }
        }
        // Bell files: arrival bells of own marches, the explore record's
        // bell, a ticket's bell.
        let bell = season.bell_at(now);
        let mut bells_wanted: Vec<(u32, u8)> = vec![];
        for m in &self.mem.marches {
            if m.arrive_bell <= bell && !m.settled {
                bells_wanted.push((m.arrive_bell, ix::region_of(m.dest.0, m.dest.1)));
            }
        }
        for (_, h) in &me.holdings {
            if h.explore.state != 0 {
                bells_wanted.push((
                    h.explore.bell,
                    ix::region_of(h.explore.p as i32, h.explore.q as i32),
                ));
            }
        }
        if let Some((_, cz)) = &me.citizen {
            if cz.ticket_bell != u32::MAX && self.spec.persona == Some(Persona::TicketHolder) {
                let s = cz.ticket_sites[cz.ticket_next.min(2) as usize];
                bells_wanted.push((cz.ticket_bell, ix::region_of(s.p as i32, s.q as i32)));
            }
        }
        bells_wanted.sort();
        bells_wanted.dedup();
        let mut bells = BTreeMap::new();
        for (b, r) in bells_wanted {
            if let Some(v) = json_of(&sh.herald, &format!("/h/bell/{b}/region/{r}")).await {
                if let Ok(bv) = BellView::from_json(&v) {
                    bells.insert((bv.bell, bv.region), bv);
                }
            }
        }
        Ok(Observation {
            now,
            season,
            me,
            provinces,
            province_bells,
            overviews,
            bells,
        })
    }

    /// One decision and its actions. `session`: economy and war allowed
    /// (else duties only). Returns the number of actions sent.
    pub async fn step<H: HeraldPort, R: RelayPort, D: DirectPort>(
        &mut self,
        sh: &Shared<H, R, D>,
        session: bool,
    ) -> usize {
        self.nudged = false;
        let obs = match self.observe(sh).await {
            Ok(o) => o,
            Err(e) => {
                sh.error(&format!("observe: {}", e.split(':').next().unwrap_or("")));
                return 0;
            }
        };
        self.citizen = obs.me.citizen.as_ref().map(|c| c.0);
        self.reconcile(sh, &obs);
        let intents = {
            let cx = Ctx {
                spec: &self.spec,
                seed: sh.cfg.seed,
                wallet: self.wallet.pubkey(),
                mem: &self.mem,
                reveal_loaded_limit: sh.cfg.reveal_loaded_limit(),
                direct: sh.direct.is_some(),
                session,
            };
            policy::decide(&obs, &cx)
        };
        // The herald's quota figure is fresh for this step; count what this
        // step spends on top of it.
        self.spent = 0;
        let mut sent = 0;
        for it in intents {
            if self.quota_left(&obs) == 0 && sponsored_kind(&it) && !self.is(Persona::Spammer) {
                continue;
            }
            if sponsored_kind(&it)
                && !matches!(
                    it,
                    Intent::Depart(_) | Intent::SettleTransit { .. } | Intent::Join { .. }
                )
                && self.quota_left(&obs) <= QUOTA_RESERVE
                && !self.is(Persona::Spammer)
            {
                continue;
            }
            sent += self.act(sh, &obs, it).await;
        }
        sh.report.lock().expect("report").steps += 1;
        sent
    }

    /// Folds what the herald shows into the memory: a march whose transit
    /// record is gone after its arrival bell was settled (by the keeper or
    /// anyone); settled marches older than two game days are forgotten.
    pub fn reconcile<H, R, D>(&mut self, sh: &Shared<H, R, D>, obs: &Observation) {
        let bell = obs.bell();
        let mut done = vec![];
        for m in self.mem.marches.iter_mut().filter(|m| m.sent && !m.settled) {
            let Some((_, h)) = obs
                .me
                .holdings
                .iter()
                .find(|(_, h)| (h.p, h.q, h.site) == (m.h.p, m.h.q, m.h.site))
            else {
                continue;
            };
            if bell > m.arrive_bell && h.transit_of(m.key.0).is_none() {
                m.settled = true;
                done.push(m.key);
            }
        }
        for k in done {
            self.journal_state(sh, k, "settled");
        }
        self.mem
            .marches
            .retain(|m| !m.settled || m.arrive_bell + 2 * BELLS_PER_DAY > bell);
    }

    fn is(&self, p: Persona) -> bool {
        self.spec.persona == Some(p)
    }

    fn quota_left(&self, obs: &Observation) -> u32 {
        obs.me.quota_left.unwrap_or(40).saturating_sub(self.spent)
    }

    fn addresses(obs: &Observation) -> Addresses {
        Addresses::new(obs.season.program, obs.season.season_id)
    }

    fn player(&self, payer: Address) -> Player {
        Player {
            actor: self.session.pubkey(),
            payer,
            wallet: self.wallet.pubkey(),
        }
    }

    async fn relay_info<H, R: RelayPort, D>(
        &self,
        sh: &Shared<H, R, D>,
    ) -> Result<RelayInfo, String> {
        let citizen = self
            .citizen
            .map(|c| format!("?citizen={c}"))
            .unwrap_or_default();
        let a = sh
            .relay
            .get(&format!("/f/relay{citizen}"))
            .await
            .map_err(|e| e.to_string())?;
        RelayInfo::from_answer(&a)
    }

    /// A sponsored player instruction through `/f/relay` (or `/f/join`).
    async fn sponsored<H, R: RelayPort, D>(
        &mut self,
        sh: &Shared<H, R, D>,
        action: &'static str,
        tag: Option<&'static str>,
        build: impl FnOnce(&Player) -> Instruction,
    ) -> Answer {
        let info = match self.relay_info(sh).await {
            Ok(i) => i,
            Err(e) => return Answer::new(0, json!({"code": "RelayUnavailable", "error": e})),
        };
        let ix = build(&self.player(info.fee_payer));
        let is_join = ix.data.first() == Some(&tag::JOIN);
        let signer = if is_join { &self.wallet } else { &self.session };
        let t = match txb::sponsored(ix, &sh.cfg.budgets, &info, &[signer]) {
            Ok(t) => t,
            Err(e) => return Answer::new(0, json!({"code": "BuildFailed", "error": e})),
        };
        let mut body =
            json!({"tx": txb::wire_b64(&t), "lastValidBlockHeight": info.last_valid_block_height});
        let path = if is_join {
            if let Some(inv) = sh.cfg.invites.get(self.spec.index as usize) {
                body["invite"] = json!(inv);
            }
            "/f/join"
        } else {
            "/f/relay"
        };
        let a = match sh.relay.post(path, &body).await {
            Ok(a) => a,
            Err(e) => Answer::new(
                0,
                json!({"code": "RelayUnavailable", "error": e.to_string()}),
            ),
        };
        if a.ok() {
            self.spent += 1;
        }
        let o = outcome(self, action, "relay", &a, tag);
        sh.record(o);
        a
    }

    /// A settle shape through `/f/relay`, charged to this bot's citizen.
    async fn settle<H, R: RelayPort, D>(
        &mut self,
        sh: &Shared<H, R, D>,
        obs: &Observation,
        action: &'static str,
        tag: Option<&'static str>,
        build: impl FnOnce(Address) -> Instruction,
    ) -> Answer {
        let info = match self.relay_info(sh).await {
            Ok(i) => i,
            Err(e) => return Answer::new(0, json!({"code": "RelayUnavailable", "error": e})),
        };
        let t = match txb::sponsored(build(info.fee_payer), &sh.cfg.budgets, &info, &[]) {
            Ok(t) => t,
            Err(e) => return Answer::new(0, json!({"code": "BuildFailed", "error": e})),
        };
        let a0 = Self::addresses(obs);
        let body = json!({
            "tx": txb::wire_b64(&t),
            "requester": self.session.pubkey().to_string(),
            "requesterSig": b64_encode(&txb::requester_sig(&t, &self.session)),
            "citizen": a0.citizen(&self.wallet.pubkey()).to_string(),
            "lastValidBlockHeight": info.last_valid_block_height,
        });
        let a = match sh.relay.post("/f/relay", &body).await {
            Ok(a) => a,
            Err(e) => Answer::new(
                0,
                json!({"code": "RelayUnavailable", "error": e.to_string()}),
            ),
        };
        if a.ok() {
            self.spent += 1;
        }
        sh.record(outcome(self, action, "relay", &a, tag));
        a
    }

    async fn ensure_funded<H, R, D: DirectPort>(&mut self, sh: &Shared<H, R, D>) -> bool {
        let Some(d) = &sh.direct else {
            return false;
        };
        if self.funded {
            return true;
        }
        let k = self.direct.pubkey();
        if d.balance(&k).await.unwrap_or(0) >= DIRECT_FUNDS / 2
            || d.airdrop(&k, DIRECT_FUNDS).await.is_ok()
        {
            self.funded = true;
        }
        self.funded
    }

    /// A persona's own transaction: simulated (the refusal code is the
    /// outcome), then sent regardless (an adversary pays for its failures).
    async fn direct_tx<H, R, D: DirectPort>(
        &mut self,
        sh: &Shared<H, R, D>,
        action: &'static str,
        tag: Option<&'static str>,
        ixs: Vec<Instruction>,
        cu_price: u64,
        others: &[&Keypair],
    ) -> bool {
        let fail = |bot: &Bot, code: &str| Outcome {
            bot: bot.spec.index,
            arch: bot.spec.arch,
            persona: bot.spec.persona,
            action,
            route: "direct",
            ok: false,
            status: 0,
            code: Some(code.to_string()),
            signature: None,
            tag,
        };
        if !self.ensure_funded(sh).await {
            sh.record(fail(self, "NoDirectPort"));
            return false;
        }
        let d = sh.direct.as_ref().expect("funded implies a port");
        let Ok(bh) = d.blockhash().await else {
            sh.record(fail(self, "RpcUnavailable"));
            return false;
        };
        let t = match txb::direct(&ixs, &sh.cfg.budgets, cu_price, &bh, &self.direct, others) {
            Ok(t) => t,
            Err(_) => {
                sh.record(fail(self, "BuildFailed"));
                return false;
            }
        };
        let wire = fclient::tx::wire(&t);
        let sim = d.simulate(&wire).await;
        let sig = d.send(&wire).await.ok();
        let (ok, code) = match sim {
            Ok(s) if s.err.is_none() => (true, None),
            Ok(s) => (
                false,
                Some(
                    s.code
                        .and_then(fclient::abi::error_name)
                        .map(String::from)
                        .unwrap_or_else(|| s.err.unwrap_or_default()),
                ),
            ),
            Err(e) => (false, Some(format!("{e}"))),
        };
        sh.record(Outcome {
            bot: self.spec.index,
            arch: self.spec.arch,
            persona: self.spec.persona,
            action,
            route: "direct",
            ok,
            status: 0,
            code,
            signature: sig,
            tag,
        });
        ok
    }

    fn journal_state<H, R, D>(&self, sh: &Shared<H, R, D>, key: (u64, u32), state: &str) {
        if let Some(j) = &sh.journal {
            if j.state(self.spec.index, key, state).is_err() {
                sh.error("journal write");
            }
        }
    }

    /// Carries out one intent; returns the transactions or requests sent.
    pub async fn act<H: HeraldPort, R: RelayPort, D: DirectPort>(
        &mut self,
        sh: &Shared<H, R, D>,
        obs: &Observation,
        it: Intent,
    ) -> usize {
        let a = Self::addresses(obs);
        let bell = obs.bell();
        match it {
            Intent::Join { faction } => {
                let wallet = self.wallet.pubkey();
                let session = self.session.pubkey();
                let expiry = obs.now + SESSION_SECS;
                let gate = obs
                    .season
                    .season
                    .as_ref()
                    .map(|s| s.join_gate)
                    .filter(|g| g.to_bytes() != [0u8; 32]);
                self.sponsored(sh, "join", None, |p| {
                    ix::join(&a, wallet, p.payer, faction, &session, expiry, gate)
                })
                .await;
                1
            }
            Intent::FileTicket { sites } => {
                let r = self
                    .sponsored(sh, "file_ticket", None, |p| ix::file_ticket(&a, p, &sites))
                    .await;
                if r.ok() {
                    self.mem.ticket_sent_bell = Some(bell);
                }
                1
            }
            Intent::Harvest { h } => {
                self.sponsored(sh, "harvest", None, |p| ix::harvest(&a, p, h))
                    .await;
                1
            }
            Intent::Build { h, item, walls } => {
                self.sponsored(sh, "build", None, |p| ix::build_item(&a, p, h, item, walls))
                    .await;
                1
            }
            Intent::Train { h, unit, n } => {
                self.sponsored(sh, "train", None, |p| ix::train(&a, p, h, unit, n))
                    .await;
                1
            }
            Intent::Muster {
                h,
                unit,
                troops,
                tile,
            } => {
                self.sponsored(sh, "muster", None, |p| {
                    ix::muster(&a, p, h, unit, troops, tile)
                })
                .await;
                1
            }
            Intent::Explore {
                h,
                at,
                host_id,
                tiles,
            } => {
                self.sponsored(sh, "explore", None, |p| {
                    ix::explore(&a, p, h, at, host_id, &tiles)
                })
                .await;
                1
            }
            Intent::SettleExplore {
                h,
                bell,
                region,
                src,
            } => {
                let w = self.wallet.pubkey();
                self.settle(sh, obs, "settle_explore", None, |fp| {
                    ix::settle_explore(&a, fp, h, &w, bell, region, src)
                })
                .await;
                1
            }
            Intent::Depart(plan) => self.depart(sh, obs, *plan).await,
            Intent::Reveal {
                key,
                route,
                args,
                late,
            } => self.reveal(sh, obs, key, route, args, late).await,
            Intent::SettleTransit { key, args, forged } => {
                let tag = forged.then_some("forged");
                let r = self
                    .settle(sh, obs, "settle_transit", tag, |fp| {
                        let mut i = ix::settle_transit(&a, fp, &args);
                        if forged {
                            // A non-canonical slot key (account 5).
                            i.accounts[5].pubkey = Address::new_from_array([0xF0; 32]);
                        }
                        i
                    })
                    .await;
                if r.ok() && !forged {
                    if let Some(m) = self.mem.march_mut(key) {
                        m.settled = true;
                    }
                    self.journal_state(sh, key, "settled");
                }
                1
            }
            Intent::Redepart { key } => {
                let Some(m) = self.mem.march(key).cloned() else {
                    return 0;
                };
                let presets = obs.season.tip_presets(sh.cfg.reveal_loaded_limit());
                let seal: [u8; 165] = m.seal.clone().try_into().unwrap_or([0x80; 165]);
                let args = DepartArgs {
                    host_id: m.key.0,
                    commit: m.commit,
                    seal,
                    arrive_bell: m.arrive_bell,
                    tip: presets[0],
                    transit_slot: (m.transit_slot + 1) % 4,
                };
                // The host stands at the destination once it arrived.
                let at = (m.dest.0 as i16, m.dest.1 as i16);
                self.sponsored(sh, "depart", Some("redepart"), |p| {
                    ix::depart(&a, p, m.h, at, &args)
                })
                .await;
                if let Some(mm) = self.mem.march_mut(key) {
                    mm.redeparted = true;
                }
                self.journal_state(sh, key, "redeparted");
                1
            }
            Intent::Prefund { targets, lamports } => {
                let from = self.direct.pubkey();
                let ixs: Vec<Instruction> = targets
                    .iter()
                    .map(|t| fclient::tx::transfer(from, *t, lamports))
                    .collect();
                // The march that follows records `(arrive, dest)` as prefunded.
                self.direct_tx(sh, "prefund", Some("prefund"), ixs, 0, &[])
                    .await;
                1
            }
            Intent::SettleOwnTicket {
                k,
                site,
                ticket_bell,
                sites,
                src,
            } => {
                let w = self.wallet.pubkey();
                let faction = self.spec.faction;
                let payer = self.direct.pubkey();
                let i = ix::settle_ticket(
                    &a,
                    payer,
                    &w,
                    faction,
                    k,
                    site,
                    ticket_bell,
                    &sites,
                    src,
                    None,
                );
                self.direct_tx(sh, "settle_own_ticket", None, vec![i], 0, &[])
                    .await;
                self.mem.own_ticket_settled = Some(ticket_bell);
                1
            }
            Intent::Hold {
                keys,
                priority_milli,
                bells,
            } => {
                let Some(d) = &sh.direct else {
                    return 0;
                };
                let scale = sh.clock.scale();
                let slot_secs = if scale > 1.5 {
                    0.4 * scale
                } else {
                    sh.cfg.slot_game_secs
                };
                let slots =
                    ((bells as f64 * obs.season.bell_secs as f64) / slot_secs).ceil() as u64;
                let r = d.hold(&keys, priority_milli, slots).await;
                sh.record(Outcome {
                    bot: self.spec.index,
                    arch: self.spec.arch,
                    persona: self.spec.persona,
                    action: "hold",
                    route: "direct",
                    ok: r.is_ok(),
                    status: 0,
                    code: r.err().map(|e| e.to_string()),
                    signature: None,
                    tag: Some("hold"),
                });
                if let Some(h) = obs.me.holdings.first() {
                    self.mem.held = Some((h.1.p, h.1.q));
                }
                1
            }
            Intent::Nudge { province } => {
                self.nudged = true;
                let body = serde_json::json!({"province": [province.0, province.1], "bell": bell});
                let r = sh.relay.post("/f/nudge", &body).await;
                let mut rep = sh.report.lock().expect("report");
                *rep.nudges
                    .entry(if r.is_ok() { "sent" } else { "failed" })
                    .or_default() += 1;
                1
            }
            Intent::Spam { h, n } => {
                let mut sent = 0;
                for _ in 0..n {
                    let r = self
                        .sponsored(sh, "harvest", Some("spam"), |p| ix::harvest(&a, p, h))
                        .await;
                    sent += 1;
                    let stop = r.status == 429
                        || matches!(
                            r.code().as_deref(),
                            Some("QuotaExceeded" | "RateLimited" | "Bucket")
                        );
                    if stop {
                        break;
                    }
                }
                self.mem.spam_day = Some(bell / BELLS_PER_DAY);
                sent
            }
        }
    }

    async fn depart<H: HeraldPort, R: RelayPort, D: DirectPort>(
        &mut self,
        sh: &Shared<H, R, D>,
        obs: &Observation,
        plan: DepartPlan,
    ) -> usize {
        let a = Self::addresses(obs);
        let arrive = plan.plain.arrive_bell;
        let round = obs.season.tlock_round(arrive);
        let sealed = match sh
            .pool
            .seal(plan.seal, plan.plain, obs.season.drand_pk, round)
            .await
        {
            Ok(s) => s,
            Err(e) => {
                sh.error(&format!("seal: {e}"));
                return 0;
            }
        };
        let key = (plan.host_id, obs.bell());
        let memo = MarchMemo {
            key,
            h: plan.h,
            transit_slot: plan.transit_slot,
            arrive_bell: arrive,
            dest: plan.dest(),
            path_others: plan.path_others.clone(),
            plain: sealed.plain,
            salt: sealed.salt,
            commit: sealed.commit,
            seal: sealed.seal.to_vec(),
            ct_hash: sealed.ct_hash,
            tip: plan.tip,
            kind: plan.seal,
            sent: false,
            reveal_tries: 0,
            revealed: false,
            late_done: false,
            settled: false,
            redeparted: false,
        };
        // The marchbook first (§9.1): a crash after this line loses nothing.
        if let Some(j) = &sh.journal {
            if j.sealed(self.spec.index, &memo, &sealed).is_err() {
                sh.error("journal write");
                return 0;
            }
        }
        let args = DepartArgs {
            host_id: plan.host_id,
            commit: sealed.commit,
            seal: sealed.seal,
            arrive_bell: arrive,
            tip: plan.tip,
            transit_slot: plan.transit_slot,
        };
        let tag = (plan.tip == 0).then_some("zero_tip");
        let ok = match plan.route {
            Route::Relay => self
                .sponsored(sh, "depart", tag, |p| {
                    ix::depart(&a, p, plan.h, plan.host_at, &args)
                })
                .await
                .ok(),
            Route::Direct => {
                let i = ix::depart(
                    &a,
                    &self.player(self.direct.pubkey()),
                    plan.h,
                    plan.host_at,
                    &args,
                );
                let session = keys::session(sh.cfg.seed, self.spec.index);
                self.direct_tx(sh, "depart", tag, vec![i], 0, &[&session])
                    .await
            }
        };
        if ok {
            self.journal_state(sh, key, "sent");
            self.mem.marches.retain(|m| m.key != key);
            self.mem.marches.push(MarchMemo { sent: true, ..memo });
        } else {
            self.journal_state(sh, key, "failed");
        }
        if plan.tip == 0 {
            self.mem.zero_tip_days.insert(obs.bell() / BELLS_PER_DAY);
        }
        if self.is(Persona::Prefunder) {
            self.mem.prefunded.insert((arrive, plan.dest()));
        }
        1
    }

    async fn reveal<H: HeraldPort, R: RelayPort, D: DirectPort>(
        &mut self,
        sh: &Shared<H, R, D>,
        obs: &Observation,
        key: (u64, u32),
        route: RevealRoute,
        args: Option<Box<ix::RevealArgs>>,
        late: bool,
    ) -> usize {
        let Some(m) = self.mem.march(key).cloned() else {
            return 0;
        };
        let a = Self::addresses(obs);
        let tag = if late {
            Some("late")
        } else if route == RevealRoute::DirectForged {
            Some("forged")
        } else {
            None
        };
        let ok = match route {
            RevealRoute::Keeper => {
                let body = json!({
                    "holding": m.h.address(&a).to_string(),
                    "transit_slot": m.transit_slot,
                    "plain_b64": b64_encode(&m.plain),
                    "salt_b64": b64_encode(&m.salt),
                    "ct_hash_b64": b64_encode(&m.ct_hash),
                });
                let r = match sh.relay.post("/f/reveal", &body).await {
                    Ok(r) => r,
                    Err(e) => Answer::new(
                        0,
                        json!({"code": "RelayUnavailable", "error": e.to_string()}),
                    ),
                };
                sh.record(outcome(self, "reveal", "keeper", &r, tag));
                r.ok()
            }
            RevealRoute::DirectSelf | RevealRoute::DirectForged => {
                let Some(args) = args else {
                    return 0;
                };
                let mut i = ix::reveal(&a, self.direct.pubkey(), &args);
                if route == RevealRoute::DirectForged {
                    // A non-canonical anchor (account 3).
                    i.accounts[3].pubkey = Address::new_from_array([0xF1; 32]);
                }
                // Outbid the keepers' first version (self_tip wants its tip).
                let b = sh.cfg.budgets.get(tag::REVEAL);
                let p_milli = obs
                    .season
                    .season
                    .as_ref()
                    .map_or(obs.season.tip_priority_milli, |s| {
                        s.min_reveal_priority_milli
                    }) as u64
                    * 2;
                let cost = fclient::fees::cost(b.cu_limit, 1, 3, b.loaded_limit);
                let price = fclient::fees::cu_price_for(p_milli, cost, b.cu_limit);
                self.direct_tx(sh, "reveal", tag, vec![i], price, &[]).await
            }
        };
        if let Some(mm) = self.mem.march_mut(key) {
            mm.reveal_tries = mm.reveal_tries.saturating_add(1);
            if late {
                mm.late_done = true;
            } else if ok {
                mm.revealed = true;
            }
        }
        self.journal_state(sh, key, if late { "late_done" } else { "reveal_try" });
        if ok && !late {
            self.journal_state(sh, key, "revealed");
        }
        1
    }
}

/// Whether an intent spends a sponsored transaction.
fn sponsored_kind(it: &Intent) -> bool {
    match it {
        Intent::Depart(d) => d.route == Route::Relay,
        Intent::Reveal { .. }
        | Intent::Prefund { .. }
        | Intent::SettleOwnTicket { .. }
        | Intent::Hold { .. }
        | Intent::Nudge { .. } => false,
        _ => true,
    }
}
