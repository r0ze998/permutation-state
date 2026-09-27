//! What `FinishSeason` computes from the final world, as a pure function so
//! the verifier recomputes exactly the same (V5 §7, §18).
//!
//! - The pool is the entry fees' 80%, the pool's share of in-play income
//!   (all of it in seasons with operator AI members), and the treasury left
//!   in nations nobody deposited into (it has no depositor to return to).
//! - Treasuries are refunded to their depositors pro rata, rounded to an
//!   exact split of the uniform deposit (the remainder goes to operations).
//!   A revealed AI member gets at most its deposit back, inside its payout;
//!   a forfeited roster caps every refund at the deposit. What the caps hold
//!   back joins the pool.
//! - With operator AI members: if the roster was revealed, the bounties go
//!   to the nations that conquered the AIs' home cities (unpaid ones join the
//!   pool), the AIs' payouts go to people, and the bond returns to the
//!   operator (operations); if it was not revealed in time, the bounties and
//!   the bond join the pool and the whole pool is split equally among every
//!   member (`settle_equal`).
//! - A season whose final world does not conserve USDC, or whose settlement
//!   does not balance exactly against what the vault took in, is voided
//!   (`void`): everyone gets back what it paid in.

use permutation_rules::params::Ruleset;
use permutation_rules::payout::{settle_with, Extras, Settlement};
use permutation_rules::roster::bounties;
use permutation_rules::scoring::{nation_scores, NationScore};
use permutation_rules::state::WorldState;

use crate::state::{RosterEntry, Season, ROSTER_FORFEITED, ROSTER_NONE, ROSTER_REVEALED};

/// How the operator's roster stands at the end.
#[derive(Clone, Copy, Debug)]
pub enum Roster<'a> {
    /// No operator AI members this season.
    None,
    /// Revealed in full (checked against the committed chain).
    Revealed(&'a [RosterEntry]),
    /// Not revealed within the grace period.
    Forfeited,
}

impl Roster<'_> {
    pub const fn outcome(&self) -> u8 {
        match self {
            Roster::None => ROSTER_NONE,
            Roster::Revealed(_) => ROSTER_REVEALED,
            Roster::Forfeited => ROSTER_FORFEITED,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Finalization {
    pub settlement: Settlement,
    /// Everything settlement paid out (its pool and every bounty paid into a
    /// share); 0 when voided.
    pub pool: u64,
    /// Operations' total after the season: the fees' share, in-play income's
    /// share (seasons without AI members), rounding, the bond when the
    /// roster was revealed; the escrow when voided.
    pub ops: u64,
    /// Per nation: what its depositors other than revealed AI members share,
    /// pro rata to `refund_base`.
    pub treasury_final: Vec<u64>,
    /// Per nation: the deposit shares `treasury_final` is refunded over.
    pub refund_base: Vec<u64>,
    /// Per member: the settlement's payout, plus a revealed AI member's
    /// treasury refund (never more than its deposit).
    pub payouts: Vec<u64>,
    /// Bitset of the members whose refund is inside `payouts`.
    pub refund_in_payout: Vec<u8>,
    pub bounty_paid: Vec<u64>,
    /// USDC was not conserved, or settlement did not balance: everyone gets
    /// back what it paid in.
    pub voided: bool,
    /// What the vault owes: payouts + final treasuries + operations.
    pub owed: u64,
}

/// USDC the vault received for the season: every entry fee, every
/// deposit, and the operator's escrow (u128: never wraps).
pub fn paid_in(season: &Season) -> u128 {
    season.member_count as u128 * season.entry_fee as u128
        + season.treasury.iter().map(|x| *x as u128).sum::<u128>()
        + season.ai_count as u128 * season.bounty_each as u128
        + season.bond as u128
}

fn sum(xs: &[u64]) -> u128 {
    xs.iter().map(|x| *x as u128).sum()
}

fn mul_div(a: u64, b: u64, c: u64) -> u64 {
    if c == 0 {
        0
    } else {
        (a as u128 * b as u128 / c as u128) as u64
    }
}

/// The settlement of the final world. `usdc_broken`: the world's
/// `WorldMeta::usdc_broken`, set by ResolveTick when any completed tick left
/// USDC unconserved, even if a later tick healed it (the verifier passes
/// what its own replay found). A world that does not conserve USDC, or a
/// settlement that does not pay out exactly `paid_in`, is voided.
pub fn finalize(
    season: &Season,
    state: &WorldState,
    rules: &Ruleset,
    roster: Roster,
    usdc_broken: bool,
) -> Finalization {
    let conserved = !usdc_broken
        && permutation_rules::invariants::usdc_conserved(state)
        && state.usdc_deposited as u128 == sum(&season.treasury)
        && state.civs.len() == season.treasury.len()
        && state.members.len() == season.member_count as usize;
    let mut scores = None;
    if conserved {
        match settle(season, state, rules, roster) {
            Ok(f) if f.owed as u128 == paid_in(season) => return f,
            Ok(f) => scores = Some(f.settlement.scores),
            Err(s) => scores = s,
        }
    }
    let scores = scores.unwrap_or_else(|| nation_scores(state, rules));
    void(season, scores)
}

/// Settlement's own balance, recomputed in u128: what it paid its members
/// never exceeds what it was given (pool + bounties), and its dust is
/// exactly the difference (a u64 `dust` that wrapped is refused).
pub fn settlement_dust(s: &Settlement) -> Option<u64> {
    let given = s.pool as u128 + sum(&s.bounty);
    let dust = given.checked_sub(sum(&s.per_member))?;
    (dust == s.dust as u128).then_some(s.dust)
}

/// A forfeited roster (WP09): the whole pool split equally among every
/// member, AI or not (nobody can tell them apart without the roster). The
/// remainder is dust (operations). The scores are kept for the history
/// record.
pub fn settle_equal(state: &WorldState, rules: &Ruleset, pool: u64) -> Settlement {
    let (n, m) = (state.civs.len(), state.members.len());
    let each = if m == 0 { 0 } else { pool / m as u64 };
    let nation_share = (0..n)
        .map(|c| each * state.members.iter().filter(|x| x.civ as usize == c).count() as u64)
        .collect();
    Settlement {
        pool,
        scores: nation_scores(state, rules),
        counted: vec![false; n],
        nation_share,
        equal_each: vec![each; n],
        per_member: vec![each; m],
        refund: true,
        dust: pool - each * m as u64,
        bounty: vec![0; n],
        redistributed: 0,
    }
}

type Refused = Option<Vec<NationScore>>;

/// The normal settlement. `Err` (with the scores if they were computed)
/// when a sum does not fit, settlement does not balance, or a total does
/// not fit a u64.
fn settle(
    season: &Season,
    state: &WorldState,
    rules: &Ruleset,
    roster: Roster,
) -> Result<Finalization, Refused> {
    let (n, m) = (state.civs.len(), state.members.len());
    let mut pool = season.pool as u128 + state.exchange_vault as u128;
    let mut ops = season.ops as u128;
    // In-play income: to operations only when no operator AI plays (WP10
    // C3): the operator takes no cut of the churn its AIs could drive.
    if season.ai_count > 0 {
        pool += state.exchange_ops as u128;
    } else {
        ops += state.exchange_ops as u128;
    }
    let entries: &[RosterEntry] = match roster {
        Roster::Revealed(e) => e,
        _ => &[],
    };
    // Treasury refunds (V5 §7.5 ①, §18.1). A treasury nobody deposited into
    // has nobody to refund: it joins the pool. A revealed AI member gets at
    // most its deposit back (the operator never recoups people's USDC); a
    // forfeited roster caps everyone at their deposit, since the AI wallets
    // are unknown. What the caps hold back joins the pool.
    let mut treasury_final = vec![0u64; n];
    let mut refund_base = vec![0u64; n];
    let mut ai_refund = vec![0u64; m];
    for c in 0..n {
        let deposited = season.treasury.get(c).copied().unwrap_or(0);
        let mut left = state.civs[c].usdc;
        if deposited == 0 {
            pool += left as u128;
            continue;
        }
        if matches!(roster, Roster::Forfeited) && left > deposited {
            pool += (left - deposited) as u128;
            left = deposited;
        }
        let capped = left.min(deposited);
        let (mut ai_shares, mut ai_paid) = (0u64, 0u64);
        for e in entries.iter().filter(|e| e.civ as usize == c) {
            let shares = e.shares.min(deposited - ai_shares);
            ai_shares += shares;
            let r = mul_div(shares, capped, deposited);
            if let Some(x) = ai_refund.get_mut(e.member as usize) {
                *x = r;
                ai_paid += r;
            }
        }
        let people = deposited - ai_shares;
        let people_part = mul_div(people, left, deposited);
        // The sum of floors never exceeds the floor of the sum.
        let rest = left
            .checked_sub(people_part)
            .and_then(|x| x.checked_sub(ai_paid))
            .ok_or(None)?;
        pool += rest as u128;
        // Uniform deposits: the people hold `k` equal shares; the remainder
        // of an equal split goes to operations, as settlement dust does.
        let k = if season.deposit > 0 {
            people / season.deposit
        } else {
            1
        };
        let dust = if k > 1 { people_part % k } else { 0 };
        ops += dust as u128;
        treasury_final[c] = people_part - dust;
        refund_base[c] = people;
    }
    let escrow = season.bounty_each as u128 * season.ai_count as u128;
    let mut members = vec![false; m];
    let mut by_civ = vec![0u64; n];
    match roster {
        Roster::None => {}
        Roster::Revealed(entries) => {
            let infos: Vec<(u16, [u8; 32])> = entries.iter().map(|e| (e.civ, e.salt)).collect();
            let b = bounties(state, &infos, season.bounty_each);
            pool += b.unpaid as u128;
            by_civ = b.by_civ;
            for e in entries {
                if let Some(x) = members.get_mut(e.member as usize) {
                    *x = true;
                }
            }
            ops += season.bond as u128;
        }
        Roster::Forfeited => pool += escrow + season.bond as u128,
    }
    // Settlement adds the bounties to the pool in u64: every total it
    // computes must fit.
    let pool = u64::try_from(pool + sum(&by_civ))
        .map(|_| pool as u64)
        .map_err(|_| None)?;
    let s = match roster {
        Roster::Forfeited => settle_equal(state, rules, pool),
        _ => settle_with(
            state,
            rules,
            pool,
            season.entry_fee,
            &Extras {
                roster: &members,
                bounty: &by_civ,
            },
        ),
    };
    let Some(dust) = settlement_dust(&s) else {
        return Err(Some(s.scores));
    };
    ops += dust as u128;
    let mut payouts = s.per_member.clone();
    let mut refund_in_payout = vec![0u8; m.div_ceil(8)];
    for (i, flag) in members.iter().enumerate() {
        if *flag {
            payouts[i] = match payouts[i].checked_add(ai_refund[i]) {
                Some(x) => x,
                None => return Err(Some(s.scores)),
            };
            refund_in_payout[i / 8] |= 1 << (i % 8);
        }
    }
    let bounty_paid = s.bounty.clone();
    let owed = sum(&payouts) + sum(&treasury_final) + ops;
    let (Ok(ops), Ok(owed), Ok(pool)) = (
        u64::try_from(ops),
        u64::try_from(owed),
        u64::try_from(s.pool as u128 + sum(&bounty_paid)),
    ) else {
        return Err(Some(s.scores));
    };
    Ok(Finalization {
        settlement: s,
        pool,
        ops,
        treasury_final,
        refund_base,
        payouts,
        refund_in_payout,
        bounty_paid,
        voided: false,
        owed,
    })
}

/// Everyone gets back what it paid in: members their entry fee (payout)
/// and deposit (the final treasury equals the deposits, refunded over the
/// deposits), the operator its escrow (operations). Uses only the Season,
/// plus the scores for the history record.
pub fn void(season: &Season, scores: Vec<NationScore>) -> Finalization {
    let n = season.treasury.len();
    let per_member = vec![season.entry_fee; season.member_count as usize];
    let ops = season.bounty_each as u128 * season.ai_count as u128 + season.bond as u128;
    let treasury_final = season.treasury.clone();
    let owed = sum(&per_member) + sum(&treasury_final) + ops;
    let settlement = Settlement {
        pool: 0,
        scores,
        counted: vec![false; n],
        nation_share: vec![0; n],
        equal_each: vec![0; n],
        per_member: per_member.clone(),
        refund: true,
        dust: 0,
        bounty: vec![0; n],
        redistributed: 0,
    };
    Finalization {
        settlement,
        pool: 0,
        ops: u64::try_from(ops).unwrap_or(u64::MAX),
        refund_base: treasury_final.clone(),
        treasury_final,
        payouts: per_member,
        refund_in_payout: Vec::new(),
        bounty_paid: vec![0; n],
        voided: true,
        owed: u64::try_from(owed).unwrap_or(u64::MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{SeasonStatus, SEASON_MAGIC};
    use permutation_rules::genesis::{new_season, Entry};
    use permutation_rules::gov::{join, Path};
    use permutation_rules::params::Preset;

    const FEE: u64 = 10_000_000;

    /// A finished world: nation 0 has an AI (member 0) and a person (member
    /// 1); nation 1 a person (member 2). Nation 0 deposited 5 USDC, nation 1
    /// nothing, but nation 1 ends with 3 USDC (contract income).
    fn fixture() -> (Ruleset, WorldState, Season) {
        let rules = Ruleset::new(Preset::Blitz);
        let entries = [
            Entry {
                name: "a".into(),
                treasury: 5_000_000,
            },
            Entry {
                name: "b".into(),
                treasury: 0,
            },
        ];
        let mut s = new_season(&rules, &[1; 32], &[2; 32], &entries).unwrap();
        for (k, civ) in [0u16, 0, 1].into_iter().enumerate() {
            join(&mut s, &rules, civ, [k as u8 + 1; 32]).unwrap();
        }
        for m in &mut s.members {
            m.windows = (1 << 18) - 1;
            m.merit[Path::Science as usize] = 1_000;
        }
        for civ in 0..2 {
            for t in permutation_rules::tech::TECHS
                .iter()
                .take(rules.science_techs[0] as usize)
            {
                s.civs[civ].techs.insert(t.tech);
            }
        }
        // Nation 0 paid nation 1 3 USDC through a contract.
        s.civs[0].usdc = 2_000_000;
        s.civs[1].usdc = 3_000_000;
        s.tick = rules.ticks_per_season;
        let ops = 3 * FEE / 5;
        let season = Season {
            magic: SEASON_MAGIC,
            season_id: 9,
            bump: 0,
            vault_bump: 0,
            admin: [0; 32],
            crank: [0; 32],
            usdc_mint: [0; 32],
            usdc_decimals: 6,
            preset: 0,
            nations: 2,
            entry_fee: FEE,
            tick_seconds: 30,
            market: true,
            status: SeasonStatus::Running,
            world_seed: [1; 32],
            season_seed: [2; 32],
            member_count: 3,
            nation_members: vec![2, 1],
            seated: 3,
            pool: 3 * FEE - ops,
            ops,
            ops_withdrawn: false,
            treasury: vec![5_000_000, 0],
            treasury_final: Vec::new(),
            payouts: Vec::new(),
            final_root: [0; 32],
            prev_season_id: 0,
            prev_history_root: [0; 32],
            history_root: [0; 32],
            ai_count: 1,
            roster_commit: [0; 32],
            bounty_each: 4_000_000,
            bond: 20_000_000,
            roster_acc: [0; 32],
            roster_revealed: 1,
            roster_outcome: 0,
            bounty_paid: Vec::new(),
            delegated: 0,
            roster_blind: [0; 32],
            refund_base: Vec::new(),
            refund_in_payout: Vec::new(),
            seed_state: 0,
            seed_oracle: [0; 32],
            seed_requested_at: 0,
            seed_requests: 0,
            deposit: 0,
            outstanding: 0,
            voided: false,
            start_by: 0,
            stage_at: 0,
            rolled_back: 0,
            aborted_from: 0,
            validator: [0; 32],
            rules_version: 0,
            rules_hash: [0; 32],
            logic_version: 0,
            created_slot: 0,
        };
        (rules, s, season)
    }

    /// The same season without operator AI members (no escrow).
    fn no_ai(season: &Season) -> Season {
        let mut s = season.clone();
        (s.ai_count, s.bounty_each, s.bond) = (0, 0, 0);
        s
    }

    /// The AI of the fixture (member 0 of nation 0), holding `shares`.
    fn ai(shares: u64) -> [RosterEntry; 1] {
        [RosterEntry {
            member: 0,
            civ: 0,
            salt: [5; 32],
            shares,
        }]
    }

    /// Everything the vault holds leaves it: payouts, refunds, operations.
    fn conserved(season: &Season, state: &WorldState, f: &Finalization) {
        let vault = season.pool
            + season.ops
            + season.treasury.iter().sum::<u64>()
            + season.bounty_each * season.ai_count as u64
            + season.bond;
        assert_eq!(state.usdc_deposited, season.treasury.iter().sum::<u64>());
        let out = f.payouts.iter().sum::<u64>() + f.treasury_final.iter().sum::<u64>() + f.ops;
        assert_eq!(out, vault);
        assert!(!f.voided);
        assert_eq!(f.owed as u128, paid_in(season));
    }

    #[test]
    fn a_revealed_roster_gives_the_ai_share_to_people_and_the_bond_back() {
        let (rules, state, season) = fixture();
        let f = finalize(&season, &state, &rules, Roster::Revealed(&ai(0)), false);
        assert_eq!(f.settlement.per_member[0], 0, "the AI takes nothing");
        assert!(f.settlement.redistributed > 0);
        assert_eq!(
            f.treasury_final,
            vec![2_000_000, 0],
            "an orphan treasury joins the pool"
        );
        // The home city was never conquered: the bounty joins the pool; the
        // bond returns to operations.
        assert!(f.ops >= season.ops + season.bond);
        conserved(&season, &state, &f);
    }

    #[test]
    fn a_conquered_home_pays_its_bounty_into_the_captors_share() {
        use permutation_rules::state::Conquest;
        let (rules, mut state, season) = fixture();
        // The AI (member 0, nation 0) lived in nation 0's capital, which
        // nation 1 conquered after the home tick; a second case where the
        // captor had a recent pact pays nothing.
        let capital = state.civs[0].capital.unwrap();
        state.home_snapshot = vec![vec![capital], vec![]];
        state.cities[capital as usize].first_conquest = Some(Conquest {
            by: 1,
            tick: 60,
            bounty: true,
        });
        let f = finalize(&season, &state, &rules, Roster::Revealed(&ai(0)), false);
        assert_eq!(f.bounty_paid, vec![0, season.bounty_each]);
        let plain = finalize(&no_ai(&season), &state, &rules, Roster::None, false);
        assert!(!plain.voided);
        assert!(
            f.settlement.nation_share[1] >= plain.settlement.nation_share[1] + season.bounty_each
        );
        conserved(&season, &state, &f);

        state.cities[capital as usize].first_conquest = Some(Conquest {
            by: 1,
            tick: 60,
            bounty: false,
        });
        let f = finalize(&season, &state, &rules, Roster::Revealed(&ai(0)), false);
        assert_eq!(
            f.bounty_paid,
            vec![0, 0],
            "a recent pact partner earns nothing"
        );
        conserved(&season, &state, &f);
    }

    /// WP09: a forfeited roster splits the whole pool (fees' pool, in-play
    /// income, orphan treasuries, bounties and bond) equally among every
    /// member.
    #[test]
    fn an_unrevealed_roster_forfeits_the_bounties_and_the_bond() {
        let (rules, state, season) = fixture();
        let f = finalize(&season, &state, &rules, Roster::Forfeited, false);
        assert!(f.settlement.per_member[0] > 0, "settled as a person");
        assert!(f
            .settlement
            .per_member
            .iter()
            .all(|x| *x == f.settlement.per_member[0]));
        assert!(f.ops < season.ops + season.bond);
        let plain = finalize(&no_ai(&season), &state, &rules, Roster::None, false);
        assert!(!plain.voided);
        assert_eq!(f.pool, plain.pool + 4_000_000 + 20_000_000);
        conserved(&season, &state, &f);
    }

    /// WP12 R2 with C26: the people's refund is rounded to an exact split of
    /// the uniform deposit; the odd base unit goes to operations.
    #[test]
    fn uniform_deposits_refund_exactly_and_the_remainder_goes_to_operations() {
        let (rules, mut state, season) = fixture();
        let mut season = no_ai(&season);
        season.deposit = 2_500_000; // nation 0: two members x 2.5 USDC
        state.civs[0].usdc = 2_000_001;
        state.civs[1].usdc = 2_999_999;
        let f = finalize(&season, &state, &rules, Roster::None, false);
        assert!(!f.voided);
        assert_eq!(f.treasury_final, vec![2_000_000, 0]);
        assert_eq!(f.refund_base, vec![5_000_000, 0]);
        let plain = {
            let mut s = season.clone();
            s.deposit = 0;
            finalize(&s, &state, &rules, Roster::None, false)
        };
        assert_eq!(f.ops, plain.ops + 1, "the odd base unit goes to operations");
        let each = 2_500_000u128 * f.treasury_final[0] as u128 / f.refund_base[0] as u128;
        assert_eq!(
            2 * each,
            f.treasury_final[0] as u128,
            "two refunds use up the treasury exactly"
        );
        assert_eq!(f.owed as u128, paid_in(&season));
    }

    /// C26: with a revealed AI depositor, `k` counts the people's deposits
    /// only (`refund_base / deposit`).
    #[test]
    fn the_refund_split_counts_the_people_depositors() {
        let (rules, mut state, mut season) = fixture();
        season.deposit = 1_000_000; // the AI 1 USDC, the people 4
        state.civs[0].usdc = 4_999_998;
        state.civs[1].usdc = 2;
        let f = finalize(
            &season,
            &state,
            &rules,
            Roster::Revealed(&ai(1_000_000)),
            false,
        );
        assert!(!f.voided);
        assert_eq!(f.refund_base[0], 4_000_000);
        assert_eq!(f.treasury_final[0], 3_999_996, "four people's shares");
        assert_eq!(
            f.payouts[0],
            f.settlement.per_member[0] + 999_999,
            "the AI's pro-rata refund, inside its payout"
        );
        conserved(&season, &state, &f);
    }

    #[test]
    fn a_world_that_minted_usdc_voids_the_season() {
        let (rules, mut state, season) = fixture();
        state.civs[1].usdc = state.civs[1].usdc.wrapping_add(u64::MAX); // -1 mod 2^64
        state.civs[0].usdc += 1; // u64 sum still "conserved"
        let f = finalize(&season, &state, &rules, Roster::Revealed(&[]), false);
        assert!(f.voided);
        assert_eq!(f.settlement.per_member, vec![FEE; 3]);
        assert_eq!(f.payouts, vec![FEE; 3]);
        assert_eq!(f.treasury_final, season.treasury);
        assert_eq!(f.refund_base, season.treasury);
        assert_eq!(
            f.ops,
            season.bounty_each * season.ai_count as u64 + season.bond
        );
        assert_eq!(f.pool, 0);
        assert_eq!(f.owed as u128, paid_in(&season));
    }

    /// WP12 R3: `Season.pool` is everything settlement paid out, including
    /// a bounty earned by a nation that takes no share.
    #[test]
    fn the_pool_counts_a_bounty_earned_by_a_nation_that_takes_no_share() {
        use permutation_rules::state::Conquest;
        let (rules, mut state, season) = fixture();
        let capital = state.civs[0].capital.unwrap();
        state.home_snapshot = vec![vec![capital], vec![]];
        state.cities[capital as usize].first_conquest = Some(Conquest {
            by: 1,
            tick: 60,
            bounty: true,
        });
        state.members[2].windows = 0; // nation 1's only person is inactive: not counted
        let f = finalize(&season, &state, &rules, Roster::Revealed(&ai(0)), false);
        assert!(!f.voided);
        assert!(!f.settlement.counted[1]);
        let paid: u64 = f.settlement.per_member.iter().sum::<u64>() + f.settlement.dust;
        assert_eq!(
            f.pool, paid,
            "Season.pool is everything settlement paid out"
        );
    }

    #[test]
    fn a_settlement_that_overpays_its_members_does_not_balance() {
        let (rules, state, season) = fixture();
        let f = finalize(&season, &state, &rules, Roster::Revealed(&ai(0)), false);
        let mut s = f.settlement.clone();
        assert_eq!(settlement_dust(&s), Some(s.dust));
        // A settle bug overpays member 1 by 3; `dust = paid_in - Σ` wraps
        // and `ops += dust` would wrap back: the u64 books still balance.
        s.per_member[1] += 3;
        s.dust = s.dust.wrapping_sub(3);
        let ops_u64 = f.ops.wrapping_sub(3);
        let owed_u64 =
            s.per_member.iter().sum::<u64>() + f.treasury_final.iter().sum::<u64>() + ops_u64;
        assert_eq!(
            owed_u64 as u128,
            paid_in(&season),
            "a u64 check is blind to it"
        );
        assert_eq!(
            settlement_dust(&s),
            None,
            "the u128 recomputation refuses it"
        );
    }

    #[test]
    fn a_break_that_healed_before_the_end_still_voids() {
        let (rules, state, season) = fixture();
        let f = finalize(&season, &state, &rules, Roster::Revealed(&ai(0)), true);
        assert!(f.voided);
        assert_eq!(f.owed as u128, paid_in(&season));
        assert_eq!(
            f.settlement.scores.len(),
            state.civs.len(),
            "scores kept for the history record"
        );
    }

    /// WP12 test 6: books that do not balance against `paid_in` void.
    #[test]
    fn a_settlement_that_does_not_balance_voids() {
        let (rules, state, mut season) = fixture();
        season.pool += 1;
        let f = finalize(&season, &state, &rules, Roster::Revealed(&ai(0)), false);
        assert!(f.voided);
        assert_eq!(f.owed as u128, paid_in(&season));
        // Roster::None in a season that escrowed a bounty and a bond.
        let (rules, state, season) = fixture();
        assert!(finalize(&season, &state, &rules, Roster::None, false).voided);
    }

    /// WP12 test 7: every honest outcome balances (the guard against a
    /// legitimate settlement tripping the void).
    #[test]
    fn honest_seasons_are_never_voided() {
        use permutation_rules::state::Conquest;
        let (rules, state, season) = fixture();
        let plain = no_ai(&season);
        conserved(
            &plain,
            &state,
            &finalize(&plain, &state, &rules, Roster::None, false),
        );
        for shares in [0, 1, 5_000_000] {
            let mut s = season.clone();
            s.deposit = 0;
            let f = finalize(&s, &state, &rules, Roster::Revealed(&ai(shares)), false);
            conserved(&s, &state, &f);
        }
        conserved(
            &season,
            &state,
            &finalize(&season, &state, &rules, Roster::Forfeited, false),
        );
        // A conquered home.
        let mut w = state.clone();
        let capital = w.civs[0].capital.unwrap();
        w.home_snapshot = vec![vec![capital], vec![]];
        w.cities[capital as usize].first_conquest = Some(Conquest {
            by: 1,
            tick: 60,
            bounty: true,
        });
        conserved(
            &season,
            &w,
            &finalize(&season, &w, &rules, Roster::Revealed(&ai(0)), false),
        );
        // A refund season: nobody scored.
        let mut w = state.clone();
        for c in &mut w.civs {
            c.techs = Default::default();
        }
        let f = finalize(&plain, &w, &rules, Roster::None, false);
        conserved(&plain, &w, &f);
        // A treasury that grew above its deposits, forfeited: capped, the
        // excess joins the pool.
        let mut w = state.clone();
        w.civs[0].usdc = 4_000_000;
        w.civs[1].usdc = 1_000_000;
        let f = finalize(&season, &w, &rules, Roster::Forfeited, false);
        conserved(&season, &w, &f);
    }

    /// WP10 C2: a revealed AI gets at most its deposit back, inside its
    /// payout (flagged); people keep their pro-rata gains.
    #[test]
    fn a_revealed_ai_gets_at_most_its_deposit_back() {
        let (rules, mut state, mut season) = fixture();
        // The AI deposited 1 USDC of nation 0's 2; nation 0 took nation 1's
        // 4 USDC: its treasury tripled.
        season.treasury = vec![2_000_000, 4_000_000];
        state.usdc_deposited = 6_000_000;
        state.civs[0].usdc = 6_000_000;
        state.civs[1].usdc = 0;
        let f = finalize(
            &season,
            &state,
            &rules,
            Roster::Revealed(&ai(1_000_000)),
            false,
        );
        assert_eq!(f.refund_in_payout, vec![0b001], "member 0 flagged");
        assert_eq!(
            f.payouts[0],
            f.settlement.per_member[0] + 1_000_000,
            "its deposit, not its share of the gain"
        );
        assert_eq!(f.refund_base, vec![1_000_000, 4_000_000]);
        assert_eq!(
            f.treasury_final,
            vec![3_000_000, 0],
            "the person's pro-rata share of the tripled treasury"
        );
        conserved(&season, &state, &f);
    }

    /// WP10: a forfeited roster caps every refund at the deposit.
    #[test]
    fn a_forfeited_roster_caps_every_refund_at_the_deposit() {
        let (rules, mut state, season) = fixture();
        state.civs[0].usdc = 7_000_000; // above the 5 USDC deposited
        state.civs[1].usdc = 0;
        state.usdc_deposited = 5_000_000;
        state.exchange_vault = 2_000_000;
        state.civs[0].usdc = 3_000_000;
        let f = finalize(&season, &state, &rules, Roster::Forfeited, false);
        assert!(f.treasury_final[0] <= season.treasury[0]);
        conserved(&season, &state, &f);
        let mut rich = state.clone();
        rich.exchange_vault = 0;
        rich.civs[0].usdc = 5_000_000;
        let f = finalize(&season, &rich, &rules, Roster::Forfeited, false);
        assert_eq!(f.treasury_final, vec![5_000_000, 0]);
        conserved(&season, &rich, &f);
    }

    /// No roster: the refund rule is the plain pro-rata one.
    #[test]
    fn no_roster_is_unchanged() {
        let (rules, state, season) = fixture();
        let season = no_ai(&season);
        let f = finalize(&season, &state, &rules, Roster::None, false);
        assert_eq!(f.treasury_final, vec![state.civs[0].usdc, 0]);
        assert_eq!(f.refund_base, season.treasury);
        assert_eq!(f.payouts, f.settlement.per_member);
        assert!(f.refund_in_payout.iter().all(|b| *b == 0));
    }

    /// WP10 C3: in AI seasons in-play income joins the pool, not operations.
    #[test]
    fn ai_season_in_play_income_joins_the_pool() {
        let (rules, mut state, season) = fixture();
        state.civs[1].usdc -= 1_000_000;
        state.exchange_ops = 1_000_000;
        let with_ai = finalize(&season, &state, &rules, Roster::Revealed(&ai(0)), false);
        conserved(&season, &state, &with_ai);
        let plain = no_ai(&season);
        let without = finalize(&plain, &state, &rules, Roster::None, false);
        conserved(&plain, &state, &without);
        assert_eq!(
            without.ops,
            plain.ops + 1_000_000 + without.settlement.dust,
            "people-only: operations keep their in-play share"
        );
        assert_eq!(
            with_ai.ops,
            season.ops + season.bond + with_ai.settlement.dust,
            "AI season: none of it"
        );
    }

    /// WP12 revision 2: the final root FinishSeason stores (the stored
    /// body's hash) equals `state_root`, and the encoding round-trips.
    #[test]
    fn the_final_root_is_the_hash_of_the_stored_body() {
        let (_, state, _) = fixture();
        let body = borsh::to_vec(&state).unwrap();
        let root = solana_program::hash::hashv(&[&body]).to_bytes();
        assert_eq!(root, state.state_root().unwrap());
        let back: WorldState = borsh::BorshDeserialize::try_from_slice(&body).unwrap();
        assert_eq!(borsh::to_vec(&back).unwrap(), body);
    }

    #[test]
    fn settle_equal_splits_the_whole_pool() {
        let (rules, state, _) = fixture();
        let s = settle_equal(&state, &rules, 100);
        assert_eq!(s.per_member, vec![33; 3]);
        assert_eq!(s.dust, 1);
        assert_eq!(s.nation_share, vec![66, 33]);
        assert_eq!(settlement_dust(&s), Some(1));
    }
}
