//! The caretaker of a vacant office (V5 §5.3, revised 2026-09-25).
//!
//! A vacant office (no candidate, or recalled with no runner-up) used to be
//! run by the operator's key, the "acting official", which let the operator
//! choose a nation's moves. Now nobody submits for it: the rules fill it
//! every tick, deterministically, from what is already on chain:
//!
//! 1. **The members' will.** The open proposal for this office with the
//!    most supporters (made on an earlier tick; the proposer counts as one,
//!    so a lone member's proposal runs too; ties: lowest id) is adopted. Its
//!    proposer earns the merit as usual (the office's half goes to nobody).
//!    A proposal to go to war (declare it, break a pact, or consent to one)
//!    needs a supporter besides its proposer: war takes two people.
//! 2. **Otherwise a minimal default** that keeps the nation alive without
//!    choosing a strategy: the science office queues the cheapest
//!    researchable tech when the queue is empty; the steward queues the first
//!    missing basic building (else two spearmen) in cities with an empty
//!    queue; the diplomat accepts peace offered to the nation. The general
//!    and the diplomat start nothing (no moves, wars, treaties or trades).
//!
//! Every order goes through the same validation and budget as an officer's
//! batch; what does not fit the office's budget is dropped from the end.

use crate::buildings::Building;
use crate::checks;
use crate::gov::{Role, NOBODY};
use crate::orders::{validate_batch, Order, OrderBatch};
use crate::params::Ruleset;
use crate::state::{CivId, ProposalKind, QueueItem, WorldState};
use crate::tech::TECHS;
use crate::units::UnitType;
use alloc::vec;
use alloc::vec::Vec;

/// Basic buildings the steward's default queues, in order.
const DEFAULT_BUILDINGS: [Building; 6] = [
    Building::Granary,
    Building::Workshop,
    Building::Walls,
    Building::Market,
    Building::Academy,
    Building::Temple,
];

/// The caretaker's batch for `role` of `civ`, and its cost, if it does
/// anything. `None` when the office is held or there is nothing to do.
pub fn batch(
    state: &WorldState,
    rules: &Ruleset,
    civ: CivId,
    role: Role,
) -> Option<(OrderBatch, u32)> {
    let nation = state.nations.get(civ as usize)?;
    if nation.holder(role) != NOBODY {
        return None;
    }
    let base = OrderBatch {
        civ,
        tick: state.tick,
        role,
        member: NOBODY,
        decision_digest: [0; 32],
        orders: Vec::new(),
        adopt: Vec::new(),
    };
    // 1. The most-supported proposal for this office.
    let top = nation
        .proposals
        .iter()
        .filter(|p| p.role == role && !p.adopted && p.tick < state.tick)
        // War (declaring it, breaking a pact) needs two people (V5 §5.6):
        // such a proposal runs only with a supporter besides its proposer.
        .filter(|p| {
            !p.orders.iter().any(|o| {
                matches!(
                    o,
                    Order::DeclareWar { .. } | Order::BreakNap { .. } | Order::ConsentWar { .. }
                )
            }) || p.supporters.iter().any(|m| *m != p.proposer)
        })
        .max_by_key(|p| (p.supporters.len(), core::cmp::Reverse(p.id)));
    if let Some(p) = top {
        let b = OrderBatch {
            adopt: vec![p.id],
            ..base.clone()
        };
        if let Ok(cost) = validate_batch(state, rules, &b) {
            return Some((b, cost));
        }
    }
    // 2. The default, trimmed to the budget.
    let mut orders = default_orders(state, civ, role);
    while !orders.is_empty() {
        let b = OrderBatch {
            orders: orders.clone(),
            ..base.clone()
        };
        if let Ok(cost) = validate_batch(state, rules, &b) {
            return Some((b, cost));
        }
        orders.pop();
    }
    None
}

fn default_orders(state: &WorldState, civ: CivId, role: Role) -> Vec<Order> {
    let me = &state.civs[civ as usize];
    match role {
        Role::Science => {
            if !me.research_queue.is_empty() {
                return Vec::new();
            }
            TECHS
                .iter()
                .enumerate()
                .filter(|(_, t)| checks::research(me.techs, t.tech).is_ok())
                .min_by_key(|(i, t)| (t.base_cost, *i))
                .map(|(_, t)| {
                    vec![Order::SetResearch {
                        techs: vec![t.tech],
                    }]
                })
                .unwrap_or_default()
        }
        Role::Steward => state
            .living_cities_of(civ)
            .filter(|c| c.queue.is_empty() && c.razing.is_none())
            .map(|c| {
                let item = DEFAULT_BUILDINGS
                    .iter()
                    .map(|b| QueueItem::Building(*b))
                    .find(|it| checks::queue_item(state, civ, c.id, it).is_ok())
                    .unwrap_or(QueueItem::Troops {
                        unit: UnitType::Spearman,
                        n: 2,
                    });
                Order::SetQueue {
                    city: c.id,
                    items: vec![item],
                }
            })
            .collect(),
        Role::Diplomat => state
            .proposals
            .iter()
            .filter(|p| p.to == civ && p.tick < state.tick && p.kind == ProposalKind::Peace)
            .map(|p| Order::AcceptPeace { civ: p.from })
            .collect(),
        Role::General => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use crate::genesis::{nation_entries, new_season};
    use crate::gov::{join, GovAction, GovEntry, Role, NOBODY};
    use crate::orders::{Order, OrderBatch};
    use crate::params::{Preset, Ruleset};
    use crate::state::QueueItem;
    use crate::tech::Tech;
    use crate::tick::{resolve_tick, TickInput};
    use alloc::vec;
    use alloc::vec::Vec;

    fn input(batches: Vec<OrderBatch>, gov: Vec<GovEntry>) -> TickInput {
        TickInput {
            vrf: [0; 32],
            batches,
            gov,
            deposits: Vec::new(),
        }
    }

    /// With every office vacant, nobody's batch is taken (the operator's
    /// neither), and the default keeps the nation researching and building.
    #[test]
    fn a_vacant_office_takes_no_batch_and_runs_the_default() {
        let rules = Ruleset::new(Preset::Blitz);
        let mut s = new_season(&rules, &[1; 32], &[2; 32], &nation_entries(6)).unwrap();
        assert!(s.nations[0].offices.iter().all(|o| *o == NOBODY));
        let scout = s.units.iter().find(|u| u.owner.civ() == Some(0)).unwrap();
        let (id, from) = (scout.id, scout.hex);
        let to = from.neighbors()[0];
        let outside = OrderBatch {
            civ: 0,
            tick: 0,
            role: Role::General,
            member: NOBODY,
            decision_digest: [0; 32],
            orders: vec![Order::MoveUnit {
                unit: id,
                path: vec![to],
            }],
            adopt: Vec::new(),
        };
        resolve_tick(&mut s, &rules, &input(vec![outside], Vec::new())).unwrap();
        assert_eq!(
            s.units[id as usize].hex, from,
            "the outside batch was ignored"
        );
        let cap = s.civs[0].capital.unwrap() as usize;
        assert_eq!(
            s.cities[cap].queue.first(),
            Some(&QueueItem::Building(crate::buildings::Building::Granary))
        );
        // Cheapest first, ties by tech order: Agriculture.
        assert!(
            s.civs[0].research_queue.first() == Some(&Tech::Agriculture)
                || s.civs[0].techs.has(Tech::Agriculture)
        );
    }

    /// A war proposal needs a supporter besides its proposer: a lone member
    /// cannot take a nation with a vacant diplomat to war.
    #[test]
    fn a_war_proposal_needs_two_people() {
        let rules = Ruleset::new(Preset::Blitz);
        let mut s = new_season(&rules, &[1; 32], &[2; 32], &nation_entries(6)).unwrap();
        let a = join(&mut s, &rules, 0, [7; 32]).unwrap();
        let b = join(&mut s, &rules, 0, [8; 32]).unwrap();
        let war = GovAction::Propose {
            role: Role::Diplomat,
            orders: vec![Order::DeclareWar { civ: 1 }],
        };
        let entry = |member, key, action| GovEntry {
            member,
            signer: key,
            action,
        };
        resolve_tick(
            &mut s,
            &rules,
            &input(Vec::new(), vec![entry(a, [7; 32], war)]),
        )
        .unwrap();
        let id = s.nations[0].proposals[0].id;
        assert!(
            super::batch(&s, &rules, 0, Role::Diplomat).is_none_or(|(b, _)| b.adopt.is_empty()),
            "alone: not adopted"
        );
        assert_eq!(s.nations[0].adopted, 0);
        // Supported by a second member, it is adopted in that very tick
        // (support counts in phase 0, before the caretaker's batch).
        resolve_tick(
            &mut s,
            &rules,
            &input(
                Vec::new(),
                vec![entry(b, [8; 32], GovAction::Support { proposal: id })],
            ),
        )
        .unwrap();
        assert_eq!(s.nations[0].adopted, 1);
    }

    /// The members' most-supported proposal runs in a vacant office.
    #[test]
    fn a_vacant_office_adopts_the_top_proposal() {
        let rules = Ruleset::new(Preset::Blitz);
        let mut s = new_season(&rules, &[1; 32], &[2; 32], &nation_entries(6)).unwrap();
        let a = join(&mut s, &rules, 0, [7; 32]).unwrap();
        let b = join(&mut s, &rules, 0, [8; 32]).unwrap();
        let entry = |member, key, action| GovEntry {
            member,
            signer: key,
            action,
        };
        let research = |t| GovAction::Propose {
            role: Role::Science,
            orders: vec![Order::SetResearch { techs: vec![t] }],
        };
        resolve_tick(
            &mut s,
            &rules,
            &input(
                Vec::new(),
                vec![
                    entry(a, [7; 32], research(Tech::Archery)),
                    entry(b, [8; 32], research(Tech::BronzeWorking)),
                ],
            ),
        )
        .unwrap();
        let ids: Vec<u32> = s.nations[0].proposals.iter().map(|p| p.id).collect();
        let bronze = s.nations[0]
            .proposals
            .iter()
            .find(|p| {
                p.orders.contains(&Order::SetResearch {
                    techs: vec![Tech::BronzeWorking],
                })
            })
            .map(|p| p.id)
            .expect("proposal kept");
        assert_eq!(ids.len(), 2);
        // Both members back bronze working: it has the most supporters.
        resolve_tick(
            &mut s,
            &rules,
            &input(
                Vec::new(),
                vec![
                    entry(a, [7; 32], GovAction::Support { proposal: bronze }),
                    entry(b, [8; 32], GovAction::Support { proposal: bronze }),
                ],
            ),
        )
        .unwrap();
        assert!(
            s.civs[0].research_queue.first() == Some(&Tech::BronzeWorking)
                || s.civs[0].techs.has(Tech::BronzeWorking),
            "queue {:?}",
            s.civs[0].research_queue
        );
    }
}
