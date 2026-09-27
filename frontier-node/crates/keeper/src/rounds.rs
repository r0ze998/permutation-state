//! Verified drand rounds with their hints (offchain design §6.4).
//!
//! Each round is verified off chain against the season's pinned key before
//! any transaction carries it, so a bad beacon never costs a 345k-CU
//! transaction; its hash-to-curve hints (2 × 145 B, SP-V2) are computed once
//! and cached. A round the drand port does not serve yet is asked again at
//! most once per slot.

use std::collections::{BTreeMap, HashMap};

use fclient::beacon;
use fclient::ix::BeaconArg;
use fclient::ports::DrandPort;

pub struct Rounds {
    pub pk96: [u8; 96],
    cache: BTreeMap<u64, BeaconArg>,
    asked: HashMap<u64, u64>,
    /// Rounds a port served that did not verify (never used).
    pub bad: u64,
    /// Rounds served and verified.
    pub fetched: u64,
    keep: usize,
}

impl Rounds {
    pub fn new(pk96: [u8; 96]) -> Rounds {
        Rounds {
            pk96,
            cache: BTreeMap::new(),
            asked: HashMap::new(),
            bad: 0,
            fetched: 0,
            keep: 4_096,
        }
    }

    /// `QUICKNET_PK_HASH` of the key in use (the season pins it).
    pub fn pk_hash(&self) -> [u8; 32] {
        beacon::pk_hash(&self.pk96)
    }

    /// The verified round `r`, if published.
    pub async fn get<D: DrandPort>(&mut self, drand: &D, r: u64, slot: u64) -> Option<BeaconArg> {
        if let Some(a) = self.cache.get(&r) {
            return Some(a.clone());
        }
        if self.asked.get(&r) == Some(&slot) {
            return None;
        }
        self.asked.insert(r, slot);
        let b = drand.round(r).await.ok().flatten()?;
        if b.round != r || !beacon::verify(r, &b.sig48, &self.pk96) {
            self.bad += 1;
            return None;
        }
        let arg = beacon::beacon_arg(&b);
        self.fetched += 1;
        self.asked.remove(&r);
        self.cache.insert(r, arg.clone());
        while self.cache.len() > self.keep {
            let first = *self.cache.keys().next().expect("non-empty");
            self.cache.remove(&first);
        }
        if self.asked.len() > self.keep {
            self.asked.clear();
        }
        Some(arg)
    }
}
