//! Running the AI members this server hosts. (A vacant office is not run
//! here: the rules' caretaker fills it, `gov::caretaker`.)
//!
//! One planner per nation (the scripted `Bot`, deciding from the full
//! state: perfect information, `fog`) plans the whole nation's orders; each
//! office held by a hosted AI member then takes the part in its domain.
//! Offices held by people (or outside agents) are left to them.
//! Every office seals its own decision in the ledger and reveals it later
//! (V5 D17), exactly like a human officer, with the same policy id
//! (`ledger::DEFAULT_POLICY`).
//!
//! Hosted AI members also take part in governance: they vote in the vote
//! window (the incumbent if active, else the candidate with the most merit;
//! never by whether a candidate is a person, which would tell observers who
//! the operator's AI members are, V5 §18.2), join open recalls of idle officers,
//! support people's proposals, and propose orders from their nation's plan
//! (which an officer adopts when it issues the same order, V5 §5.4).

use std::borrow::Cow;

use crate::bots::{persona_of, Bot};
use crate::fog::Fog;
use crate::ledger::{Ledger, DEFAULT_POLICY};
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
    fn key(&self, m: MemberId) -> [u8; 32];
}

impl Planner {
    pub fn new(civs: usize) -> Planner {
        Planner::with_personas(&(0..civs as CivId).map(persona_of).collect::<Vec<_>>())
    }

    /// Temperaments drawn per nation from `seed` (V5 §18.8); the persona
    /// follows from them.
    pub fn seeded(civs: usize, seed: &[u8]) -> Planner {
        Planner {
            bots: (0..civs as CivId)
                .map(|c| Bot::with_traits(c, crate::bots::Traits::draw(seed, c)))
                .collect(),
            plans: vec![None; civs],
        }
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

    /// Nation `civ`'s plan for the open tick, made from `view` (`Fog::belief`).
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

    /// Batches for every office held by a hosted AI member, with their
    /// decision digests and pending reveals (a vacant office is filled by
    /// the rules' caretaker, `gov::caretaker`).
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
            let auto = |m: MemberId| m != NOBODY && host.is_ai(m);
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
                .filter_map(Order::war_target)
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
                    None => parts[Role::Diplomat.index()].retain(|o| o.war_target().is_none()),
                }
            }
            for role in Role::ALL {
                let member = offices[role.index()];
                if !auto(member) {
                    continue;
                }
                let mut orders = std::mem::take(&mut parts[role.index()]);
                let adopt = adopt_one(&view, rules, civ, role, &mut orders);
                // The same policy id as every other officer's (V5 §18.2).
                let digest = ledger.commit(
                    tick,
                    civ,
                    role as u8,
                    DEFAULT_POLICY,
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
        // Each nation's view (the full state), made when first needed: AI members
        // judge proposals and plan from what their nation can see.
        let mut views: Vec<Option<Cow<WorldState>>> = vec![None; state.civs.len()];
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
                    if state.vote(id, role) != NOBODY {
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
            // Vacant offices are filled by the rules' caretaker from the
            // members' top proposal (gov::caretaker): one of the nation's
            // hosted AI members, taking turns by tick (always the same one
            // would single it out, V5 §18.2), proposes the plan's orders for
            // every vacant office that has no open proposal yet.
            let ais: Vec<MemberId> = state
                .members
                .iter()
                .enumerate()
                .filter(|(x, mm)| mm.civ == m.civ && host.is_ai(*x as MemberId))
                .map(|(x, _)| x as MemberId)
                .collect();
            let first_ai = !ais.is_empty() && ais[tick as usize % ais.len()] == id;
            let vacant: Vec<Role> = Role::ALL
                .into_iter()
                .filter(|r| {
                    n.holder(*r) == NOBODY
                        && !n.proposals.iter().any(|p| p.role == *r && !p.adopted)
                })
                .collect();
            // A member may have `max_member_proposals` open at once.
            let mine = n.proposals.iter().filter(|p| p.proposer == id).count();
            let mut room = (rules.max_member_proposals as usize).saturating_sub(mine);
            if first_ai && !vacant.is_empty() && room > 0 {
                let view = views[m.civ as usize].get_or_insert_with(|| fog.belief(state, m.civ));
                let plan = self.plan(m.civ, view, rules, fog);
                let mut parts = split_by_office(view, plan);
                // A war declared through a proposal needs a consent adopted
                // in the same tick: propose it too when the general is vacant.
                let wars: Vec<CivId> = parts[Role::Diplomat.index()]
                    .iter()
                    .filter_map(Order::war_target)
                    .collect();
                parts[Role::General.index()]
                    .extend(wars.into_iter().map(|c| Order::ConsentWar { civ: c }));
                for r in vacant {
                    if room == 0 {
                        break;
                    }
                    // A proposal's shape (gov::proposal_shape_ok): no reveal,
                    // at most `max_proposal_orders` orders and
                    // `max_proposal_bytes` bytes; never a treasury order.
                    let mut orders: Vec<Order> = Vec::new();
                    for o in parts[r.index()]
                        .iter()
                        .filter(|o| o.is_action() && !o.is_treasury_order())
                    {
                        orders.push(o.clone());
                        if orders.len() > rules.max_proposal_orders as usize
                            || borsh::object_length(&orders).unwrap_or(usize::MAX)
                                > rules.max_proposal_bytes as usize
                        {
                            orders.pop();
                            break;
                        }
                    }
                    if !orders.is_empty() {
                        push(&mut out, GovAction::Propose { role: r, orders });
                        room -= 1;
                    }
                }
            }
            let in_office = n.offices.contains(&id);
            let proposes = !in_office && room > 0 && (tick as u32 + id).is_multiple_of(10);
            let open: Vec<&gov::GovProposal> = n
                .proposals
                .iter()
                .filter(|p| p.proposer != id && !p.supporters.contains(&id))
                .filter(|p| p.orders.iter().all(|o| bot_accepts(state, m.civ, o)))
                .collect();
            if open.is_empty() && !proposes {
                continue;
            }
            let view = views[m.civ as usize].get_or_insert_with(|| fog.belief(state, m.civ));
            // Support nation-mates' proposals that would still work now, up
            // to `max_member_supports` at once.
            let backing = n
                .proposals
                .iter()
                .filter(|p| p.supporters.contains(&id))
                .count();
            let can_back = (rules.max_member_supports as usize)
                .saturating_sub(backing)
                .min(2);
            for p in open
                .into_iter()
                .filter(|p| crate::api::preflight(view, rules, m.civ, &p.orders).is_empty())
                .take(can_back)
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
                        .find(|o| {
                            !matches!(o, Order::MoveUnit { .. })
                                && o.is_action()
                                && !o.is_treasury_order()
                        })
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
        // At most `MAX_GOV_PER_SIGNER` actions per member and tick (the
        // chain's inbox limit): the first ones (votes, then vacant offices'
        // proposals) are kept.
        let mut count: std::collections::BTreeMap<MemberId, usize> = Default::default();
        out.retain(|e| {
            let k = count.entry(e.member).or_default();
            *k += 1;
            *k <= permutation_chain::state::MAX_GOV_PER_SIGNER
        });
        out
    }
}

/// Whether an AI member backs (supports or adopts) a proposed order: never
/// one that moves treasury USDC (the rules refuse those anyway, V5 §7.5),
/// and a `Transfer` only to a nation in a NAP or an alliance, the bots' own
/// gift rule (WP10).
fn bot_accepts(state: &WorldState, civ: CivId, o: &Order) -> bool {
    match o {
        _ if o.is_treasury_order() => false,
        Order::Transfer { civ: to, .. } => {
            *to != civ && (*to as usize) < state.civs.len() && state.relation(civ, *to).is_pact()
        }
        _ => true,
    }
}

/// An office run here adopts the best open proposal for it (most support,
/// then the oldest; never by who proposed it, which would tell observers
/// who the operator's AI members are), if its orders would still work and fit the budget next
/// to the office's own plan; the plan yields the lowest-priority orders to
/// make room. Never a proposal that moves treasury USDC, nor a transfer to a
/// nation without a pact (`bot_accepts`). Returns the ids to put in the batch
/// (V5 §5.4).
fn adopt_one(
    view: &WorldState,
    rules: &Ruleset,
    civ: CivId,
    role: Role,
    orders: &mut Vec<Order>,
) -> Vec<u32> {
    let n = &view.nations[civ as usize];
    let budget = permutation_rules::orders::spendable(view, rules, civ, role) as usize;
    let best = n
        .proposals
        .iter()
        .filter(|p| p.role == role && !p.adopted && p.tick < view.tick)
        .filter(|p| p.orders.iter().all(|o| bot_accepts(view, civ, o)))
        .filter(|p| crate::api::preflight(view, rules, civ, &p.orders).is_empty())
        .filter(|p| p.orders.iter().map(|o| o.cost() as usize).sum::<usize>() <= budget)
        .max_by_key(|p| (p.supporters.len(), std::cmp::Reverse(p.id)));
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
/// every nation observes the world (perfect information), the AI members act in governance,
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

#[cfg(test)]
mod tests {
    use super::*;
    use permutation_rules::genesis::{nation_entries, new_season};
    use permutation_rules::gov::GovProposal;
    use permutation_rules::orders::{Good, Side};
    use permutation_rules::state::Relation;
    use permutation_rules::Preset;

    const USDC: u64 = 1_000_000;

    /// A proposal put straight into the world (bypassing `gov::propose`,
    /// which refuses treasury orders).
    fn inject(s: &mut WorldState, civ: usize, proposer: MemberId, orders: Vec<Order>) -> u32 {
        let n = &mut s.nations[civ];
        let id = n.next_proposal;
        n.next_proposal += 1;
        n.proposals.push(GovProposal {
            id,
            role: Role::Diplomat,
            proposer,
            tick: s.tick - 1,
            orders,
            supporters: Vec::new(),
            adopted: false,
        });
        id
    }

    fn supports(gov: &[GovEntry], id: u32) -> bool {
        gov.iter()
            .any(|e| matches!(e.action, GovAction::Support { proposal } if proposal == id))
    }

    #[test]
    fn ai_members_never_propose_support_or_adopt_treasury_orders() {
        let rules = Ruleset::new(Preset::Blitz);
        let mut entries = nation_entries(6);
        for e in &mut entries {
            e.treasury = 10 * USDC;
        }
        let mut s = new_season(&rules, &[5; 32], &[6; 32], &entries).unwrap();
        seat_ai_members(&mut s, &rules, &[3; 6]).unwrap();
        // Nation 0's diplomat office is vacant: its AIs propose for it.
        s.nations[0].offices[Role::Diplomat.index()] = NOBODY;
        let mut season = AiSeason::new(rules, s, Planner::new(6), Ledger::seeded(b"wp10"));
        let mut proposed = 0;
        while season.state.tick < 20 {
            let mut vrf = [0u8; 32];
            vrf[..2].copy_from_slice(&season.state.tick.to_le_bytes());
            let input = season.plan(vrf);
            for e in &input.gov {
                if let GovAction::Propose { orders, .. } = &e.action {
                    proposed += 1;
                    assert!(!orders.iter().any(Order::is_treasury_order), "{orders:?}");
                }
            }
            season.resolve(&input).unwrap();
        }
        assert!(proposed > 0);

        // Hand-made proposals in nation 1: a treasury buy, and a transfer
        // to nation 2 (no pact).
        let (rules, mut s) = (season.rules.clone(), season.state.clone());
        s.nations[1].proposals.clear();
        s.nations[1].role_bank = [20; 4];
        s.set_relation(1, 2, Relation::Peace);
        let proposer = (0..s.members.len() as MemberId)
            .find(|m| s.members[*m as usize].civ == 1 && !s.nations[1].offices.contains(m))
            .unwrap();
        let buy = Order::ExchangeOrder {
            good: Good::Iron,
            side: Side::Buy,
            amount: 1,
            price: USDC,
        };
        let gift = Order::Transfer {
            civ: 2,
            good: Good::Gold,
            amount: 1,
        };
        let treasury = inject(&mut s, 1, proposer, vec![buy]);
        let transfer = inject(&mut s, 1, proposer, vec![gift]);
        let fog = Fog::new(&s);
        let mut planner = Planner::new(6);
        let gov = planner.member_gov(&s, &rules, &fog, &AllAi);
        assert!(!supports(&gov, treasury) && !supports(&gov, transfer));
        assert!(adopt_one(&s, &rules, 1, Role::Diplomat, &mut Vec::new()).is_empty());
        // The same transfer to a NAP partner is backed and adopted.
        s.set_relation(
            1,
            2,
            Relation::Nap {
                until: 200,
                bond_low: 0,
                bond_high: 0,
            },
        );
        let fog = Fog::new(&s);
        let gov = planner.member_gov(&s, &rules, &fog, &AllAi);
        assert!(supports(&gov, transfer) && !supports(&gov, treasury));
        assert_eq!(
            adopt_one(&s, &rules, 1, Role::Diplomat, &mut Vec::new()),
            vec![transfer]
        );
    }
}
