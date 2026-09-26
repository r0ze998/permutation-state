//! WP04/WP07: the hosted AI members play within the caps. Every batch they
//! seal runs at most `MAX_BATCH_ORDERS` orders and `MAX_FREE_ORDERS`
//! zero-cost ones (adopted proposals included), no adoption is ever
//! skipped, and every proposal they make has a valid shape.

use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::gov::{self, GovAction};
use permutation_rules::orders::{expand_batch, validate_batch, MAX_BATCH_ORDERS, MAX_FREE_ORDERS};
use permutation_rules::{Preset, RulesError, Ruleset};
use permutation_server::driver::{seat_ai_members, AiSeason, Planner};
use permutation_server::ledger::Ledger;

fn seed(tag: &str, i: u32) -> [u8; 32] {
    let mut s = [0u8; 32];
    let t = format!("wp04/{tag}/{i}");
    s[..t.len()].copy_from_slice(t.as_bytes());
    s
}

#[test]
fn honest_ai_batches_stay_within_caps() {
    let layouts: [&[usize]; 4] = [
        &[3, 3, 2, 2, 1, 0],
        &[2; 6],
        &[8; 6], // the member cap
        &[1, 1, 1, 1],
    ];
    let (mut batches, mut adoptions, mut proposals) = (0usize, 0usize, 0usize);
    let (mut most, mut most_free) = (0usize, 0usize);
    for (l, members) in layouts.iter().enumerate() {
        for i in 0..2u32 {
            let rules = Ruleset::new(Preset::Blitz);
            let mut s = new_season(
                &rules,
                &seed("w", i),
                &seed("s", i),
                &nation_entries(members.len()),
            )
            .unwrap();
            seat_ai_members(&mut s, &rules, members).unwrap();
            assert!(s.members.len() <= rules.max_members as usize);
            let mut season = AiSeason::new(
                rules,
                s,
                Planner::new(members.len()),
                Ledger::seeded(&seed("l", i)),
            );
            while !season.over() {
                let mut vrf = [0u8; 32];
                vrf[..2].copy_from_slice(&season.state.tick.to_le_bytes());
                vrf[2] = l as u8;
                vrf[3] = i as u8;
                let input = season.plan(vrf);
                let (s, rules) = (&season.state, &season.rules);
                for b in &input.batches {
                    let (orders, adopted) = expand_batch(s, b).unwrap();
                    let free = orders.iter().filter(|(o, _)| o.cost() == 0).count();
                    assert!(orders.len() <= MAX_BATCH_ORDERS, "{} orders", orders.len());
                    assert!(free <= MAX_FREE_ORDERS, "{free} free orders");
                    assert_eq!(adopted, b.adopt, "tick {}: an adoption was skipped", s.tick);
                    assert_ne!(validate_batch(s, rules, b), Err(RulesError::TooManyOrders));
                    batches += 1;
                    adoptions += adopted.len();
                    most = most.max(orders.len());
                    most_free = most_free.max(free);
                }
                for e in &input.gov {
                    if let GovAction::Propose { orders, .. } = &e.action {
                        assert!(gov::proposal_shape_ok(rules, s.tick, orders), "{orders:?}");
                        assert!(!orders.iter().any(|o| o.is_treasury_order()));
                        proposals += 1;
                    }
                }
                season.resolve(&input).unwrap();
            }
        }
    }
    println!(
        "{batches} batches, {adoptions} adoptions, {proposals} proposals; at most {most} orders, {most_free} free"
    );
    assert!(batches > 1000 && adoptions > 0 && proposals > 0);
}
