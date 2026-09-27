//! The testability spine (M1 contract §8.1): `ChainPort` and `DrandPort`.
//! Keeper, herald, bots, verifier and the in-process tests all talk to the
//! chain and to drand through these two traits, so the same code runs over
//! a real RPC, the local chain node (`localnet`, HTTP) and LiteSVM in
//! process (`localnet::InProcess`, which implements `ChainPort` directly —
//! it lives in the `localnet` crate to keep `fclient` free of LiteSVM).
//!
//! Every method returns a `Result`: the contract's sketch leaves errors
//! implicit, and a keeper must tell "not landed" from "RPC down".

use std::future::Future;

use solana_address::Address;
use solana_hash::Hash;
pub use solana_signature::Signature;

/// The Clock sysvar (40 B).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClockSysvar {
    pub slot: u64,
    pub epoch_start_timestamp: i64,
    pub epoch: u64,
    pub leader_schedule_epoch: u64,
    pub unix_timestamp: i64,
}

impl ClockSysvar {
    pub fn decode(d: &[u8]) -> Option<ClockSysvar> {
        if d.len() < 40 {
            return None;
        }
        let u = |o: usize| u64::from_le_bytes(d[o..o + 8].try_into().expect("8"));
        Some(ClockSysvar {
            slot: u(0),
            epoch_start_timestamp: u(8) as i64,
            epoch: u(16),
            leader_schedule_epoch: u(24),
            unix_timestamp: u(32) as i64,
        })
    }
    pub fn encode(&self) -> [u8; 40] {
        let mut d = [0u8; 40];
        d[0..8].copy_from_slice(&self.slot.to_le_bytes());
        d[8..16].copy_from_slice(&self.epoch_start_timestamp.to_le_bytes());
        d[16..24].copy_from_slice(&self.epoch.to_le_bytes());
        d[24..32].copy_from_slice(&self.leader_schedule_epoch.to_le_bytes());
        d[32..40].copy_from_slice(&self.unix_timestamp.to_le_bytes());
        d
    }
}

/// An account as the ports return it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub lamports: u64,
    pub data: Vec<u8>,
    pub owner: Address,
    pub executable: bool,
}

/// Result of a simulation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SimResult {
    /// `None` when the transaction would succeed.
    pub err: Option<String>,
    /// The program's custom error code, when the error is one.
    pub code: Option<u32>,
    pub logs: Vec<String>,
    pub units: u64,
    /// Post-state of the transaction's accounts, in message key order.
    pub accounts: Vec<(Address, Option<Account>)>,
}

impl SimResult {
    /// Post balance of `k`.
    pub fn post_balance(&self, k: &Address) -> Option<u64> {
        self.accounts
            .iter()
            .find(|(a, _)| a == k)
            .and_then(|(_, a)| a.as_ref().map(|x| x.lamports))
    }
}

/// A landed (or failed-and-charged) transaction's status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub slot: u64,
    pub err: Option<String>,
    pub code: Option<u32>,
}

/// Position in a transaction feed: the feed sequence number of the last
/// record seen (0 = from the start).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cursor(pub u64);

/// One transaction of the program, failed ones included, in slot order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TxRecord {
    /// Feed sequence (1, 2, …).
    pub seq: u64,
    pub slot: u64,
    pub signature: Signature,
    /// Clock `unix_timestamp` of the slot (game time on localnet).
    pub block_time: i64,
    /// Legacy wire bytes.
    pub tx: Vec<u8>,
    pub logs: Vec<String>,
    pub err: Option<String>,
    pub code: Option<u32>,
    pub units: u64,
    pub fee: u64,
    /// Post-transaction bytes of the accounts it wrote (localnet only;
    /// empty from a public RPC).
    pub post: Vec<(Address, Option<Account>)>,
}

/// A drand round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Beacon {
    pub round: u64,
    pub sig48: [u8; 48],
}

/// drand chain info.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainInfo {
    pub public_key: [u8; 96],
    pub period: u32,
    pub genesis_time: i64,
    pub genesis_seed: [u8; 32],
    pub chain_hash: [u8; 32],
    pub scheme: String,
    pub beacon_id: String,
}

impl ChainInfo {
    /// `round_time(r) = genesis + (r − 1) × period`.
    pub fn round_time(&self, r: u64) -> i64 {
        self.genesis_time + (r.saturating_sub(1) as i64) * self.period as i64
    }
    /// The first round scheduled at or after `t`.
    pub fn first_round_from(&self, t: i64) -> u64 {
        crate::clock::first_round_from(self.genesis_time, self.period, t)
    }
}

/// Port errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PortError {
    /// Transport failure (connection, timeout).
    Io(String),
    /// Non-2xx HTTP status.
    Http(u16),
    /// JSON-RPC error object.
    Rpc { code: i64, message: String },
    /// Unexpected response shape.
    Decode(String),
    /// The backend cannot serve this call.
    Unsupported(&'static str),
    /// A beacon that does not verify against the pinned key.
    BadBeacon(u64),
    /// Nothing reachable.
    Unavailable(String),
    /// A transaction refused before execution (sanitize, signature, blockhash).
    Rejected(String),
}

impl std::fmt::Display for PortError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PortError {}

pub type PortResult<T> = Result<T, PortError>;

/// The chain as every Frontier component sees it.
pub trait ChainPort: Send + Sync {
    /// Clock sysvar (slot, unix_timestamp).
    fn clock(&self) -> impl Future<Output = PortResult<ClockSysvar>> + Send;
    /// Accounts at a context slot ≥ `min_slot` (`None` = absent).
    fn accounts(
        &self,
        keys: &[Address],
        min_slot: u64,
    ) -> impl Future<Output = PortResult<Vec<Option<Account>>>> + Send;
    /// Simulates signed wire bytes: logs, CU and the accounts' post-state.
    fn simulate(&self, tx: &[u8]) -> impl Future<Output = PortResult<SimResult>> + Send;
    /// Sends signed wire bytes.
    fn send(&self, tx: &[u8]) -> impl Future<Output = PortResult<Signature>> + Send;
    fn statuses(
        &self,
        sigs: &[Signature],
    ) -> impl Future<Output = PortResult<Vec<Option<Status>>>> + Send;
    /// Program transactions (incl. failed) after `after`, in slot order.
    fn feed(&self, after: Cursor) -> impl Future<Output = PortResult<Vec<TxRecord>>> + Send;
    fn blockhash(&self) -> impl Future<Output = PortResult<(Hash, u64)>> + Send;
}

/// drand as every Frontier component sees it.
pub trait DrandPort: Send + Sync {
    /// The round if published (and, for drand-replay, if its time has come
    /// on the game clock); `Ok(None)` = not yet.
    fn round(&self, r: u64) -> impl Future<Output = PortResult<Option<Beacon>>> + Send;
    fn info(&self) -> ChainInfo;
}

/// Parses `custom program error: 0x…` / `Custom(n)` out of an error string.
pub fn custom_code(err: &str) -> Option<u32> {
    if let Some(s) = err.split("Custom(").nth(1) {
        return s.split(')').next()?.trim().parse().ok();
    }
    if let Some(s) = err.split("custom program error: 0x").nth(1) {
        let h: String = s.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        return u32::from_str_radix(&h, 16).ok();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_and_codes() {
        let c = ClockSysvar {
            slot: 5,
            epoch_start_timestamp: -1,
            epoch: 2,
            leader_schedule_epoch: 3,
            unix_timestamp: 1_800_000_000,
        };
        assert_eq!(ClockSysvar::decode(&c.encode()), Some(c));
        assert_eq!(custom_code("InstructionError(3, Custom(35))"), Some(35));
        assert_eq!(
            custom_code("Program failed: custom program error: 0x33"),
            Some(51)
        );
        assert_eq!(custom_code("AccountNotFound"), None);
    }
}
