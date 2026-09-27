//! Discovery of land objects from the program's transaction feed (M1
//! contract §6, §8.1 `feed`).
//!
//! The chain is the state: every duty reads the accounts themselves before
//! it plans a write. The PS2 records of **successful** transactions only
//! tell the keeper *which* accounts exist, since `ChainPort` has no
//! program-account scan:
//!
//! | record | what it adds |
//! |---|---|
//! | `JOIN` | citizen → (wallet, faction) |
//! | `TICKET` | the citizen has an open ticket (`ticket_bell`, n) |
//! | `SETTLE` fresh / displace | the holding `(P, Q, site)` exists |
//! | `EXPLORE` | the host's holding has an explore record to settle |
//! | `RELEASE` | the holding is gone: its hosts may be stranded |
//! | `TRANSIT_SETTLED` with a `pool_owed` delta | the holding owes the pool |
//! | `PROVINCE_OPEN` | the province exists (stranded-host scan) |
//!
//! A restarted keeper re-reads the feed from the start (cursor 0): the
//! index is small and rebuilt, never trusted over the chain.

use std::collections::{BTreeMap, BTreeSet};

use solana_address::Address;

use fclient::addr::{Addresses, SeedKind};
use fclient::ports::{ChainPort, Cursor, PortResult};
use frontier_abi::log::{self as plog, Kind};

/// Pages read per tick at most (catch-up after a restart spreads over ticks).
const PAGES_PER_TICK: usize = 8;

/// An open ticket as its TICKET record announced it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TicketRec {
    pub ticket_bell: u32,
    pub n: u8,
    /// Slot of the TICKET record.
    pub slot: u64,
}

#[derive(Default)]
pub struct LandIndex {
    pub cursor: Cursor,
    /// citizen address → (wallet, faction) from JOIN.
    pub citizens: BTreeMap<Address, (Address, u8)>,
    /// Citizens with a ticket filed and not yet seen ended.
    pub tickets: BTreeMap<Address, TicketRec>,
    /// Holdings `(P, Q, site)` founded (fresh or displace).
    pub holdings: BTreeSet<(i16, i16, u8)>,
    /// Holdings with an explore record filed (by host id → holding).
    pub explores: BTreeSet<Address>,
    /// Holdings released (RELEASE) and not yet swept of stranded hosts.
    pub released: BTreeSet<(i16, i16, u8)>,
    /// Holdings with a positive `pool_owed` delta seen.
    pub owes: BTreeSet<Address>,
    /// Provinces opened.
    pub provinces: BTreeSet<(i16, i16)>,
    /// Records ingested, by kind (status).
    pub counts: BTreeMap<&'static str, u64>,
    /// Records that did not decode.
    pub bad: u64,
}

fn fld<'a>(r: &plog::Record<'a>, name: &str) -> Option<&'a [u8]> {
    if let Some((o, w)) = plog::field(r.kind, name, false) {
        return r.key.get(o..o + w);
    }
    let (o, w) = plog::field(r.kind, name, true)?;
    r.payload.get(o..o + w)
}

fn le_u64(b: &[u8]) -> u64 {
    let mut a = [0u8; 8];
    a[..b.len().min(8)].copy_from_slice(&b[..b.len().min(8)]);
    u64::from_le_bytes(a)
}

fn le_i32(b: &[u8]) -> i32 {
    i32::from_le_bytes(b.try_into().unwrap_or([0; 4]))
}

impl LandIndex {
    /// The wallet and faction of `citizen`, trusted only when the wallet's
    /// canonical Citizen address is `citizen`; otherwise (or when unknown)
    /// read from the Citizen account and checked the same way (wave-3
    /// review, W3-C: a feed record alone never names a displaced holder).
    pub async fn citizen_of<P: ChainPort>(
        &mut self,
        port: &P,
        addrs: &Addresses,
        citizen: &Address,
    ) -> PortResult<Option<(Address, u8)>> {
        if let Some(&(w, f)) = self.citizens.get(citizen) {
            if addrs.citizen(&w) == *citizen {
                return Ok(Some((w, f)));
            }
        }
        let got = port.accounts(&[*citizen], 0).await?;
        let hc = got
            .first()
            .and_then(|a| a.as_ref())
            .filter(|a| a.owner == addrs.program)
            .and_then(|a| fclient::decode::Citizen::decode(&a.data).ok());
        match hc {
            Some(c) if addrs.citizen(&c.wallet) == *citizen => {
                self.citizens.insert(*citizen, (c.wallet, c.faction));
                Ok(Some((c.wallet, c.faction)))
            }
            _ => {
                self.citizens.remove(citizen);
                Ok(None)
            }
        }
    }

    /// Reads new feed records (at most a few pages per tick).
    pub async fn pull<P: ChainPort>(&mut self, port: &P, addrs: &Addresses) -> PortResult<usize> {
        let mut n = 0;
        for _ in 0..PAGES_PER_TICK {
            let page = port.feed(self.cursor).await?;
            let Some(last) = page.last() else { break };
            self.cursor = Cursor(last.seq);
            for tx in &page {
                if tx.err.is_some() {
                    continue;
                }
                let Ok(bodies) = fclient::log::bodies_from_logs(&tx.logs, &addrs.program) else {
                    self.bad += 1;
                    continue;
                };
                for b in bodies {
                    self.ingest(&b, tx.slot, addrs);
                    n += 1;
                }
            }
        }
        Ok(n)
    }

    /// Ingests one PS2 body.
    pub fn ingest(&mut self, body: &[u8], slot: u64, addrs: &Addresses) {
        let Ok(r) = plog::decode(body) else {
            self.bad += 1;
            return;
        };
        *self.counts.entry(r.kind.spec().name).or_default() += 1;
        let pqs = |r: &plog::Record| -> Option<(i16, i16, u8)> {
            Some((
                le_i32(fld(r, "p")?) as i16,
                le_i32(fld(r, "q")?) as i16,
                *fld(r, "site")?.first()?,
            ))
        };
        match r.kind {
            Kind::JOIN => {
                let (Some(tag), Some(wallet), Some(f)) = (
                    fld(&r, "citizen_tag15"),
                    fld(&r, "wallet"),
                    fld(&r, "faction"),
                ) else {
                    return;
                };
                let c = addrs.of(SeedKind::Citizen, tag);
                let w = Address::new_from_array(wallet.try_into().unwrap_or([0; 32]));
                // The key must be the wallet's canonical Citizen (§3.3: a
                // reader recomputes every keyed address it trusts).
                if addrs.citizen(&w) != c {
                    self.bad += 1;
                    return;
                }
                self.citizens.insert(c, (w, f[0]));
            }
            Kind::TICKET => {
                let (Some(tag), Some(tb), Some(n)) = (
                    fld(&r, "citizen_tag15"),
                    fld(&r, "ticket_bell"),
                    fld(&r, "n"),
                ) else {
                    return;
                };
                let c = addrs.of(SeedKind::Citizen, tag);
                self.tickets.insert(
                    c,
                    TicketRec {
                        ticket_bell: le_u64(tb) as u32,
                        n: n[0],
                        slot,
                    },
                );
            }
            Kind::SETTLE => {
                let Some(k) = pqs(&r) else { return };
                let outcome = fld(&r, "outcome").and_then(|o| o.first().copied());
                if matches!(
                    outcome,
                    Some(plog::settle_outcome::FRESH) | Some(plog::settle_outcome::DISPLACE)
                ) {
                    self.holdings.insert(k);
                    self.released.remove(&k);
                }
            }
            Kind::EXPLORE => {
                let Some(h) = fld(&r, "host_id") else { return };
                if let Ok(a) = addrs.holding_of_host(le_u64(h)) {
                    self.explores.insert(a);
                }
            }
            Kind::RELEASE => {
                if let Some(k) = pqs(&r) {
                    self.holdings.remove(&k);
                    self.released.insert(k);
                }
            }
            Kind::TRANSIT_SETTLED => {
                let (Some(h), Some(d)) = (fld(&r, "host_id"), fld(&r, "pool_owed_delta")) else {
                    return;
                };
                if le_u64(d) > 0 {
                    if let Ok(a) = addrs.holding_of_host(le_u64(h)) {
                        self.owes.insert(a);
                    }
                }
            }
            Kind::PROVINCE_OPEN => {
                if let (Some(p), Some(q)) = (fld(&r, "p"), fld(&r, "q")) {
                    self.provinces.insert((le_i32(p) as i16, le_i32(q) as i16));
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(kind: Kind, key: &[u8], payload: &[u8]) -> Vec<u8> {
        let mut b = vec![0u8; 512];
        let n = plog::write_body(kind, 5, key, payload, &mut b).unwrap();
        b.truncate(n);
        b.push(0); // no links
        b
    }

    #[test]
    fn records_feed_the_index() {
        let a = Addresses::new(Address::new_from_array([9; 32]), 3);
        let mut ix = LandIndex::default();
        let wallet = Address::new_from_array([4; 32]);
        let tag = fclient::addr::citizen_tag15(&wallet.to_bytes());
        let mut join = wallet.to_bytes().to_vec();
        join.extend_from_slice(&[2, 1]);
        join.extend_from_slice(&[0; 32]);
        join.extend_from_slice(&0i64.to_le_bytes());
        ix.ingest(&body(Kind::JOIN, &tag, &join), 1, &a);
        let c = a.citizen(&wallet);
        assert_eq!(ix.citizens.get(&c), Some(&(wallet, 2)));
        let mut t = 77u32.to_le_bytes().to_vec();
        t.push(2);
        t.extend_from_slice(&[0; 15 + 8 + 32]);
        ix.ingest(&body(Kind::TICKET, &tag, &t), 9, &a);
        assert_eq!(ix.tickets[&c].ticket_bell, 77);
        assert_eq!(ix.tickets[&c].n, 2);
        let mut key = (-2i32).to_le_bytes().to_vec();
        key.extend_from_slice(&3i32.to_le_bytes());
        key.push(4);
        let mut s = vec![plog::settle_outcome::FRESH];
        s.extend_from_slice(&[0; 8 + 8 + 8 + 1 + 8 + 4]);
        ix.ingest(&body(Kind::SETTLE, &key, &s), 10, &a);
        assert!(ix.holdings.contains(&(-2, 3, 4)));
        ix.ingest(&body(Kind::RELEASE, &key, &[0; 8]), 11, &a);
        assert!(!ix.holdings.contains(&(-2, 3, 4)));
        assert!(ix.released.contains(&(-2, 3, 4)));
        let host = fclient::addr::host_id(-2, 3, 4, 0, 1).unwrap();
        let mut ex = vec![0u8; 4 + 4 + 1 + 2];
        ex[8] = 1;
        ix.ingest(&body(Kind::EXPLORE, &host.to_le_bytes(), &ex), 12, &a);
        assert!(ix.explores.contains(&a.holding(-2, 3, 4)));
        ix.ingest(&[1, 2, 3], 13, &a);
        assert_eq!(ix.bad, 1);
        // A JOIN whose wallet is not the key's citizen is refused (a forged
        // record must not overwrite the real one).
        let thief = Address::new_from_array([5; 32]);
        let mut forged = thief.to_bytes().to_vec();
        forged.extend_from_slice(&[2, 1]);
        forged.extend_from_slice(&[0; 32]);
        forged.extend_from_slice(&0i64.to_le_bytes());
        ix.ingest(&body(Kind::JOIN, &tag, &forged), 14, &a);
        assert_eq!(ix.citizens.get(&c), Some(&(wallet, 2)));
        assert_eq!(ix.bad, 2);
    }
}
