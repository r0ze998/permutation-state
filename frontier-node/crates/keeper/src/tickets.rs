//! Site tickets (M1 contract §8.2 "Tickets", §5.9 SettleTicket, I-47).
//! Role `tickets`, class D.
//!
//! Every open ticket is settled as soon as `S(ticket_bell, r_site)` exists,
//! **in descending score per site**: the tickets whose current preference
//! is the same site are ordered by `(score desc, citizen_tag asc)` (the
//! program's lottery order) and, while the site can still change hands
//! (free, or provisional in the same cohort), only the best one is in
//! flight; the next goes when it lands. So an honest cohort settles in the
//! first slots after S without a displacement, and one ticket per site
//! is sent per slot. Tickets whose site is already held for good (final,
//! another cohort, a better score) all go at once: each ends `taken` and
//! moves to its next preference (`ticket_next += 1`).
//!
//! A ticket whose current site holds a provisional holding **of the same
//! cohort with a lower score** — someone settled out of order, e.g. the
//! `ticket_holder` persona — is settled with the displaced holder's rent
//! payer, Citizen and JoinShard, so the higher score displaces it.
//!
//! A ticket is **expired** at `now_bell ≥ ticket_bell + 24`; the keeper
//! settles it (`expired`) so its cohort closes. Each write is keyed by
//! `(citizen, ticket_bell, k)`: a landed `taken` changes `k` and the next
//! plan settles the next preference.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use solana_address::Address;

use fclient::abi::{status, tag, Class};
use fclient::addr::citizen_tag_u64;
use fclient::decode::{Citizen, Holding, Province};
use fclient::ix::{self, Displaced, SeedSource, Site};
use fclient::land::{self, SiteOutcome, NO_TICKET};
use fclient::ports::{ChainPort, PortResult};

use crate::beacon::AnchorInfo;
use crate::engine::{BuildCtx, Engine, Outcome, WriteSpec};
use crate::landindex::LandIndex;
use crate::seeds::SeedFinder;
use crate::Tick;

pub fn ticket_key(citizen: &Address, ticket_bell: u32, k: u8) -> String {
    format!("ticket:{citizen}:{ticket_bell}:{k}")
}

/// One ticket ready to settle this tick.
#[derive(Clone, Debug)]
pub struct Candidate {
    pub citizen: Address,
    pub wallet: Address,
    pub faction: u8,
    pub tag: u64,
    pub ticket_bell: u32,
    pub k: u8,
    pub site: (i16, i16, u8),
    pub sites: Vec<(i16, i16, u8)>,
    pub score: u64,
    /// `S(ticket_bell, region(site))` (zero for an expired ticket).
    pub seed: [u8; 32],
    pub src: SeedSource,
    pub expired: bool,
    pub cache_slot: Option<u64>,
}

/// Slots a fresh settlement may be held back for a better same-cohort
/// ticket that is still moving towards its site (overlap rule, below).
pub const OVERLAP_HOLD_SLOTS: u64 = 300;

/// A settlement in flight.
#[derive(Clone, Debug)]
pub struct Planned {
    pub citizen: Address,
    pub ticket_bell: u32,
    pub k: u8,
    pub site: (i16, i16, u8),
    pub score: u64,
    pub predicted: &'static str,
    /// The last preference: a landing ends the ticket whatever the outcome.
    pub last: bool,
    pub cache_slot: Option<u64>,
    pub first_slot: u64,
}

/// A landed settlement (tests, status).
#[derive(Clone, Debug)]
pub struct Settled {
    pub citizen: Address,
    pub ticket_bell: u32,
    pub k: u8,
    pub site: (i16, i16, u8),
    pub score: u64,
    pub predicted: &'static str,
    pub slot: u64,
    pub cache_slot: Option<u64>,
}

/// Orders one site's candidates as the lottery does: score descending,
/// then the lower citizen tag.
pub fn lottery_order(v: &mut [Candidate]) {
    v.sort_by(|a, b| b.score.cmp(&a.score).then(a.tag.cmp(&b.tag)));
}

/// Orders one site's candidates for settlement (wave-3 review, W3-C):
/// the **oldest cohort first** (scores of different cohorts come from
/// different seeds and do not compare; a later cohort settled first would
/// make an earlier winner `taken`), then the lottery order.
pub fn settle_order(v: &mut [Candidate]) {
    v.sort_by(|a, b| {
        a.ticket_bell
            .cmp(&b.ticket_bell)
            .then(b.score.cmp(&a.score))
            .then(a.tag.cmp(&b.tag))
    });
}

/// Whether a fresh settlement of `c` at its site should wait (overlap
/// rule, wave-3 review): another open ticket `o` of the same cohort that
/// will not win its own current site (`movers`) has `c`'s site among its
/// later preferences with a better score there. Settling `c` first would
/// let `o`, once taken elsewhere, displace `c` (a displaced ticket ends);
/// waiting lets `o` take the site first and `c` settle `taken` and move
/// on. Bounded by [`OVERLAP_HOLD_SLOTS`] after the seed.
pub fn overlap_hold(c: &Candidate, movers: &[&Candidate], slot: u64) -> bool {
    if c.cache_slot.is_some_and(|s| slot >= s + OVERLAP_HOLD_SLOTS) {
        return false;
    }
    movers.iter().any(|o| {
        o.citizen != c.citizen
            && o.ticket_bell == c.ticket_bell
            && o.sites.iter().skip(o.k as usize + 1).any(|&s| s == c.site)
            && land::beats(
                land::ticket_score(&c.seed, c.site.0 as i32, c.site.1 as i32, c.site.2, o.tag),
                o.tag,
                c.score,
                c.tag,
            )
    })
}

#[derive(Default)]
pub struct TicketDuty {
    pub planned: BTreeMap<String, Planned>,
    pub settled: Vec<Settled>,
    /// Tickets this keeper ended by expiry.
    pub expired: u64,
    /// Settlements sent with a displaced holder.
    pub displacing: u64,
    /// Fresh settlements held back by the overlap rule (tick count).
    pub held: u64,
}

impl TicketDuty {
    #[allow(clippy::too_many_arguments)]
    pub async fn plan<P: ChainPort>(
        &mut self,
        t: &Tick<'_>,
        port: &P,
        engine: &mut Engine,
        index: &mut LandIndex,
        seeds: &mut SeedFinder,
        anchors: &BTreeMap<(u32, u8), AnchorInfo>,
    ) -> PortResult<()> {
        let s = t.season;
        if !t.cfg.has_role("tickets") || s.effective_status(t.now) != status::RUNNING {
            return Ok(());
        }
        let Some(now_bell) = t.clock.bell_at(t.now) else {
            return Ok(());
        };
        // Forget writes that ended.
        self.planned.retain(|k, _| engine.is_pending(k));
        if index.tickets.is_empty() {
            return Ok(());
        }
        // The citizens with an open ticket, from chain.
        let who: Vec<Address> = index.tickets.keys().copied().collect();
        let mut citizens: Vec<(Address, Citizen)> = vec![];
        for chunk in who.chunks(100) {
            let got = port.accounts(chunk, 0).await?;
            for (a, acct) in chunk.iter().zip(got) {
                match acct
                    .filter(|x| x.owner == t.addrs.program)
                    .and_then(|x| Citizen::decode(&x.data).ok())
                {
                    Some(c) if c.ticket_bell != NO_TICKET => citizens.push((*a, c)),
                    // Ended (settled, exhausted, expired) or the Citizen closed.
                    _ => {
                        let _ = index.tickets.remove(a);
                    }
                }
            }
        }
        let busy_sites: BTreeSet<(i16, i16, u8)> = self
            .planned
            .values()
            .filter(|p| p.predicted != "taken" && p.predicted != "expired")
            .map(|p| p.site)
            .collect();
        let mut ready: Vec<Candidate> = vec![];
        for (addr, c) in &citizens {
            if let Some(r) = index.tickets.get_mut(addr) {
                r.ticket_bell = c.ticket_bell;
            }
            let sites = land::ticket_sites(c);
            let k = c.ticket_next;
            let Some(&site) = sites.get(k as usize) else {
                continue;
            };
            let key = ticket_key(addr, c.ticket_bell, k);
            if engine.is_pending(&key) {
                continue;
            }
            let region = ix::region_of(site.0 as i32, site.1 as i32);
            let expired = land::ticket_expired(c.ticket_bell, now_bell);
            let seed = seeds
                .find(port, t.addrs, anchors, c.ticket_bell, region, t.slot)
                .await?;
            if seed.is_none() && !expired {
                continue;
            }
            let tag = citizen_tag_u64(addr);
            let score = seed.map_or(0, |x| {
                land::ticket_score(&x.seed, site.0 as i32, site.1 as i32, site.2, tag)
            });
            ready.push(Candidate {
                citizen: *addr,
                wallet: c.wallet,
                faction: c.faction,
                tag,
                ticket_bell: c.ticket_bell,
                k,
                site,
                sites,
                score,
                seed: seed.map_or([0; 32], |x| x.seed),
                // An expired ticket's settlement does not read the seed; the
                // canonical cache address of nonce 0 fills the position.
                src: seed.map_or(SeedSource::Cache { nonce: 0 }, |x| x.src),
                expired,
                cache_slot: seed.and_then(|x| x.cache_slot),
            });
        }
        if ready.is_empty() {
            return Ok(());
        }
        // Expired tickets: all at once.
        let (expired, live): (Vec<Candidate>, Vec<Candidate>) =
            ready.into_iter().partition(|c| c.expired);
        for c in expired {
            self.send(t, engine, &c, None, "expired");
        }
        // Live tickets grouped by their current site.
        let mut by_site: BTreeMap<(i16, i16, u8), Vec<Candidate>> = BTreeMap::new();
        for c in live {
            by_site.entry(c.site).or_default().push(c);
        }
        let sites: Vec<(i16, i16, u8)> = by_site.keys().copied().collect();
        let mut provs: BTreeMap<(i16, i16), Province> = BTreeMap::new();
        let mut holds: BTreeMap<(i16, i16, u8), Holding> = BTreeMap::new();
        {
            let pk: Vec<(i16, i16)> = sites
                .iter()
                .map(|s| (s.0, s.1))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            for chunk in pk.chunks(100) {
                let keys: Vec<Address> = chunk
                    .iter()
                    .map(|&(p, q)| t.addrs.province(p as i32, q as i32))
                    .collect();
                for (k, a) in chunk.iter().zip(port.accounts(&keys, 0).await?) {
                    if let Some(pv) = a
                        .filter(|x| x.owner == t.addrs.program)
                        .and_then(|x| Province::decode(&x.data).ok())
                    {
                        provs.insert(*k, pv);
                    }
                }
            }
            for chunk in sites.chunks(100) {
                let keys: Vec<Address> = chunk
                    .iter()
                    .map(|&(p, q, st)| t.addrs.holding(p as i32, q as i32, st))
                    .collect();
                for (k, a) in chunk.iter().zip(port.accounts(&keys, 0).await?) {
                    if let Some(h) = a
                        .filter(|x| x.owner == t.addrs.program)
                        .and_then(|x| Holding::decode(&x.data).ok())
                    {
                        holds.insert(*k, h);
                    }
                }
            }
        }
        // Per candidate, what SettleTicket would do against the site as the
        // chain shows it (wave-3 review: per candidate, not once per group).
        let mut plans: BTreeMap<(i16, i16, u8), Vec<(Candidate, SiteOutcome)>> = BTreeMap::new();
        for (site, mut group) in by_site {
            let Some(pv) = provs.get(&(site.0, site.1)) else {
                continue;
            };
            let Some(mirror) = pv.site_mirror.get(site.2 as usize) else {
                continue;
            };
            settle_order(&mut group);
            let holding = holds.get(&site);
            let holder_tag = holding.map(|h| citizen_tag_u64(&h.owner_citizen));
            let v = group
                .into_iter()
                .map(|c| {
                    let o = land::predict_site(
                        mirror.state,
                        holding,
                        c.ticket_bell,
                        c.score,
                        c.tag,
                        holder_tag,
                    );
                    (c, o)
                })
                .collect();
            plans.insert(site, v);
        }
        // Tickets that will not win their current site (taken there, or a
        // better candidate is ahead): they move on and may reach a later
        // preference another ticket is about to settle (overlap rule).
        let mut movers: Vec<Candidate> = vec![];
        for v in plans.values() {
            let mut first_live = true;
            for (c, o) in v {
                match o {
                    SiteOutcome::Taken => movers.push(c.clone()),
                    _ if !first_live => movers.push(c.clone()),
                    _ => first_live = false,
                }
            }
        }
        let mover_refs: Vec<&Candidate> = movers.iter().collect();
        for (site, v) in plans {
            // Held against every candidate: each is taken now (order-free).
            for (c, o) in &v {
                if *o == SiteOutcome::Taken {
                    self.send(t, engine, c, None, "taken");
                }
            }
            if busy_sites.contains(&site) {
                continue;
            }
            // The first candidate that can change the site's holder, in
            // settlement order (oldest cohort, then the lottery order).
            let Some((top, pred)) = v.iter().find(|(_, o)| *o != SiteOutcome::Taken) else {
                continue;
            };
            match pred {
                SiteOutcome::Taken => {}
                SiteOutcome::Fresh => {
                    if overlap_hold(top, &mover_refs, t.slot) {
                        self.held += 1;
                        continue;
                    }
                    self.send(t, engine, top, None, "fresh");
                }
                SiteOutcome::Displace {
                    holder_citizen,
                    holder_rent_payer,
                    ..
                } => {
                    // The displaced holder's wallet and faction, checked
                    // against the canonical address (never trusted from a
                    // feed record alone).
                    let Some((wallet, faction)) =
                        index.citizen_of(port, t.addrs, holder_citizen).await?
                    else {
                        continue;
                    };
                    let d = Displaced {
                        rent_payer: *holder_rent_payer,
                        wallet,
                        faction,
                    };
                    self.displacing += 1;
                    self.send(t, engine, top, Some(d), "displace");
                }
            }
        }
        Ok(())
    }

    fn send(
        &mut self,
        t: &Tick<'_>,
        engine: &mut Engine,
        c: &Candidate,
        displaced: Option<Displaced>,
        predicted: &'static str,
    ) {
        let key = ticket_key(&c.citizen, c.ticket_bell, c.k);
        let a = t.addrs.clone();
        let (wallet, faction, k, tb, src) = (c.wallet, c.faction, c.k, c.ticket_bell, c.src);
        let site = Site {
            p: c.site.0,
            q: c.site.1,
            site: c.site.2,
        };
        let all: Vec<Site> = c
            .sites
            .iter()
            .map(|&(p, q, s)| Site { p, q, site: s })
            .collect();
        let added = engine.ensure(
            WriteSpec {
                key: key.clone(),
                kind: "settle-ticket",
                tag: tag::SETTLE_TICKET,
                class: Class::D,
                bell: Some(c.ticket_bell),
                region: Some(ix::region_of(c.site.0 as i32, c.site.1 as i32)),
                build: Arc::new(move |b: &BuildCtx| {
                    vec![ix::settle_ticket(
                        &a,
                        b.payer,
                        &wallet,
                        faction,
                        k,
                        site,
                        tb,
                        &all,
                        src,
                        displaced.as_ref(),
                    )]
                }),
                deadline_slot: None,
                not_before_slot: 0,
                fixed_payer: None,
            },
            t.slot,
        );
        if added {
            self.planned.insert(
                key,
                Planned {
                    citizen: c.citizen,
                    ticket_bell: c.ticket_bell,
                    k: c.k,
                    site: c.site,
                    score: c.score,
                    predicted,
                    last: c.k as usize + 1 >= c.sites.len(),
                    cache_slot: c.cache_slot,
                    first_slot: t.slot,
                },
            );
        }
    }

    /// Routes an outcome of a `ticket:` write.
    pub fn on_outcome(&mut self, key: &str, o: &Outcome) {
        let Some(p) = self.planned.remove(key) else {
            return;
        };
        if let Outcome::Landed { slot, .. } = o {
            if p.predicted == "expired" {
                self.expired += 1;
            }
            self.settled.push(Settled {
                citizen: p.citizen,
                ticket_bell: p.ticket_bell,
                k: p.k,
                site: p.site,
                score: p.score,
                predicted: p.predicted,
                slot: *slot,
                cache_slot: p.cache_slot,
            });
        }
    }

    /// Slots from the seed cache's landing to each settlement that won its
    /// site (`fresh` or `displace`): the "first slots after S" of §8.2.
    pub fn winner_latency(&self) -> Vec<u64> {
        self.settled
            .iter()
            .filter(|s| s.predicted == "fresh" || s.predicted == "displace")
            .filter_map(|s| s.cache_slot.map(|c| s.slot.saturating_sub(c)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(tag: u64, score: u64) -> Candidate {
        Candidate {
            citizen: Address::new_from_array([tag as u8; 32]),
            wallet: Address::new_from_array([0; 32]),
            faction: 0,
            tag,
            ticket_bell: 5,
            k: 0,
            site: (2, 0, 1),
            sites: vec![(2, 0, 1)],
            score,
            seed: [7; 32],
            src: SeedSource::Cache { nonce: 1 },
            expired: false,
            cache_slot: None,
        }
    }

    #[test]
    fn lottery_order_is_score_desc_then_tag() {
        let mut v = vec![cand(3, 10), cand(1, 50), cand(2, 50), cand(4, 90)];
        lottery_order(&mut v);
        let tags: Vec<u64> = v.iter().map(|c| c.tag).collect();
        assert_eq!(tags, vec![4, 1, 2, 3]);
    }

    /// Oldest cohort first, then the lottery order (wave-3 review).
    #[test]
    fn settle_order_puts_the_oldest_cohort_first() {
        let mut late = cand(9, 99);
        late.ticket_bell = 6;
        let mut v = vec![late, cand(3, 10), cand(1, 50)];
        settle_order(&mut v);
        let tags: Vec<u64> = v.iter().map(|c| c.tag).collect();
        assert_eq!(tags, vec![1, 3, 9]);
    }

    /// The overlap rule: C (fresh at Y) waits while B, who will be taken at
    /// X, has Y later with a better Y-score; not for a worse one, another
    /// cohort, or past the hold bound.
    #[test]
    fn overlap_hold_waits_for_a_better_mover() {
        let y = (2, 0, 1);
        let x = (3, 0, 1);
        let mut c = cand(3, 0);
        c.site = y;
        c.sites = vec![y];
        c.cache_slot = Some(100);
        c.score = land::ticket_score(&c.seed, 2, 0, 1, c.tag);
        let mut b = cand(2, 0);
        b.site = x;
        b.sites = vec![x, y];
        let b_at_y = land::ticket_score(&c.seed, 2, 0, 1, b.tag);
        let held = overlap_hold(&c, &[&b], 110);
        assert_eq!(held, land::beats(b_at_y, b.tag, c.score, c.tag));
        // Force both directions with a crafted score.
        let mut c2 = c.clone();
        c2.score = 0;
        assert!(overlap_hold(&c2, &[&b], 110), "a better mover: hold");
        c2.score = u64::MAX;
        assert!(!overlap_hold(&c2, &[&b], 110), "a worse mover: go");
        let mut c3 = c.clone();
        c3.score = 0;
        assert!(
            !overlap_hold(&c3, &[&b], 100 + OVERLAP_HOLD_SLOTS),
            "bounded"
        );
        let mut b2 = b.clone();
        b2.ticket_bell = 9;
        assert!(!overlap_hold(&c3, &[&b2], 110), "another cohort");
        let mut b3 = b.clone();
        b3.k = 1;
        b3.site = y;
        assert!(
            !overlap_hold(&c3, &[&b3], 110),
            "Y is not a later preference"
        );
    }
}
