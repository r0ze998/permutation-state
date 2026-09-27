//! Recorders of the verifier's fixtures (`frontier-node/fixtures/verify/`).
//! Ignored by default: they rewrite the committed files.
//!
//! - `record_land_program`: the **test-beacon program** (`PSF_FRONTIER_SO`,
//!   `scripts/build-frontier.sh --features test-beacon`) over `localnet`
//!   in process with the keeper's beacon, ring, fold and ticket duties: a
//!   season announced and created by rule, genesis rings 0–3, an honest
//!   cohort (a contested site and fallbacks), a keeper crash mid-bell and a
//!   restart, a held anchor, pre-funded anchor and cache addresses, a
//!   displacement inside a second cohort and a ticket settled expired.
//! - `record_march_synth`: the synthetic march season of
//!   [`verify_core::fixture`] (clash, transit, defence and archive records
//!   the wave-3 program cannot emit yet; W4-A/W4-B's).
//!
//! ```sh
//! PSF_FRONTIER_SO=../permutation-frontier/target/deploy-test-beacon/permutation_frontier.so \
//!   cargo test --release -p verify --test record -- --ignored --nocapture
//! ```

mod common;

use common::*;
use fclient::addr::citizen_tag_u64;
use fclient::decode::{Citizen, Province};
use fclient::ix::{self, Site};
use fclient::land;
use fclient::{Address, Keypair, Signer};
use keeper_core::Keeper;
use permutation_rules::frontier::geometry::{region_of, ring_provinces, ProvinceCoord};

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

struct Run {
    w: World,
    k: K,
    relay: Keypair,
    cfg: keeper_core::config::KeeperConfig,
    /// Keeper ticks skipped (a held anchor).
    hold: u32,
}

impl Run {
    async fn tick(&mut self) {
        if self.hold > 0 {
            self.hold -= 1;
        } else {
            self.k.tick().await.unwrap();
        }
        self.w.step();
    }
    async fn until(&mut self, max: u64, what: &str, cond: impl Fn(&World) -> bool) {
        for _ in 0..max {
            if cond(&self.w) {
                return;
            }
            self.tick().await;
        }
        panic!(
            "{what}: not reached in {max} slots; alerts {:?}",
            self.k.alerts.iter().rev().take(6).collect::<Vec<_>>()
        );
    }
    async fn next_bell(&mut self) -> u32 {
        let b = self.w.bell();
        self.until(200, "next bell", move |w| w.bell() > b).await;
        self.w.bell()
    }
    async fn join(&self, kp: &Keypair, f: u8) {
        self.w.airdrop(&kp.pubkey(), 1_000_000);
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
    fn ticket_done(&self, kp: &Keypair) -> bool {
        let a = self
            .w
            .ip
            .lock()
            .account(&self.w.addrs.citizen(&kp.pubkey()))
            .unwrap();
        Citizen::decode(&a.data).unwrap().ticket_bell == land::NO_TICKET
    }
    /// A crash mid-bell: the keeper is dropped with its writes in flight and
    /// a new one (same payer seed, no journal: nothing to adopt) starts over
    /// from the chain.
    async fn crash(&mut self) {
        let fresh = Keeper::new(
            self.cfg.clone(),
            self.w.ip.clone(),
            self.w.drand.clone(),
            &[0x33; 32],
            None,
        )
        .unwrap();
        self.k = fresh;
        self.k.start().await.unwrap();
    }
}

fn province(w: &World, p: i16, q: i16) -> Province {
    let a =
        w.ip.lock()
            .account(&w.addrs.province(p as i32, q as i32))
            .expect("province");
    Province::decode(&a.data).unwrap()
}

/// Sites of wedge `f` in rings 2..=3 whose region is `r`.
fn sites_in(w: &World, f: u8, r: u8) -> Vec<(i16, i16, u8)> {
    let mut v = vec![];
    for d in 2..=3u32 {
        for pc in ring_provinces(d) {
            if pc.wedge() != Some(f) || region_of(pc) != r {
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

/// The region of wedge `f` (rings 2–3) with the most sites.
fn best_region(f: u8) -> u8 {
    let mut n = [0u32; 16];
    for d in 2..=3u32 {
        for pc in ring_provinces(d) {
            if pc.wedge() == Some(f) {
                n[region_of(pc) as usize] += 1;
            }
        }
    }
    (0..16u8).max_by_key(|r| n[*r as usize]).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "rewrites fixtures/verify/land-program.json; needs PSF_FRONTIER_SO"]
async fn record_land_program() {
    let so = std::env::var("PSF_FRONTIER_SO").expect("PSF_FRONTIER_SO names the test-beacon .so");
    let w = world(&so, |_| {}).await;
    let (r0, r1) = (best_region(0), best_region(1));
    let mut cfg = keeper_core::config::KeeperConfig::new(
        w.program,
        SEASON_ID,
        Address::new_from_array([0xBE; 32]),
    );
    cfg.roles = ["beacon", "rings", "fold", "tickets"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    cfg.regions = if r0 == r1 {
        vec![r0]
    } else {
        vec![r0.min(r1), r0.max(r1)]
    };
    let k = Keeper::new(
        cfg.clone(),
        w.ip.clone(),
        w.drand.clone(),
        &[0x33; 32],
        None,
    )
    .unwrap();
    for a in k.payers.delay.addresses() {
        w.airdrop(&a, 2_000_000_000);
    }
    for f in &k.payers.funders.keys {
        w.airdrop(&f.pubkey(), 40_000_000_000);
    }
    let relay = Keypair::new_from_array([0xEE; 32]);
    w.airdrop(&relay.pubkey(), 200_000_000_000);
    let mut run = Run {
        w,
        k,
        relay,
        cfg,
        hold: 0,
    };
    run.k.start().await.unwrap();
    run.until(3_000, "genesis rings", |w| {
        w.season().effective_status(w.now()) == fclient::abi::status::RUNNING
            && ring_provinces(3)
                .iter()
                .all(|pc| w.ip.lock().account(&w.addrs.province(pc.p, pc.q)).is_some())
    })
    .await;
    println!(
        "genesis done at bell {}; regions {:?}",
        run.w.bell(),
        run.cfg.regions
    );

    // Pre-funded addresses: the anchor and cache of a coming bell.
    let bp = run.w.bell() + 2;
    for a in [
        run.w.addrs.anchor(bp, r0),
        run.w.addrs.seed_cache(bp, r0, 0),
    ] {
        run.w.airdrop(&a, 3_000_000);
    }

    // ---- an honest cohort in wedge 0 (region r0)
    let w0 = sites_in(&run.w, 0, r0);
    assert!(w0.len() >= 6, "wedge 0 region {r0}: {} sites", w0.len());
    let contested = w0[0];
    let mut plan: Vec<(Keypair, Vec<SiteKey>)> = vec![];
    for i in 0..3u8 {
        plan.push((wallet(i), vec![contested, w0[1 + i as usize]]));
    }
    plan.push((wallet(3), vec![w0[4]]));
    for (kp, _) in &plan {
        run.join(kp, 0).await;
    }
    let b1 = run.next_bell().await;
    for (kp, s) in &plan {
        run.file(kp, s).await;
    }
    // A keeper crash mid-bell (half a bell in).
    for _ in 0..40 {
        run.tick().await;
    }
    run.crash().await;
    let who: Vec<Address> = plan.iter().map(|(k, _)| k.pubkey()).collect();
    run.until(2_000, "cohort 1 settled", |w| {
        who.iter().all(|k| {
            let a = w.ip.lock().account(&w.addrs.citizen(k)).unwrap();
            Citizen::decode(&a.data).unwrap().ticket_bell == land::NO_TICKET
        })
    })
    .await;
    println!("cohort 1 (bell {b1}) settled");

    // ---- a held anchor: the keeper is away for the first 15 slots of a bell.
    run.next_bell().await;
    run.hold = 15;
    run.next_bell().await;

    // ---- a displacement in a second cohort (wedge 1, region r1)
    let w1 = sites_in(&run.w, 1, r1);
    assert!(w1.len() >= 2, "wedge 1 region {r1}");
    let z = w1[0];
    let (d1, d2) = (wallet(10), wallet(11));
    run.join(&d1, 1).await;
    run.join(&d2, 1).await;
    let b2 = run.next_bell().await;
    run.k.cfg.roles.retain(|r| r != "tickets");
    run.cfg.roles.retain(|r| r != "tickets");
    run.file(&d1, &[z]).await;
    run.file(&d2, &[z]).await;
    let rz = region_of(ProvinceCoord::new(z.0 as i32, z.1 as i32));
    let mut found = None;
    for _ in 0..800 {
        let slot = run.w.slot();
        found = run
            .k
            .seeds
            .find(&run.w.ip, &run.w.addrs, &run.k.beacon.anchors, b2, rz, slot)
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
        (&d2, &d1)
    } else {
        (&d1, &d2)
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
                b2,
                &[site_z],
                sz.src,
                None,
            )],
            &[&run.relay],
        )
        .await;
    run.k.cfg.roles.push("tickets".into());
    run.cfg.roles.push("tickets".into());
    let hi_pk = hi.pubkey();
    run.until(400, "displacement", move |w| {
        let a = w.ip.lock().account(&w.addrs.citizen(&hi_pk)).unwrap();
        Citizen::decode(&a.data).unwrap().ticket_bell == land::NO_TICKET
    })
    .await;
    assert_eq!(run.k.tickets.displacing, 1, "the keeper displaced");
    println!("displacement in cohort {b2} at {z:?}");

    // ---- a ticket settled expired
    let x = wallet(20);
    run.join(&x, 1).await;
    let xs = w1[1];
    run.k.cfg.roles.retain(|r| r != "tickets");
    run.cfg.roles.retain(|r| r != "tickets");
    run.file(&x, &[xs]).await;
    let tb = {
        let a = run
            .w
            .ip
            .lock()
            .account(&run.w.addrs.citizen(&x.pubkey()))
            .unwrap();
        Citizen::decode(&a.data).unwrap().ticket_bell
    };
    run.until(24 * 90, "24 bells", move |w| w.bell() >= tb + 24)
        .await;
    run.k.cfg.roles.push("tickets".into());
    run.cfg.roles.push("tickets".into());
    let xk = x.pubkey();
    run.until(80, "expired", move |w| {
        let a = w.ip.lock().account(&w.addrs.citizen(&xk)).unwrap();
        Citizen::decode(&a.data).unwrap().ticket_bell == land::NO_TICKET
    })
    .await;
    assert!(run.ticket_done(&x));
    // A few more slots so the last landings are in the feed.
    for _ in 0..4 {
        run.tick().await;
    }
    let bad: Vec<_> = run
        .k
        .alerts
        .iter()
        .filter(|a| a.1 != "not-implemented")
        .collect();
    println!("keeper alerts: {bad:?}");
    let inp = run
        .w
        .record(
            "the test-beacon program (scripts/build-frontier.sh --features test-beacon) over localnet in process at 20x, keeper W3 duties (beacon, rings, fold, tickets); record_land_program",
            &[
                "genesis",
                "honest-cohort",
                "keeper-crash-mid-bell",
                "held-anchor",
                "prefunded-addresses",
                "displacement",
                "expired-ticket",
            ],
        )
        .await;
    let r = verify_core::verify(&inp);
    println!("{}", r.markdown());
    let path = fixtures_dir().join("land-program.json");
    inp.save(&path).unwrap();
    println!("wrote {} ({} txs)", path.display(), inp.txs.len());
    assert_eq!(r.verdict, verify_core::Verdict::Pass, "{}", show(&r));
}

#[test]
#[ignore = "rewrites fixtures/verify/march-synth.json"]
fn record_march_synth() {
    let inp = verify_core::fixture::march::march();
    let r = verify_core::verify(&inp);
    println!("{}", r.markdown());
    let path = fixtures_dir().join("march-synth.json");
    inp.save(&path).unwrap();
    println!("wrote {} ({} txs)", path.display(), inp.txs.len());
    assert_eq!(r.verdict, verify_core::Verdict::Pass, "{}", show(&r));
}
