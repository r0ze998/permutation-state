//! Reading the chain: transactions and their records (fetched in parallel,
//! with retries), scans of an address's history in a slot window, the live
//! world, and the season's accounts.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use borsh::BorshDeserialize;
use permutation_chain::seat::Seat;
use permutation_chain::state::{
    MemberAccount, RosterAccount, RosterEntry, CHUNK, WORLD_CHUNKS, WORLD_HEADER, WORLD_MAGIC,
    WORLD_SEED,
};
use permutation_rules::state::WorldState;
use permutation_server::chainlink::ChainLink;
use permutation_server::codec::{base64, base64_encode};
use permutation_server::replay::{short, tx_records, ChainData, Missing, Outcome, TxRecords};
use serde_json::json;

/// World chunk `chunk` of a season (`["world", id, [chunk]]`), as base58;
/// `None` if the program id is not a valid key. For the live-world read and
/// the scans (the anchor of the season's tick records is chunk 0).
pub fn world_chunk_address(program_id: &str, season_id: u64, chunk: u8) -> Option<String> {
    let program = pda::from_base58(program_id)?;
    let (key, _) = pda::find(&[WORLD_SEED, &season_id.to_le_bytes(), &[chunk]], &program)?;
    Some(pda::to_base58(&key))
}

/// Program-derived addresses without the Solana SDK (the server does not
/// depend on it): `find_program_address` is the highest bump whose
/// `sha256(seeds ‖ bump ‖ program ‖ "ProgramDerivedAddress")` is not an
/// ed25519 point, checked as curve25519-dalek's decompression does
/// (`(y² − 1)/(d·y² + 1)` is a square mod 2²⁵⁵ − 19).
mod pda {
    type Fe = [u64; 4];
    const P: Fe = [
        0xffff_ffff_ffff_ffed,
        0xffff_ffff_ffff_ffff,
        0xffff_ffff_ffff_ffff,
        0x7fff_ffff_ffff_ffff,
    ];
    /// Edwards d = −121665/121666 mod p.
    const D: Fe = [
        0x75eb_4dca_1359_78a3,
        0x0070_0a4d_4141_d8ab,
        0x8cc7_4079_7779_e898,
        0x5203_6cee_2b6f_fe73,
    ];
    const ONE: Fe = [1, 0, 0, 0];

    fn geq(a: &Fe, b: &Fe) -> bool {
        for i in (0..4).rev() {
            if a[i] != b[i] {
                return a[i] > b[i];
            }
        }
        true
    }

    /// `a − b` for `a >= b`.
    fn sub_raw(a: &Fe, b: &Fe) -> Fe {
        let mut out = [0u64; 4];
        let mut borrow = 0u64;
        for i in 0..4 {
            let (x, o1) = a[i].overflowing_sub(b[i]);
            let (y, o2) = x.overflowing_sub(borrow);
            out[i] = y;
            borrow = (o1 || o2) as u64;
        }
        out
    }

    /// `a` (< 2²⁵⁶) reduced below p.
    fn canon(mut a: Fe) -> Fe {
        while geq(&a, &P) {
            a = sub_raw(&a, &P);
        }
        a
    }

    fn add(a: &Fe, b: &Fe) -> Fe {
        let mut out = [0u64; 4];
        let mut carry = 0u128;
        for i in 0..4 {
            let cur = a[i] as u128 + b[i] as u128 + carry;
            out[i] = cur as u64;
            carry = cur >> 64;
        }
        canon(out) // a, b < p < 2²⁵⁵: no carry out
    }

    fn sub(a: &Fe, b: &Fe) -> Fe {
        if geq(a, b) {
            sub_raw(a, b)
        } else {
            sub_raw(&add_raw(a, &P), b)
        }
    }

    fn add_raw(a: &Fe, b: &Fe) -> Fe {
        let mut out = [0u64; 4];
        let mut carry = 0u128;
        for i in 0..4 {
            let cur = a[i] as u128 + b[i] as u128 + carry;
            out[i] = cur as u64;
            carry = cur >> 64;
        }
        out
    }

    fn mul(a: &Fe, b: &Fe) -> Fe {
        let mut w = [0u64; 8];
        for i in 0..4 {
            let mut carry = 0u128;
            for j in 0..4 {
                let cur = w[i + j] as u128 + a[i] as u128 * b[j] as u128 + carry;
                w[i + j] = cur as u64;
                carry = cur >> 64;
            }
            w[i + 4] = carry as u64;
        }
        // 2²⁵⁶ ≡ 38 (mod p): fold the high half in, twice.
        let mut r = [0u64; 4];
        let mut carry = 0u128;
        for i in 0..4 {
            let cur = w[i] as u128 + 38 * w[i + 4] as u128 + carry;
            r[i] = cur as u64;
            carry = cur >> 64;
        }
        let mut c = 38 * carry;
        for x in r.iter_mut() {
            let cur = *x as u128 + c;
            *x = cur as u64;
            c = cur >> 64;
        }
        if c != 0 {
            // r wrapped past 2²⁵⁶, so it is small now: add the 38 it stands for.
            r = add_raw(&r, &[38, 0, 0, 0]);
        }
        canon(r)
    }

    fn pow(base: &Fe, e: &Fe) -> Fe {
        let mut acc = ONE;
        for i in (0..256).rev() {
            acc = mul(&acc, &acc);
            if e[i / 64] >> (i % 64) & 1 == 1 {
                acc = mul(&acc, base);
            }
        }
        acc
    }

    /// Whether `bytes` decompress to an ed25519 point.
    pub fn on_curve(bytes: &[u8; 32]) -> bool {
        let mut y = [0u64; 4];
        for (i, l) in y.iter_mut().enumerate() {
            *l = u64::from_le_bytes(bytes[i * 8..i * 8 + 8].try_into().unwrap());
        }
        y[3] &= 0x7fff_ffff_ffff_ffff;
        let y = canon(y);
        let y2 = mul(&y, &y);
        let u = sub(&y2, &ONE);
        let v = add(&mul(&D, &y2), &ONE);
        // u/v is a square iff u·v is (v ≠ 0): Euler's criterion.
        let half = [
            0xffff_ffff_ffff_fff6,
            0xffff_ffff_ffff_ffff,
            0xffff_ffff_ffff_ffff,
            0x3fff_ffff_ffff_ffff,
        ];
        pow(&mul(&u, &v), &half) != sub(&P, &ONE)
    }

    /// `find_program_address`: the address and its bump.
    pub fn find(seeds: &[&[u8]], program: &[u8; 32]) -> Option<([u8; 32], u8)> {
        (0..=255u8).rev().find_map(|bump| {
            let mut parts: Vec<&[u8]> = seeds.to_vec();
            let b = [bump];
            parts.extend([&b[..], program, b"ProgramDerivedAddress"]);
            let key = permutation_rules::hash::sha256(&parts);
            (!on_curve(&key)).then_some((key, bump))
        })
    }

    const ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

    pub fn to_base58(b: &[u8]) -> String {
        let mut digits: Vec<u8> = Vec::new();
        for &byte in b {
            let mut carry = byte as u32;
            for d in digits.iter_mut() {
                carry += (*d as u32) << 8;
                *d = (carry % 58) as u8;
                carry /= 58;
            }
            while carry > 0 {
                digits.push((carry % 58) as u8);
                carry /= 58;
            }
        }
        let zeros = b.iter().take_while(|x| **x == 0).count();
        std::iter::repeat_n('1', zeros)
            .chain(digits.iter().rev().map(|d| ALPHABET[*d as usize] as char))
            .collect()
    }

    pub fn from_base58(s: &str) -> Option<[u8; 32]> {
        let mut bytes: Vec<u8> = Vec::new();
        for c in s.bytes() {
            let mut carry = ALPHABET.iter().position(|a| *a == c)? as u32;
            for b in bytes.iter_mut() {
                carry += *b as u32 * 58;
                *b = carry as u8;
                carry >>= 8;
            }
            while carry > 0 {
                bytes.push(carry as u8);
                carry >>= 8;
            }
        }
        let zeros = s.bytes().take_while(|c| *c == b'1').count();
        bytes.extend(std::iter::repeat_n(0, zeros));
        bytes.reverse();
        bytes.try_into().ok()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn field_constants() {
            // d · 121666 + 121665 ≡ 0.
            let x = add(&mul(&D, &[121666, 0, 0, 0]), &[121665, 0, 0, 0]);
            assert_eq!(x, [0; 4]);
            // The base point's y (4/5) is on the curve.
            let mut g = [0x66u8; 32];
            g[0] = 0x58;
            assert!(on_curve(&g));
        }

        #[test]
        fn base58_round_trips() {
            let k = "J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n";
            assert_eq!(to_base58(&from_base58(k).unwrap()), k);
            assert_eq!(to_base58(&[0; 32]), "11111111111111111111111111111111");
            assert_eq!(
                from_base58("11111111111111111111111111111111"),
                Some([0; 32])
            );
            assert_eq!(from_base58("0OIl"), None);
        }

        #[test]
        fn addresses_match_the_solana_sdk() {
            const PROGRAM: &str = "J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n";
            let program = from_base58(PROGRAM).unwrap();
            // Against the chain crate (solana-program) for many ids: about
            // half of the candidate hashes are points, so the curve check runs
            // both ways.
            let mut bumps = std::collections::BTreeSet::new();
            for id in (0..96u64).chain([1_790_355_636_798, u64::MAX]) {
                let (key, bump) = find(&[b"season", &id.to_le_bytes()], &program).unwrap();
                bumps.insert(bump);
                assert_eq!(
                    Some(to_base58(&key)),
                    permutation_chain::state::season_address(PROGRAM, id),
                    "season {id}"
                );
            }
            assert!(bumps.len() > 1, "some ids need a lower bump");
            // Against @solana/web3.js findProgramAddressSync (world chunks).
            for (id, k, addr) in [
                (7, 0, "8pwS7wU6es5KKwti8QRv17nsvrVputSRNGKntv8QqLfV"),
                (7, 19, "4ZWY9Gg4rhd8T1KXYMgiYLryJ3kCbey3QSiUNYxyHkqT"),
                (
                    1_790_355_636_798,
                    0,
                    "Br5pbJnvJ7M321buMPAfvQZWCa4NxwHzw4nsZer7h7R8",
                ),
                (
                    1_790_355_636_798,
                    5,
                    "6S1hy8NsbhGD22UxK5md3Rng6bc9NUg6cNY2qAmsxBZ9",
                ),
            ] {
                assert_eq!(
                    super::super::world_chunk_address(PROGRAM, id, k).as_deref(),
                    Some(addr)
                );
            }
        }
    }
}

/// RPC calls the scans may make (`--max-scan`): one per `getTransaction`
/// and one per page of a listing, shared by the fetching threads.
pub struct Budget {
    max: usize,
    used: AtomicUsize,
    out: AtomicBool,
}

impl Budget {
    pub fn new(max: usize) -> Budget {
        Budget {
            max,
            used: AtomicUsize::new(0),
            out: AtomicBool::new(false),
        }
    }

    /// Take one call; `false` (and exhausted from then on) when none is left.
    pub fn take(&self) -> bool {
        if self.used.fetch_add(1, Ordering::SeqCst) < self.max {
            return true;
        }
        self.out.store(true, Ordering::SeqCst);
        false
    }

    pub fn used(&self) -> usize {
        self.used.load(Ordering::SeqCst).min(self.max)
    }

    pub fn left(&self) -> usize {
        self.max - self.used()
    }

    pub fn exhausted(&self) -> bool {
        self.out.load(Ordering::SeqCst)
    }

    pub fn max(&self) -> usize {
        self.max
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ScanError {
    /// The scan budget ran out.
    Budget,
    Rpc(String),
}

/// Successful transactions listing `address` in slots `lo..=hi` (`hi` None:
/// up to the newest), newest first, through `getSignaturesForAddress` (1000
/// per page, paging with `before`, which starts at `start` when given).
/// Stops at the first entry below `lo`. Each page takes one call from the
/// budget, so a long history above `hi` cannot be listed without end; a
/// listing longer than the budget can fetch is `Err(Budget)`.
pub fn signatures_in(
    rpc: &ChainLink,
    address: &str,
    lo: u64,
    hi: Option<u64>,
    start: Option<&str>,
    budget: &Budget,
) -> Result<Vec<(String, u64)>, ScanError> {
    let mut out = Vec::new();
    let mut before = start.map(String::from);
    loop {
        if !budget.take() {
            return Err(ScanError::Budget);
        }
        let mut cfg = json!({"limit": 1000, "commitment": "confirmed"});
        if let Some(b) = &before {
            cfg["before"] = json!(b);
        }
        let page = retrying(|| rpc.rpc("getSignaturesForAddress", json!([address, cfg])))
            .map_err(ScanError::Rpc)?;
        let page = page.as_array().cloned().unwrap_or_default();
        let mut below = false;
        for e in &page {
            let (Some(sig), Some(slot)) = (e["signature"].as_str(), e["slot"].as_u64()) else {
                continue;
            };
            if slot < lo {
                below = true;
                break;
            }
            if hi.is_some_and(|h| slot > h) || !e["err"].is_null() {
                continue;
            }
            out.push((sig.to_string(), slot));
        }
        if below || page.len() < 1000 {
            return Ok(out);
        }
        if out.len() > budget.left() {
            budget.out.store(true, Ordering::SeqCst);
            return Err(ScanError::Budget);
        }
        before = page
            .last()
            .and_then(|e| e["signature"].as_str().map(String::from));
    }
}

/// Backoff (ms) between the attempts of an RPC call.
const RETRY_MS: [u64; 4] = [250, 500, 1000, 2000];

/// `f`, retried 4 times with backoff on an error (HTTP 429 and the like).
fn retrying<T>(f: impl Fn() -> Result<T, String>) -> Result<T, String> {
    let mut last = f();
    for ms in RETRY_MS {
        if last.is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(ms));
        last = f();
    }
    last
}

/// The records of every transaction in `sigs`, `threads` at a time, each
/// retried with backoff on an RPC error (a `null`, not found, is final:
/// retrying it only sleeps); a transaction the RPC cannot return is an
/// `Err` for its signature. With a `budget` (scans), each fetch takes one call and
/// fetching stops when it runs out; progress goes to stderr every 500.
pub fn fetch_many(
    rpc: &ChainLink,
    program: &str,
    sigs: &[String],
    threads: usize,
    budget: Option<&Budget>,
    what: &str,
) -> Vec<(String, Result<TxRecords, String>)> {
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let out = Mutex::new(Vec::with_capacity(sigs.len()));
    std::thread::scope(|s| {
        for _ in 0..threads.max(1) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= sigs.len() || budget.is_some_and(|b| !b.take()) {
                    break;
                }
                let sig = &sigs[i];
                let r =
                    retrying(|| rpc.transaction(sig)).and_then(|tx| tx_records(sig, &tx, program));
                out.lock().unwrap().push((i, sig.clone(), r));
                let n = done.fetch_add(1, Ordering::SeqCst) + 1;
                if budget.is_some() && n % 500 == 0 {
                    eprintln!("scanning {what}: {n} transactions");
                }
            });
        }
    });
    let mut out = out.into_inner().unwrap();
    out.sort_by_key(|x| x.0);
    out.into_iter().map(|(_, s, r)| (s, r)).collect()
}

/// A slot window of an address's history to scan: `lo..=hi` (`hi` None: to
/// the newest), listing from `start` backwards.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Window {
    pub lo: u64,
    pub hi: Option<u64>,
    pub start: Option<String>,
}

impl Window {
    pub fn slots(&self) -> String {
        format!(
            "slots {}..{}",
            self.lo,
            self.hi
                .map(|h| h.to_string())
                .unwrap_or_else(|| "now".into())
        )
    }
}

/// Where the data `missing` must lie in world chunk 0's ER history: after
/// the transaction that produced the current root (`outcome.last_slot`; 0
/// before the first), and no later than the first record of a later tick
/// logged on this season's chunk 0 (a record of another season cannot
/// narrow it). Listing starts after the first fetched transaction past
/// `hi`, so every transaction of slot `hi` is in.
pub fn window_for(missing: &Missing, outcome: &Outcome, data: &ChainData) -> Window {
    let t = missing.tick();
    let lo = outcome.last_slot.unwrap_or(0);
    let hi = data
        .bound_at
        .iter()
        .filter(|(tick, _)| *tick > t)
        .map(|(_, slot)| *slot)
        .min();
    let start = hi.and_then(|h| {
        data.fetched
            .iter()
            .filter(|(_, s)| **s > h)
            .min_by(|a, b| (a.1, a.0).cmp(&(b.1, b.0)))
            .map(|(sig, _)| sig.clone())
    });
    Window { lo, hi, start }
}

/// The world as it is now, read from the chain (not from the gateway).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveWorld {
    pub root: [u8; 32],
    pub tick: u16,
    pub phase: u8,
    pub slot: u64,
    pub layer: &'static str,
}

/// One account of `getMultipleAccounts`: owner and data (`None`: missing).
pub type AccountRead = Option<(String, Vec<u8>)>;

/// `getMultipleAccounts` (base64) of `addresses`: the slot and each account.
pub fn multiple_accounts(
    rpc: &ChainLink,
    addresses: &[String],
) -> Result<(u64, Vec<AccountRead>), String> {
    let r = retrying(|| {
        rpc.rpc(
            "getMultipleAccounts",
            json!([addresses, {"encoding": "base64", "commitment": "confirmed"}]),
        )
    })?;
    let accounts = r["value"]
        .as_array()
        .ok_or("getMultipleAccounts: no value")?
        .iter()
        .map(|a| {
            if a.is_null() {
                return Ok(None);
            }
            let data = base64(a["data"][0].as_str().unwrap_or(""))?;
            Ok(Some((a["owner"].as_str().unwrap_or("").to_string(), data)))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok((r["context"]["slot"].as_u64().unwrap_or(0), accounts))
}

/// The live world: the 20 chunk accounts from the layer where all 20 are
/// owned by the program (base first, then the ER); retried `attempts` times
/// `pause` apart while the world is moving between the layers.
pub fn live_world(
    base: &ChainLink,
    er: &ChainLink,
    program: &str,
    season_id: u64,
) -> Result<LiveWorld, String> {
    let addresses = (0..WORLD_CHUNKS as u8)
        .map(|k| world_chunk_address(program, season_id, k))
        .collect::<Option<Vec<_>>>()
        .ok_or("bad program id")?;
    read_live(
        |layer| multiple_accounts(if layer == "base" { base } else { er }, &addresses),
        program,
        3,
        Duration::from_secs(2),
    )
}

/// `live_world` over `fetch(layer)` ("base" or "ER").
pub fn read_live(
    fetch: impl Fn(&'static str) -> Result<(u64, Vec<AccountRead>), String>,
    program: &str,
    attempts: usize,
    pause: Duration,
) -> Result<LiveWorld, String> {
    let owned = |c: &[AccountRead]| {
        c.len() == WORLD_CHUNKS
            && c.iter()
                .all(|a| a.as_ref().is_some_and(|(o, _)| o == program))
    };
    let mut delegated = 0;
    for attempt in 0..attempts.max(1) {
        if attempt > 0 {
            std::thread::sleep(pause);
        }
        for layer in ["base", "ER"] {
            let (slot, chunks) = fetch(layer)?;
            if owned(&chunks) {
                let data: Vec<Vec<u8>> = chunks.into_iter().map(|c| c.unwrap().1).collect();
                return decode_live(&data, slot, layer);
            }
            if layer == "base" {
                delegated = chunks
                    .iter()
                    .filter(|a| a.as_ref().is_none_or(|(o, _)| o != program))
                    .count();
            }
        }
    }
    Err(format!(
        "the world is moving between layers ({delegated} of {WORLD_CHUNKS} chunks delegated); rerun"
    ))
}

fn decode_live(chunks: &[Vec<u8>], slot: u64, layer: &'static str) -> Result<LiveWorld, String> {
    let body = world_body(chunks)?;
    let state =
        WorldState::try_from_slice(&body).map_err(|e| format!("the world does not decode: {e}"))?;
    Ok(LiveWorld {
        root: permutation_rules::hash::sha256(&[&body]),
        tick: state.tick,
        phase: state.phase_cursor,
        slot,
        layer,
    })
}

/// The world body from the chunk accounts' data, as `Chunks::body` reads
/// it: magic, the length at bytes 8..12, and the bytes from `WORLD_HEADER`
/// on across the chunks.
pub fn world_body(chunks: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    if chunks.len() != WORLD_CHUNKS || chunks.iter().any(|c| c.len() < CHUNK) {
        return Err(format!(
            "expected {WORLD_CHUNKS} world chunks of {CHUNK} bytes"
        ));
    }
    let all: Vec<u8> = chunks.iter().flat_map(|c| &c[..CHUNK]).copied().collect();
    if all[..8] != WORLD_MAGIC {
        return Err("the world account holds no world yet (genesis still running?)".into());
    }
    let len = u32::from_le_bytes(all[8..12].try_into().unwrap()) as usize;
    if len > WORLD_CHUNKS * CHUNK - WORLD_HEADER {
        return Err(format!("world body length {len} does not fit the chunks"));
    }
    Ok(all[WORLD_HEADER..WORLD_HEADER + len].to_vec())
}

/// The base layer's records before the first tick: `PS_GENESIS` (root and
/// season seed), `PS_SEAT` (root and seats) and `PS_OPEN` (root), each with
/// its transaction; `bound` = logged by an instruction that read this
/// season's account (`accounts[1]` for SeatMembers and OpenGovernment).
#[derive(Clone, Debug, Default)]
pub struct PrefixData {
    pub genesis: Vec<([u8; 32], [u8; 32], String)>,
    pub seats: Vec<([u8; 32], Vec<Seat>, String, bool)>,
    pub open: Vec<([u8; 32], String, bool)>,
    /// Transactions read, and how many could not be.
    pub fetched: HashSet<String>,
    pub unreadable: usize,
    pub malformed: Vec<String>,
}

impl PrefixData {
    pub fn add(&mut self, tx: &TxRecords, season: &str) {
        if !self.fetched.insert(tx.sig.clone()) {
            return;
        }
        for e in &tx.emitted {
            let f = &e.record;
            let bound = e
                .accounts
                .as_ref()
                .and_then(|a| a.get(1))
                .map(String::as_str)
                == Some(season);
            let root = f
                .get(1)
                .and_then(|r| <[u8; 32]>::try_from(r.as_slice()).ok());
            let bad = |this: &mut PrefixData| {
                this.malformed.push(format!(
                    "{} in {}",
                    String::from_utf8_lossy(&f[0]),
                    short(&tx.sig)
                ))
            };
            match (f.first().map(|t| t.as_slice()), root) {
                (Some(b"PS_GENESIS"), Some(root)) => {
                    match f
                        .get(2)
                        .and_then(|s| <[u8; 32]>::try_from(s.as_slice()).ok())
                    {
                        Some(seed) => self.genesis.push((root, seed, tx.sig.clone())),
                        None => bad(self),
                    }
                }
                (Some(b"PS_SEAT"), Some(root)) => {
                    match f.get(2).and_then(|s| Vec::<Seat>::try_from_slice(s).ok()) {
                        Some(seats) => self.seats.push((root, seats, tx.sig.clone(), bound)),
                        None => bad(self),
                    }
                }
                (Some(b"PS_OPEN"), Some(root)) => self.open.push((root, tx.sig.clone(), bound)),
                (Some(b"PS_GENESIS" | b"PS_SEAT" | b"PS_OPEN"), None) => bad(self),
                _ => {}
            }
        }
    }

    /// Add fetched transactions (counting the unreadable ones).
    pub fn add_all(&mut self, txs: &[(String, Result<TxRecords, String>)], season: &str) {
        for (_, r) in txs {
            match r {
                Ok(tx) => self.add(tx, season),
                Err(_) => self.unreadable += 1,
            }
        }
    }

    pub fn has_genesis(&self, seed: &[u8; 32]) -> bool {
        self.genesis.iter().any(|g| g.1 == *seed)
    }

    pub fn has_bound_open(&self) -> bool {
        self.open.iter().any(|o| o.2)
    }
}

/// The season's roster account entries (operator AI members), if any.
pub fn roster_account(base: &ChainLink, program: &str, season_id: u64) -> Option<Vec<RosterEntry>> {
    let address = permutation_chain::state::roster_address(program, season_id)?;
    let data = base.account_data(&address).ok()??;
    let r = RosterAccount::deserialize(&mut &data[..]).ok()?;
    Some(r.entries)
}

/// Every Member account of the season on the base layer, in registration
/// order; `Err` when the RPC call fails (unreadable, not an empty season).
pub fn season_members(
    base: &ChainLink,
    program: &str,
    season_id: u64,
) -> Result<Vec<MemberAccount>, String> {
    let id = base64_encode(&season_id.to_le_bytes());
    let magic = base64_encode(&permutation_chain::state::MEMBER_MAGIC);
    let res = base.rpc(
        "getProgramAccounts",
        json!([program, {"encoding": "base64", "commitment": "confirmed", "filters": [
                {"memcmp": {"offset": 0, "bytes": magic, "encoding": "base64"}},
                {"memcmp": {"offset": 8, "bytes": id, "encoding": "base64"}}]}]),
    )?;
    let mut out: Vec<MemberAccount> = res
        .as_array()
        .ok_or("getProgramAccounts returned no list")?
        .iter()
        .filter_map(|a| base64(a["account"]["data"][0].as_str()?).ok())
        .filter_map(|d| MemberAccount::deserialize(&mut &d[..]).ok())
        .collect();
    out.sort_by_key(|m| m.index);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use permutation_server::replay::{Emitted, TickRec};

    const PROGRAM: &str = "J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n";

    /// Chunks laid out as `Chunks::write_body` writes them: magic, length,
    /// the header's meta, then the body across the chunks.
    fn chunks_with(body: &[u8], len: u32) -> Vec<Vec<u8>> {
        let mut all = vec![0u8; WORLD_CHUNKS * CHUNK];
        all[..8].copy_from_slice(&WORLD_MAGIC);
        all[8..12].copy_from_slice(&len.to_le_bytes());
        all[12] = 0xee; // meta bytes are not body
        all[WORLD_HEADER..WORLD_HEADER + body.len()].copy_from_slice(body);
        all.chunks(CHUNK).map(|c| c.to_vec()).collect()
    }

    #[test]
    fn world_body_roundtrip() {
        // Crosses two chunk boundaries.
        let body: Vec<u8> = (0..2 * CHUNK + 77).map(|i| (i * 7 % 251) as u8).collect();
        let chunks = chunks_with(&body, body.len() as u32);
        assert_eq!(world_body(&chunks).unwrap(), body);
        // The body starts after the header, not at chunk 0's first byte.
        assert_eq!(chunks[0][WORLD_HEADER], body[0]);
        assert_eq!(chunks[1][0], body[CHUNK - WORLD_HEADER]);
        let too_big = (WORLD_CHUNKS * CHUNK - WORLD_HEADER + 1) as u32;
        assert!(world_body(&chunks_with(&body, too_big)).is_err());
        let mut genesis = chunks.clone();
        genesis[0][..8].copy_from_slice(b"PSGENJB1");
        assert!(world_body(&genesis).unwrap_err().contains("no world"));
        assert!(world_body(&chunks[..19]).is_err());
        // The live world is the body's root and its tick.
        let rules = permutation_rules::Ruleset::new(permutation_rules::Preset::Blitz);
        let w = permutation_rules::genesis::new_season(
            &rules,
            &[1; 32],
            &[2; 32],
            &permutation_rules::genesis::nation_entries(2),
        )
        .unwrap();
        let body = borsh::to_vec(&w).unwrap();
        let chunks = chunks_with(&body, body.len() as u32);
        let live = decode_live(&chunks, 9, "ER").unwrap();
        assert_eq!(
            (live.root, live.tick, live.slot),
            (w.state_root().unwrap(), 0, 9)
        );
    }

    #[test]
    fn live_world_mixed_owners() {
        let world = chunks_with(&[0], 1);
        let chunk = |owner: &str, k: usize| Some((owner.to_string(), world[k].clone()));
        let calls = std::cell::Cell::new(0);
        // Base: chunk 7 delegated; ER: chunk 0 missing. Moving: retried, then an error (not a failure).
        let r = read_live(
            |layer| {
                calls.set(calls.get() + 1);
                let c = (0..WORLD_CHUNKS)
                    .map(|k| match (layer, k) {
                        ("base", 7) => chunk("DELeGGvXpWV2fqJUhqcF5ZSYMS4JTLjteaAMARRSaeSh", k),
                        ("ER", 0) => None,
                        _ => chunk(PROGRAM, k),
                    })
                    .collect();
                Ok((5, c))
            },
            PROGRAM,
            3,
            Duration::ZERO,
        );
        assert_eq!(
            r.unwrap_err(),
            "the world is moving between layers (1 of 20 chunks delegated); rerun"
        );
        assert_eq!(calls.get(), 6);
        // All 20 on the ER: read from there.
        let r = read_live(
            |layer| {
                let c = (0..WORLD_CHUNKS)
                    .map(|k| {
                        if layer == "ER" {
                            chunk(PROGRAM, k)
                        } else {
                            None
                        }
                    })
                    .collect();
                Ok((8, c))
            },
            PROGRAM,
            3,
            Duration::ZERO,
        );
        assert!(r.is_err(), "a one-byte body does not decode as a world");
        let rpc_down = read_live(
            |_| Err("gateway unreachable".into()),
            PROGRAM,
            3,
            Duration::ZERO,
        );
        assert_eq!(rpc_down.unwrap_err(), "gateway unreachable");
    }

    fn tick_rec(sig: &str, tick: u16, slot: u64, bound: bool) -> (TxRecords, TickRec) {
        let fields = vec![
            b"PS_TICK".to_vec(),
            tick.to_le_bytes().to_vec(),
            vec![12],
            vec![tick as u8; 32],
            vec![tick as u8 + 1; 32],
            vec![0; 32],
        ];
        let tx = TxRecords {
            sig: sig.into(),
            slot,
            emitted: vec![Emitted {
                record: fields.clone(),
                accounts: Some(vec![if bound { "X0" } else { "Y0" }.into()]),
            }],
        };
        let rec = TickRec {
            sig: sig.into(),
            slot,
            pos: 0,
            tick,
            to: 12,
            stop: 12,
            degraded: false,
            pre: [tick as u8; 32],
            post: [tick as u8 + 1; 32],
            input_hash: [0; 32],
            fields,
            bound,
        };
        (tx, rec)
    }

    #[test]
    fn scan_window() {
        let mut d = ChainData::new("X0");
        for (sig, tick, slot, bound) in [
            ("t4", 4, 100, true),
            ("t6a", 6, 140, true),
            ("t6b", 6, 150, true),
            ("t7", 7, 170, true),
            ("foreign6", 6, 120, false),
        ] {
            d.add_tx(&tick_rec(sig, tick, slot, bound).0);
        }
        // A foreign PS_COMMITS for tick 6 at an earlier slot.
        let list = borsh::to_vec(&Vec::<(u16, u8, u32, [u8; 32])>::new()).unwrap();
        d.add_tx(&TxRecords {
            sig: "foreign-close".into(),
            slot: 110,
            emitted: vec![Emitted {
                record: vec![b"PS_COMMITS".to_vec(), 6u16.to_le_bytes().to_vec(), list],
                accounts: Some(vec!["Y0".into()]),
            }],
        });
        let outcome = Outcome {
            last_slot: Some(100),
            last_sig: Some("t4".into()),
            ..Default::default()
        };
        let gap = Missing::Successor {
            tick: 5,
            root: [5; 32],
        };
        let w = window_for(&gap, &outcome, &d);
        assert_eq!(
            w,
            Window {
                lo: 100,
                hi: Some(140),
                start: Some("t6b".into())
            }
        );
        // A tail gap: up to now.
        let tail = Missing::Commits { tick: 7 };
        let w = window_for(&tail, &outcome, &d);
        assert_eq!((w.hi, w.start), (None, None));
        // Before the first record replayed: from slot 0.
        let w = window_for(&gap, &Outcome::default(), &d);
        assert_eq!(w.lo, 0);
    }

    #[test]
    fn world_chunk_addresses_are_the_program_pdas() {
        let a = world_chunk_address(PROGRAM, 7, 0).unwrap();
        assert_ne!(a, world_chunk_address(PROGRAM, 7, 1).unwrap());
        assert_ne!(a, world_chunk_address(PROGRAM, 8, 0).unwrap());
        assert!(world_chunk_address("not a key", 7, 0).is_none());
    }
}
