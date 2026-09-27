//! Frontier money in: the entry schedule and the two prize pools (open-world
//! design §5.4, owner decision D3).
//!
//! Every wallet pays a mandatory **citizen fee** and may add an optional
//! **laurel stake**. Both fall with the days left in the season to a 25%
//! floor, both are escrowed until the wallet's first holding settles, and
//! each is then split 80% into its own pool and 20% into the operator's
//! escrow (paid only after a clean Finish; forfeited to the pools when a
//! Shade leaf stays unrevealed).
//!
//! Amounts are USDC base units (6 decimals). Every function is pure or
//! mutates only the value it is given, uses checked integer arithmetic and
//! allocates nothing, so the Frontier program, the verifier and the host
//! simulator share it bit for bit.

use crate::fixed::{Bps, BPS_ONE};
use borsh::{BorshDeserialize, BorshSerialize};

/// One USDC in base units (the mint has 6 decimals).
pub const USDC: u64 = 1_000_000;

/// Failures of the money kernels. Each is a refusal: nothing was changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EconError {
    /// A sum would exceed its integer type.
    Overflow,
    /// A subtraction would go below zero (more out than in).
    Underflow,
    /// Joining or staking after the last join day (§2.6, "Last Call").
    JoinClosed,
    /// A faction id outside `0..FACTIONS`.
    BadFaction,
    /// A reward index asked to go back in time.
    BellBackwards,
    /// A Mandate term asked to do something its state does not allow
    /// (register after close, claim before close or after the sweep).
    TermState,
    /// Season parameters outside their allowed range (`validate`).
    BadParams,
}

/// The entry prices and the pool split. `FRONTIER_28` is decision D3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EntrySchedule {
    /// Citizen fee on day 0 (4 USDC).
    pub citizen_fee_day0: u64,
    /// Laurel stake on day 0 (6 USDC).
    pub laurel_stake_day0: u64,
    /// Season length in days (28).
    pub season_days: u32,
    /// Last day on which Join and AddStake are accepted (21).
    pub last_join_day: u32,
    /// Price floor as a share of the day-0 price (25%).
    pub floor_bps: Bps,
    /// Share of every payment that goes to its pool (80%); the rest is the
    /// operator's escrow.
    pub pool_bps: Bps,
    /// **Laurel-stake accrual ramp (O4 step 2)**, bps. The laurel stake is
    /// priced by the share of the season's expected laurel accrual still to
    /// come, not by the share of days left: the expected accrual rate on
    /// day `t` is `1 + ramp × min(t, last_join_day) / last_join_day` (the
    /// world's emission grows while joins are open, then stays flat), and
    /// `l(day) = l(0) × max(floor, Σ_{t ≥ day} rate / Σ_t rate)`. 0 gives
    /// the D3 days-left schedule exactly. The citizen fee always uses days
    /// left.
    pub stake_ramp_bps: u32,
}

impl EntrySchedule {
    /// The Season 1 preset: 4 + 6 USDC, 28 days, joins until day 21.
    pub const FRONTIER_28: EntrySchedule = EntrySchedule {
        citizen_fee_day0: 4 * USDC,
        laurel_stake_day0: 6 * USDC,
        season_days: 28,
        last_join_day: 21,
        floor_bps: 2_500,
        pool_bps: 8_000,
        stake_ramp_bps: 0,
    };

    /// Season 1 after K3 (owner decision O4 step 2): `FRONTIER_28` with the
    /// laurel stake priced by expected accrual left. The ramp 20,000 bps
    /// (accrual rate 3× day 0's by the last join day) fits the simulator's
    /// world emission curve within 1.3 points of remaining share on every
    /// day [sim]; M3 re-fits it on real play.
    pub const SEASON1: EntrySchedule = EntrySchedule {
        stake_ramp_bps: 20_000,
        ..Self::FRONTIER_28
    };

    /// Refuse schedules a season must never be created with.
    pub fn validate(&self) -> Result<(), EconError> {
        let ok = self.season_days > 0
            && self.season_days <= 366
            && self.last_join_day < self.season_days
            && self.floor_bps <= BPS_ONE
            && self.pool_bps <= BPS_ONE
            && self.stake_ramp_bps <= 100_000;
        if ok {
            Ok(())
        } else {
            Err(EconError::BadParams)
        }
    }

    /// Expected accrual rate on `day`, in units of `last_join_day` × bps
    /// (integer, so the ratio of two sums is exact).
    fn accrual_rate(&self, day: u32) -> u128 {
        let j = self.last_join_day.max(1) as u128;
        j * BPS_ONE as u128 + self.stake_ramp_bps as u128 * day.min(self.last_join_day) as u128
    }

    /// `Σ_{t = day}^{season_days − 1}` of the accrual rate. At most 366
    /// terms (`validate`).
    fn accrual_left(&self, day: u32) -> u128 {
        (day..self.season_days).map(|t| self.accrual_rate(t)).sum()
    }

    /// `base × max(floor, (season_days − day) / season_days)`, rounded down
    /// (the payer's favour), or `None` after the last join day.
    pub fn price(&self, base: u64, day: u32) -> Option<u64> {
        if day > self.last_join_day || day > self.season_days || self.season_days == 0 {
            return None;
        }
        let left = (self.season_days - day) as u128;
        let by_days = base as u128 * left / self.season_days as u128;
        let floor = base as u128 * self.floor_bps as u128 / BPS_ONE as u128;
        Some(by_days.max(floor) as u64)
    }

    /// The citizen fee `c(day)`: 4.00 / 3.00 / 2.00 / 1.00 USDC at days
    /// 0 / 7 / 14 / 21 for `FRONTIER_28`.
    pub fn citizen_fee(&self, day: u32) -> Option<u64> {
        self.price(self.citizen_fee_day0, day)
    }

    /// The laurel stake `l(day)`: 6.00 / 4.50 / 3.00 / 1.50 USDC with no
    /// ramp; with a ramp, priced by the expected accrual still to come
    /// (`stake_ramp_bps`), rounded down, never below the floor.
    pub fn laurel_stake(&self, day: u32) -> Option<u64> {
        if self.stake_ramp_bps == 0 {
            return self.price(self.laurel_stake_day0, day);
        }
        if day > self.last_join_day || day >= self.season_days {
            return None;
        }
        let base = self.laurel_stake_day0 as u128;
        let by_accrual = base * self.accrual_left(day) / self.accrual_left(0);
        let floor = base * self.floor_bps as u128 / BPS_ONE as u128;
        Some(by_accrual.max(floor) as u64)
    }

    /// Days a wallet that joins on `day` can still play (the tenure
    /// denominator of §5.4).
    pub fn days_available(&self, day: u32) -> u32 {
        self.season_days.saturating_sub(day)
    }

    /// 80% to the pool, the remainder (so nothing is lost to rounding) to
    /// the operator's escrow.
    pub fn split(&self, amount: u64) -> Split {
        let pool = (amount as u128 * self.pool_bps as u128 / BPS_ONE as u128) as u64;
        Split {
            pool,
            operator: amount - pool,
        }
    }
}

/// One payment divided between its pool and the operator's escrow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Split {
    pub pool: u64,
    pub operator: u64,
}

/// Which pool a payment funds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Entry {
    CitizenFee,
    LaurelStake,
}

/// The season's money, as the Season account keeps it.
///
/// Invariant (checked by `total`, which the vault shards must equal before
/// any claim): `pending + citizen + laurel + operator_citizen +
/// operator_laurel == Σ paid − Σ withdrawn`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Pools {
    /// Paid but not yet released: the wallet has no settled holding (§3.4).
    pub pending: u64,
    /// Citizen pool `C` (80% of released citizen fees, plus forfeitures).
    pub citizen: u64,
    /// Laurel pool `L` (80% of released laurel stakes, plus forfeitures).
    pub laurel: u64,
    /// Operator's escrowed 20% of citizen fees.
    pub operator_citizen: u64,
    /// Operator's escrowed 20% of laurel stakes.
    pub operator_laurel: u64,
}

impl Pools {
    /// Everything the vault shards hold for this season.
    pub fn total(&self) -> Result<u64, EconError> {
        [
            self.citizen,
            self.laurel,
            self.operator_citizen,
            self.operator_laurel,
        ]
        .iter()
        .try_fold(self.pending, |a, b| a.checked_add(*b))
        .ok_or(EconError::Overflow)
    }

    /// The two prize pools together (what claims are paid from).
    pub fn prize(&self) -> Result<u64, EconError> {
        self.citizen
            .checked_add(self.laurel)
            .ok_or(EconError::Overflow)
    }

    /// `Join` or `AddStake` before the first holding settled: the payment
    /// sits in escrow.
    pub fn pay_pending(&mut self, amount: u64) -> Result<(), EconError> {
        self.pending = self
            .pending
            .checked_add(amount)
            .ok_or(EconError::Overflow)?;
        Ok(())
    }

    /// `WithdrawJoin`: no site after 24 hours, the wallet gets 100% back.
    pub fn withdraw_pending(&mut self, amount: u64) -> Result<(), EconError> {
        self.pending = self
            .pending
            .checked_sub(amount)
            .ok_or(EconError::Underflow)?;
        Ok(())
    }

    /// `SettleTicket` (first holding placed): an escrowed payment moves into
    /// its pool and the operator's escrow.
    pub fn release(
        &mut self,
        sched: &EntrySchedule,
        kind: Entry,
        amount: u64,
    ) -> Result<Split, EconError> {
        let pending = self
            .pending
            .checked_sub(amount)
            .ok_or(EconError::Underflow)?;
        let mut next = *self;
        next.pending = pending;
        let s = next.credit(sched, kind, amount)?;
        *self = next;
        Ok(s)
    }

    /// A payment by a wallet that already has a settled holding (a laurel
    /// stake added later): straight into the pools.
    pub fn pay_settled(
        &mut self,
        sched: &EntrySchedule,
        kind: Entry,
        amount: u64,
    ) -> Result<Split, EconError> {
        let mut next = *self;
        let s = next.credit(sched, kind, amount)?;
        *self = next;
        Ok(s)
    }

    fn credit(
        &mut self,
        sched: &EntrySchedule,
        kind: Entry,
        amount: u64,
    ) -> Result<Split, EconError> {
        let s = sched.split(amount);
        let (pool, op) = match kind {
            Entry::CitizenFee => (&mut self.citizen, &mut self.operator_citizen),
            Entry::LaurelStake => (&mut self.laurel, &mut self.operator_laurel),
        };
        *pool = pool.checked_add(s.pool).ok_or(EconError::Overflow)?;
        *op = op.checked_add(s.operator).ok_or(EconError::Overflow)?;
        Ok(s)
    }

    /// Shade forfeiture (§7.3, §8.10): the operator's escrow goes to the
    /// pools it came from, never anywhere else. Returns the amount moved.
    pub fn forfeit_operator(&mut self) -> Result<u64, EconError> {
        let citizen = self
            .citizen
            .checked_add(self.operator_citizen)
            .ok_or(EconError::Overflow)?;
        let laurel = self
            .laurel
            .checked_add(self.operator_laurel)
            .ok_or(EconError::Overflow)?;
        let moved = self
            .operator_citizen
            .checked_add(self.operator_laurel)
            .ok_or(EconError::Overflow)?;
        self.citizen = citizen;
        self.laurel = laurel;
        self.operator_citizen = 0;
        self.operator_laurel = 0;
        Ok(moved)
    }
}

/// Civilisation Share of the citizen pool: 5% plus 2% per completed Engine
/// stage, 5–15% (§5.4).
pub fn civ_share_bps(engine_stages: u8) -> Bps {
    500 + 200 * engine_stages.min(5) as Bps
}

/// Share of each faction's two pools that funds officer pay (§4.3).
pub const STEWARD_BPS: Bps = 500;

/// Officer pay rows (§4.3), in USDC base units: 3 USDC per Minister-term,
/// 0.5 per paid Warden-term, at most 10 per person per season.
pub const MINISTER_TERM_PAY: u64 = 3 * USDC;
pub const WARDEN_TERM_PAY: u64 = USDC / 2;
pub const STEWARD_PERSON_CAP: u64 = 10 * USDC;

/// A person's steward rows, capped per season (feed the result into the
/// Citizen record and the faction's steward total).
pub fn steward_rows(minister_terms: u32, paid_warden_terms: u32) -> u64 {
    let rows = MINISTER_TERM_PAY as u128 * minister_terms as u128
        + WARDEN_TERM_PAY as u128 * paid_warden_terms as u128;
    rows.min(STEWARD_PERSON_CAP as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_matches_d3() {
        let s = EntrySchedule::FRONTIER_28;
        let c: [u64; 4] = [0, 7, 14, 21].map(|d| s.citizen_fee(d).unwrap());
        let l: [u64; 4] = [0, 7, 14, 21].map(|d| s.laurel_stake(d).unwrap());
        assert_eq!(c, [4_000_000, 3_000_000, 2_000_000, 1_000_000]);
        assert_eq!(l, [6_000_000, 4_500_000, 3_000_000, 1_500_000]);
        assert_eq!(s.citizen_fee(22), None);
        // Rounded down in the payer's favour.
        assert_eq!(s.citizen_fee(1), Some(3_857_142));
    }

    #[test]
    fn a_ramp_prices_late_stakes_by_accrual_left() {
        let flat = EntrySchedule::FRONTIER_28;
        let ramp = EntrySchedule {
            stake_ramp_bps: 10_000,
            ..flat
        };
        assert_eq!(ramp.validate(), Ok(()));
        assert_eq!(ramp.laurel_stake(0), Some(6_000_000));
        let mut last = u64::MAX;
        for d in 0..=21 {
            let (f, r) = (flat.laurel_stake(d).unwrap(), ramp.laurel_stake(d).unwrap());
            // Accrual rises while joins are open, so a late stake costs more
            // than its share of days left; the price still falls every day.
            assert!(r >= f && r <= last, "day {d}: {r} vs {f}");
            last = r;
            assert_eq!(ramp.citizen_fee(d), flat.citizen_fee(d));
        }
        assert_eq!(ramp.laurel_stake(22), None);
        // Day 21: rates 2.0 × 7 of Σ(1 + t/21, t < 21) + 2.0 × 7 = 31 + 14.
        assert_eq!(ramp.laurel_stake(21), Some(6_000_000 * 14 / 45));
    }

    #[test]
    fn split_loses_nothing() {
        let s = EntrySchedule::FRONTIER_28;
        for a in [0, 1, 3, 4, 5, 3_857_142, u64::MAX] {
            let x = s.split(a);
            assert_eq!(x.pool + x.operator, a);
        }
    }
}
