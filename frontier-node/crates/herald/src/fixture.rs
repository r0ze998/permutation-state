//! A synthetic mini-season archive (tests, the viewer load run): program
//! accounts built byte for byte from the `frontier-abi` layouts and PS2
//! records with real event chains, in the order the keeper produces them —
//! anchor, reveals, seed cache, gathers, resolves (recomputed with the
//! [`Provisional`] builder so the logged digest is the recomputed one),
//! quiet skips (runs of two bells), transit settlements with seal codes,
//! closes, anchor archives, and a failed transaction whose logged records
//! must not count. The program's own land and clash instructions are W3-A
//! and W4-A's (stubs on this branch), so these transactions are fixtures,
//! not program output; `fixture_tamper` changes one logged digest.
//!
//! Provinces: A = (2, 0) ring 2 region 3 (a holding of the fixture
//! wallet, a resident, arrivals on even bells); B = (1, 1) ring 2 region 3
//! (quiet, SkipQuiet in runs of two); C = (0, 2) ring 2 region 4 (arrivals
//! when `b % 3 == 1`); D = (3, 0) ring 3 region 4 (quiet, one bell per
//! skip, a camp).

use sha2::{Digest, Sha256};
use solana_address::Address;

use fclient::log::log_line;
use fclient::ports::{Account, Signature, TxRecord};
use frontier_abi::addr::{host_id, AddrCtx};
use frontier_abi::layout::{self as L, AccountKind};
use frontier_abi::log::{self as plog, EntityKind, Kind, Link};
use permutation_rules::frontier::beacon;
use permutation_rules::frontier::clash::{BeaconClock, Fate};

use crate::clash::{ClashBuilder, Provisional};

pub const SEASON_ID: u64 = 5;
pub const GENESIS_TS: i64 = 1_800_000_000;
pub const WINDOW: u32 = 600;
pub const MARGIN: u32 = 60;
pub const CLOCK: BeaconClock = BeaconClock {
    genesis: 1_692_803_367,
    period: 3,
};
/// The fixture's provinces: `(P, Q, ring, region)`.
pub const PROVINCES: [(i16, i16, u16, u8); 4] =
    [(2, 0, 2, 3), (1, 1, 2, 3), (0, 2, 2, 4), (3, 0, 3, 4)];
pub const REGIONS: [u8; 2] = [3, 4];

pub fn program() -> Address {
    Address::new_from_array([0x5E; 32])
}

/// The fixture wallet (holding at (2, 0) site 0).
pub fn wallet() -> Address {
    Address::new_from_array([0x11; 32])
}

pub fn ctx() -> AddrCtx {
    findex::addr_ctx(&program(), SEASON_ID)
}

fn put(d: &mut [u8], o: usize, b: &[u8]) {
    d[o..o + b.len()].copy_from_slice(b);
}

fn new_account(kind: AccountKind) -> Vec<u8> {
    let mut d = vec![0u8; kind.size()];
    L::write_header(&mut d, kind, SEASON_ID);
    d
}

/// The chain of a chained account's header advanced over `bwt`.
fn advance(d: &mut [u8], entity: EntityKind, bwt: &[u8]) -> Link {
    let seq = u64::from_le_bytes(d[24..32].try_into().unwrap_or_default());
    let head: [u8; 32] = d[32..64].try_into().unwrap_or_default();
    let l = plog::advance(entity, seq, &head, bwt).unwrap_or(Link {
        entity,
        seq: seq + 1,
        head,
    });
    put(d, 24, &l.seq.to_le_bytes());
    put(d, 32, &l.head);
    l
}

/// A PS2 body; `chained` = the accounts it advances, in entity order.
fn body(
    kind: Kind,
    bell: u32,
    key: &[u8],
    payload: &[u8],
    chained: &mut [(EntityKind, &mut Vec<u8>)],
) -> Vec<u8> {
    let mut out = vec![0u8; 1_024];
    let n = plog::write_body(kind, bell, key, payload, &mut out).unwrap_or(0);
    let bwt = out[..n].to_vec();
    chained.sort_by_key(|(e, _)| *e as u8);
    let links: Vec<Link> = chained
        .iter_mut()
        .map(|(e, d)| advance(d, *e, &bwt))
        .collect();
    let m = plog::write_tail(&links, &mut out, n).unwrap_or(n);
    out.truncate(m);
    out
}

fn pad(kind: Kind, payload: Vec<u8>) -> Vec<u8> {
    let mut p = payload;
    p.resize(kind.spec().payload_len(), 0);
    p
}

fn le32(v: i32) -> [u8; 4] {
    v.to_le_bytes()
}

struct B {
    txs: Vec<TxRecord>,
    seq: u64,
    slot: u64,
}

impl B {
    fn tx(
        &mut self,
        time: i64,
        bodies: Vec<Vec<u8>>,
        post: Vec<([u8; 32], Option<Vec<u8>>)>,
        err: bool,
    ) {
        self.seq += 1;
        self.slot += 1;
        let mut sig = [0u8; 64];
        sig[..8].copy_from_slice(&self.seq.to_le_bytes());
        sig[8] = 0xF1;
        self.txs.push(TxRecord {
            seq: self.seq,
            slot: self.slot,
            signature: Signature::from(sig),
            block_time: time,
            tx: vec![],
            logs: fclient::log::in_frame(&program(), bodies.iter().map(|b| log_line(b))),
            err: err.then(|| "InstructionError(0, Custom(52))".into()),
            code: err.then_some(52),
            units: 0,
            fee: 5_000,
            post: post
                .into_iter()
                .map(|(k, d)| {
                    (
                        Address::new_from_array(k),
                        d.map(|data| Account {
                            lamports: 1_000_000,
                            data,
                            owner: program(),
                            executable: false,
                        }),
                    )
                })
                .collect(),
        });
    }
}

fn season_account() -> Vec<u8> {
    use L::world::season as S;
    let mut d = new_account(AccountKind::Season);
    d[S::STATUS] = 2;
    d[S::REGIONS] = 16;
    d[S::GENESIS_RING] = 3;
    put(&mut d, S::R_MAX, &16u16.to_le_bytes());
    put(
        &mut d,
        S::RULESET_HASH,
        &frontier_abi::presets::RULESET_HASH,
    );
    put(&mut d, S::BELL_SECS, &600u32.to_le_bytes());
    put(&mut d, S::GENESIS_TS, &GENESIS_TS.to_le_bytes());
    put(&mut d, S::END_BELL, &1_008u32.to_le_bytes());
    put(&mut d, S::DRAND_GENESIS, &CLOCK.genesis.to_le_bytes());
    put(
        &mut d,
        S::DRAND_PERIOD,
        &(CLOCK.period as u32).to_le_bytes(),
    );
    d[S::NETWORK] = 2;
    put(&mut d, S::REVEAL_WINDOW, &WINDOW.to_le_bytes());
    put(&mut d, S::SEED_MARGIN, &MARGIN.to_le_bytes());
    put(&mut d, S::WINDOW_NEXT, &WINDOW.to_le_bytes());
    put(&mut d, S::WINDOW_FROM_BELL, &u32::MAX.to_le_bytes());
    put(&mut d, S::MARCH_FEE, &10_000u64.to_le_bytes());
    put(&mut d, S::SEAL_BOND, &20_000u64.to_le_bytes());
    put(&mut d, S::MIN_REVEAL_PRIORITY_MILLI, &433u32.to_le_bytes());
    put(&mut d, S::REVEAL_CU_LIMIT, &26_000u32.to_le_bytes());
    put(&mut d, S::REVEAL_LOADED_LIMIT, &(1u32 << 20).to_le_bytes());
    d
}

fn province_account(p: i16, q: i16, ring: u16, region: u8) -> Vec<u8> {
    use L::province::{entry as E, province as P, site as S};
    let mut d = new_account(AccountKind::Province);
    put(&mut d, P::P, &p.to_le_bytes());
    put(&mut d, P::Q, &q.to_le_bytes());
    put(&mut d, P::RING, &ring.to_le_bytes());
    d[P::WEDGE] = 0;
    d[P::REGION] = region;
    put(&mut d, P::RESOLVED_NEXT, &0u32.to_le_bytes());
    put(&mut d, P::OPENED_BELL, &u32::MAX.to_le_bytes());
    // Terrain: all grassland (passable), no resources; three sites.
    let sites = [5u8, 17, 29];
    put(&mut d, P::SITES, &sites);
    d[P::SITE_COUNT] = 3;
    for i in 0..12 {
        let o = P::SITE_MIRROR + i * 64;
        put(&mut d, o + S::PEND0_BELL, &S::NO_BELL.to_le_bytes());
        put(&mut d, o + S::PEND1_BELL, &S::NO_BELL.to_le_bytes());
        put(&mut d, o + S::WALL_ITEM0_BELL, &S::NO_BELL.to_le_bytes());
        put(&mut d, o + S::WALL_ITEM1_BELL, &S::NO_BELL.to_le_bytes());
        d[o + S::FACTION] = 6;
    }
    if (p, q) == (2, 0) {
        // Site 0: the fixture wallet's holding (faction 1), garrison 200.
        let o = P::SITE_MIRROR;
        d[o + S::STATE] = S::STATE_HOLDING;
        d[o + S::FACTION] = 1;
        put(&mut d, o + S::GARRISON, &200_000u32.to_le_bytes());
        put(&mut d, o + S::WALLS_COMMITTED, &100u32.to_le_bytes());
        // Site 2 released.
        d[P::SITE_MIRROR + 2 * 64 + S::STATE] = S::STATE_RELEASED_FREE;
        // One resident of that holding.
        let e = P::ENTRIES;
        let id = host_id(2, 0, 0, 0, 1).unwrap_or(1);
        put(&mut d, e + E::ID, &id.to_le_bytes());
        d[e + E::FACTION] = 1;
        d[e + E::UNIT] = 0;
        d[e + E::TILE] = 6;
        d[e + E::STATE] = E::STATE_ROSTER;
        put(&mut d, e + E::TROOPS, &800_000u32.to_le_bytes());
        put(&mut d, e + E::STAMINA_VALUE, &100u16.to_le_bytes());
        put(&mut d, e + E::DEALT_BPS, &10_000u16.to_le_bytes());
        d[P::N_ENTRIES] = 1;
    }
    if ring >= 2 && (p, q) != (2, 0) {
        // Rings 0–1 are reserved; the others get the initial camp.
        let c = P::CAMP;
        d[c] = 40;
        d[c + 1] = 1;
        put(&mut d, c + 4, &150_000u32.to_le_bytes());
    }
    d
}

fn citizen_account(addr: &[u8; 32]) -> Vec<u8> {
    use L::player::citizen as C;
    let mut d = new_account(AccountKind::Citizen);
    put(&mut d, C::WALLET, &wallet().to_bytes());
    d[C::FACTION] = 1;
    d[C::FLAGS] = 1 | 4;
    d[C::HOLDINGS_N] = 1;
    put(&mut d, C::HOLDING, &2i16.to_le_bytes());
    put(&mut d, C::HOLDING + 2, &0i16.to_le_bytes());
    d[C::HOLDING + 4] = 0;
    d[C::HOLDING + 5] = 0;
    put(&mut d, C::TICKET_BELL, &u32::MAX.to_le_bytes());
    put(&mut d, C::CITIZEN_TAG, &addr[..8]);
    d
}

fn holding_account(citizen: &[u8; 32]) -> Vec<u8> {
    use L::player::{holding as H, transit as T};
    let mut d = new_account(AccountKind::Holding);
    put(&mut d, H::P, &2i16.to_le_bytes());
    d[H::TILE] = 5;
    d[H::STATE] = 2;
    put(&mut d, H::OWNER_CITIZEN, citizen);
    d[H::FACTION] = 1;
    d[H::FLAGS] = 1; // dormant-flag cache: the overview's flag 2
    let t = H::TRANSIT;
    d[t + T::STATE] = 1;
    d[t + T::FACTION] = 1;
    put(
        &mut d,
        t + T::HOST_ID,
        &host_id(2, 0, 0, 0, 2).unwrap_or(2).to_le_bytes(),
    );
    put(&mut d, t + T::ARRIVE_BELL, &3u32.to_le_bytes());
    d
}

/// The fixture archive: `bells` bells of the four provinces.
pub fn mini_season(bells: u32) -> Vec<TxRecord> {
    let cx = ctx();
    let mut b = B {
        txs: vec![],
        seq: 0,
        slot: 1_000,
    };
    let bell_end = |x: u32| beacon::bell_end(GENESIS_TS, x);
    // Season created, rings opened, provinces opened, the citizen joined.
    let mut season = season_account();
    let sid = SEASON_ID.to_le_bytes();
    let mut sc_payload = vec![0u8; 32];
    sc_payload.extend(1u64.to_le_bytes());
    sc_payload.extend(GENESIS_TS.to_le_bytes());
    sc_payload.extend(frontier_abi::presets::RULESET_HASH);
    let sc = body(
        Kind::SEASON_CREATED,
        plog::NO_BELL,
        &sid,
        &pad(Kind::SEASON_CREATED, sc_payload),
        &mut [(EntityKind::Season, &mut season)],
    );
    b.tx(
        GENESIS_TS - 1_000,
        vec![sc],
        vec![(cx.season, Some(season.clone()))],
        false,
    );
    for d in [2u16, 3] {
        let mut rs = new_account(AccountKind::RingSeed);
        put(&mut rs, 16, &d.to_le_bytes());
        rs[18] = 2;
        let seed: [u8; 32] = Sha256::digest([b"ring".as_slice(), &d.to_le_bytes()].concat()).into();
        put(&mut rs, 40, &seed);
        let mut payload = vec![0u8; 16];
        payload.extend(seed);
        let r = body(
            Kind::RING_OPEN,
            plog::NO_BELL,
            &d.to_le_bytes(),
            &payload,
            &mut [],
        );
        b.tx(
            GENESIS_TS - 900,
            vec![r],
            vec![(cx.ring_seed(d), Some(rs))],
            false,
        );
    }
    let mut provs: Vec<Vec<u8>> = vec![];
    for (p, q, ring, region) in PROVINCES {
        let mut d = province_account(p, q, ring, region);
        let key = [le32(p as i32), le32(q as i32)].concat();
        let mut payload = ring.to_le_bytes().to_vec();
        payload.extend([0, region]);
        let r = body(
            Kind::PROVINCE_OPEN,
            plog::NO_BELL,
            &key,
            &pad(Kind::PROVINCE_OPEN, payload),
            &mut [(EntityKind::Province, &mut d)],
        );
        b.tx(
            GENESIS_TS - 800,
            vec![r],
            vec![(cx.province(p as i32, q as i32), Some(d.clone()))],
            false,
        );
        provs.push(d);
    }
    let caddr = cx.citizen(&wallet().to_bytes());
    let mut citizen = citizen_account(&caddr);
    let mut holding = holding_account(&caddr);
    let mut jp = wallet().to_bytes().to_vec();
    jp.extend([1u8, 0]);
    let join = body(
        Kind::JOIN,
        plog::NO_BELL,
        &caddr[..15],
        &pad(Kind::JOIN, jp),
        &mut [(EntityKind::Citizen, &mut citizen)],
    );
    let settle_key = [le32(2).as_slice(), &le32(0), &[0u8]].concat();
    let settle = body(
        Kind::SETTLE,
        plog::NO_BELL,
        &settle_key,
        &pad(Kind::SETTLE, vec![0]),
        &mut [(EntityKind::Holding, &mut holding)],
    );
    b.tx(
        GENESIS_TS - 700,
        vec![join, settle],
        vec![
            (caddr, Some(citizen.clone())),
            (cx.holding(2, 0, 0), Some(holding.clone())),
        ],
        false,
    );
    let mut closes_due: Vec<(u32, [u8; 32], Vec<u8>)> = vec![];
    let mut archives: std::collections::BTreeMap<(u8, u32), Vec<u8>> = Default::default();
    for bell in 0..bells {
        let t0 = bell_end(bell);
        let t_round = beacon::tlock_round(&CLOCK, GENESIS_TS, bell);
        let a = t0 + 2;
        let s_round = beacon::seed_round(&CLOCK, beacon::reveal_close(a, WINDOW), MARGIN);
        // Reveals during the bell (arrivals into A on even bells, C when b % 3 == 1).
        let arriving: Vec<usize> = [(0usize, bell % 2 == 0), (2, bell % 3 == 1)]
            .iter()
            .filter(|x| x.1)
            .map(|x| x.0)
            .collect();
        let mut slots: Vec<(usize, [u8; 32], Vec<u8>, u64)> = vec![];
        for &k in &arriving {
            let (p, q, _, _) = PROVINCES[k];
            let hid = host_id(-2, 1, 0, 0, bell + 10).unwrap_or(bell as u64);
            let (f, i) = (2u8, 0u8);
            let mut s = new_account(AccountKind::ArrivalSlot);
            {
                use L::clash::arrival_slot as A;
                put(&mut s, A::P, &p.to_le_bytes());
                put(&mut s, A::Q, &q.to_le_bytes());
                put(&mut s, A::BELL, &bell.to_le_bytes());
                s[A::FACTION] = f;
                s[A::I] = i;
                s[A::STANCE] = (bell % 4) as u8;
                s[A::TILE] = 7;
                put(&mut s, A::HOST_ID, &hid.to_le_bytes());
                put(&mut s, A::CITIZEN_TAG, &(900 + k as u64).to_le_bytes());
                put(&mut s, A::DEP_MASS, &600_000u32.to_le_bytes());
                put(&mut s, A::DEALT_BPS, &10_000u16.to_le_bytes());
            }
            let day = bell / 144;
            let mut dd = new_account(AccountKind::ArrivalDay);
            {
                use L::clash::arrival_day as D;
                put(&mut dd, D::P, &p.to_le_bytes());
                put(&mut dd, D::Q, &q.to_le_bytes());
                put(&mut dd, D::DAY, &day.to_le_bytes());
                let prev = b
                    .txs
                    .iter()
                    .rev()
                    .flat_map(|t| t.post.iter())
                    .find(|(kk, _)| kk.to_bytes() == cx.arrival_day(p as i32, q as i32, day))
                    .and_then(|(_, a)| a.as_ref().map(|a| a.data.clone()));
                if let Some(prev) = prev {
                    dd = prev;
                }
                let (o, m) = D::bit(bell);
                dd[o] |= m;
            }
            let key = [
                le32(p as i32).as_slice(),
                &le32(q as i32),
                &bell.to_le_bytes(),
                &[f, i],
            ]
            .concat();
            let mut payload = hid.to_le_bytes().to_vec();
            payload.extend([7u8, (bell % 4) as u8]);
            let r = body(
                Kind::REVEAL,
                bell,
                &key,
                &pad(Kind::REVEAL, payload),
                &mut [],
            );
            let sa = cx.arrival_slot(p as i32, q as i32, bell, f, i);
            b.tx(
                t0 - 300,
                vec![r],
                vec![
                    (sa, Some(s.clone())),
                    (cx.arrival_day(p as i32, q as i32, day), Some(dd)),
                ],
                false,
            );
            slots.push((k, sa, s, hid));
        }
        // THE anchors, then the seed caches.
        for r in REGIONS {
            let mut an = new_account(AccountKind::BellAnchor);
            {
                use L::beacon::bell_anchor as A;
                put(&mut an, A::BELL, &bell.to_le_bytes());
                an[A::REGION] = r;
                an[A::NET] = 2;
                put(&mut an, A::ROUND, &t_round.to_le_bytes());
                put(&mut an, A::A, &a.to_le_bytes());
                put(&mut an, A::SLOT, &(b.slot + 1).to_le_bytes());
            }
            let key = [bell.to_le_bytes().as_slice(), &[r]].concat();
            let mut payload = t_round.to_le_bytes().to_vec();
            payload.extend(a.to_le_bytes());
            payload.extend((b.slot + 1).to_le_bytes());
            let rec = body(
                Kind::ANCHOR,
                bell,
                &key,
                &pad(Kind::ANCHOR, payload),
                &mut [],
            );
            b.tx(
                a,
                vec![rec],
                vec![(cx.bell_anchor(bell, r), Some(an))],
                false,
            );
        }
        let mut seeds = [[0u8; 32]; 2];
        for (ri, r) in REGIONS.iter().enumerate() {
            let seed: [u8; 32] =
                Sha256::digest([b"seed".as_slice(), &bell.to_le_bytes(), &[*r]].concat()).into();
            seeds[ri] = seed;
            let mut sd = new_account(AccountKind::SeedCache);
            {
                use L::beacon::seed_cache as S;
                put(&mut sd, S::BELL, &bell.to_le_bytes());
                sd[S::REGION] = *r;
                put(&mut sd, S::ROUND, &s_round.to_le_bytes());
                put(&mut sd, S::SEED, &seed);
                put(&mut sd, S::ANCHOR_KEY, &cx.bell_anchor(bell, *r));
                put(&mut sd, S::A, &a.to_le_bytes());
            }
            let key = [bell.to_le_bytes().as_slice(), &[*r, 0]].concat();
            let mut payload = s_round.to_le_bytes().to_vec();
            payload.extend(seed);
            payload.extend(a.to_le_bytes());
            let rec = body(Kind::SEED, bell, &key, &payload, &mut []);
            b.tx(
                a + WINDOW as i64 + MARGIN as i64 + 1,
                vec![rec],
                vec![(cx.seed_cache(bell, *r, 0), Some(sd))],
                false,
            );
        }
        let t_res = a + WINDOW as i64 + MARGIN as i64 + 5;
        // A failed resolve attempt: its records are not events.
        let bogus = body(
            Kind::CLASH,
            bell,
            &[le32(9).as_slice(), &le32(9), &bell.to_le_bytes()].concat(),
            &pad(Kind::CLASH, vec![]),
            &mut [],
        );
        b.tx(t_res, vec![bogus], vec![], true);
        // Gathers and resolves of the arrival provinces.
        for (k, sa, s, hid) in &slots {
            let (p, q, _, region) = PROVINCES[*k];
            let seed = seeds[REGIONS.iter().position(|r| *r == region).unwrap_or(0)];
            let mut ci = new_account(AccountKind::ClashInputs);
            {
                use L::clash::{arrival as R, clash_inputs as C};
                put(&mut ci, C::P, &p.to_le_bytes());
                put(&mut ci, C::Q, &q.to_le_bytes());
                put(&mut ci, C::BELL, &bell.to_le_bytes());
                put(&mut ci, C::ARRIVALS_MASK, &0x00FF_FFFFu32.to_le_bytes());
                ci[C::N_PRESENT] = 1;
                let o = C::ARRIVALS + C::position(2, 0) * 40;
                put(&mut ci, o + R::HOST_ID, &hid.to_le_bytes());
                put(&mut ci, o + R::CITIZEN_TAG, &s[40..48]);
                put(&mut ci, o + R::DEP_MASS, &600_000u32.to_le_bytes());
                put(&mut ci, o + R::TROOPS, &600_000u32.to_le_bytes());
                put(&mut ci, o + R::STAMINA, &80u16.to_le_bytes());
                put(&mut ci, o + R::DEALT, &10_000u16.to_le_bytes());
                ci[o + R::FACTION] = 2;
                ci[o + R::UNIT] = 1;
                ci[o + R::TILE] = s[L::clash::arrival_slot::TILE];
                ci[o + R::STANCE] = s[L::clash::arrival_slot::STANCE];
                ci[o + R::PRESENT] = 1;
            }
            let gkey = [
                le32(p as i32).as_slice(),
                &le32(q as i32),
                &bell.to_le_bytes(),
            ]
            .concat();
            let mut gp = vec![0u8, 24];
            gp.extend(0x00FF_FFFFu32.to_le_bytes());
            gp.push(0);
            let g = body(
                Kind::GATHER,
                bell,
                &gkey,
                &gp,
                &mut [(EntityKind::ClashInputs, &mut ci)],
            );
            let ca = cx.clash_inputs(p as i32, q as i32, bell);
            b.tx(t_res, vec![g], vec![(ca, Some(ci.clone()))], false);
            // Resolve: the provisional builder's outcome is the logged one.
            let pv = &mut provs[*k];
            let outcome = Provisional.recompute(pv, &ci, bell, &seed);
            let (digest, eng, fate) = match &outcome {
                Ok(o) => (
                    o.digest(),
                    o.engagements,
                    o.fighter(*hid).map(|f| match f.fate {
                        Fate::Stays { .. } => 1u8,
                        Fate::Withdrew { .. } => 2,
                        Fate::Bounced => 3,
                        Fate::Retreated => 4,
                        Fate::Destroyed => 5,
                    }),
                ),
                Err(e) => panic!("fixture clash {p},{q}@{bell}: {e}"),
            };
            let mut fates = [0u8; 24];
            fates[L::clash::clash_inputs::position(2, 0)] = fate.unwrap_or(0);
            {
                use L::clash::{arrival as R, clash_inputs as C};
                ci[C::FLAGS] |= 2;
                ci[C::ARRIVALS + C::position(2, 0) * 40 + R::FATE] = fate.unwrap_or(0);
                use L::province::province as P;
                put(pv, P::RESOLVED_NEXT, &(bell + 1).to_le_bytes());
                put(pv, P::LAST_DIGEST, &digest);
                let e = u32::from_le_bytes(
                    pv[P::ROSTER_EPOCH..P::ROSTER_EPOCH + 4]
                        .try_into()
                        .unwrap_or_default(),
                );
                put(pv, P::ROSTER_EPOCH, &(e + 1).to_le_bytes());
            }
            let mut cp = digest.to_vec();
            cp.extend([0x1Du8; 32]);
            cp.extend(eng.to_le_bytes());
            cp.extend(plog::pack_fates(&fates));
            let c = body(
                Kind::CLASH,
                bell,
                &gkey,
                &cp,
                &mut [
                    (EntityKind::Province, pv),
                    (EntityKind::ClashInputs, &mut ci),
                ],
            );
            b.tx(
                t_res + 1,
                vec![c],
                vec![
                    (cx.province(p as i32, q as i32), Some(pv.clone())),
                    (ca, Some(ci.clone())),
                ],
                false,
            );
            closes_due.push((bell + 2, ca, ci.clone()));
            // Settlement: the slot closes; every fifth one is a bad seal.
            let code = if bell % 5 == 4 { 5u8 } else { 0 };
            let outcome = if code > 0 { 8u8 } else { fate.unwrap_or(0) };
            let mut sp = vec![outcome, code];
            sp.extend(600_000u32.to_le_bytes());
            let st = body(
                Kind::TRANSIT_SETTLED,
                bell + 1,
                &hid.to_le_bytes(),
                &pad(Kind::TRANSIT_SETTLED, sp),
                &mut [],
            );
            b.tx(t_res + 700, vec![st], vec![(*sa, None)], false);
        }
        // Quiet provinces: B skips runs of two bells, D one bell (C and A
        // skip the bells without arrivals).
        for k in 0..PROVINCES.len() {
            let (p, q, _, _) = PROVINCES[k];
            let pv = &mut provs[k];
            let rn = u32::from_le_bytes(pv[72..76].try_into().unwrap_or_default());
            if rn > bell {
                continue;
            }
            let n = if k == 1 {
                if bell % 2 == 0 {
                    continue;
                }
                bell + 1 - rn
            } else {
                1
            };
            put(
                pv,
                L::province::province::RESOLVED_NEXT,
                &(rn + n).to_le_bytes(),
            );
            let mut sp = rn.to_le_bytes().to_vec();
            sp.push(n as u8);
            sp.extend([0x51u8; 32]);
            let r = body(
                Kind::SKIP,
                bell,
                &[le32(p as i32), le32(q as i32)].concat(),
                &sp,
                &mut [(EntityKind::Province, pv)],
            );
            b.tx(
                t_res + 2,
                vec![r],
                vec![(cx.province(p as i32, q as i32), Some(pv.clone()))],
                false,
            );
        }
        // Closes of inputs two bells old.
        let due: Vec<_> = closes_due.iter().filter(|c| c.0 == bell).cloned().collect();
        closes_due.retain(|c| c.0 != bell);
        for (_, ca, mut ci) in due {
            let mut cp = vec![AccountKind::ClashInputs as u8];
            cp.extend(&ci[64..76]);
            cp.resize(16, 0);
            let (seq, head) = (ci[24..32].to_vec(), ci[32..64].to_vec());
            let mut pl = seq;
            pl.extend(head);
            let r = body(
                Kind::CLOSE,
                bell,
                &cp,
                &pad(Kind::CLOSE, pl),
                &mut [(EntityKind::ClashInputs, &mut ci)],
            );
            b.tx(t_res + 3, vec![r], vec![(ca, None)], false);
        }
        // Archive the anchors of bell − 3 (tombstone, then close).
        if bell >= 3 {
            let old = bell - 3;
            for r in REGIONS {
                let part = old / 72;
                let aa = archives.entry((r, part)).or_insert_with(|| {
                    let mut d = new_account(AccountKind::AnchorArchive);
                    d[16] = r;
                    put(&mut d, 20, &part.to_le_bytes());
                    d
                });
                let k = (old % 72) as usize;
                aa[24 + k / 8] |= 1 << (k % 8);
                aa[33 + k / 8] |= 1 << (k % 8);
                let e = 64 + k * 84;
                put(aa, e, &2u32.to_le_bytes());
                let seed: [u8; 32] =
                    Sha256::digest([b"seed".as_slice(), &old.to_le_bytes(), &[r]].concat()).into();
                put(aa, e + 4, &seed);
                let key = [[r].as_slice(), &(old / 144).to_le_bytes()].concat();
                let mut payload = old.to_le_bytes().to_vec();
                payload.extend(2u32.to_le_bytes());
                payload.extend(seed);
                let rec = body(Kind::ARCHIVE, bell, &key, &payload, &mut []);
                let aa = aa.clone();
                b.tx(
                    t_res + 4,
                    vec![rec],
                    vec![
                        (cx.anchor_archive(r, part), Some(aa)),
                        (cx.bell_anchor(old, r), None),
                    ],
                    false,
                );
            }
        }
    }
    b.txs
}

/// The archive with one CLASH record's outcome digest flipped (the
/// herald must publish `MISMATCH` for it).
pub fn fixture_tamper(txs: &mut [TxRecord]) -> Option<(i32, i32, u32)> {
    for t in txs.iter_mut().filter(|t| t.err.is_none()) {
        for l in t.logs.iter_mut() {
            let Ok(Some(bd)) = fclient::log::body_of_line(l) else {
                continue;
            };
            let bd = &bd;
            let Ok(r) = plog::decode(bd) else { continue };
            if r.kind == Kind::CLASH {
                let key = (
                    i32::from_le_bytes(r.key[0..4].try_into().ok()?),
                    i32::from_le_bytes(r.key[4..8].try_into().ok()?),
                    u32::from_le_bytes(r.key[8..12].try_into().ok()?),
                );
                let mut nb = bd.clone();
                nb[6 + 12] ^= 0xFF;
                *l = log_line(&nb);
                return Some(key);
            }
        }
    }
    None
}
