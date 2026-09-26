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

use crate::fixed::BPS_ONE;
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

/// Basis points as the divisor of a share.
const BPS: u64 = BPS_ONE as u64;

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
    let roll = Roll::new(state, rules, extras.roster);
    let scores = nation_scores(state, rules);
    // A nation without members, without an active member who is not an
    // operator AI, or without a city (destroyed) takes no share.
    let counted: Vec<bool> = (0..n)
        .map(|c| state.city_count(c as u16) > 0 && roll.has_active(c, false))
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
    let ai_slice = ai_only_slice(&roll, &scores, &counted, pool, total);
    let takers = counted.iter().filter(|x| **x).count() as u64;
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
        refund(&roll, paid_in, &mut out);
        return out;
    }
    for (civ, pts) in points.iter().enumerate() {
        let equal_part = if counted[civ] { ai_slice / takers } else { 0 };
        let share = mul_div(by_points, *pts, total) + equal_part + bounty[civ];
        out.nation_share[civ] = share;
        if share > 0 {
            split_nation(&roll, rules, entry_fee, civ, share, &mut out);
        }
    }
    if extras.roster.iter().any(|x| *x) {
        redistribute(&roll, rules, entry_fee, &points, &mut out);
    }
    out.dust = paid_in - out.per_member.iter().sum::<u64>();
    out
}

/// The members as settlement sees them: who was active, who is on the
/// revealed operator AI roster.
struct Roll<'a> {
    state: &'a WorldState,
    active: Vec<bool>,
    roster: &'a [bool],
}

impl<'a> Roll<'a> {
    fn new(state: &'a WorldState, rules: &Ruleset, roster: &'a [bool]) -> Self {
        let active = state
            .members
            .iter()
            .map(|m| is_active_member(rules, m))
            .collect();
        Roll {
            state,
            active,
            roster,
        }
    }

    /// Member `i` is an operator AI.
    fn ai(&self, i: usize) -> bool {
        self.roster.get(i).copied().unwrap_or(false)
    }

    /// `civ` has an active member who is (`ai`) or is not an operator AI.
    fn has_active(&self, civ: usize, ai: bool) -> bool {
        (0..self.state.members.len()).any(|i| {
            self.state.members[i].civ as usize == civ && self.active[i] && self.ai(i) == ai
        })
    }

    /// Every member of `civ`, by id.
    fn members_of(&self, civ: usize) -> Vec<usize> {
        (0..self.state.members.len())
            .filter(|i| self.state.members[*i].civ as usize == civ)
            .collect()
    }

    /// The members of `civ` who are people (not operator AIs).
    fn people_of(&self, civ: usize) -> impl Iterator<Item = usize> + '_ {
        (0..self.state.members.len())
            .filter(move |i| !self.ai(*i) && self.state.members[*i].civ as usize == civ)
    }

    /// The active ones among `ids`, or all of them if none was active.
    fn active_or_all(&self, ids: &[usize]) -> Vec<usize> {
        let act: Vec<usize> = ids.iter().copied().filter(|i| self.active[*i]).collect();
        if act.is_empty() {
            ids.to_vec()
        } else {
            act
        }
    }
}

/// A nation run only by operator AI members (active, with a city) earned a
/// share by its points, but has nobody to pay: that share goes to the
/// counted nations in equal parts, not by points, so AI-only nations do not
/// feed the leader (V5 §18.5). Returns that part of `pool`.
fn ai_only_slice(
    roll: &Roll,
    scores: &[NationScore],
    counted: &[bool],
    pool: u64,
    total: u64,
) -> u64 {
    let ai_points: u64 = (0..counted.len())
        .filter(|c| {
            !counted[*c] && roll.state.city_count(*c as u16) > 0 && roll.has_active(*c, true)
        })
        .map(|c| scores[c].total())
        .sum();
    let takers = counted.iter().filter(|x| **x).count();
    if total > 0 && takers > 0 {
        mul_div(pool, ai_points, total + ai_points)
    } else {
        0
    }
}

/// Nobody achieved anything: refund what was paid in pro rata to fees paid,
/// which are equal (V5 §7.2); operator AIs' fees were the operator's, so
/// only people are refunded.
fn refund(roll: &Roll, paid_in: u64, out: &mut Settlement) {
    out.refund = true;
    let people: Vec<usize> = (0..roll.state.members.len())
        .filter(|i| !roll.ai(*i))
        .collect();
    if !people.is_empty() {
        let each = paid_in / people.len() as u64;
        for i in people {
            out.per_member[i] = each;
        }
    }
    out.dust = paid_in - out.per_member.iter().sum::<u64>();
}

/// One nation's `share` among its members (V5 §7.3): an equal part for the
/// active members (`equal_share_bps`, at most `equal_cap_bps` of the fee
/// each), the rest by merit per path and era.
fn split_nation(
    roll: &Roll,
    rules: &Ruleset,
    entry_fee: u64,
    civ: usize,
    share: u64,
    out: &mut Settlement,
) {
    let ids = roll.members_of(civ);
    let act: Vec<usize> = ids.iter().copied().filter(|i| roll.active[*i]).collect();
    let cap = act.len() as u64 * mul_div(entry_fee, rules.equal_cap_bps as u64, BPS);
    let equal_total = mul_div(share, rules.equal_share_bps as u64, BPS).min(cap);
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
    let weights = merit_components(roll.state, &out.scores[civ], &ids);
    let comp_points: u64 = weights.iter().map(|(p, _)| *p).sum();
    if comp_points == 0 {
        // No merit at all: the rest goes equally to the active members, or
        // to every member if none was active.
        let to = roll.active_or_all(&ids);
        if !to.is_empty() {
            let e = rest / to.len() as u64;
            for i in to {
                out.per_member[i] += e;
            }
        }
        return;
    }
    for (pts, merit) in &weights {
        let amount = mul_div(rest, *pts, comp_points);
        let m_total: u64 = merit.iter().sum();
        for (k, &i) in ids.iter().enumerate() {
            out.per_member[i] += mul_div(amount, merit[k], m_total);
        }
    }
}

/// The merit components of a nation's share: one per path with points and
/// some member merit on it, plus the era bonus split by total merit. Each is
/// (points, merit per member of `ids`).
fn merit_components(state: &WorldState, s: &NationScore, ids: &[usize]) -> Vec<(u64, Vec<u64>)> {
    let mut weights: Vec<(u64, Vec<u64>)> = Vec::new();
    for p in 0..PATHS {
        let m: Vec<u64> = ids
            .iter()
            .map(|&i| state.members[i].merit[p] as u64)
            .collect();
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
    weights
}

/// V5 §18.5: each operator AI's payout goes to the people of its nation by
/// merit; with no merit among them, equally to its active people (or all of
/// them). Each person takes at most `equal_cap × max(1, merit / unit)` of AI
/// payouts over the whole redistribution, where `unit` is
/// `redistribute_merit_unit_bps` of the revealed AIs' average merit: members
/// who only vote, or earn a token of merit, cannot collect it, and there is
/// no cliff (WP10). What a nation cannot take goes to the other nations with
/// people, by points, the same way; what is still left, to every person by
/// merit (or, with no merit anywhere, by what they already receive). None of
/// it returns to the operator.
fn redistribute(
    roll: &Roll,
    rules: &Ruleset,
    entry_fee: u64,
    points: &[u64],
    out: &mut Settlement,
) {
    let state = roll.state;
    let n = state.civs.len();
    let cap = mul_div(entry_fee, rules.equal_cap_bps as u64, BPS);
    let unit = mul_div(ai_avg(roll), rules.redistribute_merit_unit_bps as u64, BPS).max(1);
    let limit = Limit { cap, unit };
    // AI payouts each person has taken so far.
    let mut got = vec![0u64; state.members.len()];
    let mut leftover = 0u64;
    let mut full = Vec::with_capacity(n);
    for civ in 0..n {
        let taken: u64 = roll
            .members_of(civ)
            .into_iter()
            .filter(|i| roll.ai(*i))
            .map(|i| core::mem::take(&mut out.per_member[i]))
            .sum();
        out.redistributed += taken;
        let (left, capped) = give_people(roll, civ, taken, limit, &mut got, &mut out.per_member);
        full.push(capped);
        leftover += left;
    }
    // What a nation's people could not take: to the other nations with
    // people, by points.
    let takers: Vec<usize> = (0..n)
        .filter(|c| !full[*c] && points[*c] > 0 && roll.people_of(*c).next().is_some())
        .collect();
    let pts: u64 = takers.iter().map(|c| points[*c]).sum();
    if leftover > 0 && pts > 0 {
        let mut rest = leftover;
        for c in takers {
            let part = mul_div(leftover, points[c], pts);
            let (left, _) = give_people(roll, c, part, limit, &mut got, &mut out.per_member);
            rest -= part - left;
        }
        leftover = rest;
    }
    // Still left: to every person by merit, so an equal-share or token-merit
    // receipt does not multiply.
    let merit: Vec<u64> = (0..state.members.len())
        .map(|i| {
            if roll.ai(i) {
                0
            } else {
                state.members[i].merit_total()
            }
        })
        .collect();
    let m_total: u64 = merit.iter().sum();
    if leftover > 0 && m_total > 0 {
        let mut given = 0u64;
        for (i, m) in merit.iter().enumerate() {
            let x = mul_div(leftover, *m, m_total);
            out.per_member[i] += x;
            given += x;
        }
        leftover -= given;
    }
    // Then (no person has merit, or rounding dust): by what they receive.
    let base: u64 = (0..state.members.len())
        .filter(|i| !roll.ai(*i))
        .map(|i| out.per_member[i])
        .sum();
    if leftover > 0 && base > 0 {
        let now: Vec<u64> = out.per_member.clone();
        for (i, x) in now.iter().enumerate() {
            if !roll.ai(i) {
                out.per_member[i] += mul_div(leftover, *x, base);
            }
        }
    }
}

/// The revealed AI members' average merit (0 without any).
fn ai_avg(roll: &Roll) -> u64 {
    let (mut sum, mut k) = (0u64, 0u64);
    for i in (0..roll.state.members.len()).filter(|i| roll.ai(*i)) {
        sum = sum.saturating_add(roll.state.members[i].merit_total());
        k += 1;
    }
    sum.checked_div(k).unwrap_or(0)
}

/// A person's limit on AI payouts: `cap × max(1, merit / unit)`, continuous
/// in merit.
#[derive(Clone, Copy)]
struct Limit {
    cap: u64,
    unit: u64,
}

impl Limit {
    fn of(self, merit: u64) -> u64 {
        mul_div(self.cap, merit, self.unit).max(self.cap)
    }
}

/// Give `amount` to the people of `civ` by merit, else equally, each up to
/// their limit less what they already took (`got`). Returns what could not
/// be given and whether every person of the nation is at their limit.
fn give_people(
    roll: &Roll,
    civ: usize,
    amount: u64,
    limit: Limit,
    got: &mut [u64],
    per_member: &mut [u64],
) -> (u64, bool) {
    if amount == 0 {
        return (0, false);
    }
    let people: Vec<usize> = roll.people_of(civ).collect();
    let merit: Vec<u64> = people
        .iter()
        .map(|i| roll.state.members[*i].merit_total())
        .collect();
    let m_total: u64 = merit.iter().sum();
    let mut given = 0u64;
    if m_total > 0 {
        for (k, i) in people.iter().enumerate() {
            let room = limit.of(merit[k]).saturating_sub(got[*i]);
            let x = mul_div(amount, merit[k], m_total).min(room);
            per_member[*i] += x;
            got[*i] += x;
            given += x;
        }
    } else {
        let to = roll.active_or_all(&people);
        if to.is_empty() {
            return (amount, false);
        }
        let even = amount / to.len() as u64;
        for i in &to {
            let x = even.min(limit.cap.saturating_sub(got[*i]));
            per_member[*i] += x;
            got[*i] += x;
            given += x;
        }
    }
    let full = people
        .iter()
        .enumerate()
        .all(|(k, i)| got[*i] >= limit.of(merit[k]));
    (amount - given, full)
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
