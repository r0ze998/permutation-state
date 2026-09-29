//! W6T-2 (w6-s7 triage, U2): criterion 3's keeper latency over `localnet`
//! in process (20×, virtual time).
//!
//! - `round_425_in_publication_slot_still_anchors_in_slot`: drand answers
//!   425 to the first request for a round (drand-replay learns the chain
//!   Clock by polling, so the keeper's first request in the publication
//!   slot often came too early). The keeper asks again on its 100-ms idle
//!   ticks in the same slot, so THE anchor is still sent in the
//!   publication slot (cause B: the round was asked for at most once per
//!   slot, which made round → anchor p50 exactly 2 slots).
//! - `status_answers_during_a_long_tick`: `/v1/status` answers within 1 s
//!   with the last tick's status while a tick runs long (322 of 1,035
//!   keeper status samples were null in w6-s7).

mod common;
mod model;

use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fclient::abi::status;
use fclient::ports::{
    Account, Beacon, ChainInfo, ChainPort, ClockSysvar, Cursor, DrandPort, PortError, PortResult,
    Signature, SimResult, Status, TxRecord,
};
use fclient::Address;
use keeper_core::journal::Journal;
use keeper_core::{Keeper, PLAY_ROLES};
use solana_hash::Hash;

use common::*;

/// The test key behind drand-replay's 425: the first request for each
/// published round is answered "not yet".
#[derive(Clone)]
struct FirstAsk425 {
    inner: Gated,
    denied: Arc<Mutex<HashSet<u64>>>,
}

impl DrandPort for FirstAsk425 {
    fn round(
        &self,
        r: u64,
    ) -> impl std::future::Future<Output = Result<Option<Beacon>, PortError>> + Send {
        let inner = self.inner.clone();
        let denied = self.denied.clone();
        async move {
            let b = inner.round(r).await?;
            if b.is_some() && denied.lock().unwrap().insert(r) {
                return Ok(None);
            }
            Ok(b)
        }
    }
    fn info(&self) -> ChainInfo {
        self.inner.info()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn round_425_in_publication_slot_still_anchors_in_slot() {
    let w = world().await;
    println!("program: {}", w.which.label());
    let drand = FirstAsk425 {
        inner: w.drand.clone(),
        denied: Arc::new(Mutex::new(HashSet::new())),
    };
    let mut cfg = keeper_config(&w);
    cfg.roles = vec!["beacon".into()];
    let mut k = Keeper::new(
        cfg,
        w.ip.clone(),
        drand,
        &[0x42; 32],
        Some(Journal::open(std::path::Path::new(":memory:")).unwrap()),
    )
    .unwrap();
    fund(&w, &k.payers);
    k.start().await.unwrap();
    let s = w.season();
    let sc = fclient::clock::SeasonClock::from_season(&s);
    let info = w.drand.key.info();
    // The slot in which T(b) is first served (drand-replay's rule: round
    // time + delay ≤ the chain's Clock).
    let mut pub_slot: BTreeMap<u32, u64> = BTreeMap::new();
    let last_bell = 10u32;
    while w.now() < s.genesis_ts + (last_bell as i64 + 2) * 600 {
        for b in 0..=last_bell {
            if !pub_slot.contains_key(&b)
                && info.round_time(sc.tlock_round(b)) + DRAND_DELAY <= w.now()
            {
                pub_slot.insert(b, w.slot());
            }
        }
        k.tick().await.unwrap();
        // The keeper's 100-ms idle tick inside the same slot.
        k.tick().await.unwrap();
        w.step();
    }
    assert_eq!(w.season().effective_status(w.now()), status::RUNNING);
    let j = k.journal.as_ref().unwrap();
    let mut late = vec![];
    for b in 2..=last_bell {
        let first = (0..3)
            .flat_map(|c| j.attempts_of(&format!("anchor-multi:{b}:{c}")).unwrap())
            .map(|a| a.sent_slot)
            .min()
            .expect("anchored");
        if first != pub_slot[&b] {
            late.push((b, pub_slot[&b], first));
        }
    }
    assert!(
        late.is_empty(),
        "(bell, publication slot, first send) sent after the publication slot: {late:?}"
    );
}

/// `localnet` in process behind a delay on every read (a slow RPC).
#[derive(Clone)]
struct Slow {
    inner: localnet::InProcess,
    ms: Arc<AtomicU64>,
}

impl Slow {
    async fn wait(&self) {
        let ms = self.ms.load(Ordering::SeqCst);
        if ms > 0 {
            tokio::time::sleep(Duration::from_millis(ms)).await;
        }
    }
}

impl ChainPort for Slow {
    async fn clock(&self) -> PortResult<ClockSysvar> {
        self.inner.clock().await
    }
    async fn accounts(&self, keys: &[Address], c: u64) -> PortResult<Vec<Option<Account>>> {
        self.wait().await;
        self.inner.accounts(keys, c).await
    }
    async fn simulate(&self, t: &[u8]) -> PortResult<SimResult> {
        self.inner.simulate(t).await
    }
    async fn send(&self, t: &[u8]) -> PortResult<Signature> {
        self.inner.send(t).await
    }
    async fn statuses(&self, s: &[Signature]) -> PortResult<Vec<Option<Status>>> {
        self.inner.statuses(s).await
    }
    async fn feed(&self, after: Cursor) -> PortResult<Vec<TxRecord>> {
        self.wait().await;
        self.inner.feed(after).await
    }
    async fn blockhash(&self) -> PortResult<(Hash, u64)> {
        self.inner.blockhash().await
    }
}

/// One `GET /v1/status`: `(answered within the timeout, body)`.
async fn get_status(addr: std::net::SocketAddr, token: &str) -> (bool, serde_json::Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let go = async {
        let mut s = tokio::net::TcpStream::connect(addr).await.ok()?;
        let req = format!(
            "GET /v1/status HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n"
        );
        s.write_all(req.as_bytes()).await.ok()?;
        let mut out = vec![];
        s.read_to_end(&mut out).await.ok()?;
        let text = String::from_utf8_lossy(&out).to_string();
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
        Some(serde_json::from_str(&body).unwrap_or(serde_json::Value::Null))
    };
    match tokio::time::timeout(Duration::from_secs(1), go).await {
        Ok(Some(v)) => (true, v),
        _ => (false, serde_json::Value::Null),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn status_answers_during_a_long_tick() {
    let w = world().await;
    println!("program: {}", w.which.label());
    let slow = Slow {
        inner: w.ip.clone(),
        ms: Arc::new(AtomicU64::new(0)),
    };
    let mut cfg = keeper_config(&w);
    cfg.roles.extend(PLAY_ROLES.iter().map(|r| r.to_string()));
    let mut k = Keeper::new(cfg, slow.clone(), w.drand.clone(), &[0x43; 32], None).unwrap();
    fund(&w, &k.payers);
    k.start().await.unwrap();
    let genesis_ts = w.season().genesis_ts;
    while w.now() < genesis_ts + 3 * 600 {
        k.tick().await.unwrap();
        w.step();
    }
    assert_eq!(w.season().effective_status(w.now()), status::RUNNING);
    // The API on a port of this unit's range (41100-41999).
    let token = "s".repeat(32);
    let mut running = None;
    for port in 41_151..41_199u16 {
        let a: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        if let Ok(r) = keeper_core::api::serve(a, k.shared.clone(), token.clone()).await {
            running = Some(r);
            break;
        }
    }
    let running = running.expect("a free port in 41151-41198");
    let addr = running.addr;
    let (ok, v) = get_status(addr, &token).await;
    assert!(ok && v["slot"].as_u64().is_some(), "status before: {v}");
    // A long tick: every read takes 150 ms (the w6-s7 closes tick took
    // 1.4 s; the stack's status sampler times out at 5 s).
    slow.ms.store(150, Ordering::SeqCst);
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (stop2, tok2) = (stop.clone(), token.clone());
    let poller = tokio::spawn(async move {
        let mut samples = vec![];
        while !stop2.load(Ordering::SeqCst) {
            let t0 = Instant::now();
            let (ok, v) = get_status(addr, &tok2).await;
            samples.push((ok, t0.elapsed(), v));
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        samples
    });
    w.step();
    let t0 = Instant::now();
    k.tick().await.unwrap();
    let tick = t0.elapsed();
    stop.store(true, Ordering::SeqCst);
    let samples = poller.await.unwrap();
    running.stop();
    println!("long tick {tick:?}, {} status samples", samples.len());
    assert!(
        tick > Duration::from_millis(1_500),
        "the tick was long: {tick:?}"
    );
    assert!(samples.len() >= 5, "sampled during the tick");
    let bad: Vec<_> = samples
        .iter()
        .filter(|(ok, dt, v)| !ok || *dt > Duration::from_secs(1) || v["slot"].as_u64().is_none())
        .map(|(ok, dt, v)| (*ok, *dt, v.to_string().chars().take(40).collect::<String>()))
        .collect();
    assert!(
        bad.is_empty(),
        "{} of {} status samples not answered with a status: {bad:?}",
        bad.len(),
        samples.len()
    );
}
