//! G14's native re-run (M1 contract §13.3): **every province-bell the
//! program resolved or skipped is re-run with the native kernel** from the
//! chain's own bytes, independently of the herald and the verifier.
//!
//! Walking the program's feed in order (landed transactions, the
//! post-states of the accounts each wrote):
//!
//! - **CLASH** (ResolveFromInputs): the kernel input is rebuilt with the
//!   shared builder (`fclient::clash_model::build`) from the Province as the
//!   last transaction before the resolve left it, the ClashInputs the
//!   resolve wrote and THE seed (`S(b, r)` of THE anchor: the ANCHOR
//!   record's `A`, a SEED record naming that `A`); `clash::resolve_clash`
//!   must give the logged outcome digest, engagements and packed fates, and
//!   the logged input digest must be the §22 formula over those bytes;
//! - **SKIP** (SkipQuiet): the committed bells are replayed natively bell
//!   by bell (`fclient::clash_model::skip_replay`: the day's camp check, the
//!   settle, `resolved_next`), with `clash::is_quiet` asked at **every**
//!   bell (the program asks the kernel at most once per transaction); the
//!   replay must leave the Province the SKIP wrote (every byte after the
//!   chained header), spawn the camps its CAMP records name, and the quiet
//!   digest must be the §22 formula;
//! - **coverage**: each Province's CLASH and SKIP records advance
//!   `resolved_next` one bell at a time from its opening, with no gap and
//!   no repeat.

use std::collections::{BTreeMap, HashMap};

use fclient::clash_model;
use fclient::ports::TxRecord;
use fclient::Address;
use frontier_abi::layout::clash::clash_inputs as CL;
use frontier_abi::layout::province::province as PL;
use frontier_abi::log::{pack_fates, transit_outcome, Kind};
use permutation_rules::frontier::clash::{ClashOutcome, Fate};
use permutation_rules::frontier::geometry::{region_of, ProvinceCoord};
use serde_json::{json, Value};

use crate::checks::{field, le_u64};

/// A camp spawn: `(tile, troops, day)`.
type Spawn = (u8, u32, u32);

/// What the native re-run found.
#[derive(Clone, Debug, Default)]
pub struct NativeRerun {
    pub clashes: u64,
    pub clashes_matched: u64,
    pub skips: u64,
    /// Bells covered by SKIP records.
    pub skipped_bells: u64,
    pub skips_matched: u64,
    /// Skipped bells whose settle or camp check changed the Province (the
    /// replay did more than advance `resolved_next`).
    pub skipped_bells_changed: u64,
    /// Camps the replayed skips spawned (each matched a CAMP record).
    pub skip_camp_spawns: u64,
    /// Bells the program skipped that the kernel does not find quiet.
    pub loud_bells: Vec<String>,
    pub mismatches: Vec<String>,
    /// Province-bells resolved out of order, twice or not at all.
    pub coverage: Vec<String>,
    /// `resolved_next` per Province after the last record.
    pub resolved_next: BTreeMap<(i32, i32), u32>,
}

impl NativeRerun {
    pub fn ok(&self) -> bool {
        self.mismatches.is_empty() && self.loud_bells.is_empty() && self.coverage.is_empty()
    }

    /// Province-bells re-run natively (resolved + skipped).
    pub fn bells(&self) -> u64 {
        self.clashes + self.skipped_bells
    }

    pub fn to_json(&self) -> Value {
        json!({
            "clashes": self.clashes,
            "clashesMatched": self.clashes_matched,
            "skips": self.skips,
            "skippedBells": self.skipped_bells,
            "skipsMatched": self.skips_matched,
            "skippedBellsChanged": self.skipped_bells_changed,
            "skipCampSpawns": self.skip_camp_spawns,
            "loudBells": self.loud_bells,
            "mismatches": self.mismatches,
            "coverage": self.coverage,
            "provinces": self.resolved_next.len(),
        })
    }
}

fn u32_of(b: &[u8]) -> u32 {
    le_u64(b) as u32
}

fn i32_of(b: &[u8]) -> i32 {
    u32_of(b) as i32
}

/// The program account a transaction wrote whose bytes start with `magic`
/// and satisfy `is`.
fn post_of<'a>(
    t: &'a TxRecord,
    magic: &[u8; 8],
    is: impl Fn(&[u8]) -> bool,
) -> Option<(Address, &'a [u8])> {
    t.post.iter().find_map(|(k, a)| {
        let a = a.as_ref()?;
        (a.data.len() >= 8 && a.data[..8] == *magic && is(&a.data)).then_some((*k, &a.data[..]))
    })
}

fn province_pq(d: &[u8]) -> (i32, i32) {
    let g = |o: usize| i16::from_le_bytes([d[o], d[o + 1]]) as i32;
    (g(PL::P), g(PL::Q))
}

/// The ClashInputs fate code of a kernel fate (1–5).
fn fate_code(f: &Fate) -> u8 {
    match f {
        Fate::Stays { .. } => transit_outcome::STAYS,
        Fate::Withdrew { .. } => transit_outcome::WITHDREW,
        Fate::Bounced => transit_outcome::BOUNCED,
        Fate::Retreated => transit_outcome::RETREATED,
        Fate::Destroyed => transit_outcome::DESTROYED,
    }
}

fn packed_fates(b: &clash_model::BuiltInput, o: &ClashOutcome) -> [u8; 9] {
    let mut fates = [0u8; 24];
    for (a, &pos) in b.arrivals.iter().zip(&b.arrival_pos) {
        if let Some(fr) = o.fighters.iter().find(|f| f.id == a.id) {
            fates[pos] = fate_code(&fr.fate);
        }
    }
    pack_fates(&fates)
}

/// Re-runs every resolved and skipped province-bell of `txs` (the
/// program's feed, in order) with the native kernel.
pub fn rerun(txs: &[TxRecord], program: &Address) -> NativeRerun {
    let mut out = NativeRerun::default();
    // The last known bytes of every program account.
    let mut last: HashMap<Address, Vec<u8>> = HashMap::new();
    // THE anchor's A per (bell, region) and the seeds per (bell, region, A).
    let mut anchor_a: HashMap<(u32, u8), u64> = HashMap::new();
    let mut seeds: HashMap<(u32, u8, u64), [u8; 32]> = HashMap::new();
    let mut next: BTreeMap<(i32, i32), u32> = BTreeMap::new();
    for t in txs.iter().filter(|t| t.err.is_none()) {
        let recs = findex::records(t, program).unwrap_or_default();
        // CAMP records of this transaction, per province.
        let mut camps: BTreeMap<(i32, i32), Vec<Spawn>> = BTreeMap::new();
        for r in &recs {
            if Kind::from_u8(r.kind) == Some(Kind::CAMP) {
                let k = Kind::CAMP;
                camps
                    .entry((
                        i32_of(field(r, k, "p", false)),
                        i32_of(field(r, k, "q", false)),
                    ))
                    .or_default()
                    .push((
                        field(r, k, "tile", true).first().copied().unwrap_or(0),
                        u32_of(field(r, k, "troops", true)),
                        u32_of(field(r, k, "day", true)),
                    ));
            }
        }
        for r in &recs {
            let Some(k) = Kind::from_u8(r.kind) else {
                continue;
            };
            match k {
                Kind::ANCHOR => {
                    let bell = u32_of(field(r, k, "bell", false));
                    let region = field(r, k, "region", false).first().copied().unwrap_or(0);
                    anchor_a
                        .entry((bell, region))
                        .or_insert(le_u64(field(r, k, "a", true)));
                }
                Kind::SEED => {
                    let bell = u32_of(field(r, k, "bell", false));
                    let region = field(r, k, "region", false).first().copied().unwrap_or(0);
                    let a = le_u64(field(r, k, "a", true));
                    let seed: [u8; 32] = field(r, k, "seed", true).try_into().unwrap_or([0; 32]);
                    seeds.insert((bell, region, a), seed);
                }
                Kind::PROVINCE_OPEN => {
                    let pq = (
                        i32_of(field(r, k, "p", false)),
                        i32_of(field(r, k, "q", false)),
                    );
                    let rn = post_of(t, &PL::MAGIC, |d| province_pq(d) == pq)
                        .map(|(_, d)| u32_of(&d[PL::RESOLVED_NEXT..PL::RESOLVED_NEXT + 4]));
                    match rn {
                        Some(rn) => {
                            next.insert(pq, rn);
                        }
                        None => out
                            .coverage
                            .push(format!("{pq:?}: PROVINCE_OPEN without the Province")),
                    }
                }
                Kind::CLASH => {
                    out.clashes += 1;
                    let (p, q) = (
                        i32_of(field(r, k, "p", false)),
                        i32_of(field(r, k, "q", false)),
                    );
                    let b = u32_of(field(r, k, "bell", false));
                    let what = format!("clash ({p}, {q}) bell {b}");
                    match next.get(&(p, q)) {
                        Some(&n) if n == b => {}
                        n => out.coverage.push(format!("{what}: expected bell {n:?}")),
                    }
                    next.insert((p, q), b + 1);
                    match clash_check(t, r, (p, q), b, &last, &anchor_a, &seeds) {
                        Ok(()) => out.clashes_matched += 1,
                        Err(e) => out.mismatches.push(format!("{what}: {e}")),
                    }
                }
                Kind::SKIP => {
                    out.skips += 1;
                    let (p, q) = (
                        i32_of(field(r, k, "p", false)),
                        i32_of(field(r, k, "q", false)),
                    );
                    let b0 = u32_of(field(r, k, "b0", true));
                    let n = field(r, k, "n", true).first().copied().unwrap_or(0);
                    out.skipped_bells += n as u64;
                    let what = format!("skip ({p}, {q}) bells {b0}..{}", b0 + n as u32);
                    match next.get(&(p, q)) {
                        Some(&x) if x == b0 && n > 0 => {}
                        x => out.coverage.push(format!("{what}: expected bell {x:?}")),
                    }
                    next.insert((p, q), b0 + n as u32);
                    let logged: [u8; 32] = field(r, k, "quiet_digest", true)
                        .try_into()
                        .unwrap_or([0; 32]);
                    let spawns = camps.get(&(p, q)).cloned().unwrap_or_default();
                    match skip_check(t, (p, q), b0, n, &logged, &spawns, &last) {
                        Ok((loud, changed, spawned)) if loud.is_empty() => {
                            out.skips_matched += 1;
                            out.skipped_bells_changed += changed;
                            out.skip_camp_spawns += spawned;
                        }
                        Ok((loud, _, _)) => out
                            .loud_bells
                            .push(format!("{what}: the kernel finds bells {loud:?} not quiet")),
                        Err(e) => out.mismatches.push(format!("{what}: {e}")),
                    }
                }
                _ => {}
            }
        }
        for (k, a) in &t.post {
            match a {
                Some(a) if !a.data.is_empty() => {
                    last.insert(*k, a.data.clone());
                }
                _ => {
                    last.remove(k);
                }
            }
        }
    }
    out.resolved_next = next;
    out
}

fn clash_check(
    t: &TxRecord,
    r: &fclient::log::Record,
    pq: (i32, i32),
    b: u32,
    last: &HashMap<Address, Vec<u8>>,
    anchor_a: &HashMap<(u32, u8), u64>,
    seeds: &HashMap<(u32, u8, u64), [u8; 32]>,
) -> Result<(), String> {
    let k = Kind::CLASH;
    let (prov, _) = post_of(t, &PL::MAGIC, |d| province_pq(d) == pq)
        .ok_or("the resolve wrote no Province of its key")?;
    let before = last
        .get(&prov)
        .ok_or("no Province bytes before the resolve")?;
    let (ci, _) = post_of(t, &CL::MAGIC, |d| {
        let g = |o: usize| i16::from_le_bytes([d[o], d[o + 1]]) as i32;
        (g(CL::P), g(CL::Q)) == pq && u32_of(&d[CL::BELL..CL::BELL + 4]) == b
    })
    .ok_or("the resolve wrote no ClashInputs of its province-bell")?;
    // The gathered inputs as the resolve read them (before its fate table).
    let inputs = last
        .get(&ci)
        .ok_or("no ClashInputs bytes before the resolve")?;
    let region = region_of(ProvinceCoord::new(pq.0, pq.1));
    let a = anchor_a
        .get(&(b, region))
        .ok_or(format!("no ANCHOR of bell {b} region {region}"))?;
    let seed = seeds
        .get(&(b, region, *a))
        .ok_or(format!("no SEED naming THE anchor's A {a}"))?;
    let built = clash_model::build(before, inputs, b, seed)?;
    let o = built.resolve()?;
    let logged: [u8; 32] = field(r, k, "outcome_digest", true)
        .try_into()
        .map_err(|_| "short CLASH")?;
    if o.digest() != logged {
        return Err(format!(
            "outcome digest {} natively, {} logged",
            hex::encode(o.digest()),
            hex::encode(logged)
        ));
    }
    let eng = u32_of(field(r, k, "engagements", true));
    if o.engagements != eng {
        return Err(format!(
            "engagements {} natively, {eng} logged",
            o.engagements
        ));
    }
    if packed_fates(&built, &o)[..] != *field(r, k, "fates", true) {
        return Err("packed fates differ".into());
    }
    let idg = clash_model::input_digest(before, inputs, b, seed)?;
    if idg[..] != *field(r, k, "input_digest", true) {
        return Err("the logged input digest is not the §22 formula over the bytes".into());
    }
    Ok(())
}

fn skip_check(
    t: &TxRecord,
    pq: (i32, i32),
    b0: u32,
    n: u8,
    logged: &[u8; 32],
    spawns: &[Spawn],
    last: &HashMap<Address, Vec<u8>>,
) -> Result<(Vec<u32>, u64, u64), String> {
    let (prov, after) = post_of(t, &PL::MAGIC, |d| province_pq(d) == pq)
        .ok_or("the skip wrote no Province of its key")?;
    let before = last.get(&prov).ok_or("no Province bytes before the skip")?;
    let rep = clash_model::skip_replay(before, b0, n)?;
    let changed = rep.bells.iter().filter(|b| b.changed).count() as u64;
    // A bell the kernel would fight is the first finding (the Province
    // then differs as well).
    if !rep.loud().is_empty() {
        return Ok((rep.loud(), changed, rep.spawns.len() as u64));
    }
    // Every byte after the chained header (the SKIP's own chain link).
    let h = frontier_abi::layout::header::H_SIZE;
    if rep.after.get(h..) != after.get(h..) {
        let first = rep
            .after
            .iter()
            .zip(after)
            .enumerate()
            .skip(h)
            .find(|(_, (x, y))| x != y)
            .map(|(i, _)| i);
        return Err(format!(
            "the native replay leaves another Province (first differing byte {first:?})"
        ));
    }
    if clash_model::quiet_digest(after, b0, n)? != *logged {
        return Err("the logged quiet digest is not the §22 formula over the Province".into());
    }
    if rep.spawns != spawns {
        return Err(format!(
            "camps spawned natively {:?}, CAMP records {spawns:?}",
            rep.spawns
        ));
    }
    Ok((vec![], changed, rep.spawns.len() as u64))
}
