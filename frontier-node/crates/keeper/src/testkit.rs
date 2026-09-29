//! Fakes for the keeper's unit tests (W6T-2): a chain port over an account
//! map that counts its reads and can play a late tick, a drand port over
//! the test key, and builders for the program accounts the duties read.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use solana_address::Address;
use solana_hash::Hash;

use fclient::abi::{layout as l, magic, size};
use fclient::beacon::TestKey;
use fclient::decode::Season;
use fclient::ports::{
    Account, Beacon, ChainInfo, ChainPort, ClockSysvar, Cursor, DrandPort, PortError, PortResult,
    Signature, SimResult, Status, TxRecord,
};
use fclient::tx;

/// A chain port over an account map.
#[derive(Default)]
pub struct FakePort {
    pub accounts: Mutex<HashMap<Address, Account>>,
    pub clock: Mutex<ClockSysvar>,
    /// `accounts()` calls and the keys each asked for.
    pub calls: Mutex<Vec<Vec<Address>>>,
    pub sent: Mutex<Vec<fclient::Transaction>>,
    /// The chain slot `send` observes (a tick that runs late), when set.
    pub send_slot: AtomicU64,
    pub statuses: Mutex<HashMap<Signature, Status>>,
}

impl FakePort {
    pub fn put(&self, k: Address, a: Account) {
        self.accounts.lock().unwrap().insert(k, a);
    }
    pub fn set_clock(&self, slot: u64, now: i64) {
        let mut c = self.clock.lock().unwrap();
        c.slot = slot;
        c.unix_timestamp = now;
    }
    /// `accounts()` calls since the last take.
    pub fn take_calls(&self) -> Vec<Vec<Address>> {
        std::mem::take(&mut *self.calls.lock().unwrap())
    }
    /// How many times `k` was read since the last take (does not take).
    pub fn reads_of(&self, k: &Address) -> usize {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .map(|c| c.iter().filter(|x| *x == k).count())
            .sum()
    }
}

impl ChainPort for FakePort {
    async fn clock(&self) -> PortResult<ClockSysvar> {
        let mut c = *self.clock.lock().unwrap();
        let s = self.send_slot.load(Ordering::SeqCst);
        if s > 0 {
            c.slot = s;
        }
        Ok(c)
    }
    async fn accounts(&self, keys: &[Address], _: u64) -> PortResult<Vec<Option<Account>>> {
        self.calls.lock().unwrap().push(keys.to_vec());
        let m = self.accounts.lock().unwrap();
        Ok(keys.iter().map(|k| m.get(k).cloned()).collect())
    }
    async fn simulate(&self, _: &[u8]) -> PortResult<SimResult> {
        Ok(SimResult::default())
    }
    async fn send(&self, w: &[u8]) -> PortResult<Signature> {
        let t = tx::from_wire(w).map_err(|e| PortError::Decode(format!("{e:?}")))?;
        let sig = tx::signature(&t);
        self.sent.lock().unwrap().push(t);
        Ok(sig)
    }
    async fn statuses(&self, sigs: &[Signature]) -> PortResult<Vec<Option<Status>>> {
        let m = self.statuses.lock().unwrap();
        Ok(sigs.iter().map(|s| m.get(s).cloned()).collect())
    }
    async fn feed(&self, _: Cursor) -> PortResult<Vec<TxRecord>> {
        Ok(vec![])
    }
    async fn blockhash(&self) -> PortResult<(Hash, u64)> {
        Ok((Hash::new_from_array([3; 32]), 150))
    }
}

/// drand's test key, serving a round once `round_time ≤ now`.
#[derive(Clone)]
pub struct FakeDrand {
    pub key: Arc<TestKey>,
    pub now: Arc<AtomicI64>,
}

impl FakeDrand {
    pub fn new() -> FakeDrand {
        FakeDrand {
            key: Arc::new(TestKey::new()),
            now: Arc::new(AtomicI64::new(0)),
        }
    }
}

impl DrandPort for FakeDrand {
    fn round(
        &self,
        r: u64,
    ) -> impl std::future::Future<Output = Result<Option<Beacon>, PortError>> + Send {
        let ok = self.key.info().round_time(r) <= self.now.load(Ordering::SeqCst);
        let b = ok.then(|| self.key.beacon(r));
        async move { Ok(b) }
    }
    fn info(&self) -> ChainInfo {
        self.key.info()
    }
}

pub fn put(d: &mut [u8], o: usize, v: &[u8]) {
    d[o..o + v.len()].copy_from_slice(v);
}

/// An account of `program` with `data`.
pub fn acct(program: Address, data: Vec<u8>) -> Account {
    Account {
        lamports: 1_000_000,
        data,
        owner: program,
        executable: false,
    }
}

/// A Running season: genesis at `genesis_ts`, `end_bell`, the drand clock
/// of `info`, W = 600 s, a 60-s seed margin, a 1,008-bell close grace.
pub fn season(genesis_ts: i64, end_bell: u32, info: &ChainInfo) -> Season {
    let mut d = vec![0u8; size::SEASON];
    put(&mut d, 0, magic::SEASON);
    let mut s = Season::decode(&d).expect("season");
    s.h.season_id = 7;
    s.status = fclient::abi::status::RUNNING;
    s.genesis_ts = genesis_ts;
    s.end_bell = end_bell;
    s.drand_genesis = info.genesis_time;
    s.drand_period = info.period;
    s.reveal_window = 600;
    s.window_next = 600;
    s.window_from_bell = u32::MAX;
    s.seed_margin = 60;
    s.archive_after = 48 * 3_600;
    s.clash_close_grace = 1_008;
    s
}

/// A ClashInputs resolved at `resolved_ts` (seconds after genesis) with no
/// arrival present.
pub fn clash_inputs(resolved_ts: u32) -> Vec<u8> {
    use l::clash_inputs as c;
    let mut d = vec![0u8; size::CLASH_INPUTS];
    put(&mut d, 0, magic::CLASH_INPUTS);
    d[c::FLAGS] = 2 | 1;
    d[c::ARRIVALS_MASK..c::ARRIVALS_MASK + 4].copy_from_slice(&0x00FF_FFFFu32.to_le_bytes());
    put(&mut d, c::RESOLVED_TS, &resolved_ts.to_le_bytes());
    d
}

/// An ArrivalSlot of `host`, settled (`flags & 1`) or not, unclaimed.
pub fn arrival_slot(host: u64, settled: bool) -> Vec<u8> {
    use l::arrival_slot as s;
    let mut d = vec![0u8; size::ARRIVAL_SLOT];
    put(&mut d, 0, magic::ARRIVAL_SLOT);
    put(&mut d, s::HOST_ID, &host.to_le_bytes());
    d[s::FLAGS] = u8::from(settled);
    d
}

/// An ArrivalDay with no bit set.
pub fn arrival_day() -> Vec<u8> {
    let mut d = vec![0u8; size::ARRIVAL_DAY];
    put(&mut d, 0, magic::ARRIVAL_DAY);
    d
}

/// A Province resolved to `resolved_next`, with the sites given as
/// `(tile, state, faction, shield_until_bell)`.
pub fn province(resolved_next: u32, sites: &[(u8, u8, u8, u32)]) -> Vec<u8> {
    use l::province as p;
    use l::site as sm;
    let mut d = vec![0u8; size::PROVINCE];
    put(&mut d, 0, magic::PROVINCE);
    put(&mut d, p::RESOLVED_NEXT, &resolved_next.to_le_bytes());
    d[p::SITE_COUNT] = sites.len() as u8;
    for (k, &(tile, state, faction, shield)) in sites.iter().enumerate() {
        d[p::SITES + k] = tile;
        let o = p::SITE_MIRROR + k * p::SITE_MIRROR_STRIDE;
        d[o + sm::STATE] = state;
        d[o + sm::FACTION] = faction;
        put(&mut d, o + sm::SHIELD_UNTIL_BELL, &shield.to_le_bytes());
    }
    d
}

/// A final Holding of `faction` with one transit in `slot`:
/// `(state, host, depart_bell, arrive_bell, seal_root)`.
pub fn holding(faction: u8, slot: usize, tr: (u8, u64, u32, u32, [u8; 32])) -> Vec<u8> {
    use l::holding as h;
    use l::transit as t;
    let mut d = vec![0u8; size::HOLDING];
    put(&mut d, 0, magic::HOLDING);
    put(&mut d, 8, &7u64.to_le_bytes());
    if let Ok((p, q, site, gen, _)) = fclient::addr::host_parts(tr.1) {
        put(&mut d, h::P, &(p as i16).to_le_bytes());
        put(&mut d, h::Q, &(q as i16).to_le_bytes());
        d[h::SITE] = site;
        d[h::GEN] = gen;
    }
    d[h::STATE] = h::STATE_FINAL;
    d[h::FACTION] = faction;
    let o = h::TRANSIT + slot * h::TRANSIT_STRIDE;
    d[o + t::STATE] = tr.0;
    d[o + t::FACTION] = faction;
    put(&mut d, o + t::HOST_ID, &tr.1.to_le_bytes());
    put(&mut d, o + t::DEPART_BELL, &tr.2.to_le_bytes());
    put(&mut d, o + t::ARRIVE_BELL, &tr.3.to_le_bytes());
    put(&mut d, o + t::SEAL_ROOT, &tr.4);
    d
}
