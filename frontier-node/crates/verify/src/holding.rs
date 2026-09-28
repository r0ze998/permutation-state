//! The Holding replay of V7 (contract §8.5 V7 "holdings … through the
//! kernel's lazy functions"; W4-D notes §2, deferred to W5): an
//! **independent transcription** of the program's pinned Holding codec and
//! owner touch (`permutation-frontier/src/proc/holding.rs`, W3-B's pinned
//! rules, "the verifier replays the same sequence"), driving the kernel's
//! `holding::Holding` (`settle`, `touch_owner`, `commit_walls`, `pay`,
//! `enqueue`, `set_upkeep`) and `catalog` (`building`, `tier_up_item`,
//! `train`, `base_production`).
//!
//! - **Codec.** `tier` u8 = `Tier` in declaration order; queue item `kind`
//!   0 free, 1 `Production {resource = arg, delta}`, 2 `Upkeep`, 3
//!   `TierUp`, 4 `Walls {delta}`; stores, production, upkeep, walls,
//!   `walls_committed_before`, `food_shortfall` one to one. Writing also
//!   sets `shield_until` and the dormant-flag cache at `now`.
//! - **Touch** (every owner action): `settle` split at each finished
//!   tier-up (the new tier's base production from its completion, food
//!   upkeep re-applied at that instant), `touch_owner(now)`,
//!   `commit_walls(now)`.
//! - **Harvest** = the touch. **Build(item)**: items 0–5 the buildings
//!   (copy number `1 + (production[r] − base(tier)[r]) / per_hour + queued
//!   copies`), 6 walls (copy 1, a wall item in the site mirror), 7 the
//!   tier-up (one at a time); `pay(now, cost)`, `enqueue(now, secs,
//!   effect)`. **Train(unit, n)**: `pay(now, catalog::train(unit, n))`,
//!   `reserve[unit] += n` (whole troops, I-56).

use frontier_abi::layout::player::{accrual, holding as H, queue_item as QI};
use frontier_abi::layout::province::{province as PV, site as SM};
use permutation_rules::fixed::MILLI;
use permutation_rules::frontier::catalog;
use permutation_rules::frontier::holding::{
    Accrual, Effect, Holding as KHolding, QueueItem, Resource, Tier, QUEUE_SLOTS, RESOURCES,
};

use crate::world::le;

/// Build item of the tier-up (after the six buildings and the walls).
pub const ITEM_TIER_UP: u8 = catalog::ITEM_COUNT;

/// Queue item kinds (the program's pinned `queue_kind`).
pub mod queue_kind {
    pub const FREE: u8 = 0;
    pub const PRODUCTION: u8 = 1;
    pub const UPKEEP: u8 = 2;
    pub const TIER_UP: u8 = 3;
    pub const WALLS: u8 = 4;
}

fn i64_at(d: &[u8], o: usize) -> Result<i64, String> {
    d.get(o..o + 8)
        .map(|b| le(b) as i64)
        .ok_or(format!("holding: short at {o}"))
}

fn u32_at(d: &[u8], o: usize) -> Result<u32, String> {
    d.get(o..o + 4)
        .map(|b| le(b) as u32)
        .ok_or(format!("holding: short at {o}"))
}

fn u8_at(d: &[u8], o: usize) -> Result<u8, String> {
    d.get(o).copied().ok_or(format!("holding: short at {o}"))
}

fn put(d: &mut [u8], o: usize, b: &[u8]) -> Result<(), String> {
    d.get_mut(o..o + b.len())
        .ok_or(format!("holding: short at {o}"))?
        .copy_from_slice(b);
    Ok(())
}

fn tier_of(v: u8) -> Result<Tier, String> {
    Ok(match v {
        0 => Tier::Hamlet,
        1 => Tier::Town,
        2 => Tier::City,
        3 => Tier::Stronghold,
        _ => return Err(format!("holding: tier {v}")),
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

fn resource_of(v: u8) -> Result<Resource, String> {
    Resource::ALL
        .get(v as usize)
        .copied()
        .ok_or(format!("holding: resource {v}"))
}

/// The kernel holding of a Holding account's bytes.
pub fn read(d: &[u8]) -> Result<KHolding, String> {
    let mut stores = [Accrual::default(); RESOURCES];
    for (i, s) in stores.iter_mut().enumerate() {
        let o = H::store(i);
        *s = Accrual {
            value: i64_at(d, o + accrual::VALUE)?,
            rate: i64_at(d, o + accrual::RATE)?,
            cap: i64_at(d, o + accrual::CAP)?,
            t0: i64_at(d, o + accrual::T0)?,
            frac: i64_at(d, o + accrual::FRAC)?,
        };
    }
    let mut production = [0i64; RESOURCES];
    let mut upkeep = [0i64; RESOURCES];
    for i in 0..RESOURCES {
        production[i] = i64_at(d, H::PRODUCTION + 8 * i)?;
        upkeep[i] = i64_at(d, H::UPKEEP + 8 * i)?;
    }
    let mut queue = [None; QUEUE_SLOTS];
    for (i, q) in queue.iter_mut().enumerate() {
        let o = H::queue(i);
        let kind = u8_at(d, o + QI::KIND)?;
        if kind == queue_kind::FREE {
            continue;
        }
        let done_at = i64_at(d, o + QI::DONE_AT)?;
        let arg = u8_at(d, o + QI::ARG)?;
        let delta = i64_at(d, o + QI::DELTA)?;
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
                delta: u32::try_from(delta).map_err(|_| format!("holding: walls {delta}"))?,
            },
            k => return Err(format!("holding: queue kind {k}")),
        };
        *q = Some(QueueItem { done_at, effect });
    }
    Ok(KHolding {
        tier: tier_of(u8_at(d, H::TIER)?)?,
        order: u8_at(d, H::ORDER)?,
        founded_ts: i64_at(d, H::FOUNDED_TS)?,
        founded_day: u32_at(d, H::FOUNDED_DAY)?,
        last_owner_action: i64_at(d, H::LAST_OWNER_ACTION)?,
        stores,
        production,
        upkeep,
        queue,
        walls: u32_at(d, H::WALLS)?,
        walls_committed_before: i64_at(d, H::WALLS_COMMITTED_BEFORE)?,
        food_shortfall: i64_at(d, H::FOOD_SHORTFALL)?,
    })
}

/// Writes the kernel holding into a copy of the account bytes (every other
/// field kept), with `shield_until` and the dormant-flag cache at `now`.
pub fn write(d: &mut [u8], h: &KHolding, now: i64) -> Result<(), String> {
    put(d, H::TIER, &[tier_u8(h.tier)])?;
    put(d, H::ORDER, &[h.order])?;
    put(d, H::FOUNDED_TS, &h.founded_ts.to_le_bytes())?;
    put(d, H::FOUNDED_DAY, &h.founded_day.to_le_bytes())?;
    put(d, H::LAST_OWNER_ACTION, &h.last_owner_action.to_le_bytes())?;
    put(d, H::SHIELD_UNTIL, &h.shield_until().to_le_bytes())?;
    for (i, s) in h.stores.iter().enumerate() {
        let o = H::store(i);
        put(d, o + accrual::VALUE, &s.value.to_le_bytes())?;
        put(d, o + accrual::RATE, &s.rate.to_le_bytes())?;
        put(d, o + accrual::CAP, &s.cap.to_le_bytes())?;
        put(d, o + accrual::T0, &s.t0.to_le_bytes())?;
        put(d, o + accrual::FRAC, &s.frac.to_le_bytes())?;
    }
    for i in 0..RESOURCES {
        put(d, H::PRODUCTION + 8 * i, &h.production[i].to_le_bytes())?;
        put(d, H::UPKEEP + 8 * i, &h.upkeep[i].to_le_bytes())?;
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
        put(d, o + QI::DONE_AT, &done_at.to_le_bytes())?;
        put(d, o + QI::KIND, &[kind])?;
        put(d, o + QI::ARG, &[arg])?;
        put(d, o + QI::DELTA, &delta.to_le_bytes())?;
    }
    put(d, H::WALLS, &h.walls.to_le_bytes())?;
    put(
        d,
        H::WALLS_COMMITTED_BEFORE,
        &h.walls_committed_before.to_le_bytes(),
    )?;
    put(d, H::FOOD_SHORTFALL, &h.food_shortfall.to_le_bytes())?;
    let flags = u8_at(d, H::FLAGS)?;
    let dormant = if h.is_dormant(now) {
        H::FLAG_DORMANT_CACHE
    } else {
        0
    };
    put(d, H::FLAGS, &[(flags & !H::FLAG_DORMANT_CACHE) | dormant])
}

/// `settle(now)` split at every finished tier-up, the new tier's base
/// production applied from the tier-up's completion.
pub fn settle_tiered(h: &mut KHolding, now: i64) -> Result<(), String> {
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
        h.settle(t).map_err(|e| format!("settle: {e:?}"))?;
        if h.tier == old {
            break;
        }
        let (a, b) = (
            catalog::base_production(old),
            catalog::base_production(h.tier),
        );
        for r in 0..RESOURCES {
            h.production[r] = h.production[r].saturating_add(b[r] - a[r]);
        }
        let food = Resource::Food as usize;
        let up = h.upkeep[food];
        h.set_upkeep(t, Resource::Food, up)
            .map_err(|e| format!("set_upkeep: {e:?}"))?;
    }
    h.settle(now).map_err(|e| format!("settle: {e:?}"))
}

/// The owner touch.
pub fn touch(h: &mut KHolding, now: i64) -> Result<(), String> {
    settle_tiered(h, now)?;
    h.touch_owner(now)
        .map_err(|e| format!("touch_owner: {e:?}"))?;
    h.commit_walls(now);
    Ok(())
}

/// Copy number of building `item` (0..6) the next Build makes.
pub fn copy_number(h: &KHolding, item: u8) -> Result<u32, String> {
    let bd = catalog::BUILDINGS
        .get(item as usize)
        .ok_or(format!("build item {item}"))?;
    let r = bd.resource as usize;
    let per = bd.per_hour.saturating_mul(MILLI);
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
    u32::try_from(built + queued + 1).map_err(|_| "copy number".to_string())
}

/// HARVEST's digest: `sha256` of the eight stores.
pub fn stores_digest(d: &[u8]) -> Result<[u8; 32], String> {
    let s = d
        .get(H::STORES..H::STORES + H::STORES_N * accrual::SIZE)
        .ok_or("holding: short stores")?;
    Ok(permutation_rules::hash::sha256(&[s]))
}

/// BUILD's digest: `sha256` of the cost (eight i64 LE).
pub fn cost_digest(c: &[i64; RESOURCES]) -> [u8; 32] {
    let mut b = [0u8; 8 * RESOURCES];
    for (i, v) in c.iter().enumerate() {
        b[8 * i..8 * i + 8].copy_from_slice(&v.to_le_bytes());
    }
    permutation_rules::hash::sha256(&[&b])
}

/// One replayed owner action on a Holding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Harvest,
    Build { item: u8 },
    Train { unit: u8, n: u32 },
}

/// What a replayed action leaves: the Holding bytes, the record payload it
/// logs, and (walls) the wall item `(effective bell, delta)`.
#[derive(Clone, Debug)]
pub struct Replayed {
    pub holding: Vec<u8>,
    pub payload: Vec<u8>,
    pub wall_item: Option<(u32, u32)>,
}

/// Replays one owner action at `now` over the Holding bytes `before`.
pub fn replay(before: &[u8], a: &Action, now: i64, genesis_ts: i64) -> Result<Replayed, String> {
    let mut h = read(before)?;
    touch(&mut h, now)?;
    let mut d = before.to_vec();
    match *a {
        Action::Harvest => {
            write(&mut d, &h, now)?;
            let payload = stores_digest(&d)?.to_vec();
            Ok(Replayed {
                holding: d,
                payload,
                wall_item: None,
            })
        }
        Action::Build { item } => {
            if item > ITEM_TIER_UP {
                return Err(format!("build item {item}"));
            }
            let walls = item == catalog::ITEM_WALLS;
            let doctrine =
                permutation_rules::frontier::doctrine::of_faction(u8_at(before, H::FACTION)?)
                    .ok_or("holding: faction")?;
            let (cost, effect, secs) = if item == ITEM_TIER_UP {
                if h.queue
                    .iter()
                    .flatten()
                    .any(|q| matches!(q.effect, Effect::TierUp))
                {
                    return Err("a second tier-up in the queue".into());
                }
                catalog::tier_up_item(h.tier).ok_or("tier-up at the top tier")?
            } else {
                let n = if walls { 1 } else { copy_number(&h, item)? };
                catalog::building(item, n, doctrine).ok_or(format!("build item {item}"))?
            };
            h.pay(now, &cost).map_err(|e| format!("pay: {e:?}"))?;
            let done_at = h
                .enqueue(now, secs as i64, effect)
                .map_err(|e| format!("enqueue: {e:?}"))?;
            write(&mut d, &h, now)?;
            let mut payload = vec![item];
            payload.extend_from_slice(&cost_digest(&cost));
            payload.extend_from_slice(&done_at.to_le_bytes());
            let wall_item = if walls {
                let b = frontier_abi::prologue::bell_at(genesis_ts, done_at)
                    .and_then(|b| b.checked_add(1))
                    .ok_or("wall item bell")?;
                Some((b, catalog::WALL_STEP))
            } else {
                None
            };
            Ok(Replayed {
                holding: d,
                payload,
                wall_item,
            })
        }
        Action::Train { unit, n } => {
            let cost = catalog::train(unit, n).ok_or(format!("train unit {unit} n {n}"))?;
            h.pay(now, &cost).map_err(|e| format!("pay: {e:?}"))?;
            write(&mut d, &h, now)?;
            let at = H::reserve(unit as usize);
            let v = u32_at(&d, at)?.checked_add(n).ok_or("reserve overflow")?;
            put(&mut d, at, &v.to_le_bytes())?;
            let mut payload = vec![unit];
            payload.extend_from_slice(&n.to_le_bytes());
            payload.extend_from_slice(&now.to_le_bytes());
            Ok(Replayed {
                holding: d,
                payload,
                wall_item: None,
            })
        }
    }
}

/// The site mirror's wall fields after a walls Build (the program's
/// `wall_item`): items already in force (`effective_bell ≤
/// resolved_next`) folded into `walls_committed` (capped), the new item in
/// the first free slot. `None`: both slots busy (the program refuses).
pub fn wall_item(province: &[u8], site: usize, item: (u32, u32)) -> Option<Vec<u8>> {
    let mut d = province.to_vec();
    let o = PV::site(site);
    let rn = le(d.get(PV::RESOLVED_NEXT..PV::RESOLVED_NEXT + 4)?) as u32;
    let items = [
        (SM::WALL_ITEM0_BELL, SM::WALL_ITEM0_DELTA),
        (SM::WALL_ITEM1_BELL, SM::WALL_ITEM1_DELTA),
    ];
    let rd = |d: &[u8], at: usize| d.get(at..at + 4).map(|b| le(b) as u32);
    let mut committed = rd(&d, o + SM::WALLS_COMMITTED)?;
    for (b, dl) in items {
        let (ib, idl) = (rd(&d, o + b)?, rd(&d, o + dl)?);
        if idl > 0 && ib <= rn {
            committed = committed
                .saturating_add(idl)
                .min(permutation_rules::frontier::holding::MAX_WALLS);
            d[o + b..o + b + 4].fill(0);
            d[o + dl..o + dl + 4].fill(0);
        }
    }
    d[o + SM::WALLS_COMMITTED..o + SM::WALLS_COMMITTED + 4]
        .copy_from_slice(&committed.to_le_bytes());
    for (b, dl) in items {
        if rd(&d, o + dl)? == 0 {
            d[o + b..o + b + 4].copy_from_slice(&item.0.to_le_bytes());
            d[o + dl..o + dl + 4].copy_from_slice(&item.1.to_le_bytes());
            return Some(d);
        }
    }
    None
}

// ------------------------------------------------------------ continuity (wave-5 review)

/// The Holding bytes the owner's lazy accrual owns: `order`, `tier`,
/// `flags`; `founded_ts`, `founded_day`; `last_owner_action` through
/// `food_shortfall` (shield, stores, production, upkeep, queue, walls).
/// Only SettleTicket's founding and the owner touches (Harvest, Build,
/// Train, Explore, Muster, Dissolve, Garrison, Depart) write them; every
/// other Holding write must leave them byte-identical.
pub const ACCRUAL_RANGES: [(usize, usize); 3] = [
    (H::ORDER, H::TICKET_BELL),
    (H::FOUNDED_TS, H::HOST_SEQ),
    (H::LAST_OWNER_ACTION, H::RESERVE),
];

/// The accrual bytes of a Holding account (`None` if too short).
pub fn accrual_bytes(d: &[u8]) -> Option<Vec<u8>> {
    let mut v = vec![];
    for (a, b) in ACCRUAL_RANGES {
        v.extend_from_slice(d.get(a..b)?);
    }
    Some(v)
}

/// First differing offset (in account bytes) of the accrual ranges.
pub fn accrual_diff(a: &[u8], b: &[u8]) -> Option<usize> {
    for (x, y) in ACCRUAL_RANGES {
        for o in x..y {
            if a.get(o) != b.get(o) {
                return Some(o);
            }
        }
    }
    None
}

/// The Holding an owner touch at `now` leaves (no payment, no queue item):
/// what Explore, Muster, Dissolve, Garrison and Depart write into the
/// accrual (the program's `load_touched` + `write_holding`).
pub fn touched(before: &[u8], now: i64) -> Result<Vec<u8>, String> {
    let mut h = read(before)?;
    touch(&mut h, now)?;
    let mut d = before.to_vec();
    write(&mut d, &h, now)?;
    Ok(d)
}

/// The accrual SettleTicket writes into a founded Holding at `now` (day
/// `day`): a Hamlet founded as first holding (`order` 1) with the Hamlet's
/// base production, no food upkeep and the starter kit credited — an
/// independent transcription of the program's `founded_holding`
/// (`proc/citizen.rs`). The rest of `post` is kept.
pub fn founded(post: &[u8], now: i64, day: u32) -> Result<Vec<u8>, String> {
    let mut h = KHolding::found(now, day, 1);
    h.production = catalog::base_production(Tier::Hamlet);
    h.set_upkeep(now, Resource::Food, 0)
        .map_err(|e| format!("set_upkeep: {e:?}"))?;
    for (r, amount) in catalog::starter_kit().iter().enumerate() {
        if *amount > 0 {
            h.credit(now, Resource::ALL[r], *amount)
                .map_err(|e| format!("credit: {e:?}"))?;
        }
    }
    let mut d = post.to_vec();
    for (a, b) in ACCRUAL_RANGES {
        d.get_mut(a..b).ok_or("holding: short")?.fill(0);
    }
    write(&mut d, &h, now)?;
    Ok(d)
}
