//! Settlement arithmetic shared by the program, the play server and the
//! replay verifier. Always compiled (no `program` feature): nothing here may
//! use the MagicBlock SDK, `solana_system_interface` or `crate::processor`.

use crate::state::{MemberAccount, Season};

/// What a member receives: its prize, plus its share of what is left in
/// its nation's treasury (V5 §7.5).
pub fn claim_amount(season: &Season, member: &MemberAccount) -> u64 {
    let prize = season
        .payouts
        .get(member.index as usize)
        .copied()
        .unwrap_or(0);
    let civ = member.civ as usize;
    let deposited = season.treasury.get(civ).copied().unwrap_or(0);
    let left = season.treasury_final.get(civ).copied().unwrap_or(0);
    let refund = if deposited == 0 {
        0
    } else {
        (member.shares as u128 * left as u128 / deposited as u128) as u64
    };
    prize + refund
}
