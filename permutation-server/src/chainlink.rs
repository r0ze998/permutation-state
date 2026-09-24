//! Chain mode: the world comes from the on-chain program (through the local
//! gateway), and orders go back to it. The server keeps doing what only it
//! can do off chain — fog, previews, the decision ledger, bots — on the exact
//! bytes the program stores.
//!
//! The gateway is a local HTTP service (permutation-gateway); this is a
//! minimal HTTP/1.1 client for it, so the server needs no TLS or async stack.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use borsh::BorshDeserialize;
use permutation_rules::state::WorldState;
use permutation_rules::tick::TickInput;
use serde_json::Value;

/// Must match permutation-chain `state.rs`.
pub const WORLD_MAGIC: &[u8; 8] = b"PSWORLD5";
pub const WORLD_HEADER: usize = 8 + 4 + 64;

#[derive(Clone, Debug, Default)]
pub struct WorldMeta {
    pub season_id: u64,
    pub preset: u8,
    pub civs: u8,
    pub tick_seconds: u32,
    pub deadline: i64,
    pub finished: bool,
    pub market: bool,
}

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
}

impl ChainLink {
    /// `url` like `http://127.0.0.1:4191`.
    pub fn new(url: &str) -> Result<ChainLink, String> {
        let rest = url.strip_prefix("http://").ok_or("gateway URL must start with http://")?;
        let (host, port) = rest.trim_end_matches('/').split_once(':').ok_or("gateway URL needs a port")?;
        Ok(ChainLink { host: host.to_string(), port: port.parse().map_err(|_| "bad port")? })
    }

    pub fn url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }

    fn request(&self, method: &str, path: &str, body: Option<&str>) -> Result<Response, String> {
        let mut s = TcpStream::connect((self.host.as_str(), self.port)).map_err(|e| format!("gateway unreachable: {e}"))?;
        s.set_read_timeout(Some(Duration::from_secs(90))).ok();
        let body = body.unwrap_or("");
        let req = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.host,
            body.len()
        );
        s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
        let mut raw = Vec::new();
        s.read_to_end(&mut raw).map_err(|e| e.to_string())?;
        let split = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or("bad HTTP response")?;
        let head = String::from_utf8_lossy(&raw[..split]).to_string();
        let mut lines = head.lines();
        let status: u16 = lines.next().and_then(|l| l.split_whitespace().nth(1)).and_then(|c| c.parse().ok()).ok_or("bad status")?;
        let headers: Vec<(String, String)> = lines
            .filter_map(|l| l.split_once(':').map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string())))
            .collect();
        let mut body = raw[split + 4..].to_vec();
        if headers.iter().any(|(k, v)| k == "transfer-encoding" && v.contains("chunked")) {
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
            let logs = v["logs"].as_array().map(|l| l.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(" | ")).unwrap_or_default();
            return Err(format!("{} {}", v["error"].as_str().unwrap_or("gateway error"), logs));
        }
        Ok(v)
    }

    /// Solana JSON-RPC call (for a validator URL instead of the gateway).
    pub fn rpc(&self, method: &str, params: Value) -> Result<Value, String> {
        let body = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        let (_, _, bytes) = self.request("POST", "/", Some(&body.to_string()))?;
        let v: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if let Some(e) = v.get("error") {
            return Err(format!("{method}: {e}"));
        }
        Ok(v["result"].clone())
    }

    /// Raw account data through JSON-RPC (`None` if the account does not exist).
    pub fn account_data(&self, address: &str) -> Result<Option<Vec<u8>>, String> {
        let r = self.rpc("getAccountInfo", serde_json::json!([address, {"encoding": "base64", "commitment": "confirmed"}]))?;
        if r["value"].is_null() {
            return Ok(None);
        }
        Ok(Some(base64(r["value"]["data"][0].as_str().unwrap_or(""))?))
    }

    /// `Program data:` log records (sol_log_data) of a transaction.
    pub fn log_records(&self, signature: &str) -> Result<Vec<Vec<Vec<u8>>>, String> {
        let r = self.rpc("getTransaction", serde_json::json!([signature, {"encoding": "json", "commitment": "confirmed", "maxSupportedTransactionVersion": 0}]))?;
        let logs = r["meta"]["logMessages"].as_array().ok_or("transaction not found")?;
        logs.iter()
            .filter_map(|l| l.as_str()?.strip_prefix("Program data: "))
            .map(|l| l.split(' ').map(base64).collect::<Result<Vec<_>, _>>())
            .collect()
    }

    /// The world as the program stores it, or `None` while genesis runs.
    pub fn world(&self) -> Result<Option<Snapshot>, String> {
        let (code, headers, data) = self.request("GET", "/world.bin", None)?;
        if code != 200 {
            return Ok(None);
        }
        let h = |k: &str| headers.iter().find(|(x, _)| x == k).map(|(_, v)| v.clone()).unwrap_or_default();
        if data.len() < WORLD_HEADER || &data[..8] != WORLD_MAGIC {
            return Ok(None);
        }
        let len = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
        let m = &data[12..WORLD_HEADER];
        let meta = WorldMeta {
            season_id: u64::from_le_bytes(m[0..8].try_into().unwrap()),
            preset: m[8],
            civs: m[9],
            tick_seconds: u32::from_le_bytes(m[10..14].try_into().unwrap()),
            deadline: i64::from_le_bytes(m[14..22].try_into().unwrap()),
            finished: m[22] != 0,
            market: m[23] != 0,
        };
        let body = data.get(WORLD_HEADER..WORLD_HEADER + len).ok_or("world body truncated")?;
        let state = WorldState::try_from_slice(body).map_err(|e| format!("world decode: {e}"))?;
        Ok(Some(Snapshot { state, meta, slot: h("x-slot").parse().unwrap_or(0), layer: h("x-layer"), phase: h("x-phase") }))
    }

    /// The input the program resolved `tick` with (every office's batch and
    /// every governance action), from the PS_TICK record.
    pub fn resolved_input(&self, tick: u16) -> Result<Option<(TickInput, Value)>, String> {
        let v = self.get_json(&format!("/ticks?from={tick}"))?;
        let Some(rec) = v["records"].as_array().and_then(|r| r.iter().find(|x| x["tick"].as_u64() == Some(tick as u64) && x["to"].as_u64() == Some(12))) else {
            return Ok(None);
        };
        let bytes = from_hex(rec["input"].as_str().unwrap_or(""));
        let input = TickInput::try_from_slice(&bytes).map_err(|e| e.to_string())?;
        Ok(Some((input, rec.clone())))
    }
}

fn dechunk(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(end) = b[i..].windows(2).position(|w| w == b"\r\n") {
        let size = usize::from_str_radix(String::from_utf8_lossy(&b[i..i + end]).trim(), 16).unwrap_or(0);
        i += end + 2;
        if size == 0 || i + size > b.len() {
            break;
        }
        out.extend_from_slice(&b[i..i + size]);
        i += size + 2;
    }
    out
}

pub fn from_hex(h: &str) -> Vec<u8> {
    (0..h.len() / 2).filter_map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).ok()).collect()
}

/// Standard base64 (with padding) decoder.
pub fn base64(s: &str) -> Result<Vec<u8>, String> {
    let val = |c: u8| -> Result<u32, String> {
        Ok(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            _ => return Err(format!("bad base64 byte {c}")),
        })
    };
    let bytes: Vec<u8> = s.bytes().filter(|c| *c != b'=' && !c.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            n |= val(*c)? << (18 - 6 * i);
        }
        let take = chunk.len().saturating_sub(1);
        for i in 0..take {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    Ok(out)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_decodes_padding_variants() {
        assert_eq!(super::base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(super::base64("aGk=").unwrap(), b"hi");
        assert_eq!(super::base64("YWJj").unwrap(), b"abc");
        assert_eq!(super::base64("").unwrap(), b"");
    }
}
