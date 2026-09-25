//! Merit (V5 §7.3): what each member did for the nation, per path.
//!
//! Merit is credited by the rules as events happen, to the officer who
//! issued the order behind them (and half to the proposer if the order came
//! from an adopted proposal). The acting official earns nothing. Only the
//! members named in a credit are touched, so the cost does not grow with the
//! number of members (V5 §11).

use crate::gov::{Credit, MemberId, Path, NOBODY};
use crate::state::{MeritEntry, WorldState};

/// Credit `milli` milli-merit on `path`: to the officer, or half each to the
/// officer and the proposer.
pub fn credit(state: &mut WorldState, c: Credit, path: Path, milli: u64, what: &'static [u8]) {
    if milli == 0 {
        return;
    }
    if c.proposer == NOBODY {
        add(state, c.officer, path, milli, what);
    } else {
        let half = milli / 2;
        add(state, c.proposer, path, half, what);
        add(state, c.officer, path, milli - half, what);
    }
}

fn add(state: &mut WorldState, m: MemberId, path: Path, milli: u64, what: &'static [u8]) {
    let Some(member) = state.members.get_mut(m as usize) else {
        return; // NOBODY: the acting official earns no merit
    };
    let amount = milli.min(u32::MAX as u64) as u32;
    let slot = &mut member.merit[path as usize];
    *slot = slot.saturating_add(amount);
    state.merit_log.push(MeritEntry {
        member: m,
        path,
        milli: amount,
        what,
    });
}

/// Split `milli` among `shares` in proportion to their weights (largest
/// remainder to the first), crediting each.
pub fn credit_shared(
    state: &mut WorldState,
    shares: &[(Credit, u64)],
    path: Path,
    milli: u64,
    what: &'static [u8],
) {
    let total: u128 = shares.iter().map(|(_, w)| *w as u128).sum();
    if total == 0 || milli == 0 {
        return;
    }
    let mut given = 0u64;
    let parts: alloc::vec::Vec<u64> = shares
        .iter()
        .map(|(_, w)| {
            let p = (milli as u128 * *w as u128 / total) as u64;
            given += p;
            p
        })
        .collect();
    let rest = milli - given;
    for (i, ((c, _), p)) in shares.iter().zip(parts).enumerate() {
        credit(state, *c, path, p + if i == 0 { rest } else { 0 }, what);
    }
}
