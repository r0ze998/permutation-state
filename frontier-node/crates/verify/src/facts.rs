//! Season facts every check reads: the season's parameters (from the
//! CreateSeason instruction the chain accepted), its clock, THE anchors
//! and the caches of every `(bell, region)`, the drand signatures the
//! transactions carried, and the land's owners over time.
//!
//! These are **inputs** to the checks, not conclusions: V3 judges the
//! anchors and signatures, V11 the land; the others read the facts and,
//! where a fact they depend on is missing, say so (`MissingData`).

use std::collections::{BTreeMap, HashMap};

use frontier_abi::addr::AddrCtx;
use frontier_abi::ix as aix;
use frontier_abi::log::{settle_outcome, Kind};
use frontier_abi::presets::SeasonParams;
use frontier_abi::tags::Ix;
use permutation_rules::frontier::beacon::{self, WindowSchedule};
use permutation_rules::frontier::clash::BeaconClock;
use permutation_rules::frontier::geometry::{region_of, ProvinceCoord};

use crate::input::Config;
use crate::world::{Key, World};

/// `(bell, region)`.
pub type BellRegion = (u32, u8);

/// THE anchor of a `(bell, region)` as its ANCHOR record states it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnchorFact {
    pub rec: usize,
    pub round: u64,
    pub a: i64,
    pub slot: u64,
    pub beneficiary: [u8; 32],
}

/// A seed cache of a `(bell, region)` as its SEED record states it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeedFact {
    pub rec: usize,
    pub nonce: u8,
    pub round: u64,
    pub seed: [u8; 32],
    pub a: i64,
}

/// A drand signature a transaction carried in its instruction data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SigFact {
    pub tx: usize,
    pub round: u64,
    pub sig: [u8; 48],
    pub ix: Ix,
}

/// A citizen as its JOIN record states it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CitizenFact {
    pub rec: usize,
    pub addr: Key,
    pub tag15: [u8; 15],
    pub tag8: u64,
    pub wallet: [u8; 32],
    pub faction: u8,
    pub shard: u8,
}

/// One owner change of a site (SETTLE fresh/displace, RELEASE, CLOSE).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnerChange {
    pub rec: usize,
    pub owner: Option<Key>,
    pub gen: u8,
    pub score: u64,
    pub ticket_bell: u32,
    pub final_ts: i64,
    /// The ticket's funder (the Holding's rent payer, I-05/I-47).
    pub funder: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct Facts {
    pub ctx: AddrCtx,
    pub pk: [u8; 96],
    /// CreateSeason's parameters, the transaction and its time.
    pub params: Option<SeasonParams>,
    pub payout: Vec<u8>,
    pub create_tx: Option<usize>,
    pub announce_rec: Option<usize>,
    pub created_rec: Option<usize>,
    pub genesis_round: u64,
    pub genesis_ts: i64,
    pub clock: BeaconClock,
    pub window: WindowSchedule,
    pub margin: u32,
    pub genesis_seed: Option<[u8; 32]>,
    /// Every ANCHOR record per `(bell, region)` (more than one is a V3 FAIL).
    pub anchors: BTreeMap<BellRegion, Vec<AnchorFact>>,
    pub seeds: BTreeMap<BellRegion, Vec<SeedFact>>,
    /// ARCHIVE entries: `(a_off, seed, rec)`.
    pub archived: BTreeMap<BellRegion, (u32, [u8; 32], usize)>,
    /// Signatures carried by transactions, per round.
    pub sigs: BTreeMap<u64, Vec<SigFact>>,
    /// Ring seeds by ring (genesis rings from RING_OPEN, others from
    /// RING_SEED).
    pub ring_seeds: BTreeMap<u16, [u8; 32]>,
    pub citizens: HashMap<Key, CitizenFact>,
    pub by_tag8: HashMap<u64, Key>,
    /// Owner history per site `(P, Q, site)`, in record order.
    pub owners: BTreeMap<(i32, i32, u8), Vec<OwnerChange>>,
    /// The funder of each citizen's current ticket (TICKET.funder).
    pub ticket_funder: HashMap<Key, Vec<(usize, [u8; 32])>>,
    /// Province addresses (from PROVINCE_OPEN) → `(P, Q)`.
    pub provinces: HashMap<Key, (i32, i32)>,
    /// DEPART records per host id, in order.
    pub departs: HashMap<u64, Vec<usize>>,
}

impl Facts {
    pub fn build(w: &World, cfg: &Config) -> Facts {
        let ctx = findex::addr_ctx(&cfg.program, cfg.season_id);
        let mut f = Facts {
            ctx,
            pk: cfg.quicknet_pk,
            params: None,
            payout: vec![],
            create_tx: None,
            announce_rec: None,
            created_rec: None,
            genesis_round: 0,
            genesis_ts: 0,
            clock: BeaconClock {
                genesis: fclient::beacon::QUICKNET_GENESIS,
                period: fclient::beacon::QUICKNET_PERIOD as i64,
            },
            window: WindowSchedule::fixed(600),
            margin: 60,
            genesis_seed: None,
            anchors: BTreeMap::new(),
            seeds: BTreeMap::new(),
            archived: BTreeMap::new(),
            sigs: BTreeMap::new(),
            ring_seeds: BTreeMap::new(),
            citizens: HashMap::new(),
            by_tag8: HashMap::new(),
            owners: BTreeMap::new(),
            ticket_funder: HashMap::new(),
            provinces: HashMap::new(),
            departs: HashMap::new(),
        };
        // Season parameters: the CreateSeason the chain accepted.
        for (i, t) in w.txs.iter().enumerate().filter(|(_, t)| t.ok) {
            for ix in &t.ixs {
                let Some(tag) = ix.tag().and_then(Ix::from_tag) else {
                    continue;
                };
                match tag {
                    Ix::CreateSeason if f.params.is_none() => {
                        if let Ok(c) = aix::CreateSeason::decode(&ix.data) {
                            f.params = Some(c.params);
                            f.payout = c.payout.to_vec();
                            f.create_tx = Some(i);
                        }
                    }
                    Ix::PostAnchor => {
                        if let Ok(x) = aix::PostAnchor::decode(&ix.data) {
                            f.sig(i, x.round, x.sig48, tag);
                        }
                    }
                    Ix::PostAnchorMulti => {
                        if let Ok(x) = aix::PostAnchorMulti::decode(&ix.data) {
                            f.sig(i, x.round, x.sig48, tag);
                        }
                    }
                    Ix::PostSeed => {
                        if let Ok(x) = aix::PostSeed::decode(&ix.data) {
                            f.sig(i, x.round, x.sig48, tag);
                        }
                    }
                    Ix::PostBeacon => {
                        if let Ok(x) = aix::PostBeacon::decode(&ix.data) {
                            f.sig(i, x.round, x.sig48, tag);
                        }
                    }
                    Ix::ConsumeGenesisSeed => {
                        if let Ok(x) = aix::ConsumeGenesisSeed::decode(&ix.data) {
                            f.sig(i, x.round, x.sig48, tag);
                        }
                    }
                    Ix::ConsumeRingSeed => {
                        if let Ok(x) = aix::ConsumeRingSeed::decode(&ix.data) {
                            f.sig(i, x.round, x.sig48, tag);
                        }
                    }
                    _ => {}
                }
            }
        }
        if let Some(p) = &f.params {
            f.clock = BeaconClock {
                genesis: p.drand_genesis,
                period: p.drand_period as i64,
            };
            f.window = WindowSchedule::fixed(p.reveal_window);
            f.margin = p.seed_margin;
        }
        for (n, r) in w.recs.iter().enumerate() {
            match r.kind {
                Kind::ANNOUNCE if f.announce_rec.is_none() => f.announce_rec = Some(n),
                Kind::SEASON_CREATED if f.created_rec.is_none() => {
                    f.created_rec = Some(n);
                    f.genesis_round = r.pu64("genesis_round");
                    f.genesis_ts = r.pi64("genesis_ts");
                }
                Kind::WINDOW => {
                    f.window.window_next = r.pu32("window_next");
                    f.window.window_from_bell = r.pu32("window_from_bell");
                }
                Kind::GENESIS_SEED if f.genesis_seed.is_none() => {
                    f.genesis_seed = Some(r.p32("seed"))
                }
                Kind::ANCHOR => {
                    let k = (r.ku32("bell"), r.ku8("region"));
                    f.anchors.entry(k).or_default().push(AnchorFact {
                        rec: n,
                        round: r.pu64("round"),
                        a: r.pi64("a"),
                        slot: r.pu64("slot"),
                        beneficiary: r.p32("beneficiary"),
                    });
                }
                Kind::SEED => {
                    let k = (r.ku32("bell"), r.ku8("region"));
                    f.seeds.entry(k).or_default().push(SeedFact {
                        rec: n,
                        nonce: r.ku8("nonce"),
                        round: r.pu64("round"),
                        seed: r.p32("seed"),
                        a: r.pi64("a"),
                    });
                }
                Kind::ARCHIVE => {
                    let k = (r.pu32("bell"), r.ku8("region"));
                    f.archived.insert(k, (r.pu32("a_off"), r.p32("seed"), n));
                }
                Kind::RING_OPEN => {
                    let d = r.k("d");
                    let d = u16::from_le_bytes([d[0], d[1]]);
                    if r.pu64("round") == 0 {
                        f.ring_seeds.insert(d, r.p32("seed"));
                    }
                }
                Kind::RING_SEED => {
                    let d = r.k("d");
                    let d = u16::from_le_bytes([d[0], d[1]]);
                    f.ring_seeds.insert(d, r.p32("seed"));
                }
                Kind::JOIN => {
                    let tag15: [u8; 15] = r.k("citizen_tag15").try_into().unwrap_or([0; 15]);
                    let addr = f.ctx.citizen_by_tag15(&tag15);
                    let tag8 = u64::from_le_bytes(addr[..8].try_into().unwrap_or([0; 8]));
                    f.citizens.insert(
                        addr,
                        CitizenFact {
                            rec: n,
                            addr,
                            tag15,
                            tag8,
                            wallet: r.p32("wallet"),
                            faction: r.pu8("faction"),
                            shard: r.pu8("shard"),
                        },
                    );
                    f.by_tag8.insert(tag8, addr);
                }
                Kind::TICKET => {
                    let tag15: [u8; 15] = r.k("citizen_tag15").try_into().unwrap_or([0; 15]);
                    let addr = f.ctx.citizen_by_tag15(&tag15);
                    f.ticket_funder
                        .entry(addr)
                        .or_default()
                        .push((n, r.p32("funder")));
                }
                Kind::SETTLE => {
                    let o = r.pu8("outcome");
                    if o == settle_outcome::FRESH || o == settle_outcome::DISPLACE {
                        let owner = f.by_tag8.get(&r.pu64("citizen_tag")).copied();
                        let funder = owner.and_then(|c| f.funder_at(&c, n)).unwrap_or([0; 32]);
                        f.owners.entry(r.pqs()).or_default().push(OwnerChange {
                            rec: n,
                            owner,
                            gen: r.pu8("gen"),
                            score: r.pu64("score"),
                            ticket_bell: r.pu32("ticket_bell"),
                            final_ts: r.pi64("final_ts"),
                            funder,
                        });
                    }
                }
                Kind::PROVINCE_OPEN => {
                    let (p, q) = r.pq();
                    f.provinces.insert(f.ctx.province(p, q), (p, q));
                }
                Kind::DEPART => {
                    f.departs.entry(r.ku64("host_id")).or_default().push(n);
                }
                Kind::RELEASE => {
                    let gen = f.owner_at(r.pqs(), n).map(|c| c.gen).unwrap_or(0);
                    f.owners.entry(r.pqs()).or_default().push(OwnerChange {
                        rec: n,
                        owner: None,
                        gen,
                        score: 0,
                        ticket_bell: 0,
                        final_ts: 0,
                        funder: [0; 32],
                    });
                }
                _ => {}
            }
        }
        f
    }

    fn sig(&mut self, tx: usize, round: u64, sig: [u8; 48], ix: Ix) {
        self.sigs
            .entry(round)
            .or_default()
            .push(SigFact { tx, round, sig, ix });
    }

    /// The funder of `citizen`'s ticket as of record `n`.
    pub fn funder_at(&self, citizen: &Key, n: usize) -> Option<[u8; 32]> {
        self.ticket_funder
            .get(citizen)?
            .iter()
            .rev()
            .find(|(r, _)| *r < n)
            .map(|x| x.1)
    }

    /// The owner record of a site in force just before record `n`.
    pub fn owner_at(&self, site: (i32, i32, u8), n: usize) -> Option<&OwnerChange> {
        self.owners.get(&site)?.iter().rev().find(|c| c.rec < n)
    }

    /// The owner record of the holding of `host_id` before record `n`.
    pub fn owner_of_host(&self, host_id: u64, n: usize) -> Option<&OwnerChange> {
        let h = frontier_abi::addr::split_host_id(host_id)?;
        self.owner_at((h.province.p, h.province.q, h.site), n)
            .filter(|c| c.owner.is_some())
    }

    /// The DEPART of `host` in force at record `n`: the latest before it
    /// (with arrival bell `arrive` when given).
    pub fn depart_of(&self, w: &World, host: u64, n: usize, arrive: Option<u32>) -> Option<usize> {
        self.departs
            .get(&host)?
            .iter()
            .rev()
            .copied()
            .filter(|&d| d < n)
            .find(|&d| arrive.is_none_or(|a| w.recs[d].pu32("arrive_bell") == a))
    }

    /// `W(b)`.
    pub fn w(&self, b: u32) -> u32 {
        beacon::window(&self.window, b)
    }

    /// `T(b)`.
    pub fn tlock_round(&self, b: u32) -> u64 {
        beacon::tlock_round(&self.clock, self.genesis_ts, b)
    }

    /// `S(b, r)` for THE anchor landed at `a`.
    pub fn seed_round(&self, b: u32, a: i64) -> u64 {
        beacon::seed_round(&self.clock, beacon::reveal_close(a, self.w(b)), self.margin)
    }

    /// `A + W(b)`.
    pub fn close(&self, b: u32, a: i64) -> i64 {
        beacon::reveal_close(a, self.w(b))
    }

    pub fn round_time(&self, r: u64) -> i64 {
        beacon::round_time(self.clock.genesis, self.clock.period as u32, r)
    }

    /// THE anchor of `(bell, region)`: the first ANCHOR record.
    pub fn anchor(&self, bell: u32, region: u8) -> Option<AnchorFact> {
        self.anchors
            .get(&(bell, region))
            .and_then(|v| v.first())
            .copied()
    }

    /// `A` of `(bell, region)`, from the anchor or its archive entry.
    pub fn anchor_a(&self, bell: u32, region: u8) -> Option<i64> {
        self.anchor(bell, region).map(|a| a.a).or_else(|| {
            self.archived
                .get(&(bell, region))
                .map(|(off, _, _)| beacon::bell_end(self.genesis_ts, bell) + *off as i64)
        })
    }

    /// `S(bell, region)`'s seed: a SEED record of THE anchor with the
    /// rule's round, or the archive entry. The value is the chain's
    /// (V3 judges it).
    pub fn bell_seed(&self, bell: u32, region: u8) -> Option<[u8; 32]> {
        if let Some(a) = self.anchor(bell, region) {
            let want = self.seed_round(bell, a.a);
            if let Some(s) = self
                .seeds
                .get(&(bell, region))
                .and_then(|v| v.iter().find(|s| s.round == want && s.a == a.a))
            {
                return Some(s.seed);
            }
        }
        self.archived.get(&(bell, region)).map(|x| x.1)
    }

    /// A signature a transaction carried for `round` that verifies under
    /// the pinned key (cached by the caller through [`SigCache`]).
    pub fn good_sig(&self, round: u64, cache: &mut SigCache) -> Option<[u8; 48]> {
        let pk = self.pk;
        self.sigs
            .get(&round)?
            .iter()
            .map(|s| s.sig)
            .find(|s| cache.ok(round, s, &pk))
    }

    pub fn region(p: i32, q: i32) -> u8 {
        region_of(ProvinceCoord::new(p, q))
    }
}

/// Memoised BLS verification (a pairing is ≈ 1 ms).
#[derive(Default, Debug)]
pub struct SigCache {
    seen: HashMap<(u64, [u8; 48]), bool>,
}

impl SigCache {
    pub fn ok(&mut self, round: u64, sig: &[u8; 48], pk: &[u8; 96]) -> bool {
        *self
            .seen
            .entry((round, *sig))
            .or_insert_with(|| fclient::beacon::verify(round, sig, pk))
    }
}
