//! Chain mode: the world is the on-chain program's, read through the
//! gateway; the AI members' batches go back through it. Every other member
//! signs its own transactions (in the browser, or an agent's SDK); this
//! server only reads theirs from the chain.
//!
//! Every gateway call happens without the game lock: the follower fetches
//! first (`Poll::fetch`) and then applies what it got (`Game::on_chain`).

use permutation_chain::state::MAX_GOV_PER_SIGNER;
use permutation_rules::gov::{GovEntry, MemberId, NOBODY};
use permutation_rules::orders::OrderBatch;
use permutation_rules::state::CivId;
use permutation_rules::tick::TickInput;
use permutation_rules::{Preset, Ruleset};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::game::{role_name, Game, Host, Member, Phase, LOCAL_FEE};
use super::{lock, Shared, Site};
use crate::api::{self, GovDto, MemberMeta, OrderDto};
use crate::chainlink::{ChainLink, Snapshot, WorldMeta};
use crate::codec::hex;

pub struct ChainMode {
    pub link: Arc<ChainLink>,
    /// The gateway as the browser reaches it (`view.chain.gateway`): `/gw`
    /// behind this server's proxy, else the `--chain` URL.
    pub gateway: String,
    pub meta: WorldMeta,
    pub slot: u64,
    pub layer: String,
    pub phase: String,
    /// The gateway's /season answer: accounts, program id, member registry, pool.
    pub info: Value,
    pub last_record: Option<Value>,
    /// Tick the AI members already submitted for.
    pub ai_submitted: Option<u16>,
}

/// A number the gateway sends as a JSON number or a decimal string.
pub fn number(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|x| x.parse().ok()))
}

/// The operator AI members a season committed to (the Season account's
/// `aiCount`, public), from a gateway `/season` answer.
pub fn ai_count(info: &Value) -> u64 {
    number(&info["season"]["aiCount"])
        .or_else(|| number(&info["registration"]["aiCount"]))
        .unwrap_or(0)
}

/// The game's phase for the gateway's phase (`X-Phase`): the lobby until
/// the government opened and the world went to the ER.
pub fn phase_of(gateway_phase: &str) -> Option<Phase> {
    match gateway_phase {
        "" => None,
        "setup" | "registering" => Some(Phase::Lobby),
        _ => Some(Phase::Playing),
    }
}

impl ChainMode {
    /// The Season account's prize pool (entry fees; in-play income is added by the caller).
    pub fn pool(&self) -> u64 {
        number(&self.info["season"]["pool"]).unwrap_or(0)
    }

    pub fn ai_count(&self) -> u64 {
        ai_count(&self.info)
    }

    /// The open tick's sealed-orders phase: `commit`, `reveal` (the
    /// commitments closed; `meta.deadline` is now the reveal deadline),
    /// `frozen` (being published and resolved) or `finished`.
    pub fn tick_phase(&self) -> &'static str {
        if self.meta.finished {
            "finished"
        } else if self.meta.frozen {
            "frozen"
        } else if self.meta.revealing {
            "reveal"
        } else {
            "commit"
        }
    }
}

/// A batch as the gateway's /submit takes it.
pub fn batch_body(b: &OrderBatch) -> Value {
    json!({
        "civ": b.civ, "role": role_name(b.role), "member": api::member_ref(b.member), "tick": b.tick,
        "digest": hex(&b.decision_digest), "orders": b.orders.iter().map(OrderDto::from_order).collect::<Vec<_>>(), "adopt": b.adopt,
    })
}

/// Chain mode: wait for the on-chain world (meanwhile the site serves the
/// registration lobby with the season the gateway describes), then attach
/// the game to it and follow the chain. Never gives up: the gateway may be
/// starting, or the season still registering.
pub fn attach(site: Arc<Site>, link: Arc<ChainLink>) {
    let gateway = site.gateway.clone().unwrap_or_else(|| link.url());
    let mut said = String::new();
    let (snap, info) = loop {
        let wait = match link.world() {
            Ok(Some(snap)) => match link.get_json("/season") {
                Ok(info) => break (snap, info),
                Err(e) => format!("waiting for the season ({e})…"),
            },
            Ok(None) => {
                if let Ok(info) = link.get_json("/season") {
                    site.set_season(info);
                }
                "waiting for the on-chain world (registration, genesis or delegation in progress)…"
                    .to_string()
            }
            Err(e) => format!("waiting for the gateway ({e})…"),
        };
        if wait != said {
            eprintln!("{wait}");
            said = wait;
        }
        std::thread::sleep(Duration::from_secs(2));
    };
    let game: Shared = Arc::new(Mutex::new(Game::from_snapshot(link, snap, info, gateway)));
    {
        let g = lock(&game);
        eprintln!(
            "  the on-chain world: {} nations, {} members, phase {:?}",
            g.state.civs.len(),
            g.members.len(),
            g.phase
        );
    }
    site.install(game.clone());
    follow(game);
}

impl Game {
    /// A game on a running on-chain season: `snap` is its world, `info` the
    /// gateway's `/season`.
    pub fn from_snapshot(
        link: Arc<ChainLink>,
        snap: Snapshot,
        info: Value,
        gateway: String,
    ) -> Game {
        let mut rules = match snap.meta.preset {
            1 => Ruleset::new(Preset::Season),
            _ => Ruleset::new(Preset::Blitz),
        };
        rules.market_enabled = snap.meta.market;
        let tick_seconds = snap.meta.tick_seconds as u64;
        let season_id = snap.meta.season_id.to_le_bytes();
        let roster = link.get_json("/operator/roster");
        let chain = ChainMode {
            link,
            gateway,
            meta: snap.meta,
            slot: snap.slot,
            layer: snap.layer,
            phase: snap.phase,
            info: info.clone(),
            last_record: None,
            ai_submitted: None,
        };
        let mut g = Game::assemble(rules, snap.state, &season_id, tick_seconds, Some(chain));
        g.entry_fee = number(&info["season"]["entryFee"]).unwrap_or(LOCAL_FEE);
        g.sync_registry(&info);
        if let Ok(op) = roster {
            g.sync_operator(&op);
        }
        let started = g.state.tick > 0
            || !g.state.members.is_empty()
            || g.state
                .nations
                .iter()
                .any(|n| n.offices.iter().any(|o| *o != NOBODY));
        let phase = g.chain.as_ref().and_then(|c| phase_of(&c.phase));
        g.phase = phase.unwrap_or(if started {
            Phase::Playing
        } else {
            Phase::Lobby
        });
        g
    }

    fn chain_link(&self) -> Option<Arc<ChainLink>> {
        self.chain.as_ref().map(|c| c.link.clone())
    }

    /// Which members are the operator's AI members (run here), and their
    /// roster (V5 §18.2), from the gateway's operator-only `/operator/roster`.
    /// Every other member signs its own transactions.
    pub fn sync_operator(&mut self, op: &Value) {
        for m in op["members"].as_array().into_iter().flatten() {
            let Some(i) = m["member"].as_u64().map(|x| x as usize) else {
                continue;
            };
            let host = match m["hosted"].as_str() {
                Some("ai") => Host::Ai,
                _ => Host::External,
            };
            if let Some(x) = self.members.get_mut(i) {
                x.host = host;
            }
        }
        self.roster
            .merge(super::roster::AiRoster::from_operator(op));
    }

    /// The gateway's public member registry (names, kinds). Who is an AI
    /// comes from `sync_operator`; a member not known there is external. In
    /// a season with operator AI members every kind reads "undeclared",
    /// unattested (V5 §18.2), whatever a member declared.
    pub fn sync_registry(&mut self, info: &Value) {
        let hide = ai_count(info) > 0 || self.hides_kinds();
        for (i, m) in info["members"].as_array().into_iter().flatten().enumerate() {
            let host = self.members.get(i).map_or(Host::External, |x| x.host);
            let meta = MemberMeta {
                name: m["name"].as_str().unwrap_or("member").to_string(),
                kind: match m["kind"].as_str() {
                    Some(k) if !hide => k.to_string(),
                    _ => "undeclared".to_string(),
                },
                attested: !hide && m["attested"].as_bool().unwrap_or(false),
            };
            let civ = m["civ"].as_u64().unwrap_or(0) as u16;
            match self.members.get_mut(i) {
                Some(x) => {
                    x.meta = meta;
                    x.host = host;
                }
                None => self.members.push(Member::new(meta, host, civ)),
            }
        }
    }

    /// Apply a snapshot and what `Poll::fetch` learned with it. Returns what
    /// to send to the gateway once the lock is released: the AI members'
    /// governance, then every AI-run office's batch.
    pub fn on_chain(&mut self, poll: Poll) -> Sends {
        let Poll {
            snap,
            resolved,
            info,
            operator,
            talk,
        } = poll;
        self.receive_talk(talk);
        let changed = snap.state.tick != self.state.tick
            || snap.state.members.len() != self.state.members.len();
        let prev = std::mem::replace(&mut self.state, snap.state);
        if changed {
            if prev.tick != self.state.tick {
                self.after_tick(&prev);
                if let Some((input, rec)) = resolved {
                    // The ledger follows what landed: a decision on chain
                    // replaces one sealed here for a batch that did not
                    // land, and a sealed decision whose batch never landed
                    // goes (it could never be revealed).
                    let mut landed: Vec<(CivId, u8)> = Vec::new();
                    for b in &input.batches {
                        self.ledger.land(b);
                        if b.tick == prev.tick && b.decision_digest != [0; 32] {
                            landed.push((b.civ, b.role as u8));
                        }
                    }
                    self.ledger.keep_landed(prev.tick, &landed);
                    if let Some(c) = self.chain.as_mut() {
                        c.last_record = Some(rec);
                    }
                }
            } else {
                self.fog.update(&self.state);
            }
            self.ledger.observe(&self.state, &self.fog);
            if let Some(info) = info {
                self.sync_registry(&info);
                if let Some(c) = self.chain.as_mut() {
                    c.info = info;
                }
            }
            if let Some(op) = operator {
                self.sync_operator(&op);
            }
        }
        let announce = std::mem::take(&mut self.announce);
        let replies = self.ai_replies();
        let tick = self.state.tick;
        let Some(c) = self.chain.as_mut() else {
            return Sends::default();
        };
        c.meta = snap.meta;
        c.slot = snap.slot;
        c.layer = snap.layer;
        c.phase = snap.phase;
        self.phase = phase_of(&c.phase).unwrap_or(Phase::Playing);
        if c.meta.finished
            || c.phase != "playing"
            || tick >= self.rules.ticks_per_season
            || c.ai_submitted == Some(tick)
        {
            return Sends {
                announce,
                talk: replies,
                ..Sends::default()
            };
        }
        c.ai_submitted = Some(tick);
        let hosts = self.hosts();
        // Governance first: once every office's batch is in, the crank freezes
        // the tick's input and later actions wait for the next tick (TickFrozen).
        let gov = self
            .planner
            .member_gov(&self.state, &self.rules, &self.fog, &hosts);
        let batches = self.planner.batches(
            &self.state,
            &self.rules,
            &self.fog,
            &mut self.ledger,
            &hosts,
        );
        Sends {
            gov: gov_bodies(&gov),
            batches: batches.iter().map(batch_body).collect(),
            announce,
            talk: replies,
        }
    }
}

/// What the chain follower learns from the gateway before taking the lock.
pub struct Poll {
    pub snap: Snapshot,
    /// The input of the tick that just resolved, with its PS_TICK record.
    pub resolved: Option<(TickInput, Value)>,
    /// The season (member registry, pool), when members or the tick changed.
    pub info: Option<Value>,
    /// The operator's roster (who is hosted, AI salts), fetched with `info`.
    pub operator: Option<Value>,
    /// Members' messages since the last poll.
    pub talk: Vec<Value>,
}

impl Poll {
    /// `tick` and `members` are the game's before the snapshot.
    fn fetch(
        link: &ChainLink,
        tick: u16,
        members: usize,
        talk_next: u64,
    ) -> Result<Option<Poll>, String> {
        let Some(snap) = link.world()? else {
            return Ok(None);
        };
        let changed = snap.state.tick != tick || snap.state.members.len() != members;
        let resolved = if snap.state.tick != tick {
            link.resolved_input(tick).unwrap_or_else(|e| {
                eprintln!("tick {tick}: resolved input unreadable ({e})");
                None
            })
        } else {
            None
        };
        let info = if changed {
            link.get_json("/season").ok()
        } else {
            None
        };
        let operator = if changed && link.has_token() {
            link.get_json("/operator/roster").ok()
        } else {
            None
        };
        let talk = link
            .get_json(&format!("/talk?since={talk_next}"))
            .ok()
            .and_then(|v| v["messages"].as_array().cloned())
            .unwrap_or_default();
        Ok(Some(Poll {
            snap,
            resolved,
            info,
            operator,
            talk,
        }))
    }
}

/// Follow the chain: new ticks, then the AI members'
/// submissions. Only this thread changes the world in chain mode.
pub fn follow(game: Shared) {
    loop {
        std::thread::sleep(Duration::from_millis(400));
        let (link, tick, members, talk_next) = {
            let g = lock(&game);
            let Some(link) = g.chain_link() else { return };
            (link, g.state.tick, g.state.members.len(), g.talk_next)
        };
        let poll = match Poll::fetch(&link, tick, members, talk_next) {
            Ok(Some(p)) => p,
            Ok(None) => continue,
            Err(e) => {
                eprintln!("chain: {e}");
                continue;
            }
        };
        let sends = lock(&game).on_chain(poll);
        // Governance first: once every office's batch is in, the crank
        // freezes the tick's input and later actions wait (TickFrozen).
        post_all(&link, "/gov", &sends.gov);
        post_all(&link, "/submit", &sends.batches);
        // An AI member's home city fell: the gateway makes its salt public.
        let announce: Vec<Value> = sends
            .announce
            .iter()
            .map(|m| json!({"member": m}))
            .collect();
        post_all(&link, "/roster/announce", &announce);
        post_all(&link, "/talk", &sends.talk);
    }
}

/// What the AI members send for the open tick.
#[derive(Default)]
pub struct Sends {
    /// `/gov` bodies: one per member, with its actions for the tick.
    pub gov: Vec<Value>,
    /// `/submit` bodies: one per office.
    pub batches: Vec<Value>,
    /// AI members whose home city fell (V5 §18.3).
    pub announce: Vec<MemberId>,
    /// AI members' answers (`/talk` bodies, signed by the gateway).
    pub talk: Vec<Value>,
}

/// Governance actions as `/gov` bodies: a member's actions go together (at
/// most `MAX_GOV_PER_SIGNER` per body, the program's limit per signer and
/// tick), so a vote window's votes take one transaction per member, not one
/// per vote.
fn gov_bodies(entries: &[GovEntry]) -> Vec<Value> {
    let mut by_member: BTreeMap<MemberId, Vec<GovDto>> = BTreeMap::new();
    for e in entries {
        by_member
            .entry(e.member)
            .or_default()
            .push(GovDto::from_action(&e.action));
    }
    by_member
        .into_iter()
        .flat_map(|(member, actions)| {
            actions
                .chunks(MAX_GOV_PER_SIGNER)
                .map(|a| json!({"member": member, "actions": a}))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Requests in flight at once: each is a transaction the gateway confirms
/// on the ER, which takes about a second on devnet.
const PARALLEL_SENDS: usize = 8;

/// POST every body to `path`, `PARALLEL_SENDS` at a time; failures are logged.
fn post_all(link: &ChainLink, path: &str, bodies: &[Value]) {
    let next = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..PARALLEL_SENDS.min(bodies.len()) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(body) = bodies.get(i) else { break };
                if let Err(e) = link.post_json(path, body) {
                    let who = if body["civ"].is_null() {
                        format!("member {}", body["member"])
                    } else {
                        format!("nation {}", body["civ"])
                    };
                    eprintln!("{path} for {who} failed: {e}");
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play::http::{Request, Response};
    use crate::play::routes::respond;
    use permutation_chain::state::{WORLD_HEADER, WORLD_MAGIC};
    use permutation_rules::genesis::{nation_entries, new_season};
    use permutation_rules::gov::{GovAction, Role};
    use std::net::TcpListener;
    use std::sync::atomic::AtomicBool;

    fn vote(member: MemberId, role: Role) -> GovEntry {
        GovEntry {
            member,
            signer: [0; 32],
            action: GovAction::Vote {
                role,
                candidate: member,
            },
        }
    }

    /// A vote window's votes go out as one request per member, in the
    /// order the member cast them, never more than the program takes.
    #[test]
    fn governance_goes_out_per_member() {
        let mut entries = Vec::new();
        for m in [3, 1] {
            for r in Role::ALL {
                entries.push(vote(m, r));
            }
        }
        entries.extend((0..9).map(|_| vote(2, Role::General)));
        let bodies = gov_bodies(&entries);
        let shape: Vec<(u64, usize)> = bodies
            .iter()
            .map(|b| {
                (
                    b["member"].as_u64().unwrap(),
                    b["actions"].as_array().unwrap().len(),
                )
            })
            .collect();
        assert_eq!(shape, vec![(1, 4), (2, MAX_GOV_PER_SIGNER), (2, 1), (3, 4)]);
        let roles: Vec<&str> = bodies[0]["actions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["role"].as_str().unwrap())
            .collect();
        assert_eq!(roles, ["General", "Steward", "Science", "Diplomat"]);
    }

    /// `/world.bin` as the gateway serves it: magic, length, meta, world.
    fn world_bin(meta: &WorldMeta, state: &permutation_rules::state::WorldState) -> Vec<u8> {
        let body = borsh::to_vec(state).unwrap();
        let mut out = WORLD_MAGIC.to_vec();
        out.extend((body.len() as u32).to_le_bytes());
        out.extend(borsh::to_vec(meta).unwrap());
        out.resize(WORLD_HEADER, 0);
        out.extend(body);
        out
    }

    /// Chain mode from start to play without a restart: while there is no
    /// world the site serves the registration lobby (with what the gateway
    /// says of the season); once the world exists and the gateway plays,
    /// the full game.
    #[test]
    fn the_site_waits_for_the_world_then_serves_the_game() {
        let rules = Ruleset::new(Preset::Blitz);
        let state = new_season(
            &rules,
            b"permutation-state/world/test-001",
            b"permutation-state/season/test-01",
            &nation_entries(6),
        )
        .unwrap();
        let meta = WorldMeta {
            season_id: 7,
            civs: 6,
            tick_seconds: 30,
            deadline: super::super::game::unix_now() + 30,
            ..Default::default()
        };
        let world = world_bin(&meta, &state);
        let season = json!({"season": {"seasonId": "7", "entryFee": "10000000", "pool": "0", "aiCount": 0},
                            "programId": "PermutationProgram111", "cluster": "localnet", "accounts": {}, "members": []});
        let exists = Arc::new(AtomicBool::new(false));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let flag = exists.clone();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let Some(req) = Request::read(&stream) else {
                    continue;
                };
                let res = match req.path.as_str() {
                    "/world.bin" if flag.load(Ordering::SeqCst) => {
                        let mut r =
                            Response::new("200 OK", "application/octet-stream", world.clone());
                        r.headers.push(("X-Phase", "playing".into()));
                        r.headers.push(("X-Slot", "5".into()));
                        r
                    }
                    "/world.bin" => {
                        Response::error("503 Service Unavailable", "world not available")
                    }
                    "/season" => Response::ok(&season),
                    "/talk" => Response::ok(&json!({"messages": []})),
                    "/ticks" => Response::ok(&json!({"records": []})),
                    _ => Response::error("404 Not Found", "no"),
                };
                res.write(&mut stream);
            }
        });
        let web = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web");
        let site = Arc::new(Site::new(web, None, Some(&url)));
        let link = Arc::new(ChainLink::new(&url).unwrap());
        let s = site.clone();
        std::thread::spawn(move || attach(s, link));
        let lobby = || {
            let req = Request::read(&b"GET /api/lobby HTTP/1.1\r\n\r\n"[..]).unwrap();
            serde_json::from_slice::<Value>(&respond(&req, &site).body).unwrap()
        };
        let until = |what: &str, ok: &dyn Fn(&Value) -> bool| {
            for _ in 0..100 {
                let l = lobby();
                if ok(&l) {
                    return l;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            panic!("{what}: {}", lobby());
        };
        let l = until("the season from the gateway", &|l| !l["chain"].is_null());
        assert_eq!(
            (l["phase"].as_str(), l["gateway"].as_str()),
            (Some("registering"), Some(url.as_str()))
        );
        assert_eq!(
            (l["chain"]["seasonId"].as_str(), &l["entryFee"]),
            (Some("7"), &json!(10_000_000))
        );
        exists.store(true, Ordering::SeqCst);
        let l = until("the game", &|l| l["phase"] == "playing");
        assert_eq!(l["nations"].as_array().unwrap().len(), 6);
        assert_eq!(l["chain"]["seasonId"], "7");
        let req = Request::read(&b"GET /api/state HTTP/1.1\r\n\r\n"[..]).unwrap();
        let v: Value = serde_json::from_slice(&respond(&req, &site).body).unwrap();
        assert_eq!(
            (v["chainPhase"].as_str(), v["chain"]["slot"].as_u64()),
            (Some("commit"), Some(5))
        );
    }
}
