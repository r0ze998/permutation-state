//! V5 §18.5 after WP10: people take operator AI payouts in proportion to
//! merit, each at most `equal_cap × max(1, merit / unit)` (unit = 10% of the
//! AIs' average merit), so a member who only votes, or farms a token of
//! merit, cannot collect them, and there is no cliff. Real bot seasons: 6
//! nations × (2 AIs + 1 person); merit overridden on the final state as the
//! audit harness did (AIs 40, genuine people 20, sybils `s`).

use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::payout::{settle_with, Extras};
use permutation_rules::state::WorldState;
use permutation_rules::{Preset, Ruleset};
use permutation_server::driver::{seat_ai_members, AiSeason, Planner};
use permutation_server::ledger::Ledger;

const FEE: u64 = 10_000_000;
const SEEDS: u32 = 4;

fn seed(tag: &str, i: u32) -> [u8; 32] {
    let mut s = [0u8; 32];
    let t = format!("wp10/{tag}/{i}");
    s[..t.len()].copy_from_slice(t.as_bytes());
    s
}

fn played(i: u32) -> (Ruleset, WorldState) {
    let rules = Ruleset::new(Preset::Blitz);
    let mut s = new_season(&rules, &seed("w", i), &seed("s", i), &nation_entries(6)).unwrap();
    seat_ai_members(&mut s, &rules, &[3; 6]).unwrap();
    let mut season = AiSeason::new(rules, s, Planner::new(6), Ledger::seeded(&seed("l", i)));
    while !season.over() {
        let mut vrf = [0u8; 32];
        vrf[..2].copy_from_slice(&season.state.tick.to_le_bytes());
        vrf[2] = i as u8;
        let input = season.plan(vrf);
        season.resolve(&input).unwrap();
    }
    (season.rules, season.state)
}

/// Payouts of the people (members 3c + 2) with sybil merit `s` (milli) in
/// nations 3–5; members 3c and 3c + 1 are AIs on the revealed roster.
fn people(base: &WorldState, rules: &Ruleset, sybil: u32) -> Vec<u64> {
    let mut st = base.clone();
    for c in 0..6 {
        for k in 0..3 {
            let m = &mut st.members[3 * c + k];
            m.windows = u32::MAX;
            m.merit = [0; 5];
            m.merit[0] = if k < 2 {
                40_000
            } else if c < 3 {
                20_000
            } else {
                sybil
            };
        }
    }
    let roster: Vec<bool> = (0..18).map(|m| m % 3 != 2).collect();
    let pool = FEE * 18 * 8 / 10 + 12 * 5_000_000;
    let p = settle_with(
        &st,
        rules,
        pool,
        FEE,
        &Extras {
            roster: &roster,
            bounty: &[],
        },
    );
    assert_eq!(p.per_member.iter().sum::<u64>() + p.dust, pool);
    assert!((0..18).filter(|m| roster[*m]).all(|m| p.per_member[m] == 0));
    (0..6).map(|c| p.per_member[3 * c + 2]).collect()
}

#[test]
fn ai_payouts_follow_merit_on_played_seasons() {
    let unit = 4_000u32; // 10% of the AIs' 40 merit
    let (mut ratio_sum, mut ratio_n) = (0f64, 0u32);
    for i in 0..SEEDS {
        let (rules, base) = played(i);
        assert_eq!(rules.redistribute_merit_unit_bps, 1_000);
        let at = |s: u32| people(&base, &rules, s);
        let (none, token) = (at(0), at(1));
        for c in 3..6 {
            // (a) a token of merit is worth what none is, and below the fee.
            assert!(
                token[c].abs_diff(none[c]) <= 10_000,
                "seed {i}: {} vs {}",
                none[c],
                token[c]
            );
            assert!(token[c] < FEE, "seed {i}: sybil paid {}", token[c]);
        }
        // (b) non-decreasing in merit.
        let ladder: Vec<Vec<u64>> = [0, 1, unit + 1, 2 * unit, 20_000]
            .into_iter()
            .map(at)
            .collect();
        for c in 3..6 {
            assert!(
                ladder.windows(2).all(|w| w[0][c] <= w[1][c]),
                "seed {i} nation {c}: {:?}",
                ladder.iter().map(|l| l[c]).collect::<Vec<_>>()
            );
        }
        // (c) just above one unit of merit, a sybil earns about what its
        // merit is worth next to a genuine person's.
        let above = &ladder[2];
        let genuine: u64 = above[..3].iter().sum::<u64>() / 3;
        let sybil: u64 = above[3..].iter().sum::<u64>() / 3;
        ratio_sum += sybil as f64 / genuine as f64;
        ratio_n += 1;
    }
    let mean = ratio_sum / ratio_n as f64;
    let merit_ratio = (unit + 1) as f64 / 20_000.0;
    assert!(
        mean <= 1.5 * merit_ratio,
        "sybil/genuine {mean:.3} > 1.5 × {merit_ratio:.3}"
    );
}
