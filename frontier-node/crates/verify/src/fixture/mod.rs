//! The synthetic march season (`fixtures/verify/march-synth.json`).
//!
//! The wave-3 program this unit was cut from has no clash, transit, defence
//! or archive instructions (W4-A and W4-B write them in this wave), so
//! their records cannot be recorded from the program yet. [`march`] builds
//! a mini-season that has them, **byte for byte in the `frontier-abi`
//! layouts**, with real signed transactions (the `fclient::ix` builders),
//! real PS2 records and event chains, real test-key signatures and hints,
//! real stock-`tlock` seals, and the kernels' own clash, quota, explore,
//! camp, terrain, ticket-score and refund functions. The write-back after a
//! resolve and the TRANSIT_SETTLED payment encoding follow the contract
//! text where it is pinned and this module's documented choices where it
//! is not (W4-D notes §3); once W4-A/W4-B land, the integrator re-records
//! the march part from the program (`itest`, W4-F) and this fixture becomes
//! a regression of the verifier alone.
//!
//! [`Gen`] is the engine: accounts, chains, transactions, time. The script
//! is [`march::march`].

pub mod march;

use std::collections::{BTreeMap, BTreeSet};

use fclient::addr::Addresses;
use fclient::beacon::TestKey;
use fclient::ix::BeaconArg;
use fclient::log::{in_frame, log_line};
use fclient::ports::{Account, TxRecord};
use fclient::tx::{self, TxBudget};
use fclient::{Address, Hash, Instruction, Keypair, Signer};
use frontier_abi::addr::AddrCtx;
use frontier_abi::layout::{header as HD, write_header, AccountKind};
use frontier_abi::log::{self as plog, EntityKind, Kind, Link};
use frontier_abi::presets::SeasonParams;
use permutation_rules::frontier::beacon;
use permutation_rules::frontier::clash::BeaconClock;

use crate::world::{le, Key};

/// The engine of a synthetic season.
pub struct Gen {
    pub a: Addresses,
    pub ctx: AddrCtx,
    pub program: Address,
    pub season_id: u64,
    pub key: TestKey,
    pub acc: BTreeMap<Key, Account>,
    pub closed: BTreeSet<Key>,
    pub txs: Vec<TxRecord>,
    pub slot: u64,
    pub time: i64,
    seq: u64,
    pub p: SeasonParams,
    pub payout: Vec<u8>,
    pub genesis_round: u64,
    pub genesis_ts: i64,
    pub clock: BeaconClock,
    pub authority: Keypair,
    pub keeper: Keypair,
    pub relay: Keypair,
}

/// Default budget of operator and keeper transactions.
pub const OP: TxBudget = TxBudget {
    cu_limit: 400_000,
    cu_price: 0,
    loaded_limit: 1 << 20,
    heap: None,
};

impl Gen {
    pub fn new(program: Address, season_id: u64, t0: i64) -> Gen {
        let key = TestKey::new();
        let mut p = frontier_abi::presets::M1_LOCAL_7D;
        p.quicknet_pk_hash = fclient::beacon::pk_hash(&key.pk96);
        let a = Addresses::new(program, season_id);
        Gen {
            ctx: findex::addr_ctx(&program, season_id),
            a,
            program,
            season_id,
            key,
            acc: BTreeMap::new(),
            closed: BTreeSet::new(),
            txs: vec![],
            slot: 1_000,
            time: t0,
            seq: 0,
            p,
            payout: permutation_rules::frontier::payout::PayoutParams::REV3.to_borsh(),
            genesis_round: 0,
            genesis_ts: i64::MAX,
            clock: BeaconClock {
                genesis: fclient::beacon::QUICKNET_GENESIS,
                period: fclient::beacon::QUICKNET_PERIOD as i64,
            },
            authority: Keypair::new_from_array([0xA1; 32]),
            keeper: Keypair::new_from_array([0xBE; 32]),
            relay: Keypair::new_from_array([0xEE; 32]),
        }
    }

    // ------------------------------------------------------------ time

    /// Moves the Clock to `t` (8 game seconds per slot, 20×).
    pub fn at(&mut self, t: i64) {
        if t > self.time {
            self.slot += ((t - self.time) / 8).max(1) as u64;
            self.time = t;
        }
    }

    /// The current bell (`NO_BELL` before genesis).
    pub fn bell(&self) -> u32 {
        beacon::bell_at(self.genesis_ts, self.time).unwrap_or(plog::NO_BELL)
    }

    pub fn bell_start(&self, b: u32) -> i64 {
        beacon::bell_start(self.genesis_ts, b)
    }

    pub fn bell_end(&self, b: u32) -> i64 {
        beacon::bell_end(self.genesis_ts, b)
    }

    pub fn round_time(&self, r: u64) -> i64 {
        beacon::round_time(self.clock.genesis, self.clock.period as u32, r)
    }

    pub fn tlock_round(&self, b: u32) -> u64 {
        beacon::tlock_round(&self.clock, self.genesis_ts, b)
    }

    /// `S(b, r)` (the synthetic season never schedules a window change,
    /// so `W(b)` is `reveal_window` for every bell).
    pub fn seed_round(&self, _b: u32, a: i64) -> u64 {
        let w = self.p.reveal_window;
        beacon::seed_round(&self.clock, beacon::reveal_close(a, w), self.p.seed_margin)
    }

    pub fn beacon(&self, round: u64) -> BeaconArg {
        BeaconArg {
            round,
            sig48: self.key.sign(round),
            hints: fclient::beacon::hints_bytes(round),
        }
    }

    // ------------------------------------------------------------ accounts

    /// Creates a program account of `kind` at `k` (header written, rent).
    pub fn make(&mut self, k: Key, kind: AccountKind) {
        let mut d = vec![0u8; kind.size()];
        write_header(&mut d, kind, self.season_id);
        self.closed.remove(&k);
        self.acc.insert(
            k,
            Account {
                lamports: kind.rent(),
                data: d,
                owner: self.program,
                executable: false,
            },
        );
    }

    pub fn has(&self, k: &Key) -> bool {
        self.acc.contains_key(k)
    }

    pub fn d(&mut self, k: &Key) -> &mut Vec<u8> {
        &mut self.acc.get_mut(k).expect("account exists").data
    }

    pub fn get(&self, k: &Key) -> &[u8] {
        &self.acc.get(k).expect("account exists").data
    }

    pub fn close(&mut self, k: &Key) {
        self.acc.remove(k);
        self.closed.insert(*k);
    }

    pub fn put(&mut self, k: &Key, o: usize, b: &[u8]) {
        self.d(k)[o..o + b.len()].copy_from_slice(b);
    }

    pub fn u32at(&self, k: &Key, o: usize) -> u32 {
        le(&self.get(k)[o..o + 4]) as u32
    }

    // ------------------------------------------------------------ records

    /// A PS2 body: writes `kind`'s body and advances the chains of
    /// `chained` (in the given order within one entity kind).
    pub fn rec(
        &mut self,
        kind: Kind,
        bell: u32,
        key: &[u8],
        payload: &[u8],
        chained: &[(EntityKind, Key)],
    ) -> Vec<u8> {
        let mut out = vec![0u8; 1_024];
        let n = plog::write_body(kind, bell, key, payload, &mut out)
            .unwrap_or_else(|| panic!("{kind:?}: key {} payload {}", key.len(), payload.len()));
        let bwt = out[..n].to_vec();
        let mut ch: Vec<(EntityKind, Key)> = chained.to_vec();
        ch.sort_by_key(|(e, _)| *e as u8);
        let mut links: Vec<Link> = vec![];
        for (e, k) in ch {
            let d = self.d(&k);
            let seq = le(&d[HD::EVENT_SEQ..HD::EVENT_SEQ + 8]);
            let head: [u8; 32] = d[HD::EVENT_HEAD..HD::EVENT_HEAD + 32]
                .try_into()
                .expect("32");
            let l = plog::advance(e, seq, &head, &bwt).expect("seq");
            d[HD::EVENT_SEQ..HD::EVENT_SEQ + 8].copy_from_slice(&l.seq.to_le_bytes());
            d[HD::EVENT_HEAD..HD::EVENT_HEAD + 32].copy_from_slice(&l.head);
            links.push(l);
        }
        let m = plog::write_tail(&links, &mut out, n).expect("tail");
        out.truncate(m);
        out
    }

    /// The CLOSE record of `k` (its last record) and the account's removal.
    pub fn close_rec(
        &mut self,
        kind: AccountKind,
        raw: &[u8],
        k: &Key,
        recipient: &Address,
        bell: u32,
    ) -> Vec<u8> {
        let (seq, head) = if kind.chained() {
            let d = self.get(k);
            (
                le(&d[HD::EVENT_SEQ..HD::EVENT_SEQ + 8]),
                d[HD::EVENT_HEAD..HD::EVENT_HEAD + 32].to_vec(),
            )
        } else {
            (0, vec![0u8; 32])
        };
        let lamports = self.acc.get(k).map(|a| a.lamports).unwrap_or(0);
        let mut payload = seq.to_le_bytes().to_vec();
        payload.extend(head);
        payload.extend(recipient.to_bytes());
        payload.extend(lamports.to_le_bytes());
        let key = plog::close_key(kind, raw).expect("close key");
        let chained: Vec<(EntityKind, Key)> = EntityKind::of_account(kind)
            .map(|e| (e, *k))
            .into_iter()
            .collect();
        let b = self.rec(Kind::CLOSE, bell, &key, &payload, &chained);
        self.close(k);
        b
    }

    // ------------------------------------------------------------ transactions

    fn record(
        &mut self,
        ixs: &[Instruction],
        signers: &[&Keypair],
        budget: &TxBudget,
        bodies: Vec<Vec<u8>>,
        err: Option<u32>,
    ) -> usize {
        self.seq += 1;
        self.slot += 1;
        self.time += 1;
        let t = tx::build(ixs, budget, signers, &Hash::new_from_array([7; 32])).expect("tx builds");
        let msg = &t.message;
        let mut post = vec![];
        if err.is_none() {
            for (i, k) in msg.account_keys.iter().enumerate() {
                if !tx::is_writable_index(msg, i) {
                    continue;
                }
                let kb = k.to_bytes();
                if let Some(a) = self.acc.get(&kb) {
                    post.push((*k, Some(a.clone())));
                } else if self.closed.contains(&kb) {
                    post.push((*k, None));
                }
            }
        }
        let logs = if err.is_none() {
            in_frame(&self.program, bodies.iter().map(|b| log_line(b)))
        } else {
            vec![
                format!("Program {} invoke [1]", self.program),
                format!(
                    "Program {} failed: custom program error: {:#x}",
                    self.program,
                    err.unwrap_or(0)
                ),
            ]
        };
        self.txs.push(TxRecord {
            seq: self.seq,
            slot: self.slot,
            signature: tx::signature(&t),
            block_time: self.time,
            tx: tx::wire(&t),
            logs,
            err: err.map(|c| format!("InstructionError(0, Custom({c}))")),
            code: err,
            units: 0,
            fee: tx::fee_lamports(msg),
            post,
        });
        self.txs.len() - 1
    }

    /// A landed transaction with its records.
    pub fn send(
        &mut self,
        ixs: &[Instruction],
        signers: &[&Keypair],
        budget: &TxBudget,
        bodies: Vec<Vec<u8>>,
    ) -> usize {
        self.record(ixs, signers, budget, bodies, None)
    }

    /// A transaction the program refused with `code` (nothing written).
    pub fn fail(
        &mut self,
        ixs: &[Instruction],
        signers: &[&Keypair],
        budget: &TxBudget,
        code: u32,
    ) -> usize {
        self.record(ixs, signers, budget, vec![], Some(code))
    }

    /// The fixture: transactions, and the final state of every account the
    /// program ever owned (closed ones absent).
    pub fn input(&self, provenance: &str, scenarios: &[&str]) -> crate::Input {
        let mut finals: BTreeMap<Key, Option<Account>> = BTreeMap::new();
        for (k, a) in &self.acc {
            finals.insert(*k, Some(a.clone()));
        }
        for k in &self.closed {
            finals.entry(*k).or_insert(None);
        }
        crate::Input {
            cfg: crate::Config {
                program: self.program,
                season_id: self.season_id,
                quicknet_pk: self.key.pk96,
                ruleset_hash: frontier_abi::presets::RULESET_HASH,
                program_hash: None,
            },
            txs: self.txs.clone(),
            finals,
            final_slot: self.slot,
            program_hashes: vec![],
            provenance: provenance.into(),
            scenarios: scenarios.iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn signer(&self, k: &Keypair) -> Address {
        k.pubkey()
    }
}
