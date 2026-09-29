//! The keeper's play duties (W4-C; M1 contract §5.11, §5.12, §8.2, §21).
//!
//! | duty (role) | trigger | write | class |
//! |---|---|---|---|
//! | Decrypt (`reveal`) | round T(arrive) published | — (the seal opened off chain, the plaintext and salt journalled) | — |
//! | Reveal (`reveal`) | an opened seal, or owner material from `/v1/reveal` (from the bell start) | Reveal per arrival, grouped by `(P, Q, arrive, faction)`: **the group's final set sent in one slot**, each at the index it takes when the ones ranked above it land first, with a 1-milli bid bonus by rank (W4-C D1; v1.6 §22); `SlotMoved` → re-read and retry (≤ 4); a member the program refuses for good (`Path`, `ArrivalBell`, `Shielded`, …) leaves the group, and a member not sent (backoff) holds the ones ranked below it (wave-4 review); arrivals outside the group's final set or refused by the quota go to the settle queue; **never at or after `A + W − 2 slots`**; not once the latch is closed | **W** |
//! | SettleDeparture (`settle-departure`) | the origin resolved past `depart_bell` | SettleDeparture | D |
//! | Gather (`gather`) | window closed, the ArrivalDay bit set, every present arrival's transit in state ≥ 2 | GatherClash parts (a clear bit after a `NotQuiet` skip: one part) | D |
//! | Resolve (`resolve`) | all positions gathered, `resolved_next == b`, the seed of THE anchor | ResolveFromInputs | D |
//! | Skip (`skip`) | closed windows with clear bits from `resolved_next` | SkipQuiet over ≤ 24 bells: as soon as the run of closed quiet bells covers the province's **target bell** (a nudge: at once; a pending change or `Leave`: its bell; a departure to settle: its bell; an arrival ahead: the bell before it; W6-C), once 24 bells are due (≤ 6 per idle province-day), or when it reaches the season's end. CU limit `90k + 30k` per bell with pending ops (I-50, v1.7), capped at 1.4M; a committed prefix is re-planned; running out of CU is `NotQuiet` (gather and resolve) | D |
//! | Returns (`settle-departure`) | a `Leave` entry whose bell resolved (§21) | the return settle: SettleDeparture with `transit_slot = 0xFF` per (province, Holding) (W4-A's choice) | D |
//! | Settle (`settle`) | `close + 600` passed and the destination resolved past `arrive` | SettleTransit for every transit incl. refused, displaced, unrevealed and bad seals, with the **logged** commitment and seal | D |
//! | Closes (`close`) | slots settled and past the claim grace (or claimed), days whose province resolved past the day, inputs resolved, settled and past `clash_close_grace` | CloseArrivalSlot / CloseArrivalDay / CloseClashInputs | N |
//! | Claims (`claims`) | this keeper's late Reveals (lateness ≥ `lateness_slots`, a refund > 0) within the claim grace | ClaimDefence (≤ 6 slots), signed by the beneficiary key | D |
//!
//! The chain is the state: every plan reads the accounts it depends on;
//! the feed index ([`crate::playindex`]) only names them.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde_json::{json, Value};
use solana_address::Address;

use fclient::abi::{self, status, tag, Class};
use fclient::decode::{
    AnchorArchive, ArrivalDay, ArrivalSlot, BellAnchor, ClashInputs, Holding, Province,
};
use fclient::ix::{self, AnchorSource, HoldingRef, RevealArgs, SettleTransitArgs};
use fclient::play::{self, Opened, Target};
use fclient::ports::{Account, ChainPort, DrandPort, PortResult};
use fclient::seal;
use permutation_rules::frontier::clash::SlotEntry;

use crate::beacon::AnchorInfo;
use crate::engine::{BuildCtx, Engine, Outcome, WriteSpec};
use crate::journal::Journal;
use crate::playindex::{DepartRec, PlayIndex};
use crate::rounds::Rounds;
use crate::seeds::SeedFinder;
use crate::{Shared, Tick};

/// Slots before `A + W` after which no Reveal version is sent (§8.2: ε = 2 slots).
pub const REVEAL_EPSILON_SLOTS: i64 = 2;
/// `SlotMoved` retries per arrival (§8.2).
pub const SLOT_MOVED_RETRIES: u8 = 4;
/// Slots between two reads of a province with nothing due.
const PROVINCE_READ_SLOTS: u64 = 8;
/// Bells per SkipQuiet at most.
const SKIP_MAX: u32 = 24;
/// Claim grace (§5.12, I-52), bells.
pub const CLAIM_GRACE_BELLS: i64 = fclient::play::CLAIM_GRACE_BELLS;
const BELL_SECS: i64 = 600;

/// Owner reveal material (`/v1/reveal`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnerMat {
    pub track: String,
    pub holding: Address,
    pub transit_slot: u8,
    pub plain: [u8; 37],
    pub salt: [u8; 32],
    pub ct_hash: [u8; 32],
}

/// What the keeper knows of a transit beyond its DEPART record.
#[derive(Clone, Debug, Default)]
pub struct TransitState {
    pub opened: Option<Opened>,
    pub owner: Option<OwnerMat>,
    /// Refused for good (the program's answer, the quota): never revealed.
    pub refused: bool,
    /// Outside its group's final set when last planned (counted once;
    /// planned again while the group changes).
    pub outranked: bool,
    /// The window passed with the arrival unrevealed (liveness finding).
    pub missed: bool,
    pub slot_moved: u8,
    /// From the last Holding read: transit slot index, state, faction,
    /// citizen tag, rent payer.
    pub transit_slot: Option<u8>,
    pub tstate: u8,
    pub faction: u8,
    pub citizen_tag: u64,
    pub rent_payer: Address,
    /// The Holding's owner Citizen (the camp's Works, v1.7).
    pub owner_citizen: Address,
    /// The destination a GatherClash stamped (v1.7, W4-B F1): the only
    /// Province the transit settles against.
    pub gathered_at: Option<(i16, i16)>,
    pub holding_read_slot: u64,
}

/// A province as last read.
#[derive(Clone, Debug)]
pub struct ProvState {
    pub region: u8,
    pub rn: u32,
    pub read_slot: u64,
    pub pending_bells: BTreeSet<u32>,
    pub active: bool,
    /// `Leave` entries `(entry index, host, pend_bell)` (§21 returns).
    pub leaves: Vec<(u8, u64, u32)>,
    pub last_digest: [u8; 32],
    /// Residents of two or more factions: every bell fights (no quiet
    /// proof), so it is gathered (no-arrival fast path) and resolved at
    /// once instead of skipped.
    pub contested: bool,
}

/// An anchor as the play duties need it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnchorSeen {
    pub a: i64,
    pub slot: Option<u64>,
    pub archived: bool,
}

/// An ArrivalDay read: its bits (absent: `None`) and the Clock `now` of the read.
type DayRead = (Option<[u8; 18]>, i64);

/// Counters and findings (status JSON, tests).
#[derive(Clone, Debug, Default)]
pub struct PlayStats {
    pub opened: u64,
    pub bad_seals: u64,
    pub reveals_sent: u64,
    pub reveals_landed: u64,
    pub reveals_refused: u64,
    pub slot_moved: u64,
    /// Reveals not sent because the window was within ε of its close.
    pub reveals_missed: u64,
    pub departures_settled: u64,
    pub gathers: u64,
    pub resolves: u64,
    pub skips: u64,
    pub skipped_bells: u64,
    pub not_quiet: u64,
    pub settles: u64,
    pub closes: u64,
    pub claims: u64,
    pub returns: u64,
    /// `(bell, T(b) slot seen, last reveal landed slot)` per arrival bell.
    pub reveal_latency: BTreeMap<u32, (u64, u64)>,
    /// Close → resolve latency in slots (per resolved province-bell).
    pub resolve_latency: Vec<u64>,
}

#[derive(Default)]
pub struct PlayDuty {
    /// Resolve writes in flight: key → the slot estimate of their bell's
    /// close (`resolve_latency` is taken when the resolve lands).
    resolve_close: BTreeMap<String, u64>,
    pub index: PlayIndex,
    pub transits: BTreeMap<(u64, u32), TransitState>,
    pub provinces: BTreeMap<(i16, i16), ProvState>,
    pub anchors: BTreeMap<(u32, u8), AnchorSeen>,
    anchor_tried: BTreeMap<(u32, u8), u64>,
    /// ArrivalDay bits `(P, Q, day)` → (bits, the Clock `now` of the read).
    days: BTreeMap<(i16, i16, u32), DayRead>,
    /// Province-bells a skip found not quiet: gathered and resolved instead.
    pub not_quiet: BTreeSet<(i16, i16, u32)>,
    /// Nudged provinces → the bell a player wants to act at (§8.2 `/v1/nudge`).
    nudged: BTreeMap<(i16, i16), u32>,
    /// Estimated game seconds per slot (from the Clock samples).
    pub slot_secs: f64,
    last_clock: Option<(u64, i64)>,
    pub stats: PlayStats,
    /// Owner material by `(host, arrive)`.
    owner: BTreeMap<(u64, u32), OwnerMat>,
    /// Claimed slots (keys) this run.
    claimed: BTreeSet<Address>,
    /// Slots of refused claims, released at the next claims plan.
    unclaim: Vec<(i32, i32, u32, u8, u8)>,
    closed_sent: BTreeSet<String>,
    /// Liveness findings `(slot, detail)`.
    pub findings: Vec<(u64, String)>,
}

fn rkey((host, b): (u64, u32)) -> String {
    format!("{host}:{b}")
}

/// Whether the program would refuse a gather of `host`'s arrival at `bell`
/// with `DepartureUnsettled` (its gather_position): the Holding account is
/// the program's, live (provisional or final) with the host's generation,
/// and holds the host's transit to `bell` in state 1 (departed). Every other
/// case gathers (as present, or as a record with `present = 0`).
pub fn departure_unsettled(a: Option<&Account>, program: &Address, host: u64, bell: u32) -> bool {
    let Ok((_, _, _, gen, _)) = fclient::addr::host_parts(host) else {
        return false;
    };
    a.filter(|a| a.owner == *program)
        .and_then(|a| Holding::decode(&a.data).ok())
        .is_some_and(|h| {
            matches!(h.state, 1 | 2)
                && h.gen == gen
                && h.transit
                    .iter()
                    .any(|x| x.state == 1 && x.host_id == host && x.arrive_bell == bell)
        })
}

/// A Reveal refusal that may pass on a later try: the slot moved (re-read
/// and retried), THE anchor not posted yet, too early. Anything else the
/// program answers is final for that arrival (wave-4 review, W4-C blocker).
fn reveal_transient(code: u32) -> bool {
    matches!(
        code,
        abi::err::SLOT_MOVED | abi::err::NO_ANCHOR | abi::err::TOO_EARLY
    )
}

fn present(a: &Option<Account>, program: &Address) -> bool {
    a.as_ref()
        .is_some_and(|a| a.owner == *program && !a.data.is_empty())
}

impl PlayDuty {
    /// Routes the engine's outcomes of play writes (before planning).
    pub fn on_outcome(&mut self, key: &str, o: &Outcome) -> Vec<(String, Value)> {
        let mut tracks: Vec<(String, Value)> = vec![];
        let mut parts = key.split(':');
        let kind = parts.next().unwrap_or("");
        let host: Option<u64> = parts.next().and_then(|h| h.parse().ok());
        let bell: Option<u32> = parts.next().and_then(|h| h.parse().ok());
        let k = host.zip(bell);
        // The owner material and latency are keyed by the arrival bell.
        let ok = k.and_then(|k| self.index.departs.get(&k).map(|d| (d.host, d.arrive)));
        if kind == "close" {
            if let Outcome::Landed { .. } = o {
                self.closed_sent.insert(key.to_string());
            }
        }
        match (kind, o) {
            ("reveal", Outcome::Landed { slot, sig, .. }) => {
                self.stats.reveals_landed += 1;
                if let Some(ok) = ok {
                    let e = self.stats.reveal_latency.entry(ok.1).or_insert((0, 0));
                    e.1 = e.1.max(*slot);
                    if let Some(m) = self.owner.get(&ok) {
                        tracks.push((
                            m.track.clone(),
                            json!({"state": "landed", "signature": sig.to_string(), "slot": slot}),
                        ));
                    }
                }
            }
            ("reveal", Outcome::Failed { code, slot, .. }) => {
                let Some(k) = k else { return tracks };
                let Some(ts) = self.transits.get_mut(&k) else {
                    return tracks;
                };
                match *code {
                    Some(abi::err::SLOT_MOVED) => {
                        self.stats.slot_moved += 1;
                        ts.slot_moved += 1;
                        if ts.slot_moved > SLOT_MOVED_RETRIES {
                            ts.refused = true;
                        }
                    }
                    // A deterministic program refusal (`Path`, `ArrivalBell`,
                    // `Shielded`, a bad plaintext, …): this arrival will never
                    // be revealed. It leaves its group (the group is planned
                    // again without it) and goes to settlement; before the
                    // wave-4 review it stayed in the group, and every
                    // lower-ranked member aimed one index off (`SlotMoved`
                    // until refused, then routed).
                    Some(c) if !reveal_transient(c) && c != abi::err::QUOTA_REFUSED => {
                        let first = !ts.refused;
                        ts.refused = true;
                        if first {
                            self.stats.reveals_refused += 1;
                            self.findings.push((
                                *slot,
                                format!(
                                    "reveal refused {} host {}: out of its group",
                                    abi::error_name(c).unwrap_or("?"),
                                    k.0
                                ),
                            ));
                        }
                    }
                    Some(abi::err::QUOTA_REFUSED) => {
                        ts.refused = true;
                        self.stats.reveals_refused += 1;
                        if let Some(m) = ok.and_then(|ok| self.owner.get(&ok)) {
                            tracks.push((
                                m.track.clone(),
                                json!({"state": "refused", "code": "QuotaRefused"}),
                            ));
                        }
                    }
                    _ => {}
                }
            }
            ("reveal", Outcome::Dead { code, .. }) => {
                let Some(k) = k else { return tracks };
                if let Some(ts) = self.transits.get_mut(&k) {
                    ts.missed = true;
                }
                if let Some(m) = ok.and_then(|ok| self.owner.get(&ok)) {
                    let c = code.and_then(abi::error_name).unwrap_or("WindowClosed");
                    tracks.push((m.track.clone(), json!({"state": "expired", "code": c})));
                }
            }
            ("sdep", Outcome::Landed { .. }) => self.stats.departures_settled += 1,
            ("gather", Outcome::Landed { .. }) => self.stats.gathers += 1,
            ("resolve", Outcome::Landed { slot, .. }) => {
                self.stats.resolves += 1;
                // Close → resolve, measured at the landing (wave-4 review).
                if let Some(c) = self.resolve_close.remove(key) {
                    self.stats.resolve_latency.push(slot.saturating_sub(c));
                }
            }
            ("skip", Outcome::Landed { .. }) => self.stats.skips += 1,
            ("skip", Outcome::Failed { code, .. }) if *code == Some(abi::err::NOT_QUIET) => {
                // key skip:P,Q:b0:n
                self.stats.not_quiet += 1;
                if let (Some(pq), Some(b0)) = (key.split(':').nth(1), key.split(':').nth(2)) {
                    if let (Some((p, q)), Ok(b0)) = (parse_pq(pq), b0.parse::<u32>()) {
                        self.not_quiet.insert((p, q, b0));
                    }
                }
            }
            ("settle", Outcome::Landed { .. }) => self.stats.settles += 1,
            ("close", Outcome::Landed { .. }) => self.stats.closes += 1,
            ("claim", Outcome::Landed { .. }) => self.stats.claims += 1,
            // A refused claim (e.g. W4-B's `day == day(now_bell)` across
            // midnight): its slots may be claimed again by a new write.
            ("claim", Outcome::Failed { .. } | Outcome::Dead { .. }) => {
                if let Some(list) = key.splitn(3, ':').nth(2) {
                    for item in list.split(';') {
                        let v: Vec<i64> = item.split(',').filter_map(|x| x.parse().ok()).collect();
                        if let [p, q, b, f, i] = v[..] {
                            self.unclaim
                                .push((p as i32, q as i32, b as u32, f as u8, i as u8));
                        }
                    }
                }
            }
            ("return", Outcome::Landed { .. }) => self.stats.returns += 1,
            _ => {}
        }
        // Any write on a province that ended (landed or refused): read the
        // province again before planning on it (a refusal usually means the
        // cached state is stale).
        if !matches!(o, Outcome::Dead { .. }) {
            for pq in key.split(':').filter_map(parse_pq) {
                if let Some(p) = self.provinces.get_mut(&pq) {
                    p.read_slot = 0;
                }
            }
        }
        tracks
    }

    /// Takes owner material queued by `/v1/reveal` and nudges.
    fn take_shared(&mut self, shared: &mut Shared) {
        for (track, v) in shared.reveals.drain(..) {
            let get = |k: &str, n: usize| -> Option<Vec<u8>> {
                use base64::Engine as _;
                let b = base64::engine::general_purpose::STANDARD
                    .decode(v.get(k)?.as_str()?)
                    .ok()?;
                (b.len() == n).then_some(b)
            };
            let (Some(plain), Some(salt), Some(ct)) = (
                get("plain_b64", 37),
                get("salt_b64", 32),
                get("ct_hash_b64", 32),
            ) else {
                continue;
            };
            let Some(holding) = v
                .get("holding")
                .and_then(|h| h.as_str())
                .and_then(|h| h.parse::<Address>().ok())
            else {
                continue;
            };
            let slot_i = v.get("transit_slot").and_then(|x| x.as_u64()).unwrap_or(0) as u8;
            let p = seal::unpack(&plain.clone().try_into().expect("37"));
            let m = OwnerMat {
                track: track.clone(),
                holding,
                transit_slot: slot_i,
                plain: plain.try_into().expect("37"),
                salt: salt.try_into().expect("32"),
                ct_hash: ct.try_into().expect("32"),
            };
            self.owner.insert((p.host_id, p.arrive_bell), m);
        }
        for n in shared.nudges.drain(..) {
            let bell = n.get("bell").and_then(|b| b.as_u64()).unwrap_or(0) as u32;
            if let Some(a) = n.get("province").and_then(|p| p.as_array()) {
                if let (Some(p), Some(q)) = (a[0].as_i64(), a[1].as_i64()) {
                    let e = self.nudged.entry((p as i16, q as i16)).or_insert(0);
                    *e = (*e).max(bell);
                }
            }
        }
    }

    /// The race jitter of a write (§6.5 of the offchain design: 0 to
    /// `race_jitter_slots` slots, drawn once per write, so several keepers
    /// do not all pay for the same object).
    fn jitter(&self, t: &Tick<'_>) -> u64 {
        let j = t.cfg.race_jitter_slots;
        if j == 0 {
            return 0;
        }
        use rand::Rng;
        rand::rngs::OsRng.gen_range(0..=j)
    }

    fn observe_clock(&mut self, slot: u64, now: i64) {
        if let Some((s0, n0)) = self.last_clock {
            if slot > s0 && now >= n0 {
                let x = (now - n0) as f64 / (slot - s0) as f64;
                self.slot_secs = if self.slot_secs == 0.0 {
                    x
                } else {
                    0.8 * self.slot_secs + 0.2 * x
                };
            }
        }
        self.last_clock = Some((slot, now));
    }

    /// THE anchor of `(bell, region)`: from the beacon duty, else read.
    async fn anchor<P: ChainPort>(
        &mut self,
        t: &Tick<'_>,
        port: &P,
        beacon: &BTreeMap<(u32, u8), AnchorInfo>,
        bell: u32,
        region: u8,
    ) -> PortResult<Option<AnchorSeen>> {
        let k = (bell, region);
        if let Some(a) = beacon.get(&k) {
            let s = AnchorSeen {
                a: a.a,
                slot: Some(a.slot),
                archived: a.archived,
            };
            self.anchors.insert(k, s);
            return Ok(Some(s));
        }
        if let Some(a) = self.anchors.get(&k) {
            return Ok(Some(*a));
        }
        if self.anchor_tried.get(&k).is_some_and(|&s| t.slot < s + 2) {
            return Ok(None);
        }
        self.anchor_tried.insert(k, t.slot);
        let got = port
            .accounts(
                &[
                    t.addrs.anchor(bell, region),
                    t.addrs.archive(region, fclient::addr::archive_part(bell)),
                ],
                0,
            )
            .await?;
        if let Some(an) = got[0]
            .as_ref()
            .filter(|a| a.owner == t.addrs.program)
            .and_then(|a| BellAnchor::decode(&a.data).ok())
        {
            let s = AnchorSeen {
                a: an.a,
                slot: Some(an.slot),
                archived: false,
            };
            self.anchors.insert(k, s);
            return Ok(Some(s));
        }
        if let Some(ar) = got[1]
            .as_ref()
            .filter(|a| a.owner == t.addrs.program)
            .and_then(|a| AnchorArchive::decode(&a.data).ok())
        {
            if ar.is_archived(bell) {
                let e = ar.entries[bell as usize % ar.entries.len()];
                let s = AnchorSeen {
                    a: t.clock.genesis_ts + (bell as i64 + 1) * BELL_SECS + e.a_off as i64,
                    slot: None,
                    archived: true,
                };
                self.anchors.insert(k, s);
                return Ok(Some(s));
            }
        }
        Ok(None)
    }

    fn src(a: &AnchorSeen) -> AnchorSource {
        if a.archived {
            AnchorSource::Archive
        } else {
            AnchorSource::Anchor
        }
    }

    /// The ArrivalDay bit of `(P, Q, bell)`; `None` until read. A read
    /// after the bell's window closed is final.
    async fn day_bit<P: ChainPort>(
        &mut self,
        t: &Tick<'_>,
        port: &P,
        pq: (i16, i16),
        bell: u32,
        close: Option<i64>,
    ) -> PortResult<bool> {
        let day = bell / abi::BELLS_PER_DAY;
        let k = (pq.0, pq.1, day);
        let fresh = self
            .days
            .get(&k)
            .is_some_and(|(_, at)| close.is_some_and(|c| *at >= c));
        if !fresh {
            let got = port
                .accounts(&[t.addrs.arrival_day(pq.0 as i32, pq.1 as i32, day)], 0)
                .await?;
            let bits = got[0]
                .as_ref()
                .filter(|a| a.owner == t.addrs.program)
                .and_then(|a| ArrivalDay::decode(&a.data).ok())
                .map(|d| d.bits);
            self.days.insert(k, (bits, t.now));
        }
        let bits = self.days.get(&k).and_then(|x| x.0);
        let j = (bell % abi::BELLS_PER_DAY) as usize;
        Ok(bits.is_some_and(|b| b[j / 8] & (1 << (j % 8)) != 0))
    }

    /// Reads provinces due (touched, or not read for a while).
    async fn read_provinces<P: ChainPort>(
        &mut self,
        t: &Tick<'_>,
        port: &P,
        opened: &[(i32, i32)],
    ) -> PortResult<()> {
        for &(p, q) in opened {
            self.provinces
                .entry((p as i16, q as i16))
                .or_insert(ProvState {
                    region: ix::region_of(p, q),
                    rn: 0,
                    read_slot: 0,
                    pending_bells: BTreeSet::new(),
                    active: false,
                    leaves: vec![],
                    last_digest: [0; 32],
                    contested: false,
                });
        }
        for pq in std::mem::take(&mut self.index.touched) {
            let e = self.provinces.entry(pq).or_insert(ProvState {
                region: ix::region_of(pq.0 as i32, pq.1 as i32),
                rn: 0,
                read_slot: 0,
                pending_bells: BTreeSet::new(),
                active: false,
                leaves: vec![],
                last_digest: [0; 32],
                contested: false,
            });
            e.read_slot = 0;
        }
        let due: Vec<(i16, i16)> = self
            .provinces
            .iter()
            .filter(|(_, s)| s.read_slot == 0 || t.slot >= s.read_slot + PROVINCE_READ_SLOTS)
            .map(|(k, _)| *k)
            .collect();
        for chunk in due.chunks(64) {
            let keys: Vec<Address> = chunk
                .iter()
                .map(|&(p, q)| t.addrs.province(p as i32, q as i32))
                .collect();
            let got = port.accounts(&keys, 0).await?;
            for (pq, a) in chunk.iter().zip(got) {
                let Some(pv) = a
                    .filter(|a| a.owner == t.addrs.program)
                    .and_then(|a| Province::decode(&a.data).ok())
                else {
                    self.provinces.remove(pq);
                    continue;
                };
                let s = self.provinces.get_mut(pq).expect("due");
                s.rn = pv.resolved_next;
                s.region = pv.region;
                s.read_slot = t.slot.max(1);
                s.last_digest = pv.last_outcome_digest;
                s.pending_bells = pv
                    .entries
                    .iter()
                    .filter(|e| e.state != 0 && e.pend_op != 0 && e.state != 3)
                    .map(|e| e.pend_bell)
                    .chain(
                        pv.entries
                            .iter()
                            .filter(|e| e.state == 2)
                            .map(|e| e.from_bell.saturating_sub(1)),
                    )
                    .collect();
                s.active = pv.entries.iter().any(|e| e.state != 0);
                let mut fs: Vec<u8> = pv
                    .entries
                    .iter()
                    .filter(|e| e.state == 1)
                    .map(|e| e.faction)
                    .collect();
                fs.sort_unstable();
                fs.dedup();
                s.contested = fs.len() >= 2;
                // A nudge holds until the province is resolved through
                // b − 2 (resident actions allowed).
                let target = self
                    .nudged
                    .get(pq)
                    .copied()
                    .unwrap_or(0)
                    .max(t.clock.bell_at(t.now).unwrap_or(0));
                if self.nudged.contains_key(pq) && pv.resolved_next + 1 >= target {
                    self.nudged.remove(pq);
                }
                s.leaves = pv
                    .entries
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| e.state == 3 && e.pend_op == 5)
                    .map(|(i, e)| (i as u8, e.id, e.pend_bell))
                    .collect();
            }
        }
        Ok(())
    }

    /// Reads the Holdings of transits not settled (slot index, state,
    /// faction, citizen tag, rent payer).
    async fn read_holdings<P: ChainPort>(&mut self, t: &Tick<'_>, port: &P) -> PortResult<()> {
        let want: Vec<((u64, u32), Address)> = self
            .index
            .departs
            .iter()
            .filter(|(k, _)| !self.index.settled.contains_key(k))
            .filter(|(k, _)| {
                self.transits
                    .get(k)
                    .is_none_or(|s| s.holding_read_slot == 0 || t.slot >= s.holding_read_slot + 4)
            })
            .filter_map(|(k, d)| t.addrs.holding_of_host(d.host).ok().map(|h| (*k, h)))
            .collect();
        for chunk in want.chunks(100) {
            let keys: Vec<Address> = chunk.iter().map(|x| x.1).collect();
            let got = port.accounts(&keys, 0).await?;
            for ((k, _), a) in chunk.iter().zip(got) {
                let st = self.transits.entry(*k).or_default();
                st.holding_read_slot = t.slot.max(1);
                let Some(h) = a
                    .filter(|a| a.owner == t.addrs.program)
                    .and_then(|a| Holding::decode(&a.data).ok())
                else {
                    st.tstate = 0;
                    st.transit_slot = None;
                    continue;
                };
                match h
                    .transit
                    .iter()
                    .enumerate()
                    .find(|(_, x)| x.state != 0 && x.host_id == k.0 && x.depart_bell == k.1)
                {
                    Some((i, x)) => {
                        st.transit_slot = Some(i as u8);
                        st.tstate = x.state;
                        st.faction = x.faction;
                        st.gathered_at = x.gathered_at;
                    }
                    None => {
                        st.transit_slot = None;
                        st.tstate = 0;
                        st.gathered_at = None;
                    }
                }
                st.citizen_tag =
                    u64::from_le_bytes(h.owner_citizen.to_bytes()[..8].try_into().expect("8"));
                st.rent_payer = h.rent_payer;
                st.owner_citizen = h.owner_citizen;
            }
        }
        Ok(())
    }

    /// One tick of every play duty.
    #[allow(clippy::too_many_arguments)]
    pub async fn plan<P: ChainPort, D: DrandPort>(
        &mut self,
        t: &Tick<'_>,
        port: &P,
        drand: &D,
        rounds: &mut Rounds,
        engine: &mut Engine,
        beacon: &BTreeMap<(u32, u8), AnchorInfo>,
        seeds: &mut SeedFinder,
        shared: &mut Shared,
        journal: Option<&Journal>,
        opened: &[(i32, i32)],
    ) -> PortResult<()> {
        self.observe_clock(t.slot, t.now);
        self.take_shared(shared);
        let eff = t.season.effective_status(t.now);
        if !(eff == status::RUNNING || eff == status::ENDED) {
            return Ok(());
        }
        self.index.pull(port, t.addrs).await?;
        self.read_provinces(t, port, opened).await?;
        self.read_holdings(t, port).await?;
        if t.cfg.has_role("reveal") || t.cfg.has_role("settle") {
            self.decrypt(t, drand, rounds, journal).await;
        }
        if t.cfg.has_role("reveal") {
            self.reveals(t, port, engine, beacon, shared).await?;
        }
        if t.cfg.has_role("settle-departure") {
            self.settle_departures(t, engine);
            self.returns(t, engine);
        }
        if t.cfg.has_role("gather") || t.cfg.has_role("resolve") || t.cfg.has_role("skip") {
            self.clashes(t, port, engine, beacon, seeds).await?;
        }
        if t.cfg.has_role("settle") {
            self.settles(t, port, engine, beacon).await?;
        }
        if t.cfg.has_role("close") {
            self.closes(t, port, engine, beacon).await?;
        }
        if t.cfg.has_role("claims") {
            self.claims(t, port, engine, beacon).await?;
        }
        Ok(())
    }

    // ------------------------------------------------------------ decrypt

    async fn decrypt<D: DrandPort>(
        &mut self,
        t: &Tick<'_>,
        drand: &D,
        rounds: &mut Rounds,
        journal: Option<&Journal>,
    ) {
        let keys: Vec<(u64, u32)> = self
            .index
            .departs
            .keys()
            .copied()
            .filter(|k| !self.index.settled.contains_key(k))
            .filter(|k| self.transits.get(k).is_none_or(|s| s.opened.is_none()))
            .collect();
        for k in keys {
            let d = self.index.departs[&k].clone();
            // The journal's cache (a restart does not re-open what it opened).
            if let Some((p, s)) = journal
                .and_then(|j| j.plaintext(d.host, d.arrive).ok().flatten())
                .and_then(|(p, s)| {
                    Some((<[u8; 37]>::try_from(p).ok()?, <[u8; 32]>::try_from(s).ok()?))
                })
            {
                if seal::commit(&p, &s) == d.commit {
                    self.transits.entry(k).or_default().opened = Some(Opened {
                        code: abi::seal_code::VALID,
                        plain: Some(p),
                        salt: Some(s),
                    });
                    continue;
                }
            }
            let r = t.clock.tlock_round(d.arrive);
            if t.clock.drand.round_time(r) > t.now {
                continue;
            }
            let Some(b) = rounds.get(drand, r, t.slot).await else {
                continue;
            };
            let o = play::open_march(&d.seal, &d.commit, &b.sig48, d.host, d.arrive);
            self.stats.opened += 1;
            if !o.valid() {
                self.stats.bad_seals += 1;
            }
            self.stats
                .reveal_latency
                .entry(d.arrive)
                .or_insert((t.slot, 0));
            if let (Some(j), Some(p), Some(s)) = (journal, o.plain, o.salt) {
                let _ = j.save_plaintext(d.host, d.arrive, &p, &s);
            }
            self.transits.entry(k).or_default().opened = Some(o);
        }
    }

    /// The reveal material of a transit: owner submission first, else the
    /// opened seal. `(plain, salt, ct_hash, track)`.
    fn material(&self, d: &DepartRec) -> Option<([u8; 37], [u8; 32], [u8; 32])> {
        if let Some(m) = self.owner.get(&(d.host, d.arrive)) {
            return Some((m.plain, m.salt, m.ct_hash));
        }
        let o = self.transits.get(&(d.host, d.depart_bell))?.opened?;
        if !o.valid() {
            return None;
        }
        Some((o.plain?, o.salt?, seal::ct_hash(&d.seal)))
    }

    // ------------------------------------------------------------ reveals

    async fn reveals<P: ChainPort>(
        &mut self,
        t: &Tick<'_>,
        port: &P,
        engine: &mut Engine,
        beacon: &BTreeMap<(u32, u8), AnchorInfo>,
        shared: &mut Shared,
    ) -> PortResult<()> {
        // Groups of revealable arrivals.
        type G = (i16, i16, u32, u8);
        let mut groups: BTreeMap<G, Vec<(u64, u32)>> = BTreeMap::new();
        for (k, d) in &self.index.departs {
            if self.index.settled.contains_key(k) {
                continue;
            }
            let Some(st) = self.transits.get(k) else {
                continue;
            };
            if st.refused || st.missed || !(1..=3).contains(&st.tstate) || st.transit_slot.is_none()
            {
                continue;
            }
            let Some((plain, _, _)) = self.material(d) else {
                continue;
            };
            let p = seal::unpack(&plain);
            if self
                .index
                .slots
                .values()
                .any(|r| r.host == d.host && r.arrive == d.arrive)
            {
                continue;
            }
            groups
                .entry((p.dest_p, p.dest_q, d.arrive, st.faction))
                .or_default()
                .push(*k);
        }
        for ((dp, dq, arrive, faction), members) in groups {
            // THE anchor first, even for a group in flight: `guard_reveals`
            // cancels by it (a keeper whose only role is `reveal` otherwise
            // never learns A while its versions escalate; wave-4 review).
            let region = ix::region_of(dp as i32, dq as i32);
            let anchor = self.anchor(t, port, beacon, arrive, region).await?;
            if members
                .iter()
                .any(|k| engine.is_pending(&format!("reveal:{}", rkey(*k))))
            {
                continue;
            }
            let close = anchor.map(|a| t.clock.reveal_close(arrive, a.a));
            let eps = (REVEAL_EPSILON_SLOTS as f64 * self.slot_secs.max(0.4)).ceil() as i64;
            if let Some(c) = close {
                if t.now + eps >= c {
                    for k in &members {
                        if let Some(st) = self.transits.get_mut(k) {
                            if !st.missed {
                                st.missed = true;
                                // An outranked member was never meant to
                                // be revealed: not a liveness finding.
                                if !st.outranked {
                                    self.stats.reveals_missed += 1;
                                    self.findings.push((
                                        t.slot,
                                        format!("ValidSealUnrevealed host {} arrive {arrive}", k.0),
                                    ));
                                }
                            }
                        }
                        if let Some(m) = self.owner.get(&(k.0, arrive)) {
                            shared.tracks.insert(
                                m.track.clone(),
                                json!({"state": "expired", "code": "WindowClosed"}),
                            );
                        }
                    }
                    continue;
                }
            }
            // The latch: inputs present or the destination resolved past.
            if self.provinces.get(&(dp, dq)).is_some_and(|s| s.rn > arrive) {
                continue;
            }
            let mut keys: Vec<Address> = (0..4u8)
                .map(|i| {
                    t.addrs
                        .arrival_slot(dp as i32, dq as i32, arrive, faction, i)
                })
                .collect();
            keys.push(t.addrs.clash_inputs(dp as i32, dq as i32, arrive));
            let got = port.accounts(&keys, 0).await?;
            if present(&got[4], &t.addrs.program) {
                continue;
            }
            let mut slots: [Option<SlotEntry>; 4] = [None; 4];
            for (i, a) in got[..4].iter().enumerate() {
                if let Some(s) = a
                    .as_ref()
                    .filter(|a| a.owner == t.addrs.program)
                    .and_then(|a| ArrivalSlot::decode(&a.data).ok())
                {
                    slots[i] = Some(SlotEntry {
                        host_id: s.host_id,
                        citizen: s.citizen_tag,
                        troops: s.dep_mass,
                    });
                }
            }
            let mut cands: Vec<(SlotEntry, (u64, u32))> = members
                .iter()
                .filter_map(|k| {
                    let d = self.index.departs.get(k)?;
                    let st = self.transits.get(k)?;
                    Some((
                        SlotEntry {
                            host_id: d.host,
                            citizen: st.citizen_tag,
                            troops: d.dep_mass,
                        },
                        *k,
                    ))
                })
                .collect();
            cands.sort_by_key(|(e, _)| play::rank(dp as i32, dq as i32, arrive, e));
            let mut all: Vec<SlotEntry> = slots.iter().flatten().copied().collect();
            all.extend(cands.iter().map(|c| c.0));
            // Every final-set member at once, each at the index it will
            // take when the ones ranked above it land first; a small bid
            // bonus by rank makes a block land them in that order (a
            // different order is a `SlotMoved`, re-read and retried).
            let n_c = cands.len() as u64;
            let mut rank_i = 0u64;
            for (e, k) in cands {
                if slots.iter().flatten().any(|s| s.host_id == e.host_id) {
                    continue;
                }
                if !play::in_final_set(dp as i32, dq as i32, arrive, &all, &e) {
                    // Outranked as the group stands now: not revealed this
                    // plan, but not refused for good either — a member
                    // ranked above may still leave the group (a program
                    // refusal), and then this one belongs to the final set
                    // (wave-4 review).
                    if let Some(st) = self.transits.get_mut(&k) {
                        if !st.outranked {
                            st.outranked = true;
                            self.stats.reveals_refused += 1;
                        }
                    }
                    continue;
                }
                if let Some(st) = self.transits.get_mut(&k) {
                    st.outranked = false;
                }
                let tgt = play::target(dp as i32, dq as i32, arrive, &slots, e);
                let Some(i) = tgt.index() else {
                    if let Target::Refused(_) = tgt {
                        if let Some(st) = self.transits.get_mut(&k) {
                            st.refused = true;
                        }
                    }
                    continue;
                };
                // Build the Reveal.
                let d = self.index.departs[&k].clone();
                let st = self.transits[&k].clone();
                let Some((plain, salt, ct)) = self.material(&d) else {
                    continue;
                };
                let p = seal::unpack(&plain);
                let Some(path) =
                    play::path_provinces((d.origin.0 as i32, d.origin.1 as i32, d.origin_tile), &p)
                else {
                    if let Some(st) = self.transits.get_mut(&k) {
                        st.refused = true;
                    }
                    self.findings
                        .push((t.slot, format!("path not revealable host {}", d.host)));
                    continue;
                };
                let day_set = self.day_bit(t, port, (dp, dq), arrive, None).await?;
                let (hp, hq, hs, _, _) = match fclient::addr::host_parts(d.host) {
                    Ok(x) => x,
                    Err(_) => continue,
                };
                let args = RevealArgs {
                    holding: HoldingRef {
                        p: hp as i16,
                        q: hq as i16,
                        site: hs,
                    },
                    transit_slot: st.transit_slot.unwrap_or(0),
                    target_i: i,
                    plain,
                    salt,
                    ct_hash: ct,
                    beneficiary: t.cfg.beneficiary,
                    dest: (dp as i32, dq as i32),
                    arrive,
                    faction,
                    day_writable: !day_set,
                    path_provinces: path,
                };
                let a = t.addrs.clone();
                let deadline = close.map(|c| {
                    let slots_left = ((c - t.now) as f64 / self.slot_secs.max(0.4)).floor() as i64;
                    t.slot + (slots_left - REVEAL_EPSILON_SLOTS).max(0) as u64
                });
                let key = format!("reveal:{}", rkey(k));
                let jitter = self.jitter(t);
                if engine.ensure(
                    WriteSpec {
                        key,
                        kind: "reveal",
                        tag: tag::REVEAL,
                        class: Class::W,
                        bell: Some(arrive),
                        region: Some(region),
                        build: Arc::new(move |c: &BuildCtx| vec![ix::reveal(&a, c.payer, &args)]),
                        deadline_slot: deadline,
                        not_before_slot: t.slot + jitter,
                        fixed_payer: None,
                    },
                    t.slot,
                ) {
                    engine.set_bid_bonus(&format!("reveal:{}", rkey(k)), n_c - rank_i);
                    self.stats.reveals_sent += 1;
                    if let Some(m) = self.owner.get(&(k.0, arrive)) {
                        shared
                            .tracks
                            .insert(m.track.clone(), json!({"state": "sent"}));
                    }
                } else {
                    // Not sent (a backoff after a transient refusal): the
                    // members ranked below it cannot know their index until
                    // it lands or leaves the group; the group waits for the
                    // next plan (wave-4 review).
                    break;
                }
                slots[i as usize] = Some(e);
                rank_i += 1;
            }
        }
        Ok(())
    }

    /// Cancels Reveal writes whose window is within ε of its close (the
    /// engine's deadline slot is an estimate; this is the Clock's rule).
    pub fn guard_reveals(&mut self, t: &Tick<'_>, engine: &mut Engine) {
        let eps = (REVEAL_EPSILON_SLOTS as f64 * self.slot_secs.max(0.4)).ceil() as i64;
        for key in engine.pending_keys() {
            let Some(rest) = key.strip_prefix("reveal:") else {
                continue;
            };
            let mut it = rest.split(':');
            let (Some(h), Some(b)) = (it.next(), it.next()) else {
                continue;
            };
            let (Ok(h), Ok(b)) = (h.parse::<u64>(), b.parse::<u32>()) else {
                continue;
            };
            let Some(d) = self.index.departs.get(&(h, b)) else {
                continue;
            };
            let Some((plain, _, _)) = self.material(d) else {
                continue;
            };
            let p = seal::unpack(&plain);
            let region = ix::region_of(p.dest_p as i32, p.dest_q as i32);
            if let Some(a) = self.anchors.get(&(d.arrive, region)) {
                if t.now + eps >= t.clock.reveal_close(d.arrive, a.a) {
                    engine.cancel(&key);
                    if let Some(st) = self.transits.get_mut(&(h, b)) {
                        st.missed = true;
                    }
                    self.stats.reveals_missed += 1;
                    self.findings.push((
                        t.slot,
                        format!(
                            "ValidSealUnrevealed host {h} arrive {}: cancelled at A + W − ε",
                            d.arrive
                        ),
                    ));
                }
            }
        }
    }

    // ------------------------------------------------------------ departures and returns

    fn settle_departures(&mut self, t: &Tick<'_>, engine: &mut Engine) {
        for (k, d) in &self.index.departs {
            if self.index.departure_settled.contains(k) || self.index.settled.contains_key(k) {
                continue;
            }
            let Some(st) = self.transits.get(k) else {
                continue;
            };
            if st.tstate != 1 {
                continue;
            }
            let Some(o) = self.provinces.get(&d.origin) else {
                continue;
            };
            if o.rn <= d.depart_bell {
                continue;
            }
            let Ok((hp, hq, hs, _, _)) = fclient::addr::host_parts(d.host) else {
                continue;
            };
            let a = t.addrs.clone();
            let origin = d.origin;
            let ts = st.transit_slot.unwrap_or(0);
            let href = HoldingRef {
                p: hp as i16,
                q: hq as i16,
                site: hs,
            };
            engine.ensure(
                WriteSpec {
                    key: format!("sdep:{}:{},{}", rkey(*k), origin.0, origin.1),
                    kind: "settle-departure",
                    tag: tag::SETTLE_DEPARTURE,
                    class: Class::D,
                    bell: Some(d.depart_bell),
                    region: Some(ix::region_of(origin.0 as i32, origin.1 as i32)),
                    build: Arc::new(move |c: &BuildCtx| {
                        vec![ix::settle_departure(&a, c.payer, origin, href, ts)]
                    }),
                    deadline_slot: None,
                    not_before_slot: t.slot + self.jitter(t),
                    fixed_payer: None,
                },
                t.slot,
            );
        }
    }

    /// The §21 return settle: one SettleDeparture with `transit_slot =
    /// RETURN_SLOT` per `(province, Holding)` with a `Leave` entry whose bell
    /// resolved. The program frees at most three per transaction (v1.7
    /// `RETURN_MAX`); the province is read again after each landing and the
    /// write planned again while entries remain.
    fn returns(&mut self, t: &Tick<'_>, engine: &mut Engine) {
        for (pq, s) in &self.provinces {
            let mut holdings: BTreeSet<(i16, i16, u8)> = BTreeSet::new();
            for &(_, host, pend_bell) in &s.leaves {
                if pend_bell >= s.rn {
                    continue;
                }
                if let Ok((hp, hq, hs, _, _)) = fclient::addr::host_parts(host) {
                    holdings.insert((hp as i16, hq as i16, hs));
                }
            }
            for (hp, hq, hs) in holdings {
                let a = t.addrs.clone();
                let pq = *pq;
                let href = HoldingRef {
                    p: hp,
                    q: hq,
                    site: hs,
                };
                engine.ensure(
                    WriteSpec {
                        key: format!("return:{hp},{hq},{hs}:{},{}", pq.0, pq.1),
                        kind: "return",
                        tag: tag::SETTLE_DEPARTURE,
                        class: Class::D,
                        bell: Some(s.rn),
                        region: Some(s.region),
                        build: Arc::new(move |c: &BuildCtx| {
                            vec![ix::settle_return(&a, c.payer, pq, href)]
                        }),
                        deadline_slot: None,
                        not_before_slot: t.slot + self.jitter(t),
                        fixed_payer: None,
                    },
                    t.slot,
                );
            }
        }
    }

    // ------------------------------------------------------------ gathers, resolves, skips

    /// The earliest bell a province must be resolved through soon (its
    /// `resolved_next` must pass it), or `None` when nothing waits on it
    /// (W6-C, W5-B F4). A skip goes out as soon as its run of closed quiet
    /// bells covers this bell, in one transaction; before W6-C any pending
    /// work made the province "urgent" and every bell was skipped alone as
    /// it closed (13–40 SkipQuiet per province-day p99 in the W5 runs).
    ///
    /// - a nudge (a player acting there now): the next bell (at once);
    /// - a pending resident change or a `Leave` entry: its bell;
    /// - a departure from here to settle: its departure bell;
    /// - an arrival here ahead: the bell before it (so the arrival bell is
    ///   next when its window closes: close → resolve stays short); an
    ///   arrival bell itself left clear (nothing revealed), and a
    ///   settlement judged against this province (a bad seal nobody
    ///   revealed is settled against its origin): that bell.
    ///
    /// The season's end is `end_flush` in [`Self::clashes`].
    fn skip_target(&self, pq: (i16, i16), s: &ProvState) -> Option<u32> {
        if self.nudged.contains_key(&pq) {
            return Some(s.rn);
        }
        let own = s
            .pending_bells
            .iter()
            .copied()
            .chain(s.leaves.iter().map(|l| l.2));
        let marches = self.index.departs.iter().filter_map(|(k, d)| {
            if self.index.settled.contains_key(k) {
                return None;
            }
            let from_here = (d.origin == pq
                && !self.index.departure_settled.contains(k)
                && d.depart_bell >= s.rn)
                .then_some(d.depart_bell);
            let to_here = (self.dest_of(k, d).unwrap_or(d.origin) == pq && s.rn <= d.arrive)
                .then(|| d.arrive.saturating_sub(1).max(s.rn));
            match (from_here, to_here) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            }
        });
        // Bells already passed wait on nothing here (their settlement can
        // go now).
        own.chain(marches).filter(|&b| b >= s.rn).min()
    }

    /// Whether a skip of `n` bells from `b` goes out now: it covers the
    /// province's target bell, it is a whole batch, or it reaches the
    /// season's end.
    fn skip_now(&self, end_bell: u32, pq: (i16, i16), s: &ProvState, b: u32, n: u32) -> bool {
        let covers = self.skip_target(pq, s).is_some_and(|tb| tb < b + n);
        covers || n >= SKIP_MAX || b + n >= end_bell
    }

    fn dest_of(&self, k: &(u64, u32), d: &DepartRec) -> Option<(i16, i16)> {
        if let Some(m) = self.owner.get(&(d.host, d.arrive)) {
            let p = seal::unpack(&m.plain);
            return Some((p.dest_p, p.dest_q));
        }
        if let Some(p) = self
            .transits
            .get(k)
            .and_then(|s| s.opened)
            .and_then(|o| o.plain())
        {
            return Some((p.dest_p, p.dest_q));
        }
        self.index
            .slots
            .values()
            .find(|r| r.host == d.host && r.arrive == d.arrive)
            .map(|r| r.dest)
    }

    async fn clashes<P: ChainPort>(
        &mut self,
        t: &Tick<'_>,
        port: &P,
        engine: &mut Engine,
        beacon: &BTreeMap<(u32, u8), AnchorInfo>,
        seeds: &mut SeedFinder,
    ) -> PortResult<()> {
        let now_bell = t.clock.bell_at(t.now).unwrap_or(0);
        let pqs: Vec<(i16, i16)> = self.provinces.keys().copied().collect();
        for pq in pqs {
            let s = self.provinces[&pq].clone();
            if s.rn >= t.season.end_bell || s.rn >= now_bell || s.read_slot == 0 {
                continue;
            }
            let pk = format!("{},{}", pq.0, pq.1);
            if engine.pending_keys().iter().any(|k| {
                k.contains(&format!(":{pk}:"))
                    && (k.starts_with("gather")
                        || k.starts_with("resolve")
                        || k.starts_with("skip"))
            }) {
                continue;
            }
            let b = s.rn;
            let Some(an) = self.anchor(t, port, beacon, b, s.region).await? else {
                continue;
            };
            let close = t.clock.reveal_close(b, an.a);
            if t.now < close {
                continue;
            }
            let bit = self.day_bit(t, port, pq, b, Some(close)).await?;
            if bit || s.contested || self.not_quiet.contains(&(pq.0, pq.1, b)) {
                self.clash_bell(t, port, engine, beacon, seeds, pq, &s, b, an, close, bit)
                    .await?;
                continue;
            }
            if !t.cfg.has_role("skip") {
                continue;
            }
            // A run of closed, clear bells from b.
            let mut srcs = vec![Self::src(&an)];
            let mut recomputed = u32::from(s.pending_bells.contains(&b));
            let mut bb = b + 1;
            while (srcs.len() as u32) < SKIP_MAX && bb < t.season.end_bell && bb < now_bell {
                let Some(a2) = self.anchor(t, port, beacon, bb, s.region).await? else {
                    break;
                };
                let c2 = t.clock.reveal_close(bb, a2.a);
                if t.now < c2 || self.day_bit(t, port, pq, bb, Some(c2)).await? {
                    break;
                }
                if self.not_quiet.contains(&(pq.0, pq.1, bb)) {
                    break;
                }
                recomputed += u32::from(s.pending_bells.contains(&bb));
                srcs.push(Self::src(&a2));
                bb += 1;
            }
            let n = srcs.len() as u32;
            if !self.skip_now(t.season.end_bell, pq, &s, b, n) {
                continue;
            }
            let a = t.addrs.clone();
            let key = format!("skip:{pk}:{b}:{n}");
            // §5.5 v1.7: 90k + 30k per recomputed bell (the kernel quiet
            // test of a non-trivial roster at the first bell included).
            let cu = frontier_abi::budgets::cu_gate(
                frontier_abi::tags::Ix::SkipQuiet,
                recomputed.max(1),
            )
            .min(abi::CU_MAX);
            let dest = (pq.0 as i32, pq.1 as i32);
            if engine.ensure(
                WriteSpec {
                    key: key.clone(),
                    kind: "skip",
                    tag: tag::SKIP_QUIET,
                    class: Class::D,
                    bell: Some(b),
                    region: Some(s.region),
                    build: Arc::new(move |c: &BuildCtx| {
                        vec![ix::skip_quiet(&a, c.payer, dest, b, n as u8, &srcs)]
                    }),
                    deadline_slot: None,
                    not_before_slot: t.slot + self.jitter(t),
                    fixed_payer: None,
                },
                t.slot,
            ) {
                engine.set_cu_limit(&key, cu);
                self.stats.skipped_bells += n as u64;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn clash_bell<P: ChainPort>(
        &mut self,
        t: &Tick<'_>,
        port: &P,
        engine: &mut Engine,
        beacon: &BTreeMap<(u32, u8), AnchorInfo>,
        seeds: &mut SeedFinder,
        pq: (i16, i16),
        s: &ProvState,
        b: u32,
        an: AnchorSeen,
        close: i64,
        bit: bool,
    ) -> PortResult<()> {
        let (p, q) = (pq.0 as i32, pq.1 as i32);
        let pk = format!("{},{}", pq.0, pq.1);
        let got = port.accounts(&[t.addrs.clash_inputs(p, q, b)], 0).await?;
        let ci = got[0]
            .as_ref()
            .filter(|a| a.owner == t.addrs.program)
            .and_then(|a| ClashInputs::decode(&a.data).ok());
        let mask = ci.as_ref().map_or(0, |c| c.arrivals_mask);
        if ci.as_ref().is_some_and(|c| c.resolved()) {
            return Ok(());
        }
        if mask != 0x00FF_FFFF {
            if !t.cfg.has_role("gather") {
                return Ok(());
            }
            let src = Self::src(&an);
            let a = t.addrs.clone();
            let ben = t.cfg.beneficiary;
            if !bit {
                // The no-arrival fast path (one gather). The program takes
                // a non-empty range even here (`n == 0` is `BadData`):
                // position 0's slot, absent (the day bit is clear and the
                // window closed), with no Holding (integ-W4).
                engine.ensure(
                    WriteSpec {
                        key: format!("gather:{pk}:{b}:all"),
                        kind: "gather",
                        tag: tag::GATHER_CLASH,
                        class: Class::D,
                        bell: Some(b),
                        region: Some(s.region),
                        build: Arc::new(move |c: &BuildCtx| {
                            vec![ix::gather_clash(
                                &a,
                                c.payer,
                                (p, q),
                                b,
                                src,
                                0,
                                &[(0, 0)],
                                &[],
                                0,
                                &ben,
                            )]
                        }),
                        deadline_slot: None,
                        not_before_slot: t.slot + self.jitter(t),
                        fixed_payer: None,
                    },
                    t.slot,
                );
                return Ok(());
            }
            // The 24 positions.
            let keys: Vec<Address> = (0..24u8)
                .map(|k| t.addrs.arrival_slot(p, q, b, k / 4, k % 4))
                .collect();
            let slots = port.accounts(&keys, 0).await?;
            let mut presentp: [Option<Address>; play::POSITIONS] = [None; play::POSITIONS];
            let mut hosts: Vec<(usize, u64, Address)> = vec![];
            for (k, a) in slots.iter().enumerate() {
                if let Some(sl) = a
                    .as_ref()
                    .filter(|a| a.owner == t.addrs.program)
                    .and_then(|a| ArrivalSlot::decode(&a.data).ok())
                {
                    if let Ok(h) = t.addrs.holding_of_host(sl.host_id) {
                        presentp[k] = Some(h);
                        hosts.push((k, sl.host_id, h));
                    }
                }
            }
            // Wait only where the program would refuse the gather
            // (`DepartureUnsettled`): the host's own live Holding still holds
            // its transit in state 1. An absent, released or re-founded
            // Holding, or no matching transit (settled against another
            // Province, freed), gathers as not present (§5.11 v1.6; wave-4
            // review: waiting for a transit that is gone stalled the
            // province for good).
            let hk: Vec<Address> = hosts.iter().map(|x| x.2).collect();
            let hs = if hk.is_empty() {
                vec![]
            } else {
                port.accounts(&hk, 0).await?
            };
            for ((k, host, _), a) in hosts.iter().zip(hs) {
                if mask & (1 << k) != 0 {
                    continue;
                }
                if departure_unsettled(a.as_ref(), &t.addrs.program, *host, b) {
                    return Ok(()); // SettleDeparture first (lag waits)
                }
            }
            for part in play::gather_parts(mask, &presentp) {
                let a = t.addrs.clone();
                let key = format!("gather:{pk}:{b}:{}", part.start);
                engine.ensure(
                    WriteSpec {
                        key,
                        kind: "gather",
                        tag: tag::GATHER_CLASH,
                        class: Class::D,
                        bell: Some(b),
                        region: Some(s.region),
                        build: Arc::new(move |c: &BuildCtx| {
                            vec![ix::gather_clash(
                                &a,
                                c.payer,
                                (p, q),
                                b,
                                src,
                                part.start,
                                &part.slots,
                                &part.holdings,
                                part.bitmap,
                                &ben,
                            )]
                        }),
                        deadline_slot: None,
                        not_before_slot: t.slot + self.jitter(t),
                        fixed_payer: None,
                    },
                    t.slot,
                );
            }
            return Ok(());
        }
        if !t.cfg.has_role("resolve") {
            return Ok(());
        }
        let Some(seed) = seeds
            .find(port, t.addrs, beacon, b, s.region, t.slot)
            .await?
        else {
            return Ok(());
        };
        let a = t.addrs.clone();
        let ben = t.cfg.beneficiary;
        let src = seed.src;
        if engine.ensure(
            WriteSpec {
                key: format!("resolve:{pk}:{b}"),
                kind: "resolve",
                tag: tag::RESOLVE_FROM_INPUTS,
                class: Class::D,
                bell: Some(b),
                region: Some(s.region),
                build: Arc::new(move |c: &BuildCtx| {
                    vec![ix::resolve_from_inputs(&a, c.payer, (p, q), b, src, &ben)]
                }),
                deadline_slot: None,
                not_before_slot: t.slot + self.jitter(t),
                fixed_payer: None,
            },
            t.slot,
        ) {
            let close_slot = self.slot_at(t, close);
            self.resolve_close
                .insert(format!("resolve:{pk}:{b}"), close_slot);
        }
        Ok(())
    }

    /// The slot at which the Clock reaches `ts` (estimate from the rate).
    fn slot_at(&self, t: &Tick<'_>, ts: i64) -> u64 {
        let d = ((t.now - ts) as f64 / self.slot_secs.max(0.4)).round() as i64;
        (t.slot as i64 - d).max(0) as u64
    }

    // ------------------------------------------------------------ settlement

    async fn settles<P: ChainPort>(
        &mut self,
        t: &Tick<'_>,
        port: &P,
        engine: &mut Engine,
        beacon: &BTreeMap<(u32, u8), AnchorInfo>,
    ) -> PortResult<()> {
        let keys: Vec<(u64, u32)> = self
            .index
            .departs
            .keys()
            .copied()
            .filter(|k| !self.index.settled.contains_key(k))
            .collect();
        for k in keys {
            let key = format!("settle:{}", rkey(k));
            if engine.is_pending(&key) {
                continue;
            }
            let d = self.index.departs[&k].clone();
            let Some(st) = self.transits.get(&k).cloned() else {
                continue;
            };
            if !matches!(st.tstate, 2 | 3) {
                continue;
            }
            // The destination: the one a gather stamped (v1.7: the only
            // one the program accepts); else a valid seal's plaintext; else
            // the revealed one; else, once the seal is judged bad (a bad
            // seal nobody revealed), the origin province. An unopened seal
            // waits (settling it against the origin would be refused
            // `BadAddress` if it proves valid).
            let judged_bad = st.opened.is_some_and(|o| !o.valid());
            let dest = match st.gathered_at.or_else(|| self.dest_of(&k, &d)) {
                Some(x) => x,
                None if judged_bad => d.origin,
                None => continue,
            };
            let region = match self.provinces.get(&dest) {
                Some(ds) => {
                    if ds.rn <= d.arrive {
                        continue;
                    }
                    ds.region
                }
                None => {
                    // v1.7: a valid seal to a Province that is not open
                    // settles as routed at its canonical (absent) address.
                    let valid = st.opened.is_some_and(|o| o.valid())
                        || self.owner.contains_key(&(d.host, d.arrive));
                    if !valid {
                        continue;
                    }
                    let pk = t.addrs.province(dest.0 as i32, dest.1 as i32);
                    let got = port.accounts(&[pk], 0).await?;
                    if present(&got[0], &t.addrs.program) {
                        // Open but not read yet: the next province read plans it.
                        continue;
                    }
                    ix::region_of(dest.0 as i32, dest.1 as i32)
                }
            };
            let Some(an) = self.anchor(t, port, beacon, d.arrive, region).await? else {
                continue;
            };
            if t.now < t.clock.settle_after(d.arrive, an.a) {
                continue;
            }
            let (dp, dq) = (dest.0 as i32, dest.1 as i32);
            let mut keys: Vec<Address> = (0..4u8)
                .map(|i| t.addrs.arrival_slot(dp, dq, d.arrive, st.faction, i))
                .collect();
            keys.push(t.addrs.clash_inputs(dp, dq, d.arrive));
            keys.push(t.addrs.anchor(d.arrive, region));
            let got = port.accounts(&keys, 0).await?;
            let mut slot_i = 0u8;
            let mut slot_ben = t.cfg.beneficiary;
            for (i, a) in got[..4].iter().enumerate() {
                if let Some(sl) = a
                    .as_ref()
                    .filter(|a| a.owner == t.addrs.program)
                    .and_then(|a| ArrivalSlot::decode(&a.data).ok())
                {
                    if sl.host_id == d.host {
                        slot_i = i as u8;
                        slot_ben = sl.beneficiary;
                    }
                }
            }
            let inputs = got[4]
                .as_ref()
                .filter(|a| a.owner == t.addrs.program)
                .and_then(|a| ClashInputs::decode(&a.data).ok())
                .filter(|c| c.resolved());
            let resolver = inputs.as_ref().map_or(t.cfg.beneficiary, |c| c.resolver);
            // v1.7 (I-56): the camp's winner lists its owner's Citizen.
            let camp_citizen = inputs.as_ref().and_then(|c| {
                (0..4u8).find_map(|i| {
                    let r = c.arrivals[st.faction as usize * 4 + i as usize];
                    (r.present == 1
                        && r.host_id == d.host
                        && ix::camp_winner(c.camp_mask, st.faction, i, r.fate))
                    .then_some(st.owner_citizen)
                })
            });
            let anchor_present = present(&got[5], &t.addrs.program);
            let Ok((hp, hq, hs, _, _)) = fclient::addr::host_parts(d.host) else {
                continue;
            };
            let args = SettleTransitArgs {
                holding: HoldingRef {
                    p: hp as i16,
                    q: hq as i16,
                    site: hs,
                },
                transit_slot: st.transit_slot.unwrap_or(0),
                commit: d.commit,
                seal: d.seal,
                beneficiary: t.cfg.beneficiary,
                dest: (dp, dq),
                arrive: d.arrive,
                faction: st.faction,
                slot_i,
                home: (hp, hq),
                anchor_present,
                slot_beneficiary: slot_ben,
                resolver,
                holding_rent_payer: st.rent_payer,
                camp_citizen,
            };
            let a = t.addrs.clone();
            engine.ensure(
                WriteSpec {
                    key,
                    kind: "settle-transit",
                    tag: tag::SETTLE_TRANSIT,
                    class: Class::D,
                    bell: Some(d.arrive),
                    region: Some(region),
                    build: Arc::new(move |c: &BuildCtx| {
                        vec![ix::settle_transit(&a, c.payer, &args)]
                    }),
                    deadline_slot: None,
                    not_before_slot: t.slot + self.jitter(t),
                    fixed_payer: None,
                },
                t.slot,
            );
        }
        Ok(())
    }

    // ------------------------------------------------------------ closes

    async fn closes<P: ChainPort>(
        &mut self,
        t: &Tick<'_>,
        port: &P,
        engine: &mut Engine,
        beacon: &BTreeMap<(u32, u8), AnchorInfo>,
    ) -> PortResult<()> {
        if !t.slot.is_multiple_of(4) {
            return Ok(());
        }
        // Slots.
        let slots: Vec<(i16, i16, u32, u8, u8)> = self.index.slots.keys().copied().collect();
        for sk in slots {
            let key = format!("close:slot:{},{}:{}:{}:{}", sk.0, sk.1, sk.2, sk.3, sk.4);
            if self.closed_sent.contains(&key) || engine.is_pending(&key) {
                continue;
            }
            let region = ix::region_of(sk.0 as i32, sk.1 as i32);
            let Some(an) = self.anchor(t, port, beacon, sk.2, region).await? else {
                continue;
            };
            let grace_end = t.clock.reveal_close(sk.2, an.a) + CLAIM_GRACE_BELLS * BELL_SECS;
            let addr = t
                .addrs
                .arrival_slot(sk.0 as i32, sk.1 as i32, sk.2, sk.3, sk.4);
            let got = port.accounts(&[addr], 0).await?;
            let Some(sl) = got[0]
                .as_ref()
                .filter(|a| a.owner == t.addrs.program)
                .and_then(|a| ArrivalSlot::decode(&a.data).ok())
            else {
                self.closed_sent.insert(key);
                continue;
            };
            if sl.flags & 1 == 0 || (sl.claimed == 0 && t.now < grace_end) {
                continue;
            }
            let a = t.addrs.clone();
            let rent_to = sl.rent_to;
            engine.ensure(
                WriteSpec {
                    key,
                    kind: "close",
                    tag: tag::CLOSE_ARRIVAL_SLOT,
                    class: Class::N,
                    bell: Some(sk.2),
                    region: Some(region),
                    build: Arc::new(move |c: &BuildCtx| {
                        vec![ix::close_arrival_slot(
                            &a, c.payer, sk.0, sk.1, sk.2, sk.3, sk.4, rent_to, true,
                        )]
                    }),
                    deadline_slot: None,
                    not_before_slot: t.slot + self.jitter(t),
                    fixed_payer: None,
                },
                t.slot,
            );
        }
        // ArrivalDays.
        let days: Vec<(i16, i16, u32)> = self.index.days.iter().copied().collect();
        for dk in days {
            let key = format!("close:day:{},{}:{}", dk.0, dk.1, dk.2);
            if self.closed_sent.contains(&key) || engine.is_pending(&key) {
                continue;
            }
            let Some(ps) = self.provinces.get(&(dk.0, dk.1)) else {
                continue;
            };
            if ps.rn < abi::BELLS_PER_DAY * (dk.2 + 1) {
                continue;
            }
            let got = port
                .accounts(&[t.addrs.arrival_day(dk.0 as i32, dk.1 as i32, dk.2)], 0)
                .await?;
            let Some(ad) = got[0]
                .as_ref()
                .filter(|a| a.owner == t.addrs.program)
                .and_then(|a| ArrivalDay::decode(&a.data).ok())
            else {
                self.closed_sent.insert(key);
                continue;
            };
            let a = t.addrs.clone();
            let rent_to = ad.rent_to;
            engine.ensure(
                WriteSpec {
                    key,
                    kind: "close",
                    tag: tag::CLOSE_ARRIVAL_DAY,
                    class: Class::N,
                    bell: None,
                    region: None,
                    build: Arc::new(move |c: &BuildCtx| {
                        vec![ix::close_arrival_day(
                            &a, c.payer, dk.0, dk.1, dk.2, rent_to,
                        )]
                    }),
                    deadline_slot: None,
                    not_before_slot: t.slot + self.jitter(t),
                    fixed_payer: None,
                },
                t.slot,
            );
        }
        // ClashInputs.
        let grace = t.season.clash_close_grace as i64 * BELL_SECS;
        let inputs: Vec<(i16, i16, u32)> = self.index.inputs.iter().copied().collect();
        for ik in inputs {
            let key = format!("close:inputs:{},{}:{}", ik.0, ik.1, ik.2);
            if self.closed_sent.contains(&key) || engine.is_pending(&key) {
                continue;
            }
            let got = port
                .accounts(&[t.addrs.clash_inputs(ik.0 as i32, ik.1 as i32, ik.2)], 0)
                .await?;
            let Some(ci) = got[0]
                .as_ref()
                .filter(|a| a.owner == t.addrs.program)
                .and_then(|a| ClashInputs::decode(&a.data).ok())
            else {
                self.closed_sent.insert(key);
                continue;
            };
            let all =
                (0..24).all(|k| ci.arrivals[k].present != 1 || ci.settled_mask & (1 << k) != 0);
            if !ci.resolved() || !all || ci.resolved_ts as i64 + grace > t.now - t.clock.genesis_ts
            {
                continue;
            }
            let a = t.addrs.clone();
            let rent_to = ci.rent_to;
            engine.ensure(
                WriteSpec {
                    key,
                    kind: "close",
                    tag: tag::CLOSE_CLASH_INPUTS,
                    class: Class::N,
                    bell: Some(ik.2),
                    region: None,
                    build: Arc::new(move |c: &BuildCtx| {
                        vec![ix::close_clash_inputs(
                            &a, c.payer, ik.0, ik.1, ik.2, rent_to,
                        )]
                    }),
                    deadline_slot: None,
                    not_before_slot: t.slot + self.jitter(t),
                    fixed_payer: None,
                },
                t.slot,
            );
        }
        Ok(())
    }

    // ------------------------------------------------------------ claims

    async fn claims<P: ChainPort>(
        &mut self,
        t: &Tick<'_>,
        port: &P,
        engine: &mut Engine,
        beacon: &BTreeMap<(u32, u8), AnchorInfo>,
    ) -> PortResult<()> {
        let Some(keeper) = t.cfg.claim_key else {
            return Ok(());
        };
        for (p, q, b, f, i) in std::mem::take(&mut self.unclaim) {
            self.claimed.remove(&t.addrs.arrival_slot(p, q, b, f, i));
        }
        if t.cfg.beneficiary != keeper || !t.slot.is_multiple_of(4) {
            return Ok(());
        }
        let mine: Vec<(i16, i16, u32, u8, u8)> = self
            .index
            .slots
            .iter()
            .filter(|(_, r)| r.beneficiary == keeper)
            .map(|(k, _)| *k)
            .collect();
        let mut by_day: BTreeMap<u32, Vec<ix::ClaimSlot>> = BTreeMap::new();
        for sk in mine {
            let addr = t
                .addrs
                .arrival_slot(sk.0 as i32, sk.1 as i32, sk.2, sk.3, sk.4);
            if self.claimed.contains(&addr) {
                continue;
            }
            let region = ix::region_of(sk.0 as i32, sk.1 as i32);
            let Some(an) = self.anchor(t, port, beacon, sk.2, region).await? else {
                continue;
            };
            let Some(an_slot) = an.slot else { continue };
            let grace_end = t.clock.reveal_close(sk.2, an.a) + CLAIM_GRACE_BELLS * BELL_SECS;
            if t.now >= grace_end {
                continue;
            }
            let got = port.accounts(&[addr], 0).await?;
            let Some(sl) = got[0]
                .as_ref()
                .filter(|a| a.owner == t.addrs.program)
                .and_then(|a| ArrivalSlot::decode(&a.data).ok())
            else {
                continue;
            };
            // Late, unclaimed, a refund, inside the grace: the rule the
            // stack's `defence-pool` hold aims at too (W6-C).
            if sl.beneficiary != keeper
                || fclient::play::open_claim(&sl, an_slot, an.a, t.season, t.now).is_none()
            {
                continue;
            }
            let day = t.clock.bell_at(t.now).unwrap_or(0) / abi::BELLS_PER_DAY;
            by_day.entry(day).or_default().push(ix::ClaimSlot {
                p: sk.0 as i32,
                q: sk.1 as i32,
                bell: sk.2,
                faction: sk.3,
                i: sk.4,
            });
        }
        for (day, v) in by_day {
            for chunk in v.chunks(6) {
                let chunk = chunk.to_vec();
                let key = format!(
                    "claim:{day}:{}",
                    chunk
                        .iter()
                        .map(|c| format!("{},{},{},{},{}", c.p, c.q, c.bell, c.faction, c.i))
                        .collect::<Vec<_>>()
                        .join(";")
                );
                let a = t.addrs.clone();
                for c in &chunk {
                    self.claimed
                        .insert(t.addrs.arrival_slot(c.p, c.q, c.bell, c.faction, c.i));
                }
                engine.ensure(
                    WriteSpec {
                        key,
                        kind: "claim",
                        tag: tag::CLAIM_DEFENCE,
                        class: Class::D,
                        bell: None,
                        region: None,
                        build: Arc::new(move |_: &BuildCtx| {
                            vec![ix::claim_defence(&a, keeper, day, &chunk)]
                        }),
                        deadline_slot: None,
                        not_before_slot: t.slot + self.jitter(t),
                        fixed_payer: Some(keeper),
                    },
                    t.slot,
                );
            }
        }
        Ok(())
    }

    /// What holds each lagging province (`/v1/nudge`'s `blocking`): its
    /// lag, play writes in flight on it, departures from it waiting for
    /// its resolve.
    pub fn blocking(&self, t: &Tick<'_>, engine: &Engine) -> BTreeMap<String, Vec<Value>> {
        let now_bell = t.clock.bell_at(t.now).unwrap_or(0);
        let pending = engine.pending_keys();
        let mut out = BTreeMap::new();
        for (pq, s) in &self.provinces {
            if s.rn + 1 >= now_bell {
                continue;
            }
            let pk = format!("{},{}", pq.0, pq.1);
            let mut v = vec![
                json!({"kind": "lag", "key": format!("resolved_next {} < bell {}", s.rn, now_bell.saturating_sub(1))}),
            ];
            for k in pending
                .iter()
                .filter(|k| k.contains(&format!(":{pk}:")) || k.ends_with(&format!(":{pk}")))
            {
                v.push(json!({"kind": "write", "key": k}));
            }
            for (k, d) in &self.index.departs {
                if d.origin == *pq && !self.index.departure_settled.contains(k) {
                    v.push(json!({"kind": "departure", "key": rkey(*k)}));
                }
            }
            out.insert(pk, v);
        }
        out
    }

    /// Status JSON of the play duties.
    pub fn status(&self) -> Value {
        let s = &self.stats;
        json!({
            "transits": self.index.departs.len(),
            "settled": self.index.settled.len(),
            "opened": s.opened, "bad_seals": s.bad_seals,
            "reveals_sent": s.reveals_sent, "reveals_landed": s.reveals_landed,
            "reveals_refused": s.reveals_refused, "slot_moved": s.slot_moved,
            "reveals_missed": s.reveals_missed,
            "departures_settled": s.departures_settled, "gathers": s.gathers,
            "resolves": s.resolves, "skips": s.skips, "skipped_bells": s.skipped_bells,
            "not_quiet": s.not_quiet, "settles": s.settles, "closes": s.closes,
            "claims": s.claims, "returns": s.returns,
            "provinces_tracked": self.provinces.len(),
            "findings": self.findings.len(),
        })
    }
}

fn parse_pq(s: &str) -> Option<(i16, i16)> {
    let (p, q) = s.split_once(',')?;
    Some((p.parse().ok()?, q.parse().ok()?))
}

#[cfg(test)]
mod review_tests {
    use super::*;
    use fclient::abi::layout as l;

    fn holding(state: u8, gen: u8, transit: Option<(u64, u32, u8)>) -> Account {
        let mut d = vec![0u8; fclient::abi::size::HOLDING];
        d[..8].copy_from_slice(fclient::abi::magic::HOLDING);
        d[l::holding::STATE] = state;
        d[l::holding::GEN] = gen;
        if let Some((host, arrive, st)) = transit {
            let o = l::holding::TRANSIT;
            d[o + l::transit::STATE] = st;
            d[o + l::transit::HOST_ID..o + l::transit::HOST_ID + 8]
                .copy_from_slice(&host.to_le_bytes());
            d[o + l::transit::ARRIVE_BELL..o + l::transit::ARRIVE_BELL + 4]
                .copy_from_slice(&arrive.to_le_bytes());
        }
        Account {
            lamports: 1,
            data: d,
            owner: Address::new_from_array([7; 32]),
            executable: false,
        }
    }

    /// The gather waits only where the program answers
    /// `DepartureUnsettled` (wave-4 review, W4-C major): a freed or
    /// already-settled transit, a re-founded or released Holding, or none
    /// at all, gather as not present.
    #[test]
    fn gather_waits_only_on_a_departed_transit() {
        let program = Address::new_from_array([7; 32]);
        let host = fclient::addr::host_id(3, 0, 1, 2, 5).unwrap();
        let b = 40;
        let w = |a: Option<&Account>| departure_unsettled(a, &program, host, b);
        assert!(
            w(Some(&holding(2, 2, Some((host, b, 1))))),
            "departed: wait"
        );
        assert!(
            !w(Some(&holding(2, 2, Some((host, b, 2))))),
            "settled at origin"
        );
        assert!(!w(Some(&holding(2, 2, None))), "freed (settled elsewhere)");
        assert!(!w(Some(&holding(2, 3, Some((host, b, 1))))), "re-founded");
        assert!(!w(Some(&holding(3, 2, Some((host, b, 1))))), "released");
        assert!(
            !w(Some(&holding(2, 2, Some((host, b + 1, 1))))),
            "another bell"
        );
        assert!(!w(None), "absent");
    }

    fn prov(rn: u32) -> ProvState {
        ProvState {
            region: 0,
            rn,
            read_slot: 1,
            pending_bells: BTreeSet::new(),
            active: true,
            leaves: vec![],
            last_digest: [0; 32],
            contested: false,
        }
    }

    fn depart(host: u64, origin: (i16, i16), depart_bell: u32, arrive: u32) -> DepartRec {
        DepartRec {
            host,
            origin,
            origin_tile: 0,
            depart_bell,
            arrive,
            dep_mass: 1,
            tip: 0,
            seal_root: [0; 32],
            commit: [0; 32],
            seal: [0; 165],
            slot: 1,
        }
    }

    /// W6-C (W5-B F4): a province with pending work is skipped in one
    /// transaction once the run of closed quiet bells covers the bell the
    /// work waits for, not bell by bell as each closes; an idle one waits
    /// for a whole batch; the season's end flushes.
    #[test]
    fn skips_wait_for_the_target_bell() {
        let mut d = PlayDuty::default();
        let pq = (1, 0);
        let end = 1_000;
        // Idle: only a whole batch (or the end) goes.
        let s = prov(100);
        assert_eq!(d.skip_target(pq, &s), None);
        assert!(!d.skip_now(end, pq, &s, 100, 23));
        assert!(d.skip_now(end, pq, &s, 100, 24));
        assert!(d.skip_now(110, pq, &s, 100, 10), "reaches the end");
        // A resident change pending at bell 110: nothing before it closes,
        // then one skip through it (before W6-C: a skip per bell).
        let mut s = prov(100);
        s.pending_bells.insert(110);
        assert_eq!(d.skip_target(pq, &s), Some(110));
        assert!(!d.skip_now(end, pq, &s, 100, 1));
        assert!(!d.skip_now(end, pq, &s, 100, 10));
        assert!(d.skip_now(end, pq, &s, 100, 11));
        // A Leave entry's bell likewise.
        let mut s = prov(100);
        s.leaves.push((3, 9, 104));
        assert_eq!(d.skip_target(pq, &s), Some(104));
        // A departure from here to settle: its departure bell; once
        // resolved past it (settlement can go), nothing.
        d.index.departs.insert((7, 105), depart(7, pq, 105, 112));
        // Revealed at (5, 5) (a march never revealed is judged at its
        // origin, below).
        d.index.slots.insert(
            (5, 5, 112, 0, 0),
            crate::playindex::RevealRec {
                dest: (5, 5),
                arrive: 112,
                faction: 0,
                i: 0,
                host: 7,
                displaced: None,
                beneficiary: Address::new_from_array([0; 32]),
                slot: 1,
                created_day: false,
            },
        );
        assert_eq!(d.skip_target((5, 5), &prov(100)), Some(111));
        let s = prov(100);
        assert_eq!(d.skip_target(pq, &s), Some(105));
        assert_eq!(d.skip_target(pq, &prov(106)), None);
        d.index.departure_settled.insert((7, 105));
        assert_eq!(d.skip_target(pq, &s), None);
        // An arrival here ahead: the bell before it, so the arrival bell is
        // next when its window closes; with the province already at the
        // arrival bell (left clear: nothing revealed), that bell.
        let dest = (2, 0);
        d.index.departs.insert((8, 105), depart(8, dest, 105, 120));
        d.index.departure_settled.insert((8, 105));
        d.transits.insert((8, 105), TransitState::default());
        // The destination is unknown to this keeper: judged at the origin.
        assert_eq!(d.skip_target(dest, &prov(100)), Some(119));
        assert_eq!(d.skip_target(dest, &prov(120)), Some(120));
        assert_eq!(d.skip_target(dest, &prov(121)), None);
        // A nudge: at once.
        d.nudged.insert(pq, 130);
        assert_eq!(d.skip_target(pq, &prov(100)), Some(100));
        assert!(d.skip_now(end, pq, &prov(100), 100, 1));
    }

    #[test]
    fn reveal_refusals_are_final_but_three() {
        for c in [
            abi::err::SLOT_MOVED,
            abi::err::NO_ANCHOR,
            abi::err::TOO_EARLY,
        ] {
            assert!(reveal_transient(c));
        }
        // Path 32, ArrivalBell 31, Shielded 37 (§5.4).
        for c in [
            31u32,
            32,
            37,
            abi::err::WINDOW_CLOSED,
            abi::err::LATCH_CLOSED,
        ] {
            assert!(!reveal_transient(c), "{c}");
        }
    }
}

/// W6T-2 (w6-s7 triage, U2): the play duties' reads, the end of the
/// season, dead closes, the backup delay and the skip batches. Each test
/// runs the duty through [`PlayDuty::plan`] over a fake chain.
#[cfg(test)]
mod w6t_tests {
    use super::*;
    use crate::config::KeeperConfig;
    use crate::engine::{EngineParams, StepReport};
    use crate::pools::Payers;
    use crate::testkit::{self, FakeDrand, FakePort};
    use base64::Engine as _;
    use fclient::addr::Addresses;
    use fclient::budgets::Budgets;
    use fclient::clock::SeasonClock;
    use fclient::decode::Season;

    const G: i64 = 1_800_000_000;

    fn program() -> Address {
        Address::new_from_array([0x5F; 32])
    }

    struct Env {
        port: FakePort,
        drand: FakeDrand,
        season: Season,
        addrs: Addresses,
        cfg: KeeperConfig,
        engine: Engine,
        rounds: Rounds,
        seeds: SeedFinder,
        shared: Shared,
        beacon: BTreeMap<(u32, u8), AnchorInfo>,
    }

    fn env(roles: &[&str], end_bell: u32) -> Env {
        let drand = FakeDrand::new();
        let season = testkit::season(G, end_bell, &drand.key.info());
        let mut cfg = KeeperConfig::new(program(), 7, Address::new_from_array([0xBE; 32]));
        cfg.roles = roles.iter().map(|r| r.to_string()).collect();
        let budgets =
            Budgets::from_json(&serde_json::from_str(crate::CANONICAL_BUDGETS).unwrap()).unwrap();
        Env {
            port: FakePort::default(),
            rounds: Rounds::new(drand.key.pk96),
            drand,
            season,
            addrs: Addresses::new(program(), 7),
            cfg,
            engine: Engine::new(budgets, EngineParams::default()),
            seeds: SeedFinder::default(),
            shared: Shared::default(),
            beacon: BTreeMap::new(),
        }
    }

    impl Env {
        async fn plan(
            &mut self,
            d: &mut PlayDuty,
            slot: u64,
            now: i64,
            journal: Option<&Journal>,
            opened: &[(i32, i32)],
        ) {
            self.port.set_clock(slot, now);
            let t = Tick {
                slot,
                now,
                season: &self.season,
                clock: SeasonClock::from_season(&self.season),
                addrs: &self.addrs,
                cfg: &self.cfg,
            };
            d.plan(
                &t,
                &self.port,
                &self.drand,
                &mut self.rounds,
                &mut self.engine,
                &self.beacon,
                &mut self.seeds,
                &mut self.shared,
                journal,
                opened,
            )
            .await
            .unwrap();
        }
        fn anchor(&mut self, bell: u32, region: u8) {
            self.beacon.insert(
                (bell, region),
                AnchorInfo {
                    a: G + (bell as i64 + 1) * BELL_SECS + 5,
                    slot: 1,
                    round: 1,
                    rent_to: Address::new_from_array([1; 32]),
                    cache: None,
                    archived: false,
                },
            );
        }
    }

    fn depart(host: u64, origin: (i16, i16), tile: u8, dep: u32, arrive: u32) -> DepartRec {
        DepartRec {
            host,
            origin,
            origin_tile: tile,
            depart_bell: dep,
            arrive,
            dep_mass: 10,
            tip: 0,
            seal_root: [0; 32],
            commit: [0; 32],
            seal: [0; 165],
            slot: 1,
        }
    }

    /// A march that stays in its origin tile (no path step): the plaintext,
    /// salt and ct_hash, and the seal root they open.
    fn march(host: u64, arrive: u32, dest: (i16, i16), tile: u8) -> ([u8; 37], [u8; 32], [u8; 32]) {
        let p = seal::Plain {
            version: 1,
            host_id: host,
            arrive_bell: arrive,
            dest_p: dest.0,
            dest_q: dest.1,
            dest_tile: tile,
            stance: 1,
            ..Default::default()
        };
        (seal::pack(&p), [host as u8; 32], [7; 32])
    }

    fn owner_item(holding: Address, m: &([u8; 37], [u8; 32], [u8; 32])) -> Value {
        let b = base64::engine::general_purpose::STANDARD;
        json!({
            "holding": holding.to_string(), "transit_slot": 0,
            "plain_b64": b.encode(m.0), "salt_b64": b.encode(m.1), "ct_hash_b64": b.encode(m.2),
        })
    }

    /// Cause A (w6-s7 criterion 3): every 4th slot the closes duty read
    /// each open ClashInputs (and each ArrivalSlot) with its own RPC; with
    /// a 1,008-bell close grace none closes in the season, the list only
    /// grows (≈ 15,000 reads, 1.4 s a closes tick at the end of w6-s7).
    /// Now: one getMultipleAccounts per 100, and a key inside its grace is
    /// not read again before the grace ends.
    #[tokio::test]
    async fn closes_do_not_reread_inputs_inside_grace() {
        let mut e = env(&["close"], 1_008);
        let mut d = PlayDuty::default();
        let bell = 100u32;
        let mut n_inputs = 0;
        'outer: for p in -40i16..40 {
            for q in -40i16..40 {
                if n_inputs == 5_000 {
                    break 'outer;
                }
                d.index.inputs.insert((p, q, bell));
                e.port.put(
                    e.addrs.clash_inputs(p as i32, q as i32, bell),
                    testkit::acct(program(), testkit::clash_inputs(60_600)),
                );
                n_inputs += 1;
            }
        }
        // 1,000 ArrivalSlots settled, inside their claim grace.
        for i in 0..1_000u32 {
            let (p, q) = ((i % 40) as i16 - 20, (i / 40) as i16 - 20);
            let sk = (p, q, bell, 0u8, 0u8);
            let host = 1_000 + i as u64;
            d.index.slots.insert(
                sk,
                crate::playindex::RevealRec {
                    dest: (p, q),
                    arrive: bell,
                    faction: 0,
                    i: 0,
                    host,
                    displaced: None,
                    beneficiary: Address::new_from_array([0; 32]),
                    slot: 1,
                    created_day: false,
                },
            );
            e.anchor(bell, ix::region_of(p as i32, q as i32));
            e.port.put(
                e.addrs.arrival_slot(p as i32, q as i32, bell, 0, 0),
                testkit::acct(program(), testkit::arrival_slot(host, true)),
            );
        }
        let now0 = G + 102 * BELL_SECS;
        let mut per_pass = vec![];
        for k in 1..=10u64 {
            e.port.take_calls();
            e.plan(&mut d, 4 * k, now0 + 32 * k as i64, None, &[]).await;
            let calls = e.port.take_calls();
            assert!(
                calls.iter().all(|c| c.len() <= 100),
                "pass {k}: a read of more than 100 keys"
            );
            per_pass.push(calls.len());
        }
        assert_eq!(e.engine.pending_len(), 0, "nothing is closable yet");
        assert!(
            per_pass[0] <= 60,
            "the first pass reads in batches of 100: {per_pass:?}"
        );
        assert!(
            per_pass[1..].iter().all(|&n| n <= 2),
            "inside the grace nothing is read again: {per_pass:?}"
        );
    }

    /// End of season (w6-s7 failure 1, U2): an arrival at or after
    /// `end_bell` can never be anchored, revealed or settled (the program's
    /// Reveal refuses `WrongStatus` once Ended; 27 failed Reveals in w6-s7).
    /// The keeper does not send them; an arrival before the end still goes.
    #[tokio::test]
    async fn reveals_skip_arrivals_at_or_after_end_bell() {
        let end = 300u32;
        let mut e = env(&["reveal"], end);
        let mut d = PlayDuty::default();
        let origin = (2i16, 0i16);
        let mut keys = vec![];
        for (site, arrive) in [(1u8, end - 1), (2, end), (3, end + 2)] {
            let host = fclient::addr::host_id(2, 0, site, 1, 5).unwrap();
            let m = march(host, arrive, origin, 5);
            let root = seal::seal_root(&seal::commit(&m.0, &m.1), &m.2);
            let dep = arrive - 3;
            d.index
                .departs
                .insert((host, dep), depart(host, origin, 5, dep, arrive));
            let h = e.addrs.holding(2, 0, site);
            e.port.put(
                h,
                testkit::acct(
                    program(),
                    testkit::holding(1, 0, (1, host, dep, arrive, root)),
                ),
            );
            e.shared
                .reveals
                .push((format!("r{site}"), owner_item(h, &m)));
            keys.push((arrive, format!("reveal:{host}:{dep}")));
        }
        let now = G + (end as i64 - 1) * BELL_SECS + 30;
        e.plan(&mut d, 50, now, None, &[]).await;
        for (arrive, k) in keys {
            assert_eq!(
                e.engine.is_pending(&k),
                arrive < end,
                "arrive {arrive} (end_bell {end}): {k}"
            );
        }
    }

    /// Close waste (w6-s7: 28,655 failed CloseArrivalDay/Slot in the
    /// drain, ≈ 270 per key): a close that ended Dead was planned again at
    /// the next closes tick. Now a Dead close key waits until its account
    /// changes.
    #[tokio::test]
    async fn dead_close_key_not_replanned() {
        let mut e = env(&["close"], 1_008);
        let mut d = PlayDuty::default();
        let pq = (3i16, 0i16);
        e.port.put(
            e.addrs.province(3, 0),
            testkit::acct(program(), testkit::province(300, &[])),
        );
        d.index.days.insert((3, 0, 1));
        let day_addr = e.addrs.arrival_day(3, 0, 1);
        e.port
            .put(day_addr, testkit::acct(program(), testkit::arrival_day()));
        let key = "close:day:3,0:1".to_string();
        let now0 = G + 400 * BELL_SECS;
        e.plan(&mut d, 4, now0, None, &[(3, 0)]).await;
        assert!(e.engine.is_pending(&key), "planned once");
        assert!(d.provinces.contains_key(&pq));
        // Every rung failed: the engine ends it Dead.
        e.engine.cancel(&key);
        d.on_outcome(
            &key,
            &Outcome::Dead {
                reason: "fails with a heap frame".into(),
                code: None,
            },
        );
        // Unchanged account: never planned again (a bell and more of passes).
        for k in 2..=40u64 {
            e.plan(&mut d, 4 * k, now0 + 32 * k as i64, None, &[(3, 0)])
                .await;
            assert!(!e.engine.is_pending(&key), "re-planned at pass {k}");
        }
        // The account changed (lamports): planned again.
        let mut a = e.port.accounts.lock().unwrap()[&day_addr].clone();
        a.lamports += 1;
        e.port.put(day_addr, a);
        let mut again = false;
        for k in 41..=80u64 {
            e.plan(&mut d, 4 * k, now0 + 32 * k as i64, None, &[(3, 0)])
                .await;
            again |= e.engine.is_pending(&key);
        }
        assert!(again, "a changed account is planned again");
    }

    /// Redundancy (w6-s7: 1,716 SettleDeparture AlreadyDone and 1,504
    /// SettleTransit TransitState between keepers A and B): with
    /// `backup_delay_slots` a settle waits that many slots after it became
    /// eligible, then re-reads the transit before it is planned.
    #[tokio::test]
    async fn backup_delay_rereads_before_settle() {
        let mut e = env(&["settle-departure"], 1_008);
        e.cfg.backup_delay_slots = 9;
        let mut d = PlayDuty::default();
        e.port.put(
            e.addrs.province(2, 0),
            testkit::acct(program(), testkit::province(101, &[])),
        );
        let mut hs = vec![];
        for site in [1u8, 2] {
            let host = fclient::addr::host_id(2, 0, site, 1, 5).unwrap();
            d.index
                .departs
                .insert((host, 100), depart(host, (2, 0), 5, 100, 104));
            let h = e.addrs.holding(2, 0, site);
            e.port.put(
                h,
                testkit::acct(
                    program(),
                    testkit::holding(1, 0, (1, host, 100, 104, [0; 32])),
                ),
            );
            hs.push((host, h));
        }
        let key = |host: u64| format!("sdep:{host}:100:2,0");
        let now0 = G + 101 * BELL_SECS + 10;
        for s in 100..109u64 {
            e.plan(&mut d, s, now0 + 8 * (s as i64 - 100), None, &[(2, 0)])
                .await;
            for (host, _) in &hs {
                assert!(!e.engine.is_pending(&key(*host)), "slot {s}: sent early");
            }
        }
        // The other keeper settles host 2 just before our delay ends: the
        // re-read sees it and nothing is sent for it.
        let (h2, a2) = hs[1];
        e.port.put(
            a2,
            testkit::acct(
                program(),
                testkit::holding(1, 0, (2, h2, 100, 104, [0; 32])),
            ),
        );
        e.port.take_calls();
        e.plan(&mut d, 109, now0 + 72, None, &[(2, 0)]).await;
        assert!(e.port.reads_of(&hs[0].1) >= 1, "re-read before planning");
        assert!(
            e.engine.is_pending(&key(hs[0].0)),
            "planned after the delay"
        );
        assert!(
            !e.engine.is_pending(&key(h2)),
            "settled by the other keeper meanwhile"
        );
    }

    /// Keeper A had its reveal material twice (the seal it opened and the
    /// owner's submission) and a tick that ran late sent the next version
    /// into the slot its first version went out in: two Reveals of one
    /// march in one slot, one `AlreadyDone` (203 in w6-s7). One write per
    /// `(host, arrive)`, one version per chain slot.
    #[tokio::test]
    async fn reveal_sources_deduped_by_host_arrive() {
        let mut e = env(&["reveal"], 1_008);
        let mut d = PlayDuty::default();
        let j = Journal::open(std::path::Path::new(":memory:")).unwrap();
        let host = fclient::addr::host_id(2, 0, 1, 1, 5).unwrap();
        let (arrive, dep) = (200u32, 197u32);
        let m = march(host, arrive, (2, 0), 5);
        let commit = seal::commit(&m.0, &m.1);
        let root = seal::seal_root(&commit, &m.2);
        let mut dr = depart(host, (2, 0), 5, dep, arrive);
        dr.commit = commit;
        d.index.departs.insert((host, dep), dr);
        let h = e.addrs.holding(2, 0, 1);
        e.port.put(
            h,
            testkit::acct(
                program(),
                testkit::holding(1, 0, (1, host, dep, arrive, root)),
            ),
        );
        // Both sources: the journalled opened seal and the owner material.
        j.save_plaintext(host, arrive, &m.0, &m.1).unwrap();
        e.shared.reveals.push(("r1".into(), owner_item(h, &m)));
        let mut c = e.cfg.clone();
        c.delay_floor = 1;
        c.reveal_floor = Some(1);
        let mut payers = Payers::new(&[9u8; 32], &c).unwrap();
        for a in payers.all_addresses() {
            e.port.put(
                a,
                Account {
                    lamports: 10_000_000_000,
                    data: vec![],
                    owner: fclient::addr::system_program(),
                    executable: false,
                },
            );
        }
        payers.refresh(&e.port, 0).await.unwrap();
        let now0 = G + arrive as i64 * BELL_SECS + 30;
        let mut by_chain_slot: BTreeMap<u64, usize> = BTreeMap::new();
        for s in 50..54u64 {
            // Every tick runs late: its sends go out in the next slot, and
            // the next tick starts in that slot.
            e.plan(&mut d, s, now0 + 8 * (s as i64 - 50), Some(&j), &[])
                .await;
            e.port
                .send_slot
                .store(s + 1, std::sync::atomic::Ordering::SeqCst);
            let before = e.port.sent.lock().unwrap().len();
            let mut r = StepReport::default();
            e.engine.send(&e.port, &mut payers, None, s, &mut r).await;
            let n = e.port.sent.lock().unwrap().len() - before;
            *by_chain_slot.entry(s + 1).or_default() += n;
            // The late tick of slot s + 1 reads the same chain slot.
            if s % 2 == 0 {
                e.port.set_clock(s + 1, now0 + 8 * (s as i64 - 49));
                let before = e.port.sent.lock().unwrap().len();
                let mut r = StepReport::default();
                e.engine
                    .send(&e.port, &mut payers, None, s + 1, &mut r)
                    .await;
                let n = e.port.sent.lock().unwrap().len() - before;
                *by_chain_slot.entry(s + 1).or_default() += n;
            }
        }
        assert_eq!(
            e.engine.pending_keys(),
            vec![format!("reveal:{host}:{dep}")],
            "one write for the march"
        );
        assert!(
            by_chain_slot.values().all(|&n| n <= 1),
            "at most one Reveal of the march per slot: {by_chain_slot:?}"
        );
    }

    /// Idle days (w6-s7 criterion 3: 23 days with no arrival at 7-9
    /// SkipQuiet): a pending resident change split the day's batches at its
    /// bell. It rides a whole 24-bell batch (SkipQuiet applies it at its
    /// bell); only a nudge, an arrival or a departure to settle split.
    #[test]
    fn idle_day_with_pending_ops_is_one_batch_per_24_bells() {
        let d = PlayDuty::default();
        let pq = (1, 0);
        let end = 1_008;
        let day0 = 144 * 3;
        let mut s = ProvState {
            region: 0,
            rn: day0,
            read_slot: 1,
            pending_bells: [day0 + 5, day0 + 30, day0 + 31, day0 + 77, day0 + 100]
                .into_iter()
                .collect(),
            active: true,
            leaves: vec![],
            last_digest: [0; 32],
            contested: false,
        };
        let mut batches = vec![];
        // Bells close one by one; a skip goes out when the scheduler says
        // (into the next day, so the day's last batch is complete).
        for closed_through in day0..day0 + 144 + 48 {
            loop {
                let n = (closed_through + 1 - s.rn).min(SKIP_MAX);
                if n == 0 || !d.skip_now(end, pq, &s, s.rn, n) {
                    break;
                }
                batches.push((s.rn, n));
                s.rn += n;
                s.pending_bells.retain(|&b| b >= s.rn);
            }
        }
        let day: Vec<_> = batches.iter().filter(|b| b.0 < day0 + 144).collect();
        assert_eq!(day.len(), 6, "one SkipQuiet per 24 bells: {day:?}");
    }
}
