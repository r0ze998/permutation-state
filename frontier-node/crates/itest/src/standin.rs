//! **Stub mode only** (`ITEST_STUBS=1`, the first run of §11 W4-F): a
//! stand-in for the resolution that W4-A (GatherClash, ResolveFromInputs,
//! SkipQuiet in the program) and W4-C (the keeper's gather/resolve/skip
//! duties) deliver in this wave. Without it no Province's `resolved_next`
//! ever moves and every resident action (Muster, Dissolve, Depart, …) is
//! `NotResident`, so the first run could not reach marches at all.
//!
//! It writes the Province account directly (`set_account`, the same rule as
//! the svm harness's `resolve_through`): once bell `b`'s window has closed
//! (`now ≥ bell_end(b) + W + seed_margin`), `resolved_next = b + 1`; every
//! pending change of a bell ≤ b is settled with the kernel (a Spend makes
//! the entry departed, a Leave or Forfeit frees it); muster-pending entries
//! with `from_bell ≤ b + 1` join the roster. Arrivals are ignored (no clash
//! is fought) and nothing is logged, so the herald sees the new bytes only
//! when a later transaction writes the Province. **It is not the program**:
//! the strict run (the gate) never uses it, and fails while any of those
//! instructions answers `NotImplemented`.

use fclient::clock::SeasonClock;
use fclient::Address;
use frontier_abi::entry::{read_entry, write_entry, Entry, EntryOp};
use frontier_abi::layout::province::{entry as E, province as P};
use localnet::InProcess;

/// The bell the stand-in resolves through at `now` (exclusive: the new
/// `resolved_next`).
pub fn target(sc: &SeasonClock, now: i64) -> u32 {
    let lag = sc.reveal_window as i64 + sc.seed_margin as i64;
    sc.bell_at(now - lag).unwrap_or(0)
}

/// Resolves the Province bytes `d` through `rn − 1`. Returns whether they
/// changed.
pub fn resolve_bytes(d: &mut [u8], rn: u32) -> bool {
    let cur = u32::from_le_bytes(
        d[P::RESOLVED_NEXT..P::RESOLVED_NEXT + 4]
            .try_into()
            .expect("4"),
    );
    if cur >= rn {
        return false;
    }
    for i in 0..P::ENTRIES_N {
        let Ok(mut e) = read_entry(d, i) else {
            continue;
        };
        if e.state == E::STATE_FREE {
            continue;
        }
        if e.state == E::STATE_MUSTER_PENDING && e.from_bell <= rn {
            e.state = E::STATE_ROSTER;
        }
        match e.op {
            EntryOp::Spend { .. } if e.pend_bell < rn => {
                if let Ok(mut h) = e.to_host() {
                    if h.settle(rn).is_ok() {
                        e.set_host(&h);
                    }
                }
                e.state = E::STATE_DEPARTED;
            }
            EntryOp::Leave | EntryOp::Forfeit if e.pend_bell < rn => e = Entry::FREE,
            _ => {}
        }
        let _ = write_entry(d, i, &e);
    }
    d[P::RESOLVED_NEXT..P::RESOLVED_NEXT + 4].copy_from_slice(&rn.to_le_bytes());
    true
}

/// One pass over every Province of `program`; returns the provinces moved.
pub fn pass(ip: &InProcess, program: &Address, sc: &SeasonClock, now: i64) -> usize {
    let rn = target(sc, now);
    let mut c = ip.lock();
    let provs: Vec<_> = c
        .program_accounts(program)
        .into_iter()
        .filter(|(_, a)| a.data.len() == P::SIZE && a.data[..8] == P::MAGIC)
        .collect();
    let mut n = 0;
    for (k, mut a) in provs {
        if resolve_bytes(&mut a.data, rn) && c.set_account(k, a).is_ok() {
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spend_departs_a_leave_frees_and_a_muster_joins() {
        let mut d = vec![0u8; P::SIZE];
        d[..8].copy_from_slice(&P::MAGIC);
        let base = Entry {
            id: fclient::addr::host_id(2, 0, 1, 1, 1).unwrap(),
            faction: 1,
            unit: 0,
            tile: 3,
            state: E::STATE_ROSTER,
            troops: 50_000,
            stamina_value: 1_000,
            dealt_bps: 0,
            stamina_bell: 0,
            ready_bell: 0,
            from_bell: 0,
            pend_bell: 4,
            op: EntryOp::Spend { cost: 100 },
        };
        write_entry(&mut d, 0, &base).unwrap();
        write_entry(
            &mut d,
            1,
            &Entry {
                op: EntryOp::Leave,
                id: base.id + 1,
                ..base
            },
        )
        .unwrap();
        write_entry(
            &mut d,
            2,
            &Entry {
                state: E::STATE_MUSTER_PENDING,
                from_bell: 6,
                op: EntryOp::None,
                pend_bell: 0,
                id: base.id + 2,
                ..base
            },
        )
        .unwrap();
        assert!(resolve_bytes(&mut d, 5));
        assert!(!resolve_bytes(&mut d, 5), "idempotent");
        let e0 = read_entry(&d, 0).unwrap();
        assert_eq!(e0.state, E::STATE_DEPARTED);
        assert_eq!(e0.op, EntryOp::None, "the spend was settled");
        assert_eq!(read_entry(&d, 1).unwrap().state, E::STATE_FREE);
        assert_eq!(
            read_entry(&d, 2).unwrap().state,
            E::STATE_MUSTER_PENDING,
            "from_bell 6 > 5"
        );
        assert!(resolve_bytes(&mut d, 6));
        assert_eq!(read_entry(&d, 2).unwrap().state, E::STATE_ROSTER);
    }
}
