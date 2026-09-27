//! The native model's play instructions (W4-C): what the keeper's reveal,
//! departure, gather, resolve, skip, settlement, close, claim and return
//! duties talk to while the program's Depart/Reveal/SettleDeparture (W3-B,
//! merged), clash (W4-A) and transit (W4-B) are built in parallel. Account
//! lists, checks and error codes follow the contract (§5.10, §5.11,
//! §5.12, §21) on the `frontier-abi` offsets; it is **not** the program:
//!
//! | tag | modelled | left out |
//! |---|---|---|
//! | 0x44 Dissolve | host of the caller in the roster, no pending op → pending `Leave` at `now_bell` | bucket, `HostInTransit` |
//! | 0x50 Depart | P prologue, final holding, host in the roster without a pending op and in no transit (`HostInTransit`), resident rule, seal byte 0, arrival range, `tip ≥ tip_min`, free slot, escrow `tip + fee + bond` into the Holding; entry pending `Spend`; `DEPART` with commit and seal | stamina and cooldown, the doctrine's `dealt_bps` |
//! | 0x51 Reveal | season, transit, `Plain::validate`, commitment, THE anchor's window and BeaconLog guard or the archive tombstone, latch (inputs absent, `resolved_next ≤ arrive`), quota by the kernel's `admit_arrival` (`QuotaRefused`, `AlreadyDone`, `SlotMoved`), ArrivalDay bit and init, evidence from the instructions sysvar, `REVEAL` | the path walk and the travel time (the path provinces are accepted as given), `Shielded` |
//! | 0x52 SettleDeparture | transit state 1, origin resolved past `depart_bell` (`TooEarly`), departed entry → `troops_after`, state 2 (3 when 0 troops), entry freed | stamina and ready bell |
//! | 0x60 GatherClash | tombstone rule (`LatchClosed`), window closed (`TooEarly`), inputs init, ArrivalDay fast path, positions with their Holdings (`DepartureUnsettled`) | evidence |
//! | 0x61 ResolveFromInputs | `OutOfOrder`, `NotGathered`, the seed of THE anchor's cache (or the archive), a **model clash** (below), pending ops of the bell, fates, `CLASH` | the kernel's `resolve_clash`, camps, garrisons |
//! | 0x63 SkipQuiet | `OutOfOrder`, per bell: window closed, ArrivalDay bit clear, pending ops applied, quiet = at most one faction among residents (`NotQuiet` first / stop later), stop below 40k CU, `SKIP` | the kernel's `is_quiet` |
//! | 0x54 SettleTransit | state, time rule, logged pair against the root, the proof with `seal::judge` on THE anchor's or the archive's signature, every D5 branch, payments from the escrow (`pool_owed` for routed tips and fees), slot `settled` flag, `TRANSIT_SETTLED` | returns as muster-pending entries (troops go to `reserve`) |
//! | 0x64–0x66 closes | the contract's conditions | — |
//! | 0x70 ClaimDefence | beneficiary, claimed, claim grace, lateness, `fees::defence_refund`, claim record, pool debit | per-bell-region and per-keeper-day caps |
//! | 0x52 with `transit_slot = 0xFF`: the return settle (§21, W4-A's shape) | every `Leave` entry (state 3) of the Holding in the Province freed, `reserve += troops / 1,000` (Holding live, same gen), `DEPARTURE_SETTLED` with `destroyed = 2`; nothing → `AlreadyDone` | — |
//!
//! **Model clash:** every faction's strength is its residents' troops plus
//! its present arrivals' troops. One faction present: every arrival stays
//! (room permitting). Several: the strongest faction (ties by the seed)
//! keeps its residents and its arrivals stay; other arrivals are
//! destroyed and other residents lose half their troops. Room: 56 entries,
//! 8 per faction; an arrival without room bounces. Deterministic in the
//! gathered inputs, the Province and the seed.

use sha2::{Digest, Sha256};
use solana_program_runtime::invoke_context::InvokeContext;

use fclient::abi::layout as l;
use fclient::addr::{self as fa, SeedKind};
use fclient::seal;
use frontier_abi::log::Kind;
use permutation_rules::frontier::clash::{SlotDecision, SlotEntry, SlotRefusal};
use permutation_rules::frontier::geometry::ProvinceCoord;

use super::land::{addr_of, body, i16_at, now_bell, own_holding, owned, player, u16_at};
use super::{
    clock, close, consume, e, effective, get, i64_at, init_with_seed, is_absent, is_signer, key,
    log_ps2, move_lamports, n_accounts, program_id, put, read, season_at, season_clock,
    transfer_ix, u32_at, u64_at, write, Cursor, SeasonRef, R,
};

const BAD_DATA: u32 = 1;
const BAD_ACCOUNT: u32 = 2;
const BAD_ADDRESS: u32 = 3;
const AUTH: u32 = 4;
const WRONG_STATUS: u32 = 5;
const NO_ANCHOR: u32 = 8;
const WINDOW_CLOSED: u32 = 12;
const TOO_EARLY: u32 = 13;
const ARCHIVED: u32 = 16;
const NOT_OWNER: u32 = 20;
const INSUFFICIENT: u32 = 21;
const NOT_FINAL: u32 = 24;
const NOT_RESIDENT: u32 = 26;
const HOST_BUSY: u32 = 28;
const TRANSIT_STATE: u32 = 30;
const ARRIVAL_BELL: u32 = 31;
const COMMIT_MISMATCH: u32 = 33;
const QUOTA_REFUSED: u32 = 34;
const SLOT_MOVED: u32 = 35;
const NEED_ARRIVAL_DAY: u32 = 36;
const DEPARTURE_UNSETTLED: u32 = 38;
const NOT_GATHERED: u32 = 39;
const OUT_OF_ORDER: u32 = 40;
const NOT_QUIET: u32 = 41;
const INPUTS_OPEN: u32 = 42;
const NOT_ELIGIBLE: u32 = 43;
const TIP_TOO_LOW: u32 = 51;
const ALREADY_DONE: u32 = 52;
const LATCH_CLOSED: u32 = 53;
const SEED_NOT_READY: u32 = 54;
const BAD_PLAINTEXT: u32 = 55;
const HOST_IN_TRANSIT: u32 = 58;

const RUNNING: u8 = 3;
const ENDED: u8 = 4;
const ENTRIES: usize = 56;
const PER_FACTION: usize = 8;
const BELL_SECS: i64 = 600;
const CLAIM_GRACE_BELLS: i64 = 6;

use fclient::abi::magic;
use fclient::abi::size;

// ------------------------------------------------------------------ helpers

fn entry_off(i: usize) -> usize {
    l::province::ENTRIES + i * l::province::ENTRY_STRIDE
}
fn transit_off(i: usize) -> usize {
    l::holding::TRANSIT + i * l::holding::TRANSIT_STRIDE
}
fn pq_of(d: &[u8]) -> (i16, i16) {
    (i16_at(d, 64), i16_at(d, 66))
}
fn find_entry(pv: &[u8], host: u64) -> Option<usize> {
    (0..ENTRIES).find(|&i| {
        let o = entry_off(i);
        pv[o + l::entry::STATE] != 0 && u64_at(pv, o + l::entry::ID) == host
    })
}
fn free_entry(pv: &[u8]) -> Option<usize> {
    (0..ENTRIES).find(|&i| pv[entry_off(i) + l::entry::STATE] == 0)
}
fn count_entries(pv: &[u8]) -> usize {
    (0..ENTRIES)
        .filter(|&i| pv[entry_off(i) + l::entry::STATE] != 0)
        .count()
}
fn faction_entries(pv: &[u8], f: u8) -> usize {
    (0..ENTRIES)
        .filter(|&i| {
            let o = entry_off(i);
            matches!(pv[o + l::entry::STATE], 1 | 2) && pv[o + l::entry::FACTION] == f
        })
        .count()
}
fn clear_entry(pv: &mut [u8], i: usize) {
    let o = entry_off(i);
    pv[o..o + l::province::ENTRY_STRIDE].fill(0);
    pv[l::province::N_ENTRIES] = pv[l::province::N_ENTRIES].saturating_sub(1);
}
fn tip_min(s: &[u8]) -> u64 {
    fclient::fees::min_tip_lamports(
        u32_at(s, l::season::MIN_REVEAL_PRIORITY_MILLI),
        u32_at(s, l::season::REVEAL_CU_LIMIT),
        u32_at(s, l::season::REVEAL_LOADED_LIMIT),
    )
}
fn province_raw(p: i16, q: i16) -> [u8; 8] {
    fa::raw_province(p as i32, q as i32)
}
fn read_province(ic: &InvokeContext, i: u16, season: &SeasonRef, p: i16, q: i16) -> R<Vec<u8>> {
    owned(
        ic,
        i,
        season,
        SeedKind::Province,
        &province_raw(p, q),
        magic::PROVINCE,
        size::PROVINCE,
    )
}
/// A present program account at `i` at the canonical address of `(kind,
/// raw)`, or `None` if it is absent there (`BadAddress` elsewhere).
fn maybe(
    ic: &InvokeContext,
    i: u16,
    season: &SeasonRef,
    kind: SeedKind,
    raw: &[u8],
    magic: &[u8; 8],
) -> R<Option<Vec<u8>>> {
    let prog = program_id(ic)?;
    if addr_of(season, &prog, kind, raw) != key(ic, i)? {
        return Err(e(BAD_ADDRESS));
    }
    let (o, d, _) = read(ic, i)?;
    if is_absent(&o, &d) {
        return Ok(None);
    }
    if o != prog || d.len() < 8 || get::<8>(&d, 0) != *magic {
        return Err(e(BAD_ACCOUNT));
    }
    Ok(Some(d))
}

/// THE anchor of `(bell, region)` at account `i` or its archive entry:
/// `(A, anchor slot, sig48, seed-from-archive)`; `None` if neither.
struct AnchorView {
    a: i64,
    slot: Option<u64>,
    sig48: [u8; 48],
    archived_seed: Option<[u8; 32]>,
}
fn anchor_view(
    ic: &InvokeContext,
    i: u16,
    season: &SeasonRef,
    s: &[u8],
    bell: u32,
    region: u8,
) -> R<Option<AnchorView>> {
    let prog = program_id(ic)?;
    let k = key(ic, i)?;
    let an = addr_of(
        season,
        &prog,
        SeedKind::BellAnchor,
        &fa::raw_anchor(bell, region),
    );
    let ar = addr_of(
        season,
        &prog,
        SeedKind::AnchorArchive,
        &fa::raw_archive(region, fa::archive_part(bell)),
    );
    let (o, d, _) = read(ic, i)?;
    if k == an {
        if is_absent(&o, &d) {
            return Ok(None);
        }
        if o != prog || get::<8>(&d, 0) != *magic::BELL_ANCHOR {
            return Err(e(BAD_ACCOUNT));
        }
        return Ok(Some(AnchorView {
            a: i64_at(&d, l::bell_anchor::A),
            slot: Some(u64_at(&d, l::bell_anchor::SLOT)),
            sig48: get(&d, l::bell_anchor::SIG48),
            archived_seed: None,
        }));
    }
    if k == ar {
        if is_absent(&o, &d) {
            return Ok(None);
        }
        if o != prog || get::<8>(&d, 0) != *magic::ANCHOR_ARCHIVE {
            return Err(e(BAD_ACCOUNT));
        }
        let j = (bell % 72) as usize;
        if d[l::anchor_archive::ARCHIVED + j / 8] & (1 << (j % 8)) == 0 {
            return Ok(None);
        }
        let eo = l::anchor_archive::ENTRIES + j * l::anchor_archive::ENTRY_STRIDE;
        let a_off = u32_at(&d, eo) as i64;
        let sc = season_clock(s);
        return Ok(Some(AnchorView {
            a: sc.genesis_ts + (bell as i64 + 1) * BELL_SECS + a_off,
            slot: None,
            sig48: get(&d, eo + 36),
            archived_seed: Some(get(&d, eo + 4)),
        }));
    }
    Err(e(BAD_ADDRESS))
}

/// Evidence from the instructions sysvar at account `i`: `(price µlamports
/// per CU, CU limit, loaded limit)` of the transaction's compute-budget
/// instructions (0 when absent).
fn evidence(ic: &InvokeContext, i: u16) -> (u64, u32, u32) {
    let Ok((_, d, _)) = read(ic, i) else {
        return (0, 0, 0);
    };
    let cb = fclient::addr::compute_budget_program().to_bytes();
    let rd16 = |o: usize| {
        d.get(o..o + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]) as usize)
    };
    let (mut price, mut limit, mut loaded) = (0u64, 0u32, 0u32);
    let Some(n) = rd16(0) else {
        return (0, 0, 0);
    };
    for k in 0..n {
        let Some(mut o) = rd16(2 + 2 * k) else { break };
        let Some(na) = rd16(o) else { break };
        o += 2 + na * 33;
        let Some(pid) = d.get(o..o + 32) else { break };
        o += 32;
        let Some(dl) = rd16(o) else { break };
        o += 2;
        let Some(data) = d.get(o..o + dl) else { break };
        if pid == cb {
            match data.first() {
                Some(2) if data.len() >= 5 => {
                    limit = u32::from_le_bytes(data[1..5].try_into().expect("4"))
                }
                Some(3) if data.len() >= 9 => {
                    price = u64::from_le_bytes(data[1..9].try_into().expect("8"))
                }
                Some(4) if data.len() >= 5 => {
                    loaded = u32::from_le_bytes(data[1..5].try_into().expect("4"))
                }
                _ => {}
            }
        }
    }
    (price, limit, loaded)
}

fn writable(ic: &InvokeContext, i: u16) -> R<bool> {
    let c = ic.transaction_context.get_current_instruction_context()?;
    c.is_instruction_account_writable(i)
}

fn keeper_season(ic: &InvokeContext, i: u16) -> R<(SeasonRef, Vec<u8>, i64, u64, u32)> {
    let (season, s) = season_at(ic, i)?;
    let (slot, now) = clock(ic)?;
    let bell = now_bell(&s, now);
    Ok((season, s, now, slot, bell))
}

/// Holding at account `i`: present at its canonical address (from its
/// stored P, Q, site) or `None` when the address is the canonical one of
/// `(p, q, site)` and absent.
fn holding_at(
    ic: &InvokeContext,
    i: u16,
    season: &SeasonRef,
    pqs: (i16, i16, u8),
) -> R<Option<Vec<u8>>> {
    maybe(
        ic,
        i,
        season,
        SeedKind::Holding,
        &fa::raw_holding(pqs.0 as i32, pqs.1 as i32, pqs.2),
        magic::HOLDING,
    )
}

fn host_holding(host: u64) -> Option<(i16, i16, u8, u8)> {
    let (p, q, site, gen, _) = fa::host_parts(host).ok()?;
    Some((p as i16, q as i16, site, gen))
}

// ------------------------------------------------------------------ the model clash

/// Applies pending ops that take effect at `bell` (Spend → departed,
/// Leave → kept departed for the return settle, Forfeit → freed) and
/// musters that join. Returns whether anything changed.
fn apply_pending(pv: &mut [u8], bell: u32) -> bool {
    let mut changed = false;
    for i in 0..ENTRIES {
        let o = entry_off(i);
        let st = pv[o + l::entry::STATE];
        if st == 0 {
            continue;
        }
        if st == l::entry::STATE_MUSTER_PENDING && u32_at(pv, o + l::entry::FROM_BELL) <= bell + 1 {
            pv[o + l::entry::STATE] = l::entry::STATE_ROSTER;
            changed = true;
        }
        let op = pv[o + l::entry::PEND_OP];
        if op == 0 || u32_at(pv, o + l::entry::PEND_BELL) != bell || st != 1 {
            continue;
        }
        changed = true;
        match op {
            1 => {
                pv[o + l::entry::STATE] = l::entry::STATE_DEPARTED;
                pv[o + l::entry::PEND_OP] = 0;
            }
            5 => pv[o + l::entry::STATE] = l::entry::STATE_DEPARTED,
            6 => clear_entry(pv, i),
            _ => pv[o + l::entry::PEND_OP] = 0,
        }
    }
    changed
}

fn residents(pv: &[u8], bell: u32) -> Vec<usize> {
    (0..ENTRIES)
        .filter(|&i| {
            let o = entry_off(i);
            pv[o + l::entry::STATE] == 1 && u32_at(pv, o + l::entry::FROM_BELL) <= bell
        })
        .collect()
}

fn quiet(pv: &[u8], bell: u32) -> bool {
    let mut fs: Vec<u8> = residents(pv, bell)
        .iter()
        .map(|&i| pv[entry_off(i) + l::entry::FACTION])
        .collect();
    fs.sort();
    fs.dedup();
    fs.len() <= 1
}

/// Resolves `bell` over the gathered inputs `ci` (fates written back) and
/// the Province `pv`; returns the outcome digest.
fn model_clash(pv: &mut [u8], ci: &mut [u8], bell: u32, seed: &[u8; 32]) -> [u8; 32] {
    let mut strength = [0u64; 6];
    let res = residents(pv, bell);
    for &i in &res {
        let o = entry_off(i);
        let f = pv[o + l::entry::FACTION] as usize;
        if f < 6 {
            strength[f] += u32_at(pv, o + l::entry::TROOPS) as u64;
        }
    }
    let arr = |k: usize| l::clash_inputs::ARRIVALS + k * l::clash_inputs::ARRIVAL_STRIDE;
    for k in 0..24 {
        let o = arr(k);
        if ci[o + l::arrival::PRESENT] == 1 {
            let f = ci[o + l::arrival::FACTION] as usize;
            if f < 6 {
                strength[f] += u32_at(ci, o + l::arrival::TROOPS) as u64;
            }
        }
    }
    let present: Vec<usize> = (0..6).filter(|&f| strength[f] > 0).collect();
    let tie = |f: usize| {
        let h = Sha256::new()
            .chain_update(seed)
            .chain_update([f as u8])
            .finalize();
        u64::from_le_bytes(h[..8].try_into().expect("8"))
    };
    let winner = present
        .iter()
        .copied()
        .max_by_key(|&f| (strength[f], tie(f)))
        .unwrap_or(0);
    let contested = present.len() > 1;
    if contested {
        for &i in &res {
            let o = entry_off(i);
            if pv[o + l::entry::FACTION] as usize != winner {
                let t = u32_at(pv, o + l::entry::TROOPS) / 2;
                put(pv, o + l::entry::TROOPS, t.to_le_bytes());
            }
        }
    }
    for k in 0..24 {
        let o = arr(k);
        if ci[o + l::arrival::PRESENT] != 1 {
            continue;
        }
        let f = ci[o + l::arrival::FACTION];
        let troops = u32_at(ci, o + l::arrival::TROOPS);
        let (fate, after) = if contested && f as usize != winner {
            (l_fate::DESTROYED, 0)
        } else if count_entries(pv) >= ENTRIES || faction_entries(pv, f) >= PER_FACTION {
            (l_fate::BOUNCED, troops)
        } else {
            let i = free_entry(pv).expect("room");
            let eo = entry_off(i);
            put(
                pv,
                eo + l::entry::ID,
                u64_at(ci, o + l::arrival::HOST_ID).to_le_bytes(),
            );
            pv[eo + l::entry::FACTION] = f;
            pv[eo + l::entry::UNIT] = ci[o + l::arrival::UNIT];
            pv[eo + l::entry::TILE] = ci[o + l::arrival::TILE];
            pv[eo + l::entry::STATE] = l::entry::STATE_ROSTER;
            put(pv, eo + l::entry::TROOPS, troops.to_le_bytes());
            put(pv, eo + l::entry::FROM_BELL, (bell + 1).to_le_bytes());
            pv[l::province::N_ENTRIES] += 1;
            (l_fate::STAYS, troops)
        };
        ci[o + l::arrival::FATE] = fate;
        put(ci, o + l::arrival::TROOPS_AFTER, after.to_le_bytes());
    }
    apply_pending(pv, bell);
    let entries_region = &pv[l::province::ENTRIES..l::province::ENTRIES + ENTRIES * 48];
    let arrivals_region = &ci[l::clash_inputs::ARRIVALS..l::clash_inputs::ARRIVALS + 24 * 40];
    Sha256::new()
        .chain_update(b"model-clash")
        .chain_update(seed)
        .chain_update(bell.to_le_bytes())
        .chain_update(entries_region)
        .chain_update(arrivals_region)
        .finalize()
        .into()
}

mod l_fate {
    pub const STAYS: u8 = 1;
    pub const BOUNCED: u8 = 3;
    pub const DESTROYED: u8 = 5;
}

// ------------------------------------------------------------------ player: Dissolve, Depart

/// 0x44 Dissolve(host): P + `[holding w] [province w]`.
pub fn dissolve(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 20_000)?;
    let host = c.u64()?;
    c.done()?;
    let pl = player(ic)?;
    let h = own_holding(ic, &pl, 4)?;
    let (p, q) = pq_of(&key_province(ic, 5)?);
    let mut pv = read_province(ic, 5, &pl.season, p, q)?;
    let (hp, hq, hs, _) = host_holding(host).ok_or(e(BAD_DATA))?;
    if (hp, hq, hs)
        != (
            i16_at(&h, l::holding::P),
            i16_at(&h, l::holding::Q),
            h[l::holding::SITE],
        )
    {
        return Err(e(NOT_OWNER));
    }
    let i = find_entry(&pv, host).ok_or(e(NOT_RESIDENT))?;
    let o = entry_off(i);
    if pv[o + l::entry::STATE] != 1 || pv[o + l::entry::PEND_OP] != 0 {
        return Err(e(HOST_BUSY));
    }
    if u32_at(&pv, l::province::RESOLVED_NEXT) + 1 < pl.bell {
        return Err(e(NOT_RESIDENT));
    }
    pv[o + l::entry::PEND_OP] = 5;
    put(&mut pv, o + l::entry::PEND_BELL, pl.bell.to_le_bytes());
    write(ic, 5, &pv)?;
    let mut pay = pl.bell.to_le_bytes().to_vec();
    pay.extend_from_slice(&(u32_at(&pv, o + l::entry::TROOPS) as i64).to_le_bytes());
    log_ps2(
        ic,
        &body(Kind::DISSOLVE, pl.bell, &host.to_le_bytes(), &pay),
    );
    Ok(())
}

/// The data of account `i` (for its stored P, Q).
fn key_province(ic: &InvokeContext, i: u16) -> R<Vec<u8>> {
    let (_, d, _) = read(ic, i)?;
    if d.len() != size::PROVINCE {
        return Err(e(BAD_ACCOUNT));
    }
    Ok(d)
}

/// 0x50 Depart: P + `[holding w] [province w] [system]`.
pub fn depart(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 22_000)?;
    let host = c.u64()?;
    let commit = c.arr::<32>()?;
    let seal_b = c.arr::<165>()?;
    let arrive = c.u32()?;
    let tip = c.u64()?;
    let slot_i = c.u8()? as usize;
    c.done()?;
    let pl = player(ic)?;
    let mut h = own_holding(ic, &pl, 4)?;
    if h[l::holding::STATE] != l::holding::STATE_FINAL {
        return Err(e(NOT_FINAL));
    }
    let (p, q) = pq_of(&key_province(ic, 5)?);
    let mut pv = read_province(ic, 5, &pl.season, p, q)?;
    if u32_at(&pv, l::province::RESOLVED_NEXT) + 1 < pl.bell {
        return Err(e(NOT_RESIDENT));
    }
    // A compressed G2 point has the compression bit (0x80) set.
    if seal_b[0] & 0x80 == 0 {
        return Err(e(BAD_DATA));
    }
    let (hp, hq, hs, _) = host_holding(host).ok_or(e(BAD_DATA))?;
    if (hp, hq, hs)
        != (
            i16_at(&h, l::holding::P),
            i16_at(&h, l::holding::Q),
            h[l::holding::SITE],
        )
    {
        return Err(e(NOT_OWNER));
    }
    let i = find_entry(&pv, host).ok_or(e(NOT_RESIDENT))?;
    let o = entry_off(i);
    if pv[o + l::entry::STATE] != 1 || pv[o + l::entry::PEND_OP] != 0 {
        return Err(e(HOST_BUSY));
    }
    for t in 0..4 {
        let to = transit_off(t);
        if (1..=3).contains(&h[to]) && u64_at(&h, to + l::transit::HOST_ID) == host {
            return Err(e(HOST_IN_TRANSIT));
        }
    }
    if arrive < pl.bell + 2 || arrive > pl.bell + 72 {
        return Err(e(ARRIVAL_BELL));
    }
    if tip < tip_min(&pl.s) {
        return Err(e(TIP_TOO_LOW));
    }
    if slot_i >= 4 || h[transit_off(slot_i)] != 0 {
        return Err(e(TRANSIT_STATE));
    }
    let fee = u64_at(&pl.s, l::season::MARCH_FEE);
    let bond = u64_at(&pl.s, l::season::SEAL_BOND);
    let total = tip + fee + bond;
    let (_, _, payer_l) = read(ic, 1)?;
    if payer_l < total {
        return Err(e(INSUFFICIENT));
    }
    ic.native_invoke_signed(transfer_ix(key(ic, 1)?, key(ic, 4)?, total), &[])?;
    let troops = u32_at(&pv, o + l::entry::TROOPS);
    let to = transit_off(slot_i);
    h[to + l::transit::STATE] = 1;
    h[to + l::transit::UNIT] = pv[o + l::entry::UNIT];
    h[to + l::transit::FACTION] = pv[o + l::entry::FACTION];
    h[to + l::transit::ORIGIN_TILE] = pv[o + l::entry::TILE];
    put(&mut h, to + l::transit::ORIGIN_P, p.to_le_bytes());
    put(&mut h, to + l::transit::ORIGIN_Q, q.to_le_bytes());
    put(&mut h, to + l::transit::HOST_ID, host.to_le_bytes());
    put(&mut h, to + l::transit::DEPART_BELL, pl.bell.to_le_bytes());
    put(&mut h, to + l::transit::ARRIVE_BELL, arrive.to_le_bytes());
    put(&mut h, to + l::transit::DEPART_TS, pl.now.to_le_bytes());
    put(&mut h, to + l::transit::DEP_MASS, troops.to_le_bytes());
    let root = seal::seal_root(&commit, &seal::ct_hash(&seal_b));
    put(&mut h, to + l::transit::SEAL_ROOT, root);
    put(&mut h, to + l::transit::TIP, tip.to_le_bytes());
    h[to + l::transit::FLAGS] = 3;
    let esc = u64_at(&h, l::holding::ESCROW) + total;
    put(&mut h, l::holding::ESCROW, esc.to_le_bytes());
    put(&mut h, l::holding::LAST_OWNER_ACTION, pl.now.to_le_bytes());
    pv[o + l::entry::PEND_OP] = 1;
    put(&mut pv, o + l::entry::PEND_BELL, pl.bell.to_le_bytes());
    write(ic, 4, &h)?;
    write(ic, 5, &pv)?;
    let mut pay = (p as i32).to_le_bytes().to_vec();
    pay.extend_from_slice(&(q as i32).to_le_bytes());
    pay.push(pv[o + l::entry::TILE]);
    pay.extend_from_slice(&pl.bell.to_le_bytes());
    pay.extend_from_slice(&arrive.to_le_bytes());
    pay.extend_from_slice(&troops.to_le_bytes());
    pay.extend_from_slice(&0u16.to_le_bytes());
    pay.extend_from_slice(&tip.to_le_bytes());
    pay.extend_from_slice(&root);
    pay.extend_from_slice(&commit);
    pay.extend_from_slice(&seal_b);
    log_ps2(ic, &body(Kind::DEPART, pl.bell, &host.to_le_bytes(), &pay));
    Ok(())
}

// ------------------------------------------------------------------ Reveal

/// 0x51 Reveal (class W).
pub fn reveal(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 18_000)?;
    let slot_i = c.u8()? as usize;
    let target_i = c.u8()?;
    let plain_b = c.arr::<37>()?;
    let salt = c.arr::<32>()?;
    let ct = c.arr::<32>()?;
    let beneficiary = c.arr::<32>()?;
    c.done()?;
    if !is_signer(ic, 0)? {
        return Err(e(AUTH));
    }
    let (season, s, now, slot, _) = keeper_season(ic, 1)?;
    let n = n_accounts(ic)?;
    let prog = program_id(ic)?;
    // 1. The season takes reveals.
    let eff = effective(&s, now);
    if eff != RUNNING && eff != ENDED {
        return Err(e(WRONG_STATUS));
    }
    // 2. The holding and its transit.
    let (ho, h, _) = read(ic, 2)?;
    if ho != prog || h.len() != size::HOLDING || get::<8>(&h, 0) != *magic::HOLDING {
        return Err(e(BAD_ACCOUNT));
    }
    let hraw = fa::raw_holding(
        i16_at(&h, l::holding::P) as i32,
        i16_at(&h, l::holding::Q) as i32,
        h[l::holding::SITE],
    );
    if addr_of(&season, &prog, SeedKind::Holding, &hraw) != key(ic, 2)? {
        return Err(e(BAD_ADDRESS));
    }
    if slot_i >= 4 {
        return Err(e(BAD_DATA));
    }
    let to = transit_off(slot_i);
    if !(1..=3).contains(&h[to]) {
        return Err(e(TRANSIT_STATE));
    }
    let host = u64_at(&h, to + l::transit::HOST_ID);
    let arrive = u32_at(&h, to + l::transit::ARRIVE_BELL);
    if eff == ENDED && arrive >= u32_at(&s, l::season::END_BELL) {
        return Err(e(WRONG_STATUS));
    }
    let plain = seal::unpack(&plain_b);
    if seal::validate(&plain, host, arrive).is_err() {
        return Err(e(BAD_PLAINTEXT));
    }
    // 3. The commitment.
    let commit = seal::commit(&plain_b, &salt);
    if seal::seal_root(&commit, &ct) != get::<32>(&h, to + l::transit::SEAL_ROOT) {
        return Err(e(COMMIT_MISMATCH));
    }
    // 4. The window.
    let (dp, dq) = (plain.dest_p as i32, plain.dest_q as i32);
    let rg = fclient::ix::region_of(dp, dq);
    let sc = season_clock(&s);
    let an = maybe(
        ic,
        3,
        &season,
        SeedKind::BellAnchor,
        &fa::raw_anchor(arrive, rg),
        magic::BELL_ANCHOR,
    )?;
    let ar = maybe(
        ic,
        4,
        &season,
        SeedKind::AnchorArchive,
        &fa::raw_archive(rg, fa::archive_part(arrive)),
        magic::ANCHOR_ARCHIVE,
    )?;
    let blog = owned(
        ic,
        5,
        &season,
        SeedKind::BeaconLog,
        &[rg],
        magic::BEACON_LOG,
        size::BEACON_LOG,
    )?;
    let mut anchor_slot = None;
    match an {
        Some(a) => {
            let a_t = i64_at(&a, l::bell_anchor::A);
            let latest = u64_at(&blog, 24);
            if !sc.reveal_open(arrive, a_t, now, latest) {
                return Err(e(WINDOW_CLOSED));
            }
            anchor_slot = Some(u64_at(&a, l::bell_anchor::SLOT));
        }
        None => {
            if let Some(ar) = ar {
                let j = (arrive % 72) as usize;
                if ar[l::anchor_archive::TOMBSTONE + j / 8] & (1 << (j % 8)) != 0 {
                    return Err(e(ARCHIVED));
                }
            }
        }
    }
    let _ = anchor_slot;
    // 5. The latch.
    if maybe(
        ic,
        6,
        &season,
        SeedKind::ClashInputs,
        &fa::raw_clash_inputs(dp, dq, arrive),
        magic::CLASH_INPUTS,
    )?
    .is_some()
    {
        return Err(e(LATCH_CLOSED));
    }
    let pv = read_province(ic, 7, &season, dp as i16, dq as i16)?;
    if u32_at(&pv, l::province::RESOLVED_NEXT) > arrive {
        return Err(e(LATCH_CLOSED));
    }
    // 6. Path provinces are accepted as given (left out).
    // 7. The quota.
    let faction = h[to + l::transit::FACTION];
    let citizen_tag = u64::from_le_bytes(get(&h, l::holding::OWNER_CITIZEN));
    let x = SlotEntry {
        host_id: host,
        citizen: citizen_tag,
        troops: u32_at(&h, to + l::transit::DEP_MASS),
    };
    let mut slots = [None; 4];
    let mut datas: Vec<Option<Vec<u8>>> = vec![];
    for i in 0..4u8 {
        let d = maybe(
            ic,
            9 + i as u16,
            &season,
            SeedKind::ArrivalSlot,
            &fa::raw_arrival_slot(dp, dq, arrive, faction, i),
            magic::ARRIVAL_SLOT,
        )?;
        if let Some(d) = &d {
            slots[i as usize] = Some(SlotEntry {
                host_id: u64_at(d, l::arrival_slot::HOST_ID),
                citizen: u64_at(d, l::arrival_slot::CITIZEN_TAG),
                troops: u32_at(d, l::arrival_slot::DEP_MASS),
            });
        }
        datas.push(d);
    }
    let dec = permutation_rules::frontier::clash::admit_arrival(
        ProvinceCoord::new(dp, dq),
        arrive,
        &slots,
        x,
    );
    let (i, displaced) = match dec {
        SlotDecision::Refuse(SlotRefusal::AlreadyIn) => return Err(e(ALREADY_DONE)),
        SlotDecision::Refuse(_) => return Err(e(QUOTA_REFUSED)),
        SlotDecision::Fill { slot } => (slot, None),
        SlotDecision::Displace { slot, displaced } => (slot, Some(displaced.host_id)),
    };
    if i != target_i {
        return Err(e(SLOT_MOVED));
    }
    // 8. The ArrivalDay.
    let day = arrive / 144;
    let ad = maybe(
        ic,
        8,
        &season,
        SeedKind::ArrivalDay,
        &fa::raw_arrival_day(dp, dq, day),
        magic::ARRIVAL_DAY,
    )?;
    let bit = (arrive % 144) as usize;
    let had = ad
        .as_ref()
        .is_some_and(|d| d[l::arrival_day::BITS + bit / 8] & (1 << (bit % 8)) != 0);
    let mut created_day = false;
    if !had {
        if !writable(ic, 8)? {
            return Err(e(NEED_ARRIVAL_DAY));
        }
        let mut d = match ad {
            Some(d) => d,
            None => {
                let seed = super::seed_str(SeedKind::ArrivalDay, &fa::raw_arrival_day(dp, dq, day));
                init_with_seed(ic, 0, 8, &season, &seed, size::ARRIVAL_DAY, 0)?;
                let mut d = vec![0u8; size::ARRIVAL_DAY];
                super::header(&mut d, *magic::ARRIVAL_DAY, season.id, false);
                put(&mut d, l::arrival_day::P, (dp as i16).to_le_bytes());
                put(&mut d, l::arrival_day::Q, (dq as i16).to_le_bytes());
                put(&mut d, l::arrival_day::DAY, day.to_le_bytes());
                put(&mut d, l::arrival_day::RENT_TO, key(ic, 0)?.to_bytes());
                d
            }
        };
        d[l::arrival_day::BITS + bit / 8] |= 1 << (bit % 8);
        write(ic, 8, &d).map_err(|_| e(NEED_ARRIVAL_DAY))?;
        created_day = true;
    }
    // 9. Evidence; the slot.
    let (price, limit, loaded) = evidence(ic, n - 2);
    let si = 9 + i as u16;
    if !writable(ic, si)? {
        return Err(e(SLOT_MOVED));
    }
    let mut d = match datas[i as usize].take() {
        Some(d) => d,
        None => {
            let seed = super::seed_str(
                SeedKind::ArrivalSlot,
                &fa::raw_arrival_slot(dp, dq, arrive, faction, i),
            );
            init_with_seed(ic, 0, si, &season, &seed, size::ARRIVAL_SLOT, 0)?;
            let mut d = vec![0u8; size::ARRIVAL_SLOT];
            super::header(&mut d, *magic::ARRIVAL_SLOT, season.id, false);
            put(&mut d, l::arrival_slot::RENT_TO, key(ic, 0)?.to_bytes());
            d
        }
    };
    put(&mut d, l::arrival_slot::P, (dp as i16).to_le_bytes());
    put(&mut d, l::arrival_slot::Q, (dq as i16).to_le_bytes());
    put(&mut d, l::arrival_slot::BELL, arrive.to_le_bytes());
    d[l::arrival_slot::FACTION] = faction;
    d[l::arrival_slot::I] = i;
    d[l::arrival_slot::UNIT] = h[to + l::transit::UNIT];
    d[l::arrival_slot::STANCE] = plain.stance;
    d[l::arrival_slot::TILE] = plain.dest_tile;
    d[l::arrival_slot::FLAGS] = if created_day { 2 } else { 0 };
    put(
        &mut d,
        l::arrival_slot::RETREAT_BPS,
        plain.retreat_bps.to_le_bytes(),
    );
    put(&mut d, l::arrival_slot::HOST_ID, host.to_le_bytes());
    put(
        &mut d,
        l::arrival_slot::CITIZEN_TAG,
        citizen_tag.to_le_bytes(),
    );
    put(&mut d, l::arrival_slot::DEP_MASS, x.troops.to_le_bytes());
    put(&mut d, l::arrival_slot::BENEFICIARY, beneficiary);
    put(&mut d, l::arrival_slot::EV_SLOT, slot.to_le_bytes());
    put(&mut d, l::arrival_slot::EV_PRICE, price.to_le_bytes());
    put(&mut d, l::arrival_slot::EV_LIMIT, limit.to_le_bytes());
    put(&mut d, l::arrival_slot::EV_LOADED, loaded.to_le_bytes());
    d[l::arrival_slot::CLAIMED] = 0;
    write(ic, si, &d).map_err(|_| e(SLOT_MOVED))?;
    let mut k = dp.to_le_bytes().to_vec();
    k.extend_from_slice(&dq.to_le_bytes());
    k.extend_from_slice(&arrive.to_le_bytes());
    k.push(faction);
    k.push(i);
    let mut pay = host.to_le_bytes().to_vec();
    pay.push(plain.dest_tile);
    pay.push(plain.stance);
    pay.extend_from_slice(&plain.retreat_bps.to_le_bytes());
    pay.push(displaced.is_some() as u8);
    pay.extend_from_slice(&displaced.unwrap_or(0).to_le_bytes());
    pay.extend_from_slice(&beneficiary);
    pay.extend_from_slice(&slot.to_le_bytes());
    pay.extend_from_slice(&price.to_le_bytes());
    pay.extend_from_slice(&limit.to_le_bytes());
    pay.push(created_day as u8);
    log_ps2(ic, &body(Kind::REVEAL, now_bell(&s, now), &k, &pay));
    Ok(())
}

// ------------------------------------------------------------------ SettleDeparture

/// 0x52 SettleDeparture: `[payer s] [season] [origin province w] [holding w]`.
pub fn settle_departure(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 12_000)?;
    let ti = c.u8()? as usize;
    c.done()?;
    if ti == 0xFF {
        return settle_return(ic);
    }
    let (season, s, now, _, _) = keeper_season(ic, 1)?;
    let (ho, mut h, _) = read(ic, 3)?;
    if ho != program_id(ic)? || h.len() != size::HOLDING {
        return Err(e(BAD_ACCOUNT));
    }
    if ti >= 4 {
        return Err(e(BAD_DATA));
    }
    let to = transit_off(ti);
    if h[to] != 1 {
        return Err(e(TRANSIT_STATE));
    }
    let (op, oq) = (
        i16_at(&h, to + l::transit::ORIGIN_P),
        i16_at(&h, to + l::transit::ORIGIN_Q),
    );
    let mut pv = read_province(ic, 2, &season, op, oq)?;
    let depart_bell = u32_at(&h, to + l::transit::DEPART_BELL);
    if u32_at(&pv, l::province::RESOLVED_NEXT) <= depart_bell {
        return Err(e(TOO_EARLY));
    }
    let host = u64_at(&h, to + l::transit::HOST_ID);
    let i = find_entry(&pv, host).ok_or(e(NOT_RESIDENT))?;
    let o = entry_off(i);
    if pv[o + l::entry::STATE] != l::entry::STATE_DEPARTED {
        return Err(e(NOT_RESIDENT));
    }
    let troops = u32_at(&pv, o + l::entry::TROOPS);
    put(&mut h, to + l::transit::TROOPS_AFTER, troops.to_le_bytes());
    h[to] = if troops == 0 { 3 } else { 2 };
    clear_entry(&mut pv, i);
    write(ic, 2, &pv)?;
    write(ic, 3, &h)?;
    let mut pay = troops.to_le_bytes().to_vec();
    pay.extend_from_slice(&0u16.to_le_bytes());
    pay.push((troops == 0) as u8);
    log_ps2(
        ic,
        &body(
            Kind::DEPARTURE_SETTLED,
            now_bell(&s, now),
            &host.to_le_bytes(),
            &pay,
        ),
    );
    Ok(())
}

// ------------------------------------------------------------------ GatherClash

/// 0x60 GatherClash.
pub fn gather(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 25_000)?;
    let bell = c.u32()?;
    let start = c.u8()? as usize;
    let n = c.u8()? as usize;
    let bitmap = c.u32()?;
    let _beneficiary = c.arr::<32>()?;
    c.done()?;
    // The program checks the range first, the fast path included:
    // `n == 0` or past the 24 positions is `BadData` (integ-W4).
    if n == 0 || start + n > 24 {
        return Err(e(BAD_DATA));
    }
    if !is_signer(ic, 0)? {
        return Err(e(AUTH));
    }
    let (season, s, now, slot, _) = keeper_season(ic, 1)?;
    let (_, pd, _) = read(ic, 2)?;
    if pd.len() != size::PROVINCE {
        return Err(e(BAD_ACCOUNT));
    }
    let (p, q) = pq_of(&pd);
    let pv = read_province(ic, 2, &season, p, q)?;
    if bell < u32_at(&pv, l::province::RESOLVED_NEXT) {
        return Err(e(LATCH_CLOSED));
    }
    let rg = pv[l::province::REGION];
    let av = anchor_view(ic, 3, &season, &s, bell, rg)?.ok_or(e(NO_ANCHOR))?;
    if now < season_clock(&s).reveal_close(bell, av.a) {
        return Err(e(TOO_EARLY));
    }
    let (pi, qi) = (p as i32, q as i32);
    let ad = maybe(
        ic,
        4,
        &season,
        SeedKind::ArrivalDay,
        &fa::raw_arrival_day(pi, qi, bell / 144),
        magic::ARRIVAL_DAY,
    )?;
    let mut ci = match maybe(
        ic,
        5,
        &season,
        SeedKind::ClashInputs,
        &fa::raw_clash_inputs(pi, qi, bell),
        magic::CLASH_INPUTS,
    )? {
        Some(d) => d,
        None => {
            let seed = super::seed_str(SeedKind::ClashInputs, &fa::raw_clash_inputs(pi, qi, bell));
            init_with_seed(ic, 0, 5, &season, &seed, size::CLASH_INPUTS, 0)?;
            let mut d = vec![0u8; size::CLASH_INPUTS];
            super::header(&mut d, *magic::CLASH_INPUTS, season.id, true);
            put(&mut d, l::clash_inputs::P, p.to_le_bytes());
            put(&mut d, l::clash_inputs::Q, q.to_le_bytes());
            put(&mut d, l::clash_inputs::BELL, bell.to_le_bytes());
            put(&mut d, l::clash_inputs::RENT_TO, key(ic, 0)?.to_bytes());
            put(&mut d, l::clash_inputs::EV_SLOT, slot.to_le_bytes());
            d
        }
    };
    let bit = (bell % 144) as usize;
    let has = ad
        .as_ref()
        .is_some_and(|d| d[l::arrival_day::BITS + bit / 8] & (1 << (bit % 8)) != 0);
    let mut mask = u32_at(&ci, l::clash_inputs::ARRIVALS_MASK);
    let mut no_arrivals = 0u8;
    if !has {
        ci[l::clash_inputs::FLAGS] |= 1;
        mask = 0x00FF_FFFF;
        no_arrivals = 1;
    } else {
        if start + n > 24 {
            return Err(e(BAD_DATA));
        }
        let mut hi = 8 + n as u16;
        for j in 0..n {
            let k = start + j;
            let (f, i) = ((k / 4) as u8, (k % 4) as u8);
            let sd = maybe(
                ic,
                8 + j as u16,
                &season,
                SeedKind::ArrivalSlot,
                &fa::raw_arrival_slot(pi, qi, bell, f, i),
                magic::ARRIVAL_SLOT,
            )?;
            // bit k = absolute position k (§5.11, as the program reads it)
            let expected = bitmap & (1 << k) != 0;
            let hk = if expected {
                let x = hi;
                hi += 1;
                Some(x)
            } else {
                None
            };
            if mask & (1 << k) != 0 {
                continue;
            }
            let o = l::clash_inputs::ARRIVALS + k * l::clash_inputs::ARRIVAL_STRIDE;
            let Some(sd) = sd else {
                mask |= 1 << k;
                continue;
            };
            let host = u64_at(&sd, l::arrival_slot::HOST_ID);
            let hk = hk.ok_or(e(BAD_DATA))?;
            let (hp, hq, hs, _) = host_holding(host).ok_or(e(BAD_DATA))?;
            let h = holding_at(ic, hk, &season, (hp, hq, hs))?.ok_or(e(DEPARTURE_UNSETTLED))?;
            let t = (0..4)
                .map(transit_off)
                .find(|&to| {
                    u64_at(&h, to + l::transit::HOST_ID) == host
                        && u32_at(&h, to + l::transit::ARRIVE_BELL) == bell
                        && h[to] != 0
                })
                .ok_or(e(DEPARTURE_UNSETTLED))?;
            if !matches!(h[t], 2 | 3) {
                return Err(e(DEPARTURE_UNSETTLED));
            }
            put(&mut ci, o + l::arrival::HOST_ID, host.to_le_bytes());
            put(
                &mut ci,
                o + l::arrival::CITIZEN_TAG,
                u64_at(&sd, l::arrival_slot::CITIZEN_TAG).to_le_bytes(),
            );
            put(
                &mut ci,
                o + l::arrival::DEP_MASS,
                u32_at(&sd, l::arrival_slot::DEP_MASS).to_le_bytes(),
            );
            put(
                &mut ci,
                o + l::arrival::TROOPS,
                u32_at(&h, t + l::transit::TROOPS_AFTER).to_le_bytes(),
            );
            put(
                &mut ci,
                o + l::arrival::RETREAT,
                u16_at(&sd, l::arrival_slot::RETREAT_BPS).to_le_bytes(),
            );
            ci[o + l::arrival::FACTION] = f;
            ci[o + l::arrival::UNIT] = sd[l::arrival_slot::UNIT];
            ci[o + l::arrival::TILE] = sd[l::arrival_slot::TILE];
            ci[o + l::arrival::STANCE] = sd[l::arrival_slot::STANCE];
            ci[o + l::arrival::PRESENT] = 1;
            ci[l::clash_inputs::N_PRESENT] += 1;
            mask |= 1 << k;
        }
    }
    put(&mut ci, l::clash_inputs::ARRIVALS_MASK, mask.to_le_bytes());
    write(ic, 5, &ci)?;
    let mut k = pi.to_le_bytes().to_vec();
    k.extend_from_slice(&qi.to_le_bytes());
    k.extend_from_slice(&bell.to_le_bytes());
    let mut pay = vec![start as u8, n as u8];
    pay.extend_from_slice(&mask.to_le_bytes());
    pay.push(no_arrivals);
    log_ps2(ic, &body(Kind::GATHER, now_bell(&s, now), &k, &pay));
    Ok(())
}

/// The seed of `bell` for the province's region from the SeedCache of THE
/// anchor (accounts `si`, `ai`) or the archive entry.
fn seed_of(
    ic: &InvokeContext,
    si: u16,
    ai: u16,
    season: &SeasonRef,
    s: &[u8],
    bell: u32,
    rg: u8,
) -> R<[u8; 32]> {
    let av = anchor_view(ic, ai, season, s, bell, rg)?.ok_or(e(SEED_NOT_READY))?;
    if let Some(seed) = av.archived_seed {
        return Ok(seed);
    }
    let prog = program_id(ic)?;
    let (o, d, _) = read(ic, si)?;
    if o != prog || d.len() != size::SEED_CACHE || get::<8>(&d, 0) != *magic::SEED_CACHE {
        return Err(e(SEED_NOT_READY));
    }
    let an = addr_of(
        season,
        &prog,
        SeedKind::BellAnchor,
        &fa::raw_anchor(bell, rg),
    );
    let sc = season_clock(s);
    if get::<32>(&d, l::seed_cache::ANCHOR_KEY) != an.to_bytes()
        || u64_at(&d, l::seed_cache::ROUND) != sc.seed_round(bell, av.a)
    {
        return Err(e(SEED_NOT_READY));
    }
    Ok(get(&d, l::seed_cache::SEED))
}

/// 0x61 ResolveFromInputs: K + `[province w] [inputs w] [seedcache|archive r] [anchor|archive r] [ix sysvar]`.
pub fn resolve(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 120_000)?;
    let bell = c.u32()?;
    let beneficiary = c.arr::<32>()?;
    c.done()?;
    if !is_signer(ic, 0)? {
        return Err(e(AUTH));
    }
    let (season, s, now, _, _) = keeper_season(ic, 1)?;
    let (_, pd, _) = read(ic, 2)?;
    if pd.len() != size::PROVINCE {
        return Err(e(BAD_ACCOUNT));
    }
    let (p, q) = pq_of(&pd);
    let mut pv = read_province(ic, 2, &season, p, q)?;
    if u32_at(&pv, l::province::RESOLVED_NEXT) != bell {
        return Err(e(OUT_OF_ORDER));
    }
    let mut ci = maybe(
        ic,
        3,
        &season,
        SeedKind::ClashInputs,
        &fa::raw_clash_inputs(p as i32, q as i32, bell),
        magic::CLASH_INPUTS,
    )?
    .ok_or(e(NOT_GATHERED))?;
    if u32_at(&ci, l::clash_inputs::ARRIVALS_MASK) != 0x00FF_FFFF {
        return Err(e(NOT_GATHERED));
    }
    let rg = pv[l::province::REGION];
    let seed = seed_of(ic, 4, 5, &season, &s, bell, rg)?;
    let input_digest: [u8; 32] = Sha256::new()
        .chain_update(seed)
        .chain_update(&pv[l::province::ENTRIES..l::province::ENTRIES + ENTRIES * 48])
        .chain_update(&ci[l::clash_inputs::ARRIVALS..l::clash_inputs::ARRIVALS + 24 * 40])
        .finalize()
        .into();
    let digest = model_clash(&mut pv, &mut ci, bell, &seed);
    ci[l::clash_inputs::FLAGS] |= 2;
    put(&mut ci, l::clash_inputs::RESOLVER, beneficiary);
    let rts = (now - i64_at(&s, l::season::GENESIS_TS)).max(0) as u32;
    put(&mut ci, l::clash_inputs::RESOLVED_TS, rts.to_le_bytes());
    put(
        &mut pv,
        l::province::RESOLVED_NEXT,
        (bell + 1).to_le_bytes(),
    );
    put(&mut pv, l::province::LAST_OUTCOME_DIGEST, digest);
    let ep = u32_at(&pv, l::province::ROSTER_EPOCH) + 1;
    put(&mut pv, l::province::ROSTER_EPOCH, ep.to_le_bytes());
    write(ic, 2, &pv)?;
    write(ic, 3, &ci)?;
    let mut k = (p as i32).to_le_bytes().to_vec();
    k.extend_from_slice(&(q as i32).to_le_bytes());
    k.extend_from_slice(&bell.to_le_bytes());
    let mut pay = digest.to_vec();
    pay.extend_from_slice(&input_digest);
    pay.extend_from_slice(&0u32.to_le_bytes());
    let mut fates = [0u8; 9];
    for kk in 0..24 {
        let f = ci[l::clash_inputs::ARRIVALS + kk * 40 + l::arrival::FATE] as u32 & 7;
        let bitpos = 3 * kk;
        let v = (f as u128) << bitpos;
        for (b, byte) in fates.iter_mut().enumerate() {
            *byte |= ((v >> (8 * b)) & 0xFF) as u8;
        }
    }
    pay.extend_from_slice(&fates);
    log_ps2(ic, &body(Kind::CLASH, now_bell(&s, now), &k, &pay));
    Ok(())
}

/// 0x63 SkipQuiet(b0, n): `[payer s] [season] [province w] [ad0 r] [ad1 r] [anchor_or_archive × n r]`.
pub fn skip(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 8_000)?;
    let b0 = c.u32()?;
    let n = c.u8()? as u32;
    c.done()?;
    if !(1..=24).contains(&n) || n_accounts(ic)? != 5 + n as u16 {
        return Err(e(BAD_DATA));
    }
    let (season, s, now, _, _) = keeper_season(ic, 1)?;
    let (_, pd, _) = read(ic, 2)?;
    if pd.len() != size::PROVINCE {
        return Err(e(BAD_ACCOUNT));
    }
    let (p, q) = pq_of(&pd);
    let mut pv = read_province(ic, 2, &season, p, q)?;
    if u32_at(&pv, l::province::RESOLVED_NEXT) != b0 {
        return Err(e(OUT_OF_ORDER));
    }
    let rg = pv[l::province::REGION];
    let (pi, qi) = (p as i32, q as i32);
    let d0 = b0 / 144;
    let d1 = (b0 + n - 1) / 144;
    let ad0 = maybe(
        ic,
        3,
        &season,
        SeedKind::ArrivalDay,
        &fa::raw_arrival_day(pi, qi, d0),
        magic::ARRIVAL_DAY,
    )?;
    let ad1 = maybe(
        ic,
        4,
        &season,
        SeedKind::ArrivalDay,
        &fa::raw_arrival_day(pi, qi, if d1 != d0 { d1 } else { d0 + 1 }),
        magic::ARRIVAL_DAY,
    )?;
    let sc = season_clock(&s);
    let mut done = 0u32;
    let mut digest = [0u8; 32];
    for k in 0..n {
        let b = b0 + k;
        if k > 0 {
            let left = {
                use solana_program_runtime::solana_sbpf::vm::ContextObject;
                ic.get_remaining()
            };
            if left < 40_000 {
                break;
            }
        }
        let stop = |code: u32| if k == 0 { Err(e(code)) } else { Ok(()) };
        let Some(av) = anchor_view(ic, 5 + k as u16, &season, &s, b, rg)? else {
            stop(TOO_EARLY)?;
            break;
        };
        if now < sc.reveal_close(b, av.a) {
            stop(TOO_EARLY)?;
            break;
        }
        let ad = if b / 144 == d0 { &ad0 } else { &ad1 };
        let bit = (b % 144) as usize;
        if ad
            .as_ref()
            .is_some_and(|d| d[l::arrival_day::BITS + bit / 8] & (1 << (bit % 8)) != 0)
        {
            stop(NOT_QUIET)?;
            break;
        }
        let mut trial = pv.clone();
        let changed = apply_pending(&mut trial, b);
        if !quiet(&trial, b) {
            stop(NOT_QUIET)?;
            break;
        }
        consume(ic, if changed { 25_000 } else { 1_000 })?;
        pv = trial;
        digest = Sha256::new()
            .chain_update(digest)
            .chain_update(b.to_le_bytes())
            .finalize()
            .into();
        done += 1;
    }
    put(
        &mut pv,
        l::province::RESOLVED_NEXT,
        (b0 + done).to_le_bytes(),
    );
    write(ic, 2, &pv)?;
    let mut k = pi.to_le_bytes().to_vec();
    k.extend_from_slice(&qi.to_le_bytes());
    let mut pay = b0.to_le_bytes().to_vec();
    pay.push(done as u8);
    pay.extend_from_slice(&digest);
    log_ps2(ic, &body(Kind::SKIP, now_bell(&s, now), &k, &pay));
    Ok(())
}

// ------------------------------------------------------------------ SettleTransit

/// 0x54 SettleTransit: `[payer s,w] [season] [holding w] [dest province w] [inputs w] [slot w] [home province w]
/// [anchor|archive r] [slot_beneficiary w] [resolver w] [holding_rent_payer w] [settle_beneficiary w] [system]`.
pub fn settle_transit(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 60_000)?;
    let ti = c.u8()? as usize;
    let commit = c.arr::<32>()?;
    let seal_b = c.arr::<165>()?;
    let beneficiary = c.arr::<32>()?;
    c.done()?;
    let (season, s, now, _, bell_now) = keeper_season(ic, 1)?;
    let prog = program_id(ic)?;
    let (ho, mut h, _) = read(ic, 2)?;
    if ho != prog || h.len() != size::HOLDING || get::<8>(&h, 0) != *magic::HOLDING {
        return Err(e(BAD_ACCOUNT));
    }
    let hp = (
        i16_at(&h, l::holding::P),
        i16_at(&h, l::holding::Q),
        h[l::holding::SITE],
    );
    if addr_of(
        &season,
        &prog,
        SeedKind::Holding,
        &fa::raw_holding(hp.0 as i32, hp.1 as i32, hp.2),
    ) != key(ic, 2)?
    {
        return Err(e(BAD_ADDRESS));
    }
    if ti >= 4 {
        return Err(e(BAD_DATA));
    }
    let to = transit_off(ti);
    match h[to] {
        1 => return Err(e(DEPARTURE_UNSETTLED)),
        2 | 3 => {}
        _ => return Err(e(TRANSIT_STATE)),
    }
    let host = u64_at(&h, to + l::transit::HOST_ID);
    let arrive = u32_at(&h, to + l::transit::ARRIVE_BELL);
    let faction = h[to + l::transit::FACTION];
    // The destination as supplied (a valid seal's plaintext decides it below).
    let (_, dd, _) = read(ic, 3)?;
    if dd.len() != size::PROVINCE {
        return Err(e(BAD_ACCOUNT));
    }
    let (dp, dq) = pq_of(&dd);
    let mut dpv = read_province(ic, 3, &season, dp, dq)?;
    let rg = dpv[l::province::REGION];
    // 2. Time.
    let av = anchor_view(ic, 7, &season, &s, arrive, rg)?.ok_or(e(NO_ANCHOR))?;
    let sc = season_clock(&s);
    if now < sc.settle_after(arrive, av.a) || u32_at(&dpv, l::province::RESOLVED_NEXT) <= arrive {
        return Err(e(TOO_EARLY));
    }
    // 3. The logged pair.
    if seal::seal_root(&commit, &seal::ct_hash(&seal_b))
        != get::<32>(&h, to + l::transit::SEAL_ROOT)
    {
        return Err(e(COMMIT_MISMATCH));
    }
    // 4. The proof.
    let (code, plain) = seal::judge(&seal_b, &commit, &av.sig48, host, arrive);
    if code == 0 {
        let p = plain.expect("valid");
        if (p.dest_p, p.dest_q) != (dp, dq) {
            return Err(e(BAD_ADDRESS));
        }
    }
    // 5. The beneficiary.
    if key(ic, 11)?.to_bytes() != beneficiary {
        return Err(e(BAD_ACCOUNT));
    }
    let (pi, qi) = (dp as i32, dq as i32);
    let mut ci = maybe(
        ic,
        4,
        &season,
        SeedKind::ClashInputs,
        &fa::raw_clash_inputs(pi, qi, arrive),
        magic::CLASH_INPUTS,
    )?
    .filter(|d| d[l::clash_inputs::FLAGS] & 2 != 0);
    let slot_d = {
        let prog = program_id(ic)?;
        let k = key(ic, 5)?;
        let ok = (0..4u8).any(|i| {
            addr_of(
                &season,
                &prog,
                SeedKind::ArrivalSlot,
                &fa::raw_arrival_slot(pi, qi, arrive, faction, i),
            ) == k
        });
        if !ok {
            return Err(e(BAD_ADDRESS));
        }
        let (o, d, _) = read(ic, 5)?;
        (o == prog && d.len() == size::ARRIVAL_SLOT && u64_at(&d, l::arrival_slot::HOST_ID) == host)
            .then_some(d)
    };
    let home = read_province(ic, 6, &season, hp.0, hp.1);
    let rent_payer = get::<32>(&h, l::holding::RENT_PAYER);
    if key(ic, 10)?.to_bytes() != rent_payer {
        return Err(e(BAD_ACCOUNT));
    }
    let tip = u64_at(&h, to + l::transit::TIP);
    let fee = u64_at(&s, l::season::MARCH_FEE);
    let bond = u64_at(&s, l::season::SEAL_BOND);
    let troops = u32_at(&h, to + l::transit::TROOPS_AFTER);
    let out = frontier_abi::log::transit_outcome::BAD_SEAL;
    let mut outcome = out;
    // Payments `(account index or pool_owed = None, amount)`.
    let mut pays: Vec<(Option<u16>, u64)> = vec![];
    let mut returns = 0u32;
    let mut troops_out = 0u32;
    if code != 0 {
        // Bad seal: a Stays host still in the destination → Forfeit.
        if let Some(i) = find_entry(&dpv, host) {
            let o = entry_off(i);
            dpv[o + l::entry::PEND_OP] = 6;
            put(&mut dpv, o + l::entry::PEND_BELL, bell_now.to_le_bytes());
        }
        pays.push((Some(11), tip + fee + bond));
        // A revealed bad seal is in the resolved records: its bit is set
        // too, or CloseClashInputs could never close them (W4-C finding
        // for W4-B: §5.11 names the bit only in the fate branch).
        if let Some(ci) = &mut ci {
            if let Some(k) = (0..24).find(|&k| {
                let o = l::clash_inputs::ARRIVALS + k * 40;
                ci[o + l::arrival::PRESENT] == 1 && u64_at(ci, o + l::arrival::HOST_ID) == host
            }) {
                let sm = u32_at(ci, l::clash_inputs::SETTLED_MASK) | (1 << k);
                put(ci, l::clash_inputs::SETTLED_MASK, sm.to_le_bytes());
            }
        }
    } else {
        use frontier_abi::log::transit_outcome as t;
        let rec = ci.as_ref().and_then(|ci| {
            (0..24).find(|&k| {
                let o = l::clash_inputs::ARRIVALS + k * 40;
                ci[o + l::arrival::PRESENT] == 1 && u64_at(ci, o + l::arrival::HOST_ID) == host
            })
        });
        match (&mut ci, rec) {
            (Some(ci), Some(k)) => {
                let o = l::clash_inputs::ARRIVALS + k * 40;
                let fate = ci[o + l::arrival::FATE];
                let after = u32_at(ci, o + l::arrival::TROOPS_AFTER);
                troops_out = after;
                outcome = fate;
                if matches!(fate, 3 | 4) {
                    returns = after;
                }
                let sm = u32_at(ci, l::clash_inputs::SETTLED_MASK) | (1 << k);
                put(ci, l::clash_inputs::SETTLED_MASK, sm.to_le_bytes());
                if key(ic, 9)?.to_bytes() != get::<32>(ci, l::clash_inputs::RESOLVER) {
                    return Err(e(BAD_ACCOUNT));
                }
                let tip_to = if slot_d.is_some() { 8 } else { 10 };
                if let Some(sd) = &slot_d {
                    if key(ic, 8)?.to_bytes() != get::<32>(sd, l::arrival_slot::BENEFICIARY) {
                        return Err(e(BAD_ACCOUNT));
                    }
                }
                pays.push((Some(tip_to), tip));
                pays.push((Some(9), fee));
                pays.push((Some(10), bond));
            }
            (Some(ci), None) => {
                let recorded: Vec<SlotEntry> = (0..4)
                    .filter_map(|i| {
                        let o = l::clash_inputs::ARRIVALS + (faction as usize * 4 + i) * 40;
                        (ci[o + l::arrival::PRESENT] == 1).then(|| SlotEntry {
                            host_id: u64_at(ci, o + l::arrival::HOST_ID),
                            citizen: u64_at(ci, o + l::arrival::CITIZEN_TAG),
                            troops: u32_at(ci, o + l::arrival::DEP_MASS),
                        })
                    })
                    .collect();
                let x = SlotEntry {
                    host_id: host,
                    citizen: u64::from_le_bytes(get(&h, l::holding::OWNER_CITIZEN)),
                    troops: u32_at(&h, to + l::transit::DEP_MASS),
                };
                if key(ic, 9)?.to_bytes() != get::<32>(ci, l::clash_inputs::RESOLVER) {
                    return Err(e(BAD_ACCOUNT));
                }
                if fclient::play::bounces_unranked(pi, qi, arrive, &recorded, &x) {
                    outcome = t::BOUNCED_UNRANKED;
                    returns = troops;
                    troops_out = troops;
                    pays.push((Some(10), tip + bond));
                    pays.push((Some(9), fee));
                } else {
                    outcome = t::ROUTED;
                    returns = troops / 2;
                    troops_out = returns;
                    pays.push((None, tip));
                    pays.push((Some(9), fee));
                    pays.push((Some(10), bond));
                }
            }
            (None, _) => {
                outcome = t::ROUTED;
                returns = troops / 2;
                troops_out = returns;
                pays.push((None, tip + fee));
                pays.push((Some(10), bond));
            }
        }
    }
    // Payments from the escrow.
    let mut owed = 0u64;
    let mut paid = [0u64; 12];
    let mut esc = u64_at(&h, l::holding::ESCROW);
    for (to_i, amt) in &pays {
        esc = esc.saturating_sub(*amt);
        match to_i {
            Some(i) => {
                paid[*i as usize] += amt;
            }
            None => owed += amt,
        }
    }
    put(&mut h, l::holding::ESCROW, esc.to_le_bytes());
    let po = u64_at(&h, l::holding::POOL_OWED) + owed;
    put(&mut h, l::holding::POOL_OWED, po.to_le_bytes());
    if returns > 0 {
        let unit = h[to + l::transit::UNIT] as usize;
        if unit < 8 {
            let ro = l::holding::RESERVE + 4 * unit;
            let r = u32_at(&h, ro) + returns / 1_000;
            put(&mut h, ro, r.to_le_bytes());
        }
    }
    h[to..to + l::holding::TRANSIT_STRIDE].fill(0);
    write(ic, 2, &h)?;
    for (i, amt) in paid.iter().enumerate() {
        if *amt > 0 {
            move_lamports(ic, 2, i as u16, *amt)?;
        }
    }
    write(ic, 3, &dpv)?;
    if let Some(ci) = &ci {
        write(ic, 4, ci)?;
    }
    let mut slot_kept = 0u8;
    if let Some(mut sd) = slot_d {
        sd[l::arrival_slot::FLAGS] |= 1;
        write(ic, 5, &sd)?;
        slot_kept = 1;
    }
    let _ = home;
    let first8 = |i: u16| -> R<[u8; 8]> { Ok(key(ic, i)?.to_bytes()[..8].try_into().expect("8")) };
    let mut pay = vec![outcome, code];
    pay.extend_from_slice(&troops_out.to_le_bytes());
    let tip_to = pays.first().and_then(|x| x.0).unwrap_or(11);
    pay.extend_from_slice(&first8(tip_to)?);
    pay.extend_from_slice(&tip.to_le_bytes());
    pay.extend_from_slice(&first8(9)?);
    pay.extend_from_slice(&fee.to_le_bytes());
    pay.extend_from_slice(&first8(10)?);
    pay.extend_from_slice(&bond.to_le_bytes());
    pay.extend_from_slice(&first8(11)?);
    pay.extend_from_slice(&(if code != 0 { tip + fee + bond } else { 0 }).to_le_bytes());
    pay.extend_from_slice(&owed.to_le_bytes());
    pay.push(slot_kept);
    log_ps2(
        ic,
        &body(Kind::TRANSIT_SETTLED, bell_now, &host.to_le_bytes(), &pay),
    );
    Ok(())
}

// ------------------------------------------------------------------ closes

fn close_record(ic: &InvokeContext, bell: u32, kind: u8, raw: &[u8], to: u16) -> R<()> {
    let mut k = vec![kind];
    let mut r = raw.to_vec();
    r.resize(15, 0);
    k.extend_from_slice(&r);
    let mut pay = 0u64.to_le_bytes().to_vec();
    pay.extend_from_slice(&[0u8; 32]);
    pay.extend_from_slice(&key(ic, to)?.to_bytes());
    pay.extend_from_slice(&0u64.to_le_bytes());
    log_ps2(ic, &body(Kind::CLOSE, bell, &k, &pay));
    Ok(())
}

/// 0x64 CloseClashInputs: `[any s] [season] [province r] [inputs w] [rent_to w]`.
pub fn close_clash_inputs(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 6_000)?;
    let (p, q, bell) = (c.i16()?, c.i16()?, c.u32()?);
    c.done()?;
    let (season, s, now, _, b) = keeper_season(ic, 1)?;
    let _ = read_province(ic, 2, &season, p, q)?;
    let raw = fa::raw_clash_inputs(p as i32, q as i32, bell);
    let ci = maybe(
        ic,
        3,
        &season,
        SeedKind::ClashInputs,
        &raw,
        magic::CLASH_INPUTS,
    )?
    .ok_or(e(BAD_ACCOUNT))?;
    if key(ic, 4)?.to_bytes() != get::<32>(&ci, l::clash_inputs::RENT_TO) {
        return Err(e(BAD_ACCOUNT));
    }
    let settled = u32_at(&ci, l::clash_inputs::SETTLED_MASK);
    let all = (0..24).all(|k| {
        ci[l::clash_inputs::ARRIVALS + k * 40 + l::arrival::PRESENT] != 1 || settled & (1 << k) != 0
    });
    let grace = u32_at(&s, l::season::CLASH_CLOSE_GRACE) as i64 * BELL_SECS;
    let since = now - i64_at(&s, l::season::GENESIS_TS);
    if ci[l::clash_inputs::FLAGS] & 2 == 0
        || !all
        || u32_at(&ci, l::clash_inputs::RESOLVED_TS) as i64 + grace > since
    {
        return Err(e(INPUTS_OPEN));
    }
    close_record(ic, b, 7, &raw, 4)?;
    close(ic, 3, 4)
}

/// 0x65 CloseArrivalDay: `[any s] [season] [province r] [day w] [rent_to w]`.
pub fn close_arrival_day(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 6_000)?;
    let (p, q, day) = (c.i16()?, c.i16()?, c.u32()?);
    c.done()?;
    let (season, _, _, _, b) = keeper_season(ic, 1)?;
    let pv = read_province(ic, 2, &season, p, q)?;
    let raw = fa::raw_arrival_day(p as i32, q as i32, day);
    let d = maybe(
        ic,
        3,
        &season,
        SeedKind::ArrivalDay,
        &raw,
        magic::ARRIVAL_DAY,
    )?
    .ok_or(e(BAD_ACCOUNT))?;
    if key(ic, 4)?.to_bytes() != get::<32>(&d, l::arrival_day::RENT_TO) {
        return Err(e(BAD_ACCOUNT));
    }
    if u32_at(&pv, l::province::RESOLVED_NEXT) < 144 * (day + 1) {
        return Err(e(TOO_EARLY));
    }
    close_record(ic, b, 9, &raw, 4)?;
    close(ic, 3, 4)
}

/// 0x66 CloseArrivalSlot: `[any s] [season] [slot w] [rent_to w] ([anchor r])`.
pub fn close_arrival_slot(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 6_000)?;
    let (p, q, bell, f, i) = (c.i16()?, c.i16()?, c.u32()?, c.u8()?, c.u8()?);
    c.done()?;
    let (season, s, now, _, b) = keeper_season(ic, 1)?;
    let raw = fa::raw_arrival_slot(p as i32, q as i32, bell, f, i);
    let d = maybe(
        ic,
        2,
        &season,
        SeedKind::ArrivalSlot,
        &raw,
        magic::ARRIVAL_SLOT,
    )?
    .ok_or(e(BAD_ACCOUNT))?;
    if key(ic, 3)?.to_bytes() != get::<32>(&d, l::arrival_slot::RENT_TO) {
        return Err(e(BAD_ACCOUNT));
    }
    let ended = effective(&s, now) == ENDED;
    let case_b = ended
        && now
            >= season_clock(&s).genesis_ts
                + u32_at(&s, l::season::END_BELL) as i64 * BELL_SECS
                + 72 * 3_600;
    let case_a = d[l::arrival_slot::FLAGS] & 1 != 0 && {
        let rg = fclient::ix::region_of(p as i32, q as i32);
        let grace_over = match n_accounts(ic)? {
            5 => match anchor_view(ic, 4, &season, &s, bell, rg)? {
                Some(av) => {
                    now >= season_clock(&s).reveal_close(bell, av.a) + CLAIM_GRACE_BELLS * BELL_SECS
                }
                None => true,
            },
            _ => false,
        };
        d[l::arrival_slot::CLAIMED] != 0 || grace_over
    };
    if !case_a && !case_b {
        return Err(e(TOO_EARLY));
    }
    close_record(ic, b, 8, &raw, 3)?;
    close(ic, 2, 3)
}

// ------------------------------------------------------------------ ClaimDefence

/// 0x70 ClaimDefence: `[keeper s,w] [season] [dpool w] [claim w] [system] ([slot w] [anchor r]) × ≤ 6`.
pub fn claim_defence(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 20_000)?;
    let day = c.u32()?;
    let n = c.u8()? as u16;
    c.done()?;
    if !is_signer(ic, 0)? {
        return Err(e(AUTH));
    }
    if n == 0 || n > 6 || n_accounts(ic)? != 5 + 2 * n {
        return Err(e(BAD_DATA));
    }
    let (season, s, now, _, b) = keeper_season(ic, 1)?;
    let keeper = key(ic, 0)?;
    let sc = season_clock(&s);
    let params = fclient::fees::DefenceParams {
        defence_cap_milli: u32_at(&s, l::season::DEFENCE_CAP_MILLI),
        tip_min: tip_min(&s),
    };
    let lateness = s[l::season::LATENESS_SLOTS] as u64;
    let mut total = 0u64;
    let mut slots_out = vec![];
    for j in 0..n {
        let si = 5 + 2 * j;
        let prog = program_id(ic)?;
        let (o, d, _) = read(ic, si)?;
        if o != prog || d.len() != size::ARRIVAL_SLOT || get::<8>(&d, 0) != *magic::ARRIVAL_SLOT {
            return Err(e(BAD_ACCOUNT));
        }
        let (p, q, bell) = (
            i16_at(&d, l::arrival_slot::P),
            i16_at(&d, l::arrival_slot::Q),
            u32_at(&d, l::arrival_slot::BELL),
        );
        let raw = fa::raw_arrival_slot(
            p as i32,
            q as i32,
            bell,
            d[l::arrival_slot::FACTION],
            d[l::arrival_slot::I],
        );
        if addr_of(&season, &prog, SeedKind::ArrivalSlot, &raw) != key(ic, si)? {
            return Err(e(BAD_ADDRESS));
        }
        let rg = fclient::ix::region_of(p as i32, q as i32);
        let av = anchor_view(ic, si + 1, &season, &s, bell, rg)?.ok_or(e(NO_ANCHOR))?;
        let late = av
            .slot
            .is_some_and(|a| u64_at(&d, l::arrival_slot::EV_SLOT) >= a + lateness);
        if get::<32>(&d, l::arrival_slot::BENEFICIARY) != keeper.to_bytes()
            || d[l::arrival_slot::CLAIMED] != 0
            || now >= sc.reveal_close(bell, av.a) + CLAIM_GRACE_BELLS * BELL_SECS
            || !late
        {
            return Err(e(NOT_ELIGIBLE));
        }
        let ev = fclient::fees::Evidence {
            ev_price: u64_at(&d, l::arrival_slot::EV_PRICE),
            ev_limit: u32_at(&d, l::arrival_slot::EV_LIMIT),
            ev_loaded: u32_at(&d, l::arrival_slot::EV_LOADED),
            created_day: d[l::arrival_slot::FLAGS] & 2 != 0,
        };
        let r = fclient::fees::defence_refund(&ev, &params);
        if r == 0 {
            return Err(e(NOT_ELIGIBLE));
        }
        total += r;
        slots_out.push((si, d));
    }
    let dp = owned(
        ic,
        2,
        &season,
        SeedKind::DefencePool,
        &[],
        magic::DEFENCE_POOL,
        size::DEFENCE_POOL,
    )?;
    let raw = fa::raw_defence_claim(&keeper.to_bytes(), day);
    let mut cl = match maybe(
        ic,
        3,
        &season,
        SeedKind::DefenceClaim,
        &raw,
        magic::DEFENCE_CLAIM,
    )? {
        Some(d) => d,
        None => {
            let seed = super::seed_str(SeedKind::DefenceClaim, &raw);
            init_with_seed(ic, 0, 3, &season, &seed, size::DEFENCE_CLAIM, 0)?;
            let mut d = vec![0u8; size::DEFENCE_CLAIM];
            super::header(&mut d, *magic::DEFENCE_CLAIM, season.id, false);
            put(&mut d, l::defence_claim::BENEFICIARY, keeper.to_bytes());
            put(&mut d, l::defence_claim::DAY, day.to_le_bytes());
            d
        }
    };
    let (_, _, pool_l) = read(ic, 2)?;
    let avail = pool_l.saturating_sub(super::rent(size::DEFENCE_POOL));
    let pay = total.min(avail);
    for (si, mut d) in slots_out {
        d[l::arrival_slot::CLAIMED] = 1;
        write(ic, si, &d)?;
    }
    let cc = u64_at(&cl, l::defence_claim::CLAIMED) + pay;
    put(&mut cl, l::defence_claim::CLAIMED, cc.to_le_bytes());
    let cnt = u32_at(&cl, l::defence_claim::COUNT) + n as u32;
    put(&mut cl, l::defence_claim::COUNT, cnt.to_le_bytes());
    write(ic, 3, &cl)?;
    let _ = dp;
    if pay > 0 {
        move_lamports(ic, 2, 0, pay)?;
    }
    let mut p2 = day.to_le_bytes().to_vec();
    p2.push(n as u8);
    p2.extend_from_slice(&pay.to_le_bytes());
    p2.push((pay < total) as u8);
    log_ps2(ic, &body(Kind::DEFENCE_CLAIM, b, &keeper.to_bytes(), &p2));
    Ok(())
}

// ------------------------------------------------------------------ return settle (§21, W4-A's shape)

/// SettleDeparture with `transit_slot = 0xFF` (W4-A's return settle):
/// `[payer s] [season] [province w] [holding w (canonical, may be absent)]`.
/// Every state-3 `Leave` entry of that Holding in that Province is freed;
/// `reserve[unit] += troops / 1,000` when the Holding is live with the
/// host's generation (else the troops are lost); nothing → `AlreadyDone`.
fn settle_return(ic: &mut InvokeContext) -> R<()> {
    let (season, _, _, _, b) = keeper_season(ic, 1)?;
    let (_, pd, _) = read(ic, 2)?;
    if pd.len() != size::PROVINCE {
        return Err(e(BAD_ACCOUNT));
    }
    let (p, q) = pq_of(&pd);
    let mut pv = read_province(ic, 2, &season, p, q)?;
    let hk = key(ic, 3)?;
    let prog = program_id(ic)?;
    let mut h = {
        let (o, d, _) = read(ic, 3)?;
        (o == prog && d.len() == size::HOLDING).then_some(d)
    };
    let mut returned = 0;
    for i in 0..ENTRIES {
        let o = entry_off(i);
        if pv[o + l::entry::STATE] != l::entry::STATE_DEPARTED || pv[o + l::entry::PEND_OP] != 5 {
            continue;
        }
        let host = u64_at(&pv, o + l::entry::ID);
        let Some((hp, hq, hs, gen)) = host_holding(host) else {
            continue;
        };
        if addr_of(
            &season,
            &prog,
            SeedKind::Holding,
            &fa::raw_holding(hp as i32, hq as i32, hs),
        ) != hk
        {
            continue;
        }
        let troops = u32_at(&pv, o + l::entry::TROOPS);
        let unit = pv[o + l::entry::UNIT] as usize;
        let mut back = false;
        if let Some(h) = h.as_mut() {
            if h[l::holding::GEN] == gen && unit < 8 {
                let ro = l::holding::RESERVE + 4 * unit;
                let r = u32_at(h, ro) + troops / 1_000;
                put(h, ro, r.to_le_bytes());
                back = true;
            }
        }
        clear_entry(&mut pv, i);
        returned += 1;
        let mut pay = troops.to_le_bytes().to_vec();
        pay.extend_from_slice(&0u16.to_le_bytes());
        pay.push(if back { 2 } else { 1 });
        log_ps2(
            ic,
            &body(Kind::DEPARTURE_SETTLED, b, &host.to_le_bytes(), &pay),
        );
    }
    if returned == 0 {
        return Err(e(ALREADY_DONE));
    }
    write(ic, 2, &pv)?;
    if let Some(h) = &h {
        write(ic, 3, h)?;
    }
    Ok(())
}
