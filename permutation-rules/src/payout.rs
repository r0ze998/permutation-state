//! Prize settlement (Game Design V5 §7): the pool goes to nations by
//! achievement points, and inside each nation to its members, an equal share
//! among active members and the rest by merit.
//!
//! The same function gives the final payouts (chain program, verifier) and
//! the "if the season ended now" projection (server, clients).
//!
//! With operator AI members (V5 §18) settlement also takes the revealed
//! roster and the bounties: a nation counts only with an active member who
//! is not on the roster, bounties are added to the captor nation's share, and
//! every roster member's payout goes to the people of its nation (§18.5).

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
    /// Bounty USDC added to each nation's share (V5 §18.4).
    pub bounty: Vec<u64>,
    /// USDC taken from roster members and paid to people (V5 §18.5).
    pub redistributed: u64,
}

/// What settlement adds when the season had operator AI members.
#[derive(Clone, Copy, Debug, Default)]
pub struct Extras<'a> {
    /// Per member: on the revealed roster (empty = nobody).
    pub roster: &'a [bool],
    /// Bounty per nation (empty = none).
    pub bounty: &'a [u64],
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
    settle_with(state, rules, pool, entry_fee, &Extras::default())
}

/// `settle` with the revealed roster and bounties (V5 §18.4–§18.5).
pub fn settle_with(
    state: &WorldState,
    rules: &Ruleset,
    pool: u64,
    entry_fee: u64,
    extras: &Extras,
) -> Settlement {
    let n = state.civs.len();
    let scores = nation_scores(state, rules);
    let ai = |i: usize| extras.roster.get(i).copied().unwrap_or(false);
    let active: Vec<bool> = state
        .members
        .iter()
        .map(|m| is_active_member(rules, m))
        .collect();
    // A nation without members, without an active member who is not an
    // operator AI, or without a city (destroyed) takes no share.
    let counted: Vec<bool> = (0..n)
        .map(|c| {
            state.city_count(c as u16) > 0
                && state
                    .members
                    .iter()
                    .zip(&active)
                    .enumerate()
                    .any(|(i, (m, a))| m.civ as usize == c && *a && !ai(i))
        })
        .collect();
    let bounty_of = |c: usize| extras.bounty.get(c).copied().unwrap_or(0);
    // A bounty earned by a nation that takes no share joins the pool.
    let pool = pool + (0..n).filter(|c| !counted[*c]).map(bounty_of).sum::<u64>();
    let bounty: Vec<u64> = (0..n)
        .map(|c| if counted[c] { bounty_of(c) } else { 0 })
        .collect();
    let paid_in = pool + bounty.iter().sum::<u64>();
    let points: Vec<u64> = (0..n)
        .map(|c| if counted[c] { scores[c].total() } else { 0 })
        .collect();
    let total: u64 = points.iter().sum();
    // A nation run only by operator AI members (active, with a city) earned
    // a share by its points, but has nobody to pay: that share goes to the
    // counted nations in equal parts, not by points, so AI-only nations do
    // not feed the leader (V5 §18.5).
    let ai_only: Vec<bool> = (0..n)
        .map(|c| {
            !counted[c]
                && state.city_count(c as u16) > 0
                && state
                    .members
                    .iter()
                    .zip(&active)
                    .enumerate()
                    .any(|(i, (m, a))| m.civ as usize == c && *a && ai(i))
        })
        .collect();
    let ai_points: u64 = (0..n)
        .filter(|c| ai_only[*c])
        .map(|c| scores[c].total())
        .sum();
    let takers = counted.iter().filter(|x| **x).count() as u64;
    let ai_slice = if total > 0 && takers > 0 {
        mul_div(pool, ai_points, total + ai_points)
    } else {
        0
    };
    let by_points = pool - ai_slice;
    let mut out = Settlement {
        pool,
        scores,
        counted: counted.clone(),
        nation_share: vec![0; n],
        equal_each: vec![0; n],
        per_member: vec![0; state.members.len()],
        refund: false,
        dust: 0,
        bounty: bounty.clone(),
        redistributed: 0,
    };

    if total == 0 {
        // Nobody achieved anything: refund the pool pro rata to fees paid,
        // which are equal (V5 §7.2); operator AIs' fees were the operator's,
        // so only people are refunded.
        out.refund = true;
        let people: Vec<usize> = (0..state.members.len()).filter(|i| !ai(*i)).collect();
        if !people.is_empty() {
            let each = paid_in / people.len() as u64;
            for i in people {
                out.per_member[i] = each;
            }
        }
        out.dust = paid_in - out.per_member.iter().sum::<u64>();
        return out;
    }

    for (civ, pts) in points.iter().enumerate() {
        let equal_part = if counted[civ] { ai_slice / takers } else { 0 };
        let share = mul_div(by_points, *pts, total) + equal_part + bounty[civ];
        out.nation_share[civ] = share;
        if share == 0 {
            continue;
        }
        let ids: Vec<usize> = (0..state.members.len())
            .filter(|i| state.members[*i].civ as usize == civ)
            .collect();
        let act: Vec<usize> = ids.iter().copied().filter(|i| active[*i]).collect();

        // Equal share (20%), capped per member at half the fee.
        let cap = act.len() as u64 * mul_div(entry_fee, rules.equal_cap_bps as u64, 10_000);
        let equal_total = mul_div(share, rules.equal_share_bps as u64, 10_000).min(cap);
        let each = if act.is_empty() {
            0
        } else {
            equal_total / act.len() as u64
        };
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
        let tot: Vec<u64> = ids
            .iter()
            .map(|&i| state.members[i].merit_total())
            .collect();
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
    if extras.roster.iter().any(|x| *x) {
        redistribute(state, rules, entry_fee, &points, &active, &ai, &mut out);
    }
    out.dust = paid_in - out.per_member.iter().sum::<u64>();
    out
}

/// V5 §18.5: each operator AI's payout goes to the people of its nation by
/// merit; with no merit among them, equally to its active people (or all of
/// them), at most `equal_cap_bps` of the fee each (so members who only vote
/// cannot collect it). What a nation cannot take goes to the other nations
/// with people, by points, the same way; what is still left, to every
/// person in proportion to what they already receive. None of it returns to
/// the operator.
fn redistribute(
    state: &WorldState,
    rules: &Ruleset,
    entry_fee: u64,
    points: &[u64],
    active: &[bool],
    ai: &dyn Fn(usize) -> bool,
    out: &mut Settlement,
) {
    let n = state.civs.len();
    let cap = mul_div(entry_fee, rules.equal_cap_bps as u64, 10_000);
    let mut leftover = 0u64;
    let mut full = Vec::with_capacity(n);
    for civ in 0..n {
        let taken: u64 = (0..state.members.len())
            .filter(|i| ai(*i) && state.members[*i].civ as usize == civ)
            .map(|i| core::mem::take(&mut out.per_member[i]))
            .sum();
        out.redistributed += taken;
        let (left, capped) = give_people(state, civ, taken, cap, active, ai, &mut out.per_member);
        full.push(capped);
        leftover += left;
    }
    // What a nation's people could not take: to the other nations with
    // people, by points.
    let takers: Vec<usize> = (0..n)
        .filter(|c| !full[*c] && points[*c] > 0 && people_of(state, *c, ai).next().is_some())
        .collect();
    let pts: u64 = takers.iter().map(|c| points[*c]).sum();
    if leftover > 0 && pts > 0 {
        let mut rest = leftover;
        for c in takers {
            let part = mul_div(leftover, points[c], pts);
            let (left, _) = give_people(state, c, part, cap, active, ai, &mut out.per_member);
            rest -= part - left;
        }
        leftover = rest;
    }
    // Still left: to every person by what they receive already.
    let base: u64 = (0..state.members.len())
        .filter(|i| !ai(*i))
        .map(|i| out.per_member[i])
        .sum();
    if leftover > 0 && base > 0 {
        let now: Vec<u64> = out.per_member.clone();
        for (i, x) in now.iter().enumerate() {
            if !ai(i) {
                out.per_member[i] += mul_div(leftover, *x, base);
            }
        }
    }
}

fn people_of<'a>(
    state: &'a WorldState,
    civ: usize,
    ai: &'a dyn Fn(usize) -> bool,
) -> impl Iterator<Item = usize> + 'a {
    (0..state.members.len()).filter(move |i| !ai(*i) && state.members[*i].civ as usize == civ)
}

/// Give `amount` to the people of `civ` by merit, else equally up to `cap`
/// each. Returns what could not be given and whether the cap stopped it.
fn give_people(
    state: &WorldState,
    civ: usize,
    amount: u64,
    cap: u64,
    active: &[bool],
    ai: &dyn Fn(usize) -> bool,
    per_member: &mut [u64],
) -> (u64, bool) {
    if amount == 0 {
        return (0, false);
    }
    let people: Vec<usize> = people_of(state, civ, ai).collect();
    let merit: Vec<u64> = people
        .iter()
        .map(|i| state.members[*i].merit_total())
        .collect();
    let m_total: u64 = merit.iter().sum();
    if m_total > 0 {
        let mut given = 0;
        for (k, i) in people.iter().enumerate() {
            let x = mul_div(amount, merit[k], m_total);
            per_member[*i] += x;
            given += x;
        }
        return (amount - given, false);
    }
    let act: Vec<usize> = people.iter().copied().filter(|i| active[*i]).collect();
    let to = if act.is_empty() { &people } else { &act };
    if to.is_empty() {
        return (amount, false);
    }
    let even = amount / to.len() as u64;
    let each = even.min(cap);
    for i in to {
        per_member[*i] += each;
    }
    (amount - each * to.len() as u64, even > cap)
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
