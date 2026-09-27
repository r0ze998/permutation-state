//! Ingest backends (offchain design §4.1, §4.3): where the ordered stream of
//! program transactions comes from.
//!
//! - [`LocalnetFeed`] reads any [`ChainPort`]'s `feed` — `frontier_feed` on
//!   the local chain node, the transaction vector in process — with its
//!   post-state of written accounts.
//! - [`RpcPoll`] pages `getSignaturesForAddress(program)` back to the last
//!   signature it saw (`until`), then fetches each new transaction with
//!   `getTransaction`, oldest first. A public RPC gives no post-state, so
//!   snapshots and link addresses come from the logs alone. It stops at a
//!   transaction the node cannot return yet and resumes there next time.
//!   It also stops at a signature it has seen, so a server that ignores
//!   `until` still terminates; a server that ignores `before` (as the
//!   W1 local node MVP did; W2-C's node honours it) is detected when it serves a page twice and the pull fails
//!   with `Unsupported` rather than archive a gap — such a server must be
//!   polled before one page (≤ 1,000 transactions) fills.

use std::collections::{HashSet, VecDeque};
use std::future::Future;

use serde_json::{json, Value};
use solana_address::Address;

use fclient::ports::{ChainPort, Cursor, PortError, PortResult, Signature, TxRecord};
use fclient::rpc::RpcClient;

/// A source of program transactions in feed order.
pub trait Source: Send {
    /// Transactions after the cursor, oldest first; advances the cursor.
    fn pull(&mut self) -> impl Future<Output = PortResult<Vec<TxRecord>>> + Send;
    /// The cursor to store with the archived batch.
    fn cursor(&self) -> Value;
    /// Resumes from a stored cursor.
    fn restore(&mut self, v: &Value);
}

/// The local node's (or the in-process chain's) ordered feed.
pub struct LocalnetFeed<P: ChainPort> {
    pub port: P,
    pub after: Cursor,
}

impl<P: ChainPort> LocalnetFeed<P> {
    pub fn new(port: P) -> Self {
        LocalnetFeed {
            port,
            after: Cursor(0),
        }
    }
}

impl<P: ChainPort> Source for LocalnetFeed<P> {
    async fn pull(&mut self) -> PortResult<Vec<TxRecord>> {
        let mut out = vec![];
        loop {
            let got = self.port.feed(self.after).await?;
            let Some(last) = got.last() else { break };
            self.after = Cursor(last.seq);
            out.extend(got);
        }
        Ok(out)
    }
    fn cursor(&self) -> Value {
        json!({"feed_after": self.after.0})
    }
    fn restore(&mut self, v: &Value) {
        if let Some(a) = v.get("feed_after").and_then(|x| x.as_u64()) {
            self.after = Cursor(a);
        }
    }
}

/// `getSignaturesForAddress` + `getTransaction` over JSON-RPC.
pub struct RpcPoll {
    pub rpc: RpcClient,
    pub program: Address,
    /// Page size of `getSignaturesForAddress` (≤ 1,000).
    pub page: usize,
    last: Option<Signature>,
    seen: HashSet<Signature>,
    seen_order: VecDeque<Signature>,
}

const SEEN_KEEP: usize = 20_000;

impl RpcPoll {
    pub fn new(url: impl Into<String>, program: Address) -> RpcPoll {
        RpcPoll {
            rpc: RpcClient::new(url),
            program,
            page: 1_000,
            last: None,
            seen: HashSet::new(),
            seen_order: VecDeque::new(),
        }
    }

    fn remember(&mut self, s: Signature) {
        if self.seen.insert(s) {
            self.seen_order.push_back(s);
            if self.seen_order.len() > SEEN_KEEP {
                if let Some(old) = self.seen_order.pop_front() {
                    self.seen.remove(&old);
                }
            }
        }
    }
}

impl Source for RpcPoll {
    async fn pull(&mut self) -> PortResult<Vec<TxRecord>> {
        // Newest first, back to the last signature seen.
        let mut newest_first = vec![];
        let mut collected: HashSet<Signature> = HashSet::new();
        let mut before: Option<Signature> = None;
        loop {
            let page = self
                .rpc
                .get_signatures_for_address(
                    &self.program,
                    before.as_ref(),
                    self.last.as_ref(),
                    self.page,
                )
                .await?;
            let n = page.len();
            let mut stop = n == 0;
            for s in page {
                if Some(s.signature) == self.last || self.seen.contains(&s.signature) {
                    stop = true;
                    break;
                }
                if !collected.insert(s.signature) {
                    // The server ignored `before` and served a page again:
                    // paging on would loop, stopping would leave a gap.
                    return Err(PortError::Unsupported(
                        "getSignaturesForAddress ignored `before`: cannot page without a gap",
                    ));
                }
                newest_first.push(s);
            }
            if stop || n < self.page {
                break;
            }
            let next = newest_first.last().map(|s| s.signature);
            if next == before {
                break;
            }
            before = next;
        }
        let mut out = vec![];
        for s in newest_first.into_iter().rev() {
            match self.rpc.get_transaction(&s.signature).await? {
                Some(r) => {
                    self.last = Some(s.signature);
                    self.remember(s.signature);
                    out.push(r);
                }
                // Not retrievable yet: resume here next time.
                None => break,
            }
        }
        Ok(out)
    }
    fn cursor(&self) -> Value {
        json!({"rpc_last": self.last.map(|s| s.to_string())})
    }
    fn restore(&mut self, v: &Value) {
        self.last = v
            .get("rpc_last")
            .and_then(|x| x.as_str())
            .and_then(|s| s.parse().ok());
        if let Some(s) = self.last {
            self.remember(s);
        }
    }
}

/// A source whose transactions lack post-state (a public RPC) with the
/// written accounts fetched **before archiving** (offchain design §8.2
/// step 2, W3-D): for each landed transaction without post-state, the
/// message's writable accounts are read with `getMultipleAccounts
/// (minContextSlot = the transaction's slot)`. The bytes are "state at a
/// slot ≥ s" (a later write can show through); each chained account's
/// `(event seq, head)` says which version it is. Because the fetched
/// post-state is archived with the transaction, every later fold of the
/// archive sees the same bytes (the herald's determinism).
pub struct Enriched<S: Source> {
    pub inner: S,
    pub rpc: RpcClient,
    /// Keys never fetched (program ids, sysvars).
    pub skip: HashSet<Address>,
}

impl<S: Source> Enriched<S> {
    pub fn new(inner: S, url: impl Into<String>, program: Address) -> Enriched<S> {
        let mut skip = HashSet::new();
        skip.insert(program);
        skip.insert(fclient::addr::system_program());
        Enriched {
            inner,
            rpc: RpcClient::new(url),
            skip,
        }
    }

    /// The writable accounts of a wire transaction (fee payer excluded).
    pub fn writable_keys(&self, wire: &[u8]) -> Vec<Address> {
        let Ok(t) = fclient::tx::from_wire(wire) else {
            return vec![];
        };
        let m = &t.message;
        (1..m.account_keys.len())
            .filter(|&i| fclient::tx::is_writable_index(m, i))
            .map(|i| m.account_keys[i])
            .filter(|k| !self.skip.contains(k))
            .collect()
    }
}

impl<S: Source> Source for Enriched<S> {
    async fn pull(&mut self) -> PortResult<Vec<TxRecord>> {
        let mut got = self.inner.pull().await?;
        for r in got.iter_mut() {
            if r.err.is_some() || !r.post.is_empty() {
                continue;
            }
            let keys = self.writable_keys(&r.tx);
            if keys.is_empty() {
                continue;
            }
            let accts = self.rpc.get_multiple_accounts(&keys, r.slot).await?;
            r.post = keys.into_iter().zip(accts).collect();
        }
        Ok(got)
    }
    fn cursor(&self) -> Value {
        self.inner.cursor()
    }
    fn restore(&mut self, v: &Value) {
        self.inner.restore(v)
    }
}

/// An error of either the source or the store.
#[derive(Debug)]
pub enum IngestError {
    Port(PortError),
    Store(String),
}

impl std::fmt::Display for IngestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for IngestError {}
