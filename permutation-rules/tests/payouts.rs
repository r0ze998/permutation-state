//! Prize settlement (Game Design V5 §7).

use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::gov::{join, Path};
use permutation_rules::payout::settle;
use permutation_rules::state::WorldState;
use permutation_rules::{Preset, Ruleset};

const FEE: u64 = 10_000_000;

/// A finished season with `members[c]` members in nation `c`, all active.
fn season(members: &[usize]) -> (Ruleset, WorldState) {
    let rules = Ruleset::new(Preset::Blitz);
    let mut s = new_season(&rules, &[3; 32], &[4; 32], &nation_entries(members.len())).unwrap();
    let mut k = 0u8;
    for (civ, n) in members.iter().enumerate() {
        for _ in 0..*n {
            k = k.wrapping_add(1);
            let mut key = [0u8; 32];
            key[0] = k;
            key[1] = civ as u8;
            join(&mut s, &rules, civ as u16, key).unwrap();
        }
    }
    for m in &mut s.members {
        m.windows = (1 << 18) - 1; // active in every window
    }
    s.tick = rules.ticks_per_season;
    (rules, s)
}

fn set_tiers(s: &mut WorldState, civ: usize, wealth: u64, techs: usize) {
    let c = &mut s.civs[civ];
    c.achievements.wealth = wealth;
    for t in permutation_rules::tech::TECHS.iter().take(techs) {
        c.techs.insert(t.tech);
    }
}

#[test]
fn nothing_achieved_refunds_the_pool_equally() {
    let (rules, s) = season(&[2, 3, 0, 0, 0, 0]);
    let p = settle(&s, &rules, 40_000_003, FEE);
    // Everyone still has a capital with pop 1 and no techs: no milestone.
    assert!(p.refund);
    assert!(p.per_member.iter().all(|x| *x == 40_000_003 / 5));
    assert_eq!(p.per_member.iter().sum::<u64>() + p.dust, 40_000_003);
}

#[test]
fn nations_share_by_points_and_members_by_merit() {
    let (rules, mut s) = season(&[2, 1, 0, 0, 0, 0]);
    // Nation 0: science tier 1 → 10 points; nation 1: tier 2 → 30.
    let techs = rules.science_techs;
    set_tiers(&mut s, 0, 0, techs[0] as usize);
    set_tiers(&mut s, 1, 0, techs[1] as usize);
    // Member 0 did all of nation 0's science; member 1 only served in office.
    s.members[0].merit[Path::Science as usize] = 50_000;
    s.members[1].merit[Path::Common as usize] = 10_000;
    let pool = 80_000_000;
    let p = settle(&s, &rules, pool, FEE);
    assert!(!p.refund);
    assert_eq!(p.nation_share[0], pool * 10 / 40);
    assert_eq!(p.nation_share[1], pool * 30 / 40);
    // Nation 0: 20% equal (4M, under the 2 × 5M cap), the rest to the science merit.
    let share = p.nation_share[0];
    let equal = share / 5 / 2;
    assert_eq!(p.equal_each[0], equal);
    assert_eq!(p.per_member[0], equal + (share - 2 * equal));
    assert_eq!(p.per_member[1], equal);
    // A lone member with no merit gets the whole share of its nation.
    assert_eq!(p.per_member[2], p.nation_share[1]);
    assert_eq!(p.per_member.iter().sum::<u64>() + p.dust, pool);
}

#[test]
fn nations_without_active_members_are_not_counted() {
    let (rules, mut s) = season(&[1, 1, 0, 0, 0, 0]);
    set_tiers(&mut s, 0, 0, rules.science_techs[0] as usize);
    set_tiers(&mut s, 1, 0, rules.science_techs[0] as usize);
    s.members[1].windows = 0b1; // one window: not active
    let p = settle(&s, &rules, 1_000_000, FEE);
    assert_eq!(p.counted, vec![true, false, false, false, false, false]);
    assert_eq!(p.per_member, vec![1_000_000, 0]);
}

#[test]
fn the_equal_share_is_capped_at_half_the_fee() {
    let (rules, mut s) = season(&[3, 0, 0, 0, 0, 0]);
    set_tiers(&mut s, 0, 0, rules.science_techs[0] as usize);
    s.members[0].merit[Path::Science as usize] = 1;
    let pool = 1_000_000_000;
    let p = settle(&s, &rules, pool, FEE);
    assert_eq!(
        p.equal_each[0],
        FEE / 2,
        "20% of 1000 USDC would be far above 5 USDC each"
    );
    assert_eq!(p.per_member[1], FEE / 2);
    assert_eq!(p.per_member[0], pool - 2 * (FEE / 2));
}

#[test]
fn the_design_example_splits_as_documented() {
    // V5 §7.4: pool 1,360, points 900/620/400/400/400/(bots). Aster gets 450.
    let pool = 1_360_000_000u64;
    let points = [900u64, 620, 400, 400, 400];
    let total: u64 = points.iter().sum();
    assert_eq!(total, 2_720);
    assert_eq!(pool * 900 / total, 450_000_000);
    assert_eq!(pool * 400 / total, 200_000_000);
}
