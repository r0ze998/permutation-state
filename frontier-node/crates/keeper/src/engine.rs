//! The escalation engine (M1 contract §8.2 "Bid policy", I-21, I-49, I-50).
//!
//! A duty asks for a **write** (one object to create or advance, keyed
//! uniquely) with its class and an instruction builder. Every slot until
//! the write lands the engine sends **a new version**: a new signature from
//! a **newly drawn random payer** of the class's pool, at the class's bid
//! for that slot —
//!
//! - **W** (Reveal): from `p_start` (= `p_tip`, or `--peace-start f × p_tip`)
//!   doubling every slot to **P_def 2.0** (capped by the season's
//!   `defence_cap_milli`); never sent at or after its deadline slot;
//! - **D** (delay-only): from the low bid doubling to **P_delay 0.5**;
//! - **N**: the fixed low bid.
//!
//! Each version carries the per-kind CU limit and `L(kind)` from the budgets
//! table. **Retry ladder (I-50):** `ComputationalBudgetExceeded` → the next
//! version asks `min(2 × limit, 1.4M)`, then 1.4M; a program that fails to
//! complete (the heap access fault of a fill past 32 KiB) → the next
//! version adds `RequestHeapFrame(262,144)`. Each rung is journalled and
//! alerted: a breach is a gate failure to fix, never a stuck province.
//!
//! **Contested:** a write not landed `contested_slots` (2) slots after a
//! version at a bid ≥ `p_tip` marks its `(bell, region)` contested; the
//! keeper alerts and its duties react (anchor fallbacks every slot).
//!
//! **Duplicates:** only one version can take effect; the others fail
//! (`AlreadyDone`) or no-op and still pay their fee. New versions stop once
//! the versions' fees could exceed `per_write_cap` (§8.2's bound
//! `resends × (base + fee) ≤ per_write_cap`); the write then waits on the
//! versions already sent.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use solana_address::Address;
use solana_hash::Hash;
use solana_instruction::Instruction;
use solana_signature::Signature;
use solana_signer::Signer;

use fclient::abi::{self, err, Class};
use fclient::budgets::Budgets;
use fclient::ports::{ChainPort, Status};
use fclient::{fees, tx};

use crate::journal::{Attempt, Journal};
use crate::pools::Payers;

/// What a builder learns about the version it builds.
#[derive(Clone, Copy, Debug)]
pub struct BuildCtx {
    /// The fee payer drawn for this version (also `rent_to`, I-49).
    pub payer: Address,
    /// 0 for the first version.
    pub version: u32,
    /// Slots since the first version.
    pub slots_waiting: u64,
}

pub type Builder = Arc<dyn Fn(&BuildCtx) -> Vec<Instruction> + Send + Sync>;

/// One write a duty wants landed.
#[derive(Clone)]
pub struct WriteSpec {
    /// Unique object key (`anchor:<bell>:<region>`, …).
    pub key: String,
    /// Duty kind (journal, metrics).
    pub kind: &'static str,
    /// Frontier instruction tag (budgets).
    pub tag: u8,
    pub class: Class,
    pub bell: Option<u32>,
    pub region: Option<u8>,
    pub build: Builder,
    /// Never send a version in this slot or later (W writes: `A + W − 2 slots`).
    pub deadline_slot: Option<u64>,
    /// First slot a version may be sent (anchor fallbacks lag the combined form).
    pub not_before_slot: u64,
    /// A fixed fee payer (payer care: the transfer's source signs) instead
    /// of a draw from the class's pool.
    pub fixed_payer: Option<Address>,
}

/// Bid parameters of the fleet (milli-units of priority, §10.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineParams {
    /// W writes start here (`p_tip`, or `peace_start × p_tip`).
    pub p_start_milli: u64,
    /// D writes start here (the low bid).
    pub d_start_milli: u64,
    pub p_low_milli: u64,
    /// `min(P_def, season defence cap)`.
    pub p_def_milli: u64,
    pub p_delay_milli: u64,
    /// The tip level: the contested threshold.
    pub p_tip_milli: u64,
    pub contested_slots: u64,
    pub per_write_cap: u64,
    /// Hard stop on versions of one write.
    pub max_versions: u32,
}

impl Default for EngineParams {
    fn default() -> Self {
        EngineParams {
            p_start_milli: 433,
            d_start_milli: crate::P_LOW_MILLI,
            p_low_milli: crate::P_LOW_MILLI,
            p_def_milli: crate::P_DEF_MILLI,
            p_delay_milli: crate::P_DELAY_MILLI,
            p_tip_milli: 433,
            contested_slots: 2,
            per_write_cap: 20_000_000,
            max_versions: 64,
        }
    }
}

impl EngineParams {
    /// The bid of the version sent `slots` slots after the first.
    pub fn bid(&self, class: Class, slots: u64) -> u64 {
        let double = |start: u64, cap: u64| {
            start
                .max(1)
                .saturating_mul(1u64 << slots.min(20))
                .min(cap)
                .max(start.min(cap))
        };
        match class {
            Class::W => double(self.p_start_milli, self.p_def_milli),
            Class::D => double(self.d_start_milli, self.p_delay_milli),
            _ => self.p_low_milli,
        }
    }
}

/// One version as sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub sig: Signature,
    pub payer: Address,
    pub bid_milli: u64,
    pub cu_limit: u32,
    pub heap: Option<u32>,
    pub sent_slot: u64,
    /// Fee if it lands (base + priority).
    pub fee: u64,
}

/// How a write ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Landed (or a version found it already done, code 52).
    Landed {
        slot: u64,
        sig: Signature,
        payer: Address,
        versions: usize,
        first_slot: u64,
        already_done: bool,
    },
    /// The program refused it with this code: the duty re-plans (after a
    /// short backoff) or gives up.
    Failed {
        code: Option<u32>,
        err: String,
        slot: u64,
    },
    /// Nothing more to do: window closed, not implemented, deadline passed.
    Dead { reason: String, code: Option<u32> },
}

struct Pending {
    spec: WriteSpec,
    first_slot: Option<u64>,
    versions: Vec<Version>,
    /// Versions whose status is known (failed ones stay in `versions` for
    /// the spend bound).
    settled: std::collections::HashSet<Signature>,
    cu_limit: u32,
    heap: Option<u32>,
    contested: bool,
    /// Added to every version's bid (milli): orders the Reveals of one
    /// group sent in the same slot so a block lands them in rank order (W4-C).
    bonus_milli: u64,
}

/// What one step did.
#[derive(Default, Debug)]
pub struct StepReport {
    pub outcomes: Vec<(String, Outcome)>,
    pub sent: usize,
    /// Newly contested `(bell, region, key)`.
    pub contested: Vec<(Option<u32>, Option<u8>, String)>,
    /// Retry-ladder rungs taken `(key, cu_limit, heap)`.
    pub ladder: Vec<(String, u32, Option<u32>)>,
    pub send_errors: Vec<(String, String)>,
}

/// Counters per duty kind.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KindStats {
    pub writes: u64,
    pub versions: u64,
    pub landed: u64,
    pub failed: u64,
    pub dead: u64,
    /// Slots from the first version to landing, one entry per landed write.
    pub latency_slots: Vec<u64>,
    /// Fees of landed-or-failed versions (charged).
    pub fees_charged: u64,
}

pub struct Engine {
    pending: BTreeMap<String, Pending>,
    backoff: HashMap<String, u64>,
    /// Consecutive program refusals per key (the backoff doubles with each).
    failures: HashMap<String, u32>,
    /// Versions whose write is gone (cancelled, adopted after a restart):
    /// polled until resolved so the journal is complete.
    orphans: Vec<(Signature, u64)>,
    adopted: HashMap<String, Vec<Version>>,
    pub budgets: Budgets,
    pub params: EngineParams,
    pub stats: BTreeMap<&'static str, KindStats>,
}

/// `ComputationalBudgetExceeded` in any of its spellings.
pub fn is_cu_exceeded(e: &str) -> bool {
    e.contains("ComputationalBudgetExceeded")
        || e.contains("exceeded CUs meter")
        || e.contains("ComputeBudgetExceeded")
}

/// A program that did not complete (SBF access violation, e.g. the heap
/// fault of a fill past the default 32-KiB heap).
pub fn is_heap_fault(e: &str) -> bool {
    e.contains("ProgramFailedToComplete") || e.contains("Access violation in heap")
}

const BACKOFF_SLOTS: u64 = 2;
/// Slots after its send at which a version's blockhash has expired (150
/// valid blockhashes, one slot of margin).
pub const EXPIRY_SLOTS: u64 = 151;

impl Engine {
    pub fn new(budgets: Budgets, params: EngineParams) -> Engine {
        Engine {
            pending: BTreeMap::new(),
            backoff: HashMap::new(),
            failures: HashMap::new(),
            orphans: vec![],
            adopted: HashMap::new(),
            budgets,
            params,
            stats: BTreeMap::new(),
        }
    }

    pub fn is_pending(&self, key: &str) -> bool {
        self.pending.contains_key(key)
    }

    /// Slot of a pending write's first version (`None`: not sent yet or not pending).
    pub fn first_slot(&self, key: &str) -> Option<u64> {
        self.pending.get(key).and_then(|p| p.first_slot)
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub fn pending_keys(&self) -> Vec<String> {
        self.pending.keys().cloned().collect()
    }

    /// Adds a write unless it is already pending or in backoff. Returns
    /// whether it was added.
    pub fn ensure(&mut self, spec: WriteSpec, slot: u64) -> bool {
        if self.pending.contains_key(&spec.key)
            || self.backoff.get(&spec.key).is_some_and(|&s| slot < s)
        {
            return false;
        }
        self.backoff.remove(&spec.key);
        let b = self.budgets.get(spec.tag);
        let adopted = self.adopted.remove(&spec.key).unwrap_or_default();
        self.stats.entry(spec.kind).or_default().writes += 1;
        self.pending.insert(
            spec.key.clone(),
            Pending {
                first_slot: adopted.first().map(|v| v.sent_slot),
                versions: adopted,
                settled: Default::default(),
                cu_limit: b.cu_limit,
                heap: None,
                contested: false,
                bonus_milli: 0,
                spec,
            },
        );
        true
    }

    /// Sets the CU limit of a pending write (a duty that sizes its own,
    /// e.g. SkipQuiet's `90k + 30k` per recomputed bell, I-50, v1.7); the retry
    /// ladder continues from it.
    pub fn set_cu_limit(&mut self, key: &str, cu: u32) {
        if let Some(p) = self.pending.get_mut(key) {
            p.cu_limit = cu.min(abi::CU_MAX);
        }
    }

    /// Adds `milli` to every version's bid of a pending write (W4-C: the
    /// rank order of a reveal group sent in one slot).
    pub fn set_bid_bonus(&mut self, key: &str, milli: u64) {
        if let Some(p) = self.pending.get_mut(key) {
            p.bonus_milli = milli;
        }
    }

    /// Stops a write (its object is done on chain by another path). Versions
    /// already sent are kept as orphans for the journal.
    pub fn cancel(&mut self, key: &str) {
        if let Some(p) = self.pending.remove(key) {
            self.orphans
                .extend(p.versions.iter().map(|v| (v.sig, v.sent_slot)));
        }
    }

    /// Adopts in-flight attempts from the journal after a restart: the next
    /// `ensure` of the same object continues from them (their landing ends
    /// the write) instead of starting over.
    pub fn adopt(&mut self, attempts: &[Attempt]) {
        for a in attempts {
            let (Ok(sig), Ok(payer)) = (a.sig.parse(), a.payer.parse()) else {
                continue;
            };
            self.orphans.push((sig, a.sent_slot));
            self.adopted
                .entry(a.object_key.clone())
                .or_default()
                .push(Version {
                    sig,
                    payer,
                    bid_milli: a.bid_milli,
                    cu_limit: a.cu_limit,
                    heap: a.heap,
                    sent_slot: a.sent_slot,
                    fee: 0,
                });
        }
    }

    fn classify(
        &mut self,
        key: &str,
        st: &Status,
        v: &Version,
        journal: Option<&Journal>,
        report: &mut StepReport,
    ) -> Option<Outcome> {
        let p = self.pending.get_mut(key)?;
        let e = st.err.clone().unwrap_or_default();
        let s = self.stats.entry(p.spec.kind).or_default();
        s.fees_charged += v.fee;
        let status = |x: &str| {
            if let Some(j) = journal {
                let _ = j.set_status(&v.sig.to_string(), x, Some(st.slot), st.code);
            }
        };
        let first = p.first_slot.unwrap_or(st.slot);
        if st.err.is_none() || st.code.is_some_and(|c| err::is_done(p.spec.tag, c)) {
            status(if st.err.is_none() { "landed" } else { "done" });
            return Some(Outcome::Landed {
                slot: st.slot,
                sig: v.sig,
                payer: v.payer,
                versions: p.versions.len(),
                first_slot: first,
                already_done: st.err.is_some(),
            });
        }
        status("failed");
        if is_cu_exceeded(&e) && p.spec.tag == abi::tag::SKIP_QUIET {
            // v1.7 (wave-4 review): SkipQuiet within its gate formula
            // always finishes a quiet run (G1 rows, the kernel quiet test
            // included); running out means the first bell is not quiet
            // (the kernel ran a whole clash). It is `NotQuiet` for the
            // duty (gather and resolve), not a ladder step.
            if let Some(j) = journal {
                let _ = j.alert(
                    st.slot,
                    "skip-cu-not-quiet",
                    &format!("{key}: SkipQuiet used its gate: treated as NotQuiet"),
                );
            }
            return Some(Outcome::Failed {
                code: Some(err::NOT_QUIET),
                err: format!("{e} (SkipQuiet over its gate: not quiet)"),
                slot: st.slot,
            });
        }
        if is_cu_exceeded(&e) {
            if v.cu_limit >= p.cu_limit && p.cu_limit < abi::CU_MAX {
                p.cu_limit = Budgets::retry_cu(p.cu_limit);
                report.ladder.push((key.into(), p.cu_limit, p.heap));
                if let Some(j) = journal {
                    let _ = j.alert(
                        st.slot,
                        "cu-retry",
                        &format!(
                            "{key}: ComputationalBudgetExceeded, next limit {}",
                            p.cu_limit
                        ),
                    );
                }
            }
            return (v.cu_limit >= abi::CU_MAX).then(|| Outcome::Dead {
                reason: "compute budget exceeded at 1.4M".into(),
                code: None,
            });
        }
        if is_heap_fault(&e) {
            if p.heap.is_none() {
                p.heap = Some(abi::HEAP_FRAME_RETRY);
                report.ladder.push((key.into(), p.cu_limit, p.heap));
                if let Some(j) = journal {
                    let _ = j.alert(
                        st.slot,
                        "heap-retry",
                        &format!("{key}: {e}; next version requests a 256-KiB heap frame"),
                    );
                }
                return None;
            }
            // An older version without the frame: wait for the new one.
            v.heap?;
            return Some(Outcome::Dead {
                reason: format!("fails with a heap frame: {e}"),
                code: st.code,
            });
        }
        match st.code {
            Some(c) if err::stops_window(c) || c == err::NOT_IMPLEMENTED => Some(Outcome::Dead {
                reason: abi::error_name(c).unwrap_or("?").into(),
                code: Some(c),
            }),
            code => Some(Outcome::Failed {
                code,
                err: e,
                slot: st.slot,
            }),
        }
    }

    /// One slot: poll every version sent, settle the writes that ended, and
    /// send the next version of every write still waiting.
    pub async fn step<P: ChainPort>(
        &mut self,
        port: &P,
        payers: &mut Payers,
        journal: Option<&Journal>,
        slot: u64,
    ) -> StepReport {
        let mut report = self.poll(port, journal, slot).await;
        self.send(port, payers, journal, slot, &mut report).await;
        report
    }

    /// Polls every version sent and settles the writes that ended (the
    /// keeper polls before its duties plan, so a landing is an outcome, not
    /// a cancellation).
    pub async fn poll<P: ChainPort>(
        &mut self,
        port: &P,
        journal: Option<&Journal>,
        slot: u64,
    ) -> StepReport {
        let mut report = StepReport::default();
        let mut sigs: Vec<(String, Version)> = vec![];
        for (k, p) in &self.pending {
            for v in p.versions.iter().filter(|v| !p.settled.contains(&v.sig)) {
                sigs.push((k.clone(), v.clone()));
            }
        }
        let flat: Vec<Signature> = sigs.iter().map(|(_, v)| v.sig).collect();
        let sts = if flat.is_empty() {
            vec![]
        } else {
            port.statuses(&flat).await.unwrap_or_default()
        };
        let mut ended: Vec<(String, Outcome)> = vec![];
        let mut resolved: Vec<Signature> = vec![];
        for ((k, v), st) in sigs.iter().zip(sts.iter()) {
            let Some(st) = st else { continue };
            resolved.push(v.sig);
            if let Some(p) = self.pending.get_mut(k) {
                p.settled.insert(v.sig);
            }
            if ended.iter().any(|(e, _)| e == k) {
                if let Some(j) = journal {
                    let s = if st.err.is_none() { "landed" } else { "failed" };
                    let _ = j.set_status(&v.sig.to_string(), s, Some(st.slot), st.code);
                }
                continue;
            }
            if let Some(o) = self.classify(k, st, v, journal, &mut report) {
                ended.push((k.clone(), o));
            }
        }
        for (k, o) in ended {
            if let Some(p) = self.pending.remove(&k) {
                let s = self.stats.entry(p.spec.kind).or_default();
                match &o {
                    Outcome::Landed {
                        slot: ls,
                        first_slot,
                        ..
                    } => {
                        self.failures.remove(&k);
                        s.landed += 1;
                        s.latency_slots.push(ls.saturating_sub(*first_slot));
                    }
                    Outcome::Failed { .. } => {
                        s.failed += 1;
                        let n = self.failures.entry(k.clone()).or_default();
                        *n = n.saturating_add(1);
                        // 2, 4, 8, … slots, at most 1,024 (≈ 7 min of slots).
                        let wait = BACKOFF_SLOTS << (*n - 1).min(9);
                        self.backoff.insert(k.clone(), slot + wait);
                    }
                    Outcome::Dead { .. } => s.dead += 1,
                }
                self.orphans.extend(
                    p.versions
                        .iter()
                        .filter(|v| !resolved.contains(&v.sig))
                        .map(|v| (v.sig, v.sent_slot)),
                );
                report.outcomes.push((k, o));
            }
        }
        // Orphans: record their outcome, drop them once expired.
        if !self.orphans.is_empty() {
            let os: Vec<Signature> = self.orphans.iter().map(|o| o.0).collect();
            let sts = port.statuses(&os).await.unwrap_or_default();
            let mut keep = vec![];
            for (o, st) in self
                .orphans
                .iter()
                .zip(sts.iter().chain(std::iter::repeat(&None)))
            {
                match st {
                    Some(st) => {
                        if let Some(j) = journal {
                            let s = if st.err.is_none() { "landed" } else { "failed" };
                            let _ = j.set_status(&o.0.to_string(), s, Some(st.slot), st.code);
                        }
                    }
                    None if slot > o.1 + EXPIRY_SLOTS => {
                        if let Some(j) = journal {
                            let _ = j.set_status(&o.0.to_string(), "expired", None, None);
                        }
                    }
                    None => keep.push(*o),
                }
            }
            self.orphans = keep;
        }
        report
    }

    /// Sends the next version of every write still waiting.
    pub async fn send<P: ChainPort>(
        &mut self,
        port: &P,
        payers: &mut Payers,
        journal: Option<&Journal>,
        slot: u64,
        report: &mut StepReport,
    ) {
        let keys: Vec<String> = self.pending.keys().cloned().collect();
        let mut blockhash: Option<Hash> = None;
        for k in keys {
            let Some(p) = self.pending.get(&k) else {
                continue;
            };
            if slot < p.spec.not_before_slot
                || p.versions.last().is_some_and(|v| v.sent_slot >= slot)
            {
                continue;
            }
            if p.spec.deadline_slot.is_some_and(|d| slot >= d) {
                let p = self.pending.remove(&k).expect("pending");
                self.orphans
                    .extend(p.versions.iter().map(|v| (v.sig, v.sent_slot)));
                self.stats.entry(p.spec.kind).or_default().dead += 1;
                report.outcomes.push((
                    k,
                    Outcome::Dead {
                        reason: "deadline".into(),
                        code: None,
                    },
                ));
                continue;
            }
            // Every version expired unlanded: the write is at its version or
            // spend cap (otherwise a new version went out each slot) or its
            // sends fail. End it with a backoff so the duty re-plans a fresh
            // escalation; a write that can never send again must not stay
            // pending (integ-W2 review of W2-F: a hold longer than 64
            // versions + expiry parked it, and the anchor scan window with
            // it, until a restart).
            if !p.versions.is_empty()
                && p.versions.iter().all(|v| slot > v.sent_slot + EXPIRY_SLOTS)
            {
                let p = self.pending.remove(&k).expect("pending");
                self.orphans.extend(
                    p.versions
                        .iter()
                        .filter(|v| !p.settled.contains(&v.sig))
                        .map(|v| (v.sig, v.sent_slot)),
                );
                self.stats.entry(p.spec.kind).or_default().failed += 1;
                let n = self.failures.entry(k.clone()).or_default();
                *n = n.saturating_add(1);
                let wait = BACKOFF_SLOTS << (*n - 1).min(9);
                self.backoff.insert(k.clone(), slot + wait);
                let err = format!(
                    "{} versions expired unlanded (cap {} versions, {} lamports)",
                    p.versions.len(),
                    self.params.max_versions,
                    self.params.per_write_cap
                );
                if let Some(j) = journal {
                    let _ = j.alert(slot, "write-expired", &format!("{k}: {err}"));
                }
                report.outcomes.push((
                    k,
                    Outcome::Failed {
                        code: None,
                        err,
                        slot,
                    },
                ));
                continue;
            }
            let fees_out: u64 = p.versions.iter().map(|v| v.fee).sum();
            if p.versions.len() as u32 >= self.params.max_versions {
                continue;
            }
            let slots_waiting = p.first_slot.map_or(0, |f| slot.saturating_sub(f));
            let bid = self.params.bid(p.spec.class, slots_waiting) + p.bonus_milli;
            let payer = match p.spec.fixed_payer {
                Some(a) => match payers.keypair(&a) {
                    Some(k) => k.insecure_clone(),
                    None => {
                        report
                            .send_errors
                            .push((k.clone(), format!("no keypair for fixed payer {a}")));
                        continue;
                    }
                },
                None => payers.draw(p.spec.class).insecure_clone(),
            };
            let ctx = BuildCtx {
                payer: payer.pubkey(),
                version: p.versions.len() as u32,
                slots_waiting,
            };
            let ixs = (p.spec.build)(&ctx);
            let b = self.budgets.get(p.spec.tag);
            let mut budget = tx::TxBudget {
                cu_limit: p.cu_limit,
                cu_price: 0,
                loaded_limit: b.loaded_limit,
                heap: p.heap,
            };
            if blockhash.is_none() {
                match port.blockhash().await {
                    Ok((h, _)) => blockhash = Some(h),
                    Err(e) => {
                        report
                            .send_errors
                            .push((k.clone(), format!("blockhash: {e}")));
                        return;
                    }
                }
            }
            // `Hash` is `Copy` only under solana-hash's `copy` feature, which
            // this crate alone does not enable: rebuild it from its bytes.
            let bh = blockhash
                .as_ref()
                .map(|h| Hash::new_from_array(h.to_bytes()))
                .expect("blockhash");
            let shape = tx::shape(&tx::message(&ixs, &budget, &ctx.payer, &bh));
            let cost = fees::cost(
                budget.cu_limit,
                shape.sigs as u8,
                shape.writes as u8,
                budget.loaded_limit,
            );
            budget.cu_price = fees::cu_price_for(bid, cost, budget.cu_limit);
            let fee = fees::LAMPORTS_PER_SIGNATURE * shape.sigs as u64
                + fees::priority_fee(budget.cu_price, budget.cu_limit);
            if !p.versions.is_empty() && fees_out + fee > self.params.per_write_cap {
                continue; // wait on the versions already out (§8.2 duplicates bound)
            }
            let t = match tx::build(&ixs, &budget, &[&payer], &bh) {
                Ok(t) => t,
                Err(e) => {
                    report.send_errors.push((k.clone(), e));
                    continue;
                }
            };
            let wire = tx::wire(&t);
            let sig = tx::signature(&t);
            if let Err(e) = port.send(&wire).await {
                report.send_errors.push((k.clone(), e.to_string()));
                continue;
            }
            payers.note_spend(&ctx.payer, fee);
            let p = self.pending.get_mut(&k).expect("pending");
            if p.first_slot.is_none() {
                p.first_slot = Some(slot);
            }
            let v = Version {
                sig,
                payer: ctx.payer,
                bid_milli: bid,
                cu_limit: budget.cu_limit,
                heap: budget.heap,
                sent_slot: slot,
                fee,
            };
            if let Some(j) = journal {
                let _ = j.record_attempt(&Attempt {
                    sig: sig.to_string(),
                    kind: p.spec.kind.into(),
                    object_key: k.clone(),
                    bell: p.spec.bell,
                    region: p.spec.region,
                    class: p.spec.class.letter().into(),
                    payer: ctx.payer.to_string(),
                    bid_milli: bid,
                    cu_limit: budget.cu_limit,
                    heap: budget.heap,
                    first_valid_slot: slot,
                    sent_slot: slot,
                    landed_slot: None,
                    status: "sent".into(),
                    code: None,
                });
            }
            p.versions.push(v);
            self.stats.entry(p.spec.kind).or_default().versions += 1;
            report.sent += 1;
            // Contested: not landed `contested_slots` after a bid ≥ p_tip.
            let first = p.first_slot.unwrap_or(slot);
            if !p.contested
                && slot.saturating_sub(first) >= self.params.contested_slots
                && p.versions.iter().any(|v| {
                    v.bid_milli >= self.params.p_tip_milli
                        && v.sent_slot + self.params.contested_slots <= slot
                })
            {
                p.contested = true;
                report
                    .contested
                    .push((p.spec.bell, p.spec.region, k.clone()));
                if let Some(j) = journal {
                    let _ = j.alert(slot, "contested", &k);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::KeeperConfig;
    use fclient::ports::{Account, ClockSysvar, Cursor, PortResult, SimResult, TxRecord};
    use std::sync::Mutex;

    type Answer = Box<dyn FnMut(&fclient::Transaction) -> Option<Status> + Send>;

    /// A chain that records every send and answers statuses from a script.
    struct Script {
        sent: Mutex<Vec<fclient::Transaction>>,
        /// The status a transaction gets once sent (`None` = never lands).
        answer: Mutex<Answer>,
        statuses: Mutex<HashMap<Signature, Status>>,
    }

    impl Default for Script {
        fn default() -> Self {
            Script {
                sent: Mutex::new(vec![]),
                answer: Mutex::new(Box::new(|_| None)),
                statuses: Mutex::new(HashMap::new()),
            }
        }
    }

    impl ChainPort for Script {
        async fn clock(&self) -> PortResult<ClockSysvar> {
            Ok(ClockSysvar::default())
        }
        async fn accounts(&self, keys: &[Address], _: u64) -> PortResult<Vec<Option<Account>>> {
            Ok(keys
                .iter()
                .map(|_| {
                    Some(Account {
                        lamports: 10_000_000_000,
                        data: vec![],
                        owner: fclient::addr::system_program(),
                        executable: false,
                    })
                })
                .collect())
        }
        async fn simulate(&self, _: &[u8]) -> PortResult<SimResult> {
            Ok(SimResult::default())
        }
        async fn send(&self, w: &[u8]) -> PortResult<Signature> {
            let t = tx::from_wire(w).unwrap();
            let sig = tx::signature(&t);
            if let Some(st) = (self.answer.lock().unwrap())(&t) {
                self.statuses.lock().unwrap().insert(sig, st);
            }
            self.sent.lock().unwrap().push(t);
            Ok(sig)
        }
        async fn statuses(&self, sigs: &[Signature]) -> PortResult<Vec<Option<Status>>> {
            let m = self.statuses.lock().unwrap();
            Ok(sigs.iter().map(|s| m.get(s).cloned()).collect())
        }
        async fn feed(&self, _: Cursor) -> PortResult<Vec<TxRecord>> {
            Ok(vec![])
        }
        async fn blockhash(&self) -> PortResult<(Hash, u64)> {
            Ok((Hash::new_from_array([3; 32]), 150))
        }
    }

    fn payers() -> Payers {
        let mut c = KeeperConfig::new(
            Address::new_from_array([1; 32]),
            1,
            Address::new_from_array([2; 32]),
        );
        c.delay_floor = 1;
        c.reveal_floor = Some(1);
        Payers::new(&[9u8; 32], &c).unwrap()
    }

    fn spec(key: &str, class: Class, tag: u8) -> WriteSpec {
        WriteSpec {
            key: key.into(),
            kind: "test",
            tag,
            class,
            bell: Some(3),
            region: Some(4),
            build: Arc::new(|c: &BuildCtx| {
                vec![Instruction {
                    program_id: Address::new_from_array([7; 32]),
                    accounts: vec![solana_instruction::AccountMeta::new(c.payer, true)],
                    data: vec![c.version as u8],
                }]
            }),
            deadline_slot: None,
            not_before_slot: 0,
            fixed_payer: None,
        }
    }

    fn priority_of(t: &fclient::Transaction) -> u64 {
        tx::priority(&t.message).0
    }

    #[test]
    fn bids_double_per_slot_to_the_class_cap() {
        let p = EngineParams::default();
        let w: Vec<u64> = (0..5).map(|s| p.bid(Class::W, s)).collect();
        assert_eq!(w, vec![433, 866, 1_732, 2_000, 2_000]);
        let d: Vec<u64> = (0..5).map(|s| p.bid(Class::D, s)).collect();
        assert_eq!(d, vec![100, 200, 400, 500, 500]);
        assert_eq!(p.bid(Class::N, 9), 100);
        let capped = EngineParams {
            p_def_milli: 1_500,
            ..p
        };
        assert_eq!(capped.bid(Class::W, 9), 1_500, "season defence cap");
    }

    /// Nothing lands: one version per slot, each from a newly drawn payer
    /// with a new signature, priorities ×2 per slot to P_def, contested
    /// after 2 slots at ≥ p_tip, then the spend cap stops new versions.
    #[tokio::test]
    async fn unlanded_w_write_escalates_with_new_payers() {
        let port = Script::default();
        let mut payers = payers();
        payers.refresh(&port, 0).await.unwrap();
        let mut e = Engine::new(Budgets::placeholder(), EngineParams::default());
        let j = Journal::open(std::path::Path::new(":memory:")).unwrap();
        assert!(e.ensure(spec("w", Class::W, abi::tag::REVEAL), 10));
        assert!(
            !e.ensure(spec("w", Class::W, abi::tag::REVEAL), 10),
            "deduplicated"
        );
        let mut contested = vec![];
        for s in 10..16 {
            let r = e.step(&port, &mut payers, Some(&j), s).await;
            assert_eq!(r.sent, 1, "one version per slot");
            contested.extend(r.contested);
            // A second step in the same slot sends nothing.
            assert_eq!(e.step(&port, &mut payers, Some(&j), s).await.sent, 0);
        }
        let sent = port.sent.lock().unwrap().clone();
        let pr: Vec<u64> = sent.iter().map(priority_of).collect();
        for w in pr.windows(2) {
            assert!(w[1] >= w[0], "never lower: {pr:?}");
        }
        assert!((430..=436).contains(&pr[0]), "starts at p_tip: {pr:?}");
        assert!((1_990..=2_010).contains(&pr[5]), "ends at P_def: {pr:?}");
        let sigs: std::collections::HashSet<_> = sent.iter().map(tx::signature).collect();
        assert_eq!(sigs.len(), 6, "a new signature per version");
        let pool = payers.reveal.addresses();
        assert!(sent
            .iter()
            .all(|t| pool.contains(&t.message.account_keys[0])));
        assert_eq!(contested.len(), 1, "contested once");
        assert_eq!(j.in_flight().unwrap().len(), 6);
        // Spend cap: a tiny cap stops new versions after the first.
        let mut e2 = Engine::new(
            Budgets::placeholder(),
            EngineParams {
                per_write_cap: 1,
                ..EngineParams::default()
            },
        );
        e2.ensure(spec("w2", Class::W, abi::tag::REVEAL), 0);
        let n: usize = {
            let mut n = 0;
            for s in 0..5 {
                n += e2.step(&port, &mut payers, None, s).await.sent;
            }
            n
        };
        assert_eq!(n, 1);
    }

    /// The first version fails ComputationalBudgetExceeded, the second a
    /// heap fault; the third (1.4M-limit rung ladder + heap frame) lands.
    #[tokio::test]
    async fn cu_and_heap_retry_ladder() {
        let port = Script::default();
        let calls = std::sync::atomic::AtomicU32::new(0);
        let calls = Arc::new(calls);
        let c2 = calls.clone();
        *port.answer.lock().unwrap() = Box::new(move |t| {
            let n = c2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let b = tx::parse_budget(&t.message);
            Some(match n {
                0 => {
                    assert_eq!(b.cu_limit, Some(340_000));
                    Status {
                        slot: 1,
                        err: Some("InstructionError(3, ComputationalBudgetExceeded)".into()),
                        code: None,
                    }
                }
                1 => {
                    assert_eq!(b.cu_limit, Some(680_000));
                    assert_eq!(b.heap, None);
                    Status {
                        slot: 2,
                        err: Some("InstructionError(3, ProgramFailedToComplete)".into()),
                        code: None,
                    }
                }
                _ => {
                    assert_eq!(b.cu_limit, Some(680_000));
                    assert_eq!(b.heap, Some(262_144));
                    Status {
                        slot: 3,
                        err: None,
                        code: None,
                    }
                }
            })
        });
        let mut payers = payers();
        payers.refresh(&port, 0).await.unwrap();
        let mut e = Engine::new(Budgets::placeholder(), EngineParams::default());
        e.ensure(spec("r", Class::D, abi::tag::RESOLVE_FROM_INPUTS), 0);
        let mut ladder = vec![];
        let mut landed = None;
        for s in 0..6 {
            let r = e.step(&port, &mut payers, None, s).await;
            ladder.extend(r.ladder);
            for (_, o) in r.outcomes {
                landed = Some(o);
            }
        }
        assert_eq!(
            ladder.iter().map(|l| (l.1, l.2)).collect::<Vec<_>>(),
            vec![(680_000, None), (680_000, Some(262_144))]
        );
        assert!(matches!(landed, Some(Outcome::Landed { versions: 3, .. })));
        assert_eq!(Budgets::retry_cu(680_000), 1_360_000);
        assert_eq!(Budgets::retry_cu(1_360_000), abi::CU_MAX);
    }

    /// v1.7 (wave-4 review): a SkipQuiet that runs out of CU within its
    /// gate is `NotQuiet` for the duty (one version, no ladder step), so the
    /// keeper gathers and resolves the bell instead.
    #[tokio::test]
    async fn skip_quiet_out_of_cu_is_not_quiet() {
        let port = Script::default();
        *port.answer.lock().unwrap() = Box::new(|_| {
            Some(Status {
                slot: 1,
                err: Some("InstructionError(2, ComputationalBudgetExceeded)".into()),
                code: None,
            })
        });
        let mut payers = payers();
        payers.refresh(&port, 0).await.unwrap();
        let mut e = Engine::new(Budgets::placeholder(), EngineParams::default());
        let j = Journal::open(std::path::Path::new(":memory:")).unwrap();
        e.ensure(spec("skip:3,0:10:24", Class::D, abi::tag::SKIP_QUIET), 0);
        let mut out = None;
        let mut ladder = vec![];
        for s in 0..4 {
            let r = e.step(&port, &mut payers, Some(&j), s).await;
            ladder.extend(r.ladder);
            if let Some((_, o)) = r.outcomes.into_iter().next() {
                out = Some(o);
                break;
            }
        }
        assert!(ladder.is_empty(), "no ladder step");
        assert!(
            matches!(out, Some(Outcome::Failed { code: Some(c), .. }) if c == err::NOT_QUIET),
            "{out:?}"
        );
    }

    /// A write held past its version cap and blockhash expiry ends (Failed,
    /// with a backoff) instead of staying pending; the re-planned write
    /// starts a fresh escalation and lands once the hold ends (integ-W2
    /// review of W2-F: `long_hold`).
    #[tokio::test]
    async fn capped_write_whose_versions_expired_ends_and_restarts() {
        let port = Script::default();
        let mut payers = payers();
        payers.refresh(&port, 0).await.unwrap();
        let mut e = Engine::new(
            Budgets::placeholder(),
            EngineParams {
                max_versions: 3,
                ..EngineParams::default()
            },
        );
        let j = Journal::open(std::path::Path::new(":memory:")).unwrap();
        assert!(e.ensure(spec("a", Class::D, abi::tag::POST_ANCHOR), 0));
        let mut ended = None;
        let mut sent = 0;
        let mut end_slot = 0;
        for s in 0..200 {
            let r = e.step(&port, &mut payers, Some(&j), s).await;
            sent += r.sent;
            if let Some((_, o)) = r.outcomes.into_iter().next() {
                ended = Some(o);
                end_slot = s;
                break;
            }
        }
        assert_eq!(sent, 3, "the version cap");
        assert_eq!(
            end_slot,
            2 + EXPIRY_SLOTS + 1,
            "ends once the last version expired"
        );
        assert!(matches!(ended, Some(Outcome::Failed { code: None, .. })));
        assert!(!e.is_pending("a"));
        assert!(
            !e.ensure(spec("a", Class::D, abi::tag::POST_ANCHOR), end_slot),
            "backoff"
        );
        // The hold ends: the next version lands, and the escalation restarts.
        *port.answer.lock().unwrap() = Box::new(|_| {
            Some(Status {
                slot: 400,
                err: None,
                code: None,
            })
        });
        assert!(e.ensure(spec("a", Class::D, abi::tag::POST_ANCHOR), end_slot + 2));
        let mut landed = false;
        for s in end_slot + 2..end_slot + 6 {
            for (_, o) in e.step(&port, &mut payers, Some(&j), s).await.outcomes {
                landed |= matches!(o, Outcome::Landed { versions: 1, .. });
            }
        }
        assert!(landed, "the re-planned write lands with its first version");
        let last = port.sent.lock().unwrap().last().map(priority_of).unwrap();
        assert!(
            (95..=105).contains(&last),
            "a fresh escalation starts low: {last}"
        );
    }

    /// AlreadyDone (52) is success; WindowClosed and NotImplemented end the
    /// write; another code is a failure with a backoff.
    #[tokio::test]
    async fn codes_end_writes_as_the_keeper_mapping_says() {
        for (code, want) in [(52u32, "landed"), (12, "dead"), (99, "dead"), (8, "failed")] {
            let port = Script::default();
            *port.answer.lock().unwrap() = Box::new(move |_| {
                Some(Status {
                    slot: 5,
                    err: Some(format!("InstructionError(3, Custom({code}))")),
                    code: Some(code),
                })
            });
            let mut payers = payers();
            payers.refresh(&port, 0).await.unwrap();
            let mut e = Engine::new(Budgets::placeholder(), EngineParams::default());
            e.ensure(spec("x", Class::D, abi::tag::POST_SEED), 0);
            let mut out = None;
            for s in 0..3 {
                for (_, o) in e.step(&port, &mut payers, None, s).await.outcomes {
                    out = Some(o);
                }
            }
            let got = match out.unwrap() {
                Outcome::Landed { already_done, .. } => {
                    assert!(already_done);
                    "landed"
                }
                Outcome::Dead { .. } => "dead",
                Outcome::Failed { code, .. } => {
                    assert_eq!(code, Some(8));
                    assert!(
                        !e.ensure(spec("x", Class::D, abi::tag::POST_SEED), 2),
                        "backoff"
                    );
                    assert!(e.ensure(spec("x", Class::D, abi::tag::POST_SEED), 4));
                    "failed"
                }
            };
            assert_eq!(got, want, "code {code}");
        }
        // NoTicket (23) ends a SettleTicket as done, and only a SettleTicket.
        assert!(err::is_done(abi::tag::SETTLE_TICKET, 23));
        assert!(!err::is_done(abi::tag::POST_SEED, 23));
        assert!(err::is_done(abi::tag::POST_SEED, 52));
    }
}
