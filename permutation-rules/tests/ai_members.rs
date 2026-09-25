//! Operator AI members, bounties and treasury contracts (V5 §18).

mod common;
use common::{free, step};
use permutation_rules::contracts::ContractTerm;
use permutation_rules::genesis::{nation_entries, new_season, Entry};
use permutation_rules::gov::{join, Path};
use permutation_rules::hex::Hex;
use permutation_rules::orders::{AttackTarget, Good, Order, Side};
use permutation_rules::payout::{settle, settle_with, Extras};
use permutation_rules::roster::{bounties, home_city, roster_chain, roster_tag};
use permutation_rules::state::{Owner, Relation, StandingRule, Unit, WorldState};
use permutation_rules::units::UnitType;
use permutation_rules::{Preset, Ruleset};

const USDC: u64 = 1_000_000;
const FEE: u64 = 10 * USDC;

fn world(treasury: u64) -> (Ruleset, WorldState) {
    let rules = Ruleset::new(Preset::Blitz);
    let entries: Vec<Entry> = (0..4)
        .map(|i| Entry {
            name: format!("civ-{i}"),
            treasury,
        })
        .collect();
    let s = new_season(&rules, &[11; 32], &[22; 32], &entries).unwrap();
    (rules, s)
}

fn place(s: &mut WorldState, civ: u16, unit_type: UnitType, troops: u32, hex: Hex) -> u32 {
    let id = s.units.len() as u32;
    s.units.push(Unit {
        id,
        owner: Owner::Civ(civ),
        unit_type,
        troops,
        hex,
        path: Vec::new(),
        last_moved: None,
        used_full_mp: false,
        standing: StandingRule::None,
        alive: true,
    });
    id
}

/// Civ 0 takes civ 1's capital (pop 3, no garrison, no defence) this tick.
fn take_capital(s: &mut WorldState, rules: &Ruleset) -> usize {
    let ci = s.civs[1].capital.unwrap() as usize;
    let hex = s.cities[ci].hex;
    for u in &mut s.units {
        if u.hex == hex && !u.unit_type.is_civilian() {
            u.alive = false;
        }
    }
    s.cities[ci].defense = 0;
    s.cities[ci].pop = 3;
    let at = hex.neighbors().into_iter().find(|h| free(s, *h)).unwrap();
    let army = place(s, 0, UnitType::Spearman, 20_000, at);
    step(
        s,
        rules,
        vec![(
            0,
            vec![Order::Attack {
                army,
                target: AttackTarget::City(ci as u32),
            }],
        )],
    );
    assert_eq!(s.cities[ci].owner, Some(0));
    ci
}

fn war(s: &mut WorldState, rules: &Ruleset, a: u16, b: u16) {
    step(s, rules, vec![(a, vec![Order::DeclareWar { civ: b }])]);
    s.civs[b as usize].protection_lost = true;
}

#[test]
fn home_cities_are_drawn_at_the_home_tick_from_the_nations_cities() {
    let (rules, mut s) = world(0);
    s.tick = rules.ai_home_tick - 1;
    step(&mut s, &rules, vec![]);
    assert!(s.home_snapshot.is_empty(), "not before the home tick");
    step(&mut s, &rules, vec![]);
    assert_eq!(s.home_snapshot.len(), 4);
    for c in 0..4u16 {
        let capital = s.civs[c as usize].capital.unwrap();
        assert_eq!(s.home_snapshot[c as usize], vec![capital]);
        assert_eq!(home_city(&s, c, &[7; 32]), Some(capital));
    }
    // With several cities the salt picks among them.
    s.home_snapshot[0] = vec![0, 4, 5, 6, 7, 8, 9, 10];
    let picks: std::collections::BTreeSet<u32> = (0..32u8)
        .map(|k| home_city(&s, 0, &[k; 32]).unwrap())
        .collect();
    assert!(picks.len() > 3, "salts spread over the cities: {picks:?}");
}

#[test]
fn a_conquest_after_the_home_tick_earns_the_bounty_once() {
    let (rules, mut s) = world(0);
    s.tick = rules.ai_home_tick;
    step(&mut s, &rules, vec![]); // records the home cities
    war(&mut s, &rules, 0, 1);
    let ci = take_capital(&mut s, &rules);
    let q = s.cities[ci].first_conquest.expect("a conquest");
    assert_eq!((q.by, q.bounty), (0, true));
    let b = bounties(&s, &[(1, [3; 32]), (2, [3; 32])], 5 * USDC);
    assert_eq!(b.homes[0], Some(ci as u32));
    assert_eq!(b.by_civ[0], 5 * USDC, "civ 1's AI lived in the capital");
    assert_eq!(b.unpaid, 5 * USDC, "civ 2's AI home was never taken");
    // Taking it again later does not move the bounty.
    let before = s.cities[ci].first_conquest;
    s.cities[ci].owner = Some(1);
    s.cities[ci].captured_from = None;
    s.civs[1].capital = Some(ci as u32);
    s.civs[1].last_city_lost = None;
    take_capital(&mut s, &rules);
    assert_eq!(s.cities[ci].first_conquest, before);
}

#[test]
fn no_bounty_before_the_home_tick_or_between_recent_pact_partners() {
    // Before the home tick: no mark at all.
    let (rules, mut s) = world(0);
    s.tick = 30;
    war(&mut s, &rules, 0, 1);
    let ci = take_capital(&mut s, &rules);
    assert_eq!(s.cities[ci].first_conquest, None);

    // A NAP between captor and victim within the window voids the bounty.
    let (rules, mut s) = world(0);
    s.tick = rules.ai_home_tick;
    step(&mut s, &rules, vec![]);
    let i = s.pair_index(0, 1);
    s.pact_last[i] = Some(s.tick - 1);
    war(&mut s, &rules, 0, 1);
    let ci = take_capital(&mut s, &rules);
    assert_eq!(s.cities[ci].first_conquest.map(|q| q.bounty), Some(false));
    let b = bounties(&s, &[(1, [3; 32])], 5 * USDC);
    assert_eq!((b.by_civ[0], b.unpaid), (0, 5 * USDC));
}

#[test]
fn pacts_are_recorded_every_tick() {
    let (rules, mut s) = world(0);
    s.set_relation(
        0,
        2,
        Relation::Nap {
            until: 100,
            bond_low: 0,
            bond_high: 0,
        },
    );
    step(&mut s, &rules, vec![]);
    let i = s.pair_index(0, 2);
    assert_eq!(s.pact_last[i], Some(0));
    assert_eq!(s.pact_last[s.pair_index(0, 1)], None);
}

#[test]
fn the_roster_chain_commits_to_tags_in_order() {
    let tags: Vec<[u8; 32]> = (0..3u8)
        .map(|k| roster_tag(42, &[k; 32], &[k + 9; 32]))
        .collect();
    let c = roster_chain(&tags);
    let mut swapped = tags.clone();
    swapped.swap(0, 1);
    assert_ne!(c, roster_chain(&swapped));
    assert_ne!(c, roster_chain(&tags[..2]));
}

// ------------------------------------------------------------- settlement

/// A finished season: `members[c]` active members in nation `c`.
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
        m.windows = (1 << 18) - 1;
    }
    for civ in 0..members.len() {
        for t in permutation_rules::tech::TECHS
            .iter()
            .take(rules.science_techs[0] as usize)
        {
            s.civs[civ].techs.insert(t.tech);
        }
    }
    s.tick = rules.ticks_per_season;
    (rules, s)
}

#[test]
fn an_ai_members_payout_goes_to_the_people_of_its_nation_by_merit() {
    let (rules, mut s) = season(&[3, 1, 0, 0, 0, 0]);
    // Members 0 (AI), 1 and 2 (people) in nation 0; member 3 in nation 1.
    s.members[0].merit[Path::Science as usize] = 60_000;
    s.members[1].merit[Path::Science as usize] = 30_000;
    s.members[2].merit[Path::Science as usize] = 10_000;
    let pool = 80 * USDC;
    let plain = settle(&s, &rules, pool, FEE);
    let roster = [true, false, false, false];
    let p = settle_with(
        &s,
        &rules,
        pool,
        FEE,
        &Extras {
            roster: &roster,
            bounty: &[],
        },
    );
    assert_eq!(
        p.nation_share, plain.nation_share,
        "nations share as before"
    );
    assert_eq!(p.per_member[0], 0);
    assert_eq!(p.redistributed, plain.per_member[0]);
    let ai = plain.per_member[0];
    // 3 : 1 by the people's merit.
    let to1 = ai * 30_000 / 40_000;
    let to2 = ai * 10_000 / 40_000;
    assert_eq!(p.per_member[1], plain.per_member[1] + to1);
    assert_eq!(p.per_member[2], plain.per_member[2] + to2);
    // Rounding remainders go to everyone by what they receive, a unit at most.
    assert!(p.per_member[3] - plain.per_member[3] <= 1);
    assert_eq!(p.per_member.iter().sum::<u64>() + p.dust, pool);
}

#[test]
fn a_nation_of_only_ai_members_is_not_counted_and_its_share_is_split_equally() {
    let (rules, s) = season(&[2, 1, 1, 0, 0, 0]);
    // Nation 0 is only AI members; nations 1 and 2 have a person each.
    let roster = [true, true, false, false];
    let pool = 60 * USDC;
    let p = settle_with(
        &s,
        &rules,
        pool,
        FEE,
        &Extras {
            roster: &roster,
            bounty: &[],
        },
    );
    assert!(!p.counted[0] && p.counted[1] && p.counted[2]);
    assert_eq!(p.nation_share[0], 0);
    // All three scored the same: nation 0's third goes half and half.
    assert_eq!(p.nation_share[1], pool / 3 + pool / 3 / 2);
    assert_eq!(p.nation_share[1], p.nation_share[2]);
    assert_eq!(p.per_member.iter().sum::<u64>() + p.dust, pool);
}

#[test]
fn without_merit_people_share_an_ai_payout_equally_up_to_the_cap() {
    let (rules, s) = season(&[2, 1, 0, 0, 0, 0]);
    // Nation 0: AI + one person with no merit; the AI's cut exceeds the cap,
    // so the excess goes to nation 1's person (by points).
    let roster = [true, false, false];
    let pool = 200 * USDC;
    let plain = settle(&s, &rules, pool, FEE);
    let p = settle_with(
        &s,
        &rules,
        pool,
        FEE,
        &Extras {
            roster: &roster,
            bounty: &[],
        },
    );
    // Nation 0's person takes at most the cap directly; nation 1's person
    // (no merit either) the cap too; the rest goes to both by what they
    // receive, and nothing returns to the operator.
    let cap = FEE * rules.equal_cap_bps as u64 / 10_000;
    assert!(p.per_member[1] >= plain.per_member[1] + cap);
    assert!(p.per_member[2] >= plain.per_member[2] + cap);
    assert_eq!(p.per_member.iter().sum::<u64>() + p.dust, pool);
    assert!(p.dust < 10, "only rounding is left: {}", p.dust);
}

#[test]
fn bounties_are_added_to_the_captor_nations_share() {
    let (rules, s) = season(&[1, 1, 1, 0, 0, 0]);
    let bounty = [7 * USDC, 0, 0, 0, 0, 3 * USDC]; // civ 5 has no members: its bounty joins the pool
    let pool = 60 * USDC;
    let p = settle_with(
        &s,
        &rules,
        pool,
        FEE,
        &Extras {
            roster: &[],
            bounty: &bounty,
        },
    );
    assert_eq!(p.bounty[0], 7 * USDC);
    assert_eq!(p.bounty[5], 0);
    let base = (pool + 3 * USDC) / 3;
    assert_eq!(p.nation_share[0], base + 7 * USDC);
    assert_eq!(p.nation_share[1], base);
    assert_eq!(p.per_member.iter().sum::<u64>() + p.dust, pool + 10 * USDC);
}

// ------------------------------------------------------------- contracts

fn offer(to: Option<u16>, term: ContractTerm, usdc: u64, deadline: u16) -> Order {
    Order::OfferContract {
        to,
        term,
        usdc,
        deadline,
    }
}

#[test]
fn a_peace_contract_pays_when_peace_is_made() {
    let (rules, mut s) = world(20 * USDC);
    war(&mut s, &rules, 0, 1);
    step(
        &mut s,
        &rules,
        vec![(0, vec![offer(Some(1), ContractTerm::Peace, 4 * USDC, 40)])],
    );
    assert_eq!(s.civs[0].usdc, 16 * USDC);
    assert_eq!(s.contracts.len(), 1);
    let id = s.contracts[0].id;
    step(
        &mut s,
        &rules,
        vec![(1, vec![Order::AcceptContract { id }])],
    );
    assert!(s.contracts[0].accepted.is_some());
    step(
        &mut s,
        &rules,
        vec![(0, vec![Order::ProposePeace { civ: 1 }])],
    );
    step(
        &mut s,
        &rules,
        vec![(1, vec![Order::AcceptPeace { civ: 0 }])],
    );
    for _ in 0..3 {
        step(&mut s, &rules, vec![]);
    }
    assert!(!s.at_war(0, 1));
    assert!(s.contracts.is_empty());
    assert_eq!(s.civs[1].usdc, 24 * USDC);
    assert_eq!(s.civs[1].contract_income, 4 * USDC);
}

#[test]
fn contract_income_cannot_be_spent_on_the_market() {
    let (rules, mut s) = world(0);
    s.civs[1].usdc = 5 * USDC;
    s.civs[1].contract_income = 5 * USDC;
    s.usdc_deposited = 5 * USDC;
    s.civs[0].iron = 50 * 1000;
    let sell = Order::ExchangeOrder {
        good: Good::Iron,
        side: Side::Sell,
        amount: 5,
        price: 100_000,
    };
    let buy = Order::ExchangeOrder {
        good: Good::Iron,
        side: Side::Buy,
        amount: 5,
        price: 100_000,
    };
    step(&mut s, &rules, vec![(0, vec![sell]), (1, vec![buy])]);
    assert_eq!(s.civs[1].usdc, 5 * USDC, "nothing bought");
    assert!(s.last_skipped.iter().any(
        |k| k.civ == 1 && k.reason == permutation_rules::checks::Blocked::NotEnoughUsdc.code()
    ));
}

#[test]
fn a_kept_nap_is_paid_in_installments_and_a_broken_one_returns_the_rest() {
    let (rules, mut s) = world(20 * USDC);
    s.set_relation(
        0,
        1,
        Relation::Nap {
            until: 150,
            bond_low: 0,
            bond_high: 0,
        },
    );
    let term = ContractTerm::KeepNap {
        every: 2,
        installments: 4,
    };
    step(
        &mut s,
        &rules,
        vec![(0, vec![offer(Some(1), term, 4 * USDC, 20)])],
    );
    let id = s.contracts[0].id;
    step(
        &mut s,
        &rules,
        vec![(1, vec![Order::AcceptContract { id }])],
    );
    for _ in 0..4 {
        step(&mut s, &rules, vec![]);
    }
    assert_eq!(s.civs[1].contract_income, 2 * USDC, "two installments of 1");
    // Civ 1 breaks the NAP: the other two return to civ 0.
    step(&mut s, &rules, vec![(1, vec![Order::BreakNap { civ: 0 }])]);
    step(&mut s, &rules, vec![]);
    assert!(s.contracts.is_empty());
    assert_eq!(s.civs[0].usdc, 18 * USDC);
    assert_eq!(s.civs[1].usdc, 22 * USDC);
}

#[test]
fn an_open_capture_offer_pays_whoever_takes_the_city() {
    let (rules, mut s) = world(20 * USDC);
    let city = s.civs[1].capital.unwrap();
    step(
        &mut s,
        &rules,
        vec![(
            2,
            vec![offer(None, ContractTerm::Capture { city }, 3 * USDC, 50)],
        )],
    );
    assert_eq!(s.civs[2].usdc, 17 * USDC);
    war(&mut s, &rules, 0, 1);
    take_capital(&mut s, &rules);
    assert!(s.contracts.is_empty());
    assert_eq!(s.civs[0].usdc, 23 * USDC);
    assert_eq!(s.civs[0].contract_income, 3 * USDC);
    // An open offer cannot be cancelled, and cannot name a counterparty.
    step(
        &mut s,
        &rules,
        vec![(
            2,
            vec![offer(Some(0), ContractTerm::Capture { city: 1 }, USDC, 60)],
        )],
    );
    assert!(s.contracts.is_empty());
}

#[test]
fn unmet_contracts_return_at_the_deadline_and_at_the_season_end() {
    let (rules, mut s) = world(20 * USDC);
    war(&mut s, &rules, 0, 1);
    step(
        &mut s,
        &rules,
        vec![(0, vec![offer(Some(1), ContractTerm::Peace, 2 * USDC, 5)])],
    );
    let id = s.contracts[0].id;
    step(
        &mut s,
        &rules,
        vec![(1, vec![Order::AcceptContract { id }])],
    );
    while s.tick <= 6 {
        step(&mut s, &rules, vec![]);
    }
    assert!(s.contracts.is_empty());
    assert_eq!(s.civs[0].usdc, 20 * USDC);

    // Whatever is escrowed at the last tick goes back.
    let city = s.civs[3].capital.unwrap();
    s.tick = rules.exchange_freeze_tick - 1;
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![offer(None, ContractTerm::Capture { city }, USDC, 170)],
        )],
    );
    assert_eq!(s.contracts.len(), 1);
    s.tick = rules.ticks_per_season - 1;
    step(&mut s, &rules, vec![]);
    assert!(s.contracts.is_empty());
    assert_eq!(s.civs[0].usdc, 20 * USDC);
}

#[test]
fn offers_respect_the_spend_limit_and_the_open_cap() {
    let (rules, mut s) = world(100 * USDC);
    war(&mut s, &rules, 0, 1);
    // Above the threshold without consent: refused.
    let big = rules.spend_consent_usdc + USDC;
    step(
        &mut s,
        &rules,
        vec![(0, vec![offer(Some(1), ContractTerm::Peace, big, 40)])],
    );
    assert!(s.contracts.is_empty());
    // With a second officer's consent: escrowed.
    step(
        &mut s,
        &rules,
        vec![(
            0,
            vec![
                offer(Some(1), ContractTerm::Peace, big, 40),
                Order::ConsentSpend { usdc: big },
            ],
        )],
    );
    assert_eq!(s.contracts.len(), 1);
    // At most `contract_max_open` open offers.
    for _ in 0..rules.contract_max_open + 1 {
        step(
            &mut s,
            &rules,
            vec![(0, vec![offer(Some(1), ContractTerm::Peace, USDC, 60)])],
        );
    }
    assert_eq!(s.contracts.len(), rules.contract_max_open as usize);
}
