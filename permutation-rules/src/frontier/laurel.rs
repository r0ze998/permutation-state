//! Laurels: a fixed, zero-sum emission (open-world design §5.4, revision 2).
//!
//! Every non-dormant holding emits 1/12 laurel per bell into its province.
//! The province's emission is split among the holdings in it by **strength
//! weight**, so being stronger than your neighbours takes a larger share of
//! *their* emission, never more emission. Nothing else mints laurels except
//! Relic Sites (1 per bell to the holder). Captures, occupations and sieges
//! only move laurels that already exist.
//!
//! Credit is lazy, with a per-province **reward index** (the staking-reward
//! pattern): the province stores cumulative laurels per unit of weight; each
//! holding stores its weight and the index it last saw; any touch credits
//! `weight × Δindex`. Weights change only on province writes, and the index
//! must be accrued to the current bell before any weight changes.
//!
//! Units: `LAUREL_ONE` base units = 1 laurel, chosen so that a holding's
//! 1/12 per bell is exact. Weights: `WEIGHT_ONE` = a Hamlet, first holding,
//! no garrison.

use super::pools::EconError;
use crate::fixed::{Bps, MilliTroops, BPS_ONE};
use borsh::{BorshDeserialize, BorshSerialize};

/// One laurel in base units.
pub const LAUREL_ONE: u64 = 12_000_000;
/// A holding's emission into its province per bell: 1/12 laurel.
pub const HOLDING_EMISSION_PER_BELL: u64 = LAUREL_ONE / 12;
/// A Relic Site's emission to its holder per bell: 1 laurel (12 holdings).
pub const RELIC_EMISSION_PER_BELL: u64 = LAUREL_ONE;
/// Laurels a siege declaration escrows (§5.4 item 4).
pub const SIEGE_STAKE: u64 = 5 * LAUREL_ONE;
/// Share of the victim's banked laurels a first capture moves (§5.4 item 3).
pub const CAPTURE_BPS: Bps = 2_500;
/// Share an occupier takes of the occupied first holding's credit (§6.3).
pub const OCCUPATION_BPS: Bps = 5_000;
/// Share of each faction's emission set aside for Mandates (§4.2).
pub const MANDATE_RESERVE_BPS: Bps = 1_000;

/// Strength weight of a Hamlet, first holding, empty garrison.
pub const WEIGHT_ONE: u64 = 1_000_000;
/// Reward-index precision: cumulative laurel units per weight unit × 2^64.
pub const INDEX_SCALE: u128 = 1 << 64;

/// Holding tiers (§5.1) with their strength weights 1, 1.3, 1.6, 2.0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Tier {
    Hamlet,
    Town,
    City,
    Stronghold,
}

impl Tier {
    /// Tier weight in percent.
    pub const fn weight_pct(self) -> u64 {
        match self {
            Tier::Hamlet => 100,
            Tier::Town => 130,
            Tier::City => 160,
            Tier::Stronghold => 200,
        }
    }
}

/// Garrison factor `1 + garrison / 2,000 troops`, capped at 1.5, in bps.
pub fn garrison_factor_bps(garrison: MilliTroops) -> u64 {
    // garrison_milli / 2,000,000 × 10,000 = garrison_milli / 200
    (BPS_ONE as u64 + garrison as u64 / 200).min(15_000)
}

/// Holding-order factor in quarters: first 1, second 0.5, third 0.25.
/// `order` is 0-based; anything past the third counts as the third.
pub const fn order_quarters(order: u8) -> u64 {
    match order {
        0 => 4,
        1 => 2,
        _ => 1,
    }
}

/// Strength weight = tier × garrison factor × holding-order factor, in
/// `WEIGHT_ONE` units (0.25 … 3.0), rounded down. Always ≥ 250,000.
pub fn strength_weight(tier: Tier, garrison: MilliTroops, order: u8) -> u64 {
    // pct/100 × bps/10,000 × quarters/4 × 1,000,000 = pct × bps × quarters / 4
    tier.weight_pct() * garrison_factor_bps(garrison) * order_quarters(order) / 4
}

/// A province's reward index (one per Province account; if the program
/// keeps one per faction, each is this same structure).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RewardIndex {
    /// Cumulative laurel units per weight unit, × `INDEX_SCALE`.
    pub acc: u128,
    /// Remainder of the last division, carried so `acc` is exact over time.
    pub carry: u128,
    /// Σ strength weight of the holdings attached.
    pub total_weight: u64,
    /// Holdings currently emitting (non-dormant, and non-occupied if the
    /// program so rules).
    pub emitters: u32,
    /// The bell the index is accrued to.
    pub bell: u64,
    /// Laurel units emitted into the index since genesis.
    pub emitted: u128,
    /// Laurel units emitted while nobody held weight (never credited).
    pub orphaned: u128,
}

/// What a holding stores about its place in its province's index.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Stake {
    pub weight: u64,
    pub emits: bool,
    /// `RewardIndex::acc` when this holding was last credited.
    pub last_acc: u128,
}

impl RewardIndex {
    /// A province opened at `bell`.
    pub fn new(bell: u64) -> Self {
        RewardIndex {
            bell,
            ..Default::default()
        }
    }

    /// Laurel units emitted per bell at the current emitter count.
    pub fn emission_per_bell(&self) -> u64 {
        self.emitters as u64 * HOLDING_EMISSION_PER_BELL
    }

    /// Accrue emission up to `min(to_bell, end_bell)`: the index freezes at
    /// `T_end` (§8.4). A bell before the index's own is refused. O(1).
    pub fn accrue(&mut self, to_bell: u64, end_bell: u64) -> Result<u128, EconError> {
        let to = to_bell.min(end_bell);
        if to < self.bell {
            // Already past T_end (frozen), or a real step back in time.
            return if self.bell >= end_bell {
                Ok(0)
            } else {
                Err(EconError::BellBackwards)
            };
        }
        let bells = (to - self.bell) as u128;
        let e = bells * self.emission_per_bell() as u128;
        let mut next = *self;
        next.bell = to;
        next.emitted = next.emitted.checked_add(e).ok_or(EconError::Overflow)?;
        if e > 0 {
            if next.total_weight == 0 {
                next.orphaned = next.orphaned.checked_add(e).ok_or(EconError::Overflow)?;
            } else {
                let w = next.total_weight as u128;
                let num = e
                    .checked_mul(INDEX_SCALE)
                    .and_then(|x| x.checked_add(next.carry))
                    .ok_or(EconError::Overflow)?;
                next.acc = next.acc.checked_add(num / w).ok_or(EconError::Overflow)?;
                next.carry = num % w;
            }
        }
        *self = next;
        Ok(e)
    }

    /// Attach a holding (settled, captured into, back from dormancy). The
    /// index must already be accrued to the current bell.
    pub fn attach(&mut self, weight: u64, emits: bool) -> Result<Stake, EconError> {
        let mut next = *self;
        next.total_weight = next
            .total_weight
            .checked_add(weight)
            .ok_or(EconError::Overflow)?;
        if emits {
            next.emitters = next.emitters.checked_add(1).ok_or(EconError::Overflow)?;
        }
        *self = next;
        Ok(Stake {
            weight,
            emits,
            last_acc: self.acc,
        })
    }

    /// Detach a holding, returning its final credit (dormant, captured
    /// away, released). The index must already be accrued.
    pub fn detach(&mut self, stake: &Stake) -> Result<u64, EconError> {
        let credit = self.pending(stake)?;
        let mut next = *self;
        next.total_weight = next
            .total_weight
            .checked_sub(stake.weight)
            .ok_or(EconError::Underflow)?;
        if stake.emits {
            next.emitters = next.emitters.checked_sub(1).ok_or(EconError::Underflow)?;
        }
        *self = next;
        Ok(credit)
    }

    /// Change a holding's weight or emitting flag (muster, clash, tier-up,
    /// occupation). Credits what it earned under the old weight first.
    pub fn reweigh(
        &mut self,
        stake: &mut Stake,
        weight: u64,
        emits: bool,
    ) -> Result<u64, EconError> {
        let mut next = *self;
        let credit = next.detach(stake)?;
        let fresh = next.attach(weight, emits)?;
        *self = next;
        *stake = fresh;
        Ok(credit)
    }

    /// Laurel units the holding has earned since it was last credited:
    /// `weight × Δacc / INDEX_SCALE`, rounded down.
    pub fn pending(&self, stake: &Stake) -> Result<u64, EconError> {
        let d = self
            .acc
            .checked_sub(stake.last_acc)
            .ok_or(EconError::Underflow)?;
        let x = (stake.weight as u128)
            .checked_mul(d)
            .ok_or(EconError::Overflow)?
            / INDEX_SCALE;
        u64::try_from(x).map_err(|_| EconError::Overflow)
    }

    /// Any touch of a holding (`Bank`, `BankAfterEnd`): credit and advance.
    pub fn settle(&self, stake: &mut Stake) -> Result<u64, EconError> {
        let credit = self.pending(stake)?;
        stake.last_acc = self.acc;
        Ok(credit)
    }
}

/// Laurels a Relic Site pays its holder for `bells` held.
pub fn relic_credit(bells: u64) -> Option<u64> {
    bells.checked_mul(RELIC_EMISSION_PER_BELL)
}

fn part(x: u64, bps: Bps) -> u64 {
    (x as u128 * bps as u128 / BPS_ONE as u128) as u64
}

/// A credit split in two without loss.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Parts {
    /// The holder (owner) keeps this.
    pub keep: u64,
    /// This goes to the other party (occupier, Mandate reserve).
    pub give: u64,
}

/// An occupied first holding's credit: 50% to the occupier (§6.3).
pub fn occupation_split(credit: u64) -> Parts {
    let give = part(credit, OCCUPATION_BPS);
    Parts {
        keep: credit - give,
        give,
    }
}

/// 10% of every credit goes to the faction's Mandate reserve (§4.2).
pub fn mandate_reserve_split(credit: u64) -> Parts {
    let give = part(credit, MANDATE_RESERVE_BPS);
    Parts {
        keep: credit - give,
        give,
    }
}

/// What the captor and the victim have done together before (§5.4 item 3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PairHistory {
    /// Earlier captures between these two wallets, in either direction.
    pub prior_captures: u32,
    /// Any non-capture tie: a caravan, a delegation, a shared Company.
    pub other_ties: bool,
}

/// The laurels a capture moves and whether a refugee kit is minted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaptureTransfer {
    pub laurels: u64,
    pub refugee_kit: bool,
}

/// Captures of holdings 2–3 and Free Cities (§5.4 item 3): the first
/// capture between a pair moves 25% of the victim's banked laurels, the
/// second 25% of that, then nothing; any non-capture tie moves nothing. A
/// refugee kit only when the pair has no history at all.
pub fn capture_transfer(victim_banked: u64, h: PairHistory) -> CaptureTransfer {
    let first = part(victim_banked, CAPTURE_BPS);
    let laurels = match (h.other_ties, h.prior_captures) {
        (true, _) => 0,
        (false, 0) => first,
        (false, 1) => part(first, CAPTURE_BPS),
        _ => 0,
    };
    CaptureTransfer {
        laurels,
        refugee_kit: !h.other_ties && h.prior_captures == 0,
    }
}

/// Move `amount` banked laurels from one Citizen to another, zero-sum.
pub fn transfer(from: &mut u64, to: &mut u64, amount: u64) -> Result<(), EconError> {
    let f = from.checked_sub(amount).ok_or(EconError::Underflow)?;
    let t = to.checked_add(amount).ok_or(EconError::Overflow)?;
    *from = f;
    *to = t;
    Ok(())
}

/// Where a siege's escrowed laurels go (§5.4 item 4): back to the attacker
/// on success, to the defender on failure.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SiegeSettlement {
    pub to_attacker: u64,
    pub to_defender: u64,
}

pub fn siege_settle(succeeded: bool) -> SiegeSettlement {
    if succeeded {
        SiegeSettlement {
            to_attacker: SIEGE_STAKE,
            to_defender: 0,
        }
    } else {
        SiegeSettlement {
            to_attacker: 0,
            to_defender: SIEGE_STAKE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_span_the_design_range() {
        assert_eq!(strength_weight(Tier::Hamlet, 0, 0), WEIGHT_ONE);
        assert_eq!(strength_weight(Tier::Stronghold, 5_000_000, 0), 3_000_000);
        assert_eq!(strength_weight(Tier::Town, 500_000, 1), 812_500);
        assert_eq!(strength_weight(Tier::Hamlet, 0, 2), 250_000);
        assert_eq!(strength_weight(Tier::Hamlet, 0, 9), 250_000);
    }

    #[test]
    fn capture_pair_rules() {
        let none = PairHistory::default();
        assert_eq!(
            capture_transfer(1_000, none),
            CaptureTransfer {
                laurels: 250,
                refugee_kit: true
            }
        );
        let once = PairHistory {
            prior_captures: 1,
            other_ties: false,
        };
        assert_eq!(
            capture_transfer(1_000, once),
            CaptureTransfer {
                laurels: 62,
                refugee_kit: false
            }
        );
        let twice = PairHistory {
            prior_captures: 2,
            other_ties: false,
        };
        assert_eq!(capture_transfer(1_000, twice).laurels, 0);
        let tied = PairHistory {
            prior_captures: 0,
            other_ties: true,
        };
        assert_eq!(
            capture_transfer(1_000, tied),
            CaptureTransfer {
                laurels: 0,
                refugee_kit: false
            }
        );
    }

    #[test]
    fn index_freezes_at_end() {
        let mut ix = RewardIndex::new(10);
        let s = ix.attach(WEIGHT_ONE, true).unwrap();
        assert_eq!(
            ix.accrue(30, 20).unwrap(),
            10 * HOLDING_EMISSION_PER_BELL as u128
        );
        assert_eq!(ix.accrue(40, 20).unwrap(), 0u128);
        assert_eq!(ix.pending(&s).unwrap(), 10 * HOLDING_EMISSION_PER_BELL);
    }
}
