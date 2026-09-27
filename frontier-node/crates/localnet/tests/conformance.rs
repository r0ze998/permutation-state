//! RPC conformance (§8.7, W2-C): the response shapes the clients rely on,
//! checked here directly (error JSON with the real instruction index,
//! `getTransaction` in both encodings, paging, `dataSlice`, `withContext`,
//! the WebSocket handshake and notifications) and then through the clients
//! themselves: `tests/web3/conformance.mjs` drives `@solana/web3.js` 1.99,
//! `permutation-gateway/src/send.mjs` and `tickscan.mjs` against a live node.
//!
//! The Node half needs `node` and `permutation-gateway/node_modules`
//! (`npm ci` there). Without them it prints `NOT RUN` and passes, unless
//! `PSF_REQUIRE_WEB3=1`, which makes their absence a failure (the gate sets
//! it after `npm ci`; see W2-C notes).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use fclient::ports::{Account, ChainPort};
use fclient::rpc::RpcPort;
use fclient::{addr, tx, Address, Keypair, Signer};
use localnet::server::{self, bs58_encode};
use localnet::{Chain, Config};

fn funded(c: &mut Chain, n: u8) -> Keypair {
    let k = Keypair::new_from_array([n; 32]);
    c.airdrop(&k.pubkey(), 10_000_000_000).unwrap();
    k
}

fn budget() -> tx::TxBudget {
    tx::TxBudget {
        cu_limit: 60_000,
        cu_price: 0,
        loaded_limit: 65_536,
        heap: None,
    }
}

async fn call(port: &RpcPort, m: &str, p: Value) -> Value {
    port.rpc.call(m, p).await.unwrap()
}

async fn land(port: &RpcPort, sig: fclient::ports::Signature) -> fclient::ports::Status {
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Some(Some(s)) = port.statuses(&[sig]).await.unwrap().into_iter().next() {
            return s;
        }
    }
    panic!("{sig} did not land");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn response_shapes_the_clients_rely_on() {
    let mut chain = Chain::new(Config {
        scale: 20.0,
        ..Config::default()
    });
    let payer = funded(&mut chain, 31);
    let air = chain
        .airdrop(&Address::new_from_array([0x61; 32]), 1_000_000)
        .unwrap();
    let node = server::start(Arc::new(Mutex::new(chain)), 0).await.unwrap();
    let port = RpcPort::localnet(node.url(), addr::system_program());
    let dest = Address::new_from_array([0x62; 32]);

    // A failing transfer after three ComputeBudget instructions: the error
    // names instruction 3, as Agave's JSON does.
    let (bh, _) = port.blockhash().await.unwrap();
    let bad = tx::build(
        &[tx::transfer(payer.pubkey(), dest, u64::MAX / 4)],
        &budget(),
        &[&payer],
        &bh,
    )
    .unwrap();
    let pre = port
        .rpc
        .call(
            "sendTransaction",
            json!([base64_of(&tx::wire(&bad)), {"encoding": "base64"}]),
        )
        .await
        .unwrap_err();
    let pre = format!("{pre:?}");
    assert!(pre.contains("-32002"), "{pre}");
    let sig_bad = call(
        &port,
        "sendTransaction",
        json!([base64_of(&tx::wire(&bad)), {"encoding": "base64", "skipPreflight": true}]),
    )
    .await;
    let good = tx::build(
        &[tx::transfer(payer.pubkey(), dest, 2_000_000)],
        &budget(),
        &[&payer],
        &bh,
    )
    .unwrap();
    // base58 wire (sendTransaction's default encoding).
    let sig_good = call(
        &port,
        "sendTransaction",
        json!([bs58_encode(&tx::wire(&good))]),
    )
    .await;
    let s_bad: fclient::ports::Signature = sig_bad.as_str().unwrap().parse().unwrap();
    let s_good: fclient::ports::Signature = sig_good.as_str().unwrap().parse().unwrap();
    land(&port, s_bad).await;
    let st = land(&port, s_good).await;

    let sts = call(
        &port,
        "getSignatureStatuses",
        json!([[sig_bad, sig_good, bs58_encode(&[1; 64])]]),
    )
    .await;
    let v = &sts["value"];
    assert_eq!(v[0]["err"], json!({"InstructionError": [3, {"Custom": 1}]}));
    assert_eq!(
        v[0]["status"],
        json!({"Err": {"InstructionError": [3, {"Custom": 1}]}})
    );
    assert_eq!(v[1]["err"], Value::Null);
    assert_eq!(v[1]["confirmationStatus"], "finalized");
    assert_eq!(v[2], Value::Null);

    // getTransaction: json (the default) and base64.
    let t = call(
        &port,
        "getTransaction",
        json!([sig_good, {"maxSupportedTransactionVersion": 0}]),
    )
    .await;
    assert_eq!(t["transaction"]["signatures"][0], sig_good);
    let msg = &t["transaction"]["message"];
    assert_eq!(msg["header"]["numRequiredSignatures"], 1);
    assert_eq!(msg["accountKeys"][0], payer.pubkey().to_string());
    assert_eq!(msg["instructions"].as_array().unwrap().len(), 4);
    assert_eq!(msg["recentBlockhash"], bh.to_string());
    let meta = &t["meta"];
    assert_eq!(meta["err"], Value::Null);
    let pre: Vec<u64> = serde_json::from_value(meta["preBalances"].clone()).unwrap();
    let post: Vec<u64> = serde_json::from_value(meta["postBalances"].clone()).unwrap();
    assert_eq!(pre.len(), msg["accountKeys"].as_array().unwrap().len());
    assert_eq!(pre[0] - post[0], 2_000_000 + meta["fee"].as_u64().unwrap());
    assert_eq!(t["slot"].as_u64(), Some(st.slot));
    let t64 = call(
        &port,
        "getTransaction",
        json!([sig_good, {"encoding": "base64"}]),
    )
    .await;
    assert_eq!(t64["transaction"][1], "base64");
    assert_eq!(t64["transaction"][0], base64_of(&tx::wire(&good)));
    let tb = call(&port, "getTransaction", json!([sig_bad])).await;
    assert_eq!(
        tb["meta"]["err"],
        json!({"InstructionError": [3, {"Custom": 1}]})
    );
    assert!(!tb["meta"]["logMessages"].as_array().unwrap().is_empty());
    // An airdrop is served as the faucet's (synthetic, signed) transfer.
    let ta = call(&port, "getTransaction", json!([air.to_string()])).await;
    assert_eq!(ta["transaction"]["signatures"][0], air.to_string());
    assert_eq!(
        ta["transaction"]["message"]["accountKeys"][0],
        fclient::Signer::pubkey(&localnet::chain::faucet()).to_string()
    );
    assert_eq!(ta["meta"]["postBalances"][1], 1_000_000);

    // getSignaturesForAddress: newest first, before/until, the airdrop too.
    let all = call(
        &port,
        "getSignaturesForAddress",
        json!([payer.pubkey().to_string()]),
    )
    .await;
    let all: Vec<String> = all
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["signature"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(all.len(), 3, "airdrop, bad, good");
    let older = call(
        &port,
        "getSignaturesForAddress",
        json!([payer.pubkey().to_string(), {"before": all[0], "limit": 1}]),
    )
    .await;
    assert_eq!(older[0]["signature"], all[1]);
    let until = call(
        &port,
        "getSignaturesForAddress",
        json!([payer.pubkey().to_string(), {"until": all[2]}]),
    )
    .await;
    assert_eq!(until.as_array().unwrap().len(), 2);
    assert!(port
        .rpc
        .call(
            "getSignaturesForAddress",
            json!([payer.pubkey().to_string(), {"limit": 0}])
        )
        .await
        .is_err());

    // Accounts: base64 by default, base58 on request, dataSlice.
    let a = call(
        &port,
        "getAccountInfo",
        json!([dest.to_string(), {"encoding": "base64", "dataSlice": {"offset": 0, "length": 0}}]),
    )
    .await;
    assert_eq!(a["value"]["lamports"], 2_000_000);
    assert_eq!(a["value"]["data"], json!(["", "base64"]));
    let m = call(&port, "getMultipleAccounts", json!([[dest.to_string(), Address::new_from_array([0x63; 32]).to_string()], {"encoding": "base58"}])).await;
    assert_eq!(m["value"][0]["data"][1], "base58");
    assert_eq!(m["value"][1], Value::Null);
    assert!(
        call(&port, "isBlockhashValid", json!([bh.to_string()])).await["value"]
            .as_bool()
            .unwrap()
    );
    assert!(!call(
        &port,
        "isBlockhashValid",
        json!([fclient::Hash::new_from_array([9; 32]).to_string()])
    )
    .await["value"]
        .as_bool()
        .unwrap());
    let g = call(&port, "getGenesisHash", json!([])).await;
    assert!(g.as_str().unwrap().len() > 30);

    // simulateTransaction: post-state accounts and the error JSON.
    let (bh2, _) = port.blockhash().await.unwrap();
    let sim_bad = tx::build(
        &[tx::transfer(payer.pubkey(), dest, u64::MAX / 4)],
        &budget(),
        &[&payer],
        &bh2,
    )
    .unwrap();
    let s = call(
        &port,
        "simulateTransaction",
        json!([base64_of(&tx::wire(&sim_bad)), {"encoding": "base64", "sigVerify": true,
        "accounts": {"encoding": "base64", "addresses": [dest.to_string()]}}]),
    )
    .await;
    assert_eq!(
        s["value"]["err"],
        json!({"InstructionError": [3, {"Custom": 1}]})
    );
    assert_eq!(s["value"]["accounts"][0]["lamports"], 2_000_000);

    // Extensions: status, stateHash, snapshot needs a path without a data dir.
    let fs = call(&port, "frontier_status", json!([])).await;
    assert_eq!(fs["holds"], 0);
    assert_eq!(fs["runId"].as_str().unwrap().len(), 32);
    assert_eq!(
        call(&port, "frontier_stateHash", json!([])).await["stateHash"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert!(port.rpc.call("frontier_snapshot", json!([])).await.is_err());
    let at = call(&port, "frontier_setScale", json!([2.0])).await;
    assert!(at["atSlot"].as_u64().unwrap() > st.slot);
    node.stop();
}

fn base64_of(b: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(b)
}

/// A minimal WebSocket client: handshake, masked text frames out, text
/// frames in.
struct Ws(TcpStream);

impl Ws {
    async fn connect(addr: std::net::SocketAddr) -> Ws {
        let mut s = TcpStream::connect(addr).await.unwrap();
        s.write_all(
            format!(
                "GET / HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
                 Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
            )
            .as_bytes(),
        )
        .await
        .unwrap();
        let mut head = vec![];
        let mut b = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            s.read_exact(&mut b).await.unwrap();
            head.push(b[0]);
        }
        let head = String::from_utf8(head).unwrap();
        assert!(head.starts_with("HTTP/1.1 101"), "{head}");
        assert!(head.contains("s3pPLMBiTxaQ9kYGzzhZRbK+xOo="), "{head}");
        Ws(s)
    }

    async fn send(&mut self, v: Value) {
        let p = v.to_string().into_bytes();
        let mask = [1u8, 2, 3, 4];
        let mut f = vec![0x81u8];
        if p.len() < 126 {
            f.push(0x80 | p.len() as u8);
        } else {
            f.push(0x80 | 126);
            f.extend_from_slice(&(p.len() as u16).to_be_bytes());
        }
        f.extend_from_slice(&mask);
        f.extend(p.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        self.0.write_all(&f).await.unwrap();
    }

    async fn recv(&mut self) -> Value {
        let mut h = [0u8; 2];
        tokio::time::timeout(Duration::from_secs(10), self.0.read_exact(&mut h))
            .await
            .expect("a frame within 10 s")
            .unwrap();
        assert_eq!(h[1] & 0x80, 0, "server frames are not masked");
        let mut len = (h[1] & 0x7F) as usize;
        if len == 126 {
            let mut b = [0u8; 2];
            self.0.read_exact(&mut b).await.unwrap();
            len = u16::from_be_bytes(b) as usize;
        } else if len == 127 {
            let mut b = [0u8; 8];
            self.0.read_exact(&mut b).await.unwrap();
            len = u64::from_be_bytes(b) as usize;
        }
        let mut p = vec![0u8; len];
        self.0.read_exact(&mut p).await.unwrap();
        assert_eq!(h[0] & 0x0F, 1, "text frame");
        serde_json::from_slice(&p).unwrap()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_subscriptions_follow_agave_shapes() {
    let mut chain = Chain::new(Config {
        scale: 20.0,
        ..Config::default()
    });
    let payer = funded(&mut chain, 41);
    let node = server::start(Arc::new(Mutex::new(chain)), 0).await.unwrap();
    let port = RpcPort::localnet(node.url(), addr::system_program());
    let mut ws = Ws::connect(node.ws_addr).await;

    ws.send(json!({"jsonrpc": "2.0", "id": 1, "method": "slotSubscribe"}))
        .await;
    let r = ws.recv().await;
    assert_eq!(r["id"], 1);
    let slot_sub = r["result"].as_u64().unwrap();
    let n = ws.recv().await;
    assert_eq!(n["method"], "slotNotification");
    assert_eq!(n["params"]["subscription"], slot_sub);
    let s = n["params"]["result"]["slot"].as_u64().unwrap();
    assert_eq!(n["params"]["result"]["parent"].as_u64(), Some(s - 1));
    ws.send(json!({"jsonrpc": "2.0", "id": 2, "method": "slotUnsubscribe", "params": [slot_sub]}))
        .await;
    // Drain notifications sent before the unsubscribe.
    loop {
        let r = ws.recv().await;
        if r["id"] == 2 {
            assert_eq!(r["result"], true);
            break;
        }
    }
    // web3.js's heartbeat is a notification: no answer.
    ws.send(json!({"jsonrpc": "2.0", "method": "ping", "params": null}))
        .await;

    let (bh, _) = port.blockhash().await.unwrap();
    let t = tx::build(
        &[tx::transfer(
            payer.pubkey(),
            Address::new_from_array([0x64; 32]),
            1_000_000,
        )],
        &budget(),
        &[&payer],
        &bh,
    )
    .unwrap();
    let sig = tx::signature(&t);
    ws.send(json!({"jsonrpc": "2.0", "id": 3, "method": "logsSubscribe", "params": [{"mentions": [payer.pubkey().to_string()]}, {"commitment": "confirmed"}]})).await;
    let logs_sub = ws.recv().await["result"].as_u64().unwrap();
    ws.send(json!({"jsonrpc": "2.0", "id": 4, "method": "signatureSubscribe", "params": [sig.to_string(), {"commitment": "confirmed"}]})).await;
    let sig_sub = ws.recv().await["result"].as_u64().unwrap();
    port.send(&tx::wire(&t)).await.unwrap();
    let mut seen = (false, false);
    while seen != (true, true) {
        let n = ws.recv().await;
        match n["method"].as_str().unwrap() {
            "logsNotification" => {
                assert_eq!(n["params"]["subscription"], logs_sub);
                assert_eq!(n["params"]["result"]["value"]["signature"], sig.to_string());
                assert_eq!(n["params"]["result"]["value"]["err"], Value::Null);
                seen.0 = true;
            }
            "signatureNotification" => {
                assert_eq!(n["params"]["subscription"], sig_sub);
                assert_eq!(n["params"]["result"]["value"], json!({"err": null}));
                assert!(n["params"]["result"]["context"]["slot"].as_u64().unwrap() > 0);
                seen.1 = true;
            }
            m => panic!("unexpected {m}"),
        }
    }
    // A signature that already landed is notified at once.
    ws.send(json!({"jsonrpc": "2.0", "id": 5, "method": "signatureSubscribe", "params": [sig.to_string()]})).await;
    assert!(ws.recv().await["result"].as_u64().is_some());
    assert_eq!(ws.recv().await["method"], "signatureNotification");
    // Errors: unknown method, bad filter, unknown subscription.
    ws.send(json!({"jsonrpc": "2.0", "id": 6, "method": "nope"}))
        .await;
    assert_eq!(ws.recv().await["error"]["code"], -32601);
    ws.send(json!({"jsonrpc": "2.0", "id": 7, "method": "logsUnsubscribe", "params": [999]}))
        .await;
    loop {
        let r = ws.recv().await;
        if r["id"] == 7 {
            assert_eq!(r["error"]["code"], -32602);
            break;
        }
    }
    // A plain HTTP GET on the WebSocket port is refused.
    let mut s = TcpStream::connect(node.ws_addr).await.unwrap();
    s.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
        .await
        .unwrap();
    let mut b = vec![];
    s.read_to_end(&mut b).await.unwrap();
    assert!(String::from_utf8_lossy(&b).starts_with("HTTP/1.1 400"));
    node.stop();
}

fn gateway_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../permutation-gateway")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn web3_js_and_send_mjs_against_a_live_node() {
    let gw = gateway_dir();
    let have_node = std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    let have_web3 = gw
        .join("node_modules/@solana/web3.js/package.json")
        .exists();
    if !(have_node && have_web3) {
        let why = format!(
            "web3.js conformance NOT RUN: node {have_node}, {}/node_modules/@solana/web3.js {have_web3} (run `npm ci` in permutation-gateway)",
            gw.display()
        );
        if std::env::var("PSF_REQUIRE_WEB3").is_ok_and(|v| v == "1") {
            panic!("{why}");
        }
        eprintln!("{why}");
        return;
    }
    let mut chain = Chain::new(Config {
        scale: 20.0,
        allow_tamper: true,
        ..Config::default()
    });
    let program = Address::new_from_array([0x71; 32]);
    let owned = Address::new_from_array([0x72; 32]);
    let mut data = b"PSF1".to_vec();
    data.push(7);
    data.resize(40, 0);
    chain
        .set_account(
            owned,
            Account {
                lamports: 1_000_000,
                data,
                owner: program,
                executable: false,
            },
        )
        .unwrap();
    let node = server::start(Arc::new(Mutex::new(chain)), 0).await.unwrap();
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/web3/conformance.mjs");
    let args = vec![
        script.display().to_string(),
        node.url(),
        node.ws_url(),
        gw.canonicalize().unwrap().display().to_string(),
        program.to_string(),
        owned.to_string(),
        bs58_encode(b"PSF1"),
        bs58_encode(b"XXXX"),
    ];
    let out = tokio::time::timeout(
        Duration::from_secs(120),
        tokio::task::spawn_blocking(move || std::process::Command::new("node").args(args).output()),
    )
    .await
    .expect("the script finishes within 120 s")
    .unwrap()
    .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stdout:\n{stdout}\nstderr:\n{stderr}");
    let r: Value = serde_json::from_str(stdout.lines().last().unwrap()).unwrap();
    assert_eq!(r["ok"], true);
    let n = r["checks"].as_array().unwrap().len();
    assert_eq!(n, 13, "{r}");
    eprintln!("web3.js conformance: {n} checks passed");
    node.stop();
}
