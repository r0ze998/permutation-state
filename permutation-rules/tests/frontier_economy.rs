//! Frontier economy kernels (open-world design §5.4, §5.6; D3, D11): entry
//! schedule and pools, the zero-sum laurel emission and its reward index,
//! capture transfers, the per-capita faction index with herding damping,
//! and the closed-form claim with the 5× cap and exact conservation.
//!
//! The property tests draw random seasons from a fixed-seed generator, so
//! every run checks the same cases and a failure names its seed.

use permutation_rules::frontier::index::{
    faction_index, herding, FactionFacts, IndexParams, Path, FACTIONS, INDEX_ONE, PATHS,
};
use permutation_rules::frontier::laurel::{
    capture_transfer, mandate_reserve_split, occupation_split, siege_settle, strength_weight,
    transfer, PairHistory, RewardIndex, Stake, Tier, HOLDING_EMISSION_PER_BELL, LAUREL_ONE,
    SIEGE_STAKE, WEIGHT_ONE,
};
use permutation_rules::frontier::payout::{
    claim, settle, tenure_units, CitizenRecord, FactionTotals, Ledger, PayoutParams, Settlement,
    TENURE_ONE,
};
use permutation_rules::frontier::pools::{
    civ_share_bps, steward_rows, EconError, Entry, EntrySchedule, Pools, USDC,
};

/// SplitMix64: a small deterministic generator for property tests.
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
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

const SCHED: EntrySchedule = EntrySchedule::FRONTIER_28;

// ---------------------------------------------------------------- pools

#[test]
fn entry_is_escrowed_then_split_80_20() {
    let mut p = Pools::default();
    let fee = SCHED.citizen_fee(0).unwrap();
    let stake = SCHED.laurel_stake(0).unwrap();
    p.pay_pending(fee).unwrap();
    p.pay_pending(stake).unwrap();
    assert_eq!(
        p.prize().unwrap(),
        0,
        "nothing reaches the pools before a holding settles"
    );
    p.release(&SCHED, Entry::CitizenFee, fee).unwrap();
    p.release(&SCHED, Entry::LaurelStake, stake).unwrap();
    assert_eq!(p.citizen, 3_200_000);
    assert_eq!(p.laurel, 4_800_000);
    assert_eq!(p.operator_citizen + p.operator_laurel, 2_000_000);
    assert_eq!(p.total().unwrap(), 10 * USDC);

    // A second wallet never gets a site and withdraws 100%.
    p.pay_pending(fee).unwrap();
    p.withdraw_pending(fee).unwrap();
    assert_eq!(p.total().unwrap(), 10 * USDC);
    assert_eq!(p.withdraw_pending(1), Err(EconError::Underflow));

    // An unrevealed Shade leaf: the operator's escrow goes to the pools.
    assert_eq!(p.forfeit_operator().unwrap(), 2_000_000);
    assert_eq!(p.prize().unwrap(), 10 * USDC);
}

#[test]
fn late_joins_pay_less_and_last_call_closes() {
    assert_eq!(SCHED.citizen_fee(21), Some(USDC));
    assert_eq!(SCHED.laurel_stake(21), Some(3 * USDC / 2));
    assert_eq!(SCHED.citizen_fee(22), None);
    let mut last = u64::MAX;
    for d in 0..=21 {
        let c = SCHED.citizen_fee(d).unwrap();
        assert!(c <= last && c >= USDC);
        last = c;
    }
    assert_eq!(civ_share_bps(0), 500);
    assert_eq!(civ_share_bps(5), 1_500);
    assert_eq!(civ_share_bps(9), 1_500);
    assert_eq!(steward_rows(1, 2), 4 * USDC);
    assert_eq!(steward_rows(9, 9), 10 * USDC);
}

#[test]
fn pool_money_is_conserved_under_random_flows() {
    for seed in 0..50u64 {
        let mut r = Rng(seed);
        let mut p = Pools::default();
        let mut paid_in = 0u64;
        let mut refunded = 0u64;
        let mut open: Vec<(Entry, u64)> = Vec::new();
        for _ in 0..400 {
            let day = r.below(23) as u32;
            match r.below(4) {
                0 | 1 => {
                    let kind = if r.chance(50) {
                        Entry::CitizenFee
                    } else {
                        Entry::LaurelStake
                    };
                    let amt = match kind {
                        Entry::CitizenFee => SCHED.citizen_fee(day),
                        Entry::LaurelStake => SCHED.laurel_stake(day),
                    };
                    let Some(amt) = amt else { continue };
                    p.pay_pending(amt).unwrap();
                    paid_in += amt;
                    open.push((kind, amt));
                }
                2 if !open.is_empty() => {
                    let (kind, amt) = open.swap_remove(r.below(open.len() as u64) as usize);
                    let s = p.release(&SCHED, kind, amt).unwrap();
                    assert_eq!(s.pool + s.operator, amt);
                }
                3 if !open.is_empty() => {
                    let (_, amt) = open.swap_remove(r.below(open.len() as u64) as usize);
                    p.withdraw_pending(amt).unwrap();
                    refunded += amt;
                }
                _ => {}
            }
            assert_eq!(p.total().unwrap(), paid_in - refunded, "seed {seed}");
        }
    }
}

// ---------------------------------------------------------------- laurels

#[test]
fn province_emission_is_split_by_strength_not_created_by_it() {
    // Two holdings: a Hamlet and a Stronghold with a full garrison.
    let weak = strength_weight(Tier::Hamlet, 0, 0);
    let strong = strength_weight(Tier::Stronghold, 1_000_000, 0);
    assert_eq!((weak, strong), (WEIGHT_ONE, 3 * WEIGHT_ONE));
    let mut ix = RewardIndex::new(0);
    let mut a = ix.attach(weak, true).unwrap();
    let mut b = ix.attach(strong, true).unwrap();
    ix.accrue(12, u64::MAX).unwrap();
    let emitted = 12 * 2 * HOLDING_EMISSION_PER_BELL;
    assert_eq!(ix.emitted, emitted as u128);
    assert_eq!(ix.settle(&mut a).unwrap(), emitted / 4);
    assert_eq!(ix.settle(&mut b).unwrap(), emitted * 3 / 4);
    // 12 bells × 2 holdings × 1/12 laurel = 2 laurels in total, whatever the weights.
    assert_eq!(emitted, 2 * LAUREL_ONE);

    // Arming the Hamlet takes a larger share of the same emission.
    ix.reweigh(&mut a, strength_weight(Tier::Hamlet, 2_000_000, 0), true)
        .unwrap();
    ix.accrue(24, u64::MAX).unwrap();
    let got = ix.settle(&mut a).unwrap() + ix.settle(&mut b).unwrap();
    assert!(got <= emitted && emitted - got <= 2);
}

#[test]
fn a_wallet_farm_in_one_province_splits_that_province() {
    // 1 human Stronghold + 11 bot Hamlets (third holdings): the bots add
    // emitters, but each bot's share is small and the total is fixed.
    let mut ix = RewardIndex::new(0);
    let mut human = ix
        .attach(strength_weight(Tier::Stronghold, 0, 0), true)
        .unwrap();
    let mut bots: Vec<Stake> = (0..11)
        .map(|_| {
            ix.attach(strength_weight(Tier::Hamlet, 0, 2), true)
                .unwrap()
        })
        .collect();
    ix.accrue(144, u64::MAX).unwrap();
    let total = 144 * 12 * HOLDING_EMISSION_PER_BELL;
    let h = ix.settle(&mut human).unwrap();
    let b: u64 = bots.iter_mut().map(|s| ix.settle(s).unwrap()).sum();
    assert!(h + b <= total && total - (h + b) <= 12);
    // Weights 2.0 vs 11 × 0.25: the human earns 2/4.75 of the province.
    assert!(h.abs_diff(total * 200 / 475) <= 1);
}

/// Laurels are never created by the index: across random provinces with
/// holdings joining, leaving, re-weighing and being touched at random,
/// Σ credits + orphaned ≤ emitted, the gap is at most one unit per credit
/// event, and each holding's credit matches an exact per-bell reference.
#[test]
fn reward_index_is_zero_sum_under_random_histories() {
    for seed in 0..60u64 {
        let mut r = Rng(1_000 + seed);
        let end_bell = 200 + r.below(400);
        let mut ix = RewardIndex::new(r.below(10));
        // (stake, alive, credited, reference credit, credit events)
        let mut hs: Vec<(Stake, bool, u64, f64, u64)> = Vec::new();
        let mut credited_total = 0u64;
        let mut events = 0u64;
        let mut bell = ix.bell;
        for _ in 0..300 {
            let next = bell + r.below(6);
            // Reference: this span's emission split exactly by current weights.
            let span = next.min(end_bell).saturating_sub(bell.min(end_bell));
            let e = span as f64 * ix.emission_per_bell() as f64;
            if ix.total_weight > 0 {
                for h in hs.iter_mut().filter(|h| h.1) {
                    h.3 += e * h.0.weight as f64 / ix.total_weight as f64;
                }
            }
            ix.accrue(next, end_bell).unwrap();
            bell = next;
            let weight = strength_weight(
                [Tier::Hamlet, Tier::Town, Tier::City, Tier::Stronghold][r.below(4) as usize],
                r.below(3_000_000) as u32,
                r.below(3) as u8,
            );
            let emits = r.chance(85);
            match r.below(5) {
                0 if hs.iter().filter(|h| h.1).count() < 12 => {
                    let s = ix.attach(weight, emits).unwrap();
                    hs.push((s, true, 0, 0.0, 0));
                }
                1 => {
                    if let Some(h) = pick_alive(&mut r, &mut hs) {
                        let c = ix.detach(&h.0).unwrap();
                        h.1 = false;
                        h.2 += c;
                        h.4 += 1;
                        credited_total += c;
                        events += 1;
                    }
                }
                2 => {
                    if let Some(h) = pick_alive(&mut r, &mut hs) {
                        let c = ix.reweigh(&mut h.0, weight, emits).unwrap();
                        h.2 += c;
                        h.4 += 1;
                        credited_total += c;
                        events += 1;
                    }
                }
                _ => {
                    if let Some(h) = pick_alive(&mut r, &mut hs) {
                        let c = ix.settle(&mut h.0).unwrap();
                        h.2 += c;
                        h.4 += 1;
                        credited_total += c;
                        events += 1;
                    }
                }
            }
        }
        // Bank everything left (the banking window after T_end): the last
        // span's reference is taken before the index moves.
        let alive_weight = ix.total_weight;
        let last_span = end_bell.saturating_sub(bell) as f64 * ix.emission_per_bell() as f64;
        ix.accrue(bell + 1_000, end_bell).unwrap();
        for h in hs.iter_mut().filter(|h| h.1) {
            if alive_weight > 0 {
                h.3 += last_span * h.0.weight as f64 / alive_weight as f64;
            }
            let c = ix.settle(&mut h.0).unwrap();
            h.2 += c;
            h.4 += 1;
            credited_total += c;
            events += 1;
        }
        let emitted = ix.emitted as u64;
        let orphaned = ix.orphaned as u64;
        assert!(
            credited_total + orphaned <= emitted,
            "seed {seed}: laurels created"
        );
        assert!(
            emitted - orphaned - credited_total <= events + 1,
            "seed {seed}: {} units lost over {events} events",
            emitted - orphaned - credited_total
        );
        for (i, h) in hs.iter().enumerate() {
            let diff = (h.2 as f64 - h.3).abs();
            assert!(
                diff <= h.4 as f64 + 2.0,
                "seed {seed} holding {i}: got {} want {:.1}",
                h.2,
                h.3
            );
        }
    }
}

fn pick_alive<'a>(
    r: &mut Rng,
    hs: &'a mut [(Stake, bool, u64, f64, u64)],
) -> Option<&'a mut (Stake, bool, u64, f64, u64)> {
    let alive: Vec<usize> = (0..hs.len()).filter(|i| hs[*i].1).collect();
    if alive.is_empty() {
        return None;
    }
    let i = alive[r.below(alive.len() as u64) as usize];
    Some(&mut hs[i])
}

#[test]
fn captures_and_sieges_only_move_laurels() {
    let (mut victim, mut captor) = (40 * LAUREL_ONE, 3 * LAUREL_ONE);
    let before = victim + captor;
    let t = capture_transfer(victim, PairHistory::default());
    assert_eq!(t.laurels, 10 * LAUREL_ONE);
    assert!(t.refugee_kit);
    transfer(&mut victim, &mut captor, t.laurels).unwrap();
    assert_eq!(victim + captor, before);

    // Trading captures between alts: the second pays 25% of 25%, the
    // third nothing, and any other tie nothing at all.
    let again = capture_transfer(
        victim,
        PairHistory {
            prior_captures: 1,
            other_ties: false,
        },
    );
    assert_eq!(again.laurels, victim / 16);
    assert!(!again.refugee_kit);
    assert_eq!(
        capture_transfer(
            victim,
            PairHistory {
                prior_captures: 2,
                other_ties: false
            }
        )
        .laurels,
        0
    );
    assert_eq!(
        capture_transfer(
            victim,
            PairHistory {
                prior_captures: 0,
                other_ties: true
            }
        )
        .laurels,
        0
    );
    let too_much = victim + 1;
    assert_eq!(
        transfer(&mut victim, &mut captor, too_much),
        Err(EconError::Underflow)
    );

    let ok = siege_settle(true);
    let failed = siege_settle(false);
    assert_eq!((ok.to_attacker, ok.to_defender), (SIEGE_STAKE, 0));
    assert_eq!((failed.to_attacker, failed.to_defender), (0, SIEGE_STAKE));

    for c in [0, 1, 7, 999_999, 12_345_678] {
        let o = occupation_split(c);
        let m = mandate_reserve_split(c);
        assert_eq!(o.keep + o.give, c);
        assert_eq!(m.keep + m.give, c);
        assert_eq!(m.give, c / 10);
    }
}

// ---------------------------------------------------------------- index

fn facts(members: u64, active: u64, per_active: [u64; PATHS]) -> FactionFacts {
    FactionFacts {
        members,
        active,
        path: per_active.map(|x| x * active),
    }
}

#[test]
fn equal_factions_all_score_one() {
    let f = [facts(1_000, 600, [5, 7, 3, 2]); FACTIONS];
    assert_eq!(faction_index(&f, &IndexParams::REV2), [INDEX_ONE; FACTIONS]);
}

#[test]
fn herding_damping_matches_the_design_table() {
    // §5.6 table, β = 1.0 (equal per-capita performance): the per-capita
    // payout ratio of a faction m× the size of each other one is h_k.
    let p = IndexParams::REV2;
    for (m10, want) in [(12u64, 0.91), (15, 0.82), (20, 0.72), (30, 0.61)] {
        let mut f = [facts(1_000, 500, [4, 4, 4, 4]); FACTIONS];
        f[0] = facts(100 * m10, 50 * m10, [4, 4, 4, 4]);
        let s = faction_index(&f, &p);
        let got = s[0] as f64 / s[1] as f64;
        assert!(
            (got - want).abs() < 0.006,
            "m={} got {got:.3} want {want}",
            m10 as f64 / 10.0
        );
        assert_eq!(s[1], INDEX_ONE, "small factions are not damped");
    }
    assert_eq!(herding(1, 6, &p), INDEX_ONE);
    assert_eq!(herding(0, 6, &p), INDEX_ONE);
}

#[test]
fn per_capita_index_never_rewards_joining_a_bigger_faction() {
    // Random sizes, equal per-capita performance: the per-member payout
    // weight (√s for the citizen pool, s for the laurel pool) is never
    // larger in a bigger faction.
    let p = IndexParams::REV2;
    for seed in 0..200u64 {
        let mut r = Rng(5_000 + seed);
        let per = [
            1 + r.below(50),
            1 + r.below(50),
            1 + r.below(50),
            1 + r.below(50),
        ];
        let f: [FactionFacts; FACTIONS] = core::array::from_fn(|_| {
            let m = 1 + r.below(20_000);
            facts(m, m / 2 + 1, per)
        });
        let s = faction_index(&f, &p);
        for a in 0..FACTIONS {
            for b in 0..FACTIONS {
                if f[a].members > f[b].members {
                    assert!(
                        s[a] <= s[b],
                        "seed {seed}: bigger faction {a} scores {} > {}",
                        s[a],
                        s[b]
                    );
                }
            }
            assert!(
                s[a] <= 2 * INDEX_ONE && s[a] >= INDEX_ONE / 2 * 34 / 100,
                "seed {seed}"
            );
        }
    }
}

#[test]
fn index_is_clamped_and_shades_are_voided() {
    let p = IndexParams::REV2;
    let mut f = [facts(1_000, 500, [1, 1, 1, 1]); FACTIONS];
    f[2] = facts(1_000, 500, [100, 100, 100, 100]); // runaway
    f[3] = facts(1_000, 500, [0, 0, 0, 0]); // did nothing
    f[4] = facts(1_000, 0, [0, 0, 0, 0]); // nobody active
    let s = faction_index(&f, &p);
    assert_eq!(s[2], 2 * INDEX_ONE);
    assert_eq!(s[3], INDEX_ONE / 2);
    assert_eq!(s[4], INDEX_ONE / 2);

    // A Shade's recorded facts are subtracted (§7.3).
    let shade = FactionFacts {
        members: 1,
        active: 1,
        path: [0, 0, 0, 5_000],
    };
    let mut g = [facts(1_000, 500, [4, 4, 4, 4]); FACTIONS];
    g[0].add(&shade).unwrap();
    let with = faction_index(&g, &p);
    g[0].remove(&shade).unwrap();
    let without = faction_index(&g, &p);
    assert!(with[0] > without[0]);
    assert_eq!(without, [INDEX_ONE; FACTIONS]);
    assert_eq!(
        g[0].remove(&FactionFacts {
            members: 5_000,
            ..Default::default()
        }),
        Err(EconError::Underflow)
    );
    let _ = Path::Concord;
}

// ---------------------------------------------------------------- payout

fn rec(faction: u8, fee: u64, stake: u64, tenure: u32) -> CitizenRecord {
    CitizenRecord {
        faction,
        fee,
        stake,
        tenure,
        ..Default::default()
    }
}

fn totals_of(recs: &[CitizenRecord]) -> [FactionTotals; FACTIONS] {
    let mut t = [FactionTotals::default(); FACTIONS];
    for r in recs.iter().filter(|r| !r.voided) {
        t[r.faction as usize].add(&r.weights()).unwrap();
    }
    t
}

fn pools_of(recs: &[CitizenRecord]) -> Pools {
    let mut p = Pools::default();
    for r in recs {
        p.pay_settled(&SCHED, Entry::CitizenFee, r.fee).unwrap();
        if r.stake > 0 {
            p.pay_settled(&SCHED, Entry::LaurelStake, r.stake).unwrap();
        }
    }
    p
}

#[test]
fn a_day_21_citizen_gets_the_same_return_per_usdc() {
    // Same faction, same (full) tenure: 4 USDC on day 0, 1 USDC on day 21.
    let early = rec(0, SCHED.citizen_fee(0).unwrap(), 0, tenure_units(28, 28));
    let late = rec(0, SCHED.citizen_fee(21).unwrap(), 0, tenure_units(4, 7));
    let recs = [early, late];
    let st = settle(
        &pools_of(&recs),
        &totals_of(&recs),
        &[INDEX_ONE; FACTIONS],
        0,
        &PayoutParams::REV2,
    )
    .unwrap();
    let a = claim(&st, &early).unwrap();
    let b = claim(&st, &late).unwrap();
    assert_eq!(a.citizen, 4 * b.citizen);
    // Only citizens: 80% of fees less 5% steward pot, all returned (no rows).
    assert_eq!(a.paid + b.paid, 4_000_000);
}

#[test]
fn laurel_pool_goes_to_stakers_by_laurels_and_the_cap_sweeps() {
    let fee = 4 * USDC;
    let stake = 6 * USDC;
    let mut recs = vec![
        rec(0, fee, stake, TENURE_ONE),
        rec(0, fee, stake, TENURE_ONE),
        rec(0, fee, 0, TENURE_ONE), // not a staker: its laurels pay nothing
    ];
    recs[0].laurels = 30 * LAUREL_ONE;
    recs[1].laurels = 10 * LAUREL_ONE;
    recs[2].laurels = 1_000 * LAUREL_ONE;
    let st = settle(
        &pools_of(&recs),
        &totals_of(&recs),
        &[INDEX_ONE; FACTIONS],
        0,
        &PayoutParams::REV2,
    )
    .unwrap();
    let c: Vec<_> = recs.iter().map(|r| claim(&st, r).unwrap()).collect();
    // L = 9.6 USDC, 5% steward pot → 9.12 USDC split 3:1.
    assert_eq!(c[0].laurel, 6_840_000);
    assert_eq!(c[1].laurel, 2_280_000);
    assert_eq!(c[2].laurel, 0);

    // One whale among many small stakers hits the 5× cap; the excess is swept.
    let mut many: Vec<CitizenRecord> = (0..40)
        .map(|_| rec(1, USDC, 3 * USDC / 2, TENURE_ONE))
        .collect();
    many[0].laurels = 1_000 * LAUREL_ONE;
    for r in many.iter_mut().skip(1) {
        r.laurels = 1;
    }
    let st = settle(
        &pools_of(&many),
        &totals_of(&many),
        &[INDEX_ONE; FACTIONS],
        0,
        &PayoutParams::REV2,
    )
    .unwrap();
    let w = claim(&st, &many[0]).unwrap();
    assert_eq!(w.paid, 5 * (USDC + 3 * USDC / 2));
    assert_eq!(w.swept, w.gross - w.paid);
    assert!(w.swept > 0);
}

#[test]
fn steward_rows_are_paid_in_full_or_pro_rata() {
    let mut recs: Vec<CitizenRecord> = (0..10).map(|_| rec(0, 4 * USDC, 0, TENURE_ONE)).collect();
    // Pot = 5% of 32 USDC = 1.6 USDC. One Warden-term row (0.5) fits.
    recs[0].steward = steward_rows(0, 1);
    let st = settle(
        &pools_of(&recs),
        &totals_of(&recs),
        &[INDEX_ONE; FACTIONS],
        0,
        &PayoutParams::REV2,
    )
    .unwrap();
    assert_eq!(st.factions[0].steward_pot, 1_600_000);
    assert_eq!(claim(&st, &recs[0]).unwrap().steward, 500_000);
    // The unused 1.1 USDC went back to the citizen pool.
    assert_eq!(
        st.factions[0].fee_units_pot,
        32_000_000 - 1_600_000 + 1_100_000
    );

    // Two Minister-terms (6 USDC) do not fit: paid pro rata.
    recs[1].steward = steward_rows(1, 0);
    recs[2].steward = steward_rows(1, 0);
    let st = settle(
        &pools_of(&recs),
        &totals_of(&recs),
        &[INDEX_ONE; FACTIONS],
        0,
        &PayoutParams::REV2,
    )
    .unwrap();
    let paid: Vec<u64> = recs
        .iter()
        .map(|r| claim(&st, r).unwrap().steward)
        .collect();
    assert_eq!(paid[0], 500_000 * 1_600_000 / 6_500_000);
    assert_eq!(paid[1], 3_000_000 * 1_600_000 / 6_500_000);
    assert!(paid.iter().sum::<u64>() <= 1_600_000);
}

#[test]
fn a_claim_needs_its_record_in_the_totals() {
    let recs = [rec(0, 4 * USDC, 0, TENURE_ONE)];
    let st = settle(
        &pools_of(&recs),
        &totals_of(&recs),
        &[INDEX_ONE; FACTIONS],
        0,
        &PayoutParams::REV2,
    )
    .unwrap();
    let outsider = rec(0, 8 * USDC, 0, TENURE_ONE);
    assert_eq!(claim(&st, &outsider), Err(EconError::Underflow));
    assert_eq!(
        claim(&st, &rec(9, 1, 0, TENURE_ONE)),
        Err(EconError::BadFaction)
    );
    let mut shade = recs[0];
    shade.voided = true;
    assert_eq!(claim(&st, &shade).unwrap().paid, 0);
}

/// A random season: wallets with random faction, join day, stake, activity,
/// Works, builder flag, laurels, officer rows; a few Shades voided.
struct Season {
    recs: Vec<CitizenRecord>,
    facts: [FactionFacts; FACTIONS],
    pools: Pools,
    paid_in: u64,
    stages: u8,
}

fn random_season(seed: u64) -> Season {
    let mut r = Rng(seed);
    let n = 1 + r.below(400) as usize;
    let skew = r.below(FACTIONS as u64) as u8; // a herding faction
    let mut recs = Vec::with_capacity(n);
    let mut facts = [FactionFacts::default(); FACTIONS];
    let mut pools = Pools::default();
    let mut paid_in = 0u64;
    for _ in 0..n {
        let faction = if r.chance(30) {
            skew
        } else {
            r.below(FACTIONS as u64) as u8
        };
        let day = r.below(22) as u32;
        let fee = SCHED.citizen_fee(day).unwrap();
        let stake = if r.chance(45) {
            SCHED.laurel_stake(day).unwrap()
        } else {
            0
        };
        let avail = SCHED.days_available(day);
        let active = r.below(avail as u64 + 1) as u32;
        let heavy = r.chance(3);
        let c = CitizenRecord {
            faction,
            fee,
            stake,
            tenure: tenure_units(active, avail),
            works: if r.chance(60) { r.below(50_000) } else { 0 },
            builder: r.chance(20),
            laurels: if heavy {
                r.below(5_000) * LAUREL_ONE
            } else {
                r.below(40 * LAUREL_ONE)
            },
            steward: if r.chance(4) {
                steward_rows(r.below(3) as u32, r.below(8) as u32)
            } else {
                0
            },
            voided: r.chance(2),
        };
        pools.pay_pending(fee).unwrap();
        pools.release(&SCHED, Entry::CitizenFee, fee).unwrap();
        if stake > 0 {
            pools.pay_pending(stake).unwrap();
            pools.release(&SCHED, Entry::LaurelStake, stake).unwrap();
        }
        paid_in += fee + stake;
        let f = FactionFacts {
            members: 1,
            active: (active > 0) as u64,
            path: core::array::from_fn(|_| if active > 0 { r.below(100) } else { 0 }),
        };
        if !c.voided {
            facts[faction as usize].add(&f).unwrap();
        }
        recs.push(c);
    }
    Season {
        recs,
        facts,
        pools,
        paid_in,
        stages: r.below(7) as u8,
    }
}

/// The formula of §5.4 written out as a plain loop over members, with the
/// same rounding: the reference the closed form must match exactly.
fn reference_claims(s: &Season, index: &[u64; FACTIONS]) -> Vec<u64> {
    let p = PayoutParams::REV2;
    let live: Vec<&CitizenRecord> = s.recs.iter().filter(|r| !r.voided).collect();
    let md = |a: u128, b: u128, c: u128| if c == 0 { 0 } else { a * b / c };
    let builders: u128 = live
        .iter()
        .filter(|r| r.builder)
        .map(|r| r.fee as u128)
        .sum();
    let civ = if builders > 0 {
        s.pools.citizen as u128 * civ_share_bps(s.stages) as u128 / 10_000
    } else {
        0
    };
    let rest = s.pools.citizen as u128 - civ;
    let sq = |x: u64| permutation_rules::fixed::isqrt(x.saturating_mul(INDEX_ONE)) as u128;
    let fee_k = |k: u8| {
        live.iter()
            .filter(|r| r.faction == k)
            .map(|r| r.fee as u128)
            .sum::<u128>()
    };
    let stake_k = |k: u8| {
        live.iter()
            .filter(|r| r.faction == k)
            .map(|r| r.stake as u128)
            .sum::<u128>()
    };
    let cw: Vec<u128> = (0..6u8).map(|k| fee_k(k) * sq(index[k as usize])).collect();
    let lw: Vec<u128> = (0..6u8)
        .map(|k| stake_k(k) * index[k as usize] as u128)
        .collect();
    let (cws, lws): (u128, u128) = (cw.iter().sum(), lw.iter().sum());
    s.recs
        .iter()
        .map(|r| {
            if r.voided {
                return 0;
            }
            let k = r.faction;
            let mates: Vec<&&CitizenRecord> = live.iter().filter(|m| m.faction == k).collect();
            let c_k = md(rest, cw[k as usize], cws);
            let l_k = md(s.pools.laurel as u128, lw[k as usize], lws);
            let (pc, pl) = (c_k * 500 / 10_000, l_k * 500 / 10_000);
            let rows: u128 = mates.iter().map(|m| m.steward as u128).sum();
            let cit = c_k - pc + (pc + pl).saturating_sub(rows);
            let fu_sum: u128 = mates.iter().map(|m| m.fee as u128 * m.tenure as u128).sum();
            let w_sum: u128 = mates
                .iter()
                .map(|m| permutation_rules::frontier::payout::works_weight(m.works) as u128)
                .sum();
            let (fu_pot, w_pot) = if w_sum > 0 {
                (cit * 9_000 / 10_000, cit - cit * 9_000 / 10_000)
            } else {
                (cit, 0)
            };
            let mut g = md(fu_pot, r.fee as u128 * r.tenure as u128, fu_sum)
                + md(
                    w_pot,
                    permutation_rules::frontier::payout::works_weight(r.works) as u128,
                    w_sum,
                )
                + if r.builder {
                    md(civ, r.fee as u128, builders)
                } else {
                    0
                };
            if r.stake > 0 {
                let lam: u128 = mates
                    .iter()
                    .filter(|m| m.stake > 0)
                    .map(|m| m.laurels as u128)
                    .sum();
                let st: u128 = mates.iter().map(|m| m.stake as u128).sum();
                let lpot = l_k - pl;
                g += if lam == 0 {
                    md(lpot, r.stake as u128, st)
                } else {
                    md(lpot, r.laurels as u128, lam)
                };
            }
            g += if rows <= pc + pl {
                r.steward as u128
            } else {
                md(pc + pl, r.steward as u128, rows)
            };
            (g as u64).min(p.cap_multiple * (r.fee + r.stake))
        })
        .collect()
}

/// §5.4/D11 conservation: for random seasons, Σ claims + swept + dust ==
/// the prize pools exactly, every claim respects the 5× cap, dust is only
/// rounding, and pools + operator escrow == everything paid in.
#[test]
fn claims_conserve_money_under_random_seasons() {
    let p = PayoutParams::REV2;
    let (mut capped, mut voided, mut max_dust) = (0u32, 0u32, 0u64);
    for seed in 0..300u64 {
        let s = random_season(77_000 + seed);
        let index = faction_index(&s.facts, &IndexParams::REV2);
        let totals = totals_of(&s.recs);
        let st = settle(&s.pools, &totals, &index, s.stages, &p).unwrap();
        let prize = s.pools.prize().unwrap();
        assert!(st.allocated <= prize, "seed {seed}");
        let mut ledger = Ledger::new(prize);
        let reference = reference_claims(&s, &index);
        for (i, r) in s.recs.iter().enumerate() {
            let c = claim(&st, r).unwrap();
            assert!(c.paid <= 5 * r.paid(), "seed {seed}: cap broken");
            assert_eq!(c.paid + c.swept, c.gross);
            assert_eq!(
                c.paid, reference[i],
                "seed {seed} wallet {i}: closed form != member loop"
            );
            capped += (c.swept > 0) as u32;
            voided += r.voided as u32;
            ledger.record(&c).unwrap();
        }
        assert_eq!(
            ledger.claimed + ledger.swept + ledger.dust(),
            prize,
            "seed {seed}"
        );
        // Dust is rounding only: ≤ 6 floors per wallet plus 16 per faction,
        // unless every staker was a voided Shade (their pool then has no taker).
        let live_staker = s.recs.iter().any(|r| !r.voided && r.stake > 0);
        let any_live = s.recs.iter().any(|r| !r.voided);
        if live_staker && any_live {
            let bound = 6 * s.recs.len() as u64 + 16 * FACTIONS as u64;
            assert!(
                ledger.dust() <= bound,
                "seed {seed}: dust {} > {bound}",
                ledger.dust()
            );
            max_dust = max_dust.max(ledger.dust());
        }
        assert_eq!(
            s.pools.prize().unwrap() + s.pools.operator_citizen + s.pools.operator_laurel,
            s.paid_in,
            "seed {seed}"
        );
    }
    // The random seasons exercise the cap and Shade voiding.
    assert!(
        capped > 20 && voided > 20,
        "capped {capped}, voided {voided}"
    );
    assert!(max_dust < 3_000, "max dust {max_dust}");
}

/// No per-member loop is needed: the faction totals kept incrementally in
/// 16 shards, in any order, with records updated many times, equal the
/// totals of the final records, so every claim (an O(1) function of one
/// record and the settlement) is independent of the order of play (§9.4).
#[test]
fn incremental_sharded_totals_are_order_independent() {
    for seed in 0..40u64 {
        let s = random_season(91_000 + seed);
        let batch = totals_of(&s.recs);
        let index = faction_index(&s.facts, &IndexParams::REV2);
        let want = settle(&s.pools, &batch, &index, s.stages, &PayoutParams::REV2).unwrap();
        for order in 0..7u64 {
            let mut r = Rng(seed * 100 + order);
            let mut shards = [[FactionTotals::default(); 16]; FACTIONS];
            let mut current: Vec<CitizenRecord> = s
                .recs
                .iter()
                .map(|c| CitizenRecord {
                    laurels: 0,
                    works: 0,
                    ..*c
                })
                .collect();
            let mut perm: Vec<usize> = (0..s.recs.len()).collect();
            for i in (1..perm.len()).rev() {
                perm.swap(i, r.below(i as u64 + 1) as usize);
            }
            // Join in a random order with no laurels or Works yet.
            for &i in &perm {
                let c = current[i];
                shards[c.faction as usize][i % 16]
                    .add(&c.weights())
                    .unwrap();
            }
            // Credits land in random order and pieces: add(new) then remove(old).
            for _ in 0..3 * s.recs.len() {
                let i = r.below(s.recs.len() as u64) as usize;
                let old = current[i];
                let mut new = old;
                new.laurels =
                    (old.laurels + r.below(s.recs[i].laurels / 2 + 1)).min(s.recs[i].laurels);
                new.works = (old.works + r.below(s.recs[i].works / 2 + 1)).min(s.recs[i].works);
                let sh = &mut shards[old.faction as usize][i % 16];
                sh.add(&new.weights()).unwrap();
                sh.remove(&old.weights()).unwrap();
                current[i] = new;
            }
            // Banking window: bring every record to its final value; then
            // RevealShade removes each Shade.
            for i in 0..s.recs.len() {
                let sh = &mut shards[current[i].faction as usize][i % 16];
                sh.add(
                    &CitizenRecord {
                        voided: false,
                        ..s.recs[i]
                    }
                    .weights(),
                )
                .unwrap();
                sh.remove(&current[i].weights()).unwrap();
                if s.recs[i].voided {
                    sh.remove(&s.recs[i].weights()).unwrap();
                }
            }
            let mut folded = [FactionTotals::default(); FACTIONS];
            for k in 0..FACTIONS {
                for sh in &shards[k] {
                    folded[k].merge(sh).unwrap();
                }
            }
            assert_eq!(folded, batch, "seed {seed} order {order}");
            let got: Settlement =
                settle(&s.pools, &folded, &index, s.stages, &PayoutParams::REV2).unwrap();
            assert_eq!(got, want);
        }
    }
}

#[test]
fn passive_wallets_and_bot_farms_do_not_pay_by_themselves() {
    // One faction: 50 regular humans and 50 passive Sybil wallets, all
    // paying the citizen fee only. A passive wallet's tenure is 1.0, a
    // regular's 1.25, so the Sybil gets back less than 80% of its fee.
    let mut recs: Vec<CitizenRecord> = (0..50).map(|_| rec(0, 4 * USDC, 0, 12_500)).collect();
    recs.extend((0..50).map(|_| rec(0, 4 * USDC, 0, TENURE_ONE)));
    let st = settle(
        &pools_of(&recs),
        &totals_of(&recs),
        &[INDEX_ONE; FACTIONS],
        0,
        &PayoutParams::REV2,
    )
    .unwrap();
    let sybil = claim(&st, &recs[99]).unwrap();
    assert!(
        sybil.paid * 10 < 4 * USDC * 8,
        "passive wallet got {}",
        sybil.paid
    );
}
