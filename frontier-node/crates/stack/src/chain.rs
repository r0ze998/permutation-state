//! The stack's view of `frontier-localnet` over JSON-RPC: status, scale,
//! pause, holds, airdrops, operator transactions, the season and the
//! opened Provinces.

use std::future::Future;
use std::time::{Duration, Instant};

use fclient::decode::{Province, Season};
use fclient::ports::ChainPort;
use fclient::rpc::{RpcClient, RpcPort};
use fclient::{tx, Address, Instruction, Keypair};
use serde_json::{json, Value};

/// Every RPC call of the stack gives up after this long.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(20);

pub async fn timed<T>(f: impl Future<Output = T>) -> Result<T, String> {
    tokio::time::timeout(CALL_TIMEOUT, f)
        .await
        .map_err(|_| "RPC timed out".to_string())
}

/// `frontier_status`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Status {
    pub slot: u64,
    pub now: i64,
    pub scale: f64,
    pub paused: bool,
}

pub struct Chain {
    pub url: String,
    pub rpc: RpcClient,
    pub port: RpcPort,
}

impl Chain {
    pub fn new(url: &str, program: Address) -> Chain {
        Chain {
            url: url.to_string(),
            rpc: RpcClient::new(url),
            port: RpcPort::localnet(url, program),
        }
    }

    pub async fn call(&self, m: &str, p: Value) -> Result<Value, String> {
        timed(self.rpc.call(m, p))
            .await?
            .map_err(|e| format!("{m}: {e}"))
    }

    pub async fn status(&self) -> Result<Status, String> {
        let v = self.call("frontier_status", json!([])).await?;
        Ok(Status {
            slot: v["slot"].as_u64().unwrap_or(0),
            now: v["unixTimestamp"].as_i64().unwrap_or(0),
            scale: v["scale"].as_f64().unwrap_or(0.0),
            paused: v["paused"].as_bool().unwrap_or(false),
        })
    }

    /// Waits until the node answers `getHealth`.
    pub async fn wait_healthy(&self, limit: Duration) -> Result<(), String> {
        let t0 = Instant::now();
        loop {
            if let Ok(v) = self.call("getHealth", json!([])).await {
                if v == "ok" {
                    return Ok(());
                }
            }
            if t0.elapsed() > limit {
                return Err(format!(
                    "{} did not answer getHealth in {limit:?}",
                    self.url
                ));
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    pub async fn set_scale(&self, s: f64) -> Result<(), String> {
        self.call("frontier_setScale", json!([s])).await.map(|_| ())
    }
    pub async fn pause(&self) -> Result<(), String> {
        self.call("frontier_pause", json!([])).await.map(|_| ())
    }
    pub async fn resume(&self) -> Result<(), String> {
        self.call("frontier_resume", json!([])).await.map(|_| ())
    }

    /// `frontier_hold(keys, priority_milli, slots)` → the hold's JSON.
    pub async fn hold(
        &self,
        keys: &[Address],
        priority_milli: u64,
        slots: u64,
    ) -> Result<Value, String> {
        let ks: Vec<String> = keys.iter().map(|k| k.to_string()).collect();
        self.call("frontier_hold", json!([ks, priority_milli, slots]))
            .await
    }
    pub async fn holds(&self) -> Result<Value, String> {
        self.call("frontier_holds", json!([])).await
    }

    pub async fn airdrop(&self, k: &Address, lamports: u64) -> Result<(), String> {
        self.call("requestAirdrop", json!([k.to_string(), lamports]))
            .await
            .map(|_| ())
    }

    /// The slot of the program's newest transaction (the herald's fold is
    /// caught up when its `lastSlot` reaches it).
    pub async fn latest_program_slot(&self, program: &Address) -> Result<Option<u64>, String> {
        let v = self
            .call(
                "getSignaturesForAddress",
                json!([program.to_string(), {"limit": 1}]),
            )
            .await?;
        Ok(v.as_array()
            .and_then(|a| a.first())
            .and_then(|x| x["slot"].as_u64()))
    }

    pub async fn balance(&self, k: &Address) -> Result<u64, String> {
        let v = self.call("getBalance", json!([k.to_string()])).await?;
        Ok(v.get("value")
            .and_then(|x| x.as_u64())
            .or(v.as_u64())
            .unwrap_or(0))
    }

    pub async fn accounts(
        &self,
        keys: &[Address],
    ) -> Result<Vec<Option<fclient::ports::Account>>, String> {
        let mut out = Vec::with_capacity(keys.len());
        for c in keys.chunks(100) {
            out.extend(
                timed(self.port.accounts(c, 0))
                    .await?
                    .map_err(|e| format!("getMultipleAccounts: {e}"))?,
            );
        }
        Ok(out)
    }

    /// Sends one operator transaction and waits until it lands; an error
    /// carries the program's refusal and its logs.
    pub async fn send_op(
        &self,
        ixs: &[Instruction],
        signers: &[&Keypair],
        limit: Duration,
    ) -> Result<(), String> {
        let (bh, _) = timed(self.port.blockhash())
            .await?
            .map_err(|e| format!("blockhash: {e}"))?;
        let budget = tx::TxBudget {
            cu_limit: 1_400_000,
            cu_price: 0,
            loaded_limit: 4 * 1024 * 1024,
            heap: None,
        };
        let t = tx::build(ixs, &budget, signers, &bh)?;
        let sig = timed(self.port.send(&tx::wire(&t)))
            .await?
            .map_err(|e| format!("send: {e}"))?;
        let t0 = Instant::now();
        loop {
            let st = timed(self.port.statuses(&[sig]))
                .await?
                .map_err(|e| format!("statuses: {e}"))?;
            if let Some(Some(s)) = st.first() {
                return match &s.err {
                    None => Ok(()),
                    Some(e) => {
                        let logs = self
                            .call(
                                "getTransaction",
                                json!([sig.to_string(), {"encoding": "json"}]),
                            )
                            .await
                            .ok()
                            .and_then(|v| v["meta"]["logMessages"].as_array().cloned())
                            .unwrap_or_default();
                        Err(format!(
                            "operator transaction {sig} failed: {e} (code {:?})\n{}",
                            s.code,
                            logs.iter()
                                .filter_map(|l| l.as_str())
                                .collect::<Vec<_>>()
                                .join("\n")
                        ))
                    }
                };
            }
            if t0.elapsed() > limit {
                return Err(format!(
                    "operator transaction {sig} did not land in {limit:?}"
                ));
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    pub async fn season(&self, a: &fclient::addr::Addresses) -> Result<Option<Season>, String> {
        let got = self.accounts(&[a.season]).await?;
        match got.into_iter().next().flatten() {
            Some(acc) => Season::decode(&acc.data)
                .map(Some)
                .map_err(|e| format!("season: {e:?}")),
            None => Ok(None),
        }
    }

    /// Every Province of the program (`getProgramAccounts`, dataSize).
    pub async fn provinces(
        &self,
        program: &Address,
        season_id: u64,
    ) -> Result<Vec<Province>, String> {
        let v = self
            .call(
                "getProgramAccounts",
                json!([program.to_string(), {"encoding": "base64",
                    "filters": [{"dataSize": fclient::abi::size::PROVINCE}]}]),
            )
            .await?;
        let rows = v
            .as_array()
            .or_else(|| v.get("value").and_then(|x| x.as_array()))
            .cloned()
            .unwrap_or_default();
        let mut out = vec![];
        for r in rows {
            if let Ok(Some(a)) = fclient::rpc::parse_account(&r["account"]) {
                if let Ok(p) = Province::decode(&a.data) {
                    if p.h.season_id == season_id {
                        out.push(p);
                    }
                }
            }
        }
        Ok(out)
    }
}

/// Waits until `url` answers an HTTP GET with a status below 500.
pub async fn wait_http(url: &str, limit: Duration) -> Result<(), String> {
    let t0 = Instant::now();
    loop {
        if let Ok(Ok(r)) =
            tokio::time::timeout(Duration::from_secs(3), fclient::http::get(url)).await
        {
            if r.status < 500 {
                return Ok(());
            }
        }
        if t0.elapsed() > limit {
            return Err(format!("{url} did not answer in {limit:?}"));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Waits until something listens on 127.0.0.1:`port`.
pub async fn wait_tcp(port: u16, limit: Duration) -> Result<(), String> {
    let t0 = Instant::now();
    loop {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return Ok(());
        }
        if t0.elapsed() > limit {
            return Err(format!(
                "nothing listens on 127.0.0.1:{port} after {limit:?}"
            ));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}
