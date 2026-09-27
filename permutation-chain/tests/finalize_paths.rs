//! Settlement over many seeded seasons (WP09 §6.3, WP10 §6.2, WP12): with
//! the bond at its floor, withholding the roster never pays the operator
//! (the forfeit's equal split costs it at least `forfeit_penalty` against a
//! reveal), a treasury drained into the AI nation is not recouped through a
//! forfeit, and every honest outcome balances exactly (never voided;
//! claims plus operations use up `outstanding`).

use permutation_chain::finalize::{finalize, paid_in, Finalization, Roster};
use permutation_chain::payout::claim_amount;
use permutation_chain::state::*;
use permutation_rules::genesis::{new_season, Entry};
use permutation_rules::gov::{join, Path};
use permutation_rules::state::WorldState;
use permutation_rules::{Preset, Ruleset};

const FEE: u64 = 10_000_000;

/// splitmix64: a small deterministic generator for the seeded cases.
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
        self.next() % n
    }
}

/// One seeded season at its end: `n` members over `civs` nations (member
/// `i` in nation `i % civs`), the first `a` of them the operator's AIs,
/// every member depositing `dep`, the bond at its floor.
struct Case {
    rules: Ruleset,
    world: WorldState,
    season: Season,
    members: Vec<MemberAccount>,
    roster: Vec<RosterEntry>,
    a: usize,
}

fn case(rng: &mut Rng, n: usize, a: usize, civs: usize, dep: u64, bounty: u64) -> Case {
    let rules = Ruleset::new(Preset::Blitz);
    let count = |c: usize| (0..n).filter(|i| i % civs == c).count() as u64;
    let entries: Vec<Entry> = (0..civs)
        .map(|c| Entry {
            name: format!("n{c}"),
            treasury: dep * count(c),
        })
        .collect();
    let mut w = new_season(&rules, &[3; 32], &[4; 32], &entries).unwrap();
    for i in 0..n {
        let mut key = [0x11; 32];
        key[..8].copy_from_slice(&(i as u64).to_le_bytes());
        join(&mut w, &rules, (i % civs) as u16, key).unwrap();
    }
    for m in &mut w.members {
        m.windows = (1 << 18) - 1;
        m.merit[Path::Science as usize] = rng.below(60_000) as u32;
        m.merit[Path::Hegemony as usize] = rng.below(5_000) as u32;
    }
    for civ in 0..civs {
        let k = rng.below(rules.science_techs[0] as u64 + 1) as usize;
        for t in permutation_rules::tech::TECHS.iter().take(k) {
            w.civs[civ].techs.insert(t.tech);
        }
    }
    w.tick = rules.ticks_per_season;
    let ops = n as u64 * FEE / 5;
    let mut season = Season {
        magic: SEASON_MAGIC,
        season_id: 1,
        bump: 0,
        vault_bump: 0,
        admin: [0; 32],
        crank: [0; 32],
        usdc_mint: [0; 32],
        usdc_decimals: 6,
        preset: 0,
        nations: civs as u8,
        entry_fee: FEE,
        tick_seconds: 30,
        market: true,
        status: SeasonStatus::Running,
        world_seed: [3; 32],
        season_seed: [4; 32],
        member_count: n as u32,
        nation_members: (0..civs).map(|c| count(c) as u32).collect(),
        seated: n as u32,
        pool: n as u64 * FEE - ops,
        ops,
        ops_withdrawn: false,
        treasury: entries.iter().map(|e| e.treasury).collect(),
        treasury_final: vec![],
        payouts: vec![],
        final_root: [0; 32],
        prev_season_id: 0,
        prev_history_root: [0; 32],
        history_root: [0; 32],
        ai_count: a as u16,
        roster_commit: [0; 32],
        bounty_each: bounty,
        bond: 0,
        roster_acc: [0; 32],
        roster_revealed: a as u16,
        roster_outcome: 0,
        bounty_paid: vec![],
        delegated: 0,
        roster_blind: [0; 32],
        refund_base: vec![],
        refund_in_payout: vec![],
        seed_state: 0,
        seed_oracle: [0; 32],
        seed_requested_at: 0,
        seed_requests: 0,
        deposit: dep,
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
    season.bond = bond_floor(&season);
    let members = (0..n)
        .map(|i| MemberAccount {
            magic: MEMBER_MAGIC,
            season_id: 1,
            bump: 0,
            index: i as u32,
            civ: (i % civs) as u16,
            wallet: [i as u8; 32],
            session: [0; 32],
            kind: 0,
            name: String::new(),
            attestation: [0; 32],
            stand: 0,
            votes: [u32::MAX; 4],
            shares: dep,
            claimed: false,
            tag: [0; 32],
        })
        .collect();
    let roster = (0..a)
        .map(|i| RosterEntry {
            member: i as u32,
            civ: (i % civs) as u16,
            salt: [i as u8; 32],
            shares: dep,
        })
        .collect();
    Case {
        rules,
        world: w,
        season,
        members,
        roster,
        a,
    }
}

/// Moves every people-only nation's treasury into nation 0 (an AI nation):
/// the drain an AI-run government could push through trades.
fn drain(c: &mut Case) {
    let civs = c.world.civs.len();
    for civ in 1..civs {
        if (0..c.a).all(|i| i % civs != civ) {
            let moved = c.world.civs[civ].usdc;
            c.world.civs[civ].usdc = 0;
            c.world.civs[0].usdc += moved;
        }
    }
}

/// The Season as FinishSeason stores it, and every member's claim.
fn claims(c: &Case, f: &Finalization) -> (Season, Vec<u64>) {
    let mut s = c.season.clone();
    s.status = SeasonStatus::Finalized;
    (s.pool, s.ops, s.outstanding) = (f.pool, f.ops, f.owed);
    s.payouts = f.payouts.clone();
    s.treasury_final = f.treasury_final.clone();
    s.refund_base = f.refund_base.clone();
    s.refund_in_payout = f.refund_in_payout.clone();
    let out = c.members.iter().map(|m| claim_amount(&s, m)).collect();
    (s, out)
}

/// Books that balance: never voided, `owed == paid_in`, and the claims plus
/// the operations share use up `outstanding` (rounding stays in the vault).
fn balances(c: &Case, f: &Finalization) -> Vec<u64> {
    assert!(!f.voided, "an honest settlement was voided");
    assert_eq!(f.owed as u128, paid_in(&c.season));
    let (s, out) = claims(c, f);
    let paid: u128 = out.iter().map(|x| *x as u128).sum::<u128>() + s.ops as u128;
    assert!(
        paid <= s.outstanding as u128,
        "claims exceed what was set aside"
    );
    assert!(
        s.outstanding as u128 - paid <= c.members.len() as u128 + c.world.civs.len() as u128,
        "more than rounding left behind"
    );
    out
}

/// What the operator takes beyond its fee share: its AIs' claims and what
/// settlement adds to operations (the bond back when revealed, dust).
fn operator(c: &Case, f: &Finalization, out: &[u64]) -> i128 {
    out[..c.a].iter().map(|x| *x as i128).sum::<i128>() + f.ops as i128 - c.season.ops as i128
}

fn people(c: &Case, out: &[u64]) -> i128 {
    out[c.a..].iter().map(|x| *x as i128).sum()
}

/// WP09 §6.3: over seeded merit, member and AI counts, a forfeited roster
/// (equal split) pays the AIs at most the bond less the penalty, people at
/// least the penalty more than the reveal paid them, and balances.
#[test]
fn withholding_the_roster_never_pays_the_operator() {
    let mut rng = Rng(9);
    for round in 0..60 {
        let n = 2 + rng.below(47) as usize; // 2..=48
        let a = 1 + rng.below(12.min(n as u64 - 1)) as usize;
        let civs = 2 + rng.below(5) as usize;
        let dep = [0, 1_000_000, 2_500_001][rng.below(3) as usize];
        let bounty = [0, 1_000_000, 5_000_000][rng.below(3) as usize];
        let c = case(&mut rng, n, a, civs, dep, bounty);
        let rev = finalize(
            &c.season,
            &c.world,
            &c.rules,
            Roster::Revealed(&c.roster),
            false,
        );
        let forf = finalize(&c.season, &c.world, &c.rules, Roster::Forfeited, false);
        let (rev_out, forf_out) = (balances(&c, &rev), balances(&c, &forf));
        let penalty = forfeit_penalty(&c.season) as i128;
        let ai: i128 = forf.settlement.per_member[..a]
            .iter()
            .map(|x| *x as i128)
            .sum();
        assert!(
            ai + penalty <= c.season.bond as i128,
            "round {round} (n {n}, a {a}): AIs {ai} + penalty {penalty} > bond {}",
            c.season.bond
        );
        let people_rev: i128 = rev.settlement.per_member[a..]
            .iter()
            .map(|x| *x as i128)
            .sum();
        let people_forf: i128 = forf.settlement.per_member[a..]
            .iter()
            .map(|x| *x as i128)
            .sum();
        assert!(
            people_forf >= people_rev + penalty - n as i128,
            "round {round} (n {n}, a {a}): people {people_forf} < {people_rev} + {penalty}"
        );
        let gain = operator(&c, &rev, &rev_out) - operator(&c, &forf, &forf_out);
        assert!(
            gain >= penalty - n as i128,
            "round {round} (n {n}, a {a}): reveal − withhold = {gain} < {penalty}"
        );
    }
}

/// Audit repro `roster_reveal_grief.rs` flip (WP09): with the bond at
/// `max(default, bond_floor)`, `operator(reveal) − operator(forfeit) ≥
/// forfeit_penalty` for 18, 24 and 42 members (12 AIs).
#[test]
fn a_reveal_beats_a_forfeit_by_the_penalty() {
    let mut rng = Rng(17);
    for n in [18, 24, 42] {
        let mut c = case(&mut rng, n, 12, 6, 0, 5_000_000);
        c.season.bond = c.season.bond.max(2 * 12 * 5_000_000);
        for (i, m) in c.world.members.iter_mut().enumerate() {
            m.merit[Path::Science as usize] = if i < 12 { 50_000 } else { 100 };
        }
        let rev = finalize(
            &c.season,
            &c.world,
            &c.rules,
            Roster::Revealed(&c.roster),
            false,
        );
        let forf = finalize(&c.season, &c.world, &c.rules, Roster::Forfeited, false);
        let gain =
            operator(&c, &rev, &balances(&c, &rev)) - operator(&c, &forf, &balances(&c, &forf));
        let penalty = forfeit_penalty(&c.season) as i128;
        println!(
            "n {n}: bond {}, reveal − withhold {gain}, penalty {penalty}",
            c.season.bond
        );
        assert!(gain >= penalty - n as i128, "n {n}: {gain} < {penalty}");
    }
}

/// WP10: a treasury drained into the AI nation is not recouped through a
/// forfeit: the operator takes no more than with the reveal (up to
/// rounding), and people no less.
#[test]
fn drain_then_forfeit_does_not_pay_the_operator() {
    let mut rng = Rng(23);
    for round in 0..40 {
        let n = 4 + rng.below(30) as usize;
        let a = 1 + rng.below(3) as usize;
        let civs = 2 + rng.below(4) as usize;
        let mut c = case(&mut rng, n, a, civs, 3_000_000, 1_000_000);
        drain(&mut c);
        let rev = finalize(
            &c.season,
            &c.world,
            &c.rules,
            Roster::Revealed(&c.roster),
            false,
        );
        let forf = finalize(&c.season, &c.world, &c.rules, Roster::Forfeited, false);
        let (rev_out, forf_out) = (balances(&c, &rev), balances(&c, &forf));
        assert!(
            operator(&c, &forf, &forf_out) <= operator(&c, &rev, &rev_out) + n as i128,
            "round {round} (n {n}, a {a}): the forfeit paid the operator more"
        );
        assert!(
            people(&c, &forf_out) >= people(&c, &rev_out) - n as i128,
            "round {round} (n {n}, a {a}): the forfeit paid people less"
        );
        // A revealed AI never gets back more than its deposit.
        let (s, _) = claims(&c, &rev);
        for i in 0..a {
            assert!(
                rev.payouts[i] - rev.settlement.per_member[i] <= s.deposit,
                "AI {i} refunded above its deposit"
            );
        }
    }
}

/// WP12 test 7 over seeded seasons: no roster, a revealed roster and a
/// forfeit, with and without a drain, all balance.
#[test]
fn honest_seasons_balance_exactly() {
    let mut rng = Rng(31);
    for _ in 0..40 {
        let n = 2 + rng.below(40) as usize;
        let civs = 2 + rng.below(5) as usize;
        let dep = [0, 1, 999_999, 3_333_333][rng.below(4) as usize];
        let mut people_only = case(&mut rng, n, 0, civs, dep, 0);
        people_only.season.bond = 0;
        balances(
            &people_only,
            &finalize(
                &people_only.season,
                &people_only.world,
                &people_only.rules,
                Roster::None,
                false,
            ),
        );
        let a = 1 + rng.below(n as u64 - 1).min(11) as usize;
        let mut c = case(&mut rng, n, a, civs, dep, 2_000_000);
        if rng.below(2) == 0 {
            drain(&mut c);
        }
        for roster in [Roster::Revealed(&c.roster), Roster::Forfeited] {
            balances(&c, &finalize(&c.season, &c.world, &c.rules, roster, false));
        }
    }
}
