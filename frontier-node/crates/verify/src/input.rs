//! What the verifier is given (contract §8.5): the program id, the season
//! id, the pinned drand public key, the ruleset hash expected, optionally
//! the `.so` hash expected; the season's transactions (feed order, failed
//! ones included) and the **final account states at a pinned slot**.
//!
//! Three sources give the same [`Input`]:
//!
//! - a **fixture file** (`frontier-node/fixtures/verify/*.json`): a
//!   recorded mini-season, the transactions in `findex`'s archive JSON
//!   form and the final accounts in RPC JSON form;
//! - a **findex archive** directory (`--archive`, a speed-up only) plus
//!   the final accounts fetched from RPC or read from a file;
//! - **RPC** alone (`--rpc`): the program's transactions through `findex`'s
//!   `RpcPoll` (or `frontier_feed` on a local node) and every account the
//!   season wrote read back at the newest slot.
//!
//! The final states are read, never trusted from the archive (K4): the
//! archive's post-states are used to replay, the finals to end every chain.

use std::collections::BTreeMap;
use std::path::Path;

use fclient::ports::{Account, TxRecord};
use fclient::rpc::{account_json, parse_account};
use serde_json::{json, Value};
use solana_address::Address;

use crate::world::Key;

/// Fixture file format tag.
pub const FIXTURE_FORMAT: &str = "frontier-verify-fixture-v1";

/// What the verifier is told to expect (never read from the chain).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub program: Address,
    pub season_id: u64,
    /// The pinned drand public key (compressed G2): quicknet's in
    /// production, the `test-beacon` key (I-53) for local runs.
    pub quicknet_pk: [u8; 96],
    /// `frontier_abi::presets::RULESET_HASH` of the release expected.
    pub ruleset_hash: [u8; 32],
    /// sha256 of the deployed `.so` expected (V2), if pinned.
    pub program_hash: Option<[u8; 32]>,
}

impl Config {
    pub fn to_json(&self) -> Value {
        json!({
            "program": self.program.to_string(),
            "season_id": self.season_id,
            "quicknet_pk": hex::encode(self.quicknet_pk),
            "ruleset_hash": hex::encode(self.ruleset_hash),
            "program_hash": self.program_hash.map(hex::encode),
        })
    }
    pub fn from_json(v: &Value) -> Result<Config, String> {
        let s = |k: &str| {
            v.get(k)
                .and_then(|x| x.as_str())
                .ok_or(format!("config.{k}"))
        };
        Ok(Config {
            program: s("program")?.parse().map_err(|_| "config.program")?,
            season_id: v
                .get("season_id")
                .and_then(|x| x.as_u64())
                .ok_or("config.season_id")?,
            quicknet_pk: hex_arr(s("quicknet_pk")?)?,
            ruleset_hash: hex_arr(s("ruleset_hash")?)?,
            program_hash: match v.get("program_hash").and_then(|x| x.as_str()) {
                Some(h) => Some(hex_arr(h)?),
                None => None,
            },
        })
    }
}

pub fn hex_arr<const N: usize>(s: &str) -> Result<[u8; N], String> {
    hex::decode(s)
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| format!("want {N} bytes"))
}

/// Everything one verification reads.
#[derive(Clone, Debug)]
pub struct Input {
    pub cfg: Config,
    pub txs: Vec<TxRecord>,
    pub finals: BTreeMap<Key, Option<Account>>,
    pub final_slot: u64,
    /// Observed `.so` hashes per slot range (`(from_slot, sha256)`), from
    /// the ProgramData account or the recording.
    pub program_hashes: Vec<(u64, [u8; 32])>,
    /// Free-form provenance of a fixture (report only).
    pub provenance: String,
    /// Honest-but-adverse scenarios the fixture claims to contain (checked
    /// by the fixture tests, not by the verifier).
    pub scenarios: Vec<String>,
}

impl Input {
    pub fn to_json(&self) -> Value {
        json!({
            "format": FIXTURE_FORMAT,
            "provenance": self.provenance,
            "scenarios": self.scenarios,
            "config": self.cfg.to_json(),
            "final_slot": self.final_slot,
            "program_hashes": self.program_hashes.iter().map(|(s, h)| json!([s, hex::encode(h)])).collect::<Vec<_>>(),
            "txs": self.txs.iter().map(findex::archive::record_to_json).collect::<Vec<_>>(),
            "finals": self.finals.iter().map(|(k, a)| json!([Address::new_from_array(*k).to_string(), a.as_ref().map(account_json)])).collect::<Vec<_>>(),
        })
    }

    pub fn from_json(v: &Value) -> Result<Input, String> {
        if v.get("format").and_then(|x| x.as_str()) != Some(FIXTURE_FORMAT) {
            return Err(format!("not a {FIXTURE_FORMAT} file"));
        }
        let cfg = Config::from_json(v.get("config").ok_or("config")?)?;
        let txs = v
            .get("txs")
            .and_then(|x| x.as_array())
            .ok_or("txs")?
            .iter()
            .map(findex::archive::record_from_json)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Input {
            cfg,
            txs,
            finals: finals_from_json(v.get("finals").ok_or("finals")?)?,
            final_slot: v.get("final_slot").and_then(|x| x.as_u64()).unwrap_or(0),
            program_hashes: v
                .get("program_hashes")
                .and_then(|x| x.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|p| {
                            Some((p.get(0)?.as_u64()?, hex_arr(p.get(1)?.as_str()?).ok()?))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            provenance: v
                .get("provenance")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .into(),
            scenarios: v
                .get("scenarios")
                .and_then(|x| x.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    pub fn load(path: &Path) -> Result<Input, String> {
        let s = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let v: Value = serde_json::from_str(&s).map_err(|e| format!("{}: {e}", path.display()))?;
        Input::from_json(&v)
    }

    /// Writes the fixture (one transaction per line, so diffs stay small).
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let v = self.to_json();
        let mut out = String::from("{\n");
        let obj = v.as_object().ok_or("object")?;
        let n = obj.len();
        for (i, (k, x)) in obj.iter().enumerate() {
            out.push_str(&format!("{}: ", Value::String(k.clone())));
            match x {
                Value::Array(a) if k == "txs" || k == "finals" => {
                    out.push_str("[\n");
                    for (j, e) in a.iter().enumerate() {
                        out.push_str(&e.to_string());
                        out.push_str(if j + 1 < a.len() { ",\n" } else { "\n" });
                    }
                    out.push(']');
                }
                _ => out.push_str(&x.to_string()),
            }
            out.push_str(if i + 1 < n { ",\n" } else { "\n" });
        }
        out.push_str("}\n");
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, out).map_err(|e| e.to_string())
    }
}

pub fn finals_from_json(v: &Value) -> Result<BTreeMap<Key, Option<Account>>, String> {
    let mut m = BTreeMap::new();
    for p in v.as_array().ok_or("finals")? {
        let k: Address = p
            .get(0)
            .and_then(|x| x.as_str())
            .and_then(|s| s.parse().ok())
            .ok_or("finals key")?;
        let a = parse_account(p.get(1).unwrap_or(&Value::Null)).map_err(|e| e.to_string())?;
        m.insert(k.to_bytes(), a);
    }
    Ok(m)
}

/// Every key a successful transaction of the archive wrote (the accounts
/// whose final state the verifier reads).
pub fn written_keys(txs: &[TxRecord]) -> Vec<Address> {
    let mut v: std::collections::BTreeSet<[u8; 32]> = Default::default();
    for t in txs.iter().filter(|t| t.err.is_none()) {
        if let Ok(tx) = fclient::tx::from_wire(&t.tx) {
            for (i, k) in tx.message.account_keys.iter().enumerate() {
                if fclient::tx::is_writable_index(&tx.message, i) {
                    v.insert(k.to_bytes());
                }
            }
        }
        for (k, _) in &t.post {
            v.insert(k.to_bytes());
        }
    }
    v.into_iter().map(Address::new_from_array).collect()
}

/// Reads a findex archive directory (`manifest.json` + segments).
pub fn read_archive(dir: &Path) -> Result<Vec<TxRecord>, String> {
    let a = findex::archive::Archive::open(dir, findex::archive::SEGMENT_BYTES)
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    a.verify()?;
    a.read_after(0).map_err(|e| e.to_string())
}

/// The final states of `keys` and the slot they were read at, over RPC.
pub async fn fetch_finals(
    url: &str,
    keys: &[Address],
) -> Result<(BTreeMap<Key, Option<Account>>, u64), String> {
    use fclient::ports::ChainPort;
    let port = fclient::rpc::RpcPort::new(url, Address::default());
    let slot = port.clock().await.map_err(|e| e.to_string())?.slot;
    let mut m = BTreeMap::new();
    for chunk in keys.chunks(100) {
        let got = port
            .accounts(chunk, slot)
            .await
            .map_err(|e| e.to_string())?;
        for (k, a) in chunk.iter().zip(got) {
            m.insert(k.to_bytes(), a);
        }
    }
    Ok((m, slot))
}

/// The program's transactions over RPC: `frontier_feed` on a local node,
/// else `getSignaturesForAddress` + `getTransaction` (findex `RpcPoll`).
pub async fn fetch_txs(
    url: &str,
    program: &Address,
    localnet: bool,
) -> Result<Vec<TxRecord>, String> {
    use findex::ingest::Source;
    let mut out = vec![];
    if localnet {
        let mut s =
            findex::ingest::LocalnetFeed::new(fclient::rpc::RpcPort::localnet(url, *program));
        loop {
            let b = s.pull().await.map_err(|e| e.to_string())?;
            if b.is_empty() {
                break;
            }
            out.extend(b);
        }
    } else {
        let mut s = findex::ingest::RpcPoll::new(url, *program);
        loop {
            let b = s.pull().await.map_err(|e| e.to_string())?;
            if b.is_empty() {
                break;
            }
            out.extend(b);
        }
    }
    Ok(out)
}
