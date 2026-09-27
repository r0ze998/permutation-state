//! Where the seed `S(b, r)` of a `(bell, region)` can be read, and its
//! value (M1 contract §5.8: a SeedCache of THE anchor, or the archive entry
//! once the bell is archived).
//!
//! SettleTicket and SettleExplore name `[seedcache|archive] [anchor|archive]`;
//! the keeper's ticket order needs the seed's value too (the score). The
//! beacon duty knows the caches it created; for any other `(b, r)` (a
//! region this keeper does not anchor, a restart) the finder reads THE
//! anchor and probes the 256 cache nonces once, or reads the archive.

use std::collections::BTreeMap;

use fclient::addr::{archive_part, Addresses};
use fclient::decode::{AnchorArchive, BellAnchor, SeedCache};
use fclient::ix::SeedSource;
use fclient::ports::{ChainPort, PortResult};

use crate::beacon::AnchorInfo;

/// A seed known on chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeedInfo {
    pub src: SeedSource,
    pub seed: [u8; 32],
    /// First slot this keeper saw it.
    pub seen_slot: u64,
    /// Landing slot of the SeedCache (none for an archive entry).
    pub cache_slot: Option<u64>,
}

/// Slots between two probes of a `(b, r)` whose seed is not there yet.
const RETRY_SLOTS: u64 = 2;

#[derive(Default)]
pub struct SeedFinder {
    known: BTreeMap<(u32, u8), SeedInfo>,
    last_try: BTreeMap<(u32, u8), u64>,
    /// Anchors whose 256 nonces were probed without a cache.
    probed: BTreeMap<(u32, u8), u64>,
}

impl SeedFinder {
    pub fn known(&self, bell: u32, region: u8) -> Option<SeedInfo> {
        self.known.get(&(bell, region)).copied()
    }

    /// Forgets seeds of bells before `bell` (memory bound).
    pub fn prune_before(&mut self, bell: u32) {
        self.known = self.known.split_off(&(bell, 0));
        self.last_try = self.last_try.split_off(&(bell, 0));
        self.probed = self.probed.split_off(&(bell, 0));
    }

    /// The seed of `(bell, region)` if it is on chain.
    pub async fn find<P: ChainPort>(
        &mut self,
        port: &P,
        addrs: &Addresses,
        anchors: &BTreeMap<(u32, u8), AnchorInfo>,
        bell: u32,
        region: u8,
        slot: u64,
    ) -> PortResult<Option<SeedInfo>> {
        let k = (bell, region);
        let archived_now = anchors.get(&k).is_some_and(|a| a.archived);
        if let Some(s) = self.known.get(&k).copied() {
            // A cache closes after its bell is archived: switch the source.
            if archived_now && matches!(s.src, SeedSource::Cache { .. }) {
                let s2 = SeedInfo {
                    src: SeedSource::Archive,
                    ..s
                };
                self.known.insert(k, s2);
                return Ok(Some(s2));
            }
            return Ok(Some(s));
        }
        if self
            .last_try
            .get(&k)
            .is_some_and(|&t| slot < t + RETRY_SLOTS)
        {
            return Ok(None);
        }
        self.last_try.insert(k, slot);
        // The beacon duty's own cache.
        if let Some(c) = anchors
            .get(&k)
            .and_then(|a| a.cache)
            .filter(|_| !archived_now)
        {
            let got = port
                .accounts(&[addrs.seed_cache(bell, region, c.nonce)], 0)
                .await?;
            if let Some(sc) = got[0]
                .as_ref()
                .and_then(|a| SeedCache::decode(&a.data).ok())
            {
                return Ok(Some(self.remember(
                    k,
                    SeedSource::Cache { nonce: c.nonce },
                    sc.seed,
                    slot,
                    Some(sc.slot),
                )));
            }
        }
        // THE anchor and the archive.
        let got = port
            .accounts(
                &[
                    addrs.anchor(bell, region),
                    addrs.archive(region, archive_part(bell)),
                ],
                0,
            )
            .await?;
        if let Some(ar) = got[1]
            .as_ref()
            .filter(|a| a.owner == addrs.program)
            .and_then(|a| AnchorArchive::decode(&a.data).ok())
        {
            if ar.is_archived(bell) {
                let e = ar.entries[bell as usize % ar.entries.len()];
                return Ok(Some(self.remember(
                    k,
                    SeedSource::Archive,
                    e.seed,
                    slot,
                    None,
                )));
            }
        }
        let anchor = got[0]
            .as_ref()
            .filter(|a| a.owner == addrs.program)
            .and_then(|a| BellAnchor::decode(&a.data).ok());
        let Some(_anchor) = anchor else {
            return Ok(None);
        };
        // Probe every nonce once per 16 slots (a cache created by another
        // keeper, or by this one before a restart).
        if self.probed.get(&k).is_some_and(|&t| slot < t + 16) {
            return Ok(None);
        }
        self.probed.insert(k, slot);
        let keys: Vec<_> = (0..=255u8)
            .map(|n| addrs.seed_cache(bell, region, n))
            .collect();
        for (chunk_i, chunk) in keys.chunks(100).enumerate() {
            let got = port.accounts(chunk, 0).await?;
            for (j, a) in got.iter().enumerate() {
                let Some(sc) = a
                    .as_ref()
                    .filter(|a| a.owner == addrs.program)
                    .and_then(|a| SeedCache::decode(&a.data).ok())
                else {
                    continue;
                };
                if sc.anchor_key == addrs.anchor(bell, region) {
                    let nonce = (chunk_i * 100 + j) as u8;
                    return Ok(Some(self.remember(
                        k,
                        SeedSource::Cache { nonce },
                        sc.seed,
                        slot,
                        Some(sc.slot),
                    )));
                }
            }
        }
        Ok(None)
    }

    fn remember(
        &mut self,
        k: (u32, u8),
        src: SeedSource,
        seed: [u8; 32],
        slot: u64,
        cache_slot: Option<u64>,
    ) -> SeedInfo {
        let s = SeedInfo {
            src,
            seed,
            seen_slot: slot,
            cache_slot,
        };
        self.known.insert(k, s);
        s
    }
}
