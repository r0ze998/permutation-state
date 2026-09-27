//! What the chain says after a run: the Gate W4 conditions of
//! `inproc_day` (§12) read from the final accounts and the program's whole
//! transaction feed, never from what a component believes.
//!
//! - **stuck province-bells**: every opened Province resolved or skipped
//!   through the last bell of play (`resolved_next ≥ end_bell`), summed
//!   over provinces as bells behind;
//! - **transits**: every DEPART has a TRANSIT_SETTLED (settled or routed by
//!   rule); a transit still in state 1–3 two bells after its arrival
//!   bell's close is stuck;
//! - **seals** (I-44): every DEPART's seal is opened with the stock `tlock`
//!   opener at `T(arrive)` (`fclient::seal::judge`); every TRANSIT_SETTLED
//!   seal code must equal the stock code, and a bad seal (code > 0) must
//!   have settled as `BAD_SEAL` (else `BadSealSurvived`);
//! - **stubs**: failed transactions by instruction and error, and the
//!   instructions still answering `NotImplemented` (99).

use std::collections::{BTreeMap, BTreeSet};

use fclient::abi::tag;
use fclient::beacon::TestKey;
use fclient::clock::SeasonClock;
use fclient::decode::{Holding, Province};
use fclient::log::Record;
use fclient::ports::{ChainPort, Cursor, TxRecord};
use fclient::Address;
use frontier_abi::layout::player::holding as HL;
use frontier_abi::layout::province::province as PL;
use frontier_abi::log::{transit_outcome, Kind};
use serde_json::{json, Value};

use crate::world::World;

/// `NotImplemented` (99): a stub of a unit not merged yet.
pub const NOT_IMPLEMENTED: u32 = 99;

/// A field of a decoded record (key or payload part).
pub fn field<'a>(r: &'a Record, kind: Kind, name: &str, payload: bool) -> &'a [u8] {
    let (off, w) = frontier_abi::log::field(kind, name, payload).unwrap_or((0, 0));
    let base = if payload { kind.spec().key_len() } else { 0 };
    r.key_payload.get(base + off..base + off + w).unwrap_or(&[])
}

pub fn le_u64(b: &[u8]) -> u64 {
    let mut a = [0u8; 8];
    a[..b.len().min(8)].copy_from_slice(&b[..b.len().min(8)]);
    u64::from_le_bytes(a)
}

/// The Frontier instruction's tag of a transaction (the first instruction
/// of `program`).
pub fn frontier_tag(t: &TxRecord, program: &Address) -> Option<u8> {
    let tx = fclient::tx::from_wire(&t.tx).ok()?;
    let m = &tx.message;
    m.instructions
        .iter()
        .find(|i| m.account_keys.get(i.program_id_index as usize) == Some(program))
        .and_then(|i| i.data.first().copied())
}

pub fn tag_name(t: u8) -> String {
    frontier_abi::tags::Ix::from_tag(t)
        .map(|i| format!("{i:?}"))
        .unwrap_or_else(|| format!("0x{t:02x}"))
}

/// One DEPART as logged.
#[derive(Clone, Debug)]
pub struct Departed {
    pub host_id: u64,
    pub arrive: u32,
    pub commit: [u8; 32],
    pub seal: [u8; 165],
    /// The stock opener's code at `T(arrive)` (`fclient::seal::judge`).
    pub stock_code: u8,
}

#[derive(Clone, Debug, Default)]
pub struct Facts {
    pub txs: u64,
    pub failed: u64,
    pub landed_by_tag: BTreeMap<String, u64>,
    /// (instruction, error) → failed transactions.
    pub failed_by: BTreeMap<(String, String), u64>,
    /// Instructions that answered `NotImplemented` at least once.
    pub not_implemented: BTreeSet<String>,
    pub records: BTreeMap<String, u64>,
    pub departs: Vec<Departed>,
    /// host → (outcome, seal code) of its TRANSIT_SETTLED.
    pub settled: BTreeMap<u64, (u8, u8)>,
    /// host → landed REVEAL records of its arrival.
    pub revealed: BTreeMap<u64, u32>,
    pub provinces: Vec<(i16, i16, u32)>,
    /// Transits still in state 1–3 in some Holding: (host, arrive, state).
    pub open_transits: Vec<(u64, u32, u8)>,
    pub end_bell: u32,
    pub now_bell: u32,
    /// Instructions of the deployed program that are still stubs.
    pub program_stubs: Vec<String>,
}

impl Facts {
    /// Reads the whole feed and every program account.
    pub async fn collect(w: &World, end_bell: u32) -> Facts {
        let season = w.season();
        let sc = SeasonClock::from_season(&season);
        let now = w.now();
        let key = TestKey::new();
        let mut f = Facts {
            end_bell,
            now_bell: sc.bell_at(now).unwrap_or(0),
            ..Default::default()
        };
        let feed = localnet::InProcess::from_chain(w.ip.chain.clone(), Some(w.program));
        let mut after = 0;
        loop {
            let got = feed.feed(Cursor(after)).await.unwrap_or_default();
            let Some(last) = got.last() else { break };
            after = last.seq;
            for t in &got {
                f.txs += 1;
                let tg = frontier_tag(t, &w.program)
                    .map(tag_name)
                    .unwrap_or("-".into());
                if t.err.is_some() {
                    f.failed += 1;
                    let e = t
                        .code
                        .map(|c| {
                            fclient::abi::error_name(c)
                                .map(String::from)
                                .unwrap_or(format!("custom {c}"))
                        })
                        .unwrap_or_else(|| t.err.clone().unwrap_or_default());
                    if t.code == Some(NOT_IMPLEMENTED) {
                        f.not_implemented.insert(tg.clone());
                    }
                    *f.failed_by.entry((tg, e)).or_default() += 1;
                    continue;
                }
                *f.landed_by_tag.entry(tg).or_default() += 1;
                for r in findex::records(t, &w.program).unwrap_or_default() {
                    let Some(k) = Kind::from_u8(r.kind) else {
                        continue;
                    };
                    *f.records.entry(k.name().to_string()).or_default() += 1;
                    match k {
                        Kind::DEPART => {
                            let host_id = le_u64(field(&r, k, "host_id", false));
                            let arrive = le_u64(field(&r, k, "arrive_bell", true)) as u32;
                            let commit: [u8; 32] =
                                field(&r, k, "commit", true).try_into().unwrap_or([0; 32]);
                            let seal: [u8; 165] =
                                field(&r, k, "seal", true).try_into().unwrap_or([0; 165]);
                            let sig = key.sign(sc.tlock_round(arrive));
                            let (stock_code, _) =
                                fclient::seal::judge(&seal, &commit, &sig, host_id, arrive);
                            f.departs.push(Departed {
                                host_id,
                                arrive,
                                commit,
                                seal,
                                stock_code,
                            });
                        }
                        Kind::REVEAL => {
                            let host_id = le_u64(field(&r, k, "host_id", true));
                            *f.revealed.entry(host_id).or_default() += 1;
                        }
                        Kind::TRANSIT_SETTLED => {
                            let host_id = le_u64(field(&r, k, "host_id", false));
                            let o = field(&r, k, "outcome", true).first().copied().unwrap_or(0);
                            let c = field(&r, k, "seal_code", true)
                                .first()
                                .copied()
                                .unwrap_or(0);
                            f.settled.insert(host_id, (o, c));
                        }
                        _ => {}
                    }
                }
            }
        }
        let accts = w.ip.lock().program_accounts(&w.program);
        for (_, a) in accts {
            if a.data.len() == PL::SIZE && a.data[..8] == PL::MAGIC {
                if let Ok(p) = Province::decode(&a.data) {
                    f.provinces.push((p.p, p.q, p.resolved_next));
                }
            } else if a.data.len() == HL::SIZE && a.data[..8] == HL::MAGIC {
                if let Ok(h) = Holding::decode(&a.data) {
                    for t in h.transit.iter().filter(|t| (1..=3).contains(&t.state)) {
                        f.open_transits.push((t.host_id, t.arrive_bell, t.state));
                    }
                }
            }
        }
        f.provinces.sort();
        f.program_stubs = program_stubs(w).await;
        f
    }

    /// Province-bells not resolved through `end_bell − 1`.
    pub fn stuck_province_bells(&self) -> u64 {
        self.provinces
            .iter()
            .map(|&(_, _, rn)| self.end_bell.saturating_sub(rn) as u64)
            .sum()
    }

    /// Transits in state 1–3 two bells past their arrival bell's close
    /// (close ≈ the end of the next bell with W = one bell).
    pub fn stuck_transits(&self) -> Vec<(u64, u32, u8)> {
        self.open_transits
            .iter()
            .copied()
            .filter(|&(_, arrive, _)| self.due(arrive))
            .collect()
    }

    /// DEPARTs without a TRANSIT_SETTLED whose arrival is old enough to
    /// have settled.
    pub fn unsettled_departs(&self) -> Vec<u64> {
        self.departs
            .iter()
            .filter(|d| self.due(d.arrive) && !self.settled.contains_key(&d.host_id))
            .map(|d| d.host_id)
            .collect()
    }

    /// Settled transits whose seal code differs from the stock opener's.
    pub fn seal_disagreements(&self) -> Vec<(u64, u8, u8)> {
        self.departs
            .iter()
            .filter_map(|d| {
                let (_, c) = self.settled.get(&d.host_id)?;
                (*c != d.stock_code).then_some((d.host_id, *c, d.stock_code))
            })
            .collect()
    }

    /// Bad seals (stock code > 0) that settled with any outcome but
    /// `BAD_SEAL` (the verifier's `BadSealSurvived`).
    pub fn bad_seals_survived(&self) -> Vec<u64> {
        self.departs
            .iter()
            .filter(|d| d.stock_code > 0)
            .filter(|d| {
                self.settled
                    .get(&d.host_id)
                    .is_some_and(|(o, _)| *o != transit_outcome::BAD_SEAL)
            })
            .map(|d| d.host_id)
            .collect()
    }

    pub fn bad_seals(&self) -> usize {
        self.departs.iter().filter(|d| d.stock_code > 0).count()
    }

    /// Whether a march arriving at `arrive` is old enough to have settled
    /// by now (its close + one bell, plus a bell of margin).
    pub fn due(&self, arrive: u32) -> bool {
        arrive + 3 < self.now_bell
    }

    /// Bad seals old enough to have settled, and how many of them did.
    pub fn bad_seals_due(&self) -> (usize, usize) {
        let due: Vec<&Departed> = self
            .departs
            .iter()
            .filter(|d| d.stock_code > 0 && self.due(d.arrive))
            .collect();
        let settled = due
            .iter()
            .filter(|d| self.settled.contains_key(&d.host_id))
            .count();
        (due.len(), settled)
    }

    pub fn to_json(&self) -> Value {
        let failed: BTreeMap<String, u64> = self
            .failed_by
            .iter()
            .map(|((t, e), n)| (format!("{t}:{e}"), *n))
            .collect();
        let resolved: BTreeMap<u32, usize> =
            self.provinces
                .iter()
                .fold(BTreeMap::new(), |mut m, &(_, _, rn)| {
                    *m.entry(rn).or_default() += 1;
                    m
                });
        json!({
            "txs": self.txs,
            "failed": self.failed,
            "landedByInstruction": self.landed_by_tag,
            "failedBy": failed,
            "notImplemented": self.not_implemented,
            "programStubs": self.program_stubs,
            "records": self.records,
            "departs": self.departs.len(),
            "badSeals": self.bad_seals(),
            "badSealsDueSettled": self.bad_seals_due(),
            "revealedHosts": self.revealed.len(),
            "transitSettled": self.settled.len(),
            "provinces": self.provinces.len(),
            "provincesByResolvedNext": resolved,
            "endBell": self.end_bell,
            "nowBell": self.now_bell,
            "stuckProvinceBells": self.stuck_province_bells(),
            "stuckTransits": self.stuck_transits().len(),
            "unsettledDeparts": self.unsettled_departs().len(),
            "sealDisagreements": self.seal_disagreements().len(),
            "badSealsSurvived": self.bad_seals_survived().len(),
        })
    }
}

/// A named pass condition: pass, fail, or pending (a stub of a unit not
/// merged yet stands between the run and the condition).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cond {
    Pass(String),
    Fail(String),
    Pending(String),
}

impl Cond {
    pub fn is_fail(&self) -> bool {
        matches!(self, Cond::Fail(_))
    }
    pub fn label(&self) -> (&'static str, &str) {
        match self {
            Cond::Pass(d) => ("PASS", d),
            Cond::Fail(d) => ("FAIL", d),
            Cond::Pending(d) => ("PENDING", d),
        }
    }
}

/// The program's stubs: every assigned instruction tag simulated with no
/// accounts; a stub answers `NotImplemented` (99) before any check, a real
/// handler refuses the missing accounts. `ResolveClash` (oracle builds
/// only) is left out.
pub async fn program_stubs(w: &World) -> Vec<String> {
    let probe = fclient::Keypair::new_from_array([0x9B; 32]);
    w.airdrop(&fclient::Signer::pubkey(&probe), 1_000_000_000);
    let (bh, _) = w.ip.blockhash().await.unwrap_or_default();
    let budget = fclient::tx::TxBudget {
        cu_limit: 200_000,
        cu_price: 0,
        loaded_limit: 4 * 1024 * 1024,
        heap: None,
    };
    let mut out = vec![];
    for t in 0..=0x7Fu8 {
        let Some(ix) = frontier_abi::tags::Ix::from_tag(t) else {
            continue;
        };
        if t == tag::RESOLVE_CLASH {
            continue;
        }
        let i = fclient::Instruction {
            program_id: w.program,
            accounts: vec![],
            data: vec![t],
        };
        let Ok(tx) = fclient::tx::build(&[i], &budget, &[&probe], &bh) else {
            continue;
        };
        if let Ok(s) = w.ip.simulate(&fclient::tx::wire(&tx)).await {
            if s.code == Some(NOT_IMPLEMENTED) {
                out.push(format!("{ix:?}"));
            }
        }
    }
    out
}
