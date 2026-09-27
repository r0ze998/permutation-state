//! findex over the local chain node: the LocalnetFeed (in process) and the
//! RpcPoll (JSON-RPC on 127.0.0.1:0) archive the same transactions in the
//! same order; a restart resumes from the archived cursor without
//! duplicates; the index counts failed transactions and records the
//! written accounts' snapshots.

use std::path::PathBuf;
use std::sync::Arc;

use fclient::ports::ChainPort;
use fclient::{tx, Address, Keypair, Signer};
use findex::{Findex, LocalnetFeed, RpcPoll};
use localnet::{server, Config, InProcess};

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "findex-it-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn budget() -> tx::TxBudget {
    tx::TxBudget {
        cu_limit: 10_000,
        cu_price: 0,
        loaded_limit: 1_048_576,
        heap: None,
    }
}

/// `n` transfers from `payer` to `to` (every one mentions `to`); the odd
/// ones overdraw and fail with the fee charged.
async fn transfers(ip: &InProcess, payer: &Keypair, to: Address, n: u64, base: u64) {
    for i in 0..n {
        let (bh, _) = ip.blockhash().await.unwrap();
        let lamports = if i % 2 == 1 {
            1_000_000_000_000
        } else {
            base + i
        };
        let t = tx::build(
            &[tx::transfer(payer.pubkey(), to, lamports)],
            &budget(),
            &[payer],
            &bh,
        )
        .unwrap();
        ip.send(&tx::wire(&t)).await.unwrap();
        ip.step(1);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn feed_and_rpc_poll_archive_the_same_transactions_and_resume() {
    let ip = InProcess::new(Config::default(), None);
    let payer = Keypair::new_from_array([41; 32]);
    let to = Address::new_from_array([42; 32]);
    ip.lock().airdrop(&payer.pubkey(), 50_000_000_000).unwrap();
    // The feed is filtered by `to` so both sources see the same set.
    let feed_port = InProcess::from_chain(ip.chain.clone(), Some(to));
    transfers(&ip, &payer, to, 6, 1_000_000).await;

    let node = server::start(ip.chain.clone(), 0).await.unwrap();
    let url = node.url();

    let d1 = tmp("feed");
    let d2 = tmp("rpc");
    let system = fclient::addr::system_program();
    let mut f1 = Findex::open(&d1, system, None, 2_000).unwrap();
    let mut f2 = Findex::open(&d2, system, None, 2_000).unwrap();
    let mut feed = LocalnetFeed::new(feed_port.clone());
    let mut poll = RpcPoll::new(url.clone(), to);

    let a = f1.ingest(&mut feed).await.unwrap();
    let b = f2.ingest(&mut poll).await.unwrap();
    assert_eq!(a.len(), 6);
    let sigs = |v: &[fclient::ports::TxRecord]| v.iter().map(|r| r.signature).collect::<Vec<_>>();
    assert_eq!(sigs(&a), sigs(&b), "same transactions, same order");
    assert_eq!(a.iter().filter(|r| r.err.is_some()).count(), 3);
    assert_eq!(f1.index.tx_count().unwrap(), (6, 3));
    assert_eq!(f2.index.tx_count().unwrap(), (6, 3));
    assert!(
        a.iter().all(|r| !r.post.is_empty()),
        "the feed carries post-state"
    );
    assert!(
        b.iter().all(|r| r.post.is_empty()),
        "a JSON-RPC poll has none"
    );

    // Nothing new: nothing archived.
    assert!(f1.ingest(&mut feed).await.unwrap().is_empty());
    assert!(f2.ingest(&mut poll).await.unwrap().is_empty());

    // Restart both from their archives, then three more transactions.
    drop(f1);
    drop(f2);
    let mut f1 = Findex::open(&d1, system, None, 2_000).unwrap();
    let mut f2 = Findex::open(&d2, system, None, 2_000).unwrap();
    let mut feed = LocalnetFeed::new(feed_port.clone());
    let mut poll = RpcPoll::new(url, to);
    f1.resume(&mut feed);
    f2.resume(&mut poll);
    transfers(&ip, &payer, to, 3, 2_000_000).await;
    let a2 = f1.ingest(&mut feed).await.unwrap();
    let b2 = f2.ingest(&mut poll).await.unwrap();
    assert_eq!(a2.len(), 3, "only the new ones");
    assert_eq!(sigs(&a2), sigs(&b2));
    assert_eq!(a2[0].seq, 7, "archive sequence continues");
    assert_eq!(f1.archive.verify(), Ok(9));
    assert_eq!(f2.archive.verify(), Ok(9));
    assert!(
        f1.archive.manifest.segments.len() > 1,
        "2-KB segments rolled"
    );
    assert_eq!(f1.index.last_seq().unwrap(), 9);

    // The index rebuilds from the archive alone.
    std::fs::remove_file(d1.join("index.sqlite")).unwrap();
    let _ = std::fs::remove_file(d1.join("index.sqlite-wal"));
    let _ = std::fs::remove_file(d1.join("index.sqlite-shm"));
    let f1b = Findex::open(&d1, system, None, 2_000).unwrap();
    assert_eq!(f1b.index.tx_count().unwrap(), (9, 4));

    node.stop();
    let _ = Arc::strong_count(&ip.chain);
    let _ = std::fs::remove_dir_all(&d1);
    let _ = std::fs::remove_dir_all(&d2);
}

// ---------------------------------------------------------------- paging

mod fake_rpc {
    //! A JSON-RPC server that honours `before`, `until` and `limit` (as a
    //! public RPC does) over a fixed list of transactions; with
    //! `ignore_before` set it serves the newest page whatever `before` says
    //! (the W1 local node MVP's behaviour).
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use axum::{extract::State, routing::post, Json, Router};
    use base64::Engine;
    use serde_json::{json, Value};

    #[derive(Clone, Default)]
    pub struct Txs(pub Arc<Mutex<Vec<(String, u64)>>>, pub Arc<AtomicBool>);

    impl Txs {
        pub fn ignore_before(&self) {
            self.1.store(true, Ordering::SeqCst);
        }
    }

    async fn rpc(State(s): State<Txs>, Json(req): Json<Value>) -> Json<Value> {
        let id = req.get("id").cloned().unwrap_or(json!(1));
        let p = req.get("params").cloned().unwrap_or(json!([]));
        let all = s.0.lock().unwrap().clone();
        let result = match req.get("method").and_then(|m| m.as_str()).unwrap_or("") {
            "getSignaturesForAddress" => {
                let cfg = p.get(1).cloned().unwrap_or(json!({}));
                let limit = cfg.get("limit").and_then(|x| x.as_u64()).unwrap_or(1000) as usize;
                let before = cfg
                    .get("before")
                    .and_then(|x| x.as_str())
                    .filter(|_| !s.1.load(Ordering::SeqCst));
                let until = cfg.get("until").and_then(|x| x.as_str());
                let mut newest: Vec<&(String, u64)> = all.iter().rev().collect();
                if let Some(b) = before {
                    let i = newest
                        .iter()
                        .position(|(s, _)| s == b)
                        .map_or(newest.len(), |i| i + 1);
                    newest = newest.split_off(i);
                }
                if let Some(u) = until {
                    if let Some(i) = newest.iter().position(|(s, _)| s == u) {
                        newest.truncate(i);
                    }
                }
                json!(newest.iter().take(limit).map(|(s, slot)| json!({"signature": s, "slot": slot, "err": null, "blockTime": 0})).collect::<Vec<_>>())
            }
            "getTransaction" => {
                let sig = p.get(0).and_then(|x| x.as_str()).unwrap_or("");
                match all.iter().find(|(s, _)| s == sig) {
                    None => Value::Null,
                    Some((_, slot)) => json!({"slot": slot, "blockTime": 7,
                        "transaction": [base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3]), "base64"],
                        "meta": {"err": null, "fee": 5000, "logMessages": [], "computeUnitsConsumed": 9}}),
                }
            }
            _ => Value::Null,
        };
        Json(json!({"jsonrpc": "2.0", "id": id, "result": result}))
    }

    pub async fn start(txs: Txs) -> String {
        let l = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = l.local_addr().unwrap();
        let app = Router::new().route("/", post(rpc)).with_state(txs);
        tokio::spawn(async move {
            let _ = axum::serve(l, app).await;
        });
        format!("http://{addr}")
    }
}

#[tokio::test]
async fn rpc_poll_pages_back_to_the_last_signature() {
    let txs = fake_rpc::Txs::default();
    let sig = |i: u8| fclient::ports::Signature::from([i; 64]).to_string();
    for i in 1..=11u8 {
        txs.0.lock().unwrap().push((sig(i), i as u64));
    }
    let url = fake_rpc::start(txs.clone()).await;
    let mut poll = RpcPoll::new(url.clone(), Address::new_from_array([1; 32]));
    poll.page = 3;
    let got = findex::Source::pull(&mut poll).await.unwrap();
    assert_eq!(got.len(), 11, "four pages of 3");
    assert_eq!(
        got.iter().map(|r| r.slot).collect::<Vec<_>>(),
        (1..=11).collect::<Vec<u64>>(),
        "oldest first"
    );
    for i in 12..=18u8 {
        txs.0.lock().unwrap().push((sig(i), i as u64));
    }
    let more = findex::Source::pull(&mut poll).await.unwrap();
    assert_eq!(
        more.iter().map(|r| r.slot).collect::<Vec<_>>(),
        (12..=18).collect::<Vec<u64>>()
    );
    // A restored poller resumes after the stored signature.
    let cur = findex::Source::cursor(&poll);
    let mut again = RpcPoll::new(url, Address::new_from_array([1; 32]));
    findex::Source::restore(&mut again, &cur);
    assert!(findex::Source::pull(&mut again).await.unwrap().is_empty());
}

/// A server that ignores `before`: paging is refused, not gapped.
#[tokio::test]
async fn rpc_poll_refuses_a_server_that_ignores_before() {
    let txs = fake_rpc::Txs::default();
    let sig = |i: u8| fclient::ports::Signature::from([i; 64]).to_string();
    for i in 1..=5u8 {
        txs.0.lock().unwrap().push((sig(i), i as u64));
    }
    txs.ignore_before();
    let url = fake_rpc::start(txs).await;
    let mut poll = RpcPoll::new(url, Address::new_from_array([1; 32]));
    poll.page = 2;
    let e = findex::Source::pull(&mut poll).await.unwrap_err();
    assert!(
        matches!(e, fclient::ports::PortError::Unsupported(_)),
        "{e:?}"
    );
}

/// The local chain node honours `before` since W2-C: RpcPoll pages it in
/// pages smaller than the history and archives every transaction, oldest
/// first, the same set the in-process feed gives.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rpc_poll_pages_the_local_node() {
    let ip = InProcess::new(Config::default(), None);
    let payer = Keypair::new_from_array([43; 32]);
    let to = Address::new_from_array([44; 32]);
    ip.lock().airdrop(&payer.pubkey(), 50_000_000_000).unwrap();
    let feed_port = InProcess::from_chain(ip.chain.clone(), Some(to));
    transfers(&ip, &payer, to, 5, 1_000_000).await;
    let node = server::start(ip.chain.clone(), 0).await.unwrap();
    let mut poll = RpcPoll::new(node.url(), to);
    poll.page = 2;
    let got = findex::Source::pull(&mut poll).await.unwrap();
    let mut feed = LocalnetFeed::new(feed_port);
    let want = findex::Source::pull(&mut feed).await.unwrap();
    assert_eq!(got.len(), 5, "three pages of 2");
    let sigs = |v: &[fclient::ports::TxRecord]| v.iter().map(|r| r.signature).collect::<Vec<_>>();
    assert_eq!(sigs(&got), sigs(&want), "same transactions, oldest first");
    assert!(findex::Source::pull(&mut poll).await.unwrap().is_empty());
    node.stop();
}
