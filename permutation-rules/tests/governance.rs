//! Nations, offices and governance (V5 §4–§5): elections, proposals,
//! recalls, consents, office budgets, and the merit and achievements they
//! earn (V5 §6–§7).

mod common;
use common::nations::*;
use permutation_rules::buildings::Building;
use permutation_rules::checks::Blocked;
use permutation_rules::gov::{self, GovAction, GovEntry, Path, Role, NOBODY, NO_VOTE};
use permutation_rules::orders::{validate_batch, Order, OrderBatch};
use permutation_rules::state::{Focus, QueueItem};
use permutation_rules::tech::Tech;
use permutation_rules::RulesError;

// ------------------------------------------------------------------ governance

#[test]
fn the_first_election_fills_offices_with_at_most_two_each() {
    let (rules, mut s) = world(&[3, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    // Member 0 stands for everything and gets every vote: it may hold only two.
    elect(&s, &mut e, 0, &Role::ALL);
    e.push(act(
        1,
        GovAction::Stand {
            roles: Role::Science.bit() | Role::Diplomat.bit(),
        },
    ));
    gov::open_government(&mut s, &rules, &e).unwrap();
    let n = &s.nations[0];
    assert_eq!(n.holder(Role::General), 0);
    assert_eq!(n.holder(Role::Steward), 0);
    assert_eq!(
        n.holder(Role::Science),
        1,
        "member 0 already holds two offices"
    );
    assert_eq!(n.holder(Role::Diplomat), 1);
    assert_eq!(
        s.nations[1].offices, [NOBODY; 4],
        "no members: the acting official"
    );
    // Voting in the first election made the voters active in window 0.
    assert!(s.members.iter().all(|m| m.windows & 1 == 1));
}

#[test]
fn only_the_office_holder_can_order_and_must_seal_a_rationale() {
    let (rules, mut s) = world(&[2, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::Steward]);
    gov::open_government(&mut s, &rules, &e).unwrap();
    let capital = s.civs[0].capital.unwrap();
    let focus = Order::SetFocus {
        city: capital,
        focus: Focus::Food,
    };
    let not_holder = batch(&s, 0, Role::Steward, 1, vec![focus.clone()]);
    assert_eq!(
        validate_batch(&s, &rules, &not_holder),
        Err(RulesError::NotOfficer)
    );
    let acting = OrderBatch {
        member: NOBODY,
        ..batch(&s, 0, Role::Steward, 0, vec![focus.clone()])
    };
    assert_eq!(
        validate_batch(&s, &rules, &acting),
        Err(RulesError::NotOfficer),
        "the office is not vacant"
    );
    let mut unsealed = batch(&s, 0, Role::Steward, 0, vec![focus.clone()]);
    unsealed.decision_digest = [0; 32];
    assert_eq!(
        validate_batch(&s, &rules, &unsealed),
        Err(RulesError::MissingRationale)
    );
    let research = batch(
        &s,
        0,
        Role::Steward,
        0,
        vec![Order::SetResearch {
            techs: vec![Tech::Agriculture],
        }],
    );
    assert_eq!(
        validate_batch(&s, &rules, &research),
        Err(RulesError::WrongOffice(0))
    );
    let ok = batch(&s, 0, Role::Steward, 0, vec![focus]);
    assert_eq!(validate_batch(&s, &rules, &ok), Ok(1));
    step(&mut s, &rules, vec![ok], vec![]);
    assert_eq!(s.cities[capital as usize].focus, Focus::Food);
    assert_eq!(s.nations[0].office_last_act[Role::Steward.index()], 0);
    assert_eq!(
        s.members[0].merit[Path::Common as usize],
        rules.merit_office_tick,
        "one office tick"
    );
}

#[test]
fn unit_orders_follow_the_unit_type() {
    let (rules, mut s) = world(&[]);
    common::staff(&mut s);
    let steward = s.nations[0].holder(Role::Steward);
    let spear = s
        .units
        .iter()
        .find(|u| u.owner == permutation_rules::state::Owner::Civ(0) && !u.unit_type.is_civilian())
        .unwrap()
        .id;
    let hex = s.units[spear as usize].hex;
    let next = hex
        .neighbors()
        .into_iter()
        .find(|h| s.map.tile(*h).is_some_and(|t| t.terrain.is_passable()))
        .unwrap();
    // The steward cannot move an army: the order is dropped and recorded.
    let b = batch(
        &s,
        0,
        Role::Steward,
        steward,
        vec![Order::MoveUnit {
            unit: spear,
            path: vec![next],
        }],
    );
    step(&mut s, &rules, vec![b], vec![]);
    assert_eq!(s.units[spear as usize].hex, hex);
    assert!(s
        .last_skipped
        .iter()
        .any(|k| k.reason == Blocked::WrongOffice.code()));
}

#[test]
fn budgets_are_split_by_office_and_banked_per_office() {
    let (rules, mut s) = world(&[]);
    // Held offices that stay idle bank their share (a vacant one's
    // caretaker would spend it).
    common::staff(&mut s);
    let steward = s.nations[0].holder(Role::Steward);
    assert_eq!(s.civs[0].tick_budget, 4);
    idle(&mut s, &rules, 2);
    // Two idle ticks: each office banked its own share (1 each at B = 4).
    assert_eq!(s.nations[0].role_bank, [2, 2, 2, 2]);
    let capital = s.civs[0].capital.unwrap();
    let three = vec![
        Order::SetFocus {
            city: capital,
            focus: Focus::Food,
        },
        Order::SetFocus {
            city: capital,
            focus: Focus::Gold,
        },
        Order::SetFocus {
            city: capital,
            focus: Focus::Production,
        },
    ];
    let b = batch(&s, 0, Role::Steward, steward, three);
    assert_eq!(
        validate_batch(&s, &rules, &b),
        Ok(3),
        "1 this tick + 2 banked"
    );
    step(&mut s, &rules, vec![b], vec![]);
    assert_eq!(s.cities[capital as usize].focus, Focus::Production);
    assert_eq!(s.nations[0].role_bank[Role::Steward.index()], 0);
}

#[test]
fn war_needs_a_second_officer() {
    let (rules, mut s) = world(&[2, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::Diplomat, Role::General]);
    elect(&s, &mut e, 1, &[Role::Steward]);
    gov::open_government(&mut s, &rules, &e).unwrap();
    let declare = batch(&s, 0, Role::Diplomat, 0, vec![Order::DeclareWar { civ: 1 }]);
    // Consent from the same person (who is also the general) does not count.
    let self_consent = batch(&s, 0, Role::General, 0, vec![Order::ConsentWar { civ: 1 }]);
    let mut a = s.clone();
    step(&mut a, &rules, vec![declare.clone(), self_consent], vec![]);
    assert!(matches!(
        a.relation(0, 1),
        permutation_rules::state::Relation::Peace
    ));
    assert!(a
        .last_skipped
        .iter()
        .any(|k| k.reason == Blocked::NeedsConsent.code()));
    // The steward, a different member, consents: war.
    let consent = batch(&s, 0, Role::Steward, 1, vec![Order::ConsentWar { civ: 1 }]);
    step(&mut s, &rules, vec![declare, consent], vec![]);
    assert!(matches!(
        s.relation(0, 1),
        permutation_rules::state::Relation::War { .. }
    ));
}

#[test]
fn an_adopted_proposal_shares_its_merit_with_the_proposer() {
    let (rules, mut s) = world(&[2, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::Science]);
    gov::open_government(&mut s, &rules, &e).unwrap();
    // Member 1 proposes research; it is adopted by id the next tick.
    let research = Order::SetResearch {
        techs: vec![Tech::Agriculture],
    };
    step(
        &mut s,
        &rules,
        vec![],
        vec![act(
            1,
            GovAction::Propose {
                role: Role::Science,
                orders: vec![research],
            },
        )],
    );
    let id = s.nations[0].proposals[0].id;
    let mut b = batch(&s, 0, Role::Science, 0, vec![]);
    b.adopt = vec![id];
    step(&mut s, &rules, vec![b], vec![]);
    assert!(
        s.nations[0].proposals.is_empty(),
        "adopted proposals are removed"
    );
    assert_eq!(s.civs[0].research_queue, vec![Tech::Agriculture]);
    // Run until Agriculture completes; its merit is split half and half.
    while !s.civs[0].techs.has(Tech::Agriculture) {
        step(&mut s, &rules, vec![], vec![]);
    }
    let sci = |m: usize| s.members[m].merit[Path::Science as usize];
    assert!(sci(1) > 0);
    assert!(
        sci(0) >= sci(1) && sci(0) - sci(1) <= 1,
        "half each: {} vs {}",
        sci(0),
        sci(1)
    );
}

#[test]
fn copying_a_supported_proposal_counts_as_adopting_it() {
    let (rules, mut s) = world(&[3, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::Steward]);
    gov::open_government(&mut s, &rules, &e).unwrap();
    let capital = s.civs[0].capital.unwrap();
    let order = Order::SetQueue {
        city: capital,
        items: vec![QueueItem::Building(Building::Granary)],
    };
    step(
        &mut s,
        &rules,
        vec![],
        vec![act(
            1,
            GovAction::Propose {
                role: Role::Steward,
                orders: vec![order.clone()],
            },
        )],
    );
    let id = s.nations[0].proposals[0].id;
    step(
        &mut s,
        &rules,
        vec![],
        vec![act(2, GovAction::Support { proposal: id })],
    );
    // The steward issues the same order without citing the proposal.
    let b = batch(&s, 0, Role::Steward, 0, vec![order]);
    step(&mut s, &rules, vec![b], vec![]);
    assert_eq!(
        s.cities[capital as usize].queue_credit,
        gov::Credit {
            officer: 0,
            proposer: 1
        }
    );
}

#[test]
fn a_majority_of_active_members_recalls_an_officer() {
    let (rules, mut s) = world(&[3, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::General]);
    e.push(act(
        1,
        GovAction::Stand {
            roles: Role::General.bit(),
        },
    ));
    gov::open_government(&mut s, &rules, &e).unwrap();
    assert_eq!(s.nations[0].runner_up[Role::General.index()], 1);
    // Members 1 and 2 vote to recall: 2 of 3 recently active members.
    step(
        &mut s,
        &rules,
        vec![],
        vec![
            act(
                1,
                GovAction::Recall {
                    role: Role::General,
                },
            ),
            act(
                2,
                GovAction::Recall {
                    role: Role::General,
                },
            ),
        ],
    );
    assert_eq!(
        s.nations[0].holder(Role::General),
        1,
        "the runner-up takes over next tick"
    );
}

#[test]
fn an_idle_officer_faces_an_automatic_recall() {
    let (rules, mut s) = world(&[2, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::Science]);
    gov::open_government(&mut s, &rules, &e).unwrap();
    idle(&mut s, &rules, rules.idle_recall_ticks - 1);
    assert!(s.nations[0].recalls.is_empty());
    idle(&mut s, &rules, 1);
    let r = &s.nations[0].recalls[0];
    assert!(r.automatic && r.role == Role::Science && r.yes.is_empty());
}

#[test]
fn an_officer_who_seals_empty_batches_is_not_idle() {
    // The automatic recall is for abandoned offices: an officer who seals a
    // batch every tick (nothing to order, e.g. a full research queue) stays.
    let (rules, mut s) = world(&[2, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::Science]);
    gov::open_government(&mut s, &rules, &e).unwrap();
    for _ in 0..rules.idle_recall_ticks + 5 {
        let b = batch(&s, 0, Role::Science, 0, vec![]);
        step(&mut s, &rules, vec![b], vec![]);
    }
    assert!(s.nations[0].recalls.is_empty());
    assert_eq!(s.nations[0].holder(Role::Science), 0);
    // It never acted, so it earns no "active officer" merit.
    assert_eq!(gov::active_officer(&s, &rules, 0, Role::Science), None);
}

#[test]
fn elections_run_every_term_with_votes_from_the_window() {
    let (rules, mut s) = world(&[2, 0, 0, 0, 0, 0]);
    gov::open_government(&mut s, &rules, &[]).unwrap();
    assert_eq!(s.nations[0].offices, [NOBODY; 4]);
    // A vote before the window is ignored; standing is remembered.
    step(
        &mut s,
        &rules,
        vec![],
        vec![
            act(
                1,
                GovAction::Stand {
                    roles: Role::Diplomat.bit(),
                },
            ),
            act(
                0,
                GovAction::Vote {
                    role: Role::Diplomat,
                    candidate: 1,
                },
            ),
        ],
    );
    assert!(s.ballots.iter().all(|b| *b == [NO_VOTE; 4]));
    while s.tick < rules.term_ticks - rules.vote_window {
        step(&mut s, &rules, vec![], vec![]);
    }
    step(
        &mut s,
        &rules,
        vec![],
        vec![act(
            0,
            GovAction::Vote {
                role: Role::Diplomat,
                candidate: 1,
            },
        )],
    );
    assert_eq!(s.vote(0, Role::Diplomat), 1);
    while s.tick < rules.term_ticks {
        step(&mut s, &rules, vec![], vec![]);
    }
    assert_eq!(s.nations[0].holder(Role::Diplomat), 1);
    assert_eq!(
        s.nations[0].office_since[Role::Diplomat.index()],
        rules.term_ticks,
        "a new holder's tenure starts with the term"
    );
    assert!(s.ballots.iter().all(|b| *b == [NO_VOTE; 4]));
}

#[test]
fn governance_replays_to_the_same_root() {
    let run = || {
        let (rules, mut s) = world(&[2, 1, 0, 0, 0, 0]);
        let mut e = Vec::new();
        elect(&s, &mut e, 0, &[Role::Steward, Role::Science]);
        gov::open_government(&mut s, &rules, &e).unwrap();
        let capital = s.civs[0].capital.unwrap();
        let b = batch(
            &s,
            0,
            Role::Steward,
            0,
            vec![Order::SetQueue {
                city: capital,
                items: vec![QueueItem::Settler],
            }],
        );
        step(
            &mut s,
            &rules,
            vec![b],
            vec![act(
                1,
                GovAction::Propose {
                    role: Role::Science,
                    orders: vec![Order::SetResearch {
                        techs: vec![Tech::Writing],
                    }],
                },
            )],
        );
        idle(&mut s, &rules, 5);
        s.state_root().unwrap()
    };
    assert_eq!(run(), run());
}

#[test]
fn a_wrong_signer_is_ignored() {
    let (rules, mut s) = world(&[2, 0, 0, 0, 0, 0]);
    gov::open_government(&mut s, &rules, &[]).unwrap();
    let forged = GovEntry {
        member: 0,
        signer: key(1),
        action: GovAction::Stand { roles: 1 },
    };
    step(&mut s, &rules, vec![], vec![forged]);
    assert_eq!(s.members[0].standing_for, 0);
}

#[test]
fn members_join_only_before_the_season() {
    let (rules, mut s) = world(&[1, 0, 0, 0, 0, 0]);
    assert_eq!(
        gov::join(&mut s, &rules, 0, key(0)),
        Err(RulesError::AlreadyMember)
    );
    idle(&mut s, &rules, 1);
    assert_eq!(
        gov::join(&mut s, &rules, 0, key(5)),
        Err(RulesError::RegistrationClosed)
    );
}

// ------------------------------------------------------------------ merit and achievements

#[test]
fn growth_and_gold_earn_the_active_steward_merit() {
    let (rules, mut s) = world(&[1, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::Steward]);
    gov::open_government(&mut s, &rules, &e).unwrap();
    let capital = s.civs[0].capital.unwrap();
    let pop0 = s.cities[capital as usize].pop;
    let mut t = 0;
    while s.cities[capital as usize].pop == pop0 {
        // Stay active: re-issue the focus every few ticks.
        let b = if t % 5 == 0 {
            vec![batch(
                &s,
                0,
                Role::Steward,
                0,
                vec![Order::SetFocus {
                    city: capital,
                    focus: Focus::Food,
                }],
            )]
        } else {
            vec![]
        };
        step(&mut s, &rules, b, vec![]);
        t += 1;
    }
    let log: Vec<_> = s.merit_log.iter().map(|m| m.what).collect();
    assert!(log.contains(&&b"growth"[..]), "{log:?}");
    assert!(s.members[0].merit[Path::Prosperity as usize] > 0);
}

#[test]
fn milestones_and_eras_are_announced_and_scored() {
    let (rules, mut s) = world(&[]);
    // Give civ 0 science tier 1's techs and an envoy: science 1 and concord 1 = era 1.
    for t in permutation_rules::tech::TECHS
        .iter()
        .take(rules.science_techs[0] as usize)
    {
        s.civs[0].techs.insert(t.tech);
    }
    s.civs[0].achievements.envoy_sent = true;
    idle(&mut s, &rules, 1);
    let a = &s.civs[0].achievements;
    assert_eq!(a.tiers[2], 1);
    assert_eq!(a.tiers[3], 1);
    assert_eq!(a.era, 1);
    let score = permutation_rules::scoring::nation_scores(&s, &rules)[0];
    assert_eq!(score.total(), 10 + 10 + 10);
}

// ------------------------------------------------------------------ bounded governance state (WP07)

fn propose(m: u32, role: Role, orders: Vec<Order>) -> GovEntry {
    act(m, GovAction::Propose { role, orders })
}

/// General orders that need no world: moves of units that do not exist
/// (checked at resolution), each on its own unit.
fn moves(from: u32, n: u32) -> Vec<Order> {
    (from..from + n)
        .map(|u| Order::MoveUnit {
            unit: u,
            path: vec![],
        })
        .collect()
}

#[test]
fn proposals_are_capped() {
    let (rules, mut s) = world(&[12, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::General]);
    gov::open_government(&mut s, &rules, &e).unwrap();
    let open = |s: &permutation_rules::state::WorldState| s.nations[0].proposals.len();
    // A member's third open proposal is ignored.
    step(
        &mut s,
        &rules,
        vec![],
        (0..3)
            .map(|k| propose(1, Role::General, moves(10 * k, 1)))
            .collect(),
    );
    assert_eq!(open(&s), 2);
    // The ninth open proposal of a nation is ignored.
    step(
        &mut s,
        &rules,
        vec![],
        (2..=6)
            .flat_map(|m| (0..2).map(move |k| propose(m, Role::General, moves(100 * m + k, 1))))
            .collect(),
    );
    assert_eq!(open(&s), 8);
    assert!(s.nations[0].proposals.iter().all(|p| p.proposer <= 5));
    // Expire them (proposals live `proposal_ttl_ticks`).
    idle(&mut s, &rules, rules.proposal_ttl_ticks);
    assert_eq!(open(&s), 0);
    // A proposal with a reveal is ignored; a treasury order too (WP10).
    let reveal = Order::RevealRationale {
        tick: 0,
        policy: vec![],
        salt: [0; 16],
        text: vec![],
    };
    step(
        &mut s,
        &rules,
        vec![],
        vec![
            propose(7, Role::General, vec![moves(0, 1).remove(0), reveal]),
            propose(8, Role::General, vec![Order::ConsentSpend { usdc: 1 }]),
        ],
    );
    assert_eq!(open(&s), 0);
    // 256 bytes are accepted, 257 are not.
    let path = |n: usize| {
        (0..n)
            .map(|i| permutation_rules::hex::Hex::new(i as i32 % 3, 0))
            .collect::<Vec<_>>()
    };
    let fits: Vec<Order> = [7, 7, 7, 6]
        .iter()
        .enumerate()
        .map(|(u, n)| Order::MoveUnit {
            unit: u as u32,
            path: path(*n),
        })
        .collect();
    let mut over: Vec<Order> = [9, 9, 9]
        .iter()
        .enumerate()
        .map(|(u, n)| Order::MoveUnit {
            unit: u as u32,
            path: path(*n),
        })
        .collect();
    over.push(Order::Attack {
        army: 9,
        target: permutation_rules::orders::AttackTarget::Unit(1),
    });
    assert_eq!(borsh::object_length(&fits).unwrap(), 256);
    assert_eq!(borsh::object_length(&over).unwrap(), 257);
    assert_eq!(rules.max_proposal_bytes, 256);
    step(
        &mut s,
        &rules,
        vec![],
        vec![
            propose(9, Role::General, over),
            propose(10, Role::General, fits.clone()),
        ],
    );
    assert_eq!(open(&s), 1);
    assert_eq!(s.nations[0].proposals[0].orders, fits);
    assert_eq!(s.nations[0].proposals[0].proposer, 10);
}

#[test]
fn supports_are_capped() {
    let (rules, mut s) = world(&[5, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::General]);
    gov::open_government(&mut s, &rules, &e).unwrap();
    // Five proposals by members 1–3; member 4 backs them all.
    step(
        &mut s,
        &rules,
        vec![],
        vec![
            propose(1, Role::General, moves(10, 1)),
            propose(1, Role::General, moves(11, 1)),
            propose(2, Role::General, moves(12, 1)),
            propose(2, Role::General, moves(13, 1)),
            propose(3, Role::General, moves(14, 1)),
        ],
    );
    let ids: Vec<u32> = s.nations[0].proposals.iter().map(|p| p.id).collect();
    assert_eq!(ids.len(), 5);
    step(
        &mut s,
        &rules,
        vec![],
        ids.iter()
            .map(|id| act(4, GovAction::Support { proposal: *id }))
            .collect(),
    );
    let backing = |s: &permutation_rules::state::WorldState| {
        s.nations[0]
            .proposals
            .iter()
            .filter(|p| p.supporters.contains(&4))
            .count()
    };
    assert_eq!(backing(&s), 4, "the fifth support is ignored");
    assert!(s.nations[0].proposals[4].supporters.is_empty());
    // Once a backed proposal is adopted, a new support is accepted.
    let mut b = batch(&s, 0, Role::General, 0, vec![]);
    b.adopt = vec![ids[0]];
    step(
        &mut s,
        &rules,
        vec![b],
        vec![act(4, GovAction::Support { proposal: ids[4] })],
    );
    assert_eq!(backing(&s), 3, "support is applied before the adoption");
    step(
        &mut s,
        &rules,
        vec![],
        vec![act(4, GovAction::Support { proposal: ids[4] })],
    );
    assert_eq!(backing(&s), 4);
}

#[test]
fn adopted_proposals_leave_at_intake() {
    use permutation_rules::tick::{run_phase, TickInput};
    let (rules, mut s) = world(&[2, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::General]);
    gov::open_government(&mut s, &rules, &e).unwrap();
    step(
        &mut s,
        &rules,
        vec![],
        vec![propose(1, Role::General, moves(40, 2))],
    );
    let id = s.nations[0].proposals[0].id;
    s.nations[0].role_bank = [20; 4];
    let mut b = batch(&s, 0, Role::General, 0, moves(50, 1));
    b.adopt = vec![id];
    let input = TickInput {
        vrf: common::vrf(s.tick),
        batches: vec![b],
        ..Default::default()
    };
    run_phase(&mut s, &rules, &input, 0).unwrap();
    assert!(s.nations[0].proposals.is_empty(), "gone after phase 0");
    assert_eq!(s.nations[0].adopted, 1);
    let merged = &s.tick_orders[0];
    assert_eq!(merged.orders, [moves(50, 1), moves(40, 2)].concat());
    assert_eq!(merged.credits[0].proposer, NOBODY);
    assert!(merged.credits[1..].iter().all(|c| *c
        == gov::Credit {
            officer: 0,
            proposer: 1
        }));
}

#[test]
fn reveals_are_recorded_at_intake() {
    use permutation_rules::decision::{policy_id, rationale_hash};
    use permutation_rules::tick::{run_phase, TickInput};
    // The officer is in the last nation: no event follows its merge in
    // phase 0, so the reveal event can be recomputed on top of a batch
    // without it.
    let (rules, mut s) = world(&[0, 0, 0, 0, 0, 1]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::General]);
    gov::open_government(&mut s, &rules, &e).unwrap();
    idle(&mut s, &rules, 2);
    let reveal = Order::RevealRationale {
        tick: 1,
        policy: b"hold".to_vec(),
        salt: [3; 16],
        text: b"because".to_vec(),
    };
    let with = batch(&s, 5, Role::General, 0, vec![reveal]);
    let without = batch(&s, 5, Role::General, 0, vec![]);
    let run = |b: OrderBatch, phases: u8| {
        let mut w = s.clone();
        let input = TickInput {
            vrf: common::vrf(w.tick),
            batches: vec![b],
            ..Default::default()
        };
        for p in 0..phases {
            run_phase(&mut w, &rules, &input, p).unwrap();
        }
        w
    };
    let (a, b) = (run(without.clone(), 1), run(with.clone(), 1));
    let mut payload = Vec::new();
    payload.extend_from_slice(&5u16.to_le_bytes());
    payload.push(Role::General as u8);
    payload.extend_from_slice(&1u16.to_le_bytes());
    payload.extend_from_slice(&policy_id(b"hold"));
    payload.extend_from_slice(&rationale_hash(&[3; 16], b"because"));
    assert_eq!(payload.len(), 69);
    let head = permutation_rules::hash::sha256(&[
        &a.event_head,
        &s.tick.to_le_bytes(),
        &[6],
        b"reveal",
        &payload,
    ]);
    assert_eq!(b.event_head, head, "the reveal event, pushed in phase 0");
    assert!(b
        .tick_orders
        .iter()
        .all(|c| c.orders.iter().all(|o| o.is_action())));
    // Otherwise the two worlds are the same, and phase 2 pushes nothing more.
    let mut a = a;
    a.event_head = b.event_head;
    assert_eq!(a, b);
    let (mut a3, b3) = (run(without, 3), run(with, 3));
    let (ha, hb) = (a3.event_head, b3.event_head);
    a3.event_head = hb;
    assert_eq!(a3, b3);
    assert_ne!(ha, hb);
}

// ------------------------------------------------------------------ treasury governance (WP10)

#[test]
fn treasury_orders_cannot_be_proposed() {
    use permutation_rules::orders::{Good, Side};
    let (rules, mut s) = world(&[3, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::Diplomat]);
    gov::open_government(&mut s, &rules, &e).unwrap();
    let buy = Order::ExchangeOrder {
        good: Good::Iron,
        side: Side::Buy,
        amount: 1,
        price: 1_000_000,
    };
    let offer = Order::OfferContract {
        to: Some(1),
        term: permutation_rules::contracts::ContractTerm::Peace,
        usdc: 1_000_000,
        deadline: 20,
    };
    let next = s.nations[0].next_proposal;
    step(
        &mut s,
        &rules,
        vec![],
        vec![
            propose(1, Role::Diplomat, vec![buy]),
            propose(1, Role::Diplomat, vec![offer]),
            propose(2, Role::General, vec![Order::ConsentSpend { usdc: 9 }]),
            propose(
                2,
                Role::Science,
                vec![
                    Order::SetResearch {
                        techs: vec![Tech::Agriculture],
                    },
                    Order::ConsentSpend { usdc: 9 },
                ],
            ),
        ],
    );
    assert!(s.nations[0].proposals.is_empty());
    assert_eq!(s.nations[0].next_proposal, next);
    step(
        &mut s,
        &rules,
        vec![],
        vec![propose(
            1,
            Role::Diplomat,
            vec![Order::AcceptContract { id: 0 }],
        )],
    );
    assert_eq!(
        s.nations[0].proposals.len(),
        1,
        "AcceptContract stays proposable"
    );
}
