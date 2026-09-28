//! The SkipQuiet replay of V7 (contract §5.11 SkipQuiet, §8.5 V7
//! "SkipQuiet runs re-checked quiet"; W4-D notes §2, the integ-W4 review's
//! "SKIP quiet at every bell", deferred to W5): an **independent
//! transcription** of what the program's skip does to a Province bell by
//! bell (`permutation-frontier/src/proc/clash.rs`, W4-A's pinned rules and
//! v1.7):
//!
//! 1. the day's camp check lands first (`next_check_day = day + 1`; a spawn
//!    replaces the camp with `gen + 1`);
//! 2. the bell must be quiet — the verifier asks the **kernel's `is_quiet`
//!    at every bell** of the run (the program asks it once per transaction
//!    and relies on `trivially_quiet` ⇒ `is_quiet` afterwards; a bell where
//!    the kernel disagrees is `SkipNotQuiet`);
//! 3. from the first bell something is due (`next_due`), the bell's settle
//!    (`settle_bell`: forfeits freed, musters joining, `Leave` → departed,
//!    `Spend` → departed with its march stamina, splits and merges through
//!    the kernel, garrison changes);
//! 4. `finish_bell`: `resolved_next = b + 1`; a change bumps `roster_epoch`
//!    and recounts `n_entries`.
//!
//! The Province the transaction wrote must be the replay's (every byte but
//! the event chain's header fields), and SKIP's `quiet_digest` is taken
//! over it (v1.6 §22).
//!
//! **Independence (wave-5 review):** the settle and finish steps above are
//! this file's own transcription, and `is_quiet` is the kernel's; the camp
//! check (`camp_at`) and the terrain (`terrain_of`) go through
//! `fclient::clash_model`, which since integ-W5 `8871e2a` (W4-A D8) is the
//! program's own `frontier_abi::clash_model` — not an independent copy.
//!
//! Every function here refuses a Province shorter than the layout
//! (`Err`), so a truncated post-state in an archive fails closed instead of
//! panicking.

use frontier_abi::entry::{find_entry, read_entry, write_entry, Entry, EntryOp};
use frontier_abi::layout::province::{camp as CP, entry as E, province as PV, site as SM};
use permutation_rules::frontier::host::{settle_merge, GarrisonState, Stamina};

use crate::world::le;

/// Bells per game day.
pub const DAY_BELLS: u32 = 144;

fn rd32(d: &[u8], o: usize) -> Result<u32, String> {
    d.get(o..o + 4)
        .map(|b| le(b) as u32)
        .ok_or(format!("province: short at {o}"))
}

fn rd64(d: &[u8], o: usize) -> Result<i64, String> {
    d.get(o..o + 8)
        .map(|b| le(b) as i64)
        .ok_or(format!("province: short at {o}"))
}

fn wr(d: &mut [u8], o: usize, b: &[u8]) -> Result<(), String> {
    d.get_mut(o..o + b.len())
        .ok_or(format!("province: short at {o}"))?
        .copy_from_slice(b);
    Ok(())
}

/// The day's camp check of bell `b` written into the Province (the
/// program's `camp_check` + `Camp::write`); `Some(spawned)` when it ran.
pub fn camp_check(pd: &mut [u8], b: u32) -> Result<Option<bool>, String> {
    if pd.len() < PV::SIZE {
        return Err(format!(
            "province: {} B, the layout has {}",
            pd.len(),
            PV::SIZE
        ));
    }
    let pv = fclient::decode::Province::decode(pd).map_err(|e| format!("province: {e:?}"))?;
    let terrain = fclient::clash_model::terrain_of(&pv)?;
    let c = fclient::clash_model::camp_at(&pv, pd, &terrain, b)?;
    if !c.checked {
        return Ok(None);
    }
    let o = PV::CAMP;
    let day = b / DAY_BELLS;
    wr(pd, o + CP::NEXT_CHECK_DAY, &(day + 1).to_le_bytes())?;
    if c.spawned {
        wr(pd, o + CP::TILE, &[c.tile])?;
        wr(pd, o + CP::STATE, &[CP::STATE_PRESENT])?;
        wr(pd, o + CP::TROOPS, &c.troops.to_le_bytes())?;
        wr(pd, o + CP::GEN, &c.gen.to_le_bytes())?;
    }
    Ok(Some(c.spawned))
}

/// The first bell whose settle changes something (`u32::MAX`: none).
pub fn next_due(pd: &[u8]) -> Result<u32, String> {
    if pd.len() < PV::SIZE {
        return Err(format!(
            "province: {} B, the layout has {}",
            pd.len(),
            PV::SIZE
        ));
    }
    let mut due = u32::MAX;
    for i in 0..PV::ENTRIES_N {
        let o = PV::entry(i);
        let st = *pd.get(o + E::STATE).ok_or("province: short entry")?;
        if st != E::STATE_ROSTER && st != E::STATE_MUSTER_PENDING {
            continue;
        }
        if st == E::STATE_MUSTER_PENDING {
            due = due.min(rd32(pd, o + E::FROM_BELL)?.saturating_sub(1));
        }
        if pd[o + E::PEND_OP] != E::OP_NONE {
            due = due.min(rd32(pd, o + E::PEND_BELL)?);
        }
    }
    let n = (*pd.get(PV::SITE_COUNT).ok_or("province: short")? as usize).min(PV::SITES_N);
    for s in 0..n {
        let o = PV::site(s);
        if pd[o + SM::STATE] != SM::STATE_HOLDING {
            continue;
        }
        for (pb, dl) in [
            (SM::PEND0_BELL, SM::PEND0_DELTA),
            (SM::PEND1_BELL, SM::PEND1_DELTA),
        ] {
            if rd64(pd, o + dl)? != 0 {
                due = due.min(rd32(pd, o + pb)?);
            }
        }
    }
    Ok(due)
}

fn garrison_of(pd: &[u8], site: usize) -> Result<GarrisonState, String> {
    let o = PV::site(site);
    let slot = |b: usize, dl: usize| -> Result<Option<(u32, i64)>, String> {
        let (bell, delta) = (rd32(pd, o + b)?, rd64(pd, o + dl)?);
        Ok((bell != SM::NO_BELL && delta != 0).then_some((bell, delta)))
    };
    Ok(GarrisonState {
        troops: rd32(pd, o + SM::GARRISON)?,
        pending: [
            slot(SM::PEND0_BELL, SM::PEND0_DELTA)?,
            slot(SM::PEND1_BELL, SM::PEND1_DELTA)?,
        ],
    })
}

fn put_garrison(pd: &mut [u8], site: usize, g: &GarrisonState) -> Result<(), String> {
    let o = PV::site(site);
    wr(pd, o + SM::GARRISON, &g.troops.to_le_bytes())?;
    for (k, (b, dl)) in [
        (SM::PEND0_BELL, SM::PEND0_DELTA),
        (SM::PEND1_BELL, SM::PEND1_DELTA),
    ]
    .into_iter()
    .enumerate()
    {
        let (bell, delta) = g.pending[k].unwrap_or((SM::NO_BELL, 0));
        wr(pd, o + b, &bell.to_le_bytes())?;
        wr(pd, o + dl, &delta.to_le_bytes())?;
    }
    Ok(())
}

/// A split's or a merge's settle through the kernel (no M1 instruction
/// issues one; the entry codec keeps their encoding).
fn settle_kernel(pd: &mut [u8], i: usize, rn: u32) -> Result<(), String> {
    let mut e = read_entry(pd, i).map_err(|x| format!("entry {i}: {x:?}"))?;
    match e.op {
        EntryOp::Split { .. } => {
            let mut h = e.to_host().map_err(|x| format!("entry {i}: {x:?}"))?;
            let part = h.settle(rn).map_err(|x| format!("split settle: {x:?}"))?;
            let slot = (0..PV::ENTRIES_N).find(|&j| pd[PV::entry(j) + E::STATE] == E::STATE_FREE);
            match (part, slot) {
                (Some(nh), Some(j)) => {
                    let ne = Entry::from_host(&nh, e.tile, E::STATE_ROSTER, e.dealt_bps, rn);
                    write_entry(pd, j, &ne).map_err(|x| format!("entry {j}: {x:?}"))?;
                }
                (Some(nh), None) => h.troops = h.troops.saturating_add(nh.troops),
                _ => {}
            }
            e.set_host(&h);
        }
        EntryOp::Absorb { from } => {
            let j = find_entry(pd, from).ok_or("merge: no partner")?;
            let other = read_entry(pd, j).map_err(|x| format!("entry {j}: {x:?}"))?;
            let mut hi = e.to_host().map_err(|x| format!("entry {i}: {x:?}"))?;
            let mut hf = other.to_host().map_err(|x| format!("entry {j}: {x:?}"))?;
            settle_merge(&mut hi, &mut hf, rn).map_err(|x| format!("merge settle: {x:?}"))?;
            e.set_host(&hi);
            write_entry(pd, j, &Entry::FREE).map_err(|x| format!("entry {j}: {x:?}"))?;
        }
        _ => return Ok(()),
    }
    write_entry(pd, i, &e).map_err(|x| format!("entry {i}: {x:?}"))
}

/// Settles the pending changes of bell `b`; `true` when anything changed.
pub fn settle_bell(pd: &mut [u8], b: u32) -> Result<bool, String> {
    if pd.len() < PV::SIZE {
        return Err(format!(
            "province: {} B, the layout has {}",
            pd.len(),
            PV::SIZE
        ));
    }
    let rn = b.checked_add(1).ok_or("bell overflow")?;
    let mut changed = false;
    for i in 0..PV::ENTRIES_N {
        let o = PV::entry(i);
        let st = pd[o + E::STATE];
        if st == E::STATE_FREE || st == E::STATE_DEPARTED {
            continue;
        }
        let op = pd[o + E::PEND_OP];
        let due = op != E::OP_NONE && rd32(pd, o + E::PEND_BELL)? <= b;
        if due && op == E::OP_FORFEIT {
            pd[o..o + E::SIZE].fill(0);
            changed = true;
            continue;
        }
        if st == E::STATE_MUSTER_PENDING {
            if rd32(pd, o + E::FROM_BELL)? <= rn {
                pd[o + E::STATE] = E::STATE_ROSTER;
                changed = true;
            }
            continue;
        }
        if !due {
            continue;
        }
        match op {
            E::OP_LEAVE => pd[o + E::STATE] = E::STATE_DEPARTED,
            E::OP_SPEND => {
                let pb = rd32(pd, o + E::PEND_BELL)?;
                let eff = pb.saturating_add(1);
                let sb = rd32(pd, o + E::STAMINA_BELL)?;
                if sb > eff {
                    return Err(format!("entry {i}: stamina clock {sb} after {eff}"));
                }
                let cost = le(&pd[o + E::OP_B..o + E::OP_B + 2]) as u16;
                let v = Stamina {
                    value: le(&pd[o + E::STAMINA_VALUE..o + E::STAMINA_VALUE + 2]) as u16,
                    bell: sb,
                }
                .at(eff)
                .saturating_sub(cost);
                wr(pd, o + E::STAMINA_VALUE, &v.to_le_bytes())?;
                wr(pd, o + E::STAMINA_BELL, &eff.to_le_bytes())?;
                pd[o + E::PEND_BELL..o + E::SIZE].fill(0);
                pd[o + E::STATE] = E::STATE_DEPARTED;
            }
            E::OP_ABSORBED_INTO => continue,
            _ => settle_kernel(pd, i, rn)?,
        }
        changed = true;
    }
    let n = (pd[PV::SITE_COUNT] as usize).min(PV::SITES_N);
    for s in 0..n {
        let o = PV::site(s);
        if pd[o + SM::STATE] != SM::STATE_HOLDING {
            continue;
        }
        let g0 = garrison_of(pd, s)?;
        if g0.pending.iter().flatten().any(|(pb, _)| *pb <= b) {
            let mut g = g0;
            g.settle(rn);
            put_garrison(pd, s, &g)?;
            changed = true;
        }
    }
    Ok(changed)
}

/// Closes bell `b`: `resolved_next = b + 1`; a change bumps
/// `roster_epoch` and recounts `n_entries`.
pub fn finish_bell(pd: &mut [u8], b: u32, changed: bool) -> Result<(), String> {
    if pd.len() < PV::SIZE {
        return Err(format!(
            "province: {} B, the layout has {}",
            pd.len(),
            PV::SIZE
        ));
    }
    wr(pd, PV::RESOLVED_NEXT, &(b + 1).to_le_bytes())?;
    if changed {
        let e = rd32(pd, PV::ROSTER_EPOCH)?.wrapping_add(1);
        wr(pd, PV::ROSTER_EPOCH, &e.to_le_bytes())?;
        let n = (0..PV::ENTRIES_N)
            .filter(|&i| pd[PV::entry(i) + E::STATE] != E::STATE_FREE)
            .count();
        wr(pd, PV::N_ENTRIES, &[n as u8])?;
    }
    Ok(())
}

/// A bell of a replayed skip run the kernel does not call quiet (or cannot
/// build): `(bell, why)`.
pub type NotQuiet = (u32, String);

/// Replays a skip of `n` bells from `b0` over the Province bytes `before`:
/// the Province it leaves, or the first bell that is not quiet.
pub fn replay(before: &[u8], b0: u32, n: u32) -> Result<Vec<u8>, NotQuiet> {
    let mut pd = before.to_vec();
    let mut due = next_due(&pd).map_err(|e| (b0, e))?;
    for b in b0..b0 + n {
        let spawned = camp_check(&mut pd, b).map_err(|e| (b, e))?;
        match fclient::clash_model::build(&pd, &[], b, &[0; 32]).and_then(|x| x.quiet()) {
            Ok(true) => {}
            Ok(false) => return Err((b, "the kernel's is_quiet is false".into())),
            Err(e) => return Err((b, format!("the province does not build: {e}"))),
        }
        let mut changed = spawned == Some(true);
        if b >= due {
            changed |= settle_bell(&mut pd, b).map_err(|e| (b, e))?;
            due = next_due(&pd).map_err(|e| (b, e))?;
        }
        finish_bell(&mut pd, b, changed).map_err(|e| (b, e))?;
    }
    Ok(pd)
}
