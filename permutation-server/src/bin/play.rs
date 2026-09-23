//! Local playable server: one human civilization against five scripted bots.
//!
//!     cargo run --release --bin play -- --port 4180 --tick-seconds 30
//!
//! Serves the web client from `web/` and a small JSON API. Orders are
//! validated by the engine's own `validate_batch`; previews come from
//! `permutation_rules::preview`. This is a single-machine prototype: no
//! wallets, no fog of war yet, and the "advance now" control exists only
//! because it is single-player.

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
use permutation_server::bots::{Bot, NAMES, PERSONAS};
use permutation_server::events::diff_events;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const HUMAN: CivId = 0;

struct Game {
    rules: Ruleset,
    state: WorldState,
    bots: Vec<Bot>,
    /// The human's committed batch for the open tick.
    committed: Vec<OrderDto>,
    committed_cost: u32,
    deadline: Instant,
    tick_seconds: u64,
    paused: bool,
    /// `(tick, "kind|text")`, newest last.
    chronicle: Vec<(u16, String)>,
    last_summary: Vec<String>,
    resolved_tick: Option<u16>,
}

impl Game {
    fn new(tick_seconds: u64) -> Game {
        let rules = Ruleset::new(Preset::Blitz);
        let world: Seed = *b"permutation-state/world/play-001";
        let season: Seed = *b"permutation-state/season/play-01";
        let entries: Vec<Entry> = NAMES
            .iter()
            .enumerate()
            .map(|(i, name)| Entry {
                name: name.to_string(),
                declared_kind: if i == HUMAN as usize { DeclaredKind::Human } else { DeclaredKind::Agent },
                payout_wallet: [i as u8 + 1; 32],
                exchange_deposit: 20_000_000, // 20 test USDC; no real funds
            })
            .collect();
        let state = new_season(&rules, &world, &season, &entries).expect("genesis");
        let bots = PERSONAS
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, p)| Bot::new(i as CivId, *p))
            .collect();
        Game {
            rules,
            state,
            bots,
            committed: Vec::new(),
            committed_cost: 0,
            deadline: Instant::now() + Duration::from_secs(tick_seconds),
            tick_seconds,
            paused: true, // start paused so the player can read the map first
            chronicle: Vec::new(),
            last_summary: Vec::new(),
            resolved_tick: None,
        }
    }

    fn over(&self) -> bool {
        self.state.tick >= self.rules.ticks_per_season
    }

    fn advance(&mut self) {
        if self.over() {
            return;
        }
        let tick = self.state.tick;
        let human: Vec<Order> = self.committed.iter().filter_map(|o| o.to_order().ok()).collect();
        let mut batches = vec![OrderBatch { civ: HUMAN, tick, decision_digest: [0; 32], orders: human }];
        for b in &mut self.bots {
            batches.push(OrderBatch { civ: b.civ, tick, decision_digest: [0; 32], orders: b.orders(&self.state, &self.rules) });
        }
        let mut vrf = [0u8; 32];
        vrf[..2].copy_from_slice(&tick.to_le_bytes());
        vrf[2..10].copy_from_slice(&(self.deadline.elapsed().as_nanos() as u64).to_le_bytes());
        let prev = self.state.clone();
        if let Err(e) = resolve_tick(&mut self.state, &self.rules, &TickInput { vrf, batches }) {
            eprintln!("tick {tick} failed: {e}");
            return;
        }
        let violations = invariants::check(&self.state, &self.rules);
        if !violations.is_empty() {
            eprintln!("invariant violations after tick {tick}: {violations:?}");
        }
        let names: Vec<&str> = NAMES.to_vec();
        let ev = diff_events(&prev, &self.state, &names);
        for e in &ev {
            self.chronicle.push((tick, e.clone()));
        }
        if self.chronicle.len() > 400 {
            let cut = self.chronicle.len() - 400;
            self.chronicle.drain(..cut);
        }
        self.last_summary = ev;
        self.resolved_tick = Some(tick);
        self.committed.clear();
        self.committed_cost = 0;
        self.deadline = Instant::now() + Duration::from_secs(self.tick_seconds);
    }

    fn state_json(&self) -> Value {
        let mut v = api::world_view(&self.state, &self.rules, HUMAN);
        let secs = if self.paused {
            self.tick_seconds as f64
        } else {
            self.deadline.saturating_duration_since(Instant::now()).as_secs_f64()
        };
        let chronicle: Vec<Value> = self
            .chronicle
            .iter()
            .rev()
            .take(120)
            .map(|(t, e)| json!({"tick": t, "text": e}))
            .collect();
        let o = v.as_object_mut().unwrap();
        o.insert("secondsLeft".into(), json!(secs));
        o.insert("tickSeconds".into(), json!(self.tick_seconds));
        o.insert("paused".into(), json!(self.paused));
        o.insert("over".into(), json!(self.over()));
        o.insert("committed".into(), json!(self.committed));
        o.insert("committedCost".into(), json!(self.committed_cost));
        o.insert("chronicle".into(), json!(chronicle));
        o.insert("lastSummary".into(), json!(self.last_summary));
        o.insert("resolvedTick".into(), json!(self.resolved_tick));
        o.insert(
            "personas".into(),
            json!(NAMES.iter().enumerate().map(|(i, _)| if i == HUMAN as usize { "Human" } else { PERSONAS[i].name() }).collect::<Vec<_>>()),
        );
        v
    }
}

// ------------------------------------------------------------------ HTTP

struct Request {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn q(&self, k: &str) -> Option<&str> {
        self.query.iter().find(|(a, _)| a == k).map(|(_, b)| b.as_str())
    }
    fn qi<T: std::str::FromStr>(&self, k: &str) -> Option<T> {
        self.q(k)?.parse().ok()
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
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).ok()? == 0 || h == "\r\n" || h == "\n" {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                len = v.trim().parse().unwrap_or(0);
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
    Some(Request { method, path: path.to_string(), query, body })
}

fn respond(stream: &mut TcpStream, status: &str, ctype: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

fn json_ok(stream: &mut TcpStream, v: &Value) {
    respond(stream, "200 OK", "application/json; charset=utf-8", v.to_string().as_bytes());
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
                _ => "application/octet-stream",
            };
            respond(stream, "200 OK", ctype, &bytes)
        }
        Err(_) => respond(stream, "404 Not Found", "text/plain", b"not found"),
    }
}

fn handle(mut stream: TcpStream, game: Arc<Mutex<Game>>, web: PathBuf) {
    let Some(req) = read_request(&mut stream) else { return };
    let mut g = game.lock().unwrap();
    let (s, r) = (&g.state, &g.rules);
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/api/map") => {
            let mut v = api::map_view(s);
            v["civNames"] = json!(NAMES);
            json_ok(&mut stream, &v)
        }
        ("GET", "/api/state") => {
            let v = g.state_json();
            json_ok(&mut stream, &v)
        }
        ("GET", "/api/preview/unit") => {
            let v = api::unit_preview(s, r, HUMAN, req.qi("id").unwrap_or(u32::MAX));
            json_ok(&mut stream, &v)
        }
        ("GET", "/api/preview/city") => {
            let v = api::city_preview(s, r, HUMAN, req.qi("id").unwrap_or(u32::MAX));
            json_ok(&mut stream, &v)
        }
        ("GET", "/api/preview/research") => {
            let v = api::research_preview(s, r, HUMAN);
            json_ok(&mut stream, &v)
        }
        ("GET", "/api/preview/diplomacy") => {
            let v = api::diplomacy_preview(s, r, HUMAN, req.qi("civ").unwrap_or(u16::MAX));
            json_ok(&mut stream, &v)
        }
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
        ("POST", "/api/orders") => {
            let parsed: Result<Vec<OrderDto>, _> = serde_json::from_slice::<Value>(&req.body)
                .map_err(|e| e.to_string())
                .and_then(|v| serde_json::from_value(v["orders"].clone()).map_err(|e| e.to_string()));
            let dtos = match parsed {
                Ok(d) => d,
                Err(e) => return json_ok(&mut stream, &json!({"ok": false, "error": e})),
            };
            let orders: Result<Vec<Order>, String> = dtos.iter().map(|d| d.to_order()).collect();
            let orders = match orders {
                Ok(o) => o,
                Err(e) => return json_ok(&mut stream, &json!({"ok": false, "error": e})),
            };
            let batch = OrderBatch { civ: HUMAN, tick: s.tick, decision_digest: [0; 32], orders };
            match validate_batch(s, r, &batch) {
                Ok(cost) => {
                    g.committed = dtos;
                    g.committed_cost = cost;
                    json_ok(&mut stream, &json!({"ok": true, "cost": cost, "tick": g.state.tick}))
                }
                Err(e) => json_ok(&mut stream, &json!({"ok": false, "error": e.to_string()})),
            }
        }
        ("POST", "/api/control") => {
            let v: Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
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
                g.advance();
            }
            json_ok(&mut stream, &json!({"ok": true, "paused": g.paused, "tick": g.state.tick}))
        }
        ("GET", path) => {
            drop(g);
            static_file(&mut stream, &web, path)
        }
        _ => respond(&mut stream, "405 Method Not Allowed", "text/plain", b"method not allowed"),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let port: u16 = arg("--port").and_then(|p| p.parse().ok()).unwrap_or(4180);
    let tick_seconds: u64 = arg("--tick-seconds").and_then(|p| p.parse().ok()).unwrap_or(30);
    let web = arg("--web").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("web"));

    let game = Arc::new(Mutex::new(Game::new(tick_seconds)));
    {
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
    eprintln!("PERMUTATION STATE playable prototype on http://127.0.0.1:{port}/  (web: {})", web.display());
    for stream in listener.incoming().flatten() {
        let (game, web) = (game.clone(), web.clone());
        std::thread::spawn(move || handle(stream, game, web));
    }
}
