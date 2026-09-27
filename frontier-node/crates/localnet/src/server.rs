//! JSON-RPC over HTTP (`POST /`) with the subset `@solana/web3.js` 1.99,
//! `send.mjs` and `fclient` use (§8.7), the loopback `frontier_*`
//! extensions of the MVP, and the 400-ms slot ticker.
//!
//! MVP scope (W1-F): `getLatestBlockhash, sendTransaction, simulateTransaction,
//! getAccountInfo, getMultipleAccounts, getSignatureStatuses, getTransaction,
//! getSignaturesForAddress, getSlot, getBlockHeight, getBlockTime, getBalance,
//! getMinimumBalanceForRentExemption, getProgramAccounts (memcmp, dataSize),
//! requestAirdrop, getEpochInfo, getVersion, getHealth` and
//! `frontier_feed, frontier_pause, frontier_resume, frontier_setScale,
//! frontier_status, frontier_setAccount (--allow-tamper only)`. W2-C adds
//! `frontier_snapshot/restore/hold`, the WAL and the WS subscriptions; until
//! then they answer `-32601` with a note.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{extract::State, routing::post, Json, Router};
use base64::Engine;
use serde_json::{json, Value};
use solana_address::Address;
use solana_signature::Signature;

use fclient::ports::Account;
use fclient::rpc::account_json;

use crate::chain::{Chain, SendError, SLOT_MS};

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

pub type Shared = Arc<Mutex<Chain>>;

fn lock(s: &Shared) -> std::sync::MutexGuard<'_, Chain> {
    s.lock().unwrap_or_else(|p| p.into_inner())
}

/// Ports the M1 stack must never bind (§10.3).
pub const RESERVED: [u16; 11] = [
    4185, 4190, 4191, 4194, 18899, 17799, 28899, 27799, 26699, 5185, 5191,
];

/// A service port is 0 (tests) or in 41000–41999 and not reserved.
pub fn check_port(p: u16) -> Result<(), String> {
    if p == 0 {
        return Ok(());
    }
    if RESERVED.contains(&p) {
        return Err(format!("port {p} is reserved"));
    }
    if !(41_000..=41_999).contains(&p) {
        return Err(format!("port {p} is outside 41000-41999"));
    }
    if fclient::ports::port_in_use(p) {
        return Err(format!("port {p} is busy (a listener on some address)"));
    }
    Ok(())
}

#[derive(Debug)]
struct RpcErr {
    code: i64,
    message: String,
    data: Option<Value>,
}

fn err(code: i64, message: impl Into<String>) -> RpcErr {
    RpcErr {
        code,
        message: message.into(),
        data: None,
    }
}

fn invalid(m: impl Into<String>) -> RpcErr {
    err(-32602, m)
}

fn key(v: &Value) -> Result<Address, RpcErr> {
    v.as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| invalid("invalid pubkey"))
}

fn ctx(c: &Chain, value: Value) -> Value {
    json!({"context": {"slot": c.slot(), "apiVersion": "3.1.9"}, "value": value})
}

fn tx_bytes(p: &Value) -> Result<Vec<u8>, RpcErr> {
    let s = p
        .get(0)
        .and_then(|x| x.as_str())
        .ok_or_else(|| invalid("missing transaction"))?;
    let enc = p
        .get(1)
        .and_then(|c| c.get("encoding"))
        .and_then(|e| e.as_str())
        .unwrap_or("base58");
    match enc {
        "base64" => B64.decode(s).map_err(|e| invalid(e.to_string())),
        "base58" => bs58_decode(s).ok_or_else(|| invalid("bad base58")),
        e => Err(invalid(format!("unsupported encoding {e}"))),
    }
}

/// base58 decode (Bitcoin alphabet) without an extra dependency.
fn bs58_decode(s: &str) -> Option<Vec<u8>> {
    const A: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let mut out: Vec<u8> = vec![];
    for ch in s.bytes() {
        let mut carry = A.iter().position(|&c| c == ch)? as u32;
        for b in out.iter_mut().rev() {
            carry += (*b as u32) * 58;
            *b = (carry & 0xFF) as u8;
            carry >>= 8;
        }
        while carry > 0 {
            out.insert(0, (carry & 0xFF) as u8);
            carry >>= 8;
        }
    }
    let zeros = s.bytes().take_while(|&c| c == b'1').count();
    let mut v = vec![0u8; zeros];
    v.extend(out);
    Some(v)
}

fn err_json(e: &Option<String>, code: Option<u32>) -> Value {
    match (e, code) {
        (None, _) => Value::Null,
        (Some(_), Some(c)) => json!({"InstructionError": [0, {"Custom": c}]}),
        (Some(s), None) => {
            let name = s.split(['(', ' ', '{']).next().unwrap_or(s).to_string();
            json!(name)
        }
    }
}

fn accounts_json(c: &Chain, keys: &[Address]) -> Value {
    Value::Array(
        keys.iter()
            .map(|k| {
                c.account(k)
                    .map(|a| account_json(&a))
                    .unwrap_or(Value::Null)
            })
            .collect(),
    )
}

fn handle(state: &Shared, method: &str, p: &Value) -> Result<Value, RpcErr> {
    match method {
        "getHealth" => Ok(json!("ok")),
        "getVersion" => Ok(
            json!({"solana-core": "3.1.9", "feature-set": 0, "frontier-localnet": env!("CARGO_PKG_VERSION"), "litesvm": "0.16.0"}),
        ),
        "getSlot" | "getBlockHeight" => Ok(json!(lock(state).slot())),
        "getBlockTime" => {
            let s = p
                .get(0)
                .and_then(|x| x.as_u64())
                .ok_or_else(|| invalid("slot"))?;
            lock(state)
                .block_time(s)
                .map(|t| json!(t))
                .ok_or_else(|| err(-32004, format!("Block not available for slot {s}")))
        }
        "getEpochInfo" => {
            let c = lock(state);
            Ok(
                json!({"absoluteSlot": c.slot(), "blockHeight": c.slot(), "epoch": 0, "slotIndex": c.slot(), "slotsInEpoch": 432_000,
                "transactionCount": c.transaction_count()}),
            )
        }
        "getLatestBlockhash" => {
            let c = lock(state);
            let (h, lv) = c.latest_blockhash();
            Ok(ctx(
                &c,
                json!({"blockhash": h.to_string(), "lastValidBlockHeight": lv}),
            ))
        }
        "getMinimumBalanceForRentExemption" => {
            let n = p
                .get(0)
                .and_then(|x| x.as_u64())
                .ok_or_else(|| invalid("size"))?;
            Ok(json!(lock(state)
                .svm
                .minimum_balance_for_rent_exemption(n as usize)))
        }
        "getBalance" => {
            let k = key(p.get(0).unwrap_or(&Value::Null))?;
            let c = lock(state);
            Ok(ctx(&c, json!(c.balance(&k))))
        }
        "getAccountInfo" => {
            let k = key(p.get(0).unwrap_or(&Value::Null))?;
            let c = lock(state);
            Ok(ctx(
                &c,
                c.account(&k)
                    .map(|a| account_json(&a))
                    .unwrap_or(Value::Null),
            ))
        }
        "getMultipleAccounts" => {
            let ks: Vec<Address> = p
                .get(0)
                .and_then(|x| x.as_array())
                .ok_or_else(|| invalid("keys"))?
                .iter()
                .map(key)
                .collect::<Result<_, _>>()?;
            let c = lock(state);
            if let Some(m) = p
                .get(1)
                .and_then(|x| x.get("minContextSlot"))
                .and_then(|x| x.as_u64())
            {
                if c.slot() < m {
                    return Err(err(-32016, "Minimum context slot has not been reached"));
                }
            }
            Ok(ctx(&c, accounts_json(&c, &ks)))
        }
        "getProgramAccounts" => {
            let prog = key(p.get(0).unwrap_or(&Value::Null))?;
            let filters = p
                .get(1)
                .and_then(|x| x.get("filters"))
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default();
            let c = lock(state);
            let rows: Vec<Value> = c
                .program_accounts(&prog)
                .into_iter()
                .filter(|(_, a)| {
                    filters.iter().all(|f| {
                        if let Some(n) = f.get("dataSize").and_then(|x| x.as_u64()) {
                            return a.data.len() as u64 == n;
                        }
                        if let Some(m) = f.get("memcmp") {
                            let off =
                                m.get("offset").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
                            let enc = m
                                .get("encoding")
                                .and_then(|x| x.as_str())
                                .unwrap_or("base58");
                            let bytes = m.get("bytes").and_then(|x| x.as_str()).and_then(|s| {
                                if enc == "base64" {
                                    B64.decode(s).ok()
                                } else {
                                    bs58_decode(s)
                                }
                            });
                            return bytes.is_some_and(|b| {
                                a.data.get(off..off + b.len()) == Some(b.as_slice())
                            });
                        }
                        true
                    })
                })
                .map(|(k, a)| json!({"pubkey": k.to_string(), "account": account_json(&a)}))
                .collect();
            Ok(json!(rows))
        }
        "requestAirdrop" => {
            let k = key(p.get(0).unwrap_or(&Value::Null))?;
            let n = p
                .get(1)
                .and_then(|x| x.as_u64())
                .ok_or_else(|| invalid("lamports"))?;
            lock(state)
                .airdrop(&k, n)
                .map(|s| json!(s.to_string()))
                .map_err(|e| err(-32003, e))
        }
        "sendTransaction" => {
            let wire = tx_bytes(p)?;
            let skip = p
                .get(1)
                .and_then(|c| c.get("skipPreflight"))
                .and_then(|x| x.as_bool())
                .unwrap_or(false);
            let mut c = lock(state);
            if !skip {
                let sim = c.simulate(&wire, true).map_err(send_err)?;
                if sim.err.is_some() {
                    return Err(RpcErr {
                        code: -32002,
                        message: format!(
                            "Transaction simulation failed: {}",
                            sim.err.clone().unwrap_or_default()
                        ),
                        data: Some(
                            json!({"err": err_json(&sim.err, sim.code), "logs": sim.logs, "unitsConsumed": sim.units}),
                        ),
                    });
                }
            }
            c.submit(&wire)
                .map(|s| json!(s.to_string()))
                .map_err(send_err)
        }
        "simulateTransaction" => {
            let wire = tx_bytes(p)?;
            let cfg = p.get(1).cloned().unwrap_or(Value::Null);
            let sig_verify = cfg
                .get("sigVerify")
                .and_then(|x| x.as_bool())
                .unwrap_or(false);
            let c = lock(state);
            let sim = c.simulate(&wire, sig_verify).map_err(send_err)?;
            let want: Option<Vec<Address>> = cfg
                .get("accounts")
                .and_then(|a| a.get("addresses"))
                .and_then(|a| a.as_array())
                .map(|a| a.iter().filter_map(|k| k.as_str()?.parse().ok()).collect());
            let accounts = want.map(|ks| {
                Value::Array(
                    ks.iter()
                        .map(|k| {
                            sim.accounts
                                .iter()
                                .find(|(x, _)| x == k)
                                .and_then(|(_, a)| a.as_ref())
                                .map(account_json)
                                .unwrap_or(Value::Null)
                        })
                        .collect(),
                )
            });
            Ok(ctx(
                &c,
                json!({"err": err_json(&sim.err, sim.code), "logs": sim.logs, "unitsConsumed": sim.units,
                "accounts": accounts.unwrap_or(Value::Null), "returnData": Value::Null}),
            ))
        }
        "getSignatureStatuses" => {
            let sigs: Vec<Signature> = p
                .get(0)
                .and_then(|x| x.as_array())
                .ok_or_else(|| invalid("signatures"))?
                .iter()
                .map(|s| {
                    s.as_str()
                        .and_then(|s| s.parse().ok())
                        .ok_or_else(|| invalid("signature"))
                })
                .collect::<Result<_, _>>()?;
            let c = lock(state);
            let v: Vec<Value> = sigs
                .iter()
                .map(|s| match c.status(s) {
                    None => Value::Null,
                    Some(st) => {
                        let e = err_json(&st.err, st.code);
                        let status = if e.is_null() { json!({"Ok": null}) } else { json!({"Err": e.clone()}) };
                        json!({"slot": st.slot, "confirmations": null, "err": e, "status": status, "confirmationStatus": "finalized"})
                    }
                })
                .collect();
            Ok(ctx(&c, json!(v)))
        }
        "getTransaction" => {
            let s: Signature = p
                .get(0)
                .and_then(|x| x.as_str())
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| invalid("signature"))?;
            let c = lock(state);
            Ok(match c.transaction(&s) {
                None => Value::Null,
                Some(l) => json!({
                    "slot": l.slot, "blockTime": l.block_time, "version": "legacy",
                    "transaction": [B64.encode(&l.wire), "base64"],
                    "meta": {"err": err_json(&l.err, l.code), "fee": l.fee, "logMessages": l.logs, "computeUnitsConsumed": l.units,
                        "status": if l.err.is_none() { json!({"Ok": null}) } else { json!({"Err": err_json(&l.err, l.code)}) },
                        "preBalances": [], "postBalances": [], "innerInstructions": [], "loadedAddresses": {"writable": [], "readonly": []},
                        "returnData": if l.return_data.is_empty() { Value::Null } else { json!({"data": [B64.encode(&l.return_data), "base64"]}) }},
                }),
            })
        }
        "getSignaturesForAddress" => {
            let k = key(p.get(0).unwrap_or(&Value::Null))?;
            let limit = p
                .get(1)
                .and_then(|c| c.get("limit"))
                .and_then(|x| x.as_u64())
                .unwrap_or(1_000)
                .min(1_000) as usize;
            let c = lock(state);
            let v: Vec<Value> = c
                .signatures_for(&k, limit)
                .into_iter()
                .map(|l| json!({"signature": l.signature.to_string(), "slot": l.slot, "err": err_json(&l.err, l.code), "memo": null,
                    "blockTime": l.block_time, "confirmationStatus": "finalized"}))
                .collect();
            Ok(json!(v))
        }
        "frontier_feed" => {
            let after = p.get(0).and_then(|x| x.as_u64()).unwrap_or(0);
            let limit = p
                .get(1)
                .and_then(|x| x.as_u64())
                .unwrap_or(1_000)
                .min(10_000) as usize;
            let prog = p
                .get(2)
                .and_then(|x| x.as_str())
                .and_then(|s| s.parse::<Address>().ok());
            let c = lock(state);
            let v: Vec<Value> = c
                .feed(after, limit, prog.as_ref())
                .into_iter()
                .map(|l| json!({
                    "seq": l.seq, "slot": l.slot, "signature": l.signature.to_string(), "blockTime": l.block_time,
                    "tx": B64.encode(&l.wire), "logs": l.logs, "err": err_json(&l.err, l.code), "units": l.units, "fee": l.fee,
                    "post": l.post.iter().map(|(k, a)| json!({"key": k.to_string(), "account": a.as_ref().map(account_json)})).collect::<Vec<_>>(),
                }))
                .collect();
            Ok(json!(v))
        }
        "frontier_pause" => {
            lock(state).set_paused(true);
            Ok(json!(true))
        }
        "frontier_resume" => {
            lock(state).set_paused(false);
            Ok(json!(true))
        }
        "frontier_setScale" => {
            let s = p
                .get(0)
                .and_then(|x| x.as_f64())
                .filter(|s| *s > 0.0 && *s <= 100_000.0)
                .ok_or_else(|| invalid("scale in (0, 100000]"))?;
            lock(state).set_scale(s);
            Ok(json!(true))
        }
        "frontier_status" => {
            let c = lock(state);
            Ok(
                json!({"slot": c.slot(), "unixTimestamp": c.unix_timestamp(), "scale": c.scale(), "paused": c.paused(), "pending": c.pending(),
                "transactions": c.transaction_count()}),
            )
        }
        "frontier_setAccount" => {
            let mut c = lock(state);
            if !c.config().allow_tamper {
                return Err(err(
                    -32601,
                    "frontier_setAccount needs --allow-tamper (tamper fixtures only)",
                ));
            }
            let k = key(p.get(0).unwrap_or(&Value::Null))?;
            let a = fclient::rpc::parse_account(p.get(1).unwrap_or(&Value::Null))
                .map_err(|e| invalid(e.to_string()))?
                .unwrap_or(Account {
                    lamports: 0,
                    data: vec![],
                    owner: fclient::addr::system_program(),
                    executable: false,
                });
            c.set_account(k, a)
                .map(|_| json!(true))
                .map_err(|e| err(-32003, e))
        }
        "frontier_snapshot" | "frontier_restore" | "frontier_hold" | "slotSubscribe"
        | "signatureSubscribe" | "logsSubscribe" => Err(err(
            -32601,
            format!("{method}: not in the W1-F localnet MVP (W2-C adds it)"),
        )),
        m => Err(err(-32601, format!("Method not found: {m}"))),
    }
}

fn send_err(e: SendError) -> RpcErr {
    match e {
        SendError::BlockhashNotFound => {
            err(-32002, "Transaction simulation failed: Blockhash not found")
        }
        SendError::AlreadyProcessed => err(
            -32002,
            "Transaction simulation failed: This transaction has already been processed",
        ),
        other => err(-32602, other.to_string()),
    }
}

async fn rpc(State(state): State<Shared>, Json(req): Json<Value>) -> Json<Value> {
    let one = |r: &Value| -> Value {
        let id = r.get("id").cloned().unwrap_or(Value::Null);
        let method = r.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = r.get("params").cloned().unwrap_or(json!([]));
        match handle(&state, method, &params) {
            Ok(v) => json!({"jsonrpc": "2.0", "id": id, "result": v}),
            Err(e) => {
                let mut o = json!({"code": e.code, "message": e.message});
                if let Some(d) = e.data {
                    o["data"] = d;
                }
                json!({"jsonrpc": "2.0", "id": id, "error": o})
            }
        }
    };
    Json(match &req {
        Value::Array(batch) => Value::Array(batch.iter().map(one).collect()),
        r => one(r),
    })
}

/// The HTTP router.
pub fn router(state: Shared) -> Router {
    Router::new().route("/", post(rpc)).with_state(state)
}

/// The 400-ms slot ticker (real time at every scale, I-54).
pub fn spawn_ticker(state: Shared) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut iv = tokio::time::interval(Duration::from_millis(SLOT_MS));
        iv.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            iv.tick().await;
            let mut c = lock(&state);
            if !c.paused() {
                c.produce_block();
            }
        }
    })
}

/// A running node (ticker + server); dropping it does not stop it, call `stop`.
pub struct Running {
    pub addr: SocketAddr,
    pub state: Shared,
    ticker: tokio::task::JoinHandle<()>,
    server: tokio::task::JoinHandle<()>,
}

impl Running {
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
    pub fn stop(self) {
        self.ticker.abort();
        self.server.abort();
    }
}

/// Binds `127.0.0.1:port` (0 = any free port, for tests) and serves.
pub async fn start(state: Shared, port: u16) -> Result<Running, String> {
    check_port(port)?;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| format!("bind 127.0.0.1:{port}: {e}"))?;
    let addr = listener.local_addr().map_err(|e| e.to_string())?;
    let app = router(state.clone());
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let ticker = spawn_ticker(state.clone());
    Ok(Running {
        addr,
        state,
        ticker,
        server,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_and_base58() {
        assert!(check_port(0).is_ok());
        assert!(check_port(41_010).is_ok());
        assert!(check_port(4_185).is_err());
        assert!(check_port(38_810).is_err());
        let k = Address::new_from_array([7; 32]);
        assert_eq!(bs58_decode(&k.to_string()).unwrap(), k.to_bytes().to_vec());
        assert_eq!(bs58_decode("11").unwrap(), vec![0, 0]);
    }
}
