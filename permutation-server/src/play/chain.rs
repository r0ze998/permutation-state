//! Chain mode: the world is the on-chain program's, read through the
//! gateway; the AI members' and acting officials' batches, and the people's,
//! go back through it.
//!
//! Every gateway call happens without the game lock: the follower fetches
//! first (`Poll::fetch`) and then applies what it got (`Game::on_chain`),
//! and a person's submission is prepared under the lock, sent without it
//! (`Submission::send`), and its outcome recorded under it again.

use permutation_rules::gov::{MemberId, NOBODY};
use permutation_rules::orders::OrderBatch;
use permutation_rules::tick::TickInput;
use permutation_rules::{Preset, Ruleset};
use serde_json::{json, Value};
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
    /// Tick the AI members and acting officials already submitted for.
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

    /// The gateway's member registry (names, kinds, who hosts whom).
    pub fn sync_registry(&mut self, info: &Value) {
        for (i, m) in info["members"].as_array().into_iter().flatten().enumerate() {
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
    /// to send to the gateway (`(path, body)`) once the lock is released:
    /// the AI members' governance, then every AI-run office's batch.
    pub fn on_chain(&mut self, poll: Poll) -> Vec<(&'static str, Value)> {
        let Poll {
            snap,
            resolved,
            info,
        } = poll;
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
        }
        let tick = self.state.tick;
        let Some(c) = self.chain.as_mut() else {
            return Vec::new();
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
            return Vec::new();
        }
        c.ai_submitted = Some(tick);
        let hosts = self.hosts();
        // Governance first: once every office's batch is in, the crank freezes
        // the tick's input and later actions wait for the next tick (TickFrozen).
        let mut out = Vec::new();
        for e in self
            .planner
            .member_gov(&self.state, &self.rules, &self.fog, &hosts)
        {
            out.push((
                "/gov",
                json!({"member": e.member, "action": GovDto::from_action(&e.action)}),
            ));
        }
        for b in self.planner.batches(
            &self.state,
            &self.rules,
            &self.fog,
            &mut self.ledger,
            &hosts,
        ) {
            out.push(("/submit", batch_body(&b)));
        }
        out
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
}

impl Poll {
    /// `tick` and `members` are the game's before the snapshot.
    fn fetch(link: &ChainLink, tick: u16, members: usize) -> Result<Option<Poll>, String> {
        let Some(snap) = link.world()? else {
            return Ok(None);
        };
        let changed = snap.state.tick != tick || snap.state.members.len() != members;
        let resolved = if snap.state.tick != tick {
            link.resolved_input(tick).unwrap_or(None)
        } else {
            None
        };
        let info = if changed {
            link.get_json("/season").ok()
        } else {
            None
        };
        Ok(Some(Poll {
            snap,
            resolved,
            info,
        }))
    }
}

/// Follow the chain: new ticks, then the AI members' and acting officials'
/// submissions. Only this thread changes the world in chain mode.
pub fn follow(game: Shared) {
    loop {
        std::thread::sleep(Duration::from_millis(400));
        let (link, tick, members) = {
            let g = lock(&game);
            let Some(link) = g.chain_link() else { return };
            (link, g.state.tick, g.state.members.len())
        };
        let poll = match Poll::fetch(&link, tick, members) {
            Ok(Some(p)) => p,
            Ok(None) => continue,
            Err(e) => {
                eprintln!("chain: {e}");
                continue;
            }
        };
        let sends = lock(&game).on_chain(poll);
        for (path, body) in sends {
            if let Err(e) = link.post_json(path, &body) {
                eprintln!("{path} for nation {} failed: {e}", body["civ"]);
            }
        }
    }
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
