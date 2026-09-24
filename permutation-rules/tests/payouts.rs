//! Final prize distribution (§14.5).

use permutation_rules::genesis::{new_season, Entry};
use permutation_rules::scoring::{payouts, rankings};
use permutation_rules::state::{DeclaredKind, WorldState};
use permutation_rules::{Preset, Ruleset};

fn season(n: usize) -> (Ruleset, WorldState) {
    let rules = Ruleset::new(Preset::Blitz);
    let entries: Vec<Entry> = (0..n)
        .map(|i| Entry { name: format!("c{i}"), declared_kind: DeclaredKind::Agent, payout_wallet: [i as u8 + 1; 32], exchange_deposit: 0 })
        .collect();
    let mut s = new_season(&rules, &[3; 32], &[4; 32], &entries).unwrap();
    s.tick = rules.ticks_per_season;
    for c in &mut s.civs {
        c.active_ticks = rules.ticks_per_season; // everyone played every tick
    }
    (rules, s)
}

#[test]
fn every_unit_of_the_pool_is_paid_or_rolled_over() {
    let (rules, mut s) = season(6);
    for (i, c) in s.civs.iter_mut().enumerate() {
        c.scores.dominion = 1000 * (i as u64 + 1);
        c.scores.concord_raw = 700 * (6 - i as u64);
        c.scores.science_total = 50 * i as u64;
    }
    let pool = 60_000_001; // odd on purpose: remainders must roll over
    let p = payouts(&rules, &s, pool);
    assert_eq!(p.per_civ.iter().sum::<u64>() + p.rollover, pool);
    assert_eq!(p, payouts(&rules, &s, pool), "deterministic");
}

#[test]
fn an_entry_keeps_only_its_best_top3_placement() {
    let (rules, mut s) = season(6);
    // Civ 0 leads all three tracks.
    for (i, c) in s.civs.iter_mut().enumerate() {
        let lead = if i == 0 { 10 } else { 1 };
        c.scores.dominion = 1000 * lead + i as u64;
        c.scores.concord_raw = 1000 * lead + i as u64;
        c.scores.science_total = 100 * lead + i as u64;
    }
    let full = rankings(&rules, &s);
    assert!(full.iter().all(|r| r[0] == 0), "civ 0 first everywhere before the rule");
    let p = payouts(&rules, &s, 60_000_000);
    let top3 = |t: usize| p.tracks[t].iter().take(3).filter(|(c, _)| *c == 0).count();
    assert_eq!(top3(0) + top3(1) + top3(2), 1, "civ 0 is top-3 on exactly one track");
    // Dominion pays most (30%) and wins ties with Concord (also 30%): civ 0 keeps it.
    assert_eq!(p.tracks[0][0].0, 0);
    assert!(p.tracks[1].iter().all(|(c, _)| *c != 0) && p.tracks[2].iter().all(|(c, _)| *c != 0));
}

#[test]
fn participation_needs_activity_and_a_top_half_rank_and_is_capped() {
    let (rules, mut s) = season(6);
    for (i, c) in s.civs.iter_mut().enumerate() {
        c.scores.dominion = 100 * (i as u64 + 1);
    }
    s.civs[5].active_ticks = 10; // barely played: not eligible despite ranking first
    let pool = 1_000_000_000; // large pool so the 2 × entry-fee cap binds
    let p = payouts(&rules, &s, pool);
    assert!(!p.participation.contains(&5));
    assert!(!p.participation.is_empty());
    let cap = 2 * rules.entry_fee_usdc;
    let part_each = rules.track_share_bps[3] as u64 * pool / 10_000 / p.participation.len() as u64;
    assert!(part_each > cap, "the cap is what binds here");
    assert_eq!(p.per_civ.iter().sum::<u64>() + p.rollover, pool);
}
