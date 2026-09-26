//! Treasury governance (WP10, V5 §7.5 ④, §18.5): who may consent to
//! spending, the per-term allowance without consent, and the merit-scaled
//! limit on operator AI payouts taken by people.

mod common;
use common::nations::{act, batch, elect, idle, step, world};
use permutation_rules::checks::Blocked;
use permutation_rules::contracts::ContractTerm;
use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::gov::{self, Credit, GovAction, Path, Role, NOBODY};
use permutation_rules::markets::{spend_consent, spend_limit};
use permutation_rules::orders::{CivOrders, Good, Order, Side};
use permutation_rules::payout::{settle, settle_with, Extras};
use permutation_rules::state::WorldState;
use permutation_rules::tick::{run_phase, TickInput};
use permutation_rules::{Preset, Ruleset};

const USDC: u64 = 1_000_000;
const FEE: u64 = 10 * USDC;

fn fund(s: &mut WorldState, civ: usize, usdc: u64) {
    s.civs[civ].usdc += usdc;
    s.usdc_deposited += usdc;
}

fn buy(amount: u32, price: u64) -> Order {
    Order::ExchangeOrder {
        good: Good::Iron,
        side: Side::Buy,
        amount,
        price,
    }
}

fn sell(amount: u32, price: u64) -> Order {
    Order::ExchangeOrder {
        good: Good::Iron,
        side: Side::Sell,
        amount,
        price,
    }
}

// ------------------------------------------------------------- consent (R1)

#[test]
fn spend_limit_counts_only_a_seated_other_officer() {
    let (rules, mut s) = world(&[2, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::General]);
    elect(&s, &mut e, 1, &[Role::Diplomat]);
    gov::open_government(&mut s, &rules, &e).unwrap();
    assert_eq!(s.nations[0].holder(Role::Diplomat), 1);
    let limit = |s: &mut WorldState, officer: u32, office: Role| {
        s.tick_orders = vec![CivOrders {
            civ: 0,
            orders: vec![Order::ConsentSpend { usdc: 50 * USDC }],
            credits: vec![Credit::officer(officer)],
            origin: vec![(office as u8, 0)],
            spent: [0; 4],
        }];
        spend_limit(s, &rules, 0)
    };
    let free = rules.spend_consent_usdc;
    // (a) the caretaker (NOBODY) never consents;
    assert_eq!(limit(&mut s, NOBODY, Role::General), free);
    // (b) nor does the diplomat, from another office it holds;
    assert_eq!(limit(&mut s, 1, Role::General), free);
    // (c) a different seated officer does;
    assert_eq!(limit(&mut s, 0, Role::General), 50 * USDC);
    assert_eq!(spend_consent(&s, 0), 50 * USDC);
    // (d) never from the diplomat's own office.
    assert_eq!(limit(&mut s, 0, Role::Diplomat), free);
}

#[test]
fn the_caretaker_never_spends() {
    // Every office vacant: the caretaker runs them all.
    let (rules, mut s) = world(&[2, 0, 0, 0, 0, 0]);
    fund(&mut s, 0, 100 * USDC);
    s.civs[1].iron = 1_000 * 1_000;
    step(
        &mut s,
        &rules,
        vec![],
        vec![
            act(
                0,
                GovAction::Propose {
                    role: Role::Diplomat,
                    orders: vec![buy(10, USDC)],
                },
            ),
            act(
                1,
                GovAction::Propose {
                    role: Role::Science,
                    orders: vec![Order::ConsentSpend { usdc: 100 * USDC }],
                },
            ),
        ],
    );
    assert!(s.nations[0].proposals.is_empty(), "nothing stored");
    idle(&mut s, &rules, 1);
    let mut w = s.clone();
    let input = TickInput {
        vrf: common::vrf(w.tick),
        ..Default::default()
    };
    run_phase(&mut w, &rules, &input, 0).unwrap();
    assert!(w
        .tick_orders
        .iter()
        .all(|c| c.orders.iter().all(|o| !o.is_treasury_order())));
    assert_eq!(s.civs[0].usdc, 100 * USDC);
}

// ------------------------------------------------------------- allowance per term (R5)

/// Nation 0: a general (member 0) and a diplomat (member 1); nation 1: a
/// diplomat (member 2) who sells iron. Nation 0 holds 1,000 USDC.
fn market() -> (Ruleset, WorldState) {
    let (rules, mut s) = world(&[2, 1, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::General]);
    elect(&s, &mut e, 1, &[Role::Diplomat]);
    elect(&s, &mut e, 2, &[Role::Diplomat]);
    gov::open_government(&mut s, &rules, &e).unwrap();
    fund(&mut s, 0, 1_000 * USDC);
    s.civs[1].iron = 100_000 * 1_000;
    (rules, s)
}

#[test]
fn the_free_allowance_is_per_term() {
    let (rules, mut s) = market();
    // A lone diplomat buys one unit (about 1 USDC with fee and tariff) every
    // tick until the freeze.
    let mut term_spent = 0u64;
    let mut terms = 0;
    while s.tick < rules.exchange_freeze_tick {
        let before = s.civs[0].usdc;
        let b = vec![
            batch(&s, 0, Role::Diplomat, 1, vec![buy(1, USDC)]),
            batch(&s, 1, Role::Diplomat, 2, vec![sell(1, USDC)]),
        ];
        step(&mut s, &rules, b, vec![]);
        term_spent += before - s.civs[0].usdc;
        assert!(s.civs[0].free_spent <= rules.spend_consent_usdc);
        if s.tick % rules.term_ticks == 0 {
            assert_eq!(s.civs[0].free_spent, 0, "reset at the term boundary");
            assert!(term_spent > 0, "it buys every term");
            assert!(
                term_spent <= rules.spend_consent_usdc,
                "{term_spent} spent in a term"
            );
            term_spent = 0;
            terms += 1;
        }
    }
    assert!(terms >= 3);
}

#[test]
fn a_consent_covers_spending_without_using_the_allowance() {
    let (rules, mut s) = market();
    let b = vec![
        batch(
            &s,
            0,
            Role::General,
            0,
            vec![Order::ConsentSpend { usdc: 30 * USDC }],
        ),
        batch(&s, 0, Role::Diplomat, 1, vec![buy(1, 20 * USDC)]),
        batch(&s, 1, Role::Diplomat, 2, vec![sell(1, 20 * USDC)]),
    ];
    let before = s.civs[0].usdc;
    step(&mut s, &rules, b, vec![]);
    let spent = before - s.civs[0].usdc;
    assert!(spent > 20 * USDC, "bought: {spent}");
    assert_eq!(s.civs[0].free_spent, 0);
    // A token consent does not stop the booking.
    let b = vec![
        batch(
            &s,
            0,
            Role::General,
            0,
            vec![Order::ConsentSpend { usdc: 1 }],
        ),
        batch(&s, 0, Role::Diplomat, 1, vec![buy(1, 4 * USDC)]),
        batch(&s, 1, Role::Diplomat, 2, vec![sell(1, 4 * USDC)]),
    ];
    let (before, spent_before) = (s.civs[0].usdc, s.civs[0].market_spent);
    step(&mut s, &rules, b, vec![]);
    let spent = before - s.civs[0].usdc;
    assert!(spent >= 4 * USDC);
    assert_eq!(s.civs[0].free_spent, spent);
    assert_eq!(s.civs[0].market_spent - spent_before, spent);
}

#[test]
fn contract_escrow_uses_the_allowance() {
    let (rules, mut s) = market();
    // A peace offer needs a war.
    let war = permutation_rules::state::Relation::War {
        declared_by: 0,
        casus_belli: false,
        active_from: s.tick,
        peace_at: None,
    };
    s.set_relation(0, 1, war);
    let offer = Order::OfferContract {
        to: Some(1),
        term: ContractTerm::Peace,
        usdc: 3 * USDC,
        deadline: 40,
    };
    let b = vec![batch(&s, 0, Role::Diplomat, 1, vec![offer])];
    step(&mut s, &rules, b, vec![]);
    assert_eq!(s.contracts.len(), 1);
    assert_eq!(s.civs[0].free_spent, 3 * USDC);
    let b = vec![
        batch(&s, 0, Role::Diplomat, 1, vec![buy(1, 3 * USDC)]),
        batch(&s, 1, Role::Diplomat, 2, vec![sell(1, 3 * USDC)]),
    ];
    let before = s.civs[0].usdc;
    step(&mut s, &rules, b, vec![]);
    assert_eq!(s.civs[0].usdc, before, "nothing bought");
    assert!(s
        .last_skipped
        .iter()
        .any(|k| k.civ == 0 && k.reason == Blocked::NeedsSpendConsent.code()));
}

// ------------------------------------------------------------- redistribution (R4)

/// A finished season: `members[c]` active members in nation `c`, every
/// nation with the same techs (the same points).
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
            gov::join(&mut s, &rules, civ as u16, key).unwrap();
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

fn merit(s: &mut WorldState, m: usize, milli: u32) {
    s.members[m].merit = [0; 5];
    s.members[m].merit[Path::Science as usize] = milli;
}

/// 6 nations of 2 operator AIs (40 merit each) and a person; the people of
/// nations 0–2 have `genuine` merit, those of nations 3–5 `sybil` (milli).
/// Returns the rules, the settlement and the one without a roster.
fn farm(
    sybil: u32,
    genuine: u32,
) -> (
    Ruleset,
    permutation_rules::payout::Settlement,
    permutation_rules::payout::Settlement,
) {
    let (rules, mut s) = season(&[3; 6]);
    let mut roster = vec![false; 18];
    for civ in 0..6 {
        let base = civ * 3;
        roster[base] = true;
        roster[base + 1] = true;
        merit(&mut s, base, 40_000);
        merit(&mut s, base + 1, 40_000);
        merit(&mut s, base + 2, if civ < 3 { genuine } else { sybil });
    }
    let pool = FEE * 18 * 8 / 10;
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
    assert_eq!(p.per_member.iter().sum::<u64>() + p.dust, pool);
    (rules, p, plain)
}

#[test]
fn ai_payouts_follow_merit_without_a_cliff() {
    let unit = 4_000; // 10% of the AIs' 40 merit, in milli
                      // Members 3c, 3c + 1 are nation c's AIs, 3c + 2 its person: member 2 is
                      // a genuine person, member 11 a sybil.
    let sybil = |s: u32| farm(s, 20_000).1.per_member[11];
    let genuine = farm(20_000, 20_000).1.per_member[2];
    // (a) a token of merit is worth what none is, and below the entry fee.
    let (none, token) = (sybil(0), sybil(1));
    assert!(token.abs_diff(none) <= 10_000, "{none} vs {token}");
    assert!(token < FEE);
    // (b) non-decreasing in merit.
    let ladder: Vec<u64> = [0, 1, unit + 1, 2 * unit, 20_000]
        .into_iter()
        .map(sybil)
        .collect();
    assert!(ladder.windows(2).all(|w| w[0] <= w[1]), "{ladder:?}");
    // (c) the AI payouts a person takes stop at cap × max(1, merit / unit)
    // (genuine people with much merit take the rest: nothing is left over
    // for the by-merit remainder).
    for m in [0, 1, unit + 1, 2 * unit] {
        let (rules, p, plain) = farm(m, 200_000);
        let cap = FEE * rules.equal_cap_bps as u64 / 10_000;
        let limit = (cap as u128 * m as u128 / unit as u128).max(cap as u128) as u64;
        let taken = p.per_member[11] - plain.per_member[11];
        assert!(taken <= limit, "merit {m}: took {taken} > {limit}");
        assert!(taken + 2 >= limit, "merit {m}: took {taken}, limit {limit}");
    }
    // (d) at equal merit, sybil and genuine persons are paid alike.
    assert_eq!(ladder[4], genuine);
}

#[test]
fn still_left_goes_by_merit() {
    // Nation 0: an AI (40 merit) and a person without merit; nation 1: a
    // person with a token of merit. Both people reach their cap; the rest
    // goes by merit, so the person without merit keeps exactly the cap.
    let (rules, mut s) = season(&[2, 1, 0, 0, 0, 0]);
    merit(&mut s, 0, 40_000);
    merit(&mut s, 1, 0);
    merit(&mut s, 2, 1);
    let roster = [true, false, false];
    let pool = 400 * USDC;
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
    let cap = FEE * rules.equal_cap_bps as u64 / 10_000;
    assert_eq!(p.per_member[0], 0);
    assert_eq!(p.per_member[1], plain.per_member[1] + cap);
    assert!(p.per_member[2] > plain.per_member[2] + cap);
    assert!(p.dust < 10, "{}", p.dust);
    assert_eq!(p.per_member.iter().sum::<u64>() + p.dust, pool);
}

#[test]
fn no_roster_is_unchanged() {
    let (rules, mut s) = season(&[3, 2, 1, 0, 0, 0]);
    for m in 0..6 {
        merit(&mut s, m, 1_000 * m as u32);
    }
    let pool = 48 * USDC;
    let plain = settle(&s, &rules, pool, FEE);
    let none = settle_with(
        &s,
        &rules,
        pool,
        FEE,
        &Extras {
            roster: &[false; 6],
            bounty: &[],
        },
    );
    assert_eq!(plain, none);
    assert_eq!(plain.redistributed, 0);
}
