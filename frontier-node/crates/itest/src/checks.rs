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
    /// `(host, arrival bell)` → (outcome, seal code) of its
    /// TRANSIT_SETTLED. A host departs again after its settlement (it may
    /// not act in transit, I-44), so a TRANSIT_SETTLED belongs to the
    /// host's latest DEPART in feed order that is not settled yet
    /// (integ-W4 review, W4-F: keyed by host alone, a second march was
    /// judged by the first one's settlement).
    pub settled: BTreeMap<(u64, u32), (u8, u8)>,
    /// `(host, arrival bell)` → landed REVEAL records (the REVEAL key
    /// carries the arrival bell).
    pub revealed: BTreeMap<(u64, u32), u32>,
    /// TRANSIT_SETTLED records with no unsettled DEPART of their host
    /// before them (must be none).
    pub orphan_settles: Vec<u64>,
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
            // A feed error fails the run (integ-W4 review, W4-F: swallowed,
            // a truncated feed dropped late DEPARTs and erred toward PASS).
            let got = feed
                .feed(Cursor(after))
                .await
                .unwrap_or_else(|e| panic!("the chain feed after {after}: {e:?}"));
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
                            let arrive = le_u64(field(&r, k, "arrive", false)) as u32;
                            *f.revealed.entry((host_id, arrive)).or_default() += 1;
                        }
                        Kind::TRANSIT_SETTLED => {
                            let host_id = le_u64(field(&r, k, "host_id", false));
                            let o = field(&r, k, "outcome", true).first().copied().unwrap_or(0);
                            let c = field(&r, k, "seal_code", true)
                                .first()
                                .copied()
                                .unwrap_or(0);
                            f.settle(host_id, o, c);
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

    /// Pairs a TRANSIT_SETTLED of `host` with the host's latest DEPART not
    /// settled yet (feed order).
    pub fn settle(&mut self, host: u64, outcome: u8, code: u8) {
        let d = self
            .departs
            .iter()
            .rev()
            .find(|d| d.host_id == host && !self.settled.contains_key(&(host, d.arrive)))
            .map(|d| d.arrive);
        match d {
            Some(arrive) => {
                self.settled.insert((host, arrive), (outcome, code));
            }
            None => self.orphan_settles.push(host),
        }
    }

    /// The settlement of a departure.
    pub fn settlement(&self, d: &Departed) -> Option<(u8, u8)> {
        self.settled.get(&(d.host_id, d.arrive)).copied()
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
            .filter(|d| self.due(d.arrive) && self.settlement(d).is_none())
            .map(|d| d.host_id)
            .collect()
    }

    /// Settled transits whose seal code differs from the stock opener's.
    pub fn seal_disagreements(&self) -> Vec<(u64, u8, u8)> {
        self.departs
            .iter()
            .filter_map(|d| {
                let (_, c) = self.settlement(d)?;
                (c != d.stock_code).then_some((d.host_id, c, d.stock_code))
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
                self.settlement(d)
                    .is_some_and(|(o, _)| o != transit_outcome::BAD_SEAL)
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
        let settled = due.iter().filter(|d| self.settlement(d).is_some()).count();
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
            "revealedMarches": self.revealed.len(),
            "transitSettled": self.settled.len(),
            "orphanSettles": self.orphan_settles.len(),
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

/// Keeper alert kinds that fail the gate (integ-W4 review, W4-F): a write
/// given up (`dead`), refused with a code no duty expects (`failed`, which
/// is also how an expired write ends), and the CU/heap retry ladder (a
/// write over its budget).
pub const BAD_KEEPER_ALERTS: [&str; 4] = ["dead", "failed", "retry-ladder", "write-expired"];

/// The keeper-writes condition: no alert of a bad kind, no write dead.
/// `writes`: kind → (writes, landed, failed, dead).
pub fn keeper_writes_cond(
    alerts: &BTreeMap<String, u64>,
    samples: &[String],
    writes: &BTreeMap<String, (u64, u64, u64, u64)>,
) -> Cond {
    let mut bad: Vec<String> = alerts
        .iter()
        .filter(|(k, _)| BAD_KEEPER_ALERTS.contains(&k.as_str()))
        .map(|(k, n)| format!("{k} ×{n}"))
        .collect();
    for (k, (_, _, _, dead)) in writes {
        if *dead > 0 {
            bad.push(format!("{k}: {dead} dead"));
        }
    }
    if bad.is_empty() {
        Cond::Pass(format!("alerts {alerts:?}"))
    } else {
        Cond::Fail(format!("{bad:?}; samples {samples:?}"))
    }
}

/// Resident actions (Muster, Explore, Depart) the relay refused
/// `NotResident` against the ones it sent: `(action, refused, sent)`. The
/// bots gate them on the province's `resolved_next` and nudge a province
/// that is behind (integ-W4 review, W4-F).
pub fn not_resident_ratio(by: &BTreeMap<(String, String, String), u64>) -> Vec<(String, u64, u64)> {
    ["Muster", "Explore", "Depart"]
        .iter()
        .map(|ix| {
            let n = |res: &str| {
                by.iter()
                    .filter(|((_, i, r), _)| i == ix && r == res)
                    .map(|(_, n)| *n)
                    .sum::<u64>()
            };
            (ix.to_string(), n("NotResident"), n("sent"))
        })
        .collect()
}

/// The largest share of resident actions (per action) the relay may refuse
/// `NotResident` in the gate day (a herald view a bell old, a nudge still
/// in flight).
pub const NOT_RESIDENT_MAX_PCT: u64 = 25;

/// The settle racer's designed early tries (integ-W6t review, S29; M1
/// exit U3, M1-EXIT-NOTES §4.1), per resident action: its re-depart from
/// the bell after its arrival bell, every 8 game seconds for 4 bells, which
/// the relay refuses `NotResident` in simulation (nothing is sent) until
/// the destination resolved the arrival. Only those: a `settle_racer`
/// outcome tagged `redepart`, sent through the relay, refused
/// `NotResident`. Any other refusal (an archetype bot's, another
/// persona's, the racer's own first march) is not in here.
pub fn designed_not_resident(bots: &frontier_bots::report::Report) -> BTreeMap<String, u64> {
    let n = bots
        .persona_outcomes
        .iter()
        .filter(|o| {
            o.persona == Some(frontier_agents::Persona::SettleRacer)
                && o.tag == Some("redepart")
                && o.route == "relay"
                && !o.ok
                && o.code.as_deref() == Some("NotResident")
        })
        .count() as u64;
    [("Depart".to_string(), n)]
        .into_iter()
        .filter(|(_, n)| *n > 0)
        .collect()
}

/// The resident-liveness condition (integ-W4 review, W4-F): per resident
/// action, `NotResident / (sent + NotResident)` at most
/// [`NOT_RESIDENT_MAX_PCT`], with the settle racer's designed early tries
/// ([`designed_not_resident`]) taken out of the relay's refusals (M1 exit
/// U3). More designed refusals than the relay counted for an action is a
/// failure (the subtraction would hide a real refusal); pending while
/// stubs stand in.
pub fn resident_liveness_cond(
    by: &BTreeMap<(String, String, String), u64>,
    bots: &frontier_bots::report::Report,
    stubs: bool,
) -> Cond {
    let designed = designed_not_resident(bots);
    let raw = not_resident_ratio(by);
    let mut bad: Vec<String> = vec![];
    let ratio: Vec<(String, u64, u64)> = raw
        .iter()
        .map(|(ix, refused, sent)| {
            let d = designed.get(ix).copied().unwrap_or(0);
            if d > *refused {
                bad.push(format!(
                    "{ix}: {d} designed racer refusals but the relay counted {refused} NotResident"
                ));
            }
            (ix.clone(), refused.saturating_sub(d), *sent)
        })
        .collect();
    for (ix, refused, sent) in &ratio {
        if refused * 100 > (refused + sent) * NOT_RESIDENT_MAX_PCT {
            bad.push(format!(
                "over {NOT_RESIDENT_MAX_PCT}%: {ix} {refused} refused, {sent} sent"
            ));
        }
    }
    if bad.is_empty() {
        Cond::Pass(format!(
            "NotResident / (sent + NotResident) ≤ {NOT_RESIDENT_MAX_PCT}% per action: {ratio:?} (the settle racer's designed early tries left out: {designed:?}; relay counted {raw:?}); nudges {:?}",
            bots.nudges
        ))
    } else if stubs {
        Cond::Pending(format!("{bad:?}; {ratio:?}"))
    } else {
        Cond::Fail(format!(
            "{bad:?}; counted {ratio:?} after leaving out the settle racer's designed early tries {designed:?} (relay {raw:?}); nudges {:?}",
            bots.nudges
        ))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn dep(host: u64, arrive: u32, stock_code: u8) -> Departed {
        Departed {
            host_id: host,
            arrive,
            commit: [0; 32],
            seal: [0; 165],
            stock_code,
        }
    }

    /// Integ-W4 review (W4-F): a host that departs twice, only its first
    /// march settled — the second is unsettled (keyed by host alone it was
    /// judged by the first one's settlement).
    #[test]
    fn a_second_march_of_a_host_is_judged_on_its_own() {
        let mut f = Facts {
            now_bell: 100,
            ..Default::default()
        };
        f.departs.push(dep(7, 10, 0));
        f.settle(7, transit_outcome::STAYS, 0);
        f.departs.push(dep(7, 30, 2));
        assert_eq!(f.unsettled_departs(), vec![7], "the second march");
        let (due, settled) = f.bad_seals_due();
        assert_eq!((due, settled), (1, 0));
        // Its own settlement, with another code than the stock one.
        f.settle(7, transit_outcome::BAD_SEAL, 4);
        assert!(f.unsettled_departs().is_empty());
        assert_eq!(f.seal_disagreements(), vec![(7, 4, 2)]);
        assert!(f.bad_seals_survived().is_empty());
        // A settlement with nothing to pair.
        f.settle(9, transit_outcome::STAYS, 0);
        assert_eq!(f.orphan_settles, vec![9]);
    }

    #[test]
    fn keeper_writes_fail_on_expiry_failure_and_dead() {
        let w: BTreeMap<String, (u64, u64, u64, u64)> =
            [("reveal".to_string(), (10, 9, 1, 0))].into();
        let ok: BTreeMap<String, u64> = [("reveal-pool-low".to_string(), 4)].into();
        assert!(!keeper_writes_cond(&ok, &[], &w).is_fail());
        for bad in ["failed", "write-expired", "dead", "retry-ladder"] {
            let a: BTreeMap<String, u64> = [(bad.to_string(), 1)].into();
            assert!(keeper_writes_cond(&a, &[], &w).is_fail(), "{bad}");
        }
        let dead: BTreeMap<String, (u64, u64, u64, u64)> =
            [("gather".to_string(), (3, 2, 0, 1))].into();
        assert!(keeper_writes_cond(&ok, &[], &dead).is_fail());
    }

    // ---- resident liveness (M1 exit U3, M1-EXIT-NOTES §4.1).

    use frontier_agents::{Arch, Persona};
    use frontier_bots::report::{Outcome, Report};

    fn relay(depart_sent: u64, depart_nr: u64) -> BTreeMap<(String, String, String), u64> {
        let r = |ix: &str, res: &str| ("/f/relay".to_string(), ix.to_string(), res.to_string());
        [
            (r("Muster", "sent"), 40),
            (r("Explore", "sent"), 30),
            (r("Depart", "sent"), depart_sent),
            (r("Depart", "NotResident"), depart_nr),
        ]
        .into_iter()
        .filter(|(_, n)| *n > 0)
        .collect()
    }

    fn refusal(persona: Option<Persona>, tag: Option<&'static str>) -> Outcome {
        Outcome {
            bot: 1,
            arch: Arch::Daily,
            persona,
            action: "depart",
            route: "relay",
            ok: false,
            status: 409,
            code: Some("NotResident".into()),
            signature: None,
            tag,
        }
    }

    fn racer_try() -> Outcome {
        refusal(Some(Persona::SettleRacer), Some("redepart"))
    }

    fn bots(outcomes: impl IntoIterator<Item = Outcome>) -> Report {
        let mut r = Report::default();
        for o in outcomes {
            r.record(o);
        }
        r
    }

    /// The G14 re-run of the exit (03:13): Depart 116 refused `NotResident`
    /// against 38 sent, all 116 the settle racer's early re-depart tries
    /// (integ-W6t review, S29: refused in simulation by design, nothing
    /// sent). They are not a liveness miss.
    #[test]
    fn the_settle_racers_designed_early_tries_are_no_liveness_miss() {
        let by = relay(38, 116);
        let r = bots((0..116).map(|_| racer_try()));
        let c = resident_liveness_cond(&by, &r, false);
        assert!(matches!(c, Cond::Pass(_)), "{c:?}");
    }

    /// Any other refusal still counts: an archetype bot's Depart, another
    /// persona's, and the racer's own first march (not a re-depart try)
    /// refused `NotResident` fail the condition over the limit, next to the
    /// racer's excluded tries.
    #[test]
    fn a_refusal_that_is_not_a_racer_try_still_fails() {
        let others: [(Option<Persona>, Option<&'static str>); 3] = [
            (None, None),
            (Some(Persona::Squatter), None),
            (Some(Persona::SettleRacer), None),
        ];
        for (persona, tag) in others {
            // 116 racer tries + 20 other refusals against 38 sent:
            // 20 / 58 = 34 % > 25 %.
            let by = relay(38, 136);
            let r = bots(
                (0..116)
                    .map(|_| racer_try())
                    .chain((0..20).map(|_| refusal(persona, tag))),
            );
            let c = resident_liveness_cond(&by, &r, false);
            assert!(c.is_fail(), "{persona:?} {tag:?}: {c:?}");
        }
        // Under the limit they pass: 10 / 48 = 21 %.
        let by = relay(38, 126);
        let r = bots(
            (0..116)
                .map(|_| racer_try())
                .chain((0..10).map(|_| refusal(None, None))),
        );
        assert!(!resident_liveness_cond(&by, &r, false).is_fail());
        // With no racer at all, the old rule unchanged: 13 / 51 = 25.5 %.
        let r = bots((0..13).map(|_| refusal(None, None)));
        assert!(resident_liveness_cond(&relay(38, 13), &r, false).is_fail());
        assert!(!resident_liveness_cond(&relay(39, 13), &r, false).is_fail());
    }

    /// A racer try the relay did not count `NotResident` (another route, a
    /// racer outcome the relay never saw) is not subtracted from what the
    /// relay counted: more claimed designed refusals than the relay's own
    /// count fail rather than hide a real refusal.
    #[test]
    fn designed_refusals_beyond_the_relays_count_fail() {
        let by = relay(38, 20);
        let r = bots((0..116).map(|_| racer_try()));
        assert!(resident_liveness_cond(&by, &r, false).is_fail());
        let mut other_route = racer_try();
        other_route.route = "direct";
        let r = bots((0..20).map(|_| other_route.clone()));
        assert!(resident_liveness_cond(&by, &r, false).is_fail());
        // Stubs still turn a fail into pending.
        let r = bots(std::iter::empty());
        assert!(matches!(
            resident_liveness_cond(&by, &r, true),
            Cond::Pending(_)
        ));
    }
}
