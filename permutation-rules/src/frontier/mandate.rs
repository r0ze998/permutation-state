//! The Mandate reserve (open-world design §4.2; owner decision O10): a
//! closed-form, O(1)-per-claim split of each faction's reserve among the
//! **stakers who completed** that term's Mandate.
//!
//! Money flow, in laurel base units:
//!
//! 1. Every laurel credit sends 10% to its faction's [`Reserve`]
//!    (`laurel::mandate_reserve_split`, [`Reserve::deposit`]).
//! 2. During term `t` each completer registers once
//!    ([`MandateTerm::complete`]). Only a wallet that is a staker **when it
//!    completes** gets a share: a fee-only wallet's laurels are worth nothing
//!    to it, and paying it would only dilute the stakers the reserve is for.
//!    The share count is frozen when the term closes.
//! 3. `CloseTerm(f, t)` (permissionless, after the term ends) moves the
//!    whole reserve balance into the term's budget ([`MandateTerm::close`]).
//!    If nobody earned a share, nothing moves and the balance carries over.
//! 4. Each completer claims `min(cap, budget × s_i / S)` rounded down
//!    ([`MandateTerm::claim`]): one read of the term, one write of the
//!    Citizen, no loop over members.
//! 5. `SweepTerm` (after every share was claimed, or after the claim
//!    deadline) returns `budget − paid` to the reserve
//!    ([`MandateTerm::sweep`]): the per-person cap's cut, rounding dust and
//!    unclaimed shares all roll into the next term's budget. Nothing is
//!    ever minted or lost.
//!
//! **Conservation** (checked by [`Reserve::outstanding`] and the tests):
//! `deposited == balance + Σ_open terms (budget − paid) + paid + burned`
//! exactly, at every step, and `Σ claims of a term ≤ budget − floor_cut`.
//!
//! **Cap.** A wallet earns at most [`MANDATE_CAP_PER_TERM`] per term: 2× the
//! average holding's term emission (§4.2), so an officer cannot post a
//! Mandate that pays its own farm beyond it.
//!
//! **Share floor (m0c review: officers steering Mandates).** The divisor is
//! `max(S, floor)`, where `floor` is [`MANDATE_FLOOR_BPS`] of the faction's
//! stakers who were active in the term (a folded count). If the Ministers
//! pick a Mandate only always-online wallets can complete, few complete,
//! and without the floor those few would split the whole budget (up to the
//! cap). With it each share is worth at most `budget / floor`. What the
//! missing completers would have earned (`floor_cut`) is **burned** at the
//! sweep, not returned to the reserve: returned, it would swell the next
//! term's budget, and a steered faction would pay its few completers the
//! cap after a few terms. Laurels only weight the laurel pool, so burning
//! them scales every staker's share alike. In an ordinary term more than
//! half of the active stakers complete, so the floor seldom binds [sim].

use super::laurel::HOLDING_EMISSION_PER_BELL;
use super::pools::EconError;
use super::travel::BELLS_PER_DAY;
use borsh::{BorshDeserialize, BorshSerialize};

/// Days per governance term (Mandates, offices).
pub const TERM_DAYS: u32 = 4;
/// Bells per term.
pub const TERM_BELLS: u64 = TERM_DAYS as u64 * BELLS_PER_DAY as u64;
/// Per-wallet, per-term ceiling on Mandate pay: 2× what an average holding
/// emits in a term (2 × 576 × 1/12 = 96 laurels).
pub const MANDATE_CAP_PER_TERM: u64 = 2 * TERM_BELLS * HOLDING_EMISSION_PER_BELL;
/// Share floor: the divisor of a term's budget is at least this share
/// (bps) of the faction's stakers active in the term.
pub const MANDATE_FLOOR_BPS: u64 = 5_000;

/// The share floor for a faction-term with `active_stakers` stakers active
/// in it.
pub const fn share_floor(active_stakers: u64) -> u64 {
    active_stakers * MANDATE_FLOOR_BPS / 10_000
}

/// A faction's Mandate reserve (one field set in `FactionState`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Reserve {
    /// Laurel units waiting for the next term that closes with shares.
    pub balance: u64,
    /// Everything ever deposited (10% of every credit of the faction).
    pub deposited: u128,
    /// Everything ever paid to completers.
    pub paid: u128,
    /// Floor cuts burned at sweeps (m0c share floor).
    pub burned: u128,
}

impl Reserve {
    /// 10% of a credit arrives (`mandate_reserve_split(..).give`).
    pub fn deposit(&mut self, amount: u64) -> Result<(), EconError> {
        let balance = self
            .balance
            .checked_add(amount)
            .ok_or(EconError::Overflow)?;
        let deposited = self
            .deposited
            .checked_add(amount as u128)
            .ok_or(EconError::Overflow)?;
        self.balance = balance;
        self.deposited = deposited;
        Ok(())
    }

    /// Laurels sitting in closed, not yet swept terms:
    /// `deposited − balance − paid − burned`. The conservation identity is
    /// that this equals `Σ (budget − paid)` over those terms.
    pub fn outstanding(&self) -> Result<u128, EconError> {
        self.deposited
            .checked_sub(self.balance as u128)
            .and_then(|x| x.checked_sub(self.paid))
            .and_then(|x| x.checked_sub(self.burned))
            .ok_or(EconError::Underflow)
    }
}

/// Where a term is in its life.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum TermState {
    /// Completers register shares.
    #[default]
    Open,
    /// Budget fixed; completers claim.
    Closed,
    /// Remainder returned to the reserve; nothing more happens.
    Swept,
}

/// One (faction, term) Mandate board settlement (a small PDA).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MandateTerm {
    pub term: u32,
    pub state: TermState,
    /// Σ shares registered (one per staker who completed the term's Mandate).
    pub shares: u64,
    /// Laurel units moved in from the reserve at close.
    pub budget: u64,
    /// Shares already claimed.
    pub claimed_shares: u64,
    /// Laurel units already paid.
    pub paid: u64,
    /// The divisor fixed at close: `max(shares, floor)`.
    pub divisor: u64,
    /// The unclaimable part of the budget when the floor binds:
    /// `budget − ⌊budget × shares / divisor⌋`, burned at the sweep.
    pub floor_cut: u64,
}

impl MandateTerm {
    pub fn new(term: u32) -> Self {
        MandateTerm {
            term,
            ..Default::default()
        }
    }

    /// A wallet completed this term's Mandate. The program calls it once
    /// per wallet per term (the Citizen stores the last term it completed).
    /// Returns the shares the wallet holds for this term: 1 for a staker,
    /// 0 for a fee-only wallet (it still earns its Works; O10).
    pub fn complete(&mut self, staker: bool) -> Result<u64, EconError> {
        if self.state != TermState::Open {
            return Err(EconError::TermState);
        }
        if !staker {
            return Ok(0);
        }
        self.shares = self.shares.checked_add(1).ok_or(EconError::Overflow)?;
        Ok(1)
    }

    /// `CloseTerm` without a share floor (tests and the M0 comparison).
    pub fn close(&mut self, reserve: &mut Reserve) -> Result<u64, EconError> {
        self.close_with_floor(reserve, 0)
    }

    /// `CloseTerm`: fix the budget and the divisor `max(shares, floor)`
    /// (`floor` = [`share_floor`] of the term's active stakers). With no
    /// shares the term closes empty and the reserve keeps its balance.
    pub fn close_with_floor(
        &mut self,
        reserve: &mut Reserve,
        floor: u64,
    ) -> Result<u64, EconError> {
        if self.state != TermState::Open {
            return Err(EconError::TermState);
        }
        let budget = if self.shares > 0 { reserve.balance } else { 0 };
        reserve.balance -= budget;
        self.budget = budget;
        self.divisor = self.shares.max(floor);
        self.floor_cut = if self.divisor > self.shares {
            budget - (budget as u128 * self.shares as u128 / self.divisor as u128) as u64
        } else {
            0
        };
        self.state = TermState::Closed;
        Ok(budget)
    }

    /// What `shares_i` shares would be paid: `min(cap × shares_i,
    /// budget × shares_i / max(shares, floor))`, rounded down. Pure.
    pub fn quote(&self, shares_i: u64) -> Result<u64, EconError> {
        if self.shares == 0 || shares_i == 0 {
            return Ok(0);
        }
        let pro_rata = (self.budget as u128)
            .checked_mul(shares_i as u128)
            .ok_or(EconError::Overflow)?
            / self.divisor.max(self.shares) as u128;
        let cap = (MANDATE_CAP_PER_TERM as u128)
            .checked_mul(shares_i as u128)
            .ok_or(EconError::Overflow)?;
        Ok(pro_rata.min(cap) as u64)
    }

    /// A completer's claim (O(1)). Refused before close, after the sweep,
    /// or if more shares are claimed than were registered.
    pub fn claim(&mut self, reserve: &mut Reserve, shares_i: u64) -> Result<u64, EconError> {
        if self.state != TermState::Closed {
            return Err(EconError::TermState);
        }
        let claimed = self
            .claimed_shares
            .checked_add(shares_i)
            .ok_or(EconError::Overflow)?;
        if claimed > self.shares {
            return Err(EconError::Underflow);
        }
        let pay = self.quote(shares_i)?;
        let paid = self.paid.checked_add(pay).ok_or(EconError::Overflow)?;
        // Σ ⌊budget × s_i / D⌋ ≤ ⌊budget × S / D⌋ = budget − floor_cut, so
        // this never trips; kept as the on-chain guard of the invariant.
        if paid > self.budget - self.floor_cut {
            return Err(EconError::Underflow);
        }
        let reserve_paid = reserve
            .paid
            .checked_add(pay as u128)
            .ok_or(EconError::Overflow)?;
        self.claimed_shares = claimed;
        self.paid = paid;
        reserve.paid = reserve_paid;
        Ok(pay)
    }

    /// `SweepTerm`: return `budget − paid − floor_cut` (cap cuts, rounding,
    /// unclaimed shares) to the reserve and burn the floor cut.
    /// `deadline_passed` lets a keeper sweep a term whose shares were not
    /// all claimed in time. Returns what went back to the reserve.
    pub fn sweep(
        &mut self,
        reserve: &mut Reserve,
        deadline_passed: bool,
    ) -> Result<u64, EconError> {
        if self.state != TermState::Closed {
            return Err(EconError::TermState);
        }
        if self.claimed_shares < self.shares && !deadline_passed {
            return Err(EconError::TermState);
        }
        let back = self.budget - self.paid - self.floor_cut;
        let balance = reserve
            .balance
            .checked_add(back)
            .ok_or(EconError::Overflow)?;
        let burned = reserve
            .burned
            .checked_add(self.floor_cut as u128)
            .ok_or(EconError::Overflow)?;
        reserve.balance = balance;
        reserve.burned = burned;
        self.state = TermState::Swept;
        Ok(back)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontier::laurel::LAUREL_ONE;

    #[test]
    fn cap_is_96_laurels() {
        assert_eq!(MANDATE_CAP_PER_TERM, 96 * LAUREL_ONE);
    }

    #[test]
    fn only_stakers_get_shares_and_the_rest_rolls_over() {
        let mut r = Reserve::default();
        r.deposit(1_000).unwrap();
        let mut t = MandateTerm::new(0);
        assert_eq!(t.complete(true).unwrap(), 1);
        assert_eq!(t.complete(false).unwrap(), 0);
        assert_eq!(t.complete(true).unwrap(), 1);
        assert_eq!(t.complete(true).unwrap(), 1);
        assert_eq!(t.close(&mut r).unwrap(), 1_000);
        assert_eq!(t.complete(true), Err(EconError::TermState));
        let got = [(); 3].map(|_| t.claim(&mut r, 1).unwrap());
        assert_eq!(got, [333, 333, 333]);
        assert_eq!(t.claim(&mut r, 1), Err(EconError::Underflow));
        assert_eq!(t.sweep(&mut r, false).unwrap(), 1);
        assert_eq!(r.balance, 1);
        assert_eq!(r.outstanding().unwrap(), 0);
        assert_eq!(r.deposited, r.balance as u128 + r.paid);
    }

    /// m0c: a term that only a few (always-online) wallets could complete
    /// pays each of them at most `budget / floor`; the rest rolls back.
    #[test]
    fn a_steered_term_pays_no_more_than_the_floor_share() {
        let mut r = Reserve::default();
        r.deposit(10_000).unwrap();
        let mut t = MandateTerm::new(0);
        for _ in 0..4 {
            t.complete(true).unwrap();
        }
        // 100 stakers were active in the term; only 4 completed.
        let floor = share_floor(100);
        assert_eq!(floor, 50);
        assert_eq!(t.close_with_floor(&mut r, floor).unwrap(), 10_000);
        let got = [(); 4].map(|_| t.claim(&mut r, 1).unwrap());
        assert_eq!(got, [200; 4]); // 10_000 / 50, not 10_000 / 4
                                   // The floor cut is burned, not returned: the next term's budget does
                                   // not grow from it.
        assert_eq!(t.floor_cut, 10_000 - 800);
        assert_eq!(t.sweep(&mut r, false).unwrap(), 0);
        assert_eq!(r.burned, 9_200);
        assert_eq!(r.outstanding().unwrap(), 0);
        assert_eq!(r.deposited, r.balance as u128 + r.paid + r.burned);
        // An ordinary term (more completers than the floor): no change.
        r.deposit(1_000).unwrap();
        let mut u = MandateTerm::new(1);
        for _ in 0..60 {
            u.complete(true).unwrap();
        }
        let budget = u.close_with_floor(&mut r, floor).unwrap();
        assert_eq!(u.quote(1).unwrap(), (budget / 60).min(MANDATE_CAP_PER_TERM));
    }

    #[test]
    fn an_empty_term_keeps_the_reserve() {
        let mut r = Reserve::default();
        r.deposit(500).unwrap();
        let mut t = MandateTerm::new(3);
        t.complete(false).unwrap();
        assert_eq!(t.close(&mut r).unwrap(), 0);
        assert_eq!(r.balance, 500);
        assert_eq!(t.claim(&mut r, 1), Err(EconError::Underflow));
        assert_eq!(t.sweep(&mut r, false).unwrap(), 0);
    }
}
