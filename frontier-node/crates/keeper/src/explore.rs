//! Explore settlement (M1 contract §8.2 "Explore", §5.10 SettleExplore,
//! I-56). Role `explore`, class N.
//!
//! An `EXPLORE` record names the host; its Holding carries the record
//! `{bell, P, Q, tiles, host, state}`. Once `S(record.bell, region(P, Q))`
//! exists, SettleExplore (anyone may send it) rolls the finds and clears
//! the record. The keeper watches every Holding an EXPLORE named until its
//! record is clear.

use std::collections::BTreeMap;
use std::sync::Arc;

use solana_address::Address;

use fclient::abi::{status, tag, Class};
use fclient::decode::{Citizen, Holding};
use fclient::ix::{self, HoldingRef};
use fclient::ports::{ChainPort, PortResult};
use frontier_abi::layout::player::explore as exl;

use crate::beacon::AnchorInfo;
use crate::engine::{BuildCtx, Engine, WriteSpec};
use crate::landindex::LandIndex;
use crate::seeds::SeedFinder;
use crate::Tick;

pub fn explore_key(holding: &Address, bell: u32, host: u64) -> String {
    format!("explore:{holding}:{bell}:{host}")
}

#[derive(Default)]
pub struct ExploreDuty {
    /// `(holding, bell, host)` of every settlement sent.
    pub sent: Vec<(Address, u32, u64)>,
    /// Records seen cleared (settled by anyone).
    pub cleared: u64,
}

impl ExploreDuty {
    pub async fn plan<P: ChainPort>(
        &mut self,
        t: &Tick<'_>,
        port: &P,
        engine: &mut Engine,
        index: &mut LandIndex,
        seeds: &mut SeedFinder,
        anchors: &BTreeMap<(u32, u8), AnchorInfo>,
    ) -> PortResult<()> {
        let eff = t.season.effective_status(t.now);
        if !t.cfg.has_role("explore")
            || !(eff == status::RUNNING || eff == status::ENDED)
            || index.explores.is_empty()
        {
            return Ok(());
        }
        let hs: Vec<Address> = index.explores.iter().copied().collect();
        for chunk in hs.chunks(100) {
            let got = port.accounts(chunk, 0).await?;
            for (addr, acct) in chunk.iter().zip(got) {
                let Some(h) = acct
                    .filter(|a| a.owner == t.addrs.program)
                    .and_then(|a| Holding::decode(&a.data).ok())
                else {
                    index.explores.remove(addr);
                    continue;
                };
                let ex = h.explore;
                if ex.state != exl::STATE_PENDING {
                    index.explores.remove(addr);
                    self.cleared += 1;
                    continue;
                }
                let key = explore_key(addr, ex.bell, ex.host);
                if engine.is_pending(&key) {
                    continue;
                }
                let region = ix::region_of(ex.p as i32, ex.q as i32);
                let Some(seed) = seeds
                    .find(port, t.addrs, anchors, ex.bell, region, t.slot)
                    .await?
                else {
                    continue;
                };
                let wallet = match index.citizens.get(&h.owner_citizen) {
                    Some(x) => x.0,
                    None => {
                        let got = port.accounts(&[h.owner_citizen], 0).await?;
                        match got[0].as_ref().and_then(|a| Citizen::decode(&a.data).ok()) {
                            Some(c) => {
                                index
                                    .citizens
                                    .insert(h.owner_citizen, (c.wallet, c.faction));
                                c.wallet
                            }
                            None => continue,
                        }
                    }
                };
                let a = t.addrs.clone();
                let href = HoldingRef {
                    p: h.p,
                    q: h.q,
                    site: h.site,
                };
                let (bell, src) = (ex.bell, seed.src);
                if engine.ensure(
                    WriteSpec {
                        key,
                        kind: "settle-explore",
                        tag: tag::SETTLE_EXPLORE,
                        class: Class::N,
                        bell: Some(ex.bell),
                        region: Some(region),
                        build: Arc::new(move |c: &BuildCtx| {
                            vec![ix::settle_explore(
                                &a, c.payer, href, &wallet, bell, region, src,
                            )]
                        }),
                        deadline_slot: None,
                        not_before_slot: 0,
                        fixed_payer: None,
                    },
                    t.slot,
                ) {
                    self.sent.push((*addr, ex.bell, ex.host));
                }
            }
        }
        Ok(())
    }
}
