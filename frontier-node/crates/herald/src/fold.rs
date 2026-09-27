//! The fold (contract §8.4; offchain design §8.2): archived program
//! transactions in, per-bell files and WS diffs out.
//!
//! **Input.** [`Fold::apply`] takes the archive's [`TxRecord`]s in archive
//! order. A failed transaction changes nothing (its logged records are not
//! events). A landed one contributes its **post-state** (the exact bytes of
//! every account it wrote: the local node's feed carries them, and
//! `findex::Enriched` fetches them for a public RPC before archiving) and its
//! PS2 records.
//!
//! **Captures.** Every program account of the season is kept as a
//! [`Capture`] `(slot, archive seq, bytes)`; chained accounts also carry
//! `(event seq, event head)` in their header, so each capture is
//! identifiable and verifiable against the chain.
//!
//! **Per-bell closing.** When a Province's `resolved_next` advances from
//! `x` to `y` (ResolveFromInputs or SkipQuiet), each bell `b ∈ [x, y)`
//! closes: `province/{P},{Q}/{b}` (the Province bytes after that
//! transaction, and the province-bell's ArrivalSlots, ArrivalDay and
//! ClashInputs as captured then, before any of them closes). A CLASH record
//! writes `clash/{P},{Q}/{b}` (recomputed, [`crate::clash`]). When every
//! province of ring `d` opened by bell `b` has resolved `b`, the fold
//! writes `overview/{d}/{b}.bin` from each province's bytes **as of its own
//! resolve of `b`**. A region's `bell/{b}/region/{r}` is written once THE
//! anchor and a seed of `S(b, r)` exist, and rewritten while its resolved
//! list, tombstone and archive state change (it is `final` once archived and
//! every province of the region resolved `b`).
//!
//! **Determinism.** The fold reads nothing but the records (no wall clock,
//! no randomness; ordered maps; fixed JSON), so the same archive gives
//! byte-identical files ([`crate::files`]). Its whole state is
//! [`State`], which [`crate::checkpoint`] saves: a restart loads the
//! checkpoint and re-folds the records after it; rewrites of the same bytes
//! are no-ops.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use solana_address::Address;

use fclient::decode::{chained_header, AnchorArchive, Province, Season};
use fclient::log::bodies_from_logs;
use fclient::ports::TxRecord;
use frontier_abi::addr::AddrCtx;
use frontier_abi::layout::AccountKind;
use frontier_abi::log::{self as plog, Kind};
use permutation_rules::frontier::beacon;
use permutation_rules::frontier::clash::BeaconClock;

use crate::clash::{fighters_json, ClashBuilder};
use crate::files::{Out, Written};
use crate::overview;
use crate::records::{self, B64};

/// What the fold needs to know besides the records.
#[derive(Clone)]
pub struct FoldCfg {
    pub program: Address,
    pub season_id: u64,
    pub builder: Arc<dyn ClashBuilder>,
    /// The archive's post-state is the exact bytes each transaction wrote
    /// (the local node's feed). A public RPC gives "state at a slot ≥ s"
    /// (`findex::Enriched`): a recomputed clash that differs is then
    /// reported `unchecked`, not `MISMATCH`.
    pub exact_post: bool,
}

/// One account's bytes as written by the transaction at archive `tx`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Capture {
    pub slot: u64,
    pub tx: u64,
    pub data: Vec<u8>,
}

impl Capture {
    /// `(event seq, event head)` of a chained account.
    pub fn head(&self) -> Option<(u64, [u8; 32])> {
        chained_header(&self.data).map(|h| (h.event_seq, h.event_head))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeedRec {
    pub round: u64,
    pub seed: [u8; 32],
    pub a: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SealRec {
    pub outcome: u8,
    pub code: u8,
    pub bell: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BrState {
    pub hash: [u8; 32],
    pub fin: bool,
}

/// Alarms the herald reports (`/h/status`, logs); never fatal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Alarms {
    /// A per-bell (immutable) file rewritten with different bytes.
    pub rewrites: u64,
    /// A PS2 body that did not decode.
    pub bad_records: u64,
    /// A clash whose recomputed digest differs from the logged one.
    pub clash_mismatch: u64,
    /// A clash the builder could not recompute.
    pub clash_unchecked: u64,
    /// A file write that failed (I/O).
    pub write_errors: u64,
}

/// The fold's whole state (what a checkpoint holds).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    /// The archive sequence folded through.
    pub folded_through: u64,
    /// PS2 records of landed transactions folded (= `/h/events` numbering).
    pub events: u64,
    pub last_slot: u64,
    pub last_time: i64,
    /// Present program accounts of the season.
    pub accounts: BTreeMap<[u8; 32], Capture>,
    /// Province bytes at the resolve of `(P, Q, bell)`, until the ring's
    /// overview of that bell is written.
    pub pending: BTreeMap<(i16, i16, u32), Capture>,
    /// Next overview bell per ring.
    pub ov_next: BTreeMap<u16, u32>,
    /// Province-bells with a CLASH record.
    pub clashes: BTreeSet<(i16, i16, u32)>,
    /// SEED records: `(bell, region, nonce)`.
    pub seeds: BTreeMap<(u32, u8, u8), SeedRec>,
    /// Last-known BellAnchor bytes per `(bell, region)` (kept after close).
    pub anchors: BTreeMap<(u32, u8), Capture>,
    /// Bell-region files written: hash and finality.
    pub br: BTreeMap<(u32, u8), BrState>,
    /// TRANSIT_SETTLED per host.
    pub seals: BTreeMap<u64, SealRec>,
    /// Ring seeds from RING_OPEN (genesis rings) and RING_SEED.
    pub ring_seeds: BTreeMap<u16, [u8; 32]>,
    pub alarms: Alarms,
}

/// Where a WS diff goes (`crate::ws`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    Province(i32, i32),
    Ring(u16),
    /// A wallet's own entities (its Citizen and Holdings).
    Wallet([u8; 32]),
    /// Bell-region records, anchors and seeds (`bells: true`).
    Bells,
    None,
}

/// One WS message before numbering: `{seq, kind, key, slot, head,
/// bytes_b64}` (§8.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diff {
    /// `acct` (an account's new bytes; empty = closed), `bell` (a per-bell
    /// file was written: `key` is its `/h/…` path, no bytes) or `event` (a
    /// PS2 record: `key` = `NAME:fields`, bytes = the body).
    pub kind: &'static str,
    pub key: String,
    pub slot: u64,
    pub head: Option<[u8; 32]>,
    pub bytes: Vec<u8>,
    pub scope: Scope,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProvMeta {
    pub ring: u16,
    pub region: u8,
    /// First bell the province counts in overviews (`opened_bell`; a
    /// pre-genesis opening counts from bell 0).
    pub first: u32,
    pub addr: [u8; 32],
}

/// Bells one transaction may close at most (a SkipQuiet skips ≤ 24).
const MAX_CLOSE: u32 = 4_096;

pub struct Fold {
    pub cfg: FoldCfg,
    pub ctx: AddrCtx,
    pub st: State,
    pub out: Out,
    pub season: Option<Season>,
    pub provinces: BTreeMap<(i16, i16), ProvMeta>,
    pub rings: BTreeMap<u16, BTreeSet<(i16, i16)>>,
    pub regions: BTreeMap<u8, BTreeSet<(i16, i16)>>,
    /// Citizen address → wallet.
    pub wallet_of: BTreeMap<[u8; 32], [u8; 32]>,
    /// Diffs produced since the last [`Fold::take_diffs`].
    pub diffs: Vec<Diff>,
    /// Bumped by every applied transaction (cache key of live views).
    pub version: u64,
    /// The chain's latest `(slot, Clock unix time)` as last observed by the
    /// ingest loop (live views only; never folded into files).
    pub live: Option<(u64, i64)>,
}

fn kind_of(d: &[u8]) -> Option<AccountKind> {
    AccountKind::from_magic(d.get(..8)?.try_into().ok()?)
}

fn season_of(d: &[u8]) -> Option<u64> {
    Some(u64::from_le_bytes(d.get(8..16)?.try_into().ok()?))
}

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

impl Fold {
    /// A fresh fold writing under `out`.
    pub fn new(cfg: FoldCfg, out: Out) -> Fold {
        Fold::with_state(cfg, out, State::default())
    }

    /// A fold resuming from a saved state (derived indexes rebuilt).
    pub fn with_state(cfg: FoldCfg, out: Out, st: State) -> Fold {
        let ctx = findex::addr_ctx(&cfg.program, cfg.season_id);
        let mut f = Fold {
            cfg,
            ctx,
            st,
            out,
            season: None,
            provinces: BTreeMap::new(),
            rings: BTreeMap::new(),
            regions: BTreeMap::new(),
            wallet_of: BTreeMap::new(),
            diffs: vec![],
            version: 0,
            live: None,
        };
        let all: Vec<([u8; 32], Vec<u8>)> =
            f.st.accounts
                .iter()
                .map(|(k, c)| (*k, c.data.clone()))
                .collect();
        for (k, d) in all {
            f.index_account(&k, &d);
        }
        f
    }

    pub fn take_diffs(&mut self) -> Vec<Diff> {
        std::mem::take(&mut self.diffs)
    }

    /// The Season PDA.
    pub fn season_address(&self) -> [u8; 32] {
        self.ctx.season
    }

    fn index_account(&mut self, k: &[u8; 32], d: &[u8]) {
        match kind_of(d) {
            Some(AccountKind::Province) => {
                if let Ok(pv) = Province::decode(d) {
                    let first = if pv.opened_bell == u32::MAX {
                        0
                    } else {
                        pv.opened_bell
                    };
                    let meta = ProvMeta {
                        ring: pv.ring,
                        region: pv.region,
                        first,
                        addr: *k,
                    };
                    self.provinces.insert((pv.p, pv.q), meta);
                    self.rings.entry(pv.ring).or_default().insert((pv.p, pv.q));
                    self.regions
                        .entry(pv.region)
                        .or_default()
                        .insert((pv.p, pv.q));
                }
            }
            Some(AccountKind::Citizen) => {
                if let Some(w) = d.get(64..96) {
                    self.wallet_of.insert(*k, w.try_into().unwrap_or_default());
                }
            }
            Some(AccountKind::Season) if *k == self.ctx.season => {
                self.season = Season::decode(d).ok();
            }
            _ => {}
        }
    }

    fn unindex_account(&mut self, k: &[u8; 32], d: &[u8]) {
        match kind_of(d) {
            Some(AccountKind::Province) => {
                let pq = (
                    i16::from_le_bytes([d[64], d[65]]),
                    i16::from_le_bytes([d[66], d[67]]),
                );
                if let Some(m) = self.provinces.remove(&pq) {
                    if let Some(s) = self.rings.get_mut(&m.ring) {
                        s.remove(&pq);
                    }
                    if let Some(s) = self.regions.get_mut(&m.region) {
                        s.remove(&pq);
                    }
                }
            }
            Some(AccountKind::Citizen) => {
                self.wallet_of.remove(k);
            }
            _ => {}
        }
    }

    fn scope_of_account(&self, kind: AccountKind, d: &[u8]) -> Scope {
        let pq = |o: usize| {
            Scope::Province(
                i16::from_le_bytes([d[o], d[o + 1]]) as i32,
                i16::from_le_bytes([d[o + 2], d[o + 3]]) as i32,
            )
        };
        match kind {
            AccountKind::Province | AccountKind::ClashInputs | AccountKind::Holding
                if d.len() >= 68 =>
            {
                if kind == AccountKind::Holding {
                    let owner: [u8; 32] = d
                        .get(72..104)
                        .and_then(|x| x.try_into().ok())
                        .unwrap_or_default();
                    return match self.wallet_of.get(&owner) {
                        Some(w) => Scope::Wallet(*w),
                        None => Scope::None,
                    };
                }
                pq(64)
            }
            AccountKind::ArrivalSlot | AccountKind::ArrivalDay if d.len() >= 20 => pq(16),
            AccountKind::Citizen if d.len() >= 96 => {
                Scope::Wallet(d[64..96].try_into().unwrap_or_default())
            }
            AccountKind::BellAnchor
            | AccountKind::SeedCache
            | AccountKind::AnchorArchive
            | AccountKind::BeaconLog => Scope::Bells,
            _ => Scope::None,
        }
    }

    fn write(&mut self, rel: &str, bytes: &[u8], immutable: bool) {
        match self.out.write(rel, bytes) {
            Ok(Written::Changed) if immutable => {
                self.st.alarms.rewrites += 1;
                eprintln!("herald: ALARM immutable file {rel} rewritten with different bytes");
            }
            Ok(_) => {}
            Err(e) => {
                self.st.alarms.write_errors += 1;
                eprintln!("herald: write {rel}: {e}");
            }
        }
    }

    /// Folds one archived transaction (idempotent by archive sequence).
    pub fn apply(&mut self, tx: &TxRecord) {
        if tx.seq <= self.st.folded_through {
            return;
        }
        self.version += 1;
        self.st.last_slot = self.st.last_slot.max(tx.slot);
        self.st.last_time = self.st.last_time.max(tx.block_time);
        if tx.err.is_some() {
            self.st.folded_through = tx.seq;
            return;
        }
        // 1. The post-state.
        let mut before: BTreeMap<[u8; 32], Vec<u8>> = BTreeMap::new();
        let mut resolved: Vec<((i16, i16), u32, u32)> = vec![];
        let mut br_keys: BTreeSet<(u32, u8)> = BTreeSet::new();
        let mut rings_touched: BTreeSet<u16> = BTreeSet::new();
        for (k, a) in &tx.post {
            let key = k.to_bytes();
            let new = a.as_ref().and_then(|a| {
                (a.owner == self.cfg.program
                    && a.data.len() >= 16
                    && season_of(&a.data) == Some(self.cfg.season_id)
                    && kind_of(&a.data).is_some())
                .then(|| a.data.clone())
            });
            let old = self.st.accounts.get(&key).cloned();
            match (old, new) {
                (None, None) => {}
                (Some(o), None) => {
                    let kind = kind_of(&o.data);
                    self.unindex_account(&key, &o.data);
                    if let Some(kind) = kind {
                        if kind == AccountKind::Province {
                            before.insert(key, o.data.clone());
                        }
                        // A closed anchor or cache changes its bell-region
                        // record (`present`); an archive closes only at the
                        // season's end and leaves the records as they are.
                        if let Some(br) = records::bell_region_of_account(kind, &o.data) {
                            br_keys.insert(br);
                        }
                        let scope = self.scope_of_account(kind, &o.data);
                        self.diffs.push(Diff {
                            kind: "acct",
                            key: records::account_key(kind, &o.data, &k.to_string()),
                            slot: tx.slot,
                            head: None,
                            bytes: vec![],
                            scope,
                        });
                    }
                    self.st.accounts.remove(&key);
                }
                (old, Some(d)) => {
                    if old.as_ref().map(|o| &o.data) == Some(&d) {
                        continue;
                    }
                    let kind = kind_of(&d).unwrap_or(AccountKind::Season);
                    match kind {
                        AccountKind::Province => {
                            let old_rn = old.as_ref().and_then(|o| u32_at(&o.data, 72));
                            before.insert(
                                key,
                                old.as_ref().map(|o| o.data.clone()).unwrap_or_default(),
                            );
                            if let (Some(o), Some(n)) = (old_rn, u32_at(&d, 72)) {
                                let pq = (
                                    i16::from_le_bytes([d[64], d[65]]),
                                    i16::from_le_bytes([d[66], d[67]]),
                                );
                                if n > o {
                                    resolved.push((pq, o, n));
                                }
                            }
                        }
                        AccountKind::BellAnchor => {
                            if let Some(br) = records::bell_region_of_account(kind, &d) {
                                br_keys.insert(br);
                                self.st.anchors.insert(
                                    br,
                                    Capture {
                                        slot: tx.slot,
                                        tx: tx.seq,
                                        data: d.clone(),
                                    },
                                );
                            }
                        }
                        AccountKind::SeedCache => {
                            if let Some(br) = records::bell_region_of_account(kind, &d) {
                                br_keys.insert(br);
                            }
                        }
                        AccountKind::AnchorArchive => {
                            let o = old.as_ref().map(|o| o.data.clone()).unwrap_or_default();
                            self.archive_keys(&d, &o, &mut br_keys);
                        }
                        _ => {}
                    }
                    let fresh = old.is_none();
                    self.st.accounts.insert(
                        key,
                        Capture {
                            slot: tx.slot,
                            tx: tx.seq,
                            data: d.clone(),
                        },
                    );
                    self.index_account(&key, &d);
                    if fresh && kind == AccountKind::Province {
                        if let Ok(pv) = Province::decode(&d) {
                            self.st.ov_next.entry(pv.ring).or_insert(pv.resolved_next);
                            rings_touched.insert(pv.ring);
                        }
                    }
                    let scope = self.scope_of_account(kind, &d);
                    let head = chained_header(&d)
                        .filter(|_| kind.chained())
                        .map(|h| h.event_head);
                    self.diffs.push(Diff {
                        kind: "acct",
                        key: records::account_key(kind, &d, &k.to_string()),
                        slot: tx.slot,
                        head,
                        bytes: d,
                        scope,
                    });
                }
            }
        }
        // 2. The records.
        let bodies = match bodies_from_logs(&tx.logs, &self.cfg.program) {
            Ok(b) => b,
            Err(_) => {
                self.st.alarms.bad_records += 1;
                vec![]
            }
        };
        for body in &bodies {
            let Ok(r) = plog::decode(body) else {
                self.st.alarms.bad_records += 1;
                continue;
            };
            self.st.events += 1;
            let scope = match records::province_of_record(r.kind, r.key) {
                Some((p, q)) => Scope::Province(p, q),
                None if records::bell_region_of_record(r.kind, r.key).is_some() => Scope::Bells,
                None => Scope::None,
            };
            self.diffs.push(Diff {
                kind: "event",
                key: records::record_key(r.kind, r.key),
                slot: tx.slot,
                head: None,
                bytes: body.clone(),
                scope,
            });
            match r.kind {
                Kind::SEED => {
                    if let (Some(br), Ok(round), Ok(seed), Ok(a)) = (
                        records::bell_region_of_record(r.kind, r.key),
                        r.payload[0..8].try_into().map(u64::from_le_bytes),
                        r.payload[8..40].try_into(),
                        r.payload[40..48].try_into().map(i64::from_le_bytes),
                    ) {
                        self.st
                            .seeds
                            .insert((br.0, br.1, r.key[5]), SeedRec { round, seed, a });
                        br_keys.insert(br);
                    }
                }
                Kind::ANCHOR => {
                    if let Some(br) = records::bell_region_of_record(r.kind, r.key) {
                        br_keys.insert(br);
                    }
                }
                Kind::RING_OPEN | Kind::RING_SEED => {
                    let d = u16::from_le_bytes([r.key[0], r.key[1]]);
                    let o = if r.kind == Kind::RING_OPEN { 16 } else { 8 };
                    if let Ok(seed) = <[u8; 32]>::try_from(&r.payload[o..o + 32]) {
                        if seed != [0u8; 32] {
                            self.st.ring_seeds.insert(d, seed);
                        }
                    }
                }
                Kind::TRANSIT_SETTLED => {
                    let host = u64::from_le_bytes(r.key[..8].try_into().unwrap_or_default());
                    self.st.seals.insert(
                        host,
                        SealRec {
                            outcome: r.payload[0],
                            code: r.payload[1],
                            bell: r.bell,
                        },
                    );
                }
                Kind::CLASH => {
                    self.clash_report(tx, &r, &before);
                }
                _ => {}
            }
        }
        // 3. Per-bell closing.
        for (pq, old, new) in resolved {
            let Some(meta) = self.provinces.get(&pq).copied() else {
                continue;
            };
            if new - old > MAX_CLOSE {
                eprintln!(
                    "herald: province {pq:?} resolved_next {old} → {new}: not closing that many bells"
                );
                continue;
            }
            for b in old..new {
                self.close_bell(pq, b);
                br_keys.insert((b, meta.region));
            }
            rings_touched.insert(meta.ring);
        }
        for d in rings_touched {
            self.overview_advance(d, tx.slot);
        }
        for (b, r) in br_keys {
            self.bell_region(b, r, tx.slot);
        }
        self.st.folded_through = tx.seq;
    }

    /// Bells of an archive whose bits changed between `old` and `new`.
    fn archive_keys(&self, new: &[u8], old: &[u8], out: &mut BTreeSet<(u32, u8)>) {
        let Ok(a) = AnchorArchive::decode(new) else {
            return;
        };
        let o = AnchorArchive::decode(old).ok();
        for k in 0..72u32 {
            let b = a.part * 72 + k;
            let now = (a.is_archived(b), a.tombstoned(b));
            let was = o
                .as_ref()
                .map(|o| (o.is_archived(b), o.tombstoned(b)))
                .unwrap_or((false, false));
            if now != was {
                out.insert((b, a.region));
            }
        }
    }

    fn cap(&self, a: &[u8; 32]) -> Option<&Capture> {
        self.st.accounts.get(a)
    }

    /// The §9.2 envelope of `(P, Q)` at `bell` from the current captures.
    pub fn envelope(&self, pq: (i16, i16), bell: u32) -> Option<Value> {
        let (p, q) = (pq.0 as i32, pq.1 as i32);
        let pv = self.cap(&self.ctx.province(p, q))?;
        let (seq, head) = pv.head().unwrap_or((0, [0; 32]));
        let mut slots = vec![];
        for f in 0..6u8 {
            for i in 0..4u8 {
                if let Some(c) = self.cap(&self.ctx.arrival_slot(p, q, bell, f, i)) {
                    slots.push(json!({"key": format!("ar:{p},{q},{bell},{f},{i}"), "slot": c.slot, "bytes": B64.encode(&c.data)}));
                }
            }
        }
        let day = bell / 144;
        let day_v = self
            .cap(&self.ctx.arrival_day(p, q, day))
            .map(|c| json!({"key": format!("ad:{p},{q},{day}"), "bytes": B64.encode(&c.data)}))
            .unwrap_or(Value::Null);
        let inputs = self
            .cap(&self.ctx.clash_inputs(p, q, bell))
            .map(|c| {
                let (s, h) = c.head().unwrap_or((0, [0; 32]));
                json!({"key": format!("ci:{p},{q},{bell}"), "seq": s.to_string(), "head": hex::encode(h), "bytes": B64.encode(&c.data)})
            })
            .unwrap_or(Value::Null);
        Some(json!({
            "v": 1, "key": format!("pv:{p},{q}"), "bell": bell, "slot": pv.slot,
            "seq": seq.to_string(), "head": hex::encode(head), "bytes": B64.encode(&pv.data),
            "slots": slots, "day": day_v, "inputs": inputs,
        }))
    }

    fn close_bell(&mut self, pq: (i16, i16), b: u32) {
        let Some(meta) = self.provinces.get(&pq).copied() else {
            return;
        };
        let Some(cap) = self.cap(&meta.addr).cloned() else {
            return;
        };
        let Some(env) = self.envelope(pq, b) else {
            return;
        };
        let path = format!("h/province/{},{}/{b}", pq.0, pq.1);
        let bytes = serde_json::to_vec(&env).unwrap_or_default();
        self.write(&format!("{path}.json"), &bytes, true);
        self.diffs.push(Diff {
            kind: "bell",
            key: format!("/{path}"),
            slot: cap.slot,
            head: cap.head().map(|h| h.1),
            bytes: vec![],
            scope: Scope::Province(pq.0 as i32, pq.1 as i32),
        });
        self.st.pending.insert((pq.0, pq.1, b), cap);
    }

    /// The Season's beacon clock.
    fn clock(&self) -> Option<(BeaconClock, i64)> {
        let s = self.season.as_ref()?;
        Some((
            BeaconClock {
                genesis: s.drand_genesis,
                period: s.drand_period.max(1) as i64,
            },
            s.genesis_ts,
        ))
    }

    /// THE anchor of `(b, r)`: `(A, round, present)` from the anchor or,
    /// once archived, the archive entry.
    pub fn anchor_of(&self, b: u32, r: u8) -> Option<(i64, u64, bool)> {
        if let Some(c) = self.st.anchors.get(&(b, r)) {
            if let Ok(a) = fclient::decode::BellAnchor::decode(&c.data) {
                let present = self.st.accounts.contains_key(&self.ctx.bell_anchor(b, r));
                return Some((a.a, a.round, present));
            }
        }
        let (clock, genesis_ts) = self.clock()?;
        let ar = self.archive_of(b, r)?;
        if !ar.is_archived(b) {
            return None;
        }
        let e = ar.entries.get((b % 72) as usize)?;
        let a = beacon::bell_end(genesis_ts, b) + e.a_off as i64;
        Some((a, beacon::tlock_round(&clock, genesis_ts, b), false))
    }

    fn archive_of(&self, b: u32, r: u8) -> Option<AnchorArchive> {
        let c = self.cap(&self.ctx.anchor_archive(r, b / 72))?;
        AnchorArchive::decode(&c.data).ok()
    }

    /// `S(b, r)` from THE anchor's A.
    pub fn seed_round(&self, b: u32, a: i64) -> Option<u64> {
        let (clock, _) = self.clock()?;
        let s = self.season.as_ref()?;
        Some(beacon::seed_round(
            &clock,
            beacon::reveal_close(a, s.window(b)),
            s.seed_margin,
        ))
    }

    /// The seed of `(b, r)`: `(seed, Some(nonce))` from a cache of round
    /// `S`, or `(seed, None)` from the archive entry.
    pub fn seed_of(&self, b: u32, r: u8) -> Option<([u8; 32], Option<u8>)> {
        let (a, _, _) = self.anchor_of(b, r)?;
        let s = self.seed_round(b, a)?;
        for ((_, _, n), rec) in self.st.seeds.range((b, r, 0)..=(b, r, u8::MAX)) {
            if rec.round == s && rec.a == a {
                return Some((rec.seed, Some(*n)));
            }
        }
        let ar = self.archive_of(b, r)?;
        if ar.is_archived(b) {
            return Some((ar.entries.get((b % 72) as usize)?.seed, None));
        }
        None
    }

    fn clash_report(
        &mut self,
        tx: &TxRecord,
        r: &plog::Record<'_>,
        before: &BTreeMap<[u8; 32], Vec<u8>>,
    ) {
        let p = i32::from_le_bytes(r.key[0..4].try_into().unwrap_or_default());
        let q = i32::from_le_bytes(r.key[4..8].try_into().unwrap_or_default());
        let b = u32::from_le_bytes(r.key[8..12].try_into().unwrap_or_default());
        self.st.clashes.insert((p as i16, q as i16, b));
        let prov = self.ctx.province(p, q);
        let prov_before = before
            .get(&prov)
            .cloned()
            .or_else(|| self.cap(&prov).map(|c| c.data.clone()))
            .unwrap_or_default();
        let inputs = self
            .cap(&self.ctx.clash_inputs(p, q, b))
            .map(|c| c.data.clone())
            .unwrap_or_default();
        let outcome_digest: [u8; 32] = r.payload[0..32].try_into().unwrap_or_default();
        let input_digest: [u8; 32] = r.payload[32..64].try_into().unwrap_or_default();
        let engagements = u32::from_le_bytes(r.payload[64..68].try_into().unwrap_or_default());
        let fates = plog::unpack_fates(&r.payload[68..77].try_into().unwrap_or_default());
        let region = self
            .provinces
            .get(&(p as i16, q as i16))
            .map(|m| m.region)
            .or_else(|| prov_before.get(71).copied())
            .unwrap_or(0);
        let anchor = self.anchor_of(b, region);
        let seed = self.seed_of(b, region);
        let anchor_v = anchor
            .map(|(a, round, _)| json!({"key": format!("an:{b},{region}"), "A": a, "round": round}))
            .unwrap_or(Value::Null);
        let (cache_v, archive_v) = match seed {
            Some((_, Some(n))) => (
                json!({"key": format!("sd:{b},{region},{n}"), "nonce": n}),
                Value::Null,
            ),
            Some((_, None)) => (
                Value::Null,
                json!({"key": format!("aa:{region},{}", b / 72)}),
            ),
            None => (Value::Null, Value::Null),
        };
        let recomputed = match seed {
            None => Err("no seed of THE anchor's S round is known".to_string()),
            Some(_) if prov_before.is_empty() || inputs.is_empty() => {
                Err("the Province or ClashInputs bytes were not captured".to_string())
            }
            Some((s, _)) => self.cfg.builder.recompute(&prov_before, &inputs, b, &s),
        };
        let (check, fighters, err, digest) = match &recomputed {
            Ok(o) => {
                let d = o.digest();
                let (check, err) = match (d == outcome_digest, self.cfg.exact_post) {
                    (true, _) => ("match", Value::Null),
                    (false, true) => ("MISMATCH", Value::Null),
                    (false, false) => (
                        "unchecked",
                        json!("differs, but the post-state is not exact (RPC source)"),
                    ),
                };
                (check, fighters_json(o), err, json!(hex::encode(d)))
            }
            Err(e) => ("unchecked", json!([]), json!(e), Value::Null),
        };
        match check {
            "MISMATCH" => {
                self.st.alarms.clash_mismatch += 1;
                eprintln!("herald: ALARM clash {p},{q}@{b}: recomputed digest differs from the logged one");
            }
            "unchecked" => self.st.alarms.clash_unchecked += 1,
            _ => {}
        }
        let rep = json!({
            "v": 1, "key": format!("ci:{p},{q},{b}"), "bell": b, "province": [p, q], "slot": tx.slot,
            "inputs_b64": B64.encode(&inputs), "seed": seed.map(|s| hex::encode(s.0)),
            "anchor": anchor_v, "cache": cache_v, "archive": archive_v,
            "outcomeDigest": hex::encode(outcome_digest), "inputDigest": hex::encode(input_digest),
            "decoded": {"fighters": fighters, "engagements": engagements, "fates": fates.to_vec()},
            "heraldCheck": check, "builder": self.cfg.builder.name(),
            "recomputedDigest": digest, "checkError": err,
        });
        let path = format!("h/clash/{p},{q}/{b}");
        self.write(
            &format!("{path}.json"),
            &serde_json::to_vec(&rep).unwrap_or_default(),
            true,
        );
        self.diffs.push(Diff {
            kind: "bell",
            key: format!("/{path}"),
            slot: tx.slot,
            head: None,
            bytes: vec![],
            scope: Scope::Province(p, q),
        });
    }

    /// Overview flags of a province at `bell` from its bytes.
    fn flags(&self, pv: &Province, first: u32, bell: u32) -> u8 {
        let mut f = 0;
        if self.st.clashes.contains(&(pv.p, pv.q, bell)) {
            f |= overview::FLAG_CLASH;
        }
        let dormant = (0..(pv.site_count as usize).min(12)).any(|i| {
            pv.site_mirror[i].state == 1
                && self
                    .cap(&self.ctx.holding(pv.p as i32, pv.q as i32, i as u8))
                    .and_then(|c| c.data.get(115).copied())
                    .is_some_and(|fl| fl & 1 != 0)
        });
        if dormant {
            f |= overview::FLAG_DORMANT;
        }
        if first == bell {
            f |= overview::FLAG_OPENED;
        }
        f
    }

    fn overview_advance(&mut self, d: u16, slot: u64) {
        let Some(provs) = self.rings.get(&d).cloned() else {
            return;
        };
        let mut target = u32::MAX;
        for pq in &provs {
            let Some(m) = self.provinces.get(pq) else {
                continue;
            };
            if let Some(rn) = self.cap(&m.addr).and_then(|c| u32_at(&c.data, 72)) {
                target = target.min(rn);
            }
        }
        if target == u32::MAX || provs.is_empty() {
            return;
        }
        let mut b = *self.st.ov_next.get(&d).unwrap_or(&target);
        let mut n = 0;
        while b < target && n < MAX_CLOSE {
            let mut recs = vec![];
            for pq in &provs {
                let Some(m) = self.provinces.get(pq).copied() else {
                    continue;
                };
                if m.first > b {
                    continue;
                }
                let data = match self.st.pending.get(&(pq.0, pq.1, b)) {
                    Some(c) => c.data.clone(),
                    None => match self.cap(&m.addr) {
                        Some(c) => c.data.clone(),
                        None => continue,
                    },
                };
                if let Ok(pv) = Province::decode(&data) {
                    recs.push(overview::record(&pv, self.flags(&pv, m.first, b)));
                }
            }
            if !recs.is_empty() {
                let bytes = overview::file(self.cfg.season_id, d, b, slot, &recs);
                let path = format!("h/overview/{d}/{b}.bin");
                self.write(&path, &bytes, true);
                self.diffs.push(Diff {
                    kind: "bell",
                    key: format!("/{path}"),
                    slot,
                    head: None,
                    bytes: vec![],
                    scope: Scope::Ring(d),
                });
            }
            for pq in &provs {
                self.st.pending.remove(&(pq.0, pq.1, b));
            }
            b += 1;
            n += 1;
        }
        self.st.ov_next.insert(d, b);
    }

    /// The live overview of ring `d` (`latest.bin`): every province's
    /// current bytes; the header bell is the last bell every one of them
    /// resolved.
    pub fn overview_latest(&self, d: u16) -> Option<Vec<u8>> {
        let provs = self.rings.get(&d)?;
        let mut recs = vec![];
        let mut target = u32::MAX;
        let mut decoded = vec![];
        for pq in provs {
            let Some(m) = self.provinces.get(pq) else {
                continue;
            };
            let Some(c) = self.cap(&m.addr) else {
                continue;
            };
            if let Ok(pv) = Province::decode(&c.data) {
                target = target.min(pv.resolved_next);
                decoded.push((pv, m.first));
            }
        }
        let bell = target.saturating_sub(1);
        for (pv, first) in &decoded {
            recs.push(overview::record(pv, self.flags(pv, *first, bell)));
        }
        (!recs.is_empty())
            .then(|| overview::file(self.cfg.season_id, d, bell, self.st.last_slot, &recs))
    }

    /// The bell-region record, or `None` until THE anchor and a seed of
    /// `S` exist.
    pub fn bell_region_json(&self, b: u32, r: u8) -> Option<(Value, bool)> {
        let (a, round, present) = self.anchor_of(b, r)?;
        let s = self.seed_round(b, a)?;
        let ar = self.archive_of(b, r);
        let archived = ar.as_ref().is_some_and(|x| x.is_archived(b));
        let tombstoned = ar.as_ref().is_some_and(|x| x.tombstoned(b));
        let mut caches = vec![];
        let mut seeded = archived;
        for ((_, _, n), rec) in self.st.seeds.range((b, r, 0)..=(b, r, u8::MAX)) {
            let addr = self.ctx.seed_cache(b, r, *n);
            seeded |= rec.round == s && rec.a == a;
            caches.push(json!({
                "key": format!("sd:{b},{r},{n}"), "address": Address::new_from_array(addr).to_string(),
                "nonce": n, "round": rec.round, "seed": hex::encode(rec.seed), "A": rec.a,
                "present": self.st.accounts.contains_key(&addr),
            }));
        }
        if !seeded {
            return None;
        }
        let anchor_addr = self.ctx.bell_anchor(b, r);
        let anchor_v = match self.st.anchors.get(&(b, r)) {
            Some(c) => json!({
                "key": format!("an:{b},{r}"), "address": Address::new_from_array(anchor_addr).to_string(),
                "bytes_b64": B64.encode(&c.data), "slot": c.slot, "A": a, "round": round, "present": present,
            }),
            None => Value::Null,
        };
        let archive_v = match (&ar, archived) {
            (Some(x), true) => {
                let e = x.entries.get((b % 72) as usize);
                json!({
                    "key": format!("aa:{r},{}", b / 72),
                    "address": Address::new_from_array(self.ctx.anchor_archive(r, b / 72)).to_string(),
                    "aOff": e.map(|e| e.a_off), "seed": e.map(|e| hex::encode(e.seed)),
                })
            }
            _ => Value::Null,
        };
        let mut resolved = vec![];
        let mut all = true;
        if let Some(set) = self.regions.get(&r) {
            for pq in set {
                let Some(m) = self.provinces.get(pq) else {
                    continue;
                };
                if m.first > b {
                    continue;
                }
                let rn = self.cap(&m.addr).and_then(|c| u32_at(&c.data, 72));
                if rn.is_some_and(|rn| rn > b) {
                    resolved.push(json!([pq.0, pq.1]));
                } else {
                    all = false;
                }
            }
        }
        let fin = archived && all;
        Some((
            json!({
                "v": 1, "bell": b, "region": r, "anchor": anchor_v, "archive": archive_v, "S": s,
                "caches": caches, "tombstoned": tombstoned, "archived": archived,
                "resolved": resolved, "final": fin,
            }),
            fin,
        ))
    }

    fn bell_region(&mut self, b: u32, r: u8, slot: u64) {
        let Some((v, fin)) = self.bell_region_json(b, r) else {
            return;
        };
        let bytes = serde_json::to_vec(&v).unwrap_or_default();
        let hash: [u8; 32] = Sha256::digest(&bytes).into();
        if self.st.br.get(&(b, r)).map(|s| s.hash) == Some(hash) {
            return;
        }
        let path = format!("h/bell/{b}/region/{r}");
        self.write(&format!("{path}.json"), &bytes, false);
        self.st.br.insert((b, r), BrState { hash, fin });
        self.diffs.push(Diff {
            kind: "bell",
            key: format!("/{path}"),
            slot,
            head: None,
            bytes: vec![],
            scope: Scope::Bells,
        });
    }

    /// Whether the bell-region file is final (immutable cache).
    pub fn bell_region_final(&self, b: u32, r: u8) -> bool {
        self.st.br.get(&(b, r)).is_some_and(|s| s.fin)
    }

    /// The capture of an address.
    pub fn account(&self, a: &[u8; 32]) -> Option<&Capture> {
        self.cap(a)
    }
}
