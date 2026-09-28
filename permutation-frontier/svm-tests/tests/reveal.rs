//! Reveal (0x51, §5.11; W3-B) on the test-beacon build (any round, I-53),
//! plus its gates: G2 pre-funding of ArrivalSlot and ArrivalDay, G3
//! forgeries of every account Reveal reads, G4's Reveal side ("no Reveal
//! lands at `now ≥ A + W`, once the BeaconLog holds a round ≥ S, after the
//! first gather, or for a tombstoned bell"), G9 quota fairness over reveal
//! orders, and the error codes (G13).
//!
//! Crafted accounts (`world::holding`, module note): Provinces, Citizens
//! and Holdings as W3-A's instructions leave them; a present ClashInputs
//! (GatherClash is W4-A's) and an AnchorArchive (ArchiveAnchors is W4-B's)
//! as those instructions leave them; ArrivalSlots written as Reveal leaves
//! them where a test needs full slots of other citizens.

mod common;

use frontier_abi::layout::beacon::bell_anchor as BA;
use frontier_abi::layout::clash::{arrival_day as AD, arrival_slot as AS, clash_inputs as CI};
use frontier_abi::layout::player::{holding as H, transit as T};
use frontier_abi::layout::province::{province as P, site as SM};
use frontier_abi::layout::world::{beacon_log as BL, season as S};
use frontier_abi::layout::{write_header, AccountKind};
use frontier_abi::log::Kind;
use permutation_frontier_svm_tests::chain::{
    assert_code, expect_lands, with_account, with_writable, without_signer, Chain, SendResult,
};
use permutation_frontier_svm_tests::fixtures::tlock::SealCase;
use permutation_frontier_svm_tests::ix::reveal::at;
use permutation_frontier_svm_tests::probe;
use permutation_frontier_svm_tests::records::{self, le};
use permutation_frontier_svm_tests::world::holding::{
    open_path, trace, u32_at, u64_at, Estate, March,
};
use permutation_frontier_svm_tests::world::{archive_part, day_of, World};
use permutation_frontier_svm_tests::{
    sha256, Address, FrontierError as E, Instruction, Rng, Signer,
};
use permutation_rules::frontier::beacon as kb;
use permutation_rules::frontier::clash::{
    admit_arrival, quota_set, SlotDecision, SlotEntry, QUICKNET,
};
use permutation_rules::frontier::geometry::ProvinceCoord;

const B0: u32 = 10;

/// One march ready to reveal.
struct Scene {
    c: Chain,
    w: World,
    e: Estate,
    m: March,
    region: u8,
}

fn send_keeper(c: &mut Chain, w: &World, ix: Instruction) -> SendResult {
    c.send(&[ix], &[&w.keeper])
}

/// A departed host of estate "a" (faction 0) marching `dirs` from its
/// holding's tile, arriving at `B0 + lead`.
fn scene_with(dirs: &[u8], lead: u32, stance: u8, case: SealCase) -> Scene {
    let (mut c, w) = common::test_beacon();
    w.to_bell(&mut c, B0, 5);
    let e = w.craft_estate(&mut c, "a", 0, (2, 0), 0);
    let id = w.craft_host(&mut c, &e, &e.province, 0, 0, 0, 900, e.tile);
    let origin = (e.p as i32, e.q as i32, e.tile);
    open_path(&w, &mut c, origin, dirs);
    let m = w.plan_march(id, 0, origin, dirs, B0 + lead, stance, 0, case);
    let tip = w.tip_min(&c);
    expect_lands(
        c.send(&[w.depart_ix(&e, (e.p, e.q), &m, tip)], &[&e.wallet]),
        "Depart",
    );
    let region = World::region(m.dest.0, m.dest.1);
    Scene { c, w, e, m, region }
}

fn scene() -> Scene {
    scene_with(&[0, 0, 0, 0, 0, 0, 0, 0], 6, 1, SealCase::Valid)
}

impl Scene {
    fn ix(&self, target: u8, day_w: bool) -> Instruction {
        self.w
            .reveal_ix(&self.w.keeper.pubkey(), &self.e, &self.m, target, day_w)
    }
    /// THE anchor of the arrival posted (Clock at T(arrive)'s time).
    fn anchor(&mut self) {
        expect_lands(
            self.w.post_anchor(&mut self.c, self.m.arrive, self.region),
            "PostAnchor",
        );
    }
    fn send(&mut self, ix: Instruction) -> SendResult {
        send_keeper(&mut self.c, &self.w, ix)
    }
    fn slot(&self, i: u8) -> Address {
        self.w.a.arrival_slot(
            self.m.dest.0,
            self.m.dest.1,
            self.m.arrive,
            self.e.faction,
            i,
        )
    }
    fn day(&self) -> Address {
        self.w
            .a
            .arrival_day(self.m.dest.0, self.m.dest.1, day_of(self.m.arrive))
    }
    fn close(&self) -> i64 {
        let a = self
            .w
            .anchor_a(&self.c, self.m.arrive, self.region)
            .unwrap();
        kb::reveal_close(a, self.w.window(&self.c, self.m.arrive))
    }
}

#[test]
fn reveal_fills_a_slot_and_creates_the_day() {
    let mut s = scene();
    s.anchor();
    let keeper = s.w.keeper.pubkey();
    let before = s.c.lamports(&keeper);
    let l = expect_lands(s.send(s.ix(0, true)), "s.ix(0, true)");
    // The fee payer paid the fee and both rents.
    let rents = s.c.rent(AS::SIZE) + s.c.rent(AD::SIZE);
    assert_eq!(before - s.c.lamports(&keeper), rents + l.fee);
    let d = s.c.data(&s.slot(0));
    assert_eq!(d.len(), AS::SIZE);
    assert_eq!(u64_at(&d, AS::HOST_ID), s.m.host_id);
    assert_eq!(u64_at(&d, AS::CITIZEN_TAG), s.e.citizen_tag());
    assert_eq!(u32_at(&d, AS::DEP_MASS), 900_000);
    assert_eq!(d[AS::STANCE], 1);
    assert_eq!(d[AS::TILE], s.m.dest_tile);
    assert_eq!(d[AS::FLAGS], AS::FLAG_CREATED_DAY);
    assert_eq!(d[AS::RENT_TO..AS::RENT_TO + 32], *keeper.as_ref());
    assert_eq!(d[AS::BENEFICIARY..AS::BENEFICIARY + 32], *keeper.as_ref());
    assert_eq!(u64_at(&d, AS::EV_SLOT), s.c.slot, "evidence: landing slot");
    assert_eq!(u32_at(&d, AS::EV_LIMIT), 1_400_000, "evidence: CU limit");
    let dd = s.c.data(&s.day());
    let (at_, m) = AD::bit(s.m.arrive);
    assert!(dd[at_] & m != 0);
    assert_eq!(dd[AD::RENT_TO..AD::RENT_TO + 32], *keeper.as_ref());
    let r = records::one(&l.logs, Kind::REVEAL);
    assert_eq!(r.u64("host_id"), s.m.host_id);
    assert_eq!(r.u64("arrivalday_created") as u8, 1);
    assert!(r.links.is_empty(), "REVEAL chains nothing");
    // Nothing else is written: the Holding and the Provinces are unchanged.
    // The repeat is AlreadyDone (the host holds a slot).
    assert_code(s.send(s.ix(0, false)), E::AlreadyDone);
    assert!(l.tx_bytes <= 1_100, "Reveal tx {} B", l.tx_bytes);
}

#[test]
fn reveal_writes_nothing_else() {
    let mut s = scene();
    s.anchor();
    let keys = [s.e.holding, s.e.province, s.e.citizen, s.w.a.season];
    let before: Vec<_> = keys.iter().map(|k| s.c.data(k)).collect();
    let ix = s.ix(0, true);
    let dest = ix.accounts[at::DEST].pubkey;
    let dest0 = s.c.data(&dest);
    expect_lands(s.send(ix), "s.ix(0, true)");
    for (k, b) in keys.iter().zip(before) {
        assert_eq!(s.c.data(k), b, "{k} unchanged");
    }
    assert_eq!(s.c.data(&dest), dest0, "destination unchanged");
}

/// Four slots of other citizens of faction 0 at the arrival, masses given.
fn fill_others(s: &mut Scene, masses: [u32; 4]) -> Vec<SlotEntry> {
    let mut v = vec![];
    for (i, mass) in masses.iter().enumerate() {
        let host = 0x7000 + i as u64;
        let tag = 0x1000 + i as u64;
        s.w.craft_slot(&mut s.c, s.m.dest, s.m.arrive, 0, i as u8, host, tag, *mass);
        v.push(SlotEntry {
            host_id: host,
            citizen: tag,
            troops: *mass,
        });
    }
    let day = day_of(s.m.arrive);
    s.w.craft_day(&mut s.c, s.m.dest, day, &[s.m.arrive]);
    v
}

#[test]
fn reveal_quota_displaces_the_lowest_and_refuses_the_rest() {
    let mut s = scene();
    s.anchor();
    let others = fill_others(&mut s, [500_000, 100_000, 800_000, 100_000]);
    let dest = ProvinceCoord::new(s.m.dest.0, s.m.dest.1);
    let me = SlotEntry {
        host_id: s.m.host_id,
        citizen: s.e.citizen_tag(),
        troops: 900_000,
    };
    let slots = [
        Some(others[0]),
        Some(others[1]),
        Some(others[2]),
        Some(others[3]),
    ];
    let SlotDecision::Displace { slot, displaced } = admit_arrival(dest, s.m.arrive, &slots, me)
    else {
        panic!("displaces")
    };
    // The wrong target moved: SlotMoved (the keeper re-reads and retries).
    let wrong = (slot + 1) % 4;
    assert_code(s.send(s.ix(wrong, false)), E::SlotMoved);
    // The day bit is set: the day may stay read-only.
    let l = expect_lands(s.send(s.ix(slot, false)), "s.ix(slot, false)");
    let d = s.c.data(&s.slot(slot));
    assert_eq!(u64_at(&d, AS::HOST_ID), s.m.host_id);
    assert_eq!(d[AS::FLAGS], 0, "a displacement clears created_day");
    assert_eq!(d[AS::CLAIMED], 0);
    assert_eq!(
        d[AS::RENT_TO..AS::RENT_TO + 32],
        *s.w.keeper.pubkey().as_ref(),
        "rent_to kept"
    );
    let r = records::one(&l.logs, Kind::REVEAL);
    assert_eq!(r.u64("displace") as u8, 1);
    assert_eq!(r.u64("displaced_host"), displaced.host_id);
    // Every slot larger: refused, nothing written.
    let mut s2 = scene();
    s2.anchor();
    fill_others(&mut s2, [950_000, 960_000, 970_000, 980_000]);
    assert_code(s2.send(s2.ix(0, false)), E::QuotaRefused);
}

/// G9 (§13.3): whatever order reveals land in (the keeper retries a
/// `SlotMoved`), the final slots are `quota_set` of the arrivals, one per
/// citizen.
#[test]
fn g09_reveal_quota_is_order_free() {
    let (mut c, w) = common::test_beacon();
    w.to_bell(&mut c, B0, 5);
    let dest = (3i32, -1i32);
    w.craft_province(&mut c, dest.0 as i16, dest.1 as i16);
    let n_sites = c.data(&w.a.province(dest.0, dest.1))[P::SITE_COUNT].min(6);
    let mut rng = Rng::new(0x6909);
    let target_tile = 30u8;
    let mut marches = vec![];
    let mut estates: Vec<Estate> = vec![];
    for k in 0..n_sites {
        let e = w.craft_estate(
            &mut c,
            &format!("q{k}"),
            0,
            (dest.0 as i16, dest.1 as i16),
            k,
        );
        // Two hosts for the first citizen (one arrival per citizen).
        let hosts = if k == 0 { 2 } else { 1 };
        for h in 0..hosts {
            let troops = 100 + rng.below(4) as u32 * 100;
            let id = w.craft_host(
                &mut c,
                &e,
                &e.province,
                (2 * k + h) as usize,
                h as u32,
                0,
                troops,
                e.tile,
            );
            let origin = (dest.0, dest.1, e.tile);
            let dirs = walk_to(origin, target_tile);
            open_path(&w, &mut c, origin, &dirs);
            let m = w.plan_march(id, h, origin, &dirs, B0 + 8, 0, 0, SealCase::Valid);
            let tip = w.tip_min(&c);
            expect_lands(
                c.send(&[w.depart_ix(&e, (e.p, e.q), &m, tip)], &[&e.wallet]),
                "Depart",
            );
            marches.push((e.citizen_tag(), troops * 1000, m, k as usize));
        }
        estates.push(e);
    }
    let region = World::region(dest.0, dest.1);
    expect_lands(w.post_anchor(&mut c, B0 + 8, region), "PostAnchor");
    let pc = ProvinceCoord::new(dest.0, dest.1);
    let all: Vec<SlotEntry> = marches
        .iter()
        .map(|(tag, mass, m, _)| SlotEntry {
            host_id: m.host_id,
            citizen: *tag,
            troops: *mass,
        })
        .collect();
    let want = quota_set(pc, B0 + 8, &all);
    for order in 0..8 {
        let mut f = c.fork();
        let mut idx: Vec<usize> = (0..marches.len()).collect();
        let mut r = Rng::new(order);
        for i in (1..idx.len()).rev() {
            idx.swap(i, r.below(i as u64 + 1) as usize);
        }
        for &i in &idx {
            let (_, _, m, k) = &marches[i];
            let e = &estates[*k];
            // The keeper's target: admit_arrival over the slots it reads.
            let slots: [Option<SlotEntry>; 4] = core::array::from_fn(|j| {
                let d = f.data(&w.a.arrival_slot(dest.0, dest.1, B0 + 8, 0, j as u8));
                (d.len() == AS::SIZE).then(|| SlotEntry {
                    host_id: u64_at(&d, AS::HOST_ID),
                    citizen: u64_at(&d, AS::CITIZEN_TAG),
                    troops: u32_at(&d, AS::DEP_MASS),
                })
            });
            let me = all[i];
            let day_w = f
                .data(&w.a.arrival_day(dest.0, dest.1, day_of(B0 + 8)))
                .is_empty();
            match admit_arrival(pc, B0 + 8, &slots, me) {
                SlotDecision::Fill { slot } | SlotDecision::Displace { slot, .. } => {
                    let ix = w.reveal_ix(&w.keeper.pubkey(), e, m, slot, day_w);
                    expect_lands(f.send(&[ix], &[&w.keeper]), "Reveal");
                }
                SlotDecision::Refuse(_) => {
                    let ix = w.reveal_ix(&w.keeper.pubkey(), e, m, 0, day_w);
                    let r = f.send(&[ix], &[&w.keeper]);
                    assert!(r.is_err(), "refused arrivals write nothing");
                }
            }
        }
        let mut got: Vec<SlotEntry> = (0..4u8)
            .filter_map(|j| {
                let d = f.data(&w.a.arrival_slot(dest.0, dest.1, B0 + 8, 0, j));
                (d.len() == AS::SIZE).then(|| SlotEntry {
                    host_id: u64_at(&d, AS::HOST_ID),
                    citizen: u64_at(&d, AS::CITIZEN_TAG),
                    troops: u32_at(&d, AS::DEP_MASS),
                })
            })
            .collect();
        got.sort_by_key(|e| e.host_id);
        assert_eq!(got, want, "order {order}: final slots = quota_set");
    }
}

/// Directions from tile `origin.2` to `tile` inside one province (a greedy
/// hex walk; the province is crafted open along it).
fn walk_to(origin: (i32, i32, u8), tile: u8) -> Vec<u8> {
    let pc = ProvinceCoord::new(origin.0, origin.1);
    let goal = pc.tile(tile).unwrap();
    let mut h = pc.tile(origin.2).unwrap();
    let mut dirs = vec![];
    while h != goal {
        let (d, next) = permutation_rules::hex::DIRECTIONS
            .iter()
            .enumerate()
            .map(|(d, (dq, dr))| (d, permutation_rules::hex::Hex::new(h.q + dq, h.r + dr)))
            .min_by_key(|(_, n)| n.distance(goal))
            .unwrap();
        dirs.push(d as u8);
        h = next;
    }
    if dirs.is_empty() {
        // Same tile: out and back.
        dirs = vec![0, 3];
    }
    dirs
}

// ------------------------------------------------------------ G4, the Reveal side

#[test]
fn g04_reveal_window_closes_at_a_plus_w() {
    let mut s = scene();
    s.anchor();
    let close = s.close();
    let mut f = s.c.fork();
    f.set_time(close - 1);
    expect_lands(
        send_keeper(&mut f, &s.w, s.ix(0, true)),
        "s.ix(0, true) before close",
    );
    s.c.set_time(close);
    assert_code(s.send(s.ix(0, true)), E::WindowClosed);
}

#[test]
fn g04_reveal_refused_once_the_beacon_log_holds_s() {
    let mut s = scene();
    s.anchor();
    let a = s.w.anchor_a(&s.c, s.m.arrive, s.region).unwrap();
    let sr = kb::seed_round(&QUICKNET, s.close(), s.w.params.season.seed_margin);
    let _ = a;
    // A BeaconLog already at S(A) (as a keeper that posted S early would
    // leave it; the Clock is still before the close).
    let log = s.w.a.beacon_log(s.region);
    let mut f = s.c.fork();
    f.edit(&log, |d| {
        d[BL::LATEST_ROUND..BL::LATEST_ROUND + 8].copy_from_slice(&sr.to_le_bytes())
    });
    assert_code(send_keeper(&mut f, &s.w, s.ix(0, true)), E::WindowClosed);
    let mut f = s.c.fork();
    f.edit(&log, |d| {
        d[BL::LATEST_ROUND..BL::LATEST_ROUND + 8].copy_from_slice(&(sr - 1).to_le_bytes())
    });
    expect_lands(
        send_keeper(&mut f, &s.w, s.ix(0, true)),
        "s.ix(0, true) at S − 1",
    );
    // Through the program: PostBeacon of S (its round time is after the
    // close), then the Reveal refuses.
    expect_lands(s.w.post_beacon(&mut s.c, s.region, sr), "PostBeacon(S)");
    assert_code(s.send(s.ix(0, true)), E::WindowClosed);
}

#[test]
fn g04_reveal_latch_after_the_first_gather_or_resolve() {
    let mut s = scene();
    s.anchor();
    let (dp, dq) = s.m.dest;
    // A present ClashInputs of the province-bell (GatherClash, W4-A).
    let ci = s.w.a.clash_inputs(dp, dq, s.m.arrive);
    let mut f = s.c.fork();
    let mut d = vec![0u8; CI::SIZE];
    assert!(write_header(&mut d, AccountKind::ClashInputs, s.w.id));
    f.put_program_account(ci, d);
    assert_code(send_keeper(&mut f, &s.w, s.ix(0, true)), E::LatchClosed);
    // A pre-funded ClashInputs address is absent (lamports ignored).
    let mut f = s.c.fork();
    f.prefund(&ci, 12_345);
    expect_lands(
        send_keeper(&mut f, &s.w, s.ix(0, true)),
        "s.ix(0, true) with a pre-funded inputs address",
    );
    // The destination resolved past the arrival.
    let dest = s.w.a.province(dp, dq);
    s.w.set_resolved_next(&mut s.c, &dest, s.m.arrive + 1);
    assert_code(s.send(s.ix(0, true)), E::LatchClosed);
}

#[test]
fn g04_reveal_refused_for_a_tombstoned_bell() {
    let mut s = scene();
    // No anchor; the region-half-day archive tombstones the arrival bell.
    s.w.to_anchor_time(&mut s.c, s.m.arrive);
    let part = archive_part(s.m.arrive);
    let mut f = s.c.fork();
    s.w.craft_archive(&mut f, s.region, part, &[s.m.arrive]);
    assert_code(send_keeper(&mut f, &s.w, s.ix(0, true)), E::Archived);
    // Another bell of the same archive: open (owner submissions before THE
    // anchor are allowed).
    let mut f = s.c.fork();
    s.w.craft_archive(&mut f, s.region, part, &[s.m.arrive + 1]);
    expect_lands(
        send_keeper(&mut f, &s.w, s.ix(0, true)),
        "s.ix(0, true) before THE anchor",
    );
}

/// G4 as a property: over random anchor times, Clocks and window lengths,
/// a Reveal lands iff `now < A + W` and the BeaconLog is below `S(A)`.
#[test]
fn g04_reveal_window_property() {
    let mut rng = Rng::new(0x6404_0003);
    for case in 0..24 {
        let window = [600u32, 900, 1_200, 1_800][case % 4];
        let (mut c, w) = {
            let mut c = Chain::test_beacon();
            let mut p = permutation_frontier_svm_tests::world::params_for(&c);
            p.reveal_window = window;
            let w = World::with_params(&mut c, 1, p);
            expect_lands(w.announce(&mut c), "announce");
            expect_lands(w.create(&mut c), "create");
            expect_lands(w.init_logs(&mut c), "logs");
            for f in 0..6 {
                expect_lands(w.init_shards(&mut c, f), "shards");
            }
            expect_lands(w.consume_genesis(&mut c), "genesis");
            let g = w.genesis_ts();
            c.set_time(g);
            (c, w)
        };
        w.to_bell(&mut c, B0, 5);
        let e = w.craft_estate(&mut c, "a", 0, (2, 0), 0);
        let id = w.craft_host(&mut c, &e, &e.province, 0, 0, 0, 900, e.tile);
        let origin = (e.p as i32, e.q as i32, e.tile);
        let dirs = [0u8, 0, 0];
        open_path(&w, &mut c, origin, &dirs);
        let m = w.plan_march(id, 0, origin, &dirs, B0 + 4, 0, 0, SealCase::Valid);
        let tip = w.tip_min(&c);
        expect_lands(
            c.send(&[w.depart_ix(&e, (e.p, e.q), &m, tip)], &[&e.wallet]),
            "Depart",
        );
        let region = World::region(m.dest.0, m.dest.1);
        // THE anchor lands 0–300 s after T(arrive) is published.
        w.to_anchor_time(&mut c, m.arrive);
        c.advance(rng.range(0, 300) as i64);
        expect_lands(w.post_anchor(&mut c, m.arrive, region), "PostAnchor");
        let a = w.anchor_a(&c, m.arrive, region).unwrap();
        let close = kb::reveal_close(a, window);
        let sr = kb::seed_round(&QUICKNET, close, w.params.season.seed_margin);
        let t = a + rng.range(0, window as u64 + 120) as i64;
        let latest = if rng.coin() {
            sr - 1 - rng.below(3)
        } else {
            sr + rng.below(2)
        };
        let mut f = c.fork();
        if f.now < t {
            f.set_time(t);
        }
        let log = w.a.beacon_log(region);
        f.edit(&log, |d| {
            d[BL::LATEST_ROUND..BL::LATEST_ROUND + 8].copy_from_slice(&latest.to_le_bytes())
        });
        let ix = w.reveal_ix(&w.keeper.pubkey(), &e, &m, 0, true);
        let r = f.send(&[ix], &[&w.keeper]);
        let open = f.now < close && latest < sr;
        if open {
            expect_lands(r, "open window");
        } else {
            assert_code(r, E::WindowClosed);
        }
    }
}

// ------------------------------------------------------------ G2 pre-funding

#[test]
fn g02_prefund_arrival_slot_and_day_reveal() {
    let mut s = scene();
    s.anchor();
    let (rs, rd) = (s.c.rent(AS::SIZE), s.c.rent(AD::SIZE));
    for (ps, pd) in [(1u64, 1u64), (rs, rd), (10 * rs, 10 * rd), (rs, 0), (0, rd)] {
        let mut f = s.c.fork();
        if ps > 0 {
            f.prefund(&s.slot(0), ps);
        }
        if pd > 0 {
            f.prefund(&s.day(), pd);
        }
        let keeper = s.w.keeper.pubkey();
        let before = f.lamports(&keeper);
        let l = expect_lands(
            send_keeper(&mut f, &s.w, s.ix(0, true)),
            "Reveal on pre-funded addresses",
        );
        let paid = before - f.lamports(&keeper) - l.fee;
        assert_eq!(
            paid,
            rs.saturating_sub(ps) + rd.saturating_sub(pd),
            "the payer pays only the shortfalls"
        );
        common::assert_program_account(&f, &s.slot(0), AS::MAGIC, AS::SIZE, s.w.id);
        common::assert_program_account(&f, &s.day(), AD::MAGIC, AD::SIZE, s.w.id);
    }
    // A pre-funded, never-created slot counts as absent in the quota read:
    // the arrival fills slot 0.
    let mut f = s.c.fork();
    for i in 0..4 {
        f.prefund(&s.slot(i), 777);
    }
    expect_lands(
        send_keeper(&mut f, &s.w, s.ix(0, true)),
        "Reveal over pre-funded slots",
    );
}

// ------------------------------------------------------------ G3 forgeries

/// Every account Reveal reads, forged (§13.2): a wrong address, owner,
/// magic, season or key field is refused with its pinned code.
#[test]
fn g03_forgery_reveal_accounts() {
    let mut s = scene();
    s.anchor();
    // A second march with a path province, for the path-province rows.
    let base = s.ix(0, true);
    let fresh = Address::new_from_array(sha256(&[b"fresh address"]));
    let dest = base.accounts[at::DEST].pubkey;
    // Wrong address (absent elsewhere): BadAddress for every account whose
    // absence is a claim (anchor, archive, inputs, day, slots) and for the
    // log and the destination; the Holding must be present (BadAccount).
    for pos in [
        at::ANCHOR,
        at::ARCHIVE,
        at::BEACONLOG,
        at::INPUTS,
        at::DEST,
        at::DAY,
        at::SLOT0 + 1,
    ] {
        let ix = with_account(base.clone(), pos, fresh);
        assert_code(send_keeper(&mut s.c.fork(), &s.w, ix), E::BadAddress);
    }
    let ix = with_account(base.clone(), at::HOLDING, fresh);
    assert_code(send_keeper(&mut s.c.fork(), &s.w, ix), E::BadAccount);
    // Present copies at a non-canonical address: BadAddress (Holding,
    // Province), WrongRegion (THE anchor's kind elsewhere).
    for (pos, src) in [(at::HOLDING, s.e.holding), (at::DEST, dest)] {
        let mut f = s.c.fork();
        let k = common::copy_to_fresh(&mut f, &src, format!("copy {pos}").as_bytes());
        assert_code(
            send_keeper(&mut f, &s.w, with_account(base.clone(), pos, k)),
            E::BadAddress,
        );
    }
    let other = (s.region + 1) % 16;
    let mut f = s.c.fork();
    expect_lands(
        s.w.post_anchor(&mut f, s.m.arrive, other),
        "PostAnchor other region",
    );
    let ix = with_account(base.clone(), at::ANCHOR, s.w.a.anchor(s.m.arrive, other));
    assert_code(send_keeper(&mut f, &s.w, ix), E::WrongRegion);
    let ix = with_account(base.clone(), at::BEACONLOG, s.w.a.beacon_log(other));
    assert_code(send_keeper(&mut s.c.fork(), &s.w, ix), E::WrongRegion);
    // Wrong owner, magic, season, key fields of present accounts.
    let anchor = s.w.a.anchor(s.m.arrive, s.region);
    let log = s.w.a.beacon_log(s.region);
    type Edit = fn(&mut [u8]);
    let edits: [(&str, Edit); 3] = [
        ("magic", |d| d[0] ^= 1),
        ("season", |d| d[8] ^= 1),
        ("tail", |_| {}),
    ];
    for k in [s.e.holding, anchor, log, dest] {
        for (what, ed) in edits.iter().take(2) {
            let mut f = s.c.fork();
            f.edit(&k, *ed);
            let r = send_keeper(&mut f, &s.w, base.clone());
            assert_code(r, E::BadAccount);
            let _ = what;
        }
        let mut f = s.c.fork();
        f.set_owner(&k, Address::new_from_array([7; 32]));
        assert_code(send_keeper(&mut f, &s.w, base.clone()), E::BadAccount);
    }
    let _ = edits[2];
    // Key fields: THE anchor's bell, the log's region, the destination's P.
    let mut f = s.c.fork();
    f.edit(&anchor, |d| d[BA::BELL] ^= 1);
    assert_code(send_keeper(&mut f, &s.w, base.clone()), E::BadAccount);
    let mut f = s.c.fork();
    f.edit(&log, |d| d[BL::REGION] ^= 1);
    assert_code(send_keeper(&mut f, &s.w, base.clone()), E::BadAccount);
    let mut f = s.c.fork();
    f.edit(&dest, |d| d[P::P] ^= 1);
    assert_code(send_keeper(&mut f, &s.w, base.clone()), E::BadAccount);
    // A present slot or day with a wrong key field.
    let mut f = s.c.fork();
    s.w.craft_slot(&mut f, s.m.dest, s.m.arrive, 0, 2, 99, 99, 1);
    f.edit(&s.slot(2), |d| d[AS::I] = 3);
    assert_code(send_keeper(&mut f, &s.w, base.clone()), E::BadAccount);
    let mut f = s.c.fork();
    s.w.craft_day(&mut f, s.m.dest, day_of(s.m.arrive), &[]);
    f.edit(&s.day(), |d| d[AD::DAY] ^= 1);
    assert_code(send_keeper(&mut f, &s.w, base.clone()), E::BadAccount);
    // The instructions sysvar must be the sysvar.
    let n = base.accounts.len();
    let ix = with_account(base.clone(), n - 2, fresh);
    assert_code(send_keeper(&mut s.c.fork(), &s.w, ix), E::BadAccount);
}

#[test]
fn g03_forgery_reveal_path_provinces() {
    // A march over three provinces: forged, missing, extra and reordered
    // path provinces are refused (`Path`, `BadAddress`, `BadAccount`).
    let dirs = [0u8; 16];
    let mut s = scene_with(&dirs, 10, 0, SealCase::Valid);
    assert!(
        !s.m.path.is_empty(),
        "the march crosses provinces: {:?}",
        s.m.path
    );
    s.anchor();
    let base = s.ix(0, true);
    let k = s.m.path.len();
    let first = base.accounts[at::PATH0].pubkey;
    // Missing a path province.
    let mut short = base.clone();
    short.accounts.remove(at::PATH0);
    assert_code(send_keeper(&mut s.c.fork(), &s.w, short), E::Path);
    // An extra one (a real, present province not on the path).
    let spare = s.w.craft_province(&mut s.c, 0, 5);
    let mut extra = base.clone();
    extra.accounts.insert(
        at::PATH0 + k,
        permutation_frontier_svm_tests::AccountMeta::new_readonly(spare, false),
    );
    assert_code(send_keeper(&mut s.c.fork(), &s.w, extra), E::Path);
    if k >= 2 {
        let mut swapped = base.clone();
        swapped.accounts.swap(at::PATH0, at::PATH0 + 1);
        assert_code(send_keeper(&mut s.c.fork(), &s.w, swapped), E::Path);
    }
    // A copy of a path province at another address; the wrong owner.
    let mut f = s.c.fork();
    let copy = common::copy_to_fresh(&mut f, &first, b"path copy");
    assert_code(
        send_keeper(&mut f, &s.w, with_account(base.clone(), at::PATH0, copy)),
        E::BadAddress,
    );
    let mut f = s.c.fork();
    f.set_owner(&first, Address::new_from_array([9; 32]));
    assert_code(send_keeper(&mut f, &s.w, base.clone()), E::BadAccount);
    // An impassable step.
    let steps = trace((s.e.p as i32, s.e.q as i32, s.e.tile), &dirs);
    let st = steps[3];
    let pk = s.w.a.province(st.p, st.q);
    let mut f = s.c.fork();
    f.edit(&pk, |d| {
        let m = u64_at(d, P::PASSABLE_MASK) & !(1u64 << st.tile);
        d[P::PASSABLE_MASK..P::PASSABLE_MASK + 8].copy_from_slice(&m.to_le_bytes());
    });
    assert_code(send_keeper(&mut f, &s.w, base.clone()), E::Path);
    expect_lands(s.send(base), "s.ix(0, true) over three provinces");
}

// ------------------------------------------------------------ codes

#[test]
fn reveal_refusals() {
    let mut s = scene();
    s.anchor();
    let base = s.ix(0, true);
    // BadPlaintext: a plaintext Plain::validate refuses (its commitment is
    // checked after).
    let mut m = s.m.clone();
    m.made.plain[19..21].copy_from_slice(&60_001u16.to_le_bytes());
    let ix = s.w.reveal_ix(&s.w.keeper.pubkey(), &s.e, &m, 0, true);
    assert_code(send_keeper(&mut s.c.fork(), &s.w, ix), E::BadPlaintext);
    // CommitMismatch: another salt.
    let mut m = s.m.clone();
    m.made.salt[0] ^= 1;
    let ix = s.w.reveal_ix(&s.w.keeper.pubkey(), &s.e, &m, 0, true);
    assert_code(send_keeper(&mut s.c.fork(), &s.w, ix), E::CommitMismatch);
    // TransitState: a free transit record.
    let mut m = s.m.clone();
    m.transit_slot = 3;
    let ix = s.w.reveal_ix(&s.w.keeper.pubkey(), &s.e, &m, 0, true);
    assert_code(send_keeper(&mut s.c.fork(), &s.w, ix), E::TransitState);
    // NeedArrivalDay: the first reveal of the bell with the day read-only.
    assert_code(
        send_keeper(&mut s.c.fork(), &s.w, s.ix(0, false)),
        E::NeedArrivalDay,
    );
    // BadData: a target outside the four slots; BadAccount: two writable.
    let mut t4 = base.clone();
    t4.data[2] = 4;
    assert_code(send_keeper(&mut s.c.fork(), &s.w, t4), E::BadData);
    let two = with_writable(base.clone(), at::SLOT0 + 2, true);
    assert_code(send_keeper(&mut s.c.fork(), &s.w, two), E::BadAccount);
    // Auth: the fee payer must sign.
    let unsigned = without_signer(base.clone(), at::FEE_PAYER);
    let other = s.c.funded(b"outer payer", 1);
    assert_code(s.c.fork().send(&[unsigned], &[&other]), E::Auth);
    // WrongStatus: the season ended before the arrival bell.
    let mut f = s.c.fork();
    f.edit(&s.w.a.season, |d| {
        d[S::STATUS] = S::STATUS_ENDED;
        d[S::END_BELL..S::END_BELL + 4].copy_from_slice(&s.m.arrive.to_le_bytes());
    });
    assert_code(send_keeper(&mut f, &s.w, base.clone()), E::WrongStatus);
    // ... also with a bad plaintext: step 1 comes first (§5.11, wave-3
    // review).
    let mut m = s.m.clone();
    m.made.plain[19..21].copy_from_slice(&60_001u16.to_le_bytes());
    let ix = s.w.reveal_ix(&s.w.keeper.pubkey(), &s.e, &m, 0, true);
    assert_code(send_keeper(&mut f, &s.w, ix), E::WrongStatus);
    // RulesetMismatch.
    let mut f = s.c.fork();
    f.edit(&s.w.a.season, |d| d[S::RULESET_HASH] ^= 1);
    assert_code(send_keeper(&mut f, &s.w, base.clone()), E::RulesetMismatch);
    // Shielded: the destination tile is another faction's shielded site.
    let mut f = s.c.fork();
    let dest = s.w.a.province(s.m.dest.0, s.m.dest.1);
    let site_of_tile = {
        let d = f.data(&dest);
        (0..d[P::SITE_COUNT] as usize).find(|&k| d[P::SITES + k] == s.m.dest_tile)
    };
    let k = match site_of_tile {
        Some(k) => k,
        None => {
            f.edit(&dest, |d| d[P::SITES] = s.m.dest_tile);
            0
        }
    };
    f.edit(&dest, |d| {
        let o = P::site(k);
        d[o + SM::STATE] = SM::STATE_HOLDING;
        d[o + SM::FACTION] = 3;
        d[o + SM::SHIELD_UNTIL_BELL..o + SM::SHIELD_UNTIL_BELL + 4]
            .copy_from_slice(&(s.m.arrive + 1).to_le_bytes());
    });
    assert_code(send_keeper(&mut f, &s.w, base.clone()), E::Shielded);
    // … and a shielded host may not target another faction's site.
    f.edit(&dest, |d| {
        let o = P::site(k);
        d[o + SM::SHIELD_UNTIL_BELL..o + SM::SHIELD_UNTIL_BELL + 4]
            .copy_from_slice(&0u32.to_le_bytes());
    });
    f.edit(&s.e.holding, |d| {
        d[H::SHIELD_UNTIL..H::SHIELD_UNTIL + 8].copy_from_slice(&i64::MAX.to_le_bytes())
    });
    assert_code(send_keeper(&mut f, &s.w, base.clone()), E::Shielded);
    // TooManyAccounts.
    let mut short = base.clone();
    short.accounts.truncate(10);
    assert_code(
        send_keeper(&mut s.c.fork(), &s.w, short),
        E::TooManyAccounts,
    );
    expect_lands(s.send(base), "s.ix(0, true)");
}

#[test]
fn reveal_arrival_bell_must_fit_the_path() {
    // 20 steps (≥ 2,400 s): arriving 2 bells after the departure is too early.
    let dirs = [0u8; 20];
    let mut s = scene_with(&dirs, 2, 0, SealCase::Valid);
    s.anchor();
    assert_code(s.send(s.ix(0, true)), E::ArrivalBell);
}

#[test]
fn reveal_refuses_a_cpi() {
    let mut s = scene();
    s.anchor();
    let so = probe::load();
    let k = Address::new_from_array(sha256(&[b"cpi probe"]));
    s.c.deploy(k, &so, so.len(), None);
    let via = probe::cpi(k, &s.ix(0, true));
    assert_code(s.send(via), E::NotTopLevel);
}

#[test]
fn reveal_a_bad_seal_is_revealable_and_judged_at_settlement() {
    // Reveal does not open the seal (I-44): a garbage seal with a valid
    // commitment reveals like any other; SettleTransit (W4-B) judges it.
    let mut s = scene_with(&[0, 0, 0, 0, 0, 0, 0, 0], 6, 1, SealCase::Garbage);
    s.anchor();
    expect_lands(s.send(s.ix(0, true)), "s.ix(0, true) of a garbage seal");
    let hd = s.c.data(&s.e.holding);
    assert_eq!(hd[H::transit(0) + T::STATE], T::STATE_DEPARTED);
    assert_eq!(
        le(&hd[H::transit(0) + T::HOST_ID..H::transit(0) + T::HOST_ID + 8]),
        s.m.host_id
    );
}

// ------------------------------------------------------------ G1: the worst Reveal

/// The longest straight line from a tile of `origin` whose steps enter
/// exactly four provinces (origin included when entered, destination
/// last), ending with a step that enters the destination; an even length
/// so the pad is back-and-forth pairs across that border.
fn worst_line(origin: (i32, i32)) -> (u8, u8, usize) {
    let mut best = (0u8, 0u8, 0usize);
    for tile in 0..61u8 {
        for dir in 0..6u8 {
            for n in (2..=32usize).rev().step_by(2) {
                let steps = trace((origin.0, origin.1, tile), &vec![dir; n]);
                let path = permutation_frontier_svm_tests::world::holding::path_provinces(&steps);
                let last = steps[n - 1];
                let prev = steps[n - 2];
                if path.len() == 3 && (prev.p, prev.q) != (last.p, last.q) && n > best.2 {
                    best = (tile, dir, n);
                }
            }
        }
    }
    assert!(best.2 > 0, "a four-province line exists");
    best
}

use permutation_frontier_svm_tests::chain::Build;

/// Prints the trace build's CU checkpoints (`PSF_TRACE=1`): each
/// `sol_log_64(0x4355, tag, heap, 0, 0)` line is followed by the runtime's
/// "consumption: N units remaining"; the differences are the CU of each
/// step.
fn print_profile(logs: &[String]) {
    let mut last: Option<(u64, u64)> = None;
    let mut tag = None;
    for l in logs {
        if let Some(rest) = l.strip_prefix("Program log: 0x4355, ") {
            tag = rest
                .split(", ")
                .next()
                .and_then(|t| u64::from_str_radix(t.trim_start_matches("0x"), 16).ok());
        } else if let Some(i) = l.find("consumption: ") {
            let n: u64 = l[i + 13..]
                .split(' ')
                .next()
                .and_then(|x| x.parse().ok())
                .unwrap_or(0);
            if let Some(t) = tag.take() {
                if let Some((pt, pn)) = last {
                    println!(
                        "    trace {pt:#06x} → {t:#06x}: {} CU",
                        pn.saturating_sub(n)
                    );
                }
                last = Some((t, n));
            }
        }
    }
}

/// One worst-case fill of §13.1's Reveal row.
#[derive(Clone, Copy, Debug)]
enum Fill {
    /// The row as named: displacement of the smallest of 4 full slots and
    /// the first reveal of the province-bell (ArrivalDay created on a
    /// pre-funded address). Crafted: honest play cannot have full slots
    /// without the day's bit, so this bounds both.
    Named,
    /// Honest: the first reveal of the province-bell (fill, day created).
    FirstOfBell,
    /// Honest: displacement of the smallest of 4 full slots (day present).
    Displace,
}

/// Measures Reveal at `fill` on `build`: 32 steps over 4 provinces (the
/// last 2k steps back and forth across the destination's border, one
/// province re-location each), THE anchor present, BeaconLog read.
fn measure_reveal(
    build: Build,
    fill: Fill,
) -> (
    permutation_frontier_svm_tests::budget::Need,
    Chain,
    World,
    Instruction,
) {
    use permutation_frontier_svm_tests::world::holding::padded_line;
    let mut c = Chain::new(build);
    let w = World::running(&mut c, 1);
    w.to_bell(&mut c, B0, 5);
    let origin_pv = (2i32, 0i32);
    let (tile, dir, n) = worst_line(origin_pv);
    println!(
        "  worst line: tile {tile}, direction {dir}, {n} straight steps, then {} back and forth",
        32 - n
    );
    let e = w.craft_estate(
        &mut c,
        "worst",
        0,
        (origin_pv.0 as i16, origin_pv.1 as i16),
        0,
    );
    // A Knight host of 30,000 (the largest mass) on the line's first tile.
    let origin = (origin_pv.0, origin_pv.1, tile);
    let dirs = padded_line(origin, dir, n, 32);
    open_path(&w, &mut c, origin, &dirs);
    // Rough tiles all along (the dearer step cost, same CU).
    let id = w.craft_host(&mut c, &e, &e.province, 0, 0, 0, 30_000, tile);
    let m = w.plan_march(id, 0, origin, &dirs, B0 + 20, 2, 25_000, SealCase::Valid);
    assert_eq!(m.path.len(), 3, "4 provinces");
    assert_eq!(m.plain.path_len, 32);
    let tip = w.tip_min(&c);
    expect_lands(
        c.send(&[w.depart_ix(&e, (e.p, e.q), &m, tip)], &[&e.wallet]),
        "Depart",
    );
    let region = World::region(m.dest.0, m.dest.1);
    w.to_anchor_time(&mut c, m.arrive);
    let (a, slot) = (c.now, c.slot);
    w.craft_anchor(&mut c, m.arrive, region, a, slot);
    let day = w.a.arrival_day(m.dest.0, m.dest.1, day_of(m.arrive));
    let mut target = 0u8;
    if matches!(fill, Fill::Named | Fill::Displace) {
        // Four arrivals of other citizens of the faction, just below ours
        // and equal among themselves (ranked by their slot keys).
        let pc = ProvinceCoord::new(m.dest.0, m.dest.1);
        let mut slots = [None; 4];
        for i in 0..4u8 {
            let host = 0x5100 + i as u64;
            let tag = 0x5200 + i as u64;
            w.craft_slot(&mut c, m.dest, m.arrive, 0, i, host, tag, 29_000_000);
            slots[i as usize] = Some(SlotEntry {
                host_id: host,
                citizen: tag,
                troops: 29_000_000,
            });
        }
        let me = SlotEntry {
            host_id: id,
            citizen: e.citizen_tag(),
            troops: 30_000_000,
        };
        let SlotDecision::Displace { slot, .. } = admit_arrival(pc, m.arrive, &slots, me) else {
            panic!("displaces")
        };
        target = slot;
    }
    match fill {
        Fill::Named | Fill::FirstOfBell => c.prefund(&day, 1),
        Fill::Displace => {
            w.craft_day(&mut c, m.dest, day_of(m.arrive), &[m.arrive]);
        }
    }
    let day_w = !matches!(fill, Fill::Displace);
    let ix = w.reveal_ix(&w.keeper.pubkey(), &e, &m, target, day_w);
    let need = c
        .measure(std::slice::from_ref(&ix), &[&w.keeper])
        .unwrap_or_else(|f| panic!("{fill:?}: refused: {f:?}"));
    println!("  measured {fill:?}: {need}");
    if std::env::var("PSF_TRACE").is_ok_and(|v| v == "1") && build == Build::Trace {
        let l = expect_lands(
            c.fork().send(std::slice::from_ref(&ix), &[&w.keeper]),
            "traced Reveal",
        );
        print_profile(&l.logs);
    }
    // The client profile (budget CU limit, L(kind)) lands too (the trace
    // build's markers cost ≈ 5k CU more).
    if build == Build::Release {
        expect_lands(
            c.fork()
                .send_client(std::slice::from_ref(&ix), &[&w.keeper]),
            "Reveal at the client profile",
        );
    }
    (need, c, w, ix)
}

/// G1 for Reveal (§13.1, W3-B's measurement): CU, tx bytes and loaded data
/// against the current release `.so` at the worst fills; heap on the trace
/// build when it is built. Gate W3: ≤ 26,000 CU, tx ≤ 1,100 B, `L(reveal)`
/// measured. Figures printed (`--nocapture`) and recorded in W3-B-NOTES.
#[test]
fn g01_reveal_worst() {
    use frontier_abi::tags::Ix;
    use permutation_frontier_svm_tests::budget::{assert_within, ceilings};
    use permutation_frontier_svm_tests::chain::{so_path, Build, Profile, PAGE};
    let mut worst = 0u64;
    for fill in [Fill::Named, Fill::FirstOfBell, Fill::Displace] {
        let (need, ..) = measure_reveal(Build::Release, fill);
        let c = Chain::release();
        let pd = c.programdata_len();
        println!(
            "Reveal worst ({fill:?}), release .so ({} B, max_len {pd}): {need}",
            c.so_len
        );
        if so_path(Build::Trace).is_some() {
            let (t, ..) = measure_reveal(Build::Trace, fill);
            println!(
                "Reveal worst ({fill:?}), trace build: heap {:?} B, {} CU (trace markers included)",
                t.heap, t.cu
            );
            if let Some(h) = t.heap {
                assert!(h <= 28_672, "heap {h} B");
            }
        }
        let mut ceil = ceilings(Ix::Reveal, 0, pd);
        ceil.tx_bytes = ceil.tx_bytes.min(1_100);
        assert_within(&format!("Reveal {fill:?}"), &need, &ceil);
        worst = worst.max(need.cu);
    }
    let c = Chain::release();
    let l_formula = frontier_abi::budgets::loaded_limit_for(Ix::Reveal, c.programdata_len());
    println!(
        "Reveal: worst {worst} CU (budget 26,000, target 20,000); L(reveal) at this .so = {l_formula} B (formula, worst account set), requested {} B",
        permutation_frontier_svm_tests::chain::loaded_limit(Ix::Reveal, c.programdata_len())
    );
    assert!(worst <= 26_000);
    // The measured loaded need of the named fill, and the SIMD-0186
    // control: one page below it fails charged.
    let (need, c, w, ix) = measure_reveal(Build::Release, Fill::Named);
    let l_meas = (need.loaded as u32).div_ceil(PAGE) * PAGE;
    println!("Reveal: loaded {} B → L_measured = {l_meas} B", need.loaded);
    assert!(l_meas <= l_formula, "the formula covers the measured set");
    // SIMD-0186 control on this set: lands at L_measured, one page below
    // fails charged.
    let pd = c.programdata_len();
    let at = Profile::client(Ix::Reveal, pd).with_loaded(l_meas);
    expect_lands(
        c.fork()
            .send_with(&at, std::slice::from_ref(&ix), &[&w.keeper]),
        "Reveal at L_measured",
    );
    let below = Profile::client(Ix::Reveal, pd).with_loaded(l_meas - PAGE);
    permutation_frontier_svm_tests::chain::assert_loaded_exceeded(c.fork().send_with(
        &below,
        std::slice::from_ref(&ix),
        &[&w.keeper],
    ));
    // W5-A: g01_loaded_limit_reveal against the release binary (the named
    // fill is Reveal's worst account set: 18 accounts, three path provinces).
    common::loaded_check(&c, Ix::Reveal, &[ix], &[&w.keeper]);
}
