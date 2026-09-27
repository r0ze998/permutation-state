//! Frontier kernel bounds (M1 closeout, W1-B): CL-02 walls, production and
//! upkeep caps; CL-03 checked `duplicate_cost`; CL-04 ring-bounded
//! coordinates (also in `frontier_world::coordinates_are_bounded`); CL-05
//! monotone `Stamina::set`; CL-06 faction ids; CL-08 the per-faction ledger;
//! CL-09 vigil changes at a UTC midnight; CL-11 `validate_for_season`;
//! CL-12 no default Works rate; CL-13 the closed-form `accrual_left`; CL-15
//! the Mandate claim deadline and the final sweep; CL-18 γ > 1.
//!
//! A bound that only refuses invalid input leaves every honest outcome
//! bit-identical: the existing suites (`frontier_world`,
//! `frontier_economy`) are unchanged in what they assert. Seeded
//! generators only (no proptest dependency).

use permutation_rules::fixed::{BPS_ONE, MILLI};
use permutation_rules::frontier::clash::{FACTION_LIMIT, NEUTRAL};
use permutation_rules::frontier::geometry::{
    provinces_within, valid_faction, ProvinceCoord, FACTIONS as PLAYER_FACTIONS, R_MAX_HARD,
};
use permutation_rules::frontier::holding::{
    duplicate_cost, Accrual, Effect, Holding, HoldingError, Resource, DAY, HOUR, MAX_DUPLICATES,
    MAX_PRODUCTION_PER_HOUR, MAX_UPKEEP_PER_HOUR, MAX_WALLS, RESOURCES,
};
use permutation_rules::frontier::host::{Host, HostError, Stamina, STAMINA_CAP};
use permutation_rules::frontier::index::{pow_frac, IndexParams, FACTIONS, INDEX_ONE};
use permutation_rules::frontier::mandate::{
    claim_deadline, share_floor, MandateTerm, Reserve, BANKING_WINDOW_SECS, MANDATE_CAP_PER_TERM,
    TERM_SECS,
};
use permutation_rules::frontier::payout::{
    claim, settle, CitizenRecord, FactionTotals, PayoutParams, SeasonLedger, Settlement,
};
use permutation_rules::frontier::pools::{EconError, Entry, EntrySchedule, Pools, USDC};
use permutation_rules::frontier::siege::{
    bells_outside_vigil, change_effective_at, may_besiege, required_bells, BellReport, HoldingKind,
    Relation, SiegeCheck, SiegeRefusal, Vigil, MAX_REQUIRED_BELLS, VIGIL_NOTICE, VIGIL_SECS,
};
use permutation_rules::units::UnitType;

/// splitmix64 (as in `frontier_world`).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

/// A UTC midnight.
const GENESIS: i64 = 1_790_000_000 - 1_790_000_000 % DAY;

// ------------------------------------------------------------ CL-02

fn assert_capped(h: &Holding, t: i64, ctx: &str) {
    for r in 0..RESOURCES {
        assert!(
            (0..=MAX_PRODUCTION_PER_HOUR).contains(&h.production[r]),
            "{ctx}: production {}",
            h.production[r]
        );
        assert!(
            (0..=MAX_UPKEEP_PER_HOUR).contains(&h.upkeep[r]),
            "{ctx}: upkeep {}",
            h.upkeep[r]
        );
    }
    assert!(h.walls <= MAX_WALLS, "{ctx}: walls {}", h.walls);
    if let Ok(w) = h.walls_at(t) {
        assert!(w <= MAX_WALLS, "{ctx}: walls_at {w}");
    }
}

fn random_effect(r: &mut Rng) -> Effect {
    let resource = Resource::ALL[r.below(RESOURCES as u64) as usize];
    // Mostly honest sizes, sometimes garbage at the extremes.
    let delta = |r: &mut Rng, cap: i64| -> i64 {
        match r.below(10) {
            0 => i64::MAX,
            1 => i64::MIN,
            2 => cap,
            3 => -cap,
            4 => r.below(cap as u64 + 1) as i64,
            _ => r.below(20_000) as i64 - 2_000,
        }
    };
    match r.below(4) {
        0 => Effect::Production {
            resource,
            delta: delta(r, MAX_PRODUCTION_PER_HOUR),
        },
        1 => Effect::Upkeep {
            resource,
            delta: delta(r, MAX_UPKEEP_PER_HOUR),
        },
        2 => Effect::Walls {
            delta: match r.below(6) {
                0 => u32::MAX,
                1 => MAX_WALLS,
                2 => r.below(MAX_WALLS as u64 + 1) as u32,
                _ => 100,
            },
        },
        _ => Effect::TierUp,
    }
}

/// CL-02: a seeded fuzz of 10,000 effect sequences. No field ever passes
/// its cap, an effect that would take a field past its cap (counting
/// everything already queued) is refused, and nothing overflows (run it in
/// a debug build too: overflow checks are on there).
#[test]
fn holding_effects_are_capped() {
    let mut r = Rng(0xC1_02);
    let (mut refused, mut accepted) = (0u32, 0u32);
    for case in 0..10_000u32 {
        let t0 = GENESIS + r.below(DAY as u64) as i64;
        let mut h = Holding::found(t0, 0, 1 + r.below(3) as u8);
        let mut t = t0;
        for step in 0..12 {
            t += r.below(8 * HOUR as u64) as i64;
            let ctx = format!("case {case} step {step}");
            match r.below(5) {
                0 | 1 => {
                    let e = random_effect(&mut r);
                    let before = h.clone();
                    match h.enqueue(t, r.below(6 * HOUR as u64) as i64, e) {
                        Ok(_) => accepted += 1,
                        Err(HoldingError::AboveCap) => {
                            refused += 1;
                            // A refusal changes nothing but the settle.
                            let mut settled = before;
                            settled.settle(t).unwrap();
                            assert_eq!(h, settled, "{ctx}");
                        }
                        Err(_) => {}
                    }
                }
                2 => {
                    let per = match r.below(4) {
                        0 => i64::MAX,
                        1 => i64::MIN,
                        _ => r.below(100_000) as i64,
                    };
                    h.set_upkeep(t, Resource::Food, per).unwrap();
                }
                3 => h.commit_walls(t),
                _ => h.settle(t).unwrap(),
            }
            assert_capped(&h, t, &ctx);
        }
        // Everything finishes: still capped.
        let end = t + 28 * DAY;
        h.settle(end).unwrap();
        h.commit_walls(end);
        assert_capped(&h, end, &format!("case {case} end"));
    }
    assert!(
        refused > 1_000 && accepted > 1_000,
        "{refused} / {accepted}"
    );
}

/// CL-02: the enqueue refusal, item by item, and the walls a clash reads.
#[test]
fn enqueue_refuses_effects_past_the_caps() {
    let mut h = Holding::found(GENESIS, 0, 1);
    // Tier up to a City so there are 4 queue slots.
    h.enqueue(GENESIS, 0, Effect::TierUp).unwrap();
    h.enqueue(GENESIS, 0, Effect::TierUp).unwrap();
    h.settle(GENESIS + 1).unwrap();
    let t = GENESIS + 1;
    // Walls: the queued items count.
    h.enqueue(t, HOUR, Effect::Walls { delta: 700 }).unwrap();
    assert_eq!(
        h.enqueue(t, HOUR, Effect::Walls { delta: 501 }),
        Err(HoldingError::AboveCap)
    );
    h.enqueue(t, HOUR, Effect::Walls { delta: 500 }).unwrap();
    assert_eq!(
        h.enqueue(t, HOUR, Effect::Walls { delta: 1 }),
        Err(HoldingError::AboveCap)
    );
    assert_eq!(h.walls_at(GENESIS + 10 * HOUR).unwrap(), MAX_WALLS);
    // Production and upkeep: a single garbage delta, and the running sum.
    let food = Resource::Food;
    assert_eq!(
        h.enqueue(
            t,
            HOUR,
            Effect::Production {
                resource: food,
                delta: MAX_PRODUCTION_PER_HOUR + 1
            }
        ),
        Err(HoldingError::AboveCap)
    );
    assert_eq!(
        h.enqueue(
            t,
            HOUR,
            Effect::Upkeep {
                resource: food,
                delta: i64::MIN
            }
        ),
        Err(HoldingError::AboveCap)
    );
    h.enqueue(
        t,
        HOUR,
        Effect::Production {
            resource: food,
            delta: MAX_PRODUCTION_PER_HOUR,
        },
    )
    .unwrap();
    h.settle(GENESIS + DAY).unwrap();
    h.commit_walls(GENESIS + DAY);
    assert_eq!(h.walls, MAX_WALLS);
    assert_eq!(h.production[food as usize], MAX_PRODUCTION_PER_HOUR);
    // Upkeep set directly is clamped too.
    h.set_upkeep(GENESIS + DAY, food, i64::MAX).unwrap();
    assert_eq!(h.upkeep[food as usize], MAX_UPKEEP_PER_HOUR);
    // A siege against the most walls needs a bounded number of bells.
    assert_eq!(required_bells(MAX_WALLS, 0), 36 + 24);
    assert_eq!(required_bells(u32::MAX, 12), required_bells(MAX_WALLS, 12));
    assert_eq!(MAX_REQUIRED_BELLS, 84);
    assert!(required_bells(u32::MAX, 24) <= MAX_REQUIRED_BELLS);
    assert_eq!(required_bells(0, u32::MAX), u32::MAX, "saturates, finite");
    // Honest values are what they were: 36 + walls / 50 + extra.
    assert_eq!(required_bells(600, 12), 36 + 12 + 12);
}

/// CL-02: `rate × 28 days` at the caps stays inside `i64`, for a store and
/// for a whole holding whose fields were written directly past the caps.
#[test]
fn accruals_at_the_caps_do_not_overflow() {
    let secs = 28 * DAY;
    let hours = secs / HOUR;
    for rate in [MAX_PRODUCTION_PER_HOUR, -MAX_UPKEEP_PER_HOUR] {
        let mut a = Accrual::new(0, rate, i64::MAX, 0);
        let short = a.settle(secs);
        if rate > 0 {
            assert_eq!(a.value, rate * hours);
        } else {
            assert_eq!(short, -rate * hours);
        }
    }
    let mut h = Holding::found(GENESIS, 0, 1);
    h.production = [i64::MAX; RESOURCES];
    h.upkeep = [i64::MAX; RESOURCES];
    h.set_upkeep(GENESIS, Resource::Wood, i64::MAX).unwrap();
    h.touch_owner(GENESIS).unwrap();
    h.settle(GENESIS + secs).unwrap();
    // Every rate was read through the caps (dormant by now: half the
    // production).
    for r in 0..RESOURCES {
        assert!(h.stores[r].rate >= -MAX_UPKEEP_PER_HOUR);
        assert!(h.stores[r].rate <= MAX_PRODUCTION_PER_HOUR - MAX_UPKEEP_PER_HOUR);
    }
    // Troop upkeep saturates instead of wrapping (integ-W1 review): a
    // thousand max-size entries, then u32::MAX troops each.
    use permutation_rules::frontier::holding::troop_upkeep_per_hour;
    let small = troop_upkeep_per_hour(&[(UnitType::Knight, 30_000 * MILLI as u32)]);
    assert!(small > 0 && small < i64::MAX);
    let many = vec![(UnitType::Knight, 30_000 * MILLI as u32); 1_000];
    let big = troop_upkeep_per_hour(&many);
    assert!(big >= small, "monotone, no wrap to a negative rate");
    let huge = vec![(UnitType::Knight, u32::MAX); 100_000];
    assert_eq!(troop_upkeep_per_hour(&huge), i64::MAX);
    h.set_upkeep(GENESIS + secs, Resource::Food, troop_upkeep_per_hour(&huge))
        .unwrap();
    assert!(h.stores[Resource::Food as usize].rate >= -MAX_UPKEEP_PER_HOUR);
}

// ------------------------------------------------------------ CL-03

#[test]
fn duplicate_cost_is_checked() {
    assert_eq!(duplicate_cost(u64::MAX, u32::MAX), None);
    assert_eq!(duplicate_cost(100, MAX_DUPLICATES + 1), None);
    assert_eq!(duplicate_cost(u64::MAX, 2), None, "overflow is refused");
    assert_eq!(duplicate_cost(u64::MAX, 1), None, "×2 then /2 overflows");
    let k = (MAX_DUPLICATES - 1) as u64;
    assert_eq!(
        duplicate_cost(1_000, MAX_DUPLICATES),
        Some(1_000 * (2 + k * k) / 2)
    );
    // The existing values are kept (Eternum's rule).
    for (n, want) in [(1, 100), (2, 150), (3, 300), (4, 550), (13, 7_300)] {
        assert_eq!(duplicate_cost(100, n), Some(want), "n = {n}");
    }
    // Every copy up to the cap, for every base the catalog uses, is Some.
    for base in [20u64, 40, 60, 80, 200, 400, 800] {
        for n in 0..=MAX_DUPLICATES {
            let k = n.saturating_sub(1) as u64;
            assert_eq!(duplicate_cost(base, n), Some(base * (2 + k * k) / 2));
        }
    }
}

// ------------------------------------------------------------ CL-04

#[test]
fn province_coordinates_are_checked() {
    // No i32 pair panics or wraps.
    let ext = [i32::MIN, i32::MIN + 1, -1, 0, 1, i32::MAX - 1, i32::MAX];
    for &p in &ext {
        for &q in &ext {
            let c = ProvinceCoord::new(p, q);
            let ring = c.ring();
            let ok = ProvinceCoord::checked(p, q, R_MAX_HARD);
            assert_eq!(ok.is_ok(), ring <= R_MAX_HARD as u32, "({p}, {q})");
            assert_eq!(c.checked_index().is_some(), ring <= R_MAX_HARD as u32);
        }
    }
    assert_eq!(ProvinceCoord::new(i32::MIN, i32::MIN).ring(), u32::MAX);
    assert_eq!(
        ProvinceCoord::new(i32::MAX, i32::MIN).ring(),
        i32::MAX as u32 + 1
    );
    // Ring 128 accepted, 129 refused; r_max caps at 128.
    assert!(ProvinceCoord::checked(128, 0, 128).is_ok());
    assert!(ProvinceCoord::checked(-64, -64, 128).is_ok());
    assert!(ProvinceCoord::checked(129, 0, 128).is_err());
    assert!(ProvinceCoord::checked(129, 0, u16::MAX).is_err());
    assert!(ProvinceCoord::checked(65, 0, 64).is_err());
    assert!(ProvinceCoord::checked(0, -64, 64).is_ok());
    // The index round-trips over every province within ring 128.
    let n = provinces_within(R_MAX_HARD as u32);
    assert_eq!(n, 49_537);
    for i in 0..n {
        let p = ProvinceCoord::checked_from_index(i).unwrap();
        assert!(ProvinceCoord::checked(p.p, p.q, R_MAX_HARD).is_ok());
        assert_eq!(p.checked_index(), Some(i));
    }
    assert_eq!(ProvinceCoord::checked_from_index(n), None);
    assert_eq!(ProvinceCoord::checked_from_index(u32::MAX), None);
    // The unchecked forms are total too (integ-W1): beyond ring 128
    // `index` gives u32::MAX and never wraps, even at the i32 extremes.
    for (p, q) in [(129, 0), (i32::MAX, 0), (i32::MIN, i32::MAX), (-40_000, 1)] {
        assert_eq!(ProvinceCoord::new(p, q).index(), u32::MAX, "({p}, {q})");
        assert_eq!(ProvinceCoord::new(p, q).checked_index(), None);
    }
    assert_eq!(
        ProvinceCoord::new(128, 0).index(),
        n - 6 * 128,
        "first of ring 128"
    );
}

/// `from_index` beyond ring 128 is a caller bug: a debug build asserts
/// (it can no longer overflow `provinces_within`), a release build gives
/// the Concord.
#[test]
fn from_index_beyond_ring_128_is_refused() {
    let r = std::panic::catch_unwind(|| ProvinceCoord::from_index(u32::MAX));
    if cfg!(debug_assertions) {
        assert!(r.is_err(), "debug build asserts");
    } else {
        assert_eq!(r.ok(), Some(ProvinceCoord::CONCORD));
    }
}

// ------------------------------------------------------------ CL-05

#[test]
fn stamina_set_refuses_an_earlier_bell() {
    let mut s = Stamina {
        value: 40,
        bell: 100,
    };
    assert_eq!(s.set(99, 120), Err(HostError::TimeReversed));
    assert_eq!(
        s,
        Stamina {
            value: 40,
            bell: 100
        },
        "nothing changed"
    );
    s.set(100, 7).unwrap();
    assert_eq!(
        s,
        Stamina {
            value: 7,
            bell: 100
        },
        "the same bell is a replace"
    );
    s.set(130, 500).unwrap();
    assert_eq!(
        s,
        Stamina {
            value: STAMINA_CAP,
            bell: 130
        }
    );
}

/// The first-pass scenario: a spend at a later bell, then a set for an
/// earlier bell. Before CL-05 the set landed at the later bell and erased
/// the spend.
#[test]
fn stamina_spend_then_late_set() {
    let mut s = Stamina {
        value: 100,
        bell: 10,
    };
    s.spend(20, 60).unwrap(); // refilled to 110, then 60 spent
    assert_eq!(s.at(20), 50);
    assert_eq!(s.set(15, 120), Err(HostError::TimeReversed));
    assert_eq!(s.at(20), 50, "the later spend stands");

    // The same through a host: a Depart's march stamina is charged after
    // the origin's clash (settle at bell 6); a late duplicate result for
    // bell 5 is refused and changes nothing.
    let mut h = Host::muster(1, 9, 0, UnitType::Spearman, 1_000 * MILLI as u32, 0).unwrap();
    h.depart(5, 30, 5).unwrap();
    h.apply_clash(5, 900 * MILLI as u32, 100, false).unwrap();
    assert!(h.settle(6).unwrap().is_none());
    // Refilled by one bell (100 → 101), then the march's 30.
    assert_eq!(h.stamina, Stamina { value: 71, bell: 6 });
    let before = h;
    assert_eq!(
        h.apply_clash(5, 1_000 * MILLI as u32, STAMINA_CAP, false),
        Err(HostError::TimeReversed)
    );
    assert_eq!(h, before, "a refused result changes nothing");
    assert_eq!(h.route(4), Err(HostError::TimeReversed));
    assert_eq!(h, before);
    h.route(8).unwrap();
    assert_eq!(h.stamina, Stamina { value: 0, bell: 8 });
}

// ------------------------------------------------------------ CL-06

#[test]
fn faction_ids_are_limited() {
    assert_eq!(NEUTRAL, 6);
    assert_eq!(PLAYER_FACTIONS, 6);
    for f in 0..=u8::MAX {
        assert_eq!(valid_faction(f, false), f <= 5, "{f}");
        assert_eq!(valid_faction(f, true), f <= 6, "{f}");
        // Every accepted id indexes the kernels' fixed arrays.
        if valid_faction(f, true) {
            assert!(f < FACTION_LIMIT);
        }
    }
    // Sieges: a faction read from data outside 0..=5 is refused (the owner
    // of a Free City may be NEUTRAL).
    let base = SiegeCheck {
        province: ProvinceCoord::new(5, 2),
        kind: HoldingKind::Other,
        owner_faction: 1,
        attacker_faction: 2,
        relation: Relation::War,
        march_hostility: false,
        march_truce: false,
        founded_ts: 0,
        founded_day: 0,
        shield_until: 0,
        dormant: false,
        attacker_nearby: true,
        now: 10 * DAY,
    };
    assert_eq!(may_besiege(&base), Ok(()));
    for bad in [6u8, 7, 8, 255] {
        let c = SiegeCheck {
            attacker_faction: bad,
            ..base
        };
        assert_eq!(may_besiege(&c), Err(SiegeRefusal::BadFaction), "{bad}");
    }
    for bad in [6u8, 7, 255] {
        let c = SiegeCheck {
            owner_faction: bad,
            ..base
        };
        assert_eq!(may_besiege(&c), Err(SiegeRefusal::BadFaction), "{bad}");
    }
    let free_city = SiegeCheck {
        kind: HoldingKind::FreeCity,
        owner_faction: NEUTRAL,
        ..base
    };
    assert_eq!(may_besiege(&free_city), Ok(()));
    assert_eq!(
        may_besiege(&SiegeCheck {
            owner_faction: 7,
            ..free_city
        }),
        Err(SiegeRefusal::BadFaction)
    );
    // A bell report never counts faction 7, whatever its bit says.
    let report = BellReport {
        holders: 0xFF,
        defender_present: false,
    };
    assert!(report.holds(5) && report.holds(NEUTRAL));
    assert!(!report.holds(7) && !report.holds(8) && !report.holds(255));
}

// ------------------------------------------------------------ CL-08

fn pools_for(recs: &[CitizenRecord]) -> Pools {
    let sched = EntrySchedule::FRONTIER_28;
    let mut p = Pools::default();
    for r in recs {
        p.pay_settled(&sched, Entry::CitizenFee, r.fee).unwrap();
        if r.stake > 0 {
            p.pay_settled(&sched, Entry::LaurelStake, r.stake).unwrap();
        }
    }
    p
}

fn settlement_for(recs: &[CitizenRecord], p: &PayoutParams) -> Settlement {
    let mut t = [FactionTotals::default(); FACTIONS];
    for r in recs {
        t[r.faction as usize]
            .add(&r.weights_at(p.works_per_usdc))
            .unwrap();
    }
    settle(&pools_for(recs), &t, &[INDEX_ONE; FACTIONS], 0, p).unwrap()
}

/// CL-08: an over-claim in faction 2, offset by an under-claim in faction
/// 4, keeps the season total within the prize but is refused: faction 2
/// would draw on faction 4's pots.
#[test]
fn ledger_is_per_faction() {
    let p = PayoutParams::REV3;
    let mut recs = Vec::new();
    for f in [2u8, 2, 4, 4, 4] {
        recs.push(CitizenRecord {
            faction: f,
            fee: 4 * USDC,
            stake: 6 * USDC,
            tenure: 12_500,
            works: 300,
            laurels: 100,
            ..Default::default()
        });
    }
    let st = settlement_for(&recs, &p);
    let mut ledger = SeasonLedger::new(&st).unwrap();
    // Wallet 1 (faction 2) and wallets 2–3 (faction 4) claim honestly;
    // wallet 4 of faction 4 leaves its claim unclaimed.
    let honest: Vec<_> = recs.iter().map(|r| claim(&st, r).unwrap()).collect();
    for i in 1..4 {
        ledger.record(recs[i].faction, &honest[i]).unwrap();
    }
    let unclaimed = honest[4].paid;
    assert!(unclaimed > 0);
    // Wallet 0 of faction 2 over-claims by exactly what faction 4 left.
    let mut forged = honest[0];
    forged.citizen += unclaimed;
    forged.gross += unclaimed;
    forged.paid += unclaimed;
    let before = ledger;
    assert!(
        ledger.claimed + ledger.swept + forged.paid + forged.swept <= ledger.prize,
        "the season total alone would accept it"
    );
    assert_eq!(ledger.record(2, &forged), Err(EconError::Underflow));
    assert_eq!(ledger, before, "a refused claim changes nothing");
    // A claim whose parts do not add up is refused too.
    let mut odd = honest[4];
    odd.paid += 1;
    assert_eq!(ledger.record(4, &odd), Err(EconError::Underflow));
    assert_eq!(ledger.record(6, &honest[4]), Err(EconError::BadFaction));
    // The honest ones are accepted, and the season books balance exactly.
    ledger.record(2, &honest[0]).unwrap();
    ledger.record(4, &honest[4]).unwrap();
    assert_eq!(ledger.claimed + ledger.swept + ledger.dust(), ledger.prize);
    for k in [2usize, 4] {
        let f = ledger.factions[k];
        assert!(f.drawn_citizen <= f.pot_citizen && f.drawn_laurel <= f.pot_laurel);
    }
    let per_faction: u64 = ledger.factions.iter().map(|f| f.claimed + f.swept).sum();
    assert_eq!(
        per_faction + ledger.civ_drawn,
        ledger.claimed + ledger.swept
    );
}

// ------------------------------------------------------------ CL-09

#[test]
fn vigil_change_lands_on_a_midnight() {
    for step in 0..(2 * DAY / 60) {
        let now = GENESIS + 3 * DAY + step * 60;
        let mut v = Vigil::new(0).unwrap();
        let from = v.request_change(now, 10 * 3_600).unwrap();
        assert_eq!(from.rem_euclid(DAY), 0, "now {now}");
        assert!(from >= now + VIGIL_NOTICE && from < now + VIGIL_NOTICE + DAY);
        assert_eq!(from, change_effective_at(now));
    }
    // Exactly 24 h before a midnight: that midnight.
    assert_eq!(change_effective_at(GENESIS - DAY), GENESIS);
    assert_eq!(change_effective_at(GENESIS - DAY + 1), GENESIS + DAY);
}

/// The longest covered stretch (seconds) of `covers` over
/// `[t0, t0 + span)`, sampled every `step` seconds.
fn longest_stretch(covers: impl Fn(i64) -> bool, t0: i64, span: i64, step: i64) -> i64 {
    let (mut best, mut run) = (0, 0);
    let mut t = t0;
    while t < t0 + span {
        if covers(t) {
            run += step;
            best = best.max(run);
        } else {
            run = 0;
        }
        t += step;
    }
    best
}

/// The rule CL-09 asked for taken literally: the schedule switches at the
/// midnight and every instant is judged by the time of day in force then.
/// It still joins a window that ends at (or wraps past) midnight to one
/// that starts there: the control this test must reject.
fn naive_covers(old: u32, new: u32, from: i64, ts: i64) -> bool {
    let start = if ts < from { old } else { new } as i64;
    (ts.rem_euclid(DAY) - start).rem_euclid(DAY) < VIGIL_SECS
}

#[test]
fn no_vigil_window_exceeds_8h_across_a_change() {
    let step = 300;
    // Every pair of starts on a 30-minute grid, one change each.
    for old in (0..DAY).step_by(1_800) {
        for new in (0..DAY).step_by(1_800) {
            let now = GENESIS + 3 * DAY + 5_000;
            let mut v = Vigil::new(old as u32).unwrap();
            let from = v.request_change(now, new as u32).unwrap();
            let longest = longest_stretch(|t| v.covers(t), from - 2 * DAY, 5 * DAY, step);
            assert!(longest <= VIGIL_SECS, "{old} -> {new}: {longest}");
            // The schedule is in force: a full window a day later.
            let ws = from + DAY + new;
            assert!(v.covers(ws) && v.covers(ws + VIGIL_SECS - 1) && !v.covers(ws + VIGIL_SECS));
        }
    }
    // The worst pairs, with `now` swept over a day at 60-s steps.
    for (old, new) in [
        (16 * 3_600, 0),
        (20 * 3_600, 2 * 3_600),
        (0, 8 * 3_600 - 60),
    ] {
        for k in 0..(DAY / 60) {
            let now = GENESIS + 10 * DAY + k * 60;
            let mut v = Vigil::new(old).unwrap();
            let from = v.request_change(now, new).unwrap();
            let longest = longest_stretch(|t| v.covers(t), from - DAY, 3 * DAY, step);
            assert!(longest <= VIGIL_SECS, "{old} -> {new} at {now}: {longest}");
        }
    }
    // The control: without the window rule a 16:00 → 00:00 change makes
    // one 16-hour vigil.
    let from = change_effective_at(GENESIS + 3 * DAY);
    let naive = longest_stretch(
        |t| naive_covers(16 * 3_600, 0, from, t),
        from - 2 * DAY,
        5 * DAY,
        step,
    );
    assert_eq!(naive, 16 * HOUR, "the control must find the long vigil");
}

/// A model of the vigil across one change, with the first new window's
/// skip rule as a parameter: `skip(prev, ws)` says whether the new
/// schedule's first window (starting at `ws`) is dropped, `prev` being the
/// last old window's start.
fn model_covers(old: i64, new: i64, from: i64, skip: impl Fn(i64, i64) -> bool, t: i64) -> bool {
    let prev = from - DAY + old; // the last old window starting before `from`
    let first = from + new;
    let in_old = {
        let ws = t - (t - old).rem_euclid(DAY);
        ws < from && t < ws + VIGIL_SECS
    };
    let in_new = {
        let ws = t - (t - first).rem_euclid(DAY);
        ws >= first && t < ws + VIGIL_SECS && !(ws == first && skip(prev, first))
    };
    in_old || in_new
}

/// Longest run of covered bell starts, and the most covered bell starts in
/// any 144 consecutive bells, over `n` bells from `b0`.
fn bell_runs(covers: impl Fn(i64) -> bool, genesis: i64, b0: i64, n: usize) -> (usize, usize) {
    let c: Vec<bool> = (0..n as i64)
        .map(|k| covers(genesis + (b0 + k) * 600))
        .collect();
    let (mut run, mut longest) = (0, 0);
    for &x in &c {
        run = if x { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    let mut most = 0;
    let mut w: usize = c[..144].iter().filter(|&&x| x).count();
    most = most.max(w);
    for i in 144..c.len() {
        w = w + c[i] as usize - c[i - 144] as usize;
        most = most.max(w);
    }
    (longest, most)
}

/// CL-09 at the granularity a siege counts (integ-W1 review): bells start
/// every 600 s at a genesis offset that is not a multiple of 600, changes
/// are requested on a 60-s grid, and across any change no run of covered
/// bell starts exceeds 48 and no 144 consecutive bells hold more than 48
/// covered ones. The kernel equals the model with the day rule. Controls:
/// the naive midnight switch and the first window rule (skip only if the
/// new window starts before the old one ends) both give 96 consecutive
/// covered bells for 16:00 → 00:01.
#[test]
fn no_vigil_covers_more_than_48_bells_across_a_change() {
    let genesis = GENESIS + 7 * 60 + 13; // hh:m7:13, not a bell boundary
    let now = GENESIS + 3 * DAY + 5_000;
    let day_rule = |prev: i64, ws: i64| ws < prev + DAY;
    let check = |old: i64, new: i64| {
        let mut v = Vigil::new(old as u32).unwrap();
        let from = v.request_change(now, new as u32).unwrap();
        let b0 = (from - 2 * DAY - genesis) / 600;
        let n = 5 * 144;
        let (longest, most) = bell_runs(|t| v.covers(t), genesis, b0, n);
        assert!(
            longest <= 48,
            "{old} -> {new}: {longest} covered bells in a row"
        );
        assert!(most <= 48, "{old} -> {new}: {most} covered bells in a day");
        for k in 0..n as i64 {
            let t = genesis + (b0 + k) * 600;
            assert_eq!(
                v.covers(t),
                model_covers(old, new, from, day_rule, t),
                "{old} -> {new} at bell {}",
                b0 + k
            );
        }
        // The new schedule is in force from the second day on.
        let ws = from + DAY + new;
        assert!(v.covers(ws) && v.covers(ws + VIGIL_SECS - 1) && !v.covers(ws + VIGIL_SECS));
    };
    // Every pair on a 30-minute grid.
    for old in (0..DAY).step_by(1_800) {
        for new in (0..DAY).step_by(1_800) {
            check(old, new);
        }
    }
    // 60-s grids around the two edges: the new window starting near the old
    // one's end (new ≈ old + 8 h) and near the old start (new ≈ old, where
    // the skip switches), for every old start on a 60-s grid.
    for old in (0..DAY).step_by(60) {
        for d in (-600..=600).step_by(60) {
            for new in [old + VIGIL_SECS + d, old + d] {
                check(old, new.rem_euclid(DAY));
            }
        }
    }
    // Controls.
    let from = change_effective_at(now);
    let b0 = (from - 2 * DAY - genesis) / 600;
    let naive = |t| naive_covers(16 * 3_600, 0, from, t);
    assert_eq!(bell_runs(naive, genesis, b0, 720).0, 96, "naive midnight");
    let first_rule = |prev: i64, ws: i64| ws <= prev + VIGIL_SECS;
    for new in [60, 0] {
        let old = 16 * 3_600 - if new == 0 { 60 } else { 0 };
        let r = bell_runs(
            |t| model_covers(old, new, from, first_rule, t),
            genesis,
            b0,
            720,
        );
        assert_eq!(r.0, 96, "first rule {old} -> {new}");
        let r = bell_runs(
            |t| model_covers(old, new, from, day_rule, t),
            genesis,
            b0,
            720,
        );
        assert!(r.0 <= 48 && r.1 <= 48, "day rule {old} -> {new}: {r:?}");
    }
}

/// The O(1) count of bells outside the vigil equals counting bell by bell
/// across changes (the day after a change is not periodic).
#[test]
fn bells_outside_vigil_matches_bell_by_bell_across_a_change() {
    let mut r = Rng(0xC1_09);
    for case in 0..3_000 {
        let mut v = Vigil::new(r.below(DAY as u64) as u32).unwrap();
        let mut at = GENESIS + r.below(3 * DAY as u64) as i64;
        for _ in 0..r.below(3) {
            v.request_change(at, r.below(DAY as u64) as u32).unwrap();
            at += 7 * DAY + r.below(DAY as u64) as i64;
        }
        let b0 = r.below(2_000) as u32;
        let b1 = b0 + r.below(4_000) as u32;
        let brute = (b0..b1)
            .filter(|&b| !v.covers(GENESIS + b as i64 * 600))
            .count() as u32;
        assert_eq!(
            bells_outside_vigil(&v, GENESIS, b0, b1),
            brute,
            "case {case}"
        );
    }
}

// ------------------------------------------------------------ CL-11

#[test]
fn office_ceiling_above_paid_is_refused() {
    let with = |bps: u32| PayoutParams {
        office_ceiling_bps: bps,
        ..PayoutParams::REV3
    };
    assert_eq!(with(10_000).validate(), Ok(()));
    assert_eq!(with(10_000).validate_for_season(), Ok(()));
    assert_eq!(with(10_001).validate(), Err(EconError::BadParams));
    assert_eq!(with(50_000).validate(), Err(EconError::BadParams));
    assert_eq!(with(u32::MAX).validate(), Ok(()), "REV2 comparison runs");
    assert_eq!(
        with(u32::MAX).validate_for_season(),
        Err(EconError::BadParams)
    );
    assert_eq!(PayoutParams::REV3.validate_for_season(), Ok(()));
    assert_eq!(
        PayoutParams::REV2.validate_for_season(),
        Err(EconError::BadParams)
    );
}

// ------------------------------------------------------------ CL-12

/// There is no default Works rate: weights, AddStake, transfers and the
/// settlement all follow the season's `works_per_usdc`, and mixing rates
/// is refused rather than silently mispaid.
#[test]
fn weights_follow_the_season_rate() {
    let rec = |faction, works| CitizenRecord {
        faction,
        fee: 4 * USDC,
        stake: 6 * USDC,
        tenure: 10_000,
        works,
        laurels: 50,
        ..Default::default()
    };
    // 4 USDC buys 420 Works of weight at 105 and 560 at 140.
    let a = rec(0, 500);
    assert_eq!(a.weights_at(105).works, 420);
    assert_eq!(a.weights_at(140).works, 500);
    let mut from = rec(0, 500);
    let mut to = rec(0, 0);
    let (w_from, w_to) = from.transfer_counted(&mut to, 10, 140).unwrap();
    assert_eq!((w_from.works, w_to.works), (500, 0));
    let mut late = CitizenRecord {
        stake: 0,
        ..rec(1, 700)
    };
    let (old, new) = late.add_stake(3 * USDC, 140).unwrap();
    assert_eq!((old.works, new.works), (560, 560));
    // A season at 140: totals and claims agree, and the claims conserve.
    let p = PayoutParams {
        works_per_usdc: 140,
        ..PayoutParams::REV3
    };
    let recs = [rec(0, 500), rec(0, 100), rec(3, 900)];
    let st = settlement_for(&recs, &p);
    assert_eq!(st.works_per_usdc, 140);
    let mut ledger = SeasonLedger::new(&st).unwrap();
    for r in &recs {
        ledger.record(r.faction, &claim(&st, r).unwrap()).unwrap();
    }
    assert_eq!(ledger.claimed + ledger.swept + ledger.dust(), ledger.prize);
    // Totals summed at 140 but a settlement at 105: the wallet whose
    // weight differs is refused, never overpaid.
    let mut totals = [FactionTotals::default(); FACTIONS];
    for r in &recs {
        totals[r.faction as usize].add(&r.weights_at(105)).unwrap();
    }
    let st105 = settle(
        &pools_for(&recs),
        &totals,
        &[INDEX_ONE; FACTIONS],
        0,
        &PayoutParams {
            works_per_usdc: 140,
            ..PayoutParams::REV3
        },
    )
    .unwrap();
    // Faction 3's only wallet weighs 560 at 140 against a total of 420.
    assert_eq!(claim(&st105, &recs[2]), Err(EconError::Underflow));
}

// ------------------------------------------------------------ CL-13

fn accrual_left_loop(s: &EntrySchedule, day: u32) -> u128 {
    let j = s.last_join_day.max(1) as u128;
    (day..s.season_days)
        .map(|t| j * BPS_ONE as u128 + s.stake_ramp_bps as u128 * t.min(s.last_join_day) as u128)
        .sum()
}

#[test]
fn accrual_left_closed_form_matches_the_loop() {
    let mut n = 0u32;
    for season_days in [1u32, 2, 7, 28, 29, 100, 365, 366] {
        let joins = [0, 1, season_days / 2, season_days.saturating_sub(1)];
        for last_join_day in joins {
            if last_join_day >= season_days && season_days > 0 {
                continue;
            }
            for stake_ramp_bps in [0u32, 1, 7_777, 20_000, 100_000] {
                let s = EntrySchedule {
                    season_days,
                    last_join_day,
                    stake_ramp_bps,
                    ..EntrySchedule::SEASON1
                };
                s.validate().unwrap();
                for day in 0..=366u32 {
                    assert_eq!(
                        s.accrual_left(day),
                        accrual_left_loop(&s, day),
                        "S {season_days} j {last_join_day} R {stake_ramp_bps} d {day}"
                    );
                    // And the rate the loop sums is the schedule's own.
                    if day < season_days {
                        assert_eq!(
                            s.accrual_rate(day),
                            accrual_left_loop(&s, day) - accrual_left_loop(&s, day + 1)
                        );
                    }
                    n += 1;
                }
            }
        }
    }
    assert!(n > 40_000, "{n}");
    // Extremes do not overflow.
    let big = EntrySchedule {
        season_days: u32::MAX,
        last_join_day: u32::MAX - 1,
        stake_ramp_bps: u32::MAX,
        ..EntrySchedule::SEASON1
    };
    assert!(big.accrual_left(0) > 0);
    assert_eq!(big.accrual_left(u32::MAX), 0);
}

/// `laurel_stake` for Season 1 is unchanged: the loop-based price, day by
/// day (and the day-0 / day-21 prices of the design).
#[test]
fn season1_laurel_stake_is_unchanged() {
    let s = EntrySchedule::SEASON1;
    let base = s.laurel_stake_day0 as u128;
    let floor = base * s.floor_bps as u128 / BPS_ONE as u128;
    for day in 0..=s.last_join_day {
        let want = (base * accrual_left_loop(&s, day) / accrual_left_loop(&s, 0)).max(floor) as u64;
        assert_eq!(s.laurel_stake(day), Some(want), "day {day}");
    }
    assert_eq!(s.laurel_stake(0), Some(6 * USDC));
    assert_eq!(s.laurel_stake(s.last_join_day + 1), None);
}

// ------------------------------------------------------------ CL-15

/// A 28-day season of seven 4-day terms: claims of a term close one term
/// after it ends, and never after the banking window (season end + 72 h);
/// every term is swept before the final sweep, which is refused while a
/// closed term is outstanding.
#[test]
fn mandate_claims_close_before_the_reckoning() {
    let season_end = GENESIS + 28 * DAY;
    for k in 0..7i64 {
        let term_end = GENESIS + (k + 1) * TERM_SECS;
        let d = claim_deadline(term_end, season_end);
        assert_eq!(
            d,
            (term_end + TERM_SECS).min(season_end + BANKING_WINDOW_SECS)
        );
        assert!(d <= season_end + BANKING_WINDOW_SECS);
    }
    assert_eq!(
        claim_deadline(season_end, season_end),
        season_end + BANKING_WINDOW_SECS
    );
    assert_eq!(claim_deadline(i64::MAX, i64::MAX), i64::MAX, "saturates");

    let mut reserve = Reserve::default();
    reserve.deposit(10_000).unwrap();
    // The last term ends at the season end.
    let mut t = MandateTerm::new(6);
    for _ in 0..3 {
        t.complete(true).unwrap();
    }
    t.close(&mut reserve, season_end, season_end).unwrap();
    assert_eq!(t.claim_deadline, season_end + BANKING_WINDOW_SECS);
    assert_eq!(t.claim(t.claim_deadline, &mut reserve, 1).unwrap(), 3_333);
    // A late claim is refused …
    assert_eq!(
        t.claim(t.claim_deadline + 1, &mut reserve, 1),
        Err(EconError::TermState)
    );
    // … and the reserve cannot be finalised while the term is open.
    assert_eq!(reserve.final_sweep(0), Err(EconError::TermState));
    assert_eq!(
        t.sweep(t.claim_deadline, &mut reserve),
        Err(EconError::TermState),
        "unclaimed shares: only after the deadline"
    );
    let back = t.sweep(t.claim_deadline + 1, &mut reserve).unwrap();
    assert_eq!(back, 10_000 - 3_333);
    // A term still open (its completers not yet paid) blocks the final
    // sweep even though every closed term is swept (integ-W1 review).
    let mut open = MandateTerm::new(7);
    open.complete(true).unwrap();
    assert_eq!(reserve.outstanding(), Ok(0));
    assert_eq!(reserve.final_sweep(1), Err(EconError::TermState));
    assert!(!reserve.finalized);
    // The final sweep takes the whole balance to the laurel split.
    assert_eq!(reserve.final_sweep(0).unwrap(), 10_000 - 3_333);
    assert_eq!(reserve.balance, 0);
    assert_eq!(
        reserve.deposited,
        reserve.paid + reserve.burned + reserve.final_swept
    );
    // Nothing happens after it.
    assert_eq!(reserve.final_sweep(0), Err(EconError::TermState));
    assert_eq!(reserve.deposit(1), Err(EconError::TermState));
    let mut late = MandateTerm::new(7);
    late.complete(true).unwrap();
    assert_eq!(
        late.close(&mut reserve, season_end, season_end),
        Err(EconError::TermState)
    );
}

/// `deposited == paid + burned + final_sweep` over 200 random 7-term
/// histories (floors, caps, unclaimed shares swept at the deadline).
#[test]
fn final_sweep_conserves() {
    let mut r = Rng(0xC1_15);
    let mut swept_nonzero = 0;
    for seed in 0..200 {
        let season_end = GENESIS + 7 * TERM_SECS;
        let mut reserve = Reserve::default();
        for term in 0..7u32 {
            for _ in 0..r.below(30) {
                let big = r.chance(10);
                let credit = r.below(if big {
                    40 * MANDATE_CAP_PER_TERM
                } else {
                    MANDATE_CAP_PER_TERM
                });
                reserve.deposit(credit / 10).unwrap();
            }
            let term_end = GENESIS + (term as i64 + 1) * TERM_SECS;
            let mut t = MandateTerm::new(term);
            let mut who = Vec::new();
            for _ in 0..r.below(12) {
                let sh = t.complete(r.chance(70)).unwrap();
                if sh > 0 {
                    who.push(sh);
                }
            }
            let floor = if r.chance(50) {
                share_floor(r.below(30))
            } else {
                0
            };
            t.close_with_floor(&mut reserve, floor, term_end, season_end)
                .unwrap();
            for sh in who {
                if r.chance(80) {
                    let now = term_end + r.below(TERM_SECS as u64) as i64;
                    t.claim(now.min(t.claim_deadline), &mut reserve, sh)
                        .unwrap();
                }
            }
            t.sweep(t.claim_deadline + 1, &mut reserve).unwrap();
        }
        let out = reserve.final_sweep(0).unwrap();
        swept_nonzero += (out > 0) as u32;
        assert_eq!(reserve.balance, 0, "seed {seed}");
        assert_eq!(
            reserve.deposited,
            reserve.paid + reserve.burned + reserve.final_swept,
            "seed {seed}"
        );
        assert_eq!(reserve.outstanding().unwrap(), 0);
    }
    assert!(swept_nonzero > 50, "{swept_nonzero}");
}

// ------------------------------------------------------------ CL-18

/// γ > 1 is accepted (the simulator's γ grid uses it) and computed as the
/// largest `y` with `y^den ≤ x^num`; `IndexParams::validate` bounds it at 3.
#[test]
fn pow_frac_accepts_gamma_above_one() {
    let half = INDEX_ONE / 2;
    // 0.5^1.5 = 0.353553…
    let y = pow_frac(half, 3, 2);
    assert!((353_552..=353_554).contains(&y), "{y}");
    // Largest y with y² ≤ x³.
    let cube = (half as u128).pow(3) / INDEX_ONE as u128;
    assert!((y as u128).pow(2) / INDEX_ONE as u128 <= cube);
    // γ = 3 (the bound) is exact on a power of a half; γ > 1 shrinks x.
    assert_eq!(pow_frac(half, 3, 1), INDEX_ONE / 8);
    let mut prev = 0;
    for x in (0..=INDEX_ONE).step_by(50_000) {
        let g = pow_frac(x, 12, 5);
        assert!(g <= x && g >= prev, "x {x}");
        prev = g;
    }
    assert_eq!(pow_frac(INDEX_ONE, 12, 5), INDEX_ONE);
    let p = |num, den| IndexParams {
        gamma_num: num,
        gamma_den: den,
        ..IndexParams::REV2
    };
    assert_eq!(p(3, 2).validate(), Ok(()));
    assert_eq!(p(60, 20).validate(), Ok(()));
    assert_eq!(p(61, 20).validate(), Err(EconError::BadParams));
}
