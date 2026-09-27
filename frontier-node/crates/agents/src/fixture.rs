//! A small, fixed herald world for the unit tests of `frontier-agents` and
//! `frontier-bots` (M1 contract §3.5: off-chain tests first, binaries only
//! add IO; §11 W3-E: "unit tests against recorded herald fixtures").
//!
//! **What it is.** The files a herald would serve (§8.4) for one season at
//! bell 40: `/h/season`, `/h/me/{wallet}` for four bot wallets (not
//! joined, joined without a holding, a provisional holding, a final holding
//! with a combat host and a scout), `/h/province/{P},{Q}/latest` for rings
//! 1–2, `/h/overview/{1,2}/latest.bin` and `/h/bell/40/region/{r}`. The
//! land is the kernel's (`terrain::generate_province`, `camp::place`), the
//! account bytes are written at the `fclient::abi::layout` offsets and read
//! back with `fclient::decode`, and the beacon is the test key's.
//!
//! **What it is not.** It was not recorded from a running herald: W3-D's
//! herald is built in the same wave. The files are written by exactly one
//! producer ([`world`]) and checked for freshness by exactly one test
//! (`fixtures_are_fresh`, `FRONTIER_WRITE_FIXTURES=1` rewrites them), as
//! §3.5 asks of every vector file; W4-F re-records them from the herald of
//! the first in-process day and the parsers here must accept both.

use std::collections::BTreeMap;

use fclient::abi::layout::{self as l, citizen as lc, entry as le, holding as lh, site as ls};
use fclient::abi::{magic, size, status};
use fclient::addr::Addresses;
use fclient::beacon::TestKey;
use fclient::{Address, Signer};
use permutation_rules::frontier::camp;
use permutation_rules::frontier::catalog;
use permutation_rules::frontier::doctrine::DOCTRINES;
use permutation_rules::frontier::geometry::{ring_provinces, ProvinceCoord};
use permutation_rules::frontier::holding::Tier;
use permutation_rules::frontier::terrain::{generate_province, ProvinceTerrain};
use permutation_rules::map::Terrain;
use serde_json::json;

use crate::obs::{b64_encode, Overview, OverviewRec};

/// The fleet seed of the fixture wallets (`keys::wallet(SEED, i)`).
pub const SEED: u64 = 7;
pub const SEASON_ID: u64 = 1;
pub const GENESIS_TS: i64 = 1_800_000_000;
pub const BELL: u32 = 40;
/// Game time of the fixture: 100 s into bell 40.
pub const NOW: i64 = GENESIS_TS + BELL as i64 * 600 + 100;
pub const SLOT: u64 = 123_456;
/// Fixture wallets by `keys::wallet(SEED, index)`.
pub const UNJOINED: u32 = 0;
pub const JOINED: u32 = 1;
pub const PROVISIONAL: u32 = 2;
pub const FINAL: u32 = 3;
/// A faction-1 wallet whose holding the faction-0 bots may attack.
pub const ENEMY: u32 = 4;
pub const RING_SEEDS: [[u8; 32]; 3] = [[1; 32], [2; 32], [3; 32]];
/// The `FINAL` wallet's march in transit: host sequence 5 in transit slot
/// 3, departed at bell 35, arriving at bell 38 (values settled, state 2).
pub const IN_TRANSIT_SEQ: u32 = 5;
pub const IN_TRANSIT_SLOT: u8 = 3;
pub const IN_TRANSIT_ARRIVE: u32 = 38;
/// The in-transit march's arrival at the enemy's home province: its
/// ArrivalSlot index, the slot's beneficiary and the ClashInputs resolver,
/// served in the immutable per-bell envelope `/h/province/{P},{Q}/38` (the
/// latest envelope is bell 40's, as a herald serves it once the province
/// resolved past 38; wave-3 review, W3-E).
pub const IN_TRANSIT_SLOT_I: u8 = 2;
pub const SLOT_BENEFICIARY: [u8; 32] = [0x5B; 32];
pub const RESOLVER: [u8; 32] = [0x5E; 32];
/// `L(reveal)` the fixture season records.
pub const REVEAL_LOADED_LIMIT: u32 = 1_048_576;

pub fn program() -> Address {
    Address::new_from_array([0x42; 32])
}

pub fn addresses() -> Addresses {
    Addresses::new(program(), SEASON_ID)
}

/// Byte writer at fixed offsets.
struct W(Vec<u8>);

impl W {
    fn new(n: usize) -> W {
        W(vec![0; n])
    }
    fn b(&mut self, o: usize, v: &[u8]) -> &mut Self {
        self.0[o..o + v.len()].copy_from_slice(v);
        self
    }
    fn u8(&mut self, o: usize, v: u8) -> &mut Self {
        self.0[o] = v;
        self
    }
    fn u16(&mut self, o: usize, v: u16) -> &mut Self {
        self.b(o, &v.to_le_bytes())
    }
    fn i16(&mut self, o: usize, v: i16) -> &mut Self {
        self.b(o, &v.to_le_bytes())
    }
    fn u32(&mut self, o: usize, v: u32) -> &mut Self {
        self.b(o, &v.to_le_bytes())
    }
    fn u64(&mut self, o: usize, v: u64) -> &mut Self {
        self.b(o, &v.to_le_bytes())
    }
    fn i64(&mut self, o: usize, v: i64) -> &mut Self {
        self.b(o, &v.to_le_bytes())
    }
    fn key(&mut self, o: usize, k: &Address) -> &mut Self {
        self.b(o, k.as_ref())
    }
    fn chained(&mut self, m: &[u8; 8], seq: u64) -> &mut Self {
        self.b(l::h::MAGIC, m)
            .u64(l::h::SEASON_ID, SEASON_ID)
            .u16(l::h::LAYOUT_VERSION, 1)
            .u64(l::h::EVENT_SEQ, seq)
            .b(l::h::EVENT_HEAD, &[seq as u8; 32])
    }
    fn short(&mut self, m: &[u8; 8]) -> &mut Self {
        self.b(l::sh::MAGIC, m).u64(l::sh::SEASON_ID, SEASON_ID)
    }
}

fn season_bytes(drand: &fclient::ports::ChainInfo) -> Vec<u8> {
    use l::season as s;
    let mut w = W::new(size::SEASON);
    w.chained(magic::SEASON, 812)
        .u8(s::STATUS, status::SEEDED)
        .u8(s::REGIONS, 16)
        .u8(s::GENESIS_RING, 2)
        .u16(s::R_MAX, 16)
        .u32(s::BELL_SECS, 600)
        .i64(s::GENESIS_TS, GENESIS_TS)
        .i64(s::CREATED_TS, GENESIS_TS - 86_400)
        .u32(s::JOIN_CLOSE_BELL, 6 * 144)
        .u32(s::END_BELL, 7 * 144)
        .i64(s::DRAND_GENESIS, drand.genesis_time)
        .u32(s::DRAND_PERIOD, drand.period)
        .u8(s::NETWORK, 2)
        .b(
            s::QUICKNET_PK_HASH,
            &fclient::beacon::pk_hash(&drand.public_key),
        )
        .u32(s::REVEAL_WINDOW, 600)
        .u32(s::SEED_MARGIN, 60)
        .u32(s::WINDOW_NEXT, 600)
        .u32(s::WINDOW_FROM_BELL, u32::MAX)
        .u32(s::ARCHIVE_AFTER, 48 * 3_600)
        .u8(s::MIN_LEAD, 2)
        .u8(s::MAX_LEAD, 72)
        .u8(s::TRANSIT_SLOTS, 4)
        .u64(s::MARCH_FEE, 10_000)
        .u64(s::SEAL_BOND, 20_000)
        .u32(s::MIN_REVEAL_PRIORITY_MILLI, 433)
        .u32(s::REVEAL_CU_LIMIT, 26_000)
        .u16(s::BUCKET_RATE_PER_H, 30)
        .u16(s::BUCKET_BURST, 60)
        .u32(s::DEFENCE_CAP_MILLI, 2_000)
        .u32(s::REVEAL_LOADED_LIMIT, REVEAL_LOADED_LIMIT);
    w.0
}

/// A province of the fixture: kernel terrain and camp, plus what the
/// fixture places on it.
struct Prov {
    pc: ProvinceCoord,
    t: ProvinceTerrain,
    camp: Option<camp::Camp>,
    /// (site index, faction, gen, shield_until_bell).
    holdings: Vec<(u8, u8, u8, u32)>,
    entries: Vec<[u8; 48]>,
}

fn terrain_code(t: Terrain) -> u8 {
    t as u8
}

fn rough(t: Terrain) -> bool {
    matches!(t, Terrain::Forest | Terrain::Hills)
}

fn province_bytes(p: &Prov) -> Vec<u8> {
    use l::province as o;
    let mut w = W::new(size::PROVINCE);
    let ring = p.pc.ring();
    w.chained(magic::PROVINCE, 9)
        .i16(o::P, p.pc.p as i16)
        .i16(o::Q, p.pc.q as i16)
        .u16(o::RING, ring as u16)
        .u8(o::WEDGE, p.pc.wedge().unwrap_or(0))
        .u8(o::REGION, fclient::ix::region_of(p.pc.p, p.pc.q))
        .u32(o::RESOLVED_NEXT, BELL)
        .u32(o::OPENED_BELL, 0)
        .u8(o::N_ENTRIES, p.entries.len() as u8)
        .u8(o::N_SITES_USED, p.holdings.len() as u8);
    let (mut pass, mut rgh) = (0u64, 0u64);
    for i in 0..61usize {
        let t = p.t.terrain[i];
        w.u8(o::TERRAIN + i, terrain_code(t));
        if t.is_passable() {
            pass |= 1 << i;
        }
        if rough(t) {
            rgh |= 1 << i;
        }
    }
    w.b(o::SITES, &p.t.sites)
        .u8(o::SITE_COUNT, p.t.site_count)
        .u64(o::PASSABLE_MASK, pass)
        .u64(o::ROUGH_MASK, rgh);
    for s in 0..p.t.site_count as usize {
        let base = o::SITE_MIRROR + s * o::SITE_MIRROR_STRIDE;
        let st = if ring < 2 {
            ls::STATE_RESERVED
        } else {
            ls::STATE_FREE
        };
        w.u8(base + ls::STATE, st);
    }
    for &(s, f, gen, shield) in &p.holdings {
        let base = o::SITE_MIRROR + s as usize * o::SITE_MIRROR_STRIDE;
        w.u8(base + ls::STATE, ls::STATE_HOLDING)
            .u8(base + ls::FACTION, f)
            .u8(base + ls::ORDER, 1)
            .u8(base + ls::GEN, gen)
            .u32(base + ls::GARRISON, 200)
            .u32(base + ls::SHIELD_UNTIL_BELL, shield);
    }
    for (i, e) in p.entries.iter().enumerate() {
        w.b(o::ENTRIES + i * o::ENTRY_STRIDE, e);
    }
    if let Some(c) = p.camp {
        w.u8(o::CAMP, c.tile)
            .u8(o::CAMP + 1, 1)
            .u32(o::CAMP + 4, c.troops)
            .u32(o::CAMP + 8, 1);
    }
    w.0
}

fn entry(id: u64, faction: u8, unit: u8, tile: u8, troops: u32) -> [u8; 48] {
    let mut w = W::new(48);
    w.u64(le::ID, id)
        .u8(le::FACTION, faction)
        .u8(le::UNIT, unit)
        .u8(le::TILE, tile)
        .u8(le::STATE, le::STATE_ROSTER)
        .u32(le::TROOPS, troops)
        .u16(le::STAMINA_VALUE, 120)
        .u32(le::STAMINA_BELL, 30)
        .u32(le::READY_BELL, 30)
        .u32(le::FROM_BELL, 30);
    w.0.try_into().expect("48")
}

#[allow(clippy::too_many_arguments)]
fn holding_bytes(
    pc: ProvinceCoord,
    site: u8,
    tile: u8,
    state: u8,
    owner_citizen: &Address,
    faction: u8,
    host_seq: u32,
    reserve: [u32; 8],
    rent_payer: &Address,
) -> Vec<u8> {
    let mut w = W::new(size::HOLDING);
    w.chained(magic::HOLDING, 5)
        .i16(lh::P, pc.p as i16)
        .i16(lh::Q, pc.q as i16)
        .u8(lh::SITE, site)
        .u8(lh::GEN, 1)
        .u8(lh::TILE, tile)
        .u8(lh::STATE, state)
        .key(lh::OWNER_CITIZEN, owner_citizen)
        .u64(lh::TICKET_SCORE, 0x1234_5678)
        .u8(lh::FACTION, faction)
        .u8(lh::ORDER, 1)
        .u8(lh::TIER, 0)
        .u32(lh::TICKET_BELL, 20)
        .i64(lh::FOUNDED_TS, GENESIS_TS + 20 * 600)
        .u32(lh::HOST_SEQ, host_seq)
        .i64(lh::LAST_OWNER_ACTION, NOW - 3_600)
        .i64(lh::SHIELD_UNTIL, GENESIS_TS + 30 * 600)
        .key(lh::RENT_PAYER, rent_payer)
        .i64(lh::FINAL_TS, GENESIS_TS + 22 * 600);
    let base = catalog::base_production(Tier::Hamlet);
    for (r, &rate) in base.iter().enumerate() {
        let o = lh::STORES + r * lh::ACCRUAL_STRIDE;
        // 2,000 units of each good an hour ago, filling at the base rate.
        w.i64(o, 2_000_000)
            .i64(o + 8, rate)
            .i64(o + 16, 50_000_000)
            .i64(o + 24, NOW - 3_600);
        w.i64(lh::PRODUCTION + 8 * r, rate);
    }
    for (u, n) in reserve.iter().enumerate() {
        w.u32(lh::RESERVE + 4 * u, *n);
    }
    w.0
}

#[allow(clippy::too_many_arguments)]
fn citizen_bytes(
    wallet: &Address,
    session: &Address,
    faction: u8,
    flags: u8,
    holding: Option<(ProvinceCoord, u8)>,
    ticket_bell: u32,
    rent_payer: &Address,
) -> Vec<u8> {
    let mut w = W::new(size::CITIZEN);
    let a = addresses();
    w.chained(magic::CITIZEN, 3)
        .key(lc::WALLET, wallet)
        .key(lc::SESSION, session)
        .i64(lc::SESSION_EXPIRY, NOW + 29 * 86_400)
        .u8(lc::FACTION, faction)
        .u8(lc::FLAGS, flags)
        .u8(lc::HOLDINGS_N, holding.is_some() as u8)
        .u8(lc::EXPLORES_FLOOR_LEFT, 3)
        .u32(lc::JOIN_BELL, 10)
        .u8(
            lc::JOIN_SHARD,
            fclient::addr::join_shard_of(&wallet.to_bytes()),
        )
        .u32(lc::BUCKET_MILLI, 60_000)
        .u32(lc::TICKET_BELL, ticket_bell)
        .u64(
            lc::CITIZEN_TAG,
            fclient::addr::citizen_tag_u64(&a.citizen(wallet)),
        )
        .i64(lc::LAST_ACTION_TS, NOW - 3_600)
        .key(lc::RENT_PAYER, rent_payer);
    if let Some((pc, site)) = holding {
        w.i16(lc::HOLDING, pc.p as i16)
            .i16(lc::HOLDING + 2, pc.q as i16)
            .u8(lc::HOLDING + 4, site)
            .u8(lc::HOLDING + 5, 1);
    }
    w.0
}

/// Writes a transit record (state 2) into a Holding's bytes.
fn with_transit(
    mut b: Vec<u8>,
    slot: u8,
    host_id: u64,
    origin: (ProvinceCoord, u8),
    unit: u8,
    depart: u32,
    arrive: u32,
) -> Vec<u8> {
    use l::transit as t;
    let o = lh::TRANSIT + slot as usize * lh::TRANSIT_STRIDE;
    let mut w = W(std::mem::take(&mut b));
    w.u8(o + t::STATE, 2)
        .u8(o + t::UNIT, unit)
        .u8(o + t::FACTION, 0)
        .u8(o + t::ORIGIN_TILE, origin.1)
        .i16(o + t::ORIGIN_P, origin.0.p as i16)
        .i16(o + t::ORIGIN_Q, origin.0.q as i16)
        .u64(o + t::HOST_ID, host_id)
        .u32(o + t::DEPART_BELL, depart)
        .u32(o + t::ARRIVE_BELL, arrive)
        .i64(o + t::DEPART_TS, GENESIS_TS + depart as i64 * 600 + 30)
        .u32(o + t::DEP_MASS, 200)
        .u16(o + t::MARCH_STAMINA, 20)
        .u32(o + t::TROOPS_AFTER, 200)
        .u16(o + t::STAMINA_AFTER, 100)
        .b(o + t::SEAL_ROOT, &[0x5E; 32])
        .u64(o + t::TIP, 20_000)
        .u8(o + t::FLAGS, 3);
    w.0
}

fn anchor_bytes(key: &TestKey, bell: u32, region: u8, round: u64, a: i64) -> Vec<u8> {
    use l::bell_anchor as o;
    let mut w = W::new(size::BELL_ANCHOR);
    w.short(magic::BELL_ANCHOR)
        .u32(o::BELL, bell)
        .u8(o::REGION, region)
        .u8(o::NET, 2)
        .u64(o::ROUND, round)
        .i64(o::A, a)
        .u64(o::SLOT, SLOT - 30)
        .b(o::SIG48, &key.sign(round))
        .key(o::RENT_TO, &Address::new_from_array([9; 32]));
    w.0
}

/// The fixture world: `path → bytes` (paths as the herald serves them,
/// without the leading `/`, `.json` or `.bin` appended).
pub struct World {
    pub files: BTreeMap<String, Vec<u8>>,
    /// The faction-0 holding province (wallet `FINAL`) and its neighbour
    /// with the faction-1 holding.
    pub home: (i16, i16),
    pub enemy_home: (i16, i16),
}

fn pretty(v: &serde_json::Value) -> Vec<u8> {
    let mut s = serde_json::to_string_pretty(v).expect("json");
    s.push('\n');
    s.into_bytes()
}

/// Builds the fixture world (deterministic).
pub fn world() -> World {
    let key = TestKey::new();
    let drand = key.info();
    let a = addresses();
    let wallet = |i: u32| crate::keys::wallet(SEED, i).pubkey();
    let session = |i: u32| crate::keys::session(SEED, i).pubkey();
    let funder = Address::new_from_array([0x11; 32]);
    let mut files = BTreeMap::new();

    // ---- provinces of rings 1 and 2
    let mut provs: Vec<Prov> = vec![];
    for d in 1..=2u32 {
        for pc in ring_provinces(d) {
            let t = generate_province(&RING_SEEDS[d as usize], pc);
            let camp = camp::place(&RING_SEEDS[d as usize], pc, &t, 0, false, true);
            provs.push(Prov {
                pc,
                t,
                camp,
                holdings: vec![],
                entries: vec![],
            });
        }
    }
    // Home: the first ring-2 province of wedge 0 with ≥ 3 sites; the enemy's
    // home: a ring-2 neighbour of it in another wedge.
    let hi = provs
        .iter()
        .position(|p| p.pc.ring() == 2 && p.pc.wedge() == Some(0) && p.t.site_count >= 3)
        .expect("a wedge-0 province");
    let home = provs[hi].pc;
    let ei = provs
        .iter()
        .position(|p| {
            p.pc.ring() == 2
                && p.pc.wedge() != Some(0)
                && p.pc.distance(home) == 1
                && p.t.site_count >= 2
        })
        .or_else(|| {
            provs
                .iter()
                .position(|p| p.pc.ring() == 2 && p.pc.wedge() != Some(0) && p.t.site_count >= 2)
        })
        .expect("an enemy province");
    let enemy_home = provs[ei].pc;
    let final_tile = provs[hi].t.sites[0];
    let prov_tile = provs[hi].t.sites[1];
    let unit0 = DOCTRINES[0].unit as u8;
    let id1 = fclient::addr::host_id(home.p, home.q, 0, 1, 1).expect("host id");
    let id2 = fclient::addr::host_id(home.p, home.q, 0, 1, 2).expect("host id");
    provs[hi].holdings.push((0, 0, 1, 30));
    provs[hi].holdings.push((1, 0, 1, 30));
    provs[hi]
        .entries
        .push(entry(id1, 0, unit0, final_tile, 500));
    provs[hi].entries.push(entry(id2, 0, 6, final_tile, 100));
    let enemy_tile = provs[ei].t.sites[1];
    provs[ei].holdings.push((1, 1, 1, 30));

    for p in &provs {
        let pv = province_bytes(p);
        let env = json!({
            "v": 1,
            "key": format!("pv:{},{}", p.pc.p, p.pc.q),
            "bell": BELL,
            "slot": SLOT - 10,
            "seq": "9",
            "head": hex::encode([9u8; 32]),
            "bytes": b64_encode(&pv),
            "slots": [],
            "day": null,
            "inputs": null,
        });
        files.insert(
            format!("h/province/{},{}/latest.json", p.pc.p, p.pc.q),
            pretty(&env),
        );
    }

    // ---- the arrival bell's own envelope of the in-transit march
    {
        let pe = provs.iter().find(|p| p.pc == enemy_home).expect("enemy");
        let host = fclient::addr::host_id(home.p, home.q, 0, 1, IN_TRANSIT_SEQ).expect("host id");
        let mut sl = vec![0u8; size::ARRIVAL_SLOT];
        sl[..8].copy_from_slice(magic::ARRIVAL_SLOT);
        sl[l::sh::SEASON_ID..l::sh::SEASON_ID + 8].copy_from_slice(&SEASON_ID.to_le_bytes());
        use l::arrival_slot as sa;
        sl[sa::P..sa::P + 2].copy_from_slice(&(enemy_home.p as i16).to_le_bytes());
        sl[sa::Q..sa::Q + 2].copy_from_slice(&(enemy_home.q as i16).to_le_bytes());
        sl[sa::BELL..sa::BELL + 4].copy_from_slice(&IN_TRANSIT_ARRIVE.to_le_bytes());
        sl[sa::FACTION] = 0;
        sl[sa::I] = IN_TRANSIT_SLOT_I;
        sl[sa::HOST_ID..sa::HOST_ID + 8].copy_from_slice(&host.to_le_bytes());
        sl[sa::BENEFICIARY..sa::BENEFICIARY + 32].copy_from_slice(&SLOT_BENEFICIARY);
        let mut ci = vec![0u8; size::CLASH_INPUTS];
        ci[..8].copy_from_slice(magic::CLASH_INPUTS);
        ci[l::sh::SEASON_ID..l::sh::SEASON_ID + 8].copy_from_slice(&SEASON_ID.to_le_bytes());
        use l::clash_inputs as ca;
        ci[ca::P..ca::P + 2].copy_from_slice(&(enemy_home.p as i16).to_le_bytes());
        ci[ca::Q..ca::Q + 2].copy_from_slice(&(enemy_home.q as i16).to_le_bytes());
        ci[ca::BELL..ca::BELL + 4].copy_from_slice(&IN_TRANSIT_ARRIVE.to_le_bytes());
        ci[ca::FLAGS] = 2;
        ci[ca::RESOLVER..ca::RESOLVER + 32].copy_from_slice(&RESOLVER);
        let env = json!({
            "v": 1,
            "key": format!("pv:{},{}", enemy_home.p, enemy_home.q),
            "bell": IN_TRANSIT_ARRIVE,
            "slot": SLOT - 800,
            "seq": "8",
            "head": hex::encode([8u8; 32]),
            "bytes": b64_encode(&province_bytes(pe)),
            "slots": [{"bytes": b64_encode(&sl)}],
            "day": null,
            "inputs": {"bytes": b64_encode(&ci)},
        });
        files.insert(
            format!(
                "h/province/{},{}/{}.json",
                enemy_home.p, enemy_home.q, IN_TRANSIT_ARRIVE
            ),
            pretty(&env),
        );
    }

    // ---- overview of rings 1 and 2
    for d in 1..=2u16 {
        let mut recs: Vec<OverviewRec> = provs
            .iter()
            .filter(|p| p.pc.ring() == d as u32)
            .map(|p| {
                let mut owners = [7u8; 12];
                let mut sites = [crate::obs::site_state::FREE; 12];
                for (s, st) in sites.iter_mut().enumerate() {
                    if s >= p.t.site_count as usize || d < 2 {
                        *st = crate::obs::site_state::RESERVED;
                    }
                }
                for &(s, f, _, _) in &p.holdings {
                    owners[s as usize] = f;
                    sites[s as usize] = crate::obs::site_state::HOLDING;
                }
                let mut hosts = [0u8; 7];
                for e in &p.entries {
                    hosts[e[le::FACTION] as usize % 7] += 1;
                }
                if p.camp.is_some() {
                    hosts[6] += 1;
                }
                OverviewRec {
                    p: p.pc.p as i16,
                    q: p.pc.q as i16,
                    owners,
                    sites,
                    hosts,
                    clash: false,
                    dormant: false,
                    opened: false,
                    resolved_next: BELL,
                }
            })
            .collect();
        recs.sort_by_key(|r| (r.p, r.q));
        let ov = Overview {
            season: SEASON_ID,
            ring: d,
            bell: BELL - 1,
            slot: SLOT - 5,
            provinces: recs,
        };
        files.insert(format!("h/overview/{d}/latest.bin"), ov.encode());
    }

    // ---- season
    let season = season_bytes(&drand);
    let sj = json!({
        "v": 1,
        "programId": program().to_string(),
        "cluster": "localnet",
        "season": SEASON_ID.to_string(),
        "seasonAddress": a.season.to_string(),
        "genesisTs": GENESIS_TS,
        "bellSecs": 600,
        "W": 600,
        "delta": 60,
        "drand": {
            "chainHash": hex::encode(drand.chain_hash),
            "publicKey": hex::encode(drand.public_key),
            "period": drand.period,
            "genesis": drand.genesis_time,
        },
        "rulesetHash": hex::encode([0x1a; 32]),
        "rMax": 16,
        "rings": (0..3).map(|d| json!({"d": d, "seed": hex::encode(RING_SEEDS[d])})).collect::<Vec<_>>(),
        "tipPriorityMilli": 433,
        "revealCuLimit": 26_000,
        "marchFee": "10000",
        "sealBond": "20000",
        "quotas": {"perDay": 40, "burst": 60},
        "headSeq": "812",
        "latestSlot": SLOT,
        "latestUnix": NOW,
        "bytes_b64": b64_encode(&season),
    });
    files.insert("h/season.json".into(), pretty(&sj));

    // ---- me files (UNJOINED has none: the herald answers 404)
    let me = |i: u32, citizen: Vec<u8>, holdings: Vec<(Address, Vec<u8>)>| {
        json!({
            "v": 1,
            "wallet": wallet(i).to_string(),
            "citizen": {"address": a.citizen(&wallet(i)).to_string(), "bytes_b64": b64_encode(&citizen)},
            "holdings": holdings.iter().map(|(k, b)| json!({"address": k.to_string(), "bytes_b64": b64_encode(b)})).collect::<Vec<_>>(),
            "slots": [],
            "quota": {"left": 38, "resetsAt": GENESIS_TS + 86_400},
        })
    };
    let flags_joined = lc::FLAG_JOINED;
    files.insert(
        format!("h/me/{}.json", wallet(JOINED)),
        pretty(&me(
            JOINED,
            citizen_bytes(
                &wallet(JOINED),
                &session(JOINED),
                0,
                flags_joined,
                None,
                u32::MAX,
                &funder,
            ),
            vec![],
        )),
    );
    let prov_cit = a.citizen(&wallet(PROVISIONAL));
    files.insert(
        format!("h/me/{}.json", wallet(PROVISIONAL)),
        pretty(&me(
            PROVISIONAL,
            citizen_bytes(
                &wallet(PROVISIONAL),
                &session(PROVISIONAL),
                0,
                flags_joined | lc::FLAG_PROVISIONAL,
                Some((home, 1)),
                u32::MAX,
                &funder,
            ),
            vec![(
                a.holding(home.p, home.q, 1),
                holding_bytes(
                    home,
                    1,
                    prov_tile,
                    lh::STATE_PROVISIONAL,
                    &prov_cit,
                    0,
                    0,
                    [0; 8],
                    &funder,
                ),
            )],
        )),
    );
    let fin_cit = a.citizen(&wallet(FINAL));
    let mut reserve = [0u32; 8];
    reserve[unit0 as usize] = 300;
    files.insert(
        format!("h/me/{}.json", wallet(FINAL)),
        pretty(&me(
            FINAL,
            citizen_bytes(
                &wallet(FINAL),
                &session(FINAL),
                0,
                flags_joined | lc::FLAG_FIRST_FINAL,
                Some((home, 0)),
                u32::MAX,
                &funder,
            ),
            vec![(
                a.holding(home.p, home.q, 0),
                with_transit(
                    holding_bytes(
                        home,
                        0,
                        final_tile,
                        lh::STATE_FINAL,
                        &fin_cit,
                        0,
                        IN_TRANSIT_SEQ,
                        reserve,
                        &funder,
                    ),
                    IN_TRANSIT_SLOT,
                    fclient::addr::host_id(home.p, home.q, 0, 1, IN_TRANSIT_SEQ).expect("host id"),
                    (home, final_tile),
                    unit0,
                    35,
                    IN_TRANSIT_ARRIVE,
                ),
            )],
        )),
    );
    let en_cit = a.citizen(&wallet(ENEMY));
    files.insert(
        format!("h/me/{}.json", wallet(ENEMY)),
        pretty(&me(
            ENEMY,
            citizen_bytes(
                &wallet(ENEMY),
                &session(ENEMY),
                1,
                flags_joined | lc::FLAG_FIRST_FINAL,
                Some((enemy_home, 1)),
                u32::MAX,
                &funder,
            ),
            vec![(
                a.holding(enemy_home.p, enemy_home.q, 1),
                holding_bytes(
                    enemy_home,
                    1,
                    enemy_tile,
                    lh::STATE_FINAL,
                    &en_cit,
                    1,
                    0,
                    [0; 8],
                    &funder,
                ),
            )],
        )),
    );

    // ---- bell 40's region files of every fixture province
    let clock = fclient::clock::Drand {
        genesis: drand.genesis_time,
        period: drand.period,
    };
    let mut regions: Vec<u8> = provs
        .iter()
        .map(|p| fclient::ix::region_of(p.pc.p, p.pc.q))
        .collect();
    regions.sort();
    regions.dedup();
    // Bell 40 (the current bell: anchored, no seed yet) and bell 38 (the
    // in-transit march's arrival bell: anchored and seeded).
    for (bell, seeded) in [(IN_TRANSIT_ARRIVE, true), (BELL, false)] {
        for &r in &regions {
            let round = clock.tlock_round(GENESIS_TS, bell);
            let a_ts = clock.round_time(round) + 1;
            let s = clock.seed_round(fclient::clock::reveal_close(a_ts, 600), 60);
            let caches = if seeded {
                json!([{"nonce": 0, "round": s, "seed": hex::encode([bell as u8; 32])}])
            } else {
                json!([])
            };
            let bj = json!({
                "v": 1,
                "bell": bell,
                "region": r,
                "anchor": {"key": format!("an:{bell},{r}"), "address": a.anchor(bell, r).to_string(), "bytes_b64": b64_encode(&anchor_bytes(&key, bell, r, round, a_ts))},
                "S": s,
                "caches": caches,
                "tombstoned": false,
                "archived": false,
                "resolved": [],
            });
            files.insert(format!("h/bell/{bell}/region/{r}.json"), pretty(&bj));
        }
    }
    World {
        files,
        home: (home.p as i16, home.q as i16),
        enemy_home: (enemy_home.p as i16, enemy_home.q as i16),
    }
}

/// Where the checked-in copy lives.
pub fn dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/herald")
}

/// Writes the world under `root` (the herald's paths).
pub fn write(root: &std::path::Path, w: &World) -> std::io::Result<()> {
    for (p, b) in &w.files {
        let f = root.join(p);
        std::fs::create_dir_all(f.parent().expect("parent"))?;
        std::fs::write(f, b)?;
    }
    Ok(())
}
