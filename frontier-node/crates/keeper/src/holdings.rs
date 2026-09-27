//! Dormancy, stranded hosts and pool sweeps (M1 contract §8.2
//! "Dormancy" and "Sweeps", §5.9 ReleaseDormant, §5.10 DisbandStranded,
//! §5.12 SweepPoolOwed, I-48). All class N.
//!
//! - **Scan** (every `land_scan_slots`, and at once for a Holding a
//!   `TRANSIT_SETTLED` record says owes the pool): every Holding the feed
//!   named is read.
//! - **ReleaseDormant** (role `dormancy`): order 1, `now ≥
//!   last_owner_action + release_after`, no transit in state 1–3.
//! - **SweepPoolOwed** (role `sweep`): `pool_owed > 0` (routed tips, fees
//!   and diverted payments that stay in the Holding until swept, so no W or
//!   D instruction writes the DefencePool).
//! - **DisbandStranded** (role `dormancy`): an entry of any opened Province
//!   whose host id names a Holding that is absent or of another `gen` (a
//!   released or re-founded site). The Provinces are scanned when a
//!   Holding disappears (RELEASE, or seen absent) and every
//!   `stranded_scan_slots` as a backstop.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use solana_address::Address;

use fclient::abi::{status, tag, Class};
use fclient::addr::host_parts;
use fclient::decode::{Holding, Province};
use fclient::ix::{self, HoldingRef};
use fclient::land;
use fclient::ports::{ChainPort, PortResult};

use crate::engine::{BuildCtx, Engine, WriteSpec};
use crate::landindex::LandIndex;
use crate::Tick;

pub fn release_key(h: &Address, gen: u8) -> String {
    format!("release:{h}:{gen}")
}
pub fn sweep_key(h: &Address, owed: u64) -> String {
    format!("sweep:{h}:{owed}")
}
/// Entry pending op 6: Forfeit (a bad seal, or a disbanded stranded host).
pub const OP_FORFEIT: u8 = 6;

pub fn stranded_key(p: i16, q: i16, host: u64) -> String {
    format!("stranded:{p},{q}:{host}")
}

#[derive(Default)]
pub struct HoldingDuty {
    last_scan: Option<u64>,
    last_province_scan: Option<u64>,
    province_scan_due: bool,
    /// Writes sent, by kind (status, tests).
    pub releases: Vec<Address>,
    pub sweeps: Vec<(Address, u64)>,
    pub disbands: Vec<(i16, i16, u64)>,
}

fn simple(
    key: String,
    kind: &'static str,
    t: u8,
    build: impl Fn(&BuildCtx) -> Vec<solana_instruction::Instruction> + Send + Sync + 'static,
) -> WriteSpec {
    WriteSpec {
        key,
        kind,
        tag: t,
        class: Class::N,
        bell: None,
        region: None,
        build: Arc::new(build),
        deadline_slot: None,
        not_before_slot: 0,
        fixed_payer: None,
    }
}

impl HoldingDuty {
    #[allow(clippy::too_many_arguments)]
    pub async fn plan<P: ChainPort>(
        &mut self,
        t: &Tick<'_>,
        port: &P,
        engine: &mut Engine,
        index: &mut LandIndex,
        provinces: &BTreeSet<(i32, i32)>,
    ) -> PortResult<()> {
        let dormancy = t.cfg.has_role("dormancy");
        let sweep = t.cfg.has_role("sweep");
        let eff = t.season.effective_status(t.now);
        if !(dormancy || sweep) || !(eff == status::RUNNING || eff == status::ENDED) {
            return Ok(());
        }
        if !index.released.is_empty() {
            self.province_scan_due = true;
            index.released.clear();
        }
        let scan = self
            .last_scan
            .is_none_or(|s| t.slot >= s + t.cfg.land_scan_slots)
            || (sweep && !index.owes.is_empty());
        if scan {
            self.last_scan = Some(t.slot);
            let mut hs: BTreeSet<Address> = index
                .holdings
                .iter()
                .map(|&(p, q, s)| t.addrs.holding(p as i32, q as i32, s))
                .collect();
            hs.extend(index.owes.iter().copied());
            index.owes.clear();
            let hs: Vec<Address> = hs.into_iter().collect();
            let mut need_citizen: Vec<(Address, Holding)> = vec![];
            for chunk in hs.chunks(100) {
                let got = port.accounts(chunk, 0).await?;
                for (addr, acct) in chunk.iter().zip(got) {
                    let Some(h) = acct
                        .filter(|a| a.owner == t.addrs.program)
                        .and_then(|a| Holding::decode(&a.data).ok())
                    else {
                        // Gone (released or closed): its hosts may be stranded.
                        if index
                            .holdings
                            .iter()
                            .any(|&(p, q, s)| t.addrs.holding(p as i32, q as i32, s) == *addr)
                        {
                            index.holdings.retain(|&(p, q, s)| {
                                t.addrs.holding(p as i32, q as i32, s) != *addr
                            });
                            self.province_scan_due = true;
                        }
                        continue;
                    };
                    let href = HoldingRef {
                        p: h.p,
                        q: h.q,
                        site: h.site,
                    };
                    if sweep && h.pool_owed > 0 {
                        let a = t.addrs.clone();
                        if engine.ensure(
                            simple(
                                sweep_key(addr, h.pool_owed),
                                "sweep",
                                tag::SWEEP_POOL_OWED,
                                move |c| vec![ix::sweep_pool_owed(&a, c.payer, href)],
                            ),
                            t.slot,
                        ) {
                            self.sweeps.push((*addr, h.pool_owed));
                        }
                    }
                    if dormancy
                        && eff == status::RUNNING
                        && land::releasable(&h, t.season, t.now)
                        && !engine.is_pending(&release_key(addr, h.gen))
                    {
                        need_citizen.push((*addr, h));
                    }
                }
            }
            for (addr, h) in need_citizen {
                let Some((wallet, faction)) =
                    index.citizen_of(port, t.addrs, &h.owner_citizen).await?
                else {
                    continue;
                };
                let a = t.addrs.clone();
                let href = HoldingRef {
                    p: h.p,
                    q: h.q,
                    site: h.site,
                };
                let rent_payer = h.rent_payer;
                if engine.ensure(
                    simple(
                        release_key(&addr, h.gen),
                        "release-dormant",
                        tag::RELEASE_DORMANT,
                        move |c| {
                            vec![ix::release_dormant(
                                &a, c.payer, href, &wallet, faction, rent_payer,
                            )]
                        },
                    ),
                    t.slot,
                ) {
                    self.releases.push(addr);
                }
            }
        }
        // Stranded hosts.
        let backstop = self
            .last_province_scan
            .is_none_or(|s| t.slot >= s + t.cfg.stranded_scan_slots);
        if dormancy && (self.province_scan_due || backstop) && !provinces.is_empty() {
            self.province_scan_due = false;
            self.last_province_scan = Some(t.slot);
            self.scan_provinces(t, port, engine, provinces).await?;
        }
        Ok(())
    }

    async fn scan_provinces<P: ChainPort>(
        &mut self,
        t: &Tick<'_>,
        port: &P,
        engine: &mut Engine,
        provinces: &BTreeSet<(i32, i32)>,
    ) -> PortResult<()> {
        // (province, entry index, host id) of every occupied entry.
        let mut entries: Vec<((i16, i16), u8, u64)> = vec![];
        let pv: Vec<(i32, i32)> = provinces.iter().copied().collect();
        for chunk in pv.chunks(100) {
            let keys: Vec<Address> = chunk.iter().map(|&(p, q)| t.addrs.province(p, q)).collect();
            for (&(p, q), a) in chunk.iter().zip(port.accounts(&keys, 0).await?) {
                let Some(prov) = a
                    .filter(|x| x.owner == t.addrs.program)
                    .and_then(|x| Province::decode(&x.data).ok())
                else {
                    continue;
                };
                for (i, e) in prov.entries.iter().enumerate() {
                    // v1.5 §5.10: a disband already issued is a pending
                    // Forfeit until the province resolves its bell (a
                    // second one would be refused HostBusy).
                    if e.state != 0 && e.id != 0 && e.pend_op != OP_FORFEIT {
                        entries.push(((p as i16, q as i16), i as u8, e.id));
                    }
                }
            }
        }
        // The owner Holding of each host, by address.
        let mut owners: BTreeMap<Address, u8> = BTreeMap::new();
        for &(_, _, id) in &entries {
            if let (Ok(a), Ok((_, _, _, gen, _))) = (t.addrs.holding_of_host(id), host_parts(id)) {
                owners.insert(a, gen);
            }
        }
        let keys: Vec<Address> = owners.keys().copied().collect();
        let mut gen_of: BTreeMap<Address, Option<u8>> = BTreeMap::new();
        for chunk in keys.chunks(100) {
            for (k, a) in chunk.iter().zip(port.accounts(chunk, 0).await?) {
                let g = a
                    .filter(|x| x.owner == t.addrs.program)
                    .and_then(|x| Holding::decode(&x.data).ok())
                    .map(|h| h.gen);
                gen_of.insert(*k, g);
            }
        }
        for ((p, q), i, id) in entries {
            let (Ok(ha), Ok((_, _, _, gen, _))) = (t.addrs.holding_of_host(id), host_parts(id))
            else {
                continue;
            };
            let stranded = match gen_of.get(&ha) {
                Some(Some(g)) => *g != gen,
                _ => true,
            };
            if !stranded {
                continue;
            }
            let a = t.addrs.clone();
            if engine.ensure(
                simple(
                    stranded_key(p, q, id),
                    "disband-stranded",
                    tag::DISBAND_STRANDED,
                    move |c| vec![ix::disband_stranded(&a, c.payer, p, q, i, id)],
                ),
                t.slot,
            ) {
                self.disbands.push((p, q, id));
            }
        }
        Ok(())
    }
}
