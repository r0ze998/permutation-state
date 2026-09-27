//! `one_day_beacons` (Gate W2, M1 contract §12): one game day of genesis,
//! genesis rings, anchors, caches and beacon logs by the keeper over
//! `localnet` in process (20×, real slot counts, virtual time) with the
//! test-beacon key (I-53).
//!
//! The program is the test-beacon `.so` when `PSF_FRONTIER_SO` names one
//! (the integrator runs it that way once W2-A is merged), else the native
//! model of `tests/model`; the test prints which. Against the W2 program
//! OpenRing answers NotImplemented (99, land is W3-A's) and the ring part is
//! reported, not failed.
//!
//! Checked: every bell × region has THE anchor (round `T(b)`, anchored after
//! the round, `rent_to` = the creating fee payer, a delay-pool key) and a
//! seed cache with the seed of `S(b, A)`; the beacon logs follow the rounds;
//! anchor and cache latencies (E5 criterion 3 in slots); the fee payers of
//! every landed keeper transaction are uniform over the delay pool (χ²);
//! payer care fills the reveal pool from ≥ 4 funders; a restart in the
//! middle of the day adopts its in-flight versions from the journal and
//! resumes; the journal ends with nothing in flight; findex archives the
//! day and indexes its ANCHOR/SEED/BEACON records.

mod common;
mod model;

use std::collections::{BTreeMap, HashMap, HashSet};

use fclient::abi::{status, BELL_SECS};
use fclient::decode::{BeaconLog, BellAnchor, Frontier, SeedCache};
use fclient::ports::ChainPort;
use fclient::{Address, Signer};
use keeper_core::journal::{self, Journal};
use keeper_core::{quantile, Keeper};

use common::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_day_beacons() {
    let t0 = std::time::Instant::now();
    let w = world().await;
    println!("program: {}", w.which.label());
    let season = w.season();
    let genesis_ts = season.genesis_ts;
    let dir = std::env::temp_dir().join(format!("keeper-one-day-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let jpath = dir.join("keeper.journal.sqlite");
    let master = [0x77u8; 32];

    let cfg = keeper_config(&w);
    let lock = journal::lock(&jpath).unwrap();
    let mut k = Keeper::new(
        cfg.clone(),
        w.ip.clone(),
        w.drand.clone(),
        &master,
        Some(Journal::open(&jpath).unwrap()),
    )
    .unwrap();
    fund(&w, &k.payers);
    k.start().await.unwrap();

    // One game day of bells, plus the caches of the last bell.
    let end = genesis_ts + 144 * BELL_SECS + 720;
    let restart_at = genesis_ts + 72 * BELL_SECS + 300;
    let mut restarted = false;
    let mut relock = None;
    let mut adopted = 0;
    let mut slots = 0u64;
    let mut max_pending = 0usize;
    while w.now() < end {
        k.tick().await.unwrap();
        max_pending = max_pending.max(k.engine.pending_len());
        w.step();
        slots += 1;
        if !restarted && w.now() >= restart_at {
            // Crash mid-flight: at the first tick that sends versions after
            // `restart_at` the keeper dies before they land; a new process
            // takes the lock, reconciles the journal and adopts them.
            let r = k.tick().await.unwrap();
            if r.sent == 0 {
                continue;
            }
            restarted = true;
            let in_flight_before = k.journal.as_ref().unwrap().in_flight().unwrap().len();
            drop(k);
            // The dead process's lock goes with it; the new one takes it.
            lock.unlock().unwrap();
            relock = Some(journal::lock(&jpath).expect("the restart takes the lock"));
            let mut k2 = Keeper::new(
                cfg.clone(),
                w.ip.clone(),
                w.drand.clone(),
                &master,
                Some(Journal::open(&jpath).unwrap()),
            )
            .unwrap();
            adopted = k2.start().await.unwrap();
            println!(
                "restart at slot {}: {in_flight_before} in flight, {adopted} adopted",
                w.slot()
            );
            k = k2;
        }
    }
    let wall = t0.elapsed();
    println!(
        "ran {slots} slots ({:.1} game h) in {:.1} s; max pending writes {max_pending}",
        slots as f64 * 8.0 / 3600.0,
        wall.as_secs_f64()
    );

    // ---- genesis
    let s = w.season();
    assert_eq!(s.effective_status(w.now()), status::RUNNING);
    let g = s.genesis_round;
    let sig96 = fclient::beacon::decompress_sig(&w.drand.key.sign(g)).unwrap();
    assert_eq!(s.genesis_seed, fclient::beacon::seed_of(g, &sig96));
    let fr = Frontier::decode(&w.ip.lock().account(&w.addrs.frontier()).unwrap().data).unwrap();
    if k.rings.unsupported {
        println!("genesis rings: the program answers NotImplemented (W3-A's land instructions)");
    } else {
        assert_eq!(fr.rings_opened, s.genesis_ring as u16 + 1);
        let want: usize = (0..=s.genesis_ring as u32)
            .map(|d| permutation_rules::frontier::geometry::ring_provinces(d).len())
            .sum();
        assert_eq!(k.rings.opened.len(), want, "every genesis province");
        assert!(k.rings.genesis_done);
        println!(
            "genesis rings 0..={}: {want} provinces opened",
            s.genesis_ring
        );
    }

    // ---- anchors and caches, every bell × region
    let sc = fclient::clock::SeasonClock::from_season(&s);
    let delay: HashSet<Address> = k.payers.delay.addresses().into_iter().collect();
    let reveal: HashSet<Address> = k.payers.reveal.addresses().into_iter().collect();
    let j = k.journal.as_ref().unwrap();
    let mut a_offsets = vec![];
    for b in 0..144u32 {
        for r in 0..16u8 {
            let acct =
                w.ip.lock()
                    .account(&w.addrs.anchor(b, r))
                    .unwrap_or_else(|| panic!("anchor {b}/{r}"));
            assert_eq!(acct.owner, w.program);
            let an = BellAnchor::decode(&acct.data).unwrap();
            assert_eq!((an.bell, an.region), (b, r));
            assert_eq!(an.round, sc.tlock_round(b), "T(b)");
            assert!(
                an.a >= sc.drand.round_time(an.round) + DRAND_DELAY,
                "anchored after the round"
            );
            a_offsets.push(an.a - fclient::clock::bell_end(genesis_ts, b));
            assert!(delay.contains(&an.rent_to), "rent_to is a delay-pool payer");
            // rent returns to the payer that created it: it paid a landed version.
            let created = [
                format!("anchor-multi:{b}:{}", r as usize / 7),
                format!("anchor:{b}:{r}"),
            ]
            .iter()
            .flat_map(|key| j.attempts_of(key).unwrap())
            .any(|a| a.status == "landed" && a.payer == an.rent_to.to_string());
            assert!(created, "anchor {b}/{r}: rent_to paid a landed version");
            let info = k.beacon.anchors.get(&(b, r)).expect("known to the keeper");
            let c = info.cache.expect("a seed cache");
            let ca =
                w.ip.lock()
                    .account(&w.addrs.seed_cache(b, r, c.nonce))
                    .expect("cache");
            let cache = SeedCache::decode(&ca.data).unwrap();
            let s_round = sc.seed_round(b, an.a);
            assert_eq!(cache.round, s_round, "S(b, A)");
            let sig = fclient::beacon::decompress_sig(&w.drand.key.sign(s_round)).unwrap();
            assert_eq!(cache.seed, fclient::beacon::seed_of(s_round, &sig));
            assert_eq!(cache.anchor_key, w.addrs.anchor(b, r));
            assert!(delay.contains(&cache.rent_to));
        }
    }
    a_offsets.sort_unstable();
    println!(
        "A − bell_end: min {} s, median {} s, max {} s",
        a_offsets[0],
        a_offsets[a_offsets.len() / 2],
        a_offsets[a_offsets.len() - 1]
    );
    for r in 0..16u8 {
        let l =
            BeaconLog::decode(&w.ip.lock().account(&w.addrs.beacon_log(r)).unwrap().data).unwrap();
        assert!(
            sc.drand.round_time(l.latest_round) >= fclient::clock::bell_start(genesis_ts, 143),
            "beacon log {r} follows the rounds"
        );
    }

    // ---- latencies in slots (E5 criterion 3 at 20×: p99 ≤ 2)
    let al = &k.beacon.anchor_latency;
    let sl = &k.beacon.seed_latency;
    println!(
        "anchor latency slots (after the restart's re-scan): n {} p50 {:?} p99 {:?} max {:?}; seed latency: n {} p50 {:?} p99 {:?} max {:?}",
        al.len(), quantile(al, 0.5), quantile(al, 0.99), al.iter().max(),
        sl.len(), quantile(sl, 0.5), quantile(sl, 0.99), sl.iter().max()
    );
    assert!(quantile(al, 0.99).unwrap() <= 2);
    assert!(quantile(sl, 0.99).unwrap() <= 2);

    // ---- payers: every landed keeper tx is paid by a pool key; the delay
    // pool's draws are uniform (χ², 31 dof, p = 0.001 critical 61.10).
    // Every transaction of the run (the harness's port filters by program).
    let all = localnet::InProcess::from_chain(w.ip.chain.clone(), None);
    let mut feed = vec![];
    loop {
        let after = feed.last().map_or(0, |r: &fclient::ports::TxRecord| r.seq);
        let got = all.feed(fclient::ports::Cursor(after)).await.unwrap();
        if got.is_empty() {
            break;
        }
        feed.extend(got);
    }
    let auth = w.authority.pubkey();
    let mut by_payer: HashMap<Address, u64> = HashMap::new();
    let mut funders_paid = 0u64;
    let funders: HashSet<Address> = k.payers.funders.keys.iter().map(|x| x.pubkey()).collect();
    for r in &feed {
        let t = fclient::tx::from_wire(&r.tx).unwrap();
        let payer = t.message.account_keys[0];
        if payer == auth || r.tx.is_empty() {
            continue;
        }
        assert!(
            !reveal.contains(&payer),
            "no D/N write is paid by the reveal pool"
        );
        if funders.contains(&payer) {
            funders_paid += 1;
            continue;
        }
        assert!(delay.contains(&payer), "unknown fee payer {payer}");
        *by_payer.entry(payer).or_default() += 1;
    }
    let n: u64 = by_payer.values().sum();
    let e = n as f64 / 32.0;
    let chi2: f64 = k
        .payers
        .delay
        .addresses()
        .iter()
        .map(|a| {
            let h = *by_payer.get(a).unwrap_or(&0) as f64;
            (h - e).powi(2) / e
        })
        .sum();
    println!("delay-pool draws: {n} over 32 payers, χ² {chi2:.1}; funder transfers {funders_paid}");
    assert!(chi2 < 61.10, "χ² {chi2}");

    // ---- payer care: the reveal pool was filled by the funders (≥ 4).
    assert!(k.payers.funders.keys.len() >= 4);
    assert!(
        funders_paid >= 150,
        "every reveal payer topped up: {funders_paid}"
    );
    assert_eq!(k.payers.effective_n(fclient::abi::Class::W), 150);
    assert!(
        k.effective_n.iter().any(|x| x.1 == 150),
        "effective N logged per bell"
    );

    // ---- journal: nothing left in flight at the end but this slot's.
    let counts: BTreeMap<String, u64> = j.status_counts().unwrap().into_iter().collect();
    println!(
        "journal: {counts:?}; restart adopted {adopted}; alerts: {}",
        k.alerts.len()
    );
    let kinds: BTreeMap<&str, usize> = k.alerts.iter().fold(BTreeMap::new(), |mut m, a| {
        *m.entry(a.1.as_str()).or_default() += 1;
        m
    });
    println!("alert kinds: {kinds:?}");
    for (kind, st) in &k.engine.stats {
        println!(
            "  {kind:14} writes {:5} versions {:5} landed {:5} failed {:3} dead {:3} fees {:>11} p99 {:?}",
            st.writes, st.versions, st.landed, st.failed, st.dead, st.fees_charged,
            quantile(&st.latency_slots, 0.99)
        );
    }
    assert!(counts.get("landed").copied().unwrap_or(0) > 144 * 20);
    assert!(adopted > 0, "the restart adopted in-flight versions");
    assert!(
        counts.get("sent").copied().unwrap_or(0) <= k.engine.pending_len() as u64 * 4,
        "nothing but the last slot's versions in flight"
    );
    assert!(
        !kinds.contains_key("dead") && !kinds.contains_key("retry-ladder"),
        "no write died, no budget breach: {kinds:?}"
    );

    // ---- findex over the day's feed
    let fdir = dir.join("findex");
    let fx = findex_like(&fdir, &w).await;
    println!("findex: {fx}");
    drop(relock);
    drop(lock);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Archives and indexes the day with findex's LocalnetFeed; checks the
/// record counts and the archive.
async fn findex_like(dir: &std::path::Path, w: &World) -> String {
    let mut fx = findex::Findex::open(dir, w.program, Some(SEASON_ID), 8 * 1024 * 1024).unwrap();
    let mut feed = findex::LocalnetFeed::new(w.ip.clone());
    let got = fx.ingest(&mut feed).await.unwrap();
    let (txs, failed) = fx.index.tx_count().unwrap();
    let anchors = fx.index.records(50, 0, u32::MAX).unwrap().len();
    let seeds = fx.index.records(51, 0, u32::MAX).unwrap().len();
    let beacons = fx.index.records(52, 0, u32::MAX).unwrap().len();
    let mut per: HashMap<(u32, u8), u32> = HashMap::new();
    for r in fx.index.records(51, 0, u32::MAX).unwrap() {
        let b = u32::from_le_bytes(r.key[..4].try_into().unwrap());
        *per.entry((b, r.key[4])).or_default() += 1;
    }
    assert!(
        per.values().all(|&n| n == 1),
        "one seed cache per bell-region, also across the restart"
    );
    let verified = fx.archive.verify().unwrap();
    assert_eq!(verified as usize, got.len());
    assert!(anchors >= 144 * 16);
    assert!(seeds >= 144 * 16);
    assert!(beacons >= 144);
    let present_anchors = fx
        .index
        .present_of_kind(frontier_abi::layout::AccountKind::BellAnchor)
        .unwrap()
        .len();
    assert!(present_anchors >= 144 * 16);
    format!(
        "{txs} txs ({failed} failed), {} segments; ANCHOR {anchors}, SEED {seeds}, BEACON {beacons}; {present_anchors} anchors present",
        fx.archive.manifest.segments.len()
    )
}
