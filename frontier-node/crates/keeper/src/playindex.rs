//! Discovery of play objects from the program's transaction feed (W4-C;
//! M1 contract §6, §8.2). As the land index (W3-C), it only tells the
//! keeper *which* objects exist; every duty reads the accounts before it
//! plans a write. A restarted keeper re-reads the feed from cursor 0.
//!
//! | record | what it adds |
//! |---|---|
//! | `DEPART` | a transit: host, origin, bells, mass, tip, root, **the logged commitment and seal** (SettleTransit's proof input, I-44) |
//! | `REVEAL` | the host holds slot `(P, Q, arrive, faction, i)` (a displaced host loses it); its beneficiary |
//! | `DEPARTURE_SETTLED` | the transit's values are in its Holding |
//! | `TRANSIT_SETTLED` | the transit is settled (outcome, seal code) |
//! | `GATHER` / `CLASH` / `SKIP` | the province moved (re-read it) |
//! | `PROVINCE_OPEN` | the province exists |
//! | `DISSOLVE`, `MUSTER` | the province has pending changes |
//! | `DEFENCE_CLAIM`, `CLOSE` | claims and closes seen |

use std::collections::{BTreeMap, BTreeSet};

use solana_address::Address;

use fclient::addr::Addresses;
use fclient::ports::{ChainPort, Cursor, PortResult};
use frontier_abi::log::{self as plog, Kind};

const PAGES_PER_TICK: usize = 8;

/// A transit as its DEPART record announced it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepartRec {
    pub host: u64,
    pub origin: (i16, i16),
    pub origin_tile: u8,
    pub depart_bell: u32,
    pub arrive: u32,
    pub dep_mass: u32,
    pub tip: u64,
    pub seal_root: [u8; 32],
    pub commit: [u8; 32],
    pub seal: [u8; 165],
    pub slot: u64,
}

/// A REVEAL record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RevealRec {
    pub dest: (i16, i16),
    pub arrive: u32,
    pub faction: u8,
    pub i: u8,
    pub host: u64,
    pub displaced: Option<u64>,
    pub beneficiary: Address,
    pub slot: u64,
    pub created_day: bool,
}

#[derive(Default)]
pub struct PlayIndex {
    pub cursor: Cursor,
    /// `(host, depart_bell)` → the DEPART.
    pub departs: BTreeMap<(u64, u32), DepartRec>,
    /// host → the depart bell of its latest DEPART.
    pub latest: BTreeMap<u64, u32>,
    /// Hosts whose departure values are settled (keyed by `(host, depart_bell)`).
    pub departure_settled: BTreeSet<(u64, u32)>,
    /// `(host, depart_bell)` → `(outcome, seal code, slot)` of TRANSIT_SETTLED.
    pub settled: BTreeMap<(u64, u32), (u8, u8, u64)>,
    /// Slot `(P, Q, arrive, faction, i)` → the latest REVEAL into it.
    pub slots: BTreeMap<(i16, i16, u32, u8, u8), RevealRec>,
    /// Every REVEAL in feed order (tests, liveness).
    pub reveals: Vec<RevealRec>,
    /// ArrivalDays created `(P, Q, day)`.
    pub days: BTreeSet<(i16, i16, u32)>,
    /// ClashInputs gathered `(P, Q, bell)`.
    pub inputs: BTreeSet<(i16, i16, u32)>,
    /// Resolved `(P, Q, bell)` → outcome digest.
    pub clashes: BTreeMap<(i16, i16, u32), [u8; 32]>,
    /// Provinces with a record since they were last read.
    pub touched: BTreeSet<(i16, i16)>,
    pub provinces: BTreeSet<(i16, i16)>,
    /// `(kind, key bytes)` of CLOSE records.
    pub closed: BTreeSet<(u8, Vec<u8>)>,
    /// DEFENCE_CLAIM records `(beneficiary, day, amount)`.
    pub claims: Vec<(Address, u32, u64)>,
    pub bad: u64,
    /// Failed transactions whose program did not complete: `(signature,
    /// units consumed, "exceeded CUs meter" logged)`, for the engine's
    /// CU-meter test (W6T-2); the keeper drains it every tick.
    pub failed_meta: Vec<(fclient::ports::Signature, u64, bool)>,
}

fn fld<'a>(r: &plog::Record<'a>, name: &str) -> Option<&'a [u8]> {
    if let Some((o, w)) = plog::field(r.kind, name, false) {
        return r.key.get(o..o + w);
    }
    let (o, w) = plog::field(r.kind, name, true)?;
    r.payload.get(o..o + w)
}
fn le(b: &[u8]) -> u64 {
    let mut a = [0u8; 8];
    let n = b.len().min(8);
    a[..n].copy_from_slice(&b[..n]);
    u64::from_le_bytes(a)
}
fn i32le(b: &[u8]) -> i16 {
    i32::from_le_bytes(b.try_into().unwrap_or([0; 4])) as i16
}
fn arr<const N: usize>(b: &[u8]) -> [u8; N] {
    b.try_into().unwrap_or([0; N])
}

impl PlayIndex {
    pub async fn pull<P: ChainPort>(&mut self, port: &P, addrs: &Addresses) -> PortResult<usize> {
        let mut n = 0;
        for _ in 0..PAGES_PER_TICK {
            let page = port.feed(self.cursor).await?;
            let Some(last) = page.last() else { break };
            self.cursor = Cursor(last.seq);
            for tx in &page {
                if let Some(e) = &tx.err {
                    if e.contains("ProgramFailedToComplete") && self.failed_meta.len() < 10_000 {
                        let meter = tx.logs.iter().any(|l| l.contains("exceeded CUs meter"));
                        self.failed_meta.push((tx.signature, tx.units, meter));
                    }
                    continue;
                }
                let Ok(bodies) = fclient::log::bodies_from_logs(&tx.logs, &addrs.program) else {
                    self.bad += 1;
                    continue;
                };
                for b in bodies {
                    self.ingest(&b, tx.slot);
                    n += 1;
                }
            }
        }
        Ok(n)
    }

    /// The live transit of `host` (its latest DEPART).
    pub fn transit(&self, host: u64) -> Option<&DepartRec> {
        let b = self.latest.get(&host)?;
        self.departs.get(&(host, *b))
    }

    pub fn ingest(&mut self, body: &[u8], slot: u64) {
        let Ok(r) = plog::decode(body) else {
            self.bad += 1;
            return;
        };
        match r.kind {
            Kind::DEPART => {
                let (Some(h), Some(op), Some(oq)) =
                    (fld(&r, "host_id"), fld(&r, "origin_p"), fld(&r, "origin_q"))
                else {
                    return;
                };
                let rec = DepartRec {
                    host: le(h),
                    origin: (i32le(op), i32le(oq)),
                    origin_tile: fld(&r, "origin_tile").map_or(0, |b| b[0]),
                    depart_bell: fld(&r, "depart_bell").map_or(0, |b| le(b) as u32),
                    arrive: fld(&r, "arrive_bell").map_or(0, |b| le(b) as u32),
                    dep_mass: fld(&r, "dep_mass").map_or(0, |b| le(b) as u32),
                    tip: fld(&r, "tip").map_or(0, le),
                    seal_root: fld(&r, "seal_root").map_or([0; 32], arr),
                    commit: fld(&r, "commit").map_or([0; 32], arr),
                    seal: fld(&r, "seal").map_or([0; 165], arr),
                    slot,
                };
                self.touched.insert(rec.origin);
                self.latest.insert(rec.host, rec.depart_bell);
                self.departs.insert((rec.host, rec.depart_bell), rec);
            }
            Kind::REVEAL => {
                let (Some(p), Some(q), Some(a), Some(f), Some(i), Some(h)) = (
                    fld(&r, "p"),
                    fld(&r, "q"),
                    fld(&r, "arrive"),
                    fld(&r, "faction"),
                    fld(&r, "i"),
                    fld(&r, "host_id"),
                ) else {
                    return;
                };
                let disp = fld(&r, "displace").is_some_and(|b| b[0] != 0);
                let rec = RevealRec {
                    dest: (i32le(p), i32le(q)),
                    arrive: le(a) as u32,
                    faction: f[0],
                    i: i[0],
                    host: le(h),
                    displaced: disp.then(|| fld(&r, "displaced_host").map_or(0, le)),
                    beneficiary: Address::new_from_array(
                        fld(&r, "beneficiary").map_or([0; 32], arr),
                    ),
                    slot,
                    created_day: fld(&r, "arrivalday_created").is_some_and(|b| b[0] != 0),
                };
                self.days.insert((rec.dest.0, rec.dest.1, rec.arrive / 144));
                self.slots.insert(
                    (rec.dest.0, rec.dest.1, rec.arrive, rec.faction, rec.i),
                    rec,
                );
                self.reveals.push(rec);
            }
            Kind::DEPARTURE_SETTLED => {
                // `destroyed = 2`: a §21 return settle, not a transit's.
                if fld(&r, "destroyed").is_some_and(|b| b[0] == 2) {
                    return;
                }
                if let Some(h) = fld(&r, "host_id") {
                    let host = le(h);
                    if let Some(&b) = self.latest.get(&host) {
                        self.departure_settled.insert((host, b));
                    }
                }
            }
            Kind::TRANSIT_SETTLED => {
                if let Some(h) = fld(&r, "host_id") {
                    let host = le(h);
                    if let Some(&b) = self.latest.get(&host) {
                        let o = fld(&r, "outcome").map_or(0, |b| b[0]);
                        let c = fld(&r, "seal_code").map_or(0, |b| b[0]);
                        self.settled.insert((host, b), (o, c, slot));
                    }
                }
            }
            Kind::GATHER | Kind::CLASH => {
                let (Some(p), Some(q), Some(b)) = (fld(&r, "p"), fld(&r, "q"), fld(&r, "bell"))
                else {
                    return;
                };
                let k = (i32le(p), i32le(q), le(b) as u32);
                self.touched.insert((k.0, k.1));
                self.inputs.insert(k);
                if r.kind == Kind::CLASH {
                    self.clashes
                        .insert(k, fld(&r, "outcome_digest").map_or([0; 32], arr));
                }
            }
            Kind::SKIP | Kind::PROVINCE_OPEN => {
                let (Some(p), Some(q)) = (fld(&r, "p"), fld(&r, "q")) else {
                    return;
                };
                let k = (i32le(p), i32le(q));
                self.touched.insert(k);
                self.provinces.insert(k);
            }
            Kind::DISSOLVE | Kind::MUSTER => {
                if let Some(h) = fld(&r, "host_id") {
                    if let Ok((p, q, _, _, _)) = fclient::addr::host_parts(le(h)) {
                        self.touched.insert((p as i16, q as i16));
                    }
                }
            }
            Kind::DEFENCE_CLAIM => {
                let b = Address::new_from_array(fld(&r, "beneficiary").map_or([0; 32], arr));
                let day = fld(&r, "day").map_or(0, |x| le(x) as u32);
                let amt = fld(&r, "amount").map_or(0, le);
                self.claims.push((b, day, amt));
            }
            Kind::CLOSE => {
                let k = fld(&r, "account_kind").map_or(0, |b| b[0]);
                let key = fld(&r, "key").map(|b| b.to_vec()).unwrap_or_default();
                self.closed.insert((k, key));
            }
            _ => {}
        }
    }
}
