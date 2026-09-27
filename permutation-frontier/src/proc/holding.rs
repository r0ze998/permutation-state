//! Holdings (M1 contract §5.10): Harvest (0x40), Build (0x41), Train
//! (0x42), Explore (0x46), SettleExplore (0x47). Implemented by W3-B.
//!
//! This file also carries what the resident actions of `host.rs` share
//! (the player prologue of §5.6 over `AccountInfo`s, the Holding ↔ kernel
//! codec and the holding touch), so W3-B's three files need no edit of a
//! file another unit owns (§11).
//!
//! ## Pinned here (recorded in `W3-B-NOTES.md`)
//!
//! - **Holding ↔ kernel `holding::Holding`.** `tier` u8 = `Tier` in
//!   declaration order (0 Hamlet … 3 Stronghold); queue item `kind` 0 free,
//!   1 `Production {resource = arg, delta}`, 2 `Upkeep {resource = arg,
//!   delta}`, 3 `TierUp`, 4 `Walls {delta}`; every other field one to one.
//! - **The holding touch** (every owner action, in this order): the
//!   kernel `settle` split at each finished `TierUp` so the new tier's base
//!   production (`catalog::base_production`, the simulator's
//!   `apply_tier_bonus`) starts when the tier does; `touch_owner(now)`;
//!   `commit_walls(now)`; `shield_until` and the dormant-flag cache
//!   refreshed. The verifier replays the same sequence.
//! - **Build items:** 0–5 the six `catalog::BUILDINGS`, 6 walls
//!   (`catalog::ITEM_WALLS`), **7 the tier-up** (`catalog::tier_up_item`,
//!   one at a time). A building's copy number `n` is derived from the
//!   Holding: `1 + (production[r] − base_production(tier)[r]) / per_hour +
//!   queued copies` (the six kinds produce six distinct resources, and a
//!   first holding is founded with `production = base_production(Hamlet)`,
//!   as the simulator's `found`).
//! - **Train** credits `reserve[unit]` in whole troops (I-56).
//! - **Explore / SettleExplore:** each explored tile is one exploration;
//!   the floor (`explores_floor_left`) is spent tile by tile.

use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use frontier_abi::addr::archive_part_of;
use frontier_abi::ix as aix;
use frontier_abi::layout::AccountKind;
use frontier_abi::log::{EntityKind, Kind};
use frontier_abi::prologue::{self as ap, HoldingHdr, PlayerCtx};
use frontier_abi::tags::Ix;
use permutation_rules::fixed::MILLI;
use permutation_rules::frontier::catalog;
use permutation_rules::frontier::geometry::{tile_offset, ProvinceCoord};
use permutation_rules::frontier::holding::{
    Accrual, Effect, Holding as KHolding, HoldingError, QueueItem, Resource, Tier, QUEUE_SLOTS,
    RESOURCES,
};
use permutation_rules::hex::Hex;

use crate::addr::{self, AddrCtx};
use crate::clock::SeasonClock;
use crate::error::{kernel, BAD_ACCOUNT, OVERFLOW};
use crate::events::{self, Buf, Chained};
use crate::layout::beacon::{archive_archived, archive_entry_of, archive_key, Anchor, Cache};
use crate::layout::player::{accrual, queue_item as QI};
use crate::layout::{
    citizen as C, explore as X, holding as H, province as P, season as S, site as SM, Ro, Rw,
};
use crate::prologue::{self, check_accounts, expect_key, key, view, Now};
use crate::{FrontierError, R, RULESET_HASH};

// ------------------------------------------------------------ kernel sub-codes

/// `Kernel` (15) sub-codes of this area (logged with `sol_log_64`).
pub mod sub {
    /// `HoldingError::TimeReversed`.
    pub const HOLDING_TIME_REVERSED: u64 = 0x10;
    /// `HoldingError::TopTier` (tier-up at Stronghold).
    pub const HOLDING_TOP_TIER: u64 = 0x11;
    /// `HoldingError::AboveCap` (CL-02 caps, e.g. walls above 1,200).
    pub const HOLDING_ABOVE_CAP: u64 = 0x12;
    /// `HostError::TooSmall` (muster below 100 troops).
    pub const HOST_TOO_SMALL: u64 = 0x20;
    /// `HostError::TooLarge` (muster above 30,000 troops; a garrison past
    /// 30,000).
    pub const HOST_TOO_LARGE: u64 = 0x21;
    /// `HostError::NotAHostUnit`.
    pub const HOST_NOT_A_HOST_UNIT: u64 = 0x22;
    /// `HostError::Mismatch`.
    pub const HOST_MISMATCH: u64 = 0x23;
    /// `HostError::TimeReversed`.
    pub const HOST_TIME_REVERSED: u64 = 0x24;
}

/// A kernel holding refusal as a program code.
pub(crate) fn holding_err(e: HoldingError) -> crate::Error {
    match e {
        HoldingError::Insufficient => FrontierError::Insufficient.into(),
        HoldingError::QueueFull => FrontierError::QueueFull.into(),
        HoldingError::TimeReversed => kernel(sub::HOLDING_TIME_REVERSED),
        HoldingError::TopTier => kernel(sub::HOLDING_TOP_TIER),
        HoldingError::AboveCap => kernel(sub::HOLDING_ABOVE_CAP),
    }
}

// ------------------------------------------------------------ build items

/// Build item of the tier-up (after the six buildings and the walls).
pub const ITEM_TIER_UP: u8 = catalog::ITEM_COUNT;

// ------------------------------------------------------------ player prologue

/// What the player prologue (§5.6 steps 1–4) established.
pub(crate) struct Player {
    pub now: Now,
    pub ctx: AddrCtx,
    pub pc: PlayerCtx,
}

/// Steps 1–4 of the player prologue over `[actor, payer, season, citizen,
/// …]`: the Season at its recomputed PDA, status Running, ruleset
/// (`prologue::season`); the citizen and the actor
/// (`frontier_abi::prologue::player_prologue`); the action bucket and the
/// end bell (`PlayerStart::finish`, which writes the Citizen). The account
/// flags were checked by `check_accounts` before this runs.
pub(crate) fn player(program: &Pubkey, a: &[AccountInfo]) -> R<Player> {
    let [_, _, season_ai, ..] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let now = prologue::now()?;
    // One Season check (structure and PDA, then status and ruleset inside
    // the kernel prologue): the lean path W3-A's instructions take
    // (wave-3 review, W3-B CU budgets).
    let pc = super::citizen::player(program, a, now.ts, true)?;
    Ok(Player {
        now,
        ctx: addr::ctx(&key(season_ai), &program.to_bytes()),
        pc,
    })
}

/// Step 5, first half: the holding is present at its canonical address and
/// owned by the citizen (`NotOwner`).
pub(crate) fn owned_holding(
    _program: &Pubkey,
    pl: &Player,
    holding: &AccountInfo,
    citizen: &AccountInfo,
) -> R<HoldingHdr> {
    let hd = holding.try_borrow_data()?;
    let v = view(holding, &hd);
    Ok(ap::check_holding(
        &v,
        &pl.ctx,
        pl.pc.season.id,
        &key(citizen),
    )?)
}

/// A doctrine multiplier as the u16 the entry, transit and slot layouts
/// store (every table value is below 65,536 bps; `Overflow` otherwise).
pub(crate) fn bps16(v: permutation_rules::fixed::Bps) -> R<u16> {
    u16::try_from(v).map_err(|_| OVERFLOW)
}

/// A holding in state 1 or 2 (released and empty states are not owner
/// holdings; `NotFinal`).
pub(crate) fn live(h: &HoldingHdr) -> R<()> {
    if h.state == H::STATE_PROVISIONAL || h.state == H::STATE_FINAL {
        Ok(())
    } else {
        Err(FrontierError::NotFinal.into())
    }
}

/// The holding's own Province, present at its canonical address
/// (`BadAddress`, `BadAccount`).
pub(crate) fn own_province(
    program: &Pubkey,
    pl: &Player,
    h: &HoldingHdr,
    province: &AccountInfo,
) -> R<()> {
    expect_key(province, &pl.ctx.province(h.p as i32, h.q as i32))?;
    prologue::present(province, program, AccountKind::Province, pl.pc.season.id)?;
    let pd = province.try_borrow_data()?;
    let r = Ro(&pd);
    if r.i16(P::P)? != h.p || r.i16(P::Q)? != h.q {
        return Err(BAD_ACCOUNT);
    }
    Ok(())
}

/// Step 5, the lazy provisional → final flip (I-29, I-47), for instructions
/// that carry the holding's own Province: logs `HOLDING_FINAL` (Holding,
/// Citizen) when it happens. Returns the holding state after it.
pub(crate) fn finality(
    pl: &Player,
    h: &HoldingHdr,
    holding: &AccountInfo,
    citizen: &AccountInfo,
    province: &AccountInfo,
) -> R<u8> {
    let due = {
        let pd = province.try_borrow_data()?;
        ap::finality_due(h, &pd, pl.now.ts, pl.pc.now_bell)
    };
    if !due {
        return Ok(h.state);
    }
    let mut hd = holding.try_borrow_mut_data()?;
    let mut cd = citizen.try_borrow_mut_data()?;
    ap::apply_finality(&mut hd, &mut cd)?;
    let key = pqs_key(h.p, h.q, h.site)?;
    let payload = h.final_ts.to_le_bytes();
    events::emit(
        Kind::HOLDING_FINAL,
        pl.pc.now_bell,
        &key,
        &payload,
        &mut [
            Chained {
                entity: EntityKind::Holding,
                data: &mut hd,
            },
            Chained {
                entity: EntityKind::Citizen,
                data: &mut cd,
            },
        ],
    )?;
    Ok(H::STATE_FINAL)
}

/// The `{P i32, Q i32, site u8}` record key.
pub(crate) fn pqs_key(p: i16, q: i16, site: u8) -> R<[u8; 9]> {
    let mut k = [0u8; 9];
    k[..4].copy_from_slice(&(p as i32).to_le_bytes());
    k[4..8].copy_from_slice(&(q as i32).to_le_bytes());
    k[8] = site;
    Ok(k)
}

// ------------------------------------------------------------ Holding codec

fn tier_of(v: u8) -> R<Tier> {
    Ok(match v {
        0 => Tier::Hamlet,
        1 => Tier::Town,
        2 => Tier::City,
        3 => Tier::Stronghold,
        _ => return Err(BAD_ACCOUNT),
    })
}

fn tier_u8(t: Tier) -> u8 {
    match t {
        Tier::Hamlet => 0,
        Tier::Town => 1,
        Tier::City => 2,
        Tier::Stronghold => 3,
    }
}

fn resource_of(v: u8) -> R<Resource> {
    Resource::ALL.get(v as usize).copied().ok_or(BAD_ACCOUNT)
}

/// Queue item kinds (pinned; module note).
pub mod queue_kind {
    pub const FREE: u8 = 0;
    pub const PRODUCTION: u8 = 1;
    pub const UPKEEP: u8 = 2;
    pub const TIER_UP: u8 = 3;
    pub const WALLS: u8 = 4;
}

/// Decodes the kernel holding from a Holding account's data.
pub fn read_holding(d: &[u8]) -> R<KHolding> {
    let r = Ro(d);
    let mut stores = [Accrual::default(); RESOURCES];
    for (i, s) in stores.iter_mut().enumerate() {
        let o = H::store(i);
        *s = Accrual {
            value: r.i64(o + accrual::VALUE)?,
            rate: r.i64(o + accrual::RATE)?,
            cap: r.i64(o + accrual::CAP)?,
            t0: r.i64(o + accrual::T0)?,
            frac: r.i64(o + accrual::FRAC)?,
        };
    }
    let mut production = [0i64; RESOURCES];
    let mut upkeep = [0i64; RESOURCES];
    for i in 0..RESOURCES {
        production[i] = r.i64(H::PRODUCTION + 8 * i)?;
        upkeep[i] = r.i64(H::UPKEEP + 8 * i)?;
    }
    let mut queue = [None; QUEUE_SLOTS];
    for (i, q) in queue.iter_mut().enumerate() {
        let o = H::queue(i);
        let kind = r.u8(o + QI::KIND)?;
        if kind == queue_kind::FREE {
            continue;
        }
        let done_at = r.i64(o + QI::DONE_AT)?;
        let arg = r.u8(o + QI::ARG)?;
        let delta = r.i64(o + QI::DELTA)?;
        let effect = match kind {
            queue_kind::PRODUCTION => Effect::Production {
                resource: resource_of(arg)?,
                delta,
            },
            queue_kind::UPKEEP => Effect::Upkeep {
                resource: resource_of(arg)?,
                delta,
            },
            queue_kind::TIER_UP => Effect::TierUp,
            queue_kind::WALLS => Effect::Walls {
                delta: u32::try_from(delta).map_err(|_| BAD_ACCOUNT)?,
            },
            _ => return Err(BAD_ACCOUNT),
        };
        *q = Some(QueueItem { done_at, effect });
    }
    Ok(KHolding {
        tier: tier_of(r.u8(H::TIER)?)?,
        order: r.u8(H::ORDER)?,
        founded_ts: r.i64(H::FOUNDED_TS)?,
        founded_day: r.u32(H::FOUNDED_DAY)?,
        last_owner_action: r.i64(H::LAST_OWNER_ACTION)?,
        stores,
        production,
        upkeep,
        queue,
        walls: r.u32(H::WALLS)?,
        walls_committed_before: r.i64(H::WALLS_COMMITTED_BEFORE)?,
        food_shortfall: r.i64(H::FOOD_SHORTFALL)?,
    })
}

/// Encodes the kernel holding into a Holding account's data (every other
/// field kept), with `shield_until` and the dormant-flag cache at `now`.
/// Only what an owner action can change is written: the stores, rates,
/// production, upkeep, queue, walls and the owner-action, shield and
/// dormancy fields.
pub fn write_holding(d: &mut [u8], h: &KHolding, now: i64) -> R<()> {
    let mut w = Rw(d);
    w.set_u8(H::TIER, tier_u8(h.tier))?;
    w.set_u8(H::ORDER, h.order)?;
    w.set_i64(H::FOUNDED_TS, h.founded_ts)?;
    w.set_u32(H::FOUNDED_DAY, h.founded_day)?;
    w.set_i64(H::LAST_OWNER_ACTION, h.last_owner_action)?;
    w.set_i64(H::SHIELD_UNTIL, h.shield_until())?;
    for (i, s) in h.stores.iter().enumerate() {
        let o = H::store(i);
        w.set_i64(o + accrual::VALUE, s.value)?;
        w.set_i64(o + accrual::RATE, s.rate)?;
        w.set_i64(o + accrual::CAP, s.cap)?;
        w.set_i64(o + accrual::T0, s.t0)?;
        w.set_i64(o + accrual::FRAC, s.frac)?;
    }
    for i in 0..RESOURCES {
        w.set_i64(H::PRODUCTION + 8 * i, h.production[i])?;
        w.set_i64(H::UPKEEP + 8 * i, h.upkeep[i])?;
    }
    for (i, q) in h.queue.iter().enumerate() {
        let o = H::queue(i);
        let (done_at, kind, arg, delta) = match q {
            None => (0, queue_kind::FREE, 0, 0),
            Some(QueueItem { done_at, effect }) => match *effect {
                Effect::Production { resource, delta } => {
                    (*done_at, queue_kind::PRODUCTION, resource as u8, delta)
                }
                Effect::Upkeep { resource, delta } => {
                    (*done_at, queue_kind::UPKEEP, resource as u8, delta)
                }
                Effect::TierUp => (*done_at, queue_kind::TIER_UP, 0, 0),
                Effect::Walls { delta } => (*done_at, queue_kind::WALLS, 0, delta as i64),
            },
        };
        w.set_i64(o + QI::DONE_AT, done_at)?;
        w.set_u8(o + QI::KIND, kind)?;
        w.set_u8(o + QI::ARG, arg)?;
        w.set_i64(o + QI::DELTA, delta)?;
    }
    w.set_u32(H::WALLS, h.walls)?;
    w.set_i64(H::WALLS_COMMITTED_BEFORE, h.walls_committed_before)?;
    w.set_i64(H::FOOD_SHORTFALL, h.food_shortfall)?;
    let flags = w.u8(H::FLAGS)?;
    let dormant = if h.is_dormant(now) {
        H::FLAG_DORMANT_CACHE
    } else {
        0
    };
    w.set_u8(H::FLAGS, (flags & !H::FLAG_DORMANT_CACHE) | dormant)?;
    Ok(())
}

/// `settle(now)` split at every finished tier-up, applying the new tier's
/// base production from the tier-up's completion (the simulator's
/// `apply_tier_bonus`).
pub fn settle_tiered(h: &mut KHolding, now: i64) -> R<()> {
    loop {
        let next = h
            .queue
            .iter()
            .flatten()
            .filter(|q| matches!(q.effect, Effect::TierUp) && q.done_at <= now)
            .map(|q| q.done_at)
            .min();
        let Some(t) = next else { break };
        let t = t.max(h.stores[0].t0);
        let old = h.tier;
        h.settle(t).map_err(holding_err)?;
        if h.tier != old {
            let (a, b) = (
                catalog::base_production(old),
                catalog::base_production(h.tier),
            );
            for r in 0..RESOURCES {
                h.production[r] = h.production[r].saturating_add(b[r] - a[r]);
            }
            // Re-applies every store's rate at `t` (the simulator's idiom).
            let food = Resource::Food as usize;
            let up = h.upkeep[food];
            h.set_upkeep(t, Resource::Food, up).map_err(holding_err)?;
        } else {
            break;
        }
    }
    h.settle(now).map_err(holding_err)
}

/// The owner touch: tier-aware settle, `touch_owner(now)`,
/// `commit_walls(now)`.
pub fn touch(h: &mut KHolding, now: i64) -> R<()> {
    settle_tiered(h, now)?;
    h.touch_owner(now).map_err(holding_err)?;
    h.commit_walls(now);
    Ok(())
}

/// Copy number of building `item` (0..6) the next Build makes (module
/// note).
pub fn copy_number(h: &KHolding, item: u8) -> R<u32> {
    let bd = catalog::BUILDINGS
        .get(item as usize)
        .ok_or(FrontierError::BadData)?;
    let r = bd.resource as usize;
    let per = bd.per_hour.checked_mul(MILLI).ok_or(OVERFLOW)?;
    let base = catalog::base_production(h.tier)[r];
    let built = (h.production[r].saturating_sub(base)).max(0) / per.max(1);
    let queued = h
        .queue
        .iter()
        .flatten()
        .filter(
            |q| matches!(q.effect, Effect::Production { resource, .. } if resource == bd.resource),
        )
        .count() as i64;
    u32::try_from(built + queued + 1).map_err(|_| OVERFLOW)
}

/// `sha256` of the eight stores (value, rate, cap, t0, frac, LE): HARVEST's
/// digest.
pub fn stores_digest(d: &[u8]) -> R<[u8; 32]> {
    let s = d
        .get(H::STORES..H::STORES + H::STORES_N * accrual::SIZE)
        .ok_or(BAD_ACCOUNT)?;
    Ok(permutation_rules::hash::sha256(&[s]))
}

/// `sha256` of a cost (eight i64 LE): BUILD's digest.
pub fn cost_digest(c: &[i64; RESOURCES]) -> [u8; 32] {
    let mut b = [0u8; 8 * RESOURCES];
    for (i, v) in c.iter().enumerate() {
        b[8 * i..8 * i + 8].copy_from_slice(&v.to_le_bytes());
    }
    permutation_rules::hash::sha256(&[&b])
}

/// Loads, touches and returns the kernel holding (not written back).
pub(crate) fn load_touched(holding: &AccountInfo, now: i64) -> R<KHolding> {
    let hd = holding.try_borrow_data()?;
    let mut h = read_holding(&hd)?;
    touch(&mut h, now)?;
    Ok(h)
}

/// Emits a record chaining the Citizen, the Holding and (optionally) a
/// Province.
pub(crate) fn emit3(
    kind: Kind,
    bell: u32,
    key: &[u8],
    payload: &[u8],
    citizen: &AccountInfo,
    holding: &AccountInfo,
    province: Option<&AccountInfo>,
) -> R<()> {
    let mut cd = citizen.try_borrow_mut_data()?;
    let mut hd = holding.try_borrow_mut_data()?;
    match province {
        Some(p) => {
            let mut pd = p.try_borrow_mut_data()?;
            events::emit(
                kind,
                bell,
                key,
                payload,
                &mut [
                    Chained {
                        entity: EntityKind::Citizen,
                        data: &mut cd,
                    },
                    Chained {
                        entity: EntityKind::Holding,
                        data: &mut hd,
                    },
                    Chained {
                        entity: EntityKind::Province,
                        data: &mut pd,
                    },
                ],
            )
        }
        None => events::emit(
            kind,
            bell,
            key,
            payload,
            &mut [
                Chained {
                    entity: EntityKind::Citizen,
                    data: &mut cd,
                },
                Chained {
                    entity: EntityKind::Holding,
                    data: &mut hd,
                },
            ],
        ),
    }
}

// ------------------------------------------------------------ Harvest, Build, Train

/// 0x40 Harvest: P + `[holding w]`: the settle (harvest is implicit) and
/// the owner touch. Allowed while provisional (I-29).
pub fn harvest(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::Harvest, a, None)?;
    aix::Harvest::decode(d)?;
    let pl = player(p, a)?;
    let [_, _, _, citizen, holding] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let hh = owned_holding(p, &pl, holding, citizen)?;
    live(&hh)?;
    let h = load_touched(holding, pl.now.ts)?;
    let digest = {
        let mut hd = holding.try_borrow_mut_data()?;
        write_holding(&mut hd, &h, pl.now.ts)?;
        stores_digest(&hd)?
    };
    let key = pqs_key(hh.p, hh.q, hh.site)?;
    emit3(
        Kind::HARVEST,
        pl.pc.now_bell,
        &key,
        &digest,
        citizen,
        holding,
        None,
    )
}

/// 0x41 Build(item): P + `[holding w] [province w?]` (the holding's
/// province, for walls only). Items: module note.
pub fn build(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    let x = aix::Build::decode(d)?;
    let walls = x.item == catalog::ITEM_WALLS;
    check_accounts(Ix::Build, a, Some(&[1, 1, walls as u8]))?;
    if x.item > ITEM_TIER_UP {
        return Err(FrontierError::BadData.into());
    }
    let pl = player(p, a)?;
    let citizen = &a[3];
    let holding = &a[4];
    let province = a.get(5);
    let hh = owned_holding(p, &pl, holding, citizen)?;
    live(&hh)?;
    if let Some(pv) = province {
        own_province(p, &pl, &hh, pv)?;
        finality(&pl, &hh, holding, citizen, pv)?;
    }
    let now = pl.now.ts;
    let mut h = load_touched(holding, now)?;
    let doctrine = faction_doctrine(holding)?;
    let (cost, effect, secs) = if x.item == ITEM_TIER_UP {
        if h.queue
            .iter()
            .flatten()
            .any(|q| matches!(q.effect, Effect::TierUp))
        {
            // One tier-up at a time: the next tier's cost is its own.
            return Err(FrontierError::QueueFull.into());
        }
        catalog::tier_up_item(h.tier).ok_or_else(|| kernel(sub::HOLDING_TOP_TIER))?
    } else {
        let n = if walls { 1 } else { copy_number(&h, x.item)? };
        catalog::building(x.item, n, doctrine).ok_or(FrontierError::BadData)?
    };
    h.pay(now, &cost).map_err(holding_err)?;
    let done_at = h.enqueue(now, secs as i64, effect).map_err(holding_err)?;
    if walls {
        let pv = province.ok_or(FrontierError::TooManyAccounts)?;
        wall_item(&pl, &hh, pv, done_at, catalog::WALL_STEP)?;
    }
    {
        let mut hd = holding.try_borrow_mut_data()?;
        write_holding(&mut hd, &h, now)?;
    }
    let key = pqs_key(hh.p, hh.q, hh.site)?;
    let payload = Buf::<41>::new()
        .u8(x.item)
        .bytes(&cost_digest(&cost))
        .i64(done_at);
    emit3(
        Kind::BUILD,
        pl.pc.now_bell,
        &key,
        payload.get()?,
        citizen,
        holding,
        province,
    )
}

/// The doctrine of the holding's faction.
fn faction_doctrine(
    holding: &AccountInfo,
) -> R<&'static permutation_rules::frontier::doctrine::Doctrine> {
    let f = {
        let hd = holding.try_borrow_data()?;
        Ro(&hd).u8(H::FACTION)?
    };
    permutation_rules::frontier::doctrine::of_faction(f).ok_or(BAD_ACCOUNT)
}

/// Writes a wall item into the site mirror: `{effective_bell =
/// bell_at(done_at) + 1, delta}` in a free item after folding the items
/// already in force (`effective_bell ≤ resolved_next`) into
/// `walls_committed`; both busy → `QueueFull`.
fn wall_item(
    pl: &Player,
    hh: &HoldingHdr,
    province: &AccountInfo,
    done_at: i64,
    delta: u32,
) -> R<()> {
    let eff = ap::bell_at(pl.pc.season.genesis_ts, done_at)
        .and_then(|b| b.checked_add(1))
        .ok_or(OVERFLOW)?;
    let mut pd = province.try_borrow_mut_data()?;
    let site = site_of(&pd, hh)?;
    let o = P::site(site);
    let mut w = Rw(&mut pd);
    let resolved_next = w.u32(P::RESOLVED_NEXT)?;
    let items = [
        (SM::WALL_ITEM0_BELL, SM::WALL_ITEM0_DELTA),
        (SM::WALL_ITEM1_BELL, SM::WALL_ITEM1_DELTA),
    ];
    let mut committed = w.u32(o + SM::WALLS_COMMITTED)?;
    for (b, dl) in items {
        let (ib, idl) = (w.u32(o + b)?, w.u32(o + dl)?);
        if idl > 0 && ib <= resolved_next {
            committed = committed
                .saturating_add(idl)
                .min(permutation_rules::frontier::holding::MAX_WALLS);
            w.set_u32(o + b, 0)?;
            w.set_u32(o + dl, 0)?;
        }
    }
    w.set_u32(o + SM::WALLS_COMMITTED, committed)?;
    for (b, dl) in items {
        if w.u32(o + dl)? == 0 {
            w.set_u32(o + b, eff)?;
            w.set_u32(o + dl, delta)?;
            return Ok(());
        }
    }
    Err(FrontierError::QueueFull.into())
}

/// The site index of the holding in its Province's site table (its site
/// field is the index; the mirror must show this holding).
pub(crate) fn site_of(province: &[u8], hh: &HoldingHdr) -> R<usize> {
    let s = hh.site as usize;
    if s >= P::SITES_N {
        return Err(BAD_ACCOUNT);
    }
    let r = Ro(province);
    let o = P::site(s);
    if r.u8(o + SM::STATE)? != SM::STATE_HOLDING || r.u8(o + SM::GEN)? != hh.gen {
        return Err(BAD_ACCOUNT);
    }
    Ok(s)
}

/// 0x42 Train(unit, n): P + `[holding w]`: `catalog::train(unit, n)` paid
/// at once, `reserve[unit] += n` (whole troops, immediate, I-56).
pub fn train(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::Train, a, None)?;
    let x = aix::Train::decode(d)?;
    let pl = player(p, a)?;
    let [_, _, _, citizen, holding] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let hh = owned_holding(p, &pl, holding, citizen)?;
    live(&hh)?;
    let cost = catalog::train(x.unit, x.n).ok_or(FrontierError::BadData)?;
    let now = pl.now.ts;
    let mut h = load_touched(holding, now)?;
    h.pay(now, &cost).map_err(holding_err)?;
    {
        let mut hd = holding.try_borrow_mut_data()?;
        write_holding(&mut hd, &h, now)?;
        let mut w = Rw(&mut hd);
        let at = H::reserve(x.unit as usize);
        w.add_u32(at, x.n)?;
    }
    let key = pqs_key(hh.p, hh.q, hh.site)?;
    let payload = Buf::<13>::new().u8(x.unit).u32(x.n).i64(now);
    emit3(
        Kind::TRAIN,
        pl.pc.now_bell,
        &key,
        payload.get()?,
        citizen,
        holding,
        None,
    )
}

// ------------------------------------------------------------ Explore

/// Whether tile `t` is within one hex of tile `from` (same province).
pub fn within_one(from: u8, t: u8) -> bool {
    match (tile_offset(from), tile_offset(t)) {
        (Some(a), Some(b)) => a.distance(b) <= 1,
        _ => false,
    }
}

/// 0x46 Explore(host, n, tiles): P + `[holding w] [province w]` (where the
/// host stands). The host is the caller's (its id names this holding and
/// generation, `NotOwner`), in the roster (state 1) of this province
/// (`NotResident`), a Scout (`BadData`), with no pending change
/// (`HostBusy`); the tiles are within one hex of the host's tile
/// (`BadData`) and not yet explored (`Explored`); the holding's explore
/// record is free (`HostBusy`).
pub fn explore(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::Explore, a, None)?;
    let x = aix::Explore::decode(d)?;
    let pl = player(p, a)?;
    let [_, _, _, citizen, holding, province] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let hh = owned_holding(p, &pl, holding, citizen)?;
    live(&hh)?;
    let host = super::host::resident_host(p, &pl, &hh, holding, citizen, province, x.host_id)?;
    let now = pl.now.ts;
    let h = load_touched(holding, now)?;
    if host.entry.unit != frontier_abi::entry::unit_to_u8(permutation_rules::units::UnitType::Scout)
    {
        return Err(FrontierError::BadData.into());
    }
    if host.entry.busy() {
        return Err(FrontierError::HostBusy.into());
    }
    let n = x.n as usize;
    if !(1..=2).contains(&n) {
        return Err(FrontierError::BadData.into());
    }
    // One tile: the second is recorded as NO_TILE whatever the data says
    // (the ABI's "ignored"; fclient's builder sends 0).
    let tiles_rec = if n == 1 {
        [x.tiles[0], X::NO_TILE]
    } else {
        x.tiles
    };
    let tiles = &tiles_rec[..n];
    if n == 2 && tiles[0] == tiles[1] {
        return Err(FrontierError::BadData.into());
    }
    if tiles
        .iter()
        .any(|&t| t as usize >= P::TILES || !within_one(host.entry.tile, t))
    {
        return Err(FrontierError::BadData.into());
    }
    {
        let hd = holding.try_borrow_data()?;
        if Ro(&hd).u8(H::EXPLORE + X::STATE)? != X::STATE_FREE {
            return Err(FrontierError::HostBusy.into());
        }
    }
    {
        let mut pd = province.try_borrow_mut_data()?;
        let mut w = Rw(&mut pd);
        let mask = w.u64(P::EXPLORED_MASK)?;
        let mut m = mask;
        for &t in tiles {
            let bit = 1u64 << t;
            if m & bit != 0 {
                return Err(FrontierError::Explored.into());
            }
            m |= bit;
        }
        w.set_u64(P::EXPLORED_MASK, m)?;
    }
    {
        let mut hd = holding.try_borrow_mut_data()?;
        write_holding(&mut hd, &h, now)?;
        let mut w = Rw(&mut hd);
        let e = H::EXPLORE;
        w.set_u32(e + X::BELL, pl.pc.now_bell)?;
        w.set_i16(e + X::P, host.p)?;
        w.set_i16(e + X::Q, host.q)?;
        w.set_arr(e + X::TILES, &tiles_rec)?;
        w.set_u64(e + X::HOST, x.host_id)?;
        w.set_u8(e + X::STATE, X::STATE_PENDING)?;
    }
    let payload = Buf::<11>::new()
        .i32(host.p as i32)
        .i32(host.q as i32)
        .u8(x.n)
        .bytes(&tiles_rec);
    emit3(
        Kind::EXPLORE,
        pl.pc.now_bell,
        &x.host_id.to_le_bytes(),
        payload.get()?,
        citizen,
        holding,
        Some(province),
    )
}

// ------------------------------------------------------------ seeds

/// The bell seed `S(bell, region)` from `[seedcache|archive] [anchor|archive]`
/// (§5.8, as SettleTicket and ResolveFromInputs read it): a SeedCache of
/// THE anchor whose round is `S(A)` (the cache's address recomputed from
/// its stored nonce), or — both positions the region-half-day archive —
/// the archive entry of an archived bell. Anything missing is
/// `SeedNotReady`; a non-canonical key `BadAddress`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn bell_seed(
    program: &Pubkey,
    ctx: &AddrCtx,
    season_id: u64,
    clock: &SeasonClock,
    bell: u32,
    region: u8,
    first: &AccountInfo,
    second: &AccountInfo,
) -> R<[u8; 32]> {
    let part = archive_part_of(bell);
    let arch = ctx.anchor_archive(region, part);
    if first.key.as_array() == &arch && second.key.as_array() == &arch {
        if !prologue::presence(first, program, AccountKind::AnchorArchive, season_id)? {
            return Err(FrontierError::SeedNotReady.into());
        }
        let ad = first.try_borrow_data()?;
        if archive_key(&ad)? != (region, part) {
            return Err(BAD_ACCOUNT);
        }
        if !archive_archived(&ad, bell)? {
            return Err(FrontierError::SeedNotReady.into());
        }
        return Ok(archive_entry_of(&ad, bell)?.1);
    }
    expect_key(second, &ctx.bell_anchor(bell, region))?;
    if !prologue::presence(second, program, AccountKind::BellAnchor, season_id)? {
        return Err(FrontierError::SeedNotReady.into());
    }
    let an = {
        let d = second.try_borrow_data()?;
        Anchor::read(&d)?
    };
    if an.bell != bell || an.region != region {
        return Err(BAD_ACCOUNT);
    }
    if !prologue::presence(first, program, AccountKind::SeedCache, season_id)? {
        // An absent cache at its canonical address (nonce unknown: any
        // absent key reads as "not posted yet").
        return Err(FrontierError::SeedNotReady.into());
    }
    let c = {
        let d = first.try_borrow_data()?;
        Cache::read(&d)?
    };
    if c.bell != bell || c.region != region {
        return Err(BAD_ACCOUNT);
    }
    expect_key(first, &ctx.seed_cache(bell, region, c.nonce))?;
    if c.anchor_key != key(second) || c.a != an.a || c.round != clock.seed_round(bell, an.a) {
        return Err(FrontierError::SeedNotReady.into());
    }
    Ok(c.seed)
}

/// 0x47 SettleExplore: `[payer s] [season] [holding w] [citizen w]
/// [seedcache|archive r] [anchor|archive r]`, anyone (class N). The
/// explore record present (a cleared one is `AlreadyDone`); the seed
/// `S(record.bell, r_province)` (`SeedNotReady`); per tile
/// `explore::roll(S, P, Q, tile, host, floor)`, the floor spent tile by
/// tile; `works += Σ finds`; the record cleared.
pub fn settle_explore(p: &Pubkey, a: &[AccountInfo], d: &[u8]) -> R<()> {
    check_accounts(Ix::SettleExplore, a, None)?;
    aix::SettleExplore::decode(d)?;
    let now = prologue::now()?;
    let [_payer, season_ai, holding, citizen, first, second] = a else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let hdr = prologue::season(
        season_ai,
        p,
        Some(&RULESET_HASH),
        &[S::STATUS_RUNNING, S::STATUS_ENDED],
        now.ts,
    )?;
    let ctx = addr::ctx(&key(season_ai), &p.to_bytes());
    prologue::present(holding, p, AccountKind::Holding, hdr.id)?;
    let (hp, hq, hsite, owner, rec) = {
        let hd = holding.try_borrow_data()?;
        let r = Ro(&hd);
        let (hp, hq, hsite) = (r.i16(H::P)?, r.i16(H::Q)?, r.u8(H::SITE)?);
        let e = H::EXPLORE;
        (
            hp,
            hq,
            hsite,
            r.arr::<32>(H::OWNER_CITIZEN)?,
            (
                r.u8(e + X::STATE)?,
                r.u32(e + X::BELL)?,
                r.i16(e + X::P)?,
                r.i16(e + X::Q)?,
                r.arr::<2>(e + X::TILES)?,
                r.u64(e + X::HOST)?,
            ),
        )
    };
    expect_key(holding, &ctx.holding(hp as i32, hq as i32, hsite))?;
    expect_key(citizen, &owner)?;
    prologue::present(citizen, p, AccountKind::Citizen, hdr.id)?;
    let (state, bell, ep, eq, tiles, host) = rec;
    if state != X::STATE_PENDING {
        return Err(FrontierError::AlreadyDone.into());
    }
    let pc = ProvinceCoord::new(ep as i32, eq as i32);
    let region = permutation_rules::frontier::geometry::region_of(pc);
    let clock = {
        let sd = season_ai.try_borrow_data()?;
        SeasonClock::read(&sd)?
    };
    let seed = bell_seed(p, &ctx, hdr.id, &clock, bell, region, first, second)?;
    let mut floor_left = {
        let cd = citizen.try_borrow_data()?;
        Ro(&cd).u8(C::EXPLORES_FLOOR_LEFT)?
    };
    let mut per_tile = [0u32; 2];
    let mut total = 0u32;
    let mut floor_used = 0u8;
    let mut n = 0u32;
    for (i, &t) in tiles.iter().enumerate() {
        if t == X::NO_TILE {
            continue;
        }
        let floor = floor_left > 0;
        if floor {
            floor_left -= 1;
            floor_used += 1;
        }
        let f = permutation_rules::frontier::explore::roll(&seed, pc, t, host, floor);
        per_tile[i] = f.works;
        total = total.checked_add(f.works).ok_or(OVERFLOW)?;
        n += 1;
    }
    {
        let mut cd = citizen.try_borrow_mut_data()?;
        let mut w = Rw(&mut cd);
        w.set_u8(C::EXPLORES_FLOOR_LEFT, floor_left)?;
        w.add_u64(C::WORKS, total as u64)?;
        w.add_u32(C::EXPLORES, n)?;
    }
    {
        let mut hd = holding.try_borrow_mut_data()?;
        let mut w = Rw(&mut hd);
        w.set_arr(H::EXPLORE, &[0u8; X::SIZE])?;
    }
    let payload = Buf::<13>::new()
        .u32(per_tile[0])
        .u32(per_tile[1])
        .u32(total)
        .u8(floor_used);
    let log_bell = hdr.bell(now.ts).unwrap_or(frontier_abi::log::NO_BELL);
    emit3(
        Kind::EXPLORE_RESULT,
        log_bell,
        &host.to_le_bytes(),
        payload.get()?,
        citizen,
        holding,
        None,
    )
}

/// The global hex of tile `t` of province `(p, q)`.
pub fn tile_hex(p: i16, q: i16, t: u8) -> Option<Hex> {
    ProvinceCoord::new(p as i32, q as i32).tile(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use permutation_rules::frontier::holding::HOUR;

    fn founded(now: i64) -> KHolding {
        let mut h = KHolding::found(now, 0, 1);
        h.production = catalog::base_production(Tier::Hamlet);
        let kit = catalog::starter_kit();
        for (r, v) in kit.iter().enumerate() {
            h.credit(now, Resource::ALL[r], *v).unwrap();
        }
        h
    }

    #[test]
    fn holding_round_trips_through_the_layout() {
        let now = 1_800_000_000;
        let mut h = founded(now);
        h.enqueue(
            now,
            3_600,
            Effect::Production {
                resource: Resource::Ore,
                delta: 6_000,
            },
        )
        .unwrap();
        h.enqueue(now, 7_200, Effect::Walls { delta: 100 }).unwrap();
        let mut d = alloc::vec![0u8; H::SIZE];
        write_holding(&mut d, &h, now).unwrap();
        assert_eq!(read_holding(&d).unwrap(), h);
        // a TierUp and an Upkeep item too
        let mut h2 = h.clone();
        h2.queue[2] = Some(QueueItem {
            done_at: now + 5,
            effect: Effect::TierUp,
        });
        h2.queue[3] = Some(QueueItem {
            done_at: now + 6,
            effect: Effect::Upkeep {
                resource: Resource::Food,
                delta: -7,
            },
        });
        write_holding(&mut d, &h2, now).unwrap();
        assert_eq!(read_holding(&d).unwrap(), h2);
        assert_eq!(Ro(&d).i64(H::SHIELD_UNTIL).unwrap(), h2.shield_until());
        // unknown kinds and tiers are refused
        d[H::queue(0) + QI::KIND] = 9;
        assert!(read_holding(&d).is_err());
        d[H::queue(0) + QI::KIND] = 0;
        d[H::TIER] = 4;
        assert!(read_holding(&d).is_err());
    }

    #[test]
    fn tier_up_raises_base_production_from_its_completion() {
        let now = 1_800_000_000;
        let mut h = founded(now);
        let (cost, e, secs) = catalog::tier_up_item(Tier::Hamlet).unwrap();
        for (r, v) in cost.iter().enumerate() {
            h.credit(now, Resource::ALL[r], *v).unwrap();
        }
        h.pay(now, &cost).unwrap();
        let done = h.enqueue(now, secs as i64, e).unwrap();
        let mut a = h.clone();
        settle_tiered(&mut a, done + 2 * HOUR).unwrap();
        assert_eq!(a.tier, Tier::Town);
        assert_eq!(a.production, catalog::base_production(Tier::Town));
        // Two hours at the Town rate after completion, not at the Hamlet's.
        let mut b = h.clone();
        settle_tiered(&mut b, done).unwrap();
        let wood = Resource::Wood as usize;
        let gained = a.stores[wood].value - b.stores[wood].value;
        assert_eq!(gained, 2 * catalog::base_production(Tier::Town)[wood]);
        // idempotent
        let mut c = a.clone();
        settle_tiered(&mut c, done + 2 * HOUR).unwrap();
        assert_eq!(c, a);
    }

    #[test]
    fn copy_numbers_follow_production_and_the_queue() {
        let now = 1_800_000_000;
        let mut h = founded(now);
        assert_eq!(copy_number(&h, 0).unwrap(), 1);
        h.production[Resource::Food as usize] += 2 * 12 * MILLI;
        assert_eq!(copy_number(&h, 0).unwrap(), 3);
        h.enqueue(
            now,
            10,
            Effect::Production {
                resource: Resource::Food,
                delta: 12 * MILLI,
            },
        )
        .unwrap();
        assert_eq!(copy_number(&h, 0).unwrap(), 4);
        assert_eq!(copy_number(&h, 1).unwrap(), 1);
        assert!(copy_number(&h, 6).is_err());
    }

    #[test]
    fn explore_tiles_within_one_hex() {
        // tile 30 is the centre (hexes_within(4) order)
        assert_eq!(tile_offset(30), Some(Hex::new(0, 0)));
        let near = (0..61u8).filter(|&t| within_one(30, t)).count();
        assert_eq!(near, 7);
        assert!(!within_one(30, 0));
        assert!(!within_one(30, 61));
    }
}
