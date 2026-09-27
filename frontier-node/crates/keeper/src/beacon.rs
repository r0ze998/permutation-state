//! Beacon duties (M1 contract §8.2; offchain design §6.4): anchors (the
//! combined form **and** per-region fallbacks), seed caches with the nonce
//! switch, and the per-bell BeaconLog posts.
//!
//! - **Anchor** of bell `b`: once `T(b)` is published, every missing region
//!   of the keeper's set gets a `PostAnchorMulti` (regions in fixed chunks
//!   of `MULTI_MAX_REGIONS` = 7, the program skipping present anchors) and a
//!   per-region `PostAnchor` fallback that lags the combined form by
//!   `fallback_lag_slots` in peacetime and goes out at once for a
//!   contested bell (SP-FEE D5: holding one region held the combined form
//!   for 9.8 s while a fallback landed in 2.0 s). Class D.
//! - **Seed cache** of `(b, r)`: once `S(b, r) = first_round_from(A + W +
//!   Δ)` is published, `PostSeed` at a random unused nonce; a write not
//!   landed within `nonce_switch_slots` is dropped for the next random
//!   nonce (a held cache address stalls nothing). Each nonce is its own
//!   write key, so the journal names every address tried and a restarted
//!   keeper finds its caches; for an anchor it did not see land it probes
//!   all 256 nonces (bounded per tick) before creating one. Class D.
//! - **BeaconLog**: once per bell, `PostBeacon` for every region whose log
//!   is behind the latest round. Class N.
//!
//! Every rule time comes from the chain's Clock (the `now` of the tick).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rand::seq::SliceRandom;
use solana_address::Address;

use fclient::abi::{tag, Class};
use fclient::decode::{BeaconLog, BellAnchor, SeedCache};
use fclient::ix;
use fclient::ports::{ChainPort, DrandPort, PortResult};

use crate::engine::{BuildCtx, Engine, WriteSpec};
use crate::journal::Journal;
use crate::rounds::Rounds;
use crate::Tick;

/// `PostAnchorMulti`'s region count (v1.2 §5.5, W1-E's tx-size test).
pub const MULTI_MAX_REGIONS: usize = frontier_abi::budgets::MULTI_MAX_REGIONS;
/// Bells one tick scans for anchors (catch-up after an outage).
const SCAN_BELLS: u32 = 256;
/// Nonces a seed write may rotate through.
const NONCES: usize = 8;
/// Cache addresses probed per tick for anchors whose cache this process
/// has no record of (after a restart: 256 nonces per anchor).
const PROBE_PER_TICK: usize = 2_048;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheInfo {
    pub nonce: u8,
    pub rent_to: Address,
    pub slot: u64,
}

/// THE anchor of `(bell, region)` as read from chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnchorInfo {
    pub a: i64,
    pub slot: u64,
    pub round: u64,
    pub rent_to: Address,
    pub cache: Option<CacheInfo>,
    pub archived: bool,
}

struct SeedTrack {
    nonces: Vec<u8>,
    /// The nonce in use (the ones before it were switched away from).
    idx: usize,
    available_slot: u64,
}

pub fn anchor_key(b: u32, r: u8) -> String {
    format!("anchor:{b}:{r}")
}
pub fn multi_key(b: u32, chunk: usize) -> String {
    format!("anchor-multi:{b}:{chunk}")
}
/// One key per nonce, so the journal names the cache address each
/// version targeted (a restart finds its caches without probing).
pub fn seed_key(b: u32, r: u8, nonce: u8) -> String {
    format!("seed:{b}:{r}:{nonce}")
}

pub struct BeaconDuty {
    pub regions: Vec<u8>,
    /// Lowest bell with a region of the set not yet anchored.
    pub next_bell: Option<u32>,
    pub anchors: BTreeMap<(u32, u8), AnchorInfo>,
    seeds: BTreeMap<(u32, u8), SeedTrack>,
    /// Slot at which `T(b)` was first available to this keeper.
    pub first_seen: BTreeMap<u32, u64>,
    pub contested_bells: BTreeSet<u32>,
    /// Anchors whose journal nonces were looked at, and whose 256 cache
    /// addresses were probed (restart discovery).
    journal_checked: BTreeSet<(u32, u8)>,
    probed: BTreeSet<(u32, u8)>,
    pub beacon_log_bell: Option<u32>,
    /// Slots from `T(b)` available to the anchor's landing, per anchor
    /// (E5 criterion 3: p99 ≤ 2 at 20×).
    pub anchor_latency: Vec<u64>,
    /// Slots from `S` available to the first cache's landing.
    pub seed_latency: Vec<u64>,
}

impl BeaconDuty {
    pub fn new(regions: Vec<u8>) -> BeaconDuty {
        BeaconDuty {
            regions,
            next_bell: None,
            anchors: BTreeMap::new(),
            seeds: BTreeMap::new(),
            first_seen: BTreeMap::new(),
            contested_bells: BTreeSet::new(),
            journal_checked: BTreeSet::new(),
            probed: BTreeSet::new(),
            beacon_log_bell: None,
            anchor_latency: vec![],
            seed_latency: vec![],
        }
    }

    fn chunks(&self) -> Vec<Vec<u8>> {
        self.regions
            .chunks(MULTI_MAX_REGIONS)
            .map(|c| c.to_vec())
            .collect()
    }

    /// Plans anchors, seed caches and beacon-log posts for this tick.
    pub async fn plan<P: ChainPort, D: DrandPort>(
        &mut self,
        t: &Tick<'_>,
        port: &P,
        drand: &D,
        rounds: &mut Rounds,
        engine: &mut Engine,
        journal: Option<&Journal>,
    ) -> PortResult<()> {
        let Some(b_now) = t.clock.bell_at(t.now) else {
            return Ok(());
        };
        self.anchors_plan(t, b_now, port, drand, rounds, engine)
            .await?;
        self.seeds_plan(t, port, drand, rounds, engine, journal)
            .await?;
        self.beacon_log_plan(t, b_now, port, drand, rounds, engine)
            .await?;
        // Keep what the archive duty needs (48 h + margin), drop the rest.
        let keep = t.clock.archive_after / 600 + 2 * 144;
        let floor = b_now.saturating_sub(keep);
        self.anchors.retain(|(b, _), _| *b >= floor);
        self.first_seen.retain(|b, _| *b >= floor);
        self.probed.retain(|(b, _)| *b >= floor);
        self.journal_checked.retain(|(b, _)| *b >= floor);
        self.contested_bells.retain(|b| *b >= floor);
        Ok(())
    }

    async fn anchors_plan<P: ChainPort, D: DrandPort>(
        &mut self,
        t: &Tick<'_>,
        b_now: u32,
        port: &P,
        drand: &D,
        rounds: &mut Rounds,
        engine: &mut Engine,
    ) -> PortResult<()> {
        if b_now == 0 {
            return Ok(()); // bell 0 has not ended
        }
        let last = b_now - 1;
        let start = *self
            .next_bell
            .get_or_insert(b_now.saturating_sub(t.cfg.rescan_bells));
        let end = last.min(start.saturating_add(SCAN_BELLS - 1));
        if start > end {
            return Ok(());
        }
        // Read the anchors not known yet.
        let want: Vec<(u32, u8)> = (start..=end)
            .flat_map(|b| self.regions.iter().map(move |&r| (b, r)))
            .filter(|k| !self.anchors.contains_key(k))
            .collect();
        let keys: Vec<Address> = want.iter().map(|&(b, r)| t.addrs.anchor(b, r)).collect();
        let got = port.accounts(&keys, 0).await?;
        for (&(b, r), acct) in want.iter().zip(got) {
            let Some(acct) = acct else { continue };
            if acct.owner != t.addrs.program {
                continue;
            }
            let Ok(an) = BellAnchor::decode(&acct.data) else {
                continue;
            };
            if an.bell != b || an.region != r || an.season_id != t.addrs.season_id {
                continue;
            }
            if let Some(f) = self.first_seen.get(&b) {
                self.anchor_latency.push(an.slot.saturating_sub(*f));
            }
            // Present: its fallback (if any) has nothing left to do.
            engine.cancel(&anchor_key(b, r));
            self.anchors.insert(
                (b, r),
                AnchorInfo {
                    a: an.a,
                    slot: an.slot,
                    round: an.round,
                    rent_to: an.rent_to,
                    cache: None,
                    archived: false,
                },
            );
        }
        // Plan the missing ones, bell by bell.
        let chunks = self.chunks();
        for b in start..=end {
            let missing: Vec<u8> = self
                .regions
                .iter()
                .copied()
                .filter(|&r| !self.anchors.contains_key(&(b, r)))
                .collect();
            for (i, c) in chunks.iter().enumerate() {
                if c.iter().all(|r| !missing.contains(r)) {
                    engine.cancel(&multi_key(b, i));
                }
            }
            if missing.is_empty() {
                continue;
            }
            let round = t.clock.tlock_round(b);
            if t.clock.drand.round_time(round) > t.now {
                break;
            }
            let Some(arg) = rounds.get(drand, round, t.slot).await else {
                break;
            };
            let first = *self.first_seen.entry(b).or_insert(t.slot);
            let arg = Arc::new(arg);
            for (i, c) in chunks.iter().enumerate() {
                if !c.iter().any(|r| missing.contains(r)) {
                    continue;
                }
                let mask = c.iter().fold(0u16, |m, &r| m | (1 << r));
                let (a, arg2, ben) = (t.addrs.clone(), arg.clone(), t.cfg.beneficiary);
                engine.ensure(
                    WriteSpec {
                        key: multi_key(b, i),
                        kind: "anchor-multi",
                        tag: tag::POST_ANCHOR_MULTI,
                        class: Class::D,
                        bell: Some(b),
                        region: None,
                        build: Arc::new(move |c: &BuildCtx| {
                            vec![ix::post_anchor_multi(&a, c.payer, b, &arg2, mask, &ben)]
                        }),
                        deadline_slot: None,
                        not_before_slot: 0,
                        fixed_payer: None,
                    },
                    t.slot,
                );
            }
            let lag = if self.contested_bells.contains(&b) {
                0
            } else {
                t.cfg.fallback_lag_slots
            };
            for &r in &missing {
                let (a, arg2, ben) = (t.addrs.clone(), arg.clone(), t.cfg.beneficiary);
                engine.ensure(
                    WriteSpec {
                        key: anchor_key(b, r),
                        kind: "anchor",
                        tag: tag::POST_ANCHOR,
                        class: Class::D,
                        bell: Some(b),
                        region: Some(r),
                        build: Arc::new(move |c: &BuildCtx| {
                            vec![ix::post_anchor(&a, c.payer, r, b, &arg2, &ben)]
                        }),
                        deadline_slot: None,
                        not_before_slot: first + lag,
                        fixed_payer: None,
                    },
                    t.slot,
                );
            }
        }
        let mut next = start;
        while next <= end
            && self
                .regions
                .iter()
                .all(|&r| self.anchors.contains_key(&(next, r)))
        {
            next += 1;
        }
        self.next_bell = Some(next);
        Ok(())
    }

    async fn seeds_plan<P: ChainPort, D: DrandPort>(
        &mut self,
        t: &Tick<'_>,
        port: &P,
        drand: &D,
        rounds: &mut Rounds,
        engine: &mut Engine,
        journal: Option<&Journal>,
    ) -> PortResult<()> {
        let switch = t.cfg.nonce_switch_slots.max(1);
        // 1. Which cache addresses to look at: every nonce a tracked write
        //    has tried; for an anchor this process did not see land (a
        //    restart, another keeper), the nonces its journal names, then —
        //    bounded per tick — all 256.
        let mut want: Vec<((u32, u8), u8)> = vec![];
        for (k, s) in &self.seeds {
            for n in &s.nonces[..=s.idx] {
                want.push((*k, *n));
            }
        }
        let mut probe_budget = PROBE_PER_TICK;
        let unknown: Vec<(u32, u8)> = self
            .anchors
            .iter()
            .filter(|(k, a)| a.cache.is_none() && !a.archived && !self.seeds.contains_key(k))
            .map(|(k, _)| *k)
            .collect();
        for (b, r) in unknown {
            if self.journal_checked.insert((b, r)) {
                // First the nonces this keeper's journal names.
                let ns: Vec<u8> = journal
                    .and_then(|j| j.object_keys_with_prefix(&format!("seed:{b}:{r}:")).ok())
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|k| k.rsplit(':').next()?.parse().ok())
                    .collect();
                want.extend(ns.into_iter().map(|n| ((b, r), n)));
            } else if !self.first_seen.contains_key(&b)
                && !self.probed.contains(&(b, r))
                && probe_budget >= 256
            {
                // Not seen landing by this process and not in its journal:
                // any keeper's cache counts, so look at all 256 nonces.
                probe_budget -= 256;
                self.probed.insert((b, r));
                want.extend((0..=255).map(|n| ((b, r), n)));
            }
        }
        if !want.is_empty() {
            let keys: Vec<Address> = want
                .iter()
                .map(|&((b, r), n)| t.addrs.seed_cache(b, r, n))
                .collect();
            let got = port.accounts(&keys, 0).await?;
            for (&((b, r), n), acct) in want.iter().zip(got) {
                let Some(acct) = acct else { continue };
                if acct.owner != t.addrs.program {
                    continue;
                }
                let Ok(c) = SeedCache::decode(&acct.data) else {
                    continue;
                };
                if c.bell != b || c.region != r || c.nonce != n {
                    continue;
                }
                let Some(a) = self.anchors.get_mut(&(b, r)) else {
                    continue;
                };
                if a.cache.is_none() {
                    if let Some(s) = self.seeds.get(&(b, r)) {
                        self.seed_latency
                            .push(c.slot.saturating_sub(s.available_slot));
                    }
                    a.cache = Some(CacheInfo {
                        nonce: n,
                        rent_to: c.rent_to,
                        slot: c.slot,
                    });
                }
                if let Some(s) = self.seeds.remove(&(b, r)) {
                    for m in &s.nonces[..=s.idx] {
                        engine.cancel(&seed_key(b, r, *m));
                    }
                }
            }
        }
        // 2. New writes and the nonce switch.
        let cands: Vec<((u32, u8), i64)> = self
            .anchors
            .iter()
            .filter(|(k, a)| a.cache.is_none() && !a.archived && self.probed_or_seen(k))
            .map(|(k, a)| (*k, a.a))
            .collect();
        for ((b, r), a_ts) in cands {
            let s_round = t.clock.seed_round(b, a_ts);
            if t.clock.drand.round_time(s_round) > t.now {
                continue;
            }
            let Some(arg) = rounds.get(drand, s_round, t.slot).await else {
                continue;
            };
            let track = self.seeds.entry((b, r)).or_insert_with(|| {
                let mut all: Vec<u8> = (0..=255).collect();
                all.shuffle(&mut rand::rngs::OsRng);
                SeedTrack {
                    nonces: all[..NONCES].to_vec(),
                    idx: 0,
                    available_slot: t.slot,
                }
            });
            let key = seed_key(b, r, track.nonces[track.idx]);
            if let Some(f) = engine.first_slot(&key) {
                if t.slot < f + switch || track.idx + 1 >= track.nonces.len() {
                    continue;
                }
                // Not landed within the switch time: a held cache address
                // must not stall the seed; the next random nonce goes out.
                engine.cancel(&key);
                track.idx += 1;
            } else if engine.is_pending(&key) {
                continue;
            }
            let n = track.nonces[track.idx];
            let (a, ben, arg) = (t.addrs.clone(), t.cfg.beneficiary, Arc::new(arg));
            engine.ensure(
                WriteSpec {
                    key: seed_key(b, r, n),
                    kind: "seed",
                    tag: tag::POST_SEED,
                    class: Class::D,
                    bell: Some(b),
                    region: Some(r),
                    build: Arc::new(move |c: &BuildCtx| {
                        vec![ix::post_seed(&a, c.payer, r, b, n, &arg, &ben)]
                    }),
                    deadline_slot: None,
                    not_before_slot: 0,
                    fixed_payer: None,
                },
                t.slot,
            );
        }
        Ok(())
    }

    /// A cache may be created once the journal was consulted and, for an
    /// anchor this process did not see land, all nonces were probed.
    fn probed_or_seen(&self, k: &(u32, u8)) -> bool {
        self.journal_checked.contains(k)
            && (self.first_seen.contains_key(&k.0) || self.probed.contains(k))
    }

    async fn beacon_log_plan<P: ChainPort, D: DrandPort>(
        &mut self,
        t: &Tick<'_>,
        b_now: u32,
        port: &P,
        drand: &D,
        rounds: &mut Rounds,
        engine: &mut Engine,
    ) -> PortResult<()> {
        if self.beacon_log_bell == Some(b_now) {
            return Ok(());
        }
        // The latest round published by now (quicknet's latency ≤ 2 s).
        let mut latest = t.clock.drand.first_round_from(t.now);
        while latest > 1 && t.clock.drand.round_time(latest) > t.now - 2 {
            latest -= 1;
        }
        let Some(arg) = rounds.get(drand, latest, t.slot).await else {
            return Ok(());
        };
        let keys: Vec<Address> = self
            .regions
            .iter()
            .map(|&r| t.addrs.beacon_log(r))
            .collect();
        let got = port.accounts(&keys, 0).await?;
        let arg = Arc::new(arg);
        for (&r, acct) in self.regions.iter().zip(got) {
            let behind = acct
                .and_then(|a| BeaconLog::decode(&a.data).ok())
                .is_none_or(|l| l.latest_round < latest);
            if !behind {
                continue;
            }
            let (a, arg2) = (t.addrs.clone(), arg.clone());
            engine.ensure(
                WriteSpec {
                    key: format!("beacon:{b_now}:{r}"),
                    kind: "beacon-log",
                    tag: tag::POST_BEACON,
                    class: Class::N,
                    bell: Some(b_now),
                    region: Some(r),
                    build: Arc::new(move |c: &BuildCtx| {
                        vec![ix::post_beacon(&a, c.payer, r, &arg2)]
                    }),
                    deadline_slot: None,
                    not_before_slot: 0,
                    fixed_payer: None,
                },
                t.slot,
            );
        }
        self.beacon_log_bell = Some(b_now);
        Ok(())
    }

    /// The random nonces a seed write of `(b, r)` rotates through (metrics, tests).
    pub fn seed_nonces(&self, b: u32, r: u8) -> Option<Vec<u8>> {
        self.seeds.get(&(b, r)).map(|s| s.nonces.clone())
    }

    /// A write of this duty was contested.
    pub fn on_contested(&mut self, bell: Option<u32>) {
        if let Some(b) = bell {
            self.contested_bells.insert(b);
        }
    }
}
