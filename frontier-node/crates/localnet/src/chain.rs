//! The chain: LiteSVM with the mainnet feature set (SBPF v2, the SIMD-0388
//! BLS12-381 syscalls, 64 locks), mainnet rent (5,080 lamports per byte incl.
//! the 128-B overhead), a block builder per slot, a scaled Clock, SIMD-0186
//! loaded-data enforcement and an ordered transaction feed.
//!
//! **Time (I-54).** A slot is 400 ms of real time at every scale; the Clock
//! advances `0.4 × scale` game seconds per slot. The ticker in
//! [`crate::server`] calls [`Chain::produce_block`] every 400 ms;
//! [`crate::InProcess`] calls it on demand (virtual time).
//!
//! **Blocks.** Pending transactions are ordered by priority (§10.1, highest
//! first; ties by arrival) and executed in sequence while the block's
//! requested CU stay ≤ 100M and each writable account's ≤ 40M; the rest wait
//! for the next slot (and are dropped when their blockhash expires).
//!
//! **Loaded data (I-45).** Before execution the node computes SIMD-0186's
//! loaded size — Σ(data + 64) over the message's accounts (the instructions
//! sysvar counts 0) plus, for every invoked LoaderV3 program, its ProgramData
//! (data + 64) — and fails a transaction above its requested limit with
//! `MaxLoadedAccountsDataSizeExceeded`, **fee charged**. LiteSVM 0.16 counts
//! the listed accounts but not the ProgramData, and charges nothing when
//! loading fails (`litesvm_alone_undercounts_programdata` shows it), so the
//! check lives here.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use litesvm::LiteSVM;
use solana_account::Account as SolAccount;
use solana_address::Address;
use solana_clock::Clock;
use solana_hash::Hash;
use solana_message::Message;
use solana_signature::Signature;
use solana_transaction::Transaction;

use fclient::ports::{Account, ClockSysvar, SimResult, Status, TxRecord};
use fclient::{addr, fees, tx};

/// Real duration of a slot.
pub const SLOT_MS: u64 = 400;
/// Block CU cap and per-writable-account cap (mainnet).
pub const BLOCK_CU: u64 = 100_000_000;
pub const ACCOUNT_CU: u64 = 40_000_000;
/// A blockhash stays valid this many slots.
pub const BLOCKHASH_SLOTS: u64 = 150;
/// Mainnet rent per byte (incl. the 128-B account overhead).
pub const LAMPORTS_PER_BYTE: u64 = 5_080;
/// SIMD-0186 per-account base.
pub const ACCOUNT_BASE: u64 = 64;

/// Node configuration.
#[derive(Clone, Debug)]
pub struct Config {
    /// Game seconds per real second (1 = real time, 20 = Mode A).
    pub scale: f64,
    /// Clock `unix_timestamp` at slot 0 (a past date, so every round exists).
    pub g0: i64,
    /// First slot.
    pub slot0: u64,
    /// `frontier_setAccount` allowed (tamper fixtures only).
    pub allow_tamper: bool,
}

impl Default for Config {
    fn default() -> Self {
        // 2026-08-01T00:00:00Z.
        Config {
            scale: 1.0,
            g0: 1_785_542_400,
            slot0: 1,
            allow_tamper: false,
        }
    }
}

/// Why a transaction was refused before it could land (not charged).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SendError {
    Decode(String),
    BlockhashNotFound,
    AlreadyProcessed,
    /// Signature verification failed or the message does not sanitize.
    Invalid(String),
    /// The fee payer cannot pay the fee.
    InsufficientFundsForFee,
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

struct Pending {
    tx: Transaction,
    wire: Vec<u8>,
    arrival: u64,
    priority_milli: u64,
    cu_limit: u64,
    blockhash_slot: u64,
}

/// A landed or failed-and-charged transaction.
#[derive(Clone, Debug)]
pub struct Landed {
    pub seq: u64,
    pub slot: u64,
    pub block_time: i64,
    pub signature: Signature,
    pub wire: Vec<u8>,
    pub keys: Vec<Address>,
    pub logs: Vec<String>,
    pub err: Option<String>,
    pub code: Option<u32>,
    pub units: u64,
    pub fee: u64,
    pub post: Vec<(Address, Option<Account>)>,
    pub return_data: Vec<u8>,
}

impl Landed {
    pub fn record(&self) -> TxRecord {
        TxRecord {
            seq: self.seq,
            slot: self.slot,
            signature: self.signature,
            block_time: self.block_time,
            tx: self.wire.clone(),
            logs: self.logs.clone(),
            err: self.err.clone(),
            code: self.code,
            units: self.units,
            fee: self.fee,
            post: self.post.clone(),
        }
    }
}

/// What one block did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockReport {
    pub slot: u64,
    pub unix_timestamp: i64,
    pub landed: usize,
    pub failed: usize,
    pub deferred: usize,
    pub dropped: usize,
    pub cu: u64,
}

pub struct Chain {
    pub svm: LiteSVM,
    cfg: Config,
    slot: u64,
    /// Game time in milliseconds (the Clock shows its floor in seconds).
    game_ms: i128,
    scale: f64,
    pending_scale: Option<f64>,
    paused: bool,
    blockhashes: VecDeque<(Hash, u64)>,
    mempool: Vec<Pending>,
    arrivals: u64,
    landed: Vec<Landed>,
    by_sig: HashMap<Signature, usize>,
    block_times: BTreeMap<u64, i64>,
    /// Signatures refused at execution without a charge (e.g. an
    /// `InsufficientFundsForFee` at block time); reported as absent.
    dropped: HashSet<Signature>,
}

fn to_account(a: SolAccount) -> Account {
    Account {
        lamports: a.lamports,
        data: a.data,
        owner: a.owner,
        executable: a.executable,
    }
}

impl Chain {
    pub fn new(cfg: Config) -> Chain {
        let mut svm = LiteSVM::new()
            .with_blockhash_check(false)
            .with_log_bytes_limit(Some(200_000))
            .with_transaction_history(0);
        let mut rent: solana_rent::Rent = svm.get_sysvar();
        rent.lamports_per_byte = LAMPORTS_PER_BYTE;
        #[allow(deprecated)]
        {
            rent.exemption_threshold = 1.0f64.to_le_bytes();
        }
        svm.set_sysvar(&rent);
        let mut c = Chain {
            svm,
            slot: cfg.slot0,
            game_ms: cfg.g0 as i128 * 1_000,
            scale: cfg.scale,
            cfg,
            pending_scale: None,
            paused: false,
            blockhashes: VecDeque::new(),
            mempool: vec![],
            arrivals: 0,
            landed: vec![],
            by_sig: HashMap::new(),
            block_times: BTreeMap::new(),
            dropped: HashSet::new(),
        };
        c.write_clock();
        let h = c.svm.latest_blockhash();
        c.blockhashes.push_back((h, c.slot));
        c.block_times.insert(c.slot, c.unix_timestamp());
        c
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }
    pub fn slot(&self) -> u64 {
        self.slot
    }
    pub fn unix_timestamp(&self) -> i64 {
        (self.game_ms.div_euclid(1_000)) as i64
    }
    pub fn scale(&self) -> f64 {
        self.scale
    }
    pub fn paused(&self) -> bool {
        self.paused
    }
    pub fn set_paused(&mut self, p: bool) {
        self.paused = p;
    }
    /// Takes effect at the next slot boundary (`frontier_setScale`).
    pub fn set_scale(&mut self, s: f64) {
        self.pending_scale = Some(s);
    }
    pub fn clock(&self) -> ClockSysvar {
        ClockSysvar {
            slot: self.slot,
            epoch_start_timestamp: self.cfg.g0,
            epoch: 0,
            leader_schedule_epoch: 1,
            unix_timestamp: self.unix_timestamp(),
        }
    }
    pub fn block_time(&self, slot: u64) -> Option<i64> {
        self.block_times.get(&slot).copied()
    }

    fn write_clock(&mut self) {
        let c = self.clock();
        self.svm.set_sysvar(&Clock {
            slot: c.slot,
            epoch_start_timestamp: c.epoch_start_timestamp,
            epoch: c.epoch,
            leader_schedule_epoch: c.leader_schedule_epoch,
            unix_timestamp: c.unix_timestamp,
        });
    }

    pub fn latest_blockhash(&self) -> (Hash, u64) {
        let (h, s) = *self.blockhashes.back().expect("a blockhash");
        (h, s + BLOCKHASH_SLOTS)
    }

    fn blockhash_slot(&self, h: &Hash) -> Option<u64> {
        self.blockhashes
            .iter()
            .find(|(x, _)| x == h)
            .map(|(_, s)| *s)
    }

    // ------------------------------------------------------------ programs and accounts

    /// Deploys `so` under LoaderV3 at `program` with ProgramData of `max_len`
    /// bytes (≥ the ELF; zero padded as a real `--max-len` deploy) and the
    /// given upgrade authority (AnnounceSeason reads it, I-51).
    pub fn deploy(
        &mut self,
        program: Address,
        so: &[u8],
        max_len: usize,
        authority: Option<Address>,
    ) -> Result<(), String> {
        let loader = addr::address(fclient::abi::LOADER_V3);
        let pd = addr::programdata(&program);
        let max_len = max_len.max(so.len());
        let mut data = vec![0u8; fees::PROGRAMDATA_META as usize + max_len];
        data[..4].copy_from_slice(&3u32.to_le_bytes()); // UpgradeableLoaderState::ProgramData
        data[4..12].copy_from_slice(&self.slot.to_le_bytes());
        if let Some(a) = authority {
            data[12] = 1;
            data[13..45].copy_from_slice(a.as_ref());
        }
        data[45..45 + so.len()].copy_from_slice(so);
        let lamports = self.svm.minimum_balance_for_rent_exemption(data.len());
        self.svm
            .set_account(
                pd,
                SolAccount {
                    lamports,
                    data,
                    owner: loader,
                    executable: false,
                    rent_epoch: 0,
                },
            )
            .map_err(|e| format!("programdata: {e:?}"))?;
        let mut pdata = vec![0u8; 36];
        pdata[..4].copy_from_slice(&2u32.to_le_bytes()); // UpgradeableLoaderState::Program
        pdata[4..].copy_from_slice(pd.as_ref());
        let lamports = self.svm.minimum_balance_for_rent_exemption(36);
        self.svm
            .set_account(
                program,
                SolAccount {
                    lamports,
                    data: pdata,
                    owner: loader,
                    executable: true,
                    rent_epoch: 0,
                },
            )
            .map_err(|e| format!("program: {e:?}"))
    }

    pub fn account(&self, k: &Address) -> Option<Account> {
        self.svm
            .get_account(k)
            .filter(|a| a.lamports > 0 || !a.data.is_empty())
            .map(to_account)
    }

    pub fn balance(&self, k: &Address) -> u64 {
        self.svm.get_balance(k).unwrap_or(0)
    }

    pub fn set_account(&mut self, k: Address, a: Account) -> Result<(), String> {
        self.svm
            .set_account(
                k,
                SolAccount {
                    lamports: a.lamports,
                    data: a.data,
                    owner: a.owner,
                    executable: a.executable,
                    rent_epoch: 0,
                },
            )
            .map_err(|e| format!("{e:?}"))
    }

    pub fn program_accounts(&self, program: &Address) -> Vec<(Address, Account)> {
        self.svm
            .get_program_accounts(program)
            .into_iter()
            .map(|(k, a)| (k, to_account(a)))
            .collect()
    }

    /// Credits `lamports` at once (not through a block), recorded in the
    /// status table at the current slot.
    pub fn airdrop(&mut self, k: &Address, lamports: u64) -> Result<Signature, String> {
        let r = self
            .svm
            .airdrop(k, lamports)
            .map_err(|e| format!("{:?}", e.err))?;
        let sig = r.signature;
        let seq = self.landed.len() as u64 + 1;
        self.landed.push(Landed {
            seq,
            slot: self.slot,
            block_time: self.unix_timestamp(),
            signature: sig,
            wire: vec![],
            keys: vec![*k],
            logs: r.logs,
            err: None,
            code: None,
            units: r.compute_units_consumed,
            fee: r.fee,
            post: vec![],
            return_data: vec![],
        });
        self.by_sig.insert(sig, self.landed.len() - 1);
        Ok(sig)
    }

    // ------------------------------------------------------------ loaded data (I-45)

    /// SIMD-0186 loaded size of `msg` against the current state.
    pub fn loaded_size(&self, msg: &Message) -> u64 {
        let ix_sysvar = addr::instructions_sysvar();
        let loader = addr::address(fclient::abi::LOADER_V3);
        let keys: HashSet<Address> = msg.account_keys.iter().copied().collect();
        let mut total = 0u64;
        for k in &msg.account_keys {
            if *k == ix_sysvar {
                continue;
            }
            if let Some(a) = self.svm.get_account(k) {
                total += ACCOUNT_BASE + a.data.len() as u64;
            }
        }
        let mut counted: HashSet<Address> = HashSet::new();
        for ci in &msg.instructions {
            let pid = msg.account_keys[ci.program_id_index as usize];
            let Some(p) = self.svm.get_account(&pid) else {
                continue;
            };
            if p.owner != loader || p.data.len() < 36 || p.data[..4] != 2u32.to_le_bytes() {
                continue;
            }
            let pd = Address::new_from_array(p.data[4..36].try_into().expect("32"));
            if keys.contains(&pd) || !counted.insert(pd) {
                continue;
            }
            if let Some(a) = self.svm.get_account(&pd) {
                total += ACCOUNT_BASE + a.data.len() as u64;
            }
        }
        total
    }

    // ------------------------------------------------------------ transactions

    fn decode(wire: &[u8]) -> Result<Transaction, SendError> {
        tx::from_wire(wire).map_err(SendError::Decode)
    }

    /// Accepts a transaction into the mempool (sendTransaction).
    pub fn submit(&mut self, wire: &[u8]) -> Result<Signature, SendError> {
        let t = Self::decode(wire)?;
        let sig = tx::signature(&t);
        if self.by_sig.contains_key(&sig)
            || self.mempool.iter().any(|p| tx::signature(&p.tx) == sig)
        {
            return Err(SendError::AlreadyProcessed);
        }
        let bslot = self
            .blockhash_slot(&t.message.recent_blockhash)
            .ok_or(SendError::BlockhashNotFound)?;
        t.verify().map_err(|e| SendError::Invalid(e.to_string()))?;
        let (priority_milli, _) = tx::priority(&t.message);
        let cu_limit = tx::parse_budget(&t.message).effective_cu_limit() as u64;
        self.arrivals += 1;
        self.dropped.remove(&sig);
        self.mempool.push(Pending {
            tx: t,
            wire: wire.to_vec(),
            arrival: self.arrivals,
            priority_milli,
            cu_limit,
            blockhash_slot: bslot,
        });
        Ok(sig)
    }

    /// Simulates against the current state (with the loaded-data check).
    pub fn simulate(&self, wire: &[u8], sig_verify: bool) -> Result<SimResult, SendError> {
        let t = Self::decode(wire)?;
        if sig_verify {
            t.verify().map_err(|e| SendError::Invalid(e.to_string()))?;
        }
        let keys = t.message.account_keys.clone();
        let limit = tx::parse_budget(&t.message).effective_loaded_limit() as u64;
        let need = self.loaded_size(&t.message);
        if need > limit {
            return Ok(SimResult {
                err: Some("MaxLoadedAccountsDataSizeExceeded".into()),
                code: None,
                logs: vec![],
                units: 0,
                accounts: keys.iter().map(|k| (*k, self.account(k))).collect(),
            });
        }
        let post_state = |post: &[(Address, solana_account::AccountSharedData)]| -> Vec<(Address, Option<Account>)> {
            keys.iter()
                .map(|k| {
                    let a = post.iter().find(|(x, _)| x == k).map(|(_, a)| to_account(SolAccount::from(a.clone()))).or_else(|| self.account(k));
                    (*k, a)
                })
                .collect()
        };
        Ok(match self.svm.simulate_transaction(t.clone()) {
            Ok(info) => SimResult {
                err: None,
                code: None,
                logs: info.meta.logs,
                units: info.meta.compute_units_consumed,
                accounts: post_state(&info.post_accounts),
            },
            Err(f) => {
                let err = format!("{:?}", f.err);
                SimResult {
                    code: fclient::ports::custom_code(&err),
                    err: Some(err),
                    logs: f.meta.logs,
                    units: f.meta.compute_units_consumed,
                    accounts: post_state(&[]),
                }
            }
        })
    }

    /// Charges `fee` to the fee payer without executing (a failed load).
    fn charge(&mut self, payer: &Address, fee: u64) -> bool {
        let Some(mut a) = self.svm.get_account(payer) else {
            return false;
        };
        if a.lamports < fee {
            return false;
        }
        a.lamports -= fee;
        self.svm.set_account(*payer, a).is_ok()
    }

    fn execute(&mut self, p: Pending) -> Option<Landed> {
        let t = p.tx;
        let sig = tx::signature(&t);
        let keys = t.message.account_keys.clone();
        let fee = tx::fee_lamports(&t.message);
        let limit = tx::parse_budget(&t.message).effective_loaded_limit() as u64;
        let need = self.loaded_size(&t.message);
        let slot = self.slot;
        let block_time = self.unix_timestamp();
        let writable: Vec<Address> = (0..keys.len())
            .filter(|&i| tx::is_writable_index(&t.message, i))
            .map(|i| keys[i])
            .collect();
        let (logs, err, code, units, fee, ret) = if need > limit {
            if !self.charge(&keys[0], fee) {
                self.dropped.insert(sig);
                return None;
            }
            (
                vec![format!(
                    "frontier-localnet: loaded {need} B > limit {limit} B (SIMD-0186)"
                )],
                Some("MaxLoadedAccountsDataSizeExceeded".to_string()),
                None,
                0,
                fee,
                vec![],
            )
        } else {
            match self.svm.send_transaction(t.clone()) {
                Ok(m) => (
                    m.logs,
                    None,
                    None,
                    m.compute_units_consumed,
                    m.fee,
                    m.return_data.data,
                ),
                Err(f) => {
                    let e = format!("{:?}", f.err);
                    if e.contains("InsufficientFundsForFee")
                        || e.contains("AccountNotFound")
                        || e.contains("SignatureFailure")
                    {
                        self.dropped.insert(sig);
                        return None;
                    }
                    (
                        f.meta.logs,
                        Some(e.clone()),
                        fclient::ports::custom_code(&e),
                        f.meta.compute_units_consumed,
                        f.meta.fee,
                        f.meta.return_data.data,
                    )
                }
            }
        };
        let (err, code) = match err {
            Some(e) => {
                let c = code.or_else(|| fclient::ports::custom_code(&e));
                (Some(e), c)
            }
            None => (None, None),
        };
        let post = writable.iter().map(|k| (*k, self.account(k))).collect();
        let seq = self.landed.len() as u64 + 1;
        Some(Landed {
            seq,
            slot,
            block_time,
            signature: sig,
            wire: p.wire,
            keys,
            logs,
            err,
            code,
            units,
            fee,
            post,
            return_data: ret,
        })
    }

    /// Produces one block: advances the slot and the Clock (`0.4 × scale`
    /// game seconds), then executes the mempool by priority under the caps.
    pub fn produce_block(&mut self) -> BlockReport {
        if let Some(s) = self.pending_scale.take() {
            self.scale = s;
        }
        self.slot += 1;
        self.game_ms += (SLOT_MS as f64 * self.scale).round() as i128;
        self.write_clock();
        self.svm.expire_blockhash();
        let h = self.svm.latest_blockhash();
        self.blockhashes.push_back((h, self.slot));
        while self
            .blockhashes
            .front()
            .is_some_and(|(_, s)| s + BLOCKHASH_SLOTS < self.slot)
        {
            self.blockhashes.pop_front();
        }
        self.block_times.insert(self.slot, self.unix_timestamp());
        let mut report = BlockReport {
            slot: self.slot,
            unix_timestamp: self.unix_timestamp(),
            ..Default::default()
        };

        let mut pool = std::mem::take(&mut self.mempool);
        pool.sort_by(|a, b| {
            b.priority_milli
                .cmp(&a.priority_milli)
                .then(a.arrival.cmp(&b.arrival))
        });
        let mut per_account: HashMap<Address, u64> = HashMap::new();
        for p in pool {
            if p.blockhash_slot + BLOCKHASH_SLOTS < self.slot {
                report.dropped += 1;
                self.dropped.insert(tx::signature(&p.tx));
                continue;
            }
            let writable: Vec<Address> = (0..p.tx.message.account_keys.len())
                .filter(|&i| tx::is_writable_index(&p.tx.message, i))
                .map(|i| p.tx.message.account_keys[i])
                .collect();
            let fits = report.cu + p.cu_limit <= BLOCK_CU
                && writable
                    .iter()
                    .all(|k| per_account.get(k).copied().unwrap_or(0) + p.cu_limit <= ACCOUNT_CU);
            if !fits {
                report.deferred += 1;
                self.mempool.push(p);
                continue;
            }
            report.cu += p.cu_limit;
            for k in writable {
                *per_account.entry(k).or_default() += p.cu_limit;
            }
            if let Some(l) = self.execute(p) {
                if l.err.is_some() {
                    report.failed += 1;
                } else {
                    report.landed += 1;
                }
                self.by_sig.insert(l.signature, self.landed.len());
                self.landed.push(l);
            }
        }
        report
    }

    pub fn status(&self, sig: &Signature) -> Option<Status> {
        let l = &self.landed[*self.by_sig.get(sig)?];
        Some(Status {
            slot: l.slot,
            err: l.err.clone(),
            code: l.code,
        })
    }

    pub fn transaction(&self, sig: &Signature) -> Option<&Landed> {
        self.by_sig.get(sig).map(|&i| &self.landed[i])
    }

    /// Transactions that mention `program` (incl. failed), after feed seq `after`.
    pub fn feed(&self, after: u64, limit: usize, program: Option<&Address>) -> Vec<&Landed> {
        self.landed
            .iter()
            .skip(after as usize)
            .filter(|l| !l.wire.is_empty() && program.is_none_or(|p| l.keys.contains(p)))
            .take(limit)
            .collect()
    }

    /// Newest first, as `getSignaturesForAddress`.
    pub fn signatures_for(&self, k: &Address, limit: usize) -> Vec<&Landed> {
        self.landed
            .iter()
            .rev()
            .filter(|l| l.keys.contains(k))
            .take(limit)
            .collect()
    }

    pub fn pending(&self) -> usize {
        self.mempool.len()
    }

    pub fn transaction_count(&self) -> u64 {
        self.landed.len() as u64
    }

    pub fn is_dropped(&self, sig: &Signature) -> bool {
        self.dropped.contains(sig)
    }
}
