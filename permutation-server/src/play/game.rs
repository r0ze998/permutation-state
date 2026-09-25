//! The game the play server hosts: the world, its members and who runs
//! them, the lobby, and resolving ticks (local mode). Chain mode's side is in
//! `chain`, the JSON views in `views`.

use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::gov::{self, GovAction, GovEntry, MemberId, Role, NOBODY};
use permutation_rules::invariants;
use permutation_rules::orders::{role_allows, validate_batch, Order, OrderBatch};
use permutation_rules::payout::{settle, Settlement};
use permutation_rules::rng::Seed;
use permutation_rules::state::{CivId, WorldState};
use permutation_rules::tick::{resolve_tick, TickInput};
use permutation_rules::{Preset, Ruleset};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

use super::chain::ChainMode;
use super::http::Request;
use super::roster::{AiEntry, AiRoster};
use crate::api::{self, MemberMeta, OrderDto};
use crate::codec::hex;
use crate::driver::{local_key, Hosting, Planner};
use crate::events::{diff_events, diff_gov_events};
use crate::fog::Fog;
use crate::ledger::Ledger;

/// How long a claimed human member stays "here" without any request.
pub const IDLE: Duration = Duration::from_secs(60);
/// Test entry fee of local seasons (no real funds).
pub const LOCAL_FEE: u64 = 10_000_000;
/// Chronicle lines kept.
const CHRONICLE: usize = 500;

/// Who runs a member's decisions.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Host {
    /// A person in the browser.
    Human,
    /// This server's AI (`driver`).
    Ai,
    /// An outside agent that signs its own transactions.
    External,
}

impl Host {
    pub fn name(self) -> &'static str {
        match self {
            Host::Human => "human",
            Host::Ai => "ai",
            Host::External => "external",
        }
    }
}

pub struct Member {
    pub meta: MemberMeta,
    pub host: Host,
    pub civ: CivId,
    /// Browser token of whoever holds this human member.
    pub token: Option<String>,
    pub last_seen: Option<Instant>,
    /// Orders committed for the open tick, per office held (human officers).
    pub committed: [Vec<OrderDto>; 4],
    pub cost: [u32; 4],
    pub adopt: [Vec<u32>; 4],
    /// Tick for which this member ended its turn.
    pub ready: Option<u16>,
    /// Lobby choices, applied when the government opens (local mode).
    pub stand: u8,
    pub votes: [MemberId; 4],
    pub deposit: u64,
}

impl Member {
    pub fn new(meta: MemberMeta, host: Host, civ: CivId) -> Member {
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

    /// Claimed by a browser that asked for something within `IDLE`.
    pub fn here(&self) -> bool {
        self.token.is_some() && self.last_seen.is_some_and(|t| t.elapsed() < IDLE)
    }
}

/// Who is asking.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Viewer {
    /// Holds this member's token: may act for it.
    Member(MemberId),
    /// Reads this nation's view (the full state).
    Watch(CivId),
    Spectator,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Phase {
    Lobby,
    Playing,
}

impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Phase::Lobby => "lobby",
            Phase::Playing => "playing",
        }
    }
}

/// The hosting map handed to the AI driver (no borrow of the game).
pub struct Hosts {
    hosts: Vec<Host>,
}

impl Hosting for Hosts {
    fn is_ai(&self, m: MemberId) -> bool {
        self.hosts.get(m as usize) == Some(&Host::Ai)
    }
    fn key(&self, m: MemberId) -> [u8; 32] {
        local_key(m)
    }
}

pub fn role_name(r: Role) -> String {
    format!("{r:?}")
}

pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub struct Game {
    pub rules: Ruleset,
    pub state: WorldState,
    pub fog: Fog,
    pub ledger: Ledger,
    pub planner: Planner,
    pub members: Vec<Member>,
    pub phase: Phase,
    /// Human governance actions for the next tick (local mode).
    pub pending_gov: Vec<GovEntry>,
    pub deadline: Instant,
    pub tick_seconds: u64,
    pub paused: bool,
    /// `(tick, "kind|text")`, newest last; filtered per viewer when served.
    pub chronicle: Vec<(u16, String)>,
    pub last_events: Vec<String>,
    pub resolved_tick: Option<u16>,
    pub chain: Option<ChainMode>,
    pub token_counter: u64,
    pub entry_fee: u64,
    /// The operator's AI members (V5 §18.2): only this server knows them.
    pub roster: AiRoster,
    /// AI members whose home city fell since the last send (chain mode:
    /// announced to the gateway, which makes their salts public).
    pub announce: Vec<MemberId>,
    /// Members' messages (V5 §18.7), newest last, and the next id to fetch.
    pub talk: Vec<serde_json::Value>,
    pub talk_next: u64,
    /// Messages already answered by this server's AI members.
    pub talk_answered: std::collections::BTreeSet<u64>,
}

/// Names of the members this server hosts, people and AI alike (V5 §18.2).
pub const SEAT_NAMES: [&str; 24] = [
    "Aoi", "Ren", "Mika", "Sora", "Yuki", "Haru", "Kai", "Nao", "Rin", "Toma", "Lina", "Mateo",
    "Ines", "Otto", "Freya", "Iris", "Lucas", "Nora", "Theo", "Zara", "Emil", "Selin", "Kofi",
    "Ada",
];

impl Game {
    /// Local season: the world exists at tick 0; members join in the lobby.
    /// Every nation gets `ai_members` hosted AI members.
    pub fn new(tick_seconds: u64, ai_members: usize) -> Game {
        let rules = Ruleset::new(Preset::Blitz);
        let world: Seed = *b"permutation-state/world/play-005";
        let season: Seed = *b"permutation-state/season/play-05";
        let state = new_season(&rules, &world, &season, &nation_entries(6))
            .expect("genesis of a fixed seed");
        let mut g = Game::assemble(rules, state, &world, tick_seconds, None);
        for civ in 0..g.state.civs.len() as CivId {
            for k in 0..ai_members {
                // Named like anyone else: the operator's AI members are not
                // told apart while the season is played (V5 §18.2).
                let name = SEAT_NAMES[g.members.len() % SEAT_NAMES.len()].to_string();
                let meta = MemberMeta {
                    name,
                    kind: "undeclared".into(),
                    attested: false,
                };
                let id = match g.join_local(civ, meta, Host::Ai) {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!(
                            "{}: AI member {} not added: {e}",
                            g.state.civs[civ as usize].name,
                            k + 1
                        );
                        break;
                    }
                };
                g.roster.entries.push(AiEntry {
                    member: id,
                    civ,
                    salt: Sha256::new()
                        .chain_update(b"PS/local-ai")
                        .chain_update(id.to_le_bytes())
                        .finalize()
                        .into(),
                    fallen: None,
                });
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

    pub fn assemble(
        rules: Ruleset,
        state: WorldState,
        seed: &[u8],
        tick_seconds: u64,
        chain: Option<ChainMode>,
    ) -> Game {
        let fog = Fog::new(&state);
        let mut ledger = Ledger::new(seed);
        ledger.observe(&state, &fog);
        let local = chain.is_none();
        // The AIs' temperaments are drawn per season from a secret (the
        // operator token in chain mode), so nobody can predict them (V5 §18.8).
        let secret: Vec<u8> = chain
            .as_ref()
            .and_then(|c| c.link.secret())
            .map(|s| s.to_vec())
            .unwrap_or_else(|| seed.to_vec());
        let planner = Planner::seeded(state.civs.len(), &[&secret[..], seed].concat());
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
            roster: AiRoster::default(),
            announce: Vec::new(),
            talk: Vec::new(),
            talk_next: 0,
            talk_answered: Default::default(),
        }
    }

    pub fn hosts(&self) -> Hosts {
        Hosts {
            hosts: self.members.iter().map(|m| m.host).collect(),
        }
    }

    pub fn metas(&self) -> Vec<MemberMeta> {
        self.members.iter().map(|m| m.meta.clone()).collect()
    }

    pub fn names(&self) -> Vec<String> {
        self.state.civs.iter().map(|c| c.name.clone()).collect()
    }

    pub fn over(&self) -> bool {
        self.state.tick >= self.rules.ticks_per_season
    }

    /// A fresh browser token (unpredictable: clock, process, counter).
    pub fn token(&mut self, salt: u64) -> String {
        self.token_counter += 1;
        let mut h = Sha256::new();
        h.update(b"PS/member-token");
        h.update(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
                .to_le_bytes(),
        );
        h.update(std::process::id().to_le_bytes());
        h.update(self.token_counter.to_le_bytes());
        h.update(salt.to_le_bytes());
        hex(&h.finalize()[..16])
    }

    pub fn viewer(&self, req: &Request) -> Viewer {
        let t = req
            .header("x-member-token")
            .or_else(|| req.header("x-seat-token"))
            .or_else(|| req.q("token"));
        if let Some(t) = t.filter(|t| !t.is_empty()) {
            if let Some(i) = self
                .members
                .iter()
                .position(|m| m.token.as_deref() == Some(t))
            {
                return Viewer::Member(i as MemberId);
            }
        }
        if let Some(m) = req
            .qi::<MemberId>("member")
            .filter(|m| (*m as usize) < self.state.members.len())
        {
            return Viewer::Watch(self.state.members[m as usize].civ);
        }
        match req.qi::<CivId>("civ") {
            Some(c) if (c as usize) < self.state.civs.len() => Viewer::Watch(c),
            _ => Viewer::Spectator,
        }
    }

    pub fn civ_of(&self, v: Viewer) -> Option<CivId> {
        match v {
            Viewer::Member(m) => self.members.get(m as usize).map(|x| x.civ),
            Viewer::Watch(c) => Some(c),
            Viewer::Spectator => None,
        }
    }

    // -------------------------------------------------------------- lobby (local)

    pub fn join_local(
        &mut self,
        civ: CivId,
        meta: MemberMeta,
        host: Host,
    ) -> Result<MemberId, String> {
        if self.phase != Phase::Lobby || self.chain.is_some() {
            return Err("registration is closed".into());
        }
        let next = self.state.members.len() as MemberId;
        let id = gov::join(&mut self.state, &self.rules, civ, local_key(next))
            .map_err(|e| e.to_string())?;
        self.members.push(Member::new(meta, host, civ));
        Ok(id)
    }

    /// Close the lobby: apply everyone's candidacy and votes, hold the first election.
    pub fn start(&mut self) -> Result<(), String> {
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
                let human = (0..n).find(|j| {
                    let x = &self.members[*j];
                    x.host == Host::Human && x.civ == civ && x.stand & r.bit() != 0
                });
                if let Some(h) = human {
                    self.members[i].votes[r.index()] = h as MemberId;
                }
            }
        }
        let mut pre = Vec::new();
        for (i, m) in self.members.iter().enumerate() {
            let id = i as MemberId;
            pre.push(GovEntry {
                member: id,
                signer: local_key(id),
                action: GovAction::Stand { roles: m.stand },
            });
            for r in Role::ALL {
                let c = m.votes[r.index()];
                if c != NOBODY {
                    pre.push(GovEntry {
                        member: id,
                        signer: local_key(id),
                        action: GovAction::Vote {
                            role: r,
                            candidate: c,
                        },
                    });
                }
            }
        }
        gov::open_government(&mut self.state, &self.rules, &pre).map_err(|e| e.to_string())?;
        self.phase = Phase::Playing;
        self.ledger.observe(&self.state, &self.fog);
        self.deadline = Instant::now() + Duration::from_secs(self.tick_seconds);
        let names = self.names();
        for (c, nation) in self.state.nations.iter().enumerate() {
            let who: Vec<String> = nation
                .offices
                .iter()
                .zip(["general", "steward", "science officer", "diplomat"])
                .map(|(m, r)| match self.members.get(*m as usize) {
                    Some(x) if *m != NOBODY => format!("{r} {}", x.meta.name),
                    _ => format!("{r} vacant (caretaker)"),
                })
                .collect();
            self.chronicle.push((
                0,
                format!("gov|First election — {}: {}", names[c], who.join(", ")),
            ));
        }
        Ok(())
    }

    // -------------------------------------------------------------- ticks

    /// The prize pool as it stands: entry fees (chain: the Season account's
    /// pool) plus in-play income.
    pub fn pool(&self) -> u64 {
        match &self.chain {
            Some(c) => c.pool() + self.state.exchange_vault,
            None => {
                let ops = self.entry_fee * self.rules.ops_share_bps as u64 / 10_000;
                self.state.members.len() as u64 * (self.entry_fee - ops) + self.state.exchange_vault
            }
        }
    }

    /// The payouts if the season ended now.
    pub fn projection(&self) -> Settlement {
        settle(&self.state, &self.rules, self.pool(), self.entry_fee)
    }

    /// The batches of offices held by people (of `only` that member, if
    /// given): their committed orders, the adopted proposals and the reveals
    /// of their earlier decisions. `empty`: also a batch for an office with
    /// nothing to send (on chain, it ends that office's turn so the tick can
    /// resolve early).
    ///
    /// A decision is sealed in the ledger only for a batch that is sent: a
    /// sealed decision whose batch never reached the engine could never be
    /// revealed, and would hold up that office's later reveals.
    pub fn human_batches(&mut self, only: Option<MemberId>, empty: bool) -> Vec<OrderBatch> {
        let tick = self.state.tick;
        let mut out = Vec::new();
        for i in 0..self.members.len() {
            let id = i as MemberId;
            if self.members[i].host != Host::Human || only.is_some_and(|m| m != id) {
                continue;
            }
            let civ = self.members[i].civ;
            for r in Role::ALL {
                if self.state.nations[civ as usize].holder(r) != id {
                    continue;
                }
                let mut orders: Vec<Order> = self.members[i].committed[r.index()]
                    .iter()
                    .filter_map(|d| d.to_order().ok())
                    .collect();
                let adopt = self.members[i].adopt[r.index()].clone();
                let reveals = self.ledger.reveals(civ, r as u8, tick, &orders);
                if orders.is_empty() && reveals.is_empty() && adopt.is_empty() && !empty {
                    continue;
                }
                let digest = match self.ledger.record(tick, civ, r as u8) {
                    Some(rec) => rec.digest,
                    None => self.ledger.commit(tick, civ, r as u8, "human", ""),
                };
                orders.extend(reveals);
                out.push(OrderBatch {
                    civ,
                    tick,
                    role: r,
                    member: id,
                    decision_digest: digest,
                    orders,
                    adopt,
                });
            }
        }
        out
    }

    /// After a tick resolved (`prev` is the world before it): chronicle,
    /// fog, and the members' drafts cleared.
    pub fn after_tick(&mut self, prev: &WorldState) {
        let names = self.names();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let members: Vec<String> = self.members.iter().map(|m| m.meta.name.clone()).collect();
        let mut ev = diff_events(prev, &self.state, &names);
        ev.extend(diff_gov_events(prev, &self.state, &names, &members));
        for e in &ev {
            self.chronicle.push((prev.tick, e.clone()));
        }
        if self.chronicle.len() > CHRONICLE {
            let cut = self.chronicle.len() - CHRONICLE;
            self.chronicle.drain(..cut);
        }
        // Operator AI members whose home city was just conquered (V5 §18.3).
        for e in self.roster.check(&self.state) {
            let (_, by, _, bounty) = e.fallen.unwrap_or_default();
            let who = self
                .members
                .get(e.member as usize)
                .map_or("?".into(), |m| m.meta.name.clone());
            let line = if bounty {
                format!(
                    "bounty|{} conquers the home of {} of {}, an operator AI member: bounty {} USDC",
                    names.get(by as usize).unwrap_or(&"?"),
                    who,
                    names.get(e.civ as usize).unwrap_or(&"?"),
                    self.roster.bounty_each / 1_000_000
                )
            } else {
                format!(
                    "bounty|{} conquers the home of {} of {}, an operator AI member: no bounty (a recent pact)",
                    names.get(by as usize).unwrap_or(&"?"),
                    who,
                    names.get(e.civ as usize).unwrap_or(&"?")
                )
            };
            self.chronicle.push((prev.tick, line.clone()));
            ev.push(line);
            self.announce.push(e.member);
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

    /// Local mode: resolve the open tick in-process. If the engine refuses
    /// the tick, the world and the queued governance are left as they were
    /// and the next attempt waits a full tick.
    pub fn advance(&mut self) {
        if self.over() || self.phase != Phase::Playing {
            return;
        }
        let tick = self.state.tick;
        self.deadline = Instant::now() + Duration::from_secs(self.tick_seconds);
        let hosts = self.hosts();
        let queued = self.pending_gov.clone();
        let mut gov_entries = self
            .planner
            .member_gov(&self.state, &self.rules, &self.fog, &hosts);
        gov_entries.extend(queued.iter().cloned());
        let mut batches = self.planner.batches(
            &self.state,
            &self.rules,
            &self.fog,
            &mut self.ledger,
            &hosts,
        );
        batches.extend(self.human_batches(None, false));
        let deposits: Vec<(CivId, u64)> = if tick == 0 {
            self.members
                .iter()
                .filter(|m| m.deposit > 0)
                .map(|m| (m.civ, m.deposit))
                .collect()
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
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        vrf[2..10].copy_from_slice(&nanos.to_le_bytes());
        let prev = self.state.clone();
        if let Err(e) = resolve_tick(
            &mut self.state,
            &self.rules,
            &TickInput {
                vrf,
                batches,
                gov: gov_entries,
                deposits,
            },
        ) {
            eprintln!("tick {tick} failed: {e}");
            self.state = prev;
            return;
        }
        self.pending_gov.clear();
        self.after_tick(&prev);
        for (civ, role, orders) in &accepted {
            self.ledger.mark_revealed(*civ, *role, orders, tick);
        }
        self.ledger.observe(&self.state, &self.fog);
        let violations = invariants::check(&self.state, &self.rules);
        if !violations.is_empty() {
            eprintln!("invariant violations after tick {tick}: {violations:?}");
        }
    }

    /// Every person here ended its turn.
    pub fn all_humans_ready(&self) -> bool {
        let tick = self.state.tick;
        let here: Vec<&Member> = self
            .members
            .iter()
            .filter(|m| m.host == Host::Human && m.here())
            .collect();
        !here.is_empty() && here.iter().all(|m| m.ready == Some(tick))
    }

    /// People here who have not ended their turn.
    pub fn waiting_for(&self) -> Vec<MemberId> {
        let tick = self.state.tick;
        self.members
            .iter()
            .enumerate()
            .filter(|(_, x)| x.host == Host::Human && x.here() && x.ready != Some(tick))
            .map(|(i, _)| i as MemberId)
            .collect()
    }

    /// Split `orders` among the offices `m` holds. Err if an order belongs to
    /// an office it does not hold (it can be proposed instead).
    pub fn split_for_member(
        &self,
        m: MemberId,
        orders: &[(OrderDto, Order)],
    ) -> Result<[Vec<(OrderDto, Order)>; 4], String> {
        let civ = self.members[m as usize].civ;
        let n = &self.state.nations[civ as usize];
        let mut out: [Vec<(OrderDto, Order)>; 4] = Default::default();
        for (d, o) in orders {
            match Role::ALL
                .into_iter()
                .find(|r| n.holder(*r) == m && role_allows(&self.state, *r, o))
            {
                Some(r) => out[r.index()].push((d.clone(), o.clone())),
                None => {
                    let office =
                        api::office_of(&self.state, o).unwrap_or_else(|| "another office".into());
                    return Err(format!("{} belongs to the {office}, which you do not hold: propose it instead (POST /api/gov Propose)", d.type_name()));
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lobby() -> Game {
        Game::new(30, 1)
    }

    fn human(g: &mut Game, civ: CivId, stand: u8) -> MemberId {
        let meta = MemberMeta {
            name: "P".into(),
            kind: "human".into(),
            attested: false,
        };
        let id = g.join_local(civ, meta, Host::Human).unwrap();
        let m = &mut g.members[id as usize];
        m.stand = stand;
        for r in Role::ALL.into_iter().filter(|r| stand & r.bit() != 0) {
            m.votes[r.index()] = id;
        }
        id
    }

    #[test]
    fn ai_members_vote_for_the_person_who_stands() {
        let mut g = lobby();
        let p = human(&mut g, 0, Role::General.bit());
        g.start().unwrap();
        assert_eq!(g.state.nations[0].holder(Role::General), p);
        assert!(g
            .chronicle
            .iter()
            .any(|(_, l)| l.starts_with("gov|First election")));
        assert!(g
            .join_local(
                1,
                MemberMeta {
                    name: "late".into(),
                    kind: "human".into(),
                    attested: false
                },
                Host::Human
            )
            .is_err());
    }

    /// An office with nothing to send seals no decision: otherwise its
    /// reveal would wait forever and hold up the office's later reveals.
    #[test]
    fn an_idle_office_seals_no_decision() {
        let mut g = lobby();
        let p = human(&mut g, 0, 0x0f);
        g.start().unwrap();
        let held: Vec<Role> = Role::ALL
            .into_iter()
            .filter(|r| g.state.nations[0].holder(*r) == p)
            .collect();
        assert!(!held.is_empty());
        assert!(g.human_batches(None, false).is_empty());
        for r in &held {
            assert!(g.ledger.record(0, 0, *r as u8).is_none());
        }
        // With `empty`, a batch per office, each with its sealed decision.
        let batches = g.human_batches(Some(p), true);
        assert_eq!(batches.len(), held.len());
        assert!(batches
            .iter()
            .all(|b| g.ledger.record(0, 0, b.role as u8).map(|r| r.digest)
                == Some(b.decision_digest)));
        // Someone else's batches are not built (nor sealed) on their behalf.
        assert!(g.human_batches(Some(p + 1), true).is_empty());
    }

    #[test]
    fn advance_resolves_a_tick_and_keeps_queued_governance_until_then() {
        let mut g = lobby();
        let p = human(&mut g, 0, 0);
        g.start().unwrap();
        g.pending_gov.push(GovEntry {
            member: p,
            signer: local_key(p),
            action: GovAction::Stand {
                roles: Role::Steward.bit(),
            },
        });
        g.advance();
        assert_eq!(g.state.tick, 1);
        assert!(g.pending_gov.is_empty());
        assert_eq!(g.resolved_tick, Some(0));
        assert_eq!(
            g.state.members[p as usize].standing_for,
            Role::Steward.bit()
        );
    }
}
