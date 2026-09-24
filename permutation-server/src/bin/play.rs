//! Playable server: human seats, scripted bots, outside agents, spectators.
//!
//!     cargo run --release --bin play -- --port 4180 --tick-seconds 30 [--humans 2]
//!     cargo run --release --bin play -- --port 4185 --chain http://127.0.0.1:4190
//!
//! Serves the web client from `web/`, `llms.txt` for agents, and a JSON API.
//! Orders are validated by the engine's own `validate_batch`; previews come
//! from `permutation_rules::preview`. Every view, preview and bot decision is
//! made from that civ's fogged belief state (`vision`, §7.4); only order
//! validation and resolution see the full state.
//!
//! Seats (one per civilization):
//! * human — claimed in the browser (`POST /api/claim`), which returns a seat
//!   token. Requests carrying it (`X-Seat-Token`) act for that civ. Several
//!   people can hold seats at once; a tick resolves early once every claimed
//!   seat ended its turn.
//! * bot — a scripted reference player run by this server.
//! * external — an outside agent (e.g. one that joined through x402). It
//!   reads its fogged view here (`?civ=N`), dry-runs orders with
//!   `/api/validate` and signs its batches itself with its own session key.
//!
//! Requests without a seat token read `?civ=N`'s fogged view, or without
//! `civ` the omniscient spectator view. Views are not secrets: the on-chain
//! world is public, so fog is an honest-play interface (see DESIGN.md); what
//! is enforced is who may order for a civ (the chain checks the session key).
//!
//! Two modes:
//! * local (default): the engine runs in this process.
//! * `--chain http://127.0.0.1:4190`: the world is the on-chain program's
//!   (read through permutation-gateway), ticks resolve on the MagicBlock ER,
//!   and seats submit on chain with their own session keys.

use permutation_rules::genesis::{new_season, Entry};
use permutation_rules::hex::Hex;
use permutation_rules::invariants;
use permutation_rules::markets::{clear_amm, AmmOrder};
use permutation_rules::orders::{validate_batch, Order, OrderBatch, Side};
use permutation_rules::rng::Seed;
use permutation_rules::state::{CivId, DeclaredKind, WorldState};
use permutation_rules::tick::{resolve_tick, TickInput};
use permutation_rules::{Preset, Ruleset};
use permutation_server::api::{self, OrderDto};
use permutation_server::bots::{persona_of, Bot, NAMES};
use permutation_server::chainlink::{ChainLink, Snapshot, WorldMeta};
use permutation_server::events::diff_events;
use permutation_server::fog::{line_is_public, Fog};
use permutation_server::ledger::{hex, Ledger};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long a claimed seat stays reserved without any request from its holder.
const SEAT_IDLE: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, PartialEq, Debug)]
enum SeatKind {
    Human,
    Bot,
    External,
}

impl SeatKind {
    fn name(self) -> &'static str {
        match self {
            SeatKind::Human => "human",
            SeatKind::Bot => "bot",
            SeatKind::External => "external",
        }
    }
}

impl Seat {
    /// Held by someone who is still here.
    fn active(&self) -> bool {
        self.token.is_some() && self.last_seen.is_some_and(|t| t.elapsed() < SEAT_IDLE)
    }
}

struct Seat {
    civ: CivId,
    kind: SeatKind,
    /// Token of whoever claimed this human seat.
    token: Option<String>,
    /// Last request made with that token. A seat idle for `SEAT_IDLE` can be
    /// claimed by someone else (a closed tab must not lock a seat forever).
    last_seen: Option<Instant>,
    /// The batch committed for the open tick (human seats).
    committed: Vec<OrderDto>,
    committed_cost: u32,
    /// Tick for which this seat ended its turn (local) or whose batch the
    /// chain accepted (chain mode).
    ready: Option<u16>,
}

/// Who is asking.
#[derive(Clone, Copy, PartialEq)]
enum Viewer {
    /// Holds the seat token of this civ: may order for it.
    Seat(CivId),
    /// Reads this civ's fogged view.
    Watch(CivId),
    /// Omniscient, read-only.
    Spectator,
}

impl Viewer {
    fn civ(self) -> Option<CivId> {
        match self {
            Viewer::Seat(c) | Viewer::Watch(c) => Some(c),
            Viewer::Spectator => None,
        }
    }
}

struct Game {
    rules: Ruleset,
    state: WorldState,
    fog: Fog,
    ledger: Ledger,
    bots: Vec<Bot>,
    seats: Vec<Seat>,
    deadline: Instant,
    tick_seconds: u64,
    paused: bool,
    /// `(tick, "kind|text")`, newest last; filtered per viewer when served.
    chronicle: Vec<(u16, String)>,
    /// Events of the last resolved tick (unfiltered).
    last_events: Vec<String>,
    resolved_tick: Option<u16>,
    chain: Option<ChainMode>,
    token_counter: u64,
}

/// Chain mode bookkeeping (see the module docs).
struct ChainMode {
    link: Arc<ChainLink>,
    meta: WorldMeta,
    slot: u64,
    layer: String,
    phase: String,
    /// The gateway's /season answer: accounts, program id, civ registry.
    info: Value,
    /// PS_TICK record of the last resolved tick (signature, CU, roots).
    last_record: Option<Value>,
    /// Tick the bots already submitted for.
    bots_submitted: Option<u16>,
    error: Option<String>,
}

fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn seat(civ: CivId, kind: SeatKind) -> Seat {
    Seat { civ, kind, token: None, last_seen: None, committed: Vec::new(), committed_cost: 0, ready: None }
}

impl Game {
    /// Local season: `humans` human seats (civs 0..humans), the rest bots.
    fn new(tick_seconds: u64, humans: usize) -> Game {
        let rules = Ruleset::new(Preset::Blitz);
        let world: Seed = *b"permutation-state/world/play-001";
        let season: Seed = *b"permutation-state/season/play-01";
        let humans = humans.clamp(1, NAMES.len());
        let entries: Vec<Entry> = NAMES
            .iter()
            .enumerate()
            .map(|(i, name)| Entry {
                name: name.to_string(),
                declared_kind: if i < humans { DeclaredKind::Human } else { DeclaredKind::Agent },
                payout_wallet: [i as u8 + 1; 32],
                exchange_deposit: 20_000_000, // 20 test USDC; no real funds
            })
            .collect();
        let state = new_season(&rules, &world, &season, &entries).expect("genesis");
        let seats: Vec<Seat> =
            (0..NAMES.len()).map(|i| seat(i as CivId, if i < humans { SeatKind::Human } else { SeatKind::Bot })).collect();
        Game::assemble(rules, state, seats, &world, tick_seconds, None)
    }

    /// Attach to a running on-chain season through the gateway.
    fn from_chain(link: Arc<ChainLink>) -> Result<Game, String> {
        let info = link.get_json("/season")?;
        let snap = loop {
            match link.world()? {
                Some(s) => break s,
                None => {
                    eprintln!("waiting for the on-chain world (genesis or delegation in progress)…");
                    std::thread::sleep(Duration::from_secs(2));
                }
            }
        };
        let rules = match snap.meta.preset {
            1 => Ruleset::new(Preset::Season),
            _ => Ruleset::new(Preset::Blitz),
        };
        let registry = info["season"]["civs"].as_array().cloned().unwrap_or_default();
        let seats = (0..snap.state.civs.len())
            .map(|i| {
                let hosted = registry.get(i).and_then(|c| c["hosted"].as_str()).unwrap_or("external");
                let kind = match hosted {
                    "human" => SeatKind::Human,
                    "bot" => SeatKind::Bot,
                    _ => SeatKind::External,
                };
                seat(i as CivId, kind)
            })
            .collect();
        let tick_seconds = snap.meta.tick_seconds as u64;
        let season_id = snap.meta.season_id.to_le_bytes();
        let chain = ChainMode {
            link,
            meta: snap.meta,
            slot: snap.slot,
            layer: snap.layer,
            phase: snap.phase,
            info,
            last_record: None,
            bots_submitted: None,
            error: None,
        };
        Ok(Game::assemble(rules, snap.state, seats, &season_id, tick_seconds, Some(chain)))
    }

    fn assemble(rules: Ruleset, state: WorldState, seats: Vec<Seat>, seed: &[u8], tick_seconds: u64, chain: Option<ChainMode>) -> Game {
        let bots = seats.iter().filter(|s| s.kind == SeatKind::Bot).map(|s| Bot::new(s.civ, persona_of(s.civ))).collect();
        let fog = Fog::new(&state);
        let mut ledger = Ledger::new(seed);
        ledger.observe(&state, &fog);
        let local = chain.is_none();
        Game {
            rules,
            state,
            fog,
            ledger,
            bots,
            seats,
            deadline: Instant::now() + Duration::from_secs(tick_seconds),
            tick_seconds,
            paused: local, // local games start paused so players can read the map first
            chronicle: Vec::new(),
            last_events: Vec::new(),
            resolved_tick: None,
            chain,
            token_counter: 0,
        }
    }

    fn names(&self) -> Vec<String> {
        self.state.civs.iter().map(|c| c.name.clone()).collect()
    }

    fn over(&self) -> bool {
        self.state.tick >= self.rules.ticks_per_season
    }

    fn viewer(&self, req: &Request) -> Viewer {
        if let Some(t) = req.header("x-seat-token").or_else(|| req.q("token")).filter(|t| !t.is_empty()) {
            if let Some(s) = self.seats.iter().find(|s| s.token.as_deref() == Some(t)) {
                return Viewer::Seat(s.civ);
            }
        }
        match req.qi::<CivId>("civ") {
            Some(c) if (c as usize) < self.state.civs.len() => Viewer::Watch(c),
            _ => Viewer::Spectator,
        }
    }

    fn claim(&mut self, civ: CivId) -> Result<String, &'static str> {
        self.token_counter += 1;
        let mut h = Sha256::new();
        h.update(b"PS/seat-token");
        h.update(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0).to_le_bytes());
        h.update(std::process::id().to_le_bytes());
        h.update(self.token_counter.to_le_bytes());
        h.update(civ.to_le_bytes());
        let token = hex(&h.finalize()[..16]);
        let s = self.seats.get_mut(civ as usize).ok_or("no such civ")?;
        if s.kind != SeatKind::Human {
            return Err("this seat is not a human seat");
        }
        if s.active() {
            return Err("this seat is already taken");
        }
        // A new holder replaces an idle one; the old token stops working.
        s.token = Some(token.clone());
        s.last_seen = Some(Instant::now());
        Ok(token)
    }

    /// Every claimed human seat ended its turn for the open tick.
    fn all_humans_ready(&self) -> bool {
        let tick = self.state.tick;
        let claimed: Vec<&Seat> = self.seats.iter().filter(|s| s.kind == SeatKind::Human && s.active()).collect();
        !claimed.is_empty() && claimed.iter().all(|s| s.ready == Some(tick))
    }

    /// A human seat's batch for the open tick: its committed orders plus the
    /// reveals of its earlier decisions.
    fn seat_batch(&mut self, civ: CivId) -> (Vec<OrderDto>, [u8; 32]) {
        let tick = self.state.tick;
        let digest = match self.ledger.record(tick, civ) {
            Some(r) => r.digest,
            None => self.ledger.commit(tick, civ, "human", ""),
        };
        let mut dtos = self.seats[civ as usize].committed.clone();
        let committed: Vec<Order> = dtos.iter().filter_map(|d| d.to_order().ok()).collect();
        dtos.extend(self.ledger.reveals(civ, tick, &committed).iter().map(OrderDto::from_order));
        (dtos, digest)
    }

    fn submission(&mut self, civ: CivId) -> Value {
        let tick = self.state.tick;
        let (dtos, digest) = self.seat_batch(civ);
        json!({"civ": civ, "tick": tick, "digest": hex(&digest), "orders": dtos})
    }

    /// Bookkeeping after a tick resolved (both modes).
    fn after_tick(&mut self, prev: &WorldState) {
        let names = self.names();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let ev = diff_events(prev, &self.state, &names);
        for e in &ev {
            self.chronicle.push((prev.tick, e.clone()));
        }
        if self.chronicle.len() > 400 {
            let cut = self.chronicle.len() - 400;
            self.chronicle.drain(..cut);
        }
        self.last_events = ev;
        self.fog.update(&self.state);
        self.resolved_tick = Some(prev.tick);
        for s in &mut self.seats {
            s.committed.clear();
            s.committed_cost = 0;
        }
    }

    /// Apply a chain snapshot. Returns the bot batches to submit (sent by the
    /// caller after releasing the lock).
    fn on_chain(&mut self, snap: Snapshot) -> Vec<Value> {
        let link = self.chain.as_ref().map(|c| c.link.clone()).expect("chain mode");
        if snap.state.tick != self.state.tick {
            let prev = std::mem::replace(&mut self.state, snap.state);
            let tick = prev.tick;
            self.after_tick(&prev);
            // Reveals count once the program resolved them (PS_TICK record);
            // outside agents' commitments are learnt from the same record.
            if let Ok(Some((batches, rec))) = link.resolved_batches(tick) {
                for b in &batches {
                    if self.seats.get(b.civ as usize).is_some_and(|s| s.kind == SeatKind::External) {
                        self.ledger.ingest_external(b);
                    } else {
                        self.ledger.mark_revealed(b.civ, &b.orders, tick);
                    }
                }
                if let Some(c) = self.chain.as_mut() {
                    c.last_record = Some(rec);
                }
            }
            self.ledger.observe(&self.state, &self.fog);
        } else {
            self.state = snap.state;
        }
        let tick = self.state.tick;
        let c = self.chain.as_mut().unwrap();
        c.meta = snap.meta;
        c.slot = snap.slot;
        c.layer = snap.layer;
        c.phase = snap.phase;
        // Orders go to the ER: only while the season is being played there.
        if c.meta.finished || c.phase != "playing" || tick >= self.rules.ticks_per_season || c.bots_submitted == Some(tick) {
            return Vec::new();
        }
        c.bots_submitted = Some(tick);
        let mut out = Vec::new();
        for b in &mut self.bots {
            let view = self.fog.belief(&self.state, b.civ);
            let mut orders = b.orders(&view, &self.rules, &self.fog.memory(b.civ).explored);
            let digest = self.ledger.commit(tick, b.civ, &b.policy(), &b.rationale(&view, &orders));
            let reveals = self.ledger.reveals(b.civ, tick, &orders);
            orders.extend(reveals);
            let dtos: Vec<OrderDto> = orders.iter().map(OrderDto::from_order).collect();
            out.push(json!({"civ": b.civ, "tick": tick, "digest": hex(&digest), "orders": dtos}));
        }
        out
    }

    /// Local mode: resolve the open tick in-process.
    fn advance(&mut self) {
        if self.over() {
            return;
        }
        let tick = self.state.tick;
        // Every batch commits to its decision (§4.3) and opens earlier ones.
        let mut batches = Vec::new();
        let humans: Vec<CivId> = self.seats.iter().filter(|s| s.kind == SeatKind::Human).map(|s| s.civ).collect();
        for civ in humans {
            let (dtos, digest) = self.seat_batch(civ);
            let orders = dtos.iter().filter_map(|o| o.to_order().ok()).collect();
            batches.push(OrderBatch { civ, tick, decision_digest: digest, orders });
        }
        for b in &mut self.bots {
            let view = self.fog.belief(&self.state, b.civ);
            let mut orders = b.orders(&view, &self.rules, &self.fog.memory(b.civ).explored);
            let digest = self.ledger.commit(tick, b.civ, &b.policy(), &b.rationale(&view, &orders));
            let reveals = self.ledger.reveals(b.civ, tick, &orders);
            orders.extend(reveals);
            batches.push(OrderBatch { civ: b.civ, tick, decision_digest: digest, orders });
        }
        // Which batches the engine will accept, so reveals are only marked when processed.
        let accepted: Vec<(CivId, Vec<Order>)> = batches
            .iter()
            .filter(|b| validate_batch(&self.state, &self.rules, b).is_ok())
            .map(|b| (b.civ, b.orders.clone()))
            .collect();
        let mut vrf = [0u8; 32];
        vrf[..2].copy_from_slice(&tick.to_le_bytes());
        vrf[2..10].copy_from_slice(&(self.deadline.elapsed().as_nanos() as u64).to_le_bytes());
        let prev = self.state.clone();
        if let Err(e) = resolve_tick(&mut self.state, &self.rules, &TickInput { vrf, batches }) {
            eprintln!("tick {tick} failed: {e}");
            return;
        }
        self.after_tick(&prev);
        for (civ, orders) in &accepted {
            self.ledger.mark_revealed(*civ, orders, tick);
        }
        self.ledger.observe(&self.state, &self.fog);
        let violations = invariants::check(&self.state, &self.rules);
        if !violations.is_empty() {
            eprintln!("invariant violations after tick {tick}: {violations:?}");
        }
        self.deadline = Instant::now() + Duration::from_secs(self.tick_seconds);
    }

    fn seats_json(&self) -> Value {
        let tick = self.state.tick;
        json!(self
            .seats
            .iter()
            .map(|s| json!({
                "civ": s.civ, "name": self.state.civs[s.civ as usize].name, "kind": s.kind.name(),
                "persona": match s.kind { SeatKind::Bot => persona_of(s.civ).name(), SeatKind::Human => "Human", SeatKind::External => "Agent" },
                "claimed": s.active(), "ready": s.ready == Some(tick),
            }))
            .collect::<Vec<_>>())
    }

    fn state_json(&self, viewer: Viewer) -> Value {
        let civ = viewer.civ();
        let mut v = match civ {
            Some(c) => api::world_view(&self.fog.belief(&self.state, c), &self.rules, Some(c), &self.fog),
            None => api::world_view(&self.state, &self.rules, None, &self.fog),
        };
        let secs = if let Some(c) = &self.chain {
            (c.meta.deadline - unix_now()).max(0) as f64
        } else if self.paused {
            self.tick_seconds as f64
        } else {
            self.deadline.saturating_duration_since(Instant::now()).as_secs_f64()
        };
        let names = self.names();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        // Research news is private to a civ and its allies; spectators see all.
        let public = |e: &str| civ.is_none_or(|c| line_is_public(&self.state, c, e, &names));
        let chronicle: Vec<Value> =
            self.chronicle.iter().rev().filter(|(_, e)| public(e)).take(120).map(|(t, e)| json!({"tick": t, "text": e})).collect();
        let seat = match viewer {
            Viewer::Seat(c) => self.seats.get(c as usize),
            _ => None,
        };
        let t = self.state.tick;
        let o = v.as_object_mut().unwrap();
        o.insert("viewer".into(), json!(match viewer { Viewer::Seat(_) => "seat", Viewer::Watch(_) => "watch", Viewer::Spectator => "spectator" }));
        o.insert("secondsLeft".into(), json!(secs));
        o.insert("tickSeconds".into(), json!(self.tick_seconds));
        o.insert("paused".into(), json!(self.paused));
        o.insert("over".into(), json!(self.over()));
        o.insert("committed".into(), json!(seat.map(|s| s.committed.clone()).unwrap_or_default()));
        o.insert("committedCost".into(), json!(seat.map_or(0, |s| s.committed_cost)));
        o.insert("ready".into(), json!(seat.is_some_and(|s| s.ready == Some(t))));
        o.insert("chronicle".into(), json!(chronicle));
        o.insert("lastSummary".into(), json!(self.last_events.iter().filter(|e| public(e)).collect::<Vec<_>>()));
        o.insert("resolvedTick".into(), json!(self.resolved_tick));
        o.insert("seats".into(), self.seats_json());
        if let Some(c) = civ {
            // obsRoot is public: an outside agent commits its decision against it.
            let own = matches!(viewer, Viewer::Seat(_));
            o.insert(
                "decision".into(),
                json!({
                    "tick": t,
                    "obsRoot": self.ledger.root(t, c).map(|h| hex(&h)),
                    "digest": if own { self.ledger.record(t, c).map(|r| hex(&r.digest)) } else { None },
                    "rationale": if own { self.ledger.record(t, c).map(|r| r.text.clone()) } else { None },
                }),
            );
        }
        if let Some(c) = &self.chain {
            o.insert(
                "chain".into(),
                json!({
                    "seasonId": c.meta.season_id.to_string(),
                    "programId": c.info["programId"],
                    "cluster": c.info["cluster"],
                    "accounts": c.info["accounts"],
                    "endpoints": c.info["endpoints"],
                    "gateway": c.link.url(),
                    "layer": c.layer, "slot": c.slot, "phase": c.phase,
                    "finished": c.meta.finished,
                    "humanSubmitted": seat.is_some_and(|s| s.ready == Some(t)),
                    "lastTick": c.last_record.as_ref().map(|r| json!({"tick": r["tick"], "signature": r["signature"], "cu": r["cu"], "root": r["root"], "preRoot": r["preRoot"]})),
                    "error": c.error,
                }),
            );
        }
        o.insert(
            "personas".into(),
            json!(self
                .seats
                .iter()
                .map(|s| match s.kind {
                    SeatKind::Human => "Human",
                    SeatKind::External => "Agent",
                    SeatKind::Bot => persona_of(s.civ).name(),
                })
                .collect::<Vec<_>>()),
        );
        v
    }
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
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: content-type, x-seat-token\r\nConnection: close\r\n\r\n",
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
    if rel.contains("..") {
        return respond(stream, "400 Bad Request", "text/plain", b"bad path");
    }
    let file: PathBuf = web.join(rel);
    match std::fs::read(&file) {
        Ok(bytes) => {
            let ctype = match file.extension().and_then(|e| e.to_str()) {
                Some("html") => "text/html; charset=utf-8",
                Some("css") => "text/css; charset=utf-8",
                Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
                Some("svg") => "image/svg+xml",
                Some("png") => "image/png",
                Some("txt") | Some("md") => "text/plain; charset=utf-8",
                _ => "application/octet-stream",
            };
            respond(stream, "200 OK", ctype, &bytes)
        }
        Err(_) => respond(stream, "404 Not Found", "text/plain", b"not found"),
    }
}

fn parse_orders(body: &Value) -> Result<Vec<(OrderDto, Order)>, String> {
    let dtos: Vec<OrderDto> = serde_json::from_value(body["orders"].clone()).map_err(|e| e.to_string())?;
    dtos.into_iter().map(|d| d.to_order().map(|o| (d, o))).collect()
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
    if let Viewer::Seat(c) = viewer {
        g.seats[c as usize].last_seen = Some(Instant::now());
    }
    // Everything a viewer can ask about is answered from its belief state
    // (spectators: the full state).
    let belief = match viewer.civ() {
        Some(c) => g.fog.belief(&g.state, c),
        None => g.state.clone(),
    };
    let (s, r) = (&belief, &g.rules);
    let need_civ = |stream: &mut TcpStream| {
        json_status(stream, "400 Bad Request", &json!({"ok": false, "error": "previews are per civilization: pass ?civ=N or a seat token"}))
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
        ("GET", "/api/seats") => {
            let v = json!({"seats": g.seats_json(), "mode": if g.chain.is_some() { "chain" } else { "local" }, "you": match viewer { Viewer::Seat(c) => json!(c), _ => Value::Null }});
            json_ok(&mut stream, &v)
        }
        ("POST", "/api/claim") => {
            let civ = req.json()["civ"].as_u64().unwrap_or(u64::MAX).min(u16::MAX as u64) as CivId;
            match g.claim(civ) {
                Ok(token) => {
                    eprintln!("seat {civ} ({}) claimed", g.state.civs[civ as usize].name);
                    json_ok(&mut stream, &json!({"ok": true, "civ": civ, "token": token}))
                }
                Err(e) => json_status(&mut stream, "409 Conflict", &json!({"ok": false, "error": e})),
            }
        }
        ("GET", "/api/preview/unit") => match viewer.civ() {
            Some(c) => json_ok(&mut stream, &api::unit_preview(s, r, c, req.qi("id").unwrap_or(u32::MAX))),
            None => need_civ(&mut stream),
        },
        ("GET", "/api/preview/city") => match viewer.civ() {
            Some(c) => json_ok(&mut stream, &api::city_preview(s, r, c, req.qi("id").unwrap_or(u32::MAX))),
            None => need_civ(&mut stream),
        },
        ("GET", "/api/preview/research") => match viewer.civ() {
            Some(c) => json_ok(&mut stream, &api::research_preview(s, r, c)),
            None => need_civ(&mut stream),
        },
        ("GET", "/api/preview/diplomacy") => match viewer.civ() {
            Some(c) => json_ok(&mut stream, &api::diplomacy_preview(s, r, c, req.qi("with").or(req.qi("other")).unwrap_or(u16::MAX))),
            None => need_civ(&mut stream),
        },
        ("GET", "/api/preview/path") => {
            let (Some(unit), Some(q), Some(rr)) = (req.qi::<u32>("unit"), req.qi::<i32>("q"), req.qi::<i32>("r")) else {
                return respond(&mut stream, "400 Bad Request", "text/plain", b"unit, q, r required");
            };
            let goal = Hex::new(q, rr);
            let path = permutation_rules::preview::path_to(s, r, unit, goal);
            // When there is no path, say why: the destination itself, or the route.
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
            // Dry run for any civ (outside agents check before signing).
            let body = req.json();
            let civ = match viewer {
                Viewer::Seat(c) => c,
                _ => match body["civ"].as_u64() {
                    Some(c) if (c as usize) < g.state.civs.len() => c as CivId,
                    _ => return json_status(&mut stream, "400 Bad Request", &json!({"ok": false, "error": "civ required"})),
                },
            };
            let v = match parse_orders(&body) {
                Err(e) => json!({"ok": false, "error": e}),
                Ok(pairs) => {
                    let batch = OrderBatch { civ, tick: g.state.tick, decision_digest: [0; 32], orders: pairs.into_iter().map(|(_, o)| o).collect() };
                    let spendable = permutation_rules::orders::spendable(&g.state, civ);
                    // Judged from what this civ knows (its belief state).
                    let belief = g.fog.belief(&g.state, civ);
                    let warnings = api::preflight(&belief, &g.rules, civ, &batch.orders);
                    match validate_batch(&g.state, &g.rules, &batch) {
                        Ok(cost) => json!({"ok": true, "tick": g.state.tick, "cost": cost, "spendable": spendable, "warnings": warnings}),
                        Err(e) => json!({"ok": false, "tick": g.state.tick, "error": e.to_string(), "spendable": spendable, "warnings": warnings}),
                    }
                }
            };
            json_ok(&mut stream, &v)
        }
        ("POST", "/api/orders") => {
            let Viewer::Seat(civ) = viewer else {
                return json_status(&mut stream, "403 Forbidden", &json!({"ok": false, "error": "claim a seat first (POST /api/claim); outside agents sign their own batches"}));
            };
            let body = req.json();
            let pairs = match parse_orders(&body) {
                Ok(p) => p,
                Err(e) => return json_ok(&mut stream, &json!({"ok": false, "error": e})),
            };
            let (dtos, orders): (Vec<OrderDto>, Vec<Order>) = pairs.into_iter().unzip();
            let batch = OrderBatch { civ, tick: g.state.tick, decision_digest: [0; 32], orders };
            match validate_batch(&g.state, &g.rules, &batch) {
                Ok(cost) => {
                    let text = body["rationale"].as_str().unwrap_or("").trim().to_string();
                    let tick = g.state.tick;
                    let digest = g.ledger.commit(tick, civ, "human", &text);
                    let seat = &mut g.seats[civ as usize];
                    seat.committed = dtos;
                    seat.committed_cost = cost;
                    if g.chain.is_none() {
                        return json_ok(&mut stream, &json!({"ok": true, "cost": cost, "tick": tick, "digest": hex(&digest)}));
                    }
                    // Chain mode: submit now (this ends the seat's turn).
                    let body = g.submission(civ);
                    let link = g.chain.as_ref().unwrap().link.clone();
                    drop(g);
                    match link.post_json("/submit", &body) {
                        Ok(r) => {
                            let mut g = game.lock().unwrap();
                            g.seats[civ as usize].ready = Some(tick);
                            if let Some(c) = g.chain.as_mut() {
                                c.error = None;
                            }
                            json_ok(&mut stream, &json!({"ok": true, "cost": cost, "tick": tick, "digest": hex(&digest), "signature": r["signature"]}))
                        }
                        Err(e) => json_ok(&mut stream, &json!({"ok": false, "error": format!("chain: {e}")})),
                    }
                }
                Err(e) => json_ok(&mut stream, &json!({"ok": false, "error": e.to_string()})),
            }
        }
        ("GET", "/api/decisions") => {
            let limit: usize = req.qi("limit").unwrap_or(120).min(600);
            let open = g.state.tick;
            let own = match viewer {
                Viewer::Seat(c) => Some(c),
                _ => None,
            };
            let records: Vec<Value> = g
                .ledger
                .history(limit)
                .into_iter()
                .filter(|r| r.tick < open || Some(r.civ) == own)
                .map(|r| {
                    let revealed = r.revealed_at.is_some();
                    json!({
                        "tick": r.tick, "civ": r.civ,
                        "obsRoot": hex(&r.obs_root), "digest": hex(&r.digest),
                        "external": r.external,
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
            let (Some(tick), Some(civ), Some(kind), Some(id)) =
                (req.qi::<u16>("tick"), req.qi::<CivId>("of").or(req.qi("civ")), req.q("kind"), req.qi::<u32>("id"))
            else {
                return respond(&mut stream, "400 Bad Request", "text/plain", b"tick, civ, kind, id required");
            };
            // Past observations only; another civ's view only down to fog/ownership of a tile.
            if tick >= g.state.tick || (viewer != Viewer::Seat(civ) && kind != "tile" && kind != "header") {
                return json_ok(&mut stream, &json!({"ok": false, "error": "not available"}));
            }
            match g.ledger.proof(tick, civ, kind, id) {
                Some((root, leaf, proof)) => {
                    let v = json!({
                        "ok": true, "tick": tick, "civ": civ, "root": hex(&root),
                        "kind": leaf.kind, "id": leaf.id, "body": hex(&leaf.body),
                        "proof": proof.iter().map(|p| json!([hex(&p.sibling), p.left])).collect::<Vec<_>>(),
                    });
                    json_ok(&mut stream, &v)
                }
                None => json_ok(&mut stream, &json!({"ok": false, "error": "no such leaf (older observations keep only their root)"})),
            }
        }
        ("POST", "/api/control") => {
            let Viewer::Seat(civ) = viewer else {
                return json_status(&mut stream, "403 Forbidden", &json!({"ok": false, "error": "only seated players control the game"}));
            };
            let v = req.json();
            let tick = g.state.tick;
            if g.chain.is_some() {
                // Chain mode: no pause; "advance" ends the seat's turn by
                // submitting whatever is committed (possibly nothing).
                if v["advance"].as_bool() == Some(true) {
                    let body = g.submission(civ);
                    let link = g.chain.as_ref().unwrap().link.clone();
                    drop(g);
                    let res = link.post_json("/submit", &body);
                    let mut g = game.lock().unwrap();
                    match &res {
                        Ok(_) => g.seats[civ as usize].ready = Some(tick),
                        Err(e) => {
                            if let Some(c) = g.chain.as_mut() {
                                c.error = Some(e.clone());
                            }
                        }
                    }
                    return json_ok(&mut stream, &json!({"ok": res.is_ok(), "tick": tick, "error": res.err()}));
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
                // End this seat's turn; the tick resolves once every seated player did.
                g.seats[civ as usize].ready = Some(tick);
                if g.all_humans_ready() {
                    g.advance();
                }
            }
            let waiting: Vec<CivId> = g
                .seats
                .iter()
                .filter(|s| s.kind == SeatKind::Human && s.active() && s.ready != Some(g.state.tick))
                .map(|s| s.civ)
                .collect();
            json_ok(&mut stream, &json!({"ok": true, "paused": g.paused, "tick": g.state.tick, "waitingFor": waiting}))
        }
        _ => json_status(&mut stream, "404 Not Found", &json!({"ok": false, "error": "no such endpoint; see /llms.txt"})),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let port: u16 = arg("--port").and_then(|p| p.parse().ok()).unwrap_or(4180);
    let tick_seconds: u64 = arg("--tick-seconds").and_then(|p| p.parse().ok()).unwrap_or(30);
    let humans: usize = arg("--humans").and_then(|p| p.parse().ok()).unwrap_or(1);
    let web = arg("--web").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("web"));

    let chain = arg("--chain");
    let game = match &chain {
        Some(url) => {
            let link = Arc::new(ChainLink::new(url).expect("--chain http://host:port"));
            Arc::new(Mutex::new(Game::from_chain(link).expect("attach to the on-chain season")))
        }
        None => Arc::new(Mutex::new(Game::new(tick_seconds, humans))),
    };
    if chain.is_some() {
        // Follow the chain: new ticks, then the bots' on-chain submissions.
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
            let submissions = game.lock().unwrap().on_chain(snap);
            for body in submissions {
                if let Err(e) = link.post_json("/submit", &body) {
                    eprintln!("bot civ {} submit failed: {e}", body["civ"]);
                }
            }
        });
    } else {
        let game = game.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(150));
            let mut g = game.lock().unwrap();
            if !g.paused && !g.over() && Instant::now() >= g.deadline {
                g.advance();
            }
        });
    }
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind");
    {
        let g = game.lock().unwrap();
        eprintln!("PERMUTATION STATE on http://127.0.0.1:{port}/  (web: {})", web.display());
        for s in &g.seats {
            eprintln!("  seat {} {:<9} {}", s.civ, g.state.civs[s.civ as usize].name, s.kind.name());
        }
        eprintln!("  spectate: http://127.0.0.1:{port}/?spectate   agents: http://127.0.0.1:{port}/llms.txt");
    }
    for stream in listener.incoming().flatten() {
        let (game, web) = (game.clone(), web.clone());
        std::thread::spawn(move || handle(stream, game, web));
    }
}
