//! `keeper::land_season` (Gate W3 "keeper land tests over `localnet`", M1
//! contract §8.2 land duties, §5.9, §5.10, §5.12, I-47, I-48): the keeper's
//! land roles over `localnet` in process (20×, virtual time, the
//! test-beacon key), against the program when `PSF_FRONTIER_SO` names one
//! (W3-A/W3-B's land; a program without it is reported, not failed) or
//! else the native model of `tests/model` (its land part is
//! `tests/model/land.rs`; the test prints which).
//!
//! One land scenario, the keeper's part checked at each step:
//!
//! 1. **Genesis rings** 0–3 and their 37 provinces (the ring duty).
//! 2. **A ticket cohort** of one bell: three citizens contest one site
//!    (each with a distinct fallback), the others take a site each. The
//!    keeper settles in descending score per site, so the lottery winner
//!    takes the contested site **without a displacement**, the losers end
//!    `taken` there and found their fallbacks; every winner lands within a
//!    few slots of the seed cache; every cohort record closes.
//! 3. **Explore**: a final holding musters a host (model: simplified
//!    Muster) and explores; the keeper settles the record once
//!    `S(record.bell, r)` exists (the floor's 4 Works).
//! 4. **Sweep**: a Holding's `pool_owed` (a fixture: routed tips are
//!    W4-B's SettleTransit) is swept into the DefencePool.
//! 5. **Fold** every bell while joins are open: the folded
//!    `occupied_sites`, `wedge_occupied`, `wedge_open` equal the shards'
//!    and funds' live counts.
//! 6. **A crowding ring**: wedge 0 is filled to θ, so OpenRing(4) lands on
//!    the folded values, ConsumeRingSeed(4) at `ring_seed_round(t_open)`,
//!    then OpenProvince × 24; once the fold counts the new sites the rule
//!    stops (`NotCrowded`) and no ring opens.
//! 7. **A displacement** (`ticket_holder`): a lower-score ticket of a
//!    second cohort is settled first by someone else; the keeper settles
//!    the higher one with the displaced holder's accounts and it displaces
//!    (`gen + 1`, the displaced Citizen reverts, its rent payer repaid).
//! 8. **Dormancy**: from the feed, every holding released had been idle
//!    `≥ release_after` (1 game hour in this season) and at most two bells
//!    more; the holding whose owner acts every 25 game minutes is never
//!    released; the released explorer's host is disbanded as stranded.
//! 9. **Expiry**: a ticket nobody settled for 24 bells is settled
//!    `expired` and its cohort closes.
//!
//! **Resident actions** (the explore host's Muster in step 3, the
//! keep-alive owner's Musters, the stranded host of step 8) need the
//! province resolved through `b − 2`, which on the real program only
//! W4-A's SkipQuiet/ResolveFromInputs advance. When the first Muster is
//! refused `NotResident` (26) the run records it and goes on without
//! them: steps 1, 2 and 4–9 (sweep, fold, crowding ring, displacement,
//! dormancy release, expiry) run on the program too (wave-3 review,
//! W3-C). Every run asserts that no write failed except with an expected
//! code.

mod common;
mod model;

use std::collections::BTreeMap;

use fclient::abi::layout as l;
use fclient::abi::status;
use fclient::addr::citizen_tag_u64;
use fclient::decode::{Citizen, Frontier, Holding, JoinShard, Province, RingSeed};
use fclient::ix::{self, HoldingRef, Site};
use fclient::land;
use fclient::ports::{ChainPort, Cursor};
use fclient::{Address, Keypair, Signer};
use frontier_abi::log::{self as plog, Kind};
use keeper_core::{quantile, Keeper};
use permutation_rules::frontier::geometry::ring_provinces;

use common::*;

const RELEASE_AFTER: i64 = 3_600;
const THETA_BPS: u16 = 1_000;
const KEEP_EVERY: i64 = 1_500;
/// The gen of a site's first holding: every founding bumps the site's gen
/// (W3-A's pinned choice in the program; integ-W3).
const FIRST_GEN: u8 = 1;

type K = Keeper<localnet::InProcess, Gated>;
/// `(P, Q, site)`.
type SiteKey = (i16, i16, u8);

fn wallet(i: u8) -> Keypair {
    Keypair::new_from_array([0x40 + i; 32])
}

fn player(w: &Keypair, payer: &Keypair) -> ix::Player {
    ix::Player {
        actor: w.pubkey(),
        payer: payer.pubkey(),
        wallet: w.pubkey(),
    }
}

fn province(w: &World, p: i16, q: i16) -> Province {
    let a =
        w.ip.lock()
            .account(&w.addrs.province(p as i32, q as i32))
            .expect("province");
    Province::decode(&a.data).unwrap()
}

fn citizen(w: &World, k: &Keypair) -> Citizen {
    let a =
        w.ip.lock()
            .account(&w.addrs.citizen(&k.pubkey()))
            .expect("citizen");
    Citizen::decode(&a.data).unwrap()
}

fn holding(w: &World, s: (i16, i16, u8)) -> Option<Holding> {
    w.ip.lock()
        .account(&w.addrs.holding(s.0 as i32, s.1 as i32, s.2))
        .filter(|a| a.owner == w.program)
        .map(|a| Holding::decode(&a.data).unwrap())
}

fn frontier(w: &World) -> Frontier {
    Frontier::decode(&w.ip.lock().account(&w.addrs.frontier()).unwrap().data).unwrap()
}

fn bell_of(w: &World) -> u32 {
    let s = w.season();
    fclient::clock::bell_at(s.genesis_ts, w.now()).unwrap_or(0)
}

/// The sites of wedge `f` in rings 2..=3: `(P, Q, site)`.
fn wedge_sites(w: &World, f: u8) -> Vec<(i16, i16, u8)> {
    let mut v = vec![];
    for d in 2..=3u32 {
        for pc in ring_provinces(d) {
            if pc.wedge() != Some(f) {
                continue;
            }
            let pv = province(w, pc.p as i16, pc.q as i16);
            for s in 0..pv.site_count {
                v.push((pc.p as i16, pc.q as i16, s));
            }
        }
    }
    v
}

/// The chain, the keeper, and a player who keeps one holding alive.
struct Run {
    w: World,
    k: K,
    relay: Keypair,
    keep: Option<(Keypair, (i16, i16, u8), i64)>,
    keep_musters: u32,
}

impl Run {
    /// One keeper tick and one block; the keep-alive owner acts every
    /// `KEEP_EVERY` game seconds once its holding is final.
    async fn tick(&mut self) {
        self.k.tick().await.unwrap();
        self.w.step();
        let Some((kp, site, last)) = self.keep.take() else {
            return;
        };
        let mut last = last;
        let final_now = holding(&self.w, site).is_some_and(|h| self.w.now() >= h.final_ts);
        if final_now && self.w.now() >= last + KEEP_EVERY {
            let h = HoldingRef {
                p: site.0,
                q: site.1,
                site: site.2,
            };
            self.w
                .send(
                    &[ix::muster(
                        &self.w.addrs,
                        &player(&kp, &self.relay),
                        h,
                        5,
                        100,
                        1,
                    )],
                    &[&self.relay, &kp],
                )
                .await;
            self.keep_musters += 1;
            last = self.w.now();
        }
        self.keep = Some((kp, site, last));
    }

    async fn until(&mut self, max: u64, what: &str, cond: impl Fn(&World, &K) -> bool) {
        for _ in 0..max {
            if cond(&self.w, &self.k) {
                return;
            }
            self.tick().await;
        }
        panic!(
            "{what}: not reached in {max} slots (last alerts: {:?})",
            self.k.alerts.iter().rev().take(8).collect::<Vec<_>>()
        );
    }

    async fn next_bell(&mut self) -> u32 {
        let b = bell_of(&self.w);
        self.until(200, "next bell", move |w, _| bell_of(w) > b)
            .await;
        bell_of(&self.w)
    }

    async fn join(&self, kp: &Keypair, f: u8) {
        self.w
            .send(
                &[ix::join(
                    &self.w.addrs,
                    kp.pubkey(),
                    self.relay.pubkey(),
                    f,
                    &Address::default(),
                    0,
                    None,
                )],
                &[&self.relay, kp],
            )
            .await;
    }

    async fn file(&self, kp: &Keypair, sites: &[(i16, i16, u8)]) {
        let s: Vec<Site> = sites
            .iter()
            .map(|&(p, q, site)| Site { p, q, site })
            .collect();
        self.w
            .send(
                &[ix::file_ticket(&self.w.addrs, &player(kp, &self.relay), &s)],
                &[&self.relay, kp],
            )
            .await;
    }
}

/// From the feed: per holding address, the game times of its owner actions
/// (SETTLE fresh/displace, MUSTER, EXPLORE) and of its RELEASE.
async fn holding_history(w: &World) -> BTreeMap<Address, (Vec<i64>, Option<i64>)> {
    let mut out: BTreeMap<Address, (Vec<i64>, Option<i64>)> = BTreeMap::new();
    let mut cur = Cursor(0);
    loop {
        let page = w.ip.feed(cur).await.unwrap();
        let Some(last) = page.last() else { break };
        cur = Cursor(last.seq);
        for tx in page.iter().filter(|t| t.err.is_none()) {
            for b in fclient::log::bodies_from_logs(&tx.logs, &w.addrs.program).unwrap() {
                let Ok(r) = plog::decode(&b) else { continue };
                let pqs = |r: &plog::Record| {
                    let p = i32::from_le_bytes(r.key[0..4].try_into().unwrap());
                    let q = i32::from_le_bytes(r.key[4..8].try_into().unwrap());
                    w.addrs.holding(p, q, r.key[8])
                };
                let host = |r: &plog::Record| {
                    let id = u64::from_le_bytes(r.key[0..8].try_into().unwrap());
                    w.addrs.holding_of_host(id).unwrap()
                };
                match r.kind {
                    Kind::SETTLE if r.payload[0] <= 1 => {
                        out.entry(pqs(&r)).or_default().0.push(tx.block_time)
                    }
                    Kind::MUSTER | Kind::EXPLORE => {
                        out.entry(host(&r)).or_default().0.push(tx.block_time)
                    }
                    Kind::RELEASE => out.entry(pqs(&r)).or_default().1 = Some(tx.block_time),
                    _ => {}
                }
            }
        }
    }
    out
}

/// A started keeper with the land roles over a fresh world, genesis rings
/// open and the season Running (`None`: the program has no land).
async fn started() -> Option<Run> {
    let w = world_with(|p| {
        p.dormant_after_secs = 1_800;
        p.release_after_secs = RELEASE_AFTER as u32;
        p.theta_early_bps = THETA_BPS;
    })
    .await;
    let auth = w.authority.pubkey();
    for f in 0..6u8 {
        match w
            .try_send(&[ix::init_shards(&w.addrs, auth, f)], &[&w.authority])
            .await
        {
            Ok(()) => {}
            Err((Some(99), _)) => return None,
            Err(e) => panic!("InitShards: {e:?}"),
        }
    }
    let mut cfg = keeper_config(&w);
    cfg.roles = ["beacon", "rings", "archive", "fold", "tickets"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let k = Keeper::new(cfg, w.ip.clone(), w.drand.clone(), &[0x33; 32], None).unwrap();
    fund(&w, &k.payers);
    let relay = Keypair::new_from_array([0xEE; 32]);
    w.airdrop(&relay.pubkey(), 200_000_000_000);
    let mut run = Run {
        w,
        k,
        relay,
        keep: None,
        keep_musters: 0,
    };
    run.k.start().await.unwrap();
    run.until(3_000, "genesis rings", |w, k| {
        (k.rings.genesis_done && w.season().effective_status(w.now()) == status::RUNNING)
            || k.rings.unsupported
    })
    .await;
    (!run.k.rings.unsupported).then_some(run)
}

/// Wave-3 review (W3-C major): overlapping fallbacks in one honest
/// cohort. A and B contest X; B's fallback is Y; C's only site is Y; D
/// and E contest Z with fallback Y as well. Whatever the scores, the
/// keeper never displaces anyone (it holds a fresh settlement while a
/// better same-cohort ticket may still come to its site), and the result
/// is the lottery's: at each site the best ticket of those that reach it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn overlapping_fallbacks_never_displace() {
    let Some(mut run) = started().await else {
        println!("land: NotImplemented by this program; not applicable");
        return;
    };
    println!("program: {}", run.w.which.label());
    let w0 = wedge_sites(&run.w, 0);
    let (x, y, z) = (w0[0], w0[1], w0[2]);
    let kps: Vec<Keypair> = (200..205u8).map(wallet).collect();
    let prefs: Vec<Vec<SiteKey>> = vec![vec![x], vec![x, y], vec![y], vec![z, y], vec![z, y]];
    for kp in &kps {
        run.join(kp, 0).await;
    }
    let bell = run.next_bell().await;
    for (kp, p) in kps.iter().zip(&prefs) {
        run.file(kp, p).await;
    }
    assert_eq!(bell_of(&run.w), bell, "one cohort");
    let cits: Vec<Address> = kps
        .iter()
        .map(|k| run.w.addrs.citizen(&k.pubkey()))
        .collect();
    let all = cits.clone();
    run.until(2_000, "cohort settled", move |w, _| {
        all.iter().all(|c| {
            let a = w.ip.lock().account(c).unwrap();
            Citizen::decode(&a.data).unwrap().ticket_bell == land::NO_TICKET
        })
    })
    .await;
    run.tick().await;
    assert_eq!(
        run.k.tickets.displacing, 0,
        "no displacement in an honest cohort"
    );
    // The lottery by hand: repeatedly, the best ticket at its current
    // preference among the remaining takes the site if free.
    let score = |i: usize, s: SiteKey| {
        let r = ix::region_of(s.0 as i32, s.1 as i32);
        let seed = run.k.seeds.known(bell, r).expect("seed").seed;
        let tag = citizen_tag_u64(&cits[i]);
        (
            land::ticket_score(&seed, s.0 as i32, s.1 as i32, s.2, tag),
            tag,
        )
    };
    let mut next = [0usize; 5];
    let mut owner: BTreeMap<SiteKey, usize> = BTreeMap::new();
    loop {
        let mut moved = false;
        for s in [x, y, z] {
            if owner.contains_key(&s) {
                continue;
            }
            // Only tickets whose current preference is s, and no better
            // ticket that could still reach s later.
            let here: Vec<usize> = (0..5)
                .filter(|&i| prefs[i].get(next[i]) == Some(&s))
                .collect();
            let Some(&best) = here.iter().max_by(|&&a, &&b| {
                let (sa, ta) = score(a, s);
                let (sb, tb) = score(b, s);
                if land::beats(sa, ta, sb, tb) {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Less
                }
            }) else {
                continue;
            };
            let (sb, tb) = score(best, s);
            // o moves on from its current site if that site is owned or a
            // better ticket is there too (the keeper's `movers`).
            let moves = |o: usize| -> bool {
                let Some(&cur) = prefs[o].get(next[o]) else {
                    return false;
                };
                owner.contains_key(&cur)
                    || (0..5).any(|b| {
                        b != o && prefs[b].get(next[b]) == Some(&cur) && {
                            let (s1, t1) = score(b, cur);
                            let (s2, t2) = score(o, cur);
                            land::beats(s1, t1, s2, t2)
                        }
                    })
            };
            let waits = (0..5).any(|o| {
                o != best && moves(o) && prefs[o].iter().skip(next[o] + 1).any(|&t| t == s) && {
                    let (so, to) = score(o, s);
                    land::beats(so, to, sb, tb)
                }
            });
            if waits {
                continue;
            }
            owner.insert(s, best);
            for i in here {
                if i != best {
                    next[i] += 1;
                }
            }
            next[best] = usize::MAX / 2;
            moved = true;
        }
        if !moved {
            break;
        }
    }
    for (&s, &i) in &owner {
        let h = holding(&run.w, s).expect("founded");
        assert_eq!(h.owner_citizen, cits[i], "site {s:?}");
        assert_eq!(h.gen, FIRST_GEN, "site {s:?} founded once");
    }
    println!(
        "overlap: owners {:?}, {} settlements, {} fresh settlements held",
        owner,
        run.k.tickets.settled.len(),
        run.k.tickets.held
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn land_season() {
    let t0 = std::time::Instant::now();
    let w = world_with(|p| {
        p.dormant_after_secs = 1_800;
        p.release_after_secs = RELEASE_AFTER as u32;
        p.theta_early_bps = THETA_BPS;
    })
    .await;
    println!("program: {}", w.which.label());
    let auth = w.authority.pubkey();
    for f in 0..6u8 {
        match w
            .try_send(&[ix::init_shards(&w.addrs, auth, f)], &[&w.authority])
            .await
        {
            Ok(()) => {}
            Err((Some(99), _)) => {
                println!("InitShards: NotImplemented by this program; land test not applicable");
                return;
            }
            Err(e) => panic!("InitShards: {e:?}"),
        }
    }
    let mut cfg = keeper_config(&w);
    cfg.roles = [
        "beacon", "rings", "archive", "fold", "tickets", "explore", "dormancy", "sweep",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let k = Keeper::new(cfg, w.ip.clone(), w.drand.clone(), &[0x33; 32], None).unwrap();
    fund(&w, &k.payers);
    let relay = Keypair::new_from_array([0xEE; 32]);
    w.airdrop(&relay.pubkey(), 200_000_000_000);
    let mut run = Run {
        w,
        k,
        relay,
        keep: None,
        keep_musters: 0,
    };
    run.k.start().await.unwrap();

    // ---- 1. genesis rings
    run.until(3_000, "genesis rings", |w, k| {
        (k.rings.genesis_done && w.season().effective_status(w.now()) == status::RUNNING)
            || k.rings.unsupported
    })
    .await;
    if run.k.rings.unsupported {
        println!("rings: NotImplemented by this program (W3-A); land test not applicable");
        return;
    }
    assert_eq!(
        run.k.rings.opened.len(),
        keeper_core::rings::RingDuty::provinces_of_rings(3)
    );
    println!(
        "genesis: rings 0-3, {} provinces by bell {}",
        run.k.rings.opened.len(),
        bell_of(&run.w)
    );

    // ---- 2. one cohort in wedge 0: exactly ⌈θ × open⌉ holdings.
    let w0 = wedge_sites(&run.w, 0);
    let w1 = wedge_sites(&run.w, 1);
    let n0 = (w0.len() * THETA_BPS as usize).div_ceil(10_000).max(5);
    assert!(n0 < w0.len(), "wedge 0 has {} sites", w0.len());
    println!(
        "wedge 0: {} open sites, {n0} holdings planned; wedge 1: {} sites",
        w0.len(),
        w1.len()
    );
    let contested = w0[0];
    // Players 0..3 contest `contested` with fallbacks w0[1..4]; the singles
    // take w0[4..n0]; the keep-alive player takes w0[n0]: n0 holdings.
    let mut plan: Vec<(Keypair, Vec<SiteKey>)> = vec![];
    for i in 0..3u8 {
        plan.push((wallet(i), vec![contested, w0[1 + i as usize]]));
    }
    for (i, &site) in w0.iter().enumerate().take(n0).skip(4) {
        plan.push((wallet(i as u8), vec![site]));
    }
    let keep_site = w0[n0];
    plan.push((wallet(60), vec![keep_site]));
    for (kp, _) in &plan {
        run.join(kp, 0).await;
    }
    let bell_b = run.next_bell().await;
    for (kp, sites) in &plan {
        run.file(kp, sites).await;
    }
    assert_eq!(bell_of(&run.w), bell_b, "one cohort bell");
    let all: Vec<Address> = plan
        .iter()
        .map(|(kp, _)| run.w.addrs.citizen(&kp.pubkey()))
        .collect();
    run.until(1_500, "cohort settled", move |w, _| {
        all.iter().all(|c| {
            let a = w.ip.lock().account(c).unwrap();
            Citizen::decode(&a.data).unwrap().ticket_bell == land::NO_TICKET
        })
    })
    .await;
    // One more tick polls the last landings into the duty's record.
    run.tick().await;
    let region_x = ix::region_of(contested.0 as i32, contested.1 as i32);
    let seed = run
        .k
        .seeds
        .known(bell_b, region_x)
        .expect("the cohort's seed")
        .seed;
    let w = &run.w;
    let mut contenders: Vec<(u64, u64, usize)> = (0..3)
        .map(|i| {
            let tag = citizen_tag_u64(&w.addrs.citizen(&plan[i].0.pubkey()));
            let s = land::ticket_score(
                &seed,
                contested.0 as i32,
                contested.1 as i32,
                contested.2,
                tag,
            );
            (s, tag, i)
        })
        .collect();
    contenders.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let winner = contenders[0].2;
    let hx = holding(w, contested).expect("contested site founded");
    assert_eq!(
        hx.owner_citizen,
        w.addrs.citizen(&plan[winner].0.pubkey()),
        "the lottery winner holds the contested site"
    );
    assert_eq!(
        hx.gen, FIRST_GEN,
        "no displacement: settled in descending score"
    );
    let mut founded: Vec<(i16, i16, u8)> = vec![contested];
    for &(_, _, i) in &contenders[1..] {
        let fb = plan[i].1[1];
        let h = holding(w, fb).expect("fallback founded");
        assert_eq!(h.owner_citizen, w.addrs.citizen(&plan[i].0.pubkey()));
        assert_eq!(h.gen, FIRST_GEN);
        founded.push(fb);
    }
    for (kp, sites) in &plan[3..] {
        let h = holding(w, sites[0]).expect("single site founded");
        assert_eq!(h.owner_citizen, w.addrs.citizen(&kp.pubkey()));
        founded.push(sites[0]);
    }
    assert_eq!(founded.len(), n0);
    assert_eq!(run.k.tickets.displacing, 0);
    for &(p, q, _) in plan.iter().flat_map(|x| x.1.iter()) {
        for c in province(w, p, q)
            .cohorts
            .iter()
            .filter(|c| c.bell == bell_b && c.filed > 0)
        {
            assert_eq!(c.settled, c.filed, "cohort of ({p}, {q}) closed");
        }
    }
    let lat = run.k.tickets.winner_latency();
    println!(
        "cohort bell {bell_b}: {} settlements ({} winners); seed cache → winner landing p50 {:?} p99 {:?} max {:?} slots",
        run.k.tickets.settled.len(),
        lat.len(),
        quantile(&lat, 0.5),
        quantile(&lat, 0.99),
        lat.iter().max()
    );
    assert_eq!(lat.len(), n0);
    assert!(
        lat.iter().all(|&x| x <= 6),
        "winners in the first slots after S: {lat:?}"
    );

    // ---- 3. explore: a loser's fallback holding, once final.
    let wi = contenders[1].2;
    let wc = plan[wi].1[1];
    let wkp = wallet(wi as u8);
    run.until(600, "holdings final", move |w, _| {
        w.now() >= holding(w, wc).unwrap().final_ts
    })
    .await;
    let hw = HoldingRef {
        p: wc.0,
        q: wc.1,
        site: wc.2,
    };
    let first_muster = run
        .w
        .try_send(
            &[ix::muster(
                &run.w.addrs,
                &player(&wkp, &run.relay),
                hw,
                5,
                100,
                2,
            )],
            &[&run.relay, &wkp],
        )
        .await;
    let resident = match first_muster {
        Ok(()) => true,
        Err((Some(26), _)) => {
            println!(
                "resident actions: NotResident (26) on this program (resolves are W4-A's); \
                 explore, keep-alive and stranded-host parts skipped, the rest runs"
            );
            false
        }
        Err(e) => panic!("Muster: {e:?}"),
    };
    let mut host = 0u64;
    if resident {
        run.keep = Some((wallet(60), keep_site, 0));
        let hwh = holding(&run.w, wc).unwrap();
        host = fclient::addr::host_id(wc.0 as i32, wc.1 as i32, wc.2, hwh.gen, hwh.host_seq - 1)
            .unwrap();
        let tile = (1..61u8)
            .find(|t| province(&run.w, wc.0, wc.1).explored_mask & (1 << t) == 0)
            .unwrap();
        run.w
            .send(
                &[ix::explore(
                    &run.w.addrs,
                    &player(&wkp, &run.relay),
                    hw,
                    (wc.0, wc.1),
                    host,
                    &[tile],
                )],
                &[&run.relay, &wkp],
            )
            .await;
        run.until(600, "explore settled", move |w, _| {
            holding(w, wc).is_some_and(|h| h.explore.state == 0)
        })
        .await;
        let ec = citizen(&run.w, &wkp);
        assert_eq!(
            (ec.works, ec.explores, ec.explores_floor_left),
            (4, 1, 2),
            "the floor find"
        );
        assert_eq!(run.k.explore.sent.len(), 1);
        println!(
            "explore: host {host:#x} tile {tile} settled by the keeper, works {}",
            ec.works
        );
    }

    // ---- 4. sweep (pool_owed as a fixture)
    let owed = 12_345u64;
    let ka = run
        .w
        .addrs
        .holding(keep_site.0 as i32, keep_site.1 as i32, keep_site.2);
    {
        let mut acct = run.w.ip.lock().account(&ka).unwrap();
        acct.lamports += owed;
        acct.data[l::holding::POOL_OWED..l::holding::POOL_OWED + 8]
            .copy_from_slice(&owed.to_le_bytes());
        run.w.ip.lock().set_account(ka, acct).unwrap();
    }
    let pool_before = run.w.ip.lock().balance(&run.w.addrs.defence_pool());
    let sweep_missing = |k: &K| {
        k.alerts
            .iter()
            .any(|a| a.1 == "not-implemented" && a.2.starts_with("sweep:"))
    };
    run.until(100, "sweep", move |w, k| {
        holding(w, keep_site).unwrap().pool_owed == 0 || sweep_missing(k)
    })
    .await;
    let sweep_ok = !sweep_missing(&run.k);
    if sweep_ok {
        assert_eq!(
            run.w.ip.lock().balance(&run.w.addrs.defence_pool()),
            pool_before + owed
        );
        println!("sweep: {owed} lamports of pool_owed swept");
    } else {
        // SweepPoolOwed is W4-B's (a stub in wave 3); ReleaseDormant sweeps
        // the same pool_owed at step 8 on the program.
        println!("sweep: SweepPoolOwed NotImplemented on this program (W4-B); skipped");
    }

    // ---- 5. fold
    let b_fold = run.next_bell().await;
    run.until(200, "fold of the bell", move |_, k| {
        k.fold.folded.contains(&b_fold)
    })
    .await;
    let fr = frontier(&run.w);
    let mut live = 0u32;
    let mut by_wedge = [0u32; 6];
    for f in 0..6u8 {
        for sh in 0..8u8 {
            let a = run
                .w
                .ip
                .lock()
                .account(&run.w.addrs.join_shard(f, sh))
                .unwrap();
            let js = JoinShard::decode(&a.data).unwrap();
            live += js.holdings;
            for (x, y) in by_wedge.iter_mut().zip(js.holdings_by_wedge) {
                *x += y;
            }
        }
    }
    let funds: Vec<fclient::decode::ProvinceFund> = (0..6u8)
        .map(|f| {
            let a = run
                .w
                .ip
                .lock()
                .account(&run.w.addrs.province_fund(f))
                .unwrap();
            fclient::decode::ProvinceFund::decode(&a.data).unwrap()
        })
        .collect();
    assert_eq!(fr.fold_bell, b_fold);
    assert_eq!((fr.occupied_sites, fr.wedge_occupied), (live, by_wedge));
    assert_eq!(fr.occupied_sites as usize, n0);
    for (f, pf) in funds.iter().enumerate() {
        assert_eq!(
            fr.wedge_open[f], pf.open_sites,
            "wedge {f} open sites folded"
        );
    }
    assert_eq!(
        fr.provinces_opened,
        funds.iter().map(|f| f.provinces_opened).sum::<u32>()
    );
    assert!(fr.wedge_open[0] as usize >= w0.len());
    println!(
        "fold bell {b_fold}: occupied {} (wedge 0: {} of {} open); {} bells folded, part 2 {:?} slots into its bell",
        fr.occupied_sites,
        fr.wedge_occupied[0],
        fr.wedge_open[0],
        run.k.fold.folded.len(),
        run.k.fold.latency.last().map(|x| x.1)
    );

    // ---- 6. the crowding ring
    run.until(1_500, "ring 4 seeded and provisioned", |_, k| {
        k.rings.complete.contains(&4)
    })
    .await;
    let rs4 = RingSeed::decode(
        &run.w
            .ip
            .lock()
            .account(&run.w.addrs.ring_seed(4))
            .unwrap()
            .data,
    )
    .unwrap();
    let s = run.w.season();
    let sc = fclient::clock::SeasonClock::from_season(&s);
    assert_eq!(rs4.status, 2);
    assert_eq!(
        rs4.round,
        sc.drand.ring_seed_round(rs4.t_open, s.seed_margin)
    );
    let sig = fclient::beacon::decompress_sig(&run.w.drand.key.sign(rs4.round)).unwrap();
    assert_eq!(rs4.seed, fclient::beacon::seed_of(rs4.round, &sig));
    for pc in ring_provinces(4) {
        assert!(run.k.rings.opened.contains(&(pc.p, pc.q)));
    }
    run.until(3_000, "crowding rings stop", |_, k| {
        let top = k.rings.complete.iter().max().copied().unwrap_or(0);
        k.rings.complete.len() == top as usize + 1
            && matches!(k.rings.waiting, Some(land::RingWait::NotCrowded))
    })
    .await;
    let rings_now = frontier(&run.w).rings_opened;
    for _ in 0..80 {
        run.tick().await;
    }
    assert_eq!(
        frontier(&run.w).rings_opened,
        rings_now,
        "no ring while not crowded"
    );
    println!(
        "rings: ring 4 opened at t_open {} (seeded at slot {:?}); {rings_now} rings, {} provinces; now {:?}",
        rs4.t_open,
        run.k.rings.seeded_at.iter().find(|x| x.0 == 4).map(|x| x.1),
        run.k.rings.opened.len(),
        run.k.rings.waiting
    );

    // ---- 7. displacement in a second cohort (wedge 1)
    let z = w1[0];
    let (d1, d2) = (wallet(100), wallet(101));
    run.join(&d1, 1).await;
    run.join(&d2, 1).await;
    let bell_b2 = run.next_bell().await;
    // Nobody but the test settles this cohort until the lower score is in.
    run.k.cfg.roles.retain(|r| r != "tickets");
    run.file(&d1, &[z]).await;
    run.file(&d2, &[z]).await;
    let rz = ix::region_of(z.0 as i32, z.1 as i32);
    let mut found = None;
    for _ in 0..600 {
        let slot = run.w.slot();
        found = run
            .k
            .seeds
            .find(
                &run.w.ip,
                &run.w.addrs,
                &run.k.beacon.anchors,
                bell_b2,
                rz,
                slot,
            )
            .await
            .unwrap();
        if found.is_some() {
            break;
        }
        run.tick().await;
    }
    let sz = found.expect("S(b2, r) cached");
    let score_of = |kp: &Keypair| {
        let tag = citizen_tag_u64(&run.w.addrs.citizen(&kp.pubkey()));
        (
            land::ticket_score(&sz.seed, z.0 as i32, z.1 as i32, z.2, tag),
            tag,
        )
    };
    let (a1, a2) = (score_of(&d1), score_of(&d2));
    let (lo, hi) = if land::beats(a1.0, a1.1, a2.0, a2.1) {
        (wallet(101), wallet(100))
    } else {
        (wallet(100), wallet(101))
    };
    let site_z = Site {
        p: z.0,
        q: z.1,
        site: z.2,
    };
    run.w
        .send(
            &[ix::settle_ticket(
                &run.w.addrs,
                run.relay.pubkey(),
                &lo.pubkey(),
                1,
                0,
                site_z,
                bell_b2,
                &[site_z],
                sz.src,
                None,
            )],
            &[&run.relay],
        )
        .await;
    assert_eq!(
        holding(&run.w, z).unwrap().owner_citizen,
        run.w.addrs.citizen(&lo.pubkey()),
        "the lower score settled first"
    );
    let relay_before = run.w.ip.lock().balance(&run.relay.pubkey());
    run.k.cfg.roles.push("tickets".into());
    let hi_c = run.w.addrs.citizen(&hi.pubkey());
    run.until(300, "displacement", move |w, _| {
        let a = w.ip.lock().account(&hi_c).unwrap();
        Citizen::decode(&a.data).unwrap().ticket_bell == land::NO_TICKET
    })
    .await;
    let hz = holding(&run.w, z).unwrap();
    assert_eq!(hz.owner_citizen, hi_c, "the higher score displaces");
    assert_eq!(hz.gen, FIRST_GEN + 1, "rewritten in place, gen + 1");
    assert_eq!(run.k.tickets.displacing, 1);
    let lc = citizen(&run.w, &lo);
    assert_eq!(lc.flags & l::citizen::FLAG_PROVISIONAL, 0);
    assert_eq!((lc.holdings_n, lc.ticket_bell), (0, land::NO_TICKET));
    assert!(
        run.w.ip.lock().balance(&run.relay.pubkey())
            >= relay_before + fclient::abi::rent(fclient::abi::size::HOLDING),
        "the displaced holding's rent payer is repaid"
    );
    for c in province(&run.w, z.0, z.1)
        .cohorts
        .iter()
        .filter(|c| c.bell == bell_b2 && c.filed > 0)
    {
        assert_eq!((c.filed, c.settled), (2, 2));
    }
    founded.push(z);
    println!(
        "displacement: cohort bell {bell_b2}, site {z:?}, gen {}",
        hz.gen
    );

    // ---- 8. dormancy
    let horizon = run.w.now() + RELEASE_AFTER + 1_200;
    run.until(
        RELEASE_AFTER as u64,
        "idle holdings released",
        move |w, _| w.now() >= horizon,
    )
    .await;
    let hist = holding_history(&run.w).await;
    let mut released = 0;
    for s in &founded {
        let a = run.w.addrs.holding(s.0 as i32, s.1 as i32, s.2);
        let (acts, rel) = &hist[&a];
        if *s == keep_site && resident {
            assert!(rel.is_none(), "the active holding is kept");
            assert!(holding(&run.w, *s).is_some());
            continue;
        }
        let at = rel.unwrap_or_else(|| panic!("{s:?} released"));
        let last = acts.iter().copied().filter(|&t| t <= at).max().unwrap();
        let idle = at - last;
        assert!(idle >= RELEASE_AFTER, "{s:?} released after {idle} s idle");
        assert!(
            idle <= RELEASE_AFTER + 1_200,
            "{s:?} released {idle} s after its last action"
        );
        assert!(holding(&run.w, *s).is_none());
        released += 1;
    }
    if resident {
        let pv = province(&run.w, wc.0, wc.1);
        assert!(
            pv.entries
                .iter()
                .all(|x| x.id != host || x.pend_op == keeper_core::holdings::OP_FORFEIT),
            "the released holding's host is disbanded (a pending Forfeit, v1.5)"
        );
        assert!(run.k.holdings.disbands.iter().any(|d| d.2 == host));
    }
    println!(
        "dormancy: {released} released after ≥ {RELEASE_AFTER} s idle; keep-alive kept ({} musters); {} stranded hosts disbanded",
        run.keep_musters,
        run.k.holdings.disbands.len()
    );

    // ---- 9. expiry
    let x = wallet(120);
    run.join(&x, 1).await;
    let xs = w1[1];
    run.k.cfg.roles.retain(|r| r != "tickets");
    run.file(&x, &[xs]).await;
    let tb = citizen(&run.w, &x).ticket_bell;
    run.until(24 * 80, "24 bells", move |w, _| bell_of(w) >= tb + 24)
        .await;
    run.k.cfg.roles.push("tickets".into());
    let xc = run.w.addrs.citizen(&x.pubkey());
    run.until(50, "expired", move |w, _| {
        let a = w.ip.lock().account(&xc).unwrap();
        Citizen::decode(&a.data).unwrap().ticket_bell == land::NO_TICKET
    })
    .await;
    run.tick().await;
    assert_eq!(run.k.tickets.expired, 1);
    assert!(
        holding(&run.w, xs).is_none(),
        "an expired ticket founds nothing"
    );
    for c in province(&run.w, xs.0, xs.1)
        .cohorts
        .iter()
        .filter(|c| c.bell == tb && c.filed > 0)
    {
        assert_eq!(c.settled, c.filed, "the expired ticket's cohort closed");
    }
    println!(
        "expiry: the ticket of bell {tb} settled expired at bell {}",
        bell_of(&run.w)
    );

    // ---- the keeper's view
    let st = run
        .k
        .status_json(run.w.slot(), run.w.now(), Some(bell_of(&run.w)));
    println!("status duties: {}", st["duties"]);
    let bad: Vec<_> = run
        .k
        .alerts
        .iter()
        .filter(|a| a.1 == "failed" || a.1 == "dead" || a.1 == "not-implemented")
        .collect();
    println!(
        "alerts: {} total, {} failed/dead",
        run.k.alerts.len(),
        bad.len()
    );
    for a in bad.iter().take(12) {
        println!("  {a:?}");
    }
    // Wave-3 review (W3-C): assert, not only print. No write may die or
    // meet NotImplemented; a failure is allowed only with a code the
    // keeper expects in normal operation (a fold part of an older bell, a
    // ticket that moved, a holding no longer dormant, a seed not there
    // yet, a province not resolved for the keep-alive host).
    const EXPECTED: [&str; 6] = [
        "FoldStale",
        "TicketState",
        "NotDormant",
        "SeedNotReady",
        "NoAnchor",
        "NotResident",
    ];
    let unexpected: Vec<_> = bad
        .iter()
        .filter(|a| {
            let stub = !sweep_ok && a.1 == "not-implemented" && a.2.starts_with("sweep:");
            !stub && (a.1 != "failed" || !EXPECTED.iter().any(|c| a.2.contains(c)))
        })
        .collect();
    assert!(unexpected.is_empty(), "unexpected alerts: {unexpected:?}");
    println!(
        "land_season: {:.1} game h, {} slots, in {:.1} s",
        (run.w.now() - s.genesis_ts) as f64 / 3_600.0,
        run.w.slot(),
        t0.elapsed().as_secs_f64()
    );
}
