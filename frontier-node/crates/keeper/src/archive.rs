//! Archive duty (M1 contract §5.8, §8.2 "Archive"; I-44, I-49).
//!
//! 48 h after an anchor landed (`A + archive_after`), `ArchiveAnchors`
//! moves up to 8 bells of one region-half-day into its AnchorArchive (v1.3:
//! one archive per region and `part = bell / 72`) — the program sets the tombstone and archived bits first,
//! stores `{a_off, seed, sig}`, then closes each anchor **to its `rent_to`**,
//! the keeper payer that created it (I-49: the pool refills). Once a bell is
//! archived its seed caches close to their own `rent_to` (`CloseSeedCache`,
//! class N). The instructions are W4-B's; against a program that answers
//! `NotImplemented` the duty stops and reports it.

use std::collections::BTreeMap;
use std::sync::Arc;

use solana_address::Address;

use fclient::abi::{tag, Class};
use fclient::addr::archive_part;
use fclient::ix::{self, ArchiveItem};

use crate::beacon::AnchorInfo;
use crate::engine::{BuildCtx, Engine, WriteSpec};
use crate::Tick;

/// Bells per ArchiveAnchors transaction (§5.8).
pub const ARCHIVE_BATCH: usize = 8;

/// One ArchiveAnchors transaction to send.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveBatch {
    pub region: u8,
    /// The archive part (half day, `bell / 72`, v1.3).
    pub part: u32,
    pub items: Vec<ArchiveItem>,
}

/// Groups every archivable anchor (`now ≥ A + archive_after`, its seed cache
/// known, not archived yet) by region and archive part, in bell order, ≤ 8
/// per batch.
pub fn plan_archive(
    anchors: &BTreeMap<(u32, u8), AnchorInfo>,
    now: i64,
    archive_after: u32,
) -> Vec<ArchiveBatch> {
    let mut by: BTreeMap<(u8, u32), Vec<ArchiveItem>> = BTreeMap::new();
    for (&(b, r), a) in anchors {
        if a.archived || a.a + archive_after as i64 > now {
            continue;
        }
        let Some(c) = a.cache else { continue };
        by.entry((r, archive_part(b)))
            .or_default()
            .push(ArchiveItem {
                bell: b,
                cache_nonce: c.nonce,
                anchor_rent_to: a.rent_to,
            });
    }
    let mut out = vec![];
    for ((region, part), mut items) in by {
        items.sort_by_key(|i| i.bell);
        for chunk in items.chunks(ARCHIVE_BATCH) {
            out.push(ArchiveBatch {
                region,
                part,
                items: chunk.to_vec(),
            });
        }
    }
    out
}

pub fn batch_key(b: &ArchiveBatch) -> String {
    format!(
        "archive:{}:{}:{}",
        b.region,
        b.part,
        b.items.first().map_or(0, |i| i.bell)
    )
}

#[derive(Default)]
pub struct ArchiveDuty {
    /// The program has no ArchiveAnchors yet (99).
    pub unsupported: bool,
    /// Bells archived by a landed batch, per region: `(bell, cache nonce, cache rent_to)`.
    pub to_close: Vec<(u32, u8, u8, Address)>,
    pub archived_bells: u64,
    pub caches_closed: u64,
    /// The bells of every batch sent, by write key.
    batches: std::collections::HashMap<String, (u8, Vec<u32>)>,
}

impl ArchiveDuty {
    /// Remembers which bells the batch sent under its key carries.
    pub fn track(&mut self, b: &ArchiveBatch) {
        self.batches.insert(
            batch_key(b),
            (b.region, b.items.iter().map(|i| i.bell).collect()),
        );
    }

    pub fn plan(
        &mut self,
        t: &Tick<'_>,
        anchors: &BTreeMap<(u32, u8), AnchorInfo>,
        engine: &mut Engine,
    ) {
        if self.unsupported || !t.cfg.has_role("archive") {
            return;
        }
        for batch in plan_archive(anchors, t.now, t.clock.archive_after) {
            let (a, (r, d, items)) = (
                t.addrs.clone(),
                (batch.region, batch.part, batch.items.clone()),
            );
            let added = engine.ensure(
                WriteSpec {
                    key: batch_key(&batch),
                    kind: "archive",
                    tag: tag::ARCHIVE_ANCHORS,
                    class: Class::D,
                    bell: batch.items.first().map(|i| i.bell),
                    region: Some(batch.region),
                    build: Arc::new(move |c: &BuildCtx| {
                        vec![ix::archive_anchors(&a, c.payer, r, d, &items)]
                    }),
                    deadline_slot: None,
                    not_before_slot: 0,
                    fixed_payer: None,
                },
                t.slot,
            );
            if added {
                // The builder carries exactly these bells; a later plan may
                // group more under the same key only once this one is done.
                self.track(&batch);
            }
        }
        for &(bell, region, nonce, rent_to) in &self.to_close {
            let a = t.addrs.clone();
            engine.ensure(
                WriteSpec {
                    key: format!("close-seed:{bell}:{region}:{nonce}"),
                    kind: "close-seed",
                    tag: tag::CLOSE_SEED_CACHE,
                    class: Class::N,
                    bell: Some(bell),
                    region: Some(region),
                    build: Arc::new(move |c: &BuildCtx| {
                        vec![ix::close_seed_cache(
                            &a, c.payer, bell, region, nonce, rent_to,
                        )]
                    }),
                    deadline_slot: None,
                    not_before_slot: 0,
                    fixed_payer: None,
                },
                t.slot,
            );
        }
    }

    /// A batch landed: its anchors are archived; their caches may close.
    pub fn on_landed(&mut self, key: &str, anchors: &mut BTreeMap<(u32, u8), AnchorInfo>) {
        if key.starts_with("archive:") {
            // Exactly the bells that batch carried.
            let Some((region, bells)) = self.batches.remove(key) else {
                return;
            };
            for b in bells {
                if let Some(a) = anchors.get_mut(&(b, region)) {
                    if a.archived {
                        continue;
                    }
                    a.archived = true;
                    self.archived_bells += 1;
                    if let Some(c) = a.cache {
                        self.to_close.push((b, region, c.nonce, c.rent_to));
                    }
                }
            }
        } else if let Some(rest) = key.strip_prefix("close-seed:") {
            let p: Vec<u32> = rest.split(':').filter_map(|x| x.parse().ok()).collect();
            if let [b, r, n] = p[..] {
                self.to_close
                    .retain(|x| !(x.0 == b && x.1 == r as u8 && x.2 == n as u8));
                self.caches_closed += 1;
            }
        }
    }

    pub fn on_dead(&mut self, key: &str, code: Option<u32>) -> bool {
        if (key.starts_with("archive:") || key.starts_with("close-seed:"))
            && code == Some(fclient::abi::err::NOT_IMPLEMENTED)
        {
            self.unsupported = true;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beacon::CacheInfo;

    fn info(a: i64, cache: bool, payer: u8) -> AnchorInfo {
        AnchorInfo {
            a,
            slot: 1,
            round: 1,
            rent_to: Address::new_from_array([payer; 32]),
            cache: cache.then_some(CacheInfo {
                nonce: payer,
                rent_to: Address::new_from_array([payer + 1; 32]),
                slot: 2,
            }),
            archived: false,
        }
    }

    /// Only anchors 48 h old with a known cache, grouped by region and half
    /// day (v1.3), ≤ 8
    /// bells per transaction, each item carrying the anchor's own `rent_to`
    /// (the payer that created it gets its rent back, I-49).
    #[test]
    fn archive_batches_by_region_day_with_the_creating_payer() {
        let t0 = 1_000_000i64;
        let mut m = BTreeMap::new();
        for b in 140..155u32 {
            m.insert((b, 3u8), info(t0 + 600 * b as i64, b != 150, b as u8));
        }
        m.insert((141, 4), info(t0 + 600 * 141, true, 9));
        let now = t0 + 600 * 152 + 172_800; // bells ≤ 152 are 48 h old
        let plan = plan_archive(&m, now, 172_800);
        let shape: Vec<(u8, u32, Vec<u32>)> = plan
            .iter()
            .map(|b| (b.region, b.part, b.items.iter().map(|i| i.bell).collect()))
            .collect();
        assert_eq!(
            shape,
            vec![
                (3, 1, vec![140, 141, 142, 143]),
                (3, 2, vec![144, 145, 146, 147, 148, 149, 151, 152]),
                (4, 1, vec![141]),
            ],
            "150 has no cache yet; 153+ are younger than 48 h"
        );
        assert!(plan.iter().flat_map(|b| &b.items).all(|i| i.anchor_rent_to
            == Address::new_from_array([i.bell as u8; 32])
            || i.bell == 141));
        let mut d = ArchiveDuty::default();
        d.on_landed(&batch_key(&plan[1]), &mut m);
        assert_eq!(d.archived_bells, 0, "an unknown batch marks nothing");
        d.track(&plan[1]);
        d.on_landed(&batch_key(&plan[1]), &mut m);
        assert_eq!(d.archived_bells, 8);
        assert_eq!(d.to_close.len(), 8);
        assert!(
            plan_archive(&m, now, 172_800).iter().all(|b| b.part == 1),
            "archived bells are not planned again"
        );
    }
}
