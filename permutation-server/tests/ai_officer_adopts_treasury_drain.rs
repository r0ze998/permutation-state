//! Audit repro (money, high; flipped by WP10): the operator's hidden AI
//! officers must not adopt an outsider's proposal that spends the nation's
//! treasury.
//!
//! Setup: 6 nations, two operator AI members each (they win every office,
//! as `seat_ai_members` seats them), plus people who register without
//! standing. Nation 0 has 6 people; nation 3 has one person, Y, who
//! deposited 200 USDC. Every other depositor puts in 10 USDC.
//!
//! At tick 1 two ordinary members send governance actions only (no batch):
//! - X (nation 0, no office): Propose{Diplomat: ExchangeOrder Buy Gold 1 @ ~all
//!   of the treasury} and Propose{General: ConsentSpend(u64::MAX)}.
//! - Y (nation 3, no office): Propose{Diplomat: ExchangeOrder Sell Gold 1 @ same price}.
//!
//! Before WP10 the AI officers adopted all three at tick 2 and nation 0's
//! treasury moved to nation 3 (Y's refund by shares 200 → 266 USDC). Now
//! treasury orders cannot be proposed (the rules drop them), the bots never
//! support or adopt one, and every treasury stays as deposited. Refunds are
//! shares of the final treasuries, so they are unchanged too.

use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::gov::{self, GovAction, GovEntry, MemberId, Role};
use permutation_rules::orders::{Good, Order, Side};
use permutation_rules::tick::{resolve_tick, TickInput};
use permutation_rules::{Preset, Ruleset};
use permutation_server::driver::{local_key, seat_ai_members, Hosting, Planner};
use permutation_server::fog::Fog;
use permutation_server::ledger::Ledger;

const USDC: u64 = 1_000_000;

/// The operator hosts only its AI members; people act for themselves.
struct Operator(Vec<bool>);
impl Hosting for Operator {
    fn is_ai(&self, m: MemberId) -> bool {
        self.0.get(m as usize).copied().unwrap_or(false)
    }
    fn key(&self, m: MemberId) -> [u8; 32] {
        local_key(m)
    }
}

#[test]
fn ai_officers_do_not_adopt_an_outsiders_treasury_drain() {
    let rules = Ruleset::new(Preset::Blitz);
    // People per nation (beyond the 2 AI each); Y is nation 3's person.
    let people = [6usize, 1, 1, 1, 1, 1];
    let (x_civ, y_civ) = (0u16, 3u16);
    let mut entries = nation_entries(6);
    for (c, e) in entries.iter_mut().enumerate() {
        let y = if c == y_civ as usize { 200 - 10 } else { 0 };
        e.treasury = (2 + people[c] as u64) * 10 * USDC + y * USDC;
    }
    let mut s = new_season(&rules, &[7; 32], &[9; 32], &entries).unwrap();

    // People register (tick 0) without standing for office.
    let mut first_person = [0 as MemberId; 6];
    for (c, k) in people.iter().enumerate() {
        for j in 0..*k {
            let key = local_key(s.members.len() as u32);
            let id = gov::join(&mut s, &rules, c as u16, key).unwrap();
            if j == 0 {
                first_person[c] = id;
            }
        }
    }
    let n_people = s.members.len();
    // The operator's 2 AI members per nation stand, vote and win the first election.
    seat_ai_members(&mut s, &rules, &[2; 6]).unwrap();
    let ai: Vec<bool> = (0..s.members.len()).map(|m| m >= n_people).collect();
    let (x, y) = (first_person[x_civ as usize], first_person[y_civ as usize]);
    for c in [x_civ, y_civ] {
        let offices = s.nations[c as usize].offices;
        assert!(
            offices.iter().all(|m| ai[*m as usize]),
            "nation {c}: AI officers {offices:?}"
        );
        assert!(!offices.contains(&x) && !offices.contains(&y));
    }

    let host = Operator(ai.clone());
    let mut planner = Planner::new(6);
    let mut fog = Fog::new(&s);
    let mut ledger = Ledger::seeded(b"audit");
    let deposited: Vec<u64> = s.civs.iter().map(|c| c.usdc).collect();
    let price = s.civs[x_civ as usize].free_usdc() * 10 / 11; // + 5% fee + 5% tariff = all of it
    let mut adopted = vec![];
    let mut supported = vec![];
    while s.tick < 3 {
        ledger.observe(&s, &fog);
        let mut gov = planner.member_gov(&s, &rules, &fog, &host);
        let batches = planner.batches(&s, &rules, &fog, &mut ledger, &host);
        if s.tick == 1 {
            let buy = Order::ExchangeOrder {
                good: Good::Gold,
                side: Side::Buy,
                amount: 1,
                price,
            };
            let sell = Order::ExchangeOrder {
                good: Good::Gold,
                side: Side::Sell,
                amount: 1,
                price,
            };
            let prop = |m: MemberId, role, o: Order| GovEntry {
                member: m,
                signer: local_key(m),
                action: GovAction::Propose {
                    role,
                    orders: vec![o],
                },
            };
            gov.push(prop(x, Role::Diplomat, buy));
            gov.push(prop(
                x,
                Role::General,
                Order::ConsentSpend { usdc: u64::MAX },
            ));
            gov.push(prop(y, Role::Diplomat, sell));
        }
        if s.tick == 2 {
            for c in [x_civ, y_civ] {
                assert!(
                    s.nations[c as usize]
                        .proposals
                        .iter()
                        .all(|p| !p.orders.iter().any(Order::is_treasury_order)),
                    "nation {c} stored a treasury proposal"
                );
            }
            for b in &batches {
                if b.civ == x_civ || b.civ == y_civ {
                    adopted.extend(b.adopt.iter().map(|id| (b.civ, *id)));
                }
            }
            for e in &gov {
                if let GovAction::Support { proposal } = e.action {
                    supported.push((e.member, proposal));
                }
            }
        }
        let input = TickInput {
            vrf: [s.tick as u8; 32],
            batches,
            gov,
            deposits: vec![],
        };
        resolve_tick(&mut s, &rules, &input).unwrap();
        fog.update(&s);
    }
    assert!(adopted.is_empty(), "adopted: {adopted:?}");
    assert!(supported.is_empty(), "AI supports: {supported:?}");
    let final_t: Vec<u64> = s.civs.iter().map(|c| c.usdc).collect();
    assert_eq!(final_t, deposited, "every treasury as deposited");
}
