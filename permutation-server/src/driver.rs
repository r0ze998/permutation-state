//! Running what no person runs: the acting official of every vacant office
//! (V5 §8, "代行") and the AI members this server hosts.
//!
//! One planner per nation (the scripted `Bot`, deciding from the nation's
//! fogged belief state) plans the whole nation's orders; each office then
//! takes the part in its domain, if the office is vacant or held by a hosted
//! AI member. Offices held by people (or outside agents) are left to them.
//! Every office seals its own decision in the ledger and reveals it later
//! (V5 D17), exactly like a human officer.
//!
//! Hosted AI members also take part in governance: they vote in the vote
//! window (the incumbent if active, else a human candidate, else the
//! candidate with the most merit), join open recalls of idle officers,
//! support people's proposals, and propose orders from their nation's plan
//! (which an officer adopts when it issues the same order, V5 §5.4).

use crate::bots::{persona_of, Bot};
use crate::fog::Fog;
use crate::ledger::Ledger;
use permutation_rules::decision::Hash;
use permutation_rules::gov::{self, GovAction, GovEntry, MemberId, Role, NOBODY};
use permutation_rules::orders::{split_by_office, Order, OrderBatch};
use permutation_rules::state::{CivId, WorldState};
use permutation_rules::tick::{resolve_tick, TickInput};
use permutation_rules::{RulesError, Ruleset};
use sha2::{Digest, Sha256};

/// The key a locally hosted member signs its governance actions with.
pub fn local_key(member: MemberId) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"PS/local-member-key");
    h.update(member.to_le_bytes());
    h.finalize().into()
}

pub struct Planner {
    pub bots: Vec<Bot>,
    /// Each nation's plan for the open tick. A bot plans once per tick: its
    /// plan updates its own memory (wars it started), so planning twice
    /// would not be the same decision.
    plans: Vec<Option<(u16, Vec<Order>)>>,
}

/// Who runs each member's decisions.
pub trait Hosting {
    /// Orders and governance of this member are made by this server's AI.
    fn is_ai(&self, m: MemberId) -> bool;
    /// A person (for AI deference in votes).
    fn is_human(&self, m: MemberId) -> bool;
    fn key(&self, m: MemberId) -> [u8; 32];
}

impl Planner {
    pub fn new(civs: usize) -> Planner {
        Planner::with_personas(&(0..civs as CivId).map(persona_of).collect::<Vec<_>>())
    }

    pub fn with_personas(personas: &[crate::bots::Persona]) -> Planner {
        Planner {
            bots: personas
                .iter()
                .enumerate()
                .map(|(c, p)| Bot::new(c as CivId, *p))
                .collect(),
            plans: vec![None; personas.len()],
        }
    }

    /// Nation `civ`'s plan for the open tick, made from its belief `view`.
    fn plan(&mut self, civ: CivId, view: &WorldState, rules: &Ruleset, fog: &Fog) -> Vec<Order> {
        let slot = &mut self.plans[civ as usize];
        match slot {
            Some((tick, orders)) if *tick == view.tick => orders.clone(),
            _ => {
                let orders = self.bots[civ as usize].orders(view, rules, &fog.memory(civ).explored);
                *slot = Some((view.tick, orders.clone()));
                orders
            }
        }
    }

    /// Batches for every office run by the acting official or a hosted AI
    /// member, with their decision digests and pending reveals.
    pub fn batches(
        &mut self,
        state: &WorldState,
        rules: &Ruleset,
        fog: &Fog,
        ledger: &mut Ledger,
        host: &dyn Hosting,
    ) -> Vec<OrderBatch> {
        let tick = state.tick;
        let mut out = Vec::new();
        for civ in 0..state.civs.len() as CivId {
            let offices = state.nations[civ as usize].offices;
            let auto = |m: MemberId| m == NOBODY || host.is_ai(m);
            if !offices.iter().any(|m| auto(*m)) {
                continue;
            }
            let view = fog.belief(state, civ);
            let orders = self.plan(civ, &view, rules, fog);
            let bot = &self.bots[civ as usize];
            let mut parts = split_by_office(&view, orders);
            // War needs a second officer (V5 §5.6): the hosted general or
            // steward consents if it is someone other than the diplomat.
            let diplomat = offices[Role::Diplomat.index()];
            let wars: Vec<CivId> = parts[Role::Diplomat.index()]
                .iter()
                .filter_map(|o| match o {
                    Order::DeclareWar { civ } | Order::BreakNap { civ } => Some(*civ),
                    _ => None,
                })
                .collect();
            if !wars.is_empty() {
                let consenter = [Role::General, Role::Steward].into_iter().find(|r| {
                    let m = offices[r.index()];
                    auto(m) && (m != diplomat || m == NOBODY)
                });
                match consenter {
                    Some(r) => {
                        parts[r.index()].extend(wars.iter().map(|c| Order::ConsentWar { civ: *c }))
                    }
                    None => parts[Role::Diplomat.index()].retain(|o| {
                        !matches!(o, Order::DeclareWar { .. } | Order::BreakNap { .. })
                    }),
                }
            }
            for role in Role::ALL {
                let member = offices[role.index()];
                if !auto(member) {
                    continue;
                }
                let mut orders = std::mem::take(&mut parts[role.index()]);
                let adopt = adopt_one(&view, rules, civ, role, &mut orders, host);
                let policy = format!(
                    "{}/{}@2",
                    if member == NOBODY { "acting" } else { "agent" },
                    bot.persona.name().to_lowercase()
                );
                let digest = ledger.commit(
                    tick,
                    civ,
                    role as u8,
                    &policy,
                    &bot.rationale(&view, &orders),
                );
                orders.extend(ledger.reveals(civ, role as u8, tick, &orders));
                out.push(OrderBatch {
                    civ,
                    tick,
                    role,
                    member,
                    decision_digest: digest,
                    orders,
                    adopt,
                });
            }
        }
        out
    }

    /// Governance actions of the hosted AI members for this tick.
    pub fn member_gov(
        &mut self,
        state: &WorldState,
        rules: &Ruleset,
        fog: &Fog,
        host: &dyn Hosting,
    ) -> Vec<GovEntry> {
        let tick = state.tick;
        let mut out = Vec::new();
        let vote_open = gov::vote_open(rules, tick);
        // Each nation's fogged view, made when first needed: AI members
        // judge proposals and plan from what their nation can see.
        let mut views: Vec<Option<WorldState>> = vec![None; state.civs.len()];
        for (id, m) in state.members.iter().enumerate() {
            let id = id as MemberId;
            if !host.is_ai(id) {
                continue;
            }
            let n = &state.nations[m.civ as usize];
            let push = |out: &mut Vec<GovEntry>, action| {
                out.push(GovEntry {
                    member: id,
                    signer: host.key(id),
                    action,
                })
            };
            if vote_open {
                for role in Role::ALL {
                    if n.votes.iter().any(|v| v.voter == id && v.role == role) {
                        continue;
                    }
                    let candidates: Vec<MemberId> = state
                        .members
                        .iter()
                        .enumerate()
                        .filter(|(_, x)| x.civ == m.civ && x.standing_for & role.bit() != 0)
                        .map(|(c, _)| c as MemberId)
                        .collect();
                    let holder = n.holder(role);
                    let incumbent = candidates.contains(&holder)
                        && gov::active_officer(state, rules, m.civ, role).is_some();
                    let pick = if incumbent {
                        Some(holder)
                    } else if let Some(h) = candidates.iter().copied().find(|c| host.is_human(*c)) {
                        Some(h)
                    } else {
                        candidates.iter().copied().max_by_key(|c| {
                            (
                                state.members[*c as usize].merit_total(),
                                std::cmp::Reverse(*c),
                            )
                        })
                    };
                    if let Some(candidate) = pick {
                        push(&mut out, GovAction::Vote { role, candidate });
                    }
                }
            }
            for r in &n.recalls {
                if r.automatic && r.holder != id && !r.yes.contains(&id) {
                    push(&mut out, GovAction::Recall { role: r.role });
                }
            }
            let in_office = n.offices.contains(&id);
            let proposes = !in_office && (tick as u32 + id).is_multiple_of(10);
            let open: Vec<&gov::GovProposal> = n
                .proposals
                .iter()
                .filter(|p| p.proposer != id && !p.supporters.contains(&id))
                .collect();
            if open.is_empty() && !proposes {
                continue;
            }
            let view = views[m.civ as usize].get_or_insert_with(|| fog.belief(state, m.civ));
            // Support nation-mates' proposals that would still work now.
            for p in open
                .into_iter()
                .filter(|p| crate::api::preflight(view, rules, m.civ, &p.orders).is_empty())
                .take(2)
            {
                push(&mut out, GovAction::Support { proposal: p.id });
            }
            // A member without office proposes from its nation's plan now and then.
            if proposes {
                let plan = self.plan(m.civ, view, rules, fog);
                let parts = split_by_office(view, plan);
                if let Some((role, order)) = Role::ALL.into_iter().find_map(|r| {
                    parts[r.index()]
                        .iter()
                        .find(|o| !matches!(o, Order::MoveUnit { .. }))
                        .map(|o| (r, o.clone()))
                }) {
                    push(
                        &mut out,
                        GovAction::Propose {
                            role,
                            orders: vec![order],
                        },
                    );
                }
            }
        }
        out
    }
}

/// An office run here adopts the best open proposal for it (most support,
/// people's first), if its orders would still work and fit the budget next
/// to the office's own plan; the plan yields the lowest-priority orders to
/// make room. Returns the ids to put in the batch (V5 §5.4).
fn adopt_one(
    view: &WorldState,
    rules: &Ruleset,
    civ: CivId,
    role: Role,
    orders: &mut Vec<Order>,
    host: &dyn Hosting,
) -> Vec<u32> {
    let n = &view.nations[civ as usize];
    let budget = permutation_rules::orders::spendable(view, rules, civ, role) as usize;
    let best = n
        .proposals
        .iter()
        .filter(|p| p.role == role && !p.adopted && p.tick < view.tick)
        .filter(|p| !p.supporters.is_empty() || host.is_human(p.proposer))
        .filter(|p| crate::api::preflight(view, rules, civ, &p.orders).is_empty())
        .filter(|p| p.orders.iter().map(|o| o.cost() as usize).sum::<usize>() <= budget)
        .max_by_key(|p| {
            (
                host.is_human(p.proposer),
                p.supporters.len(),
                std::cmp::Reverse(p.id),
            )
        });
    let Some(p) = best else { return Vec::new() };
    let units: Vec<u32> = p.orders.iter().filter_map(Order::commanded_unit).collect();
    orders.retain(|o| {
        o.commanded_unit().is_none_or(|u| !units.contains(&u)) && !p.orders.contains(o)
    });
    let adopted: usize = p.orders.iter().map(|o| o.cost() as usize).sum();
    while orders.iter().map(|o| o.cost() as usize).sum::<usize>() + adopted > budget {
        orders.pop();
    }
    vec![p.id]
}

/// Hosting where every member is this server's AI (sim, replay, benches).
pub struct AllAi;

impl Hosting for AllAi {
    fn is_ai(&self, _: MemberId) -> bool {
        true
    }
    fn is_human(&self, _: MemberId) -> bool {
        false
    }
    fn key(&self, m: MemberId) -> [u8; 32] {
        local_key(m)
    }
}

/// Register `members[c]` AI members in nation `c` (keys from `local_key`),
/// each standing for two offices (rotating, so every office has candidates)
/// and voting for itself, then hold the first election.
pub fn seat_ai_members(
    state: &mut WorldState,
    rules: &Ruleset,
    members: &[usize],
) -> Result<(), permutation_rules::RulesError> {
    let mut pre = Vec::new();
    for (civ, k) in members.iter().enumerate().take(state.civs.len()) {
        for j in 0..*k {
            let next = state.members.len() as MemberId;
            let id = gov::join(state, rules, civ as CivId, local_key(next))?;
            let roles = [Role::ALL[(2 * j) % 4], Role::ALL[(2 * j + 1) % 4]];
            let key = local_key(id);
            pre.push(GovEntry {
                member: id,
                signer: key,
                action: GovAction::Stand {
                    roles: roles[0].bit() | roles[1].bit(),
                },
            });
            for r in roles {
                pre.push(GovEntry {
                    member: id,
                    signer: key,
                    action: GovAction::Vote {
                        role: r,
                        candidate: id,
                    },
                });
            }
        }
    }
    gov::open_government(state, rules, &pre)
}

/// A season run entirely by this server's AI: the loop the simulation, the
/// replay recorder, the tick logger and the golden test share. Each tick,
/// every nation observes its fogged view, the AI members act in governance,
/// every office plans and seals its decision, and the engine resolves.
pub struct AiSeason {
    pub rules: Ruleset,
    pub state: WorldState,
    pub planner: Planner,
    pub fog: Fog,
    pub ledger: Ledger,
}

impl AiSeason {
    /// `state` is the opening state, with the members seated.
    pub fn new(rules: Ruleset, state: WorldState, planner: Planner, ledger: Ledger) -> AiSeason {
        let fog = Fog::new(&state);
        AiSeason {
            rules,
            state,
            planner,
            fog,
            ledger,
        }
    }

    pub fn over(&self) -> bool {
        self.state.tick >= self.rules.ticks_per_season
    }

    /// The open tick's input, with `vrf` as its randomness: records the
    /// observations, then the AI members' governance and every office's batch.
    pub fn plan(&mut self, vrf: [u8; 32]) -> TickInput {
        self.ledger.observe(&self.state, &self.fog);
        let gov = self
            .planner
            .member_gov(&self.state, &self.rules, &self.fog, &AllAi);
        let batches = self.planner.batches(
            &self.state,
            &self.rules,
            &self.fog,
            &mut self.ledger,
            &AllAi,
        );
        TickInput {
            vrf,
            batches,
            gov,
            deposits: Vec::new(),
        }
    }

    /// Resolve the open tick with `input` (from `plan`) and refresh the fog.
    /// Returns the new state root.
    pub fn resolve(&mut self, input: &TickInput) -> Result<Hash, RulesError> {
        let root = resolve_tick(&mut self.state, &self.rules, input)?;
        self.fog.update(&self.state);
        Ok(root)
    }
}
