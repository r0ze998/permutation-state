//! The season as the verifier reads it: every archived transaction, its
//! message (account keys, writability, the program's instructions), the
//! PS2 records **the program itself printed** (`fclient::log::
//! bodies_from_logs`, frame-checked) decoded with `frontier-abi::log`, and
//! the post-states it wrote (from `localnet` or a fixture; empty from a
//! public RPC).
//!
//! Nothing here judges anything: a transaction that cannot be decoded, or
//! a record that does not decode, is kept as a [`Problem`] and becomes a
//! FAIL in V1 (fail closed, K2).

use std::collections::BTreeMap;

use fclient::ports::{Account, TxRecord};
use frontier_abi::log::{self as plog, EntityKind, Kind, Link};
use solana_address::Address;

pub type Key = [u8; 32];

/// One program instruction of a transaction.
#[derive(Clone, Debug)]
pub struct Ix {
    /// Index of the instruction in the message.
    pub index: usize,
    pub data: Vec<u8>,
    /// Account keys in instruction order, with writability.
    pub accounts: Vec<(Key, bool)>,
}

impl Ix {
    pub fn tag(&self) -> Option<u8> {
        self.data.first().copied()
    }
    pub fn key(&self, i: usize) -> Option<Key> {
        self.accounts.get(i).map(|a| a.0)
    }
}

/// A decoded record, owned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rec {
    pub kind: Kind,
    pub bell: u32,
    pub key: Vec<u8>,
    pub payload: Vec<u8>,
    pub bwt: Vec<u8>,
    pub links: Vec<Link>,
    /// Index of the transaction in [`World::txs`].
    pub tx: usize,
    /// Position of the record in its transaction.
    pub pos: usize,
}

impl Rec {
    fn field(&self, name: &str, in_payload: bool) -> &[u8] {
        let part = if in_payload { &self.payload } else { &self.key };
        match plog::field(self.kind, name, in_payload) {
            Some((o, w)) => part.get(o..o + w).unwrap_or(&[]),
            None => &[],
        }
    }
    /// A payload field's bytes (empty when the kind has no such field).
    pub fn p(&self, name: &str) -> &[u8] {
        self.field(name, true)
    }
    /// A key field's bytes.
    pub fn k(&self, name: &str) -> &[u8] {
        self.field(name, false)
    }
    pub fn pu8(&self, n: &str) -> u8 {
        self.p(n).first().copied().unwrap_or(0)
    }
    pub fn pu16(&self, n: &str) -> u16 {
        le(self.p(n)) as u16
    }
    pub fn pu32(&self, n: &str) -> u32 {
        le(self.p(n)) as u32
    }
    pub fn pu64(&self, n: &str) -> u64 {
        le(self.p(n))
    }
    pub fn pi64(&self, n: &str) -> i64 {
        le(self.p(n)) as i64
    }
    pub fn pi32(&self, n: &str) -> i32 {
        le(self.p(n)) as u32 as i32
    }
    pub fn p32(&self, n: &str) -> [u8; 32] {
        self.p(n).try_into().unwrap_or([0; 32])
    }
    pub fn ku8(&self, n: &str) -> u8 {
        self.k(n).first().copied().unwrap_or(0)
    }
    pub fn ku32(&self, n: &str) -> u32 {
        le(self.k(n)) as u32
    }
    pub fn ku64(&self, n: &str) -> u64 {
        le(self.k(n))
    }
    pub fn ki32(&self, n: &str) -> i32 {
        le(self.k(n)) as u32 as i32
    }
    /// `(P, Q)` of a key that starts with them.
    pub fn pq(&self) -> (i32, i32) {
        (self.ki32("p"), self.ki32("q"))
    }
    /// `(P, Q, site)` of a holding key.
    pub fn pqs(&self) -> (i32, i32, u8) {
        (self.ki32("p"), self.ki32("q"), self.ku8("site"))
    }
    /// The whole body (with the tail), as the program printed it.
    pub fn encode(&self) -> Vec<u8> {
        let mut v = self.bwt.clone();
        v.push(self.links.len() as u8);
        for l in &self.links {
            v.push(l.entity as u8);
            v.extend_from_slice(&l.seq.to_le_bytes());
            v.extend_from_slice(&l.head);
        }
        v
    }
}

/// Little-endian integer of up to 8 bytes.
pub fn le(b: &[u8]) -> u64 {
    let mut v = [0u8; 8];
    let n = b.len().min(8);
    v[..n].copy_from_slice(&b[..n]);
    u64::from_le_bytes(v)
}

/// Something the verifier could not read (a FAIL: K2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub tx: usize,
    pub what: String,
}

/// One archived transaction.
#[derive(Clone, Debug)]
pub struct Tx {
    pub seq: u64,
    pub slot: u64,
    pub time: i64,
    pub sig: String,
    pub ok: bool,
    pub code: Option<u32>,
    pub fee_payer: Option<Key>,
    /// Every key of the message with its writability.
    pub keys: Vec<(Key, bool)>,
    /// The program's instructions (top level).
    pub ixs: Vec<Ix>,
    /// Records in log order (successful transactions only).
    pub recs: Vec<usize>,
    pub post: BTreeMap<Key, Option<Account>>,
    /// Compute-budget fields (evidence).
    pub cu_price: u64,
    pub cu_limit: u32,
    pub loaded_limit: u32,
}

impl Tx {
    pub fn post_data(&self, k: &Key) -> Option<&[u8]> {
        self.post
            .get(k)
            .and_then(|a| a.as_ref())
            .map(|a| a.data.as_slice())
    }
    /// The first program instruction with this tag.
    pub fn ix(&self, tag: u8) -> Option<&Ix> {
        self.ixs.iter().find(|i| i.tag() == Some(tag))
    }
    pub fn writable(&self, k: &Key) -> bool {
        self.keys.iter().any(|(x, w)| x == k && *w)
    }
}

/// The whole input, parsed.
#[derive(Clone, Debug, Default)]
pub struct World {
    pub program: Key,
    pub txs: Vec<Tx>,
    pub recs: Vec<Rec>,
    pub problems: Vec<Problem>,
    /// Signatures seen more than once (`DuplicateEvent`).
    pub duplicate_sigs: Vec<(usize, String)>,
    pub finals: BTreeMap<Key, Option<Account>>,
    pub final_slot: u64,
    /// Whether post-states were supplied (localnet, fixtures).
    pub has_post: bool,
    /// Per account, the successful transactions that carry its post-state
    /// (ascending; [`World::state_before`] bisects it).
    pub posts: std::collections::HashMap<Key, Vec<usize>>,
}

impl World {
    pub fn parse(
        program: &Address,
        txs: &[TxRecord],
        finals: BTreeMap<Key, Option<Account>>,
        final_slot: u64,
    ) -> World {
        let mut w = World {
            program: program.to_bytes(),
            finals,
            final_slot,
            ..World::default()
        };
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        for (i, t) in txs.iter().enumerate() {
            let sig = t.signature.to_string();
            let dup = seen.insert(sig.clone(), i).is_some();
            if dup {
                w.duplicate_sigs.push((i, sig.clone()));
            }
            let mut tx = Tx {
                seq: t.seq,
                slot: t.slot,
                time: t.block_time,
                sig,
                ok: t.err.is_none(),
                code: t.code,
                fee_payer: None,
                keys: vec![],
                ixs: vec![],
                recs: vec![],
                post: t
                    .post
                    .iter()
                    .map(|(k, a)| (k.to_bytes(), a.clone()))
                    .collect(),
                cu_price: 0,
                cu_limit: 0,
                loaded_limit: 0,
            };
            if !t.post.is_empty() {
                w.has_post = true;
            }
            match fclient::tx::from_wire(&t.tx) {
                Ok(parsed) => {
                    let msg = &parsed.message;
                    tx.keys = msg
                        .account_keys
                        .iter()
                        .enumerate()
                        .map(|(k, a)| (a.to_bytes(), fclient::tx::is_writable_index(msg, k)))
                        .collect();
                    tx.fee_payer = tx.keys.first().map(|k| k.0);
                    let b = fclient::tx::parse_budget(msg);
                    tx.cu_price = b.cu_price.unwrap_or(0);
                    tx.cu_limit = b.cu_limit.unwrap_or(0);
                    tx.loaded_limit = b.loaded_limit.unwrap_or(0);
                    for (n, ci) in msg.instructions.iter().enumerate() {
                        let pid = msg.account_keys.get(ci.program_id_index as usize);
                        if pid.map(|p| p.to_bytes()) != Some(w.program) {
                            continue;
                        }
                        tx.ixs.push(Ix {
                            index: n,
                            data: ci.data.clone(),
                            accounts: ci
                                .accounts
                                .iter()
                                .filter_map(|&a| tx.keys.get(a as usize).copied())
                                .collect(),
                        });
                    }
                }
                Err(e) => w.problems.push(Problem {
                    tx: i,
                    what: format!("transaction bytes do not decode: {e}"),
                }),
            }
            // A transaction the archive lists twice counts once (V1 reports
            // the duplicate).
            if dup {
                tx.ok = false;
                tx.post.clear();
            }
            if tx.ok {
                match fclient::log::bodies_from_logs(&t.logs, program) {
                    Ok(bodies) => {
                        for (pos, b) in bodies.iter().enumerate() {
                            match plog::decode(b) {
                                Ok(r) => {
                                    let n = w.recs.len();
                                    w.recs.push(Rec {
                                        kind: r.kind,
                                        bell: r.bell,
                                        key: r.key.to_vec(),
                                        payload: r.payload.to_vec(),
                                        bwt: r.body_without_tail.to_vec(),
                                        links: r
                                            .links
                                            .iter()
                                            .take(r.n_links)
                                            .flatten()
                                            .copied()
                                            .collect(),
                                        tx: i,
                                        pos,
                                    });
                                    tx.recs.push(n);
                                }
                                Err(e) => w.problems.push(Problem {
                                    tx: i,
                                    what: format!("record {pos} does not decode: {e:?}"),
                                }),
                            }
                        }
                    }
                    Err(e) => w.problems.push(Problem {
                        tx: i,
                        what: format!("log lines do not decode: {e:?}"),
                    }),
                }
            }
            if tx.ok {
                for k in tx.post.keys() {
                    w.posts.entry(*k).or_default().push(i);
                }
            }
            w.txs.push(tx);
        }
        w
    }

    /// Records of one kind, in order.
    pub fn of(&self, kind: Kind) -> impl Iterator<Item = &Rec> {
        self.recs.iter().filter(move |r| r.kind == kind)
    }

    pub fn tx_of(&self, r: &Rec) -> &Tx {
        &self.txs[r.tx]
    }

    /// The post-state of `k` written by the last successful transaction
    /// before `tx` (exclusive) that wrote it.
    pub fn state_before(&self, k: &Key, tx: usize) -> Option<&[u8]> {
        let v = self.posts.get(k)?;
        let i = v.partition_point(|&t| t < tx);
        let t = *v.get(i.checked_sub(1)?)?;
        self.txs[t]
            .post
            .get(k)
            .and_then(|a| a.as_ref())
            .map(|a| a.data.as_slice())
    }

    /// The post-state of `k` after transaction `tx` (inclusive: its own
    /// post if it wrote `k`, else the last earlier one).
    pub fn state_after(&self, k: &Key, tx: usize) -> Option<&[u8]> {
        self.state_before(k, tx + 1)
    }

    /// The last post-state of `k` in the whole archive.
    pub fn last_post(&self, k: &Key) -> Option<Option<&Account>> {
        let t = *self.posts.get(k)?.last()?;
        self.txs[t].post.get(k).map(|a| a.as_ref())
    }

    pub fn entity_name(e: EntityKind) -> &'static str {
        match e {
            EntityKind::Season => "Season",
            EntityKind::Frontier => "Frontier",
            EntityKind::JoinShard => "JoinShard",
            EntityKind::Citizen => "Citizen",
            EntityKind::Holding => "Holding",
            EntityKind::Province => "Province",
            EntityKind::ClashInputs => "ClashInputs",
        }
    }
}

/// Base58 of a key (report entities).
pub fn b58(k: &Key) -> String {
    Address::new_from_array(*k).to_string()
}
