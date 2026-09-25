//! On a six-fold symmetric map (six nations, `mapgen`) the rules treat
//! every position alike: a season played with the scripted bots, replayed in
//! the world turned by 60° with every order turned the same way, ends with
//! each nation on the same score. Any rule that breaks a tie by absolute
//! direction or tile index (instead of `Hex::turned` / `neighbors_in`)
//! fails this.

use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::gov::GovAction;
use permutation_rules::mapgen::rotate_world;
use permutation_rules::scoring::nation_scores;
use permutation_rules::tick::resolve_tick;
use permutation_rules::{Preset, Ruleset};
use permutation_server::bots::PERSONAS;
use permutation_server::driver::{seat_ai_members, AiSeason, Planner};
use permutation_server::ledger::Ledger;

fn seed(tag: &str, i: u8) -> [u8; 32] {
    let mut s = [i; 32];
    s[..tag.len()].copy_from_slice(tag.as_bytes());
    s
}

#[test]
fn turning_the_world_turns_the_season() {
    let rules = Ruleset::new(Preset::Blitz);
    let members = [3, 3, 2, 2, 1, 0];
    for (i, k) in [(1u8, 1u8), (2, 3), (3, 5)] {
        let start = || {
            let mut s = new_season(
                &rules,
                &seed("world", i),
                &seed("season", i),
                &nation_entries(6),
            )
            .unwrap();
            seat_ai_members(&mut s, &rules, &members).unwrap();
            s
        };
        let persona: Vec<_> = (0..6)
            .map(|c| PERSONAS[(c + i as usize) % PERSONAS.len()])
            .collect();
        let mut season = AiSeason::new(
            rules.clone(),
            start(),
            Planner::with_personas(&persona),
            Ledger::seeded(&seed("ledger", i)),
        );
        let mut inputs = Vec::new();
        while !season.over() {
            let mut vrf = [0u8; 32];
            vrf[..2].copy_from_slice(&season.state.tick.to_le_bytes());
            let input = season.plan(vrf);
            season.resolve(&input).unwrap();
            inputs.push(input);
        }
        let mut turned = start();
        rotate_world(&mut turned, k);
        for mut input in inputs {
            input
                .batches
                .iter_mut()
                .for_each(|b| b.orders.iter_mut().for_each(|o| o.rotate(k)));
            for e in &mut input.gov {
                if let GovAction::Propose { orders, .. } = &mut e.action {
                    orders.iter_mut().for_each(|o| o.rotate(k));
                }
            }
            resolve_tick(&mut turned, &rules, &input).unwrap();
        }
        let a: Vec<_> = nation_scores(&season.state, &rules)
            .iter()
            .map(|x| (x.total(), x.tiers))
            .collect();
        let b: Vec<_> = nation_scores(&turned, &rules)
            .iter()
            .map(|x| (x.total(), x.tiers))
            .collect();
        assert_eq!(a, b, "season {i} turned by {k} × 60°");
        assert!(a.iter().any(|x| x.0 > 0), "the season actually played");
    }
}
