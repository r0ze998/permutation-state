//! Prize settlement (Game Design V5 §7): the pool goes to nations by
//! achievement points, and inside each nation to its members, an equal share
//! among active members and the rest by merit.
//!
//! The same function gives the final payouts (chain program, verifier) and
//! the "if the season ended now" projection (server, clients).

use crate::gov::{is_active_member, PATHS};
use crate::params::Ruleset;
use crate::scoring::{nation_scores, NationScore};
use crate::state::WorldState;
use alloc::vec;
use alloc::vec::Vec;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settlement {
    pub pool: u64,
    /// Points per nation, as scored (whether or not the nation is counted).
    pub scores: Vec<NationScore>,
    /// Whether the nation shares the pool: it still has a city and at least
    /// one active member (V5 §7.2).
    pub counted: Vec<bool>,
    /// USDC per nation.
    pub nation_share: Vec<u64>,
    /// Equal share per active member, per nation.
    pub equal_each: Vec<u64>,
    /// USDC per member (index = member id).
    pub per_member: Vec<u64>,
    /// No counted nation scored: every member gets the same refund.
    pub refund: bool,
    /// Integer remainders, not paid to anyone (they go to operations).
    pub dust: u64,
}

fn mul_div(a: u64, b: u64, c: u64) -> u64 {
    if c == 0 {
        0
    } else {
        (a as u128 * b as u128 / c as u128) as u64
    }
}

/// Split `pool` (USDC base units) by V5 §7.2–§7.3. `entry_fee` is the fee
/// every member paid; it caps the equal share at `equal_cap_bps` of it.
pub fn settle(state: &WorldState, rules: &Ruleset, pool: u64, entry_fee: u64) -> Settlement {
    let n = state.civs.len();
    let scores = nation_scores(state, rules);
    let active: Vec<bool> = state.members.iter().map(|m| is_active_member(rules, m)).collect();
    // A nation without members, without an active member, or without a
    // city (destroyed) takes no share.
    let counted: Vec<bool> = (0..n)
        .map(|c| state.city_count(c as u16) > 0 && state.members.iter().zip(&active).any(|(m, a)| m.civ as usize == c && *a))
        .collect();
    let points: Vec<u64> = (0..n).map(|c| if counted[c] { scores[c].total() } else { 0 }).collect();
    let total: u64 = points.iter().sum();
    let mut out = Settlement {
        pool,
        scores,
        counted,
        nation_share: vec![0; n],
        equal_each: vec![0; n],
        per_member: vec![0; state.members.len()],
        refund: false,
        dust: 0,
    };

    if total == 0 {
        // Nobody achieved anything: refund the pool pro rata to fees paid,
        // which are equal (V5 §7.2).
        out.refund = true;
        let members = state.members.len() as u64;
        if members > 0 {
            let each = pool / members;
            out.per_member.iter_mut().for_each(|p| *p = each);
        }
        out.dust = pool - out.per_member.iter().sum::<u64>();
        return out;
    }

    for (civ, pts) in points.iter().enumerate() {
        let share = mul_div(pool, *pts, total);
        out.nation_share[civ] = share;
        if share == 0 {
            continue;
        }
        let ids: Vec<usize> = (0..state.members.len()).filter(|i| state.members[*i].civ as usize == civ).collect();
        let act: Vec<usize> = ids.iter().copied().filter(|i| active[*i]).collect();

        // Equal share (20%), capped per member at half the fee.
        let cap = act.len() as u64 * mul_div(entry_fee, rules.equal_cap_bps as u64, 10_000);
        let equal_total = mul_div(share, rules.equal_share_bps as u64, 10_000).min(cap);
        let each = if act.is_empty() { 0 } else { equal_total / act.len() as u64 };
        out.equal_each[civ] = each;
        for i in &act {
            out.per_member[*i] += each;
        }
        let rest = share - each * act.len() as u64;

        // Merit share: one component per path with points and member merit,
        // plus the era bonus, split by merit within the component.
        let s = &out.scores[civ];
        let path_merit = |i: usize, p: usize| state.members[i].merit[p] as u64;
        let mut weights: Vec<(u64, Vec<u64>)> = Vec::new();
        for p in 0..PATHS {
            let m: Vec<u64> = ids.iter().map(|&i| path_merit(i, p)).collect();
            if s.path_points[p] > 0 && m.iter().any(|x| *x > 0) {
                weights.push((s.path_points[p], m));
            }
        }
        let tot: Vec<u64> = ids.iter().map(|&i| state.members[i].merit_total()).collect();
        if s.era_points > 0 && tot.iter().any(|x| *x > 0) {
            weights.push((s.era_points, tot));
        }
        let comp_points: u64 = weights.iter().map(|(p, _)| *p).sum();
        if comp_points == 0 {
            // No merit at all: the rest goes equally to the active members,
            // or to every member if none was active.
            let to: &[usize] = if act.is_empty() { &ids } else { &act };
            if !to.is_empty() {
                let e = rest / to.len() as u64;
                for i in to {
                    out.per_member[*i] += e;
                }
            }
            continue;
        }
        for (pts, merit) in &weights {
            let amount = mul_div(rest, *pts, comp_points);
            let m_total: u64 = merit.iter().sum();
            for (k, &i) in ids.iter().enumerate() {
                out.per_member[i] += mul_div(amount, merit[k], m_total);
            }
        }
    }
    out.dust = pool - out.per_member.iter().sum::<u64>();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_div_is_exact_and_safe() {
        assert_eq!(mul_div(u64::MAX, 3, 3), u64::MAX);
        assert_eq!(mul_div(10, 1, 0), 0);
        assert_eq!(mul_div(1_360, 900, 2_720), 450);
    }
}
