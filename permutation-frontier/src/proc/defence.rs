//! The defence pool (M1 contract §5.12, I-21, I-22, I-52): ClaimDefence
//! (0x70, class D). Implemented by W4-B.
//!
//! `[keeper s,w (= beneficiary)] [season] [dpool w] [claim w] [system]
//! ([slot w] [anchor r]) × n` (1 ≤ n ≤ 6); data `day u32, n u8`.
//!
//! Checks in order: the season Running or Ended; `day == day(now_bell)`
//! (`BadData`: a keeper's claims count against the cap of the day they
//! land in); the DefencePool and the DefenceClaim `dc‖(keeper_tag8(keeper),
//! day)` canonical (the claim created when absent, the keeper paying its
//! rent, pre-funding-safe; a present one must name this keeper and day).
//! Per slot, **ArrivalSlot evidence only** (I-21):
//!
//! 1. a present ArrivalSlot at its canonical address (from its stored key;
//!    `BadAddress`);
//! 2. `beneficiary == keeper` and `claimed == 0` (`NotEligible`, 43);
//! 3. THE anchor `an‖(bell, region_of(P, Q))` present (`NoAnchor`) and
//!    `now < close + claim_grace` (6 bells, I-52; `WindowClosed`);
//! 4. lateness `ev_slot − anchor.slot ≥ lateness_slots` and
//!    `fees::defence_refund(evidence, season) > 0` (`NotEligible`).
//!
//! Effects: every slot's `claimed = 1` (a slot listed twice is refused by
//! step 2); the refunds summed with the caps: **per (bell, region) within
//! the claim** (`per_bell_region_cap`) and **per keeper-day** (the
//! DefenceClaim's `claimed` against `per_keeper_day_cap`); paid from the
//! pool's lamports above its rent (an empty pool pays partially: `partial
//! = 1`); `DefenceClaim.{claimed, count}`, `DefencePool.{paid_total,
//! claims}`; log `DEFENCE_CLAIM {beneficiary; day, slots, amount,
//! partial}`.
//!
//! **Open finding (W4-B F2, notes):** the per-(bell, region) cap is applied
//! within one claim only. Its global form needs a per-(bell, region)
//! counter no M1 account holds (the DefencePool and the DefenceClaim have
//! no such field); across claims and keepers the bound is the per-slot cap
//! (`defence_cap × cost`) times the slots of the bell-region.

use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use frontier_abi::addr::{day_of, defence_claim_seed, keeper_tag8};
use frontier_abi::ix as aix;
use frontier_abi::layout::AccountKind;
use frontier_abi::log::Kind;
use frontier_abi::tags::Ix;
use permutation_rules::frontier::fees::{self, DefenceParams, Evidence};
use permutation_rules::frontier::geometry::{region_of, ProvinceCoord};

use crate::addr;
use crate::clock::SeasonClock;
use crate::error::{BAD_ACCOUNT, OVERFLOW};
use crate::events::{self, Buf};
use crate::init::{self, SeasonSigner};
use crate::layout::{
    arrival_slot as AS, bell_anchor as BA, defence_claim as DCL, defence_pool as DP, init_header,
    season as S, Ro, Rw,
};
use crate::prologue::{self, check_accounts, expect_key, key};
use crate::{FrontierError, R, RULESET_HASH};

/// Seconds per bell.
const BELL_SECS: i64 = 600;

/// Refunds capped per `(bell, region)` inside one claim (≤ 6 groups).
#[derive(Clone, Copy, Debug, Default)]
pub struct Caps {
    groups: [(u32, u8, u64); DCL::MAX_SLOTS],
    n: usize,
}

impl Caps {
    /// Adds a refund for `(bell, region)`; returns the part the cap
    /// admits.
    pub fn admit(&mut self, bell: u32, region: u8, refund: u64, cap: u64) -> u64 {
        let i = match self.groups[..self.n]
            .iter()
            .position(|g| g.0 == bell && g.1 == region)
        {
            Some(i) => i,
            None => {
                if self.n == self.groups.len() {
                    return 0;
                }
                self.groups[self.n] = (bell, region, 0);
                self.n += 1;
                self.n - 1
            }
        };
        let room = cap.saturating_sub(self.groups[i].2);
        let take = refund.min(room);
        self.groups[i].2 += take;
        take
    }
}

/// `fees::defence_refund` in u64 arithmetic when no product can overflow
/// (always, for evidence a runtime can produce: `price ≤ 2⁶⁴ / 1.4M`), the
/// kernel's u128 form otherwise. The same integers: a u128 quotient of
/// operands that fit u64 equals the u64 quotient (host test
/// `refund_is_the_kernels`). Saves the two u128 divisions per slot on SBF.
pub fn refund(ev: &Evidence, s: &DefenceParams) -> u64 {
    refund_u64(ev, s).unwrap_or_else(|| fees::defence_refund(ev, s))
}

fn refund_u64(ev: &Evidence, s: &DefenceParams) -> Option<u64> {
    let c = fees::cost(ev.limit, 1, 2 + ev.created_day as u8, ev.loaded);
    let paid = ev
        .price_micro
        .checked_mul(ev.limit as u64)?
        .checked_add(999_999)?
        / 1_000_000;
    let capped = (s.defence_cap_milli as u64).checked_mul(c)? / 1_000;
    let capped = capped.saturating_sub(fees::PRIORITY_BASE_LAMPORTS);
    let spent = paid.min(capped);
    Some(spent.saturating_sub(s.tip_min.saturating_sub(fees::PRIORITY_BASE_LAMPORTS)))
}

/// 0x70 ClaimDefence (module doc).
pub fn claim_defence(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    let x = aix::ClaimDefence::decode(d)?;
    if x.n == 0 || x.n as usize > DCL::MAX_SLOTS {
        return Err(FrontierError::BadData.into());
    }
    check_accounts(Ix::ClaimDefence, a, Some(&[1, x.n]))?;
    let now = prologue::now()?;
    let (keeper, season_ai, dpool, claim, rest) = match a {
        [k, s, dp, c, _system, rest @ ..] => (k, s, dp, c, rest),
        _ => return Err(FrontierError::TooManyAccounts.into()),
    };
    let hdr = prologue::season(
        season_ai,
        p,
        Some(&RULESET_HASH),
        &[S::STATUS_RUNNING, S::STATUS_ENDED],
        now.ts,
    )?;
    let now_bell = hdr.bell(now.ts).ok_or(FrontierError::WrongStatus)?;
    if x.day != day_of(now_bell) {
        return Err(FrontierError::BadData.into());
    }
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    let keeper_key = key(keeper);
    expect_key(dpool, &ctx.defence_pool())?;
    prologue::present(dpool, p, AccountKind::DefencePool, hdr.id)?;
    let claim_seed = defence_claim_seed(&keeper_tag8(&keeper_key), x.day);
    expect_key(claim, &ctx.of(&claim_seed))?;
    let claim_present = prologue::presence(claim, p, AccountKind::DefenceClaim, hdr.id)?;
    let claimed_before = if claim_present {
        let cd = claim.try_borrow_data()?;
        let r = Ro(&cd);
        if r.arr::<32>(DCL::BENEFICIARY)? != keeper_key || r.u32(DCL::DAY)? != x.day {
            return Err(BAD_ACCOUNT);
        }
        r.u64(DCL::CLAIMED)?
    } else {
        0
    };
    let (clock, lateness, params) = {
        let sd = season_ai.try_borrow_data()?;
        let r = Ro(&sd);
        (
            SeasonClock::read(&sd)?,
            r.u8(S::LATENESS_SLOTS)?,
            DefenceParams {
                defence_cap_milli: r.u32(S::DEFENCE_CAP_MILLI)?,
                tip_min: fees::min_tip_lamports(
                    r.u32(S::MIN_REVEAL_PRIORITY_MILLI)?,
                    r.u32(S::REVEAL_CU_LIMIT)?,
                    r.u32(S::REVEAL_LOADED_LIMIT)?,
                ),
            },
        )
    };
    let (region_cap, day_cap) = {
        let pd = dpool.try_borrow_data()?;
        let r = Ro(&pd);
        (
            r.u64(DP::PER_BELL_REGION_CAP)?,
            r.u64(DP::PER_KEEPER_DAY_CAP)?,
        )
    };

    let mut caps = Caps::default();
    let mut total = 0u64;
    let mut last: Option<(u32, u8, [u8; 32], u64, i64)> = None;
    for i in 0..x.n as usize {
        let (slot, anchor) = (&rest[2 * i], &rest[2 * i + 1]);
        // 1. The slot, present and canonical; 2. this keeper's reveal, not
        // claimed.
        prologue::present(slot, p, AccountKind::ArrivalSlot, hdr.id)?;
        let (sp, sq, bell, faction, si, ben, claimed, ev_slot, ev) = {
            let sd = slot.try_borrow_data()?;
            let r = Ro(&sd);
            (
                r.i16(AS::P)?,
                r.i16(AS::Q)?,
                r.u32(AS::BELL)?,
                r.u8(AS::FACTION)?,
                r.u8(AS::I)?,
                r.arr::<32>(AS::BENEFICIARY)?,
                r.u8(AS::CLAIMED)?,
                r.u64(AS::EV_SLOT)?,
                AS::evidence(&sd).ok_or(BAD_ACCOUNT)?,
            )
        };
        expect_key(
            slot,
            &ctx.arrival_slot(sp as i32, sq as i32, bell, faction, si),
        )?;
        if ben != keeper_key || claimed != 0 {
            return Err(FrontierError::NotEligible.into());
        }
        // 3. THE anchor and the claim grace (the anchor of a bell-region
        // already read in this claim is reused: one address derivation per
        // bell-region).
        let region = region_of(ProvinceCoord::new(sp as i32, sq as i32));
        let (anchor_slot, grace_end) = match last {
            Some((b, r, ref k, s, g)) if b == bell && r == region && anchor.key.as_array() == k => {
                (s, g)
            }
            _ => {
                let anchor_key = ctx.bell_anchor(bell, region);
                expect_key(anchor, &anchor_key)?;
                if !prologue::presence(anchor, p, AccountKind::BellAnchor, hdr.id)? {
                    return Err(FrontierError::NoAnchor.into());
                }
                let (a, a_slot) = {
                    let ad = anchor.try_borrow_data()?;
                    let r = Ro(&ad);
                    if r.u32(BA::BELL)? != bell || r.u8(BA::REGION)? != region {
                        return Err(BAD_ACCOUNT);
                    }
                    (r.i64(BA::A)?, r.u64(BA::SLOT)?)
                };
                let g = clock
                    .reveal_close(bell, a)
                    .checked_add(DCL::CLAIM_GRACE_BELLS as i64 * BELL_SECS)
                    .ok_or(OVERFLOW)?;
                last = Some((bell, region, anchor_key, a_slot, g));
                (a_slot, g)
            }
        };
        if now.ts >= grace_end {
            return Err(FrontierError::WindowClosed.into());
        }
        // 4. Late and paid above the tip level.
        if ev_slot.saturating_sub(anchor_slot) < lateness as u64 {
            return Err(FrontierError::NotEligible.into());
        }
        let refund = refund(&ev, &params);
        if refund == 0 {
            return Err(FrontierError::NotEligible.into());
        }
        {
            let mut sd = slot.try_borrow_mut_data()?;
            Rw(&mut sd).set_u8(AS::CLAIMED, 1)?;
        }
        total = total
            .checked_add(caps.admit(bell, region, refund, region_cap))
            .ok_or(OVERFLOW)?;
    }
    let capped = total.min(day_cap.saturating_sub(claimed_before));
    let available = dpool.lamports().saturating_sub(init::rent(DP::SIZE)?);
    let paid = capped.min(available);
    let partial = paid < capped;

    if !claim_present {
        init::init_with_seed(
            keeper,
            claim,
            season_ai,
            &SeasonSigner::new(hdr.id, hdr.bump),
            &claim_seed,
            DCL::SIZE,
            init::rent(DCL::SIZE)?,
            p,
        )?;
        let mut cd = claim.try_borrow_mut_data()?;
        init_header(&mut cd, AccountKind::DefenceClaim, hdr.id)?;
        let mut w = Rw(&mut cd);
        w.set_arr(DCL::BENEFICIARY, &keeper_key)?;
        w.set_u32(DCL::DAY, x.day)?;
    }
    {
        let mut cd = claim.try_borrow_mut_data()?;
        let mut w = Rw(&mut cd);
        w.add_u64(DCL::CLAIMED, paid)?;
        w.add_u32(DCL::COUNT, x.n as u32)?;
    }
    init::move_lamports(dpool, keeper, paid)?;
    {
        let mut pd = dpool.try_borrow_mut_data()?;
        let mut w = Rw(&mut pd);
        w.add_u64(DP::PAID_TOTAL, paid)?;
        w.add_u64(DP::CLAIMS, x.n as u64)?;
    }
    let payload = Buf::<14>::new()
        .u32(x.day)
        .u8(x.n)
        .u64(paid)
        .u8(partial as u8);
    events::emit(
        Kind::DEFENCE_CLAIM,
        now_bell,
        &keeper_key,
        payload.get()?,
        &mut [],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refund_is_the_kernels() {
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let edges = [
            0u64,
            1,
            999_999,
            1_000_000,
            u32::MAX as u64,
            u64::MAX / 2,
            u64::MAX,
        ];
        for k in 0..20_000 {
            let pick = |v: u64, i: usize| {
                if k % 5 == 0 {
                    edges[i % edges.len()]
                } else {
                    v
                }
            };
            let ev = Evidence {
                price_micro: pick(next() >> (next() % 64), k),
                limit: pick(next() >> 32, k + 1) as u32,
                loaded: pick(next() >> 32, k + 2) as u32,
                created_day: next() & 1 == 1,
            };
            let s = DefenceParams {
                defence_cap_milli: (next() % 5_000) as u32,
                tip_min: pick(next() >> (next() % 64), k + 3),
            };
            assert_eq!(
                refund(&ev, &s),
                fees::defence_refund(&ev, &s),
                "{ev:?} {s:?}"
            );
        }
    }

    #[test]
    fn caps_bind_per_bell_region() {
        let mut c = Caps::default();
        assert_eq!(c.admit(10, 3, 700, 1_000), 700);
        assert_eq!(c.admit(10, 3, 700, 1_000), 300, "the group's cap binds");
        assert_eq!(c.admit(10, 4, 700, 1_000), 700, "another region");
        assert_eq!(c.admit(11, 3, 700, 1_000), 700, "another bell");
        assert_eq!(c.admit(10, 3, 1, 1_000), 0);
    }
}
