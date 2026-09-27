//! Holdings, hosts and marches for W3-B's tests (M1 contract §11 wave 3).
//!
//! Build on [`super::World`] (a running season through the program's own
//! instructions). The Province, Citizen and first Holding a player needs
//! are created by OpenProvince, Join and SettleTicket, which are W3-A's
//! (same wave), so this module **crafts** them byte for byte in the frozen
//! `frontier-abi` layouts, as those instructions leave them:
//!
//! - **Province** ([`World::craft_province`]): terrain from the kernel's
//!   `terrain::generate_province` under a fixed ring seed; `passable_mask`
//!   bit i ⇔ tile i is passable, `rough_mask` bit i ⇔ Forest or Hills;
//!   sites and site mirrors (free, pending garrison bells `NO_BELL`);
//!   `resolved_next` = the current bell.
//! - **Citizen and Holding** ([`World::craft_estate`]): joined, first
//!   holding final; the Holding is `Holding::found(now, day, 1)` with
//!   `production = base_production(Hamlet)` and the starter kit credited
//!   (the simulator's `found`), in the program's pinned codec
//!   ([`write_kholding`]: `tier` in declaration order, queue kinds 1–4).
//!
//! Every test that relies on a crafted account says so. Resolves are W4-A's
//! (wave 4): [`World::resolve_through`] stands in for them on the entries a
//! W3-B test needs (musters joining, a Depart's Spend settled into a
//! departed entry), and says so.
//!
//! Marches: [`March`] carries a seal to `T(arrive)` from the fixtures'
//! [`SealKit`](crate::fixtures::tlock::SealKit), and [`find_path`] builds
//! a passable path through crafted provinces.

use fclient::addr::Addresses;
use fclient::ix::{HoldingRef, Player};
use frontier_abi::entry::{read_entry, write_entry, Entry, EntryOp};
use frontier_abi::layout::beacon::bell_anchor as BA;
use frontier_abi::layout::clash::{arrival_day as AD, arrival_slot as AS};
use frontier_abi::layout::player::{
    accrual as AC, citizen as C, holding as H, queue_item as QI, transit as T,
};
use frontier_abi::layout::province::{entry as E, province as P, site as SM};
use frontier_abi::layout::world::season as S;
use frontier_abi::layout::{write_header, AccountKind};
use permutation_rules::fixed::MILLI;
use permutation_rules::frontier::catalog;
use permutation_rules::frontier::geometry::{locate, region_of, ProvinceCoord};
use permutation_rules::frontier::holding::{
    Accrual, Effect, Holding as KHolding, QueueItem, Resource, Tier, QUEUE_SLOTS, RESOURCES,
};
use permutation_rules::frontier::host::Host;
use permutation_rules::frontier::terrain;
use permutation_rules::hex::{Hex, DIRECTIONS};
use permutation_rules::map::Terrain;
use solana_address::Address;
use solana_keypair::Keypair;
use solana_signer::Signer;

use crate::chain::Chain;
use crate::records::le;
use crate::world::World;

/// The ring seed crafted provinces are generated from.
pub const RING_SEED: [u8; 32] = [0x5Au8; 32];

/// `NO_BELL` of an empty pending garrison slot.
pub const NO_BELL: u32 = SM::NO_BELL;

/// Queue item kinds of the program's Holding codec (pinned by W3-B).
pub mod queue_kind {
    pub const FREE: u8 = 0;
    pub const PRODUCTION: u8 = 1;
    pub const UPKEEP: u8 = 2;
    pub const TIER_UP: u8 = 3;
    pub const WALLS: u8 = 4;
}

/// Build item of the tier-up (after the six buildings and the walls).
pub const ITEM_TIER_UP: u8 = catalog::ITEM_COUNT;

/// A player with a final first holding in a crafted province.
pub struct Estate {
    pub wallet: Keypair,
    pub faction: u8,
    pub p: i16,
    pub q: i16,
    pub site: u8,
    pub gen: u8,
    pub tile: u8,
    pub holding: Address,
    pub province: Address,
    pub citizen: Address,
}

impl Estate {
    /// The player prologue with the wallet acting and paying.
    pub fn player(&self) -> Player {
        Player {
            actor: self.wallet.pubkey(),
            payer: self.wallet.pubkey(),
            wallet: self.wallet.pubkey(),
        }
    }
    pub fn href(&self) -> HoldingRef {
        HoldingRef {
            p: self.p,
            q: self.q,
            site: self.site,
        }
    }
    /// The host id of sequence number `seq` of this holding.
    pub fn host_id(&self, seq: u32) -> u64 {
        fclient::addr::host_id(self.p as i32, self.q as i32, self.site, self.gen, seq)
            .expect("host id")
    }
    /// The Citizen's quota tag (first 8 bytes of its address).
    pub fn citizen_tag(&self) -> u64 {
        fclient::addr::citizen_tag_u64(&self.citizen)
    }
}

// ------------------------------------------------------------ Holding codec

fn tier_u8(t: Tier) -> u8 {
    match t {
        Tier::Hamlet => 0,
        Tier::Town => 1,
        Tier::City => 2,
        Tier::Stronghold => 3,
    }
}

fn tier_of(v: u8) -> Tier {
    [Tier::Hamlet, Tier::Town, Tier::City, Tier::Stronghold][v as usize]
}

/// Writes the kernel holding into Holding data (the program's codec).
pub fn write_kholding(d: &mut [u8], h: &KHolding) {
    let put = |d: &mut [u8], o: usize, v: &[u8]| d[o..o + v.len()].copy_from_slice(v);
    d[H::TIER] = tier_u8(h.tier);
    d[H::ORDER] = h.order;
    put(d, H::FOUNDED_TS, &h.founded_ts.to_le_bytes());
    put(d, H::FOUNDED_DAY, &h.founded_day.to_le_bytes());
    put(d, H::LAST_OWNER_ACTION, &h.last_owner_action.to_le_bytes());
    put(d, H::SHIELD_UNTIL, &h.shield_until().to_le_bytes());
    for (i, s) in h.stores.iter().enumerate() {
        let o = H::store(i);
        put(d, o + AC::VALUE, &s.value.to_le_bytes());
        put(d, o + AC::RATE, &s.rate.to_le_bytes());
        put(d, o + AC::CAP, &s.cap.to_le_bytes());
        put(d, o + AC::T0, &s.t0.to_le_bytes());
        put(d, o + AC::FRAC, &s.frac.to_le_bytes());
    }
    for i in 0..RESOURCES {
        put(d, H::PRODUCTION + 8 * i, &h.production[i].to_le_bytes());
        put(d, H::UPKEEP + 8 * i, &h.upkeep[i].to_le_bytes());
    }
    for (i, q) in h.queue.iter().enumerate() {
        let o = H::queue(i);
        let (t, k, a, dl) = match q {
            None => (0, queue_kind::FREE, 0u8, 0i64),
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
        put(d, o + QI::DONE_AT, &t.to_le_bytes());
        d[o + QI::KIND] = k;
        d[o + QI::ARG] = a;
        put(d, o + QI::DELTA, &dl.to_le_bytes());
    }
    put(d, H::WALLS, &h.walls.to_le_bytes());
    put(
        d,
        H::WALLS_COMMITTED_BEFORE,
        &h.walls_committed_before.to_le_bytes(),
    );
    put(d, H::FOOD_SHORTFALL, &h.food_shortfall.to_le_bytes());
}

/// Reads the kernel holding from Holding data (the program's codec).
pub fn read_kholding(d: &[u8]) -> KHolding {
    let i64_at = |o: usize| le(&d[o..o + 8]) as i64;
    let stores = core::array::from_fn(|i| {
        let o = H::store(i);
        Accrual {
            value: i64_at(o + AC::VALUE),
            rate: i64_at(o + AC::RATE),
            cap: i64_at(o + AC::CAP),
            t0: i64_at(o + AC::T0),
            frac: i64_at(o + AC::FRAC),
        }
    });
    let mut queue = [None; QUEUE_SLOTS];
    for (i, q) in queue.iter_mut().enumerate() {
        let o = H::queue(i);
        let done_at = i64_at(o + QI::DONE_AT);
        let arg = d[o + QI::ARG];
        let delta = i64_at(o + QI::DELTA);
        *q = match d[o + QI::KIND] {
            queue_kind::PRODUCTION => Some(Effect::Production {
                resource: Resource::ALL[arg as usize],
                delta,
            }),
            queue_kind::UPKEEP => Some(Effect::Upkeep {
                resource: Resource::ALL[arg as usize],
                delta,
            }),
            queue_kind::TIER_UP => Some(Effect::TierUp),
            queue_kind::WALLS => Some(Effect::Walls {
                delta: delta as u32,
            }),
            _ => None,
        }
        .map(|effect| QueueItem { done_at, effect });
    }
    KHolding {
        tier: tier_of(d[H::TIER]),
        order: d[H::ORDER],
        founded_ts: i64_at(H::FOUNDED_TS),
        founded_day: le(&d[H::FOUNDED_DAY..H::FOUNDED_DAY + 4]) as u32,
        last_owner_action: i64_at(H::LAST_OWNER_ACTION),
        stores,
        production: core::array::from_fn(|i| i64_at(H::PRODUCTION + 8 * i)),
        upkeep: core::array::from_fn(|i| i64_at(H::UPKEEP + 8 * i)),
        queue,
        walls: le(&d[H::WALLS..H::WALLS + 4]) as u32,
        walls_committed_before: i64_at(H::WALLS_COMMITTED_BEFORE),
        food_shortfall: i64_at(H::FOOD_SHORTFALL),
    }
}

/// A first holding as the simulator founds it (`sim.rs` `found`): base
/// production of a Hamlet with its rates applied (`set_upkeep(now, Food,
/// 0)`, the simulator's idiom) and the starter kit.
pub fn founded(now: i64, day: u32) -> KHolding {
    let mut h = KHolding::found(now, day, 1);
    h.production = catalog::base_production(Tier::Hamlet);
    h.set_upkeep(now, Resource::Food, 0).expect("rates");
    for (r, v) in catalog::starter_kit().iter().enumerate() {
        h.credit(now, Resource::ALL[r], *v).expect("credit");
    }
    h
}

// ------------------------------------------------------------ small readers

pub fn u32_at(d: &[u8], o: usize) -> u32 {
    le(&d[o..o + 4]) as u32
}
pub fn u64_at(d: &[u8], o: usize) -> u64 {
    le(&d[o..o + 8])
}
pub fn i64_at(d: &[u8], o: usize) -> i64 {
    le(&d[o..o + 8]) as i64
}
pub fn i16_at(d: &[u8], o: usize) -> i16 {
    i16::from_le_bytes([d[o], d[o + 1]])
}

/// Entries of a Province's data.
pub fn entry_at(d: &[u8], i: usize) -> Entry {
    read_entry(d, i).expect("entry")
}

/// The index of the entry of `host_id` (any non-free state).
pub fn entry_of(d: &[u8], host_id: u64) -> Option<usize> {
    frontier_abi::entry::find_entry(d, host_id)
}

// ------------------------------------------------------------ crafting

impl World {
    /// The current bell.
    pub fn bell(&self, c: &Chain) -> u32 {
        self.bell_at(c.now).expect("after genesis")
    }

    /// `bell_start(b)`.
    pub fn bell_start(&self, b: u32) -> i64 {
        self.genesis_ts() + 600 * b as i64
    }

    /// Moves the Clock to `bell_start(b) + off` (never back).
    pub fn to_bell(&self, c: &mut Chain, b: u32, off: i64) {
        let t = self.bell_start(b) + off;
        if c.now < t {
            c.set_time(t);
        }
    }

    /// Province bytes of `(p, q)` from the kernel terrain under
    /// [`RING_SEED`] (module note), `resolved_next` given.
    pub fn province_bytes(&self, p: i16, q: i16, resolved_next: u32) -> Vec<u8> {
        let pc = ProvinceCoord::new(p as i32, q as i32);
        let t = terrain::generate_province(&RING_SEED, pc);
        let mut d = vec![0u8; P::SIZE];
        assert!(write_header(&mut d, AccountKind::Province, self.id));
        d[P::P..P::P + 2].copy_from_slice(&p.to_le_bytes());
        d[P::Q..P::Q + 2].copy_from_slice(&q.to_le_bytes());
        d[P::RING..P::RING + 2].copy_from_slice(&(pc.ring() as u16).to_le_bytes());
        d[P::WEDGE] = pc.wedge().unwrap_or(0);
        d[P::REGION] = region_of(pc);
        d[P::RESOLVED_NEXT..P::RESOLVED_NEXT + 4].copy_from_slice(&resolved_next.to_le_bytes());
        let (mut pass, mut rough) = (0u64, 0u64);
        for i in 0..P::TILES {
            let tr = t.terrain[i];
            d[P::TERRAIN + i] = tr as u8;
            d[P::RESOURCE + i] = t.resource[i].map_or(0, |r| 1 + r as u8);
            if tr.is_passable() {
                pass |= 1 << i;
            }
            if matches!(tr, Terrain::Forest | Terrain::Hills) {
                rough |= 1 << i;
            }
        }
        d[P::PASSABLE_MASK..P::PASSABLE_MASK + 8].copy_from_slice(&pass.to_le_bytes());
        d[P::ROUGH_MASK..P::ROUGH_MASK + 8].copy_from_slice(&rough.to_le_bytes());
        d[P::SITES..P::SITES + 12].copy_from_slice(&t.sites);
        d[P::SITE_COUNT] = t.site_count;
        for s in 0..P::SITES_N {
            let o = P::site(s);
            d[o + SM::FACTION] = 6;
            d[o + SM::PEND0_BELL..o + SM::PEND0_BELL + 4].copy_from_slice(&NO_BELL.to_le_bytes());
            d[o + SM::PEND1_BELL..o + SM::PEND1_BELL + 4].copy_from_slice(&NO_BELL.to_le_bytes());
        }
        d
    }

    /// A crafted Province at `(p, q)` (module note), resolved through the
    /// bell before the current one.
    pub fn craft_province(&self, c: &mut Chain, p: i16, q: i16) -> Address {
        let k = self.a.province(p as i32, q as i32);
        if !c.is_absent(&k) {
            return k;
        }
        let b = self.bell_at(c.now).unwrap_or(0);
        let d = self.province_bytes(p, q, b);
        c.put_program_account(k, d);
        k
    }

    /// Makes tiles passable (flat) in a crafted Province (terrain byte and
    /// masks together).
    pub fn open_tiles(&self, c: &mut Chain, p: i16, q: i16, tiles: &[u8]) {
        let k = self.a.province(p as i32, q as i32);
        c.edit(&k, |d| {
            let mut pass = u64_at(d, P::PASSABLE_MASK);
            let mut rough = u64_at(d, P::ROUGH_MASK);
            for &t in tiles {
                if pass & (1u64 << t) == 0 {
                    d[P::TERRAIN + t as usize] = Terrain::Plains as u8;
                    rough &= !(1u64 << t);
                }
                pass |= 1u64 << t;
            }
            d[P::PASSABLE_MASK..P::PASSABLE_MASK + 8].copy_from_slice(&pass.to_le_bytes());
            d[P::ROUGH_MASK..P::ROUGH_MASK + 8].copy_from_slice(&rough.to_le_bytes());
        });
    }

    /// Sets a crafted Province's `resolved_next` (stands in for W4-A's
    /// resolves where only the bell counter matters).
    pub fn set_resolved_next(&self, c: &mut Chain, province: &Address, b: u32) {
        c.edit(province, |d| {
            d[P::RESOLVED_NEXT..P::RESOLVED_NEXT + 4].copy_from_slice(&b.to_le_bytes())
        });
    }

    /// Resolves a crafted Province through bell `b` the way ResolveFromInputs
    /// (W4-A) leaves a quiet bell's entries: `resolved_next = b + 1`; every
    /// pending change of a bell ≤ b settled with the kernel (a Spend makes
    /// the entry departed, state 3; a Leave frees it); muster-pending
    /// entries with `from_bell ≤ b + 1` join the roster. A stand-in, used by
    /// SettleDeparture and resident-action tests.
    pub fn resolve_through(&self, c: &mut Chain, province: &Address, b: u32) {
        let rn = b + 1;
        c.edit(province, |d| {
            for i in 0..P::ENTRIES_N {
                let mut e = read_entry(d, i).expect("entry");
                if e.state == E::STATE_FREE {
                    continue;
                }
                if e.state == E::STATE_MUSTER_PENDING && e.from_bell <= rn {
                    e.state = E::STATE_ROSTER;
                }
                match e.op {
                    EntryOp::Spend { .. } if e.pend_bell < rn => {
                        let mut h = e.to_host().unwrap();
                        h.settle(rn).unwrap();
                        e.set_host(&h);
                        e.state = E::STATE_DEPARTED;
                    }
                    EntryOp::Leave if e.pend_bell < rn => e = Entry::FREE,
                    _ => {}
                }
                write_entry(d, i, &e).unwrap();
            }
            d[P::RESOLVED_NEXT..P::RESOLVED_NEXT + 4].copy_from_slice(&rn.to_le_bytes());
        });
    }

    /// A funded player with a final first holding on site `site` of a
    /// crafted province `(p, q)` (module note).
    pub fn craft_estate(
        &self,
        c: &mut Chain,
        label: &str,
        faction: u8,
        pq: (i16, i16),
        site: u8,
    ) -> Estate {
        let (p, q) = pq;
        let wallet = c.funded(format!("estate-{}-{label}", self.id).as_bytes(), 100);
        let province = self.craft_province(c, p, q);
        let pd = c.data(&province);
        assert!(site < pd[P::SITE_COUNT], "site {site} exists");
        let tile = pd[P::SITES + site as usize];
        let gen = 1u8;
        let now = c.now;
        let bell = self.bell_at(now).unwrap_or(0);
        let citizen = self.a.citizen(&wallet.pubkey());
        let holding = self.a.holding(p as i32, q as i32, site);
        // Province site mirror.
        c.edit(&province, |d| {
            let o = P::site(site as usize);
            d[o + SM::STATE] = SM::STATE_HOLDING;
            d[o + SM::FACTION] = faction;
            d[o + SM::ORDER] = 1;
            d[o + SM::GEN] = gen;
            d[P::N_SITES_USED] += 1;
        });
        // Holding.
        let kh = founded(now, bell / 144);
        let mut hd = vec![0u8; H::SIZE];
        assert!(write_header(&mut hd, AccountKind::Holding, self.id));
        hd[H::P..H::P + 2].copy_from_slice(&p.to_le_bytes());
        hd[H::Q..H::Q + 2].copy_from_slice(&q.to_le_bytes());
        hd[H::SITE] = site;
        hd[H::GEN] = gen;
        hd[H::TILE] = tile;
        hd[H::STATE] = H::STATE_FINAL;
        hd[H::OWNER_CITIZEN..H::OWNER_CITIZEN + 32].copy_from_slice(citizen.as_ref());
        hd[H::FACTION] = faction;
        hd[H::TICKET_BELL..H::TICKET_BELL + 4].copy_from_slice(&bell.to_le_bytes());
        hd[H::RENT_PAYER..H::RENT_PAYER + 32].copy_from_slice(wallet.pubkey().as_ref());
        hd[H::FINAL_TS..H::FINAL_TS + 8].copy_from_slice(&now.to_le_bytes());
        write_kholding(&mut hd, &kh);
        // Shield over: tests attack each other's sites unless they set it.
        hd[H::SHIELD_UNTIL..H::SHIELD_UNTIL + 8].copy_from_slice(&0i64.to_le_bytes());
        c.put_program_account(holding, hd);
        // Citizen.
        let mut cd = vec![0u8; C::SIZE];
        assert!(write_header(&mut cd, AccountKind::Citizen, self.id));
        cd[C::WALLET..C::WALLET + 32].copy_from_slice(wallet.pubkey().as_ref());
        cd[C::SESSION_EXPIRY..C::SESSION_EXPIRY + 8].copy_from_slice(&0i64.to_le_bytes());
        cd[C::FACTION] = faction;
        cd[C::FLAGS] = C::FLAG_JOINED | C::FLAG_FIRST_HOLDING_FINAL;
        cd[C::HOLDINGS_N] = 1;
        cd[C::EXPLORES_FLOOR_LEFT] = C::EXPLORES_FLOOR;
        cd[C::JOIN_SHARD] = fclient::addr::join_shard_of(&wallet.pubkey().to_bytes());
        cd[C::BUCKET_MILLI..C::BUCKET_MILLI + 4].copy_from_slice(&60_000u32.to_le_bytes());
        let bt = (now - self.genesis_ts()).max(0) as u32;
        cd[C::BUCKET_T..C::BUCKET_T + 4].copy_from_slice(&bt.to_le_bytes());
        let o = C::HOLDING;
        cd[o..o + 2].copy_from_slice(&p.to_le_bytes());
        cd[o + 2..o + 4].copy_from_slice(&q.to_le_bytes());
        cd[o + 4] = site;
        cd[o + 5] = gen;
        cd[C::TICKET_BELL..C::TICKET_BELL + 4].copy_from_slice(&C::NO_TICKET.to_le_bytes());
        let tag = fclient::addr::citizen_tag_u64(&citizen);
        cd[C::CITIZEN_TAG..C::CITIZEN_TAG + 8].copy_from_slice(&tag.to_le_bytes());
        cd[C::RENT_PAYER..C::RENT_PAYER + 32].copy_from_slice(wallet.pubkey().as_ref());
        c.put_program_account(citizen, cd);
        Estate {
            wallet,
            faction,
            p,
            q,
            site,
            gen,
            tile,
            holding,
            province,
            citizen,
        }
    }

    /// Edits the kernel holding of an estate.
    pub fn edit_kholding(&self, c: &mut Chain, e: &Estate, f: impl FnOnce(&mut KHolding)) {
        let mut d = c.data(&e.holding);
        let mut h = read_kholding(&d);
        f(&mut h);
        write_kholding(&mut d, &h);
        c.set_data(&e.holding, d);
    }

    /// Sets `reserve[unit]` (whole troops).
    pub fn set_reserve(&self, c: &mut Chain, e: &Estate, unit: u8, n: u32) {
        c.edit(&e.holding, |d| {
            let o = H::reserve(unit as usize);
            d[o..o + 4].copy_from_slice(&n.to_le_bytes())
        });
    }

    /// Credits every store with `units` whole units (stands in for hours of
    /// production).
    pub fn enrich(&self, c: &mut Chain, e: &Estate, units: i64) {
        let now = c.now;
        self.edit_kholding(c, e, |h| {
            for r in Resource::ALL {
                h.stores[r as usize].cap = i64::MAX / 4;
                h.credit(now, r, units * MILLI).unwrap();
            }
        });
    }

    /// Puts a roster host (state 1) into `province` at entry `i`, issued by
    /// the estate's holding with sequence `seq` (crafted as Muster plus a
    /// resolve leave it). Returns its id.
    #[allow(clippy::too_many_arguments)]
    pub fn craft_host(
        &self,
        c: &mut Chain,
        e: &Estate,
        province: &Address,
        i: usize,
        seq: u32,
        unit: u8,
        troops: u32,
        tile: u8,
    ) -> u64 {
        let id = e.host_id(seq);
        let b = self.bell_at(c.now).unwrap_or(0);
        let unit_t = frontier_abi::entry::unit_from_u8(unit).unwrap();
        let h = Host::muster(
            id,
            frontier_abi::addr::holding_key_of_host(id),
            e.faction,
            unit_t,
            troops * MILLI as u32,
            b.saturating_sub(2),
        )
        .unwrap();
        let entry = Entry::from_host(&h, tile, E::STATE_ROSTER, 10_000, b.saturating_sub(1));
        c.edit(province, |d| {
            write_entry(d, i, &entry).unwrap();
            d[P::N_ENTRIES] += 1;
        });
        c.edit(&e.holding, |d| {
            let s = u32_at(d, H::HOST_SEQ).max(seq + 1);
            d[H::HOST_SEQ..H::HOST_SEQ + 4].copy_from_slice(&s.to_le_bytes());
        });
        id
    }

    /// A crafted BellAnchor `(bell, region)` as PostAnchor leaves it, with
    /// `A` = `a` and the slot given (for release-build measurements, whose
    /// fixture rounds do not cover later bells).
    pub fn craft_anchor(&self, c: &mut Chain, bell: u32, region: u8, a: i64, slot: u64) -> Address {
        let k = self.a.anchor(bell, region);
        let mut d = vec![0u8; BA::SIZE];
        assert!(write_header(&mut d, AccountKind::BellAnchor, self.id));
        d[BA::BELL..BA::BELL + 4].copy_from_slice(&bell.to_le_bytes());
        d[BA::REGION] = region;
        d[BA::NET] = S::NETWORK_QUICKNET;
        d[BA::ROUND..BA::ROUND + 8].copy_from_slice(&self.tlock_round(bell).to_le_bytes());
        d[BA::A..BA::A + 8].copy_from_slice(&a.to_le_bytes());
        d[BA::SLOT..BA::SLOT + 8].copy_from_slice(&slot.to_le_bytes());
        d[BA::RENT_TO..BA::RENT_TO + 32].copy_from_slice(self.keeper.pubkey().as_ref());
        c.put_program_account(k, d);
        k
    }

    /// A crafted ArrivalSlot as a Reveal leaves it.
    #[allow(clippy::too_many_arguments)]
    pub fn craft_slot(
        &self,
        c: &mut Chain,
        dest: (i32, i32),
        bell: u32,
        faction: u8,
        i: u8,
        host_id: u64,
        citizen_tag: u64,
        dep_mass: u32,
    ) -> Address {
        let k = self.a.arrival_slot(dest.0, dest.1, bell, faction, i);
        let mut d = vec![0u8; AS::SIZE];
        assert!(write_header(&mut d, AccountKind::ArrivalSlot, self.id));
        d[AS::P..AS::P + 2].copy_from_slice(&(dest.0 as i16).to_le_bytes());
        d[AS::Q..AS::Q + 2].copy_from_slice(&(dest.1 as i16).to_le_bytes());
        d[AS::BELL..AS::BELL + 4].copy_from_slice(&bell.to_le_bytes());
        d[AS::FACTION] = faction;
        d[AS::I] = i;
        d[AS::HOST_ID..AS::HOST_ID + 8].copy_from_slice(&host_id.to_le_bytes());
        d[AS::CITIZEN_TAG..AS::CITIZEN_TAG + 8].copy_from_slice(&citizen_tag.to_le_bytes());
        d[AS::DEP_MASS..AS::DEP_MASS + 4].copy_from_slice(&dep_mass.to_le_bytes());
        d[AS::RENT_TO..AS::RENT_TO + 32].copy_from_slice(self.keeper.pubkey().as_ref());
        c.put_program_account(k, d);
        k
    }

    /// A crafted ArrivalDay with the bits of `bells` set.
    pub fn craft_day(&self, c: &mut Chain, dest: (i32, i32), day: u32, bells: &[u32]) -> Address {
        let k = self.a.arrival_day(dest.0, dest.1, day);
        let mut d = vec![0u8; AD::SIZE];
        assert!(write_header(&mut d, AccountKind::ArrivalDay, self.id));
        d[AD::P..AD::P + 2].copy_from_slice(&(dest.0 as i16).to_le_bytes());
        d[AD::Q..AD::Q + 2].copy_from_slice(&(dest.1 as i16).to_le_bytes());
        d[AD::DAY..AD::DAY + 4].copy_from_slice(&day.to_le_bytes());
        for b in bells {
            let (at, m) = AD::bit(*b);
            d[at] |= m;
        }
        d[AD::RENT_TO..AD::RENT_TO + 32].copy_from_slice(self.keeper.pubkey().as_ref());
        c.put_program_account(k, d);
        k
    }
}

// ------------------------------------------------------------ paths

/// One step of a path: the direction and where it lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub dir: u8,
    pub p: i32,
    pub q: i32,
    pub tile: u8,
}

/// The steps of `dirs` from tile `tile` of province `(p, q)`.
pub fn trace(origin: (i32, i32, u8), dirs: &[u8]) -> Vec<Step> {
    let mut h = ProvinceCoord::new(origin.0, origin.1)
        .tile(origin.2)
        .expect("tile");
    dirs.iter()
        .map(|&d| {
            let (dq, dr) = DIRECTIONS[d as usize];
            h = Hex::new(h.q + dq, h.r + dr);
            let (pc, t) = locate(h);
            Step {
                dir: d,
                p: pc.p,
                q: pc.q,
                tile: t,
            }
        })
        .collect()
}

/// The path provinces Reveal takes (distinct, not the destination, in
/// first-entered order).
pub fn path_provinces(steps: &[Step]) -> Vec<(i32, i32)> {
    let last = steps.last().expect("a step");
    let mut v: Vec<(i32, i32)> = vec![];
    for s in steps {
        if (s.p, s.q) != (last.p, last.q) && !v.contains(&(s.p, s.q)) {
            v.push((s.p, s.q));
        }
    }
    v
}

/// A straight march of `n` steps in direction `dir`, then back-and-forth
/// pairs and one triangle as needed to use exactly `total` steps ending on
/// a tile of the last province: the worst-case walk (many steps, many
/// provinces). Returns the directions.
pub fn padded_line(origin: (i32, i32, u8), dir: u8, n: usize, total: usize) -> Vec<u8> {
    let mut dirs = vec![dir; n];
    let back = (dir + 3) % 6;
    // A triangle (0 → 4 → 2 returns home) fixes an odd remainder.
    if (total - n) % 2 == 1 {
        let tri = [dir, (dir + 4) % 6, (dir + 2) % 6];
        dirs.extend_from_slice(&tri);
    }
    while dirs.len() < total {
        // Step back and forth inside the last province.
        dirs.push(back);
        dirs.push(dir);
    }
    let steps = trace(origin, &dirs);
    let last = steps.last().unwrap();
    let back_at = trace(origin, &dirs[..n]);
    let l = back_at.last().unwrap();
    assert_eq!(
        (last.p, last.q, last.tile),
        (l.p, l.q, l.tile),
        "pads return home"
    );
    dirs
}

/// Opens every tile a path touches (crafted provinces, module note).
pub fn open_path(w: &World, c: &mut Chain, origin: (i32, i32, u8), dirs: &[u8]) {
    let steps = trace(origin, dirs);
    let mut by: Vec<((i32, i32), Vec<u8>)> = vec![];
    for s in &steps {
        match by.iter_mut().find(|(k, _)| *k == (s.p, s.q)) {
            Some((_, v)) => v.push(s.tile),
            None => by.push(((s.p, s.q), vec![s.tile])),
        }
    }
    for ((p, q), tiles) in by {
        w.craft_province(c, p as i16, q as i16);
        w.open_tiles(c, p as i16, q as i16, &tiles);
    }
}

/// Travel seconds of a path over crafted provinces (as Reveal computes it
/// before the doctrine bias): 120 s flat, 180 s rough, halved mounted.
pub fn path_secs(c: &Chain, a: &Addresses, steps: &[Step], cavalry: bool) -> u32 {
    steps
        .iter()
        .map(|s| {
            let d = c.data(&a.province(s.p, s.q));
            let rough = u64_at(&d, P::ROUGH_MASK) & (1u64 << s.tile) != 0;
            let secs = if rough { 180 } else { 120 };
            if cavalry {
                secs / 2
            } else {
                secs
            }
        })
        .sum()
}

/// A breadth-first path of ≤ 32 steps over the passable tiles of crafted
/// provinces from `from` to `to` (global hexes via `locate`), if any.
pub fn find_path(
    c: &Chain,
    a: &Addresses,
    from: (i32, i32, u8),
    to: (i32, i32, u8),
) -> Option<Vec<u8>> {
    use std::collections::{HashMap, VecDeque};
    let start = ProvinceCoord::new(from.0, from.1).tile(from.2)?;
    let goal = ProvinceCoord::new(to.0, to.1).tile(to.2)?;
    let passable = |h: Hex| {
        let (pc, t) = locate(h);
        let d = c.data(&a.province(pc.p, pc.q));
        !d.is_empty() && u64_at(&d, P::PASSABLE_MASK) & (1u64 << t) != 0
    };
    let mut prev: HashMap<Hex, (Hex, u8)> = HashMap::new();
    let mut q = VecDeque::from([(start, 0usize)]);
    prev.insert(start, (start, 9));
    while let Some((h, n)) = q.pop_front() {
        if h == goal {
            let mut dirs = vec![];
            let mut x = h;
            while x != start {
                let (p, d) = prev[&x];
                dirs.push(d);
                x = p;
            }
            dirs.reverse();
            return Some(dirs);
        }
        if n == 32 {
            continue;
        }
        for (d, (dq, dr)) in DIRECTIONS.iter().enumerate() {
            let nh = Hex::new(h.q + dq, h.r + dr);
            if prev.contains_key(&nh) || !passable(nh) {
                continue;
            }
            prev.insert(nh, (h, d as u8));
            q.push_back((nh, n + 1));
        }
    }
    None
}

/// The transit record `i` of a Holding's data: `(state, host_id,
/// arrive_bell, dep_mass)`.
pub fn transit_of(d: &[u8], i: usize) -> (u8, u64, u32, u32) {
    let o = H::transit(i);
    (
        d[o + T::STATE],
        u64_at(d, o + T::HOST_ID),
        u32_at(d, o + T::ARRIVE_BELL),
        u32_at(d, o + T::DEP_MASS),
    )
}

// ------------------------------------------------------------ marches

/// A march: its plaintext, its seal to `T(arrive)` and what Depart and
/// Reveal need.
#[derive(Clone, Debug)]
pub struct March {
    pub host_id: u64,
    pub transit_slot: u8,
    pub arrive: u32,
    pub dest: (i32, i32),
    pub dest_tile: u8,
    pub plain: Plain,
    pub made: MadeSeal,
    /// Path provinces other than the destination, first-entered order.
    pub path: Vec<(i32, i32)>,
}

use crate::fixtures::tlock::{MadeSeal, SealCase, SealKit};
use fclient::ix::{DepartArgs, RevealArgs};
use permutation_rules::frontier::seal::{encode_path, Plain};
use solana_instruction::Instruction;

impl World {
    /// Plans a march of `host_id` from `origin` along `dirs`, arriving at
    /// `arrive`, sealed as `case` to `T(arrive)`.
    #[allow(clippy::too_many_arguments)]
    pub fn plan_march(
        &self,
        host_id: u64,
        transit_slot: u8,
        origin: (i32, i32, u8),
        dirs: &[u8],
        arrive: u32,
        stance: u8,
        retreat_bps: u16,
        case: SealCase,
    ) -> March {
        let steps = trace(origin, dirs);
        let last = *steps.last().expect("a step");
        let (n, path) = encode_path(dirs).expect("path");
        let plain = Plain {
            version: 1,
            host_id,
            arrive_bell: arrive,
            dest_p: last.p as i16,
            dest_q: last.q as i16,
            dest_tile: last.tile,
            stance,
            retreat_bps,
            path_len: n,
            path,
            reserved: [0; 3],
        };
        let made = SealKit::new(&self.beacons).make(case, &plain, self.tlock_round(arrive));
        March {
            host_id,
            transit_slot,
            arrive,
            dest: (last.p, last.q),
            dest_tile: last.tile,
            plain,
            made,
            path: path_provinces(&steps),
        }
    }

    /// The Season's `tip_min` (§10.1).
    pub fn tip_min(&self, c: &Chain) -> u64 {
        let d = c.data(&self.a.season);
        permutation_rules::frontier::fees::min_tip_lamports(
            u32_at(&d, S::MIN_REVEAL_PRIORITY_MILLI),
            u32_at(&d, S::REVEAL_CU_LIMIT),
            u32_at(&d, S::REVEAL_LOADED_LIMIT),
        )
    }

    /// Depart of `m` from the province the host stands in.
    pub fn depart_ix(&self, e: &Estate, host_at: (i16, i16), m: &March, tip: u64) -> Instruction {
        fclient::ix::depart(
            &self.a,
            &e.player(),
            e.href(),
            host_at,
            &DepartArgs {
                host_id: m.host_id,
                commit: m.made.commit,
                seal: m.made.seal,
                arrive_bell: m.arrive,
                tip,
                transit_slot: m.transit_slot,
            },
        )
    }

    /// Reveal of `m` into slot `target_i`.
    pub fn reveal_ix(
        &self,
        fee_payer: &Address,
        e: &Estate,
        m: &March,
        target_i: u8,
        day_writable: bool,
    ) -> Instruction {
        fclient::ix::reveal(
            &self.a,
            *fee_payer,
            &RevealArgs {
                holding: e.href(),
                transit_slot: m.transit_slot,
                target_i,
                plain: m.made.plain,
                salt: m.made.salt,
                ct_hash: m.made.ct_hash,
                beneficiary: *fee_payer,
                dest: m.dest,
                arrive: m.arrive,
                faction: e.faction,
                day_writable,
                path_provinces: m.path.clone(),
            },
        )
    }

    /// The region of a province.
    pub fn region(p: i32, q: i32) -> u8 {
        region_of(ProvinceCoord::new(p, q))
    }
}
