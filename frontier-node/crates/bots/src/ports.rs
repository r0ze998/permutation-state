//! What a bot talks to (M1 contract §8.6: "observations only from the
//! herald plus own accounts; every write through the relay"):
//!
//! - [`HeraldPort`]: the herald's files (`/h/*`), HTTP ([`HttpHerald`]) or a
//!   directory of recorded files ([`DirHerald`], the unit tests);
//! - [`RelayPort`]: the relay's public routes (`/f/*`), HTTP
//!   ([`HttpRelay`]); W4-F's in-process day implements it over the relay's
//!   logic or a stub;
//! - [`DirectPort`]: the local chain's RPC, only for the personas whose
//!   transactions the relay never sponsors (their own funded key):
//!   [`RpcDirect`] over `frontier-localnet`.
//!
//! Every port is async and `Send + Sync` so 1,000 bots share one of each.

use std::future::Future;
use std::path::PathBuf;

use fclient::http;
use fclient::ports::{ChainPort, PortError, SimResult};
use fclient::rpc::{RpcClient, RpcPort};
use fclient::{Address, Hash};
use serde_json::{json, Value};

pub type PortResult<T> = Result<T, PortError>;

/// An HTTP answer: status and JSON body (`Null` when the body is not JSON).
#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    pub status: u16,
    pub body: Value,
}

impl Answer {
    pub fn new(status: u16, body: Value) -> Answer {
        Answer { status, body }
    }

    /// 2xx and not `{ok: false}`.
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status) && self.body.get("ok") != Some(&Value::Bool(false))
    }

    /// The machine-readable refusal code (`code`), if any.
    pub fn code(&self) -> Option<String> {
        self.body
            .get("code")
            .and_then(|c| c.as_str())
            .map(String::from)
    }

    pub fn signature(&self) -> Option<String> {
        self.body
            .get("signature")
            .and_then(|c| c.as_str())
            .map(String::from)
    }

    fn from_http(r: http::Response) -> Answer {
        let body = serde_json::from_slice(&r.body).unwrap_or(Value::Null);
        Answer {
            status: r.status,
            body,
        }
    }
}

/// The herald's read path.
pub trait HeraldPort: Send + Sync {
    /// `GET path` (e.g. `/h/season`): the body, or `None` on 404.
    fn get(&self, path: &str) -> impl Future<Output = PortResult<Option<Vec<u8>>>> + Send;
}

/// The relay's public routes.
pub trait RelayPort: Send + Sync {
    fn get(&self, path: &str) -> impl Future<Output = PortResult<Answer>> + Send;
    fn post(&self, path: &str, body: &Value) -> impl Future<Output = PortResult<Answer>> + Send;
}

/// The local chain, for a persona's own transactions.
pub trait DirectPort: Send + Sync {
    fn blockhash(&self) -> impl Future<Output = PortResult<Hash>> + Send;
    fn simulate(&self, wire: &[u8]) -> impl Future<Output = PortResult<SimResult>> + Send;
    fn send(&self, wire: &[u8]) -> impl Future<Output = PortResult<String>> + Send;
    fn airdrop(&self, key: &Address, lamports: u64) -> impl Future<Output = PortResult<()>> + Send;
    fn balance(&self, key: &Address) -> impl Future<Output = PortResult<u64>> + Send;
    /// `frontier_hold(keys, priority_milli, slots)` (the contention emulator).
    fn hold(
        &self,
        keys: &[Address],
        priority_milli: u32,
        slots: u64,
    ) -> impl Future<Output = PortResult<()>> + Send;
}

// ------------------------------------------------------------------ HTTP

/// The herald over loopback HTTP.
#[derive(Clone, Debug)]
pub struct HttpHerald {
    pub base: String,
}

impl HttpHerald {
    pub fn new(base: impl Into<String>) -> HttpHerald {
        HttpHerald {
            base: base.into().trim_end_matches('/').to_string(),
        }
    }
}

impl HeraldPort for HttpHerald {
    async fn get(&self, path: &str) -> PortResult<Option<Vec<u8>>> {
        let r = http::get(&format!("{}{path}", self.base)).await?;
        match r.status {
            200 => Ok(Some(r.body)),
            404 => Ok(None),
            s => Err(PortError::Http(s)),
        }
    }
}

/// A directory of herald files: `/h/season` → `root/h/season.json`,
/// `/h/overview/2/latest.bin` → `root/h/overview/2/latest.bin`.
#[derive(Clone, Debug)]
pub struct DirHerald {
    pub root: PathBuf,
}

impl DirHerald {
    pub fn new(root: impl Into<PathBuf>) -> DirHerald {
        DirHerald { root: root.into() }
    }

    pub fn file_of(&self, path: &str) -> PathBuf {
        let p = path.trim_start_matches('/');
        if p.ends_with(".bin") {
            self.root.join(p)
        } else {
            self.root.join(format!("{p}.json"))
        }
    }
}

impl HeraldPort for DirHerald {
    async fn get(&self, path: &str) -> PortResult<Option<Vec<u8>>> {
        match std::fs::read(self.file_of(path)) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(PortError::Io(e.to_string())),
        }
    }
}

/// The relay's public listener over loopback HTTP.
#[derive(Clone, Debug)]
pub struct HttpRelay {
    pub base: String,
}

impl HttpRelay {
    pub fn new(base: impl Into<String>) -> HttpRelay {
        HttpRelay {
            base: base.into().trim_end_matches('/').to_string(),
        }
    }
}

impl RelayPort for HttpRelay {
    async fn get(&self, path: &str) -> PortResult<Answer> {
        Ok(Answer::from_http(
            http::get(&format!("{}{path}", self.base)).await?,
        ))
    }
    async fn post(&self, path: &str, body: &Value) -> PortResult<Answer> {
        Ok(Answer::from_http(
            http::post_json(&format!("{}{path}", self.base), body).await?,
        ))
    }
}

/// `frontier-localnet`'s JSON-RPC.
pub struct RpcDirect {
    pub port: RpcPort,
}

impl RpcDirect {
    pub fn new(url: impl Into<String>, program: Address) -> RpcDirect {
        RpcDirect {
            port: RpcPort::localnet(url, program),
        }
    }
    fn rpc(&self) -> &RpcClient {
        &self.port.rpc
    }
}

impl DirectPort for RpcDirect {
    async fn blockhash(&self) -> PortResult<Hash> {
        Ok(self.port.blockhash().await?.0)
    }
    async fn simulate(&self, wire: &[u8]) -> PortResult<SimResult> {
        self.port.simulate(wire).await
    }
    async fn send(&self, wire: &[u8]) -> PortResult<String> {
        Ok(self.port.send(wire).await?.to_string())
    }
    async fn airdrop(&self, key: &Address, lamports: u64) -> PortResult<()> {
        self.rpc().request_airdrop(key, lamports).await.map(|_| ())
    }
    async fn balance(&self, key: &Address) -> PortResult<u64> {
        self.rpc().get_balance(key).await
    }
    async fn hold(&self, keys: &[Address], priority_milli: u32, slots: u64) -> PortResult<()> {
        let ks: Vec<String> = keys.iter().map(|k| k.to_string()).collect();
        self.rpc()
            .call("frontier_hold", json!([ks, priority_milli, slots]))
            .await
            .map(|_| ())
    }
}

/// No direct port (`--rpc` not given): every call is `Unsupported`.
pub struct NoDirect;

impl DirectPort for NoDirect {
    async fn blockhash(&self) -> PortResult<Hash> {
        Err(PortError::Unsupported("no direct port"))
    }
    async fn simulate(&self, _: &[u8]) -> PortResult<SimResult> {
        Err(PortError::Unsupported("no direct port"))
    }
    async fn send(&self, _: &[u8]) -> PortResult<String> {
        Err(PortError::Unsupported("no direct port"))
    }
    async fn airdrop(&self, _: &Address, _: u64) -> PortResult<()> {
        Err(PortError::Unsupported("no direct port"))
    }
    async fn balance(&self, _: &Address) -> PortResult<u64> {
        Err(PortError::Unsupported("no direct port"))
    }
    async fn hold(&self, _: &[Address], _: u32, _: u64) -> PortResult<()> {
        Err(PortError::Unsupported("no direct port"))
    }
}
