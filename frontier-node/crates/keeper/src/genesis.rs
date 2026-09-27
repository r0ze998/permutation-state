//! Genesis (M1 contract §8.2 "Genesis", §5.7, §5.9, I-30).
//!
//! - **Genesis seed** (role `beacon`): once the season is Created and its
//!   `genesis_round` is published, `ConsumeGenesisSeed` (class D). Nothing
//!   expires but the 7-day abort guard.
//! - **Genesis rings** (role `rings`): once Seeded, `OpenRing(d)` for
//!   `d = rings_opened ≤ g` one at a time (a genesis ring is seeded at open,
//!   `sha256("PSF-RING" ‖ genesis_seed ‖ le16(d))`, so no ConsumeRingSeed),
//!   then `OpenProvince` for every province of rings `0..=g` (class D).
//!   Rings beyond `g` (the crowding rule) are W3-C's.
//!
//! A program that answers `NotImplemented` (99) for OpenRing or
//! OpenProvince — the W2 program, whose land instructions arrive in wave 3
//! (W3-A) — ends the ring part: the keeper reports it and keeps its beacon
//! duties running.

use std::collections::BTreeSet;
use std::sync::Arc;

use solana_address::Address;

use fclient::abi::{status, tag, Class};
use fclient::decode::{Frontier, RingSeed};
use fclient::ix;
use fclient::ports::{ChainPort, DrandPort, PortResult};
use permutation_rules::frontier::geometry::ring_provinces;

use crate::engine::{BuildCtx, Engine, WriteSpec};
use crate::rounds::Rounds;
use crate::Tick;

pub const GENESIS_SEED_KEY: &str = "genesis-seed";

pub fn ring_key(d: u16) -> String {
    format!("open-ring:{d}")
}
pub fn province_key(p: i32, q: i32) -> String {
    format!("open-province:{p},{q}")
}

#[derive(Default)]
pub struct GenesisDuty {
    /// Provinces of the genesis rings seen present on chain.
    pub opened: BTreeSet<(i32, i32)>,
    /// The program has no land instructions yet (99 on OpenRing/OpenProvince).
    pub rings_unsupported: bool,
    /// Every genesis ring opened and every genesis province created.
    pub rings_done: bool,
    /// Slot the genesis round was first available, and the slot the seed landed.
    pub seed_available_slot: Option<u64>,
}

impl GenesisDuty {
    pub async fn plan<P: ChainPort, D: DrandPort>(
        &mut self,
        t: &Tick<'_>,
        port: &P,
        drand: &D,
        rounds: &mut Rounds,
        engine: &mut Engine,
    ) -> PortResult<()> {
        let s = t.season;
        let eff = s.effective_status(t.now);
        if t.cfg.has_role("beacon") && s.status == status::CREATED {
            let g = s.genesis_round;
            if t.clock.drand.round_time(g) <= t.now {
                if let Some(arg) = rounds.get(drand, g, t.slot).await {
                    self.seed_available_slot.get_or_insert(t.slot);
                    let a = t.addrs.clone();
                    engine.ensure(
                        WriteSpec {
                            key: GENESIS_SEED_KEY.into(),
                            kind: "genesis-seed",
                            tag: tag::CONSUME_GENESIS_SEED,
                            class: Class::D,
                            bell: None,
                            region: None,
                            build: Arc::new(move |c: &BuildCtx| {
                                vec![ix::consume_genesis_seed(&a, c.payer, &arg)]
                            }),
                            deadline_slot: None,
                            not_before_slot: 0,
                            fixed_payer: None,
                        },
                        t.slot,
                    );
                }
            }
        }
        if !t.cfg.has_role("rings")
            || self.rings_unsupported
            || self.rings_done
            || !(eff == status::SEEDED || eff == status::RUNNING)
        {
            return Ok(());
        }
        let g = s.genesis_ring as u16;
        let fr = port.accounts(&[t.addrs.frontier()], 0).await?;
        let Some(fr) = fr[0].as_ref().and_then(|a| Frontier::decode(&a.data).ok()) else {
            return Ok(());
        };
        let opened = fr.rings_opened;
        if opened <= g {
            let a = t.addrs.clone();
            let d = opened;
            engine.ensure(
                WriteSpec {
                    key: ring_key(d),
                    kind: "open-ring",
                    tag: tag::OPEN_RING,
                    class: Class::D,
                    bell: None,
                    region: None,
                    build: Arc::new(move |c: &BuildCtx| vec![ix::open_ring(&a, c.payer, d)]),
                    deadline_slot: None,
                    not_before_slot: 0,
                    fixed_payer: None,
                },
                t.slot,
            );
        }
        // Provinces of every opened genesis ring whose seed is set.
        let rings: Vec<u16> = (0..opened.min(g + 1)).collect();
        let seeds = port
            .accounts(
                &rings
                    .iter()
                    .map(|&d| t.addrs.ring_seed(d))
                    .collect::<Vec<Address>>(),
                0,
            )
            .await?;
        let mut want: Vec<(i32, i32)> = vec![];
        for (&d, acct) in rings.iter().zip(seeds) {
            let seeded = acct
                .and_then(|a| RingSeed::decode(&a.data).ok())
                .is_some_and(|r| r.status == 2);
            if !seeded {
                continue;
            }
            want.extend(
                ring_provinces(d as u32)
                    .into_iter()
                    .map(|p| (p.p, p.q))
                    .filter(|k| !self.opened.contains(k)),
            );
        }
        if !want.is_empty() {
            let keys: Vec<Address> = want.iter().map(|&(p, q)| t.addrs.province(p, q)).collect();
            let got = port.accounts(&keys, 0).await?;
            for (&(p, q), acct) in want.iter().zip(got) {
                if acct.is_some_and(|a| a.owner == t.addrs.program && !a.data.is_empty()) {
                    self.opened.insert((p, q));
                    engine.cancel(&province_key(p, q));
                    continue;
                }
                let a = t.addrs.clone();
                let (pi, qi) = (p as i16, q as i16);
                engine.ensure(
                    WriteSpec {
                        key: province_key(p, q),
                        kind: "open-province",
                        tag: tag::OPEN_PROVINCE,
                        class: Class::D,
                        bell: None,
                        region: Some(ix::region_of(p, q)),
                        build: Arc::new(move |c: &BuildCtx| {
                            vec![ix::open_province(&a, c.payer, pi, qi)]
                        }),
                        deadline_slot: None,
                        not_before_slot: 0,
                        fixed_payer: None,
                    },
                    t.slot,
                );
            }
        }
        let total: usize = (0..=g as u32).map(|d| ring_provinces(d).len()).sum();
        self.rings_done = opened > g && self.opened.len() == total;
        Ok(())
    }

    /// A write of this duty died with NotImplemented: the program has no
    /// land instructions yet.
    pub fn on_dead(&mut self, key: &str, code: Option<u32>) -> bool {
        if (key.starts_with("open-ring:") || key.starts_with("open-province:"))
            && code == Some(fclient::abi::err::NOT_IMPLEMENTED)
        {
            self.rings_unsupported = true;
            return true;
        }
        false
    }
}
