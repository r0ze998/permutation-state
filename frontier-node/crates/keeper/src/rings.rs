//! Rings and provinces (M1 contract §8.2 "Genesis" and "Rings", §5.9,
//! I-30, I-46, I-48). Role `rings`, class D.
//!
//! - **OpenRing(d)** for `d = Frontier.rings_opened`: a genesis ring
//!   (`d ≤ g`) opens as soon as the season is Seeded (seeded at open, no
//!   ConsumeRingSeed); a crowding ring (`d > g`) opens when the rule of
//!   §5.9 holds on the **folded** values ([`fclient::land::ring_open_check`]:
//!   Running, one bell since the last opening, some wedge crowded past θ,
//!   every wedge fund able to pay `d` provinces).
//! - **ConsumeRingSeed(d)** once the RingSeed's round
//!   (`ring_seed_round(t_open)`) is published.
//! - **OpenProvince** for every province of every seeded ring not yet on
//!   chain (6d per ring; the Concord is ring 0).
//!
//! A program that answers `NotImplemented` (99) for a land instruction
//! ends this duty with an alert (the W2 program); the beacon duties keep
//! running.

use std::collections::BTreeSet;
use std::sync::Arc;

use solana_address::Address;

use fclient::abi::{status, tag, Class};
use fclient::decode::{Frontier, RingSeed};
use fclient::ix;
use fclient::land::{ring_open_check, RingWait};
use fclient::ports::{ChainPort, DrandPort, PortResult};
use permutation_rules::frontier::geometry::ring_provinces;

use crate::engine::{BuildCtx, Engine, WriteSpec};
use crate::rounds::Rounds;
use crate::Tick;

pub fn ring_key(d: u16) -> String {
    format!("open-ring:{d}")
}
pub fn ring_seed_key(d: u16) -> String {
    format!("ring-seed:{d}")
}
pub fn province_key(p: i32, q: i32) -> String {
    format!("open-province:{p},{q}")
}

#[derive(Default)]
pub struct RingDuty {
    /// Provinces seen present on chain.
    pub opened: BTreeSet<(i32, i32)>,
    /// Rings whose every province is present.
    pub complete: BTreeSet<u16>,
    /// The program has no land instructions (99 on a ring or province write).
    pub unsupported: bool,
    /// Every genesis ring opened and every genesis province created.
    pub genesis_done: bool,
    /// Why the next crowding ring waits (status).
    pub waiting: Option<RingWait>,
    /// `(d, slot)` when this keeper first saw ring d opened / seeded.
    pub opened_at: Vec<(u16, u64)>,
    pub seeded_at: Vec<(u16, u64)>,
    seen_open: BTreeSet<u16>,
    seen_seeded: BTreeSet<u16>,
}

impl RingDuty {
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
        if !t.cfg.has_role("rings")
            || self.unsupported
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
        for d in 0..opened {
            if self.seen_open.insert(d) {
                self.opened_at.push((d, t.slot));
            }
        }
        // The next ring.
        let d = opened;
        let ready = if d <= g {
            self.waiting = None;
            true
        } else if d <= s.r_max {
            let funds = port.accounts(&t.addrs.province_funds(), 0).await?;
            let lam: [u64; 6] =
                core::array::from_fn(|w| funds[w].as_ref().map_or(0, |a| a.lamports));
            match ring_open_check(s, &fr, &lam, t.now) {
                Ok(()) => {
                    self.waiting = None;
                    true
                }
                Err(w) => {
                    self.waiting = Some(w);
                    false
                }
            }
        } else {
            self.waiting = Some(RingWait::AtMax);
            false
        };
        if ready {
            let a = t.addrs.clone();
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
        // Seeds and provinces of the opened rings not complete yet.
        let rings: Vec<u16> = (0..opened).filter(|d| !self.complete.contains(d)).collect();
        if rings.is_empty() {
            self.genesis_done = opened > g;
            return Ok(());
        }
        let seeds = port
            .accounts(
                &rings
                    .iter()
                    .map(|&d| t.addrs.ring_seed(d))
                    .collect::<Vec<Address>>(),
                0,
            )
            .await?;
        let mut want: Vec<(u16, i32, i32)> = vec![];
        for (&d, acct) in rings.iter().zip(seeds) {
            let Some(rs) = acct.and_then(|a| RingSeed::decode(&a.data).ok()) else {
                continue;
            };
            match rs.status {
                // Requested: consume the ring seed once its round is out.
                1 if t.clock.drand.round_time(rs.round) <= t.now => {
                    if let Some(arg) = rounds.get(drand, rs.round, t.slot).await {
                        let a = t.addrs.clone();
                        engine.ensure(
                            WriteSpec {
                                key: ring_seed_key(d),
                                kind: "ring-seed",
                                tag: tag::CONSUME_RING_SEED,
                                class: Class::D,
                                bell: None,
                                region: None,
                                build: Arc::new(move |c: &BuildCtx| {
                                    vec![ix::consume_ring_seed(&a, c.payer, d, &arg)]
                                }),
                                deadline_slot: None,
                                not_before_slot: 0,
                                fixed_payer: None,
                            },
                            t.slot,
                        );
                    }
                }
                2 => {
                    if self.seen_seeded.insert(d) {
                        self.seeded_at.push((d, t.slot));
                    }
                    engine.cancel(&ring_seed_key(d));
                    want.extend(
                        ring_provinces(d as u32)
                            .into_iter()
                            .map(|p| (d, p.p, p.q))
                            .filter(|&(_, p, q)| !self.opened.contains(&(p, q))),
                    );
                    if want.iter().all(|w| w.0 != d) {
                        self.complete.insert(d);
                    }
                }
                _ => {}
            }
        }
        if !want.is_empty() {
            let keys: Vec<Address> = want
                .iter()
                .map(|&(_, p, q)| t.addrs.province(p, q))
                .collect();
            let mut got = vec![];
            for chunk in keys.chunks(100) {
                got.extend(port.accounts(chunk, 0).await?);
            }
            let mut missing: BTreeSet<u16> = BTreeSet::new();
            for (&(d, p, q), acct) in want.iter().zip(got) {
                if acct.is_some_and(|a| a.owner == t.addrs.program && !a.data.is_empty()) {
                    self.opened.insert((p, q));
                    engine.cancel(&province_key(p, q));
                    continue;
                }
                missing.insert(d);
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
            for &(d, _, _) in &want {
                if !missing.contains(&d) {
                    self.complete.insert(d);
                }
            }
        }
        self.genesis_done = opened > g && (0..=g).all(|d| self.complete.contains(&d));
        Ok(())
    }

    /// A write of this duty died with NotImplemented: the program has no
    /// land instructions yet.
    pub fn on_dead(&mut self, key: &str, code: Option<u32>) -> bool {
        if (key.starts_with("open-ring:")
            || key.starts_with("ring-seed:")
            || key.starts_with("open-province:"))
            && code == Some(fclient::abi::err::NOT_IMPLEMENTED)
        {
            self.unsupported = true;
            return true;
        }
        false
    }

    /// Provinces of every ring `0..=d_max` (the genesis count check).
    pub fn provinces_of_rings(d_max: u16) -> usize {
        (0..=d_max as u32).map(|d| ring_provinces(d).len()).sum()
    }
}
