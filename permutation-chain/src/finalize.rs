//! What `FinishSeason` computes from the final world, as a pure function so
//! the verifier recomputes exactly the same (V5 §7, §18).
//!
//! - The pool is the entry fees' 80%, the pool's share of in-play income,
//!   and the treasury left in nations nobody deposited into (it has no
//!   depositor to return to).
//! - With operator AI members: if the roster was revealed, the bounties go
//!   to the nations that conquered the AIs' home cities (unpaid ones join the
//!   pool), the AIs' payouts go to people, and the bond returns to the
//!   operator (operations); if it was not revealed in time, the bounties and
//!   the bond join the pool and everyone is settled as a person.

use permutation_rules::params::Ruleset;
use permutation_rules::payout::{settle_with, Extras, Settlement};
use permutation_rules::roster::bounties;
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
    /// Everything paid into settlement (pool and bounties).
    pub pool: u64,
    /// Added to operations: its share of in-play income, rounding, and the
    /// bond when the roster was revealed.
    pub ops_add: u64,
    pub treasury_final: Vec<u64>,
    pub bounty_paid: Vec<u64>,
}

pub fn finalize(
    season: &Season,
    state: &WorldState,
    rules: &Ruleset,
    roster: Roster,
) -> Finalization {
    let n = state.civs.len();
    let mut pool = season.pool + state.exchange_vault;
    let mut ops_add = state.exchange_ops;
    // A treasury nobody deposited into has nobody to be refunded: its USDC
    // (contract income, market sales) joins the pool.
    let treasury_final: Vec<u64> = (0..n)
        .map(|c| {
            let left = state.civs[c].usdc;
            if season.treasury.get(c).copied().unwrap_or(0) == 0 {
                pool += left;
                0
            } else {
                left
            }
        })
        .collect();
    let escrow = season.bounty_each * season.ai_count as u64;
    let mut members = vec![false; state.members.len()];
    let mut by_civ = vec![0u64; n];
    match roster {
        Roster::None => {}
        Roster::Revealed(entries) => {
            let infos: Vec<(u16, [u8; 32])> = entries.iter().map(|e| (e.civ, e.salt)).collect();
            let b = bounties(state, &infos, season.bounty_each);
            pool += b.unpaid;
            by_civ = b.by_civ;
            for e in entries {
                if let Some(x) = members.get_mut(e.member as usize) {
                    *x = true;
                }
            }
            ops_add += season.bond;
        }
        Roster::Forfeited => pool += escrow + season.bond,
    }
    let s = settle_with(
        state,
        rules,
        pool,
        season.entry_fee,
        &Extras {
            roster: &members,
            bounty: &by_civ,
        },
    );
    ops_add += s.dust;
    let bounty_paid = s.bounty.clone();
    Finalization {
        pool: pool + bounty_paid.iter().sum::<u64>(),
        ops_add,
        treasury_final,
        bounty_paid,
        settlement: s,
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

    /// Everything the vault holds leaves it: payouts, refunds, operations.
    fn conserved(season: &Season, state: &WorldState, f: &Finalization) {
        let vault = season.pool
            + season.ops
            + season.treasury.iter().sum::<u64>()
            + season.bounty_each * season.ai_count as u64
            + season.bond;
        assert_eq!(state.usdc_deposited, season.treasury.iter().sum::<u64>());
        let out = f.settlement.per_member.iter().sum::<u64>()
            + f.treasury_final.iter().sum::<u64>()
            + season.ops
            + f.ops_add;
        assert_eq!(out, vault);
    }

    #[test]
    fn a_revealed_roster_gives_the_ai_share_to_people_and_the_bond_back() {
        let (rules, state, season) = fixture();
        let entries = [RosterEntry {
            member: 0,
            civ: 0,
            salt: [5; 32],
            shares: 0,
        }];
        let f = finalize(&season, &state, &rules, Roster::Revealed(&entries));
        assert_eq!(f.settlement.per_member[0], 0, "the AI takes nothing");
        assert!(f.settlement.redistributed > 0);
        assert_eq!(
            f.treasury_final,
            vec![2_000_000, 0],
            "an orphan treasury joins the pool"
        );
        // The home city was never conquered: the bounty joins the pool; the
        // bond returns to operations.
        assert!(f.ops_add >= season.bond);
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
        let entries = [RosterEntry {
            member: 0,
            civ: 0,
            salt: [5; 32],
            shares: 0,
        }];
        let f = finalize(&season, &state, &rules, Roster::Revealed(&entries));
        assert_eq!(f.bounty_paid, vec![0, season.bounty_each]);
        let plain = finalize(&season, &state, &rules, Roster::None);
        assert!(
            f.settlement.nation_share[1] >= plain.settlement.nation_share[1] + season.bounty_each
        );
        conserved(&season, &state, &f);

        state.cities[capital as usize].first_conquest = Some(Conquest {
            by: 1,
            tick: 60,
            bounty: false,
        });
        let f = finalize(&season, &state, &rules, Roster::Revealed(&entries));
        assert_eq!(
            f.bounty_paid,
            vec![0, 0],
            "a recent pact partner earns nothing"
        );
        conserved(&season, &state, &f);
    }

    #[test]
    fn an_unrevealed_roster_forfeits_the_bounties_and_the_bond() {
        let (rules, state, season) = fixture();
        let f = finalize(&season, &state, &rules, Roster::Forfeited);
        assert!(f.settlement.per_member[0] > 0, "settled as a person");
        assert!(f.ops_add < season.bond);
        let plain = finalize(&season, &state, &rules, Roster::None);
        assert_eq!(f.pool, plain.pool + 4_000_000 + 20_000_000);
        conserved(&season, &state, &f);
    }
}
