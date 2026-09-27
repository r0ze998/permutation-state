//! Crash safety (§8.7): the ledger WAL plus snapshots restore the chain
//! deterministically — the same state hash, slot, game clock, statuses and
//! feed — from a snapshot, from the ledger alone, after a torn write, and
//! after a rewind (`frontier_restore`); a ledger whose re-execution differs
//! from its record is refused; snapshots are taken every N game seconds.

use std::path::{Path, PathBuf};

use fclient::{addr, fees, tx, Address, Instruction, Keypair, Signer};
use localnet::persist::{self, Record};
use localnet::{Chain, Config};

const MEMO: &str = "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr";

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("psf-localnet-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn cfg(d: &Path) -> Config {
    Config {
        scale: 20.0,
        data_dir: Some(d.to_path_buf()),
        fsync: false,
        // 8 game seconds per slot: a snapshot every 5 slots.
        snapshot_every_secs: 40,
        keep_snapshots: 2,
        ..Config::default()
    }
}

fn budget() -> tx::TxBudget {
    tx::TxBudget {
        cu_limit: 200_000,
        cu_price: 1_000,
        loaded_limit: fees::RUNTIME_MAX_LOADED,
        heap: None,
    }
}

fn memo(program: Address, text: &[u8]) -> Instruction {
    Instruction {
        program_id: program,
        accounts: vec![],
        data: text.to_vec(),
    }
}

/// seq, slot, signature, error, units, logs.
type FeedRow = (u64, u64, String, Option<String>, u64, Vec<String>);

/// Everything a restart must reproduce.
#[derive(Debug, PartialEq, Eq)]
struct View {
    slot: u64,
    unix: i64,
    hash: [u8; 32],
    count: u64,
    feed: Vec<FeedRow>,
    statuses: Vec<Option<(u64, Option<String>)>>,
}

fn view(c: &Chain, sigs: &[fclient::ports::Signature]) -> View {
    View {
        slot: c.slot(),
        unix: c.unix_timestamp(),
        hash: c.state_hash(),
        count: c.transaction_count(),
        feed: c
            .feed(0, 100_000, None)
            .into_iter()
            .map(|l| {
                (
                    l.seq,
                    l.slot,
                    l.signature.to_string(),
                    l.err.clone(),
                    l.units,
                    l.logs.clone(),
                )
            })
            .collect(),
        statuses: sigs
            .iter()
            .map(|s| c.status(s).map(|st| (st.slot, st.err)))
            .collect(),
    }
}

/// A little history: airdrops, a LoaderV3 deploy, memo calls on both
/// loaders, a failing transfer, a scale change and many blocks.
fn play(c: &mut Chain, round: u8, sigs: &mut Vec<fclient::ports::Signature>) {
    let payer = Keypair::new_from_array([round; 32]);
    sigs.push(c.airdrop(&payer.pubkey(), 50_000_000_000).unwrap());
    let v3 = Address::new_from_array([0x3A; 32]);
    if c.account(&v3).is_none() {
        let elf = c.account(&addr::address(MEMO)).unwrap().data;
        c.deploy(v3, &elf, 200_000, Some(payer.pubkey())).unwrap();
    }
    for i in 0..12u8 {
        let (bh, _) = c.latest_blockhash();
        let prog = if i % 2 == 0 { addr::address(MEMO) } else { v3 };
        let t = tx::build(
            &[memo(prog, format!("r{round} i{i}").as_bytes())],
            &budget(),
            &[&payer],
            &bh,
        )
        .unwrap();
        sigs.push(c.submit(&tx::wire(&t)).unwrap());
        if i % 5 == 0 {
            // More than the payer has: lands as a failure, fee charged.
            let bad = tx::build(
                &[tx::transfer(
                    payer.pubkey(),
                    Address::new_from_array([i; 32]),
                    u64::MAX / 4,
                )],
                &budget(),
                &[&payer],
                &bh,
            )
            .unwrap();
            sigs.push(c.submit(&tx::wire(&bad)).unwrap());
        }
        if i == 6 {
            c.set_scale(if round.is_multiple_of(2) { 2.0 } else { 20.0 });
        }
        c.produce_block();
    }
}

#[test]
fn restart_from_snapshot_and_ledger_is_identical() {
    let d = dir("restart");
    let mut sigs = vec![];
    let before = {
        let mut c = Chain::open(cfg(&d)).unwrap();
        play(&mut c, 1, &mut sigs);
        play(&mut c, 2, &mut sigs);
        assert!(
            !persist::snapshots_in(&d).is_empty(),
            "periodic snapshots were taken"
        );
        assert!(
            persist::snapshots_in(&d).len() <= 2,
            "only the newest two are kept"
        );
        let v = view(&c, &sigs);
        // A crash: the chain is dropped without any shutdown step.
        std::mem::forget(c);
        v
    };
    let ok = before.feed.iter().filter(|f| f.3.is_none()).count();
    let failed = before.feed.iter().filter(|f| f.3.is_some()).count();
    assert_eq!((ok, failed), (24, 6), "12 memos and 3 failures per round");
    assert!(before.feed.iter().any(|f| !f.5.is_empty()), "logs recorded");

    let mut c = Chain::open(cfg(&d)).unwrap();
    assert_eq!(view(&c, &sigs), before, "snapshot + ledger");
    // Transaction details survive too (getTransaction's fields).
    let l = c.transaction(&sigs[2]).unwrap();
    assert!(!l.pre_balances.is_empty() && l.pre_balances.len() == l.post_balances.len());
    // The chain continues where it stopped: the next slot, same scale.
    let slot = c.slot();
    play(&mut c, 3, &mut sigs);
    assert_eq!(c.slot(), slot + 12);
    let after = view(&c, &sigs);
    drop(c);

    // Ledger only: remove every snapshot and replay from the empty chain.
    for (_, p) in persist::snapshots_in(&d) {
        std::fs::remove_file(p).unwrap();
    }
    let c = Chain::open(cfg(&d)).unwrap();
    assert_eq!(view(&c, &sigs), after, "ledger alone");
    drop(c);

    // A torn last record (a crash mid-write) is cut off; nothing visible
    // was lost because a block is visible only after its record is whole.
    let wal = d.join(persist::WAL_FILE);
    let len = std::fs::metadata(&wal).unwrap().len();
    let mut b = std::fs::read(&wal).unwrap();
    b.extend_from_slice(&[0x40, 0, 0, 0, persist::REC_BLOCK, 1, 2, 3]);
    std::fs::write(&wal, &b).unwrap();
    let c = Chain::open(cfg(&d)).unwrap();
    assert_eq!(view(&c, &sigs), after, "torn tail");
    assert_eq!(std::fs::metadata(&wal).unwrap().len(), len, "tail cut off");
    drop(c);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_ledger_that_replays_differently_is_refused() {
    let d = dir("mismatch");
    let mut sigs = vec![];
    {
        let mut c = Chain::open(Config {
            snapshot_every_secs: 1_000_000,
            ..cfg(&d)
        })
        .unwrap();
        play(&mut c, 4, &mut sigs);
    }
    // Rewrite one recorded outcome (units), with a valid checksum.
    let wal = d.join(persist::WAL_FILE);
    let (h, recs, _) = persist::read_wal(&wal).unwrap();
    let mut out = h.encode();
    let mut changed = false;
    for (_, mut r) in recs {
        if let Record::Block { txs, .. } = &mut r {
            if let Some(t) = txs.first_mut().filter(|_| !changed) {
                t.units += 1;
                changed = true;
            }
        }
        out.extend(r.frame());
    }
    assert!(changed);
    std::fs::write(&wal, out).unwrap();
    let e = Chain::open(cfg(&d)).err().expect("refused");
    assert!(e.contains("replays differently"), "{e}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn rewind_to_a_snapshot_truncates_history_and_ledger() {
    let d = dir("rewind");
    let mut sigs = vec![];
    let mut c = Chain::open(Config {
        snapshot_every_secs: 1_000_000,
        ..cfg(&d)
    })
    .unwrap();
    play(&mut c, 5, &mut sigs);
    let snap = c.snapshot(None).unwrap();
    assert_eq!(snap.state_hash, c.state_hash());
    let at = view(&c, &sigs);
    let n = sigs.len();
    play(&mut c, 6, &mut sigs);
    assert_ne!(c.state_hash(), snap.state_hash);
    let later = sigs[n + 1];
    assert!(c.status(&later).is_some());

    let r = c.restore(&snap.path).unwrap();
    assert_eq!(r.state_hash, snap.state_hash);
    assert_eq!(view(&c, &sigs[..n]), at);
    assert!(c.status(&later).is_none(), "later history is gone");
    // The rewound chain continues and restarts consistently.
    play(&mut c, 7, &mut sigs);
    let v = view(&c, &sigs);
    drop(c);
    let c = Chain::open(cfg(&d)).unwrap();
    assert_eq!(view(&c, &sigs), v);
    // A snapshot of another timeline or run is refused.
    let mut other = Chain::new(Config::default());
    assert!(other.restore(&snap.path).is_err());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn in_memory_snapshots_need_a_path() {
    let mut c = Chain::new(Config::default());
    assert!(c.snapshot(None).is_err());
    let d = dir("mem");
    std::fs::create_dir_all(&d).unwrap();
    let p = d.join("s.bin");
    let s = c.snapshot(Some(&p)).unwrap();
    c.produce_block();
    c.airdrop(&Address::new_from_array([1; 32]), 5).unwrap();
    assert_ne!(c.state_hash(), s.state_hash);
    assert_eq!(c.restore(&p).unwrap().state_hash, s.state_hash);
    let _ = std::fs::remove_dir_all(&d);
}

/// Measurement (W2-C notes): ledger and snapshot sizes and restart times
/// for 20,000 memo transactions over 400 blocks.
/// `cargo test --release -p localnet --test persist -- --ignored --nocapture`
#[test]
#[ignore = "measurement"]
fn measure_ledger_and_restart() {
    use std::time::Instant;
    let d = dir("measure");
    let payers: Vec<Keypair> = (0..50u8)
        .map(|i| Keypair::new_from_array([100 + i; 32]))
        .collect();
    let t0 = Instant::now();
    let mut c = Chain::open(Config {
        snapshot_every_secs: 1_000_000,
        ..cfg(&d)
    })
    .unwrap();
    for p in &payers {
        c.airdrop(&p.pubkey(), 50_000_000_000).unwrap();
    }
    let mut n = 0u64;
    for b in 0..400u32 {
        let (bh, _) = c.latest_blockhash();
        for p in &payers {
            let t = tx::build(
                &[memo(addr::address(MEMO), format!("b{b}").as_bytes())],
                &budget(),
                &[p],
                &bh,
            )
            .unwrap();
            c.submit(&tx::wire(&t)).unwrap();
            n += 1;
        }
        c.produce_block();
    }
    let run = t0.elapsed().as_secs_f64();
    let wal = std::fs::metadata(d.join(persist::WAL_FILE)).unwrap().len();
    let hash = c.state_hash();
    let t1 = Instant::now();
    let s = c.snapshot(None).unwrap();
    let snap_secs = t1.elapsed().as_secs_f64();
    drop(c);
    let t2 = Instant::now();
    let c = Chain::open(cfg(&d)).unwrap();
    let from_snap = t2.elapsed().as_secs_f64();
    assert_eq!(c.state_hash(), hash);
    drop(c);
    for (_, p) in persist::snapshots_in(&d) {
        std::fs::remove_file(p).unwrap();
    }
    let t3 = Instant::now();
    let c = Chain::open(cfg(&d)).unwrap();
    let from_ledger = t3.elapsed().as_secs_f64();
    assert_eq!(c.state_hash(), hash);
    eprintln!(
        "{n} txs in 400 blocks: run {run:.2} s; ledger {wal} B ({:.0} B/tx); snapshot {} B in {snap_secs:.3} s; \
         restart from snapshot {from_snap:.3} s, from the ledger alone (re-executing all) {from_ledger:.2} s",
        wal as f64 / n as f64,
        s.bytes
    );
    let _ = std::fs::remove_dir_all(&d);
}
