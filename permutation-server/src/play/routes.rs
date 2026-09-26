//! The JSON API. Each route runs under the game lock and returns what to
//! answer; a route that must call the gateway returns an `Outcome` that is
//! carried out after the lock is released. See `llms.txt` for the API as
//! agents read it.
//!
//! In chain mode every member signs its own transactions: the routes that
//! act for a member are refused, the same way for every member, and
//! nothing here depends on who runs a member (V5 §18.2).

use permutation_rules::gov::{self, GovAction, GovEntry, MemberId, Role, NOBODY};
use permutation_rules::hex::Hex;
use permutation_rules::markets::{clear_amm, AmmOrder};
use permutation_rules::orders::{
    check_structure, role_allows, spendable, validate_batch, Order, OrderBatch, Side,
};
use permutation_rules::state::{CivId, WorldState};
use serde_json::{json, Value};
use std::borrow::Cow;
use std::net::TcpStream;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::game::{role_name, Game, Host, Phase, Viewer};
use super::http::{Request, Response, IO_TIMEOUT};
use super::talk::MAX_TALK_CHARS;
use super::views::pending_lobby;
use super::{lock, proxy, Site};
use crate::api::{self, GovDto, MemberMeta, OrderDto};
use crate::chainlink::ChainLink;
use crate::codec::hex;
use crate::driver::local_key;
use crate::ledger::DEFAULT_POLICY;

/// Longest member name kept.
const MAX_NAME: usize = 24;
/// Largest simulated treasury deposit in the lobby (100 test USDC).
const MAX_LOCAL_DEPOSIT: u64 = 100_000_000;
/// Chain mode's answer to every route that would act for a member.
pub const CHAIN_REFUSAL: &str = "chain mode: sign in the browser";
/// Routes that act for a member (local mode only).
const ACTING: [(&str, &str); 5] = [
    ("POST", "/api/orders"),
    ("POST", "/api/control"),
    ("POST", "/api/gov"),
    ("POST", "/api/talk"),
    ("POST", "/api/lobby"),
];

/// Answer one connection.
pub fn handle(mut stream: TcpStream, site: &Site) {
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let Some(mut req) = Request::read(&stream) else {
        return;
    };
    req.peer = stream.peer_addr().ok().map(|a| a.ip());
    respond(&req, site).write(&mut stream);
}

pub fn respond(req: &Request, site: &Site) -> Response {
    // The gateway's public listener, never with anything of this server's.
    if proxy::is_proxied(&req.path) {
        return match &site.proxy {
            Some(p) => p.forward(req),
            None => Response::error("404 Not Found", "no gateway here (see --gateway-proxy)"),
        };
    }
    if req.method == "OPTIONS" {
        return Response::text("204 No Content", "");
    }
    if !req.path.starts_with("/api/") {
        return Response::static_file(&site.web, &req.path);
    }
    let gateway = site.gateway.as_deref().unwrap_or("");
    let Some(game) = site.game() else {
        return pending(req, "registering", gateway, site.season().as_ref());
    };
    let outcome = {
        let mut g = lock(game);
        if g.starting() {
            let info = g.chain.as_ref().map(|c| &c.info);
            return pending(req, "starting", gateway, info);
        }
        route(&mut g, req)
    };
    outcome.finish()
}

/// Chain mode before the government opened: the lobby, and 503 for the
/// rest of the API.
fn pending(req: &Request, phase: &str, gateway: &str, info: Option<&Value>) -> Response {
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/api/lobby") => Response::ok(&pending_lobby(phase, gateway, info)),
        _ => Response::error("503 Service Unavailable", "registering"),
    }
}

/// What a route decided.
pub enum Outcome {
    Reply(Response),
    /// Read a gateway endpoint (the history layer).
    Fetch {
        link: Arc<ChainLink>,
        path: &'static str,
    },
}

impl From<Response> for Outcome {
    fn from(r: Response) -> Outcome {
        Outcome::Reply(r)
    }
}

impl Outcome {
    /// Carry out the gateway part, without the lock.
    fn finish(self) -> Response {
        match self {
            Outcome::Reply(r) => r,
            Outcome::Fetch { link, path } => match link.get_json(path) {
                Ok(v) => Response::ok(&v),
                Err(e) => not_ok(format!("gateway: {e}")),
            },
        }
    }
}

/// `{"ok": false, "error": …}` with 200: the request was understood, the
/// game (or the chain) said no.
fn not_ok(error: impl Into<String>) -> Response {
    Response::ok(&json!({"ok": false, "error": error.into()}))
}

fn refused(error: impl Into<String>) -> Outcome {
    not_ok(error).into()
}

fn route(g: &mut Game, req: &Request) -> Outcome {
    let (method, path) = (req.method.as_str(), req.path.as_str());
    // Before anything else: the answer must not depend on who asks.
    if g.chain.is_some() {
        if ACTING.contains(&(method, path)) {
            return Response::error("400 Bad Request", CHAIN_REFUSAL).into();
        }
        if (method, path) == ("POST", "/api/start") {
            return Response::error("400 Bad Request", "chain mode: the season starts on chain")
                .into();
        }
    }
    let viewer = g.viewer(req);
    if let Viewer::Member(m) = viewer {
        g.members[m as usize].last_seen = Some(Instant::now());
    }
    let civ = g.civ_of(viewer);
    match (method, path) {
        ("GET", "/api/map") => {
            let mut v = api::map_view(&belief(g, civ));
            v["civNames"] = json!(g.names());
            Response::ok(&v).into()
        }
        ("GET", "/api/state") => Response::ok(&g.state_json(viewer)).into(),
        ("GET", "/api/lobby") => Response::ok(&g.lobby_json(viewer)).into(),
        ("POST", "/api/join") => join(g, req),
        ("POST", "/api/lobby") => lobby_choices(g, viewer, req),
        ("POST", "/api/start") => start(g, viewer),
        ("GET", path) if path.starts_with("/api/preview/") => {
            preview(g, civ, req, &path["/api/preview/".len()..])
        }
        ("POST", "/api/validate") => validate(g, civ, req),
        ("POST", "/api/orders") => orders(g, viewer, req),
        ("POST", "/api/gov") => governance(g, viewer, req),
        ("GET", "/api/decisions") => decisions(g, viewer, civ, req),
        ("GET", "/api/decisions/proof") => proof(g, civ, req),
        ("POST", "/api/control") => control(g, viewer, req),
        ("GET", "/api/talk") => Response::ok(&json!({"messages": g.talk})).into(),
        ("POST", "/api/talk") => talk(g, viewer, req),
        ("GET", "/api/roster") => Response::ok(&g.roster_json()).into(),
        // The history layer: the seasons this one follows (chain mode).
        ("GET", "/api/history") => match g.chain.as_ref() {
            Some(c) => Outcome::Fetch {
                link: c.link.clone(),
                path: "/history",
            },
            None => Response::ok(&json!({"lineage": []})).into(),
        },
        _ => Response::error("404 Not Found", "no such endpoint; see /llms.txt").into(),
    }
}

/// What `civ` believes the world is (spectators: the full state).
fn belief(g: &Game, civ: Option<CivId>) -> Cow<'_, WorldState> {
    match civ {
        Some(c) => g.fog.belief(&g.state, c),
        None => Cow::Borrowed(&g.state),
    }
}

fn parse_orders(v: &Value) -> Result<Vec<(OrderDto, Order)>, String> {
    let dtos: Vec<OrderDto> = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    dtos.into_iter()
        .map(|d| d.to_order().map(|o| (d, o)))
        .collect()
}

/// `{"General": [proposalId, …], …}`: the proposals to adopt per office.
fn parse_adopt(v: &Value) -> Vec<(Role, Vec<u32>)> {
    v.as_object()
        .map(|o| {
            o.iter()
                .filter_map(|(k, v)| {
                    Some((
                        api::parse_role(k)?,
                        v.as_array()?
                            .iter()
                            .filter_map(|x| x.as_u64().map(|x| x as u32))
                            .collect(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn adopted(adopt: &[(Role, Vec<u32>)], role: Role) -> Vec<u32> {
    adopt
        .iter()
        .find(|(r, _)| *r == role)
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

/// A `["General", "Steward"]` list as a `Role::bit` mask.
fn parse_roles(v: &Value) -> u8 {
    v.as_array().map_or(0, |a| {
        a.iter()
            .filter_map(|r| r.as_str().and_then(api::parse_role))
            .fold(0, |acc, r| acc | r.bit())
    })
}

// ------------------------------------------------------------------ lobby

fn join(g: &mut Game, req: &Request) -> Outcome {
    let b = req.json();
    let Some(nation) = b["civ"]
        .as_u64()
        .filter(|c| (*c as usize) < g.state.civs.len())
    else {
        return Response::error("400 Bad Request", "civ (nation) required").into();
    };
    let name: String = b["name"]
        .as_str()
        .unwrap_or("")
        .trim()
        .chars()
        .take(MAX_NAME)
        .collect();
    let name = if name.is_empty() {
        format!("Player {}", g.members.len() + 1)
    } else {
        name
    };
    let kind = match b["kind"].as_str() {
        Some("agent") => "agent",
        Some("human") | None => "human",
        _ => "undeclared",
    };
    let meta = MemberMeta {
        name,
        kind: kind.into(),
        attested: false,
    };
    match g.join_local(nation as CivId, meta, Host::Human) {
        Ok(id) => {
            let token = g.token(id as u64);
            let x = &mut g.members[id as usize];
            x.token = Some(token.clone());
            x.last_seen = Some(Instant::now());
            x.stand = parse_roles(&b["stand"]);
            x.deposit = b["deposit"].as_u64().unwrap_or(0).min(MAX_LOCAL_DEPOSIT);
            eprintln!("member {id} joined nation {nation}");
            Response::ok(&json!({"ok": true, "member": id, "civ": nation, "token": token})).into()
        }
        Err(e) => Response::error("409 Conflict", e).into(),
    }
}

/// A message from the member this browser holds (V5 §18.7), local mode (in
/// chain mode members sign their messages and send them to the gateway).
fn talk(g: &mut Game, viewer: Viewer, req: &Request) -> Outcome {
    let Viewer::Member(m) = viewer else {
        return Response::error("403 Forbidden", "join first").into();
    };
    let b = req.json();
    let text: String = b["text"]
        .as_str()
        .unwrap_or("")
        .trim()
        .chars()
        .take(MAX_TALK_CHARS)
        .collect();
    if text.is_empty() {
        return refused("text required");
    }
    let to = if let Some(c) = b["to"]["civ"].as_u64() {
        json!({"civ": c})
    } else if let Some(x) = b["to"]["member"].as_u64() {
        json!({"member": x})
    } else {
        Value::Null
    };
    // Kept here; the AI members answer at once.
    let id = g.push_talk(json!(m), to, json!(text));
    for r in g.ai_replies() {
        g.push_talk(r["member"].clone(), r["to"].clone(), r["text"].clone());
    }
    Response::ok(&json!({"ok": true, "id": id})).into()
}

/// Pre-season candidacy and votes (local mode).
fn lobby_choices(g: &mut Game, viewer: Viewer, req: &Request) -> Outcome {
    let Viewer::Member(m) = viewer else {
        return Response::error("403 Forbidden", "join first").into();
    };
    if g.phase != Phase::Lobby {
        return refused("the season has started; use /api/gov");
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
    Response::ok(&json!({"ok": true})).into()
}

fn start(g: &mut Game, viewer: Viewer) -> Outcome {
    if !matches!(viewer, Viewer::Member(_)) {
        return Response::error("403 Forbidden", "only members start the season").into();
    }
    match g.start() {
        Ok(()) => Response::ok(&json!({"ok": true, "tick": g.state.tick})).into(),
        Err(e) => refused(e),
    }
}

// ------------------------------------------------------------------ previews

/// Previews of a nation's options need the nation; paths and market quotes
/// are also answered for spectators (from the full state).
fn preview(g: &Game, civ: Option<CivId>, req: &Request, what: &str) -> Outcome {
    let s = &belief(g, civ);
    let r = &g.rules;
    let per_nation = |f: &dyn Fn(CivId) -> Value| match civ {
        Some(c) => Response::ok(&f(c)).into(),
        None => Response::error(
            "400 Bad Request",
            "previews are per nation: pass ?civ=N, ?member=M or a member token",
        )
        .into(),
    };
    let v = match what {
        "unit" => {
            return per_nation(&|c| api::unit_preview(s, r, c, req.qi("id").unwrap_or(u32::MAX)))
        }
        "city" => {
            return per_nation(&|c| api::city_preview(s, r, c, req.qi("id").unwrap_or(u32::MAX)))
        }
        "research" => return per_nation(&|c| api::research_preview(s, r, c)),
        "diplomacy" => {
            return per_nation(&|c| {
                api::diplomacy_preview(
                    s,
                    r,
                    c,
                    req.qi("with").or(req.qi("other")).unwrap_or(u16::MAX),
                )
            })
        }
        "path" => {
            let (Some(unit), Some(q), Some(rr)) = (
                req.qi::<u32>("unit"),
                req.qi::<i32>("q"),
                req.qi::<i32>("r"),
            ) else {
                return Response::text("400 Bad Request", "unit, q, r required").into();
            };
            let goal = Hex::new(q, rr);
            let path = permutation_rules::preview::path_to(s, r, unit, goal);
            let why = match (&path, s.units.get(unit as usize)) {
                (None, Some(u)) => Some(
                    match permutation_rules::checks::enter(s, r, u.owner.civ(), goal) {
                        Err(b) => api::blocked(b),
                        Ok(()) => json!({ "code": "Unreachable" }),
                    },
                ),
                _ => None,
            };
            json!({ "path": path.map(|p| p.iter().map(|h| [h.q, h.r]).collect::<Vec<_>>()), "blocked": why })
        }
        "amm" => amm_quote(s, g, req),
        _ => return Response::error("404 Not Found", "no such preview; see /llms.txt").into(),
    };
    Response::ok(&v).into()
}

/// What a gold-market order would fill at now.
fn amm_quote(s: &WorldState, g: &Game, req: &Request) -> Value {
    let pool = if req.q("good") == Some("Horses") {
        1
    } else {
        0
    };
    let side = if req.q("side") == Some("Sell") {
        Side::Sell
    } else {
        Side::Buy
    };
    let amount: u32 = req.qi("amount").unwrap_or(1);
    let p = s.pools[pool];
    let limit_gold = if side == Side::Buy { u32::MAX } else { 0 };
    let c = clear_amm(
        &g.rules,
        p.goods,
        p.gold,
        &[AmmOrder {
            side,
            qty: amount,
            limit_gold,
            budget_milli: i64::MAX,
        }],
    );
    let spot = p.gold as f64 / p.goods.max(1) as f64;
    let price = c.price_milli as f64 / 1000.0;
    json!({
        "price": price, "spot": spot,
        "impact": if spot > 0.0 { (price - spot) / spot } else { 0.0 },
        "gold": c.fills.first().and_then(|f| *f).map(|(g, _)| g as f64 / 1000.0),
        "fee": (c.hub_fee_milli + c.burned_milli) as f64 / 1000.0,
        "filled": c.fills.first().is_some_and(|f| f.is_some()),
    })
}

/// Dry run for any office (outside agents check before signing); with
/// `member`, of that member's drafts as its offices' batches.
fn validate(g: &Game, civ: Option<CivId>, req: &Request) -> Outcome {
    let body = req.json();
    if !body["member"].is_null() {
        return validate_member(g, &body);
    }
    let nation = match civ.or_else(|| body["civ"].as_u64().map(|c| c as CivId)) {
        Some(c) if (c as usize) < g.state.civs.len() => c,
        _ => return Response::error("400 Bad Request", "civ required").into(),
    };
    let orders: Vec<Order> = match parse_orders(&body["orders"]) {
        Ok(pairs) => pairs.into_iter().map(|(_, o)| o).collect(),
        Err(e) => return refused(e),
    };
    let warnings = api::preflight(&g.fog.belief(&g.state, nation), &g.rules, nation, &orders);
    let offices: Vec<Value> = Role::ALL
        .iter()
        .map(|role| {
            let mine: Vec<Order> = orders.iter().filter(|o| role_allows(&g.state, *role, o)).cloned().collect();
            let cost = check_structure(&g.rules, g.state.tick, &mine);
            let spendable = spendable(&g.state, &g.rules, nation, *role);
            let error = match &cost {
                Err(e) => Some(e.to_string()),
                Ok(c) if *c > spendable => Some(format!("orders cost {c}, only {spendable} spendable")),
                _ => None,
            };
            json!({"role": role_name(*role), "orders": mine.len(), "cost": cost.as_ref().ok(), "spendable": spendable,
                   "error": error, "holder": api::member_ref(g.state.nations[nation as usize].holder(*role))})
        })
        .collect();
    let ok = offices.iter().all(|o| o["error"].is_null());
    Response::ok(&json!({"ok": ok, "tick": g.state.tick, "offices": offices, "warnings": warnings}))
        .into()
}

/// `{member, orders, adopt: {Role: [ids]}}`: the member's drafts split by
/// the offices it holds, each checked as the engine will check its batch
/// (`validate_batch`, any digest); orders of offices it does not hold are
/// `refused` (they can be proposed instead). `ok`: every held office's
/// batch passes. The same for every member, AI or not.
fn validate_member(g: &Game, body: &Value) -> Outcome {
    let Some(m) = body["member"]
        .as_u64()
        .filter(|m| (*m as usize) < g.state.members.len())
        .map(|m| m as MemberId)
    else {
        return Response::error("400 Bad Request", "no such member").into();
    };
    let pairs = match parse_orders(&body["orders"]) {
        Ok(p) => p,
        Err(e) => return refused(e),
    };
    let adopt = parse_adopt(&body["adopt"]);
    let civ = g.state.members[m as usize].civ;
    let tick = g.state.tick;
    let (parts, refused_orders) = g.split_orders(m, &pairs);
    let offices: Vec<Value> = Role::ALL
        .into_iter()
        .filter(|r| g.state.nations[civ as usize].holder(*r) == m)
        .map(|role| {
            let part = &parts[role.index()];
            let ids = adopted(&adopt, role);
            let batch = OrderBatch {
                civ,
                tick,
                role,
                member: m,
                decision_digest: [1; 32],
                orders: part.iter().map(|(_, o)| o.clone()).collect(),
                adopt: ids.clone(),
            };
            let (cost, error) = match validate_batch(&g.state, &g.rules, &batch) {
                Ok(c) => (Some(c), None),
                Err(e) => (None, Some(e.to_string())),
            };
            json!({"role": role_name(role), "orders": part.iter().map(|(d, _)| d).collect::<Vec<_>>(),
                   "adopt": ids, "cost": cost, "spendable": spendable(&g.state, &g.rules, civ, role), "error": error})
        })
        .collect();
    let all: Vec<Order> = pairs.iter().map(|(_, o)| o.clone()).collect();
    let warnings = api::preflight(&g.fog.belief(&g.state, civ), &g.rules, civ, &all);
    let ok = offices.iter().all(|o| o["error"].is_null());
    let refused_orders: Vec<Value> = refused_orders
        .into_iter()
        .map(|(d, why)| json!({"order": d, "error": why}))
        .collect();
    Response::ok(&json!({"ok": ok, "tick": tick, "offices": offices, "warnings": warnings, "refused": refused_orders}))
        .into()
}

// ------------------------------------------------------------------ acting

/// A person commits its offices' orders for the open tick (local mode).
fn orders(g: &mut Game, viewer: Viewer, req: &Request) -> Outcome {
    let Viewer::Member(m) = viewer else {
        return Response::error(
            "403 Forbidden",
            "join a nation first; outside agents sign their own batches",
        )
        .into();
    };
    if g.phase != Phase::Playing || g.over() {
        return refused("the season is not running");
    }
    let body = req.json();
    let pairs = match parse_orders(&body["orders"]) {
        Ok(p) => p,
        Err(e) => return refused(e),
    };
    let parts = match g.split_for_member(m, &pairs) {
        Ok(p) => p,
        Err(e) => return refused(e),
    };
    let civ = g.members[m as usize].civ;
    let tick = g.state.tick;
    let text = body["rationale"].as_str().unwrap_or("").trim().to_string();
    let adopt = parse_adopt(&body["adopt"]);
    let held: Vec<Role> = Role::ALL
        .into_iter()
        .filter(|r| g.state.nations[civ as usize].holder(*r) == m)
        .collect();
    if held.is_empty() {
        return refused("you hold no office: propose orders instead (POST /api/gov Propose)");
    }
    // Validate every office's batch as the engine will, before keeping any.
    let mut results = Vec::new();
    for role in &held {
        let orders: Vec<Order> = parts[role.index()].iter().map(|(_, o)| o.clone()).collect();
        let ids = adopted(&adopt, *role);
        let batch = OrderBatch {
            civ,
            tick,
            role: *role,
            member: m,
            decision_digest: [1; 32],
            orders,
            adopt: ids.clone(),
        };
        match validate_batch(&g.state, &g.rules, &batch) {
            Ok(cost) => results.push((*role, cost, ids)),
            Err(e) => {
                return Response::ok(
                    &json!({"ok": false, "role": role_name(*role), "error": e.to_string()}),
                )
                .into()
            }
        }
    }
    let mut offices = Vec::new();
    for (role, cost, ids) in results {
        let digest = g
            .ledger
            .commit(tick, civ, role as u8, DEFAULT_POLICY, &text);
        offices.push(json!({"role": role_name(role), "digest": hex(&digest), "cost": cost}));
        let x = &mut g.members[m as usize];
        x.committed[role.index()] = parts[role.index()].iter().map(|(d, _)| d.clone()).collect();
        x.cost[role.index()] = cost;
        x.adopt[role.index()] = ids;
    }
    Response::ok(&json!({"ok": true, "tick": tick, "offices": offices})).into()
}

/// A member's governance action, queued for the next tick (local mode).
fn governance(g: &mut Game, viewer: Viewer, req: &Request) -> Outcome {
    let Viewer::Member(m) = viewer else {
        return Response::error("403 Forbidden", "join a nation first").into();
    };
    let dto: GovDto = match serde_json::from_value(req.json()["action"].clone()) {
        Ok(d) => d,
        Err(e) => return refused(e.to_string()),
    };
    let action = match dto.to_action() {
        Ok(a) => a,
        Err(e) => return refused(e),
    };
    if g.phase == Phase::Lobby {
        return refused("in the lobby, set your candidacy and votes with POST /api/lobby");
    }
    if matches!(action, GovAction::Vote { .. }) && !gov::vote_open(&g.rules, g.state.tick) {
        let next = gov::next_term_start(&g.rules, g.state.tick);
        return refused(format!(
            "votes open {} ticks before the next term ({next:?})",
            g.rules.vote_window
        ));
    }
    // One pending action of each kind per member; supports and proposals add up.
    let same_kind =
        |e: &GovEntry| std::mem::discriminant(&e.action) == std::mem::discriminant(&action);
    let replaces = !matches!(
        action,
        GovAction::Support { .. } | GovAction::Propose { .. }
    );
    g.pending_gov
        .retain(|e| !(e.member == m && same_kind(e) && replaces));
    g.pending_gov.push(GovEntry {
        member: m,
        signer: local_key(m),
        action,
    });
    let queued = g.pending_gov.iter().filter(|e| e.member == m).count();
    Response::ok(&json!({"ok": true, "tick": g.state.tick, "queued": queued})).into()
}

/// Pause, tick length, and ending the member's turn (local mode).
fn control(g: &mut Game, viewer: Viewer, req: &Request) -> Outcome {
    let Viewer::Member(m) = viewer else {
        return Response::error("403 Forbidden", "only members control the game").into();
    };
    let v = req.json();
    let tick = g.state.tick;
    let advance = v["advance"].as_bool() == Some(true);
    if let Some(p) = v["paused"].as_bool() {
        g.paused = p;
        if !p {
            g.deadline = Instant::now() + Duration::from_secs(g.tick_seconds);
        }
    }
    if let Some(t) = v["tickSeconds"].as_u64() {
        g.tick_seconds = t.clamp(5, 3600);
    }
    if advance {
        // End this member's turn; the tick resolves once every person here did.
        g.members[m as usize].ready = Some(tick);
        if g.all_humans_ready() {
            g.advance();
        }
    }
    Response::ok(&json!({"ok": true, "paused": g.paused, "tick": g.state.tick, "waitingFor": g.waiting_for()})).into()
}

// ------------------------------------------------------------------ decisions

fn decisions(g: &Game, viewer: Viewer, civ: Option<CivId>, req: &Request) -> Outcome {
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
            let mine = Some(r.civ) == own;
            json!({
                // Never who sealed a decision (which would tell this
                // server's AI members apart, V5 §18.2).
                "tick": r.tick, "civ": r.civ, "role": Role::from_index(r.role as usize).map(role_name),
                "obsRoot": hex(&r.obs_root), "digest": hex(&r.digest),
                // Only what has been revealed on the event chain is public.
                "policy": if revealed || mine { json!(r.policy) } else { Value::Null },
                "reveal": if revealed { json!({"at": r.revealed_at, "salt": hex(&r.salt), "text": r.text}) } else { Value::Null },
                "serverVerified": if revealed { json!(r.verified()) } else { Value::Null },
            })
        })
        .collect();
    Response::ok(&json!({ "open": open, "records": records })).into()
}

/// Inclusion proof of one leaf of an observation.
fn proof(g: &Game, civ: Option<CivId>, req: &Request) -> Outcome {
    let (Some(tick), Some(of), Some(kind), Some(id)) = (
        req.qi::<u16>("tick"),
        req.qi::<CivId>("of").or(req.qi("civ")),
        req.q("kind"),
        req.qi::<u32>("id"),
    ) else {
        return Response::text("400 Bad Request", "tick, civ, kind, id required").into();
    };
    if tick >= g.state.tick || (civ != Some(of) && kind != "tile" && kind != "header") {
        return refused("not available");
    }
    match g.ledger.proof(tick, of, kind, id) {
        Some((root, leaf, proof)) => Response::ok(&json!({
            "ok": true, "tick": tick, "civ": of, "root": hex(&root),
            "kind": leaf.kind, "id": leaf.id, "body": hex(&leaf.body),
            "proof": proof.iter().map(|p| json!([hex(&p.sibling), p.left])).collect::<Vec<_>>(),
        }))
        .into(),
        None => refused("no such leaf (older observations keep only their root)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chainlink::WorldMeta;
    use crate::driver::seat_ai_members;
    use crate::play::chain::ChainMode;
    use crate::play::game::unix_now;
    use crate::play::http::SECURITY_HEADERS;
    use crate::play::proxy::{fake::fake_gateway, GatewayProxy};
    use crate::play::Shared;
    use permutation_rules::genesis::{nation_entries, new_season};
    use permutation_rules::{Preset, Ruleset};
    use std::collections::BTreeSet;
    use std::io::{Read, Write};
    use std::path::PathBuf;
    use std::sync::Mutex;

    fn web() -> PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web")
    }

    fn shared(g: Game) -> Shared {
        Arc::new(Mutex::new(g))
    }

    /// A local season.
    fn server() -> Site {
        Site::with_game(web(), shared(Game::new(30, 1)))
    }

    fn answer(site: &Site, raw: &str) -> (String, Value) {
        let req = Request::read(raw.as_bytes()).unwrap();
        let res = respond(&req, site);
        (
            res.status.to_string(),
            serde_json::from_slice(&res.body).unwrap_or(Value::Null),
        )
    }

    fn call(
        site: &Site,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Value,
    ) -> (String, Value) {
        let body = body.to_string();
        let auth = token.map_or(String::new(), |t| format!("X-Member-Token: {t}\r\n"));
        answer(
            site,
            &format!(
                "{method} {path} HTTP/1.1\r\n{auth}Content-Length: {}\r\n\r\n{body}",
                body.len()
            ),
        )
    }

    /// The gateway's `/season` of `chain_game`.
    fn season_info() -> Value {
        json!({
            "season": {"seasonId": "7", "entryFee": "10000000", "pool": "0", "aiCount": 6},
            "programId": "PermutationProgram111", "cluster": "localnet",
            "accounts": {"season": "SeasonPda111", "vault": "VaultPda111"},
        })
    }

    /// A chain season on tick 0 with two members per nation, each elected
    /// to two offices (`seat_ai_members`: the first General and Steward,
    /// the second Science and Diplomat). Even members are the operator's AI
    /// members, odd ones sign their own transactions; member 1 declared
    /// itself human and member 3 is attested. No gateway is ever called.
    fn chain_game() -> Game {
        let rules = Ruleset::new(Preset::Blitz);
        let mut state = new_season(
            &rules,
            b"permutation-state/world/test-001",
            b"permutation-state/season/test-01",
            &nation_entries(6),
        )
        .unwrap();
        seat_ai_members(&mut state, &rules, &[2; 6]).unwrap();
        let mut info = season_info();
        info["members"] = json!(state
            .members
            .iter()
            .enumerate()
            .map(
                |(i, m)| json!({"index": i, "civ": m.civ, "name": format!("M{i}"),
                "kind": if i == 1 { "human" } else { "undeclared" }, "attested": i == 3})
            )
            .collect::<Vec<_>>());
        let chain = ChainMode {
            link: Arc::new(ChainLink::new("http://127.0.0.1:9").unwrap()),
            gateway: "/gw".into(),
            meta: WorldMeta {
                season_id: 7,
                tick_seconds: 30,
                deadline: unix_now() + 20,
                ..Default::default()
            },
            slot: 1,
            layer: "er".into(),
            phase: "playing".into(),
            info: info.clone(),
            last_record: None,
            ai_submitted: None,
        };
        let mut g = Game::assemble(rules, state, b"test", 30, Some(chain));
        g.sync_registry(&info);
        let n = g.state.members.len();
        let members: Vec<Value> = (0..n)
            .map(|i| json!({"member": i, "civ": g.state.members[i].civ, "hosted": if i % 2 == 0 { "ai" } else { "external" }}))
            .collect();
        let ai: Vec<Value> = (0..n)
            .step_by(2)
            .map(|i| json!({"member": i, "civ": g.state.members[i].civ, "salt": hex(&[i as u8; 32])}))
            .collect();
        g.sync_operator(&json!({"members": members, "ai": ai, "bountyEach": "5000000"}));
        g.phase = Phase::Playing;
        g
    }

    fn chain_site() -> Site {
        let site = Site::new(
            web(),
            Some(GatewayProxy::new("http://127.0.0.1:9").unwrap()),
            Some("http://127.0.0.1:4191"),
        );
        site.install(shared(chain_game()));
        site
    }

    fn keys(v: &Value) -> BTreeSet<String> {
        v.as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default()
    }

    #[test]
    fn a_person_joins_starts_orders_and_ends_the_turn() {
        let game = server();
        let (status, j) = call(
            &game,
            "POST",
            "/api/join",
            None,
            json!({"civ": 0, "name": "Ada", "stand": ["General", "Steward", "Science", "Diplomat"]}),
        );
        assert_eq!(status, "200 OK", "{j}");
        let token = j["token"].as_str().unwrap().to_string();
        let t = Some(token.as_str());
        let (_, lobby) = call(&game, "GET", "/api/lobby", t, json!({}));
        assert_eq!(lobby["you"]["name"], "Ada");
        assert_eq!(
            call(&game, "POST", "/api/start", t, json!({})).1["ok"],
            true
        );
        let (_, state) = call(&game, "GET", "/api/state", t, json!({}));
        assert_eq!(state["phase"], "playing");
        assert_eq!(state["member"]["host"], "human");
        assert_eq!(state["waiting"], 0);
        let (_, o) = call(
            &game,
            "POST",
            "/api/orders",
            t,
            json!({"orders": [], "rationale": "wait and see"}),
        );
        assert_eq!(o["ok"], true, "{o}");
        let (_, c) = call(&game, "POST", "/api/control", t, json!({"advance": true}));
        assert_eq!(c["tick"], 1, "the only person here ended the turn: {c}");
        // Only the header carries a token (no query, no seat header).
        let (_, v) = call(
            &game,
            "GET",
            &format!("/api/state?token={token}"),
            None,
            json!({}),
        );
        assert_eq!(v["viewer"], "spectator");
        let (_, v) = answer(
            &game,
            &format!("GET /api/state HTTP/1.1\r\nX-Seat-Token: {token}\r\n\r\n"),
        );
        assert_eq!(v["viewer"], "spectator");
    }

    #[test]
    fn refusals_have_the_right_status() {
        let game = server();
        assert_eq!(
            call(&game, "POST", "/api/orders", None, json!({})).0,
            "403 Forbidden"
        );
        assert_eq!(
            call(&game, "POST", "/api/join", None, json!({"civ": 99})).0,
            "400 Bad Request"
        );
        assert_eq!(
            call(&game, "GET", "/api/preview/unit", None, json!({})).0,
            "400 Bad Request"
        );
        assert_eq!(
            call(&game, "GET", "/api/preview/nothing?civ=0", None, json!({})).0,
            "404 Not Found"
        );
        assert_eq!(
            call(&game, "GET", "/api/nothing", None, json!({})).0,
            "404 Not Found"
        );
        // No seats are handed out any more: every member holds its own keys.
        for (method, path) in [("POST", "/api/claim"), ("GET", "/api/seats")] {
            assert_eq!(
                call(&game, method, path, None, json!({"civ": 0})).0,
                "404 Not Found"
            );
        }
        // Without --gateway-proxy there is no /gw.
        assert_eq!(
            call(&game, "GET", "/gw/season", None, json!({})).0,
            "404 Not Found"
        );
    }

    #[test]
    fn members_never_say_who_hosts_them_and_ai_answer_talk() {
        let game = server();
        let (_, lobby) = call(&game, "GET", "/api/lobby", None, json!({}));
        for m in lobby["members"].as_array().unwrap() {
            assert!(
                m.get("host").is_none() && m.get("claimable").is_none(),
                "{m}"
            );
            assert!(m.get("ready").is_none(), "{m}");
            assert_ne!(m["kind"], "agent");
            assert!(!m["name"].as_str().unwrap().contains("AI"), "{m}");
        }
        assert!(lobby["nations"][0].get("seats").is_none());
        assert_eq!(
            lobby["roster"]["aiCount"],
            lobby["members"].as_array().unwrap().len()
        );
        assert_eq!(lobby["roster"]["fallen"], json!([]));
        let (_, join) = call(
            &game,
            "POST",
            "/api/join",
            None,
            json!({"civ": 1, "name": "Hypatia"}),
        );
        let token = join["token"].as_str().unwrap().to_string();
        let (_, sent) = call(
            &game,
            "POST",
            "/api/talk",
            Some(&token),
            json!({"to": {"civ": 0}, "text": "Peace?"}),
        );
        assert_eq!(sent["ok"], true);
        let (_, talk) = call(&game, "GET", "/api/talk", None, json!({}));
        let msgs = talk["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["text"], "Peace?");
        // Nation 0's AI answers (unless its temperament keeps it silent).
        assert!(msgs.len() <= 2);
    }

    #[test]
    fn validate_reports_per_office_costs() {
        let game = server();
        let (_, v) = call(
            &game,
            "POST",
            "/api/validate",
            None,
            json!({"civ": 1, "orders": []}),
        );
        assert_eq!(v["ok"], true);
        assert_eq!(v["offices"].as_array().unwrap().len(), 4);
        assert!(v.get("refused").is_none());
    }

    /// A handler that panics while holding the lock must not take the whole
    /// server down with it.
    #[test]
    fn a_poisoned_lock_still_serves() {
        let site = server();
        let g2 = site.game().unwrap().clone();
        let _ = std::thread::spawn(move || {
            let _g = g2.lock().unwrap();
            panic!("a handler panicked");
        })
        .join();
        assert!(site.game().unwrap().is_poisoned());
        assert_eq!(
            call(&site, "GET", "/api/lobby", None, json!({})).0,
            "200 OK"
        );
    }

    /// Chain mode listens before the world exists: the lobby says so, the
    /// rest of the API waits, the page is served; then the game takes over
    /// without a restart.
    #[test]
    fn before_the_government_opens_only_the_lobby_answers() {
        let site = Site::new(
            web(),
            Some(GatewayProxy::new("http://127.0.0.1:9").unwrap()),
            Some("http://127.0.0.1:4191/"),
        );
        let lobby = |site: &Site| call(site, "GET", "/api/lobby", None, json!({}));
        assert_eq!(
            lobby(&site),
            (
                "200 OK".to_string(),
                json!({"phase": "registering", "mode": "chain", "gateway": "/gw", "entryFee": null, "chain": null})
            )
        );
        let waits = |site: &Site| {
            for (method, path) in [
                ("GET", "/api/state"),
                ("GET", "/api/map"),
                ("GET", "/api/decisions"),
                ("GET", "/api/talk"),
                ("POST", "/api/validate"),
                ("POST", "/api/orders"),
                ("POST", "/api/lobby"),
            ] {
                assert_eq!(
                    call(site, method, path, None, json!({})),
                    (
                        "503 Service Unavailable".to_string(),
                        json!({"ok": false, "error": "registering"})
                    ),
                    "{method} {path}"
                );
            }
        };
        waits(&site);
        let page = respond(
            &Request::read(&b"GET / HTTP/1.1\r\n\r\n"[..]).unwrap(),
            &site,
        );
        assert_eq!(page.status, "200 OK");
        assert!(page.ctype.starts_with("text/html"));
        // What the gateway says of the season: what the browser checks the
        // payment request against.
        site.set_season(season_info());
        let (_, l) = lobby(&site);
        assert_eq!(l["entryFee"], 10_000_000);
        assert_eq!(
            l["chain"],
            json!({"seasonId": "7", "programId": "PermutationProgram111", "cluster": "localnet",
                   "accounts": {"season": "SeasonPda111", "vault": "VaultPda111"}})
        );
        // The world exists; the government has not opened yet.
        let mut g = chain_game();
        g.phase = Phase::Lobby;
        site.install(shared(g));
        let (_, l) = lobby(&site);
        assert_eq!(
            (l["phase"].as_str(), l["gateway"].as_str()),
            (Some("starting"), Some("/gw"))
        );
        assert_eq!(l["chain"]["programId"], "PermutationProgram111");
        waits(&site);
        // The government opened: the whole game.
        lock(site.game().unwrap()).phase = Phase::Playing;
        let (_, l) = lobby(&site);
        assert_eq!(
            (l["phase"].as_str(), l["mode"].as_str()),
            (Some("playing"), Some("chain"))
        );
        assert_eq!(l["gateway"], "/gw");
        assert_eq!(l["chain"]["gateway"], "/gw");
        assert_eq!(l["you"], Value::Null);
        let (status, state) = call(&site, "GET", "/api/state", None, json!({}));
        assert_eq!(
            (status.as_str(), state["phase"].as_str()),
            ("200 OK", Some("playing"))
        );
        assert_eq!(state["chain"]["gateway"], "/gw");
    }

    /// Every route that would act for a member is refused, word for word
    /// the same whoever asks and however: nothing tells an operator AI
    /// member apart (V5 §18.2).
    #[test]
    fn chain_mode_refuses_every_action_the_same_way() {
        let site = chain_site();
        let n = lock(site.game().unwrap()).state.members.len();
        for (method, path) in ACTING {
            let mut answers = BTreeSet::new();
            let bodies = [
                json!({}),
                json!({"orders": [], "rationale": "x"}),
                json!({"action": {"type": "Stand", "roles": ["General"]}}),
                json!({"advance": true}),
                json!({"text": "hello", "to": {"civ": 1}}),
            ];
            for m in 0..n {
                for body in &bodies {
                    for token in [None, Some("0123456789abcdef")] {
                        let (status, v) = call(
                            &site,
                            method,
                            &format!("{path}?member={m}"),
                            token,
                            body.clone(),
                        );
                        answers.insert((status, v.to_string()));
                    }
                }
            }
            let (status, v) = call(&site, method, path, None, json!({}));
            answers.insert((status, v.to_string()));
            assert_eq!(
                answers.into_iter().collect::<Vec<_>>(),
                vec![(
                    "400 Bad Request".to_string(),
                    json!({"ok": false, "error": CHAIN_REFUSAL}).to_string()
                )],
                "{method} {path}"
            );
        }
        let (status, v) = call(&site, "POST", "/api/start", None, json!({}));
        assert_eq!(
            (status.as_str(), &v["ok"]),
            ("400 Bad Request", &json!(false))
        );
        assert_eq!(
            call(&site, "POST", "/api/claim", None, json!({"civ": 0})).0,
            "404 Not Found"
        );
        // No member tokens: a token in the header, the query or the old seat
        // header is just a spectator.
        for raw in [
            "GET /api/state HTTP/1.1\r\nX-Member-Token: 0123456789abcdef\r\n\r\n",
            "GET /api/state?token=0123456789abcdef HTTP/1.1\r\n\r\n",
            "GET /api/state HTTP/1.1\r\nX-Seat-Token: 0123456789abcdef\r\n\r\n",
        ] {
            assert_eq!(answer(&site, raw).1["viewer"], "spectator");
        }
    }

    /// `?member=M`: that member's public block, with the same fields for an
    /// operator AI member as for anyone else, and nothing only a server
    /// holding its token would know.
    #[test]
    fn a_members_public_block_is_the_same_for_every_member() {
        let site = chain_site();
        let mut shapes = BTreeSet::new();
        for m in 0..4 {
            let (_, v) = call(
                &site,
                "GET",
                &format!("/api/state?member={m}"),
                None,
                json!({}),
            );
            assert_eq!(v["viewer"], "watch");
            let b = &v["member"];
            assert_eq!(b["id"], m);
            for private in ["committed", "ready", "host", "digest", "drafts"] {
                assert!(b.get(private).is_none(), "{private}: {b}");
            }
            assert!(v.get("waiting").is_none());
            for k in [
                "merit",
                "offices",
                "projectedPayout",
                "standingFor",
                "activeWindows",
                "civ",
                "name",
            ] {
                assert!(b.get(k).is_some(), "{k}: {b}");
            }
            assert_eq!(
                (b["kind"].as_str(), &b["attested"]),
                (Some("undeclared"), &json!(false))
            );
            assert_eq!(b["offices"].as_array().unwrap().len(), 2);
            shapes.insert(keys(b));
            // No public list says who ended a turn, who hosts whom, or (with
            // AI members this season) anyone's declared kind.
            for x in v["members"].as_array().unwrap() {
                assert_eq!(keys(x), ["civ", "id", "name"].map(String::from).into());
            }
            for o in v["gov"]["offices"].as_array().unwrap() {
                assert_eq!(o["holder"]["kind"], "undeclared");
                assert_eq!(o["holder"]["attested"], false);
            }
        }
        assert_eq!(shapes.len(), 1, "{shapes:?}");
        // The same block in local mode.
        let local = server();
        let (_, v) = call(&local, "GET", "/api/state?member=0", None, json!({}));
        assert_eq!(v["viewer"], "watch");
        assert_eq!(Some(keys(&v["member"])), shapes.into_iter().next());
        // Decisions never say whose they are.
        let (_, d) = call(&site, "GET", "/api/decisions", None, json!({}));
        assert!(d["records"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r.get("external").is_none()));
    }

    /// `/api/validate` with `member`: the drafts split by the offices it
    /// holds and checked as its batches, orders of other offices refused;
    /// the same for an AI member as for anyone else.
    #[test]
    fn validate_checks_a_members_drafts_as_its_batches() {
        let site = chain_site();
        let (city, unit) = {
            let g = lock(site.game().unwrap());
            let city = g
                .state
                .cities
                .iter()
                .find(|c| c.owner == Some(0))
                .unwrap()
                .id;
            (city, g.state.units.len() as u32)
        };
        let orders = json!([
            {"type": "SetResearch", "techs": ["Writing"]},
            {"type": "Purchase", "city": city, "gold": 1},
        ]);
        // Member 1 (signs its own batches) holds Science and Diplomat.
        let (status, v) = call(
            &site,
            "POST",
            "/api/validate",
            None,
            json!({"member": 1, "orders": orders, "adopt": {"Science": [], "General": [7]}}),
        );
        assert_eq!(status, "200 OK");
        assert_eq!(v["ok"], true, "{v}");
        let roles: Vec<&str> = v["offices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["role"].as_str().unwrap())
            .collect();
        assert_eq!(roles, ["Science", "Diplomat"]);
        let science = &v["offices"][0];
        assert_eq!(
            science["orders"],
            json!([{"type": "SetResearch", "techs": ["Writing"]}])
        );
        assert!(
            science["cost"].is_u64() && science["spendable"].is_u64() && science["error"].is_null()
        );
        assert_eq!(v["offices"][1]["orders"], json!([]));
        let refused = v["refused"].as_array().unwrap();
        assert_eq!(refused.len(), 1);
        assert_eq!(refused[0]["order"]["type"], "Purchase");
        assert!(
            refused[0]["error"]
                .as_str()
                .unwrap()
                .contains("propose it instead"),
            "{v}"
        );
        assert!(v["warnings"].is_array() && v["tick"] == 0);
        // Member 0, an operator AI member, holds General and Steward: the
        // same answer, for its offices.
        let (_, a) = call(
            &site,
            "POST",
            "/api/validate",
            None,
            json!({"member": 0, "orders": orders, "adopt": {"Science": [], "General": [7]}}),
        );
        assert_eq!(keys(&a), keys(&v));
        assert_eq!(keys(&a["offices"][0]), keys(&v["offices"][0]));
        assert_eq!(a["refused"][0]["order"]["type"], "SetResearch");
        // Adopting a proposal that does not exist fails that office's batch.
        let general = &a["offices"][0];
        assert_eq!(
            (general["role"].as_str(), &general["adopt"]),
            (Some("General"), &json!([7]))
        );
        assert!(
            general["error"].is_string() && general["cost"].is_null(),
            "{a}"
        );
        assert_eq!(a["ok"], false);
        // A batch over the office's budget fails as the engine would fail
        // it (an order that would only be skipped is a warning).
        let (_, bad) = call(
            &site,
            "POST",
            "/api/validate",
            None,
            json!({"member": 0, "orders": [
                {"type": "MoveUnit", "unit": unit, "path": [[0, 0]]},
                {"type": "MoveUnit", "unit": unit + 1, "path": [[0, 0]]},
            ]}),
        );
        assert_eq!(bad["ok"], false, "{bad}");
        assert!(bad["offices"][0]["error"].is_string(), "{bad}");
        assert_eq!(bad["warnings"][0]["blocked"]["code"], "UnknownUnit");
        assert_eq!(
            call(
                &site,
                "POST",
                "/api/validate",
                None,
                json!({"member": 999, "orders": []})
            )
            .0,
            "400 Bad Request"
        );
    }

    /// An open `Capture` offer (no `to`) is echoed by `/api/validate` exactly
    /// as it was sent, under the office and in `refused` alike: the browser
    /// matches the echo against its drafts and sends nothing on a mismatch.
    #[test]
    fn validate_echoes_an_open_offer_as_sent() {
        let site = chain_site();
        let open = json!({"type": "OfferContract", "term": {"kind": "Capture", "city": 11}, "usdc": 3000000, "deadline": 90});
        // Member 1 holds Diplomat: the offer is its Diplomat batch's.
        let (status, v) = call(
            &site,
            "POST",
            "/api/validate",
            None,
            json!({"member": 1, "orders": [open]}),
        );
        assert_eq!(status, "200 OK");
        let diplomat = v["offices"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["role"] == "Diplomat")
            .unwrap_or_else(|| panic!("{v}"));
        assert_eq!(diplomat["orders"], json!([open]), "{v}");
        assert!(diplomat["orders"][0].get("to").is_none(), "{v}");
        // Member 0 does not: the offer is refused, still echoed as sent.
        let (_, a) = call(
            &site,
            "POST",
            "/api/validate",
            None,
            json!({"member": 0, "orders": [open]}),
        );
        assert_eq!(a["refused"][0]["order"], open, "{a}");
        // An addressed offer keeps its `to`.
        let to = json!({"type": "OfferContract", "to": 3, "term": {"kind": "Peace"}, "usdc": 4000000, "deadline": 60});
        let (_, b) = call(
            &site,
            "POST",
            "/api/validate",
            None,
            json!({"member": 0, "orders": [to]}),
        );
        assert_eq!(b["refused"][0]["order"], to, "{b}");
    }

    /// `/api/state` for a member or a nation's watcher: the AI roster under
    /// `aiRoster` (the same object as `/api/lobby`'s and `/api/roster`'s
    /// `roster`), and the nation's members under `roster`. A spectator,
    /// who has no nation, gets the AI roster under both.
    #[test]
    fn the_state_carries_the_ai_roster_and_the_nations_members() {
        let is_ai_roster = |r: &Value| {
            r.is_object()
                && r["aiCount"].is_u64()
                && r["bountyEach"].is_string()
                && r["revealed"].is_boolean()
                && r["fallen"].is_array()
                && r["homeTick"].is_u64()
        };
        let site = chain_site();
        let (_, lobby) = call(&site, "GET", "/api/lobby", None, json!({}));
        assert!(is_ai_roster(&lobby["roster"]), "{}", lobby["roster"]);
        let (_, roster) = call(&site, "GET", "/api/roster", None, json!({}));
        for m in 0..4u64 {
            let (_, v) = call(
                &site,
                "GET",
                &format!("/api/state?member={m}"),
                None,
                json!({}),
            );
            let ai = &v["aiRoster"];
            assert!(is_ai_roster(ai), "{ai}");
            assert_eq!(ai, &lobby["roster"]);
            assert_eq!(ai["aiCount"], roster["aiCount"]);
            assert_eq!(ai["aiCount"], 6);
            assert_eq!(ai["bountyEach"], "5000000");
            let civ = v["member"]["civ"].as_u64().unwrap();
            let list = v["roster"].as_array().unwrap_or_else(|| panic!("{v}"));
            let g = lock(site.game().unwrap());
            let ids: Vec<u64> = (0..g.state.members.len() as u64)
                .filter(|i| g.state.members[*i as usize].civ as u64 == civ)
                .collect();
            assert_eq!(
                list.iter()
                    .map(|x| x["id"].as_u64().unwrap())
                    .collect::<Vec<_>>(),
                ids
            );
            assert!(list.iter().any(|x| x["id"] == m));
            assert!(list
                .iter()
                .all(|x| x["merit"].is_number() && x["activeWindows"].is_number()));
        }
        // A nation's watcher without `member`: the same two keys.
        let (_, w) = call(&site, "GET", "/api/state?civ=2", None, json!({}));
        assert!(
            is_ai_roster(&w["aiRoster"]) && w["roster"].is_array(),
            "{w}"
        );
        // A spectator: no nation, so `roster` stays the AI roster.
        let (_, s) = call(&site, "GET", "/api/state", None, json!({}));
        assert!(is_ai_roster(&s["aiRoster"]), "{s}");
        assert_eq!(s["roster"], s["aiRoster"]);
        // Local mode, a person holding its member token.
        let local = server();
        let (_, j) = call(
            &local,
            "POST",
            "/api/join",
            None,
            json!({"civ": 1, "name": "Ada"}),
        );
        let token = j["token"].as_str().unwrap_or_else(|| panic!("{j}"));
        let (_, v) = call(&local, "GET", "/api/state", Some(token), json!({}));
        assert_eq!(v["viewer"], "member");
        assert!(is_ai_roster(&v["aiRoster"]), "{v}");
        assert_eq!(v["aiRoster"]["aiCount"], 6);
        let list = v["roster"].as_array().unwrap();
        assert!(list.iter().any(|x| x["id"] == j["member"]), "{v}");
        assert!(list.len() >= 2, "the AI member and the person: {v}");
    }

    /// The public view carries the open tick's sealed-orders phase; the
    /// countdown to commit stops when the commitments close.
    #[test]
    fn the_view_shows_the_tick_phase() {
        let site = chain_site();
        let state = || call(&site, "GET", "/api/state", None, json!({})).1;
        let set = |revealing: bool, frozen: bool, left: i64| {
            let mut g = lock(site.game().unwrap());
            let c = g.chain.as_mut().unwrap();
            c.meta.revealing = revealing;
            c.meta.frozen = frozen;
            c.meta.deadline = unix_now() + left;
        };
        let v = state();
        assert_eq!(v["chainPhase"], "commit");
        let (a, b) = (
            v["secondsLeft"].as_f64().unwrap(),
            v["phaseSecondsLeft"].as_f64().unwrap(),
        );
        assert!((18.0..=20.0).contains(&a) && a == b, "{a} {b}");
        set(true, false, 4);
        let v = state();
        assert_eq!(v["chainPhase"], "reveal");
        assert_eq!(v["secondsLeft"], 0.0);
        assert!((3.0..=4.0).contains(&v["phaseSecondsLeft"].as_f64().unwrap()));
        set(true, true, 4);
        let v = state();
        assert_eq!(
            (v["chainPhase"].as_str(), &v["phaseSecondsLeft"]),
            (Some("frozen"), &json!(0.0))
        );
        let local = call(&server(), "GET", "/api/state", None, json!({})).1;
        assert!(local["chainPhase"].is_null());
        assert_eq!(local["phaseSecondsLeft"], local["secondsLeft"]);
    }

    /// `/gw/*` goes to the gateway's public listener even before the world
    /// exists, without this server's or the browser's credentials.
    #[test]
    fn the_gateway_answers_under_gw_without_credentials() {
        let (url, got) = fake_gateway(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"ok\":true}",
        );
        let site = Site::new(
            web(),
            Some(GatewayProxy::new(&url).unwrap()),
            Some("http://127.0.0.1:4191"),
        );
        let (status, v) = answer(
            &site,
            "GET /gw/usdc?owner=abc HTTP/1.1\r\nAuthorization: Bearer operator\r\nX-Member-Token: m\r\n\r\n",
        );
        assert_eq!((status.as_str(), v), ("200 OK", json!({"ok": true})));
        let sent = got.recv().unwrap().to_ascii_lowercase();
        assert!(
            sent.starts_with("get /usdc?owner=abc http/1.1\r\n"),
            "{sent}"
        );
        assert!(
            !sent.contains("authorization") && !sent.contains("x-member-token"),
            "{sent}"
        );
    }

    /// The security headers go out on the wire with every kind of answer.
    #[test]
    fn every_answer_carries_the_security_headers() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let site = Arc::new(server());
        let paths = ["/", "/api/lobby", "/api/nothing", "/no-such-file"];
        let s = site.clone();
        let server = std::thread::spawn(move || {
            for _ in 0..paths.len() {
                let (stream, _) = listener.accept().unwrap();
                handle(stream, &s);
            }
        });
        for path in paths {
            let mut c = TcpStream::connect(addr).unwrap();
            c.write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes())
                .unwrap();
            let mut raw = String::new();
            c.read_to_string(&mut raw).unwrap();
            let head = &raw[..raw.find("\r\n\r\n").unwrap()];
            for (k, v) in SECURITY_HEADERS {
                assert!(head.contains(&format!("\r\n{k}: {v}")), "{path}: {head}");
            }
        }
        server.join().unwrap();
    }
}
