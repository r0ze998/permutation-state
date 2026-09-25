//! Chain mode: the world is the on-chain program's, read through the
//! gateway; the AI members' batches, and the people's,
//! go back through it.
//!
//! Every gateway call happens without the game lock: the follower fetches
//! first (`Poll::fetch`) and then applies what it got (`Game::on_chain`),
//! and a person's submission is prepared under the lock, sent without it
//! (`Submission::send`), and its outcome recorded under it again.

use permutation_chain::state::MAX_GOV_PER_SIGNER;
use permutation_rules::gov::{GovEntry, MemberId, NOBODY};
use permutation_rules::orders::OrderBatch;
use permutation_rules::tick::TickInput;
use permutation_rules::{Preset, Ruleset};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::game::{role_name, Game, Host, Member, Phase, LOCAL_FEE};
use super::{lock, Shared};
use crate::api::{self, GovDto, MemberMeta, OrderDto};
use crate::chainlink::{ChainLink, Snapshot, WorldMeta};
use crate::codec::hex;

pub struct ChainMode {
    pub link: Arc<ChainLink>,
    pub meta: WorldMeta,
    pub slot: u64,
    pub layer: String,
    pub phase: String,
    /// The gateway's /season answer: accounts, program id, member registry, pool.
    pub info: Value,
    pub last_record: Option<Value>,
    /// Tick the AI members already submitted for.
    pub ai_submitted: Option<u16>,
    pub error: Option<String>,
}

impl ChainMode {
    /// The Season account's prize pool (entry fees; in-play income is added by the caller).
    pub fn pool(&self) -> u64 {
        self.info["season"]["pool"]
            .as_str()
            .and_then(|x| x.parse().ok())
            .unwrap_or(0)
    }
}

/// A batch as the gateway's /submit takes it.
pub fn batch_body(b: &OrderBatch) -> Value {
    json!({
        "civ": b.civ, "role": role_name(b.role), "member": api::member_ref(b.member), "tick": b.tick,
        "digest": hex(&b.decision_digest), "orders": b.orders.iter().map(OrderDto::from_order).collect::<Vec<_>>(), "adopt": b.adopt,
    })
}

/// Retry `f` every two seconds until it succeeds (the gateway may be starting).
fn wait_for<T>(what: &str, mut f: impl FnMut() -> Result<Option<T>, String>) -> T {
    loop {
        match f() {
            Ok(Some(v)) => return v,
            Ok(None) => eprintln!("waiting for {what}…"),
            Err(e) => eprintln!("waiting for the gateway ({e})…"),
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

impl Game {
    /// Attach to a running on-chain season through the gateway, waiting for
    /// it rather than giving up.
    pub fn from_chain(link: Arc<ChainLink>) -> Game {
        let snap = wait_for(
            "the on-chain world (registration, genesis or delegation in progress)",
            || link.world(),
        );
        let info = wait_for("the season", || link.get_json("/season").map(Some));
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
            meta: snap.meta,
            slot: snap.slot,
            layer: snap.layer,
            phase: snap.phase,
            info: info.clone(),
            last_record: None,
            ai_submitted: None,
            error: None,
        };
        let mut g = Game::assemble(rules, snap.state, &season_id, tick_seconds, Some(chain));
        g.entry_fee = info["season"]["entryFee"]
            .as_str()
            .and_then(|x| x.parse().ok())
            .unwrap_or(LOCAL_FEE);
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
        g.phase = if started {
            Phase::Playing
        } else {
            Phase::Lobby
        };
        g
    }

    fn chain_link(&self) -> Option<Arc<ChainLink>> {
        self.chain.as_ref().map(|c| c.link.clone())
    }

    /// Which members the gateway hosts, and the operator's AI roster (V5
    /// §18.2), from its operator-only `/operator/roster`.
    pub fn sync_operator(&mut self, op: &Value) {
        for m in op["members"].as_array().into_iter().flatten() {
            let Some(i) = m["member"].as_u64().map(|x| x as usize) else {
                continue;
            };
            let host = match m["hosted"].as_str() {
                Some("human") => Host::Human,
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

    /// The gateway's public member registry (names, kinds). Who is hosted
    /// comes from `sync_operator`; a member not known there is external.
    pub fn sync_registry(&mut self, info: &Value) {
        for (i, m) in info["members"].as_array().into_iter().flatten().enumerate() {
            let host = self.members.get(i).map_or(Host::External, |x| x.host);
            let meta = MemberMeta {
                name: m["name"].as_str().unwrap_or("member").to_string(),
                kind: m["kind"].as_str().unwrap_or("undeclared").to_string(),
                attested: m["attested"].as_bool().unwrap_or(false),
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
                    for b in &input.batches {
                        let external = b.member != NOBODY
                            && self
                                .members
                                .get(b.member as usize)
                                .is_some_and(|m| m.host == Host::External);
                        if external {
                            self.ledger.ingest_external(b);
                        } else {
                            self.ledger
                                .mark_revealed(b.civ, b.role as u8, &b.orders, prev.tick);
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
        self.phase = if c.phase == "setup" || c.phase == "registering" {
            Phase::Lobby
        } else {
            Phase::Playing
        };
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

    /// Chain mode: `m`'s batches for the open tick, ready to send.
    pub fn submission(&mut self, m: MemberId) -> Option<Submission> {
        let link = self.chain_link()?;
        let batches = self.human_batches(Some(m), true);
        Some(Submission {
            link,
            member: m,
            tick: self.state.tick,
            batches,
        })
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

/// A person's batches on their way to the chain (signed by the gateway with
/// the member's session key).
pub struct Submission {
    link: Arc<ChainLink>,
    member: MemberId,
    tick: u16,
    batches: Vec<OrderBatch>,
}

impl Submission {
    /// Send every batch (without the lock), then record the outcome: the
    /// member's turn ends only if every batch went through. Returns the
    /// signatures, or the first error.
    pub fn send(self, game: &Shared) -> Result<Vec<Value>, String> {
        let mut sigs = Vec::new();
        let mut error = None;
        for b in &self.batches {
            match self.link.post_json("/submit", &batch_body(b)) {
                Ok(res) => sigs.push(res["signature"].clone()),
                Err(e) => {
                    error = Some(format!("chain: {e}"));
                    break;
                }
            }
        }
        let mut g = lock(game);
        if error.is_none() {
            if let Some(x) = g.members.get_mut(self.member as usize) {
                x.ready = Some(self.tick);
            }
        }
        if let Some(c) = g.chain.as_mut() {
            c.error = error.clone();
        }
        error.map_or(Ok(sigs), Err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use permutation_rules::gov::{GovAction, Role};

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
}
