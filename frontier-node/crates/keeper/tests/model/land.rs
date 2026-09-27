//! The native model's land instructions (W3-C): what the keeper's land
//! duties talk to while the program's land (W3-A) and holdings (W3-B) are
//! built in parallel. Account lists, checks and effects follow the
//! contract (§5.6, §5.9, §5.10, §5.12) on the `frontier-abi` offsets; the
//! left-outs are listed per instruction. It is **not** the program.
//!
//! | tag | modelled | left out |
//! |---|---|---|
//! | 0x02 InitShards | 8 JoinShards of a faction | — |
//! | 0x20 OpenRing | genesis rings; `d > g` by the crowding rule on the folded values, RingSeed requested with `ring_seed_round(now)` | — |
//! | 0x21 ConsumeRingSeed | round rule, test-key verify, seed | — |
//! | 0x22 OpenProvince | funded from the wedge fund; sites from `terrain::generate_province`; rings 0–1 reserved; the initial camp (`camp::place`); the fund's counters | terrain/resource arrays and masks |
//! | 0x23 FoldOccupancy | three parts, `FoldStale` between parts | — |
//! | 0x30 Join | Citizen, JoinShard members | the capacity rule (a ring can always open here), join gate |
//! | 0x33 FileTicket | sites (ring ≥ 2, opened ring, province present, own wedge), cohorts, `CohortFull`, escrow from the payer | the bucket; the adjacent-wedge fallback |
//! | 0x34 SettleTicket | expired / fresh / displace / taken, the score, seed from THE anchor's cache or the archive, cohorts, escrow → Holding, displaced Citizen and JoinShard; DECISIONS K9 (W4-C): a fresh settlement refused `TicketState` while an earlier cohort of the Province is open | — |
//! | 0x35 ReleaseDormant | dormancy, transits, site released-free, Citizen refugee, JoinShard, `pool_owed` → pool, close to the rent payer | — |
//! | 0x43 Muster | **simplified**: no reserve or cap check; the entry is a roster entry at once | reserve, caps, the pending state |
//! | 0x46 Explore | **simplified**: host entry in this province, tiles not explored, record free | the Scout unit, tile adjacency |
//! | 0x47 SettleExplore | seed, `explore::roll` with the floor, works | — |
//! | 0x48 DisbandStranded | host's Holding absent or of another gen | — |
//! | 0x55 SweepPoolOwed | `pool_owed` → pool | — |
//!
//! Every successful land instruction logs its PS2 record (§6) with an empty
//! tail (the model keeps no event chains).

use solana_address::Address;
use solana_program_runtime::invoke_context::InvokeContext;

use fclient::abi::layout as l;
use fclient::addr::{self as fa, SeedKind};
use frontier_abi::log::Kind;
use permutation_rules::frontier::geometry::ProvinceCoord;

use super::{beacon_arg, effective};
use super::{
    clock, consume, e, get, header, i64_at, init_with_seed, is_absent, is_signer, key, log_ps2,
    move_lamports, n_accounts, program_id, put, read, rent, season_at, season_clock, seed_str,
    transfer_ix, u32_at, u64_at, verify, write, Cursor, SeasonRef, R,
};

const BAD_DATA: u32 = 1;
const BAD_ACCOUNT: u32 = 2;
const BAD_ADDRESS: u32 = 3;
const AUTH: u32 = 4;
const WRONG_STATUS: u32 = 5;
const WRONG_ROUND: u32 = 7;
const CAPACITY: u32 = 10;
const TOO_EARLY: u32 = 13;
const INSUFFICIENT: u32 = 21;
const NO_TICKET: u32 = 23;
const NOT_FINAL: u32 = 24;
const PROVINCE_FULL: u32 = 27;
const NOT_OWNER: u32 = 20;
const FOLD_STALE: u32 = 44;
const TICKET_STATE: u32 = 45;
const NOT_DORMANT: u32 = 46;
const HOST_BUSY: u32 = 28;
const EXPLORED: u32 = 47;
const SEED_NOT_READY: u32 = 54;
const RESERVED_SITE: u32 = 57;
const COHORT_FULL: u32 = 60;

const SEEDED: u8 = 2;
const RUNNING: u8 = 3;
const ENDED: u8 = 4;

pub(super) fn i16_at(d: &[u8], o: usize) -> i16 {
    i16::from_le_bytes(get(d, o))
}
pub(super) fn u16_at(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(get(d, o))
}

pub(super) fn addr_of(season: &SeasonRef, prog: &Address, kind: SeedKind, raw: &[u8]) -> Address {
    fa::with_seed(&season.key, &seed_str(kind, raw), prog)
}

/// Reads account `i` as the program's account of `magic`/`size` at the
/// canonical address of `(kind, raw)`.
pub(super) fn owned(
    ic: &InvokeContext,
    i: u16,
    season: &SeasonRef,
    kind: SeedKind,
    raw: &[u8],
    magic: &[u8; 8],
    size: usize,
) -> R<Vec<u8>> {
    let prog = program_id(ic)?;
    if addr_of(season, &prog, kind, raw) != key(ic, i)? {
        return Err(e(BAD_ADDRESS));
    }
    let (o, d, _) = read(ic, i)?;
    if o != prog || d.len() != size || get::<8>(&d, 0) != *magic {
        return Err(e(BAD_ACCOUNT));
    }
    Ok(d)
}

pub(super) fn body(kind: Kind, bell: u32, key: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut b = vec![0u8; 512];
    let n = frontier_abi::log::write_body(kind, bell, key, payload, &mut b).expect("widths");
    b.truncate(n);
    b.push(0);
    b
}

pub(super) fn pqs(p: i16, q: i16, site: u8) -> Vec<u8> {
    let mut k = (p as i32).to_le_bytes().to_vec();
    k.extend_from_slice(&(q as i32).to_le_bytes());
    k.push(site);
    k
}

pub(super) fn now_bell(s: &[u8], now: i64) -> u32 {
    season_clock(s).bell_at(now).unwrap_or(0)
}

pub(super) fn running(s: &[u8], now: i64) -> R<()> {
    if effective(s, now) != RUNNING {
        return Err(e(WRONG_STATUS));
    }
    Ok(())
}

// ------------------------------------------------------------------ player prologue

/// `[0 actor s] [1 payer s,w] [2 season] [3 citizen w]` (§5.6, no bucket).
pub(super) struct Player {
    pub(super) season: SeasonRef,
    pub(super) s: Vec<u8>,
    pub(super) c: Vec<u8>,
    pub(super) now: i64,
    pub(super) bell: u32,
}

pub(super) fn player(ic: &InvokeContext) -> R<Player> {
    let (season, s) = season_at(ic, 2)?;
    let (_, now) = clock(ic)?;
    running(&s, now)?;
    if !is_signer(ic, 0)? || !is_signer(ic, 1)? {
        return Err(e(AUTH));
    }
    let (o, c, _) = read(ic, 3)?;
    let prog = program_id(ic)?;
    if o != prog || c.len() != fclient::abi::size::CITIZEN {
        return Err(e(BAD_ACCOUNT));
    }
    let wallet = Address::new_from_array(get(&c, l::citizen::WALLET));
    let tag = fa::citizen_tag15(&wallet.to_bytes());
    if addr_of(&season, &prog, SeedKind::Citizen, &tag) != key(ic, 3)? {
        return Err(e(BAD_ADDRESS));
    }
    let actor = key(ic, 0)?;
    let session = Address::new_from_array(get(&c, l::citizen::SESSION));
    let ok = actor == wallet || (actor == session && now < i64_at(&c, l::citizen::SESSION_EXPIRY));
    if !ok {
        return Err(e(AUTH));
    }
    let bell = now_bell(&s, now);
    if bell >= u32_at(&s, l::season::END_BELL) {
        return Err(e(WRONG_STATUS));
    }
    Ok(Player {
        season,
        s,
        c,
        now,
        bell,
    })
}

/// The holding at account `i` owned by the player's citizen.
pub(super) fn own_holding(ic: &InvokeContext, pl: &Player, i: u16) -> R<Vec<u8>> {
    let (o, h, _) = read(ic, i)?;
    let prog = program_id(ic)?;
    if o != prog || h.len() != fclient::abi::size::HOLDING {
        return Err(e(BAD_ACCOUNT));
    }
    let raw = fa::raw_holding(
        i16_at(&h, l::holding::P) as i32,
        i16_at(&h, l::holding::Q) as i32,
        h[l::holding::SITE],
    );
    if addr_of(&pl.season, &prog, SeedKind::Holding, &raw) != key(ic, i)? {
        return Err(e(BAD_ADDRESS));
    }
    if get::<32>(&h, l::holding::OWNER_CITIZEN) != key(ic, 3)?.to_bytes() {
        return Err(e(NOT_OWNER));
    }
    Ok(h)
}

fn cohorts(pv: &[u8]) -> Vec<(usize, u32, u16, u16)> {
    (0..l::province::COHORTS)
        .map(|i| {
            let o = l::province::TICKET_COHORTS + i * l::province::COHORT_STRIDE;
            (o, u32_at(pv, o), u16_at(pv, o + 4), u16_at(pv, o + 6))
        })
        .collect()
}

fn cohort_closed(pv: &[u8], ticket_bell: u32, bell: u32) -> bool {
    cohorts(pv)
        .iter()
        .filter(|c| c.2 > 0 && c.1 == ticket_bell)
        .all(|c| c.3 >= c.2 || bell >= c.1.saturating_add(24))
}

/// The lazy provisional → final flip (§5.6 step 5).
fn flip_final(h: &mut [u8], c: &mut [u8], pv: &[u8], now: i64, bell: u32) {
    if h[l::holding::STATE] == l::holding::STATE_PROVISIONAL
        && now >= i64_at(h, l::holding::FINAL_TS)
        && cohort_closed(pv, u32_at(h, l::holding::TICKET_BELL), bell)
    {
        h[l::holding::STATE] = l::holding::STATE_FINAL;
        c[l::citizen::FLAGS] |= l::citizen::FLAG_FIRST_FINAL;
        c[l::citizen::FLAGS] &= !l::citizen::FLAG_PROVISIONAL;
    }
}

fn settle_cohort(pv: &mut [u8], ticket_bell: u32) {
    for (o, b, f, st) in cohorts(pv) {
        if b == ticket_bell && f > 0 && st < f {
            put(pv, o + 6, (st + 1).to_le_bytes());
            return;
        }
    }
}

// ------------------------------------------------------------------ world

pub fn init_shards(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 38_000)?;
    let f = c.u8()?;
    c.done()?;
    if f > 5 || n_accounts(ic)? != 11 {
        return Err(e(BAD_DATA));
    }
    let (season, s) = season_at(ic, 1)?;
    if !is_signer(ic, 0)? || key(ic, 0)?.to_bytes() != get::<32>(&s, l::season::AUTHORITY) {
        return Err(e(AUTH));
    }
    for sh in 0..8u8 {
        let i = 2 + sh as u16;
        init_with_seed(
            ic,
            0,
            i,
            &season,
            &seed_str(SeedKind::JoinShard, &fa::raw_join_shard(f, sh)),
            fclient::abi::size::JOIN_SHARD,
            0,
        )?;
        let mut d = vec![0u8; fclient::abi::size::JOIN_SHARD];
        header(&mut d, *fclient::abi::magic::JOIN_SHARD, season.id, true);
        d[l::join_shard::FACTION] = f;
        d[l::join_shard::SHARD] = sh;
        write(ic, i, &d)?;
    }
    Ok(())
}

pub fn open_ring(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 25_000)?;
    let d = c.u16()?;
    c.done()?;
    if n_accounts(ic)? != 11 {
        return Err(e(BAD_DATA));
    }
    let (season, s) = season_at(ic, 1)?;
    let (_, now) = clock(ic)?;
    let eff = effective(&s, now);
    if eff != SEEDED && eff != RUNNING {
        return Err(e(WRONG_STATUS));
    }
    let prog = program_id(ic)?;
    let mut f = owned(
        ic,
        2,
        &season,
        SeedKind::Frontier,
        &[],
        fclient::abi::magic::FRONTIER,
        fclient::abi::size::FRONTIER,
    )?;
    let opened = u16_at(&f, l::frontier::RINGS_OPENED);
    if d != opened || d > u16_at(&s, l::season::R_MAX) {
        return Err(e(BAD_DATA));
    }
    let bell = season_clock(&s).bell_at(now).unwrap_or(0);
    let g = s[l::season::GENESIS_RING] as u16;
    let mut funds = [0u64; 6];
    for (w, fl) in funds.iter_mut().enumerate() {
        if addr_of(&season, &prog, SeedKind::ProvinceFund, &[w as u8]) != key(ic, 4 + w as u16)? {
            return Err(e(BAD_ADDRESS));
        }
        *fl = read(ic, 4 + w as u16)?.2;
    }
    let seed_s = seed_str(SeedKind::RingSeed, &d.to_le_bytes());
    init_with_seed(ic, 0, 3, &season, &seed_s, fclient::abi::size::RING_SEED, 0)?;
    let mut r = vec![0u8; fclient::abi::size::RING_SEED];
    header(&mut r, *fclient::abi::magic::RING_SEED, season.id, false);
    put(&mut r, l::ring_seed::D, d.to_le_bytes());
    put(&mut r, l::ring_seed::OPENED_BELL, bell.to_le_bytes());
    put(&mut r, l::ring_seed::T_OPEN, now.to_le_bytes());
    put(&mut r, l::ring_seed::PAYER, key(ic, 0)?.to_bytes());
    let (round, seed) = if d <= g {
        let seed = permutation_rules::hash::sha256(&[
            b"PSF-RING",
            &get::<32>(&s, l::season::GENESIS_SEED),
            &d.to_le_bytes(),
        ]);
        r[l::ring_seed::STATUS] = 2;
        put(&mut r, l::ring_seed::SEED, seed);
        (0u64, seed)
    } else {
        // The crowding rule on the folded values (§5.9).
        if eff != RUNNING {
            return Err(e(WRONG_STATUS));
        }
        if bell < u32_at(&f, l::frontier::LAST_RING_OPEN_BELL).saturating_add(1) {
            return Err(e(TOO_EARLY));
        }
        let theta = if now - i64_at(&s, l::season::GENESIS_TS)
            < u32_at(&s, l::season::THETA_SWITCH_SECS) as i64
        {
            u16_at(&s, l::season::THETA_EARLY_BPS)
        } else {
            u16_at(&s, l::season::THETA_LATE_BPS)
        } as u64;
        let crowded = (0..6).any(|w| {
            let open = u32_at(&f, l::frontier::WEDGE_OPEN + 4 * w) as u64;
            let occ = u32_at(&f, l::frontier::WEDGE_OCCUPIED + 4 * w) as u64;
            open > 0 && occ * 10_000 >= theta * open
        });
        if !crowded {
            return Err(e(CAPACITY));
        }
        if funds
            .iter()
            .any(|&x| x.saturating_sub(rent(128)) < d as u64 * rent(4_096))
        {
            return Err(e(INSUFFICIENT));
        }
        let sc = season_clock(&s);
        let round = sc
            .drand
            .ring_seed_round(now, u32_at(&s, l::season::SEED_MARGIN));
        r[l::ring_seed::STATUS] = 1;
        put(&mut r, l::ring_seed::ROUND, round.to_le_bytes());
        (round, [0u8; 32])
    };
    write(ic, 3, &r)?;
    put(&mut f, l::frontier::RINGS_OPENED, (d + 1).to_le_bytes());
    put(&mut f, l::frontier::LAST_RING_OPEN_BELL, bell.to_le_bytes());
    put(&mut f, l::frontier::LAST_RING_OPEN_TS, now.to_le_bytes());
    write(ic, 2, &f)?;
    let mut pl = now.to_le_bytes().to_vec();
    pl.extend_from_slice(&round.to_le_bytes());
    pl.extend_from_slice(&seed);
    log_ps2(ic, &body(Kind::RING_OPEN, bell, &d.to_le_bytes(), &pl));
    Ok(())
}

pub fn consume_ring_seed(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 330_000)?;
    let d = c.u16()?;
    let b = beacon_arg(c)?;
    c.done()?;
    if n_accounts(ic)? != 3 || !is_signer(ic, 0)? {
        return Err(e(BAD_ACCOUNT));
    }
    let (season, s) = season_at(ic, 1)?;
    let (_, now) = clock(ic)?;
    let eff = effective(&s, now);
    if eff != SEEDED && eff != RUNNING {
        return Err(e(WRONG_STATUS));
    }
    let mut r = owned(
        ic,
        2,
        &season,
        SeedKind::RingSeed,
        &d.to_le_bytes(),
        fclient::abi::magic::RING_SEED,
        fclient::abi::size::RING_SEED,
    )?;
    if r[l::ring_seed::STATUS] != 1 {
        return Err(e(52)); // AlreadyDone
    }
    if b.round != u64_at(&r, l::ring_seed::ROUND) {
        return Err(e(WRONG_ROUND));
    }
    let seed = verify(&b)?;
    r[l::ring_seed::STATUS] = 2;
    put(&mut r, l::ring_seed::SEED, seed);
    write(ic, 2, &r)?;
    let mut pl = b.round.to_le_bytes().to_vec();
    pl.extend_from_slice(&seed);
    log_ps2(
        ic,
        &body(Kind::RING_SEED, now_bell(&s, now), &d.to_le_bytes(), &pl),
    );
    Ok(())
}

pub fn open_province(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 150_000)?;
    let p = c.i16()?;
    let q = c.i16()?;
    c.done()?;
    if n_accounts(ic)? != 6 {
        return Err(e(BAD_DATA));
    }
    let (season, s) = season_at(ic, 1)?;
    let (_, now) = clock(ic)?;
    let eff = effective(&s, now);
    if eff != SEEDED && eff != RUNNING {
        return Err(e(WRONG_STATUS));
    }
    let prog = program_id(ic)?;
    let (pi, qi) = (p as i32, q as i32);
    let pc =
        ProvinceCoord::checked(pi, qi, u16_at(&s, l::season::R_MAX)).map_err(|_| e(BAD_DATA))?;
    let ring = pc.ring() as u16;
    let wedge = fclient::ix::wedge_of(pi, qi);
    let rd = owned(
        ic,
        2,
        &season,
        SeedKind::RingSeed,
        &ring.to_le_bytes(),
        fclient::abi::magic::RING_SEED,
        fclient::abi::size::RING_SEED,
    )?;
    if rd[l::ring_seed::STATUS] != 2 {
        return Err(e(SEED_NOT_READY));
    }
    let mut fd = owned(
        ic,
        3,
        &season,
        SeedKind::ProvinceFund,
        &[wedge],
        fclient::abi::magic::PROVINCE_FUND,
        fclient::abi::size::PROVINCE_FUND,
    )?;
    let flam = read(ic, 3)?.2;
    let pseed = seed_str(SeedKind::Province, &fa::raw_province(pi, qi));
    if fa::with_seed(&season.key, &pseed, &prog) != key(ic, 4)? {
        return Err(e(BAD_ADDRESS));
    }
    let (po, pd, plam) = read(ic, 4)?;
    if !is_absent(&po, &pd) {
        return Err(e(BAD_ACCOUNT));
    }
    let size = fclient::abi::size::PROVINCE;
    let need = rent(size).saturating_sub(plam);
    if flam < need + rent(fclient::abi::size::PROVINCE_FUND) {
        return Err(e(INSUFFICIENT));
    }
    move_lamports(ic, 3, 4, need)?;
    let (id, bump) = season.seeds();
    ic.native_invoke_signed(
        super::allocate_with_seed_ix(key(ic, 4)?, season.key, &pseed, size as u64, prog),
        &[&[frontier_abi::layout::world::season::PDA_PREFIX, &id, &bump]],
    )?;
    let bell = season_clock(&s).bell_at(now).unwrap_or(0);
    let ring_seed: [u8; 32] = get(&rd, l::ring_seed::SEED);
    let terr = permutation_rules::frontier::terrain::generate_province(&ring_seed, pc);
    let reserved = ring < 2;
    let mut d = vec![0u8; size];
    header(&mut d, *fclient::abi::magic::PROVINCE, season.id, true);
    put(&mut d, l::province::P, p.to_le_bytes());
    put(&mut d, l::province::Q, q.to_le_bytes());
    put(&mut d, l::province::RING, ring.to_le_bytes());
    d[l::province::WEDGE] = wedge;
    d[l::province::REGION] = fclient::ix::region_of(pi, qi);
    put(&mut d, l::province::RESOLVED_NEXT, bell.to_le_bytes());
    put(&mut d, l::province::OPENED_BELL, bell.to_le_bytes());
    let n = terr.site_count as usize;
    for i in 0..n {
        d[l::province::SITES + i] = terr.sites[i];
        if reserved {
            d[l::province::SITE_MIRROR + i * l::province::SITE_MIRROR_STRIDE] =
                l::site::STATE_RESERVED;
        }
    }
    d[l::province::SITE_COUNT] = terr.site_count;
    let camp =
        permutation_rules::frontier::camp::place(&ring_seed, pc, &terr, bell / 144, false, true);
    if let Some(cp) = camp {
        d[l::province::CAMP] = cp.tile;
        d[l::province::CAMP + 1] = 1;
        put(&mut d, l::province::CAMP + 4, cp.troops.to_le_bytes());
    }
    write(ic, 4, &d)?;
    let open = if reserved { 0 } else { n as u32 };
    let x = u32_at(&fd, l::province_fund::PROVINCES_OPENED) + 1;
    put(&mut fd, l::province_fund::PROVINCES_OPENED, x.to_le_bytes());
    let x = u32_at(&fd, l::province_fund::OPEN_SITES) + open;
    put(&mut fd, l::province_fund::OPEN_SITES, x.to_le_bytes());
    let x = u64_at(&fd, l::province_fund::SPENT_TOTAL) + need;
    put(&mut fd, l::province_fund::SPENT_TOTAL, x.to_le_bytes());
    write(ic, 3, &fd)?;
    let mut pl = ring.to_le_bytes().to_vec();
    pl.push(wedge);
    pl.push(fclient::ix::region_of(pi, qi));
    pl.extend_from_slice(&[0u8; 32]);
    pl.push(terr.site_count);
    pl.push(camp.map_or(0, |c| c.tile));
    pl.extend_from_slice(&camp.map_or(0, |c| c.troops).to_le_bytes());
    pl.push(reserved as u8);
    let mut k = pi.to_le_bytes().to_vec();
    k.extend_from_slice(&qi.to_le_bytes());
    log_ps2(ic, &body(Kind::PROVINCE_OPEN, bell, &k, &pl));
    Ok(())
}

pub fn fold(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 20_000)?;
    let part = c.u8()?;
    c.done()?;
    let (season, s) = season_at(ic, 1)?;
    let (_, now) = clock(ic)?;
    running(&s, now)?;
    let prog = program_id(ic)?;
    let mut f = owned(
        ic,
        2,
        &season,
        SeedKind::Frontier,
        &[],
        fclient::abi::magic::FRONTIER,
        fclient::abi::size::FRONTIER,
    )?;
    let bell = now_bell(&s, now);
    let fb = u32_at(&f, l::frontier::FOLD_BELL);
    let fp = f[l::frontier::FOLD_PART];
    match part {
        0 | 1 => {
            if n_accounts(ic)? != 27 {
                return Err(e(BAD_DATA));
            }
            if part == 1 && (fb != bell || fp != 1) {
                return Err(e(FOLD_STALE));
            }
            let (mut occ, mut wedge) = if part == 0 {
                (0u32, [0u32; 6])
            } else {
                (
                    u32_at(&f, l::frontier::ACC_OCCUPIED),
                    core::array::from_fn(|w| u32_at(&f, l::frontier::ACC_WEDGE + 4 * w)),
                )
            };
            let factions = if part == 0 { 0..3u8 } else { 3..6u8 };
            let mut i = 3u16;
            for fac in factions {
                for sh in 0..8u8 {
                    let js = owned(
                        ic,
                        i,
                        &season,
                        SeedKind::JoinShard,
                        &fa::raw_join_shard(fac, sh),
                        fclient::abi::magic::JOIN_SHARD,
                        fclient::abi::size::JOIN_SHARD,
                    )?;
                    occ += u32_at(&js, l::join_shard::HOLDINGS);
                    for (w, x) in wedge.iter_mut().enumerate() {
                        *x += u32_at(&js, l::join_shard::HOLDINGS_BY_WEDGE + 4 * w);
                    }
                    i += 1;
                }
            }
            put(&mut f, l::frontier::ACC_OCCUPIED, occ.to_le_bytes());
            for (w, x) in wedge.iter().enumerate() {
                put(&mut f, l::frontier::ACC_WEDGE + 4 * w, x.to_le_bytes());
            }
            if part == 0 {
                put(&mut f, l::frontier::FOLD_BELL, bell.to_le_bytes());
                f[l::frontier::FOLD_PART] = 1;
            } else {
                put(&mut f, l::frontier::OCCUPIED_SITES, occ.to_le_bytes());
                for (w, x) in wedge.iter().enumerate() {
                    put(&mut f, l::frontier::WEDGE_OCCUPIED + 4 * w, x.to_le_bytes());
                }
                f[l::frontier::FOLD_PART] = 2;
            }
        }
        2 => {
            if n_accounts(ic)? != 9 {
                return Err(e(BAD_DATA));
            }
            if fb != bell || fp != 2 {
                return Err(e(FOLD_STALE));
            }
            let (mut open, mut provs) = (0u32, 0u32);
            for w in 0..6u8 {
                let fd = owned(
                    ic,
                    3 + w as u16,
                    &season,
                    SeedKind::ProvinceFund,
                    &[w],
                    fclient::abi::magic::PROVINCE_FUND,
                    fclient::abi::size::PROVINCE_FUND,
                )?;
                let o = u32_at(&fd, l::province_fund::OPEN_SITES);
                put(
                    &mut f,
                    l::frontier::WEDGE_OPEN + 4 * w as usize,
                    o.to_le_bytes(),
                );
                open += o;
                provs += u32_at(&fd, l::province_fund::PROVINCES_OPENED);
            }
            put(&mut f, l::frontier::OPEN_SITES, open.to_le_bytes());
            put(&mut f, l::frontier::PROVINCES_OPENED, provs.to_le_bytes());
            f[l::frontier::FOLD_PART] = 0;
        }
        _ => return Err(e(BAD_DATA)),
    }
    let _ = prog;
    write(ic, 2, &f)?;
    let mut pl = vec![0u8; 4 + 24 + 4 + 24 + 4];
    pl[..4].copy_from_slice(&get::<4>(&f, l::frontier::OCCUPIED_SITES));
    log_ps2(ic, &body(Kind::FOLD, bell, &[part], &pl));
    Ok(())
}

// ------------------------------------------------------------------ citizens

pub fn join(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 20_000)?;
    let faction = c.u8()?;
    let session = c.arr::<32>()?;
    let expiry = c.i64()?;
    c.done()?;
    if n_accounts(ic)? != 7 || !is_signer(ic, 0)? || !is_signer(ic, 1)? {
        return Err(e(AUTH));
    }
    let (season, s) = season_at(ic, 2)?;
    let (_, now) = clock(ic)?;
    running(&s, now)?;
    let bell = now_bell(&s, now);
    if bell >= u32_at(&s, l::season::JOIN_CLOSE_BELL) || faction > 5 {
        return Err(e(BAD_DATA));
    }
    if expiry > now + 30 * 86_400 {
        return Err(e(BAD_DATA));
    }
    let prog = program_id(ic)?;
    let wallet = key(ic, 0)?;
    let tag = fa::citizen_tag15(&wallet.to_bytes());
    let shard = fa::join_shard_of(&wallet.to_bytes());
    let mut js = owned(
        ic,
        5,
        &season,
        SeedKind::JoinShard,
        &fa::raw_join_shard(faction, shard),
        fclient::abi::magic::JOIN_SHARD,
        fclient::abi::size::JOIN_SHARD,
    )?;
    let cseed = seed_str(SeedKind::Citizen, &tag);
    init_with_seed(ic, 1, 4, &season, &cseed, fclient::abi::size::CITIZEN, 0)?;
    let caddr = key(ic, 4)?;
    let mut d = vec![0u8; fclient::abi::size::CITIZEN];
    header(&mut d, *fclient::abi::magic::CITIZEN, season.id, true);
    put(&mut d, l::citizen::WALLET, wallet.to_bytes());
    put(&mut d, l::citizen::SESSION, session);
    put(&mut d, l::citizen::SESSION_EXPIRY, expiry.to_le_bytes());
    d[l::citizen::FACTION] = faction;
    d[l::citizen::FLAGS] = l::citizen::FLAG_JOINED;
    d[l::citizen::EXPLORES_FLOOR_LEFT] = 3;
    put(&mut d, l::citizen::JOIN_BELL, bell.to_le_bytes());
    d[l::citizen::JOIN_SHARD] = shard;
    put(&mut d, l::citizen::TICKET_BELL, u32::MAX.to_le_bytes());
    put(
        &mut d,
        l::citizen::CITIZEN_TAG,
        fa::citizen_tag_u64(&caddr).to_le_bytes(),
    );
    put(&mut d, l::citizen::LAST_ACTION_TS, now.to_le_bytes());
    put(&mut d, l::citizen::RENT_PAYER, key(ic, 1)?.to_bytes());
    write(ic, 4, &d)?;
    let m = u32_at(&js, l::join_shard::MEMBERS) + 1;
    put(&mut js, l::join_shard::MEMBERS, m.to_le_bytes());
    write(ic, 5, &js)?;
    let _ = prog;
    let mut pl = wallet.to_bytes().to_vec();
    pl.push(faction);
    pl.push(shard);
    pl.extend_from_slice(&session);
    pl.extend_from_slice(&expiry.to_le_bytes());
    log_ps2(ic, &body(Kind::JOIN, bell, &tag, &pl));
    Ok(())
}

pub fn file_ticket(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 12_000)?;
    let n = c.u8()?;
    if !(1..=3).contains(&n) {
        return Err(e(BAD_DATA));
    }
    let mut sites = vec![];
    for _ in 0..n {
        sites.push((c.i16()?, c.i16()?, c.u8()?));
    }
    c.done()?;
    let mut pl = player(ic)?;
    let prog = program_id(ic)?;
    let f = owned(
        ic,
        4,
        &pl.season,
        SeedKind::Frontier,
        &[],
        fclient::abi::magic::FRONTIER,
        fclient::abi::size::FRONTIER,
    )?;
    let cflags = pl.c[l::citizen::FLAGS];
    if cflags & (l::citizen::FLAG_PROVISIONAL | l::citizen::FLAG_FIRST_FINAL) != 0
        || pl.c[l::citizen::HOLDINGS_N] > 0
        || u32_at(&pl.c, l::citizen::TICKET_BELL) != u32::MAX
    {
        return Err(e(TICKET_STATE));
    }
    let faction = pl.c[l::citizen::FACTION];
    let provs: Vec<(i16, i16)> = {
        let mut v: Vec<(i16, i16)> = vec![];
        for s in &sites {
            if !v.contains(&(s.0, s.1)) {
                v.push((s.0, s.1));
            }
        }
        v
    };
    if n_accounts(ic)? != 5 + provs.len() as u16 + 1 {
        return Err(e(BAD_DATA));
    }
    let opened = u16_at(&f, l::frontier::RINGS_OPENED);
    let mut pds = vec![];
    for (j, &(p, q)) in provs.iter().enumerate() {
        let d = owned(
            ic,
            5 + j as u16,
            &pl.season,
            SeedKind::Province,
            &fa::raw_province(p as i32, q as i32),
            fclient::abi::magic::PROVINCE,
            fclient::abi::size::PROVINCE,
        )?;
        pds.push(d);
    }
    for &(p, q, site) in &sites {
        let pc = ProvinceCoord::new(p as i32, q as i32);
        let ring = pc.ring() as u16;
        if ring < 2 {
            return Err(e(RESERVED_SITE));
        }
        if ring >= opened {
            return Err(e(BAD_DATA));
        }
        let j = provs.iter().position(|x| *x == (p, q)).expect("listed");
        if site >= pds[j][l::province::SITE_COUNT] {
            return Err(e(BAD_DATA));
        }
        if fclient::ix::wedge_of(p as i32, q as i32) != faction {
            return Err(e(BAD_DATA));
        }
    }
    // Cohorts: one per province for this bell.
    for pd in pds.iter_mut() {
        let cs = cohorts(pd);
        let slot = cs
            .iter()
            .find(|c| c.1 == pl.bell && c.2 > 0)
            .or_else(|| {
                cs.iter()
                    .find(|c| c.2 == 0 || c.3 >= c.2 || pl.bell >= c.1.saturating_add(24))
            })
            .copied()
            .ok_or(e(COHORT_FULL))?;
        let (o, b, f, _) = slot;
        if b == pl.bell && f > 0 {
            put(pd, o + 4, (f + 1).to_le_bytes());
        } else {
            put(pd, o, pl.bell.to_le_bytes());
            put(pd, o + 4, 1u16.to_le_bytes());
            put(pd, o + 6, 0u16.to_le_bytes());
        }
    }
    // Escrow of the Holding's rent into the Citizen.
    let hrent = rent(fclient::abi::size::HOLDING);
    let esc = u64_at(&pl.c, l::citizen::TICKET_ESCROW);
    let need = hrent.saturating_sub(esc);
    if need > 0 {
        ic.native_invoke_signed(transfer_ix(key(ic, 1)?, key(ic, 3)?, need), &[])?;
    }
    put(&mut pl.c, l::citizen::TICKET_BELL, pl.bell.to_le_bytes());
    for i in 0..3 {
        let o = l::citizen::TICKET_SITES + i * l::citizen::TICKET_SITE_STRIDE;
        let (p, q, st) = sites.get(i).copied().unwrap_or((0, 0, 0));
        put(&mut pl.c, o, p.to_le_bytes());
        put(&mut pl.c, o + 2, q.to_le_bytes());
        pl.c[o + 4] = st;
    }
    pl.c[l::citizen::TICKET_NEXT] = 0;
    put(&mut pl.c, l::citizen::TICKET_ESCROW, hrent.to_le_bytes());
    put(&mut pl.c, l::citizen::TICKET_FUNDER, key(ic, 1)?.to_bytes());
    put(&mut pl.c, l::citizen::LAST_ACTION_TS, pl.now.to_le_bytes());
    write(ic, 3, &pl.c)?;
    for (j, pd) in pds.iter().enumerate() {
        write(ic, 5 + j as u16, pd)?;
    }
    let _ = prog;
    let wallet: [u8; 32] = get(&pl.c, l::citizen::WALLET);
    let mut body_pl = pl.bell.to_le_bytes().to_vec();
    body_pl.push(n);
    let mut raw = [0u8; 15];
    for (i, &(p, q, st)) in sites.iter().enumerate() {
        raw[5 * i..5 * i + 2].copy_from_slice(&p.to_le_bytes());
        raw[5 * i + 2..5 * i + 4].copy_from_slice(&q.to_le_bytes());
        raw[5 * i + 4] = st;
    }
    body_pl.extend_from_slice(&raw);
    body_pl.extend_from_slice(&hrent.to_le_bytes());
    body_pl.extend_from_slice(key(ic, 1)?.as_ref());
    log_ps2(
        ic,
        &body(Kind::TICKET, pl.bell, &fa::citizen_tag15(&wallet), &body_pl),
    );
    Ok(())
}

/// The seed of `(bell, region)` from `[seedcache|archive] [anchor|archive]`
/// at accounts `i`, `i + 1`: `(seed, round S)`.
fn seed_from(
    ic: &InvokeContext,
    season: &SeasonRef,
    s: &[u8],
    i: u16,
    bell: u32,
    region: u8,
) -> R<([u8; 32], u64)> {
    use frontier_abi::layout::beacon::{
        anchor_archive as arc, bell_anchor as ban, seed_cache as sdc,
    };
    let prog = program_id(ic)?;
    let (k0, k1) = (key(ic, i)?, key(ic, i + 1)?);
    let sc = season_clock(s);
    let archive = addr_of(
        season,
        &prog,
        SeedKind::AnchorArchive,
        &fa::raw_archive(region, fa::archive_part(bell)),
    );
    if k0 == archive && k1 == archive {
        let (o, d, _) = read(ic, i)?;
        let (bo, bm) = arc::bit(arc::ARCHIVED, bell);
        if o != prog || d.len() != arc::SIZE || d[bo] & bm == 0 {
            return Err(e(SEED_NOT_READY));
        }
        let eo = arc::entry(bell);
        let a_off = u32_at(&d, eo) as i64;
        let a = fclient::clock::bell_end(sc.genesis_ts, bell) + a_off;
        return Ok((get(&d, eo + 4), sc.seed_round(bell, a)));
    }
    let anchor = addr_of(
        season,
        &prog,
        SeedKind::BellAnchor,
        &fa::raw_anchor(bell, region),
    );
    if k1 != anchor {
        return Err(e(BAD_ADDRESS));
    }
    let (ao, ad, _) = read(ic, i + 1)?;
    if ao != prog || ad.len() != ban::SIZE {
        return Err(e(SEED_NOT_READY));
    }
    let (co, cd, _) = read(ic, i)?;
    if co != prog
        || cd.len() != sdc::SIZE
        || get::<32>(&cd, sdc::ANCHOR_KEY) != anchor.to_bytes()
        || u32_at(&cd, sdc::BELL) != bell
    {
        return Err(e(SEED_NOT_READY));
    }
    let nonce = cd[sdc::NONCE];
    if addr_of(
        season,
        &prog,
        SeedKind::SeedCache,
        &fa::raw_seed_cache(bell, region, nonce),
    ) != k0
    {
        return Err(e(BAD_ADDRESS));
    }
    let round = u64_at(&cd, sdc::ROUND);
    if round != sc.seed_round(bell, i64_at(&ad, ban::A)) {
        return Err(e(SEED_NOT_READY));
    }
    Ok((get(&cd, sdc::SEED), round))
}

pub fn settle_ticket(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 30_000)?;
    let k = c.u8()?;
    c.done()?;
    if !is_signer(ic, 0)? {
        return Err(e(AUTH));
    }
    let (season, s) = season_at(ic, 1)?;
    let (_, now) = clock(ic)?;
    running(&s, now)?;
    let prog = program_id(ic)?;
    let bell = now_bell(&s, now);
    // The citizen and its ticket.
    let (co, mut cz, _) = read(ic, 2)?;
    if co != prog || cz.len() != fclient::abi::size::CITIZEN {
        return Err(e(BAD_ACCOUNT));
    }
    let wallet: [u8; 32] = get(&cz, l::citizen::WALLET);
    if addr_of(
        &season,
        &prog,
        SeedKind::Citizen,
        &fa::citizen_tag15(&wallet),
    ) != key(ic, 2)?
    {
        return Err(e(BAD_ADDRESS));
    }
    let tb = u32_at(&cz, l::citizen::TICKET_BELL);
    if tb == u32::MAX {
        return Err(e(NO_TICKET));
    }
    if k != cz[l::citizen::TICKET_NEXT] {
        return Err(e(TICKET_STATE));
    }
    let sites: Vec<(i16, i16, u8)> = (0..3)
        .map(|i| {
            let o = l::citizen::TICKET_SITES + i * l::citizen::TICKET_SITE_STRIDE;
            (i16_at(&cz, o), i16_at(&cz, o + 2), cz[o + 4])
        })
        .take_while(|x| (x.0, x.1) != (0, 0))
        .collect();
    let (p, q, site) = *sites.get(k as usize).ok_or(e(TICKET_STATE))?;
    let mut provs: Vec<(i16, i16)> = vec![];
    for s2 in &sites {
        if !provs.contains(&(s2.0, s2.1)) {
            provs.push((s2.0, s2.1));
        }
    }
    let others: Vec<(i16, i16)> = provs.iter().copied().filter(|x| *x != (p, q)).collect();
    let n_acc = n_accounts(ic)?;
    let base = 8 + others.len() as u16;
    let displaced_given = match n_acc - base {
        1 => false,
        4 => true,
        _ => return Err(e(BAD_DATA)),
    };
    // The site's accounts.
    let (pi, qi) = (p as i32, q as i32);
    let mut pv = owned(
        ic,
        4,
        &season,
        SeedKind::Province,
        &fa::raw_province(pi, qi),
        fclient::abi::magic::PROVINCE,
        fclient::abi::size::PROVINCE,
    )?;
    let faction = cz[l::citizen::FACTION];
    let shard = fa::join_shard_of(&wallet);
    let mut js = owned(
        ic,
        5,
        &season,
        SeedKind::JoinShard,
        &fa::raw_join_shard(faction, shard),
        fclient::abi::magic::JOIN_SHARD,
        fclient::abi::size::JOIN_SHARD,
    )?;
    let hraw = fa::raw_holding(pi, qi, site);
    if addr_of(&season, &prog, SeedKind::Holding, &hraw) != key(ic, 3)? {
        return Err(e(BAD_ADDRESS));
    }
    let mut other_pv = vec![];
    for (j, &(op, oq)) in others.iter().enumerate() {
        other_pv.push(owned(
            ic,
            8 + j as u16,
            &season,
            SeedKind::Province,
            &fa::raw_province(op as i32, oq as i32),
            fclient::abi::magic::PROVINCE,
            fclient::abi::size::PROVINCE,
        )?);
    }
    let ctag = fa::citizen_tag_u64(&key(ic, 2)?);
    let region = fclient::ix::region_of(pi, qi);
    let mut outcome = 3u8;
    let mut score = 0u64;
    let mut displaced_tag = 0u64;
    let mut gen = 0u8;
    let mut final_ts = 0i64;
    let mut ends = false;
    if bell >= tb.saturating_add(24) {
        if displaced_given {
            return Err(e(BAD_DATA));
        }
        ends = true;
    } else {
        let (seed, round) = seed_from(ic, &season, &s, 6, tb, region)?;
        score = fclient::land::ticket_score(&seed, pi, qi, site, ctag);
        let mo = l::province::SITE_MIRROR + site as usize * l::province::SITE_MIRROR_STRIDE;
        let mstate = pv[mo + l::site::STATE];
        let (ho, hd, hlam) = read(ic, 3)?;
        final_ts = season_clock(&s).drand.round_time(round) + 600;
        outcome = 2; // taken
        if mstate == l::site::STATE_FREE || mstate == l::site::STATE_RELEASED_FREE {
            if !is_absent(&ho, &hd) {
                return Err(e(BAD_ACCOUNT));
            }
            outcome = 0;
            // Every founding bumps the site's gen, the first holding of a
            // site included (gen 1), as the program does (W3-A's pinned
            // choice, citizen.rs module doc; integ-W3).
            gen = pv[mo + l::site::GEN].wrapping_add(1);
        } else if mstate == l::site::STATE_HOLDING
            && ho == prog
            && hd.len() == fclient::abi::size::HOLDING
            && hd[l::holding::STATE] == l::holding::STATE_PROVISIONAL
            && u32_at(&hd, l::holding::TICKET_BELL) == tb
        {
            let their = u64_at(&hd, l::holding::TICKET_SCORE);
            let their_c = Address::new_from_array(get(&hd, l::holding::OWNER_CITIZEN));
            let their_tag = fa::citizen_tag_u64(&their_c);
            if fclient::land::beats(score, ctag, their, their_tag) {
                outcome = 1;
                displaced_tag = their_tag;
                gen = hd[l::holding::GEN].wrapping_add(1);
            }
        }
        if (outcome == 1) != displaced_given {
            return Err(e(BAD_DATA));
        }
        let hrent = rent(fclient::abi::size::HOLDING);
        let funder = Address::new_from_array(get(&cz, l::citizen::TICKET_FUNDER));
        let esc = u64_at(&cz, l::citizen::TICKET_ESCROW);
        // DECISIONS K9: a fresh settlement waits while an earlier cohort
        // of this Province is open (the model refuses `TicketState`; the
        // program's code is the integrator's choice).
        if outcome == 0
            && cohorts(&pv)
                .iter()
                .any(|c| c.2 > 0 && c.1 < tb && c.3 < c.2 && bell < c.1.saturating_add(24))
        {
            return Err(e(TICKET_STATE));
        }
        match outcome {
            0 => {
                // Fresh: the Holding funded from the Citizen's escrow.
                let need = hrent.saturating_sub(hlam);
                if esc < need {
                    return Err(e(INSUFFICIENT));
                }
                move_lamports(ic, 2, 3, need)?;
                let (id, bump) = season.seeds();
                let hseed = seed_str(SeedKind::Holding, &hraw);
                ic.native_invoke_signed(
                    super::allocate_with_seed_ix(
                        key(ic, 3)?,
                        season.key,
                        &hseed,
                        fclient::abi::size::HOLDING as u64,
                        prog,
                    ),
                    &[&[frontier_abi::layout::world::season::PDA_PREFIX, &id, &bump]],
                )?;
                // Any escrow above the Holding's shortfall stays for refunds.
                put(
                    &mut cz,
                    l::citizen::TICKET_ESCROW,
                    (esc - need).to_le_bytes(),
                );
            }
            1 => {
                // Displace: the displaced holder's accounts.
                let dcz_i = base + 1;
                let their_c = Address::new_from_array(get(&hd, l::holding::OWNER_CITIZEN));
                if key(ic, dcz_i)? != their_c {
                    return Err(e(BAD_ACCOUNT));
                }
                let old_payer = Address::new_from_array(get(&hd, l::holding::RENT_PAYER));
                if key(ic, base)? != old_payer {
                    return Err(e(BAD_ACCOUNT));
                }
                // The new escrow pays the old rent payer back.
                move_lamports(ic, 2, base, esc.min(hrent))?;
                put(
                    &mut cz,
                    l::citizen::TICKET_ESCROW,
                    esc.saturating_sub(hrent).to_le_bytes(),
                );
                let (_, mut dcz, _) = read(ic, dcz_i)?;
                let dwallet: [u8; 32] = get(&dcz, l::citizen::WALLET);
                let dfac = dcz[l::citizen::FACTION];
                let mut djs = owned(
                    ic,
                    base + 2,
                    &season,
                    SeedKind::JoinShard,
                    &fa::raw_join_shard(dfac, fa::join_shard_of(&dwallet)),
                    fclient::abi::magic::JOIN_SHARD,
                    fclient::abi::size::JOIN_SHARD,
                )?;
                dcz[l::citizen::FLAGS] &= !l::citizen::FLAG_PROVISIONAL;
                dcz[l::citizen::HOLDINGS_N] = 0;
                for b in &mut dcz[l::citizen::HOLDING..l::citizen::HOLDING + 6] {
                    *b = 0;
                }
                write(ic, dcz_i, &dcz)?;
                let x = u32_at(&djs, l::join_shard::HOLDINGS).saturating_sub(1);
                put(&mut djs, l::join_shard::HOLDINGS, x.to_le_bytes());
                let w = fclient::ix::wedge_of(pi, qi) as usize;
                let o = l::join_shard::HOLDINGS_BY_WEDGE + 4 * w;
                let x = u32_at(&djs, o).saturating_sub(1);
                put(&mut djs, o, x.to_le_bytes());
                // Same shard (both of one faction and shard): write through ours.
                if key(ic, base + 2)? == key(ic, 5)? {
                    js = djs;
                } else {
                    write(ic, base + 2, &djs)?;
                }
            }
            _ => {}
        }
        if outcome == 0 || outcome == 1 {
            let mut h = vec![0u8; fclient::abi::size::HOLDING];
            header(&mut h, *fclient::abi::magic::HOLDING, season.id, true);
            put(&mut h, l::holding::P, p.to_le_bytes());
            put(&mut h, l::holding::Q, q.to_le_bytes());
            h[l::holding::SITE] = site;
            h[l::holding::GEN] = gen;
            h[l::holding::TILE] = pv[l::province::SITES + site as usize];
            h[l::holding::STATE] = l::holding::STATE_PROVISIONAL;
            put(&mut h, l::holding::OWNER_CITIZEN, key(ic, 2)?.to_bytes());
            put(&mut h, l::holding::TICKET_SCORE, score.to_le_bytes());
            h[l::holding::FACTION] = faction;
            h[l::holding::ORDER] = 1;
            put(&mut h, l::holding::TICKET_BELL, tb.to_le_bytes());
            put(&mut h, l::holding::FOUNDED_TS, now.to_le_bytes());
            put(&mut h, l::holding::FOUNDED_DAY, (bell / 144).to_le_bytes());
            put(&mut h, l::holding::LAST_OWNER_ACTION, now.to_le_bytes());
            put(&mut h, l::holding::RENT_PAYER, funder.to_bytes());
            put(&mut h, l::holding::FINAL_TS, final_ts.to_le_bytes());
            write(ic, 3, &h)?;
            let mo = l::province::SITE_MIRROR + site as usize * l::province::SITE_MIRROR_STRIDE;
            pv[mo + l::site::STATE] = l::site::STATE_HOLDING;
            pv[mo + l::site::FACTION] = faction;
            pv[mo + l::site::ORDER] = 1;
            pv[mo + l::site::GEN] = gen;
            let ep = u32_at(&pv, l::province::ROSTER_EPOCH) + 1;
            put(&mut pv, l::province::ROSTER_EPOCH, ep.to_le_bytes());
            cz[l::citizen::FLAGS] |= l::citizen::FLAG_PROVISIONAL;
            cz[l::citizen::HOLDINGS_N] = 1;
            let o = l::citizen::HOLDING;
            put(&mut cz, o, p.to_le_bytes());
            put(&mut cz, o + 2, q.to_le_bytes());
            cz[o + 4] = site;
            cz[o + 5] = gen;
            let x = u32_at(&js, l::join_shard::HOLDINGS) + 1;
            put(&mut js, l::join_shard::HOLDINGS, x.to_le_bytes());
            let w = fclient::ix::wedge_of(pi, qi) as usize;
            let o = l::join_shard::HOLDINGS_BY_WEDGE + 4 * w;
            let x = u32_at(&js, o) + 1;
            put(&mut js, o, x.to_le_bytes());
            ends = true;
        } else {
            let next = k + 1;
            cz[l::citizen::TICKET_NEXT] = next;
            if next as usize >= sites.len() {
                ends = true;
            }
        }
    }
    if ends {
        put(&mut cz, l::citizen::TICKET_BELL, u32::MAX.to_le_bytes());
        settle_cohort(&mut pv, tb);
        for d in other_pv.iter_mut() {
            settle_cohort(d, tb);
        }
    }
    write(ic, 2, &cz)?;
    write(ic, 4, &pv)?;
    write(ic, 5, &js)?;
    if ends {
        for (j, d) in other_pv.iter().enumerate() {
            write(ic, 8 + j as u16, d)?;
        }
    }
    let mut pl = vec![outcome];
    pl.extend_from_slice(&ctag.to_le_bytes());
    pl.extend_from_slice(&score.to_le_bytes());
    pl.extend_from_slice(&displaced_tag.to_le_bytes());
    pl.push(gen);
    pl.extend_from_slice(&final_ts.to_le_bytes());
    pl.extend_from_slice(&tb.to_le_bytes());
    log_ps2(ic, &body(Kind::SETTLE, bell, &pqs(p, q, site), &pl));
    Ok(())
}

pub fn release_dormant(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 20_000)?;
    c.done()?;
    if n_accounts(ic)? != 8 || !is_signer(ic, 0)? {
        return Err(e(BAD_DATA));
    }
    let (season, s) = season_at(ic, 1)?;
    let (_, now) = clock(ic)?;
    running(&s, now)?;
    let prog = program_id(ic)?;
    let (ho, h, hlam) = read(ic, 2)?;
    if ho != prog || h.len() != fclient::abi::size::HOLDING {
        return Err(e(BAD_ACCOUNT));
    }
    let (p, q, site) = (
        i16_at(&h, l::holding::P),
        i16_at(&h, l::holding::Q),
        h[l::holding::SITE],
    );
    let (pi, qi) = (p as i32, q as i32);
    if addr_of(
        &season,
        &prog,
        SeedKind::Holding,
        &fa::raw_holding(pi, qi, site),
    ) != key(ic, 2)?
    {
        return Err(e(BAD_ADDRESS));
    }
    if h[l::holding::ORDER] != 1
        || now
            < i64_at(&h, l::holding::LAST_OWNER_ACTION)
                + u32_at(&s, l::season::RELEASE_AFTER_SECS) as i64
    {
        return Err(e(NOT_DORMANT));
    }
    for i in 0..4 {
        let st = h[l::holding::TRANSIT + i * l::holding::TRANSIT_STRIDE];
        if (1..=3).contains(&st) {
            return Err(e(NOT_DORMANT));
        }
    }
    let mut pv = owned(
        ic,
        3,
        &season,
        SeedKind::Province,
        &fa::raw_province(pi, qi),
        fclient::abi::magic::PROVINCE,
        fclient::abi::size::PROVINCE,
    )?;
    if key(ic, 4)?.to_bytes() != get::<32>(&h, l::holding::OWNER_CITIZEN) {
        return Err(e(BAD_ACCOUNT));
    }
    let (_, mut cz, _) = read(ic, 4)?;
    let wallet: [u8; 32] = get(&cz, l::citizen::WALLET);
    let fac = cz[l::citizen::FACTION];
    let mut js = owned(
        ic,
        5,
        &season,
        SeedKind::JoinShard,
        &fa::raw_join_shard(fac, fa::join_shard_of(&wallet)),
        fclient::abi::magic::JOIN_SHARD,
        fclient::abi::size::JOIN_SHARD,
    )?;
    if key(ic, 6)?.to_bytes() != get::<32>(&h, l::holding::RENT_PAYER) {
        return Err(e(BAD_ACCOUNT));
    }
    if addr_of(&season, &prog, SeedKind::DefencePool, &[]) != key(ic, 7)? {
        return Err(e(BAD_ADDRESS));
    }
    let mo = l::province::SITE_MIRROR + site as usize * l::province::SITE_MIRROR_STRIDE;
    pv[mo + l::site::STATE] = l::site::STATE_RELEASED_FREE;
    let ep = u32_at(&pv, l::province::ROSTER_EPOCH) + 1;
    put(&mut pv, l::province::ROSTER_EPOCH, ep.to_le_bytes());
    write(ic, 3, &pv)?;
    cz[l::citizen::FLAGS] |= l::citizen::FLAG_REFUGEE;
    cz[l::citizen::FLAGS] &= !(l::citizen::FLAG_PROVISIONAL | l::citizen::FLAG_FIRST_FINAL);
    cz[l::citizen::HOLDINGS_N] = 0;
    for b in &mut cz[l::citizen::HOLDING..l::citizen::HOLDING + 6] {
        *b = 0;
    }
    write(ic, 4, &cz)?;
    let x = u32_at(&js, l::join_shard::HOLDINGS).saturating_sub(1);
    put(&mut js, l::join_shard::HOLDINGS, x.to_le_bytes());
    let w = fclient::ix::wedge_of(pi, qi) as usize;
    let o = l::join_shard::HOLDINGS_BY_WEDGE + 4 * w;
    let x = u32_at(&js, o).saturating_sub(1);
    put(&mut js, o, x.to_le_bytes());
    let x = u32_at(&js, l::join_shard::RELEASED) + 1;
    put(&mut js, l::join_shard::RELEASED, x.to_le_bytes());
    write(ic, 5, &js)?;
    // pool_owed → the pool, the rest (rent) → the rent payer; close.
    let owed = u64_at(&h, l::holding::POOL_OWED).min(hlam);
    if owed > 0 {
        move_lamports(ic, 2, 7, owed)?;
    }
    super::close(ic, 2, 6)?;
    let bell = now_bell(&s, now);
    log_ps2(
        ic,
        &body(
            Kind::RELEASE,
            bell,
            &pqs(p, q, site),
            &cz[l::citizen::CITIZEN_TAG..l::citizen::CITIZEN_TAG + 8],
        ),
    );
    Ok(())
}

// ------------------------------------------------------------------ holdings

pub fn muster(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 20_000)?;
    let unit = c.u8()?;
    let troops = c.u32()?;
    let tile = c.u8()?;
    c.done()?;
    let mut pl = player(ic)?;
    let mut h = own_holding(ic, &pl, 4)?;
    let (p, q, site) = (
        i16_at(&h, l::holding::P),
        i16_at(&h, l::holding::Q),
        h[l::holding::SITE],
    );
    let mut pv = owned(
        ic,
        5,
        &pl.season,
        SeedKind::Province,
        &fa::raw_province(p as i32, q as i32),
        fclient::abi::magic::PROVINCE,
        fclient::abi::size::PROVINCE,
    )?;
    flip_final(&mut h, &mut pl.c, &pv, pl.now, pl.bell);
    if h[l::holding::STATE] != l::holding::STATE_FINAL {
        return Err(e(NOT_FINAL));
    }
    let free = (0..fclient::abi::ENTRIES)
        .find(|&i| pv[l::province::ENTRIES + i * l::province::ENTRY_STRIDE + l::entry::STATE] == 0)
        .ok_or(e(PROVINCE_FULL))?;
    // The program's numbering (W3-B): the host takes `host_seq`, then
    // `host_seq += 1`.
    let seq = u32_at(&h, l::holding::HOST_SEQ);
    put(&mut h, l::holding::HOST_SEQ, (seq + 1).to_le_bytes());
    let id =
        fa::host_id(p as i32, q as i32, site, h[l::holding::GEN], seq).map_err(|_| e(BAD_DATA))?;
    let o = l::province::ENTRIES + free * l::province::ENTRY_STRIDE;
    put(&mut pv, o + l::entry::ID, id.to_le_bytes());
    pv[o + l::entry::FACTION] = h[l::holding::FACTION];
    pv[o + l::entry::UNIT] = unit;
    pv[o + l::entry::TILE] = tile;
    pv[o + l::entry::STATE] = l::entry::STATE_ROSTER;
    put(
        &mut pv,
        o + l::entry::TROOPS,
        (troops * 1_000).to_le_bytes(),
    );
    put(&mut pv, o + l::entry::FROM_BELL, pl.bell.to_le_bytes());
    pv[l::province::N_ENTRIES] += 1;
    put(&mut h, l::holding::LAST_OWNER_ACTION, pl.now.to_le_bytes());
    write(ic, 4, &h)?;
    write(ic, 5, &pv)?;
    write(ic, 3, &pl.c)?;
    let mut body_pl = vec![unit];
    body_pl.extend_from_slice(&troops.to_le_bytes());
    body_pl.push(tile);
    body_pl.push(free as u8);
    log_ps2(
        ic,
        &body(Kind::MUSTER, pl.bell, &id.to_le_bytes(), &body_pl),
    );
    Ok(())
}

pub fn explore(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 15_000)?;
    let host = c.u64()?;
    let n = c.u8()?;
    let t0 = c.u8()?;
    let t1 = c.u8()?;
    c.done()?;
    let mut pl = player(ic)?;
    let mut h = own_holding(ic, &pl, 4)?;
    let prog = program_id(ic)?;
    let (o5, mut pv, _) = read(ic, 5)?;
    if o5 != prog || pv.len() != fclient::abi::size::PROVINCE {
        return Err(e(BAD_ACCOUNT));
    }
    let (pp, pq) = (i16_at(&pv, l::province::P), i16_at(&pv, l::province::Q));
    let home = {
        let (hp, hq) = (i16_at(&h, l::holding::P), i16_at(&h, l::holding::Q));
        (hp, hq)
    };
    if home == (pp, pq) {
        let pvc = pv.clone();
        flip_final(&mut h, &mut pl.c, &pvc, pl.now, pl.bell);
    }
    if h[l::holding::STATE] != l::holding::STATE_FINAL {
        return Err(e(NOT_FINAL));
    }
    let addrs = fa::Addresses::new(prog, pl.season.id);
    if addrs.holding_of_host(host).ok() != Some(key(ic, 4)?) {
        return Err(e(NOT_OWNER));
    }
    let at = (0..fclient::abi::ENTRIES).find(|&i| {
        let o = l::province::ENTRIES + i * l::province::ENTRY_STRIDE;
        u64_at(&pv, o + l::entry::ID) == host && pv[o + l::entry::STATE] == 1
    });
    if at.is_none() {
        return Err(e(26)); // NotResident
    }
    let ex = l::holding::EXPLORE;
    if h[ex + l::explore::STATE] != 0 {
        return Err(e(28)); // HostBusy (record in use)
    }
    let tiles = if n == 2 { vec![t0, t1] } else { vec![t0] };
    let mut mask = u64_at(&pv, l::province::EXPLORED_MASK);
    for &t in &tiles {
        if t >= 61 || mask & (1 << t) != 0 {
            return Err(e(EXPLORED));
        }
        mask |= 1 << t;
    }
    put(&mut pv, l::province::EXPLORED_MASK, mask.to_le_bytes());
    put(&mut h, ex + l::explore::BELL, pl.bell.to_le_bytes());
    put(&mut h, ex + l::explore::P, pp.to_le_bytes());
    put(&mut h, ex + l::explore::Q, pq.to_le_bytes());
    h[ex + l::explore::TILES] = t0;
    h[ex + l::explore::TILES + 1] = if n == 2 { t1 } else { 0xFF };
    put(&mut h, ex + l::explore::HOST, host.to_le_bytes());
    h[ex + l::explore::STATE] = 1;
    put(&mut h, l::holding::LAST_OWNER_ACTION, pl.now.to_le_bytes());
    write(ic, 4, &h)?;
    write(ic, 5, &pv)?;
    write(ic, 3, &pl.c)?;
    let mut body_pl = (pp as i32).to_le_bytes().to_vec();
    body_pl.extend_from_slice(&(pq as i32).to_le_bytes());
    body_pl.push(n);
    body_pl.push(t0);
    body_pl.push(if n == 2 { t1 } else { 0xFF });
    log_ps2(
        ic,
        &body(Kind::EXPLORE, pl.bell, &host.to_le_bytes(), &body_pl),
    );
    Ok(())
}

pub fn settle_explore(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 12_000)?;
    c.done()?;
    if n_accounts(ic)? != 6 || !is_signer(ic, 0)? {
        return Err(e(BAD_DATA));
    }
    let (season, s) = season_at(ic, 1)?;
    let (_, now) = clock(ic)?;
    let eff = effective(&s, now);
    if eff != RUNNING && eff != ENDED {
        return Err(e(WRONG_STATUS));
    }
    let prog = program_id(ic)?;
    let (ho, mut h, _) = read(ic, 2)?;
    if ho != prog || h.len() != fclient::abi::size::HOLDING {
        return Err(e(BAD_ACCOUNT));
    }
    let ex = l::holding::EXPLORE;
    if h[ex + l::explore::STATE] != 1 {
        return Err(e(52)); // AlreadyDone
    }
    if key(ic, 3)?.to_bytes() != get::<32>(&h, l::holding::OWNER_CITIZEN) {
        return Err(e(BAD_ACCOUNT));
    }
    let (_, mut cz, _) = read(ic, 3)?;
    let bell = u32_at(&h, ex + l::explore::BELL);
    let (p, q) = (
        i16_at(&h, ex + l::explore::P),
        i16_at(&h, ex + l::explore::Q),
    );
    let region = fclient::ix::region_of(p as i32, q as i32);
    let (seed, _) = seed_from(ic, &season, &s, 4, bell, region)?;
    let host = u64_at(&h, ex + l::explore::HOST);
    let floor_left = cz[l::citizen::EXPLORES_FLOOR_LEFT];
    let floor = floor_left > 0;
    let pc = ProvinceCoord::new(p as i32, q as i32);
    let mut works = 0u32;
    let mut per = [0u8; 8];
    for (i, t) in [h[ex + l::explore::TILES], h[ex + l::explore::TILES + 1]]
        .into_iter()
        .enumerate()
    {
        if t == 0xFF {
            continue;
        }
        let f = permutation_rules::frontier::explore::roll(&seed, pc, t, host, floor);
        per[4 * i..4 * i + 4].copy_from_slice(&f.works.to_le_bytes());
        works += f.works;
    }
    if floor {
        cz[l::citizen::EXPLORES_FLOOR_LEFT] = floor_left - 1;
    }
    let w = u64_at(&cz, l::citizen::WORKS) + works as u64;
    put(&mut cz, l::citizen::WORKS, w.to_le_bytes());
    let x = u32_at(&cz, l::citizen::EXPLORES) + 1;
    put(&mut cz, l::citizen::EXPLORES, x.to_le_bytes());
    for b in &mut h[ex..ex + 24] {
        *b = 0;
    }
    write(ic, 2, &h)?;
    write(ic, 3, &cz)?;
    let mut pl = per.to_vec();
    pl.extend_from_slice(&works.to_le_bytes());
    pl.push(floor as u8);
    log_ps2(
        ic,
        &body(
            Kind::EXPLORE_RESULT,
            now_bell(&s, now),
            &host.to_le_bytes(),
            &pl,
        ),
    );
    Ok(())
}

pub fn disband_stranded(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 8_000)?;
    let entry = c.u8()? as usize;
    c.done()?;
    if n_accounts(ic)? != 4 || !is_signer(ic, 0)? {
        return Err(e(BAD_DATA));
    }
    let (season, s) = season_at(ic, 1)?;
    let (_, now) = clock(ic)?;
    let eff = effective(&s, now);
    if eff != RUNNING && eff != ENDED {
        return Err(e(WRONG_STATUS));
    }
    let prog = program_id(ic)?;
    let (po, mut pv, _) = read(ic, 2)?;
    if po != prog || pv.len() != fclient::abi::size::PROVINCE || entry >= fclient::abi::ENTRIES {
        return Err(e(BAD_ACCOUNT));
    }
    let raw = fa::raw_province(
        i16_at(&pv, l::province::P) as i32,
        i16_at(&pv, l::province::Q) as i32,
    );
    if addr_of(&season, &prog, SeedKind::Province, &raw) != key(ic, 2)? {
        return Err(e(BAD_ADDRESS));
    }
    let o = l::province::ENTRIES + entry * l::province::ENTRY_STRIDE;
    if pv[o + l::entry::STATE] == 0 {
        return Err(e(52));
    }
    let id = u64_at(&pv, o + l::entry::ID);
    let want = fa::Addresses::new(prog, season.id)
        .holding_of_host(id)
        .map_err(|_| e(BAD_DATA))?;
    if want != key(ic, 3)? {
        return Err(e(BAD_ADDRESS));
    }
    let (_, _, _, gen, _) = fa::host_parts(id).map_err(|_| e(BAD_DATA))?;
    let (ho, hd, _) = read(ic, 3)?;
    let stranded = is_absent(&ho, &hd)
        || (ho == prog && hd.len() == fclient::abi::size::HOLDING && hd[l::holding::GEN] != gen);
    if !stranded {
        return Err(e(NOT_DORMANT));
    }
    let troops = u32_at(&pv, o + l::entry::TROOPS);
    // v1.5 §5.10 (the program's rule): a host in a roster (state 1 or 2)
    // gets the pending op Forfeit of now_bell, freed by that bell's
    // resolve (the model has none, so it stays pending); others are freed.
    let st = pv[o + l::entry::STATE];
    if (st == 1 || st == 2)
        && u32_at(&pv, l::province::RESOLVED_NEXT) < u32_at(&s, l::season::END_BELL)
    {
        if pv[o + l::entry::PEND_OP] != 0 {
            return Err(e(HOST_BUSY));
        }
        pv[o + l::entry::PEND_OP] = 6;
        put(
            &mut pv,
            o + l::entry::PEND_BELL,
            now_bell(&s, now).to_le_bytes(),
        );
    } else {
        for b in &mut pv[o..o + l::province::ENTRY_STRIDE] {
            *b = 0;
        }
        pv[l::province::N_ENTRIES] = pv[l::province::N_ENTRIES].saturating_sub(1);
    }
    let ep = u32_at(&pv, l::province::ROSTER_EPOCH) + 1;
    put(&mut pv, l::province::ROSTER_EPOCH, ep.to_le_bytes());
    write(ic, 2, &pv)?;
    log_ps2(
        ic,
        &body(
            Kind::STRANDED,
            now_bell(&s, now),
            &id.to_le_bytes(),
            &troops.to_le_bytes(),
        ),
    );
    Ok(())
}

pub fn sweep_pool_owed(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 5_000)?;
    c.done()?;
    if n_accounts(ic)? != 4 || !is_signer(ic, 0)? {
        return Err(e(BAD_DATA));
    }
    let (season, s) = season_at(ic, 1)?;
    let (_, now) = clock(ic)?;
    let prog = program_id(ic)?;
    let (ho, mut h, _) = read(ic, 2)?;
    if ho != prog || h.len() != fclient::abi::size::HOLDING {
        return Err(e(BAD_ACCOUNT));
    }
    let raw = fa::raw_holding(
        i16_at(&h, l::holding::P) as i32,
        i16_at(&h, l::holding::Q) as i32,
        h[l::holding::SITE],
    );
    if addr_of(&season, &prog, SeedKind::Holding, &raw) != key(ic, 2)? {
        return Err(e(BAD_ADDRESS));
    }
    if addr_of(&season, &prog, SeedKind::DefencePool, &[]) != key(ic, 3)? {
        return Err(e(BAD_ADDRESS));
    }
    let owed = u64_at(&h, l::holding::POOL_OWED);
    if owed == 0 {
        return Err(e(52));
    }
    move_lamports(ic, 2, 3, owed)?;
    put(&mut h, l::holding::POOL_OWED, 0u64.to_le_bytes());
    write(ic, 2, &h)?;
    log_ps2(
        ic,
        &body(
            Kind::POOL_SWEEP,
            now_bell(&s, now),
            &raw[..9],
            &owed.to_le_bytes(),
        ),
    );
    Ok(())
}
