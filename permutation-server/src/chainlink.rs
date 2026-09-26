//! Chain mode: the world comes from the on-chain program (through the local
//! gateway), and orders go back to it. The server keeps doing what only it
//! can do off chain — previews, the decision ledger, bots — on the exact
//! bytes the program stores.
//!
//! The gateway is a local HTTP service (permutation-gateway); this is a
//! minimal HTTP/1.1 client for it, so the server needs no TLS or async stack.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use borsh::BorshDeserialize;
use permutation_rules::state::WorldState;
use permutation_rules::tick::{TickInput, PHASE_COUNT};
use serde_json::Value;

pub use permutation_chain::state::WorldMeta;
use permutation_chain::state::{WORLD_HEADER, WORLD_MAGIC};

use crate::codec::{base64, from_hex};

pub struct Snapshot {
    pub state: WorldState,
    pub meta: WorldMeta,
    pub slot: u64,
    pub layer: String,
    pub phase: String,
}

/// Status, lower-cased headers, body.
type Response = (u16, Vec<(String, String)>, Vec<u8>);

pub struct ChainLink {
    host: String,
    port: u16,
    /// When set, `log_records` keeps only records this program emitted.
    program: Option<String>,
    /// The operator token (V5 §18.2): sent as `Authorization: Bearer …` so
    /// the gateway lets this server act for hosted members.
    token: Option<String>,
}

impl ChainLink {
    /// `url` like `http://127.0.0.1:4191`.
    pub fn new(url: &str) -> Result<ChainLink, String> {
        let (host, port) = parse_http_url(url)?;
        Ok(ChainLink {
            program: None,
            token: None,
            host,
            port,
        })
    }

    /// Present the operator token on every request.
    pub fn with_token(mut self, token: Option<String>) -> ChainLink {
        self.token = token.filter(|t| !t.is_empty());
        self
    }

    pub fn has_token(&self) -> bool {
        self.token.is_some()
    }

    /// A secret derived from the operator token (seeds the AI temperaments).
    pub fn secret(&self) -> Option<[u8; 32]> {
        use sha2::{Digest, Sha256};
        let t = self.token.as_ref()?;
        Some(
            Sha256::new()
                .chain_update(b"PS/operator-secret")
                .chain_update(t.as_bytes())
                .finalize()
                .into(),
        )
    }

    pub fn url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }

    fn request(&self, method: &str, path: &str, body: Option<&str>) -> Result<Response, String> {
        let mut s = TcpStream::connect((self.host.as_str(), self.port))
            .map_err(|e| format!("gateway unreachable: {e}"))?;
        s.set_read_timeout(Some(Duration::from_secs(90))).ok();
        let body = body.unwrap_or("");
        let auth = self
            .token
            .as_ref()
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        let req = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.host,
            body.len()
        );
        s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
        let mut raw = Vec::new();
        s.read_to_end(&mut raw).map_err(|e| e.to_string())?;
        let split = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .ok_or("bad HTTP response")?;
        let head = String::from_utf8_lossy(&raw[..split]).to_string();
        let mut lines = head.lines();
        let status: u16 = lines
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|c| c.parse().ok())
            .ok_or("bad status")?;
        let headers: Vec<(String, String)> = lines
            .filter_map(|l| {
                l.split_once(':')
                    .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
            })
            .collect();
        let mut body = raw[split + 4..].to_vec();
        if headers
            .iter()
            .any(|(k, v)| k == "transfer-encoding" && v.contains("chunked"))
        {
            body = dechunk(&body);
        }
        Ok((status, headers, body))
    }

    pub fn get_json(&self, path: &str) -> Result<Value, String> {
        let (code, _, body) = self.request("GET", path, None)?;
        let v: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
        if code >= 400 {
            return Err(v["error"].as_str().unwrap_or("gateway error").to_string());
        }
        Ok(v)
    }

    pub fn post_json(&self, path: &str, body: &Value) -> Result<Value, String> {
        let (code, _, bytes) = self.request("POST", path, Some(&body.to_string()))?;
        let v: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if code >= 400 {
            let logs = v["logs"]
                .as_array()
                .map(|l| {
                    l.iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join(" | ")
                })
                .unwrap_or_default();
            return Err(format!(
                "{} {}",
                v["error"].as_str().unwrap_or("gateway error"),
                logs
            ));
        }
        Ok(v)
    }

    /// Solana JSON-RPC call (for a validator URL instead of the gateway).
    pub fn rpc(&self, method: &str, params: Value) -> Result<Value, String> {
        let body =
            serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        let (_, _, bytes) = self.request("POST", "/", Some(&body.to_string()))?;
        let v: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if let Some(e) = v.get("error") {
            return Err(format!("{method}: {e}"));
        }
        Ok(v["result"].clone())
    }

    /// Raw account data through JSON-RPC (`None` if the account does not exist).
    pub fn account_data(&self, address: &str) -> Result<Option<Vec<u8>>, String> {
        let r = self.rpc(
            "getAccountInfo",
            serde_json::json!([address, {"encoding": "base64", "commitment": "confirmed"}]),
        )?;
        if r["value"].is_null() {
            return Ok(None);
        }
        Ok(Some(base64(r["value"]["data"][0].as_str().unwrap_or(""))?))
    }

    /// Keep only the log records of `program` (base58) from now on.
    pub fn with_program(mut self, program: &str) -> ChainLink {
        self.program = Some(program.to_string());
        self
    }

    /// `Program data:` log records (sol_log_data) of a transaction. A failed
    /// transaction has none. With `with_program`, only records emitted while
    /// that program was the one executing (the innermost invocation) count,
    /// so no other program can forge them.
    pub fn log_records(&self, signature: &str) -> Result<Vec<Vec<Vec<u8>>>, String> {
        let r = self.rpc("getTransaction", serde_json::json!([signature, {"encoding": "json", "commitment": "confirmed", "maxSupportedTransactionVersion": 0}]))?;
        if !r["meta"]["err"].is_null() {
            return Ok(Vec::new());
        }
        let logs = r["meta"]["logMessages"]
            .as_array()
            .ok_or("transaction not found")?;
        let mut stack: Vec<&str> = Vec::new();
        let mut out = Vec::new();
        for l in logs.iter().filter_map(|l| l.as_str()) {
            if let Some(rest) = l.strip_prefix("Program ") {
                let mut words = rest.split(' ');
                let (id, what) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
                if what == "invoke" {
                    stack.push(id);
                    continue;
                }
                if (what == "success" || what == "failed:") && stack.last() == Some(&id) {
                    stack.pop();
                    continue;
                }
            }
            let Some(data) = l.strip_prefix("Program data: ") else {
                continue;
            };
            if let Some(p) = &self.program {
                if stack.last().copied() != Some(p.as_str()) {
                    continue;
                }
            }
            out.push(data.split(' ').map(base64).collect::<Result<Vec<_>, _>>()?);
        }
        Ok(out)
    }

    /// The world as the program stores it, or `None` while genesis runs.
    pub fn world(&self) -> Result<Option<Snapshot>, String> {
        let (code, headers, data) = self.request("GET", "/world.bin", None)?;
        if code != 200 {
            return Ok(None);
        }
        let h = |k: &str| {
            headers
                .iter()
                .find(|(x, _)| x == k)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        if data.len() < WORLD_HEADER || data[..8] != WORLD_MAGIC {
            return Ok(None);
        }
        let len = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
        let meta = WorldMeta::deserialize(&mut &data[12..WORLD_HEADER])
            .map_err(|e| format!("world meta: {e}"))?;
        let body = data
            .get(WORLD_HEADER..WORLD_HEADER + len)
            .ok_or("world body truncated")?;
        let state = WorldState::try_from_slice(body).map_err(|e| format!("world decode: {e}"))?;
        Ok(Some(Snapshot {
            state,
            meta,
            slot: h("x-slot").parse().unwrap_or(0),
            layer: h("x-layer"),
            phase: h("x-phase"),
        }))
    }

    /// The input the program resolved `tick` with (every office's batch and
    /// every governance action), from the PS_TICK record that completed it.
    pub fn resolved_input(&self, tick: u16) -> Result<Option<(TickInput, Value)>, String> {
        let v = self.get_json(&format!("/ticks?from={tick}"))?;
        let Some(rec) = v["records"].as_array().and_then(|r| {
            r.iter().find(|x| {
                x["tick"].as_u64() == Some(tick as u64)
                    && x["to"].as_u64() == Some(PHASE_COUNT as u64)
            })
        }) else {
            return Ok(None);
        };
        let bytes = from_hex(rec["input"].as_str().unwrap_or(""));
        let input = TickInput::try_from_slice(&bytes).map_err(|e| e.to_string())?;
        Ok(Some((input, rec.clone())))
    }
}

/// Host and port of a gateway URL like `http://127.0.0.1:4191` (plain HTTP,
/// an explicit port, no path).
pub fn parse_http_url(url: &str) -> Result<(String, u16), String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or("gateway URL must start with http://")?;
    let (host, port) = rest
        .trim_end_matches('/')
        .rsplit_once(':')
        .ok_or("gateway URL needs a port")?;
    if host.is_empty() || host.contains('/') {
        return Err("gateway URL: bad host".into());
    }
    Ok((host.to_string(), port.parse().map_err(|_| "bad port")?))
}

/// The body of a `Transfer-Encoding: chunked` response.
pub fn dechunk(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(end) = b[i..].windows(2).position(|w| w == b"\r\n") {
        let size =
            usize::from_str_radix(String::from_utf8_lossy(&b[i..i + end]).trim(), 16).unwrap_or(0);
        i += end + 2;
        if size == 0 || i + size > b.len() {
            break;
        }
        out.extend_from_slice(&b[i..i + size]);
        i += size + 2;
    }
    out
}
