//! Clashes (§5.11; W4-A): GatherClash, ResolveFromInputs, ResolveClash
//! (oracle), SkipQuiet, CloseClashInputs, CloseArrivalDay,
//! CloseArrivalSlot and the return settle (§21), with their gates: G1 (RFI
//! over all 1,240 fills with the full M1 write-back, the I-43 storage fill,
//! SkipQuiet's CU-aware stop), G2 (ClashInputs pre-funding, re-creation),
//! G3 (forgeries), G6, G8, G9 (the clash side), G11 and G13 rows.
//!
//! Crafted accounts (`world::clash`, `world::holding`, module notes):
//! Provinces with rosters, pending musters, departed entries and garrisons
//! as OpenProvince, Muster, SettleTicket and earlier resolves leave them;
//! ArrivalSlots, ArrivalDays and the transit records of their Holdings as
//! Reveal and SettleDeparture leave them; THE anchor and a SeedCache as
//! PostAnchor and PostSeed leave them (release-build measurements cannot
//! verify later rounds); a settled ArrivalSlot as SettleTransit (W4-B)
//! leaves it. Every test that relies on one says so.

mod common;

use frontier_abi::entry::{read_entry, write_entry, Entry, EntryOp};
use frontier_abi::layout::clash::{arrival as AR, arrival_slot as AS, clash_inputs as CI};
use frontier_abi::layout::player::{holding as H, transit as T};
use frontier_abi::layout::province::{camp as CP, entry as EN, province as P, site as SM};
use frontier_abi::layout::world::season as S;
use frontier_abi::log::{EntityKind, Kind};
use permutation_frontier_svm_tests::budget::{assert_within, ceilings};
use permutation_frontier_svm_tests::chain::{
    assert_code, expect_lands, with_account, with_writable, without_signer, Build, Chain,
    SendResult,
};
use permutation_frontier_svm_tests::ix::clash::{self as cix, gather_at, resolve_at, skip_at};
use permutation_frontier_svm_tests::ix::host as hix;
use permutation_frontier_svm_tests::records::{self, ChainWatch};
use permutation_frontier_svm_tests::world::clash::{
    bell_seed, entry_of, roster_fill, Fill, THIRDS,
};
use permutation_frontier_svm_tests::world::holding::{u32_at, u64_at};
use permutation_frontier_svm_tests::world::{day_of, World};
use permutation_frontier_svm_tests::{Address, Instruction, Ix, Keypair, Rng, Signer};
use permutation_rules::frontier::clash::Fighter;
use permutation_rules::frontier::geometry::{region_of, ProvinceCoord};
use permutation_rules::frontier::stance::{Posture, Stance};
use permutation_rules::units::UnitType;

use permutation_frontier_svm_tests::FrontierError as E;

/// The bell fills resolve at.
const B: u32 = 20;

fn keeper_send(c: &mut Chain, w: &World, ix: Instruction) -> SendResult {
    c.send(&[ix], &[&w.keeper])
}

/// A running season on `build`, the Clock in bell `B`.
fn world_on(build: Build) -> (Chain, World) {
    let mut c = Chain::new(build);
    let w = World::running(&mut c, 1);
    w.to_bell(&mut c, B, 5);
    (c, w)
}

/// A fill crafted at bell `B` with its bell ready (anchor, cache, Clock
/// past the close). Returns the seed.
fn ready(c: &mut Chain, w: &World, f: &Fill) -> [u8; 32] {
    w.craft_fill(c, f, B);
    w.ready_bell(c, B, f.region(), None)
}

/// Gathers the 24 positions in thirds and resolves; returns the RFI's
/// landed transaction.
fn gather_and_resolve(
    c: &mut Chain,
    w: &World,
    f: &Fill,
) -> permutation_frontier_svm_tests::chain::Landed {
    for ix in w.gather_parts(c, f.dest(), B, &THIRDS) {
        expect_lands(keeper_send(c, w, ix), "GatherClash");
    }
    expect_lands(
        keeper_send(c, w, w.resolve_ix(f.dest(), B)),
        "ResolveFromInputs",
    )
}

/// Fate codes of the arrival record (§5.3).
fn fate_code(f: &permutation_rules::frontier::clash::Fate) -> u8 {
    use permutation_rules::frontier::clash::Fate;
    match f {
        Fate::Stays { .. } => AR::FATE_STAYS,
        Fate::Withdrew { .. } => AR::FATE_WITHDREW,
        Fate::Bounced => AR::FATE_BOUNCED,
        Fate::Retreated => AR::FATE_RETREATED,
        Fate::Destroyed => AR::FATE_DESTROYED,
    }
}

/// The CLASH record's outcome digest.
fn clash_digest(logs: &[String]) -> [u8; 32] {
    let r = records::one(logs, Kind::CLASH);
    r.field("outcome_digest", true).try_into().unwrap()
}

// ================================================================ basics

#[test]
fn clash_gather_and_resolve_a_fill_as_the_kernel_does() {
    let (mut c, w) = world_on(Build::TestBeacon);
    let f = Fill::adversarial(2, 7, 4, -2);
    let seed = ready(&mut c, &w, &f);
    let native = f.native(&seed, B);
    let pk = w.a.province(f.p, f.q);
    let ck = w.a.clash_inputs(f.p, f.q, B);
    let pw = ChainWatch::new(&c, pk, EntityKind::Province);
    // gathers create the inputs (chained, GATHER records)
    let mut logs = vec![];
    for ix in w.gather_parts(&c, f.dest(), B, &THIRDS) {
        logs.extend(expect_lands(keeper_send(&mut c, &w, ix), "GatherClash").logs);
    }
    let cd = c.data(&ck);
    assert_eq!(u32_at(&cd, CI::ARRIVALS_MASK), CI::ALL_GATHERED);
    assert_eq!(cd[CI::N_PRESENT], 24);
    assert_eq!(
        cd[CI::RENT_TO..CI::RENT_TO + 32],
        *w.keeper.pubkey().as_ref()
    );
    assert_eq!(records::of_kind(&logs, Kind::GATHER).len(), 3);
    let cw = ChainWatch::new(&c, ck, EntityKind::ClashInputs);
    let l = expect_lands(
        keeper_send(&mut c, &w, w.resolve_ix(f.dest(), B)),
        "w.resolve_ix(",
    );
    pw.check(&c, &l.logs, 1);
    cw.check(&c, &l.logs, 1);
    for r in records::records(&l.logs) {
        records::check_tail(&r);
    }
    assert_eq!(clash_digest(&l.logs), native.digest(), "on chain = native");
    assert_eq!(w.last_digest(&c, f.dest()), native.digest());
    assert_eq!(w.resolved_next(&c, f.dest()), B + 1);
    // the fate table and the resolver
    let cd = c.data(&ck);
    assert_eq!(cd[CI::FLAGS] & CI::FLAG_RESOLVED, CI::FLAG_RESOLVED);
    assert_eq!(
        cd[CI::RESOLVER..CI::RESOLVER + 32],
        *w.keeper.pubkey().as_ref()
    );
    for (fa, i, a) in &f.arrivals {
        let k = CI::position(*fa, *i);
        let o = CI::arrival(k);
        let fr = native.fighter(a.id).unwrap();
        assert_eq!(cd[o + AR::FATE], fate_code(&fr.fate));
        assert_eq!(u32_at(&cd, o + AR::TROOPS_AFTER), fr.troops);
    }
    // residents and staying arrivals in the roster with their post-clash values
    let pd = c.data(&pk);
    for fr in &native.fighters {
        let at = frontier_abi::entry::find_entry(&pd, fr.id);
        match fr.fate {
            permutation_rules::frontier::clash::Fate::Stays { tile }
            | permutation_rules::frontier::clash::Fate::Withdrew { tile } => {
                let e = read_entry(&pd, at.expect("roster entry")).unwrap();
                assert_eq!((e.troops, e.tile), (fr.troops, tile), "{}", fr.id);
                assert_eq!(e.state, EN::STATE_ROSTER);
                if fr.arrival {
                    assert_eq!(e.from_bell, B + 1);
                    assert_eq!(e.stamina_value, fr.stamina);
                }
            }
            permutation_rules::frontier::clash::Fate::Destroyed if !fr.arrival => {
                assert!(at.is_none(), "destroyed resident freed")
            }
            permutation_rules::frontier::clash::Fate::Bounced if !fr.arrival => {
                let e = read_entry(&pd, at.unwrap()).unwrap();
                assert_eq!(
                    (e.state, e.op),
                    (EN::STATE_DEPARTED, EntryOp::Leave),
                    "sent home"
                );
            }
            _ => {}
        }
    }
    for g in &native.garrisons {
        let j = f.garrisons.iter().position(|x| x.id == g.id).unwrap();
        assert_eq!(u32_at(&pd, P::site(j) + SM::GARRISON), g.troops);
    }
    let n = (0..P::ENTRIES_N)
        .filter(|&i| read_entry(&pd, i).unwrap().state != EN::STATE_FREE)
        .count();
    assert_eq!(pd[P::N_ENTRIES] as usize, n);
    assert!(n <= 56);
}

#[test]
fn g06_a_gather_that_omits_a_present_slot_cannot_complete() {
    let (mut c, w) = world_on(Build::TestBeacon);
    let f = Fill::adversarial(0, 3, 6, 1);
    ready(&mut c, &w, &f);
    // position 0 is present: listing it without its Holding refuses
    let mut ix = w.gather_ix(&c, f.dest(), B, 0, 8);
    let d0 = ix.data.clone();
    let mut bad = d0.clone();
    let bm = u32::from_le_bytes(bad[7..11].try_into().unwrap()) & !1;
    bad[7..11].copy_from_slice(&bm.to_le_bytes());
    ix.data = bad;
    ix.accounts.remove(gather_at::SLOT0 + 8); // drop position 0's Holding
    assert_code(keeper_send(&mut c, &w, ix), E::BadData);
    // a resolve before every position is gathered
    let parts = w.gather_parts(&c, f.dest(), B, &THIRDS);
    expect_lands(keeper_send(&mut c, &w, parts[0].clone()), "GatherClash");
    assert_code(
        keeper_send(&mut c, &w, w.resolve_ix(f.dest(), B)),
        E::NotGathered,
    );
    // repeats are no-ops, the rest completes
    expect_lands(keeper_send(&mut c, &w, parts[0].clone()), "GatherClash");
    expect_lands(keeper_send(&mut c, &w, parts[2].clone()), "GatherClash");
    assert_code(
        keeper_send(&mut c, &w, w.resolve_ix(f.dest(), B)),
        E::NotGathered,
    );
    expect_lands(keeper_send(&mut c, &w, parts[1].clone()), "GatherClash");
    expect_lands(
        keeper_send(&mut c, &w, w.resolve_ix(f.dest(), B)),
        "ResolveFromInputs",
    );
    // a clear ArrivalDay bit gathers in one step, whatever the range
    let q = roster_fill(8, 1, 5, vec![]);
    w.craft_fill(&mut c, &q, B);
    w.ready_bell(&mut c, B, q.region(), None);
    let gix = w.gather_ix(&c, q.dest(), B, 5, 1);
    let l = expect_lands(keeper_send(&mut c, &w, gix), "GatherClash");
    let cd = c.data(&w.a.clash_inputs(q.p, q.q, B));
    assert_eq!(u32_at(&cd, CI::ARRIVALS_MASK), CI::ALL_GATHERED);
    assert_eq!(cd[CI::FLAGS], CI::FLAG_NO_ARRIVALS);
    assert_eq!(records::one(&l.logs, Kind::GATHER).u64("no_arrivals"), 1);
    expect_lands(
        keeper_send(&mut c, &w, w.resolve_ix(q.dest(), B)),
        "ResolveFromInputs",
    );
}

// ================================================================ G1

/// The 1,240 fills of §13.1: SP-V2's 40 and the 1,200 of the screen, at
/// distinct destinations (the lab's coordinates are outside ring 128,
/// where host ids do not exist).
fn all_fills() -> Vec<(&'static str, Fill)> {
    let mut v = vec![];
    let mut j = 0i32;
    let at = |j: i32| (-20 + j % 41, -20 + 3 * (j / 41));
    for (kind, seeds) in [
        (0u8, 1..=4u64),
        (1, 1..=4),
        (2, 1..=16),
        (3, 1..=4),
        (4, 1..=4),
        (5, 1..=8),
    ] {
        for seed in seeds {
            let (p, q) = at(j);
            v.push(("spv2-40", Fill::adversarial(kind, seed, p, q)));
            j += 1;
        }
    }
    for kind in [2u8, 3, 4, 5] {
        for seed in 100..400u64 {
            let (p, q) = at(j);
            v.push(("gen-1200", Fill::adversarial(kind, seed, p, q)));
            j += 1;
        }
    }
    v
}

/// One fill through gathers and the resolve on `c`: the resolve's need and
/// the gathers' worst CU; asserts the digest.
fn pipeline(
    c: &mut Chain,
    w: &World,
    f: &Fill,
) -> (permutation_frontier_svm_tests::budget::Need, u64, usize) {
    let seed = ready(c, w, f);
    let native = f.native(&seed, B);
    let mut worst_gather = (0u64, 0usize);
    for ix in w.gather_parts(c, f.dest(), B, &THIRDS) {
        let n = c
            .measure(std::slice::from_ref(&ix), &[&w.keeper])
            .expect("gather");
        worst_gather = worst_gather.max((n.cu, n.tx_bytes));
        expect_lands(keeper_send(c, w, ix), "GatherClash");
    }
    let rix = w.resolve_ix(f.dest(), B);
    let need = c
        .measure(std::slice::from_ref(&rix), &[&w.keeper])
        .expect("resolve");
    let l = expect_lands(keeper_send(c, w, rix), "ResolveFromInputs");
    assert_eq!(
        clash_digest(&l.logs),
        native.digest(),
        "{}: on chain = native",
        f.name
    );
    (need, worst_gather.0, worst_gather.1)
}

fn stats(v: &[u64]) -> (u64, u64, u64) {
    let mut s = v.to_vec();
    s.sort_unstable();
    let mean = s.iter().sum::<u64>() / s.len().max(1) as u64;
    (*s.first().unwrap_or(&0), mean, *s.last().unwrap_or(&0))
}

/// G1 / §13.1 (I-50): ResolveFromInputs over all 1,240 fills with the full
/// M1 write-back on the full-gather path (3 gathers), plus the I-43
/// storage fills: CU ≤ 340,000 on the release build, heap ≤ 28 KiB on the
/// trace build, every on-chain digest the native kernel's; gathers ≤ 40k.
#[test]
fn g01_resolve_from_inputs_all_fills() {
    let mut fills = all_fills();
    assert_eq!(fills.len(), 1_240);
    for (k, (r, d)) in [(40usize, 1usize), (40, 8), (30, 8), (20, 4)]
        .into_iter()
        .enumerate()
    {
        fills.push((
            "storage",
            Fill::storage(900 + k as u64, 30 + k as i32, 40, r, d),
        ));
    }
    let (mut c, w) = world_on(Build::Release);
    let (mut ct, wt) = world_on(Build::Trace);
    let pd = c.programdata_len();
    let ceil = ceilings(Ix::ResolveFromInputs, 0, pd);
    let gceil = ceilings(Ix::GatherClash, 0, pd);
    let (mut cus, mut heaps, mut gcu) = (vec![], vec![], vec![]);
    let mut worst = (0u64, String::new());
    let mut worst_heap = (0u32, String::new());
    let mut worst_bytes = 0usize;
    for (set, f) in &fills {
        let (need, g, gb) = pipeline(&mut c, &w, f);
        let (tneed, _, _) = pipeline(&mut ct, &wt, f);
        let heap = tneed.heap.expect("trace build heap");
        assert!(
            need.cu <= ceil.cu as u64,
            "{set} {}: {} CU",
            f.name,
            need.cu
        );
        assert!(heap <= ceil.heap, "{set} {}: heap {heap} B", f.name);
        assert!(need.tx_bytes as u32 <= ceil.tx_bytes);
        assert!(need.loaded <= ceil.loaded as u64);
        assert!(g <= gceil.cu as u64, "{set} {}: gather {g} CU", f.name);
        assert!(
            gb as u32 <= gceil.tx_bytes,
            "{set} {}: gather {gb} B",
            f.name
        );
        if need.cu > worst.0 {
            worst = (need.cu, format!("{set} {}", f.name));
        }
        if heap > worst_heap.0 {
            worst_heap = (heap, format!("{set} {}", f.name));
        }
        worst_bytes = worst_bytes.max(need.tx_bytes);
        cus.push(need.cu);
        heaps.push(heap as u64);
        gcu.push(g);
    }
    let (lo, mean, hi) = stats(&cus);
    let (hlo, hmean, hhi) = stats(&heaps);
    let (_, gmean, ghi) = stats(&gcu);
    println!(
        "G1 RFI {} fills: CU min {lo} mean {mean} max {hi} ({}); heap min {hlo} mean {hmean} max {hhi} ({}); tx {} B; gathers mean {gmean} max {ghi} CU; programdata {pd} B",
        fills.len(),
        worst.1,
        worst_heap.1,
        worst_bytes
    );
}

// ================================================================ G11 / SkipQuiet

/// A quiet roster at `(p, q)`: 6 factions × `per` residents, each faction
/// on its own two hexes (≤ 6 hosts a hex), and, with `churn`, a pending
/// change taking effect at every bell of `b0..b0 + 24` (a Leave, a
/// departure, a forfeit or a muster joining, in turn) and a garrison change.
fn quiet_world(p: i32, q: i32, seed: u64, per: usize, b0: u32, churn: bool) -> Fill {
    let base = Fill::adversarial(0, seed, p, q);
    let passable: Vec<u8> = (0..61u8)
        .filter(|&t| base.terrain.terrain[t as usize].is_passable())
        .collect();
    let mut rng = Rng::new(seed);
    let mut residents = vec![];
    for f in 0..6u8 {
        for n in 0..per {
            let tile = passable[2 * f as usize + n % 2];
            residents.push(Fighter {
                id: permutation_frontier_svm_tests::world::clash::resident_id(p, q, f, n),
                faction: f,
                unit: UnitType::Spearman,
                troops: 1_000_000 + rng.below(5_000) as u32 * 1_000,
                stamina: 30 + rng.below(90) as u16,
                tile,
                posture: Posture::Stance(Stance::Hold),
                retreat_bps: None,
                dealt_bps: permutation_frontier_svm_tests::world::clash::dealt(
                    f,
                    Posture::Stance(Stance::Hold),
                    false,
                ),
            });
        }
    }
    let mut f = roster_fill(p, q, seed, residents);
    // one holding of faction 0 on its own hex (garrison, walls)
    let gt = passable[0];
    f.garrisons
        .push(permutation_rules::frontier::clash::Garrison {
            id: permutation_frontier_svm_tests::world::clash::garrison_id(p, q, 0),
            faction: 0,
            tile: gt,
            troops: 2_000_000,
            walls: true,
            posture: Posture::Stance(Stance::Hold),
        });
    f.terrain.sites[0] = gt;
    f.terrain.site_count = 1;
    let _ = (b0, churn);
    f
}

/// Crafts `f` resolved through `b0 − 1`, then (with `churn`) a pending
/// change at every bell `b0 + k` (module note of `quiet_world`).
fn craft_quiet(c: &mut Chain, w: &World, f: &Fill, b0: u32, churn: bool) {
    w.craft_fill(c, f, b0);
    if !churn {
        return;
    }
    let pk = w.a.province(f.p, f.q);
    let n_res = f.residents.len();
    c.edit(&pk, |d| {
        for k in 0..24u32 {
            let i = (k as usize * 7) % n_res;
            let mut e = read_entry(d, i).unwrap();
            if e.op != EntryOp::None {
                continue;
            }
            e.pend_bell = b0 + k;
            e.op = match k % 3 {
                0 => EntryOp::Leave,
                1 => EntryOp::Spend { cost: 74 },
                _ => EntryOp::Forfeit,
            };
            write_entry(d, i, &e).unwrap();
        }
        // musters joining at b0 + 1 .. (faction 5's second hex, within
        // the caps Muster keeps)
        let free: Vec<usize> = (0..P::ENTRIES_N)
            .filter(|&i| read_entry(d, i).unwrap().state == EN::STATE_FREE)
            .collect();
        let mut free = free.into_iter();
        let per5 = f.residents.iter().filter(|r| r.faction == 5).count() as u32;
        for k in 0..4u32.min(8 - per5) {
            let j = free.next().unwrap();
            let mut x = *f.residents.iter().rev().find(|r| r.faction == 5).unwrap();
            x.id = permutation_frontier_svm_tests::world::clash::resident_id(
                f.p,
                f.q,
                5,
                20 + k as usize,
            );
            let mut e = entry_of(&x, EN::STATE_MUSTER_PENDING, b0);
            e.from_bell = b0 + 1 + k;
            write_entry(d, j, &e).unwrap();
        }
        // a garrison change at every other bell
        let o = P::site(0);
        d[o + SM::PEND0_BELL..o + SM::PEND0_BELL + 4].copy_from_slice(&(b0 + 3).to_le_bytes());
        d[o + SM::PEND0_DELTA..o + SM::PEND0_DELTA + 8].copy_from_slice(&500_000i64.to_le_bytes());
        d[o + SM::PEND1_BELL..o + SM::PEND1_BELL + 4].copy_from_slice(&(b0 + 9).to_le_bytes());
        d[o + SM::PEND1_DELTA..o + SM::PEND1_DELTA + 8]
            .copy_from_slice(&(-200_000i64).to_le_bytes());
        let n = (0..P::ENTRIES_N)
            .filter(|&i| read_entry(d, i).unwrap().state != EN::STATE_FREE)
            .count();
        d[P::N_ENTRIES] = n as u8;
    });
}

/// Anchors (and seed caches) for bells `b0..b0 + n` of `f`'s region, the
/// Clock past the last close.
fn ready_run(c: &mut Chain, w: &World, f: &Fill, b0: u32, n: u32) {
    for b in b0..b0 + n {
        w.ready_bell(c, b, f.region(), None);
    }
}

/// The Province's game state: every byte but the event header, the last
/// digest, the resolve summary and the quiet cache (G11's comparison).
fn state_bytes(c: &Chain, w: &World, f: &Fill) -> Vec<u8> {
    let d = c.data(&w.a.province(f.p, f.q));
    let mut v = d[64..P::LAST_DIGEST].to_vec();
    v.extend_from_slice(&d[P::LAST_DIGEST + 32..P::QUIET_OK]);
    v.extend_from_slice(&d[P::QUIET_OK + 1..P::RESOLVE_SUMMARY]);
    v.extend_from_slice(&d[P::RESOLVE_SUMMARY + 32..]);
    v
}

/// Gather (one step: no arrivals) and resolve bell `b`.
fn resolve_quiet_bell(c: &mut Chain, w: &World, f: &Fill, b: u32) {
    let ix = w.gather_ix(c, f.dest(), b, 0, 1);
    expect_lands(keeper_send(c, w, ix), "GatherClash");
    expect_lands(
        keeper_send(c, w, w.resolve_ix(f.dest(), b)),
        "ResolveFromInputs",
    );
}

/// The keeper's catch-up: SkipQuiet as far as it goes (prefixes), a gather
/// and a resolve where a bell is not quiet. Returns (skips, resolves, the
/// SKIP records' committed counts).
fn catch_up(c: &mut Chain, w: &World, f: &Fill, b0: u32, n: u32) -> (u32, u32, Vec<u64>) {
    let (mut skips, mut resolves, mut counts) = (0, 0, vec![]);
    loop {
        let rn = w.resolved_next(c, f.dest());
        if rn >= b0 + n {
            return (skips, resolves, counts);
        }
        let k = (b0 + n - rn).min(24) as u8;
        let ix = w.skip_ix(f.dest(), rn, k);
        match c.send(&[ix], &[&w.keeper]) {
            Ok(l) => {
                skips += 1;
                counts.push(records::one(&l.logs, Kind::SKIP).u64("n"));
            }
            Err(e) if e.code == Some(E::NotQuiet.code()) => {
                resolve_quiet_bell(c, w, f, rn);
                resolves += 1;
            }
            Err(e) => panic!("SkipQuiet: {} {:?}\n{}", e.err, e.code, e.logs.join("\n")),
        }
    }
}

/// G11 (§13.3): SkipQuiet over a run leaves the Province a gather and a
/// resolve of every bell would, for random rosters with and without
/// pending changes, rosters changed at every bell (the CU-aware stop
/// commits prefixes whose sum is the run), and a camp check crossing a
/// day boundary.
#[test]
fn g11_skip_quiet_equals_resolving_every_bell() {
    let b0 = 130u32; // the run crosses day 1 (bell 144): the camp's check
    for (seed, per, churn) in [
        (11u64, 3usize, false),
        (12, 8, false),
        (13, 5, true),
        (14, 8, true),
        (15, 1, true),
    ] {
        let (mut c, w) = world_on(Build::TestBeacon);
        let f = quiet_world(10, 3, seed, per, b0, churn);
        w.to_bell(&mut c, b0, 1);
        craft_quiet(&mut c, &w, &f, b0, churn);
        ready_run(&mut c, &w, &f, b0, 24);
        let mut a = c.fork();
        let mut b = c.fork();
        let (skips, resolves, counts) = catch_up(&mut a, &w, &f, b0, 24);
        for bell in b0..b0 + 24 {
            resolve_quiet_bell(&mut b, &w, &f, bell);
        }
        assert_eq!(w.resolved_next(&a, f.dest()), b0 + 24);
        assert_eq!(
            state_bytes(&a, &w, &f),
            state_bytes(&b, &w, &f),
            "seed {seed}: skip and resolve leave the same Province ({skips} skips {counts:?}, {resolves} resolves)"
        );
        assert!(skips >= 1, "seed {seed}");
        println!("G11 seed {seed} per {per} churn {churn}: {skips} skips {counts:?}, {resolves} resolves");
    }
}

/// G1 / §13.1 SkipQuiet: 24 bells, 48 residents with a pending change at
/// every bell (the quiet test recomputed every bell), 2 ArrivalDays, 24
/// anchor keys: within `60k + 30k × recomputed bells`, the stop committing
/// a prefix within the limit; and an idle 24-bell run (one quiet test).
#[test]
fn g01_skip_quiet_budget() {
    let b0 = 130u32;
    for churn in [false, true] {
        let (mut c, w) = world_on(Build::Release);
        let f = quiet_world(12, 5, 21, 8, b0, churn);
        w.to_bell(&mut c, b0, 1);
        craft_quiet(&mut c, &w, &f, b0, churn);
        ready_run(&mut c, &w, &f, b0, 24);
        let ix = w.skip_ix(f.dest(), b0, 24);
        let need = c
            .measure(std::slice::from_ref(&ix), &[&w.keeper])
            .expect("SkipQuiet");
        let l = expect_lands(keeper_send(&mut c, &w, ix), "SkipQuiet");
        let n = records::one(&l.logs, Kind::SKIP).u64("n") as u32;
        // quiet tests: 1 without churn; with churn every bell after a change
        let recomputed = if churn {
            n
        } else {
            1 + (b0 % 144 + n > 144) as u32
        };
        if churn {
            assert_eq!(
                n, 24,
                "a churned but trivially quiet roster skips the whole run"
            );
        }
        let ceil = ceilings(Ix::SkipQuiet, recomputed, c.programdata_len());
        println!("G1 SkipQuiet churn {churn}: {n} bells, {recomputed} quiet tests");
        assert_within(&format!("SkipQuiet churn {churn}"), &need, &ceil);
    }
}

// ================================================================ §21: hosts that leave

/// An estate with a roster host at bell `B0` of a test-beacon season
/// (crafted Province, Citizen and Holding: `world::holding`).
const B0: u32 = 10;

fn estate() -> (
    Chain,
    World,
    permutation_frontier_svm_tests::world::holding::Estate,
    u64,
) {
    let (mut c, w) = common::test_beacon();
    w.to_bell(&mut c, B0, 5);
    let e = w.craft_estate(&mut c, "a", 0, (2, 0), 0);
    let id = w.craft_host(&mut c, &e, &e.province, 0, 0, 0, 1_200, e.tile);
    w.set_reserve(&mut c, &e, 0, 50);
    (c, w, e, id)
}

fn dest_of(e: &permutation_frontier_svm_tests::world::holding::Estate) -> (i32, i32) {
    (e.p as i32, e.q as i32)
}

/// Skips bell `b` of the estate's province (THE anchor crafted, the Clock
/// past the close).
fn skip_bell(
    c: &mut Chain,
    w: &World,
    dest: (i32, i32),
    b: u32,
) -> permutation_frontier_svm_tests::chain::Landed {
    let region = region_of(ProvinceCoord::new(dest.0, dest.1));
    w.ready_bell(c, b, region, None);
    expect_lands(keeper_send(c, w, w.skip_ix(dest, b, 1)), "w.skip_ix(")
}

fn reserve(
    c: &Chain,
    e: &permutation_frontier_svm_tests::world::holding::Estate,
    unit: usize,
) -> u32 {
    u32_at(&c.data(&e.holding), H::reserve(unit))
}

/// §21 (for W4-A): the resolve or skip of the Dissolve's bell keeps the
/// Leave entry (state 3, its post-clash troops); the return settle
/// (SettleDeparture, `transit_slot = 0xFF`) credits `reserve += troops /
/// 1,000` and frees it; a second is `AlreadyDone`.
#[test]
fn clash_dissolve_returns_troops_to_the_reserve() {
    let (mut c, w, e, id) = estate();
    let dest = dest_of(&e);
    let ix = hix::dissolve(&w.a, &e.player(), e.href(), id);
    expect_lands(c.send(&[ix], &[&e.wallet]), "Dissolve");
    for path in ["skip", "resolve"] {
        let mut f = c.fork();
        if path == "skip" {
            skip_bell(&mut f, &w, dest, B0);
        } else {
            let region = region_of(ProvinceCoord::new(dest.0, dest.1));
            w.ready_bell(&mut f, B0, region, None);
            let g = w.gather_ix(&f, dest, B0, 0, 1);
            expect_lands(keeper_send(&mut f, &w, g), "GatherClash");
            expect_lands(
                keeper_send(&mut f, &w, w.resolve_ix(dest, B0)),
                "ResolveFromInputs",
            );
        }
        let en = w.entries(&f, dest);
        let (_, x) = en.iter().find(|(_, x)| x.id == id).expect("kept");
        assert_eq!(
            (x.state, x.op),
            (EN::STATE_DEPARTED, EntryOp::Leave),
            "{path}"
        );
        assert_eq!(x.troops, 1_200_000);
        let watch = [
            ChainWatch::new(&f, e.holding, EntityKind::Holding),
            ChainWatch::new(&f, e.province, EntityKind::Province),
        ];
        let rix = cix::settle_return(&w.a, w.keeper.pubkey(), (e.p, e.q), e.href());
        let l = expect_lands(keeper_send(&mut f, &w, rix.clone()), "cix::settle_return(");
        for wch in &watch {
            wch.check(&f, &l.logs, 1);
        }
        let rec = records::one(&l.logs, Kind::DEPARTURE_SETTLED);
        assert_eq!(rec.key_u64("host_id"), id);
        assert_eq!(rec.u64("destroyed"), 2, "a return");
        assert_eq!(rec.u64("troops_after"), 1_200_000);
        assert_eq!(reserve(&f, &e, 0), 50 + 1_200, "{path}: whole troops back");
        assert!(w.entries(&f, dest).iter().all(|(_, x)| x.id != id), "freed");
        assert_code(keeper_send(&mut f, &w, rix), E::AlreadyDone);
    }
}

/// The return settle of a host whose Holding was re-founded (another
/// generation) or is gone: the troops are lost (`STRANDED`), the entry
/// freed. (A resident bounced home by its clash also leaves as a Leave;
/// that path has no test of its own yet: integ-W4 review, W5-A.)
#[test]
fn clash_return_settle_loses_troops_of_a_refounded_holding() {
    let (mut c, w, e, id) = estate();
    let dest = dest_of(&e);
    expect_lands(
        c.send(
            &[hix::dissolve(&w.a, &e.player(), e.href(), id)],
            &[&e.wallet],
        ),
        "Dissolve",
    );
    skip_bell(&mut c, &w, dest, B0);
    c.edit(&e.holding, |d| d[H::GEN] = 2);
    let l = expect_lands(
        keeper_send(
            &mut c,
            &w,
            cix::settle_return(&w.a, w.keeper.pubkey(), (e.p, e.q), e.href()),
        ),
        "cix::settle_return(",
    );
    assert_eq!(
        records::one(&l.logs, Kind::STRANDED).u64("troops_lost"),
        1_200_000
    );
    assert_eq!(reserve(&c, &e, 0), 50);
    assert!(w.entries(&c, dest).iter().all(|(_, x)| x.id != id));
    // a Holding that is gone: the same, at its canonical absent address
    let (mut c, w, e, id) = estate();
    expect_lands(
        c.send(
            &[hix::dissolve(&w.a, &e.player(), e.href(), id)],
            &[&e.wallet],
        ),
        "Dissolve",
    );
    skip_bell(&mut c, &w, dest_of(&e), B0);
    c.remove(&e.holding);
    let l = expect_lands(
        keeper_send(
            &mut c,
            &w,
            cix::settle_return(&w.a, w.keeper.pubkey(), (e.p, e.q), e.href()),
        ),
        "cix::settle_return(",
    );
    assert_eq!(records::of_kind(&l.logs, Kind::STRANDED).len(), 1);
}

/// v1.5 §5.10: DisbandStranded's pending `Forfeit` (a roster host of a
/// Holding that is gone) is freed by the resolve or skip of its bell, the
/// roster frozen until then.
#[test]
fn clash_forfeit_is_freed_by_the_settle_of_its_bell() {
    let (mut c, w, e, _) = estate();
    let dest = dest_of(&e);
    // a host of a Holding that does not exist (site 5, never founded)
    let stranded = fclient::addr::host_id(e.p as i32, e.q as i32, 5, 1, 3).unwrap();
    c.edit(&e.province, |d| {
        let mut en = read_entry(d, 0).unwrap();
        en.id = stranded;
        write_entry(d, 1, &en).unwrap();
        d[P::N_ENTRIES] += 1;
    });
    let ix = hix::disband_stranded(&w.a, w.keeper.pubkey(), e.p, e.q, 1, stranded);
    expect_lands(keeper_send(&mut c, &w, ix), "DisbandStranded");
    let en = read_entry(&c.data(&e.province), 1).unwrap();
    assert_eq!((en.state, en.op), (EN::STATE_ROSTER, EntryOp::Forfeit));
    skip_bell(&mut c, &w, dest, B0);
    assert_eq!(read_entry(&c.data(&e.province), 1).unwrap(), Entry::FREE);
    assert_eq!(w.resolved_next(&c, dest), B0 + 1);
}

/// A departure through the program's own instructions: Depart at `B0`,
/// the origin's skip of `B0` settles the Spend (state 3, post-clash
/// values), SettleDeparture then moves them into the transit record.
#[test]
fn clash_departure_settles_after_the_origin_resolve() {
    let (mut c, w, e, id) = estate();
    let dest = dest_of(&e);
    let origin = (e.p as i32, e.q as i32, e.tile);
    let dirs = [0u8, 0];
    permutation_frontier_svm_tests::world::holding::open_path(&w, &mut c, origin, &dirs);
    let m = w.plan_march(
        id,
        0,
        origin,
        &dirs,
        B0 + 4,
        0,
        0,
        permutation_frontier_svm_tests::fixtures::tlock::SealCase::Valid,
    );
    let tip = w.tip_min(&c);
    expect_lands(
        c.send(&[w.depart_ix(&e, (e.p, e.q), &m, tip)], &[&e.wallet]),
        "Depart",
    );
    let sd = hix::settle_departure(&w.a, w.keeper.pubkey(), (e.p, e.q), e.href(), 0);
    assert_code(keeper_send(&mut c, &w, sd.clone()), E::TooEarly);
    skip_bell(&mut c, &w, dest, B0);
    let (_, x) = w
        .entries(&c, dest)
        .into_iter()
        .find(|(_, x)| x.id == id)
        .expect("departed entry kept");
    assert_eq!((x.state, x.op), (EN::STATE_DEPARTED, EntryOp::None));
    expect_lands(keeper_send(&mut c, &w, sd), "SettleDeparture");
    let hd = c.data(&e.holding);
    assert_eq!(hd[H::transit(0) + T::STATE], T::STATE_SETTLED);
    assert_eq!(u32_at(&hd, H::transit(0) + T::TROOPS_AFTER), 1_200_000);
    assert!(w.entries(&c, dest).iter().all(|(_, x)| x.id != id));
}

// ================================================================ G8

/// Contiguous gather ranges covering the 24 positions in random sizes
/// (1–8), in random order, with two repeats.
fn random_parts(rng: &mut Rng) -> Vec<(u8, u8)> {
    let mut parts = vec![];
    let mut k = 0u8;
    while k < 24 {
        let n = (1 + rng.below(8) as u8).min(24 - k);
        parts.push((k, n));
        k += n;
    }
    for i in (1..parts.len()).rev() {
        parts.swap(i, rng.below(i as u64 + 1) as usize);
    }
    for _ in 0..2 {
        let r = parts[rng.below(parts.len() as u64) as usize];
        let at = rng.below(parts.len() as u64 + 1) as usize;
        parts.insert(at, r);
    }
    parts
}

/// The Holdings of a fill's present slots in position order (ResolveClash).
fn fill_holdings(w: &World, f: &Fill) -> Vec<Address> {
    let mut v: Vec<(usize, Address)> = f
        .arrivals
        .iter()
        .map(|(fa, i, _)| {
            let k = CI::position(*fa, *i);
            let (hp, hq, s) =
                permutation_frontier_svm_tests::world::clash::arrival_home(f.p, f.q, k);
            (k, w.a.holding(hp, hq, s))
        })
        .collect();
    v.sort_by_key(|x| x.0);
    v.into_iter().map(|x| x.1).collect()
}

/// Entries after a resolve: at most 56, at most 48 in the roster and 8 a
/// faction once the musters join (I-43).
fn assert_room(c: &Chain, w: &World, f: &Fill) {
    let en = w.entries(c, f.dest());
    assert!(en.len() <= 56, "{}: {} entries", f.name, en.len());
    let roster: Vec<_> = en
        .iter()
        .filter(|(_, x)| matches!(x.state, EN::STATE_ROSTER | EN::STATE_MUSTER_PENDING))
        .collect();
    assert!(roster.len() <= 48, "{}: roster {}", f.name, roster.len());
    for fa in 0..6u8 {
        let n = roster.iter().filter(|(_, x)| x.faction == fa).count();
        assert!(n <= 8, "{}: faction {fa} has {n}", f.name);
    }
}

/// G8 (§13.3): gathers in random order and with repeats, then
/// ResolveFromInputs, equal ResolveClash (oracle build) and the native
/// kernel digest; the I-43 storage fills never exceed 56 entries or 8 a
/// faction after the musters join.
#[test]
fn g08_gathers_in_any_order_equal_the_oracle_and_the_kernel() {
    let mut fills = vec![];
    for (j, (kind, seed)) in [
        (0u8, 31u64),
        (1, 32),
        (2, 33),
        (3, 34),
        (4, 35),
        (5, 36),
        (2, 37),
        (5, 38),
    ]
    .into_iter()
    .enumerate()
    {
        fills.push(Fill::adversarial(kind, seed, -10 + 3 * j as i32, 5));
    }
    for (j, (r, d)) in [(40usize, 1usize), (40, 8), (30, 8), (24, 4), (10, 2)]
        .into_iter()
        .enumerate()
    {
        fills.push(Fill::storage(50 + j as u64, 20 + 3 * j as i32, -9, r, d));
    }
    let (mut c, w) = world_on(Build::TestBeacon);
    let (mut co, wo) = world_on(Build::Oracle);
    let mut rng = Rng::new(0x6008);
    for f in &fills {
        let seed = ready(&mut c, &w, f);
        let native = f.native(&seed, B).digest();
        for part in random_parts(&mut rng) {
            let ix = w.gather_ix(&c, f.dest(), B, part.0, part.1);
            expect_lands(keeper_send(&mut c, &w, ix), "GatherClash");
        }
        let l = expect_lands(
            keeper_send(&mut c, &w, w.resolve_ix(f.dest(), B)),
            "ResolveFromInputs",
        );
        assert_eq!(clash_digest(&l.logs), native, "{}: RFI = native", f.name);
        assert_room(&c, &w, f);
        // the oracle: one transaction, the Province only
        ready(&mut co, &wo, f);
        let ix = cix::oracle(
            &wo.a,
            wo.keeper.pubkey(),
            f.dest(),
            B,
            &fill_holdings(&wo, f),
            &wo.keeper.pubkey(),
        );
        // test-only instruction: its any-account list has no L(kind) of
        // its own, so it asks for the 4-MiB maximum
        let prof = co
            .profile_of(
                std::slice::from_ref(&ix),
                permutation_frontier_svm_tests::chain::Profile::ladder,
            )
            .with_loaded(4 * 1024 * 1024);
        let lo = expect_lands(co.send_with(&prof, &[ix], &[&wo.keeper]), "ResolveClash");
        assert_eq!(
            clash_digest(&lo.logs),
            native,
            "{}: ResolveClash = native",
            f.name
        );
        assert!(
            co.is_absent(&wo.a.clash_inputs(f.p, f.q, B)),
            "the oracle writes the Province only"
        );
        assert_eq!(
            state_bytes(&c, &w, f),
            state_bytes(&co, &wo, f),
            "{}: the same Province",
            f.name
        );
    }
}

// ================================================================ camps

/// I-56: the camp's daily check at the first skip or resolve of a day, in
/// a province with a holding; the camp fights as a NEUTRAL garrison; an
/// arrival that takes it is marked in the inputs' `camp_mask` and the
/// camp is cleared (`CAMP` records for the spawn and the clear).
#[test]
fn clash_camp_respawns_daily_and_is_taken_by_arrivals() {
    let (mut c, w) = world_on(Build::TestBeacon);
    // a province with one holding, no roster: skip into day 1 until a camp stands
    let mut f = roster_fill(-6, 8, 77, vec![]);
    f.garrisons
        .push(permutation_rules::frontier::clash::Garrison {
            id: permutation_frontier_svm_tests::world::clash::garrison_id(-6, 8, 0),
            faction: 2,
            tile: f.terrain.sites[0],
            troops: 1_000_000,
            walls: false,
            posture: Posture::Stance(Stance::Hold),
        });
    f.terrain.site_count = 1;
    let mut b = 140u32;
    w.to_bell(&mut c, b, 1);
    w.craft_fill(&mut c, &f, b);
    let mut spawned_at = None;
    for _ in 0..8 {
        // the day's check is due at the first bell of each day
        let day_start = (b / 144 + 1) * 144;
        ready_run(&mut c, &w, &f, b, day_start - b + 1);
        let l = expect_lands(
            keeper_send(
                &mut c,
                &w,
                w.skip_ix(f.dest(), b, (day_start - b + 1) as u8),
            ),
            "SkipQuiet",
        );
        let camp = w.camp(&c, f.dest());
        assert_eq!(camp.3, day_start / 144 + 1, "checked once for the day");
        if camp.1 == CP::STATE_PRESENT {
            let r = records::one(&l.logs, Kind::CAMP);
            assert_eq!(r.u64("tile") as u8, camp.0);
            assert_eq!(r.u64("troops") as u32, camp.2);
            assert!((100..=400).contains(&camp.2));
            spawned_at = Some(day_start);
            b = day_start + 1;
            break;
        }
        b = day_start + 1;
    }
    let spawned = spawned_at.expect("a camp within 8 days (chance 1/2 a day)");
    let (tile, _, troops, _, gen) = w.camp(&c, f.dest());
    assert_eq!(gen, 1);
    // an arrival of faction 4 on the camp's tile takes it
    let a = Fighter {
        id: permutation_frontier_svm_tests::world::clash::arrival_id(f.p, f.q, 16),
        faction: 4,
        unit: UnitType::Knight,
        troops: 20_000_000,
        stamina: 100,
        tile,
        posture: Posture::Stance(Stance::Assault),
        retreat_bps: None,
        dealt_bps: permutation_frontier_svm_tests::world::clash::dealt(
            4,
            Posture::Stance(Stance::Assault),
            true,
        ),
    };
    let home = permutation_frontier_svm_tests::world::clash::arrival_home(f.p, f.q, 16);
    w.craft_transit_holding(&mut c, home, 4, &a, b, T::STATE_SETTLED);
    w.craft_full_slot(&mut c, f.dest(), b, 4, 0, &a, 0x4444);
    w.craft_day(&mut c, f.dest(), b / 144, &[b]);
    let seed = w.ready_bell(&mut c, b, f.region(), None);
    let g = w.gather_ix(&c, f.dest(), b, 16, 1);
    expect_lands(keeper_send(&mut c, &w, g), "GatherClash");
    let rest = w.gather_parts(&c, f.dest(), b, &[(0, 16), (17, 7)]);
    for ix in rest {
        expect_lands(keeper_send(&mut c, &w, ix), "GatherClash");
    }
    let l = expect_lands(
        keeper_send(&mut c, &w, w.resolve_ix(f.dest(), b)),
        "ResolveFromInputs",
    );
    // the native clash with the camp as a NEUTRAL garrison
    let mut nf = f.clone();
    nf.garrisons
        .push(permutation_rules::frontier::clash::Garrison {
            id: u64::MAX - gen as u64,
            faction: 6,
            tile,
            troops: troops * 1_000,
            walls: false,
            posture: Posture::Stance(Stance::Hold),
        });
    nf.arrivals = vec![(4, 0, a)];
    let native = nf.native(&seed, b);
    assert_eq!(clash_digest(&l.logs), native.digest());
    let cg = native
        .garrisons
        .iter()
        .find(|g| g.id == u64::MAX - gen as u64)
        .unwrap();
    assert!(
        cg.troops < 1_000 || cg.attackers_hold,
        "the knight takes the camp: {cg:?}"
    );
    assert_eq!(w.camp(&c, f.dest()).1, CP::STATE_NONE, "cleared");
    let cd = c.data(&w.a.clash_inputs(f.p, f.q, b));
    assert_eq!(u32_at(&cd, CI::RSV_76), 1 << 16, "camp_mask: position 16");
    let camps = records::of_kind(&l.logs, Kind::CAMP);
    assert_eq!(camps.last().unwrap().u64("troops"), 0, "the clear");
    let _ = spawned;
}

// ================================================================ closes

/// A resolved fill at `B` with arrivals (crafted accounts, module note).
fn resolved_fill(c: &mut Chain, w: &World, seed: u64, p: i32, q: i32) -> Fill {
    let f = Fill::adversarial(2, seed, p, q);
    ready(c, w, &f);
    gather_and_resolve(c, w, &f);
    f
}

/// CloseClashInputs (§5.11): resolved, every recorded host settled (the
/// `settled_mask` SettleTransit leaves, W4-B: crafted), the close grace
/// passed; `InputsOpen` before each; `rent_to` checked; CLOSE carries the
/// final chain; the address is tombstoned for gathers (G2 re-creation).
#[test]
fn clash_close_clash_inputs() {
    let (mut c, w) = world_on(Build::TestBeacon);
    let f = resolved_fill(&mut c, &w, 41, 3, 3);
    let ck = w.a.clash_inputs(f.p, f.q, B);
    let close = |w: &World, rent_to: Address| {
        cix::close_clash_inputs(&w.a, w.keeper.pubkey(), f.p as i16, f.q as i16, B, rent_to)
    };
    let rent_to = w.keeper.pubkey();
    assert_code(keeper_send(&mut c, &w, close(&w, rent_to)), E::InputsOpen);
    // every recorded host settled (as SettleTransit leaves the mask)
    let recorded: u32 = (0..24)
        .filter(|&k| u64_at(&c.data(&ck), CI::arrival(k) + AR::HOST_ID) != 0)
        .fold(0, |m, k| m | 1 << k);
    c.edit(&ck, |d| {
        d[CI::SETTLED_MASK..CI::SETTLED_MASK + 4].copy_from_slice(&recorded.to_le_bytes())
    });
    assert_code(keeper_send(&mut c, &w, close(&w, rent_to)), E::InputsOpen);
    let grace = u32_at(&c.data(&w.a.season), S::CLASH_CLOSE_GRACE) as i64;
    c.advance(grace * 600);
    assert_code(
        keeper_send(&mut c, &w, close(&w, Address::new_unique())),
        E::BadAccount,
    );
    let watch = ChainWatch::new(&c, ck, EntityKind::ClashInputs);
    let before = c.lamports(&rent_to);
    let l = expect_lands(
        keeper_send(&mut c, &w, close(&w, rent_to)),
        "cix::close_clash_inputs(",
    );
    let r = records::one(&l.logs, Kind::CLOSE);
    records::check_tail(&r);
    let (seq, _) = records::head_of(&c, &ck);
    let _ = (seq, watch);
    assert!(c.is_absent(&ck));
    assert!(c.lamports(&rent_to) > before);
    // the resolved bell stays shut to gathers
    let g = w.gather_ix(&c, f.dest(), B, 0, 8);
    assert_code(keeper_send(&mut c, &w, g), E::LatchClosed);
    // inputs gathered but never resolved: InputsOpen
    let f2 = Fill::adversarial(0, 42, 7, 3);
    ready(&mut c, &w, &f2);
    let (b2, rn) = (B, w.resolved_next(&c, f2.dest()));
    assert_eq!(rn, b2);
    let g = w.gather_ix(&c, f2.dest(), B, 0, 8);
    expect_lands(keeper_send(&mut c, &w, g), "GatherClash");
    let ix = cix::close_clash_inputs(
        &w.a,
        w.keeper.pubkey(),
        f2.p as i16,
        f2.q as i16,
        B,
        rent_to,
    );
    assert_code(keeper_send(&mut c, &w, ix), E::InputsOpen);
}

/// CloseArrivalDay: only once the province resolved every bell of the day
/// (`TooEarly`); `rent_to` checked; CLOSE logged.
#[test]
fn clash_close_arrival_day() {
    let (mut c, w) = world_on(Build::TestBeacon);
    let f = resolved_fill(&mut c, &w, 43, 5, 5);
    let day = day_of(B);
    let dk = w.a.arrival_day(f.p, f.q, day);
    let ix = |w: &World, r: Address| {
        cix::close_arrival_day(&w.a, w.keeper.pubkey(), f.p as i16, f.q as i16, day, r)
    };
    assert_code(
        keeper_send(&mut c, &w, ix(&w, w.keeper.pubkey())),
        E::TooEarly,
    );
    w.set_resolved_next(&mut c, &w.a.province(f.p, f.q), 144 * (day + 1));
    assert_code(
        keeper_send(&mut c, &w, ix(&w, Address::new_unique())),
        E::BadAccount,
    );
    let l = expect_lands(
        keeper_send(&mut c, &w, ix(&w, w.keeper.pubkey())),
        "cix::close_arrival_day(",
    );
    records::check_tail(&records::one(&l.logs, Kind::CLOSE));
    assert!(c.is_absent(&dk));
}

/// CloseArrivalSlot: (a) settled by SettleTransit (flag crafted, W4-B) and
/// claimed, or its claim grace past THE anchor's close (an archived anchor,
/// absent, counts as past); (b) the season Ended 72 h ago (crafted status,
/// EndSeason is W4-B's). `TooEarly` otherwise.
#[test]
fn clash_close_arrival_slot() {
    let (mut c, w) = world_on(Build::TestBeacon);
    let f = resolved_fill(&mut c, &w, 44, 9, 5);
    let (fa, i) = (f.arrivals[0].0, f.arrivals[0].1);
    let sk = w.a.arrival_slot(f.p, f.q, B, fa, i);
    let rent_to = w.keeper.pubkey();
    let ix = |w: &World, anchor: bool| {
        cix::close_arrival_slot(
            &w.a,
            w.keeper.pubkey(),
            f.p as i16,
            f.q as i16,
            B,
            fa,
            i,
            rent_to,
            anchor,
        )
    };
    assert_code(keeper_send(&mut c, &w, ix(&w, true)), E::TooEarly);
    c.edit(&sk, |d| d[AS::FLAGS] |= AS::FLAG_SETTLED);
    // within the grace, unclaimed: too early; claimed: closes
    let mut g = c.fork();
    assert_code(keeper_send(&mut g, &w, ix(&w, true)), E::TooEarly);
    g.edit(&sk, |d| d[AS::CLAIMED] = 1);
    expect_lands(
        keeper_send(&mut g, &w, ix(&w, false)),
        "cix::close_arrival_slot(",
    );
    assert!(g.is_absent(&sk));
    // past the grace
    let mut g = c.fork();
    let close = w.close_of(&g, B, f.region());
    g.set_time(close + 6 * 600);
    let l = expect_lands(
        keeper_send(&mut g, &w, ix(&w, true)),
        "cix::close_arrival_slot(",
    );
    records::check_tail(&records::one(&l.logs, Kind::CLOSE));
    // an archived anchor (absent): the grace passed long ago
    let mut g = c.fork();
    g.remove(&w.a.anchor(B, f.region()));
    expect_lands(
        keeper_send(&mut g, &w, ix(&w, true)),
        "cix::close_arrival_slot(",
    );
    // (b) the season ended 72 h ago, even unsettled
    let mut g = c.fork();
    g.edit(&sk, |d| d[AS::FLAGS] = 0);
    g.edit(&w.a.season, |d| d[S::STATUS] = S::STATUS_ENDED);
    let end = w.bell_start(u32_at(&g.data(&w.a.season), S::END_BELL));
    g.set_time(end + 72 * 3_600);
    assert_code(
        keeper_send(
            &mut g,
            &w,
            cix::close_arrival_slot(
                &w.a,
                w.keeper.pubkey(),
                f.p as i16,
                f.q as i16,
                B,
                fa,
                i,
                Address::new_unique(),
                false,
            ),
        ),
        E::BadAccount,
    );
    expect_lands(
        keeper_send(&mut g, &w, ix(&w, false)),
        "cix::close_arrival_slot(",
    );
}

// ================================================================ refusals (G13)

#[test]
fn clash_gather_refusals() {
    let (mut c, w) = world_on(Build::TestBeacon);
    let f = Fill::adversarial(0, 51, -3, -7);
    w.craft_fill(&mut c, &f, B);
    // the window is open: TooEarly; THE anchor missing: NoAnchor
    let g = |c: &Chain| w.gather_ix(c, f.dest(), B, 0, 8);
    assert_code(
        {
            let ix = g(&c);
            keeper_send(&mut c, &w, ix)
        },
        E::NoAnchor,
    );
    let a = w.bell_end(B) + 1;
    {
        let sl = c.slot;
        w.craft_anchor(&mut c, B, f.region(), a, sl)
    };
    assert_code(
        {
            let ix = g(&c);
            keeper_send(&mut c, &w, ix)
        },
        E::TooEarly,
    );
    w.ready_bell(&mut c, B, f.region(), None);
    // a bell below resolved_next: LatchClosed
    let mut x = c.fork();
    w.set_resolved_next(&mut x, &w.a.province(f.p, f.q), B + 1);
    assert_code(
        {
            let ix = g(&x);
            keeper_send(&mut x, &w, ix)
        },
        E::LatchClosed,
    );
    // past the season: WrongStatus
    let mut x = c.fork();
    let end = u32_at(&x.data(&w.a.season), S::END_BELL);
    x.edit(&w.a.season, |d| {
        d[S::END_BELL..S::END_BELL + 4].copy_from_slice(&B.to_le_bytes())
    });
    assert_code(
        {
            let ix = g(&x);
            keeper_send(&mut x, &w, ix)
        },
        E::WrongStatus,
    );
    let _ = end;
    // the range: BadData
    let mut ix = g(&c);
    ix.data[6] = 0; // n = 0
    assert_code(keeper_send(&mut c.fork(), &w, ix), E::BadData);
    let mut ix = w.gather_ix(&c, f.dest(), B, 20, 4);
    ix.data[5] = 21; // start 21 + 4 > 24
    assert_code(keeper_send(&mut c.fork(), &w, ix), E::BadData);
    // an account list that does not match n and the bitmap
    let mut ix = g(&c);
    ix.accounts.pop();
    assert_code(keeper_send(&mut c.fork(), &w, ix), E::TooManyAccounts);
    // non-canonical province, inputs, day, slot, holding: BadAddress
    for at in [
        gather_at::PROVINCE,
        gather_at::INPUTS,
        gather_at::DAY,
        gather_at::SLOT0,
        gather_at::SLOT0 + 8,
    ] {
        let k = g(&c).accounts[at].pubkey;
        let fresh = common::copy_to_fresh(&mut c, &k, &[at as u8]);
        let ix = with_account(g(&c), at, fresh);
        assert_code(keeper_send(&mut c.fork(), &w, ix), E::BadAddress);
    }
    // a slot at its address with another bell's fields: BadAccount
    let mut x = c.fork();
    let sk = w.a.arrival_slot(f.p, f.q, B, 0, 0);
    x.edit(&sk, |d| {
        d[AS::BELL..AS::BELL + 4].copy_from_slice(&(B + 1).to_le_bytes())
    });
    assert_code(
        {
            let ix = g(&x);
            keeper_send(&mut x, &w, ix)
        },
        E::BadAccount,
    );
    // a transit still departed (SettleDeparture not landed): DepartureUnsettled
    let mut x = c.fork();
    let (hp, hq, s) = permutation_frontier_svm_tests::world::clash::arrival_home(f.p, f.q, 0);
    x.edit(&w.a.holding(hp, hq, s), |d| {
        d[H::transit(0) + T::STATE] = T::STATE_DEPARTED
    });
    assert_code(
        {
            let ix = g(&x);
            keeper_send(&mut x, &w, ix)
        },
        E::DepartureUnsettled,
    );
    // the fee payer must sign: Auth; a writable anchor: BadAccount
    let other = Keypair::new();
    let ix = without_signer(with_account(g(&c), 0, other.pubkey()), 0);
    assert_code(keeper_send(&mut c.fork(), &w, ix), E::Auth);
    let ix = with_writable(g(&c), gather_at::ANCHOR, true);
    assert_code(keeper_send(&mut c.fork(), &w, ix), E::BadAccount);
    // a season that is not Running or Ended: WrongStatus
    let mut x = c.fork();
    x.edit(&w.a.season, |d| d[S::STATUS] = S::STATUS_ABORTED);
    assert_code(
        {
            let ix = g(&x);
            keeper_send(&mut x, &w, ix)
        },
        E::WrongStatus,
    );
    // and it lands
    expect_lands(
        {
            let ix = g(&c);
            keeper_send(&mut c, &w, ix)
        },
        "w.gather_ix(",
    );
}

#[test]
fn clash_resolve_refusals() {
    let (mut c, w) = world_on(Build::TestBeacon);
    let f = Fill::adversarial(3, 52, -5, -7);
    w.craft_fill(&mut c, &f, B);
    let a = w.bell_end(B) + 1;
    {
        let sl = c.slot;
        w.craft_anchor(&mut c, B, f.region(), a, sl)
    };
    let close = w.close_of(&c, B, f.region());
    c.set_time(close);
    let r = || w.resolve_ix(f.dest(), B);
    // nothing gathered: NotGathered; partly: NotGathered
    assert_code(keeper_send(&mut c, &w, r()), E::NotGathered);
    let parts = w.gather_parts(&c, f.dest(), B, &THIRDS);
    expect_lands(keeper_send(&mut c, &w, parts[0].clone()), "GatherClash");
    assert_code(keeper_send(&mut c, &w, r()), E::NotGathered);
    for ix in &parts[1..] {
        expect_lands(keeper_send(&mut c, &w, ix.clone()), "GatherClash");
    }
    // no SeedCache of THE anchor yet: SeedNotReady
    assert_code(keeper_send(&mut c, &w, r()), E::SeedNotReady);
    w.ready_bell(&mut c, B, f.region(), None);
    // out of order
    let mut x = c.fork();
    w.set_resolved_next(&mut x, &w.a.province(f.p, f.q), B - 1);
    assert_code(keeper_send(&mut x, &w, r()), E::OutOfOrder);
    // non-canonical province and inputs
    for at in [resolve_at::PROVINCE, resolve_at::INPUTS] {
        let k = r().accounts[at].pubkey;
        let fresh = common::copy_to_fresh(&mut c, &k, &[0x40 + at as u8]);
        assert_code(
            keeper_send(&mut c.fork(), &w, with_account(r(), at, fresh)),
            E::BadAddress,
        );
    }
    // a forged SeedCache (another nonce's address holding this one's data): BadAddress
    let cache = w.a.seed_cache(B, f.region(), 0);
    let fresh = common::copy_to_fresh(&mut c, &cache, b"cache");
    assert_code(
        keeper_send(
            &mut c.fork(),
            &w,
            with_account(r(), resolve_at::SEED, fresh),
        ),
        E::BadAddress,
    );
    // and it lands; the second is out of order
    expect_lands(keeper_send(&mut c, &w, r()), "w.resolve_ix(");
    assert_code(keeper_send(&mut c, &w, r()), E::OutOfOrder);
}

#[test]
fn clash_skip_refusals() {
    let b0 = 30u32;
    let (mut c, w) = world_on(Build::TestBeacon);
    w.to_bell(&mut c, b0, 1);
    let f = quiet_world(-9, -2, 61, 4, b0, false);
    craft_quiet(&mut c, &w, &f, b0, false);
    let sk = |n: u8| w.skip_ix(f.dest(), b0, n);
    // THE anchor missing: NoAnchor; open: TooEarly
    assert_code(keeper_send(&mut c, &w, sk(1)), E::NoAnchor);
    let a = w.bell_end(b0) + 1;
    {
        let sl = c.slot;
        w.craft_anchor(&mut c, b0, f.region(), a, sl)
    };
    assert_code(keeper_send(&mut c, &w, sk(1)), E::TooEarly);
    ready_run(&mut c, &w, &f, b0, 3);
    // out of order
    assert_code(
        keeper_send(&mut c, &w, w.skip_ix(f.dest(), b0 + 1, 1)),
        E::OutOfOrder,
    );
    // a day account that is not canonical: BadAddress
    for at in [
        skip_at::DAY0,
        skip_at::DAY1,
        skip_at::PROVINCE,
        skip_at::ANCHOR0,
    ] {
        let k = sk(1).accounts[at].pubkey;
        let fresh = if at == skip_at::PROVINCE {
            common::copy_to_fresh(&mut c, &k, b"skip-province")
        } else {
            Address::new_unique()
        };
        assert_code(
            keeper_send(&mut c.fork(), &w, with_account(sk(1), at, fresh)),
            E::BadAddress,
        );
    }
    // n and the anchors disagree: BadData
    let mut ix = sk(2);
    ix.data[5] = 3;
    assert_code(keeper_send(&mut c.fork(), &w, ix), E::BadData);
    // an arrival at b0 (the day's bit): NotQuiet
    let mut x = c.fork();
    w.craft_day(&mut x, f.dest(), b0 / 144, &[b0]);
    assert_code(keeper_send(&mut x, &w, sk(1)), E::NotQuiet);
    // hostile residents on one hex: NotQuiet (the kernel's test)
    let mut x = c.fork();
    x.edit(&w.a.province(f.p, f.q), |d| {
        let t = read_entry(d, 0).unwrap().tile;
        let mut e = read_entry(d, 5).unwrap();
        assert_ne!(e.faction, read_entry(d, 0).unwrap().faction);
        e.tile = t;
        write_entry(d, 5, &e).unwrap();
    });
    assert_code(keeper_send(&mut x, &w, sk(1)), E::NotQuiet);
    // past the season: WrongStatus
    let mut x = c.fork();
    x.edit(&w.a.season, |d| {
        d[S::END_BELL..S::END_BELL + 4].copy_from_slice(&b0.to_le_bytes())
    });
    assert_code(keeper_send(&mut x, &w, sk(1)), E::WrongStatus);
    // a later bell whose window is open ends the run (prefix)
    let l = expect_lands(keeper_send(&mut c, &w, sk(4)), "w.skip_ix(");
    assert_eq!(records::one(&l.logs, Kind::SKIP).u64("n"), 3);
    assert_eq!(w.resolved_next(&c, f.dest()), b0 + 3);
}

// ================================================================ G2 / G3

/// G2 (§13.2): ClashInputs created by GatherClash on a pre-funded address
/// (1 lamport, rent, 10× rent): the payer pays only the shortfall; a
/// pre-funded, never-created ArrivalSlot and ArrivalDay count as absent.
#[test]
fn g02_prefund_clash_inputs_gather() {
    let rent = Chain::new(Build::TestBeacon).rent(CI::SIZE);
    for (j, pre) in common::prefunds(rent).into_iter().enumerate() {
        let (mut c, w) = world_on(Build::TestBeacon);
        let f = roster_fill(4 + j as i32, -4, 70 + j as u64, vec![]);
        w.craft_fill(&mut c, &f, B);
        w.ready_bell(&mut c, B, f.region(), None);
        let ck = w.a.clash_inputs(f.p, f.q, B);
        c.prefund(&ck, pre);
        // never-created slot and day, pre-funded: absent
        c.prefund(&w.a.arrival_slot(f.p, f.q, B, 0, 0), rent);
        c.prefund(&w.a.arrival_day(f.p, f.q, day_of(B)), rent);
        let before = c.lamports(&w.keeper.pubkey());
        let g = w.gather_ix(&c, f.dest(), B, 0, 4);
        let l = expect_lands(keeper_send(&mut c, &w, g), "GatherClash");
        let paid = common::paid(before, &c, &w.keeper, &l);
        assert_eq!(paid, rent.saturating_sub(pre), "pre-funded {pre}");
        common::assert_program_account(&c, &ck, CI::MAGIC, CI::SIZE, w.id);
        assert_eq!(c.data(&ck)[CI::FLAGS], CI::FLAG_NO_ARRIVALS);
        expect_lands(
            keeper_send(&mut c, &w, w.resolve_ix(f.dest(), B)),
            "ResolveFromInputs",
        );
    }
}

/// G2 re-creation (I-46): once a bell is resolved or skipped, GatherClash
/// refuses it (`LatchClosed`), so a closed or never-needed ClashInputs
/// cannot be created again.
#[test]
fn g02_recreation_clash_inputs_latch() {
    let (mut c, w) = world_on(Build::TestBeacon);
    let f = resolved_fill(&mut c, &w, 45, 11, -2);
    let g = w.gather_ix(&c, f.dest(), B, 0, 8);
    assert_code(keeper_send(&mut c, &w, g), E::LatchClosed);
    // a skipped bell
    let q = roster_fill(12, -6, 46, vec![]);
    w.craft_fill(&mut c, &q, B);
    w.ready_bell(&mut c, B, q.region(), None);
    expect_lands(
        keeper_send(&mut c, &w, w.skip_ix(q.dest(), B, 1)),
        "SkipQuiet",
    );
    let g = w.gather_ix(&c, q.dest(), B, 0, 1);
    assert_code(keeper_send(&mut c, &w, g), E::LatchClosed);
    assert!(c.is_absent(&w.a.clash_inputs(q.p, q.q, B)));
}

/// G3 (§13.2): forged accounts in the clash instructions: a copy at
/// another address (`BadAddress`), wrong owner, magic, season or key
/// fields at the canonical address (`BadAccount`).
#[test]
fn g03_forgery_clash_accounts() {
    let (mut c, w) = world_on(Build::TestBeacon);
    let f = Fill::adversarial(4, 53, 2, -9);
    ready(&mut c, &w, &f);
    let parts = w.gather_parts(&c, f.dest(), B, &THIRDS);
    // the inputs: another province-bell's at this address, then wrong
    // season, magic and owner
    expect_lands(keeper_send(&mut c, &w, parts[0].clone()), "GatherClash");
    let ck = w.a.clash_inputs(f.p, f.q, B);
    for forge in 0..4 {
        let mut x = c.fork();
        match forge {
            0 => x.edit(&ck, |d| {
                d[CI::BELL..CI::BELL + 4].copy_from_slice(&(B + 1).to_le_bytes())
            }),
            1 => x.edit(&ck, |d| d[8] ^= 1),
            2 => x.edit(&ck, |d| d[0] ^= 1),
            _ => x.set_owner(&ck, Address::new_unique()),
        }
        assert_code(keeper_send(&mut x, &w, parts[1].clone()), E::BadAccount);
    }
    for ix in &parts[1..] {
        expect_lands(keeper_send(&mut c, &w, ix.clone()), "GatherClash");
    }
    // the Province: wrong season, magic, owner
    let pk = w.a.province(f.p, f.q);
    for forge in 0..3 {
        let mut x = c.fork();
        match forge {
            0 => x.edit(&pk, |d| d[8] ^= 1),
            1 => x.edit(&pk, |d| d[0] ^= 1),
            _ => x.set_owner(&pk, Address::new_unique()),
        }
        assert_code(
            keeper_send(&mut x, &w, w.resolve_ix(f.dest(), B)),
            E::BadAccount,
        );
    }
    // THE anchor of another bell at this bell's address: BadAccount
    let mut x = c.fork();
    x.edit(&w.a.anchor(B, f.region()), |d| {
        let o = frontier_abi::layout::beacon::bell_anchor::BELL;
        d[o..o + 4].copy_from_slice(&(B + 1).to_le_bytes())
    });
    assert_code(
        keeper_send(&mut x, &w, w.resolve_ix(f.dest(), B)),
        E::BadAccount,
    );
    expect_lands(
        keeper_send(&mut c, &w, w.resolve_ix(f.dest(), B)),
        "ResolveFromInputs",
    );
}

// ================================================================ G9, the clash side

/// Directions from tile `origin.2` to `tile` inside one province (a greedy
/// hex walk, as W3-B's G9 test takes it).
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
        dirs = vec![0, 3];
    }
    dirs
}

/// G9 (§13.3), the clash side, through the program's own instructions:
/// marches depart, reveal in different orders (the keeper retrying a
/// `SlotMoved`), the origin skips the departure bells, SettleDeparture
/// lands, and the destination's gathers and resolve give the same clash
/// whatever the order. (SettleTransit's side is W4-B's.)
#[test]
fn g09_clash_outcome_is_order_free() {
    use permutation_rules::frontier::clash::{admit_arrival, SlotDecision, SlotEntry};
    let (mut c, w) = common::test_beacon();
    w.to_bell(&mut c, B0, 5);
    let dest = (3i32, -1i32);
    w.craft_province(&mut c, dest.0 as i16, dest.1 as i16);
    let n_sites = c.data(&w.a.province(dest.0, dest.1))[P::SITE_COUNT].min(6);
    let mut rng = Rng::new(0x6909);
    let target = 30u8;
    let arrive = B0 + 8;
    let mut marches = vec![];
    let mut estates = vec![];
    for k in 0..n_sites {
        let e = w.craft_estate(
            &mut c,
            &format!("g9-{k}"),
            0,
            (dest.0 as i16, dest.1 as i16),
            k,
        );
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
            let dirs = walk_to(origin, target);
            permutation_frontier_svm_tests::world::holding::open_path(&w, &mut c, origin, &dirs);
            let m = w.plan_march(
                id,
                h,
                origin,
                &dirs,
                arrive,
                0,
                0,
                permutation_frontier_svm_tests::fixtures::tlock::SealCase::Valid,
            );
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
    expect_lands(w.post_anchor(&mut c, arrive, region), "PostAnchor");
    let pc = ProvinceCoord::new(dest.0, dest.1);
    let all: Vec<SlotEntry> = marches
        .iter()
        .map(|(tag, mass, m, _)| SlotEntry {
            host_id: m.host_id,
            citizen: *tag,
            troops: *mass,
        })
        .collect();
    let mut digests = vec![];
    for order in 0..3u64 {
        let mut f = c.fork();
        let mut idx: Vec<usize> = (0..marches.len()).collect();
        let mut r = Rng::new(order + 7);
        for i in (1..idx.len()).rev() {
            idx.swap(i, r.below(i as u64 + 1) as usize);
        }
        for &i in &idx {
            let (_, _, m, k) = &marches[i];
            let e = &estates[*k];
            let slots: [Option<SlotEntry>; 4] = core::array::from_fn(|j| {
                let d = f.data(&w.a.arrival_slot(dest.0, dest.1, arrive, 0, j as u8));
                (d.len() == AS::SIZE).then(|| SlotEntry {
                    host_id: u64_at(&d, AS::HOST_ID),
                    citizen: u64_at(&d, AS::CITIZEN_TAG),
                    troops: u32_at(&d, AS::DEP_MASS),
                })
            });
            let day_w = f
                .data(&w.a.arrival_day(dest.0, dest.1, day_of(arrive)))
                .is_empty();
            match admit_arrival(pc, arrive, &slots, all[i]) {
                SlotDecision::Fill { slot } | SlotDecision::Displace { slot, .. } => {
                    let ix = w.reveal_ix(&w.keeper.pubkey(), e, m, slot, day_w);
                    expect_lands(f.send(&[ix], &[&w.keeper]), "Reveal");
                }
                SlotDecision::Refuse(_) => {}
            }
        }
        // the origin skips the departure bells; the departures settle
        let quiet = roster_fill(dest.0, dest.1, 1, vec![]);
        ready_run(&mut f, &w, &quiet, B0, 8);
        let _ = catch_up(&mut f, &w, &quiet, B0, 8);
        for (_, _, m, k) in &marches {
            let e = &estates[*k];
            let ix = hix::settle_departure(
                &w.a,
                w.keeper.pubkey(),
                (e.p, e.q),
                e.href(),
                m.transit_slot,
            );
            expect_lands(keeper_send(&mut f, &w, ix), "SettleDeparture");
        }
        // the destination's clash
        w.ready_bell(&mut f, arrive, region, Some(bell_seed(arrive, region)));
        for ix in w.gather_parts(&f, dest, arrive, &THIRDS) {
            expect_lands(keeper_send(&mut f, &w, ix), "GatherClash");
        }
        let l = expect_lands(
            keeper_send(&mut f, &w, w.resolve_ix(dest, arrive)),
            "ResolveFromInputs",
        );
        let cd = f.data(&w.a.clash_inputs(dest.0, dest.1, arrive));
        assert!(cd[CI::N_PRESENT] >= 1);
        // the fate of each host (slot positions follow the reveal order)
        let mut fates: Vec<(u64, u8, u32)> = (0..24)
            .map(CI::arrival)
            .filter(|&o| cd[o + AR::PRESENT] == 1)
            .map(|o| {
                (
                    u64_at(&cd, o + AR::HOST_ID),
                    cd[o + AR::FATE],
                    u32_at(&cd, o + AR::TROOPS_AFTER),
                )
            })
            .collect();
        fates.sort();
        let pd = state_bytes(&f, &w, &quiet);
        digests.push((clash_digest(&l.logs), fates, pd));
    }
    assert!(
        digests.windows(2).all(|d| d[0] == d[1]),
        "the same clash, fates and Province in every order"
    );
}

// ================================================================ G1: the other kinds

/// Sends `ixs` at `L(ix)` (loads and lands) and one page below the tight
/// limit (fails, charged) — the `g01_loaded_limit` helper of W2-B.
fn check(c: &Chain, ix: Ix, ixs: &[Instruction], signers: &[&Keypair]) {
    use permutation_frontier_svm_tests::chain::{assert_loaded_exceeded, Profile, PAGE};
    let pd = c.programdata_len();
    let l = frontier_abi::budgets::loaded_limit_for(ix, pd);
    let p = Profile::ladder(ix, pd).with_loaded(l);
    let t = c.transaction(&p, ixs, signers);
    let need = c.loaded_size(&t.message);
    let tight = (need.div_ceil(PAGE as u64) * PAGE as u64) as u32;
    println!(
        "g01 L({}) = {l} B at programdata {pd} B: need {need} B, tight {tight} B",
        ix.name()
    );
    assert!(
        need <= l as u64,
        "{}: need {need} B > L(kind) {l} B",
        ix.name()
    );
    let mut f = c.fork();
    expect_lands(f.send_with(&p, ixs, signers), ix.name());
    let mut f = c.fork();
    assert_loaded_exceeded(f.send_with(&p.with_loaded(tight - PAGE), ixs, signers));
}

/// G1 (§13.1): GatherClash with 12 positions, 10 Holdings, the others
/// absent or pre-funded (≤ 40k, its byte ceiling); the closes (≤ 8k) and
/// the return settle (SettleDeparture's 15k); `g01_loaded_limit_*` of the
/// clash kinds on the release build.
#[test]
fn g01_budget_clash_kinds() {
    let (mut c, w) = world_on(Build::Release);
    let pd = c.programdata_len();
    let f = Fill::adversarial(5, 81, -14, 6);
    ready(&mut c, &w, &f);
    // positions 10, 11 absent (one pre-funded)
    for k in [10u8, 11] {
        let sk = w.a.arrival_slot(f.p, f.q, B, k / 4, k % 4);
        c.remove(&sk);
        if k == 11 {
            c.prefund(&sk, 1_000_000);
        }
    }
    let g = w.gather_ix(&c, f.dest(), B, 0, 12);
    let need = c.measure(std::slice::from_ref(&g), &[&w.keeper]).unwrap();
    assert_within(
        "GatherClash 12 positions, 10 Holdings",
        &need,
        &ceilings(Ix::GatherClash, 0, pd),
    );
    check(&c, Ix::GatherClash, std::slice::from_ref(&g), &[&w.keeper]);
    expect_lands(keeper_send(&mut c, &w, g), "GatherClash");
    for ix in w.gather_parts(&c, f.dest(), B, &[(12, 8), (20, 4)]) {
        expect_lands(keeper_send(&mut c, &w, ix), "GatherClash");
    }
    let r = w.resolve_ix(f.dest(), B);
    check(
        &c,
        Ix::ResolveFromInputs,
        std::slice::from_ref(&r),
        &[&w.keeper],
    );
    expect_lands(keeper_send(&mut c, &w, r), "ResolveFromInputs");
    // closes: every recorded host settled (crafted: SettleTransit is W4-B's),
    // the grace passed, the day resolved through
    let ck = w.a.clash_inputs(f.p, f.q, B);
    c.edit(&ck, |d| {
        d[CI::SETTLED_MASK..CI::SETTLED_MASK + 4].copy_from_slice(&0x00FF_FFFFu32.to_le_bytes())
    });
    let grace = u32_at(&c.data(&w.a.season), S::CLASH_CLOSE_GRACE) as i64;
    c.advance(grace * 600);
    let rent_to = w.keeper.pubkey();
    let cl = cix::close_clash_inputs(&w.a, w.keeper.pubkey(), f.p as i16, f.q as i16, B, rent_to);
    assert_within(
        "CloseClashInputs",
        &c.measure(std::slice::from_ref(&cl), &[&w.keeper]).unwrap(),
        &ceilings(Ix::CloseClashInputs, 0, pd),
    );
    check(&c, Ix::CloseClashInputs, &[cl], &[&w.keeper]);
    let day = day_of(B);
    w.set_resolved_next(&mut c, &w.a.province(f.p, f.q), 144 * (day + 1));
    let cd = cix::close_arrival_day(
        &w.a,
        w.keeper.pubkey(),
        f.p as i16,
        f.q as i16,
        day,
        rent_to,
    );
    assert_within(
        "CloseArrivalDay",
        &c.measure(std::slice::from_ref(&cd), &[&w.keeper]).unwrap(),
        &ceilings(Ix::CloseArrivalDay, 0, pd),
    );
    check(&c, Ix::CloseArrivalDay, &[cd], &[&w.keeper]);
    let (fa, i) = (f.arrivals[0].0, f.arrivals[0].1);
    c.edit(&w.a.arrival_slot(f.p, f.q, B, fa, i), |d| {
        d[AS::FLAGS] |= AS::FLAG_SETTLED
    });
    let cs = cix::close_arrival_slot(
        &w.a,
        w.keeper.pubkey(),
        f.p as i16,
        f.q as i16,
        B,
        fa,
        i,
        rent_to,
        true,
    );
    assert_within(
        "CloseArrivalSlot",
        &c.measure(std::slice::from_ref(&cs), &[&w.keeper]).unwrap(),
        &ceilings(Ix::CloseArrivalSlot, 0, pd),
    );
    check(&c, Ix::CloseArrivalSlot, &[cs], &[&w.keeper]);
    // SkipQuiet's worst account set: 24 anchors, both days
    let b0 = 130u32;
    let q = quiet_world(16, -12, 82, 8, b0, false);
    w.to_bell(&mut c, b0, 1);
    craft_quiet(&mut c, &w, &q, b0, false);
    ready_run(&mut c, &w, &q, b0, 24);
    w.craft_day(&mut c, q.dest(), day_of(b0), &[]);
    w.craft_day(&mut c, q.dest(), day_of(b0) + 1, &[]);
    check(
        &c,
        Ix::SkipQuiet,
        &[w.skip_ix(q.dest(), b0, 24)],
        &[&w.keeper],
    );
    // the return settle: a Leave entry of an estate
    let (mut c, w, e, id) = estate();
    expect_lands(
        c.send(
            &[hix::dissolve(&w.a, &e.player(), e.href(), id)],
            &[&e.wallet],
        ),
        "Dissolve",
    );
    skip_bell(&mut c, &w, dest_of(&e), B0);
    let rs = cix::settle_return(&w.a, w.keeper.pubkey(), (e.p, e.q), e.href());
    assert_within(
        "SettleDeparture (return)",
        &c.measure(std::slice::from_ref(&rs), &[&w.keeper]).unwrap(),
        &ceilings(Ix::SettleDeparture, 0, c.programdata_len()),
    );
}

#[test]
#[ignore]
fn zz_profile_rfi() {
    let (mut c, w) = world_on(Build::Trace);
    let name = std::env::var("FILL").unwrap_or("wide#4".into());
    let f = all_fills()
        .into_iter()
        .find(|(_, f)| f.name == name)
        .unwrap()
        .1;
    ready(&mut c, &w, &f);
    for ix in w.gather_parts(&c, f.dest(), B, &THIRDS) {
        expect_lands(keeper_send(&mut c, &w, ix), "GatherClash");
    }
    let l = expect_lands(keeper_send(&mut c, &w, w.resolve_ix(f.dest(), B)), "RFI");
    for x in &l.logs {
        println!("{x}");
    }
}

#[test]
#[ignore]
fn zz_profile_skip() {
    let b0 = 130u32;
    let (mut c, w) = world_on(Build::Trace);
    let f = quiet_world(12, 5, 21, 8, b0, false);
    w.to_bell(&mut c, b0, 1);
    craft_quiet(&mut c, &w, &f, b0, false);
    ready_run(&mut c, &w, &f, b0, 24);
    let l = expect_lands(keeper_send(&mut c, &w, w.skip_ix(f.dest(), b0, 24)), "skip");
    for x in &l.logs {
        println!("{x}");
    }
}

#[test]
#[ignore]
fn zz_profile_gather() {
    let (mut c, w) = world_on(Build::Trace);
    let f = Fill::adversarial(5, 81, -14, 6);
    ready(&mut c, &w, &f);
    for k in [10u8, 11] {
        c.remove(&w.a.arrival_slot(f.p, f.q, B, k / 4, k % 4));
    }
    let g = w.gather_ix(&c, f.dest(), B, 0, 12);
    let l = expect_lands(keeper_send(&mut c, &w, g), "g");
    for x in &l.logs {
        println!("{x}");
    }
}

// ================================================================ wave-4 review (v1.7)

/// G1 (v1.7): SkipQuiet over a roster the kernel finds quiet but the
/// trivial test does not (two Scouts of different factions sharing a hex
/// nobody else holds: neither contests it) — the kernel quiet test runs at
/// the first bell — within `cu_gate(SkipQuiet, 1)`, for one bell and for
/// 24 (within one day: no camp check). Before v1.7 the gate was 60k + 30k
/// = 90k; the review measured 109,382 CU.
#[test]
fn g01_skip_quiet_kernel_quiet_roster_budget() {
    let b0 = 100u32;
    for n in [1u8, 24] {
        let (mut c, w) = world_on(Build::Release);
        let mut f = quiet_world(12, 5, 21, 8, b0, false);
        let passable: Vec<u8> = (0..61u8)
            .filter(|&t| f.terrain.terrain[t as usize].is_passable())
            .collect();
        let hex = passable[12];
        let mut changed = 0;
        for r in f.residents.iter_mut() {
            let first_of = |fac: u8| {
                r.faction == fac
                    && r.id
                        == permutation_frontier_svm_tests::world::clash::resident_id(12, 5, fac, 0)
            };
            if first_of(0) || first_of(1) {
                r.unit = UnitType::Scout;
                r.tile = hex;
                changed += 1;
            }
        }
        assert_eq!(changed, 2);
        assert_eq!(f.residents.len(), 48);
        w.to_bell(&mut c, b0, 1);
        craft_quiet(&mut c, &w, &f, b0, false);
        ready_run(&mut c, &w, &f, b0, n as u32);
        let ix = w.skip_ix(f.dest(), b0, n);
        let need = c
            .measure(std::slice::from_ref(&ix), &[&w.keeper])
            .expect("SkipQuiet");
        let l = expect_lands(keeper_send(&mut c, &w, ix), "SkipQuiet");
        assert_eq!(
            records::one(&l.logs, Kind::SKIP).u64("n"),
            n as u64,
            "kernel-quiet: the whole run"
        );
        let ceil = ceilings(Ix::SkipQuiet, 1, c.programdata_len());
        println!("G1 SkipQuiet kernel-quiet 48 residents, {n} bells");
        assert_within(&format!("SkipQuiet kernel-quiet n={n}"), &need, &ceil);
    }
}

/// The SKIP digest of a run: `sha256("PSF-QUIET-v1" ‖ le32(b0) ‖ n ‖
/// province[SITE_MIRROR .. TICKET_COHORTS])` (§6 v1.6).
fn quiet_digest_of(pd: &[u8], b0: u32, n: u8) -> [u8; 32] {
    permutation_frontier_svm_tests::sha256(&[
        b"PSF-QUIET-v1",
        &b0.to_le_bytes(),
        &[n],
        &pd[P::SITE_MIRROR..P::TICKET_COHORTS],
    ])
}

/// G11 (v1.7, wave-4 review): a SkipQuiet that stops before bell b (the
/// trivial test fails there after a change) leaves exactly what resolving
/// its committed bells `b0 .. b0 + n − 1` leaves — the day's camp check of
/// the stopping bell included (it is undone, and runs again with that
/// bell) — and its `quiet_digest` is the digest of that replay. Seed 11
/// crosses day 1 at bell 144 with a camp spawn (the review's case).
#[test]
fn g11_skip_stop_commits_exactly_its_bells() {
    let b0 = 130u32;
    let mut stopped = 0;
    for (seed, per, churn) in [
        (11u64, 3usize, false),
        (13, 5, true),
        (14, 8, true),
        (15, 1, true),
    ] {
        let (mut c, w) = world_on(Build::TestBeacon);
        let f = quiet_world(10, 3, seed, per, b0, churn);
        w.to_bell(&mut c, b0, 1);
        craft_quiet(&mut c, &w, &f, b0, churn);
        ready_run(&mut c, &w, &f, b0, 24);
        let camp0 = w.camp(&c, f.dest());
        let mut a = c.fork();
        let mut b = c.fork();
        let l = expect_lands(
            keeper_send(&mut a, &w, w.skip_ix(f.dest(), b0, 24)),
            "SkipQuiet",
        );
        let r = records::one(&l.logs, Kind::SKIP);
        let n = r.u64("n") as u8;
        for bell in b0..b0 + n as u32 {
            resolve_quiet_bell(&mut b, &w, &f, bell);
        }
        assert_eq!(w.resolved_next(&a, f.dest()), b0 + n as u32);
        assert_eq!(
            state_bytes(&a, &w, &f),
            state_bytes(&b, &w, &f),
            "seed {seed}: the skip of {n} bells equals resolving exactly them"
        );
        let pd = a.data(&w.a.province(f.p, f.q));
        assert_eq!(
            r.field("quiet_digest", true),
            &quiet_digest_of(&pd, b0, n)[..],
            "seed {seed}: SKIP's digest is the replay's"
        );
        // A CAMP record only for a committed bell's check.
        for camp in records::of_kind(&l.logs, Kind::CAMP) {
            assert!(
                (camp.u64("day") as u32) * 144 < b0 + n as u32,
                "seed {seed}: CAMP of an uncommitted bell"
            );
        }
        if (n as u32) < 24 && b0 + (n as u32) == 144 {
            // Stopped at the day's first bell: its check was undone.
            assert_eq!(w.camp(&a, f.dest()).3, camp0.3, "seed {seed}");
            stopped += 1;
        }
        println!(
            "G11 stop seed {seed}: {n} bells committed, camp {:?}",
            w.camp(&a, f.dest())
        );
    }
    assert!(stopped >= 1, "a run stopped at the camp's bell");
}

/// v1.7 (wave-4 review): the return settle frees at most three `Leave`
/// entries of the Holding per transaction, within SettleDeparture's G1
/// budget; five entries take two transactions, then `AlreadyDone`.
#[test]
fn clash_return_settle_is_bounded() {
    let (mut c, w, e, id) = estate();
    let dest = dest_of(&e);
    let (hp, hq, hs, gen, _) = fclient::addr::host_parts(id).unwrap();
    c.edit(&e.province, |d| {
        let tpl = read_entry(d, 0).unwrap();
        let mut j = 1;
        for n in 0..5u32 {
            while read_entry(d, j).unwrap().state != EN::STATE_FREE {
                j += 1;
            }
            let mut x = tpl;
            x.id = fclient::addr::host_id(hp, hq, hs, gen, 100 + n).unwrap();
            x.state = EN::STATE_DEPARTED;
            x.op = EntryOp::Leave;
            x.pend_bell = B0 - 1;
            x.troops = 1_000_000 + n * 1_000;
            write_entry(d, j, &x).unwrap();
        }
        let k = (0..P::ENTRIES_N)
            .filter(|&i| read_entry(d, i).unwrap().state != EN::STATE_FREE)
            .count();
        d[P::N_ENTRIES] = k as u8;
    });
    let rix = cix::settle_return(&w.a, w.keeper.pubkey(), (e.p, e.q), e.href());
    let need = c
        .measure(std::slice::from_ref(&rix), &[&w.keeper])
        .expect("return settle");
    assert_within(
        "return settle, 3 of 5 Leave entries",
        &need,
        &ceilings(Ix::SettleDeparture, 0, c.programdata_len()),
    );
    let l = expect_lands(keeper_send(&mut c, &w, rix.clone()), "return 1");
    assert_eq!(records::of_kind(&l.logs, Kind::DEPARTURE_SETTLED).len(), 3);
    let l = expect_lands(keeper_send(&mut c, &w, rix.clone()), "return 2");
    assert_eq!(records::of_kind(&l.logs, Kind::DEPARTURE_SETTLED).len(), 2);
    assert_code(keeper_send(&mut c, &w, rix), E::AlreadyDone);
    let back: u32 = (0..5u32).map(|n| 1_000 + n).sum();
    assert_eq!(reserve(&c, &e, 0), 50 + back);
    assert!(w
        .entries(&c, dest)
        .iter()
        .all(|(_, x)| x.op != EntryOp::Leave));
}

/// v1.7 (L10): CloseClashInputs needs a settled bit only for present
/// records. A record the gather wrote with a host id and `present = 0` (an
/// arrival whose Holding was re-founded before the gather) has no
/// SettleTransit that could set its bit; the inputs close anyway once the
/// present records are settled and the grace passed.
#[test]
fn clash_close_clash_inputs_needs_only_present_records() {
    let (mut c, w) = world_on(Build::TestBeacon);
    let f = Fill::adversarial(2, 45, 4, 6);
    ready(&mut c, &w, &f);
    let (fa, i, a0) = f.arrivals[0];
    let k = fa as usize * 4 + i as usize;
    let (hp, hq, hs, _, _) = fclient::addr::host_parts(a0.id).unwrap();
    c.edit(&w.a.holding(hp, hq, hs), |d| {
        d[H::GEN] = d[H::GEN].wrapping_add(1)
    });
    gather_and_resolve(&mut c, &w, &f);
    let ck = w.a.clash_inputs(f.p, f.q, B);
    let cd = c.data(&ck);
    assert_eq!(u64_at(&cd, CI::arrival(k) + AR::HOST_ID), a0.id, "recorded");
    assert_eq!(
        cd[CI::arrival(k) + AR::PRESENT],
        0,
        "not present: re-founded"
    );
    let present: u32 = (0..24)
        .filter(|&j| cd[CI::arrival(j) + AR::PRESENT] == 1)
        .fold(0, |m, j| m | 1 << j);
    c.edit(&ck, |d| {
        d[CI::SETTLED_MASK..CI::SETTLED_MASK + 4].copy_from_slice(&present.to_le_bytes())
    });
    let grace = u32_at(&c.data(&w.a.season), S::CLASH_CLOSE_GRACE) as i64;
    c.advance(grace * 600);
    let ix = cix::close_clash_inputs(
        &w.a,
        w.keeper.pubkey(),
        f.p as i16,
        f.q as i16,
        B,
        w.keeper.pubkey(),
    );
    expect_lands(keeper_send(&mut c, &w, ix), "CloseClashInputs");
    assert!(c.is_absent(&ck));
}

/// v1.7 (wave-4 review): no-arrival inputs of a bell that another keeper
/// then skipped (never resolved) close once the Province passed the bell
/// and the grace ran from the bell's end; before, `InputsOpen`.
#[test]
fn clash_close_no_arrival_inputs_after_a_skip() {
    let (mut c, w) = world_on(Build::TestBeacon);
    let f = quiet_world(6, 7, 31, 2, B, false);
    craft_quiet(&mut c, &w, &f, B, false);
    ready_run(&mut c, &w, &f, B, 1);
    let g = w.gather_ix(&c, f.dest(), B, 0, 1);
    expect_lands(keeper_send(&mut c, &w, g), "GatherClash (no arrivals)");
    let ck = w.a.clash_inputs(f.p, f.q, B);
    assert_eq!(c.data(&ck)[CI::FLAGS], CI::FLAG_NO_ARRIVALS);
    expect_lands(
        keeper_send(&mut c, &w, w.skip_ix(f.dest(), B, 1)),
        "SkipQuiet of the gathered bell",
    );
    let ix = || {
        cix::close_clash_inputs(
            &w.a,
            w.keeper.pubkey(),
            f.p as i16,
            f.q as i16,
            B,
            w.keeper.pubkey(),
        )
    };
    assert_code(keeper_send(&mut c, &w, ix()), E::InputsOpen);
    let grace = u32_at(&c.data(&w.a.season), S::CLASH_CLOSE_GRACE) as i64;
    c.set_time(w.bell_start(B + 1) + grace * 600);
    expect_lands(keeper_send(&mut c, &w, ix()), "CloseClashInputs");
    assert!(c.is_absent(&ck));
}

/// v1.7 (wave-4 review): the season-end fallback of CloseClashInputs and
/// CloseArrivalDay (as CloseArrivalSlot's case b): once the Season Ended
/// 72 h ago, inputs never resolved and a day never resolved to its end
/// close.
#[test]
fn clash_closes_after_the_season_end() {
    let (mut c, w) = world_on(Build::TestBeacon);
    let f = Fill::adversarial(0, 46, 8, 2);
    ready(&mut c, &w, &f);
    let g = w.gather_ix(&c, f.dest(), B, 0, 8);
    expect_lands(keeper_send(&mut c, &w, g), "GatherClash (part)");
    let ci = cix::close_clash_inputs(
        &w.a,
        w.keeper.pubkey(),
        f.p as i16,
        f.q as i16,
        B,
        w.keeper.pubkey(),
    );
    let day = day_of(B);
    let ad = cix::close_arrival_day(
        &w.a,
        w.keeper.pubkey(),
        f.p as i16,
        f.q as i16,
        day,
        w.keeper.pubkey(),
    );
    assert_code(keeper_send(&mut c, &w, ci.clone()), E::InputsOpen);
    assert_code(keeper_send(&mut c, &w, ad.clone()), E::TooEarly);
    c.edit(&w.a.season, |d| d[S::STATUS] = S::STATUS_ENDED);
    let end = w.bell_start(u32_at(&c.data(&w.a.season), S::END_BELL));
    c.set_time(end + 72 * 3_600 - 1);
    assert_code(keeper_send(&mut c, &w, ci.clone()), E::InputsOpen);
    c.set_time(end + 72 * 3_600);
    expect_lands(
        keeper_send(&mut c, &w, ci),
        "CloseClashInputs after the end",
    );
    expect_lands(keeper_send(&mut c, &w, ad), "CloseArrivalDay after the end");
}

// ================================================================ W5-A: G13 completion

/// The probe deployed next to the Frontier program, for CPIs.
fn with_probe(c: &mut Chain) -> Address {
    let so = permutation_frontier_svm_tests::probe::load();
    let k = Address::new_from_array(permutation_frontier_svm_tests::sha256(&[b"cpi probe"]));
    c.deploy(k, &so, so.len(), None);
    k
}

/// G13 rows of the clash area W4-A left to W5-A: GatherClash,
/// ResolveFromInputs and SkipQuiet through a CPI (`NotTopLevel`);
/// GatherClash on a Season of another ruleset (`RulesetMismatch`);
/// ResolveFromInputs on an Aborted season (`WrongStatus`) and over a
/// stored roster the kernel refuses (two entries with one host id:
/// `Kernel`).
#[test]
fn clash_g13_top_level_ruleset_status_kernel() {
    let (mut c, w) = world_on(Build::TestBeacon);
    let probe_id = with_probe(&mut c);
    let via = |ix: &Instruction| permutation_frontier_svm_tests::probe::cpi(probe_id, ix);
    let f = Fill::adversarial(1, 11, 4, -2);
    ready(&mut c, &w, &f);
    let parts = w.gather_parts(&c, f.dest(), B, &THIRDS);
    assert_code(
        keeper_send(&mut c.fork(), &w, via(&parts[0])),
        E::NotTopLevel,
    );
    let mut g = c.fork();
    g.edit(&w.a.season, |d| d[S::RULESET_HASH] ^= 1);
    assert_code(
        keeper_send(&mut g, &w, parts[0].clone()),
        E::RulesetMismatch,
    );
    for ix in parts {
        expect_lands(keeper_send(&mut c, &w, ix), "GatherClash");
    }
    let rfi = w.resolve_ix(f.dest(), B);
    assert_code(keeper_send(&mut c.fork(), &w, via(&rfi)), E::NotTopLevel);
    let mut ab = c.fork();
    w.craft_status(&mut ab, S::STATUS_ABORTED);
    assert_code(keeper_send(&mut ab, &w, rfi.clone()), E::WrongStatus);
    // Two live entries naming one host: the kernel's DuplicateId.
    let mut k = c.fork();
    let pk = w.a.province(f.p, f.q);
    k.edit(&pk, |d| {
        let live: Vec<usize> = (0..P::ENTRIES_N)
            .filter(|&i| read_entry(d, i).unwrap().state == EN::STATE_ROSTER)
            .collect();
        let a = read_entry(d, live[0]).unwrap();
        let mut b = read_entry(d, live[1]).unwrap();
        b.id = a.id;
        write_entry(d, live[1], &b).unwrap();
    });
    assert_code(keeper_send(&mut k, &w, rfi.clone()), E::Kernel);
    expect_lands(keeper_send(&mut c, &w, rfi), "ResolveFromInputs");
    // SkipQuiet through a CPI.
    let b0 = 130u32;
    let (mut c2, w2) = world_on(Build::TestBeacon);
    let probe2 = with_probe(&mut c2);
    let q = quiet_world(12, 5, 21, 8, b0, false);
    w2.to_bell(&mut c2, b0, 1);
    craft_quiet(&mut c2, &w2, &q, b0, false);
    ready_run(&mut c2, &w2, &q, b0, 2);
    let skip = w2.skip_ix(q.dest(), b0, 1);
    assert_code(
        keeper_send(
            &mut c2.fork(),
            &w2,
            permutation_frontier_svm_tests::probe::cpi(probe2, &skip),
        ),
        E::NotTopLevel,
    );
    expect_lands(keeper_send(&mut c2, &w2, skip), "SkipQuiet");
}
