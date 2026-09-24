//! Playable server for Game Design V5: nations, members, offices.
//!
//!     cargo run --release --bin play -- --port 4185 [--tick-seconds 30] [--ai-members 2] [--autostart]
//!     cargo run --release --bin play -- --port 4185 --chain http://127.0.0.1:4191
//!
//! Serves the web client from `web/`, `llms.txt` for agents, and a JSON API.
//! Every view, preview and AI decision is made from a nation's fogged belief
//! state (`vision`, §7.4); only validation and resolution see the full state.
//!
//! People and agents are **members** of one of the season's nations (V5 §4).
//! Members elect four officers; an officer orders within its office, every
//! member proposes, supports, votes and recalls (V5 §5).
//!
//! * human member — joins in the browser (`POST /api/join` in the lobby, or
//!   `POST /api/claim` of a member the gateway registered in chain mode),
//!   which returns a member token (`X-Member-Token`).
//! * AI member — a reference AI hosted by this server (`driver`).
//! * external member — an outside agent (e.g. joined through x402). It reads
//!   its nation's view here (`?member=M` or `?civ=N`), dry-runs with
//!   `/api/validate` and signs its own transactions.
//!
//! Vacant offices are run by the acting official (V5 §8).
//!
//! Two modes:
//! * local (default): the engine runs in this process. The season opens in
//!   a lobby; it starts when a member presses start (or at once with
//!   `--autostart`). Entry fees are simulated test USDC.
//! * `--chain http://127.0.0.1:4191`: the world is the on-chain program's
//!   (read through permutation-gateway), ticks resolve on the MagicBlock ER,
//!   and members act on chain with their own session keys.

use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::gov::{self, GovAction, GovEntry, MemberId, Role, NOBODY};
use permutation_rules::hex::Hex;
use permutation_rules::invariants;
use permutation_rules::markets::{clear_amm, AmmOrder};
use permutation_rules::orders::{check_structure, role_allows, validate_batch, Order, OrderBatch, Side};
use permutation_rules::payout::{settle, Settlement};
use permutation_rules::rng::Seed;
use permutation_rules::state::{CivId, WorldState};
use permutation_rules::tick::{resolve_tick, TickInput};
use permutation_rules::{Preset, Ruleset};
use permutation_server::api::{self, GovDto, MemberMeta, OrderDto};
use permutation_server::bots::persona_of;
use permutation_server::chainlink::{ChainLink, Snapshot, WorldMeta};
use permutation_server::driver::{local_key, Hosting, Planner};
use permutation_server::events::{diff_events, diff_gov_events};
use permutation_server::fog::{line_is_public, Fog};
use permutation_server::ledger::{hex, Ledger};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long a claimed human member stays "here" without any request.
const IDLE: Duration = Duration::from_secs(60);
/// Test entry fee of local seasons (no real funds).
const LOCAL_FEE: u64 = 10_000_000;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Host {
    Human,
    Ai,
    External,
}

impl Host {
    fn name(self) -> &'static str {
        match self {
            Host::Human => "human",
            Host::Ai => "ai",
            Host::External => "external",
        }
    }
}

struct Member {
    meta: MemberMeta,
    host: Host,
    civ: CivId,
    /// Browser token of whoever holds this human member.
    token: Option<String>,
    last_seen: Option<Instant>,
    /// Orders committed for the open tick, per office held (human officers).
    committed: [Vec<OrderDto>; 4],
    cost: [u32; 4],
    adopt: [Vec<u32>; 4],
    /// Tick for which this member ended its turn.
    ready: Option<u16>,
    /// Lobby choices, applied when the government opens (local mode).
    stand: u8,
    votes: [MemberId; 4],
    deposit: u64,
}

impl Member {
    fn here(&self) -> bool {
        self.token.is_some() && self.last_seen.is_some_and(|t| t.elapsed() < IDLE)
    }
}

/// Who is asking.
#[derive(Clone, Copy, PartialEq)]
enum Viewer {
    /// Holds this member's token: may act for it.
    Member(MemberId),
    /// Reads this nation's fogged view.
    Watch(CivId),
    Spectator,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    Lobby,
    Playing,
}

/// The hosting map handed to the AI driver (no borrow of the game).
struct Hosts {
    hosts: Vec<Host>,
}

impl Hosting for Hosts {
    fn is_ai(&self, m: MemberId) -> bool {
        self.hosts.get(m as usize) == Some(&Host::Ai)
    }
    fn is_human(&self, m: MemberId) -> bool {
        self.hosts.get(m as usize) == Some(&Host::Human)
    }
    fn key(&self, m: MemberId) -> [u8; 32] {
        local_key(m)
    }
}

struct Game {
    rules: Ruleset,
    state: WorldState,
    fog: Fog,
    ledger: Ledger,
    planner: Planner,
    members: Vec<Member>,
    phase: Phase,
    /// Human governance actions for the next tick (local mode).
    pending_gov: Vec<GovEntry>,
    deadline: Instant,
    tick_seconds: u64,
    paused: bool,
    /// `(tick, "kind|text")`, newest last; filtered per viewer when served.
    chronicle: Vec<(u16, String)>,
    last_events: Vec<String>,
    resolved_tick: Option<u16>,
    chain: Option<ChainMode>,
    token_counter: u64,
    entry_fee: u64,
}

struct ChainMode {
    link: Arc<ChainLink>,
    meta: WorldMeta,
    slot: u64,
    layer: String,
    phase: String,
    /// The gateway's /season answer: accounts, program id, member registry, pool.
    info: Value,
    last_record: Option<Value>,
    /// Tick the AI members and acting officials already submitted for.
    ai_submitted: Option<u16>,
    error: Option<String>,
}

fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn role_name(r: Role) -> String {
    format!("{r:?}")
}

fn new_member(meta: MemberMeta, host: Host, civ: CivId) -> Member {
    Member {
        meta,
        host,
        civ,
        token: None,
        last_seen: None,
        committed: Default::default(),
        cost: [0; 4],
        adopt: Default::default(),
        ready: None,
        stand: 0,
        votes: [NOBODY; 4],
        deposit: 0,
    }
}

impl Game {
    /// Local season: the world exists at tick 0; members join in the lobby.
    /// Every nation gets `ai_members` hosted AI members.
    fn new(tick_seconds: u64, ai_members: usize) -> Game {
        let rules = Ruleset::new(Preset::Blitz);
        let world: Seed = *b"permutation-state/world/play-005";
        let season: Seed = *b"permutation-state/season/play-05";
        let state = new_season(&rules, &world, &season, &nation_entries(6)).expect("genesis");
        let mut g = Game::assemble(rules, state, &world, tick_seconds, None);
        for civ in 0..g.state.civs.len() as CivId {
            for k in 0..ai_members {
                let name = format!("{}-AI{}", g.state.civs[civ as usize].name, k + 1);
                let meta = MemberMeta { name, kind: "agent".into(), attested: false };
                let id = g.join_local(civ, meta, Host::Ai).expect("AI member joins");
                // AI members stand for two offices (rotating) and vote for themselves.
                let roles = [Role::ALL[(2 * k) % 4], Role::ALL[(2 * k + 1) % 4]];
                let m = &mut g.members[id as usize];
                m.stand = roles[0].bit() | roles[1].bit();
                for r in roles {
                    m.votes[r.index()] = id;
                }
            }
        }
        g
    }

    /// Attach to a running on-chain season through the gateway.
    fn from_chain(link: Arc<ChainLink>) -> Result<Game, String> {
        // The gateway may be starting or restarting: wait for it rather than give up.
        let snap = loop {
            match link.world() {
                Ok(Some(s)) => break s,
                Ok(None) => eprintln!("waiting for the on-chain world (registration, genesis or delegation in progress)…"),
                Err(e) => eprintln!("waiting for the gateway ({e})…"),
            }
            std::thread::sleep(Duration::from_secs(2));
        };
        let info = loop {
            match link.get_json("/season") {
                Ok(v) => break v,
                Err(e) => eprintln!("waiting for the gateway ({e})…"),
            }
            std::thread::sleep(Duration::from_secs(2));
        };
        let mut rules = match snap.meta.preset {
            1 => Ruleset::new(Preset::Season),
            _ => Ruleset::new(Preset::Blitz),
        };
        rules.market_enabled = snap.meta.market;
        let tick_seconds = snap.meta.tick_seconds as u64;
        let season_id = snap.meta.season_id.to_le_bytes();
        let chain = ChainMode {
            link,
            meta: snap.meta.clone(),
            slot: snap.slot,
            layer: snap.layer.clone(),
            phase: snap.phase.clone(),
            info: info.clone(),
            last_record: None,
            ai_submitted: None,
            error: None,
        };
        let mut g = Game::assemble(rules, snap.state, &season_id, tick_seconds, Some(chain));
        g.entry_fee = info["season"]["entryFee"].as_str().and_then(|x| x.parse().ok()).unwrap_or(LOCAL_FEE);
        g.sync_registry(&info);
        g.phase = if g.state.nations.iter().any(|n| n.offices.iter().any(|o| *o != NOBODY)) || g.state.tick > 0 || !g.state.members.is_empty() { Phase::Playing } else { Phase::Lobby };
        Ok(g)
    }

    fn assemble(rules: Ruleset, state: WorldState, seed: &[u8], tick_seconds: u64, chain: Option<ChainMode>) -> Game {
        let fog = Fog::new(&state);
        let mut ledger = Ledger::new(seed);
        ledger.observe(&state, &fog);
        let local = chain.is_none();
        let planner = Planner::new(state.civs.len());
        Game {
            rules,
            state,
            fog,
            ledger,
            planner,
            members: Vec::new(),
            phase: if local { Phase::Lobby } else { Phase::Playing },
            pending_gov: Vec::new(),
            deadline: Instant::now() + Duration::from_secs(tick_seconds),
            tick_seconds,
            paused: false,
            chronicle: Vec::new(),
            last_events: Vec::new(),
            resolved_tick: None,
            chain,
            token_counter: 0,
            entry_fee: LOCAL_FEE,
        }
    }

    /// Chain mode: the gateway's member registry (names, kinds, who hosts whom).
    fn sync_registry(&mut self, info: &Value) {
        for (i, m) in info["members"].as_array().cloned().unwrap_or_default().iter().enumerate() {
            let host = match m["hosted"].as_str() {
                Some("human") => Host::Human,
                Some("ai") => Host::Ai,
                _ => Host::External,
            };
            let meta = MemberMeta {
                name: m["name"].as_str().unwrap_or("member").to_string(),
                kind: m["kind"].as_str().unwrap_or("undeclared").to_string(),
                attested: m["attested"].as_bool().unwrap_or(false),
            };
            let civ = m["civ"].as_u64().unwrap_or(0) as CivId;
            if i < self.members.len() {
                self.members[i].meta = meta;
                self.members[i].host = host;
            } else {
                self.members.push(new_member(meta, host, civ));
            }
        }
    }

    fn hosts(&self) -> Hosts {
        Hosts { hosts: self.members.iter().map(|m| m.host).collect() }
    }

    fn metas(&self) -> Vec<MemberMeta> {
        self.members.iter().map(|m| m.meta.clone()).collect()
    }

    fn names(&self) -> Vec<String> {
        self.state.civs.iter().map(|c| c.name.clone()).collect()
    }

    fn over(&self) -> bool {
        self.state.tick >= self.rules.ticks_per_season
    }

    fn token(&mut self, salt: u64) -> String {
        self.token_counter += 1;
        let mut h = Sha256::new();
        h.update(b"PS/member-token");
        h.update(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0).to_le_bytes());
        h.update(std::process::id().to_le_bytes());
        h.update(self.token_counter.to_le_bytes());
        h.update(salt.to_le_bytes());
        hex(&h.finalize()[..16])
    }

    fn viewer(&self, req: &Request) -> Viewer {
        let t = req.header("x-member-token").or_else(|| req.header("x-seat-token")).or_else(|| req.q("token"));
        if let Some(t) = t.filter(|t| !t.is_empty()) {
            if let Some(i) = self.members.iter().position(|m| m.token.as_deref() == Some(t)) {
                return Viewer::Member(i as MemberId);
            }
        }
        if let Some(m) = req.qi::<MemberId>("member").filter(|m| (*m as usize) < self.state.members.len()) {
            return Viewer::Watch(self.state.members[m as usize].civ);
        }
        match req.qi::<CivId>("civ") {
            Some(c) if (c as usize) < self.state.civs.len() => Viewer::Watch(c),
            _ => Viewer::Spectator,
        }
    }

    fn civ_of(&self, v: Viewer) -> Option<CivId> {
        match v {
            Viewer::Member(m) => self.members.get(m as usize).map(|x| x.civ),
            Viewer::Watch(c) => Some(c),
            Viewer::Spectator => None,
        }
    }

    // -------------------------------------------------------------- lobby (local)

    fn join_local(&mut self, civ: CivId, meta: MemberMeta, host: Host) -> Result<MemberId, String> {
        if self.phase != Phase::Lobby || self.chain.is_some() {
            return Err("registration is closed".into());
        }
        let next = self.state.members.len() as MemberId;
        let id = gov::join(&mut self.state, &self.rules, civ, local_key(next)).map_err(|e| e.to_string())?;
        self.members.push(new_member(meta, host, civ));
        Ok(id)
    }

    /// Close the lobby: apply everyone's candidacy and votes, hold the first election.
    fn start(&mut self) -> Result<(), String> {
        if self.phase != Phase::Lobby {
            return Err("the season has already started".into());
        }
        // AI members defer to people: where a human stands, AI votes go to them.
        let n = self.members.len();
        for i in 0..n {
            if self.members[i].host != Host::Ai {
                continue;
            }
            let civ = self.members[i].civ;
            for r in Role::ALL {
                if let Some(h) = (0..n).find(|j| self.members[*j].host == Host::Human && self.members[*j].civ == civ && self.members[*j].stand & r.bit() != 0) {
                    self.members[i].votes[r.index()] = h as MemberId;
                }
            }
        }
        let mut pre = Vec::new();
        for (i, m) in self.members.iter().enumerate() {
            let id = i as MemberId;
            pre.push(GovEntry { member: id, signer: local_key(id), action: GovAction::Stand { roles: m.stand } });
            for r in Role::ALL {
                let c = m.votes[r.index()];
                if c != NOBODY {
                    pre.push(GovEntry { member: id, signer: local_key(id), action: GovAction::Vote { role: r, candidate: c } });
                }
            }
        }
        gov::open_government(&mut self.state, &self.rules, &pre).map_err(|e| e.to_string())?;
        self.phase = Phase::Playing;
        self.ledger.observe(&self.state, &self.fog);
        self.deadline = Instant::now() + Duration::from_secs(self.tick_seconds);
        let names = self.names();
        let offices: Vec<String> = self
            .state
            .nations
            .iter()
            .enumerate()
            .map(|(c, x)| {
                let who: Vec<String> = x
                    .offices
                    .iter()
                    .zip(["general", "steward", "science officer", "diplomat"])
                    .map(|(m, r)| format!("{r} {}", if *m == NOBODY { "the acting official".to_string() } else { self.members[*m as usize].meta.name.clone() }))
                    .collect();
                format!("{}: {}", names[c], who.join(", "))
            })
            .collect();
        for line in offices {
            self.chronicle.push((0, format!("gov|First election — {line}")));
        }
        Ok(())
    }

    // -------------------------------------------------------------- ticks

    fn pool(&self) -> u64 {
        match &self.chain {
            Some(c) => c.info["season"]["pool"].as_str().and_then(|x| x.parse().ok()).unwrap_or(0) + self.state.exchange_vault,
            None => {
                let ops = self.entry_fee * self.rules.ops_share_bps as u64 / 10_000;
                self.state.members.len() as u64 * (self.entry_fee - ops) + self.state.exchange_vault
            }
        }
    }

    fn projection(&self) -> Settlement {
        settle(&self.state, &self.rules, self.pool(), self.entry_fee)
    }

    /// Batches of the offices held by people: their committed orders, the
    /// adopted proposals and the reveals of their earlier decisions.
    /// The human office holders' committed batches. `empty`: also an empty
    /// batch for an office with nothing to send (on chain, it ends that
    /// office's turn so the tick can resolve early).
    fn human_batches(&mut self, empty: bool) -> Vec<OrderBatch> {
        let tick = self.state.tick;
        let mut out = Vec::new();
        for i in 0..self.members.len() {
            if self.members[i].host != Host::Human {
                continue;
            }
            let id = i as MemberId;
            let civ = self.members[i].civ;
            for r in Role::ALL {
                if self.state.nations[civ as usize].holder(r) != id {
                    continue;
                }
                let orders: Vec<Order> = self.members[i].committed[r.index()].iter().filter_map(|d| d.to_order().ok()).collect();
                let adopt = self.members[i].adopt[r.index()].clone();
                let digest = match self.ledger.record(tick, civ, r as u8) {
                    Some(rec) => rec.digest,
                    None => self.ledger.commit(tick, civ, r as u8, "human", ""),
                };
                let mut orders = orders;
                orders.extend(self.ledger.reveals(civ, r as u8, tick, &orders));
                if orders.is_empty() && adopt.is_empty() && !empty {
                    continue;
                }
                out.push(OrderBatch { civ, tick, role: r, member: id, decision_digest: digest, orders, adopt });
            }
        }
        out
    }

    fn after_tick(&mut self, prev: &WorldState) {
        let names = self.names();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let members: Vec<String> = self.members.iter().map(|m| m.meta.name.clone()).collect();
        let mut ev = diff_events(prev, &self.state, &names);
        ev.extend(diff_gov_events(prev, &self.state, &names, &members));
        for e in &ev {
            self.chronicle.push((prev.tick, e.clone()));
        }
        if self.chronicle.len() > 500 {
            let cut = self.chronicle.len() - 500;
            self.chronicle.drain(..cut);
        }
        self.last_events = ev;
        self.fog.update(&self.state);
        self.resolved_tick = Some(prev.tick);
        for m in &mut self.members {
            m.committed = Default::default();
            m.cost = [0; 4];
            m.adopt = Default::default();
        }
    }

    /// Local mode: resolve the open tick in-process.
    fn advance(&mut self) {
        if self.over() || self.phase != Phase::Playing {
            return;
        }
        let tick = self.state.tick;
        let hosts = self.hosts();
        let mut gov_entries = self.planner.member_gov(&self.state, &self.rules, &self.fog, &hosts);
        gov_entries.append(&mut self.pending_gov);
        let mut batches = self.planner.batches(&self.state, &self.rules, &self.fog, &mut self.ledger, &hosts);
        batches.extend(self.human_batches(false));
        let deposits: Vec<(CivId, u64)> = if tick == 0 {
            self.members.iter().filter(|m| m.deposit > 0).map(|m| (m.civ, m.deposit)).collect()
        } else {
            Vec::new()
        };
        // Which batches the engine accepts, so reveals are only marked when processed.
        let accepted: Vec<(CivId, u8, Vec<Order>)> = batches
            .iter()
            .filter(|b| validate_batch(&self.state, &self.rules, b).is_ok())
            .map(|b| (b.civ, b.role as u8, b.orders.clone()))
            .collect();
        let mut vrf = [0u8; 32];
        vrf[..2].copy_from_slice(&tick.to_le_bytes());
        vrf[2..10].copy_from_slice(&(self.deadline.elapsed().as_nanos() as u64).to_le_bytes());
        let prev = self.state.clone();
        if let Err(e) = resolve_tick(&mut self.state, &self.rules, &TickInput { vrf, batches, gov: gov_entries, deposits }) {
            eprintln!("tick {tick} failed: {e}");
            return;
        }
        self.after_tick(&prev);
        for (civ, role, orders) in &accepted {
            self.ledger.mark_revealed(*civ, *role, orders, tick);
        }
        self.ledger.observe(&self.state, &self.fog);
        let violations = invariants::check(&self.state, &self.rules);
        if !violations.is_empty() {
            eprintln!("invariant violations after tick {tick}: {violations:?}");
        }
        self.deadline = Instant::now() + Duration::from_secs(self.tick_seconds);
    }

    /// Every person here ended its turn.
    fn all_humans_ready(&self) -> bool {
        let tick = self.state.tick;
        let here: Vec<&Member> = self.members.iter().filter(|m| m.host == Host::Human && m.here()).collect();
        !here.is_empty() && here.iter().all(|m| m.ready == Some(tick))
    }

    /// Chain mode: apply a snapshot. Returns what to send to the gateway
    /// (`(path, body)`) after the lock is released.
    fn on_chain(&mut self, snap: Snapshot) -> Vec<(&'static str, Value)> {
        let link = self.chain.as_ref().map(|c| c.link.clone()).expect("chain mode");
        if snap.state.tick != self.state.tick || snap.state.members.len() != self.state.members.len() {
            let prev = std::mem::replace(&mut self.state, snap.state);
            let tick = prev.tick;
            if prev.tick != self.state.tick {
                self.after_tick(&prev);
                if let Ok(Some((input, rec))) = link.resolved_input(tick) {
                    for b in &input.batches {
                        if b.member != NOBODY && self.members.get(b.member as usize).is_some_and(|m| m.host == Host::External) {
                            self.ledger.ingest_external(b);
                        } else {
                            self.ledger.mark_revealed(b.civ, b.role as u8, &b.orders, tick);
                        }
                    }
                    if let Some(c) = self.chain.as_mut() {
                        c.last_record = Some(rec);
                    }
                }
            } else {
                self.fog.update(&self.state);
            }
            self.ledger.observe(&self.state, &self.fog);
            if let Ok(info) = link.get_json("/season") {
                self.sync_registry(&info);
                if let Some(c) = self.chain.as_mut() {
                    c.info = info;
                }
            }
        } else {
            self.state = snap.state;
        }
        let tick = self.state.tick;
        let c = self.chain.as_mut().unwrap();
        c.meta = snap.meta;
        c.slot = snap.slot;
        c.layer = snap.layer;
        c.phase = snap.phase;
        self.phase = if c.phase == "setup" || c.phase == "registering" { Phase::Lobby } else { Phase::Playing };
        if c.meta.finished || c.phase != "playing" || tick >= self.rules.ticks_per_season || c.ai_submitted == Some(tick) {
            return Vec::new();
        }
        c.ai_submitted = Some(tick);
        let hosts = self.hosts();
        // Governance first: once every office's batch is in, the crank freezes
        // the tick's input and later actions wait for the next tick (TickFrozen).
        let mut out = Vec::new();
        for e in self.planner.member_gov(&self.state, &self.rules, &self.fog, &hosts) {
            out.push(("/gov", json!({"member": e.member, "action": GovDto::from_action(&e.action)})));
        }
        for b in self.planner.batches(&self.state, &self.rules, &self.fog, &mut self.ledger, &hosts) {
            out.push(("/submit", batch_body(&b)));
        }
        out
    }

    // -------------------------------------------------------------- views

    fn members_json(&self) -> Value {
        let tick = self.state.tick;
        json!(self
            .members
            .iter()
            .enumerate()
            .map(|(i, m)| json!({
                "id": i, "civ": m.civ, "name": m.meta.name, "kind": m.meta.kind, "attested": m.meta.attested,
                "host": m.host.name(), "claimed": m.here(), "ready": m.ready == Some(tick),
                "claimable": m.host == Host::Human && !m.here(),
            }))
            .collect::<Vec<_>>())
    }

    fn lobby_json(&self, viewer: Viewer) -> Value {
        let p = self.projection();
        let nations: Vec<Value> = self
            .state
            .civs
            .iter()
            .enumerate()
            .map(|(c, x)| {
                let members = self.members.iter().filter(|m| m.civ as usize == c).count();
                json!({"civ": c, "name": x.name, "members": members, "persona": persona_of(c as CivId).name(),
                       "era": x.achievements.era, "points": p.scores.get(c).map(|s| s.total()),
                       "share": p.nation_share.get(c), "perMember": if members == 0 { Value::Null } else { json!(p.nation_share.get(c).copied().unwrap_or(0) / members as u64) }})
            })
            .collect();
        let you = match viewer {
            Viewer::Member(m) => {
                let x = &self.members[m as usize];
                json!({"id": m, "civ": x.civ, "name": x.meta.name, "stand": Role::ALL.iter().filter(|r| x.stand & r.bit() != 0).map(|r| role_name(*r)).collect::<Vec<_>>(),
                       "votes": Role::ALL.iter().map(|r| api::member_ref(x.votes[r.index()])).collect::<Vec<_>>(), "deposit": x.deposit})
            }
            _ => Value::Null,
        };
        json!({
            "phase": match self.phase { Phase::Lobby => "lobby", Phase::Playing => "playing" },
            "mode": if self.chain.is_some() { "chain" } else { "local" },
            "nations": nations, "members": self.members_json(), "you": you,
            "entryFee": self.entry_fee, "pool": self.pool(), "market": self.rules.market_enabled,
            "offices": Role::ALL.iter().map(|r| role_name(*r)).collect::<Vec<_>>(),
        })
    }

    fn state_json(&self, viewer: Viewer) -> Value {
        let civ = self.civ_of(viewer);
        let belief = civ.map(|c| self.fog.belief(&self.state, c));
        let s = belief.as_ref().unwrap_or(&self.state);
        let mut v = api::world_view(s, &self.rules, civ, &self.fog);
        let secs = if let Some(c) = &self.chain {
            (c.meta.deadline - unix_now()).max(0) as f64
        } else if self.paused || self.phase == Phase::Lobby {
            self.tick_seconds as f64
        } else {
            self.deadline.saturating_duration_since(Instant::now()).as_secs_f64()
        };
        let names = self.names();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let public = |e: &str| civ.is_none_or(|c| line_is_public(&self.state, c, e, &names));
        let chronicle: Vec<Value> =
            self.chronicle.iter().rev().filter(|(_, e)| public(e)).take(150).map(|(t, e)| json!({"tick": t, "text": e})).collect();
        let me = match viewer {
            Viewer::Member(m) => Some(m),
            _ => None,
        };
        let metas = self.metas();
        let projection = self.projection();
        let t = self.state.tick;
        let o = v.as_object_mut().unwrap();
        o.insert("phase".into(), json!(match self.phase { Phase::Lobby => "lobby", Phase::Playing => "playing" }));
        o.insert("viewer".into(), json!(match viewer { Viewer::Member(_) => "member", Viewer::Watch(_) => "watch", Viewer::Spectator => "spectator" }));
        o.insert("secondsLeft".into(), json!(secs));
        o.insert("tickSeconds".into(), json!(self.tick_seconds));
        o.insert("paused".into(), json!(self.paused));
        o.insert("over".into(), json!(self.over()));
        o.insert("chronicle".into(), json!(chronicle));
        o.insert("lastSummary".into(), json!(self.last_events.iter().filter(|e| public(e)).collect::<Vec<_>>()));
        o.insert("resolvedTick".into(), json!(self.resolved_tick));
        o.insert("members".into(), self.members_json());
        o.insert("achievements".into(), api::achievements_view(&self.state, &self.rules));
        o.insert("projection".into(), api::settlement_view(&projection));
        o.insert(
            "season".into(),
            json!({
                "preset": format!("{:?}", self.rules.preset), "ticks": self.rules.ticks_per_season,
                "entryFee": self.entry_fee, "pool": self.pool(), "market": self.rules.market_enabled,
                "marketFreeze": self.rules.exchange_freeze_tick, "transferFreeze": self.rules.transfer_freeze_tick,
                "termTicks": self.rules.term_ticks, "voteWindow": self.rules.vote_window,
                "phases": [0, self.rules.expansion_start, self.rules.contention_start, self.rules.crisis_start, self.rules.resolution_start],
                "activityWindow": self.rules.activity_window_ticks, "activeWindowsNeeded": self.rules.active_windows_needed,
                "equalShareBps": self.rules.equal_share_bps, "opsShareBps": self.rules.ops_share_bps,
                "spendConsentUsdc": self.rules.spend_consent_usdc, "deliveryTicks": self.rules.delivery_ticks,
                "tariffTable": self.rules.tariff_table_bps, "tariffFull": self.rules.tariff_full_usdc,
            }),
        );
        if let Some(c) = civ {
            o.insert("gov".into(), api::gov_view(s, &self.rules, c, &metas));
            o.insert("roster".into(), api::roster_view(&self.state, &self.rules, c, &metas));
            o.insert("facts".into(), api::facts_view(&self.state, &self.rules, c));
            o.insert("skipped".into(), api::skipped_view(&self.state, c));
            // obsRoot is public: an outside agent commits its decision against it.
            o.insert("decision".into(), json!({"tick": t, "obsRoot": self.ledger.root(t, c).map(|h| hex(&h))}));
        }
        if let Some(m) = me {
            let x = &self.members[m as usize];
            let mut mv = api::member_view(&self.state, &self.rules, m, &metas, Some(&projection));
            mv["committed"] = json!(Role::ALL.iter().map(|r| json!({"role": role_name(*r), "orders": x.committed[r.index()], "cost": x.cost[r.index()], "adopt": x.adopt[r.index()],
                "digest": self.ledger.record(t, x.civ, *r as u8).map(|rec| hex(&rec.digest))})).collect::<Vec<_>>());
            mv["ready"] = json!(x.ready == Some(t));
            mv["host"] = json!(x.host.name());
            o.insert("member".into(), mv);
        }
        if let Some(c) = &self.chain {
            o.insert(
                "chain".into(),
                json!({
                    "seasonId": c.meta.season_id.to_string(),
                    "programId": c.info["programId"], "cluster": c.info["cluster"],
                    "accounts": c.info["accounts"], "endpoints": c.info["endpoints"],
                    "gateway": c.link.url(), "layer": c.layer, "slot": c.slot, "phase": c.phase,
                    "finished": c.meta.finished,
                    "lastTick": c.last_record.as_ref().map(|r| json!({"tick": r["tick"], "signature": r["signature"], "cu": r["cu"], "root": r["root"], "preRoot": r["preRoot"]})),
                    "error": c.error,
                }),
            );
        }
        v
    }

    /// Split `orders` among the offices `m` holds. Err if an order belongs to
    /// an office it does not hold (it can be proposed instead).
    fn split_for_member(&self, m: MemberId, orders: &[(OrderDto, Order)]) -> Result<[Vec<(OrderDto, Order)>; 4], String> {
        let civ = self.members[m as usize].civ;
        let n = &self.state.nations[civ as usize];
        let mut out: [Vec<(OrderDto, Order)>; 4] = Default::default();
        for (d, o) in orders {
            match Role::ALL.into_iter().find(|r| n.holder(*r) == m && role_allows(&self.state, *r, o)) {
                Some(r) => out[r.index()].push((d.clone(), o.clone())),
                None => {
                    let office = api::office_of(&self.state, o).unwrap_or_else(|| "another office".into());
                    return Err(format!("{} belongs to the {office}, which you do not hold: propose it instead (POST /api/gov Propose)", d_type(d)));
                }
            }
        }
        Ok(out)
    }
}

fn d_type(d: &OrderDto) -> String {
    serde_json::to_value(d).ok().and_then(|v| v["type"].as_str().map(String::from)).unwrap_or_default()
}

/// A batch as the gateway's /submit takes it.
fn batch_body(b: &OrderBatch) -> Value {
    json!({
        "civ": b.civ, "role": role_name(b.role), "member": api::member_ref(b.member), "tick": b.tick,
        "digest": hex(&b.decision_digest), "orders": b.orders.iter().map(OrderDto::from_order).collect::<Vec<_>>(), "adopt": b.adopt,
    })
}

// ------------------------------------------------------------------ HTTP

struct Request {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    /// Lower-cased names.
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn q(&self, k: &str) -> Option<&str> {
        self.query.iter().find(|(a, _)| a == k).map(|(_, b)| b.as_str())
    }
    fn qi<T: std::str::FromStr>(&self, k: &str) -> Option<T> {
        self.q(k)?.parse().ok()
    }
    fn header(&self, k: &str) -> Option<&str> {
        self.headers.iter().find(|(a, _)| a == k).map(|(_, b)| b.as_str())
    }
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(json!({}))
    }
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let mut len = 0usize;
    let mut headers = Vec::new();
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).ok()? == 0 || h == "\r\n" || h == "\n" {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim().to_string());
            if k == "content-length" {
                len = v.parse().unwrap_or(0);
            }
            if headers.len() < 64 {
                headers.push((k, v));
            }
        }
    }
    let mut body = vec![0u8; len.min(1 << 20)];
    reader.read_exact(&mut body).ok()?;
    let (path, qs) = target.split_once('?').unwrap_or((&target, ""));
    let query = qs
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (a, b) = p.split_once('=').unwrap_or((p, ""));
            (a.to_string(), b.to_string())
        })
        .collect();
    Some(Request { method, path: path.to_string(), query, headers, body })
}

fn respond(stream: &mut TcpStream, status: &str, ctype: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: content-type, x-member-token, x-seat-token\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

fn json_ok(stream: &mut TcpStream, v: &Value) {
    respond(stream, "200 OK", "application/json; charset=utf-8", v.to_string().as_bytes());
}

fn json_status(stream: &mut TcpStream, status: &str, v: &Value) {
    respond(stream, status, "application/json; charset=utf-8", v.to_string().as_bytes());
}

fn static_file(stream: &mut TcpStream, web: &Path, path: &str) {
    let rel = if path == "/" { "index.html" } else { path.trim_start_matches('/') };
    let rel = if rel.ends_with('/') { format!("{rel}index.html") } else { rel.to_string() };
    if rel.contains("..") {
        return respond(stream, "400 Bad Request", "text/plain", b"bad path");
    }
    let file: PathBuf = web.join(&rel);
    match std::fs::read(&file) {
        Ok(bytes) => {
            let ctype = match file.extension().and_then(|e| e.to_str()) {
                Some("html") => "text/html; charset=utf-8",
                Some("css") => "text/css; charset=utf-8",
                Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
                Some("svg") => "image/svg+xml",
                Some("png") => "image/png",
                Some("json") => "application/json; charset=utf-8",
                Some("txt") | Some("md") => "text/plain; charset=utf-8",
                _ => "application/octet-stream",
            };
            respond(stream, "200 OK", ctype, &bytes)
        }
        Err(_) => respond(stream, "404 Not Found", "text/plain", b"not found"),
    }
}

fn parse_orders(v: &Value) -> Result<Vec<(OrderDto, Order)>, String> {
    let dtos: Vec<OrderDto> = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    dtos.into_iter().map(|d| d.to_order().map(|o| (d, o))).collect()
}

fn parse_roles(v: &Value) -> u8 {
    v.as_array().map_or(0, |a| a.iter().filter_map(|r| r.as_str().and_then(api::parse_role)).fold(0, |acc, r| acc | r.bit()))
}

fn handle(mut stream: TcpStream, game: Arc<Mutex<Game>>, web: PathBuf) {
    let Some(req) = read_request(&mut stream) else { return };
    if req.method == "OPTIONS" {
        return respond(&mut stream, "204 No Content", "text/plain", b"");
    }
    if !req.path.starts_with("/api/") {
        return static_file(&mut stream, &web, &req.path);
    }
    let mut g = game.lock().unwrap();
    let viewer = g.viewer(&req);
    if let Viewer::Member(m) = viewer {
        g.members[m as usize].last_seen = Some(Instant::now());
    }
    let civ = g.civ_of(viewer);
    // Everything a viewer can ask about is answered from its nation's belief
    // state (spectators: the full state).
    let belief = match civ {
        Some(c) => g.fog.belief(&g.state, c),
        None => g.state.clone(),
    };
    let (s, r) = (&belief, &g.rules);
    let need_civ = |stream: &mut TcpStream| {
        json_status(stream, "400 Bad Request", &json!({"ok": false, "error": "previews are per nation: pass ?civ=N, ?member=M or a member token"}))
    };
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/api/map") => {
            let mut v = api::map_view(s);
            v["civNames"] = json!(g.names());
            json_ok(&mut stream, &v)
        }
        ("GET", "/api/state") => {
            let v = g.state_json(viewer);
            json_ok(&mut stream, &v)
        }
        ("GET", "/api/lobby") | ("GET", "/api/seats") => {
            let v = g.lobby_json(viewer);
            json_ok(&mut stream, &v)
        }
        ("POST", "/api/join") => {
            let b = req.json();
            let Some(nation) = b["civ"].as_u64().filter(|c| (*c as usize) < g.state.civs.len()) else {
                return json_status(&mut stream, "400 Bad Request", &json!({"ok": false, "error": "civ (nation) required"}));
            };
            let name = b["name"].as_str().unwrap_or("").trim().chars().take(24).collect::<String>();
            let name = if name.is_empty() { format!("Player {}", g.members.len() + 1) } else { name };
            let kind = match b["kind"].as_str() {
                Some("agent") => "agent",
                Some("human") | None => "human",
                _ => "undeclared",
            };
            let meta = MemberMeta { name, kind: kind.into(), attested: false };
            match g.join_local(nation as CivId, meta, Host::Human) {
                Ok(id) => {
                    let token = g.token(id as u64);
                    let x = &mut g.members[id as usize];
                    x.token = Some(token.clone());
                    x.last_seen = Some(Instant::now());
                    x.stand = parse_roles(&b["stand"]) ;
                    x.deposit = b["deposit"].as_u64().unwrap_or(0).min(100_000_000);
                    eprintln!("member {id} joined nation {nation}");
                    json_ok(&mut stream, &json!({"ok": true, "member": id, "civ": nation, "token": token}))
                }
                Err(e) => json_status(&mut stream, "409 Conflict", &json!({"ok": false, "error": e})),
            }
        }
        ("POST", "/api/claim") => {
            // Chain mode: take over a human member the gateway registered.
            let b = req.json();
            let Some(m) = b["member"].as_u64().map(|m| m as usize).filter(|m| *m < g.members.len()) else {
                return json_status(&mut stream, "400 Bad Request", &json!({"ok": false, "error": "member required"}));
            };
            if g.members[m].host != Host::Human || g.members[m].here() {
                return json_status(&mut stream, "409 Conflict", &json!({"ok": false, "error": "this member is not claimable"}));
            }
            let token = g.token(m as u64);
            let x = &mut g.members[m];
            x.token = Some(token.clone());
            x.last_seen = Some(Instant::now());
            json_ok(&mut stream, &json!({"ok": true, "member": m, "civ": x.civ, "token": token}))
        }
        ("POST", "/api/lobby") => {
            let Viewer::Member(m) = viewer else {
                return json_status(&mut stream, "403 Forbidden", &json!({"ok": false, "error": "join first"}));
            };
            if g.phase != Phase::Lobby {
                return json_ok(&mut stream, &json!({"ok": false, "error": "the season has started; use /api/gov"}));
            }
            let b = req.json();
            let x = &mut g.members[m as usize];
            if b.get("stand").is_some() {
                x.stand = parse_roles(&b["stand"]);
            }
            if let Some(votes) = b["votes"].as_object() {
                for (k, v) in votes {
                    if let Some(role) = api::parse_role(k) {
                        x.votes[role.index()] = v.as_u64().map_or(NOBODY, |c| c as MemberId);
                    }
                }
            }
            json_ok(&mut stream, &json!({"ok": true}))
        }
        ("POST", "/api/start") => {
            if !matches!(viewer, Viewer::Member(_)) {
                return json_status(&mut stream, "403 Forbidden", &json!({"ok": false, "error": "only members start the season"}));
            }
            match g.start() {
                Ok(()) => json_ok(&mut stream, &json!({"ok": true, "tick": g.state.tick})),
                Err(e) => json_ok(&mut stream, &json!({"ok": false, "error": e})),
            }
        }
        ("GET", "/api/preview/unit") => match civ {
            Some(c) => json_ok(&mut stream, &api::unit_preview(s, r, c, req.qi("id").unwrap_or(u32::MAX))),
            None => need_civ(&mut stream),
        },
        ("GET", "/api/preview/city") => match civ {
            Some(c) => json_ok(&mut stream, &api::city_preview(s, r, c, req.qi("id").unwrap_or(u32::MAX))),
            None => need_civ(&mut stream),
        },
        ("GET", "/api/preview/research") => match civ {
            Some(c) => json_ok(&mut stream, &api::research_preview(s, r, c)),
            None => need_civ(&mut stream),
        },
        ("GET", "/api/preview/diplomacy") => match civ {
            Some(c) => json_ok(&mut stream, &api::diplomacy_preview(s, r, c, req.qi("with").or(req.qi("other")).unwrap_or(u16::MAX))),
            None => need_civ(&mut stream),
        },
        ("GET", "/api/preview/path") => {
            let (Some(unit), Some(q), Some(rr)) = (req.qi::<u32>("unit"), req.qi::<i32>("q"), req.qi::<i32>("r")) else {
                return respond(&mut stream, "400 Bad Request", "text/plain", b"unit, q, r required");
            };
            let goal = Hex::new(q, rr);
            let path = permutation_rules::preview::path_to(s, r, unit, goal);
            let why = match (&path, s.units.get(unit as usize)) {
                (None, Some(u)) => Some(match permutation_rules::checks::enter(s, r, u.owner.civ(), goal) {
                    Err(b) => api::blocked(b),
                    Ok(()) => json!({ "code": "Unreachable" }),
                }),
                _ => None,
            };
            let v = json!({ "path": path.map(|p| p.iter().map(|h| [h.q, h.r]).collect::<Vec<_>>()), "blocked": why });
            json_ok(&mut stream, &v)
        }
        ("GET", "/api/preview/amm") => {
            let pool = if req.q("good") == Some("Horses") { 1 } else { 0 };
            let side = if req.q("side") == Some("Sell") { Side::Sell } else { Side::Buy };
            let amount: u32 = req.qi("amount").unwrap_or(1);
            let p = s.pools[pool];
            let c = clear_amm(r, p.goods, p.gold, &[AmmOrder { side, qty: amount, limit_gold: if side == Side::Buy { u32::MAX } else { 0 }, budget_milli: i64::MAX }]);
            let spot = p.gold as f64 / p.goods.max(1) as f64;
            let v = json!({
                "price": c.price_milli as f64 / 1000.0, "spot": spot,
                "impact": if spot > 0.0 { (c.price_milli as f64 / 1000.0 - spot) / spot } else { 0.0 },
                "gold": c.fills.first().and_then(|f| *f).map(|(g, _)| g as f64 / 1000.0),
                "fee": (c.hub_fee_milli + c.burned_milli) as f64 / 1000.0,
                "filled": c.fills.first().is_some_and(|f| f.is_some()),
            });
            json_ok(&mut stream, &v)
        }
        ("POST", "/api/validate") => {
            // Dry run for any office (outside agents check before signing).
            let body = req.json();
            let nation = match civ {
                Some(c) => c,
                None => match body["civ"].as_u64() {
                    Some(c) if (c as usize) < g.state.civs.len() => c as CivId,
                    _ => return json_status(&mut stream, "400 Bad Request", &json!({"ok": false, "error": "civ required"})),
                },
            };
            let v = match parse_orders(&body["orders"]) {
                Err(e) => json!({"ok": false, "error": e}),
                Ok(pairs) => {
                    let orders: Vec<Order> = pairs.into_iter().map(|(_, o)| o).collect();
                    let warnings = api::preflight(&belief, &g.rules, nation, &orders);
                    let offices: Vec<Value> = Role::ALL
                        .iter()
                        .map(|role| {
                            let mine: Vec<Order> = orders.iter().filter(|o| role_allows(&g.state, *role, o)).cloned().collect();
                            let cost = check_structure(&g.rules, g.state.tick, &mine);
                            let spendable = permutation_rules::orders::spendable(&g.state, &g.rules, nation, *role);
                            json!({"role": role_name(*role), "orders": mine.len(), "cost": cost.as_ref().ok(), "spendable": spendable,
                                   "error": match cost { Err(e) => Some(e.to_string()), Ok(c) if c > spendable => Some(format!("orders cost {c}, only {spendable} spendable")), _ => None },
                                   "holder": api::member_ref(g.state.nations[nation as usize].holder(*role))})
                        })
                        .collect();
                    let ok = offices.iter().all(|o| o["error"].is_null());
                    json!({"ok": ok, "tick": g.state.tick, "offices": offices, "warnings": warnings})
                }
            };
            json_ok(&mut stream, &v)
        }
        ("POST", "/api/orders") => {
            let Viewer::Member(m) = viewer else {
                return json_status(&mut stream, "403 Forbidden", &json!({"ok": false, "error": "join a nation first; outside agents sign their own batches"}));
            };
            if g.phase != Phase::Playing || g.over() {
                return json_ok(&mut stream, &json!({"ok": false, "error": "the season is not running"}));
            }
            let body = req.json();
            let pairs = match parse_orders(&body["orders"]) {
                Ok(p) => p,
                Err(e) => return json_ok(&mut stream, &json!({"ok": false, "error": e})),
            };
            let parts = match g.split_for_member(m, &pairs) {
                Ok(p) => p,
                Err(e) => return json_ok(&mut stream, &json!({"ok": false, "error": e})),
            };
            let x_civ = g.members[m as usize].civ;
            let tick = g.state.tick;
            let text = body["rationale"].as_str().unwrap_or("").trim().to_string();
            let adopt: Vec<(Role, Vec<u32>)> = body["adopt"]
                .as_object()
                .map(|o| o.iter().filter_map(|(k, v)| Some((api::parse_role(k)?, v.as_array()?.iter().filter_map(|x| x.as_u64().map(|x| x as u32)).collect()))).collect())
                .unwrap_or_default();
            // Validate every office's batch as the engine will.
            let held: Vec<Role> = Role::ALL.into_iter().filter(|r| g.state.nations[x_civ as usize].holder(*r) == m).collect();
            if held.is_empty() {
                return json_ok(&mut stream, &json!({"ok": false, "error": "you hold no office: propose orders instead (POST /api/gov Propose)"}));
            }
            let mut results = Vec::new();
            let mut digests = Vec::new();
            for role in &held {
                let orders: Vec<Order> = parts[role.index()].iter().map(|(_, o)| o.clone()).collect();
                let ids = adopt.iter().find(|(r, _)| r == role).map(|(_, v)| v.clone()).unwrap_or_default();
                let batch = OrderBatch { civ: x_civ, tick, role: *role, member: m, decision_digest: [1; 32], orders, adopt: ids.clone() };
                match validate_batch(&g.state, &g.rules, &batch) {
                    Ok(cost) => results.push((*role, cost, ids)),
                    Err(e) => return json_ok(&mut stream, &json!({"ok": false, "role": role_name(*role), "error": e.to_string()})),
                }
            }
            for (role, cost, ids) in &results {
                let digest = g.ledger.commit(tick, x_civ, *role as u8, "human", &text);
                digests.push(json!({"role": role_name(*role), "digest": hex(&digest), "cost": cost}));
                let x = &mut g.members[m as usize];
                x.committed[role.index()] = parts[role.index()].iter().map(|(d, _)| d.clone()).collect();
                x.cost[role.index()] = *cost;
                x.adopt[role.index()] = ids.clone();
            }
            if g.chain.is_none() {
                return json_ok(&mut stream, &json!({"ok": true, "tick": tick, "offices": digests}));
            }
            // Chain mode: submit each office's batch now (signed by the gateway with the member's session key).
            let batches: Vec<OrderBatch> = g.human_batches(true).into_iter().filter(|b| b.member == m).collect();
            let link = g.chain.as_ref().unwrap().link.clone();
            drop(g);
            let mut sigs = Vec::new();
            for b in &batches {
                match link.post_json("/submit", &batch_body(b)) {
                    Ok(res) => sigs.push(res["signature"].clone()),
                    Err(e) => return json_ok(&mut stream, &json!({"ok": false, "error": format!("chain: {e}")})),
                }
            }
            let mut g = game.lock().unwrap();
            g.members[m as usize].ready = Some(tick);
            if let Some(c) = g.chain.as_mut() {
                c.error = None;
            }
            json_ok(&mut stream, &json!({"ok": true, "tick": tick, "offices": digests, "signatures": sigs}))
        }
        ("POST", "/api/gov") => {
            let Viewer::Member(m) = viewer else {
                return json_status(&mut stream, "403 Forbidden", &json!({"ok": false, "error": "join a nation first"}));
            };
            let body = req.json();
            let dto: GovDto = match serde_json::from_value(body["action"].clone()) {
                Ok(d) => d,
                Err(e) => return json_ok(&mut stream, &json!({"ok": false, "error": e.to_string()})),
            };
            let action = match dto.to_action() {
                Ok(a) => a,
                Err(e) => return json_ok(&mut stream, &json!({"ok": false, "error": e})),
            };
            if g.phase == Phase::Lobby && g.chain.is_none() {
                return json_ok(&mut stream, &json!({"ok": false, "error": "in the lobby, set your candidacy and votes with POST /api/lobby"}));
            }
            if let GovAction::Vote { .. } = action {
                if !gov::vote_open(&g.rules, g.state.tick) {
                    let next = gov::next_term_start(&g.rules, g.state.tick);
                    return json_ok(&mut stream, &json!({"ok": false, "error": format!("votes open {} ticks before the next term ({:?})", g.rules.vote_window, next)}));
                }
            }
            if g.chain.is_none() {
                g.pending_gov.retain(|e| !(e.member == m && std::mem::discriminant(&e.action) == std::mem::discriminant(&action) && !matches!(action, GovAction::Support { .. } | GovAction::Propose { .. })));
                g.pending_gov.push(GovEntry { member: m, signer: local_key(m), action });
                return json_ok(&mut stream, &json!({"ok": true, "tick": g.state.tick, "queued": g.pending_gov.iter().filter(|e| e.member == m).count()}));
            }
            let link = g.chain.as_ref().unwrap().link.clone();
            drop(g);
            match link.post_json("/gov", &json!({"member": m, "action": dto})) {
                Ok(res) => json_ok(&mut stream, &json!({"ok": true, "signature": res["signature"]})),
                Err(e) => json_ok(&mut stream, &json!({"ok": false, "error": format!("chain: {e}")})),
            }
        }
        ("GET", "/api/decisions") => {
            let limit: usize = req.qi("limit").unwrap_or(120).min(600);
            let open = g.state.tick;
            let own = civ.filter(|_| matches!(viewer, Viewer::Member(_)));
            let records: Vec<Value> = g
                .ledger
                .history(limit)
                .into_iter()
                .filter(|r| r.tick < open || Some(r.civ) == own)
                .map(|r| {
                    let revealed = r.revealed_at.is_some();
                    json!({
                        "tick": r.tick, "civ": r.civ, "role": Role::from_index(r.role as usize).map(role_name),
                        "obsRoot": hex(&r.obs_root), "digest": hex(&r.digest), "external": r.external,
                        // Only what has been revealed on the event chain is public.
                        "policy": if revealed || Some(r.civ) == own { json!(r.policy) } else { Value::Null },
                        "reveal": if revealed { json!({"at": r.revealed_at, "salt": hex(&r.salt), "text": r.text}) } else { Value::Null },
                        "serverVerified": if revealed { json!(r.verified()) } else { Value::Null },
                    })
                })
                .collect();
            json_ok(&mut stream, &json!({ "open": open, "records": records }))
        }
        ("GET", "/api/decisions/proof") => {
            let (Some(tick), Some(of), Some(kind), Some(id)) =
                (req.qi::<u16>("tick"), req.qi::<CivId>("of").or(req.qi("civ")), req.q("kind"), req.qi::<u32>("id"))
            else {
                return respond(&mut stream, "400 Bad Request", "text/plain", b"tick, civ, kind, id required");
            };
            if tick >= g.state.tick || (civ != Some(of) && kind != "tile" && kind != "header") {
                return json_ok(&mut stream, &json!({"ok": false, "error": "not available"}));
            }
            match g.ledger.proof(tick, of, kind, id) {
                Some((root, leaf, proof)) => {
                    let v = json!({
                        "ok": true, "tick": tick, "civ": of, "root": hex(&root),
                        "kind": leaf.kind, "id": leaf.id, "body": hex(&leaf.body),
                        "proof": proof.iter().map(|p| json!([hex(&p.sibling), p.left])).collect::<Vec<_>>(),
                    });
                    json_ok(&mut stream, &v)
                }
                None => json_ok(&mut stream, &json!({"ok": false, "error": "no such leaf (older observations keep only their root)"})),
            }
        }
        ("POST", "/api/control") => {
            let Viewer::Member(m) = viewer else {
                return json_status(&mut stream, "403 Forbidden", &json!({"ok": false, "error": "only members control the game"}));
            };
            let v = req.json();
            let tick = g.state.tick;
            if g.chain.is_some() {
                // Chain mode: no pause; "advance" submits whatever is committed.
                if v["advance"].as_bool() == Some(true) {
                    let batches: Vec<OrderBatch> = g.human_batches(true).into_iter().filter(|b| b.member == m).collect();
                    let link = g.chain.as_ref().unwrap().link.clone();
                    drop(g);
                    let mut err = None;
                    for b in &batches {
                        if let Err(e) = link.post_json("/submit", &batch_body(b)) {
                            err = Some(e);
                        }
                    }
                    let mut g = game.lock().unwrap();
                    if err.is_none() {
                        g.members[m as usize].ready = Some(tick);
                    } else if let Some(c) = g.chain.as_mut() {
                        c.error = err.clone();
                    }
                    return json_ok(&mut stream, &json!({"ok": err.is_none(), "tick": tick, "error": err}));
                }
                return json_ok(&mut stream, &json!({"ok": true, "paused": false, "tick": tick}));
            }
            if let Some(p) = v["paused"].as_bool() {
                g.paused = p;
                if !p {
                    let secs = g.tick_seconds;
                    g.deadline = Instant::now() + Duration::from_secs(secs);
                }
            }
            if let Some(t) = v["tickSeconds"].as_u64() {
                g.tick_seconds = t.clamp(5, 3600);
            }
            if v["advance"].as_bool() == Some(true) {
                // End this member's turn; the tick resolves once every person here did.
                g.members[m as usize].ready = Some(tick);
                if g.all_humans_ready() {
                    g.advance();
                }
            }
            let waiting: Vec<MemberId> = g
                .members
                .iter()
                .enumerate()
                .filter(|(_, x)| x.host == Host::Human && x.here() && x.ready != Some(g.state.tick))
                .map(|(i, _)| i as MemberId)
                .collect();
            json_ok(&mut stream, &json!({"ok": true, "paused": g.paused, "tick": g.state.tick, "waitingFor": waiting}))
        }
        _ => json_status(&mut stream, "404 Not Found", &json!({"ok": false, "error": "no such endpoint; see /llms.txt"})),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let port: u16 = arg("--port").and_then(|p| p.parse().ok()).unwrap_or(4185);
    let tick_seconds: u64 = arg("--tick-seconds").and_then(|p| p.parse().ok()).unwrap_or(30);
    let ai_members: usize = arg("--ai-members").and_then(|p| p.parse().ok()).unwrap_or(2);
    let autostart = args.iter().any(|a| a == "--autostart");
    let web = arg("--web").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("web"));

    let chain = arg("--chain");
    let game = match &chain {
        Some(url) => {
            let link = Arc::new(ChainLink::new(url).expect("--chain http://host:port"));
            Arc::new(Mutex::new(Game::from_chain(link).expect("attach to the on-chain season")))
        }
        None => {
            let mut g = Game::new(tick_seconds, ai_members);
            if autostart {
                g.start().expect("start");
            }
            Arc::new(Mutex::new(g))
        }
    };
    if chain.is_some() {
        // Follow the chain: new ticks, then the AI members' and acting officials' submissions.
        let game = game.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(400));
            let link = game.lock().unwrap().chain.as_ref().unwrap().link.clone();
            let snap = match link.world() {
                Ok(Some(s)) => s,
                Ok(None) => continue,
                Err(e) => {
                    eprintln!("chain: {e}");
                    continue;
                }
            };
            let sends = game.lock().unwrap().on_chain(snap);
            for (path, body) in sends {
                if let Err(e) = link.post_json(path, &body) {
                    eprintln!("{path} for nation {} failed: {e}", body["civ"]);
                }
            }
        });
    } else {
        let game = game.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(150));
            let mut g = game.lock().unwrap();
            if g.phase == Phase::Playing && !g.paused && !g.over() && Instant::now() >= g.deadline {
                g.advance();
            }
        });
    }
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind");
    {
        let g = game.lock().unwrap();
        eprintln!("PERMUTATION STATE (V5) on http://127.0.0.1:{port}/  (web: {})", web.display());
        eprintln!("  {} nations, {} members, phase {:?}", g.state.civs.len(), g.members.len(), g.phase);
        eprintln!("  spectate: http://127.0.0.1:{port}/?spectate   agents: http://127.0.0.1:{port}/llms.txt");
    }
    for stream in listener.incoming().flatten() {
        let (game, web) = (game.clone(), web.clone());
        std::thread::spawn(move || handle(stream, game, web));
    }
}
