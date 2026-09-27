//! JSON-RPC over HTTP (`POST /`) with the subset `@solana/web3.js` 1.99,
//! `send.mjs` and `fclient` use (§8.7), the loopback `frontier_*`
//! extensions, the WebSocket subscriptions ([`crate::ws`]) and the 400-ms
//! slot ticker.
//!
//! **Methods:** `getLatestBlockhash, isBlockhashValid, sendTransaction,
//! simulateTransaction (accounts post-state, returnData),
//! getAccountInfo, getMultipleAccounts, getSignatureStatuses,
//! getTransaction (json and base64), getSignaturesForAddress (before,
//! until, limit), getSlot, getBlockHeight, getBlockTime, getBalance,
//! getMinimumBalanceForRentExemption, getProgramAccounts (memcmp, dataSize,
//! dataSlice, withContext), requestAirdrop, getEpochInfo, getVersion,
//! getHealth, getGenesisHash`. Account encodings `base64` (what web3.js
//! asks for) and `base58`; `dataSlice` on every account read.
//!
//! **Extensions** (loopback only): `frontier_feed, frontier_status,
//! frontier_pause, frontier_resume, frontier_setScale (at the next slot
//! boundary), frontier_hold, frontier_release, frontier_holds,
//! frontier_snapshot, frontier_restore, frontier_stateHash,
//! frontier_setAccount (--allow-tamper only)`.
//!
//! Conformance: `tests/conformance.rs` drives every method through
//! web3.js 1.99 and `permutation-gateway/src/send.mjs` themselves.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{extract::State, routing::post, Json, Router};
use base64::Engine;
use serde_json::{json, Value};
use solana_address::Address;
use solana_signature::Signature;

use fclient::ports::Account;
use fclient::rpc::account_json;

use crate::chain::{Chain, Hold, Landed, SendError, SnapInfo, SLOT_MS};

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

pub type Shared = Arc<Mutex<Chain>>;

pub fn lock(s: &Shared) -> std::sync::MutexGuard<'_, Chain> {
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
        .ok_or_else(|| invalid("Invalid param: WrongSize"))
}

fn sig(v: &Value) -> Result<Signature, RpcErr> {
    v.as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| invalid("Invalid param: WrongSize"))
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

const B58: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// base58 decode (Bitcoin alphabet) without an extra dependency.
pub fn bs58_decode(s: &str) -> Option<Vec<u8>> {
    let mut out: Vec<u8> = vec![];
    for ch in s.bytes() {
        let mut carry = B58.iter().position(|&c| c == ch)? as u32;
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

/// base58 encode (Bitcoin alphabet).
pub fn bs58_encode(b: &[u8]) -> String {
    let mut digits: Vec<u8> = vec![];
    for &x in b {
        let mut carry = x as u32;
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
    let zeros = b.iter().take_while(|&&x| x == 0).count();
    let mut s = String::with_capacity(zeros + digits.len());
    s.extend(std::iter::repeat_n('1', zeros));
    s.extend(digits.iter().rev().map(|&d| B58[d as usize] as char));
    s
}

/// How to render account data (`encoding`, `dataSlice`).
struct AcctOpts {
    base58: bool,
    slice: Option<(usize, usize)>,
}

fn acct_opts(cfg: Option<&Value>) -> Result<AcctOpts, RpcErr> {
    let enc = cfg
        .and_then(|c| c.get("encoding"))
        .and_then(|e| e.as_str())
        .unwrap_or("base64");
    let base58 = match enc {
        "base64" | "jsonParsed" => false,
        "base58" => true,
        e => return Err(invalid(format!("unsupported encoding {e}"))),
    };
    let slice = cfg.and_then(|c| c.get("dataSlice")).map(|d| {
        (
            d.get("offset").and_then(|x| x.as_u64()).unwrap_or(0) as usize,
            d.get("length").and_then(|x| x.as_u64()).unwrap_or(0) as usize,
        )
    });
    Ok(AcctOpts { base58, slice })
}

fn acct_json(a: &Account, o: &AcctOpts) -> Value {
    let data: &[u8] = match o.slice {
        Some((off, len)) => {
            // Saturating: an offset + length past usize::MAX is "to the
            // end", never a wrap and a panic (integ-W2 review of W2-C).
            let s = off.min(a.data.len());
            &a.data[s..s.saturating_add(len).min(a.data.len())]
        }
        None => &a.data,
    };
    let mut v = account_json(a);
    v["data"] = if o.base58 {
        json!([bs58_encode(data), "base58"])
    } else {
        json!([B64.encode(data), "base64"])
    };
    v
}

fn status_json(l: &Landed) -> Value {
    if l.err_json.is_null() {
        json!({"Ok": null})
    } else {
        json!({"Err": l.err_json})
    }
}

/// A legacy transaction in the RPC's `json` encoding.
fn tx_json(wire: &[u8]) -> Value {
    let Ok(t) = fclient::tx::from_wire(wire) else {
        return Value::Null;
    };
    let m = &t.message;
    json!({
        "signatures": t.signatures.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        "message": {
            "header": {
                "numRequiredSignatures": m.header.num_required_signatures,
                "numReadonlySignedAccounts": m.header.num_readonly_signed_accounts,
                "numReadonlyUnsignedAccounts": m.header.num_readonly_unsigned_accounts,
            },
            "accountKeys": m.account_keys.iter().map(|k| k.to_string()).collect::<Vec<_>>(),
            "recentBlockhash": m.recent_blockhash.to_string(),
            "instructions": m.instructions.iter().map(|ci| json!({
                "programIdIndex": ci.program_id_index,
                "accounts": ci.accounts,
                "data": bs58_encode(&ci.data),
                "stackHeight": null,
            })).collect::<Vec<_>>(),
        },
    })
}

fn hold_json(h: &Hold) -> Value {
    json!({"id": h.id, "keys": h.keys.iter().map(|k| k.to_string()).collect::<Vec<_>>(),
        "priorityMilli": h.priority_milli, "fromSlot": h.from_slot, "untilSlot": h.until_slot,
        "filledCu": h.filled_cu, "notionalLamports": h.notional_lamports})
}

fn snap_json(s: &SnapInfo) -> Value {
    json!({"slot": s.slot, "path": s.path.display().to_string(), "stateHash": hex::encode(s.state_hash),
        "accounts": s.accounts, "bytes": s.bytes})
}

fn handle(state: &Shared, method: &str, p: &Value) -> Result<Value, RpcErr> {
    match method {
        "getHealth" => Ok(json!("ok")),
        "getVersion" => Ok(
            json!({"solana-core": "3.1.9", "feature-set": 0, "frontier-localnet": env!("CARGO_PKG_VERSION"), "litesvm": "0.16.0"}),
        ),
        "getGenesisHash" => {
            let c = lock(state);
            let g = crate::chain::genesis_hash(c.config());
            Ok(json!(g.to_string()))
        }
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
        "isBlockhashValid" => {
            let h: fclient::Hash = p
                .get(0)
                .and_then(|x| x.as_str())
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| invalid("blockhash"))?;
            let c = lock(state);
            Ok(ctx(&c, json!(c.blockhash_valid(&h))))
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
            let o = acct_opts(p.get(1))?;
            let c = lock(state);
            Ok(ctx(
                &c,
                c.account(&k)
                    .map(|a| acct_json(&a, &o))
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
            if ks.len() > 100 {
                return Err(invalid("Too many inputs provided; max 100"));
            }
            let o = acct_opts(p.get(1))?;
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
            let v: Vec<Value> = ks
                .iter()
                .map(|k| {
                    c.account(k)
                        .map(|a| acct_json(&a, &o))
                        .unwrap_or(Value::Null)
                })
                .collect();
            Ok(ctx(&c, json!(v)))
        }
        "getProgramAccounts" => {
            let prog = key(p.get(0).unwrap_or(&Value::Null))?;
            let cfg = p.get(1);
            let o = acct_opts(cfg)?;
            let filters = cfg
                .and_then(|x| x.get("filters"))
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default();
            let with_context = cfg
                .and_then(|x| x.get("withContext"))
                .and_then(|x| x.as_bool())
                .unwrap_or(false);
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
                                off.checked_add(b.len())
                                    .and_then(|end| a.data.get(off..end))
                                    == Some(b.as_slice())
                            });
                        }
                        true
                    })
                })
                .map(|(k, a)| json!({"pubkey": k.to_string(), "account": acct_json(&a, &o)}))
                .collect();
            Ok(if with_context {
                ctx(&c, json!(rows))
            } else {
                json!(rows)
            })
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
                let (sim, ej, _, _) = c.simulate_full(&wire, true).map_err(send_err)?;
                if sim.err.is_some() {
                    return Err(RpcErr {
                        code: -32002,
                        message: format!(
                            "Transaction simulation failed: {}",
                            sim.err.clone().unwrap_or_default()
                        ),
                        data: Some(
                            json!({"err": ej, "logs": sim.logs, "unitsConsumed": sim.units, "accounts": null, "returnData": null}),
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
            let (sim, ej, ret, ret_pid) = c.simulate_full(&wire, sig_verify).map_err(send_err)?;
            let o = acct_opts(cfg.get("accounts"))?;
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
                                .map(|(_, a)| a.clone())
                                .unwrap_or_else(|| c.account(k))
                                .map(|a| acct_json(&a, &o))
                                .unwrap_or(Value::Null)
                        })
                        .collect(),
                )
            });
            let return_data = match ret_pid {
                Some(pid) if !ret.is_empty() => {
                    json!({"programId": pid.to_string(), "data": [B64.encode(&ret), "base64"]})
                }
                _ => Value::Null,
            };
            Ok(ctx(
                &c,
                json!({"err": ej, "logs": sim.logs, "unitsConsumed": sim.units,
                "accounts": accounts.unwrap_or(Value::Null), "returnData": return_data, "innerInstructions": null}),
            ))
        }
        "getSignatureStatuses" => {
            let sigs: Vec<Signature> = p
                .get(0)
                .and_then(|x| x.as_array())
                .ok_or_else(|| invalid("signatures"))?
                .iter()
                .map(sig)
                .collect::<Result<_, _>>()?;
            if sigs.len() > 256 {
                return Err(invalid("Too many inputs provided; max 256"));
            }
            let c = lock(state);
            let v: Vec<Value> = sigs
                .iter()
                .map(|s| match c.transaction(s) {
                    None => Value::Null,
                    Some(l) => json!({"slot": l.slot, "confirmations": null, "err": l.err_json,
                        "status": status_json(l), "confirmationStatus": "finalized"}),
                })
                .collect();
            Ok(ctx(&c, json!(v)))
        }
        "getTransaction" => {
            let s = sig(p.get(0).unwrap_or(&Value::Null))?;
            let enc = p
                .get(1)
                .and_then(|c| c.get("encoding"))
                .and_then(|e| e.as_str())
                .unwrap_or("json");
            let c = lock(state);
            Ok(match c.transaction(&s) {
                None => Value::Null,
                Some(l) => {
                    let transaction = match enc {
                        "base64" => json!([B64.encode(&l.wire), "base64"]),
                        "base58" => json!([bs58_encode(&l.wire), "base58"]),
                        _ => tx_json(&l.wire),
                    };
                    json!({
                        "slot": l.slot, "blockTime": l.block_time, "version": "legacy",
                        "transaction": transaction,
                        "meta": {"err": l.err_json, "fee": l.fee, "logMessages": l.logs, "computeUnitsConsumed": l.units,
                            "status": status_json(l),
                            "preBalances": l.pre_balances, "postBalances": l.post_balances,
                            "innerInstructions": [], "preTokenBalances": [], "postTokenBalances": [], "rewards": [],
                            "loadedAddresses": {"writable": [], "readonly": []},
                            "returnData": if l.return_data.is_empty() { Value::Null } else {
                                let pid = l.keys.get(tx_program_index(&l.wire)).map(|k| k.to_string()).unwrap_or_default();
                                json!({"programId": pid, "data": [B64.encode(&l.return_data), "base64"]}) }},
                    })
                }
            })
        }
        "getSignaturesForAddress" => {
            let k = key(p.get(0).unwrap_or(&Value::Null))?;
            let cfg = p.get(1);
            let limit = cfg
                .and_then(|c| c.get("limit"))
                .and_then(|x| x.as_u64())
                .unwrap_or(1_000);
            if !(1..=1_000).contains(&limit) {
                return Err(invalid("Invalid limit; max 1000"));
            }
            let before = cfg
                .and_then(|c| c.get("before"))
                .filter(|v| !v.is_null())
                .map(sig)
                .transpose()?;
            let until = cfg
                .and_then(|c| c.get("until"))
                .filter(|v| !v.is_null())
                .map(sig)
                .transpose()?;
            let c = lock(state);
            let v: Vec<Value> = c
                .signatures_for(&k, limit as usize, before.as_ref(), until.as_ref())
                .into_iter()
                .map(|l| json!({"signature": l.signature.to_string(), "slot": l.slot, "err": l.err_json, "memo": null,
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
                    "tx": B64.encode(&l.wire), "logs": l.logs, "err": l.err_json, "units": l.units, "fee": l.fee,
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
            let mut c = lock(state);
            c.set_scale(s);
            Ok(json!({"atSlot": c.slot() + 1}))
        }
        "frontier_status" => {
            let c = lock(state);
            Ok(
                json!({"slot": c.slot(), "unixTimestamp": c.unix_timestamp(), "scale": c.scale(), "pendingScale": c.pending_scale(),
                "paused": c.paused(), "pending": c.pending(), "transactions": c.transaction_count(), "holds": c.holds().len(),
                "runId": hex::encode(c.run_id()), "ledgerBytes": c.wal_offset(),
                "dataDir": c.config().data_dir.as_ref().map(|d| d.display().to_string())}),
            )
        }
        "frontier_hold" => {
            let keys: Vec<Address> = p
                .get(0)
                .and_then(|x| x.as_array())
                .ok_or_else(|| invalid("keys"))?
                .iter()
                .map(key)
                .collect::<Result<_, _>>()?;
            let prio = p
                .get(1)
                .and_then(|x| x.as_u64())
                .ok_or_else(|| invalid("priority_milli"))?;
            let slots = p
                .get(2)
                .and_then(|x| x.as_u64())
                .ok_or_else(|| invalid("slots"))?;
            lock(state)
                .hold(keys, prio, slots)
                .map(|h| hold_json(&h))
                .map_err(invalid)
        }
        "frontier_release" => {
            let id = p
                .get(0)
                .and_then(|x| x.as_u64())
                .ok_or_else(|| invalid("id"))?;
            Ok(lock(state)
                .release(id)
                .map(|h| hold_json(&h))
                .unwrap_or(Value::Null))
        }
        "frontier_holds" => Ok(json!(lock(state)
            .holds()
            .iter()
            .map(hold_json)
            .collect::<Vec<_>>())),
        "frontier_snapshot" => {
            let path = p.get(0).and_then(|x| x.as_str()).map(PathBuf::from);
            lock(state)
                .snapshot(path.as_deref())
                .map(|s| snap_json(&s))
                .map_err(|e| err(-32003, e))
        }
        "frontier_restore" => {
            let path = p
                .get(0)
                .and_then(|x| x.as_str())
                .map(PathBuf::from)
                .ok_or_else(|| invalid("snapshot path"))?;
            lock(state)
                .restore(&path)
                .map(|s| snap_json(&s))
                .map_err(|e| err(-32003, e))
        }
        "frontier_stateHash" => {
            let c = lock(state);
            Ok(json!({"slot": c.slot(), "stateHash": hex::encode(c.state_hash())}))
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
        "slotSubscribe" | "signatureSubscribe" | "logsSubscribe" => Err(err(
            -32601,
            format!("{method}: use the WebSocket endpoint (RPC port + 1)"),
        )),
        m => Err(err(-32601, format!("Method not found: {m}"))),
    }
}

/// Index of the first non-ComputeBudget instruction's program in the keys.
fn tx_program_index(wire: &[u8]) -> usize {
    let cbp = fclient::addr::compute_budget_program();
    fclient::tx::from_wire(wire)
        .ok()
        .and_then(|t| {
            t.message
                .instructions
                .iter()
                .map(|ci| ci.program_id_index as usize)
                .find(|&i| t.message.account_keys.get(i) != Some(&cbp))
        })
        .unwrap_or(usize::MAX)
}

fn send_err(e: SendError) -> RpcErr {
    match e {
        SendError::BlockhashNotFound => RpcErr {
            code: -32002,
            message: "Transaction simulation failed: Blockhash not found".into(),
            data: Some(
                json!({"err": "BlockhashNotFound", "logs": [], "accounts": null, "unitsConsumed": 0, "returnData": null}),
            ),
        },
        SendError::AlreadyProcessed => RpcErr {
            code: -32002,
            message: "Transaction simulation failed: This transaction has already been processed"
                .into(),
            data: Some(
                json!({"err": "AlreadyProcessed", "logs": [], "accounts": null, "unitsConsumed": 0, "returnData": null}),
            ),
        },
        SendError::Invalid(m) => err(
            -32003,
            format!("Transaction signature verification failure: {m}"),
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

/// A running node (ticker + HTTP + WebSocket); call `stop` to end it.
pub struct Running {
    pub addr: SocketAddr,
    pub ws_addr: SocketAddr,
    pub state: Shared,
    ticker: tokio::task::JoinHandle<()>,
    server: tokio::task::JoinHandle<()>,
    ws: tokio::task::JoinHandle<()>,
}

impl Running {
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
    pub fn ws_url(&self) -> String {
        format!("ws://{}", self.ws_addr)
    }
    pub fn stop(self) {
        self.ticker.abort();
        self.server.abort();
        self.ws.abort();
    }
}

/// Binds `127.0.0.1:port` for JSON-RPC and `127.0.0.1:port+1` for the
/// WebSocket subscriptions (both any free port when `port` is 0, for
/// tests) and serves.
pub async fn start(state: Shared, port: u16) -> Result<Running, String> {
    let ws_port = if port == 0 { 0 } else { port + 1 };
    start_with(state, port, ws_port).await
}

/// As [`start`], with an explicit WebSocket port.
pub async fn start_with(state: Shared, port: u16, ws_port: u16) -> Result<Running, String> {
    check_port(port)?;
    check_port(ws_port)?;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| format!("bind 127.0.0.1:{port}: {e}"))?;
    let wsl = tokio::net::TcpListener::bind(("127.0.0.1", ws_port))
        .await
        .map_err(|e| format!("bind 127.0.0.1:{ws_port}: {e}"))?;
    let addr = listener.local_addr().map_err(|e| e.to_string())?;
    let ws_addr = wsl.local_addr().map_err(|e| e.to_string())?;
    let app = router(state.clone());
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let ws = crate::ws::spawn(state.clone(), wsl);
    let ticker = spawn_ticker(state.clone());
    Ok(Running {
        addr,
        ws_addr,
        state,
        ticker,
        server,
        ws,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An oversized or overflowing `dataSlice` is clipped to the account's
    /// data, never a wrap and a worker panic (integ-W2 review of W2-C:
    /// offset 1, length u64::MAX panicked the release build).
    #[test]
    fn data_slice_past_the_end_is_clipped() {
        let a = Account {
            lamports: 1,
            data: vec![1, 2, 3],
            owner: Address::new_from_array([7; 32]),
            executable: false,
        };
        for (off, len, want) in [
            (1usize, usize::MAX, vec![2u8, 3]),
            (usize::MAX, usize::MAX, vec![]),
            (5, 1, vec![]),
            (0, 2, vec![1, 2]),
        ] {
            let o = AcctOpts {
                base58: false,
                slice: Some((off, len)),
            };
            let v = acct_json(&a, &o);
            assert_eq!(v["data"][0], B64.encode(&want), "offset {off} length {len}");
        }
    }

    #[test]
    fn ports_and_base58() {
        assert!(check_port(0).is_ok());
        assert!(check_port(41_010).is_ok());
        assert!(check_port(4_185).is_err());
        assert!(check_port(38_810).is_err());
        let k = Address::new_from_array([7; 32]);
        assert_eq!(bs58_decode(&k.to_string()).unwrap(), k.to_bytes().to_vec());
        assert_eq!(bs58_encode(k.as_ref()), k.to_string());
        assert_eq!(bs58_decode("11").unwrap(), vec![0, 0]);
        assert_eq!(bs58_encode(&[0, 0, 1]), "112");
        assert_eq!(bs58_encode(&[]), "");
        let s = Signature::from([9u8; 64]);
        assert_eq!(bs58_encode(s.as_ref()), s.to_string());
    }
}
