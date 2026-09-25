//! The JSON API. Each route runs under the game lock and returns what to
//! answer; a route that must call the gateway returns an `Outcome` that is
//! carried out after the lock is released. See `llms.txt` for the API as
//! agents read it.

use permutation_rules::gov::{self, GovAction, GovEntry, MemberId, Role, NOBODY};
use permutation_rules::hex::Hex;
use permutation_rules::markets::{clear_amm, AmmOrder};
use permutation_rules::orders::{
    check_structure, role_allows, validate_batch, Order, OrderBatch, Side,
};
use permutation_rules::state::{CivId, WorldState};
use serde_json::{json, Value};
use std::net::TcpStream;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::chain::Submission;
use super::game::{role_name, Game, Host, Phase, Viewer};
use super::http::{Request, Response, IO_TIMEOUT};
use super::{lock, Shared};
use crate::api::{self, GovDto, MemberMeta, OrderDto};
use crate::chainlink::ChainLink;
use crate::codec::hex;
use crate::driver::local_key;

/// Longest member name kept.
const MAX_NAME: usize = 24;
/// Largest simulated treasury deposit in the lobby (100 test USDC).
const MAX_LOCAL_DEPOSIT: u64 = 100_000_000;

/// Answer one connection.
pub fn handle(mut stream: TcpStream, game: Shared, web: &Path) {
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let Some(req) = Request::read(&stream) else {
        return;
    };
    respond(&req, &game, web).write(&mut stream);
}

pub fn respond(req: &Request, game: &Shared, web: &Path) -> Response {
    if req.method == "OPTIONS" {
        return Response::text("204 No Content", "");
    }
    if !req.path.starts_with("/api/") {
        return Response::static_file(web, &req.path);
    }
    let outcome = route(&mut lock(game), req);
    outcome.finish(game)
}

/// What a route decided.
pub enum Outcome {
    Reply(Response),
    /// Send a person's batches to the chain, then answer `/api/orders`.
    Orders {
        submission: Submission,
        tick: u16,
        offices: Vec<Value>,
    },
    /// Send a person's batches to end its turn (`/api/control` advance).
    EndTurn {
        submission: Submission,
        tick: u16,
    },
    /// Send a governance action to the chain.
    Gov {
        link: Arc<ChainLink>,
        body: Value,
    },
    /// Relay a person's message through the gateway (V5 §18.7).
    Talk {
        link: Arc<ChainLink>,
        body: Value,
    },
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
    /// Carry out the chain part, without the lock.
    fn finish(self, game: &Shared) -> Response {
        match self {
            Outcome::Reply(r) => r,
            Outcome::Orders {
                submission,
                tick,
                offices,
            } => match submission.send(game) {
                Ok(sigs) => Response::ok(
                    &json!({"ok": true, "tick": tick, "offices": offices, "signatures": sigs}),
                ),
                Err(e) => Response::ok(&json!({"ok": false, "error": e})),
            },
            Outcome::EndTurn { submission, tick } => {
                let error = submission.send(game).err();
                Response::ok(&json!({"ok": error.is_none(), "tick": tick, "error": error}))
            }
            Outcome::Gov { link, body } => match link.post_json("/gov", &body) {
                Ok(res) => Response::ok(&json!({"ok": true, "signature": res["signature"]})),
                Err(e) => Response::ok(&json!({"ok": false, "error": format!("chain: {e}")})),
            },
            Outcome::Fetch { link, path } => match link.get_json(path) {
                Ok(v) => Response::ok(&v),
                Err(e) => Response::ok(&json!({"ok": false, "error": format!("gateway: {e}")})),
            },
            Outcome::Talk { link, body } => match link.post_json("/talk", &body) {
                Ok(res) => Response::ok(&json!({"ok": true, "id": res["id"], "tick": res["tick"]})),
                Err(e) => Response::ok(&json!({"ok": false, "error": format!("gateway: {e}")})),
            },
        }
    }
}

/// `{"ok": false, "error": …}` with 200: the request was understood, the game said no.
fn refused(error: impl Into<String>) -> Outcome {
    Response::ok(&json!({"ok": false, "error": error.into()})).into()
}

fn route(g: &mut Game, req: &Request) -> Outcome {
    let viewer = g.viewer(req);
    if let Viewer::Member(m) = viewer {
        g.members[m as usize].last_seen = Some(Instant::now());
    }
    let civ = g.civ_of(viewer);
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/api/map") => {
            let mut v = api::map_view(&belief(g, civ));
            v["civNames"] = json!(g.names());
            Response::ok(&v).into()
        }
        ("GET", "/api/state") => Response::ok(&g.state_json(viewer)).into(),
        ("GET", "/api/lobby") | ("GET", "/api/seats") => Response::ok(&g.lobby_json(viewer)).into(),
        ("POST", "/api/join") => join(g, req),
        ("POST", "/api/claim") => claim(g, req),
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
fn belief(g: &Game, civ: Option<CivId>) -> WorldState {
    match civ {
        Some(c) => g.fog.belief(&g.state, c),
        None => g.state.clone(),
    }
}

fn parse_orders(v: &Value) -> Result<Vec<(OrderDto, Order)>, String> {
    let dtos: Vec<OrderDto> = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    dtos.into_iter()
        .map(|d| d.to_order().map(|o| (d, o)))
        .collect()
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

/// Chain mode: take over a seat for a person the gateway registered, in
/// nation `civ`. Seats are given by nation, never picked by member: which
/// members are seats for people must not be learnt by trying (V5 §18.2).
fn claim(g: &mut Game, req: &Request) -> Outcome {
    let Some(civ) = req.json()["civ"].as_u64() else {
        return Response::error("400 Bad Request", "civ (nation) required").into();
    };
    let Some(m) = (0..g.members.len()).find(|i| {
        g.members[*i].civ as u64 == civ
            && g.members[*i].host == Host::Human
            && !g.members[*i].here()
    }) else {
        return Response::error("409 Conflict", "no free seat in this nation").into();
    };
    let token = g.token(m as u64);
    let x = &mut g.members[m];
    x.token = Some(token.clone());
    x.last_seen = Some(Instant::now());
    Response::ok(&json!({"ok": true, "member": m, "civ": x.civ, "token": token})).into()
}

/// A message from the member this browser holds (V5 §18.7): public,
/// relayed and signed by the gateway in chain mode.
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
        .take(280)
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
    if let Some(link) = g.chain.as_ref().map(|c| c.link.clone()) {
        return Outcome::Talk {
            link,
            body: json!({"member": m, "to": to, "text": text}),
        };
    }
    // Local mode: kept here; the AI members answer at once.
    let id = g.talk_next;
    g.talk
        .push(json!({"id": id, "tick": g.state.tick, "member": m, "to": to, "text": text}));
    g.talk_next += 1;
    for r in g.ai_replies() {
        let id = g.talk_next;
        g.talk.push(json!({"id": id, "tick": g.state.tick, "member": r["member"], "to": r["to"], "text": r["text"]}));
        g.talk_next += 1;
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

/// Dry run for any office (outside agents check before signing).
fn validate(g: &Game, civ: Option<CivId>, req: &Request) -> Outcome {
    let body = req.json();
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
            let spendable = permutation_rules::orders::spendable(&g.state, &g.rules, nation, *role);
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

// ------------------------------------------------------------------ acting

/// A person commits its offices' orders for the open tick (chain mode: and
/// submits them).
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
    let adopt: Vec<(Role, Vec<u32>)> = body["adopt"]
        .as_object()
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
        .unwrap_or_default();
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
        let ids = adopt
            .iter()
            .find(|(r, _)| r == role)
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
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
        let digest = g.ledger.commit(tick, civ, role as u8, "human", &text);
        offices.push(json!({"role": role_name(role), "digest": hex(&digest), "cost": cost}));
        let x = &mut g.members[m as usize];
        x.committed[role.index()] = parts[role.index()].iter().map(|(d, _)| d.clone()).collect();
        x.cost[role.index()] = cost;
        x.adopt[role.index()] = ids;
    }
    match g.submission(m) {
        Some(submission) => Outcome::Orders {
            submission,
            tick,
            offices,
        },
        None => Response::ok(&json!({"ok": true, "tick": tick, "offices": offices})).into(),
    }
}

/// A member's governance action: queued for the next tick (local), or sent
/// to the chain.
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
    if g.phase == Phase::Lobby && g.chain.is_none() {
        return refused("in the lobby, set your candidacy and votes with POST /api/lobby");
    }
    if matches!(action, GovAction::Vote { .. }) && !gov::vote_open(&g.rules, g.state.tick) {
        let next = gov::next_term_start(&g.rules, g.state.tick);
        return refused(format!(
            "votes open {} ticks before the next term ({next:?})",
            g.rules.vote_window
        ));
    }
    if let Some(c) = &g.chain {
        return Outcome::Gov {
            link: c.link.clone(),
            body: json!({"member": m, "action": dto}),
        };
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

/// Pause, tick length, and ending the member's turn.
fn control(g: &mut Game, viewer: Viewer, req: &Request) -> Outcome {
    let Viewer::Member(m) = viewer else {
        return Response::error("403 Forbidden", "only members control the game").into();
    };
    let v = req.json();
    let tick = g.state.tick;
    let advance = v["advance"].as_bool() == Some(true);
    if g.chain.is_some() {
        // Chain mode: no pause; "advance" submits whatever is committed.
        if advance {
            if let Some(submission) = g.submission(m) {
                return Outcome::EndTurn { submission, tick };
            }
        }
        return Response::ok(&json!({"ok": true, "paused": false, "tick": tick})).into();
    }
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
                "tick": r.tick, "civ": r.civ, "role": Role::from_index(r.role as usize).map(role_name),
                "obsRoot": hex(&r.obs_root), "digest": hex(&r.digest), "external": r.external,
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
    use std::sync::Mutex;

    fn server() -> Shared {
        Arc::new(Mutex::new(Game::new(30, 1)))
    }

    fn call(
        game: &Shared,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Value,
    ) -> (String, Value) {
        let body = body.to_string();
        let auth = token.map_or(String::new(), |t| format!("X-Member-Token: {t}\r\n"));
        let raw = format!(
            "{method} {path} HTTP/1.1\r\n{auth}Content-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let req = Request::read(raw.as_bytes()).unwrap();
        let web = Path::new(env!("CARGO_MANIFEST_DIR")).join("web");
        let res = respond(&req, game, &web);
        (
            res.status.to_string(),
            serde_json::from_slice(&res.body).unwrap_or(Value::Null),
        )
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
        // Seats are claimed by nation, never by member: asking for a member
        // tells nothing about who is an AI (V5 §18.2).
        assert_eq!(
            call(&game, "POST", "/api/claim", None, json!({"member": 0})).0,
            "400 Bad Request"
        );
        assert_eq!(
            call(&game, "POST", "/api/claim", None, json!({"civ": 0})).0,
            "409 Conflict",
            "a local season has no seats for people to claim"
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
            assert_ne!(m["kind"], "agent");
            assert!(!m["name"].as_str().unwrap().contains("AI"), "{m}");
        }
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
    }

    /// A handler that panics while holding the lock must not take the whole
    /// server down with it.
    #[test]
    fn a_poisoned_lock_still_serves() {
        let game = server();
        let g2 = game.clone();
        let _ = std::thread::spawn(move || {
            let _g = g2.lock().unwrap();
            panic!("a handler panicked");
        })
        .join();
        assert!(game.is_poisoned());
        assert_eq!(
            call(&game, "GET", "/api/lobby", None, json!({})).0,
            "200 OK"
        );
    }
}
