//! Frontier money out: the closed-form claim (open-world design §5.4, owner
//! decision D11).
//!
//! ```text
//! claim_i = min( 5 × paid_i,
//!      C_k' · [0.9 · fee_i·u_i / Σ_k'(fee·u) + 0.1 · w̃_i / Σ_k' w̃]        (citizen pool)
//!    + C_civ · fee_i·[builder_i] / Σ'(fee·builder)                        (Civilisation Share)
//!    + L_k' · λ̃_i·[staker_i] / Λ_k'                                        (laurel pool)
//!    + steward_i )
//! ```
//!
//! * `w̃_i = min(Works_i, 105 Works per USDC of fee_i)` ([`works_weight_at`]):
//!   linear, so splitting play over more wallets earns nothing extra (the
//!   earlier `√Works` paid N wallets √N times one wallet's weight), and
//!   capped by the fee, so a wallet's Works weight never outgrows what it
//!   paid. 105 per USDC is 15 a day over the days a fee buys (4 USDC on
//!   day 0 = 28 days × 15); revision 2 had 140 (K3, O4 step 1).
//! * `steward_i` is bounded by the **officer-pay ceiling** (K3, O3):
//!   officer pay can lift a claim to at most 95% of what the wallet paid,
//!   so no wallet ends a season in profit because of an office, however
//!   many offices a farm wins. The cut is swept like the 5× cap's.
//! * `λ̃_i` ([`CitizenRecord::counted_laurels`]) counts only laurels banked
//!   **while the wallet was a staker**: a stake added late (`AddStake`,
//!   priced by its day) does not reach back to laurels banked before it.
//!   Laurels a wallet transfers away (captures, siege stakes) come out of
//!   its counted laurels, so laurels that are worthless to their holder
//!   (a fee-only alt's) cannot be moved to a staker.
//!
//! There is no loop over members anywhere. Every wallet's contribution is
//! a `Weights` value derived from its own Citizen record; the faction
//! totals are sums of those values, kept incrementally in the FactionShards
//! (`add` on every credit, `remove` for a revealed Shade). `settle` turns
//! the two pools and six faction totals into pots, O(6); `claim` turns one
//! record and the settlement into a payment, O(1).
//!
//! **Conservation.** Every term is `pot × weight / Σ weight` rounded down,
//! so for any set of wallets whose weights sum to the totals, Σ claims ≤ the
//! prize pools. What a cap cuts off is `swept` (to the next season's pools);
//! rounding remainders and unclaimed shares are `dust`. [`SeasonLedger`]
//! keeps `claimed + swept + dust == citizen + laurel` exactly and refuses
//! any claim that would break it, and (CL-08) keeps one [`FactionLedger`]
//! per faction, so a claim can never draw on another faction's pots even
//! while the season total is fine.

/// Version of this kernel, bound into `RULESET_HASH` (`super::KERNEL_VERSIONS`):
/// bump it whenever an honest outcome changes. v2: M1 CL-07/CL-08 per-faction SeasonLedger, CL-11 office ceiling.
pub const PAYOUT_VERSION: u16 = 2;

use super::index::{citizen_faction_weight, laurel_faction_weight, FACTIONS};
use super::pools::USDC;
use super::pools::{civ_share_bps, EconError, Pools, STEWARD_BPS};
use crate::fixed::{Bps, BPS_ONE};
use borsh::{BorshDeserialize, BorshSerialize};

/// 1.0 tenure unit, in bps.
pub const TENURE_ONE: u32 = 10_000;

/// Tenure units `u = 1 + 0.25 × min(1, active_days / (0.43 × days_available))`
/// in bps (10,000 … 12,500), where `days_available` counts from the join day
/// so a late joiner can reach the full bonus (§5.4).
pub fn tenure_units(active_days: u32, days_available: u32) -> u32 {
    if days_available == 0 {
        return TENURE_ONE;
    }
    // 0.25 × active / (0.43 × avail) in bps = 2,500 × 100 × active / (43 × avail)
    let bonus = 250_000u64 * active_days as u64 / (43 * days_available as u64);
    TENURE_ONE + bonus.min(2_500) as u32
}

/// Works weight per USDC of citizen fee before K3 (20 Works a day over the
/// 28 days a 4-USDC day-0 fee buys). Kept for comparison runs.
pub const WORKS_PER_USDC_REV2: u64 = 140;

/// Works weight per USDC of citizen fee in [`PayoutParams::REV3`] (the
/// Season 1 default). No kernel reads it: every weight takes the season's
/// own `works_per_usdc` (CL-12 removed `CitizenRecord::weights()` and
/// `works_weight()`, which always used this constant).
pub const WORKS_PER_USDC: u64 = PayoutParams::REV3.works_per_usdc;

/// A wallet's Works weight: its season Works (daily-capped at credit
/// time), linear, capped at `per_usdc` per USDC of fee paid. Linear so that
/// N wallets with W Works between them weigh what one wallet with W Works
/// weighs (the earlier `√Works` rewarded splitting by √N). The rate is a
/// season constant (`PayoutParams::works_per_usdc`), fixed before joins
/// open, because the FactionShards sum these weights incrementally.
pub fn works_weight_at(works: u64, fee: u64, per_usdc: u64) -> u64 {
    let cap = (fee as u128 * per_usdc as u128 / USDC as u128) as u64;
    works.min(cap)
}

/// The payout fields of one Citizen account.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CitizenRecord {
    pub faction: u8,
    /// Citizen fee paid (released from escrow).
    pub fee: u64,
    /// Laurel stake paid; 0 = not a staker.
    pub stake: u64,
    /// `tenure_units`, bps.
    pub tenure: u32,
    /// Works points banked this season (daily-capped at credit time).
    pub works: u64,
    /// Engine pledges passed the builder threshold.
    pub builder: bool,
    /// Banked laurels (all of them).
    pub laurels: u64,
    /// Laurels already banked when the stake was added by `AddStake` after
    /// Join (0 for a stake paid at Join): they never count in the laurel
    /// pool ([`CitizenRecord::counted_laurels`]).
    pub laurels_at_stake: u64,
    /// Officer pay rows, already capped per person (`pools::steward_rows`).
    pub steward: u64,
    /// A revealed Shade: removed from its faction's totals, claims nothing.
    pub voided: bool,
}

impl CitizenRecord {
    /// What the wallet paid, the base of the 5× cap.
    pub fn paid(&self) -> u64 {
        self.fee.saturating_add(self.stake)
    }

    pub fn staker(&self) -> bool {
        self.stake > 0
    }

    /// `λ̃_i`: laurels banked while the wallet was a staker, the only ones
    /// that count in the laurel pool and the only ones it can transfer.
    pub fn counted_laurels(&self) -> u64 {
        if self.staker() {
            self.laurels.saturating_sub(self.laurels_at_stake)
        } else {
            0
        }
    }

    /// `AddStake` after Join (until the last join day, priced by that day,
    /// `pools::EntrySchedule::laurel_stake`). The program banks every
    /// holding of the wallet first, so `laurels` is complete; none of them
    /// will count. Returns the record's weights before and after at the
    /// season's Works rate `works_per_usdc`, for the shard update
    /// (`add(new)` then `remove(old)`).
    pub fn add_stake(
        &mut self,
        stake: u64,
        works_per_usdc: u64,
    ) -> Result<(Weights, Weights), EconError> {
        if self.staker() || stake == 0 {
            return Err(EconError::Underflow);
        }
        let old = self.weights_at(works_per_usdc);
        self.paid().checked_add(stake).ok_or(EconError::Overflow)?;
        self.stake = stake;
        self.laurels_at_stake = self.laurels;
        Ok((old, self.weights_at(works_per_usdc)))
    }

    /// Move `amount` counted laurels to `to` (a capture's 25%, a failed
    /// siege's stake): refused if `self` has fewer counted laurels. The
    /// receiver's laurels count if it is a staker. Returns both records'
    /// weights before at the season's Works rate, for the shard updates.
    pub fn transfer_counted(
        &mut self,
        to: &mut CitizenRecord,
        amount: u64,
        works_per_usdc: u64,
    ) -> Result<(Weights, Weights), EconError> {
        if amount > self.counted_laurels() {
            return Err(EconError::Underflow);
        }
        let receiver = to.laurels.checked_add(amount).ok_or(EconError::Overflow)?;
        let old = (
            self.weights_at(works_per_usdc),
            to.weights_at(works_per_usdc),
        );
        self.laurels -= amount;
        to.laurels = receiver;
        Ok(old)
    }

    /// This wallet's contribution to its faction's totals at the season's
    /// Works rate (`PayoutParams::works_per_usdc`,
    /// `Settlement::works_per_usdc`). The only form: there is no default
    /// rate (CL-12).
    pub fn weights_at(&self, works_per_usdc: u64) -> Weights {
        Weights {
            fee: self.fee,
            fee_units: self.fee as u128 * self.tenure as u128,
            works: works_weight_at(self.works, self.fee, works_per_usdc),
            builder_fee: if self.builder { self.fee } else { 0 },
            stake: self.stake,
            laurels: self.counted_laurels(),
            steward: self.steward,
        }
    }
}

/// One wallet's (or one shard's) share of the payout denominators.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Weights {
    pub fee: u64,
    pub fee_units: u128,
    pub works: u64,
    pub builder_fee: u64,
    pub stake: u64,
    pub laurels: u64,
    pub steward: u64,
}

/// A faction's payout denominators (the sum of its 16 FactionShards).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FactionTotals {
    /// Σ fee (the faction's weight in the citizen split).
    pub fee: u128,
    /// Σ fee·u.
    pub fee_units: u128,
    /// Σ w̃ (linear, fee-capped Works weights).
    pub works: u128,
    /// Σ fee·[builder] (summed across factions for the Civilisation Share).
    pub builder_fee: u128,
    /// Σ stake of stakers.
    pub stake: u128,
    /// Λ: Σ laurels of stakers.
    pub laurels: u128,
    /// Σ steward rows.
    pub steward: u128,
}

impl FactionTotals {
    fn fields(&mut self) -> [&mut u128; 7] {
        [
            &mut self.fee,
            &mut self.fee_units,
            &mut self.works,
            &mut self.builder_fee,
            &mut self.stake,
            &mut self.laurels,
            &mut self.steward,
        ]
    }

    fn values(w: &Weights) -> [u128; 7] {
        [
            w.fee as u128,
            w.fee_units,
            w.works as u128,
            w.builder_fee as u128,
            w.stake as u128,
            w.laurels as u128,
            w.steward as u128,
        ]
    }

    /// Add a contribution (or the difference between a record's new and
    /// old weights, as `add(new)` then `remove(old)`).
    pub fn add(&mut self, w: &Weights) -> Result<(), EconError> {
        let mut n = *self;
        for (f, v) in n.fields().into_iter().zip(Self::values(w)) {
            *f = f.checked_add(v).ok_or(EconError::Overflow)?;
        }
        *self = n;
        Ok(())
    }

    /// Remove a contribution (a record's old weights; a revealed Shade).
    pub fn remove(&mut self, w: &Weights) -> Result<(), EconError> {
        let mut n = *self;
        for (f, v) in n.fields().into_iter().zip(Self::values(w)) {
            *f = f.checked_sub(v).ok_or(EconError::Underflow)?;
        }
        *self = n;
        Ok(())
    }

    /// Fold one shard's totals into the faction's (`FinalizeFaction`).
    pub fn merge(&mut self, other: &FactionTotals) -> Result<(), EconError> {
        let mut n = *self;
        let mut o = *other;
        for (f, v) in n.fields().into_iter().zip(o.fields()) {
            *f = f.checked_add(*v).ok_or(EconError::Overflow)?;
        }
        *self = n;
        Ok(())
    }
}

/// Payout shares (D11) and the K3 bounds (owner decisions O3, O4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PayoutParams {
    /// Part of a faction's citizen pool paid by fee × tenure (90%); the
    /// rest by the Works weight.
    pub fee_units_bps: Bps,
    /// Steward pot as a share of each faction's two pools (5%).
    pub steward_bps: Bps,
    /// Wallet cap as a multiple of what the wallet paid (5).
    pub cap_multiple: u64,
    /// **Officer-pay ceiling (O3)**, in bps of what the wallet paid. Officer
    /// pay may raise a wallet's claim to at most this share of what it
    /// paid, and never beyond: `steward_i ≤ max(0, ceiling × paid_i −
    /// rest_of_claim_i)`. At or below 10,000 no wallet can end the season in
    /// profit because of an office, however many offices a farm wins. What
    /// the ceiling cuts is swept to the next season's pools, like the 5×
    /// cap. `u32::MAX` disables the ceiling (revision 2).
    pub office_ceiling_bps: u32,
    /// Works weight per USDC of citizen fee (O4 step 1).
    pub works_per_usdc: u64,
}

impl PayoutParams {
    /// Revision 2 as specified: no officer ceiling, 140 Works per USDC.
    pub const REV2: PayoutParams = PayoutParams {
        fee_units_bps: 9_000,
        steward_bps: STEWARD_BPS,
        cap_multiple: 5,
        office_ceiling_bps: u32::MAX,
        works_per_usdc: WORKS_PER_USDC_REV2,
    };

    /// After K3 (owner decisions O3, O4): officer pay may lift a claim to
    /// at most 95% of what the wallet paid; 105 Works per USDC (15 a day
    /// over the 28 days a day-0 fee buys). The simulator measurements
    /// behind both values are in `frontier/m0b/sim/ECONOMY.md`.
    pub const REV3: PayoutParams = PayoutParams {
        fee_units_bps: 9_000,
        steward_bps: STEWARD_BPS,
        cap_multiple: 5,
        office_ceiling_bps: 9_500,
        works_per_usdc: 105,
    };

    /// The borsh encoding CreateSeason, the verifier and the JS SDK hash
    /// (one producer: `frontier-abi`'s `presets.json` takes it from here).
    pub fn to_borsh(&self) -> alloc::vec::Vec<u8> {
        borsh::to_vec(self).unwrap_or_default()
    }

    /// Refuse parameters no settlement may run with. The officer-pay
    /// ceiling is at most 10,000 bps (CL-11): above what the wallet paid
    /// an office would be profitable, which O3 rules out. `u32::MAX` (no
    /// ceiling) stays accepted here for `REV2` comparison runs only.
    pub fn validate(&self) -> Result<(), EconError> {
        let ok = self.fee_units_bps <= BPS_ONE
            && self.steward_bps <= 2_000
            && (1..=10).contains(&self.cap_multiple)
            && (self.office_ceiling_bps == u32::MAX || self.office_ceiling_bps <= BPS_ONE)
            && self.works_per_usdc <= 10_000;
        if ok {
            Ok(())
        } else {
            Err(EconError::BadParams)
        }
    }

    /// The strict form `CreateSeason` calls (CL-11): [`PayoutParams::validate`]
    /// and a real officer-pay ceiling (`u32::MAX`, "off", is refused).
    pub fn validate_for_season(&self) -> Result<(), EconError> {
        self.validate()?;
        if self.office_ceiling_bps == u32::MAX {
            return Err(EconError::BadParams);
        }
        Ok(())
    }
}

impl Default for PayoutParams {
    fn default() -> Self {
        Self::REV3
    }
}

/// One faction's pots and denominators after settlement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FactionPots {
    /// `C_k`: the faction's part of the citizen pool before the steward cut.
    pub citizen: u64,
    /// `L_k`: the faction's part of the laurel pool before the steward cut.
    pub laurel: u64,
    /// 5% of `C_k + L_k`.
    pub steward_pot: u64,
    /// Paid by fee × tenure.
    pub fee_units_pot: u64,
    /// Paid by the Works weight.
    pub works_pot: u64,
    /// `L_k'`: paid by laurels among stakers.
    pub laurel_pot: u64,
    /// No staker banked a laurel: the laurel pot is split by stake instead.
    pub laurel_by_stake: bool,
    pub totals: FactionTotals,
}

impl FactionPots {
    /// What the steward rows are paid in total (an upper bound on the sum
    /// of rounded-down per-wallet steward payments).
    pub fn steward_paid(&self) -> u64 {
        if self.totals.steward <= self.steward_pot as u128 {
            self.totals.steward as u64
        } else {
            self.steward_pot
        }
    }
}

/// The season's settlement (`FinalizeFaction` for all six, after the
/// Shades are voided). Everything a claim needs, and nothing per member.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Settlement {
    /// `citizen + laurel` pools at settlement.
    pub prize: u64,
    /// Civilisation Share pot `C_civ`.
    pub civ_pot: u64,
    /// Σ fee·[builder] over all factions.
    pub builder_fee: u128,
    pub factions: [FactionPots; FACTIONS],
    pub cap_multiple: u64,
    /// `PayoutParams::office_ceiling_bps` of the season.
    pub office_ceiling_bps: u32,
    /// `PayoutParams::works_per_usdc` of the season.
    pub works_per_usdc: u64,
    /// Σ of all pots (≤ `prize`); the difference is rounding dust.
    pub allocated: u64,
}

fn mul_div(a: u128, b: u128, c: u128) -> Result<u128, EconError> {
    if c == 0 {
        return Ok(0);
    }
    Ok(a.checked_mul(b).ok_or(EconError::Overflow)? / c)
}

fn bps_of(x: u64, bps: Bps) -> u64 {
    (x as u128 * bps as u128 / BPS_ONE as u128) as u64
}

/// Turn the pools, the six faction totals and the six faction indices
/// (`index::faction_index`) into pots. O(FACTIONS).
pub fn settle(
    pools: &Pools,
    totals: &[FactionTotals; FACTIONS],
    index: &[u64; FACTIONS],
    engine_stages: u8,
    p: &PayoutParams,
) -> Result<Settlement, EconError> {
    p.validate()?;
    let prize = pools.prize()?;
    let builder_fee = totals
        .iter()
        .try_fold(0u128, |a, t| a.checked_add(t.builder_fee))
        .ok_or(EconError::Overflow)?;
    let civ_pot = if builder_fee > 0 {
        bps_of(pools.citizen, civ_share_bps(engine_stages))
    } else {
        0
    };
    let citizen_rest = pools.citizen - civ_pot;

    let mut cw = [0u128; FACTIONS];
    let mut lw = [0u128; FACTIONS];
    for k in 0..FACTIONS {
        cw[k] = citizen_faction_weight(totals[k].fee, index[k]).ok_or(EconError::Overflow)?;
        lw[k] = laurel_faction_weight(totals[k].stake, index[k]).ok_or(EconError::Overflow)?;
    }
    let cw_all = cw
        .iter()
        .try_fold(0u128, |a, x| a.checked_add(*x))
        .ok_or(EconError::Overflow)?;
    let lw_all = lw
        .iter()
        .try_fold(0u128, |a, x| a.checked_add(*x))
        .ok_or(EconError::Overflow)?;

    let mut out = Settlement {
        prize,
        civ_pot,
        builder_fee,
        cap_multiple: p.cap_multiple,
        office_ceiling_bps: p.office_ceiling_bps,
        works_per_usdc: p.works_per_usdc,
        ..Default::default()
    };
    let mut allocated = civ_pot as u128;
    for k in 0..FACTIONS {
        let t = totals[k];
        let c_k = mul_div(citizen_rest as u128, cw[k], cw_all)? as u64;
        let l_k = mul_div(pools.laurel as u128, lw[k], lw_all)? as u64;
        let pot_c = bps_of(c_k, p.steward_bps);
        let pot_l = bps_of(l_k, p.steward_bps);
        let steward_pot = pot_c + pot_l;
        // Rows that fit are paid in full and the rest of the pot returns to
        // the citizen pool; rows that do not fit are paid pro rata (§4.3).
        let leftover = (steward_pot as u128).saturating_sub(t.steward) as u64;
        let citizen_after = c_k - pot_c + leftover;
        let (fee_units_pot, works_pot) = if t.works > 0 {
            let f = bps_of(citizen_after, p.fee_units_bps);
            (f, citizen_after - f)
        } else {
            (citizen_after, 0)
        };
        let pots = FactionPots {
            citizen: c_k,
            laurel: l_k,
            steward_pot,
            fee_units_pot: if t.fee_units > 0 { fee_units_pot } else { 0 },
            works_pot,
            laurel_pot: if t.stake > 0 { l_k - pot_l } else { 0 },
            laurel_by_stake: t.laurels == 0,
            totals: t,
        };
        allocated += pots.fee_units_pot as u128
            + pots.works_pot as u128
            + pots.laurel_pot as u128
            + pots.steward_paid() as u128;
        out.factions[k] = pots;
    }
    out.allocated = u64::try_from(allocated).map_err(|_| EconError::Overflow)?;
    debug_assert!(out.allocated <= prize);
    Ok(out)
}

/// One wallet's claim, by part.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Claim {
    pub citizen: u64,
    pub civ: u64,
    pub laurel: u64,
    /// Officer pay after the officer-pay ceiling.
    pub steward: u64,
    /// Officer pay the ceiling cut off (part of `swept`).
    pub office_cut: u64,
    /// Sum of the parts before the ceiling and the cap.
    pub gross: u64,
    /// What the wallet receives:
    /// `min(citizen + civ + laurel + steward, cap_multiple × paid)`.
    pub paid: u64,
    /// `gross − paid` (officer-ceiling cut and 5×-cap cut), swept to the
    /// next season's pools.
    pub swept: u64,
}

/// `part × w / total`, refusing a weight larger than its total (a record
/// that is not in the totals would otherwise be overpaid).
fn share(pot: u64, w: u128, total: u128) -> Result<u64, EconError> {
    if w > total {
        return Err(EconError::Underflow);
    }
    Ok(mul_div(pot as u128, w, total)? as u64)
}

/// The closed-form claim of one Citizen. O(1): reads only the record and
/// the settlement. Also the live "prize if the season ended now" when fed
/// a projected settlement.
pub fn claim(st: &Settlement, rec: &CitizenRecord) -> Result<Claim, EconError> {
    let f = st
        .factions
        .get(rec.faction as usize)
        .ok_or(EconError::BadFaction)?;
    if rec.voided {
        return Ok(Claim::default());
    }
    let w = rec.weights_at(st.works_per_usdc);
    let t = &f.totals;
    let citizen = share(f.fee_units_pot, w.fee_units, t.fee_units)?
        .checked_add(share(f.works_pot, w.works as u128, t.works)?)
        .ok_or(EconError::Overflow)?;
    let civ = share(st.civ_pot, w.builder_fee as u128, st.builder_fee)?;
    let laurel = if w.stake == 0 {
        0
    } else if f.laurel_by_stake {
        share(f.laurel_pot, w.stake as u128, t.stake)?
    } else {
        share(f.laurel_pot, w.laurels as u128, t.laurels)?
    };
    let steward = if t.steward <= f.steward_pot as u128 {
        if w.steward as u128 > t.steward {
            return Err(EconError::Underflow);
        }
        w.steward
    } else {
        share(f.steward_pot, w.steward as u128, t.steward)?
    };
    let base = [civ, laurel]
        .iter()
        .try_fold(citizen, |a, x| a.checked_add(*x))
        .ok_or(EconError::Overflow)?;
    let gross = base.checked_add(steward).ok_or(EconError::Overflow)?;
    // O3: officer pay may lift the claim to `ceiling × paid`, never above.
    let kept = if st.office_ceiling_bps == u32::MAX {
        steward
    } else {
        let ceiling = (rec.paid() as u128 * st.office_ceiling_bps as u128 / BPS_ONE as u128)
            .min(u64::MAX as u128) as u64;
        steward.min(ceiling.saturating_sub(base))
    };
    let cap = rec.paid().saturating_mul(st.cap_multiple);
    let paid = (base + kept).min(cap);
    Ok(Claim {
        citizen,
        civ,
        laurel,
        steward: kept,
        office_cut: steward - kept,
        gross,
        paid,
        swept: gross - paid,
    })
}

/// One faction's claim counters (CL-08). The pots are what that faction's
/// claims may draw on, from its [`FactionPots`]:
///
/// * `pot_citizen` = fee×tenure pot + Works pot + steward rows paid (the
///   steward rows are cut from the faction's citizen and laurel parts, so
///   they are counted once, here);
/// * `pot_laurel` = the laurel pot.
///
/// A claim draws its citizen and steward parts (before the officer
/// ceiling and the cap) from `pot_citizen` and its laurel part from
/// `pot_laurel`; `claimed` and `swept` are the faction's share of what
/// was paid and cut (the Civilisation Share is season-wide, in
/// [`SeasonLedger`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FactionLedger {
    pub pot_citizen: u64,
    pub pot_laurel: u64,
    /// Paid to this faction's wallets from its own pots.
    pub claimed: u64,
    /// Cut from this faction's parts by the officer ceiling and the cap.
    pub swept: u64,
    /// Drawn on `pot_citizen` so far (`claimed + swept` split by pot).
    pub drawn_citizen: u64,
    /// Drawn on `pot_laurel` so far.
    pub drawn_laurel: u64,
}

impl FactionLedger {
    /// The faction's counters for a settlement's pots.
    pub fn new(p: &FactionPots) -> Result<Self, EconError> {
        let pot_citizen = p
            .fee_units_pot
            .checked_add(p.works_pot)
            .and_then(|x| x.checked_add(p.steward_paid()))
            .ok_or(EconError::Overflow)?;
        Ok(FactionLedger {
            pot_citizen,
            pot_laurel: p.laurel_pot,
            ..Default::default()
        })
    }

    /// What is left of the faction's pots.
    pub fn left(&self) -> u64 {
        (self.pot_citizen - self.drawn_citizen) + (self.pot_laurel - self.drawn_laurel)
    }
}

/// The season's claim counters: `claimed + swept + dust() == prize`
/// always, and per faction (CL-08) no faction's claims ever draw more than
/// its own pots, even while the season total is fine. `record` refuses
/// (and changes nothing) otherwise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SeasonLedger {
    pub prize: u64,
    pub claimed: u64,
    pub swept: u64,
    /// The Civilisation Share pot and what claims have drawn on it.
    pub civ_pot: u64,
    pub civ_drawn: u64,
    pub factions: [FactionLedger; FACTIONS],
}

impl SeasonLedger {
    /// Counters for a settlement: the season's prize, the Civilisation
    /// Share and every faction's pots.
    pub fn new(st: &Settlement) -> Result<Self, EconError> {
        let mut factions = [FactionLedger::default(); FACTIONS];
        for (f, p) in factions.iter_mut().zip(st.factions.iter()) {
            *f = FactionLedger::new(p)?;
        }
        Ok(SeasonLedger {
            prize: st.prize,
            claimed: 0,
            swept: 0,
            civ_pot: st.civ_pot,
            civ_drawn: 0,
            factions,
        })
    }

    /// Book one claim of a wallet of `faction`; refused (nothing changes)
    /// if it would take the season past its prize, the Civilisation Share
    /// past its pot, or the faction past either of its own pots.
    pub fn record(&mut self, faction: u8, c: &Claim) -> Result<(), EconError> {
        let mut n = *self;
        let f = n
            .factions
            .get_mut(faction as usize)
            .ok_or(EconError::BadFaction)?;
        // The parts as drawn from each pot (before the ceiling and the cap).
        let steward_gross = c
            .steward
            .checked_add(c.office_cut)
            .ok_or(EconError::Overflow)?;
        let citizen_side = c
            .citizen
            .checked_add(steward_gross)
            .ok_or(EconError::Overflow)?;
        let parts = citizen_side
            .checked_add(c.laurel)
            .and_then(|x| x.checked_add(c.civ))
            .ok_or(EconError::Overflow)?;
        let settled = c.paid.checked_add(c.swept).ok_or(EconError::Overflow)?;
        if parts != c.gross || settled != c.gross {
            // Not a claim `claim()` produced.
            return Err(EconError::Underflow);
        }
        f.drawn_citizen = f
            .drawn_citizen
            .checked_add(citizen_side)
            .ok_or(EconError::Overflow)?;
        f.drawn_laurel = f
            .drawn_laurel
            .checked_add(c.laurel)
            .ok_or(EconError::Overflow)?;
        if f.drawn_citizen > f.pot_citizen || f.drawn_laurel > f.pot_laurel {
            return Err(EconError::Underflow);
        }
        // The faction's share of what was paid and cut: the cap and the
        // ceiling cut the claim as a whole; the Civilisation Share part is
        // counted as paid first (it is season-wide).
        let civ_paid = c.civ.min(c.paid);
        let own_paid = c.paid - civ_paid;
        let own_swept = (c.gross - c.civ) - own_paid;
        f.claimed = f.claimed.checked_add(own_paid).ok_or(EconError::Overflow)?;
        f.swept = f.swept.checked_add(own_swept).ok_or(EconError::Overflow)?;
        n.civ_drawn = n.civ_drawn.checked_add(c.civ).ok_or(EconError::Overflow)?;
        if n.civ_drawn > n.civ_pot {
            return Err(EconError::Underflow);
        }
        n.claimed = n.claimed.checked_add(c.paid).ok_or(EconError::Overflow)?;
        n.swept = n.swept.checked_add(c.swept).ok_or(EconError::Overflow)?;
        if n.claimed.checked_add(n.swept).ok_or(EconError::Overflow)? > n.prize {
            return Err(EconError::Underflow);
        }
        *self = n;
        Ok(())
    }

    /// Rounding remainders and unclaimed shares (swept at close).
    pub fn dust(&self) -> u64 {
        self.prize - self.claimed - self.swept
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tenure_reaches_full_bonus_at_43_percent() {
        assert_eq!(tenure_units(0, 28), 10_000);
        assert_eq!(tenure_units(28, 28), 12_500);
        assert_eq!(tenure_units(13, 28), 12_500); // 13 ≥ 0.43 × 28 = 12.04
        assert_eq!(tenure_units(6, 28), 10_000 + 250_000 * 6 / (43 * 28));
        // A day-21 joiner reaches the full bonus after 4 active days of 7.
        assert_eq!(tenure_units(4, 7), 12_500);
    }
}
