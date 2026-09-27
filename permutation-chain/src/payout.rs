//! Settlement arithmetic shared by the program, the play server and the
//! replay verifier. Always compiled (no `program` feature): nothing here may
//! use the MagicBlock SDK, `solana_system_interface` or `crate::processor`.

use crate::state::{MemberAccount, Season, SeasonStatus};

/// What a member receives. After an abort: its refund
/// (`lifecycle::refund_amount`). Otherwise its prize, plus its share of what
/// is left in its nation's treasury (V5 §7.5): pro rata over `refund_base`
/// (the deposits less the revealed AI members' shares), or over the
/// deposits for a season finalized before `refund_base` existed. A revealed
/// AI member's refund is already inside its prize (`refund_in_payout`).
pub fn claim_amount(season: &Season, member: &MemberAccount) -> u64 {
    if season.status == SeasonStatus::Aborted {
        return crate::lifecycle::refund_amount(season, member);
    }
    let i = member.index as usize;
    let prize = season.payouts.get(i).copied().unwrap_or(0);
    if season
        .refund_in_payout
        .get(i / 8)
        .is_some_and(|b| b & (1 << (i % 8)) != 0)
    {
        return prize;
    }
    let civ = member.civ as usize;
    let base = if season.refund_base.is_empty() {
        season.treasury.get(civ).copied().unwrap_or(0)
    } else {
        season.refund_base.get(civ).copied().unwrap_or(0)
    };
    let left = season.treasury_final.get(civ).copied().unwrap_or(0);
    let refund = if base == 0 {
        0
    } else {
        u64::try_from(member.shares as u128 * left as u128 / base as u128).unwrap_or(u64::MAX)
    };
    prize.saturating_add(refund)
}
