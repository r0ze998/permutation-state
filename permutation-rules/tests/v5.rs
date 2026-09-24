//! Game Design V5: nations, offices, governance, merit, achievements, and
//! the v0.2 rule fixes that V5 carries over.

use permutation_rules::buildings::Building;
use permutation_rules::checks::Blocked;
use permutation_rules::fixed::MILLI;
use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::gov::{self, GovAction, GovEntry, MemberId, Path, Role, NOBODY};
use permutation_rules::invariants;
use permutation_rules::orders::{office_batches, validate_batch, Good, Order, OrderBatch};
use permutation_rules::rng::Seed;
use permutation_rules::state::{Focus, QueueItem, WorldState};
use permutation_rules::tech::Tech;
use permutation_rules::tick::{resolve_tick, TickInput};
use permutation_rules::{Preset, RulesError, Ruleset};

const DIGEST: [u8; 32] = [9; 32];

fn key(m: u32) -> [u8; 32] {
    let mut k = [0u8; 32];
    k[..4].copy_from_slice(&(m + 1).to_le_bytes());
    k
}

fn vrf(tick: u16) -> Seed {
    let mut s = [0u8; 32];
    s[..2].copy_from_slice(&tick.to_le_bytes());
    s
}

/// Six nations; `members[c]` members join nation `c` (ids in order).
fn world(members: &[usize]) -> (Ruleset, WorldState) {
    let rules = Ruleset::new(Preset::Blitz);
    let mut s = new_season(&rules, &[11; 32], &[22; 32], &nation_entries(6)).unwrap();
    let mut id = 0;
    for (civ, n) in members.iter().enumerate() {
        for _ in 0..*n {
            assert_eq!(gov::join(&mut s, &rules, civ as u16, key(id)).unwrap(), id);
            id += 1;
        }
    }
    (rules, s)
}

fn act(m: MemberId, action: GovAction) -> GovEntry {
    GovEntry { member: m, signer: key(m), action }
}

/// Member `m` stands for `roles`, and everyone in its nation votes for it.
fn elect(s: &WorldState, entries: &mut Vec<GovEntry>, m: MemberId, roles: &[Role]) {
    let mask = roles.iter().fold(0, |a, r| a | r.bit());
    entries.push(act(m, GovAction::Stand { roles: mask }));
    let civ = s.members[m as usize].civ;
    for (v, x) in s.members.iter().enumerate() {
        if x.civ == civ {
            for r in roles {
                entries.push(act(v as u32, GovAction::Vote { role: *r, candidate: m }));
            }
        }
    }
}

/// One office's batch from a member (or the acting official).
fn batch(s: &WorldState, civ: u16, role: Role, member: MemberId, orders: Vec<Order>) -> OrderBatch {
    OrderBatch { civ, tick: s.tick, role, member, decision_digest: DIGEST, orders, adopt: vec![] }
}

fn step(s: &mut WorldState, rules: &Ruleset, batches: Vec<OrderBatch>, gov: Vec<GovEntry>) {
    let before = s.clone();
    resolve_tick(s, rules, &TickInput { vrf: vrf(s.tick), batches, gov, deposits: vec![] }).unwrap();
    let v = invariants::check(s, rules);
    assert!(v.is_empty(), "{v:?}");
    assert!(invariants::check_monotonic(&before, s).is_empty());
}

fn idle(s: &mut WorldState, rules: &Ruleset, ticks: u16) {
    for _ in 0..ticks {
        step(s, rules, vec![], vec![]);
    }
}

// ------------------------------------------------------------------ governance

#[test]
fn the_first_election_fills_offices_with_at_most_two_each() {
    let (rules, mut s) = world(&[3, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    // Member 0 stands for everything and gets every vote: it may hold only two.
    elect(&s, &mut e, 0, &Role::ALL);
    e.push(act(1, GovAction::Stand { roles: Role::Science.bit() | Role::Diplomat.bit() }));
    gov::open_government(&mut s, &rules, &e).unwrap();
    let n = &s.nations[0];
    assert_eq!(n.holder(Role::General), 0);
    assert_eq!(n.holder(Role::Steward), 0);
    assert_eq!(n.holder(Role::Science), 1, "member 0 already holds two offices");
    assert_eq!(n.holder(Role::Diplomat), 1);
    assert_eq!(s.nations[1].offices, [NOBODY; 4], "no members: the acting official");
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
    let focus = Order::SetFocus { city: capital, focus: Focus::Food };
    let not_holder = batch(&s, 0, Role::Steward, 1, vec![focus.clone()]);
    assert_eq!(validate_batch(&s, &rules, &not_holder), Err(RulesError::NotOfficer));
    let acting = batch(&s, 0, Role::Steward, NOBODY, vec![focus.clone()]);
    assert_eq!(validate_batch(&s, &rules, &acting), Err(RulesError::NotOfficer), "the office is not vacant");
    let mut unsealed = batch(&s, 0, Role::Steward, 0, vec![focus.clone()]);
    unsealed.decision_digest = [0; 32];
    assert_eq!(validate_batch(&s, &rules, &unsealed), Err(RulesError::MissingRationale));
    let research = batch(&s, 0, Role::Steward, 0, vec![Order::SetResearch { techs: vec![Tech::Agriculture] }]);
    assert_eq!(validate_batch(&s, &rules, &research), Err(RulesError::WrongOffice(0)));
    let ok = batch(&s, 0, Role::Steward, 0, vec![focus]);
    assert_eq!(validate_batch(&s, &rules, &ok), Ok(1));
    step(&mut s, &rules, vec![ok], vec![]);
    assert_eq!(s.cities[capital as usize].focus, Focus::Food);
    assert_eq!(s.nations[0].office_last_act[Role::Steward.index()], 0);
    assert_eq!(s.members[0].merit[Path::Common as usize], rules.merit_office_tick, "one office tick");
}

#[test]
fn unit_orders_follow_the_unit_type() {
    let (rules, mut s) = world(&[]);
    let spear = s.units.iter().find(|u| u.owner == permutation_rules::state::Owner::Civ(0) && !u.unit_type.is_civilian()).unwrap().id;
    let hex = s.units[spear as usize].hex;
    let next = hex.neighbors().into_iter().find(|h| s.map.tile(*h).is_some_and(|t| t.terrain.is_passable())).unwrap();
    // The steward cannot move an army: the order is dropped and recorded.
    let b = batch(&s, 0, Role::Steward, NOBODY, vec![Order::MoveUnit { unit: spear, path: vec![next] }]);
    step(&mut s, &rules, vec![b], vec![]);
    assert_eq!(s.units[spear as usize].hex, hex);
    assert!(s.last_skipped.iter().any(|k| k.reason == Blocked::WrongOffice.code()));
}

#[test]
fn budgets_are_split_by_office_and_banked_per_office() {
    let (rules, mut s) = world(&[]);
    assert_eq!(s.civs[0].tick_budget, 4);
    idle(&mut s, &rules, 2);
    // Two idle ticks: each office banked its own share (1 each at B = 4).
    assert_eq!(s.nations[0].role_bank, [2, 2, 2, 2]);
    let capital = s.civs[0].capital.unwrap();
    let three = vec![
        Order::SetFocus { city: capital, focus: Focus::Food },
        Order::SetFocus { city: capital, focus: Focus::Gold },
        Order::SetFocus { city: capital, focus: Focus::Production },
    ];
    let b = batch(&s, 0, Role::Steward, NOBODY, three);
    assert_eq!(validate_batch(&s, &rules, &b), Ok(3), "1 this tick + 2 banked");
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
    assert!(matches!(a.relation(0, 1), permutation_rules::state::Relation::Peace));
    assert!(a.last_skipped.iter().any(|k| k.reason == Blocked::NeedsConsent.code()));
    // The steward, a different member, consents: war.
    let consent = batch(&s, 0, Role::Steward, 1, vec![Order::ConsentWar { civ: 1 }]);
    step(&mut s, &rules, vec![declare, consent], vec![]);
    assert!(matches!(s.relation(0, 1), permutation_rules::state::Relation::War { .. }));
}

#[test]
fn an_adopted_proposal_shares_its_merit_with_the_proposer() {
    let (rules, mut s) = world(&[2, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::Science]);
    gov::open_government(&mut s, &rules, &e).unwrap();
    // Member 1 proposes research; it is adopted by id the next tick.
    let research = Order::SetResearch { techs: vec![Tech::Agriculture] };
    step(&mut s, &rules, vec![], vec![act(1, GovAction::Propose { role: Role::Science, orders: vec![research] })]);
    let id = s.nations[0].proposals[0].id;
    let mut b = batch(&s, 0, Role::Science, 0, vec![]);
    b.adopt = vec![id];
    step(&mut s, &rules, vec![b], vec![]);
    assert!(s.nations[0].proposals.is_empty(), "adopted proposals are removed");
    assert_eq!(s.civs[0].research_queue, vec![Tech::Agriculture]);
    // Run until Agriculture completes; its merit is split half and half.
    while !s.civs[0].techs.has(Tech::Agriculture) {
        step(&mut s, &rules, vec![], vec![]);
    }
    let sci = |m: usize| s.members[m].merit[Path::Science as usize];
    assert!(sci(1) > 0);
    assert!(sci(0) >= sci(1) && sci(0) - sci(1) <= 1, "half each: {} vs {}", sci(0), sci(1));
}

#[test]
fn copying_a_supported_proposal_counts_as_adopting_it() {
    let (rules, mut s) = world(&[3, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::Steward]);
    gov::open_government(&mut s, &rules, &e).unwrap();
    let capital = s.civs[0].capital.unwrap();
    let order = Order::SetQueue { city: capital, items: vec![QueueItem::Building(Building::Granary)] };
    step(&mut s, &rules, vec![], vec![act(1, GovAction::Propose { role: Role::Steward, orders: vec![order.clone()] })]);
    let id = s.nations[0].proposals[0].id;
    step(&mut s, &rules, vec![], vec![act(2, GovAction::Support { proposal: id })]);
    // The steward issues the same order without citing the proposal.
    let b = batch(&s, 0, Role::Steward, 0, vec![order]);
    step(&mut s, &rules, vec![b], vec![]);
    assert_eq!(s.cities[capital as usize].queue_credit, gov::Credit { officer: 0, proposer: 1 });
}

#[test]
fn a_majority_of_active_members_recalls_an_officer() {
    let (rules, mut s) = world(&[3, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::General]);
    e.push(act(1, GovAction::Stand { roles: Role::General.bit() }));
    gov::open_government(&mut s, &rules, &e).unwrap();
    assert_eq!(s.nations[0].runner_up[Role::General.index()], 1);
    // Members 1 and 2 vote to recall: 2 of 3 recently active members.
    step(&mut s, &rules, vec![], vec![act(1, GovAction::Recall { role: Role::General }), act(2, GovAction::Recall { role: Role::General })]);
    assert_eq!(s.nations[0].holder(Role::General), 1, "the runner-up takes over next tick");
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
    step(&mut s, &rules, vec![], vec![act(1, GovAction::Stand { roles: Role::Diplomat.bit() }), act(0, GovAction::Vote { role: Role::Diplomat, candidate: 1 })]);
    assert!(s.nations[0].votes.is_empty());
    while s.tick < rules.term_ticks - rules.vote_window {
        step(&mut s, &rules, vec![], vec![]);
    }
    step(&mut s, &rules, vec![], vec![act(0, GovAction::Vote { role: Role::Diplomat, candidate: 1 })]);
    assert_eq!(s.nations[0].votes.len(), 1);
    while s.tick < rules.term_ticks {
        step(&mut s, &rules, vec![], vec![]);
    }
    assert_eq!(s.nations[0].holder(Role::Diplomat), 1);
    assert_eq!(s.nations[0].office_since[Role::Diplomat.index()], rules.term_ticks, "a new holder's tenure starts with the term");
    assert!(s.nations[0].votes.is_empty());
}

#[test]
fn governance_replays_to_the_same_root() {
    let run = || {
        let (rules, mut s) = world(&[2, 1, 0, 0, 0, 0]);
        let mut e = Vec::new();
        elect(&s, &mut e, 0, &[Role::Steward, Role::Science]);
        gov::open_government(&mut s, &rules, &e).unwrap();
        let capital = s.civs[0].capital.unwrap();
        let b = batch(&s, 0, Role::Steward, 0, vec![Order::SetQueue { city: capital, items: vec![QueueItem::Settler] }]);
        step(&mut s, &rules, vec![b], vec![act(1, GovAction::Propose { role: Role::Science, orders: vec![Order::SetResearch { techs: vec![Tech::Writing] }] })]);
        idle(&mut s, &rules, 5);
        s.state_root().unwrap()
    };
    assert_eq!(run(), run());
}

#[test]
fn a_wrong_signer_is_ignored() {
    let (rules, mut s) = world(&[2, 0, 0, 0, 0, 0]);
    gov::open_government(&mut s, &rules, &[]).unwrap();
    let forged = GovEntry { member: 0, signer: key(1), action: GovAction::Stand { roles: 1 } };
    step(&mut s, &rules, vec![], vec![forged]);
    assert_eq!(s.members[0].standing_for, 0);
}

#[test]
fn members_join_only_before_the_season() {
    let (rules, mut s) = world(&[1, 0, 0, 0, 0, 0]);
    assert_eq!(gov::join(&mut s, &rules, 0, key(0)), Err(RulesError::AlreadyMember));
    idle(&mut s, &rules, 1);
    assert_eq!(gov::join(&mut s, &rules, 0, key(5)), Err(RulesError::RegistrationClosed));
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
        let b = if t % 5 == 0 { vec![batch(&s, 0, Role::Steward, 0, vec![Order::SetFocus { city: capital, focus: Focus::Food }])] } else { vec![] };
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
    // Give civ 0 four techs and an envoy: science 1 and concord 1 = era 1.
    for t in [Tech::Agriculture, Tech::BronzeWorking, Tech::Writing, Tech::Mysticism] {
        s.civs[0].techs.insert(t);
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

// ------------------------------------------------------------------ v0.2 fixes carried into V5

#[test]
fn a_city_completes_one_item_per_tick_and_banks_no_production() {
    let (rules, mut s) = world(&[]);
    let capital = s.civs[0].capital.unwrap() as usize;
    s.cities[capital].queue = vec![QueueItem::Scout, QueueItem::Scout, QueueItem::Scout];
    s.cities[capital].prod = 500 * MILLI;
    let units = s.units.len();
    idle(&mut s, &rules, 1);
    assert_eq!(s.units.len(), units + 1, "one item per tick (v0.2 C1)");
    let scout_cost = permutation_rules::units::stats(permutation_rules::units::UnitType::Scout).prod_cost as i64 * MILLI;
    assert!(s.cities[capital].prod <= scout_cost, "the store is capped at the next item's cost");
}

#[test]
fn star_gate_stages_are_spaced_and_never_bought() {
    let (rules, mut s) = world(&[]);
    let capital = s.civs[0].capital.unwrap();
    for t in permutation_rules::tech::TECHS {
        s.civs[0].techs.insert(t.tech);
    }
    s.civs[0].gold = 10_000 * MILLI;
    s.cities[capital as usize].queue = vec![QueueItem::Building(Building::StarGate1), QueueItem::Building(Building::StarGate2)];
    s.nations[0].role_bank = [4; 4];
    let buy = |s: &WorldState| batch(s, 0, Role::Steward, NOBODY, vec![Order::Purchase { city: capital, gold: 1_000 }]);
    let b = buy(&s);
    step(&mut s, &rules, vec![b], vec![]);
    assert!(s.last_skipped.iter().any(|k| k.reason == Blocked::CannotBuyStarGate.code()));
    // Complete stage I by hand, then stage II must wait `star_gate_spacing` ticks.
    s.cities[capital as usize].prod = 1_000_000 * MILLI;
    idle(&mut s, &rules, 1);
    let first = s.civs[0].last_star_gate.unwrap();
    for _ in 0..rules.star_gate_spacing - 1 {
        let c = &mut s.cities[capital as usize];
        c.prod = 1_000_000 * MILLI;
        idle(&mut s, &rules, 1);
        assert_eq!(s.civs[0].scores.star_gate_stages, 1, "stage II waits");
    }
    s.cities[capital as usize].prod = 1_000_000 * MILLI;
    idle(&mut s, &rules, 1);
    assert_eq!(s.civs[0].scores.star_gate_stages, 2);
    assert_eq!(s.civs[0].last_star_gate, Some(first + rules.star_gate_spacing));
    assert_eq!(s.civs[0].achievements.star_gate_max, 2);
}

#[test]
fn one_purchase_per_city_per_tick() {
    let (rules, mut s) = world(&[]);
    let capital = s.civs[0].capital.unwrap();
    s.cities[capital as usize].queue = vec![QueueItem::Building(Building::Granary)];
    s.civs[0].gold = 1_000 * MILLI;
    s.nations[0].role_bank = [4; 4];
    let two = vec![Order::Purchase { city: capital, gold: 3 }, Order::Purchase { city: capital, gold: 3 }];
    let b = batch(&s, 0, Role::Steward, NOBODY, two);
    step(&mut s, &rules, vec![b], vec![]);
    assert!(s.last_skipped.iter().any(|k| k.reason == Blocked::AlreadyPurchased.code()));
}

#[test]
fn transfer_caps_apply_per_pair_per_tick() {
    let (rules, mut s) = world(&[]);
    idle(&mut s, &rules, 1);
    let cap = s.civs[0].last.gold.max(10);
    s.civs[0].gold = 1_000 * MILLI;
    s.nations[0].role_bank = [4; 4];
    let g1 = s.civs[1].gold;
    let two = vec![Order::Transfer { civ: 1, good: Good::Gold, amount: cap }, Order::Transfer { civ: 1, good: Good::Gold, amount: cap }];
    let b = batch(&s, 0, Role::Diplomat, NOBODY, two);
    step(&mut s, &rules, vec![b], vec![]);
    let received = (s.civs[1].gold - g1) / MILLI - s.civs[1].last.gold as i64;
    assert_eq!(received, cap as i64, "the second transfer exceeds the per-pair cap (v0.2 C4)");
    assert!(s.civs[0].achievements.trade[1] > 0 && s.civs[1].achievements.trade[0] > 0);
}

#[test]
fn a_city_strikes_back_at_archers() {
    use permutation_rules::combat::{resolve_engagement, Combatant, Situation};
    use permutation_rules::units::UnitType;
    let rules = Ruleset::new(Preset::Blitz);
    let archers = Combatant::Army { unit: UnitType::Archer, troops: 10_000 };
    let city = Combatant::City { defense: 8_000 };
    let sit = Situation { ranged_attack: true, city_strike: 8_000, ..Default::default() };
    let (_, to_att) = resolve_engagement(&rules, archers, city, sit, 10_000, 10_000);
    assert!(to_att > 0, "v0.2 C5");
    let field = Situation { ranged_attack: true, ..Default::default() };
    let spear = Combatant::Army { unit: UnitType::Spearman, troops: 10_000 };
    assert_eq!(resolve_engagement(&rules, archers, spear, field, 10_000, 10_000).1, 0, "armies still do not retaliate");
}

#[test]
fn the_science_focus_raises_science() {
    let (rules, mut s) = world(&[]);
    let capital = s.civs[0].capital.unwrap() as usize;
    let mut science = s.clone();
    science.cities[capital].focus = Focus::Science;
    idle(&mut s, &rules, 1);
    idle(&mut science, &rules, 1);
    assert!(science.civs[0].science_store > s.civs[0].science_store, "v0.2 C6");
}

#[test]
fn a_reveal_alone_does_not_make_an_officer_active() {
    let (rules, mut s) = world(&[1, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::General]);
    gov::open_government(&mut s, &rules, &e).unwrap();
    idle(&mut s, &rules, 1);
    let reveal = Order::RevealRationale { tick: 0, policy: b"human".to_vec(), salt: [0; 16], text: b"x".to_vec() };
    let b = batch(&s, 0, Role::General, 0, vec![reveal]);
    step(&mut s, &rules, vec![b], vec![]);
    assert_eq!(s.nations[0].office_last_act[Role::General.index()], gov::NEVER, "v0.2 C9");
}

#[test]
fn bot_nations_run_every_office_through_the_acting_official() {
    let (rules, mut s) = world(&[]);
    let capital = s.civs[0].capital.unwrap();
    let orders = vec![
        Order::SetQueue { city: capital, items: vec![QueueItem::Settler] },
        Order::SetResearch { techs: vec![Tech::Agriculture] },
        Order::DeclareWar { civ: 1 },
    ];
    let batches = office_batches(&s, 0, [0; 32], orders);
    assert_eq!(batches.iter().map(|b| b.role).collect::<Vec<_>>(), vec![Role::General, Role::Steward, Role::Science, Role::Diplomat]);
    step(&mut s, &rules, batches, vec![]);
    assert!(matches!(s.relation(0, 1), permutation_rules::state::Relation::War { .. }), "the acting official consents to itself");
    assert_eq!(s.civs[0].research_queue, vec![Tech::Agriculture]);
}
